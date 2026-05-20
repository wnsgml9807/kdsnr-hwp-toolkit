//! Stage 5: pixel-diff harness (한컴 GT 비교).
//!
//! `feedback_eval_harness_last.md` — 모든 byte-eq port 끝난 뒤 한 번에 측정.
//!
//! ## 흐름
//!
//! ```text
//!   hwpx ──► (parser + layout + render-engine + SvgSurface) ──► our.svg
//!                                                                  │
//!                                                                  ▼
//!                                                              resvg/usvg
//!                                                                  │
//!                                                                  ▼
//!                                                              our.png
//!                                                                  │
//!                                                                  ├── pixel-diff
//!                                                                  │
//!   hwp (한컴) ──► HwpViewer ──► hancom.pdf ──► pdftoppm ──► gt.png ──┘
//!                                                                  │
//!                                                                  ▼
//!                                                          per-page score
//! ```
//!
//! ## 본 module 의 책임 (skeleton 단계)
//!
//! - **PageScore**: 1 page 의 픽셀 비교 결과 (match_pct, mismatch_regions, summary)
//! - **DocumentScore**: 모든 page 의 PageScore aggregation
//! - **DiffOptions**: 비교 옵션 (color tolerance, anti-alias 마진, sub-region filter)
//! - **score_pages(our_png, gt_png) → PageScore**: 픽셀별 비교
//!
//! 본 module 은 PNG → byte buffer 의 비교 logic 만. SVG→PNG 변환 (resvg) 과 PDF→PNG 변환
//! (pdftoppm) 은 caller 가 외부 도구 호출 후 byte buffer 전달.
//!
//! ## 우선순위
//!
//! 본 skeleton 은 다음을 보장:
//! 1. 같은 크기 PNG buffer 2개의 RGBA 픽셀 비교
//! 2. tolerance (± per-channel) 적용
//! 3. region-of-interest (특정 bbox 만 비교) 옵션
//! 4. mismatch heatmap byte buffer 생성 (caller 가 PNG 인코딩)
//! 5. 점수 산정: `100.0 * (matched_pixels / total_pixels)`

/// 비교 옵션.
#[derive(Debug, Clone, Copy)]
pub struct DiffOptions {
    /// 픽셀 channel 별 허용 오차 (0~255). 0 = exact match.
    pub color_tolerance: u8,
    /// 픽셀 차이 임계값 — 그 이하면 "anti-alias / sub-pixel" 로 보고 무시.
    /// `Some(N)` 이면 max(|Δr|,|Δg|,|Δb|) ≤ N 인 픽셀은 match 로 카운트.
    pub aa_threshold: Option<u8>,
    /// 비교할 region (None = 전체).
    pub roi: Option<Rect>,
    /// Alpha channel 도 비교에 포함할지.
    pub compare_alpha: bool,
}

impl Default for DiffOptions {
    fn default() -> Self {
        Self {
            color_tolerance: 0,
            aa_threshold: Some(2), // 일반 anti-aliasing 마진
            roi: None,
            compare_alpha: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// 1 page 의 픽셀 비교 결과.
#[derive(Debug, Clone, PartialEq)]
pub struct PageScore {
    /// 총 비교 픽셀 수.
    pub total_pixels: u64,
    /// match (tolerance 안) 픽셀 수.
    pub matched_pixels: u64,
    /// mismatch 픽셀 수 (= total - matched).
    pub mismatched_pixels: u64,
    /// 점수 (0~100).
    pub score_pct: f32,
    /// mismatch 의 평균 RGB delta.
    pub avg_delta: f32,
    /// 가장 큰 mismatch 영역들 (top-N bboxes — 본 skeleton 에선 빈 vec).
    pub mismatch_regions: Vec<Rect>,
}

impl PageScore {
    pub fn perfect() -> Self {
        Self {
            total_pixels: 0,
            matched_pixels: 0,
            mismatched_pixels: 0,
            score_pct: 100.0,
            avg_delta: 0.0,
            mismatch_regions: Vec::new(),
        }
    }
}

/// document 전체 (multi-page) 점수 aggregation.
#[derive(Debug, Clone)]
pub struct DocumentScore {
    pub page_scores: Vec<PageScore>,
    pub avg_score_pct: f32,
    pub worst_page_idx: usize,
    pub worst_page_score: f32,
}

impl DocumentScore {
    pub fn aggregate(pages: Vec<PageScore>) -> Self {
        if pages.is_empty() {
            return Self {
                page_scores: Vec::new(),
                avg_score_pct: 100.0,
                worst_page_idx: 0,
                worst_page_score: 100.0,
            };
        }
        let sum: f32 = pages.iter().map(|p| p.score_pct).sum();
        let avg = sum / pages.len() as f32;
        let (worst_idx, worst) = pages
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| a.score_pct.partial_cmp(&b.score_pct).unwrap())
            .map(|(i, p)| (i, p.score_pct))
            .unwrap_or((0, 100.0));
        Self {
            page_scores: pages,
            avg_score_pct: avg,
            worst_page_idx: worst_idx,
            worst_page_score: worst,
        }
    }
}

/// 두 RGBA buffer 의 픽셀 비교. width × height × 4 byte format 강제.
///
/// # Returns
///
/// `Result<PageScore, String>` — buffer size mismatch 면 Err.
pub fn score_pages(
    our_rgba: &[u8],
    gt_rgba: &[u8],
    width: u32,
    height: u32,
    opts: &DiffOptions,
) -> Result<PageScore, String> {
    let expected_size = (width as usize) * (height as usize) * 4;
    if our_rgba.len() != expected_size {
        return Err(format!(
            "our_rgba size {} != expected {} (w={} h={})",
            our_rgba.len(),
            expected_size,
            width,
            height
        ));
    }
    if gt_rgba.len() != expected_size {
        return Err(format!(
            "gt_rgba size {} != expected {} (w={} h={})",
            gt_rgba.len(),
            expected_size,
            width,
            height
        ));
    }

    // ROI 범위
    let (x0, y0, x1, y1) = match opts.roi {
        Some(r) => (
            r.x.min(width),
            r.y.min(height),
            (r.x + r.w).min(width),
            (r.y + r.h).min(height),
        ),
        None => (0, 0, width, height),
    };

    let mut total: u64 = 0;
    let mut matched: u64 = 0;
    let mut delta_sum: f64 = 0.0;

    for y in y0..y1 {
        for x in x0..x1 {
            let idx = ((y * width + x) * 4) as usize;
            let or_ = our_rgba[idx];
            let og = our_rgba[idx + 1];
            let ob = our_rgba[idx + 2];
            let oa = our_rgba[idx + 3];
            let gr = gt_rgba[idx];
            let gg = gt_rgba[idx + 1];
            let gb = gt_rgba[idx + 2];
            let ga = gt_rgba[idx + 3];

            let dr = or_.abs_diff(gr);
            let dg = og.abs_diff(gg);
            let db = ob.abs_diff(gb);
            let da = oa.abs_diff(ga);

            let max_d = dr.max(dg).max(db);
            let max_d_with_a = if opts.compare_alpha { max_d.max(da) } else { max_d };

            total += 1;
            let is_match = if let Some(aa) = opts.aa_threshold {
                max_d_with_a <= opts.color_tolerance.max(aa)
            } else {
                max_d_with_a <= opts.color_tolerance
            };
            if is_match {
                matched += 1;
            } else {
                delta_sum += max_d_with_a as f64;
            }
        }
    }

    let mismatched = total - matched;
    let score_pct = if total == 0 {
        100.0
    } else {
        100.0 * (matched as f32) / (total as f32)
    };
    let avg_delta = if mismatched == 0 {
        0.0
    } else {
        (delta_sum / mismatched as f64) as f32
    };

    Ok(PageScore {
        total_pixels: total,
        matched_pixels: matched,
        mismatched_pixels: mismatched,
        score_pct,
        avg_delta,
        mismatch_regions: Vec::new(),
    })
}

/// ink-only ROI score — 흰배경 매치를 점수에서 제외.
///
/// 두 buffer 의 union ink mask (어느 쪽이라도 ink 인 픽셀) 만 비교 대상으로 함.
/// 흰배경이 90%+ 인 페이지에서도 visible content quality 정확히 측정.
///
/// `ink_threshold`: luminance 값 (0~255). 그 이하면 ink 픽셀로 간주 (기본 200 권장).
///   - 200 = 회색 글자/얇은 선까지 포함
///   - 240 = 진한 글자/도형만
///
/// # 반환
///
/// `PageScore`. `total_pixels` 는 ink ROI 의 픽셀 수 (전체 가 아님).
pub fn score_pages_ink_only(
    our_rgba: &[u8],
    gt_rgba: &[u8],
    width: u32,
    height: u32,
    opts: &DiffOptions,
    ink_threshold: u8,
) -> Result<PageScore, String> {
    let expected_size = (width as usize) * (height as usize) * 4;
    if our_rgba.len() != expected_size || gt_rgba.len() != expected_size {
        return Err(format!(
            "buffer size mismatch (expected {}, our={}, gt={})",
            expected_size, our_rgba.len(), gt_rgba.len()
        ));
    }

    let mut total: u64 = 0;
    let mut matched: u64 = 0;
    let mut delta_sum: f64 = 0.0;

    for y in 0..height {
        for x in 0..width {
            let idx = ((y * width + x) * 4) as usize;
            let or_ = our_rgba[idx];
            let og = our_rgba[idx + 1];
            let ob = our_rgba[idx + 2];
            let gr = gt_rgba[idx];
            let gg = gt_rgba[idx + 1];
            let gb = gt_rgba[idx + 2];

            // ITU-R BT.601 luminance
            let our_lum = (0.299 * or_ as f32 + 0.587 * og as f32 + 0.114 * ob as f32) as u32;
            let gt_lum = (0.299 * gr as f32 + 0.587 * gg as f32 + 0.114 * gb as f32) as u32;
            let our_ink = our_lum < ink_threshold as u32;
            let gt_ink = gt_lum < ink_threshold as u32;
            if !our_ink && !gt_ink { continue; }  // 둘 다 흰배경 → ROI 제외

            total += 1;
            let dr = or_.abs_diff(gr);
            let dg = og.abs_diff(gg);
            let db = ob.abs_diff(gb);
            let max_d = dr.max(dg).max(db);
            let is_match = if let Some(aa) = opts.aa_threshold {
                max_d <= opts.color_tolerance.max(aa)
            } else {
                max_d <= opts.color_tolerance
            };
            if is_match { matched += 1; } else { delta_sum += max_d as f64; }
        }
    }

    let mismatched = total - matched;
    let score_pct = if total == 0 {
        100.0
    } else {
        100.0 * (matched as f32) / (total as f32)
    };
    let avg_delta = if mismatched == 0 {
        0.0
    } else {
        (delta_sum / mismatched as f64) as f32
    };

    Ok(PageScore {
        total_pixels: total,
        matched_pixels: matched,
        mismatched_pixels: mismatched,
        score_pct,
        avg_delta,
        mismatch_regions: Vec::new(),
    })
}

/// **alignment-tolerant ink IoU** — 글자 위치 1-2px 어긋남을 tolerate 하는 점수.
///
/// 두 buffer 의 ink mask 를 추출 후 각자 `dilate_radius` 만큼 morphological dilate.
/// IoU = (gt_dil ∩ our_dil) / (gt_dil ∪ our_dil). 글자가 정확한 위치 + 모양이면 1.0,
/// 위치만 어긋나면 IoU 가 1.0 에 가까움 (dilate 가 흡수), 글자 자체가 다른 위치/크기/누락 이면 낮음.
///
/// `ink_threshold`: luminance 컷오프 (200 권장).
/// `dilate_radius`: 1-3 px (시각 perception 과 일치하는 값).
///
/// # 반환값
///
/// `(iou_pct, gt_ink_count, our_ink_count, intersection, union)`.
pub fn score_pages_ink_iou(
    our_rgba: &[u8],
    gt_rgba: &[u8],
    width: u32,
    height: u32,
    ink_threshold: u8,
    dilate_radius: u32,
) -> Result<(f32, u64, u64, u64, u64), String> {
    let expected_size = (width as usize) * (height as usize) * 4;
    if our_rgba.len() != expected_size || gt_rgba.len() != expected_size {
        return Err(format!("buffer size mismatch"));
    }
    let w = width as usize;
    let h = height as usize;
    let n = w * h;

    // ink mask 추출
    let mut gt_ink = vec![false; n];
    let mut our_ink = vec![false; n];
    let mut gt_ink_count: u64 = 0;
    let mut our_ink_count: u64 = 0;
    for i in 0..n {
        let j = i * 4;
        let gr = gt_rgba[j]; let gg = gt_rgba[j+1]; let gb = gt_rgba[j+2];
        let or_ = our_rgba[j]; let og = our_rgba[j+1]; let ob = our_rgba[j+2];
        let gl = (0.299 * gr as f32 + 0.587 * gg as f32 + 0.114 * gb as f32) as u32;
        let ol = (0.299 * or_ as f32 + 0.587 * og as f32 + 0.114 * ob as f32) as u32;
        if gl < ink_threshold as u32 { gt_ink[i] = true; gt_ink_count += 1; }
        if ol < ink_threshold as u32 { our_ink[i] = true; our_ink_count += 1; }
    }

    // dilate (Manhattan ball radius)
    let gt_dil = dilate_mask(&gt_ink, w, h, dilate_radius);
    let our_dil = dilate_mask(&our_ink, w, h, dilate_radius);

    // IoU
    let mut intersection: u64 = 0;
    let mut union: u64 = 0;
    for i in 0..n {
        if gt_dil[i] || our_dil[i] { union += 1; }
        if gt_dil[i] && our_dil[i] { intersection += 1; }
    }
    let iou_pct = if union == 0 { 100.0 } else {
        100.0 * (intersection as f32) / (union as f32)
    };
    Ok((iou_pct, gt_ink_count, our_ink_count, intersection, union))
}

/// Manhattan ball dilate. radius=0 면 원본 그대로.
fn dilate_mask(mask: &[bool], w: usize, h: usize, radius: u32) -> Vec<bool> {
    if radius == 0 { return mask.to_vec(); }
    let r = radius as i32;
    let mut out = vec![false; mask.len()];
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            if !mask[(y as usize) * w + (x as usize)] { continue; }
            // 픽셀 (x,y) 가 ink → 반경 r 안 모두 mark
            for dy in -r..=r {
                let ny = y + dy;
                if ny < 0 || ny >= h as i32 { continue; }
                let remain = r - dy.abs();
                for dx in -remain..=remain {
                    let nx = x + dx;
                    if nx < 0 || nx >= w as i32 { continue; }
                    out[(ny as usize) * w + (nx as usize)] = true;
                }
            }
        }
    }
    out
}

/// mismatch 영역 heatmap RGBA buffer 생성 (caller 가 PNG 인코딩).
///
/// 출력: 같은 w/h 의 RGBA buffer. match=흰색 투명, mismatch=빨강 (alpha 강조).
pub fn make_heatmap_rgba(
    our_rgba: &[u8],
    gt_rgba: &[u8],
    width: u32,
    height: u32,
    opts: &DiffOptions,
) -> Result<Vec<u8>, String> {
    let expected_size = (width as usize) * (height as usize) * 4;
    if our_rgba.len() != expected_size || gt_rgba.len() != expected_size {
        return Err("buffer size mismatch".to_string());
    }
    let mut out = vec![0u8; expected_size];
    let threshold = match opts.aa_threshold {
        Some(t) => opts.color_tolerance.max(t),
        None => opts.color_tolerance,
    };
    for i in (0..expected_size).step_by(4) {
        let dr = our_rgba[i].abs_diff(gt_rgba[i]);
        let dg = our_rgba[i + 1].abs_diff(gt_rgba[i + 1]);
        let db = our_rgba[i + 2].abs_diff(gt_rgba[i + 2]);
        let max_d = dr.max(dg).max(db);
        if max_d > threshold {
            // 빨강 + alpha = max_d 강도
            out[i] = 0xff;
            out[i + 1] = 0;
            out[i + 2] = 0;
            out[i + 3] = max_d.saturating_mul(2);
        } else {
            // 투명
            out[i] = 0xff;
            out[i + 1] = 0xff;
            out[i + 2] = 0xff;
            out[i + 3] = 0;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_rgba(width: u32, height: u32, color: [u8; 4]) -> Vec<u8> {
        let mut buf = Vec::with_capacity((width * height * 4) as usize);
        for _ in 0..(width * height) {
            buf.extend_from_slice(&color);
        }
        buf
    }

    #[test]
    fn identical_buffers_score_100() {
        let a = make_rgba(10, 10, [128, 128, 128, 255]);
        let b = a.clone();
        let score = score_pages(&a, &b, 10, 10, &DiffOptions::default()).unwrap();
        assert_eq!(score.score_pct, 100.0);
        assert_eq!(score.mismatched_pixels, 0);
        assert_eq!(score.total_pixels, 100);
    }

    #[test]
    fn completely_different_buffers_score_0() {
        let a = make_rgba(10, 10, [0, 0, 0, 255]);
        let b = make_rgba(10, 10, [255, 255, 255, 255]);
        let opts = DiffOptions { color_tolerance: 0, aa_threshold: None, ..DiffOptions::default() };
        let score = score_pages(&a, &b, 10, 10, &opts).unwrap();
        assert_eq!(score.score_pct, 0.0);
        assert_eq!(score.mismatched_pixels, 100);
    }

    #[test]
    fn aa_threshold_treats_small_diff_as_match() {
        let a = make_rgba(10, 10, [128, 128, 128, 255]);
        let b = make_rgba(10, 10, [130, 130, 130, 255]); // diff = 2
        let opts = DiffOptions { color_tolerance: 0, aa_threshold: Some(2), ..DiffOptions::default() };
        let score = score_pages(&a, &b, 10, 10, &opts).unwrap();
        assert_eq!(score.score_pct, 100.0);
    }

    #[test]
    fn aa_threshold_does_not_help_when_diff_exceeds() {
        let a = make_rgba(10, 10, [128, 128, 128, 255]);
        let b = make_rgba(10, 10, [140, 140, 140, 255]); // diff = 12
        let opts = DiffOptions { color_tolerance: 0, aa_threshold: Some(2), ..DiffOptions::default() };
        let score = score_pages(&a, &b, 10, 10, &opts).unwrap();
        assert_eq!(score.score_pct, 0.0);
        // avg_delta = 12 (all mismatches differ by exactly 12)
        assert_eq!(score.avg_delta, 12.0);
    }

    #[test]
    fn roi_limits_comparison_to_subregion() {
        let mut a = make_rgba(10, 10, [0, 0, 0, 255]);
        let b = make_rgba(10, 10, [255, 255, 255, 255]);
        // ROI: 2x2 at (5,5) — only 4 pixels checked
        let opts = DiffOptions {
            roi: Some(Rect { x: 5, y: 5, w: 2, h: 2 }),
            color_tolerance: 0,
            aa_threshold: None,
            compare_alpha: false,
        };
        // make those 4 ROI pixels match (white)
        for y in 5..7 {
            for x in 5..7 {
                let idx = ((y * 10 + x) * 4) as usize;
                a[idx] = 255; a[idx + 1] = 255; a[idx + 2] = 255;
            }
        }
        let score = score_pages(&a, &b, 10, 10, &opts).unwrap();
        assert_eq!(score.total_pixels, 4);
        assert_eq!(score.score_pct, 100.0);
    }

    #[test]
    fn alpha_compare_off_ignores_alpha_difference() {
        let a = make_rgba(10, 10, [128, 128, 128, 0]);
        let b = make_rgba(10, 10, [128, 128, 128, 255]);
        let opts = DiffOptions { compare_alpha: false, ..DiffOptions::default() };
        let score = score_pages(&a, &b, 10, 10, &opts).unwrap();
        assert_eq!(score.score_pct, 100.0);
    }

    #[test]
    fn alpha_compare_on_detects_alpha_difference() {
        let a = make_rgba(10, 10, [128, 128, 128, 0]);
        let b = make_rgba(10, 10, [128, 128, 128, 255]);
        let opts = DiffOptions { compare_alpha: true, aa_threshold: None, color_tolerance: 0, ..DiffOptions::default() };
        let score = score_pages(&a, &b, 10, 10, &opts).unwrap();
        assert_eq!(score.score_pct, 0.0);
    }

    #[test]
    fn size_mismatch_returns_error() {
        let a = vec![0u8; 100];
        let b = vec![0u8; 100];
        let r = score_pages(&a, &b, 10, 10, &DiffOptions::default());
        assert!(r.is_err());
    }

    #[test]
    fn document_aggregate_finds_worst_page() {
        let pages = vec![
            PageScore { score_pct: 100.0, total_pixels: 0, matched_pixels: 0, mismatched_pixels: 0, avg_delta: 0.0, mismatch_regions: Vec::new() },
            PageScore { score_pct: 50.0,  total_pixels: 0, matched_pixels: 0, mismatched_pixels: 0, avg_delta: 0.0, mismatch_regions: Vec::new() },
            PageScore { score_pct: 95.0,  total_pixels: 0, matched_pixels: 0, mismatched_pixels: 0, avg_delta: 0.0, mismatch_regions: Vec::new() },
        ];
        let doc = DocumentScore::aggregate(pages);
        assert_eq!(doc.worst_page_idx, 1);
        assert_eq!(doc.worst_page_score, 50.0);
        // avg = (100 + 50 + 95) / 3 ≈ 81.67
        assert!((doc.avg_score_pct - 81.6667).abs() < 0.01);
    }

    #[test]
    fn empty_document_aggregate_returns_perfect() {
        let doc = DocumentScore::aggregate(Vec::new());
        assert_eq!(doc.avg_score_pct, 100.0);
        assert!(doc.page_scores.is_empty());
    }

    #[test]
    fn heatmap_marks_mismatches_red_with_alpha() {
        let a = make_rgba(4, 1, [0, 0, 0, 255]);
        let mut b = make_rgba(4, 1, [0, 0, 0, 255]);
        // 픽셀 0, 2 만 차이 (50 diff)
        b[0] = 50;
        b[8] = 50;
        let opts = DiffOptions { color_tolerance: 0, aa_threshold: None, ..DiffOptions::default() };
        let heat = make_heatmap_rgba(&a, &b, 4, 1, &opts).unwrap();
        // pixel 0: mismatch (red, alpha=100)
        assert_eq!(heat[0..4], [0xff, 0, 0, 100]);
        // pixel 1: match (transparent white)
        assert_eq!(heat[4..8], [0xff, 0xff, 0xff, 0]);
        // pixel 2: mismatch
        assert_eq!(heat[8..12], [0xff, 0, 0, 100]);
        // pixel 3: match
        assert_eq!(heat[12..16], [0xff, 0xff, 0xff, 0]);
    }

    #[test]
    fn perfect_constructor_yields_100_score() {
        let p = PageScore::perfect();
        assert_eq!(p.score_pct, 100.0);
        assert_eq!(p.mismatched_pixels, 0);
    }
}

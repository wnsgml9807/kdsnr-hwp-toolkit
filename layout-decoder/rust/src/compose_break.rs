//! `ColCompositor::ComposeBreak` — Hancom 라인 분할 알고리즘.
//!
//! 1:1 포팅 from `libHncDrawingEngine_arm64.dylib`:
//! `Hnc::Shape::Text::ColCompositor::ComposeBreak` (`FUN_0030590c`, size=1108).
//!
//! ## 핵심 발견 (Knuth-Plass 가 아니다)
//!
//! 이름과 달리 **Knuth-Plass DP 가 아니라 greedy line-fill** 알고리즘:
//! 1. 총 자연 width 합산 (SIMD-unrolled)
//! 2. **overflow path** (`*pfVar16 <= fVar29`, 즉 `composition[0] <= total_width`):
//!    line_count 의 균등 분포로 break 채움
//! 3. **fit path**: line index 별로 `composition[min(idx, size-1)]` 을 column width 로 써서
//!    widths 를 누적, 초과 시 그 인덱스에서 break
//!
//! 함수 시그니처는 5 vectors + Composition + from + to 지만 **body 에서 실제 사용은
//! widths (param_2) + composition (param_6) + line_count (`this._count`) + out_breaks 만**.
//! stretches/shrinks/penalties/heights 와 from/to 는 unused (다른 ColCompositor 메소드에서
//! 활용될 가능성).
//!
//! ## DAT 상수 정정
//!
//! `_DAT_007415b0/b8/c0/c8 = [2, 3, 0, 1]` 은 East-Asian penalty 가 아니라 NEON 4-lane
//! 인덱스 벡터. SIMD 로 균등 분포 채울 때 lane 별 base offset 으로 사용.
//!
//! ## ColCompositor 멤버 사용
//!
//! - `this._count` (offset +0x10): 목표 line count. 0 이면 1 로 default. 매번 호출 시 in/out.
//!
//! ## 참고
//!
//! - 원본 decompile: `kdsnr-hwp-toolkit/work/hft_re/layout_re/Text_ColCompositor__ComposeBreak_0030590c.txt`
//! - SIMD 헬퍼 `Hnc::Util::MathUtil::Ceil` (libc ceil 호출).

/// `ColCompositor::ComposeBreak` 의 입력.
#[derive(Debug)]
pub struct ComposeBreakInput<'a> {
    /// `param_2` — chars 의 자연 width 배열.
    pub widths: &'a [f32],
    /// `param_3` — stretch 배열 (현재 body 에서 미사용).
    pub _stretches: &'a [f32],
    /// `param_4` — shrink 또는 penalty 배열 (현재 body 에서 미사용).
    pub _shrinks: &'a [f32],
    /// `param_5` — height 배열 (현재 body 에서 미사용).
    pub _heights: &'a [f32],
    /// `param_6` — line-specific column widths. `composition[min(line_idx, size-1)]` 으로
    /// 각 라인의 maximum width 를 얻음.
    pub composition_widths: &'a [f32],
    /// `param_7` — from (현재 body 에서 미사용).
    pub _from: i32,
    /// `param_8` — to (현재 body 에서 미사용).
    pub _to: i32,
}

/// `ColCompositor::ComposeBreak` 의 출력.
#[derive(Debug, Default)]
pub struct ComposeBreakOutput {
    /// 각 line 의 마지막 char index. `breaks[i]` 는 i-th line 의 마지막 포함 char.
    /// `breaks.len() == line_count` (입력으로 받은 값).
    pub breaks: Vec<u32>,
}

/// ColCompositor::ComposeBreak 알고리즘 1:1 포팅.
///
/// `this_line_count` 는 in/out: 0 으로 들어오면 1 로 set 됨. 함수 반환 값은 최종 line count.
pub fn compose_break(input: &ComposeBreakInput<'_>, this_line_count: &mut u32) -> ComposeBreakOutput {
    let n_widths = input.widths.len();
    let n_uint = n_widths as u32;

    // `param_1 + 0x10` — ColCompositor 의 line count member.
    // (uVar8 == 0) → 1 로 default
    if *this_line_count == 0 {
        *this_line_count = 1;
    }
    let line_count = *this_line_count as usize;
    let line_count_u = *this_line_count;

    // iVar6 = ceil(N * 0.5 / line_count). MathUtil::Ceil 은 표준 ceil.
    // SIMD 청크 너비 chunk_size = iVar6 * 2.
    let chars_per_half = ((n_uint as f32) * 0.5_f32 / (line_count_u as f32)).ceil() as i32;
    let chunk_size = (chars_per_half * 2) as u32;

    // === 총 width 합산 ===
    //
    // 한컴 원본은 SIMD 로 16개씩 unroll 하지만 동작은 단순 sum.
    // chunk_size < 16 인 경우는 단순 loop. 그 외는 16-aligned chunk 로 sum.
    // Rust 에서는 그냥 fold 로 1:1.
    //
    // 단, 한컴은 `if iVar6 < 1: fVar29 = 0.0` 으로 빠짐. 즉 line_count > N*0.5 이면 sum 0.
    // 이건 의도: line 수가 너무 많으면 어차피 overflow path 안 가니까 sum 계산 skip.
    let mut total_width: f32 = 0.0;
    if chars_per_half >= 1 {
        let lim = chunk_size.max(1) as usize;
        // Hancom 의 sum 범위는 `lim` chars (즉 chunk_size). N 미만이면 chunk_size, 초과면 N.
        let sum_n = lim.min(n_widths);
        for i in 0..sum_n {
            total_width += input.widths[i];
        }
    }

    // === out_breaks 를 line_count 크기로 resize ===
    let mut breaks: Vec<u32> = Vec::with_capacity(line_count);

    // === composition_widths[0] 와 total_width 비교 ===
    //
    // composition 이 비어있으면 정의되지 않은 동작 (한컴 코드는 NULL deref).
    // Rust 에선 안전하게 처리.
    let first_col_w = input.composition_widths.first().copied().unwrap_or(0.0);

    if first_col_w <= total_width {
        // === Overflow path: 균등 분포 ===
        //
        // 한컴: SIMD 로 4 lane 씩 unroll, lane indices = [2,3,0,1] from _DAT_007415b0/b8/c0/c8.
        // 의미: lane k 의 base 는 `chunk_size * (lane_offset + 5) - 1`, 다음 chunk 는 +4 lane 씩.
        // 단순화한 1:1 동작:
        //
        // 첫 SIMD chunk (16 lanes 4 그룹):
        //   for group g in {0,4,8,12}:  // group offsets
        //     for lane (in order 0,1,2,3 mapped via [2,3,0,1]):
        //       lane_offset = [2,3,0,1][lane]
        //       value = chunk_size * (lane_offset + 5 + g) - 1
        //       value = min(value, N-1)
        //       breaks[chunk_idx*16 + g + lane] = value
        //
        // 이후 single-step 추가:
        //   base = (iVar6 + iVar6 * tail_start) * 2 - 1 = chunk_size * (tail_start + 1) - 1
        //   for i in tail_start..line_count:
        //     breaks[i] = min(N-1, base)
        //     base += chunk_size

        if line_count == 0 {
            return ComposeBreakOutput { breaks };
        }

        let last_char_idx = (n_uint as i32 - 1).max(0);

        // 16-lane SIMD chunks (line_count >= 16 만)
        let simd_tail = line_count & !0xf;  // line_count 의 16-aligned 부분

        // Lane index 벡터 [2, 3, 0, 1] (SIMD 4-lane 의 NEON 순서)
        let lane_indices: [i32; 4] = [2, 3, 0, 1];

        // 16개 단위 unroll: 한 iteration 에 4 groups × 4 lanes = 16 breaks
        let mut chunk_idx = 0_usize;
        while chunk_idx < simd_tail {
            // 4 groups, each writing 4 breaks via NEON_smin
            for group in 0..4 {
                let group_offset = group * 4 + 1;  // +1, +5, +9, +13
                for lane in 0..4 {
                    let lane_offset = lane_indices[lane];
                    let value = (chunk_size as i32) * (lane_offset + group_offset) - 1;
                    let clamped = value.min(last_char_idx);
                    breaks.push(clamped as u32);
                }
            }
            chunk_idx += 16;
        }

        if simd_tail < line_count {
            // tail: single-step
            // base = chunk_size * (simd_tail + 1) - 1
            let mut base = (chars_per_half + chars_per_half * (simd_tail as i32)) * 2 - 1;
            for _ in simd_tail..line_count {
                let clamped = base.min(last_char_idx);
                breaks.push(clamped as u32);
                base += chunk_size as i32;
            }
        }

        return ComposeBreakOutput { breaks };
    }

    // === Fit path: greedy line-fill ===
    //
    // 각 line_idx 에 대해, composition[min(line_idx, comp_size-1)] 를 column width 로 두고,
    // widths 를 sum 하다가 초과하면 그 char index 를 break 로 기록.
    //
    // uVar14 = next start char (initially 0)
    // uVar13 = current line index
    // uVar10 = composition.size (clamped to >=0)

    let n_comp = input.composition_widths.len();
    let cap_lines = (n_comp as i32).max(0) as usize;  // composition.size

    if n_uint < 1 {
        // 입력 widths 비어있으면 모든 break 를 N 으로 채우는 fallback 으로
        // (한컴 코드의 line 222-226 branch).
        // 이미 빈 breaks 로 return 해도 동일.
        // 하지만 한컴은 line_count 만큼 breaks[i] = N 으로 채움. 1:1 로 처리.
        for _ in 0..line_count {
            breaks.push(n_uint);
        }
        if breaks.len() > cap_lines {
            breaks.truncate(cap_lines);
        }
        return ComposeBreakOutput { breaks };
    }

    // n_widths >= 1 인 일반 경로
    let mut next_start: u32 = 0;
    let comp_last = (n_comp as i32 - 1).max(0);

    for line_idx in 0..line_count {
        if next_start >= n_uint {
            break;
        }

        // line-specific column width
        let comp_idx = (line_idx as i32).min(comp_last).max(0) as usize;
        let line_col_w = input.composition_widths.get(comp_idx).copied().unwrap_or(0.0);

        // 마지막 char 면 break = N-1
        let cur_start = next_start;
        if cur_start == n_uint - 1 {
            // 마지막 char 까지 도달
            breaks.push(n_uint - 1);
            next_start = n_uint;
            continue;
        }

        // Sum widths from cur_start, advance until exceed line_col_w
        let mut sum: f32 = 0.0;
        let mut found_break: Option<u32> = None;
        let remaining = (n_uint - cur_start) as usize;
        for offset in 0..remaining {
            sum += input.widths[(cur_start as usize) + offset];
            if line_col_w < sum {
                let break_idx = cur_start + offset as u32;
                found_break = Some(break_idx);
                break;
            }
        }

        let last_char_in_line = match found_break {
            Some(idx) => {
                // 초과 발생 → break_idx 가 다음 line 의 시작. 현 line 의 마지막 char = idx - 1.
                next_start = idx;
                idx.saturating_sub(1)
            }
            None => {
                // 끝까지 소진 → 모든 char fit, 현 line 끝 = N-1
                next_start = n_uint;
                cur_start  // hancom: uVar12 = uVar20 (start) 일 때는 break 기록 skip
            }
        };

        // 한컴 분기: if (uVar12 != uVar20) — 즉 새 break 가 진짜 진전했을 때만 기록
        if found_break.is_some() {
            breaks.push(last_char_in_line);
        } else {
            // 단순 다음 char 로 이동 (uVar14 = uVar20 + 1, uVar12 = uVar20)
            breaks.push(cur_start);
            next_start = cur_start + 1;
        }
    }

    // === 나머지 breaks 슬롯을 N (sentinel) 으로 채움 ===
    //
    // 한컴 line 268-300: SIMD-unroll 으로 채우고 나머지는 single-step.
    while breaks.len() < line_count {
        breaks.push(n_uint);
    }

    // === resize to min(line_count, cap_lines) ===
    if breaks.len() > cap_lines {
        breaks.truncate(cap_lines);
    }

    ComposeBreakOutput { breaks }
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn input_simple<'a>(widths: &'a [f32], col_widths: &'a [f32]) -> ComposeBreakInput<'a> {
        ComposeBreakInput {
            widths,
            _stretches: &[],
            _shrinks: &[],
            _heights: &[],
            composition_widths: col_widths,
            _from: 0,
            _to: 0,
        }
    }

    #[test]
    fn default_line_count_zero_set_to_one() {
        let widths = [10.0, 20.0];
        let cols = [100.0];
        let inp = input_simple(&widths, &cols);
        let mut lc: u32 = 0;
        compose_break(&inp, &mut lc);
        assert_eq!(lc, 1, "0 line_count must be defaulted to 1");
    }

    #[test]
    fn empty_widths_fills_with_sentinel() {
        let cols = [100.0];
        let inp = input_simple(&[], &cols);
        let mut lc: u32 = 3;
        let r = compose_break(&inp, &mut lc);
        // n_widths = 0, composition_widths.len() = 1 → cap_lines = 1
        // line_count = 3, 채워진 후 cap_lines 로 truncate
        assert_eq!(r.breaks.len(), 1);
        assert_eq!(r.breaks[0], 0);  // sentinel = N = 0
    }

    #[test]
    fn single_line_fits_under_column_width() {
        // 3 chars total 30, column = 100, line_count = 1 → fit, no break
        let widths = [10.0, 10.0, 10.0];
        let cols = [100.0];
        let inp = input_simple(&widths, &cols);
        let mut lc: u32 = 1;
        let r = compose_break(&inp, &mut lc);
        // 모든 char fit → 한 line, break at last char (N-1=2) or continued
        // 한컴 logic: cur_start=0, sum 30 < 100, found_break=None
        // → last_char_in_line = cur_start = 0, breaks.push(0), next_start = 1
        // 다음 loop: cur_start = 1, sum 20 < 100, found_break = None
        // → breaks.push(1), next_start = 2
        // 등등. 한 줄에 line_count=1 이므로 첫 iteration 끝나면 종료. breaks = [0].
        // 결과: breaks.len() = line_count = 1
        assert_eq!(r.breaks.len(), 1);
    }

    #[test]
    fn greedy_fill_three_lines() {
        // 6 chars × 10 width = 60 total. column = 20. expect 3 lines.
        let widths = [10.0, 10.0, 10.0, 10.0, 10.0, 10.0];
        let cols = [20.0, 20.0, 20.0];
        let inp = input_simple(&widths, &cols);
        let mut lc: u32 = 3;
        let r = compose_break(&inp, &mut lc);
        // total_width=60 > column[0]=20 → overflow path with uniform distribute
        // line_count=3, chunk_size = ceil(6*0.5/3) * 2 = 1*2 = 2
        // last_char_idx = 5
        // simd_tail = 0 (3 < 16)
        // tail: base = (1 + 1*0) * 2 - 1 = 1; line_count=3
        //   i=0: clamped = min(1, 5) = 1, base = 3
        //   i=1: clamped = min(3, 5) = 3, base = 5
        //   i=2: clamped = min(5, 5) = 5
        // breaks = [1, 3, 5]
        assert_eq!(r.breaks, vec![1, 3, 5]);
    }

    #[test]
    fn fit_path_per_line_column_width() {
        // total = 30, first column = 100 (fits) → fit path
        // widths [10,10,10], cols [25, 25] → first line gets up to width 25
        // Sum at chars: 10, 20, 30. exceeds 25 at char 2 (sum=30 > 25).
        // → break_idx = 2, next_start = 2, last_char_in_line = 1
        // breaks = [1, ...]
        // Next iteration: cur_start=2, line_idx=1, col_w = 25, sum 10 < 25, found_break=None
        // → if found_break.is_none, breaks.push(cur_start)=2, next_start = 3
        // breaks = [1, 2]
        let widths = [10.0, 10.0, 10.0];
        let cols = [100.0, 25.0];
        let inp = input_simple(&widths, &cols);
        let mut lc: u32 = 2;
        let r = compose_break(&inp, &mut lc);
        // total = 30 vs cols[0]=100 → 100 > 30, fit path
        assert_eq!(r.breaks.len(), 2);
    }
}

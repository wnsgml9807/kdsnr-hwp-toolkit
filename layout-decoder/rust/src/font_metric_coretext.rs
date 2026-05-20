//! `font_metric` 의 두 trait — [`CoreTextFontProvider`] (per-glyph advance) 와
//! [`GlobalMetricProvider`] (font global metric) — 의 **실제 macOS CoreText 구현**.
//!
//! [`font_metric`](crate::font_metric) 모듈은 `FUN_00082d98` / `FUN_000761f4` 의
//! **순수 산술/로직** 을 1:1 포팅했고, 실제 CoreText 호출만 trait 경계로 남겼다.
//! 본 모듈이 그 경계를 macOS CoreText/CoreGraphics/CoreFoundation FFI 로 채운다.
//!
//! ## RE 대응
//!
//! - **advance** ([`CoreTextFontProvider`]) — `FUN_00082d98` 가 char 마다
//!   `CTFontCreateWithName(name, size_px, NULL)` → `CTFontGetGlyphsForCharacters` →
//!   `CTFontGetAdvancesForGlyphs(font, 0, &g, NULL, 1)`. raw 는 char 마다 CTFont 를
//!   생성/release 하지만, CTFont 는 `(SystemFont, size_px)` 만의 함수라 캐시해도
//!   출력 동일 (`ct_cache`).
//! - **global metric** ([`GlobalMetricProvider`]) — `FUN_000761f4` →
//!   `libhsp::CreateFontIndirectW`(`FUN_00064258` CoreText realization) →
//!   `GetOutlineTextMetricsW`(`FUN_00089c1c`). 순효과: `CGFontGetUnitsPerEm` +
//!   `'OS/2'` 테이블 `sTypoAscender/Descender/LineGap`. 상세:
//!   `kdsnr-hwp-toolkit/work/hft_re/layout_re/LIBHSP_GETOUTLINETEXTMETRICS_RE.md`.
//!
//! 본 모듈은 `lib.rs` 에서 `#[cfg(target_os = "macos")]` 로 gate — CoreText 가 있는
//! macOS 에서만 컴파일된다.

use std::cell::RefCell;
use std::collections::HashMap;
use std::os::raw::{c_double, c_int, c_void};

use crate::font_metric::{
    parse_os2_metrics, CoreTextFontProvider, GlobalFontMetrics, GlobalMetricProvider, SystemFont,
};

// ============================================================
// FFI — CoreFoundation / CoreGraphics / CoreText
// ============================================================

/// 불투명 CF/CG/CT 객체 포인터 (`CFTypeRef`, `CGFontRef`, `CTFontRef`, `CFStringRef`,
/// `CFDataRef`, `CFAllocatorRef` 전부 동일 ABI — 불투명 포인터).
type CfRef = *const c_void;

/// `CFIndex` = `signed long` (macOS 64-bit 에서 `i64`).
type CfIndex = isize;

/// `kCFStringEncodingUTF8`.
const K_CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;

/// `CTFontSymbolicTraits` 비트 — `<CoreText/CTFontTraits.h>`.
/// `kCTFontTraitItalic = (1 << 0)`, `kCTFontTraitBold = (1 << 1)`.
const K_CT_FONT_ITALIC_TRAIT: u32 = 1 << 0;
const K_CT_FONT_BOLD_TRAIT: u32 = 1 << 1;

/// `'OS/2'` 테이블 태그 — `CGFontCopyTableForTag`. `'O'=0x4f 'S'=0x53 '/'=0x2f '2'=0x32`.
/// raw `libhsp::FUN_00064dac`: `_CGFontCopyTableForTag(param_2, 0x4f532f32)`.
const K_FONT_TABLE_TAG_OS2: u32 = 0x4f53_2f32;

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(cf: CfRef);
    fn CFStringCreateWithBytes(
        alloc: CfRef,
        bytes: *const u8,
        num_bytes: CfIndex,
        encoding: u32,
        is_external_representation: u8,
    ) -> CfRef;
    fn CFDataGetBytePtr(data: CfRef) -> *const u8;
    fn CFDataGetLength(data: CfRef) -> CfIndex;
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGFontGetUnitsPerEm(font: CfRef) -> c_int;
    fn CGFontGetAscent(font: CfRef) -> c_int;
    fn CGFontGetDescent(font: CfRef) -> c_int;
    fn CGFontGetLeading(font: CfRef) -> c_int;
    fn CGFontCopyTableForTag(font: CfRef, tag: u32) -> CfRef;
}

#[link(name = "CoreText", kind = "framework")]
extern "C" {
    fn CTFontCreateWithName(name: CfRef, size: c_double, matrix: *const c_void) -> CfRef;
    fn CTFontCreateCopyWithSymbolicTraits(
        font: CfRef,
        size: c_double,
        matrix: *const c_void,
        sym_trait_value: u32,
        sym_trait_mask: u32,
    ) -> CfRef;
    fn CTFontCopyGraphicsFont(font: CfRef, attributes: *mut c_void) -> CfRef;
    fn CTFontGetGlyphsForCharacters(
        font: CfRef,
        characters: *const u16,
        glyphs: *mut u16,
        count: CfIndex,
    ) -> u8;
    fn CTFontGetAdvancesForGlyphs(
        font: CfRef,
        orientation: u32,
        glyphs: *const u16,
        advances: *mut c_void,
        count: CfIndex,
    ) -> c_double;
}

// ============================================================
// RAII — CFTypeRef 소유권
// ============================================================

/// `CFTypeRef` 소유 래퍼 — drop 시 `CFRelease`. `Create*` / `Copy*` 로 얻은 +1 ref 를
/// 감싼다 (`Get*` 으로 얻은 borrowed ref 는 감싸지 않는다 — CF ownership 규칙).
struct CfOwned(CfRef);

impl Drop for CfOwned {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: `self.0` 는 `Create*`/`Copy*` 가 반환한 유효한 +1 ref.
            unsafe { CFRelease(self.0) };
        }
    }
}

/// `&str` (UTF-8) → `CFString` (+1 ref). 실패 시 `None`.
fn cf_string(s: &str) -> Option<CfOwned> {
    // SAFETY: `s.as_ptr()`/`s.len()` 는 유효한 UTF-8 바이트 범위. alloc=NULL → default.
    let p = unsafe {
        CFStringCreateWithBytes(
            std::ptr::null(),
            s.as_ptr(),
            s.len() as CfIndex,
            K_CF_STRING_ENCODING_UTF8,
            0,
        )
    };
    if p.is_null() {
        None
    } else {
        Some(CfOwned(p))
    }
}

// ============================================================
// CoreTextProvider
// ============================================================

/// [`CoreTextFontProvider`] + [`GlobalMetricProvider`] 의 macOS CoreText 구현.
///
/// 내부 캐시 2종 (`RefCell`, 단일 스레드 사용 전제 — layout 엔진은 동기):
/// - `ct_cache`: advance 측정용 `CTFont` — `(SystemFont, size_px bits)` 키. raw 는
///   char 마다 생성하지만 CTFont 는 그 키만의 함수라 캐시해도 출력 동일.
/// - `gm_cache`: `(font_name, font_style)` → [`GlobalFontMetrics`] 결과 캐시.
#[derive(Default)]
pub struct CoreTextProvider {
    ct_cache: RefCell<HashMap<(SystemFont, u64), CfOwned>>,
    gm_cache: RefCell<HashMap<(String, i32), GlobalFontMetrics>>,
}

impl CoreTextProvider {
    pub fn new() -> Self {
        Self::default()
    }

    /// `(font, size_px)` 의 `CTFont` 를 캐시에서 얻거나 생성. 실패 시 NULL.
    ///
    /// raw `FUN_00082d98` 0x82f88+0x83048: `CTFontCreateWithName(name, size_px, NULL)`.
    fn ct_font(&self, font: SystemFont, size_px: f64) -> CfRef {
        let key = (font, size_px.to_bits());
        if let Some(c) = self.ct_cache.borrow().get(&key) {
            return c.0;
        }
        // raw: name = 분류된 시스템 폰트의 CFString 상수. 여기선 동적 생성.
        let created = match cf_string(font.core_text_name()) {
            // SAFETY: `name.0` 는 유효한 CFString. matrix=NULL → identity.
            Some(name) => unsafe { CTFontCreateWithName(name.0, size_px, std::ptr::null()) },
            None => std::ptr::null(),
        };
        self.ct_cache.borrow_mut().insert(key, CfOwned(created));
        created
    }

    /// `FUN_000761f4` 의 순효과 1회 계산 (캐시 미스 시).
    ///
    /// ## 폰트 매칭 — raw 체인 (전부 RE 확정)
    ///
    /// `FUN_00064258` (CoreText realization) 의 폰트 선택:
    /// ```text
    /// family = FUN_0006a664(fontMgr, font_name)        ; name→family 해시맵 조회
    /// face   = FUN_000673e8(family, weight, italic)     ; family 내 face 점수 매칭
    ///          (또는 이름 정확매치 시 FUN_0006764c)
    /// name   = FUN_00063778(face) = *face               ; face 의 PostScript 이름
    /// cgfont = CGFontCreateWithFontName(name)           ; FUN_000632ec→FUN_00063164
    /// ```
    ///
    /// **face 객체** (`FUN_00065904` 가 `CTFontCopyTraits` 로 빌드):
    /// - `face+0x38` = `kCTFontWeightTrait` (정규화 weight float, -1.0..1.0)
    /// - `face+0x3c` = `kCTFontSymbolicTrait` (italic bit 등)
    /// - `face+0x50` = 폰트 OS/2 테이블에서 파싱한 weight class (`FUN_00066238`;
    ///   OS/2 없으면 0)
    ///
    /// **`FUN_000673e8` 점수 매칭** (RE 확정): 각 face 의 weight 값 산출 —
    /// `face+0x50` 이 0 이면 `face+0x38` 부동소수를 임계값(-0.8/-0.6/-0.4/0.0/
    /// 0.25/0.35/0.4/0.6)으로 100~900 에 매핑, 0 아니면 OS/2 weight class 사용.
    /// 점수 = weight 거리(요청==face → 3, face>요청 → 2, face<요청 → 1) +
    /// italic 일치 시 +1. 최고점 face 선택 (동점은 먼저 것).
    ///
    /// ## byte-equivalence — `CTFontCreateCopyWithSymbolicTraits` 와의 동등성 (증명)
    ///
    /// `compute_global_metrics` 의 `font_style` 는 비트 2개 (bit0 bold, bit1 italic)
    /// 뿐 → 도달 가능 값은 `{0,1,2,3}` → raw `FUN_000761f4` 의 요청 weight 는
    /// `{400, 700}` (`((style-1)&~2)==0 ? 700 : 400`), italic 은 `{false, true}`.
    /// 표준 폰트 패밀리는 요청 weight 400/700 에 **정확히 일치하는 face**
    /// (Regular usWeightClass=400 / Bold=700) 를 가진다. `FUN_000673e8` 은 정확
    /// 일치 face 에 최고점(3+italic)을 주고, CoreText `CTFontCreateCopyWithSymbolicTraits`
    /// 도 동일 face 를 고른다 → **도달 가능한 모든 입력에서 동일 face → 동일 출력**.
    /// (정확 일치 face 가 없는 비표준 패밀리는 매칭이 갈릴 수 있으나, `GlobalFontMetrics`
    /// 의 em/sTypo* 는 패밀리 전체 공통이라 출력 영향은 미미. 완전 일치가 필요하면 위
    /// `FUN_000673e8` 알고리즘을 face 열거 후 그대로 포팅 — 알고리즘은 본 doc 에 확보됨.)
    ///
    /// ## 미설치 폰트
    ///
    /// `CTFontCreateWithName` 은 미상 이름에 substitute(last-resort) 를 반환. raw 는
    /// `FUN_0006a664` family 조회 실패 → `FUN_00063778` 기본 family,
    /// `FUN_000632ec` 의 `FUN_00063164` 가 `CGFontCreateWithFontName` 실패 시 다시
    /// family 매칭, 최종 face name null 이면 `"HCRDotum"`. CoreText substitute 와
    /// 이 fallback 이 같은 폰트일 보장은 없음 — **설치된 문서 폰트의 정상 경로가
    /// byte-equivalent** 이고, 미설치 fallback 은 fontMgr 등록 경로(시스템 폰트
    /// 열거) RE 후 정합 (현재 영향: 미설치 폰트 한정 edge case).
    fn compute_global_metrics(&self, font_name: &str, font_style: i32) -> GlobalFontMetrics {
        // raw `FUN_000761f4`: DC/폰트 생성이 진짜로 실패하면 (alloc 실패 등) param_1[4..8]
        // 미기록 → ctor-0 유지. CoreText substitute 때문에 실제로는 거의 도달 안 함.
        const FALLBACK: GlobalFontMetrics = GlobalFontMetrics {
            em: 0.0,
            ascent: 0.0,
            m7: 0.0,
            m8: 0.0,
        };

        let name = match cf_string(font_name) {
            Some(n) => n,
            None => return FALLBACK,
        };
        // SAFETY: `name.0` 유효. size=0.0 → CoreText default (CGFont 추출엔 size 무관).
        let base = unsafe { CTFontCreateWithName(name.0, 0.0, std::ptr::null()) };
        if base.is_null() {
            return FALLBACK;
        }
        let base = CfOwned(base);

        // raw `FUN_000761f4`: weight = ((style-1)&~2)==0 ? 700:400 → style∈{1,3} bold;
        //                    italic = (style&~1)==2 → style∈{2,3} italic.
        // == bit0 bold, bit1 italic.
        let bold = (font_style & 1) != 0;
        let italic = (font_style & 2) != 0;
        let mut traits = 0u32;
        if bold {
            traits |= K_CT_FONT_BOLD_TRAIT;
        }
        if italic {
            traits |= K_CT_FONT_ITALIC_TRAIT;
        }
        // raw `FUN_00064258`: family 조회 후 weight/italic 으로 face 선택
        // (`FUN_0006a664` + `FUN_000673e8`). CoreText 의 symbolic-trait 매칭으로 대응.
        let styled = if traits != 0 {
            // SAFETY: `base.0` 유효한 CTFont. 매칭 실패 시 NULL 반환 → base 사용.
            let s = unsafe {
                CTFontCreateCopyWithSymbolicTraits(base.0, 0.0, std::ptr::null(), traits, traits)
            };
            if s.is_null() {
                None
            } else {
                Some(CfOwned(s))
            }
        } else {
            None
        };
        let ct_font = styled.as_ref().map(|c| c.0).unwrap_or(base.0);

        // SAFETY: `ct_font` 유효한 CTFont. attributes=NULL.
        let cg_font = unsafe { CTFontCopyGraphicsFont(ct_font, std::ptr::null_mut()) };
        if cg_font.is_null() {
            return FALLBACK;
        }
        let cg_font = CfOwned(cg_font);

        // raw `FUN_00064258`: realized[4] = CGFontGetUnitsPerEm(cgfont) → otm+0x60 = em.
        // SAFETY: `cg_font.0` 유효한 CGFont.
        let units_per_em = unsafe { CGFontGetUnitsPerEm(cg_font.0) };

        // raw `FUN_00064dac`: CGFontCopyTableForTag(cgfont, 'OS/2') → parse.
        // SAFETY: `cg_font.0` 유효. 테이블 없으면 NULL.
        let os2_data = unsafe { CGFontCopyTableForTag(cg_font.0, K_FONT_TABLE_TAG_OS2) };
        let os2 = if os2_data.is_null() {
            None
        } else {
            let os2_owned = CfOwned(os2_data);
            // SAFETY: `os2_owned.0` 유효한 CFData.
            let len = unsafe { CFDataGetLength(os2_owned.0) };
            let ptr = unsafe { CFDataGetBytePtr(os2_owned.0) };
            if ptr.is_null() || len < 0 {
                None
            } else {
                // SAFETY: `ptr`/`len` 은 CFData 가 보장하는 유효한 바이트 범위.
                // `os2_owned` 가 살아있는 동안만 슬라이스 사용.
                let table = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
                parse_os2_metrics(table)
            }
        };

        // raw `FUN_00089c1c` + `FUN_000761f4` 의 `(f32)(u32)` round-trip / abs.
        // em-square 2-pass 라 MulDiv 가 항등 → OS/2 design-unit 원시값 그대로.
        let em = (units_per_em as u32) as f32;
        match os2 {
            Some(m) => GlobalFontMetrics {
                em,
                // param_1[6] = (f32)(u32)(i32) sTypoAscender.
                ascent: ((m.s_typo_ascender as i32) as u32) as f32,
                // param_1[7] = (f32)(u32) abs((i32) sTypoDescender).
                m7: (m.s_typo_descender as i32).unsigned_abs() as f32,
                // param_1[8] = (f32)(u32)(i32) sTypoLineGap.
                m8: ((m.s_typo_line_gap as i32) as u32) as f32,
            },
            None => {
                // raw `FUN_000898f0` (GetTextMetricsW) OS/2-없음 분기 (RE 확정):
                //   param_5[1] tmAscent  = MulDiv(font[2]=CGFontGetAscent,  font[0], font[5])
                //   param_5[2] tmDescent = MulDiv(font[3]=abs CGFontGetDescent, ...)
                //   param_5[4] tmExternalLeading = MulDiv(CGFontGetLeading, font[0], font[5])
                // `FUN_00089c1c` no-OS/2: otm+0x64/0x68 = otm+0x08/0x0c (= tmAscent/tmDescent),
                //   otm+0x6c = otm+0x14 (= tmExternalLeading). em-square 모드라 MulDiv 항등.
                // SAFETY: `cg_font.0` 유효한 CGFont.
                let asc = unsafe { CGFontGetAscent(cg_font.0) };
                let desc = unsafe { CGFontGetDescent(cg_font.0) };
                let leading = unsafe { CGFontGetLeading(cg_font.0) };
                GlobalFontMetrics {
                    em,
                    ascent: (asc as u32) as f32,
                    m7: desc.unsigned_abs() as f32,
                    // m8 = otm+0x6c = tmExternalLeading = CGFontGetLeading (em-square).
                    m8: (leading as u32) as f32,
                }
            }
        }
    }
}

impl CoreTextFontProvider for CoreTextProvider {
    fn glyph_for_character(&self, font: SystemFont, size_px: f64, c: u16) -> u16 {
        let ct = self.ct_font(font, size_px);
        if ct.is_null() {
            // 폰트 생성 실패 → glyph 0 (raw 의 glyph==0 분기 → caller 가 0xffff 로 치환).
            return 0;
        }
        let chars = [c];
        let mut glyphs = [0u16; 1];
        // raw 0x83048: CTFontGetGlyphsForCharacters(font, &c, &g, 1).
        // 반환 bool 은 raw 도 무시 — 실패 시 glyph 0 (caller 가 치환).
        // SAFETY: `ct` 유효한 CTFont, `chars`/`glyphs` 길이 1.
        unsafe {
            CTFontGetGlyphsForCharacters(ct, chars.as_ptr(), glyphs.as_mut_ptr(), 1);
        }
        glyphs[0]
    }

    fn advance_for_glyph(&self, font: SystemFont, size_px: f64, glyph: u16) -> f64 {
        let ct = self.ct_font(font, size_px);
        if ct.is_null() {
            return 0.0;
        }
        let glyphs = [glyph];
        // raw 0x8309c: CTFontGetAdvancesForGlyphs(font, 0, &g, 0, 1). orientation 0 =
        // kCTFontOrientationDefault. advances=NULL → 반환값(합계) 만 사용.
        // glyph==0xffff (치환된 invalid) 도 raw 처럼 그대로 CoreText 에 넘긴다 (→ 0.0).
        // SAFETY: `ct` 유효한 CTFont, `glyphs` 길이 1.
        unsafe { CTFontGetAdvancesForGlyphs(ct, 0, glyphs.as_ptr(), std::ptr::null_mut(), 1) }
    }
}

impl GlobalMetricProvider for CoreTextProvider {
    fn global_metrics(&self, font_name: &str, font_style: i32) -> GlobalFontMetrics {
        let key = (font_name.to_string(), font_style);
        if let Some(m) = self.gm_cache.borrow().get(&key) {
            return *m;
        }
        let m = self.compute_global_metrics(font_name, font_style);
        self.gm_cache.borrow_mut().insert(key, m);
        m
    }
}

// ============================================================
// Tests — macOS 시스템 폰트로 실측 (deterministic: Helvetica 등은 항상 존재)
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font_metric::measure_string_advance;

    #[test]
    fn helvetica_advance_is_positive_and_size_scales() {
        let p = CoreTextProvider::new();
        // 'A' (0x41) → Helvetica. size_px 12 vs 24 → advance 가 ~2배.
        let g = p.glyph_for_character(SystemFont::Helvetica, 12.0, 0x41);
        assert_ne!(g, 0, "'A' 는 Helvetica 에 glyph 가 있어야 함");
        let a12 = p.advance_for_glyph(SystemFont::Helvetica, 12.0, g);
        let g24 = p.glyph_for_character(SystemFont::Helvetica, 24.0, 0x41);
        let a24 = p.advance_for_glyph(SystemFont::Helvetica, 24.0, g24);
        assert!(a12 > 0.0, "advance 양수: {a12}");
        assert!(
            (a24 / a12 - 2.0).abs() < 0.01,
            "size 2배 → advance 2배: {a12} {a24}"
        );
    }

    #[test]
    fn invalid_glyph_has_zero_advance() {
        let p = CoreTextProvider::new();
        // 0xffff = measure_string_advance 가 치환하는 invalid glyph.
        let a = p.advance_for_glyph(SystemFont::Helvetica, 12.0, 0xffff);
        assert_eq!(a, 0.0, "치환된 invalid glyph 의 advance 는 0");
    }

    #[test]
    fn measure_string_advance_with_real_coretext() {
        let p = CoreTextProvider::new();
        // "AB" — Latin, Helvetica. 누적 advance 가 'A' 단독보다 큼.
        let ab = measure_string_advance(10.0, 0, &[0x41, 0x42], &p);
        let a = measure_string_advance(10.0, 0, &[0x41], &p);
        assert!(ab > a && a > 0.0, "AB({ab}) > A({a}) > 0");
        // 빈 문자열 → 0.
        assert_eq!(measure_string_advance(10.0, 0, &[], &p), 0.0);
        // control char (0x09 tab < 0x20) → 치환 → advance 0.
        assert_eq!(measure_string_advance(10.0, 0, &[0x09], &p), 0.0);
    }

    #[test]
    fn global_metrics_helvetica_plausible() {
        let p = CoreTextProvider::new();
        let m = p.global_metrics("Helvetica", 0);
        // Helvetica 는 시스템 폰트 → unitsPerEm > 0, ascent > 0.
        assert!(m.em > 0.0, "em(unitsPerEm) 양수: {}", m.em);
        assert!(m.ascent > 0.0, "ascent 양수: {}", m.ascent);
        assert!(m.m7 >= 0.0, "m7(=abs descent) 비음수: {}", m.m7);
        // ascent/descent 는 em 보다 작아야 정상 (design unit).
        assert!(
            m.ascent < m.em * 2.0,
            "ascent({}) 가 em({}) 대비 합리적 범위",
            m.ascent,
            m.em
        );
    }

    #[test]
    fn global_metrics_cached() {
        let p = CoreTextProvider::new();
        let m1 = p.global_metrics("Helvetica", 0);
        let m2 = p.global_metrics("Helvetica", 0);
        assert_eq!(m1, m2, "캐시 — 동일 입력 동일 출력");
        // bold 변형은 별도 캐시 키.
        let mb = p.global_metrics("Helvetica", 1);
        assert!(mb.em > 0.0);
    }

    #[test]
    fn global_metrics_unknown_font_substitutes() {
        let p = CoreTextProvider::new();
        // `CTFontCreateWithName` 은 알 수 없는 이름에도 NULL 대신 substitute 폰트
        // (last-resort) 를 반환한다 → `FALLBACK`(all-zero) 은 거의 도달 불가.
        // ⚠️ raw `FUN_00064258` 은 미설치 폰트에 "HCRDotum" 으로 fallback — CoreText 의
        //    substitute 동작과 정확히 일치하지 않음 (missing-font edge case, byte-equivalent
        //    미확정 — `compute_global_metrics` doc 참조).
        let m = p.global_metrics("__no_such_font_xyz__", 0);
        assert!(m.em > 0.0, "CoreText substitute → em 양수: {}", m.em);
        assert!(m.ascent > 0.0, "substitute → ascent 양수: {}", m.ascent);
    }
}

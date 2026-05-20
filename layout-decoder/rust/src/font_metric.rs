//! `Hnc::Shape::Text` 의 **font metric 경계** — `CharItemView::CharItemView` ctor 가
//! glyph 의 ascent/descent/advance 를 채우는 경로 (`GetRealFont` → `FUN_000764fc` →
//! `FUN_00082d98`).
//!
//! ## RE 결론 (11번째 세션, import 해석 + `otool -L` 검증)
//!
//! macOS `libHncDrawingEngine` 의 layout font metric 은 **전부 macOS CoreText 로 귀결**:
//! - per-glyph advance — `FUN_00082d98` 가 `CTFontGetAdvancesForGlyphs` 를 **하드코딩된
//!   시스템 폰트** (`Helvetica` / `Helvetica-Bold` / `STHeitiTC-Medium` /
//!   `AppleSDGothicNeo-Medium` / `AppleSDGothicNeo-Bold`) 로 호출. codepoint 분류로 폰트 선택.
//! - font global metric (em / ascent / descent) — `FUN_000761f4` 가
//!   `libhsp.dylib::CreateFontIndirectW` + `GetOutlineTextMetricsW` 로. `libhsp.dylib` 는
//!   macOS 시스템 프레임워크만 link 하는 순수 CoreText-backed GDI emulation shim.
//! - HFT 독자 폰트 metric 경로 없음. glyph **shape** 렌더링은 별개 경로 (실제 폰트 embed).
//!
//! end-to-end byte-equivalent 위해선 Rust 포트도 CoreText 로 동일 측정 — 본 모듈은 그 중
//! **순수 산술/로직 부분** (codepoint→폰트 분류기, DPI 공식, string→advance 측정 루프,
//! glyph 치환 규칙) 을 1:1 포팅. 실제 CoreText 호출 (`CTFontGetGlyphsForCharacters` /
//! `CTFontGetAdvancesForGlyphs`) 만 FFI 경계 ([`CoreTextFontProvider`]).
//!
//! raw 출처: `bcompositor/font_metric_deps.txt` (`FUN_00082d98` @ 0x82d98),
//! `bcompositor/font_imports.txt` (DAT 상수 + import 해석).

/// `FUN_00082d98` 의 codepoint 분류기가 선택하는 macOS 시스템 폰트.
///
/// raw: `0x796000` 영역 CFString 상수 — Ghidra 심볼명으로 확정.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SystemFont {
    /// `"Helvetica"` (`0x796200`) — Latin/default, non-bold.
    Helvetica,
    /// `"Helvetica-Bold"` (`0x7961e0`) — Latin/default, bold.
    HelveticaBold,
    /// `"STHeitiTC-Medium"` (`0x7961c0`) — CJK (Hanja 등). bold 변형 없음.
    StHeitiTcMedium,
    /// `"AppleSDGothicNeo-Medium"` (`0x7961a0`) — Hangul, non-bold.
    AppleSdGothicNeoMedium,
    /// `"AppleSDGothicNeo-Bold"` (`0x796180`) — Hangul, bold.
    AppleSdGothicNeoBold,
}

impl SystemFont {
    /// CoreText `CTFontCreateWithName` 에 넘기는 폰트명.
    pub fn core_text_name(self) -> &'static str {
        match self {
            SystemFont::Helvetica => "Helvetica",
            SystemFont::HelveticaBold => "Helvetica-Bold",
            SystemFont::StHeitiTcMedium => "STHeitiTC-Medium",
            SystemFont::AppleSdGothicNeoMedium => "AppleSDGothicNeo-Medium",
            SystemFont::AppleSdGothicNeoBold => "AppleSDGothicNeo-Bold",
        }
    }
}

/// raw `DAT_00742858` — Hangul lane 분류용 offset 테이블 (u16×4).
/// `FUN_00082d98` ASM `0x82eec ldr d11,[x8,#0x858]` → `add v0,v0,v11`.
const HANGUL_LANE_OFFSET: [u16; 4] = [0x56a0, 0x2850, 0x2835, 0xced0];

/// raw `DAT_00742860` — Hangul lane 분류용 limit 테이블 (u16×4).
/// `FUN_00082d98` ASM `0x82ef4 ldr d12,[x8,#0x860]` → `cmhi v0,v12,v0` (unsigned `v12 > v0`).
const HANGUL_LANE_LIMIT: [u16; 4] = [0x001d, 0x0017, 0x0031, 0x0060];

/// `FUN_00082d98` (raw `0x82f04`-`0x82f70`) 의 codepoint→폰트 분류기 1:1 포팅.
///
/// `c` = UTF-16 code unit, `bold` = `font_render_obj.flag & 1` (raw `w28` bit 0).
///
/// raw 분기 (순서대로):
/// ```text
/// 0x82f04-1c  Hangul lane: any i: (u16)(c + DAT858[i]) < DAT860[i]   → AppleSDGothicNeo
/// 0x82f20-34  Hangul#2: w10 = ((c+0x5400)>>2)&0x3fff;
///             ccmp 로 `w10 < 0xae9  ||  (c & 0xff00) == 0x1100`       → AppleSDGothicNeo
///             (w10 < 0xae9 ⟺ c ∈ [0xAC00,0xD7A3] Hangul Syllables)
/// 0x82f38-3c  (c & 0xe000) != 0                                       → STHeitiTC
/// 0x82f40-4c  (c & 0xff80) == 0x2e80                                  → STHeitiTC
/// 0x82f50-60  (u16)(c + 0xcfc0) < 0xc0                                → STHeitiTC
/// 0x82f64-70  (c & 0xfff0) == 0x31f0                                  → STHeitiTC
/// 0x82f74-78  else                                                    → Helvetica
/// ```
/// AppleSDGothicNeo / Helvetica 는 `bold` 로 `-Bold` / `-Medium`(`Helvetica`) 분기
/// (raw `0x82fa0` / `0x82f74` 의 `tbnz w28,#0x0`). STHeitiTC 는 bold 변형 없음.
pub fn select_system_font(c: u16, bold: bool) -> SystemFont {
    // raw 0x82f04-1c — Hangul lane check (NEON 4-lane `cmhi`).
    let hangul_lane = (0..4).any(|i| {
        c.wrapping_add(HANGUL_LANE_OFFSET[i]) < HANGUL_LANE_LIMIT[i]
    });
    // raw 0x82f20-34 — Hangul#2: w10 = ((c+0x5400)>>2) & 0x3fff.
    //   ccmp 로직: `cmp w10,#0xae9; ccmp w9,#0x1100,#4(Z),cs; b.eq` →
    //   taken if (w10 >= 0xae9 ? w9 == 0x1100 : true) = `w10 < 0xae9 || (c&0xff00)==0x1100`.
    let w10 = ((c as u32).wrapping_add(0x5400) >> 2) & 0x3fff;
    let hangul_2 = w10 < 0xae9 || (c & 0xff00) == 0x1100;

    if hangul_lane || hangul_2 {
        // raw 0x82fa0: tbnz w28,#0x0 → -Bold(0x170) / -Medium(0x190).
        return if bold {
            SystemFont::AppleSdGothicNeoBold
        } else {
            SystemFont::AppleSdGothicNeoMedium
        };
    }

    // raw 0x82f38-70 — CJK (STHeitiTC-Medium, bold 변형 없음).
    if (c & 0xe000) != 0
        || (c & 0xff80) == 0x2e80
        || c.wrapping_add(0xcfc0) < 0xc0
        || (c & 0xfff0) == 0x31f0
    {
        return SystemFont::StHeitiTcMedium;
    }

    // raw 0x82f74-78 — Latin/default (Helvetica).
    if bold {
        SystemFont::HelveticaBold
    } else {
        SystemFont::Helvetica
    }
}

/// Win32 `MulDiv` — `libhsp.dylib::_MulDiv` (`0xb42a4`) 1:1 포팅.
///
/// `(number * numerator) / denominator`, **ties-away-from-zero** 반올림. `denominator == 0`
/// 또는 결과가 `i32` 범위를 벗어나면 `-1`.
///
/// `FUN_000764fc` / `FUN_000761f4` / `FUN_00082d98` 가 pt→px DPI 변환 (`MulDiv(size, 0x60,
/// 0x48)` = `size * 96 / 72`) 에 사용.
///
/// raw (`libhsp.dylib` arm64 disasm):
/// ```text
/// 0xb42a4  cbz w2 → return -1                 ; denom == 0
/// 0xb42ac  cneg w9, w0, lt                    ; n9 = (denom < 0) ? -number : number
/// 0xb42b0  cneg w8, w2, mi                    ; n8 = abs(denom)
/// 0xb42b4  smull x9, w9, w1                   ; product = (i64)n9 * (i64)numerator
/// 0xb42b8  lsr w10, w8, #1                    ; bias = n8 >> 1
/// 0xb42bc  tbnz x9, #0x3f → 0xb42d8           ; product < 0 → negative path
/// 0xb42c0  add x9, x9, x10 ; udiv x0, x9, x8  ; result = (product + bias) / n8
/// 0xb42c8  lsr x8, x0, #31 ; cbz x8 → ret     ; (result >> 31) != 0 → -1
/// 0xb42d8  sub x9, x10, x9 ; udiv x8, x9, x8  ; q = (bias - product) / n8
/// 0xb42e4  neg x0, x8                         ; result = -q
/// 0xb42e8  cmp x0, #-0x80000000 ; b.lt → -1   ; result < i32::MIN → -1
/// ```
pub fn mul_div(number: i32, numerator: i32, denominator: i32) -> i32 {
    // raw 0xb42a4: cbz w2 → -1.
    if denominator == 0 {
        return -1;
    }
    // raw 0xb42ac-b0: denom 부호를 number 로 fold, denom 은 abs.
    let n9: i64 = if denominator < 0 {
        -(number as i64)
    } else {
        number as i64
    };
    let n8: i64 = (denominator as i64).abs();
    // raw 0xb42b4: smull — signed 64-bit product.
    let product: i64 = n9 * numerator as i64;
    // raw 0xb42b8: lsr w10,w8,#1 — 반올림 bias = floor(abs(denom)/2).
    let bias: i64 = n8 >> 1;
    // raw 0xb42bc: tbnz x9,#0x3f — product 부호 분기.
    if product >= 0 {
        // raw 0xb42c0-cc: result = (product+bias)/n8; (result>>31)!=0 면 overflow.
        let result: i64 = (product + bias) / n8;
        if (result >> 31) != 0 {
            -1
        } else {
            result as i32
        }
    } else {
        // raw 0xb42d8-f0: q = (bias - product)/n8 (= (bias+|product|)/n8); result = -q.
        let q: i64 = (bias - product) / n8;
        let result: i64 = -q;
        if result < i32::MIN as i64 {
            -1
        } else {
            result as i32
        }
    }
}

/// `FUN_000761f4` (`0x761f4`) 가 채우는 폰트 **global metric** — `font_render_obj`
/// (`CharItemView+0x30` 의 SharePtr managed object) 의 `+0x10`/`+0x18`/`+0x1c`/`+0x20`.
///
/// raw: `FUN_000761f4` 가 `libhsp.dylib::CreateFontIndirectW` (font face name 으로) +
/// `GetOutlineTextMetricsW` 로 추출 → `param_1[4]`(em) / `param_1[6]`(ascent) /
/// `param_1[7]`(`abs(local_274)`) / `param_1[8]`. **`libhsp::GetOutlineTextMetricsW` 의
/// `OUTLINETEXTMETRICW` 비표준 layout 은 후속 RE 대상** — 본 struct 는 그 출력 경계.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlobalFontMetrics {
    /// `font_render_obj[4]` (`+0x10`) — em-size. raw `param_1[4] = (f32)NEON_ucvtf(local_2a0)`.
    pub em: f32,
    /// `font_render_obj[6]` (`+0x18`) — ascent. raw `param_1[6] = (f32)NEON_ucvtf(local_278)`.
    pub ascent: f32,
    /// `font_render_obj[7]` (`+0x1c`) — raw `param_1[7] = (f32)abs(local_274)`.
    pub m7: f32,
    /// `font_render_obj[8]` (`+0x20`) — raw `param_1[8] = (f32)NEON_ucvtf(local_270)`.
    pub m8: f32,
}

/// OS/2 테이블의 typographic metric (font design unit). `parse_os2_metrics` 출력.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Os2Metrics {
    /// OS/2 `sTypoAscender` (raw 0x44, i16 BE).
    pub s_typo_ascender: i16,
    /// OS/2 `sTypoDescender` (raw 0x46, i16 BE).
    pub s_typo_descender: i16,
    /// OS/2 `sTypoLineGap` (raw 0x48, i16 BE).
    pub s_typo_line_gap: i16,
}

/// raw OS/2 테이블 → typographic metric 파싱. `libhsp::FUN_00082eec` (OS/2 파서, 10404B)
/// 1:1 — 표준 OpenType OS/2 레이아웃을 **big-endian** 으로 순차 디코딩한다.
///
/// `FUN_00082eec` 는 raw 테이블을 `param_1` (`ushort*`) 에 그대로 옮긴다
/// (`param_1[0x23] = (local_48[0x44]<<8) | local_48[0x45]` 등 — parsed struct = raw 의
/// big-endian 사본). `FUN_00089c1c` 가 parsed struct `+0x46` 을 sTypoAscender 로 읽으니
/// raw 오프셋 0x44 = sTypoAscender (표준 OpenType 레이아웃과 일치):
/// - sTypoAscender  @ 0x44 (i16 BE)
/// - sTypoDescender @ 0x46 (i16 BE)
/// - sTypoLineGap   @ 0x48 (i16 BE)
///
/// **유일한 가드**: `FUN_00082eec` 의 `if (param_3 < 0x56) goto LAB_00085774` — 테이블
/// 크기가 0x56(86) 미만이면 파싱 실패 (`FUN_00064dac` 가 `0` 반환 → OS/2 없음 → fallback).
///
/// `FUN_00082eec` 의 `FUN_00027ba8(&reader, ptr, len, 0x10e1)` + `if (local_38 == 0x10e1)`
/// 는 **검증이 아니다** (RE 확정): `FUN_00027ba8` 은 reader-struct init
/// (`{start, cursor, len, flags}` 4-필드 대입) 일 뿐이고, ASM `0x82f1c mov w3,#0x10e1`
/// 로 리터럴 `0x10e1` 을 4번째 인자로 넘긴 뒤 그 필드를 `0x10e1` 과 비교 — **항상 참인
/// tautology**. 추가 OS/2 검증 없음. raw 출처: `LIBHSP_GETOUTLINETEXTMETRICS_RE.md` §6.
pub fn parse_os2_metrics(table: &[u8]) -> Option<Os2Metrics> {
    // raw `FUN_00082eec` 0x82ef4: param_3 < 0x56 → 실패.
    if table.len() < 0x56 {
        return None;
    }
    let be16 = |o: usize| i16::from_be_bytes([table[o], table[o + 1]]);
    Some(Os2Metrics {
        s_typo_ascender: be16(0x44),
        s_typo_descender: be16(0x46),
        s_typo_line_gap: be16(0x48),
    })
}

/// `Hnc::Shape::Text::FUN_000761f4` (`0x761f4`) — font global metric 추출의 출력 경계.
///
/// `FUN_000761f4` 는 `font_render_obj` (`param_1`) 의 `[0]`size / `[1]`style /
/// `[2]`facename 으로 LOGFONTW 를 만들어 `libhsp::CreateFontIndirectW` +
/// `GetOutlineTextMetricsW` 를 **2번** 호출한다:
/// 1. `lfHeight = MulDiv(size, 0x60, 0x48)` (양수) — `otm+0x60` = em
///    (`CGFontGetUnitsPerEm`) 만 읽음.
/// 2. `lfHeight = -(1차 em)` (음수, **em-square 모드**) — 재측정.
///
/// em-square 모드에선 realized-font 의 MulDiv numerator(`+0x00`) == denominator(`+0x14`)
/// == `unitsPerEm` → `FUN_00089c1c` 의 `MulDiv(OS/2.sTypo*, _, _)` 가 항등이 되어 결과가
/// **`font_size` 와 무관** (design-unit 원시값). 따라서 본 trait 은 size 를 받지 않는다.
///
/// 순효과 (`LIBHSP_GETOUTLINETEXTMETRICS_RE.md` §7):
/// - `em`     = `(f32)(u32) CGFontGetUnitsPerEm`
/// - `ascent` = `(f32)(u32) OS/2.sTypoAscender`        (없으면 `CGFontGetAscent`)
/// - `m7`     = `(f32)(u32) abs(OS/2.sTypoDescender)`  (없으면 `abs(CGFontGetDescent)`)
/// - `m8`     = `(f32)(u32) OS/2.sTypoLineGap`         (없으면 미확정 — fallback `0.0`)
///
/// **폰트 = run 의 실제 문서 폰트** (`font_render_obj[2]` face name). advance 경로의
/// [`select_system_font`] (Helvetica/STHeitiTC/AppleSDGothicNeo) 과 **다른 경로** —
/// global metric 은 run 의 실제 폰트, advance 는 per-char 분류 시스템 폰트.
pub trait GlobalMetricProvider {
    /// `font_name` = `font_render_obj` 의 face name (run 의 실제 문서 폰트).
    /// `font_style` 비트: bit0 = bold, bit1 = italic — raw `FUN_000761f4` 의
    /// `weight = ((style-1)&~2)==0 ? 700:400`, `italic = (style&~1)==2` 와 동일.
    fn global_metrics(&self, font_name: &str, font_style: i32) -> GlobalFontMetrics;
}

/// `FUN_000764fc` 가 `param_6` (= `CharItemView + 0x38`, ctor ASM `002efc00 add x5,x24,#0x8`
/// 로 검증, `x24 = this+0x30`) 에 쓰는 5 float.
///
/// **정정**: 기존 `glyph.rs` `CharItemView` 주석의 "`+0x4c` width (`FUN_000764fc` 에서 set)"
/// 는 오기 — `FUN_000764fc` 는 `this+0x38..0x48` 만 쓴다. advance-derived **width 는
/// `+0x3c`**, ascent `+0x40`, descent(`m7`) `+0x44`, `m8` `+0x48`. `+0x4c`/`+0x50` 은
/// ctor 가 `stp q0,q0,[this+0x40]` 로 0 초기화한 뒤 `FUN_000764fc` 가 안 덮어써서 0 으로 남음.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CharCtorMetrics {
    /// `CharItemView+0x38` — `param_6[0]` = `(f32)(u32)em` (round-trip).
    pub em: f32,
    /// `CharItemView+0x3c` — `param_6[1]` = `advance * 72.0 / 96.0`. glyph 의 layout width.
    pub width: f32,
    /// `CharItemView+0x40` — `param_6[2]` = `(f32)(u32)ascent * mul / (f32)(u32)em`.
    pub ascent: f32,
    /// `CharItemView+0x44` — `param_6[3]` = `(f32)(u32)m7 * mul / (f32)(u32)em`.
    pub m7: f32,
    /// `CharItemView+0x48` — `param_6[4]` = `(f32)(u32)m8 * mul / (f32)(u32)em`.
    pub m8: f32,
}

/// `Hnc::Shape::Text::FUN_000764fc` (`0x764fc`) 의 metric 결합 공식 1:1 포팅.
///
/// `font_render_obj` 의 global metric (`FUN_000761f4` 출력) + glyph advance
/// (`FUN_00082d98` 출력, CoreText) 를 결합해 `CharItemView` 의 ctor metric 영역을 채운다.
///
/// raw (`font_metric_deps.txt` `FUN_000764fc` 0x765a0-0x7662c):
/// ```text
/// d9  = param_1[6],[7]          ; s8 = param_1[8] ; s10 = param_1[4](em) ; s0 = param_1[0](size)
/// w0  = MulDiv((i32)size, 0x60, 0x48)                ; fVar4 = (f32)w0 = mul
/// em_r= (f32)(u32)em ; p6_r=(f32)(u32)p6 ; p7_r=(f32)(u32)p7 ; p8_r=(f32)(u32)p8   (fcvtzu+ucvtf)
/// d4  = advance (FUN_00082d98 출력, sp+0x40)
/// param_6[0] = em_r
/// param_6[1] = advance * 72.0 / 96.0
/// param_6[2] = p6_r * mul / em_r
/// param_6[3] = p7_r * mul / em_r
/// param_6[4] = p8_r * mul / em_r
/// ```
///
/// `(f32)(u32)x` round-trip = ARM `fcvtzu`(saturating, toward-zero) + `ucvtf`. Rust 의
/// `x as u32`(1.45+ saturating) + `as f32` 와 동일. `(i32)size` = `fcvtzs`. raw 는 `em_r == 0`
/// 가드 없음 → 본 포트도 그대로 (division → inf/nan, raw 와 동일).
pub fn combine_char_metrics(
    font_size: f32,
    global: &GlobalFontMetrics,
    advance: f32,
) -> CharCtorMetrics {
    // raw 0x765b0-c0: w0 = MulDiv((i32)font_size, 0x60, 0x48); fVar4 = (f32)w0.
    let mul = mul_div(font_size as i32, 0x60, 0x48) as f32;
    // raw 0x765c4-d8: fcvtzu + ucvtf round-trip on em / p6 / p7 / p8.
    let em_r = (global.em as u32) as f32;
    let p6_r = (global.ascent as u32) as f32;
    let p7_r = (global.m7 as u32) as f32;
    let p8_r = (global.m8 as u32) as f32;
    CharCtorMetrics {
        em: em_r,
        width: advance * 72.0 / 96.0,
        ascent: p6_r * mul / em_r,
        m7: p7_r * mul / em_r,
        m8: p8_r * mul / em_r,
    }
}

/// `FUN_00082d98` 의 CoreText FFI 경계 — per-glyph 의 두 CoreText 호출.
///
/// `FUN_00082d98` 는 char string 을 순회하며 char 마다:
/// 1. codepoint 분류 ([`select_system_font`]) 로 시스템 폰트 선택,
/// 2. `CTFontCreateWithName(size_px, font_name, NULL)` 로 CTFont 생성,
/// 3. `CTFontGetGlyphsForCharacters(font, &c, &glyph, 1)` 로 glyph id 조회,
/// 4. glyph 치환 규칙 적용 ([`measure_string_advance`] 가 1:1 포팅 — **순수 로직, FFI 아님**),
/// 5. `CTFontGetAdvancesForGlyphs(font, 0, &glyph, NULL, 1)` 로 advance 측정,
/// 6. `CFRelease(font)`.
///
/// 본 trait 은 그 중 **순수 CoreText 호출만** 추상화한다 (`glyph_for_character` = 2+3,
/// `advance_for_glyph` = 5). 치환 규칙(4) 은 [`measure_string_advance`] 안에서 raw 와 1:1.
///
/// raw 는 `CGFontCreateWithFontName` 도 char 마다 생성/`CFRelease` 하지만 그 CGFont 는
/// **한 번도 쓰이지 않는다** (`uVar6` — 생성 직후 release, 어떤 출력에도 영향 없음). 따라서
/// 본 trait 은 모델하지 않는다. 마찬가지로 `CGContextSaveGState`/`SetTextMatrix`/
/// `RestoreGState`/`FUN_00084514` 는 그래픽스 컨텍스트의 text matrix(italic skew 포함) 를
/// 설정할 뿐 — `CTFontGetAdvancesForGlyphs` 의 advance 는 font-intrinsic 이라 영향 없음.
pub trait CoreTextFontProvider {
    /// raw `0x82f88`+`0x83048`: `CTFontCreateWithName(size_px, font, NULL)` 로 CTFont 를
    /// 만들고 `CTFontGetGlyphsForCharacters(font, &c, &glyph_out, 1)` 로 code unit `c` 의
    /// glyph id 를 얻는다. CTFont 는 size 를 받지만 glyph id 는 size-independent.
    fn glyph_for_character(&self, font: SystemFont, size_px: f64, c: u16) -> u16;

    /// raw `0x8309c`: `CTFontGetAdvancesForGlyphs(font, 0, &glyph, NULL, 1)` 로 `glyph` 의
    /// advance(point) 를 측정. `glyph == 0xffff` (치환된 invalid glyph) 면 CoreText 가
    /// advance `0.0` 을 반환 — 구현체는 이를 동일하게 재현해야 한다.
    fn advance_for_glyph(&self, font: SystemFont, size_px: f64, glyph: u16) -> f64;
}

/// `Hnc::Shape::Text::FUN_00082d98` (`0x82d98`) 의 string→advance 측정 1:1 포팅.
///
/// `font_render_obj` (`CharItemView+0x30` SharePtr managed) 의 `+0x00` size(f32) /
/// `+0x04` style(i32) 와 char string 으로, 문자열 전체의 누적 advance(point) 를 반환한다.
/// [`combine_char_metrics`] 의 `advance` 입력이 이 값이다.
///
/// `style` 비트필드: bit0 = bold, bit1 = italic. advance 출력에는 **bold 만** 영향
/// (italic 은 text matrix skew 라 advance 와 무관 — 위 [`CoreTextFontProvider`] 참조).
///
/// raw (`font_metric_deps.txt` `FUN_00082d98`):
/// ```text
/// 0x82ddc  if (param_1+0x20 == 0) return;          ; CGContext 없으면 출력 없음 (아래 NOTE)
/// 0x82e94  iVar4 = MulDiv((i32)size, 0x60, 0x48)
/// 0x82eac  dVar14 = (f64)iVar4 * 72.0 / 96.0       ; CTFont point size
/// 0x82ec8  if ((i32)len < 1) fVar12 = 0.0;
///   else  for each char c in string:
/// 0x82f04   font = classify(c, bold=style&1)        ; == select_system_font
/// 0x83048   CTFontGetGlyphsForCharacters(font, &c, &g, 1)
/// 0x83054   if (c < 0x20)                  g = -1
/// 0x83064   else if (g == 0 || (u32)(c-0x7f) < 0x21 || c == 0x200b) g = -1
/// 0x8309c   dVar13 = CTFontGetAdvancesForGlyphs(font, 0, &g, 0, 1)
/// 0x830b4   dVar15 += dVar13
/// 0x830c8   fVar12 = (f32)dVar15
/// ```
///
/// **NOTE — `param_1+0x20` (CGContext) 가드**: raw 는 그래픽스 컨텍스트가 없으면 함수 전체를
/// skip 한다 (`param_7` 출력 미기록). 이 가드는 "렌더 대상 컨텍스트가 유효한가" 의 precondition
/// 으로, advance 측정 자체는 CGContext 와 무관하다 (`CTFontGetAdvancesForGlyphs` 는
/// font-intrinsic). 본 포트는 컨텍스트가 유효한 경로 (= 유일하게 출력을 내는 경로) 만 모델하며,
/// CGContext 유효성은 [`CoreTextFontProvider`] 구현이 전제한다.
pub fn measure_string_advance(
    font_size: f32,
    font_style: i32,
    string: &[u16],
    provider: &dyn CoreTextFontProvider,
) -> f32 {
    // raw 0x82e94-ec4: dVar14 = (f64)MulDiv((i32)size, 0x60, 0x48) * 72.0 / 96.0.
    let size_px = mul_div(font_size as i32, 0x60, 0x48) as f64 * 72.0 / 96.0;
    // raw 0x82f74 `tbnz w28,#0x0` 등: bold = style bit 0.
    let bold = (font_style & 1) != 0;

    // raw 0x82ec8 `cmp w21,#0x1; b.lt`: len < 1 → fVar12 = 0.0. 빈 string 이면 loop 0회.
    let mut acc: f64 = 0.0;
    for &c in string {
        // raw 0x82f04-70: codepoint→시스템폰트 분류 (== select_system_font).
        let font = select_system_font(c, bold);
        // raw 0x83048: CTFontGetGlyphsForCharacters.
        let raw_glyph = provider.glyph_for_character(font, size_px, c);
        // raw 0x83050-84: glyph 치환. `uVar1 < 0x20` → -1; 아니면 glyph 가 0 이거나
        // `(u32)(c-0x7f) < 0x21` ([0x7f,0x9f]) 이거나 `c == 0x200b` (ZWSP) → -1.
        // `(u32)(c-0x7f)` 는 unsigned wrap (ASM `sub w10,w8,#0x7f; cmp w10,#0x21` unsigned).
        let glyph: u16 = if c < 0x20 {
            0xffff
        } else if raw_glyph == 0
            || (c as u32).wrapping_sub(0x7f) < 0x21
            || c == 0x200b
        {
            0xffff
        } else {
            raw_glyph
        };
        // raw 0x8309c: CTFontGetAdvancesForGlyphs (glyph 0xffff → CoreText 가 0.0).
        let advance = provider.advance_for_glyph(font, size_px, glyph);
        // raw 0x830b4 `fadd d10,d10,d9`: dVar15 += dVar13.
        acc += advance;
    }
    // raw 0x830c8 `fcvt s0,d10`: fVar12 = (f32)dVar15.
    acc as f32
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latin_ascii_picks_helvetica() {
        // 'A' (0x41), '0' (0x30), space (0x20) — Latin/default.
        assert_eq!(select_system_font(0x41, false), SystemFont::Helvetica);
        assert_eq!(select_system_font(0x41, true), SystemFont::HelveticaBold);
        assert_eq!(select_system_font(0x30, false), SystemFont::Helvetica);
        assert_eq!(select_system_font(0x20, false), SystemFont::Helvetica);
    }

    #[test]
    fn hangul_syllables_pick_apple_sd_gothic_neo() {
        // 가 (0xAC00), 힣 (0xD7A3) — Hangul Syllables block.
        //   raw: ((c+0x5400)>>2)&0x3fff < 0xae9.
        assert_eq!(
            select_system_font(0xAC00, false),
            SystemFont::AppleSdGothicNeoMedium
        );
        assert_eq!(
            select_system_font(0xAC00, true),
            SystemFont::AppleSdGothicNeoBold
        );
        assert_eq!(
            select_system_font(0xD7A3, false),
            SystemFont::AppleSdGothicNeoMedium
        );
        // 한 (0xD55C).
        assert_eq!(
            select_system_font(0xD55C, false),
            SystemFont::AppleSdGothicNeoMedium
        );
    }

    #[test]
    fn hangul_syllable_boundary() {
        // 0xABFF (just below) → NOT Hangul Syllables.
        //   ((0xABFF+0x5400)>>2)&0x3fff = (0xFFFF>>2)&0x3fff = 0x3fff >= 0xae9.
        //   0xABFF: &0xe000 = 0xA000 != 0 → STHeitiTC (CJK 분기에 걸림).
        assert_eq!(select_system_font(0xABFF, false), SystemFont::StHeitiTcMedium);
        // 0xD7A4 (just above 0xD7A3): ((0xD7A4+0x5400)>>2)&0x3fff = 0xAE9, not < 0xae9.
        //   0xD7A4 & 0xe000 = 0xC000 != 0 → STHeitiTC.
        assert_eq!(select_system_font(0xD7A4, false), SystemFont::StHeitiTcMedium);
    }

    #[test]
    fn hangul_compat_jamo_lane() {
        // 0x3130-0x318F (Hangul Compatibility Jamo) — lane i=3:
        //   (u16)(c + 0xced0) < 0x60. c=0x3130 → 0x3130+0xced0 = 0x10000 → 0x0000 < 0x60 ✓.
        assert_eq!(
            select_system_font(0x3130, false),
            SystemFont::AppleSdGothicNeoMedium
        );
        assert_eq!(
            select_system_font(0x318F, false),
            SystemFont::AppleSdGothicNeoMedium
        );
        // 0x3190 — lane 3 한계 밖. c+0xced0 = 0x60, not < 0x60.
        //   0x3190 & 0xe000 = 0x2000 != 0 → STHeitiTC.
        assert_eq!(select_system_font(0x3190, false), SystemFont::StHeitiTcMedium);
    }

    #[test]
    fn hangul_jamo_ext_a_lane() {
        // 0xA960-0xA97C (Hangul Jamo Extended-A) — lane i=0:
        //   (u16)(c + 0x56a0) < 0x1d. c=0xA960 → 0xA960+0x56a0 = 0x10000 → 0 < 0x1d ✓.
        assert_eq!(
            select_system_font(0xA960, false),
            SystemFont::AppleSdGothicNeoMedium
        );
        assert_eq!(
            select_system_font(0xA97C, false),
            SystemFont::AppleSdGothicNeoMedium
        );
    }

    #[test]
    fn hangul_jamo_1100_block() {
        // 0x1100-0x11FF (Hangul Jamo) — (c & 0xff00) == 0x1100 → hangul_2.
        assert_eq!(
            select_system_font(0x1100, false),
            SystemFont::AppleSdGothicNeoMedium
        );
        assert_eq!(
            select_system_font(0x11FF, false),
            SystemFont::AppleSdGothicNeoMedium
        );
    }

    #[test]
    fn cjk_picks_st_heiti() {
        // CJK Unified Ideographs (0x4E00-0x9FFF): & 0xe000 != 0 → STHeitiTC.
        assert_eq!(select_system_font(0x4E00, false), SystemFont::StHeitiTcMedium);
        assert_eq!(select_system_font(0x6F22, false), SystemFont::StHeitiTcMedium); // 漢
        // bold 무관 — STHeitiTC 는 변형 없음.
        assert_eq!(select_system_font(0x6F22, true), SystemFont::StHeitiTcMedium);
        // CJK Radicals Supplement / Kangxi (0x2E80-0x2EFF): (c & 0xff80) == 0x2e80.
        assert_eq!(select_system_font(0x2E80, false), SystemFont::StHeitiTcMedium);
        // CJK Symbols (0x3000-0x303F): (u16)(c+0xcfc0) < 0xc0. 0x3000+0xcfc0 = 0xFFC0...
        //   0xFFC0 not < 0xc0. 0x3000 & 0xe000 = 0x2000 != 0 → STHeitiTC anyway.
        assert_eq!(select_system_font(0x3000, false), SystemFont::StHeitiTcMedium);
    }

    #[test]
    fn cfc0_range_picks_st_heiti() {
        // (u16)(c + 0xcfc0) < 0xc0 → c ∈ [0x3040, 0x30FF] (Hiragana+Katakana).
        //   c=0x3040 → 0x3040+0xcfc0 = 0x10000 → 0 < 0xc0 ✓.
        //   하지만 0x3040 & 0xe000 = 0x2000 != 0 라 이미 STHeitiTC. 둘 다 STHeitiTC.
        assert_eq!(select_system_font(0x3041, false), SystemFont::StHeitiTcMedium); // ぁ
        assert_eq!(select_system_font(0x30A2, false), SystemFont::StHeitiTcMedium); // ア
    }

    #[test]
    fn mul_div_basic() {
        // raw libhsp _MulDiv. pt→px 변환: MulDiv(size, 96, 72) = size * 4 / 3.
        assert_eq!(mul_div(72, 0x60, 0x48), 96); // 72 * 96 / 72 = 96
        assert_eq!(mul_div(9, 0x60, 0x48), 12); // 9 * 96 / 72 = 12
        // ties-away-from-zero: 10*96/72 = 13.33 → 13; 11*96/72 = 14.67 → 15.
        assert_eq!(mul_div(10, 0x60, 0x48), 13);
        assert_eq!(mul_div(11, 0x60, 0x48), 15);
        // exact half: 3*1/2 = 1.5 → 2 (away from zero).
        assert_eq!(mul_div(3, 1, 2), 2);
        assert_eq!(mul_div(1, 1, 2), 1); // 0.5 → 1
    }

    #[test]
    fn mul_div_zero_denominator_returns_minus_one() {
        // raw 0xb42a4: cbz w2 → -1.
        assert_eq!(mul_div(100, 50, 0), -1);
    }

    #[test]
    fn mul_div_negative() {
        // product < 0 경로: -q, ties away from zero.
        assert_eq!(mul_div(-3, 1, 2), -2); // -1.5 → -2
        assert_eq!(mul_div(-10, 0x60, 0x48), -13); // -13.33 → -13
        assert_eq!(mul_div(-11, 0x60, 0x48), -15); // -14.67 → -15
        // 음수 denominator: MulDiv(a,b,-c) = MulDiv(-a,b,c).
        assert_eq!(mul_div(72, 0x60, -0x48), -96);
        assert_eq!(mul_div(-72, 0x60, -0x48), 96);
    }

    #[test]
    fn mul_div_overflow_returns_minus_one() {
        // 결과가 i32 양수 범위 초과 → -1 (raw `lsr x8,x0,#31; cbz`).
        assert_eq!(mul_div(i32::MAX, i32::MAX, 1), -1);
        // 음수 경로 overflow.
        assert_eq!(mul_div(i32::MAX, i32::MAX, -1), -1);
    }

    #[test]
    fn core_text_names() {
        assert_eq!(SystemFont::Helvetica.core_text_name(), "Helvetica");
        assert_eq!(SystemFont::HelveticaBold.core_text_name(), "Helvetica-Bold");
        assert_eq!(
            SystemFont::StHeitiTcMedium.core_text_name(),
            "STHeitiTC-Medium"
        );
        assert_eq!(
            SystemFont::AppleSdGothicNeoMedium.core_text_name(),
            "AppleSDGothicNeo-Medium"
        );
        assert_eq!(
            SystemFont::AppleSdGothicNeoBold.core_text_name(),
            "AppleSDGothicNeo-Bold"
        );
    }

    #[test]
    fn combine_char_metrics_basic() {
        // font_size=10, em=1000, ascent=750, m7=250, m8=200, advance=600.
        // mul = MulDiv(10, 0x60, 0x48) = (960 + 36) / 72 = 13 (13.83 → trunc 13).
        let g = GlobalFontMetrics {
            em: 1000.0,
            ascent: 750.0,
            m7: 250.0,
            m8: 200.0,
        };
        let m = combine_char_metrics(10.0, &g, 600.0);
        assert_eq!(mul_div(10, 0x60, 0x48), 13);
        assert_eq!(m.em, 1000.0); // (f32)(u32)1000
        assert_eq!(m.width, 600.0 * 72.0 / 96.0); // 450.0
        assert_eq!(m.ascent, 750.0 * 13.0 / 1000.0); // 9.75
        assert_eq!(m.m7, 250.0 * 13.0 / 1000.0); // 3.25
        assert_eq!(m.m8, 200.0 * 13.0 / 1000.0); // 2.6
    }

    #[test]
    fn combine_char_metrics_round_trips_through_u32() {
        // raw fcvtzu + ucvtf: em/ascent/m7/m8 모두 (f32)(u32)x 로 floor (toward zero).
        // em=1000.9 → (u32)1000 → 1000.0. ascent=750.7 → 750.0.
        let g = GlobalFontMetrics {
            em: 1000.9,
            ascent: 750.7,
            m7: 250.3,
            m8: 0.9,
        };
        let m = combine_char_metrics(12.0, &g, 480.0);
        // mul = MulDiv(12, 0x60, 0x48) = (1152 + 36)/72 = 16 (16.5 → 16).
        assert_eq!(mul_div(12, 0x60, 0x48), 16);
        assert_eq!(m.em, 1000.0, "em round-tripped through u32");
        assert_eq!(m.ascent, 750.0 * 16.0 / 1000.0);
        assert_eq!(m.m7, 250.0 * 16.0 / 1000.0);
        assert_eq!(m.m8, 0.0 * 16.0 / 1000.0, "m8=0.9 → (u32)0");
        assert_eq!(m.width, 480.0 * 72.0 / 96.0); // 360.0 (advance 는 round-trip 없음)
    }

    #[test]
    fn combine_char_metrics_negative_metric_saturates_to_zero() {
        // fcvtzu on negative f32 saturates to 0 (Rust `as u32` 동일).
        let g = GlobalFontMetrics {
            em: 2048.0,
            ascent: -5.0,
            m7: 100.0,
            m8: -1.0,
        };
        let m = combine_char_metrics(8.0, &g, 1024.0);
        assert_eq!(m.ascent, 0.0, "negative ascent → (u32)0");
        assert_eq!(m.m8, 0.0, "negative m8 → (u32)0");
        assert_eq!(m.em, 2048.0);
    }

    // ===== measure_string_advance (FUN_00082d98) =====

    use std::cell::RefCell;

    /// raw glyph 을 `raw_glyph(c)` 로, advance 를 `glyph as f64` (단 `0xffff` → `0.0`) 로
    /// 돌려주는 mock. glyph/advance 호출의 (font, arg) 를 모두 기록한다.
    struct MockProvider {
        raw_glyph: fn(u16) -> u16,
        glyph_calls: RefCell<Vec<(SystemFont, f64, u16)>>,
        advance_calls: RefCell<Vec<(SystemFont, u16)>>,
    }

    impl MockProvider {
        fn new(raw_glyph: fn(u16) -> u16) -> Self {
            MockProvider {
                raw_glyph,
                glyph_calls: RefCell::new(Vec::new()),
                advance_calls: RefCell::new(Vec::new()),
            }
        }
        /// glyph = char 자체 (substitution 테스트용 — 대부분의 char 가 그대로 통과).
        fn identity() -> Self {
            Self::new(|c| c)
        }
    }

    impl CoreTextFontProvider for MockProvider {
        fn glyph_for_character(&self, font: SystemFont, size_px: f64, c: u16) -> u16 {
            self.glyph_calls.borrow_mut().push((font, size_px, c));
            (self.raw_glyph)(c)
        }
        fn advance_for_glyph(&self, font: SystemFont, _size_px: f64, glyph: u16) -> f64 {
            self.advance_calls.borrow_mut().push((font, glyph));
            // raw: 치환된 invalid glyph(0xffff) 은 CoreText 가 0.0 을 반환.
            if glyph == 0xffff {
                0.0
            } else {
                glyph as f64
            }
        }
    }

    #[test]
    fn measure_empty_string_is_zero() {
        // raw 0x82ec8 `cmp w21,#0x1; b.lt`: len < 1 → fVar12 = 0.0.
        let p = MockProvider::identity();
        assert_eq!(measure_string_advance(10.0, 0, &[], &p), 0.0);
        assert!(p.glyph_calls.borrow().is_empty());
        assert!(p.advance_calls.borrow().is_empty());
    }

    #[test]
    fn measure_accumulates_per_char_advance() {
        // 'A'(0x41) 'B'(0x42) 'C'(0x43) — 전부 치환 안 됨 → glyph = char → advance = char.
        let p = MockProvider::identity();
        let adv = measure_string_advance(10.0, 0, &[0x41, 0x42, 0x43], &p);
        assert_eq!(adv, (0x41 + 0x42 + 0x43) as f32); // 198.0
        // 호출 횟수 = char 수.
        assert_eq!(p.glyph_calls.borrow().len(), 3);
        assert_eq!(p.advance_calls.borrow().len(), 3);
    }

    #[test]
    fn measure_size_px_passed_to_provider() {
        // raw 0x82e94-ec4: size_px = MulDiv((i32)size, 0x60, 0x48) * 72.0 / 96.0.
        let p = MockProvider::identity();
        measure_string_advance(12.0, 0, &[0x41], &p);
        let expected = mul_div(12, 0x60, 0x48) as f64 * 72.0 / 96.0;
        assert_eq!(p.glyph_calls.borrow()[0].1, expected);
    }

    #[test]
    fn measure_control_char_below_0x20_substituted() {
        // raw 0x83054 `cmp w8,#0x20; b.cs`: c < 0x20 → glyph = -1 → advance 0.
        let p = MockProvider::identity();
        // 0x09(TAB) 0x1f — 둘 다 < 0x20.
        assert_eq!(measure_string_advance(10.0, 0, &[0x09, 0x1f], &p), 0.0);
        // glyph_for_character 는 그래도 호출됨 (raw: CTFontGetGlyphsForCharacters 무조건 호출).
        assert_eq!(p.glyph_calls.borrow().len(), 2);
        // advance_for_glyph 은 치환된 0xffff 로 호출.
        assert_eq!(
            p.advance_calls.borrow().iter().map(|c| c.1).collect::<Vec<_>>(),
            vec![0xffff, 0xffff]
        );
    }

    #[test]
    fn measure_glyph_zero_substituted() {
        // raw 0x8306c `cmp w9,#0x0`: raw glyph == 0 (.notdef) → -1 → advance 0.
        let p = MockProvider::new(|_| 0);
        assert_eq!(measure_string_advance(10.0, 0, &[0x41, 0x42], &p), 0.0);
        assert_eq!(
            p.advance_calls.borrow().iter().map(|c| c.1).collect::<Vec<_>>(),
            vec![0xffff, 0xffff]
        );
    }

    #[test]
    fn measure_c1_control_and_del_range_substituted() {
        // raw 0x83068 `sub w10,w8,#0x7f; ... cmp w10,#0x21` unsigned:
        // (u32)(c-0x7f) < 0x21 ⟺ c ∈ [0x7f, 0x9f] → glyph = -1.
        let p = MockProvider::identity();
        // 0x7f(DEL), 0x80, 0x9f — 전부 치환.
        assert_eq!(measure_string_advance(10.0, 0, &[0x7f, 0x80, 0x9f], &p), 0.0);
        // 0xa0 = (0xa0-0x7f)=0x21, not < 0x21 → 치환 안 됨.
        let p2 = MockProvider::identity();
        assert_eq!(measure_string_advance(10.0, 0, &[0xa0], &p2), 0xa0 as f32);
        // 0x7e = (0x7e-0x7f) wrap → 큰 unsigned, not < 0x21 → 치환 안 됨.
        let p3 = MockProvider::identity();
        assert_eq!(measure_string_advance(10.0, 0, &[0x7e], &p3), 0x7e as f32);
    }

    #[test]
    fn measure_zwsp_substituted() {
        // raw 0x83078 `mov w10,#0x200b; ccmp w8,w10`: c == 0x200b (ZWSP) → glyph = -1.
        let p = MockProvider::identity();
        // 0x200b 는 >= 0x20 이고 [0x7f,0x9f] 도 아니지만 명시적으로 치환됨.
        assert_eq!(measure_string_advance(10.0, 0, &[0x200b], &p), 0.0);
        assert_eq!(p.advance_calls.borrow()[0].1, 0xffff);
    }

    #[test]
    fn measure_font_selection_per_char_with_bold() {
        // raw 0x82f04-70 분류 + style bit0 = bold. 'A'→Helvetica, '가'(0xAC00)→AppleSDGothicNeo.
        let p = MockProvider::identity();
        measure_string_advance(10.0, 1 /* bold */, &[0x41, 0xAC00], &p);
        let fonts: Vec<SystemFont> =
            p.glyph_calls.borrow().iter().map(|c| c.0).collect();
        assert_eq!(
            fonts,
            vec![SystemFont::HelveticaBold, SystemFont::AppleSdGothicNeoBold]
        );
        // advance_for_glyph 도 동일 폰트로 호출.
        let adv_fonts: Vec<SystemFont> =
            p.advance_calls.borrow().iter().map(|c| c.0).collect();
        assert_eq!(
            adv_fonts,
            vec![SystemFont::HelveticaBold, SystemFont::AppleSdGothicNeoBold]
        );
    }

    #[test]
    fn measure_italic_bit_does_not_affect_advance() {
        // style bit1 = italic — text matrix skew 일 뿐 advance 와 무관.
        // bit0(bold) 만 같으면 italic 유무로 결과/폰트가 바뀌면 안 됨.
        let p_plain = MockProvider::identity();
        let p_italic = MockProvider::identity();
        let plain = measure_string_advance(10.0, 0b00, &[0x41, 0xAC00], &p_plain);
        let italic = measure_string_advance(10.0, 0b10, &[0x41, 0xAC00], &p_italic);
        assert_eq!(plain, italic);
        assert_eq!(
            p_plain.glyph_calls.borrow().iter().map(|c| c.0).collect::<Vec<_>>(),
            p_italic.glyph_calls.borrow().iter().map(|c| c.0).collect::<Vec<_>>()
        );
    }

    #[test]
    fn measure_mixed_substituted_and_real() {
        // 'A'(real) TAB(<0x20, sub) '0'(0x30, real) ZWSP(sub) 'z'(0x7a, real).
        let p = MockProvider::identity();
        let adv = measure_string_advance(10.0, 0, &[0x41, 0x09, 0x30, 0x200b, 0x7a], &p);
        // 치환된 것은 0, 나머지는 glyph(=char) 만큼.
        assert_eq!(adv, (0x41 + 0x30 + 0x7a) as f32);
    }

    /// 0x56 바이트 OS/2 테이블을 만들고 sTypo* 필드만 big-endian 으로 채운다.
    fn make_os2_table(asc: i16, desc: i16, gap: i16) -> Vec<u8> {
        let mut t = vec![0u8; 0x56];
        t[0x44..0x46].copy_from_slice(&asc.to_be_bytes());
        t[0x46..0x48].copy_from_slice(&desc.to_be_bytes());
        t[0x48..0x4a].copy_from_slice(&gap.to_be_bytes());
        t
    }

    #[test]
    fn parse_os2_metrics_extracts_stypo_fields() {
        // 표준 폰트 비슷한 값 — sTypoAscender 양수, sTypoDescender 음수.
        let table = make_os2_table(1854, -434, 67);
        let m = parse_os2_metrics(&table).expect("0x56 바이트면 파싱 성공");
        assert_eq!(m.s_typo_ascender, 1854);
        assert_eq!(m.s_typo_descender, -434);
        assert_eq!(m.s_typo_line_gap, 67);
    }

    #[test]
    fn parse_os2_metrics_size_guard() {
        // raw `FUN_00082eec` 의 `param_3 < 0x56` 가드 — 0x55 바이트는 거부.
        let mut short = make_os2_table(1000, -200, 0);
        short.truncate(0x55);
        assert_eq!(parse_os2_metrics(&short), None);
        // 정확히 0x56 은 통과.
        assert!(parse_os2_metrics(&make_os2_table(1000, -200, 0)).is_some());
        // 더 큰 테이블 (v1+ 86+ 바이트) 도 통과, 같은 오프셋.
        let mut big = make_os2_table(900, -100, 10);
        big.resize(0x60, 0);
        assert_eq!(
            parse_os2_metrics(&big).unwrap(),
            Os2Metrics { s_typo_ascender: 900, s_typo_descender: -100, s_typo_line_gap: 10 }
        );
    }
}

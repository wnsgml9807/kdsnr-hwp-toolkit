//! `Hnc::Shape::ShapeRenderConverter` — Hancom shape → Render type conversion bridge.
//!
//! raw 출처: `libHncDrawingEngine.dylib`.
//!
//! ShapeRenderConverter 는 단일 namespace 의 free functions 집합 (no instance state):
//! `nm | c++filt | grep ShapeRenderConverter::` 결과 ~25 method.
//!
//! ## 본 module 의 완성 범위 (L-5c-9b1 시점)
//!
//! - `ApplyColorMode(Flag&, ColorMode)` (raw 0x1df7e4, 40B): Flag bit 0x40/0x80 set
//! - `ToRenderColor(Color, ColorMapper*) → DrawingType::Color` (raw 0x1dfb9c, ~330B):
//!   7-way dispatcher on Color::type_tag with byte-eq output layout.
//! - 보조 type: `RenderColor` (DrawingType::Color 6B layout) + `ColorMapperLike` trait
//!
//! ## 보류 (L-5c-9b2)
//!
//! - `ToSolidBrush(SolidBrush, ColorMapper*, RenderMode, bool) → Render::Brush` (raw 0x1e04f4)
//! - `ToHatchBrush` / `ToImageBrush` / `ToOuterShadow` / `ToReflection` 등
//! - `LogicalToRender(Paths, Size)` / `RenderToDevice` (좌표 변환)
//! - `ToPath` (Subpath → Render::Path conversion)

use crate::color::{color_type, Color, PresetStyle};
use crate::degree::Degree;
use crate::flag::Flag;
use crate::math_util::round_to_i64;
use crate::pixel_util::hsl_to_rgb;

/// `Hnc::Shape::ColorMode` enum (placeholder).
/// raw 의 정확한 enum variant 값은 0x1df7e4 의 cmp 결과로 추론:
/// - 1 = Grayscale (set bit 0x40)
/// - 2 = BlackWhite (set bit 0x80)
/// - 다른 값 = no-op
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorMode {
    None = 0,
    Grayscale = 1,
    BlackWhite = 2,
}

impl ColorMode {
    pub fn from_u32(v: u32) -> Self {
        match v {
            1 => Self::Grayscale,
            2 => Self::BlackWhite,
            _ => Self::None,
        }
    }

    /// raw bit mask: ColorMode → set flag bit.
    /// Grayscale=1 → 0x40, BlackWhite=2 → 0x80, else → 0 (no change).
    pub fn raw_flag_bit(self) -> u64 {
        match self {
            Self::Grayscale => 0x40,
            Self::BlackWhite => 0x80,
            Self::None => 0,
        }
    }
}

/// `ShapeRenderConverter::ApplyColorMode(Flag&, ColorMode)` — raw `0x1df7e4` (40B).
///
/// raw 흐름:
/// ```asm
/// 0x1df7e4: cmp w1, #1; b.eq 0x1df7fc   ; if ColorMode == 1 (Grayscale)
/// 0x1df7ec: cmp w1, #2; b.ne 0x1df80c   ; if != 2 (BlackWhite), return no-op
/// 0x1df7f4: mov w8, #0x80; b 0x1df800   ; else w8 = 0x80
/// 0x1df7fc: mov w8, #0x40                ; w8 = 0x40 (Grayscale case)
/// 0x1df800: ldr x9, [x0]; orr x8, x9, x8; str x8, [x0]; ret
/// ```
///
/// 의미: Flag 의 하위 8B 에 ColorMode bit (Grayscale=0x40 또는 BlackWhite=0x80) OR.
/// 다른 ColorMode 값은 no-op.
pub fn apply_color_mode(flag: &mut Flag, mode: ColorMode) {
    let bit = mode.raw_flag_bit();
    if bit != 0 {
        flag.0 |= bit;
    }
}

/// `ShapeRenderConverter::ToColorMode(Flag const&) -> ColorMode` — raw `0x180918` (44B).
///
/// `apply_color_mode` 의 reverse: Flag bit pattern 을 검사하여 ColorMode enum 추출.
///
/// raw 흐름:
/// ```asm
/// 180918: ldr x8, [x0]              ; x8 = flag.value (u64)
/// 18091c: mov w9, #1
/// 180920: mov w10, #2
/// 180924: tst x8, #0x8000000        ; bit 27 (Mode3 flag)
/// 180928: mov w11, #3
/// 18092c: csel w11, wzr, w11, eq    ; w11 = (bit27 set) ? 3 : 0
/// 180930: tst w8, #0x80             ; bit 7 (BlackWhite)
/// 180934: csel w10, w10, w11, ne    ; w10 = (bit7 set) ? 2 : w11
/// 180938: tst w8, #0x40             ; bit 6 (Grayscale)
/// 18093c: csel w0, w9, w10, ne      ; w0 = (bit6 set) ? 1 : w10
/// 180940: ret
/// ```
///
/// Priority order (raw 의 csel chain 으로 도출):
/// 1. bit 0x40 set → `ColorMode::Grayscale` (1)
/// 2. bit 0x80 set → `ColorMode::BlackWhite` (2)
/// 3. bit 0x08000000 set → `ColorMode::Mode3` (3)  (별도 enum variant 필요)
/// 4. 그 외 → `ColorMode::None` (0)
///
/// **Note**: `ColorMode` enum 이 현재 3 variant 만 정의 (raw asm 의 mode 3 은
/// 별도 의미 — Hancom 의 light grayscale 또는 inverted 추정). `to_color_mode` 의
/// return 으로 u32 직접 노출하여 enum extension 강제 없이 raw byte-eq 유지.
pub fn to_color_mode(flag: &Flag) -> u32 {
    let v = flag.0;
    // raw 의 csel chain 을 priority decode 로 풀어 씀.
    if v & 0x40 != 0 {
        return 1; // Grayscale
    }
    if v & 0x80 != 0 {
        return 2; // BlackWhite
    }
    if v & 0x0800_0000 != 0 {
        return 3; // Mode3 (raw bit 27)
    }
    0 // None
}

// =============================================================================
// SRC 좌표 변환 (logical / render / device) — 6 scalar 변환 family
// =============================================================================
//
// 좌표계 정의:
// - logical  : 한컴 raw 좌표 (1 unit = 1 mm × engine.unit 의 의미? `0xdb2f8` 의
//              계산 `p * 96 / engine.unit` 보면 unit 이 mm/inch 환산 계수)
// - render   : 96 DPI 의 floating point 좌표 (브라우저/SVG 의 자연 단위와 일치)
// - device   : 최종 pixel 좌표 (Round 로 정수화, scale 인자 적용)
//
// 변환 공식 (정공법 raw 인용):
// - logical → render : `render = logical * 96.0 / engine.unit`
// - render → logical : `logical = render * engine.unit / 96.0`
// - render → device  : `device = render * (scale != 0 ? scale : 1.0)`
// - logical → device : `device = Round(logical_to_render(logical) * (double)scale)`
// - device → logical : `logical = (device / scale) * engine.unit / 96.0`
//   여기서 scale 0 보호: `s' = (scale == 0) ? 1.0 : scale`
//
// 96.0 = `0x42c00000` (raw `mov w8, #0x42c00000; fmov s1, w8` 패턴).
// engine.unit = `ShapeEngine::GetInstance()[+0x4]` = `unit` field.

/// `ShapeRenderConverter::LogicalToRender(float)` — raw `0xdb2f8` (48B).
///
/// ```asm
/// db2f8: stp d9, d8, [sp, ...]
/// db304: mov w8, #0x42c00000; fmov s1, w8    ; s1 = 96.0
/// db30c: fmul s8, s0, s1                      ; s8 = p * 96
/// db310: bl ShapeEngine::GetInstance
/// db314: ldr s0, [x0, #0x4]                   ; s0 = engine.unit
/// db318: fdiv s0, s8, s0                      ; s0 = (p * 96) / unit
/// ```
pub fn logical_to_render_scalar(p: f32) -> f32 {
    let unit = crate::shape_engine::read_instance().unit;
    (p * 96.0) / unit
}

/// `ShapeRenderConverter::RenderToLogical(float)` — raw `0x14029c` (52B).
///
/// ```asm
/// 14029c: stp d9, d8, ...
/// 1402a8: fmov s8, s0                         ; s8 = p
/// 1402ac: bl ShapeEngine::GetInstance
/// 1402b0: ldr s0, [x0, #0x4]                  ; s0 = engine.unit
/// 1402b4: fmul s0, s0, s8                      ; s0 = unit * p
/// 1402b8: mov w8, #0x42c00000; fmov s1, w8    ; s1 = 96.0
/// 1402c0: fdiv s0, s0, s1                      ; s0 = (unit * p) / 96
/// ```
pub fn render_to_logical_scalar(p: f32) -> f32 {
    let unit = crate::shape_engine::read_instance().unit;
    (unit * p) / 96.0
}

/// `ShapeRenderConverter::RenderToDevice(float p, float scale)` — raw `0x1dfac8` (20B).
///
/// ```asm
/// 1dfac8: fcmp s1, #0.0
/// 1dfacc: fmov s2, #1.0
/// 1dfad0: fcsel s1, s2, s1, eq                ; s1 = (scale == 0) ? 1.0 : scale
/// 1dfad4: fmul s0, s1, s0                      ; s0 = scale_safe * p
/// 1dfad8: ret
/// ```
///
/// **leaf 함수** (ShapeEngine 호출 없음). `s1 == 0` 일 때 1.0 으로 대체 — div-by-zero
/// 같은 보호적 동작이지만 raw 의 의도는 "scale 미지정 시 identity".
pub fn render_to_device_scalar(p: f32, scale: f32) -> f32 {
    let scale_safe = if scale == 0.0 { 1.0 } else { scale };
    scale_safe * p
}

/// `ShapeRenderConverter::LogicalToDevice(float p, float scale)` — raw `0x106cc8` (64B).
///
/// ```asm
/// 106cc8: stp d9, d8, ...
/// 106cd4: fmov s8, s1                          ; s8 = scale (caller-saved across bl)
/// 106cd8-dc: s1 = 96.0
/// 106ce0: fmul s9, s0, s1                      ; s9 = p * 96
/// 106ce4: bl ShapeEngine::GetInstance
/// 106ce8: ldr s0, [x0, #0x4]                   ; s0 = engine.unit
/// 106cec: fdiv s0, s9, s0                      ; s0 = (p * 96) / unit  (= logical_to_render)
/// 106cf0: fcvt d0, s0                          ; d0 = double(logical_to_render(p))
/// 106cf4: fcvt d1, s8                          ; d1 = double(scale)
/// 106cf8: fmul d0, d1, d0                      ; d0 = scale * logical_to_render(p)
/// 106cfc: epilog
/// 106d04: b MathUtil::Round (tail call)         ; returns i64
/// ```
///
/// **scale=0 보호 없음** (raw 가 직접 곱셈) — caller 책임.
pub fn logical_to_device_scalar(p: f32, scale: f32) -> i64 {
    let render = logical_to_render_scalar(p);
    let device_d = scale as f64 * render as f64;
    crate::math_util::round_to_i64(device_d)
}

/// `ShapeRenderConverter::DeviceToLogical(int p, float scale)` — raw `0xe5a10` (68B).
///
/// ```asm
/// e5a10: stp d9, d8, ...
/// e5a1c: fcmp s0, #0.0
/// e5a20: fmov s1, #1.0
/// e5a24: fcsel s0, s1, s0, eq                 ; s0 = (scale == 0) ? 1.0 : scale
/// e5a28: scvtf s1, w0                          ; s1 = (float)p (signed int → float)
/// e5a2c: fdiv s8, s1, s0                       ; s8 = p / scale_safe
/// e5a30: bl ShapeEngine::GetInstance
/// e5a34: ldr s0, [x0, #0x4]                    ; s0 = engine.unit
/// e5a38: fmul s0, s8, s0                       ; s0 = (p / scale) * unit
/// e5a3c-44: s0 /= 96.0
/// ```
pub fn device_to_logical_scalar(p: i32, scale: f32) -> f32 {
    let scale_safe = if scale == 0.0 { 1.0 } else { scale };
    let render = (p as f32) / scale_safe;
    let unit = crate::shape_engine::read_instance().unit;
    (render * unit) / 96.0
}

// =============================================================================
// ApplyRenderMode — RenderMode 별 RenderColor 변형 dispatch
// =============================================================================

/// `Hnc::Type::DrawingType::ColorUtil` 3 함수 trait 추상화.
///
/// raw 의 ColorUtil::ToGray/ToLightGray/ToInverseGray (각 ~240B + 3KB lazy lookup
/// table) 는 byte-eq 가능하지만 NEON SIMD 로 init 되는 256×3 entry u32 lookup
/// table 의 정확한 byte-eq port 가 별도 세션 필요.
///
/// 본 trait 으로 위임 — 향후 ColorUtilProviderImpl 이 byte-eq 동작 구현.
pub trait ColorUtilProvider {
    /// raw `ColorUtil::ToGray(Color&)` (`0x27868`, ~492B). in-place 변환.
    fn to_gray(&self, color: &mut RenderColor);
    /// raw `ColorUtil::ToLightGray(Color&)` (`0x27a68`, ~512B).
    fn to_light_gray(&self, color: &mut RenderColor);
    /// raw `ColorUtil::ToInverseGray(Color&)` (`0x27c6c`, ~480B).
    fn to_inverse_gray(&self, color: &mut RenderColor);
}

/// `ShapeRenderConverter::ApplyRenderMode(Color& c, RenderMode mode)` —
/// raw `0x1dfdf0` (~248B).
///
/// In-place 변환: `c` 를 mode 에 따라 수정. RenderMode 0/9+ 은 no-op.
///
/// ```asm
/// 1dfdfc: sub w8, w1, #1
/// 1dfe00: cmp w8, #7
/// 1dfe04: b.hi 0x1dfeac (no-op return)
/// 1dfe14-20: jump table @ 0x743ab2[idx] → case
/// ```
///
/// 8 case (mode = 1..8):
/// 1. ColorUtil::ToGray         (tail call)
/// 2. ColorUtil::ToLightGray    (tail call)
/// 3. ColorUtil::ToInverseGray  (tail call)
/// 4. write light gray solid    `r=g=b=0xA0, type=1, alpha unchanged`
/// 5. write white               `r=g=b=0xFF, type=1, alpha unchanged`
/// 6. write black               `r=g=b=0x00, type=1, alpha unchanged`
/// 7. GetSysColor(5) → write    (system color 5)
/// 8. GetSysColor(8) → write    (system color 8)
///
/// 다른 mode 값 → no-op (early return).
pub fn apply_render_mode<C: ColorUtilProvider, S: SystemColorProvider>(
    color: &mut RenderColor,
    mode: u32,
    color_util: &C,
    sys_provider: &S,
) {
    // raw 1dfdfc-04: idx = mode - 1; if idx > 7 → return
    let idx = mode.wrapping_sub(1);
    if idx > 7 {
        return;
    }
    match idx {
        // case 1: mode=1 → ToGray
        0 => color_util.to_gray(color),
        // case 2: mode=2 → ToLightGray
        1 => color_util.to_light_gray(color),
        // case 3: mode=3 → ToInverseGray
        2 => color_util.to_inverse_gray(color),
        // case 4: mode=4 → light gray solid (0xA0, 0xA0, 0xA0)
        // raw 1dfe54-68: strb 1 [+4]; strh 0xa0a0 [+0]; strb 0xa0 [+2]
        3 => {
            color.b0 = 0xA0;
            color.b1 = 0xA0;
            color.b2 = 0xA0;
            color.color_type = 1;
            // raw: alpha 변경 없음
        }
        // case 5: mode=5 → white
        // raw 1dfe78-8c: strb 1; strh 0xffff; strb 0xff
        4 => {
            color.b0 = 0xFF;
            color.b1 = 0xFF;
            color.b2 = 0xFF;
            color.color_type = 1;
        }
        // case 6: mode=6 → black
        // raw 1dfe9c-a8: strb 1; strh 0; strb 0
        5 => {
            color.b0 = 0;
            color.b1 = 0;
            color.b2 = 0;
            color.color_type = 1;
        }
        // case 7: mode=7 → GetSysColor(5)
        // raw 1dfeb8-bc: mov w0, #5; b 0x1dfec4
        // 0x1dfec4-d8: bl GetSysColor; strb 1 [+4]; strh w0 [+0]; strb w0>>16 [+2]
        6 => {
            let colorref = sys_provider.get_sys_color(5);
            color.b0 = (colorref & 0xff) as u8;
            color.b1 = ((colorref >> 8) & 0xff) as u8;
            color.b2 = ((colorref >> 16) & 0xff) as u8;
            color.color_type = 1;
        }
        // case 8: mode=8 → GetSysColor(8)
        7 => {
            let colorref = sys_provider.get_sys_color(8);
            color.b0 = (colorref & 0xff) as u8;
            color.b1 = ((colorref >> 8) & 0xff) as u8;
            color.b2 = ((colorref >> 16) & 0xff) as u8;
            color.color_type = 1;
        }
        _ => unreachable!("guarded by idx > 7 check"),
    }
}

// =============================================================================
// `Hnc::Type::DrawingType::Color` — 6B output of ToRenderColor.
// =============================================================================

/// `Hnc::Type::DrawingType::Color` — raw 6B (struct 1B align, often 8B padded).
///
/// raw layout (derived from `DrawingType::Color::Color(u8,u8,u8,u8,u8)` @ `0x26d4c`
/// + ToRenderColor 의 각 case strb 패턴):
///
/// | offset | field      | 의미 (per color_type)                          |
/// |--------|-----------|------------------------------------------------|
/// | 0      | b0         | RGB: r / CMYK: c / Preset/System/HSL: r       |
/// | 1      | b1         | RGB: g / CMYK: m / Preset/System/HSL: g       |
/// | 2      | b2         | RGB: b / CMYK: y / Preset/System/HSL: b       |
/// | 3      | b3         | CMYK: k (others uninit)                       |
/// | 4      | color_type | 0 = CMYK, 1 = RGB (다른 type 도 0/1 으로 normalize) |
/// | 5      | alpha      | 0xff usually (raw 의 `mov w8, #0xff??` 로 강제)   |
///
/// raw 의 `Color(u8 r, u8 g, u8 b)` (`0x26d1c`):
/// ```asm
/// mov w8, #0xff01      ; type=1 (RGB), alpha=0xff
/// strh w8, [x0, #0x4]  ; bytes [4..6]
/// strb w1, [x0]        ; r
/// strb w2, [x0, #1]    ; g
/// strb w3, [x0, #2]    ; b
/// ```
///
/// raw 의 `Color(u8 a, u8 r, u8 g, u8 b)` 4-arg (`0x26cd4`):
/// ```asm
/// mov w8, #0x1
/// strb w8, [x0, #4]    ; type=1 (RGB)
/// strb w1, [x0, #5]    ; alpha = first arg
/// strb w2, [x0]        ; r
/// strb w3, [x0, #1]    ; g
/// strb w4, [x0, #2]    ; b
/// ```
///
/// raw 의 `Color(u8 a, u8 c, u8 m, u8 y, u8 k)` 5-arg (`0x26d4c`):
/// ```asm
/// strb wzr, [x0, #4]   ; type=0 (CMYK)
/// strb w1, [x0, #5]    ; alpha
/// strb w2, [x0]        ; c
/// strb w3, [x0, #1]    ; m
/// strb w4, [x0, #2]    ; y
/// strb w5, [x0, #3]    ; k
/// ```
///
/// ToRenderColor invalid path (`0x1dfca4`):
/// ```asm
/// mov w8, #0xff01      ; type=1, alpha=0xff
/// strh w8, [x19, #4]
/// strh wzr, [x19]      ; r = g = 0
/// strb wzr, [x19, #2]  ; b = 0
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderColor {
    pub b0: u8,
    pub b1: u8,
    pub b2: u8,
    pub b3: u8,
    pub color_type: u8,
    pub alpha: u8,
}

pub const RENDER_COLOR_SIZE_BYTES: usize = 6;

const _: () = assert!(std::mem::size_of::<RenderColor>() == RENDER_COLOR_SIZE_BYTES);

impl RenderColor {
    /// raw invalid output (ToRenderColor 의 0x1dfca4 분기).
    /// `(r=0, g=0, b=0, _=0, color_type=1=RGB, alpha=0xff)`.
    pub const INVALID: RenderColor = RenderColor {
        b0: 0,
        b1: 0,
        b2: 0,
        b3: 0,
        color_type: 0x01,
        alpha: 0xff,
    };

    /// RGB with full opacity (matches `DrawingType::Color(r, g, b)` @ `0x26d1c`).
    pub fn rgb(r: u8, g: u8, b: u8) -> Self {
        // raw mov w8,#0xff01; strh w8,[x0,#4] → [4]=0x01 (RGB), [5]=0xff (alpha)
        Self {
            b0: r,
            b1: g,
            b2: b,
            b3: 0,
            color_type: 0x01,
            alpha: 0xff,
        }
    }

    /// CMYK with full opacity (matches `DrawingType::Color(a, c, m, y, k)` @ `0x26d4c`).
    pub fn cmyk(c: u8, m: u8, y: u8, k: u8) -> Self {
        Self {
            b0: c,
            b1: m,
            b2: y,
            b3: k,
            color_type: 0x00,
            alpha: 0xff,
        }
    }
}

// =============================================================================
// `Hnc::Shape::ColorMapper` — opaque + trait.
// =============================================================================

/// `Hnc::Shape::ColorMapper` (raw class with vtable, theme color resolution).
///
/// 본 단계 (L-5c-9b1) 에서는 ColorMapper 의 internal layout (두 개의 RB tree) 을
/// 1:1 port 하지 않음 — ToRenderColor 의 SCHEME case 에서 mapper 가 호출되는
/// 동작만 trait 으로 추상화. mapper 의 byte-eq 내부 port 는 별도 세션 (L-5c-?).
///
/// raw ToRenderColor SCHEME case (0x1dfc00-0x1dfca0) 의 mapper 사용:
/// 1. `mapper[+0]` (= x8) 가 null 이면 invalid.
/// 2. `mapper[+8]` 의 RB tree (node key at +0x1c, value at +0x20) 에서 scheme 검색
///    → mapped scheme value (x10).
/// 3. `mapper[+0][+0x10]` 의 두 번째 RB tree (key at +0x20) 에서 x10 검색.
///    upper_bound 의 key <= x10 이면 통과.
/// 4. 통과 시 `mapper->GetColor(original_scheme)` (raw `__ZNK3Hnc5Shape11ColorMapper8GetColorENS0_5Color11SchemeStyleE`)
///    호출, 그 결과를 sret 으로 반환.
/// 5. 어느 단계든 실패 시 INVALID 출력.
///
/// 본 trait 은 위 4단계 의 dispatch logic 만 노출:
/// - `try_resolve_scheme(scheme) → Option<RenderColor>`: 두 tree 검증 + GetColor 결합.
pub trait ColorMapperLike {
    /// raw ToRenderColor SCHEME path 의 결과:
    /// - Some(color) : 두 tree 검증 통과 + `GetColor(original_scheme)` 호출 결과
    /// - None        : 어느 단계 든 실패 → invalid color
    fn try_resolve_scheme(&self, scheme_style: u32) -> Option<RenderColor>;
}

// =============================================================================
// `Hnc::Shape::ShapeRenderConverter::ToRenderColor` — 7-way dispatcher.
// =============================================================================

/// `SystemColorProvider` — Windows API `GetSysColor` trait 추상화.
///
/// raw ToRenderColor SYSTEM case (0x1dfccc-d0) 가 `bl _GetSysColor` 를 호출.
/// macOS dylib 의 `_GetSysColor` symbol stub 은 Hancom 의 platform shim — 별도 port.
///
/// 본 port 는 caller 가 system color 의미 (예: COLOR_WINDOW = (240,240,240)) 를
/// 구현하도록 trait 으로 위임. 반환값 layout: `(B << 16) | (G << 8) | R`.
pub trait SystemColorProvider {
    /// raw `GetSysColor(int sys_color_idx) -> COLORREF (u32 in BBGGRR order)`.
    fn get_sys_color(&self, idx: u32) -> u32;
}

/// `Hnc::Shape::ShapeRenderConverter::ToRenderColor(Color const&, ColorMapper const*)`
/// — raw `0x1dfb9c` (~330B).
///
/// `Hnc::Shape::Color` (24B) → `Hnc::Type::DrawingType::Color` (6B) byte-eq conversion.
///
/// # raw dispatcher (0x1dfb9c-dc):
///
/// ```asm
/// 1dfb9c: prologue (sp -= 0x50, save x19/x20/x21/x22/d8/d9)
/// 1dfbb4: x19 = &result (sret)
/// 1dfbb8: w8  = color.type_tag (offset +0xc)
/// 1dfbbc: cmp w8, #6
/// 1dfbc0: b.hi 0x1dfca4   ; type > 6 → invalid
/// 1dfbc4: x20 = &color
/// 1dfbc8-dc: jump table at 0x743aab[type] → case label.
///   jump table values map type → case:
///     0 = RGB     → 0x1dfbe0
///     1 = CMYK    → 0x1dfbf0
///     2 = SCHEME  → 0x1dfc00
///     3 = SYSTEM  → 0x1dfccc
///     4 = PRESET  → 0x1dfcd8
///     5 = SC_RGB  → 0x1dfcf0
///     6 = HSL     → 0x1dfd44
/// ```
///
/// # 7 case 별 결과 layout (모두 6B RenderColor):
///
/// | type   | b0..b3 의미              | color_type | alpha |
/// |--------|-------------------------|-----------|-------|
/// | RGB    | r, g, b, 0              | 1         | 0xff  |
/// | CMYK   | c, m, y, k              | 0         | 0xff  |
/// | SCHEME | mapper->GetColor 결과 그대로 sret-overwrite | (그대로) | (그대로) |
/// | SYSTEM | r, g, b ← `GetSysColor(value)` 의 low 24bit | 1 | 0xff |
/// | PRESET | r, g, b ← `Color::GetPresetColor(value)` 의 low 24bit | 1 | 0xff |
/// | SC_RGB | round(r*255), round(g*255), round(b*255), 0 | 1 | 0xff |
/// | HSL    | r, g, b ← `PixelUtil::HslToRgb(h,s,l,1.0,1.0)` | 1 | 0xff |
/// | invalid (type>6, scheme fail) | 0, 0, 0, 0 | 1 | 0xff |
///
/// raw asm 인용 (RGB case @ 0x1dfbe0):
/// ```asm
/// ldrh w8, [x20]       ; w8 = color.value[0..2] = (g<<8)|r
/// strh w8, [x19]       ; result[0..2] = r,g
/// ldrb w8, [x20, #2]   ; w8 = color.value[2] = b
/// b 0x1dfce8           ; → strb w8, [x19, #2]; b 0x1dfd98
///                       ; → mov w8, #0xff01; strh w8, [x19, #4]; ret
/// ```
///
/// CMYK case (0x1dfbf0):
/// ```asm
/// ldr w8, [x20]        ; w8 = color.value[0..4] = c,m,y,k
/// str w8, [x19]        ; result[0..4] = c,m,y,k
/// mov w8, #0xff00      ; type=0 (CMYK), alpha=0xff
/// b 0x1dfd9c           ; → strh w8, [x19, #4]; ret
/// ```
pub fn to_render_color<M: ColorMapperLike, S: SystemColorProvider>(
    color: &Color,
    mapper: Option<&M>,
    sys_provider: &S,
) -> RenderColor {
    // raw 1dfbb8-c0: cmp w8, #6; b.hi 0x1dfca4 (invalid)
    if color.type_tag > 6 {
        return RenderColor::INVALID;
    }

    match color.type_tag {
        // raw case RGB (0x1dfbe0): 1:1 byte transfer + alpha marker.
        color_type::RGB => {
            // raw: w8 = ldrh [x20]; strh w8, [x19]; w8 = ldrb [x20, #2]; strb w8, [x19, #2]
            // 즉 color.value[0..3] → result[0..3]
            RenderColor::rgb(color.value[0], color.value[1], color.value[2])
        }

        // raw case CMYK (0x1dfbf0): 4-byte transfer, alpha_type = 0xff00.
        color_type::CMYK => {
            // raw: w8 = ldr [x20]; str w8, [x19]
            RenderColor::cmyk(
                color.value[0],
                color.value[1],
                color.value[2],
                color.value[3],
            )
        }

        // raw case SCHEME (0x1dfc00-0xa0 → b.le 0x1dfdb8 tail-call GetColor).
        color_type::SCHEME => {
            // raw: if mapper == null → invalid
            let Some(m) = mapper else {
                return RenderColor::INVALID;
            };
            let scheme_value = u32::from_le_bytes([
                color.value[0],
                color.value[1],
                color.value[2],
                color.value[3],
            ]);
            // raw 의 두 tree 검증 + GetColor 호출 = try_resolve_scheme.
            // None 이면 invalid.
            m.try_resolve_scheme(scheme_value).unwrap_or(RenderColor::INVALID)
        }

        // raw case SYSTEM (0x1dfccc): w0 = GetSysColor(value); fall through to common write.
        color_type::SYSTEM => {
            let sys_idx = u32::from_le_bytes([
                color.value[0],
                color.value[1],
                color.value[2],
                color.value[3],
            ]);
            let colorref = sys_provider.get_sys_color(sys_idx);
            // raw 0x1dfce0-ec: strh w0, [x19]; lsr w8, w0, #16; strb w8, [x19, #2]
            // 즉 r = w0[0..8], g = w0[8..16], b = w0[16..24]
            let r = (colorref & 0xff) as u8;
            let g = ((colorref >> 8) & 0xff) as u8;
            let b = ((colorref >> 16) & 0xff) as u8;
            RenderColor::rgb(r, g, b)
        }

        // raw case PRESET (0x1dfcd8): w0 = Color::GetPresetColor(value); 동일 write.
        color_type::PRESET => {
            let preset_value = u32::from_le_bytes([
                color.value[0],
                color.value[1],
                color.value[2],
                color.value[3],
            ]);
            let colorref = Color::get_preset_color(PresetStyle(preset_value));
            let r = (colorref & 0xff) as u8;
            let g = ((colorref >> 8) & 0xff) as u8;
            let b = ((colorref >> 16) & 0xff) as u8;
            RenderColor::rgb(r, g, b)
        }

        // raw case SC_RGB (0x1dfcf0): 3 f32 → round(* 255) → u8 each.
        color_type::SC_RGB => {
            // raw: d8 = 255.0 (bit pattern 0x406fe00000000000)
            //      for each of [+0, +4, +8]: ldr s0; fcvt d0,s0; fmul d0,d0,d8; bl Round; → u8
            let r_f32 = f32::from_le_bytes([
                color.value[0],
                color.value[1],
                color.value[2],
                color.value[3],
            ]);
            let g_f32 = f32::from_le_bytes([
                color.value[4],
                color.value[5],
                color.value[6],
                color.value[7],
            ]);
            let b_f32 = f32::from_le_bytes([
                color.value[8],
                color.value[9],
                color.value[10],
                color.value[11],
            ]);
            let r = round_to_i64(r_f32 as f64 * 255.0) as u8;
            let g = round_to_i64(g_f32 as f64 * 255.0) as u8;
            let b = round_to_i64(b_f32 as f64 * 255.0) as u8;
            // raw 0x1dfd34: bfi w21, w22, #8, #24 → w21 = r | (g << 8)
            //      0x1dfd38: strb w0, [x19, #2] → b at +2
            //      0x1dfd3c: strh w21, [x19]    → (r, g) at +0, +1
            //      0x1dfd40: b 0x1dfd98 → mov w8, #0xff01; strh w8, [x19, #4]
            RenderColor::rgb(r, g, b)
        }

        // raw case HSL (0x1dfd44): in-place Degree ctor + HslToRgb call.
        color_type::HSL => {
            // raw 0x1dfd4c-50: Degree::Degree(hsl.h) on stack
            let h_f32 = f32::from_le_bytes([
                color.value[0],
                color.value[1],
                color.value[2],
                color.value[3],
            ]);
            let s_f32 = f32::from_le_bytes([
                color.value[4],
                color.value[5],
                color.value[6],
                color.value[7],
            ]);
            let l_f32 = f32::from_le_bytes([
                color.value[8],
                color.value[9],
                color.value[10],
                color.value[11],
            ]);
            let mut r = 0u8;
            let mut g = 0u8;
            let mut b = 0u8;
            let degree = Degree::from_float(h_f32);
            // raw 0x1dfd5c-74: out r/g/b ptrs + Degree + s + l + 1.0 + 1.0
            hsl_to_rgb(&mut r, &mut g, &mut b, &degree, s_f32, l_f32, 1.0, 1.0);
            // raw 0x1dfd80-94: ldrb out_r/g/b; orr + strb + strh
            RenderColor::rgb(r, g, b)
        }

        // unreachable: type_tag > 6 guarded above; 0..6 covered.
        _ => RenderColor::INVALID,
    }
}

// =============================================================================
// L-5c-9b3 (부분): SRC small Render-effect output factories
// =============================================================================

/// raw `Hnc::Shape::Render::Grayscale` — 16B effect output.
///
/// raw 의 SRC::ToGrayscale @ `0x17f458` 가 생성하는 struct.
/// ```text
/// +0x00: vtable*  (raw `0x778c90` — Hnc::Shape::Render::Grayscale 의 vtable addr)
/// +0x08: state   (u8 — raw 의 `strb wzr` = 0)
/// (7B padding)
/// ```
///
/// 16B alloc'd via `new(0x10)`. **vtable addr 는 raw 의 표식** — 본 Rust port 는
/// rendering callback 시 enum tag 으로 dispatch (실제 함수 ptr 호출 안 함).
#[repr(C)]
pub struct RenderGrayscale {
    /// raw +0x00: vtable ptr. byte-eq 위해 raw addr 0x778c90 마커 그대로 보존.
    pub vtable: *const u8,
    /// raw +0x08: state byte (= 0 in this ctor).
    pub state: u8,
}

/// raw vtable address marker — `0x778c90` (libHncDrawingEngine binary offset).
pub const RENDER_GRAYSCALE_VTABLE_ADDR: usize = 0x778c90;

impl RenderGrayscale {
    /// raw vtable addr 로 채우는 byte-eq init.
    ///
    /// 본 Rust port 는 vtable 을 dereference 하지 않음 (`*const u8` 로만 보관).
    /// dispatch 가 필요한 callsite 는 별도 enum 사용.
    pub fn new() -> Self {
        RenderGrayscale {
            vtable: RENDER_GRAYSCALE_VTABLE_ADDR as *const u8,
            state: 0,
        }
    }
}

/// raw `ShapeRenderConverter::ToGrayscale(Grayscale const&)` @ `0x17f458` (14 instr) 1:1.
///
/// ```asm
/// 17f458-460: stack frame setup (32B)
/// 17f464:  mov  x19, x8           ; sret slot
/// 17f468:  mov  w0, #0x10         ; alloc size = 16
/// 17f46c:  bl   __Znwm            ; new RenderGrayscale (16B)
/// 17f470:  adrp x8, 0x778000      ; vtable page
/// 17f474:  add  x8, x8, #0xc90    ; vtable = 0x778c90
/// 17f478:  str  x8, [x0]          ; new_obj.vtable = vtable
/// 17f47c:  strb wzr, [x0, #0x8]   ; new_obj.state = 0
/// 17f480:  str  x0, [x19]         ; *sret = new_obj
/// 17f484-c: epilogue
/// 17f48c:  ret
/// ```
///
/// 입력 `Grayscale` argument 는 **무시** — Grayscale 가 marker-only (empty) class 라
/// fields 가 없음. SRC 는 단지 결과로 RenderGrayscale 을 만들어 sret 으로 전달.
///
/// **byte-eq**: 정확히 raw 의 16B alloc + 8B vtable + 1B state(0) + 7B padding.
pub fn to_grayscale(_input_grayscale: &()) -> Box<RenderGrayscale> {
    Box::new(RenderGrayscale::new())
}

/// raw `ShapeRenderConverter::ToFillRenderMode(BWMode)` @ `0x1b9368` (7 instr) 1:1.
///
/// ```asm
/// 1b9368: sub  w8, w0, #0x2         ; w8 = bw - 2
/// 1b936c: cmp  w8, #0xa             ; bw-2 vs 10
/// 1b9370: b.hi out_of_range          ; if > 10 → return 0
/// 1b9374: adrp x9, 0x750000          ; table page
/// 1b9378: add  x9, x9, #0x8a0        ; table @ 0x7508a0 (11 entries u32)
/// 1b937c: ldr  w0, [x9, w8, sxtw #2] ; return table[bw-2]
/// 1b9380: ret
/// 1b9384: mov  w0, #0x0
/// 1b9388: ret
/// ```
///
/// 이미 `bw_mode::to_fill_render_mode_u32` 로 byte-eq port 됨 → 본 SRC wrapper 는 단순 위임.
#[inline]
pub fn to_fill_render_mode(bw_raw: u32) -> u32 {
    crate::bw_mode::to_fill_render_mode_u32(bw_raw)
}

/// raw `ShapeRenderConverter::ToOutlineRenderMode(BWMode)` @ `0x1b9408` — 동일 패턴.
#[inline]
pub fn to_outline_render_mode(bw_raw: u32) -> u32 {
    crate::bw_mode::to_outline_render_mode_u32(bw_raw)
}

// =============================================================================
// L-5c-9b3 (4종): SRC ToBiLevel / ToColorTemperature / ToSaturation / ToLuminance
// =============================================================================
//
// 공통 패턴 (raw 의 4 함수 모두 동일):
// 1. 입력 shape effect (16B: vtable@+0 + PropertyBag@+0x8) 에서 PropertyBag 의
//    underlying ControlBlock<PropertyBagImpl>* 를 `[x0+0x8]` 으로 추출.
// 2. PropertyKey(id) 를 stack 에 build, GetValueHelper 호출하여 typed value
//    pointer (= Property+0xc) 획득.
// 3. 4B (float 또는 u32) load.
// 4. PropertyKey D1 (Rust 에선 Drop 으로 자동).
// 5. (optional) 값 clamp.
// 6. `new(0x10)` (16B) — vtable + value 저장.
// 7. sret 으로 반환 (Rust 에선 Box<RenderXxx> 반환).
//
// **byte-eq scope**: 결과 RenderMode struct 의 vtable/value 가 raw 의 alloc 과 1:1.
// 입력 shape effect 의 PropertyBag 가 비어 있거나 ctrl null 이면 raw 는
// `mov x0, #0; bl GetValueHelper` 하여 helper 내부에서 `out_of_range` 던짐
// (Rust 에선 panic). 호출자 책임.

// ----- 입력 effect wrappers (Hnc::Shape::{BiLevel,ColorTemperature,Saturation,Luminance})

/// raw 16B `Hnc::Shape::BiLevel` — PropertyBag wrapper (vtable + bag).
///
/// raw layout (확정 from C2(float) ctor `0x147efc`):
/// ```text
/// +0x00: vtable*  (raw `0x77aec0`)
/// +0x08: PropertyBag bag    ; ControlBlock<PropertyBagImpl>* (8B)
/// ```
///
/// **본 단계 scope**: SRC ToBiLevel 의 입력 layout 만 byte-eq.
/// 자체 ctor (`0x147efc` 등) / dtor (`0x148124`) / vtable methods 는 별도 sub-task.
#[repr(C)]
pub struct ShapeBiLevel {
    /// raw +0x00: vtable ptr (= `0x77aec0`).
    pub vtable: *const u8,
    /// raw +0x08: PropertyBag (= 8B SharePtr).
    pub bag: crate::property_bag::PropertyBag,
}
pub const SHAPE_BILEVEL_VTABLE_ADDR: usize = 0x77aec0;
const _: () = assert!(std::mem::size_of::<ShapeBiLevel>() == 16);
const _: () = assert!(std::mem::align_of::<ShapeBiLevel>() == 8);

impl ShapeBiLevel {
    /// Empty BiLevel — default PropertyBag (non-merged). raw 의 default ctor 대체.
    pub fn new_empty() -> Self {
        ShapeBiLevel {
            vtable: SHAPE_BILEVEL_VTABLE_ADDR as *const u8,
            bag: crate::property_bag::PropertyBag::new(false),
        }
    }
}

/// raw 16B `Hnc::Shape::ColorTemperature` — PropertyBag wrapper.
#[repr(C)]
pub struct ShapeColorTemperature {
    pub vtable: *const u8,
    pub bag: crate::property_bag::PropertyBag,
}
pub const SHAPE_COLOR_TEMPERATURE_VTABLE_ADDR: usize = 0x77b150;
const _: () = assert!(std::mem::size_of::<ShapeColorTemperature>() == 16);

impl ShapeColorTemperature {
    pub fn new_empty() -> Self {
        ShapeColorTemperature {
            vtable: SHAPE_COLOR_TEMPERATURE_VTABLE_ADDR as *const u8,
            bag: crate::property_bag::PropertyBag::new(false),
        }
    }
}

/// raw 16B `Hnc::Shape::Saturation` — PropertyBag wrapper.
#[repr(C)]
pub struct ShapeSaturation {
    pub vtable: *const u8,
    pub bag: crate::property_bag::PropertyBag,
}
pub const SHAPE_SATURATION_VTABLE_ADDR: usize = 0x77b1d8;
const _: () = assert!(std::mem::size_of::<ShapeSaturation>() == 16);

impl ShapeSaturation {
    pub fn new_empty() -> Self {
        ShapeSaturation {
            vtable: SHAPE_SATURATION_VTABLE_ADDR as *const u8,
            bag: crate::property_bag::PropertyBag::new(false),
        }
    }
}

/// raw 16B `Hnc::Shape::Luminance` — PropertyBag wrapper.
#[repr(C)]
pub struct ShapeLuminance {
    pub vtable: *const u8,
    pub bag: crate::property_bag::PropertyBag,
}
pub const SHAPE_LUMINANCE_VTABLE_ADDR: usize = 0x77af48;
const _: () = assert!(std::mem::size_of::<ShapeLuminance>() == 16);

impl ShapeLuminance {
    pub fn new_empty() -> Self {
        ShapeLuminance {
            vtable: SHAPE_LUMINANCE_VTABLE_ADDR as *const u8,
            bag: crate::property_bag::PropertyBag::new(false),
        }
    }
}

// ----- 출력 RenderMode types (Hnc::Shape::Render::{...})

/// raw 16B `Hnc::Shape::Render::BiLevel` — output of `SRC::ToBiLevel`.
///
/// ```text
/// +0x00: vtable* (raw `0x778c30`)
/// +0x08: threshold (f32, clamped [0,1])
/// (4B padding)
/// ```
#[repr(C)]
pub struct RenderBiLevel {
    pub vtable: *const u8,
    pub threshold: f32,
    _pad: u32,
}
pub const RENDER_BILEVEL_VTABLE_ADDR: usize = 0x778c30;
const _: () = assert!(std::mem::size_of::<RenderBiLevel>() == 16);

/// raw 16B `Hnc::Shape::Render::ColorTemperature` — output of `SRC::ToColorTemperature`.
///
/// ```text
/// +0x00: vtable* (raw `0x778918`)
/// +0x08: temperature (u32 — w-register read; **no clamping**)
/// (4B padding)
/// ```
#[repr(C)]
pub struct RenderColorTemperature {
    pub vtable: *const u8,
    pub temperature: u32,
    _pad: u32,
}
pub const RENDER_COLOR_TEMPERATURE_VTABLE_ADDR: usize = 0x778918;
const _: () = assert!(std::mem::size_of::<RenderColorTemperature>() == 16);

/// raw 16B `Hnc::Shape::Render::Saturation` — output of `SRC::ToSaturation`.
///
/// ```text
/// +0x00: vtable* (raw `0x7793c0`)
/// +0x08: saturation (f32 — **no clamping** per raw asm)
/// (4B padding)
/// ```
#[repr(C)]
pub struct RenderSaturation {
    pub vtable: *const u8,
    pub saturation: f32,
    _pad: u32,
}
pub const RENDER_SATURATION_VTABLE_ADDR: usize = 0x7793c0;
const _: () = assert!(std::mem::size_of::<RenderSaturation>() == 16);

/// raw 16B `Hnc::Shape::Render::Luminance` — output of `SRC::ToLuminance`.
///
/// ```text
/// +0x00: vtable* (raw `0x778db0`)
/// +0x08: brightness (f32, clamped [-1,1])
/// +0x0c: contrast   (f32, clamped [-1,1])
/// ```
#[repr(C)]
pub struct RenderLuminance {
    pub vtable: *const u8,
    pub brightness: f32,
    pub contrast: f32,
}
pub const RENDER_LUMINANCE_VTABLE_ADDR: usize = 0x778db0;
const _: () = assert!(std::mem::size_of::<RenderLuminance>() == 16);

// ----- helper: extract PropertyBagImpl* from a shape effect's `bag` field

/// raw `ldr x8, [x0, #0x8]; cbz; ldr x0, [x8]` pattern — extract PropertyBagImpl*
/// from a 16B shape effect (vtable @ +0, bag @ +0x8). null-safe.
///
/// 본 함수는 **byte-eq utility** — raw 의 `0x148a24-0x148a38` (BiLevel) /
/// `0x151288-0x15129c` (ColorTemperature) / `0x1d04bc-0x1d04d0` (Saturation) /
/// `0x19a768-0x19a77c` (Luminance) 공통 패턴.
///
/// # Safety
/// `bag` 가 valid PropertyBag (ctrl 가 null 이거나 valid ControlBlock).
#[inline]
unsafe fn extract_bag_impl(
    bag: &crate::property_bag::PropertyBag,
) -> *const crate::property_bag::PropertyBagImpl {
    let ctrl = bag.ctrl;
    if ctrl.is_null() {
        std::ptr::null()
    } else {
        (*ctrl).obj as *const _
    }
}

// ----- SRC::ToBiLevel / ToColorTemperature / ToSaturation / ToLuminance

/// raw `ShapeRenderConverter::ToBiLevel(BiLevel const&)` @ `0x148a00` (37 instr) 1:1.
///
/// ```asm
/// 148a18: w8 = 0x393; str w8, [sp]       ; PropertyKey.int_id = 0x393
/// 148a20: str xzr, [sp, #0x8]            ; PropertyKey.str_ptr = null
/// 148a24-38: x0 = [x0+0x8] = bag.ctrl; deref → PropertyBagImpl*
/// 148a38: x1 = sp (= &PropertyKey)
/// 148a3c: bl GetValueHelper             ; x0 = void* (= Property+0xc)
/// 148a40: s8 = [x0]                     ; load 4B as float
/// 148a48: PropertyKey D1
/// 148a4c: w0 = 0x10; bl __Znwm           ; alloc 16B
/// 148a54-5c: vtable = 0x778c30; str at +0
/// 148a60: str s8, [x0, #0x8]             ; threshold = raw value
/// // clamp: if (s8 < 0): s0 = 0; if (s8 > 1): s0 = 1; else: s0 = s8
/// 148a64-78: clamp [0, 1]
/// 148a7c: str s0, [x0, #0x8]
/// 148a80: str x0, [x19]                  ; *sret = result
/// 148a94: ret
/// ```
///
/// **byte-eq**: PropertyKey int_id = 0x393, vtable = `0x778c30`, threshold clamped [0,1].
///
/// # Panics
/// PropertyBag 가 null 또는 key 0x393 가 bag 에 없으면 `GetValue*` 가 panic.
/// (raw 는 `out_of_range` 던짐 → ___cxa_throw)
pub fn to_bi_level(input: &ShapeBiLevel) -> Box<RenderBiLevel> {
    let key = crate::property_key::PropertyKey::from_int(0x393);
    let raw_value: f32 = unsafe {
        let bag_impl = extract_bag_impl(&input.bag);
        let value_ptr =
            crate::property_bag::PropertyBagImpl::get_value_addr(bag_impl, &key) as *const f32;
        *value_ptr
    };
    // raw clamp: NaN/neg → 0, > 1 → 1, else → raw_value.
    // raw asm: `movi d0, #0; fcmp s8, #0.0; b.mi clamp_low; fmov s0, #1.0;
    //          fcmp s8, s0; b.le ok; clamp_low: str 0/1; ok: str raw`
    // Equivalent to: s0 starts as 0, then becomes raw_value if 0<=raw<=1, else 0 or 1.
    let mut threshold = raw_value;
    if !(raw_value >= 0.0) {
        // covers NaN and negative
        threshold = 0.0;
    } else if raw_value > 1.0 {
        threshold = 1.0;
    }
    Box::new(RenderBiLevel {
        vtable: RENDER_BILEVEL_VTABLE_ADDR as *const u8,
        threshold,
        _pad: 0,
    })
}

/// raw `ShapeRenderConverter::ToColorTemperature(ColorTemperature const&)` @ `0x151268`
/// (29 instr) 1:1.
///
/// 동일 패턴 — key 0x3cd, vtable 0x778918, value 는 **u32 (w20 register, not float)**,
/// **clamping 없음**.
pub fn to_color_temperature(
    input: &ShapeColorTemperature,
) -> Box<RenderColorTemperature> {
    let key = crate::property_key::PropertyKey::from_int(0x3cd);
    let temperature: u32 = unsafe {
        let bag_impl = extract_bag_impl(&input.bag);
        let value_ptr =
            crate::property_bag::PropertyBagImpl::get_value_addr(bag_impl, &key) as *const u32;
        *value_ptr
    };
    Box::new(RenderColorTemperature {
        vtable: RENDER_COLOR_TEMPERATURE_VTABLE_ADDR as *const u8,
        temperature,
        _pad: 0,
    })
}

/// raw `ShapeRenderConverter::ToSaturation(Saturation const&)` @ `0x1d0498` (32 instr) 1:1.
///
/// 동일 패턴 — key 0x3cc, vtable 0x7793c0, value 는 f32, **clamping 없음**.
pub fn to_saturation(input: &ShapeSaturation) -> Box<RenderSaturation> {
    let key = crate::property_key::PropertyKey::from_int(0x3cc);
    let saturation: f32 = unsafe {
        let bag_impl = extract_bag_impl(&input.bag);
        let value_ptr =
            crate::property_bag::PropertyBagImpl::get_value_addr(bag_impl, &key) as *const f32;
        *value_ptr
    };
    Box::new(RenderSaturation {
        vtable: RENDER_SATURATION_VTABLE_ADDR as *const u8,
        saturation,
        _pad: 0,
    })
}

/// raw `ShapeRenderConverter::ToLuminance(Luminance const&)` @ `0x19a740` (~56 instr) 1:1.
///
/// **2 키 sequence**: 0x399 (brightness) + 0x39a (contrast). 둘 다 f32, 각각 [-1, 1] clamp.
///
/// raw 의 stack 사용:
/// ```asm
/// 19a75c: w8 = 0x399; str at sp        ; key 1
/// ... GetValue → s8 (brightness)
/// 19a790: w8 = 0x39a; str at sp        ; key 2 (reuse stack)
/// ... GetValue → s9 (contrast)
/// 19a7c4: alloc 16B
/// 19a7cc-d8: vtable 0x778db0 + stp s8,s9 [+0x8, +0xc]
/// 19a7dc-94: clamp brightness [-1, 1] → +0x8
/// 19a7f8-810: clamp contrast [-1, 1] → +0xc
/// ```
///
/// **byte-eq**: 2 GetValue 호출 sequence + 2 clamp + sret store.
pub fn to_luminance(input: &ShapeLuminance) -> Box<RenderLuminance> {
    // raw `19a75c-78`: key 0x399 (brightness)
    let brightness = unsafe {
        let key = crate::property_key::PropertyKey::from_int(0x399);
        let bag_impl = extract_bag_impl(&input.bag);
        let value_ptr =
            crate::property_bag::PropertyBagImpl::get_value_addr(bag_impl, &key) as *const f32;
        *value_ptr
    };
    // raw `19a790-bc`: key 0x39a (contrast)
    let contrast = unsafe {
        let key = crate::property_key::PropertyKey::from_int(0x39a);
        let bag_impl = extract_bag_impl(&input.bag);
        let value_ptr =
            crate::property_bag::PropertyBagImpl::get_value_addr(bag_impl, &key) as *const f32;
        *value_ptr
    };
    // raw `19a7dc-810`: clamp 둘 다 [-1, 1].
    // 알고리즘: s0 starts as -1; if v < -1 → -1; elif v > 1 → 1; else v.
    // raw asm: `fmov s0, #-1.0; fcmp v, s0; b.mi clamp_neg; fmov s0, #1.0;
    //          fcmp v, s0; b.le ok; clamp_neg: store s0 (= -1 or 1); ok: store v`
    let clamp = |v: f32| -> f32 {
        if !(v >= -1.0) {
            -1.0
        } else if v > 1.0 {
            1.0
        } else {
            v
        }
    };
    Box::new(RenderLuminance {
        vtable: RENDER_LUMINANCE_VTABLE_ADDR as *const u8,
        brightness: clamp(brightness),
        contrast: clamp(contrast),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::SystemStyle;
    use std::ptr;

    /// Mock mapper: returns fixed color for one known scheme, None otherwise.
    struct MockMapper {
        known_scheme: u32,
        result: RenderColor,
    }

    impl ColorMapperLike for MockMapper {
        fn try_resolve_scheme(&self, scheme: u32) -> Option<RenderColor> {
            if scheme == self.known_scheme {
                Some(self.result)
            } else {
                None
            }
        }
    }

    /// Mock sys color provider with hardcoded table.
    struct MockSys;
    impl SystemColorProvider for MockSys {
        fn get_sys_color(&self, idx: u32) -> u32 {
            // arbitrary mapping for tests
            match idx {
                5 => 0x00_00_00_00, // black
                8 => 0x00_ff_ff_ff, // white (R=ff, G=ff, B=ff)
                _ => 0x00_80_80_80, // mid gray
            }
        }
    }

    // ====== ColorMode 기존 테스트 ======

    #[test]
    fn color_mode_from_u32_matches_raw_cmp_arms() {
        assert_eq!(ColorMode::from_u32(0), ColorMode::None);
        assert_eq!(ColorMode::from_u32(1), ColorMode::Grayscale);
        assert_eq!(ColorMode::from_u32(2), ColorMode::BlackWhite);
        assert_eq!(ColorMode::from_u32(3), ColorMode::None,
            "raw cmp w1,#2; b.ne 0x1df80c (return) for value > 2");
        assert_eq!(ColorMode::from_u32(0xff), ColorMode::None);
    }

    #[test]
    fn raw_flag_bit_matches_raw_const() {
        assert_eq!(ColorMode::Grayscale.raw_flag_bit(), 0x40,
            "raw 0x1df7fc mov w8,#0x40");
        assert_eq!(ColorMode::BlackWhite.raw_flag_bit(), 0x80,
            "raw 0x1df7f4 mov w8,#0x80");
        assert_eq!(ColorMode::None.raw_flag_bit(), 0,
            "raw b.ne 0x1df80c (skip orr+str)");
    }

    #[test]
    fn apply_color_mode_grayscale_sets_bit_0x40() {
        let mut f = Flag(0);
        apply_color_mode(&mut f, ColorMode::Grayscale);
        assert_eq!(f.0, 0x40);
    }

    #[test]
    fn apply_color_mode_blackwhite_sets_bit_0x80() {
        let mut f = Flag(0);
        apply_color_mode(&mut f, ColorMode::BlackWhite);
        assert_eq!(f.0, 0x80);
    }

    #[test]
    fn apply_color_mode_preserves_other_bits() {
        let mut f = Flag(0x1234);
        apply_color_mode(&mut f, ColorMode::Grayscale);
        assert_eq!(f.0, 0x1234 | 0x40,
            "raw orr x8, x9, x8 = OR, 다른 bit 보존");
    }

    #[test]
    fn apply_color_mode_none_is_noop() {
        let mut f = Flag(0xdead);
        apply_color_mode(&mut f, ColorMode::None);
        assert_eq!(f.0, 0xdead, "raw cmp != 1 && cmp != 2 → b.ne return");
    }

    #[test]
    fn apply_color_mode_grayscale_twice_idempotent() {
        let mut f = Flag(0);
        apply_color_mode(&mut f, ColorMode::Grayscale);
        apply_color_mode(&mut f, ColorMode::Grayscale);
        assert_eq!(f.0, 0x40, "OR with same bit twice = same bit");
    }

    // ====== ToColorMode (reverse of ApplyColorMode) tests ======

    #[test]
    fn to_color_mode_none_for_unset() {
        assert_eq!(to_color_mode(&Flag(0)), 0, "no bits → None");
    }

    #[test]
    fn to_color_mode_grayscale_for_bit_40() {
        assert_eq!(to_color_mode(&Flag(0x40)), 1);
    }

    #[test]
    fn to_color_mode_blackwhite_for_bit_80() {
        assert_eq!(to_color_mode(&Flag(0x80)), 2);
    }

    #[test]
    fn to_color_mode_grayscale_priority_over_blackwhite() {
        // raw csel chain: bit6 takes priority over bit7
        assert_eq!(to_color_mode(&Flag(0xc0)), 1, "0x40+0x80 → Grayscale wins");
    }

    #[test]
    fn to_color_mode_mode3_for_bit_27() {
        assert_eq!(to_color_mode(&Flag(0x0800_0000)), 3);
    }

    #[test]
    fn to_color_mode_blackwhite_priority_over_mode3() {
        // raw: bit7 set → return 2 (BW) regardless of bit27
        assert_eq!(
            to_color_mode(&Flag(0x80 | 0x0800_0000)),
            2,
            "BW > Mode3"
        );
    }

    #[test]
    fn to_color_mode_grayscale_priority_over_mode3() {
        assert_eq!(
            to_color_mode(&Flag(0x40 | 0x0800_0000)),
            1,
            "Gray > Mode3"
        );
    }

    #[test]
    fn to_color_mode_preserves_other_bits_unchecked() {
        // 다른 bit 가 있어도 무시
        assert_eq!(to_color_mode(&Flag(0xdead_0000_0040)), 1);
        assert_eq!(to_color_mode(&Flag(0xdead_0000_0000)), 0);
    }

    // ====== Coordinate scaler family tests ======

    #[test]
    fn logical_to_render_default_unit_scales_by_96() {
        // default unit=1.0 → output = input * 96
        // (race-aware: test 가 unit 을 변경 안 함)
        let r = logical_to_render_scalar(1.0);
        assert_eq!(r, 96.0);
        let r2 = logical_to_render_scalar(0.5);
        assert_eq!(r2, 48.0);
    }

    #[test]
    fn render_to_logical_default_unit_inverse() {
        // default unit=1.0 → output = input / 96
        let l = render_to_logical_scalar(96.0);
        assert_eq!(l, 1.0);
        let l2 = render_to_logical_scalar(48.0);
        assert_eq!(l2, 0.5);
    }

    #[test]
    fn render_to_device_scale_nonzero_multiplies() {
        // raw: device = scale * p
        assert_eq!(render_to_device_scalar(10.0, 2.0), 20.0);
        assert_eq!(render_to_device_scalar(5.0, 3.0), 15.0);
    }

    #[test]
    fn render_to_device_scale_zero_uses_one() {
        // raw fcsel eq: scale == 0 → 1.0
        assert_eq!(render_to_device_scalar(10.0, 0.0), 10.0);
    }

    #[test]
    fn device_to_logical_scale_zero_uses_one() {
        // raw fcsel eq: scale == 0 → 1.0
        // result = (10 / 1) * 1.0 / 96 = 10/96
        let l = device_to_logical_scalar(96, 0.0);
        assert_eq!(l, 1.0, "device 96 px at scale=0 (→1) → logical 1");
    }

    #[test]
    fn device_to_logical_nonzero_scale_divides_first() {
        // (192 / 2) * 1.0 / 96 = 1.0
        let l = device_to_logical_scalar(192, 2.0);
        assert_eq!(l, 1.0);
    }

    #[test]
    fn logical_to_device_rounds_to_i64() {
        // logical 1.0, scale 1.0 → render 96.0 * 1.0 = 96.0 → Round → 96
        let d = logical_to_device_scalar(1.0, 1.0);
        assert_eq!(d, 96);
        // logical 0.5, scale 2.0 → render 48.0 * 2.0 = 96.0 → 96
        let d2 = logical_to_device_scalar(0.5, 2.0);
        assert_eq!(d2, 96);
    }

    #[test]
    fn logical_to_device_rounds_half_away_from_zero() {
        // logical 0.5 / 96, scale 1.0 → 0.5 → round_half_away_from_zero → 1
        // To get exactly 0.5: pick logical L so that L * 96 = 0.5 → L = 0.5/96
        let d = logical_to_device_scalar(0.5 / 96.0, 1.0);
        assert_eq!(d, 1, "raw MathUtil::Round: 0.5 → 1");
    }

    // ====== ApplyRenderMode tests ======

    struct NoopColorUtil;
    impl ColorUtilProvider for NoopColorUtil {
        fn to_gray(&self, c: &mut RenderColor) {
            // 검증용: r 만 표시로 0xAA
            c.b0 = 0xAA;
        }
        fn to_light_gray(&self, c: &mut RenderColor) {
            c.b0 = 0xBB;
        }
        fn to_inverse_gray(&self, c: &mut RenderColor) {
            c.b0 = 0xCC;
        }
    }

    #[test]
    fn apply_render_mode_zero_is_noop() {
        let mut c = RenderColor::rgb(0x11, 0x22, 0x33);
        let original = c;
        apply_render_mode(&mut c, 0, &NoopColorUtil, &MockSys);
        assert_eq!(c, original, "mode=0 → no-op");
    }

    #[test]
    fn apply_render_mode_nine_plus_is_noop() {
        let mut c = RenderColor::rgb(0x11, 0x22, 0x33);
        let original = c;
        apply_render_mode(&mut c, 9, &NoopColorUtil, &MockSys);
        assert_eq!(c, original);
        apply_render_mode(&mut c, 100, &NoopColorUtil, &MockSys);
        assert_eq!(c, original);
    }

    #[test]
    fn apply_render_mode_1_dispatches_to_gray() {
        let mut c = RenderColor::rgb(0x11, 0x22, 0x33);
        apply_render_mode(&mut c, 1, &NoopColorUtil, &MockSys);
        assert_eq!(c.b0, 0xAA, "mode=1 → ColorUtil::ToGray");
    }

    #[test]
    fn apply_render_mode_2_dispatches_to_light_gray() {
        let mut c = RenderColor::rgb(0x11, 0x22, 0x33);
        apply_render_mode(&mut c, 2, &NoopColorUtil, &MockSys);
        assert_eq!(c.b0, 0xBB, "mode=2 → ToLightGray");
    }

    #[test]
    fn apply_render_mode_3_dispatches_to_inverse_gray() {
        let mut c = RenderColor::rgb(0x11, 0x22, 0x33);
        apply_render_mode(&mut c, 3, &NoopColorUtil, &MockSys);
        assert_eq!(c.b0, 0xCC, "mode=3 → ToInverseGray");
    }

    #[test]
    fn apply_render_mode_4_writes_light_gray_solid() {
        let mut c = RenderColor::rgb(0x11, 0x22, 0x33);
        apply_render_mode(&mut c, 4, &NoopColorUtil, &MockSys);
        assert_eq!(
            (c.b0, c.b1, c.b2),
            (0xA0, 0xA0, 0xA0),
            "raw mov w8, #0xa0a0; strh; mov w8, #0xa0; strb"
        );
        assert_eq!(c.color_type, 1);
    }

    #[test]
    fn apply_render_mode_5_writes_white() {
        let mut c = RenderColor::rgb(0x11, 0x22, 0x33);
        apply_render_mode(&mut c, 5, &NoopColorUtil, &MockSys);
        assert_eq!((c.b0, c.b1, c.b2), (0xFF, 0xFF, 0xFF));
        assert_eq!(c.color_type, 1);
    }

    #[test]
    fn apply_render_mode_6_writes_black() {
        let mut c = RenderColor::rgb(0x11, 0x22, 0x33);
        apply_render_mode(&mut c, 6, &NoopColorUtil, &MockSys);
        assert_eq!((c.b0, c.b1, c.b2), (0, 0, 0));
        assert_eq!(c.color_type, 1);
    }

    #[test]
    fn apply_render_mode_7_uses_sys_color_5() {
        // MockSys: 5 → 0x00_00_00_00 = black
        let mut c = RenderColor::rgb(0x11, 0x22, 0x33);
        apply_render_mode(&mut c, 7, &NoopColorUtil, &MockSys);
        assert_eq!((c.b0, c.b1, c.b2), (0, 0, 0));
        assert_eq!(c.color_type, 1);
    }

    #[test]
    fn apply_render_mode_8_uses_sys_color_8() {
        // MockSys: 8 → 0x00_ff_ff_ff (R=ff, G=ff, B=ff = white)
        let mut c = RenderColor::rgb(0x11, 0x22, 0x33);
        apply_render_mode(&mut c, 8, &NoopColorUtil, &MockSys);
        assert_eq!((c.b0, c.b1, c.b2), (0xff, 0xff, 0xff));
        assert_eq!(c.color_type, 1);
    }

    #[test]
    fn apply_render_mode_constants_preserve_alpha() {
        // raw 의 case 4/5/6 에서 alpha (+5) 는 안 건드림
        let mut c = RenderColor {
            b0: 0x11, b1: 0x22, b2: 0x33, b3: 0,
            color_type: 1, alpha: 0x80,
        };
        apply_render_mode(&mut c, 5, &NoopColorUtil, &MockSys);
        assert_eq!(c.alpha, 0x80, "raw 는 alpha 안 건드림");
    }

    // ====== RenderColor layout tests ======

    #[test]
    fn render_color_size_is_six_bytes() {
        assert_eq!(std::mem::size_of::<RenderColor>(), 6);
    }

    #[test]
    fn render_color_field_offsets() {
        let c = RenderColor::rgb(0x11, 0x22, 0x33);
        let p = &c as *const RenderColor as usize;
        assert_eq!(&c.b0 as *const _ as usize - p, 0);
        assert_eq!(&c.b1 as *const _ as usize - p, 1);
        assert_eq!(&c.b2 as *const _ as usize - p, 2);
        assert_eq!(&c.b3 as *const _ as usize - p, 3);
        assert_eq!(&c.color_type as *const _ as usize - p, 4);
        assert_eq!(&c.alpha as *const _ as usize - p, 5);
    }

    #[test]
    fn invalid_has_alpha_ff_type_one() {
        assert_eq!(RenderColor::INVALID.b0, 0);
        assert_eq!(RenderColor::INVALID.b1, 0);
        assert_eq!(RenderColor::INVALID.b2, 0);
        assert_eq!(RenderColor::INVALID.color_type, 1, "raw 0xff01 → type=1");
        assert_eq!(RenderColor::INVALID.alpha, 0xff);
    }

    #[test]
    fn rgb_helper_matches_raw_pattern() {
        let c = RenderColor::rgb(0x80, 0x90, 0xA0);
        assert_eq!((c.b0, c.b1, c.b2), (0x80, 0x90, 0xA0));
        assert_eq!(c.color_type, 1);
        assert_eq!(c.alpha, 0xff);
    }

    #[test]
    fn cmyk_helper_matches_raw_pattern() {
        let c = RenderColor::cmyk(0x10, 0x20, 0x30, 0x40);
        assert_eq!((c.b0, c.b1, c.b2, c.b3), (0x10, 0x20, 0x30, 0x40));
        assert_eq!(c.color_type, 0, "raw 0xff00 → type=0");
        assert_eq!(c.alpha, 0xff);
    }

    // ====== ToRenderColor dispatcher tests ======

    #[test]
    fn to_render_color_invalid_for_type_greater_six() {
        let c = Color {
            value: [0; 12],
            type_tag: 7,
            color_effect: ptr::null_mut(),
        };
        let r = to_render_color::<MockMapper, MockSys>(&c, None, &MockSys);
        assert_eq!(r, RenderColor::INVALID);
    }

    #[test]
    fn to_render_color_rgb_byte_eq() {
        let c = Color::from_rgb(0xAB, 0xCD, 0xEF, ptr::null_mut());
        let r = to_render_color::<MockMapper, MockSys>(&c, None, &MockSys);
        assert_eq!(r, RenderColor::rgb(0xAB, 0xCD, 0xEF));
    }

    #[test]
    fn to_render_color_cmyk_byte_eq() {
        // Color::from_cmyk uses Cmyk struct. Build with explicit Cmyk.
        let cmyk = crate::drawing_type::Cmyk {
            c: 0x11,
            m: 0x22,
            y: 0x33,
            k: 0x44,
        };
        let c = Color::from_cmyk(&cmyk);
        let r = to_render_color::<MockMapper, MockSys>(&c, None, &MockSys);
        assert_eq!(r, RenderColor::cmyk(0x11, 0x22, 0x33, 0x44));
    }

    #[test]
    fn to_render_color_system_via_callback() {
        let c = Color::from_system_style(SystemStyle::WINDOW_TEXT); // value=5
        let r = to_render_color::<MockMapper, MockSys>(&c, None, &MockSys);
        // MockSys: 5 → 0 → (0, 0, 0)
        assert_eq!(r, RenderColor::rgb(0, 0, 0));

        let c2 = Color::from_system_style(SystemStyle::WINDOW); // value=8
        let r2 = to_render_color::<MockMapper, MockSys>(&c2, None, &MockSys);
        // MockSys: 8 → 0xffffff → (0xff, 0xff, 0xff)
        assert_eq!(r2, RenderColor::rgb(0xff, 0xff, 0xff));
    }

    #[test]
    fn to_render_color_preset_via_table() {
        // PresetStyle(0) → PRESET_R/G/B[0] = (0xf0, 0xf8, 0xff)
        let c = Color::from_preset_style(PresetStyle(0));
        let r = to_render_color::<MockMapper, MockSys>(&c, None, &MockSys);
        assert_eq!(r, RenderColor::rgb(0xf0, 0xf8, 0xff));
    }

    #[test]
    fn to_render_color_preset_out_of_range_zero() {
        // PresetStyle(0xc0) > 0xbd → table returns 0 → (0, 0, 0) RGB
        let c = Color::from_preset_style(PresetStyle(0xc0));
        let r = to_render_color::<MockMapper, MockSys>(&c, None, &MockSys);
        assert_eq!(r, RenderColor::rgb(0, 0, 0));
    }

    #[test]
    fn to_render_color_scrgb_byte_eq() {
        // ScRgb (0.5, 0.0, 1.0) → round(0.5*255)=128, round(0.0*255)=0, round(1.0*255)=255
        let scrgb = crate::drawing_type::ScRgb {
            r: 0.5,
            g: 0.0,
            b: 1.0,
        };
        let c = Color::from_scrgb(&scrgb);
        let r = to_render_color::<MockMapper, MockSys>(&c, None, &MockSys);
        assert_eq!(r, RenderColor::rgb(128, 0, 255));
    }

    #[test]
    fn to_render_color_hsl_byte_eq() {
        // Hsl (0, 1, 0.5) = primary red
        let hsl = crate::drawing_type::Hsl {
            h: 0.0,
            s: 1.0,
            l: 0.5,
        };
        let c = Color::from_hsl(&hsl);
        let r = to_render_color::<MockMapper, MockSys>(&c, None, &MockSys);
        assert_eq!(r, RenderColor::rgb(255, 0, 0));
    }

    #[test]
    fn to_render_color_scheme_no_mapper_invalid() {
        // SCHEME 인데 mapper=None → invalid (raw cbz x1)
        let c = Color::from_scheme_style(
            crate::scheme_style::SchemeStyle::Background1,
            ptr::null_mut(),
        );
        let r = to_render_color::<MockMapper, MockSys>(&c, None, &MockSys);
        assert_eq!(r, RenderColor::INVALID);
    }

    #[test]
    fn to_render_color_scheme_mapper_resolves() {
        // Build a Color with SCHEME u32 value = 0x10 (phClr placeholder per FormatScheme).
        let c = Color::from_scheme_raw_u32(0x10, ptr::null_mut());
        let mapper = MockMapper {
            known_scheme: 0x10,
            result: RenderColor::rgb(0x11, 0x22, 0x33),
        };
        let r = to_render_color(&c, Some(&mapper), &MockSys);
        assert_eq!(r, RenderColor::rgb(0x11, 0x22, 0x33));
    }

    #[test]
    fn to_render_color_scheme_mapper_no_match_invalid() {
        let c = Color::from_scheme_raw_u32(0x99, ptr::null_mut());
        let mapper = MockMapper {
            known_scheme: 0x10,
            result: RenderColor::rgb(0x11, 0x22, 0x33),
        };
        let r = to_render_color(&c, Some(&mapper), &MockSys);
        assert_eq!(r, RenderColor::INVALID);
    }

    #[test]
    fn to_render_color_preset_at_index_1() {
        // PresetStyle(1) → PRESET_R/G/B[1] = (0xfa, 0xeb, 0xd7) = antiquewhite-ish
        let c = Color::from_preset_style(PresetStyle(1));
        let r = to_render_color::<MockMapper, MockSys>(&c, None, &MockSys);
        assert_eq!(r, RenderColor::rgb(0xfa, 0xeb, 0xd7));
    }

    // ----- L-5c-9b3 (부분): small SRC factories

    #[test]
    fn render_grayscale_raw_layout_16b() {
        // raw 17f468: mov w0, #0x10 = alloc 16B
        assert_eq!(std::mem::size_of::<RenderGrayscale>(), 16);
    }

    #[test]
    fn render_grayscale_vtable_addr_byte_eq() {
        // raw 17f470-474: vtable @ 0x778c90
        let r = RenderGrayscale::new();
        assert_eq!(r.vtable as usize, 0x778c90);
        assert_eq!(r.state, 0);
    }

    #[test]
    fn to_grayscale_returns_boxed_render_grayscale() {
        let g = to_grayscale(&());
        // raw 17f478-7c: vtable + state=0
        assert_eq!(g.vtable as usize, RENDER_GRAYSCALE_VTABLE_ADDR);
        assert_eq!(g.state, 0);
    }

    #[test]
    fn to_fill_render_mode_delegates_to_bwmode_table() {
        // raw 1b9368 sub w8, w0, #0x2; table[bw-2]
        assert_eq!(to_fill_render_mode(2), 1); // V2 → V1
        assert_eq!(to_fill_render_mode(11), 0); // V11 → V0 (last table entry = 0)
        assert_eq!(to_fill_render_mode(0), 0); // bw=0 → out-of-range → 0
        assert_eq!(to_fill_render_mode(13), 0); // > 11 → out-of-range → 0
    }

    #[test]
    fn to_outline_render_mode_delegates_to_bwmode_table() {
        // 동일 패턴 — bw_mode::to_outline_render_mode_u32 위임.
        // 위임만 검증 (값은 bw_mode tests 가 이미 검증).
        let r = to_outline_render_mode(2);
        let expected = crate::bw_mode::to_outline_render_mode_u32(2);
        assert_eq!(r, expected);
    }

    // ===== L-5c-9b3 (4종): SRC ToBiLevel / ToColorTemperature / ToSaturation / ToLuminance

    use crate::property::{state, PEnum, PFloat};
    use crate::property_key::PropertyKey;

    // ----- layout tests

    #[test]
    fn shape_bilevel_layout_16b_bag_offset_8() {
        let s = ShapeBiLevel::new_empty();
        let base = &s as *const _ as usize;
        let bag_addr = &s.bag as *const _ as usize;
        assert_eq!(std::mem::size_of::<ShapeBiLevel>(), 16);
        assert_eq!(bag_addr - base, 0x8);
        assert_eq!(s.vtable as usize, SHAPE_BILEVEL_VTABLE_ADDR);
    }

    #[test]
    fn shape_color_temperature_layout_16b() {
        let s = ShapeColorTemperature::new_empty();
        let base = &s as *const _ as usize;
        let bag_addr = &s.bag as *const _ as usize;
        assert_eq!(std::mem::size_of::<ShapeColorTemperature>(), 16);
        assert_eq!(bag_addr - base, 0x8);
        assert_eq!(s.vtable as usize, SHAPE_COLOR_TEMPERATURE_VTABLE_ADDR);
    }

    #[test]
    fn shape_saturation_layout_16b() {
        let s = ShapeSaturation::new_empty();
        let base = &s as *const _ as usize;
        let bag_addr = &s.bag as *const _ as usize;
        assert_eq!(std::mem::size_of::<ShapeSaturation>(), 16);
        assert_eq!(bag_addr - base, 0x8);
        assert_eq!(s.vtable as usize, SHAPE_SATURATION_VTABLE_ADDR);
    }

    #[test]
    fn shape_luminance_layout_16b() {
        let s = ShapeLuminance::new_empty();
        let base = &s as *const _ as usize;
        let bag_addr = &s.bag as *const _ as usize;
        assert_eq!(std::mem::size_of::<ShapeLuminance>(), 16);
        assert_eq!(bag_addr - base, 0x8);
        assert_eq!(s.vtable as usize, SHAPE_LUMINANCE_VTABLE_ADDR);
    }

    #[test]
    fn render_outputs_layouts_16b() {
        assert_eq!(std::mem::size_of::<RenderBiLevel>(), 16);
        assert_eq!(std::mem::size_of::<RenderColorTemperature>(), 16);
        assert_eq!(std::mem::size_of::<RenderSaturation>(), 16);
        assert_eq!(std::mem::size_of::<RenderLuminance>(), 16);
    }

    // ----- ToBiLevel — integration with PropertyBag::attach + PFloat

    #[test]
    fn to_bi_level_within_range_byte_eq() {
        let mut input = ShapeBiLevel::new_empty();
        unsafe {
            let key = PropertyKey::from_int(0x393);
            input
                .bag
                .attach(&key, PFloat::create_attach_ctrl(state::ENABLED_EXPLICIT, 0.5));
        }
        let out = to_bi_level(&input);
        assert_eq!(out.vtable as usize, RENDER_BILEVEL_VTABLE_ADDR);
        assert_eq!(out.threshold, 0.5);
    }

    #[test]
    fn to_bi_level_clamps_negative_to_zero() {
        let mut input = ShapeBiLevel::new_empty();
        unsafe {
            let key = PropertyKey::from_int(0x393);
            input
                .bag
                .attach(&key, PFloat::create_attach_ctrl(state::ENABLED_EXPLICIT, -0.3));
        }
        let out = to_bi_level(&input);
        assert_eq!(out.threshold, 0.0);
    }

    #[test]
    fn to_bi_level_clamps_above_one_to_one() {
        let mut input = ShapeBiLevel::new_empty();
        unsafe {
            let key = PropertyKey::from_int(0x393);
            input
                .bag
                .attach(&key, PFloat::create_attach_ctrl(state::ENABLED_EXPLICIT, 1.7));
        }
        let out = to_bi_level(&input);
        assert_eq!(out.threshold, 1.0);
    }

    #[test]
    fn to_bi_level_clamps_nan_to_zero() {
        let mut input = ShapeBiLevel::new_empty();
        unsafe {
            let key = PropertyKey::from_int(0x393);
            input.bag.attach(
                &key,
                PFloat::create_attach_ctrl(state::ENABLED_EXPLICIT, f32::NAN),
            );
        }
        let out = to_bi_level(&input);
        assert_eq!(out.threshold, 0.0);
    }

    // ----- ToColorTemperature — integration with PEnum (u32 value at +0xc)

    #[test]
    fn to_color_temperature_passes_through_u32_value() {
        let mut input = ShapeColorTemperature::new_empty();
        unsafe {
            let key = PropertyKey::from_int(0x3cd);
            // PEnum 의 value (+0xc) = u32, no clamping — raw 도 동일.
            input
                .bag
                .attach(&key, PEnum::create_attach_ctrl(state::ENABLED_EXPLICIT, 6500));
        }
        let out = to_color_temperature(&input);
        assert_eq!(out.vtable as usize, RENDER_COLOR_TEMPERATURE_VTABLE_ADDR);
        assert_eq!(out.temperature, 6500);
    }

    #[test]
    fn to_color_temperature_zero_value() {
        let mut input = ShapeColorTemperature::new_empty();
        unsafe {
            let key = PropertyKey::from_int(0x3cd);
            input
                .bag
                .attach(&key, PEnum::create_attach_ctrl(state::ENABLED_EXPLICIT, 0));
        }
        let out = to_color_temperature(&input);
        assert_eq!(out.temperature, 0);
    }

    // ----- ToSaturation — float value, no clamping (raw asm 무영향)

    #[test]
    fn to_saturation_passes_through_float_no_clamping() {
        let mut input = ShapeSaturation::new_empty();
        unsafe {
            let key = PropertyKey::from_int(0x3cc);
            input
                .bag
                .attach(&key, PFloat::create_attach_ctrl(state::ENABLED_EXPLICIT, 2.5));
        }
        let out = to_saturation(&input);
        assert_eq!(out.vtable as usize, RENDER_SATURATION_VTABLE_ADDR);
        // 2.5 — clamping 없음 (raw asm 에 fcmp 없음). 그대로 pass-through.
        assert_eq!(out.saturation, 2.5);
    }

    #[test]
    fn to_saturation_negative_value_passes_through() {
        let mut input = ShapeSaturation::new_empty();
        unsafe {
            let key = PropertyKey::from_int(0x3cc);
            input
                .bag
                .attach(&key, PFloat::create_attach_ctrl(state::ENABLED_EXPLICIT, -1.5));
        }
        let out = to_saturation(&input);
        // No clamping → -1.5 pass through
        assert_eq!(out.saturation, -1.5);
    }

    // ----- ToLuminance — 2 keys (brightness 0x399, contrast 0x39a), each clamped [-1, 1]

    #[test]
    fn to_luminance_within_range_pass_through() {
        let mut input = ShapeLuminance::new_empty();
        unsafe {
            input.bag.attach(
                &PropertyKey::from_int(0x399),
                PFloat::create_attach_ctrl(state::ENABLED_EXPLICIT, 0.3),
            );
            input.bag.attach(
                &PropertyKey::from_int(0x39a),
                PFloat::create_attach_ctrl(state::ENABLED_EXPLICIT, -0.7),
            );
        }
        let out = to_luminance(&input);
        assert_eq!(out.vtable as usize, RENDER_LUMINANCE_VTABLE_ADDR);
        assert_eq!(out.brightness, 0.3);
        assert_eq!(out.contrast, -0.7);
    }

    #[test]
    fn to_luminance_clamps_brightness_above_one_to_one() {
        let mut input = ShapeLuminance::new_empty();
        unsafe {
            input.bag.attach(
                &PropertyKey::from_int(0x399),
                PFloat::create_attach_ctrl(state::ENABLED_EXPLICIT, 2.5),
            );
            input.bag.attach(
                &PropertyKey::from_int(0x39a),
                PFloat::create_attach_ctrl(state::ENABLED_EXPLICIT, 0.0),
            );
        }
        let out = to_luminance(&input);
        assert_eq!(out.brightness, 1.0);
        assert_eq!(out.contrast, 0.0);
    }

    #[test]
    fn to_luminance_clamps_brightness_below_neg_one_to_neg_one() {
        let mut input = ShapeLuminance::new_empty();
        unsafe {
            input.bag.attach(
                &PropertyKey::from_int(0x399),
                PFloat::create_attach_ctrl(state::ENABLED_EXPLICIT, -3.0),
            );
            input.bag.attach(
                &PropertyKey::from_int(0x39a),
                PFloat::create_attach_ctrl(state::ENABLED_EXPLICIT, 0.0),
            );
        }
        let out = to_luminance(&input);
        assert_eq!(out.brightness, -1.0);
    }

    #[test]
    fn to_luminance_clamps_contrast_above_one_to_one() {
        let mut input = ShapeLuminance::new_empty();
        unsafe {
            input.bag.attach(
                &PropertyKey::from_int(0x399),
                PFloat::create_attach_ctrl(state::ENABLED_EXPLICIT, 0.0),
            );
            input.bag.attach(
                &PropertyKey::from_int(0x39a),
                PFloat::create_attach_ctrl(state::ENABLED_EXPLICIT, 5.0),
            );
        }
        let out = to_luminance(&input);
        assert_eq!(out.contrast, 1.0);
    }

    #[test]
    fn to_luminance_clamps_nan_to_neg_one() {
        // raw 의 `b.mi` (negative or NaN) → -1
        let mut input = ShapeLuminance::new_empty();
        unsafe {
            input.bag.attach(
                &PropertyKey::from_int(0x399),
                PFloat::create_attach_ctrl(state::ENABLED_EXPLICIT, f32::NAN),
            );
            input.bag.attach(
                &PropertyKey::from_int(0x39a),
                PFloat::create_attach_ctrl(state::ENABLED_EXPLICIT, f32::NAN),
            );
        }
        let out = to_luminance(&input);
        assert_eq!(out.brightness, -1.0);
        assert_eq!(out.contrast, -1.0);
    }

    #[test]
    #[should_panic(expected = "GetValue")]
    fn to_bi_level_panics_on_missing_key() {
        // Empty bag → GetValue → out_of_range panic (raw ___cxa_throw 대응)
        let input = ShapeBiLevel::new_empty();
        let _ = to_bi_level(&input);
    }

    #[test]
    fn extract_bag_impl_returns_null_for_null_ctrl() {
        let mut bag = crate::property_bag::PropertyBag::new(false);
        // Force ctrl to null (defensive test) — manual hack
        let saved = bag.ctrl;
        bag.ctrl = std::ptr::null_mut();
        unsafe {
            let r = extract_bag_impl(&bag);
            assert!(r.is_null());
        }
        // Restore so Drop doesn't panic
        bag.ctrl = saved;
    }
}

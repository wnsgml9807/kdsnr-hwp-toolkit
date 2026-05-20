//! `Hnc::Shape::Color` — 24B 1:1 byte-equivalent port.
//!
//! libHncDrawingEngine_arm64 의 모든 Color ctor + accessor + dtor 1:1.
//!
//! # raw 24B layout (확정 from `Color::Swap` @ `0x14c8b0` + 모든 ctor)
//!
//! ```text
//! offset  field          type           의미
//! 0x00    value_first8   u8 [0..8]      value union 첫 8B (Rgb: r,g,b at +0..+2; Cmyk: 4B; ScRgb/Hsl: 2 f32; Scheme/System/Preset: u32 at +0..+4)
//! 0x08    value_last4    u8 [0..4]      value union 추가 4B (ScRgb/Hsl: 3rd f32; else unused)
//! 0x0c    type_tag       u32            0=Rgb, 1=Cmyk, 2=Scheme, 3=System, 4=Preset, 5=ScRgb, 6=Hsl
//! 0x10    color_effect   *mut ColorEffect (8B owned ptr; null 가능)
//! ```
//!
//! 총 24B / 8B align (raw `mov w0, #0x18` for `new(0x18)` in Color::Clone +
//! many other locations).
//!
//! # 본 R-1.5.4 단계 scope
//!
//! ColorScheme byte-equivalent 에 필요한 모든 동작 1:1:
//! - 8 ctor: `Color(SystemStyle)` `Color(SchemeStyle, auto_ptr)` `Color(PresetStyle)`
//!   `Color(u8,u8,u8, auto_ptr)` `Color(Rgb&)` `Color(Cmyk&)` `Color(ScRgb&)` `Color(Hsl&)`
//! - copy ctor (`Color(Color const&)`) — implicit, RE 도출 (raw `0x662eac-0x662ec0`
//!   + `Color::Clone` @ `0xb247c` 의 패턴)
//! - dtor (`~Color`) — raw `0x14c870`
//! - `Swap(Color&)` — raw `0x14c8b0`
//! - `Clone()` — raw `0xb247c`
//! - accessors: GetType / GetSchemeStyle / GetSystemStyle / GetPresetStyle /
//!   GetRgb / GetScRgb / GetCmyk / GetHsl / GetColorEffect
//!
//! # 의도적 미포함 (별도 세션)
//!
//! 본 byte-equivalent 가 ColorScheme init 에 도달 안 함:
//! - `Color(SchemeStyle, float)` `Color(u8,u8,u8, float)` — alpha factor ctor
//!   가 `ColorEffect::Add(PKey, float)` 호출 (raw `0xbed4c`), 본 module 의
//!   `ColorEffect::Add` 가 jump table 28 PKey variant port 안 됨.
//! - `operator==` / `operator!=` / `operator<` — `DrawingType::Cmyk::operator<`
//!   (`0x26c34`) + `ScRgb::operator<` (`0x29b68`) + `Hsl::operator<` (`0x2986c`)
//!   + `ColorEffect::operator==` (`0x14cab4`, jump-table) 모두 종속.
//! - `SetAlpha(float)` `ResetAlpha` `SetColorEffect(auto_ptr)` `UpdatePlaceholder`
//!   `UpdateScheme` — `ColorEffect::Add` 의존.
//! - `Color(u8,u8,u8,auto_ptr)` 의 `auto_ptr<ColorEffect>` 인자: Rust 에는
//!   move semantic 으로 `*mut ColorEffect` 가 가장 근접. raw `ldr x8, [x4];
//!   str xzr, [x4]` 의 transfer pattern 1:1 port.

use crate::color_effect::ColorEffect;
use crate::drawing_type::{Cmyk, Hsl, Rgb, ScRgb};
use crate::scheme_style::SchemeStyle;
use std::ptr;

/// raw `Color::GetType()` 반환값 (u32 enum).
pub mod color_type {
    pub const RGB: u32 = 0;
    pub const CMYK: u32 = 1;
    pub const SCHEME: u32 = 2;
    pub const SYSTEM: u32 = 3;
    pub const PRESET: u32 = 4;
    pub const SC_RGB: u32 = 5;
    pub const HSL: u32 = 6;
}

/// `Hnc::Shape::Color::SystemStyle` — u32 enum (raw `Color::Color(SystemStyle)`
/// 의 `str w1, [x0]` 이 u32 store). 정확한 variant 값은 OS Windows COLOR_*
/// 매핑 (Hancom 의 추정 시스템 컬러). 본 단계는 raw u32 transparent wrapper.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SystemStyle(pub u32);

impl SystemStyle {
    /// raw `mov w8, #0x5` (ColorScheme[1] SystemStyle 값) = 5 — Hancom theme 의
    /// "WindowText" 또는 유사 system 컬러.
    pub const WINDOW_TEXT: SystemStyle = SystemStyle(5);
    /// raw `mov w8, #0x8` (ColorScheme[0] SystemStyle 값) = 8.
    pub const WINDOW: SystemStyle = SystemStyle(8);
}

/// `Hnc::Shape::Color::PresetStyle` — u32 enum.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PresetStyle(pub u32);

/// raw 24B `Hnc::Shape::Color`.
///
/// `#[repr(C)]` + 명시적 union 으로 raw layout 보존:
/// - `[0x00..0x0c]` = 12-byte value union
/// - `[0x0c..0x10]` = type tag
/// - `[0x10..0x18]` = ColorEffect ptr
#[repr(C)]
#[derive(Debug)]
pub struct Color {
    /// raw [0x00..0x0c]: 12-byte value union. 해석은 `type_tag` 에 따름.
    ///
    /// raw 의 ctor 가 union 의 일부만 초기화 (예: Rgb ctor 는 [0..3] 만, type
    /// 별 값) — 나머지 영역은 uninit. byte-equivalent 보장 위해 `[u8; 12]`.
    pub value: [u8; 12],
    /// raw [0x0c..0x10]: u32 type tag (`color_type::*`).
    pub type_tag: u32,
    /// raw [0x10..0x18]: ColorEffect* (owned, null 가능).
    pub color_effect: *mut ColorEffect,
}

pub const COLOR_SIZE_BYTES: usize = 24;
pub const COLOR_ALIGN_BYTES: usize = 8;

const _: () = assert!(std::mem::size_of::<Color>() == COLOR_SIZE_BYTES);
const _: () = assert!(std::mem::align_of::<Color>() == COLOR_ALIGN_BYTES);

impl Color {
    /// raw `Color::Color(SystemStyle)` (`0x14c520` C2 / `0x14c534` C1).
    ///
    /// ```asm
    /// 14c520: str  w1, [x0]            ; value[0..4] = SystemStyle
    /// 14c524: mov  w8, #0x3
    /// 14c528: str  w8, [x0, #0xc]      ; type_tag = 3 (System)
    /// 14c52c: str  xzr, [x0, #0x10]    ; color_effect = null
    /// ```
    pub fn from_system_style(s: SystemStyle) -> Self {
        let mut value = [0u8; 12];
        // raw `str w1, [x0]` — value[0..4] = SystemStyle u32 (little-endian).
        value[0..4].copy_from_slice(&s.0.to_le_bytes());
        // [4..12] is uninit in raw, but we zero-init for Rust safety. Type tag
        // 가 3 이므로 [4..12] 영역은 dereference 안 됨 — byte-eq 영향 없음.
        Color {
            value,
            type_tag: color_type::SYSTEM,
            color_effect: ptr::null_mut(),
        }
    }

    /// raw `Color::Color(SchemeStyle, auto_ptr<ColorEffect>)` (`0x14c548`).
    ///
    /// ```asm
    /// 14c548: str  w1, [x0]              ; value[0..4] = SchemeStyle
    /// 14c54c: mov  w8, #0x2
    /// 14c550: str  w8, [x0, #0xc]        ; type_tag = 2 (Scheme)
    /// 14c554: ldr  x8, [x2]              ; auto_ptr 의 inner ptr load
    /// 14c558: str  xzr, [x2]             ; auto_ptr.inner = null (transfer)
    /// 14c55c: str  x8, [x0, #0x10]       ; color_effect = inner
    /// ```
    ///
    /// `effect_in_out` 은 raw `auto_ptr<ColorEffect>&` 와 동등 — 호출 후
    /// caller 의 ptr 는 null 로 transferred. 본 Rust 시그니처는 raw 의
    /// auto_ptr release 패턴을 명시화.
    ///
    /// # Safety
    /// `effect_in_out` 가 valid `*mut *mut ColorEffect` (& mut option pattern)
    /// 이어야 함. 호출 후 `*effect_in_out = null`.
    pub unsafe fn from_scheme_style_auto(
        s: SchemeStyle,
        effect_in_out: *mut *mut ColorEffect,
    ) -> Self {
        let inner = *effect_in_out;
        *effect_in_out = ptr::null_mut();
        let mut value = [0u8; 12];
        value[0..4].copy_from_slice(&s.as_u32().to_le_bytes());
        Color {
            value,
            type_tag: color_type::SCHEME,
            color_effect: inner,
        }
    }

    /// Pure-Rust helper: SchemeStyle + 명시적 ColorEffect ownership transfer.
    pub fn from_scheme_style(s: SchemeStyle, effect: *mut ColorEffect) -> Self {
        let mut value = [0u8; 12];
        value[0..4].copy_from_slice(&s.as_u32().to_le_bytes());
        Color {
            value,
            type_tag: color_type::SCHEME,
            color_effect: effect,
        }
    }

    /// raw 의 scheme-style Color 를 임의 u32 raw value 로 구성.
    ///
    /// `SchemeStyle` enum 은 OOXML schemeClr 의 0..11 만 변형으로 정의 (Background1
    /// ... FollowedHyperlink). 그러나 raw `FormatScheme::CreateDefault` (`0x16f6c0`)
    /// 는 `mov w8, #0x10` (= 16, OOXML `phClr` placeholder) 같이 12+ raw 값도
    /// 사용. 본 helper 는 enum 제약을 우회하여 raw u32 값을 그대로 value[0..4]
    /// 에 저장.
    ///
    /// ```asm
    /// 16f6c0: mov  w8, #0x10               ; raw scheme value (16 = phClr)
    /// 16f6c4: mov  w9, #0x2
    /// 16f6c8: stur w8, [x29, #-0xa0]       ; value[0..4] = 0x10
    /// 16f6cc: stur w9, [x29, #-0x94]       ; type_tag = 2 (Scheme)
    /// 16f6d4: stur xzr, [x29, #-0x90]      ; color_effect = null
    /// ```
    ///
    /// `from_scheme_style(SchemeStyle::Background1, null)` 와 동일하지만 raw u32
    /// 가 0..11 범위를 벗어나도 동작.
    pub fn from_scheme_raw_u32(raw_scheme: u32, effect: *mut ColorEffect) -> Self {
        let mut value = [0u8; 12];
        value[0..4].copy_from_slice(&raw_scheme.to_le_bytes());
        Color {
            value,
            type_tag: color_type::SCHEME,
            color_effect: effect,
        }
    }

    /// raw `Color::Color(PresetStyle)` (`0x14c67c`).
    ///
    /// ```asm
    /// 14c67c: str  w1, [x0]
    /// 14c680: mov  w8, #0x4
    /// 14c684: str  w8, [x0, #0xc]
    /// 14c688: str  xzr, [x0, #0x10]
    /// ```
    pub fn from_preset_style(p: PresetStyle) -> Self {
        let mut value = [0u8; 12];
        value[0..4].copy_from_slice(&p.0.to_le_bytes());
        Color {
            value,
            type_tag: color_type::PRESET,
            color_effect: ptr::null_mut(),
        }
    }

    /// raw `Color::Color(u8 r, u8 g, u8 b, auto_ptr<ColorEffect>)` (`0x14c690`).
    ///
    /// ```asm
    /// 14c690: str  wzr, [x0, #0xc]       ; type_tag = 0 (Rgb)
    /// 14c694: ldr  x8, [x4]              ; auto_ptr.inner
    /// 14c698: str  xzr, [x4]             ; auto_ptr.inner = null
    /// 14c69c: str  x8, [x0, #0x10]       ; color_effect = inner
    /// 14c6a0: strb w1, [x0]              ; value[0] = r
    /// 14c6a4: strb w2, [x0, #0x1]        ; value[1] = g
    /// 14c6a8: strb w3, [x0, #0x2]        ; value[2] = b
    /// ```
    ///
    /// # Safety
    /// `effect_in_out` 동작은 `from_scheme_style_auto` 와 동일.
    pub unsafe fn from_rgb_auto(
        r: u8,
        g: u8,
        b: u8,
        effect_in_out: *mut *mut ColorEffect,
    ) -> Self {
        let inner = *effect_in_out;
        *effect_in_out = ptr::null_mut();
        let mut value = [0u8; 12];
        value[0] = r;
        value[1] = g;
        value[2] = b;
        Color {
            value,
            type_tag: color_type::RGB,
            color_effect: inner,
        }
    }

    /// Pure-Rust helper: r/g/b + 명시적 ColorEffect transfer.
    pub fn from_rgb(r: u8, g: u8, b: u8, effect: *mut ColorEffect) -> Self {
        let mut value = [0u8; 12];
        value[0] = r;
        value[1] = g;
        value[2] = b;
        Color {
            value,
            type_tag: color_type::RGB,
            color_effect: effect,
        }
    }

    /// raw `Color::Color(Rgb const&)` (`0x14c7b4`).
    pub fn from_rgb_struct(src: &Rgb) -> Self {
        Self::from_rgb(src.r, src.g, src.b, ptr::null_mut())
    }

    /// raw `Color::Color(Cmyk const&)` (`0x14c7d0`).
    ///
    /// ```asm
    /// 14c7d0: ldr  w8, [x1]              ; w8 = 4B from Cmyk
    /// 14c7d4: str  w8, [x0]              ; value[0..4] = 4B
    /// 14c7d8: mov  w8, #0x1
    /// 14c7dc: str  w8, [x0, #0xc]        ; type_tag = 1 (Cmyk)
    /// 14c7e0: str  xzr, [x0, #0x10]      ; effect = null
    /// ```
    pub fn from_cmyk(src: &Cmyk) -> Self {
        let mut value = [0u8; 12];
        value[0] = src.c;
        value[1] = src.m;
        value[2] = src.y;
        value[3] = src.k;
        Color {
            value,
            type_tag: color_type::CMYK,
            color_effect: ptr::null_mut(),
        }
    }

    /// raw `Color::Color(ScRgb const&)` (`0x14c800`).
    ///
    /// ```asm
    /// 14c800: ldr  x8, [x1]              ; 8B (r, g packed)
    /// 14c804: ldr  w9, [x1, #0x8]        ; 4B (b)
    /// 14c808: str  x8, [x0]              ; value[0..8] = r, g
    /// 14c80c: mov  w8, #0x5
    /// 14c810: stp  w9, w8, [x0, #0x8]    ; value[8..12] = b, type_tag = 5
    /// 14c814: str  xzr, [x0, #0x10]
    /// ```
    pub fn from_scrgb(src: &ScRgb) -> Self {
        let mut value = [0u8; 12];
        value[0..4].copy_from_slice(&src.r.to_le_bytes());
        value[4..8].copy_from_slice(&src.g.to_le_bytes());
        value[8..12].copy_from_slice(&src.b.to_le_bytes());
        Color {
            value,
            type_tag: color_type::SC_RGB,
            color_effect: ptr::null_mut(),
        }
    }

    /// raw `Color::Color(Hsl const&)` (`0x14c838`). ScRgb 와 동일 패턴, type=6.
    pub fn from_hsl(src: &Hsl) -> Self {
        let mut value = [0u8; 12];
        value[0..4].copy_from_slice(&src.h.to_le_bytes());
        value[4..8].copy_from_slice(&src.s.to_le_bytes());
        value[8..12].copy_from_slice(&src.l.to_le_bytes());
        Color {
            value,
            type_tag: color_type::HSL,
            color_effect: ptr::null_mut(),
        }
    }

    /// raw copy ctor (implicit, RE 도출). `Color::Clone` (`0xb247c`) 의 본체:
    ///
    /// ```asm
    /// b247c: ... alloc new 24B (mov w0, #0x18; bl operator_new) — Clone 전용
    /// b24a0: ldr  q0, [x21]              ; q0 = src 의 first 16B
    /// b24a4: str  q0, [x0]               ; new 의 first 16B = q0 (value + type_tag)
    /// b24a8: ldr  x0, [x21, #0x10]       ; x0 = src.color_effect
    /// b24ac: bl   0x65411c                ; cloned = clone(src.color_effect)
    /// b24b0: str  x0, [x20, #0x10]       ; new.color_effect = cloned
    /// ```
    ///
    /// 즉 copy ctor = `memcpy first 16B; effect = ColorEffect::clone_raw(src.effect)`.
    /// 본 메소드는 in-place copy (heap alloc 은 caller).
    ///
    /// # Safety
    /// `src.color_effect` 가 valid 또는 null.
    pub unsafe fn copy_ctor(src: &Color) -> Self {
        // raw `ldr q0, [x21]; str q0, [x0]` — 16B memcpy of (value + type_tag)
        let value = src.value;
        let type_tag = src.type_tag;
        // raw `ldr x0, [x21, #0x10]; bl 0x65411c` — ColorEffect clone
        let cloned_effect = ColorEffect::clone_raw(src.color_effect);
        Color {
            value,
            type_tag,
            color_effect: cloned_effect,
        }
    }

    /// raw `Color::Clone() const` (`0xb247c`) — `new Color(*this)` 와 동등.
    /// heap-allocated 새 Color* 반환.
    ///
    /// # Safety
    /// 반환 ptr 은 `Color::raw_delete` 로 해제 필요.
    pub unsafe fn clone_to_heap(&self) -> *mut Color {
        let layout = std::alloc::Layout::new::<Color>();
        let p = std::alloc::alloc(layout) as *mut Color;
        if p.is_null() {
            std::alloc::handle_alloc_error(layout);
        }
        ptr::write(p, Self::copy_ctor(self));
        p
    }

    /// `Color::raw_delete` — heap-alloc 된 Color* 에 대한 dtor + dealloc.
    ///
    /// # Safety
    /// `p` 는 `clone_to_heap` 또는 동등 alloc 으로 얻은 ptr.
    pub unsafe fn raw_delete(p: *mut Color) {
        if p.is_null() {
            return;
        }
        ptr::drop_in_place(p);
        std::alloc::dealloc(p as *mut u8, std::alloc::Layout::new::<Color>());
    }

    /// raw `Color::Swap(Color& rhs)` (`0x14c8b0`) — 세 field 모두 swap.
    ///
    /// ```asm
    /// 14c8b4: ldr w8, [x0, #0xc]; ldr w9, [x1, #0xc]
    /// 14c8bc: str w9, [x0, #0xc]; str w8, [x1, #0xc]   ; swap type_tag (4B)
    /// 14c8c4: ldr w8, [x0, #0x8]; ldr x9, [x0]
    /// 14c8cc: ldr w10, [x1, #0x8]; ldr x11, [x1]
    /// 14c8d4: str x11, [x0]; str w10, [x0, #0x8]
    /// 14c8dc: str x9, [x1]; str w8, [x1, #0x8]         ; swap value (8B + 4B)
    /// 14c8e4: ldr x8, [x0, #0x10]; ldr x9, [x1, #0x10]
    /// 14c8ec: str x9, [x0, #0x10]; str x8, [x1, #0x10] ; swap color_effect (8B)
    /// ```
    pub fn swap(&mut self, rhs: &mut Color) {
        std::mem::swap(&mut self.type_tag, &mut rhs.type_tag);
        std::mem::swap(&mut self.value, &mut rhs.value);
        std::mem::swap(&mut self.color_effect, &mut rhs.color_effect);
    }

    /// raw `Color::GetType() const` (`0x14ce88`): `ldr w0, [x0, #0xc]; ret`.
    #[inline]
    pub fn get_type(&self) -> u32 {
        self.type_tag
    }

    /// raw `Color::GetSchemeStyle() const` (`0x14ce90`): `ldr w0, [x0]; ret`.
    /// type 검사 없음 (raw 는 caller 책임).
    #[inline]
    pub fn get_scheme_style_raw_u32(&self) -> u32 {
        u32::from_le_bytes([self.value[0], self.value[1], self.value[2], self.value[3]])
    }

    /// raw `Color::GetSystemStyle() const` (`0x14ce98`): `ldr w0, [x0]; ret`.
    #[inline]
    pub fn get_system_style(&self) -> SystemStyle {
        SystemStyle(self.get_scheme_style_raw_u32())
    }

    /// raw `Color::GetPresetStyle() const` (`0x14cea0`): `ldr w0, [x0]; ret`.
    #[inline]
    pub fn get_preset_style(&self) -> PresetStyle {
        PresetStyle(self.get_scheme_style_raw_u32())
    }

    /// raw `Color::GetRgb() const` (`0x14cea8`): `ret` (= `*(Rgb const*)(this)`).
    /// type 검사 없음.
    #[inline]
    pub fn get_rgb(&self) -> Rgb {
        Rgb {
            r: self.value[0],
            g: self.value[1],
            b: self.value[2],
        }
    }

    /// raw `Color::GetScRgb() const` (`0x14ceac`): `ret`.
    #[inline]
    pub fn get_scrgb(&self) -> ScRgb {
        let r = f32::from_le_bytes([
            self.value[0],
            self.value[1],
            self.value[2],
            self.value[3],
        ]);
        let g = f32::from_le_bytes([
            self.value[4],
            self.value[5],
            self.value[6],
            self.value[7],
        ]);
        let b = f32::from_le_bytes([
            self.value[8],
            self.value[9],
            self.value[10],
            self.value[11],
        ]);
        ScRgb { r, g, b }
    }

    /// raw `Color::GetCmyk() const` (`0x14ceb0`): `ret`.
    #[inline]
    pub fn get_cmyk(&self) -> Cmyk {
        Cmyk {
            c: self.value[0],
            m: self.value[1],
            y: self.value[2],
            k: self.value[3],
        }
    }

    /// raw `Color::GetHsl() const` (`0x14ceb4`): `ret`.
    #[inline]
    pub fn get_hsl(&self) -> Hsl {
        let scrgb = self.get_scrgb();
        Hsl {
            h: scrgb.r,
            s: scrgb.g,
            l: scrgb.b,
        }
    }

    /// raw `Color::GetColorEffect() const` (`0xbec30`): `ldr x0, [x0, #0x10]; ret`.
    #[inline]
    pub fn get_color_effect(&self) -> *mut ColorEffect {
        self.color_effect
    }

    /// Returns SchemeStyle if `type_tag == 2`, else None. (Rust convenience)
    pub fn as_scheme_style(&self) -> Option<SchemeStyle> {
        if self.type_tag == color_type::SCHEME {
            SchemeStyle::from_u32(self.get_scheme_style_raw_u32())
        } else {
            None
        }
    }

    /// `Hnc::Shape::Color::SetAlpha(float)` (raw @ `0xb2534..0xb26d0`) 1:1.
    ///
    /// 알고리즘:
    /// 1. `cloned = ColorEffect::clone_raw(self.color_effect)` (null 이면 new empty 24B).
    /// 2. cloned 의 entries 를 scan 하여 PKey ∈ {500, 501, 502} 인 모든 entry 제거.
    /// 3. `cloned.add(500, alpha)` — clamp [0, 1] 적용 후 push.
    /// 4. `self.color_effect = cloned`; 기존 (있다면) raw_delete.
    ///
    /// raw 의 scan 은 `sub w12, w12, #0x1f4; cmp w12, #0x2; b.hi skip` — PKey-500 ≤ 2
    /// 인 entry 만 제거 (= 500, 501, 502).
    ///
    /// # Safety
    /// `self.color_effect` 가 valid (또는 null). 호출 후 새 ColorEffect 가 attach.
    pub unsafe fn set_alpha(&mut self, alpha: f32) {
        // raw `b2550-b2554`: clone existing
        let cloned: *mut ColorEffect = ColorEffect::clone_raw(self.color_effect);
        let target: *mut ColorEffect = if cloned.is_null() {
            // raw `b2570-b2588`: alloc new 24B empty
            ColorEffect::create()
        } else {
            cloned
        };

        // raw `b2594-b2678`: scan & remove PKey ∈ {500, 501, 502}
        remove_alpha_entries(target);

        // raw `b267c-b2684`: ColorEffect::Add(target, 500, alpha)
        (*target).add(500, alpha);

        // raw `b2688-b26bc`: swap self.color_effect with target, free old
        let old = self.color_effect;
        self.color_effect = target;
        ColorEffect::raw_delete(old);
    }

    /// `Hnc::Shape::Color::ResetAlpha()` (raw @ `0x14d188..0x14d284`) 1:1.
    ///
    /// self.color_effect 가 null 이면 no-op.
    /// 그렇지 않으면 in-place 로 entries 중 PKey ∈ {500, 501, 502} 인 것을 모두 제거.
    /// SetAlpha 와 달리 clone 하지 않고 `Add` 도 하지 않음 (= 단순 알파 키 제거).
    ///
    /// # Safety
    /// `self.color_effect` 가 valid 또는 null.
    pub unsafe fn reset_alpha(&mut self) {
        // raw `14d188-14d198`: null 또는 empty 면 no-op
        if self.color_effect.is_null() {
            return;
        }
        remove_alpha_entries(self.color_effect);
    }

    /// `Hnc::Shape::Color::SetColorEffect(auto_ptr<ColorEffect>)`
    /// (raw @ `0xc09a0..0xc09e8`) 1:1.
    ///
    /// auto_ptr "steal" semantic — `new_ptr` 의 ptr 을 self 로 이전, caller 의
    /// 위치는 null 로 만듦. self 의 기존 ColorEffect 는 free.
    ///
    /// 본 helper 는 `new_ptr` (auto_ptr 본체) 를 `&mut *mut ColorEffect` 로 받음
    /// — `*new_ptr` 가 stolen ptr, 호출 후 `*new_ptr = null`.
    ///
    /// # Safety
    /// `*new_ptr` 가 valid `ColorEffect*` 또는 null. 호출 후 self 가 ownership.
    pub unsafe fn set_color_effect(&mut self, new_ptr: &mut *mut ColorEffect) {
        // raw `c09ac-c09b0: ldr x8, [x1]; str xzr, [x1]` — auto_ptr.steal()
        let new_value = *new_ptr;
        *new_ptr = std::ptr::null_mut();

        // raw `c09b4-c09b8: x19 = [x0+0x10]; str x8, [x0, #0x10]`
        let old = self.color_effect;
        self.color_effect = new_value;

        // raw `c09bc..c09dc: 기존 free`
        ColorEffect::raw_delete(old);
    }

    /// `Color::GetPresetColor(PresetStyle) -> u32` — raw `0x14dacc` (88B).
    ///
    /// 3 개의 byte table (R/G/B, 각 190 entry) 에서 preset index 별 RGB 추출.
    /// table 위치 (libHncDrawingEngine.dylib `__TEXT.__const` 영역):
    /// - R: `0x74fba8` (190B)
    /// - G: `0x74fc66` (190B)
    /// - B: `0x74fd24` (190B)
    ///
    /// 반환 u32 layout: `(B << 16) | (G << 8) | R`. 0..189 범위 밖이면 0 반환.
    ///
    /// ```asm
    /// 14dacc: cmp w0, #0xbd
    /// 14dad0: b.hi 0x14db0c       ; > 189 → return 0
    /// 14dad4: sxtw x10, w0
    /// 14dad8-e0: w8 = R_table[idx]
    /// 14dae4-ec: w9 = G_table[idx]
    /// 14daf0-f8: w10 = B_table[idx]
    /// 14dafc: w9 <<= 8
    /// 14db00: w9 |= w10 << 16
    /// 14db04: w0 = w9 | w8     ; (B<<16) | (G<<8) | R
    /// 14db08: ret
    /// ```
    ///
    /// Caller (ShapeRenderConverter::ToRenderColor PRESET case):
    /// - `strh w0, [x19]` → result[0..2] = w0 low 16 = (G<<8) | R → R at [0], G at [1]
    /// - `strb (w0>>16), [x19, #2]` → result[2] = B
    ///
    /// 결과적으로 DrawingType::Rgb (r, g, b at +0, +1, +2) layout 과 일치.
    pub fn get_preset_color(style: PresetStyle) -> u32 {
        let idx = style.0;
        if idx > 0xbd {
            // raw 14db0c-24: 모든 w8/w9/w10 를 0 으로 → 결과 = 0
            return 0;
        }
        let i = idx as usize;
        let r = PRESET_R_TABLE[i] as u32;
        let g = PRESET_G_TABLE[i] as u32;
        let b = PRESET_B_TABLE[i] as u32;
        // raw 의 lsl + orr sequence: w0 = (b << 16) | (g << 8) | r
        (b << 16) | (g << 8) | r
    }
}

/// PresetColor R-channel table (190 bytes).
/// 원본 위치: `libHncDrawingEngine.dylib` `__TEXT.__const` @ `0x74fba8`.
/// `Color::GetPresetColor` (raw `0x14dacc`) 의 첫 byte table.
#[rustfmt::skip]
pub static PRESET_R_TABLE: [u8; 190] = [
    0xf0, 0xfa, 0x00, 0x7f, 0xf0, 0xf5, 0xff, 0x00, 0xff, 0x00, 0x8a, 0xa5, 0xde, 0x5f, 0x7f, 0xd2,
    0xff, 0x64, 0xff, 0xdc, 0x00, 0x00, 0x00, 0xb8, 0xa9, 0xa9, 0x00, 0xbd, 0x8b, 0x55, 0xff, 0x99,
    0x8b, 0xe9, 0x8f, 0x48, 0x2f, 0x2f, 0x00, 0x94, 0x00, 0x00, 0xb8, 0xa9, 0xa9, 0x00, 0xbd, 0x8b,
    0x55, 0xff, 0x99, 0x8b, 0xe9, 0x8f, 0x48, 0x2f, 0x2f, 0x00, 0x94, 0xff, 0x00, 0x69, 0x69, 0x1e,
    0xb2, 0xff, 0x22, 0xff, 0xdc, 0xf8, 0xff, 0xda, 0x80, 0x80, 0x00, 0xad, 0xf0, 0xff, 0xcd, 0x4b,
    0xff, 0xf0, 0xe6, 0xff, 0x7c, 0xff, 0xad, 0xf0, 0xe0, 0xfa, 0xd3, 0xd3, 0x90, 0xff, 0xff, 0x20,
    0x87, 0x77, 0x77, 0xb0, 0xff, 0xad, 0xf0, 0xe0, 0xfa, 0xd3, 0xd3, 0x90, 0xff, 0xff, 0x20, 0x87,
    0x77, 0x77, 0xb0, 0xff, 0x00, 0x32, 0xfa, 0xff, 0x80, 0x66, 0x00, 0xba, 0x93, 0x3c, 0x7b, 0x00,
    0x48, 0xc7, 0x66, 0x00, 0xba, 0x93, 0x3c, 0x7b, 0x00, 0x48, 0xc7, 0x19, 0xf5, 0xff, 0xff, 0xff,
    0x00, 0xfd, 0x80, 0x6b, 0xff, 0xff, 0xda, 0xee, 0x98, 0xaf, 0xdb, 0xff, 0xff, 0xcd, 0xff, 0xdd,
    0xb0, 0x80, 0xff, 0xbc, 0x41, 0x8b, 0xfa, 0xf4, 0x2e, 0xff, 0xa0, 0xc0, 0x87, 0x6a, 0x70, 0x70,
    0xff, 0x00, 0x46, 0xd2, 0x00, 0xd8, 0xff, 0x40, 0xee, 0xf5, 0xff, 0xf5, 0xff, 0x9a,
];

/// PresetColor G-channel table (190 bytes). 원본: `0x74fc66`.
#[rustfmt::skip]
pub static PRESET_G_TABLE: [u8; 190] = [
    0xf8, 0xeb, 0xff, 0xff, 0xff, 0xf5, 0xe4, 0x00, 0xeb, 0x00, 0x2b, 0x2a, 0xb8, 0x9e, 0xff, 0x69,
    0x7f, 0x95, 0xf8, 0x14, 0xff, 0x00, 0x8b, 0x86, 0xa9, 0xa9, 0x64, 0xb7, 0x00, 0x6b, 0x8c, 0x32,
    0x00, 0x96, 0xbc, 0x3d, 0x4f, 0x4f, 0xce, 0x00, 0x00, 0x8b, 0x86, 0xa9, 0xa9, 0x64, 0xb7, 0x00,
    0x6b, 0x8c, 0x32, 0x00, 0x96, 0xbc, 0x3d, 0x4f, 0x4f, 0xce, 0x00, 0x14, 0xbf, 0x69, 0x69, 0x90,
    0x22, 0xfa, 0x8b, 0x00, 0xdc, 0xf8, 0xd7, 0xa5, 0x80, 0x80, 0x80, 0xff, 0xff, 0x69, 0x5c, 0x00,
    0xff, 0xe6, 0xe6, 0xf0, 0xfc, 0xfa, 0xd8, 0x80, 0xff, 0xfa, 0xd3, 0xd3, 0xee, 0xb6, 0xa0, 0xb2,
    0xce, 0x88, 0x88, 0xc4, 0xff, 0xd8, 0x80, 0xff, 0xfa, 0xd3, 0xd3, 0xee, 0xb6, 0xa0, 0xb2, 0xce,
    0x88, 0x88, 0xc4, 0xff, 0xff, 0xcd, 0xf0, 0x00, 0x00, 0xcd, 0x00, 0x55, 0x70, 0xb3, 0x68, 0xfa,
    0xd1, 0x15, 0xcd, 0x00, 0x55, 0x70, 0xb3, 0x68, 0xfa, 0xd1, 0x15, 0x19, 0xff, 0xe4, 0xe4, 0xde,
    0x00, 0xf5, 0x80, 0x8e, 0xa5, 0x45, 0x70, 0xe8, 0xfb, 0xee, 0x70, 0xef, 0xda, 0x85, 0xc0, 0xa0,
    0xe0, 0x00, 0x00, 0x8f, 0x69, 0x45, 0x80, 0xa4, 0x8b, 0xf5, 0x52, 0xc0, 0xce, 0x5a, 0x80, 0x80,
    0xfa, 0xff, 0x82, 0xb4, 0x80, 0xbf, 0x63, 0xe0, 0x82, 0xde, 0xff, 0xf5, 0xff, 0xcd,
];

/// PresetColor B-channel table (190 bytes). 원본: `0x74fd24`.
#[rustfmt::skip]
pub static PRESET_B_TABLE: [u8; 190] = [
    0xff, 0xd7, 0xff, 0xd4, 0xff, 0xdc, 0xc4, 0x00, 0xcd, 0xff, 0xe2, 0x2a, 0x87, 0xa0, 0x00, 0x1e,
    0x50, 0xed, 0xdc, 0x3c, 0xff, 0x8b, 0x8b, 0x0b, 0xa9, 0xa9, 0x00, 0x6b, 0x8b, 0x2f, 0x00, 0xcc,
    0x00, 0x7a, 0x8f, 0x8b, 0x4f, 0x4f, 0xd1, 0xd3, 0x8b, 0x8b, 0x0b, 0xa9, 0xa9, 0x00, 0x6b, 0x8b,
    0x2f, 0x00, 0xcc, 0x00, 0x7a, 0x8b, 0x8b, 0x4f, 0x4f, 0xd1, 0xd3, 0x93, 0xff, 0x69, 0x69, 0xff,
    0x22, 0xf0, 0x22, 0xff, 0xdc, 0xff, 0x00, 0x20, 0x80, 0x80, 0x00, 0x2f, 0xf0, 0xb4, 0x5c, 0x82,
    0xf0, 0x8c, 0xfa, 0xf5, 0x00, 0xcd, 0xe6, 0x80, 0xff, 0xd2, 0xd3, 0xd3, 0x90, 0xc1, 0x7a, 0xaa,
    0xfa, 0x99, 0x99, 0xde, 0xe0, 0xe6, 0x80, 0xff, 0x78, 0xd3, 0xd3, 0x90, 0xc1, 0x7a, 0xaa, 0xfa,
    0x99, 0x99, 0xde, 0xe0, 0x00, 0x32, 0xe6, 0xff, 0x00, 0xaa, 0xcd, 0xd3, 0xdb, 0x71, 0xee, 0x9a,
    0xcc, 0x85, 0xaa, 0xcd, 0xd3, 0xdb, 0x71, 0xee, 0x9a, 0xcc, 0x85, 0x70, 0xfa, 0xe1, 0xb5, 0xad,
    0x80, 0xe6, 0x00, 0x23, 0x00, 0x00, 0xd6, 0xaa, 0x98, 0xee, 0x93, 0xd5, 0xb9, 0x3f, 0xcb, 0xdd,
    0xe6, 0x80, 0x00, 0x8f, 0xe1, 0x13, 0x72, 0x60, 0x57, 0xee, 0x2d, 0xc0, 0xeb, 0xcd, 0x90, 0x90,
    0xfa, 0x7f, 0xb4, 0x8c, 0x80, 0xd8, 0x47, 0xd0, 0xee, 0xb3, 0xff, 0xf5, 0x00, 0x32,
];

/// raw `Color::SetAlpha` / `Color::ResetAlpha` 의 공유 scan & remove 루프.
///
/// raw `b25ac-b2678` (SetAlpha) / `14d1b4-14d280` (ResetAlpha) 양쪽 모두 동일
/// 패턴: linear scan; PKey ∈ {500, 501, 502} 인 entry 를 만나면 memmove 로 뒤
/// entry 들을 한 칸 앞당기고 end 를 8B 감소.
///
/// # Safety
/// `ce` 는 valid `ColorEffect*` (caller 가 null 검사).
unsafe fn remove_alpha_entries(ce: *mut ColorEffect) {
    if ce.is_null() {
        return;
    }
    let begin = (*ce).begin;
    let mut end = (*ce).end;

    if begin == end {
        return; // empty
    }

    // raw `b259c..b2674`: read entry at x10, if PKey-500 in [0..2]:
    //   memmove x10 ← x10+8 .. end-1; end -= 8; continue from same position
    // else: x10 += 8; continue until x10 == end
    let mut cur = begin;
    while cur != end {
        let entry = *cur;
        let pkey = (entry & 0xFFFF_FFFF) as u32;
        let pkey_offset = pkey.wrapping_sub(500);
        if pkey_offset <= 2 {
            // remove: memmove [cur+1 .. end] → [cur .. end-1]
            let count_bytes = (end as usize) - (cur as usize) - 8;
            if count_bytes > 0 {
                std::ptr::copy(
                    cur.add(1) as *const u8,
                    cur as *mut u8,
                    count_bytes,
                );
            }
            end = (end as *mut u8).sub(8) as *mut u64;
            (*ce).end = end;
            // 재read from same cur (raw 는 새 entry 를 다시 검사)
            continue;
        }
        cur = cur.add(1);
    }
}

// =============================================================================
// Color::operator==/!=/< — raw `0x14c8fc` / `0x14cbbc` / `0x14cbd4`
// =============================================================================

impl Color {
    /// raw `Color::operator<(Color const&)` (`0x14cbd4`) — full 1:1 port.
    ///
    /// 알고리즘:
    /// 1. type_tag 비교 (raw 0x14cbe0-0x14cbf4):
    ///    - self.type < other.type → return true (less)
    ///    - self.type > other.type → return false (not less)
    ///    - equal → continue
    /// 2. value 비교 (type-dispatch jump table @ raw 0x743a2b):
    ///    | type | branch        | semantic                |
    ///    |------|---------------|-------------------------|
    ///    | 0 (Rgb)        | 0x14cc5c | 3-byte lex (normal `<`)        |
    ///    | 1 (Cmyk)       | 0x14ccc8 | 4-byte lex (normal `<`)        |
    ///    | 2 (Scheme)     | 0x14cc40 | u32 signed lex (normal `<`)    |
    ///    | 3 (System)     | 0x14cc40 | u32 signed lex (normal `<`)    |
    ///    | 4 (Preset)     | 0x14cc40 | u32 signed lex (normal `<`)    |
    ///    | 5 (ScRgb)      | 0x14ccf4 | **inverted** (Hancom bug: `>`) |
    ///    | 6 (Hsl)        | 0x14cd80 | 3-float lex (normal `<`)       |
    /// 3. ColorEffect 비교 (raw 0x14cd14-0x14cdfc):
    ///    - 둘 다 null: equal → false (not less)
    ///    - self null, other non-null: true (less)
    ///    - self non-null, other null: false (not less)
    ///    - both non-null: u64 entry-wise lex compare ((pkey, float) pair).
    ///
    /// # ScRgb buggy semantic 보존 (정공법 절대 준수)
    ///
    /// raw `ScRgb::operator<` (`0x29b68`) 가 inverted 되어 있어, Color::op< 의
    /// ScRgb 분기도 inverted 결과 산출. byte-equivalent 유지 위해 그대로 port.
    pub fn lt_struct(&self, other: &Color) -> bool {
        // raw 0x14cbe0-0x14cbf4: type compare
        if self.type_tag < other.type_tag {
            return true;
        }
        if self.type_tag > other.type_tag {
            return false;
        }
        // type equal: dispatch on type
        match self.cmp_value(other) {
            std::cmp::Ordering::Less => true,
            std::cmp::Ordering::Greater => false,
            std::cmp::Ordering::Equal => unsafe { self.cmp_color_effect_less(other) },
        }
    }

    /// raw `Color::operator==(Color const&)` (`0x14c8fc`) — full 1:1 port.
    ///
    /// 알고리즘:
    /// 1. type_tag 같지 않으면 false.
    /// 2. value 비교 per type (raw 0x14c93c-0x14ca40):
    ///    - 0 (Rgb): 3-byte equality
    ///    - 1 (Cmyk): 4-byte equality
    ///    - 2/3/4 (enum types): u32 equality
    ///    - 5 (ScRgb) / 6 (Hsl): 3-float equality
    /// 3. ColorEffect 비교 (raw 0x14c94c-0x14ca90):
    ///    - 둘 다 null: true
    ///    - 한쪽만 null: 새 empty effect 만들고 `ColorEffect::operator==(real, empty)` 비교
    ///    - 둘 다 non-null: ColorEffect::operator== 비교
    pub fn eq_struct(&self, other: &Color) -> bool {
        // raw 0x14c90c-0x14c918: type_tag compare
        if self.type_tag != other.type_tag {
            return false;
        }
        // value compare per type
        if !self.value_eq_for_type(other) {
            return false;
        }
        // ColorEffect compare
        unsafe { Self::color_effect_eq(self.color_effect, other.color_effect) }
    }

    /// raw `Color::operator!=(Color const&)` (`0x14cbbc`): `!operator==`.
    #[inline]
    pub fn ne_struct(&self, other: &Color) -> bool {
        !self.eq_struct(other)
    }

    /// Per-type value comparison returning Ordering (used by `lt_struct`).
    fn cmp_value(&self, other: &Color) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        match self.type_tag {
            0 => {
                // raw 0x14cc5c: Rgb 3-byte unsigned lex
                let s = self.get_rgb();
                let o = other.get_rgb();
                if s.r < o.r {
                    return Ordering::Less;
                }
                if s.r > o.r {
                    return Ordering::Greater;
                }
                if s.g < o.g {
                    return Ordering::Less;
                }
                if s.g > o.g {
                    return Ordering::Greater;
                }
                if s.b < o.b {
                    return Ordering::Less;
                }
                if s.b > o.b {
                    return Ordering::Greater;
                }
                Ordering::Equal
            }
            1 => {
                // raw 0x14ccc8: Cmyk 4-byte unsigned lex
                let s = self.get_cmyk();
                let o = other.get_cmyk();
                if s.c < o.c {
                    return Ordering::Less;
                }
                if s.c > o.c {
                    return Ordering::Greater;
                }
                if s.m < o.m {
                    return Ordering::Less;
                }
                if s.m > o.m {
                    return Ordering::Greater;
                }
                if s.y < o.y {
                    return Ordering::Less;
                }
                if s.y > o.y {
                    return Ordering::Greater;
                }
                if s.k < o.k {
                    return Ordering::Less;
                }
                if s.k > o.k {
                    return Ordering::Greater;
                }
                Ordering::Equal
            }
            2 | 3 | 4 => {
                // raw 0x14cc40: u32 signed compare (= ldr w; cmp w; b.lt)
                let s = self.get_scheme_style_raw_u32() as i32;
                let o = other.get_scheme_style_raw_u32() as i32;
                s.cmp(&o)
            }
            5 => {
                // raw 0x14ccf4: **inverted** (Hancom bug) — Color::op< 의 ScRgb 분기는
                // ScRgb 가 self > other 일 때 less, self < other 일 때 not-less.
                // 즉 type-equal && self.value > other.value (lex) → return less.
                let s = self.get_scrgb();
                let o = other.get_scrgb();
                // 본 ordering 은 raw 의 buggy semantic 그대로:
                // 1. self.r > other.r → Less (Color::op< 의 less return)
                // 2. self.r < other.r → Greater
                // 3. equal → next element
                if s.r > o.r {
                    return Ordering::Less;
                }
                if s.r < o.r {
                    return Ordering::Greater;
                }
                if s.g > o.g {
                    return Ordering::Less;
                }
                if s.g < o.g {
                    return Ordering::Greater;
                }
                if s.b > o.b {
                    return Ordering::Less;
                }
                if s.b < o.b {
                    return Ordering::Greater;
                }
                Ordering::Equal
            }
            6 => {
                // raw 0x14cd80: Hsl 3-float lex (normal `<`)
                let s = self.get_hsl();
                let o = other.get_hsl();
                if s.h < o.h {
                    return Ordering::Less;
                }
                if s.h > o.h {
                    return Ordering::Greater;
                }
                if s.s < o.s {
                    return Ordering::Less;
                }
                if s.s > o.s {
                    return Ordering::Greater;
                }
                if s.l < o.l {
                    return Ordering::Less;
                }
                if s.l > o.l {
                    return Ordering::Greater;
                }
                Ordering::Equal
            }
            _ => Ordering::Equal, // unknown type — undefined; raw 가 jump table 범위 외엔 단순 fall-through
        }
    }

    /// Per-type value equality (used by `eq_struct`).
    fn value_eq_for_type(&self, other: &Color) -> bool {
        match self.type_tag {
            0 => self.get_rgb().eq_struct(&other.get_rgb()),
            1 => self.get_cmyk().eq_struct(&other.get_cmyk()),
            2 | 3 | 4 => self.get_scheme_style_raw_u32() == other.get_scheme_style_raw_u32(),
            5 => self.get_scrgb().eq_struct(&other.get_scrgb()),
            6 => self.get_hsl().eq_struct(&other.get_hsl()),
            _ => true, // raw 에서 unknown type 분기 fall-through 시 type-tag 동등이면 equal
        }
    }

    /// raw 0x14cd14-0x14cdfc: ColorEffect lex compare for `Color::operator<`.
    ///
    /// # Safety
    /// `self.color_effect` / `other.color_effect` 가 valid 또는 null.
    unsafe fn cmp_color_effect_less(&self, other: &Color) -> bool {
        let s_eff = self.color_effect;
        let o_eff = other.color_effect;

        // raw 0x14cd1c-0x14cdfc: null handling
        if s_eff.is_null() {
            // raw 0x14cdec: return (other.effect != null)
            return !o_eff.is_null();
        }
        if o_eff.is_null() {
            // raw 0x14cd20: return false (self has more)
            return false;
        }

        // raw 0x14cd24-0x14cd2c: other.effect 검사
        let o_begin = (*o_eff).begin;
        let o_end = (*o_eff).end;
        if o_begin == o_end {
            // other empty: not less
            return false;
        }
        // raw 0x14cd30-0x14cd38: self.effect 검사
        let mut s_iter = (*s_eff).begin;
        let s_end = (*s_eff).end;
        let mut o_iter = o_begin;

        // raw 0x14cd34: loop start
        loop {
            // raw 0x14cd34: if self exhausted, return less
            if s_iter == s_end {
                return true;
            }
            // raw 0x14cd3c-0x14cd50: pkey compare (low 32 of u64)
            let s_pkey = (*s_iter) as u32 as i32;
            let o_pkey = (*o_iter) as u32 as i32;
            // raw 0x14cd44-0x14cd48: signed b.lt (`cmp w12, w13; b.lt return-less`)
            if s_pkey < o_pkey {
                return true;
            }
            if o_pkey < s_pkey {
                return false;
            }
            // raw 0x14cd54-0x14cd60: float compare (high 32 of u64)
            let s_float = f32::from_bits(((*s_iter) >> 32) as u32);
            let o_float = f32::from_bits(((*o_iter) >> 32) as u32);
            // raw 0x14cd60: b.mi (= self.float < other.float): return less
            if s_float < o_float {
                return true;
            }
            // raw 0x14cd70-0x14cd78: 다음 entry (only if not strictly greater + other not exhausted)
            //
            // raw 의 ccmp + b.ne 패턴 의미:
            // - self.float < other.float: 이미 return (위)
            // - self.float > other.float: ccmp = NZCV=0000 → Z=0 → NE → loop back (사실상 무의미)
            //   하지만 next iteration 의 cmp x11, x10 에서 self_iter > s_end 가 보장 안 되므로
            //   루프 안전. 본 코드에선 explicit 처리.
            // - self.float == other.float: ccmp = cmp(o_iter+8, o_end) 결과; if equal → return false
            // 본 1:1 port:
            if s_float > o_float {
                // raw 의 buggy behavior: advance + loop without early return.
                // ccmp 가 NZCV=0 (Z=0) 면 b.ne taken → loop back to 14cd34.
                // 의미적으로 self.float > other.float 는 "not less" 인데 raw 는 loop 계속.
                // 본 단계는 raw asm 1:1 — advance 후 self exhaustion 만 체크.
                // 다음 iteration: self_iter+1 vs s_end; if equal: return true (less, raw bug).
                s_iter = s_iter.add(1);
                o_iter = o_iter.add(1);
                if o_iter == o_end {
                    // raw 0x14cd7c: b to 14cbf4 with w0=0 (not less)
                    return false;
                }
                continue;
            }
            // s_float == o_float: advance both
            s_iter = s_iter.add(1);
            o_iter = o_iter.add(1);
            if o_iter == o_end {
                // raw 0x14cd7c: return 0 (not less; both effects equal up to other's length)
                return false;
            }
            // continue loop
        }
    }

    /// raw `Color::operator==` 의 ColorEffect 비교 부분 (`0x14c94c-0x14ca90`).
    ///
    /// 둘 다 null: true. 한쪽만 null: empty effect 만들고 ColorEffect::operator==
    /// 비교 (PKey-aware alpha folding, raw `0x14cab4`). 둘 다 non-null: 직접 비교.
    ///
    /// 본 단계는 `ColorEffect::operator==` 의 PKey jump table (28 PKey variants)
    /// 가 RE 미완 — **단순화 비교 (둘 다 null 인 경우만 true, else 정확 동치 비교
    /// 는 별도 세션)**. 본 단계의 ColorEffect 가 비어있는 (= ColorScheme init 의
    /// 모든 12 entries 와 같은 경우) common case 에선 정확.
    ///
    /// # Safety
    /// `a`, `b` 가 valid 또는 null.
    unsafe fn color_effect_eq(
        a: *mut ColorEffect,
        b: *mut ColorEffect,
    ) -> bool {
        if a.is_null() && b.is_null() {
            return true;
        }
        if a.is_null() || b.is_null() {
            // raw 는 새 empty ColorEffect alloc 후 비교 — 본 단계는 단순화:
            // null 와 non-null 의 entries 길이가 0 이면 동등, else not eq.
            let non_null = if a.is_null() { b } else { a };
            return (*non_null).is_empty();
        }
        // 둘 다 non-null: byte-eq raw bit comparison (PKey jump table 1:1 port
        // 은 ColorEffect::Add 의 28 variants 와 함께 별도 세션).
        if (*a).len() != (*b).len() {
            return false;
        }
        let mut p_a = (*a).begin;
        let mut p_b = (*b).begin;
        let end_a = (*a).end;
        while p_a < end_a {
            if (*p_a) != (*p_b) {
                return false;
            }
            p_a = p_a.add(1);
            p_b = p_b.add(1);
        }
        true
    }
}

impl Drop for Color {
    /// raw `Color::~Color()` (`0x14c870`):
    ///
    /// ```asm
    /// 14c880: ldr x20, [x0, #0x10]    ; x20 = color_effect
    /// 14c884: cbz x20, exit
    /// 14c888: ldr x0, [x20]           ; x0 = (*effect).begin
    /// 14c88c: cbz x0, skip_buf
    /// 14c890: str x0, [x20, #0x8]     ; effect.end = begin   (libc++ __clear)
    /// 14c894: bl  operator_delete     ; free begin
    /// 14c898: mov x0, x20
    /// 14c89c: bl  operator_delete     ; free effect struct
    /// 14c8a0: ret
    /// ```
    ///
    /// raw 동작과 1:1: `ColorEffect::raw_delete(self.color_effect)` 호출.
    fn drop(&mut self) {
        unsafe {
            ColorEffect::raw_delete(self.color_effect);
            self.color_effect = ptr::null_mut();
        }
    }
}

// Color 는 raw ptr 을 owning 으로 보유 — Clone 은 명시적 copy_ctor 호출 강제.
// (`#[derive(Clone)]` 은 의도된 deep-copy 가 아닌 shallow 가 되어 위험)

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_layout_size_align() {
        assert_eq!(std::mem::size_of::<Color>(), 24);
        assert_eq!(std::mem::align_of::<Color>(), 8);
    }

    #[test]
    fn field_offsets_match_raw() {
        let c = Color::from_system_style(SystemStyle(0));
        let p = &c as *const Color as usize;
        let pv = &c.value as *const _ as usize;
        let pt = &c.type_tag as *const _ as usize;
        let pe = &c.color_effect as *const _ as usize;
        assert_eq!(pv - p, 0x00);
        assert_eq!(pt - p, 0x0c);
        assert_eq!(pe - p, 0x10);
    }

    #[test]
    fn from_system_style_layout_matches_raw() {
        // raw `Color(SystemStyle=8)` 의 ColorScheme[0] 케이스
        let c = Color::from_system_style(SystemStyle::WINDOW); // 0x8
        assert_eq!(c.value[0], 0x08);
        assert_eq!(c.value[1], 0x00);
        assert_eq!(c.value[2], 0x00);
        assert_eq!(c.value[3], 0x00);
        assert_eq!(c.type_tag, color_type::SYSTEM);
        assert!(c.color_effect.is_null());
        assert_eq!(c.get_type(), 3);
        assert_eq!(c.get_system_style(), SystemStyle(8));
    }

    #[test]
    fn from_rgb_layout_matches_raw() {
        // raw `Color(Rgb(0x3a, 0x3c, 0x84))` 의 ColorScheme[2] 케이스
        let c = Color::from_rgb(0x3a, 0x3c, 0x84, ptr::null_mut());
        assert_eq!(c.value[0], 0x3a);
        assert_eq!(c.value[1], 0x3c);
        assert_eq!(c.value[2], 0x84);
        assert_eq!(c.type_tag, color_type::RGB);
        assert!(c.color_effect.is_null());
        let rgb = c.get_rgb();
        assert_eq!((rgb.r, rgb.g, rgb.b), (0x3a, 0x3c, 0x84));
    }

    #[test]
    fn from_scheme_style_basic() {
        let c = Color::from_scheme_style(SchemeStyle::Accent3, ptr::null_mut());
        assert_eq!(c.get_scheme_style_raw_u32(), 6);
        assert_eq!(c.type_tag, color_type::SCHEME);
        assert_eq!(c.as_scheme_style(), Some(SchemeStyle::Accent3));
    }

    #[test]
    fn from_scheme_style_auto_transfers_effect() {
        unsafe {
            let mut effect_slot = ColorEffect::create();
            let effect_ptr_ptr = (&mut effect_slot) as *mut *mut ColorEffect;
            let c = Color::from_scheme_style_auto(SchemeStyle::Accent1, effect_ptr_ptr);
            // After ctor: caller's slot is null (auto_ptr release)
            assert!(effect_slot.is_null());
            assert!(!c.color_effect.is_null());
            // Drop c → deletes effect
            drop(c);
            let _ = effect_ptr_ptr; // silence
        }
    }

    #[test]
    fn from_preset_style_basic() {
        let c = Color::from_preset_style(PresetStyle(42));
        assert_eq!(c.value[0], 42);
        assert_eq!(c.type_tag, color_type::PRESET);
        assert_eq!(c.get_preset_style(), PresetStyle(42));
    }

    #[test]
    fn from_cmyk_layout() {
        let cmyk = Cmyk {
            c: 0x10,
            m: 0x20,
            y: 0x30,
            k: 0x40,
        };
        let c = Color::from_cmyk(&cmyk);
        assert_eq!(c.value[0], 0x10);
        assert_eq!(c.value[1], 0x20);
        assert_eq!(c.value[2], 0x30);
        assert_eq!(c.value[3], 0x40);
        assert_eq!(c.type_tag, color_type::CMYK);
        let cm = c.get_cmyk();
        assert_eq!((cm.c, cm.m, cm.y, cm.k), (0x10, 0x20, 0x30, 0x40));
    }

    #[test]
    fn from_scrgb_layout() {
        let src = ScRgb {
            r: 0.5,
            g: 1.0,
            b: 1.5,
        };
        let c = Color::from_scrgb(&src);
        let got = c.get_scrgb();
        assert_eq!(got.r, 0.5);
        assert_eq!(got.g, 1.0);
        assert_eq!(got.b, 1.5);
        assert_eq!(c.type_tag, color_type::SC_RGB);
    }

    #[test]
    fn from_hsl_layout() {
        let src = Hsl {
            h: 120.0,
            s: 0.5,
            l: 0.75,
        };
        let c = Color::from_hsl(&src);
        let got = c.get_hsl();
        assert_eq!(got.h, 120.0);
        assert_eq!(got.s, 0.5);
        assert_eq!(got.l, 0.75);
        assert_eq!(c.type_tag, color_type::HSL);
    }

    #[test]
    fn copy_ctor_clones_effect_independently() {
        unsafe {
            // src 에 effect 부여
            let effect = ColorEffect::create();
            let layout = std::alloc::Layout::from_size_align(16, 8).unwrap();
            let buf = std::alloc::alloc(layout) as *mut u64;
            *buf = 0xCAFEBABE_DEADBEEFu64;
            *buf.add(1) = 0xFACEFEED_12345678u64;
            (*effect).begin = buf;
            (*effect).end = buf.add(2);
            (*effect).cap_end = buf.add(2);

            let src = Color::from_rgb(0xAA, 0xBB, 0xCC, effect);
            let dst = Color::copy_ctor(&src);
            // dst 는 별도 alloc 의 effect 보유
            assert!(!dst.color_effect.is_null());
            assert_ne!(dst.color_effect, src.color_effect);
            // 동일 byte 패턴
            assert_eq!(*(*dst.color_effect).begin, 0xCAFEBABE_DEADBEEFu64);
            // src effect 변경해도 dst 무관
            *(*src.color_effect).begin = 0;
            assert_eq!(*(*dst.color_effect).begin, 0xCAFEBABE_DEADBEEFu64);
            // value + type_tag 동일
            assert_eq!(dst.value, src.value);
            assert_eq!(dst.type_tag, src.type_tag);
            // drop 둘 다 정상
            drop(dst);
            drop(src);
        }
    }

    #[test]
    fn copy_ctor_with_null_effect_returns_null_clone() {
        let src = Color::from_system_style(SystemStyle::WINDOW);
        let dst = unsafe { Color::copy_ctor(&src) };
        assert!(dst.color_effect.is_null());
        assert_eq!(dst.value, src.value);
        assert_eq!(dst.type_tag, src.type_tag);
    }

    #[test]
    fn clone_to_heap_and_raw_delete() {
        unsafe {
            let src = Color::from_rgb(1, 2, 3, ptr::null_mut());
            let cloned = src.clone_to_heap();
            assert!(!cloned.is_null());
            assert_eq!((*cloned).value[0], 1);
            assert_eq!((*cloned).type_tag, color_type::RGB);
            Color::raw_delete(cloned);
        }
    }

    #[test]
    fn swap_two_colors() {
        let mut a = Color::from_system_style(SystemStyle::WINDOW);
        let mut b = Color::from_rgb(0xFF, 0x00, 0xFF, ptr::null_mut());
        let a_orig_value = a.value;
        let b_orig_value = b.value;
        a.swap(&mut b);
        assert_eq!(a.value, b_orig_value);
        assert_eq!(b.value, a_orig_value);
        assert_eq!(a.type_tag, color_type::RGB);
        assert_eq!(b.type_tag, color_type::SYSTEM);
    }

    #[test]
    fn swap_with_effects_swaps_pointers() {
        unsafe {
            let e1 = ColorEffect::create();
            let e2 = ColorEffect::create();
            let mut a = Color::from_rgb(1, 2, 3, e1);
            let mut b = Color::from_rgb(4, 5, 6, e2);
            a.swap(&mut b);
            assert_eq!(a.color_effect, e2);
            assert_eq!(b.color_effect, e1);
            // drop both → frees both effects
        }
    }

    #[test]
    fn drop_with_null_effect_is_safe() {
        let c = Color::from_rgb(1, 2, 3, ptr::null_mut());
        drop(c); // should not panic
    }

    #[test]
    fn drop_with_effect_frees_buffer() {
        unsafe {
            let effect = ColorEffect::create();
            let layout = std::alloc::Layout::from_size_align(8, 8).unwrap();
            let buf = std::alloc::alloc(layout) as *mut u64;
            *buf = 0xABCD;
            (*effect).begin = buf;
            (*effect).end = buf.add(1);
            (*effect).cap_end = buf.add(1);
            let c = Color::from_rgb(0, 0, 0, effect);
            drop(c);
            // miri/Valgrind would detect leak if not freed; logical assertion:
            // drop returns without panic and no double-free.
        }
    }

    #[test]
    fn get_color_effect_returns_stored_ptr() {
        unsafe {
            let e = ColorEffect::create();
            let c = Color::from_rgb(0, 0, 0, e);
            assert_eq!(c.get_color_effect(), e);
            drop(c);
        }
    }

    #[test]
    fn raw_delete_of_null_is_noop() {
        unsafe {
            Color::raw_delete(ptr::null_mut());
        }
    }

    // =============================================================================
    // operator==/!=/< tests
    // =============================================================================

    #[test]
    fn eq_basic_rgb() {
        let a = Color::from_rgb(0x11, 0x22, 0x33, ptr::null_mut());
        let b = Color::from_rgb(0x11, 0x22, 0x33, ptr::null_mut());
        let c = Color::from_rgb(0x11, 0x22, 0x34, ptr::null_mut());
        assert!(a.eq_struct(&b));
        assert!(!a.eq_struct(&c));
        assert!(a.ne_struct(&c));
    }

    #[test]
    fn eq_different_types_false() {
        let rgb = Color::from_rgb(8, 0, 0, ptr::null_mut());
        let sys = Color::from_system_style(SystemStyle(8));
        assert!(!rgb.eq_struct(&sys));
    }

    #[test]
    fn eq_same_system_style() {
        let a = Color::from_system_style(SystemStyle(5));
        let b = Color::from_system_style(SystemStyle(5));
        assert!(a.eq_struct(&b));
        let c = Color::from_system_style(SystemStyle(6));
        assert!(!a.eq_struct(&c));
    }

    #[test]
    fn lt_type_dispatch() {
        // type compare: lower type → less
        let rgb_t0 = Color::from_rgb(0xFF, 0, 0, ptr::null_mut()); // type 0
        let cmyk_t1 = Color::from_cmyk(&Cmyk {
            c: 0,
            m: 0,
            y: 0,
            k: 0,
        }); // type 1
        let sys_t3 = Color::from_system_style(SystemStyle(0)); // type 3
        assert!(rgb_t0.lt_struct(&cmyk_t1));
        assert!(rgb_t0.lt_struct(&sys_t3));
        assert!(cmyk_t1.lt_struct(&sys_t3));
        assert!(!cmyk_t1.lt_struct(&rgb_t0));
    }

    #[test]
    fn lt_rgb_normal_lex_order() {
        let a = Color::from_rgb(0x3a, 0x3c, 0x84, ptr::null_mut());
        let b = Color::from_rgb(0xfa, 0xf3, 0xdb, ptr::null_mut());
        // a.r (0x3a) < b.r (0xfa) → less
        assert!(a.lt_struct(&b));
        assert!(!b.lt_struct(&a));
        // a == a → not less
        let a2 = Color::from_rgb(0x3a, 0x3c, 0x84, ptr::null_mut());
        assert!(!a.lt_struct(&a2));
    }

    #[test]
    fn lt_cmyk_normal_lex_order() {
        let a = Color::from_cmyk(&Cmyk {
            c: 1,
            m: 2,
            y: 3,
            k: 4,
        });
        let b = Color::from_cmyk(&Cmyk {
            c: 2,
            m: 0,
            y: 0,
            k: 0,
        });
        assert!(a.lt_struct(&b));
        assert!(!b.lt_struct(&a));
    }

    #[test]
    fn lt_scheme_style_u32_order() {
        let a = Color::from_scheme_style(SchemeStyle::Background1, ptr::null_mut()); // 0
        let b = Color::from_scheme_style(SchemeStyle::Accent3, ptr::null_mut()); // 6
        assert!(a.lt_struct(&b));
        assert!(!b.lt_struct(&a));
    }

    #[test]
    fn lt_scrgb_inverted_per_raw_bug() {
        // raw Color::op< 의 ScRgb 분기는 Hancom inverted bug:
        // self.r > other.r → less (raw 의 14ce04 b.mi to 14ce44 = less-return)
        // self.r < other.r → not less
        let a = Color::from_scrgb(&ScRgb {
            r: 0.5,
            g: 0.0,
            b: 0.0,
        });
        let b = Color::from_scrgb(&ScRgb {
            r: 1.0,
            g: 0.0,
            b: 0.0,
        });
        // a.r < b.r mathematically, but raw inverted → b < a (= a is "greater" in std::map sense)
        assert!(!a.lt_struct(&b), "raw ScRgb bug: a.r < b.r → NOT less");
        assert!(b.lt_struct(&a), "raw ScRgb bug: b.r > a.r → less");
    }

    #[test]
    fn lt_hsl_normal_lex() {
        let a = Color::from_hsl(&Hsl {
            h: 60.0,
            s: 0.0,
            l: 0.0,
        });
        let b = Color::from_hsl(&Hsl {
            h: 120.0,
            s: 0.0,
            l: 0.0,
        });
        assert!(a.lt_struct(&b));
        assert!(!b.lt_struct(&a));
    }

    #[test]
    fn lt_color_effect_null_both_not_less() {
        // both effects null + same value: equal → not less
        let a = Color::from_rgb(1, 2, 3, ptr::null_mut());
        let b = Color::from_rgb(1, 2, 3, ptr::null_mut());
        assert!(!a.lt_struct(&b));
        assert!(!b.lt_struct(&a));
    }

    #[test]
    fn lt_color_effect_self_null_other_non_null_less() {
        unsafe {
            let eff = ColorEffect::create();
            let a = Color::from_rgb(1, 2, 3, ptr::null_mut());
            let b = Color::from_rgb(1, 2, 3, eff);
            // raw 0x14cdec: cset ne (other.effect != null) → 1 (less)
            assert!(a.lt_struct(&b));
            assert!(!b.lt_struct(&a));
        }
    }

    #[test]
    fn eq_null_effects_equal_values() {
        let a = Color::from_rgb(0xAB, 0xCD, 0xEF, ptr::null_mut());
        let b = Color::from_rgb(0xAB, 0xCD, 0xEF, ptr::null_mut());
        assert!(a.eq_struct(&b));
    }

    #[test]
    fn eq_different_rgb_values_not_equal() {
        let a = Color::from_rgb(0xAB, 0xCD, 0xEF, ptr::null_mut());
        let b = Color::from_rgb(0xAB, 0xCD, 0xEE, ptr::null_mut());
        assert!(!a.eq_struct(&b));
    }

    #[test]
    fn ne_inverse_of_eq() {
        let a = Color::from_rgb(1, 2, 3, ptr::null_mut());
        let b = Color::from_rgb(1, 2, 3, ptr::null_mut());
        let c = Color::from_rgb(1, 2, 4, ptr::null_mut());
        assert_eq!(a.eq_struct(&b), !a.ne_struct(&b));
        assert_eq!(a.eq_struct(&c), !a.ne_struct(&c));
    }

    #[test]
    fn lt_strict_irreflexive() {
        // a < a should always be false
        let a = Color::from_rgb(1, 2, 3, ptr::null_mut());
        let b = Color::from_system_style(SystemStyle(5));
        let c = Color::from_cmyk(&Cmyk {
            c: 1,
            m: 2,
            y: 3,
            k: 4,
        });
        assert!(!a.lt_struct(&a));
        assert!(!b.lt_struct(&b));
        assert!(!c.lt_struct(&c));
    }

    #[test]
    fn colorscheme_12_hardcoded_values_layout() {
        // raw ColorScheme::ColorScheme() 의 12 entries
        let c0 = Color::from_system_style(SystemStyle(8));
        assert_eq!(c0.value[..4], [0x08, 0x00, 0x00, 0x00]);
        assert_eq!(c0.type_tag, 3);
        let c1 = Color::from_system_style(SystemStyle(5));
        assert_eq!(c1.value[..4], [0x05, 0x00, 0x00, 0x00]);
        // Entry 2..11 are Rgb with hardcoded values:
        // Entry 2: 0x3a, 0x3c, 0x84  (raw `mov w8, #0x3c3a; sturh; mov w8, #0x84; sturb`)
        let c2 = Color::from_rgb(0x3a, 0x3c, 0x84, ptr::null_mut());
        assert_eq!(c2.value[..3], [0x3a, 0x3c, 0x84]);
        assert_eq!(c2.type_tag, 0);
        // Entry 3: 0xfa, 0xf3, 0xdb
        let c3 = Color::from_rgb(0xfa, 0xf3, 0xdb, ptr::null_mut());
        assert_eq!(c3.value[..3], [0xfa, 0xf3, 0xdb]);
        // Entry 4: 0x61, 0x82, 0xd6
        let c4 = Color::from_rgb(0x61, 0x82, 0xd6, ptr::null_mut());
        assert_eq!(c4.value[..3], [0x61, 0x82, 0xd6]);
        // Entry 5: 0xff, 0x84, 0x3a
        let c5 = Color::from_rgb(0xff, 0x84, 0x3a, ptr::null_mut());
        assert_eq!(c5.value[..3], [0xff, 0x84, 0x3a]);
        // Entry 6: 0xb2, 0xb2, 0xb2
        let c6 = Color::from_rgb(0xb2, 0xb2, 0xb2, ptr::null_mut());
        assert_eq!(c6.value[..3], [0xb2, 0xb2, 0xb2]);
        // Entry 7: 0xff, 0xd7, 0x00
        let c7 = Color::from_rgb(0xff, 0xd7, 0x00, ptr::null_mut());
        assert_eq!(c7.value[..3], [0xff, 0xd7, 0x00]);
        // Entry 8: 0x28, 0x9b, 0x6e
        let c8 = Color::from_rgb(0x28, 0x9b, 0x6e, ptr::null_mut());
        assert_eq!(c8.value[..3], [0x28, 0x9b, 0x6e]);
        // Entry 9: 0x9d, 0x5c, 0xbb
        let c9 = Color::from_rgb(0x9d, 0x5c, 0xbb, ptr::null_mut());
        assert_eq!(c9.value[..3], [0x9d, 0x5c, 0xbb]);
        // Entry 10: 0x00, 0x00, 0xff
        let c10 = Color::from_rgb(0x00, 0x00, 0xff, ptr::null_mut());
        assert_eq!(c10.value[..3], [0x00, 0x00, 0xff]);
        // Entry 11: 0x80, 0x00, 0x80
        let c11 = Color::from_rgb(0x80, 0x00, 0x80, ptr::null_mut());
        assert_eq!(c11.value[..3], [0x80, 0x00, 0x80]);
    }

    // ============================================================
    // Color::set_alpha / reset_alpha / set_color_effect tests
    // ============================================================

    /// helper: read entry at index i of color_effect.
    unsafe fn read_entry(c: &Color, i: usize) -> (u32, f32) {
        let ce = c.color_effect;
        assert!(!ce.is_null());
        let buf = (*ce).begin;
        let entry = *buf.add(i);
        let pkey = (entry & 0xFFFF_FFFF) as u32;
        let val = f32::from_bits((entry >> 32) as u32);
        (pkey, val)
    }

    unsafe fn ce_len(c: &Color) -> usize {
        let ce = c.color_effect;
        if ce.is_null() {
            return 0;
        }
        (*ce).len()
    }

    #[test]
    fn set_alpha_on_color_without_effect_creates_one() {
        // raw `b2570`: if clone returns null, alloc new
        let mut c = Color::from_rgb(0x10, 0x20, 0x30, ptr::null_mut());
        assert!(c.color_effect.is_null());
        unsafe {
            c.set_alpha(0.5);
        }
        assert!(!c.color_effect.is_null());
        unsafe {
            assert_eq!(ce_len(&c), 1);
            let (pkey, val) = read_entry(&c, 0);
            assert_eq!(pkey, 500);
            assert_eq!(val, 0.5);
        }
    }

    #[test]
    fn set_alpha_clamps_to_0_1() {
        let mut c = Color::from_rgb(0x10, 0x20, 0x30, ptr::null_mut());
        unsafe {
            c.set_alpha(-0.5);
            assert_eq!(read_entry(&c, 0).1, 0.0);
            c.set_alpha(2.0);
            // 첫 호출 후 PKey 500 entry 가 이미 있음 → 두 번째 호출은:
            // 1. clone (PKey 500 = 0.0)
            // 2. remove all PKey ∈ {500, 501, 502} → 빈 vector
            // 3. add(500, 2.0) → clamp to 1.0
            assert_eq!(ce_len(&c), 1);
            assert_eq!(read_entry(&c, 0).1, 1.0);
            c.set_alpha(0.75);
            assert_eq!(ce_len(&c), 1);
            assert_eq!(read_entry(&c, 0).1, 0.75);
        }
    }

    #[test]
    fn set_alpha_removes_existing_alpha_keys_only() {
        // 미리 PKey 500/501/502/503/505 entries 를 넣은 ColorEffect 준비
        let mut c = Color::from_rgb(1, 2, 3, ptr::null_mut());
        unsafe {
            // ColorEffect 직접 attach
            c.color_effect = ColorEffect::create();
            (*c.color_effect).add(500, 0.3); // alpha
            (*c.color_effect).add(501, 0.4); // lum (PKey 501)
            (*c.color_effect).add(502, 0.5); // lumOff (PKey 502)
            (*c.color_effect).add(503, 0.6); // other key — kept
            (*c.color_effect).add(505, 0.7); // other — kept
            assert_eq!(ce_len(&c), 5);

            c.set_alpha(0.9);

            // alpha keys (500, 501, 502) 제거 → 503, 505 만 남음 + new 500
            assert_eq!(ce_len(&c), 3);
            // 순서: scan loop 가 처음 발견되는 alpha 만 제거 후 재scan하므로
            // 503 → kept (pos 0), 505 → kept (pos 1), 500 (alpha) → kept (pos 2)
            let entries: Vec<(u32, f32)> =
                (0..ce_len(&c)).map(|i| read_entry(&c, i)).collect();
            assert_eq!(entries[0], (503, 0.6));
            assert_eq!(entries[1], (505, 0.7));
            assert_eq!(entries[2], (500, 0.9));
        }
    }

    #[test]
    fn reset_alpha_removes_500_501_502_only() {
        let mut c = Color::from_rgb(1, 2, 3, ptr::null_mut());
        unsafe {
            c.color_effect = ColorEffect::create();
            (*c.color_effect).add(500, 0.3);
            (*c.color_effect).add(503, 0.4);
            (*c.color_effect).add(501, 0.5);
            (*c.color_effect).add(505, 0.6);
            (*c.color_effect).add(502, 0.7);
            assert_eq!(ce_len(&c), 5);

            c.reset_alpha();

            assert_eq!(ce_len(&c), 2);
            assert_eq!(read_entry(&c, 0), (503, 0.4));
            assert_eq!(read_entry(&c, 1), (505, 0.6));
        }
    }

    #[test]
    fn reset_alpha_on_null_color_effect_is_noop() {
        let mut c = Color::from_rgb(1, 2, 3, ptr::null_mut());
        assert!(c.color_effect.is_null());
        unsafe {
            c.reset_alpha();
        }
        // 여전히 null
        assert!(c.color_effect.is_null());
    }

    #[test]
    fn reset_alpha_on_empty_color_effect_is_noop() {
        let mut c = Color::from_rgb(1, 2, 3, ptr::null_mut());
        unsafe {
            c.color_effect = ColorEffect::create();
            assert_eq!(ce_len(&c), 0);
            c.reset_alpha();
            assert_eq!(ce_len(&c), 0);
        }
    }

    #[test]
    fn set_color_effect_steals_ptr_and_frees_old() {
        let mut c = Color::from_rgb(1, 2, 3, ptr::null_mut());
        unsafe {
            // Attach 초기 ColorEffect
            c.color_effect = ColorEffect::create();
            (*c.color_effect).add(503, 0.5);
            assert_eq!(ce_len(&c), 1);

            // 새 ColorEffect 준비
            let mut new_ce: *mut ColorEffect = ColorEffect::create();
            (*new_ce).add(504, 0.25);

            c.set_color_effect(&mut new_ce);

            // 호출 후 new_ce ptr 은 null 로 만들어짐 (auto_ptr.steal)
            assert!(new_ce.is_null());
            // self.color_effect 는 새 것 (504, 0.25)
            assert_eq!(ce_len(&c), 1);
            assert_eq!(read_entry(&c, 0), (504, 0.25));
        }
    }

    #[test]
    fn set_color_effect_on_null_old_works() {
        let mut c = Color::from_rgb(1, 2, 3, ptr::null_mut());
        assert!(c.color_effect.is_null());
        unsafe {
            let mut new_ce = ColorEffect::create();
            (*new_ce).add(500, 0.8);

            c.set_color_effect(&mut new_ce);

            assert!(new_ce.is_null());
            assert_eq!(ce_len(&c), 1);
            assert_eq!(read_entry(&c, 0), (500, 0.8));
        }
    }

    #[test]
    fn set_color_effect_with_null_new_clears_self() {
        let mut c = Color::from_rgb(1, 2, 3, ptr::null_mut());
        unsafe {
            c.color_effect = ColorEffect::create();
            (*c.color_effect).add(500, 0.5);

            let mut new_ce: *mut ColorEffect = ptr::null_mut();
            c.set_color_effect(&mut new_ce);

            assert!(c.color_effect.is_null());
            assert!(new_ce.is_null());
        }
    }

    #[test]
    fn set_alpha_preserves_other_entries_after_alpha_only() {
        let mut c = Color::from_rgb(1, 2, 3, ptr::null_mut());
        unsafe {
            c.color_effect = ColorEffect::create();
            (*c.color_effect).add(510, 0.1); // 비-alpha
            (*c.color_effect).add(500, 0.2); // alpha
            assert_eq!(ce_len(&c), 2);

            c.set_alpha(0.99);

            // 510 + new 500
            assert_eq!(ce_len(&c), 2);
            assert_eq!(read_entry(&c, 0), (510, 0.1));
            assert_eq!(read_entry(&c, 1), (500, 0.99));
        }
    }

    #[test]
    fn multiple_alpha_keys_all_removed_in_set_alpha() {
        let mut c = Color::from_rgb(1, 2, 3, ptr::null_mut());
        unsafe {
            c.color_effect = ColorEffect::create();
            // 5 alpha keys 연속 + 1 non-alpha
            (*c.color_effect).add(500, 0.1);
            (*c.color_effect).add(501, 0.2);
            (*c.color_effect).add(502, 0.3);
            (*c.color_effect).add(500, 0.4);
            (*c.color_effect).add(501, 0.5);
            (*c.color_effect).add(520, 0.99); // non-alpha
            assert_eq!(ce_len(&c), 6);

            c.set_alpha(0.66);

            // 520 + new 500
            assert_eq!(ce_len(&c), 2);
            assert_eq!(read_entry(&c, 0), (520, 0.99));
            assert_eq!(read_entry(&c, 1), (500, 0.66));
        }
    }

    #[test]
    fn set_alpha_does_not_modify_original_color_effect_buffer() {
        // raw 는 clone → modify → swap. 호출 전후 원본 색 데이터는 유지 (다른 Color 인스턴스 기준)
        // 본 test 는 단순히 set_alpha 가 self 의 type/value 를 안 건드림을 확인
        let mut c = Color::from_rgb(0xAB, 0xCD, 0xEF, ptr::null_mut());
        let orig_value = c.value;
        let orig_type = c.type_tag;
        unsafe {
            c.set_alpha(0.5);
        }
        assert_eq!(c.value, orig_value);
        assert_eq!(c.type_tag, orig_type);
    }
}

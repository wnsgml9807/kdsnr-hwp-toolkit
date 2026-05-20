//! `Hnc::Property::Property` (abstract 16B) + concrete sub-class `PColor` (40B).
//!
//! libHncFoundation 의 `Property` 는 PropertyBag 의 value type. 모든 sub-class
//! (PColor, PFloat, PBool, PInt, PSize, ...) 의 abstract base.
//!
//! # raw `Hnc::Property::Property` 16B layout (확정 from `Property(State)` @ `0x4c2f8`)
//!
//! ```text
//! offset   field           type           크기
//! 0x00     vtable          func**         8B (abstract — sub-class 마다 다름)
//! 0x08     state           u32 (=State)   4B
//! 0x0c     _pad            u32            4B (alignment)
//! ```
//!
//! ## raw `Property::Property(State)` @ `0x4c2f8` (5 instr)
//!
//! ```asm
//! adrp x8, 0xd9000
//! add  x8, x8, #0x2f0          ; vtable @ 0xd92f0
//! str  x8, [x0]                  ; self.vtable = ...
//! str  w1, [x0, #0x8]           ; self.state = arg
//! ret
//! ```
//!
//! ## `Hnc::Property::State` enum (inferred from `IsEnable` / `operator==`)
//!
//! - 0 = Default (기본값, 명시적 설정 안 됨)
//! - 1 = ??? (operator== 에서 1 == 2 → equal 의 special case)
//! - 2 = ??? (operator== 에서 2 == 1 → equal)
//! - 3 = Disabled
//! - else = Enabled
//!
//! `IsEnable()` (raw `0x4c36c`): `state != 0 AND state != 3`.
//!
//! # raw `PColor` 40B layout (확정 from SolidBrush::SetColor 의 alloc @ `0x654258`)
//!
//! ```text
//! offset   field          type                         크기
//! 0x00     base           Property (vtable + state)    16B
//! 0x10     color_body     [u8; 16]                     16B (Rgb/Cmyk/etc. body)
//! 0x20     color_effect   *mut ControlBlock<ColorEffect> 8B (cloned)
//! ```
//!
//! 총 40B = 0x28.
//!
//! **편의**: Rust 의 `Color` 가 이미 24B (16B body + 8B color_effect) 로 raw 의
//! `+0x10..+0x28` 영역과 정확히 일치 — `PColor.body = Color` 로 직접 embed 가능.
//!
//! ## raw PColor 생성 (SolidBrush::SetColor 의 INSERT path, `0x654258-0x654288`)
//!
//! ```asm
//! mov  w0, #0x28              ; sizeof(PColor) = 40
//! bl   __Znwm
//! mov  x23, x0
//! mov  x1, x22                 ; state (인자)
//! bl   Property::Property(State)
//! adrp x8, 0x794000
//! add  x8, x8, #0x18           ; PColor vtable @ 0x794018
//! str  x8, [x23]               ; self.vtable = PColor vtable (overrides base vtable)
//! ldr  q0, [x21]               ; 16B Color body
//! str  q0, [x23, #0x10]
//! ldr  x0, [x21, #0x10]        ; src.color_effect
//! bl   0x65411c                 ; ColorEffect clone
//! str  x0, [x23, #0x20]
//! ```

use crate::color::Color;
use std::ptr;

/// `Hnc::Property::State` — u32 enum.
///
/// raw 의 정확한 enum 매핑은 partial RE 결과. 본 상수는 inferred values.
pub mod state {
    pub const DEFAULT: u32 = 0;
    /// raw operator== 의 special case (state 1 ↔ 2 가 equal).
    /// 의미 미확정 — 추정: Enabled with default value vs Enabled with explicit value.
    pub const ENABLED_DEFAULT: u32 = 1;
    pub const ENABLED_EXPLICIT: u32 = 2;
    pub const DISABLED: u32 = 3;
}

/// raw 16B `Hnc::Property::Property` (abstract base).
///
/// vtable layout (vfunc 인덱스):
/// - vfunc[0]: D1 (complete dtor)
/// - vfunc[1]: D0 (deleting dtor) — `delete this` 의 entry point
/// - vfunc[2..]: sub-class 별로 다름 (Clone, GetValue, SetValue 등)
///
/// **개념적으로 abstract** — 직접 인스턴스화 안 됨. 그러나 raw `0x4c2f8` 의 ctor
/// 가 호출 가능 (Property::Property(State)) — sub-class ctor 가 super() 호출.
#[repr(C)]
pub struct Property {
    /// raw +0x00: vtable ptr (sub-class 별 vtable 주소).
    pub vtable: *const u8,
    /// raw +0x08: state (u32 enum).
    pub state: u32,
    /// raw +0x0c: 4B alignment padding.
    pub _pad: u32,
}

pub const PROPERTY_SIZE_BYTES: usize = 16;
pub const PROPERTY_ALIGN_BYTES: usize = 8;

const _: () = assert!(std::mem::size_of::<Property>() == PROPERTY_SIZE_BYTES);
const _: () = assert!(std::mem::align_of::<Property>() == PROPERTY_ALIGN_BYTES);

impl Property {
    /// raw `Property::Property(State)` @ `0x4c2f8` 1:1.
    ///
    /// 추정: 본 ctor 는 sub-class 의 base init 으로만 사용 — 직접 호출 시
    /// vtable = abstract base 의 placeholder.
    pub fn new(state: u32) -> Self {
        Property {
            // raw 의 abstract vtable @ 0xd92f0 (Foundation); Rust 에선 null 로
            // — sub-class ctor 가 즉시 덮어쓸 것이므로 placeholder.
            vtable: ptr::null(),
            state,
            _pad: 0,
        }
    }

    /// raw `Property::GetState() const` @ `0x4c358` (2 instr).
    ///
    /// ```asm
    /// add x0, x0, #0x8
    /// ret
    /// ```
    ///
    /// raw 는 `&self.state` 의 ptr 반환 (`State*`). Rust 는 값 반환 (편의).
    #[inline]
    pub fn get_state(&self) -> u32 {
        self.state
    }

    /// raw `Property::SetState(State const&)` @ `0x4c360` (3 instr).
    ///
    /// ```asm
    /// ldr w8, [x1]        ; dereference State* 인자
    /// str w8, [x0, #0x8]  ; self.state = *arg
    /// ret
    /// ```
    #[inline]
    pub fn set_state(&mut self, new_state: u32) {
        self.state = new_state;
    }

    /// raw `Property::IsEnable() const` @ `0x4c36c` 1:1.
    ///
    /// ```asm
    /// ldr w8, [x0, #0x8]
    /// cmp w8, #0
    /// ccmp w8, #3, #0x4, ne
    /// cset w0, ne
    /// ```
    ///
    /// = `state != 0 AND state != 3`.
    #[inline]
    pub fn is_enable(&self) -> bool {
        self.state != state::DEFAULT && self.state != state::DISABLED
    }

    /// raw `operator==(Property const&)` @ `0x4c318` 1:1.
    ///
    /// state 비교의 special case: state 1 ↔ 2 는 equal (둘 다 "enabled" 의 variants).
    ///
    /// ```asm
    /// ldr  w8, [x0, #0x8]
    /// ldr  w9, [x1, #0x8]
    /// cmp  w8, #2
    /// ccmp w9, #1, #0x0, eq       ; (a == 2) ? cmp(b, 1) : flags=0
    /// ccmp w8, w9, #0x4, ne       ; if NE: cmp(a, b); else: flags=#0x4 (Z=1)
    /// cset w11, eq
    /// cmp  w8, #1
    /// ccmp w9, #2, #0x0, eq       ; (a == 1) ? cmp(b, 2) : flags=0
    /// csel w0, #1, w11, eq
    /// ```
    pub fn eq_op(&self, other: &Property) -> bool {
        let a = self.state;
        let b = other.state;
        // (a == 2 && b == 1) || (a == 1 && b == 2) || (a == b)
        if a == 1 && b == 2 {
            return true;
        }
        if a == 2 && b == 1 {
            return true;
        }
        a == b
    }

    /// raw `operator<(Property const&)` @ `0x4c344` 1:1: state lex compare.
    #[inline]
    pub fn lt_op(&self, other: &Property) -> bool {
        self.state < other.state
    }
}

/// raw 40B `PColor` (Property sub-class) — SolidBrush key `0x259` 의 value type.
///
/// Layout 확정 from SolidBrush::SetColor (`0x173128`) 의 INSERT path (`0x654258-0x654288`).
///
/// ```text
/// offset   field          type           크기
/// 0x00     base           Property       16B
/// 0x10     color          Color (Rust)   24B (= raw 의 16B body + 8B color_effect)
/// ```
///
/// 총 40B (= raw `mov w0, #0x28` 와 일치).
///
/// **Rust 의 `Color` (24B) 가 raw `+0x10..+0x28` 영역과 byte-identical** — 16B
/// body + 8B color_effect ptr 의 layout 일치 (확인: `color.rs` 의 layout 주석).
#[repr(C)]
pub struct PColor {
    /// raw +0x00..+0x10: Property base.
    pub base: Property,
    /// raw +0x10..+0x28: Color (16B body + 8B color_effect ptr).
    pub color: Color,
}

pub const PCOLOR_SIZE_BYTES: usize = 40;
pub const PCOLOR_ALIGN_BYTES: usize = 8;

const _: () = assert!(std::mem::size_of::<PColor>() == PCOLOR_SIZE_BYTES);
const _: () = assert!(std::mem::align_of::<PColor>() == PCOLOR_ALIGN_BYTES);

impl PColor {
    /// raw PColor 생성 (SolidBrush::SetColor 의 INSERT path 1:1).
    ///
    /// ```asm
    /// new(0x28)
    /// Property::Property(state)
    /// vtable = PColor vtable @ 0x794018 (override base)
    /// 16B memcpy of Color body
    /// ColorEffect clone (refcount++)
    /// ```
    ///
    /// 본 Rust port:
    /// - state 인자
    /// - `Color::copy_ctor` 로 src Color 의 deep clone (ColorEffect refcount++ 포함)
    ///
    /// # Safety
    /// `src` 는 valid Color (color_effect 가 valid ControlBlock 또는 null).
    pub unsafe fn new(state: u32, src: &Color) -> Self {
        PColor {
            base: Property::new(state),
            // Color::copy_ctor 가 raw 의 `q0 memcpy + bl 0x65411c (effect clone)` 1:1.
            color: Color::copy_ctor(src),
        }
    }

    /// Heap-allocated 으로 생성 — SolidBrush::SetColor 의 `new(0x28)` 와 1:1.
    ///
    /// 반환 ptr 은 manual delete 필요 (또는 `Box::from_raw` 로 reclaim).
    ///
    /// # Safety
    /// `src` 는 valid Color.
    pub unsafe fn create_raw(state: u32, src: &Color) -> *mut PColor {
        Box::into_raw(Box::new(Self::new(state, src)))
    }

    /// `Color` 값 read-only 접근.
    #[inline]
    pub fn color(&self) -> &Color {
        &self.color
    }

    /// `Property::GetState()` forwarding.
    #[inline]
    pub fn get_state(&self) -> u32 {
        self.base.get_state()
    }

    /// `Property::SetState(State)` forwarding.
    #[inline]
    pub fn set_state(&mut self, s: u32) {
        self.base.set_state(s);
    }

    /// raw PColor vfunc[2] `operator==(PColor const&)` @ `0x6544b4` 1:1.
    ///
    /// ```text
    /// EqualsType(self, other)   ; vtable typeinfo 매치
    /// && Property::operator==(self, other) ; state 비교 (1↔2 special)
    /// && Color::operator==(self.color, other.color)
    /// ```
    ///
    /// 본 Rust port 는 enum 으로 type check (PColor 만 비교) → 자동 type match.
    pub fn eq_op(&self, other: &PColor) -> bool {
        if !self.base.eq_op(&other.base) {
            return false;
        }
        // Color::eq_struct = raw Color::operator==
        self.color.eq_struct(&other.color)
    }

    /// raw PColor vfunc[3] `operator<(PColor const&)` @ `0x654504` 1:1.
    ///
    /// type compare 부분은 Rust 의 enum dispatch 로 type 동일 보장.
    /// 그 외엔 Property::lt (state) → Color::lt 의 ordering.
    pub fn lt_op(&self, other: &PColor) -> bool {
        // state 우선 비교
        if self.base.state != other.base.state {
            return self.base.lt_op(&other.base);
        }
        // state 동일 → Color::lt_struct
        self.color.lt_struct(&other.color)
    }

    /// raw PColor vfunc[4] `Clone() const` @ `0x6545a4` 1:1.
    ///
    /// ```asm
    /// new(0x28)              ; alloc 40B
    /// [new+0x8] = state      ; copy state from self
    /// vtable = 0x794018      ; PColor vtable
    /// [new+0x10] = Color body (16B memcpy)
    /// [new+0x20] = ColorEffect clone (refcount++)
    /// return new
    /// ```
    ///
    /// Returns: heap-alloc `*mut PColor` — caller responsible for dealloc.
    ///
    /// # Safety
    /// `self` 는 valid.
    pub unsafe fn clone_to_heap(&self) -> *mut PColor {
        let cloned = PColor {
            base: Property::new(self.base.state),
            color: Color::copy_ctor(&self.color),
        };
        Box::into_raw(Box::new(cloned))
    }

    /// raw `0x6541e8` 의 PColor + ControlBlock alloc + Attach 단계 (INSERT path).
    ///
    /// ```asm
    /// 0x654258: new(0x28)              ; PColor (40B)
    /// 0x654268: Property::Property(state)
    /// 0x65426c-0x654274: vtable = 0x794018
    /// 0x654278-0x65427c: 16B memcpy Color body
    /// 0x654280-0x654288: ColorEffect clone (refcount++) + store at +0x20
    /// 0x65428c-0x65429c: new(0x10) ControlBlock {obj: PColor*, refcount: 1}
    /// 0x6542a0-0x6542b0: bl __emplace_unique(tree, &key, &ctrl)
    /// (cleanup local ControlBlock)
    /// ```
    ///
    /// 본 Rust 는 raw 의 alloc + Attach 흐름의 핵심을 1:1 port: PColor heap-alloc +
    /// ControlBlock wrap (refcount=1).
    ///
    /// Returns: heap-alloc `*mut ControlBlock<Property>` — caller 가 PropertyBag::Attach
    /// 으로 넘겨야 함 (Attach 가 refcount++ 후 내부 SharePtr 저장).
    ///
    /// # Safety
    /// `color` 는 valid.
    pub unsafe fn create_attach_ctrl(
        state: u32,
        color: &Color,
    ) -> *mut crate::share_ptr::ControlBlock<Property> {
        // raw 0x654258-0x654288: alloc PColor + init
        let pcolor_ptr = PColor::create_raw(state, color);

        // raw 0x65428c-0x65429c: alloc 16B ControlBlock + init
        let ctrl = crate::share_ptr::ControlBlock {
            obj: pcolor_ptr as *mut Property, // PColor 는 Property 의 첫 field (sub-class)
            refcount: 1,
        };
        Box::into_raw(Box::new(ctrl))
    }
}

impl Drop for PColor {
    /// raw `~PColor()` 추정: Color 의 color_effect refcount-- (Color::Drop) + base no-op.
    ///
    /// Rust 의 자동 field drop 으로 모두 처리.
    fn drop(&mut self) {
        // Color 의 Drop 이 color_effect 의 ControlBlock refcount-- 처리.
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// PEnum (= ValueProperty<HatchStyle> 등의 u32 enum) — 16B
// ─────────────────────────────────────────────────────────────────────────────

/// raw 16B `PEnum` (Property sub-class for u32 enum values).
///
/// HatchBrush key `0x25a` (HatchStyle) 의 value type. SetHatchStyle (`0x18dcc0`) 가
/// `bl 0x6674b8(impl, key, &style, 1)` 으로 호출. 그 helper 가 PEnum 을 alloc.
///
/// ## Layout (확정 from `0x667528-0x66754c` of helper)
///
/// ```text
/// +0x00: vtable (8B)         (PEnum-specific @ 0x794728)
/// +0x08: state (u32)          (inherited from Property base)
/// +0x0c: value (u32)          (raw 의 Property._pad slot 을 활용)
/// ```
///
/// 총 16B (= raw `mov w0, #0x10`).
///
/// raw 의 PEnum 은 Property 의 base layout + +0x0c 의 pad 영역을 value 로 사용.
/// Rust 는 별도 struct 로 명시적으로 expose.
///
/// ## raw alloc + init (`0x667528-0x66754c`)
///
/// ```asm
/// mov  w0, #0x10
/// bl   __Znwm
/// mov  x23, x0
/// mov  x1, x22                  ; state
/// bl   Property::Property(State)
/// adrp x8, 0x794000
/// add  x8, x8, #0x728           ; PEnum vtable @ 0x794728
/// str  x8, [x23]
/// ldr  w8, [x20]                ; arg value (u32)
/// str  w8, [x23, #0xc]
/// ```
#[repr(C)]
pub struct PEnum {
    /// raw +0x00: vtable.
    pub vtable: *const u8,
    /// raw +0x08: state (u32 — inherited from Property base).
    pub state: u32,
    /// raw +0x0c: enum value (u32 — overlays Property's _pad slot).
    pub value: u32,
}

pub const PENUM_SIZE_BYTES: usize = 16;
pub const PENUM_ALIGN_BYTES: usize = 8;

const _: () = assert!(std::mem::size_of::<PEnum>() == PENUM_SIZE_BYTES);
const _: () = assert!(std::mem::align_of::<PEnum>() == PENUM_ALIGN_BYTES);

impl PEnum {
    /// raw PEnum 생성 (HatchBrush::SetHatchStyle 의 INSERT path 1:1).
    ///
    /// ```asm
    /// new(0x10)
    /// Property::Property(state)
    /// vtable = 0x794728 (PEnum vtable)
    /// value = arg (u32)
    /// ```
    pub fn new(state: u32, value: u32) -> Self {
        PEnum {
            vtable: ptr::null(),
            state,
            value,
        }
    }

    /// Heap alloc — raw `new(0x10)` 와 1:1.
    pub fn create_raw(state: u32, value: u32) -> *mut PEnum {
        Box::into_raw(Box::new(Self::new(state, value)))
    }

    /// 본 PEnum 을 ControlBlock<Property> 로 wrap — Attach 의 인자 형태.
    ///
    /// raw `0x667550-0x667564` 의 ControlBlock alloc + init 1:1:
    /// ```asm
    /// new(0x10)                ; ControlBlock
    /// stp x23, #1, [x0]        ; {obj: PEnum*, refcount: 1}
    /// ```
    pub fn create_attach_ctrl(
        state: u32,
        value: u32,
    ) -> *mut crate::share_ptr::ControlBlock<Property> {
        let penum_ptr = PEnum::create_raw(state, value);
        let ctrl = crate::share_ptr::ControlBlock {
            obj: penum_ptr as *mut Property,
            refcount: 1,
        };
        Box::into_raw(Box::new(ctrl))
    }

    /// raw `GetState` (inherited from Property).
    #[inline]
    pub fn get_state(&self) -> u32 {
        self.state
    }

    /// Get enum value.
    #[inline]
    pub fn get_value(&self) -> u32 {
        self.value
    }

    /// `SetState` (inherited).
    #[inline]
    pub fn set_state(&mut self, s: u32) {
        self.state = s;
    }

    /// `SetValue` — raw 의 UPDATE path 에서 in-place mutate.
    #[inline]
    pub fn set_value(&mut self, v: u32) {
        self.value = v;
    }

    /// vfunc[4] Clone (추정 layout 동일 logic with PColor).
    pub unsafe fn clone_to_heap(&self) -> *mut PEnum {
        Box::into_raw(Box::new(PEnum {
            vtable: ptr::null(),
            state: self.state,
            value: self.value,
        }))
    }
}

impl Drop for PEnum {
    /// raw `~PEnum()`: trivial (no owned heap; value is plain u32).
    fn drop(&mut self) {}
}

// ─────────────────────────────────────────────────────────────────────────────
// PFloat (= ValueProperty<float>) — 16B (Pen's thickness key 0x2bc, MiterLimit, etc.)
// ─────────────────────────────────────────────────────────────────────────────

/// raw 16B `PFloat` (Property sub-class for f32 values).
///
/// Pen::SetThickness (`0x173674`) 가 key `0x2bc` 와 함께 `bl 0x653cb4(impl, key, &width, 1)`
/// 호출. 그 helper 가 PFloat 을 alloc.
///
/// ## Layout (확정 from `0x653d24-0x653d48`)
///
/// ```text
/// +0x00: vtable (8B)         (PFloat vtable @ 0x793fb8)
/// +0x08: state (u32)
/// +0x0c: value (f32)          (PEnum 의 u32 와 동일 slot, 다른 type)
/// ```
///
/// 총 16B = `mov w0, #0x10`. PEnum 과 byte layout 동일, vtable + value type 만 다름.
#[repr(C)]
pub struct PFloat {
    /// raw +0x00: vtable.
    pub vtable: *const u8,
    /// raw +0x08: state.
    pub state: u32,
    /// raw +0x0c: f32 value.
    pub value: f32,
}

pub const PFLOAT_SIZE_BYTES: usize = 16;
pub const PFLOAT_ALIGN_BYTES: usize = 8;

const _: () = assert!(std::mem::size_of::<PFloat>() == PFLOAT_SIZE_BYTES);
const _: () = assert!(std::mem::align_of::<PFloat>() == PFLOAT_ALIGN_BYTES);

impl PFloat {
    /// raw PFloat 생성 (Pen::SetThickness 의 INSERT path 1:1).
    pub fn new(state: u32, value: f32) -> Self {
        PFloat {
            vtable: ptr::null(),
            state,
            value,
        }
    }

    pub fn create_raw(state: u32, value: f32) -> *mut PFloat {
        Box::into_raw(Box::new(Self::new(state, value)))
    }

    /// raw `0x653d4c-0x653d5c`: ControlBlock alloc + obj = PFloat.
    pub fn create_attach_ctrl(
        state: u32,
        value: f32,
    ) -> *mut crate::share_ptr::ControlBlock<Property> {
        let pf_ptr = PFloat::create_raw(state, value);
        let ctrl = crate::share_ptr::ControlBlock {
            obj: pf_ptr as *mut Property,
            refcount: 1,
        };
        Box::into_raw(Box::new(ctrl))
    }

    #[inline]
    pub fn get_state(&self) -> u32 {
        self.state
    }

    #[inline]
    pub fn get_value(&self) -> f32 {
        self.value
    }

    #[inline]
    pub fn set_value(&mut self, v: f32) {
        self.value = v;
    }

    pub unsafe fn clone_to_heap(&self) -> *mut PFloat {
        Box::into_raw(Box::new(PFloat {
            vtable: ptr::null(),
            state: self.state,
            value: self.value,
        }))
    }
}

impl Drop for PFloat {
    fn drop(&mut self) {}
}

// ─────────────────────────────────────────────────────────────────────────────
// PBool (= ValueProperty<bool>) — 16B (16w)
// ─────────────────────────────────────────────────────────────────────────────

/// raw 16B `PBool` (Property sub-class for u8 bool values).
///
/// GradientBrush key `0x261` / `0x265` 의 value type. GradientBrush::C2Ev 가
/// `bl 0x6475a4` 으로 호출 — bool helper.
///
/// ## Layout (확정 from `0x6475a4` 의 alloc + init)
///
/// ```text
/// +0x00: vtable (8B)
/// +0x08: state (u32)
/// +0x0c: value (u8) + 3B uninit pad
/// ```
///
/// 총 16B (raw `mov w0, #0x10` @ `0x647614`).
///
/// ## raw alloc + init (`0x647614-0x647634`)
///
/// ```asm
/// mov  w0, #0x10
/// bl   __Znwm
/// mov  x23, x0
/// mov  x1, x22                  ; state
/// bl   Property::Property(State)
/// adrp x8, ?
/// add  x8, x8, #?              ; PBool vtable (TBD — vtable address RE 후속)
/// str  x8, [x23]
/// ldrb w8, [x20]                ; arg byte
/// strb w8, [x23, #0xc]
/// ```
#[repr(C)]
pub struct PBool {
    /// raw +0x00: vtable.
    pub vtable: *const u8,
    /// raw +0x08: state (u32).
    pub state: u32,
    /// raw +0x0c: bool value (u8 — 1B). +0x0d..+0x10 uninit.
    pub value: u8,
    /// raw +0x0d..+0x10: 3B uninit padding (alignment).
    pub _pad: [u8; 3],
}

pub const PBOOL_SIZE_BYTES: usize = 16;
pub const PBOOL_ALIGN_BYTES: usize = 8;

const _: () = assert!(std::mem::size_of::<PBool>() == PBOOL_SIZE_BYTES);
const _: () = assert!(std::mem::align_of::<PBool>() == PBOOL_ALIGN_BYTES);

impl PBool {
    pub fn new(state: u32, value: bool) -> Self {
        PBool {
            vtable: ptr::null(),
            state,
            value: if value { 1 } else { 0 },
            _pad: [0u8; 3],
        }
    }

    pub fn create_raw(state: u32, value: bool) -> *mut PBool {
        Box::into_raw(Box::new(Self::new(state, value)))
    }

    /// raw `0x6475a4` 의 helper sequence (alloc 16B + Property::Property + vtable +
    /// value@+0xc + ControlBlock + bag.attach) 의 ControlBlock 부분만 노출.
    pub fn create_attach_ctrl(
        state: u32,
        value: bool,
    ) -> *mut crate::share_ptr::ControlBlock<Property> {
        let pb_ptr = PBool::create_raw(state, value);
        let ctrl = crate::share_ptr::ControlBlock {
            obj: pb_ptr as *mut Property,
            refcount: 1,
        };
        Box::into_raw(Box::new(ctrl))
    }

    #[inline]
    pub fn get_value(&self) -> bool {
        self.value != 0
    }

    #[inline]
    pub fn set_value(&mut self, v: bool) {
        self.value = if v { 1 } else { 0 };
    }
}

impl Drop for PBool {
    fn drop(&mut self) {}
}

// ─────────────────────────────────────────────────────────────────────────────
// PVec4 (= ValueProperty<16B blob>) — 32B (16w)
// ─────────────────────────────────────────────────────────────────────────────

/// raw 32B `PVec4` (Property sub-class for 16B value — vec4 of floats 또는 4B
/// rect 등).
///
/// GradientBrush key `0x262` (4x f32 = (0.5, 0.5, 0.5, 0.5)) / `0x263` (4x f32 =
/// (0, 0, 0, 0)) 의 value type. GradientBrush::C2Ev 가 `bl 0x656fb4` 으로 호출.
///
/// ## Layout (확정 from `0x656fb4` 의 alloc + init)
///
/// ```text
/// +0x00: vtable (8B)            (PVec4 vtable @ 0x794318)
/// +0x08: state (u32)             (inherited from Property)
/// +0x0c: value (16B blob)        (q0 = 4 floats 또는 16B raw)
/// +0x1c: 4B padding              (align to 32B)
/// ```
///
/// 총 32B (raw `mov w0, #0x20` @ `0x657024`).
///
/// ## raw alloc + init (`0x657024-0x657048`)
///
/// ```asm
/// mov  w0, #0x20
/// bl   __Znwm
/// mov  x23, x0
/// mov  x1, x22                  ; state
/// bl   Property::Property(State)
/// adrp x8, 0x794000
/// add  x8, x8, #0x318           ; PVec4 vtable @ 0x794318
/// str  x8, [x23]
/// ldr  q0, [x20]                ; load 16B from caller's stack arg
/// stur q0, [x23, #0xc]          ; store 16B at +0x0c
/// ```
#[repr(C)]
pub struct PVec4 {
    /// raw +0x00: vtable.
    pub vtable: *const u8,
    /// raw +0x08: state (u32).
    pub state: u32,
    /// raw +0x0c..+0x1c: 16B blob (4 floats 또는 4 i32 또는 OffsetRect 등).
    pub value: [u8; 16],
    /// raw +0x1c..+0x20: 4B align padding (uninit in raw — 32B alloc align).
    pub _pad: [u8; 4],
}

pub const PVEC4_SIZE_BYTES: usize = 32;
pub const PVEC4_ALIGN_BYTES: usize = 8;

const _: () = assert!(std::mem::size_of::<PVec4>() == PVEC4_SIZE_BYTES);
const _: () = assert!(std::mem::align_of::<PVec4>() == PVEC4_ALIGN_BYTES);

impl PVec4 {
    pub fn new(state: u32, value: [u8; 16]) -> Self {
        PVec4 {
            vtable: ptr::null(),
            state,
            value,
            _pad: [0u8; 4],
        }
    }

    /// 4 float values 로 PVec4 생성 (GradientBrush key `0x262` / `0x263` 의 패턴).
    pub fn from_f32x4(state: u32, v: [f32; 4]) -> Self {
        let mut bytes = [0u8; 16];
        bytes[0..4].copy_from_slice(&v[0].to_le_bytes());
        bytes[4..8].copy_from_slice(&v[1].to_le_bytes());
        bytes[8..12].copy_from_slice(&v[2].to_le_bytes());
        bytes[12..16].copy_from_slice(&v[3].to_le_bytes());
        Self::new(state, bytes)
    }

    pub fn create_raw(state: u32, value: [u8; 16]) -> *mut PVec4 {
        Box::into_raw(Box::new(Self::new(state, value)))
    }

    /// raw `0x656fb4` 의 helper 의 ControlBlock 부분.
    pub fn create_attach_ctrl(
        state: u32,
        value: [u8; 16],
    ) -> *mut crate::share_ptr::ControlBlock<Property> {
        let pv_ptr = PVec4::create_raw(state, value);
        let ctrl = crate::share_ptr::ControlBlock {
            obj: pv_ptr as *mut Property,
            refcount: 1,
        };
        Box::into_raw(Box::new(ctrl))
    }

    #[inline]
    pub fn as_f32x4(&self) -> [f32; 4] {
        let mut out = [0.0_f32; 4];
        for i in 0..4 {
            out[i] = f32::from_le_bytes([
                self.value[i * 4],
                self.value[i * 4 + 1],
                self.value[i * 4 + 2],
                self.value[i * 4 + 3],
            ]);
        }
        out
    }
}

impl Drop for PVec4 {
    fn drop(&mut self) {}
}

// ─────────────────────────────────────────────────────────────────────────────
// PStops (= ValueProperty<GradientStops>) — 40B (16y)
// ─────────────────────────────────────────────────────────────────────────────

/// raw 40B `PStops` (Property sub-class for `Hnc::Shape::GradientStops` =
/// std::vector<SharePtr<GradientStop>> 24B).
///
/// GradientBrush key `0x266` 의 value type. raw `0x655508` 의 alloc target.
///
/// ## Layout (확정 from `0x655578-0x6555a0` of helper `0x655508`)
///
/// ```text
/// +0x00: vtable (8B)                  (= 0x794138)
/// +0x08: state (u32) + 4B pad         (Property base)
/// +0x10: GradientStopsVec (24B)       (begin/end/cap_end)
/// ```
///
/// 총 40B (raw `mov w0, #0x28` @ `0x655578`).
///
/// ## raw alloc + init (`0x655578-0x6555a0`)
///
/// ```asm
/// 0x655578: mov  w0, #0x28          ; alloc 40B
/// 0x65557c: bl   __Znwm
/// 0x655580: mov  x23, x0
/// 0x655584: mov  x1, x22            ; state
/// 0x655588: bl   Property::Property(State)
/// 0x65558c: adrp x8, 0x794000
/// 0x655590: add  x8, x8, #0x138     ; PStops vtable @ 0x794138
/// 0x655594: mov  x0, x23
/// 0x655598: str  x8, [x0], #0x10    ; *x0 = vtable, x0 += 0x10 (= &body)
/// 0x65559c: mov  x1, x20            ; src GradientStopsVec ptr
/// 0x6555a0: bl   0x62fd78           ; GradientStops::CopyFrom (deep clone)
/// ```
#[repr(C)]
pub struct PStops {
    /// raw +0x00: vtable.
    pub vtable: *const u8,
    /// raw +0x08: state (u32 — inherited).
    pub state: u32,
    /// raw +0x0c..+0x10: 4B pad (Property base 의 _pad).
    pub _pad: u32,
    /// raw +0x10..+0x28: GradientStopsVec (24B = begin/end/cap_end).
    pub stops: crate::gradient_stop::GradientStopsVec,
}

pub const PSTOPS_SIZE_BYTES: usize = 40;
pub const PSTOPS_ALIGN_BYTES: usize = 8;

const _: () = assert!(std::mem::size_of::<PStops>() == PSTOPS_SIZE_BYTES);
const _: () = assert!(std::mem::align_of::<PStops>() == PSTOPS_ALIGN_BYTES);

impl PStops {
    /// raw `0x655578-0x6555a0` 1:1 — alloc 40B + Property base + GradientStops
    /// deep clone via `clone_deep()`.
    ///
    /// # Safety
    /// `src_vec` 은 valid `&GradientStopsVec`.
    pub unsafe fn new(state: u32, src_vec: &crate::gradient_stop::GradientStopsVec) -> Self {
        PStops {
            vtable: ptr::null(),
            state,
            _pad: 0,
            // raw `bl 0x62fd78`: GradientStops::CopyFrom — deep clone with
            // refcount++ on each element ctrl.
            stops: src_vec.clone_deep(),
        }
    }

    pub unsafe fn create_raw(
        state: u32,
        src_vec: &crate::gradient_stop::GradientStopsVec,
    ) -> *mut PStops {
        Box::into_raw(Box::new(Self::new(state, src_vec)))
    }

    /// raw `0x655508` 의 ControlBlock 부분 (alloc 16B + obj + strong=1).
    pub unsafe fn create_attach_ctrl(
        state: u32,
        src_vec: &crate::gradient_stop::GradientStopsVec,
    ) -> *mut crate::share_ptr::ControlBlock<Property> {
        let ps_ptr = PStops::create_raw(state, src_vec);
        let ctrl = crate::share_ptr::ControlBlock {
            obj: ps_ptr as *mut Property,
            refcount: 1,
        };
        Box::into_raw(Box::new(ctrl))
    }

    /// Number of stops in the inner vector.
    #[inline]
    pub fn len(&self) -> usize {
        self.stops.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.stops.is_empty()
    }
}

impl Drop for PStops {
    /// raw `~PStops()` 추정: inner GradientStopsVec 의 drop — 모든 ctrl release +
    /// buffer dealloc. Rust 의 자동 field drop 으로 처리.
    fn drop(&mut self) {
        // GradientStopsVec 의 Drop 이 자동 호출.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn property_raw_layout() {
        assert_eq!(std::mem::size_of::<Property>(), 16);
        assert_eq!(std::mem::align_of::<Property>(), 8);
    }

    #[test]
    fn property_field_offsets() {
        let p = Property::new(0);
        let base = &p as *const _ as usize;
        assert_eq!(&p.vtable as *const _ as usize - base, 0x00);
        assert_eq!(&p.state as *const _ as usize - base, 0x08);
    }

    #[test]
    fn property_get_set_state() {
        let mut p = Property::new(0);
        assert_eq!(p.get_state(), 0);
        p.set_state(2);
        assert_eq!(p.get_state(), 2);
    }

    #[test]
    fn property_is_enable_matches_raw_semantic() {
        // state != 0 AND state != 3 → enable
        let p0 = Property::new(0);
        let p1 = Property::new(1);
        let p2 = Property::new(2);
        let p3 = Property::new(3);
        let p4 = Property::new(4);
        assert!(!p0.is_enable());
        assert!(p1.is_enable());
        assert!(p2.is_enable());
        assert!(!p3.is_enable());
        assert!(p4.is_enable());
    }

    #[test]
    fn property_eq_special_case_1_eq_2() {
        // raw special: state 1 == state 2 (and vice-versa)
        let p1 = Property::new(1);
        let p2 = Property::new(2);
        assert!(p1.eq_op(&p2));
        assert!(p2.eq_op(&p1));
    }

    #[test]
    fn property_eq_same_state() {
        let p = Property::new(3);
        let q = Property::new(3);
        assert!(p.eq_op(&q));
    }

    #[test]
    fn property_eq_different_state_normal() {
        let p0 = Property::new(0);
        let p3 = Property::new(3);
        assert!(!p0.eq_op(&p3));
        assert!(!p3.eq_op(&p0));
    }

    #[test]
    fn property_lt_state_order() {
        let p0 = Property::new(0);
        let p2 = Property::new(2);
        assert!(p0.lt_op(&p2));
        assert!(!p2.lt_op(&p0));
        assert!(!p0.lt_op(&p0));
    }

    #[test]
    fn pcolor_raw_layout() {
        assert_eq!(std::mem::size_of::<PColor>(), 40);
        assert_eq!(std::mem::align_of::<PColor>(), 8);
    }

    #[test]
    fn pcolor_field_offsets_match_raw() {
        unsafe {
            let src = Color::from_rgb(255, 0, 0, ptr::null_mut());
            let pc = PColor::new(state::ENABLED_EXPLICIT, &src);
            let base = &pc as *const _ as usize;
            assert_eq!(&pc.base as *const _ as usize - base, 0x00);
            assert_eq!(&pc.color as *const _ as usize - base, 0x10);
            assert_eq!(&pc.base.vtable as *const _ as usize - base, 0x00);
            assert_eq!(&pc.base.state as *const _ as usize - base, 0x08);
        }
    }

    #[test]
    fn pcolor_state_passed_via_property_ctor() {
        unsafe {
            let src = Color::from_rgb(1, 2, 3, ptr::null_mut());
            let pc = PColor::new(state::ENABLED_EXPLICIT, &src);
            assert_eq!(pc.get_state(), state::ENABLED_EXPLICIT);
            assert!(pc.base.is_enable());
        }
    }

    #[test]
    fn pcolor_embeds_color_at_offset_0x10() {
        unsafe {
            let src = Color::from_rgb(0xAB, 0xCD, 0xEF, ptr::null_mut());
            let pc = PColor::new(0, &src);
            let rgb = pc.color().get_rgb();
            assert_eq!(rgb.r, 0xAB);
            assert_eq!(rgb.g, 0xCD);
            assert_eq!(rgb.b, 0xEF);
        }
    }

    #[test]
    fn pcolor_create_raw_returns_heap_box() {
        unsafe {
            let src = Color::from_rgb(0, 0, 0, ptr::null_mut());
            let p = PColor::create_raw(0, &src);
            assert!(!p.is_null());
            // Cleanup
            drop(Box::from_raw(p));
        }
    }

    #[test]
    fn pcolor_set_state_round_trip() {
        unsafe {
            let src = Color::from_rgb(0, 0, 0, ptr::null_mut());
            let mut pc = PColor::new(0, &src);
            pc.set_state(2);
            assert_eq!(pc.get_state(), 2);
        }
    }

    #[test]
    fn pcolor_eq_same_state_same_color_true() {
        unsafe {
            let c1 = Color::from_rgb(1, 2, 3, ptr::null_mut());
            let c2 = Color::from_rgb(1, 2, 3, ptr::null_mut());
            let pc1 = PColor::new(2, &c1);
            let pc2 = PColor::new(2, &c2);
            assert!(pc1.eq_op(&pc2));
        }
    }

    #[test]
    fn pcolor_eq_diff_color_false() {
        unsafe {
            let c1 = Color::from_rgb(1, 2, 3, ptr::null_mut());
            let c2 = Color::from_rgb(99, 88, 77, ptr::null_mut());
            let pc1 = PColor::new(2, &c1);
            let pc2 = PColor::new(2, &c2);
            assert!(!pc1.eq_op(&pc2));
        }
    }

    #[test]
    fn pcolor_eq_special_state_1_eq_2() {
        unsafe {
            // Property::eq_op 의 1 ↔ 2 special case 가 PColor 에도 전파
            let c = Color::from_rgb(5, 5, 5, ptr::null_mut());
            let c2 = Color::from_rgb(5, 5, 5, ptr::null_mut());
            let pc_s1 = PColor::new(1, &c);
            let pc_s2 = PColor::new(2, &c2);
            assert!(pc_s1.eq_op(&pc_s2)); // state 1 == state 2 (special) + Color eq
        }
    }

    #[test]
    fn pcolor_clone_to_heap_yields_independent_copy() {
        unsafe {
            let src = Color::from_rgb(10, 20, 30, ptr::null_mut());
            let pc = PColor::new(state::ENABLED_EXPLICIT, &src);
            let cloned = pc.clone_to_heap();
            assert!(!cloned.is_null());
            assert_ne!(cloned as usize, &pc as *const _ as usize);
            assert_eq!((*cloned).get_state(), pc.get_state());
            // Color body 동일
            assert_eq!((*cloned).color.get_rgb().r, 10);
            assert_eq!((*cloned).color.get_rgb().g, 20);
            assert_eq!((*cloned).color.get_rgb().b, 30);
            // Cleanup
            drop(Box::from_raw(cloned));
        }
    }

    #[test]
    fn pcolor_attach_to_bag_e2e() {
        // SolidBrush::SetColor 의 INSERT path 의 end-to-end:
        // 1. create_attach_ctrl(state, color) → ControlBlock<Property>*
        // 2. PropertyBag.attach(key=0x259, ctrl)
        // 3. bag.contains(key) == true, get_state == state
        use crate::property_bag::PropertyBag;
        use crate::property_key::PropertyKey;
        unsafe {
            let mut bag = PropertyBag::new(false);
            let key = PropertyKey::from_int(0x259); // SolidBrush color key
            let color = Color::from_rgb(0xAB, 0xCD, 0xEF, ptr::null_mut());
            let ctrl = PColor::create_attach_ctrl(state::ENABLED_EXPLICIT, &color);
            assert!(!ctrl.is_null());
            assert_eq!((*ctrl).refcount, 1);

            // Attach into bag
            let old = bag.attach(&key, ctrl);
            assert!(old.is_null(), "no existing key");
            assert!(bag.contains(&key));
            assert_eq!(bag.get_state(&key), state::ENABLED_EXPLICIT);

            // Verify Color value is preserved through the tree node
            // Read node directly via find_equal
            if let Some(impl_ref) = bag.impl_ref() {
                match impl_ref.find_equal(&key) {
                    Ok(node) => {
                        let stored_ctrl = (*node).value;
                        assert!(!stored_ctrl.is_null());
                        let prop_ptr = (*stored_ctrl).obj;
                        assert!(!prop_ptr.is_null());
                        // PColor is the concrete type — cast back
                        let pc = prop_ptr as *mut PColor;
                        let rgb = (*pc).color.get_rgb();
                        assert_eq!(rgb.r, 0xAB);
                        assert_eq!(rgb.g, 0xCD);
                        assert_eq!(rgb.b, 0xEF);
                    }
                    Err(_) => panic!("key should be found"),
                }
            } else {
                panic!("bag.impl_ref should be Some");
            }
        }
    }

    // ─── PEnum tests ───

    #[test]
    fn penum_raw_layout() {
        assert_eq!(std::mem::size_of::<PEnum>(), 16);
        assert_eq!(std::mem::align_of::<PEnum>(), 8);
    }

    #[test]
    fn penum_field_offsets_match_raw() {
        let pe = PEnum::new(2, 0x1234);
        let base = &pe as *const _ as usize;
        assert_eq!(&pe.vtable as *const _ as usize - base, 0x00);
        assert_eq!(&pe.state as *const _ as usize - base, 0x08);
        assert_eq!(&pe.value as *const _ as usize - base, 0x0c);
    }

    #[test]
    fn penum_state_and_value() {
        let pe = PEnum::new(2, 42);
        assert_eq!(pe.get_state(), 2);
        assert_eq!(pe.get_value(), 42);
    }

    #[test]
    fn penum_set_value() {
        let mut pe = PEnum::new(1, 0);
        pe.set_value(99);
        assert_eq!(pe.value, 99);
    }

    #[test]
    fn penum_clone_to_heap_independent() {
        unsafe {
            let pe = PEnum::new(2, 7);
            let cloned = pe.clone_to_heap();
            assert!(!cloned.is_null());
            assert_eq!((*cloned).get_state(), 2);
            assert_eq!((*cloned).get_value(), 7);
            drop(Box::from_raw(cloned));
        }
    }

    #[test]
    fn penum_attach_to_bag_e2e() {
        // HatchBrush::SetHatchStyle 의 INSERT path end-to-end
        use crate::property_bag::PropertyBag;
        use crate::property_key::PropertyKey;
        unsafe {
            let mut bag = PropertyBag::new(false);
            let key = PropertyKey::from_int(0x25a); // HatchStyle key
            let ctrl = PEnum::create_attach_ctrl(state::ENABLED_EXPLICIT, 5);
            let old = bag.attach(&key, ctrl);
            assert!(old.is_null());
            assert!(bag.contains(&key));
            assert_eq!(bag.get_state(&key), state::ENABLED_EXPLICIT);

            // Verify value preserved through bag → SharePtr → PEnum
            if let Some(impl_ref) = bag.impl_ref() {
                let r = impl_ref.find_equal(&key).expect("found");
                let stored_ctrl = (*r).value;
                let prop = (*stored_ctrl).obj;
                let pe = prop as *mut PEnum;
                assert_eq!((*pe).get_value(), 5);
            }
        }
    }

    #[test]
    fn hatch_brush_three_keys_e2e() {
        // HatchBrush 의 3 setters (HatchStyle/ForeColor/BackColor) 를 1 PropertyBag 에 attach.
        // raw HatchBrush::HatchBrush(style, fore, back) 의 행동 mimics.
        use crate::property_bag::PropertyBag;
        use crate::property_key::PropertyKey;
        unsafe {
            let mut bag = PropertyBag::new(false);
            let s = state::ENABLED_EXPLICIT;

            // HatchStyle = 3 (= "DiagonalCross" 등)
            let style_key = PropertyKey::from_int(0x25a);
            bag.attach(&style_key, PEnum::create_attach_ctrl(s, 3));

            // ForeColor = RGB red
            let fore_key = PropertyKey::from_int(0x25b);
            let red = Color::from_rgb(0xFF, 0, 0, ptr::null_mut());
            bag.attach(&fore_key, PColor::create_attach_ctrl(s, &red));

            // BackColor = RGB blue
            let back_key = PropertyKey::from_int(0x25c);
            let blue = Color::from_rgb(0, 0, 0xFF, ptr::null_mut());
            bag.attach(&back_key, PColor::create_attach_ctrl(s, &blue));

            assert_eq!(bag.impl_ref().unwrap().tree.size, 3);

            // Verify all 3 retrievable
            assert!(bag.contains(&style_key));
            assert!(bag.contains(&fore_key));
            assert!(bag.contains(&back_key));

            let impl_ref = bag.impl_ref().unwrap();

            // HatchStyle value check
            let n = impl_ref.find_equal(&style_key).expect("style");
            let pe = (*(*n).value).obj as *mut PEnum;
            assert_eq!((*pe).get_value(), 3);

            // ForeColor body check
            let n = impl_ref.find_equal(&fore_key).expect("fore");
            let pc = (*(*n).value).obj as *mut PColor;
            let rgb = (*pc).color.get_rgb();
            assert_eq!(rgb.r, 0xFF);
            assert_eq!(rgb.g, 0);

            // BackColor body check
            let n = impl_ref.find_equal(&back_key).expect("back");
            let pc = (*(*n).value).obj as *mut PColor;
            let rgb = (*pc).color.get_rgb();
            assert_eq!(rgb.b, 0xFF);
            assert_eq!(rgb.r, 0);
        }
    }

    // ─── PFloat tests ───

    #[test]
    fn pfloat_raw_layout() {
        assert_eq!(std::mem::size_of::<PFloat>(), 16);
        assert_eq!(std::mem::align_of::<PFloat>(), 8);
    }

    #[test]
    fn pfloat_field_offsets() {
        let pf = PFloat::new(2, 1.5);
        let base = &pf as *const _ as usize;
        assert_eq!(&pf.vtable as *const _ as usize - base, 0x00);
        assert_eq!(&pf.state as *const _ as usize - base, 0x08);
        assert_eq!(&pf.value as *const _ as usize - base, 0x0c);
    }

    #[test]
    fn pfloat_value_round_trip() {
        let mut pf = PFloat::new(2, 0.75);
        assert_eq!(pf.get_value(), 0.75);
        pf.set_value(2.5);
        assert_eq!(pf.value, 2.5);
    }

    #[test]
    fn pen_thickness_e2e() {
        // Pen::SetThickness 의 INSERT path end-to-end
        use crate::property_bag::PropertyBag;
        use crate::property_key::PropertyKey;
        unsafe {
            let mut bag = PropertyBag::new(false);
            let key = PropertyKey::from_int(0x2bc); // Thickness key (per Pen::SetThickness disasm)
            let ctrl = PFloat::create_attach_ctrl(state::ENABLED_EXPLICIT, 1.5);
            let old = bag.attach(&key, ctrl);
            assert!(old.is_null());

            let r = bag.impl_ref().unwrap().find_equal(&key).expect("found");
            let pf = (*(*r).value).obj as *mut PFloat;
            assert!((*pf).get_value() - 1.5 < f32::EPSILON);
            assert!((*pf).get_value() - 1.5 > -f32::EPSILON);
        }
    }

    #[test]
    fn property_bag_backed_solid_brush_demo() {
        // SolidBrush = vtable + PropertyBag (16B raw layout).
        // 본 test 는 PropertyBag-backed Brush 의 functional equivalence 검증.
        //
        // 1. Create empty PropertyBag (= SolidBrush 의 +0x08 field)
        // 2. SetColor 와 동등 (PColor attach @ 0x259)
        // 3. GetColor 와 동등 (find_equal @ 0x259 → PColor.color)
        use crate::property_bag::PropertyBag;
        use crate::property_key::PropertyKey;
        unsafe {
            let mut bag = PropertyBag::new(false);
            let color_key = PropertyKey::from_int(0x259);

            // SetColor(red)
            let red = Color::from_rgb(0xFF, 0, 0, ptr::null_mut());
            bag.attach(
                &color_key,
                PColor::create_attach_ctrl(state::ENABLED_EXPLICIT, &red),
            );

            // GetColor (read back)
            let r = bag
                .impl_ref()
                .unwrap()
                .find_equal(&color_key)
                .expect("color present");
            let pc = (*(*r).value).obj as *mut PColor;
            assert_eq!((*pc).color.get_rgb().r, 0xFF);

            // SetColor again with different color → REPLACE path
            let blue = Color::from_rgb(0, 0, 0xFF, ptr::null_mut());
            let old = bag.attach(
                &color_key,
                PColor::create_attach_ctrl(state::ENABLED_EXPLICIT, &blue),
            );
            assert!(!old.is_null(), "REPLACE returns old SharePtr");

            // Verify new color
            let r = bag
                .impl_ref()
                .unwrap()
                .find_equal(&color_key)
                .expect("color present");
            let pc = (*(*r).value).obj as *mut PColor;
            assert_eq!((*pc).color.get_rgb().b, 0xFF);
            assert_eq!((*pc).color.get_rgb().r, 0);

            // tree size still 1 (replace, not append)
            assert_eq!(bag.impl_ref().unwrap().tree.size, 1);
        }
    }

    #[test]
    fn pcolor_lt_state_order() {
        unsafe {
            let c = Color::from_rgb(0, 0, 0, ptr::null_mut());
            let c2 = Color::from_rgb(0, 0, 0, ptr::null_mut());
            let pc_low = PColor::new(0, &c);
            let pc_high = PColor::new(2, &c2);
            assert!(pc_low.lt_op(&pc_high));
            assert!(!pc_high.lt_op(&pc_low));
        }
    }

    // =========================================================================
    // 16w: PBool / PVec4 tests
    // =========================================================================

    #[test]
    fn pbool_raw_16b_layout() {
        // raw `0x647614: mov w0, #0x10` — PBool is 16B / 8B align
        assert_eq!(std::mem::size_of::<PBool>(), 16);
        assert_eq!(std::mem::align_of::<PBool>(), 8);
    }

    #[test]
    fn pbool_field_offsets_match_raw() {
        // raw: vtable@0x00, state@0x08, value(u8)@0x0c
        let p = PBool::new(2, true);
        let base = &p as *const _ as usize;
        assert_eq!(&p.vtable as *const _ as usize - base, 0x00);
        assert_eq!(&p.state as *const _ as usize - base, 0x08);
        assert_eq!(&p.value as *const _ as usize - base, 0x0c);
    }

    #[test]
    fn pbool_value_round_trip() {
        let p_true = PBool::new(2, true);
        assert_eq!(p_true.get_value(), true);
        assert_eq!(p_true.value, 1);
        let p_false = PBool::new(2, false);
        assert_eq!(p_false.get_value(), false);
        assert_eq!(p_false.value, 0);
    }

    #[test]
    fn pvec4_raw_32b_layout() {
        // raw `0x657024: mov w0, #0x20` — PVec4 is 32B / 8B align
        assert_eq!(std::mem::size_of::<PVec4>(), 32);
        assert_eq!(std::mem::align_of::<PVec4>(), 8);
    }

    #[test]
    fn pvec4_field_offsets_match_raw() {
        // raw: vtable@0x00, state@0x08, value(16B)@0x0c, pad@0x1c
        let p = PVec4::from_f32x4(2, [0.5, 0.5, 0.5, 0.5]);
        let base = &p as *const _ as usize;
        assert_eq!(&p.vtable as *const _ as usize - base, 0x00);
        assert_eq!(&p.state as *const _ as usize - base, 0x08);
        assert_eq!(&p.value as *const _ as usize - base, 0x0c);
        assert_eq!(&p._pad as *const _ as usize - base, 0x1c);
    }

    #[test]
    fn pvec4_from_f32x4_round_trip() {
        // raw `movi.4s v0, #0x3f, lsl #24` = (0.5, 0.5, 0.5, 0.5)
        let p = PVec4::from_f32x4(2, [0.5, 0.5, 0.5, 0.5]);
        let v = p.as_f32x4();
        assert_eq!(v, [0.5, 0.5, 0.5, 0.5]);

        // raw `stp xzr, xzr, [sp]` = (0, 0, 0, 0)
        let p0 = PVec4::from_f32x4(2, [0.0, 0.0, 0.0, 0.0]);
        assert_eq!(p0.as_f32x4(), [0.0, 0.0, 0.0, 0.0]);
        // value bytes 모두 0
        assert_eq!(p0.value, [0u8; 16]);
    }

    #[test]
    fn pvec4_movi_4s_pattern_matches_raw() {
        // raw `movi.4s v0, #0x3f, lsl #24` produces 4 lanes of 0x3F000000 (= 0.5).
        let p = PVec4::from_f32x4(2, [0.5; 4]);
        // value bytes per lane: 0x00, 0x00, 0x00, 0x3F (little-endian)
        for lane in 0..4 {
            assert_eq!(p.value[lane * 4 + 0], 0x00);
            assert_eq!(p.value[lane * 4 + 1], 0x00);
            assert_eq!(p.value[lane * 4 + 2], 0x00);
            assert_eq!(p.value[lane * 4 + 3], 0x3F);
        }
    }
}

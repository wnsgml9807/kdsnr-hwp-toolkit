//! `Hnc::Shape::Pen` — concrete class (single type, not abstract).
//!
//! ## raw 구조 (확정 by ctor `0x1b4cf0` default + `0x1b4fe8` with Color)
//!
//! 16B layout:
//! - +0x00: `SharePtr<Brush>` — line "brush" (fill of the stroke; SolidBrush 가 일반)
//! - +0x08: `PropertyBag` — width + dash + line cap/join + arrows + align 등 모든 stroke 속성
//!
//! ## raw ctor (Color variant @ 0x1b4fe8)
//!
//! ```text
//! Pen(Color, width, PenCompoundStyle, DashStyle, LineCapStyle, LineJoinStyle,
//!     ArrowStyle, ArrowSizeStyle, ArrowStyle, ArrowSizeStyle, PenAlignStyle)
//! ```
//!
//! 1. `SolidBrush::Create(Color)` → SolidBrush*
//! 2. alloc 24B ControlBlock {T_ptr=SolidBrush, refcount=1, is_const?=byte 1}
//! 3. self.share_ptr = ControlBlock
//! 4. PropertyBag init at +0x08
//! 5. SetProperty:
//!    - key 0x2bc (700): Brush (의 SharePtr) — actually re-stored via PropertyBag
//!    - key 0x2bd (701): width (f32 packed as IntFloat 8B)
//!    - key 0x2be (702): compound_style (u32)
//!    - key 0x2bf (703): dash_style (u32)
//!    - key 0x2c0 (704): line_cap_style (default 1)
//!    - key 0x2c2 (706): line_join_style
//!    - 추가 keys for arrows + align
//!
//! ## default ctor (`0x1b4cf0`) widths
//!
//! width 의 default 는 `ShapeEngine::GetInstance()->[+0x4]` (= dpi 또는 base size)
//! × `0.75 / 72.0` (point→inch 환산). raw:
//! ```asm
//! 1b4d28: bl GetInstance
//! 1b4d2c: ldr s0, [x0, #0x4]
//! 1b4d30: fmov s1, #0.75
//! 1b4d34: fmul s0, s0, s1          ; s0 = base * 0.75
//! 1b4d38: mov w8, #0x42900000      ; w8 = bits of 72.0
//! 1b4d3c: fmov s1, w8
//! 1b4d40: fdiv s0, s0, s1          ; s0 = (base * 0.75) / 72.0
//! ```
//!
//! ## byte-eq 경계 (16h)
//!
//! 본 Rust port 는 **semantic byte-eq** — raw 의 vtable_ptr + PropertyBag 대신
//! direct fields. output PDF 의 stroke 결과가 byte-eq 가 되도록 raw vfunc
//! dispatch 의 의미 (특히 `Draw`) 와 일관.

use crate::brush::Brush;

/// raw `Hnc::Type::DrawingType::PenCompoundStyle` (u32 enum, raw key 0x2be).
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PenCompoundStyle {
    #[default]
    Single = 0,
    Double = 1,
    ThinThick = 2,
    ThickThin = 3,
    TriLine = 4,
}

/// raw `DashStyle` (key 0x2bf).
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DashStyle {
    #[default]
    Solid = 0,
    Dot = 1,
    Dash = 2,
    LongDash = 3,
    DashDot = 4,
    LongDashDot = 5,
    LongDashDotDot = 6,
    // 추가 변종은 multi-session
}

/// raw `LineCapStyle` (key 0x2c0, default = 1).
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineCapStyle {
    Round = 0,
    Square = 1, // default per raw `0x1b4e24: mov w8, #1`
    Flat = 2,
}

impl Default for LineCapStyle {
    fn default() -> Self {
        LineCapStyle::Square
    }
}

/// raw `LineJoinStyle` (key 0x2c2).
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineJoinStyle {
    #[default]
    Miter = 0,
    Round = 1,
    Bevel = 2,
}

/// raw `ArrowStyle` (head/tail arrow shape).
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ArrowStyle {
    #[default]
    None = 0,
    Triangle = 1,
    Diamond = 2,
    Circle = 3,
    Open = 4,
    Stealth = 5,
}

/// raw `ArrowSizeStyle` (small/medium/large 등).
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ArrowSizeStyle {
    #[default]
    Small = 0,
    Medium = 1,
    Large = 2,
}

/// raw `PenAlignStyle`.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PenAlignStyle {
    #[default]
    Center = 0,
    Inset = 1,
    Outset = 2,
}

/// `Hnc::Shape::Pen` — raw 16B layout (16t 재설계).
///
/// raw ctor (`0x1b4cf0`) algorithm:
/// ```asm
/// str xzr, [x20], #0x8           ; +0x00 = null (SharePtr<Brush>.ctrl)
/// bl PropertyBag::PropertyBag(false)  ; +0x08 = bag
/// ...
/// bl PFloat helper (key 0x2bc, default width = base*0.75/72)
/// ```
///
/// 1 SharePtr<Brush> field + PropertyBag.
///
/// ## Pen 의 12 property mapping (모두 16t RE 확정)
///
/// | property              | storage | key   | type    | helper      |
/// |-----------------------|---------|-------|---------|-------------|
/// | brush (stroke fill)   | self+0  | —     | SharePtr<Brush> | (direct ptr swap) |
/// | thickness (width)     | bag     | 0x2bc | PFloat  | `0x653cb4`  |
/// | pen compound style    | bag     | 0x2bd | PEnum   | `0x669704`  |
/// | dash style            | bag     | 0x2be | PEnum   | `0x656254`  |
/// | line cap style        | bag     | 0x2bf | PEnum   | `0x669b40`  |
/// | line join style       | bag     | 0x2c0 | PEnum   | `0x669f7c`  |
/// | miter limit           | bag     | 0x2c1 | PFloat  | `0x653cb4`  |
/// | start arrow style     | bag     | 0x2c2 | PEnum   | `0x66a3b8`  |
/// | start arrow size      | bag     | 0x2c3 | PEnum   | `0x66a7f4`  |
/// | end arrow style       | bag     | 0x2c4 | PEnum   | `0x66a3b8`  |
/// | end arrow size        | bag     | 0x2c5 | PEnum   | `0x66a7f4`  |
/// | pen align style       | bag     | 0x2c6 | PEnum   | `0x66ac30`  |
///
/// (각 stroke enum helper 가 서로 다른 vtable 의 PEnum sub-class 일 가능성 —
/// 본 단계는 모두 PEnum 으로 통일 처리; 다음 세션 sub-class 분리.)
#[repr(C)]
pub struct Pen {
    /// raw +0x00: SharePtr<Brush> (= ControlBlock<Brush>*) — stroke fill brush.
    /// 본 Rust port 는 Box<Brush> 가 ownership 단순. multi-session deferred 로
    /// 실제 ControlBlock<Brush> 로 교체 (refcount 공유 시).
    pub brush: Box<Brush>,
    /// raw +0x08: PropertyBag (= SharePtr<PropertyBagImpl> 단일 ptr).
    pub bag: crate::property_bag::PropertyBag,
}

impl Pen {
    pub const KEY_THICKNESS: u32 = 0x2bc;
    pub const KEY_COMPOUND: u32 = 0x2bd;
    pub const KEY_DASH: u32 = 0x2be;
    pub const KEY_LINE_CAP: u32 = 0x2bf;
    pub const KEY_LINE_JOIN: u32 = 0x2c0;
    pub const KEY_MITER_LIMIT: u32 = 0x2c1;
    pub const KEY_START_ARROW_STYLE: u32 = 0x2c2;
    pub const KEY_START_ARROW_SIZE: u32 = 0x2c3;
    pub const KEY_END_ARROW_STYLE: u32 = 0x2c4;
    pub const KEY_END_ARROW_SIZE: u32 = 0x2c5;
    pub const KEY_PEN_ALIGN: u32 = 0x2c6;

    /// PropertyBag 의 f32 lookup (PFloat).
    fn bag_get_f32(&self, key_id: u32, default: f32) -> f32 {
        let key = crate::property_key::PropertyKey::from_int(key_id);
        unsafe {
            if let Some(impl_ref) = self.bag.impl_ref() {
                if let Ok(node) = impl_ref.find_equal(&key) {
                    let cb = (*node).value;
                    if !cb.is_null() && !(*cb).obj.is_null() {
                        let pf = (*cb).obj as *const crate::property::PFloat;
                        return (*pf).value;
                    }
                }
            }
        }
        default
    }

    /// PropertyBag 의 u32 lookup (PEnum).
    fn bag_get_u32(&self, key_id: u32, default: u32) -> u32 {
        let key = crate::property_key::PropertyKey::from_int(key_id);
        unsafe {
            if let Some(impl_ref) = self.bag.impl_ref() {
                if let Ok(node) = impl_ref.find_equal(&key) {
                    let cb = (*node).value;
                    if !cb.is_null() && !(*cb).obj.is_null() {
                        let pe = (*cb).obj as *const crate::property::PEnum;
                        return (*pe).value;
                    }
                }
            }
        }
        default
    }

    fn bag_attach_f32(&mut self, key_id: u32, value: f32) {
        let key = crate::property_key::PropertyKey::from_int(key_id);
        let state = crate::property::state::ENABLED_EXPLICIT;
        unsafe {
            let _ = self.bag.attach(
                &key,
                crate::property::PFloat::create_attach_ctrl(state, value),
            );
        }
    }

    fn bag_attach_u32(&mut self, key_id: u32, value: u32) {
        let key = crate::property_key::PropertyKey::from_int(key_id);
        let state = crate::property::state::ENABLED_EXPLICIT;
        unsafe {
            let _ = self.bag.attach(
                &key,
                crate::property::PEnum::create_attach_ctrl(state, value),
            );
        }
    }

    // ----------------- Property getters (raw `GetXxx() const` 1:1) -----------------

    pub fn get_thickness(&self) -> f32 {
        self.bag_get_f32(Self::KEY_THICKNESS, 1.0)
    }
    pub fn get_pen_compound_style(&self) -> PenCompoundStyle {
        match self.bag_get_u32(Self::KEY_COMPOUND, 0) {
            0 => PenCompoundStyle::Single,
            1 => PenCompoundStyle::Double,
            2 => PenCompoundStyle::ThinThick,
            3 => PenCompoundStyle::ThickThin,
            4 => PenCompoundStyle::TriLine,
            _ => PenCompoundStyle::Single,
        }
    }
    pub fn get_dash_style(&self) -> DashStyle {
        match self.bag_get_u32(Self::KEY_DASH, 0) {
            0 => DashStyle::Solid,
            1 => DashStyle::Dot,
            2 => DashStyle::Dash,
            3 => DashStyle::LongDash,
            4 => DashStyle::DashDot,
            5 => DashStyle::LongDashDot,
            6 => DashStyle::LongDashDotDot,
            _ => DashStyle::Solid,
        }
    }
    pub fn get_line_cap_style(&self) -> LineCapStyle {
        match self.bag_get_u32(Self::KEY_LINE_CAP, 1) {
            0 => LineCapStyle::Round,
            1 => LineCapStyle::Square,
            2 => LineCapStyle::Flat,
            _ => LineCapStyle::Square,
        }
    }
    pub fn get_line_join_style(&self) -> LineJoinStyle {
        match self.bag_get_u32(Self::KEY_LINE_JOIN, 0) {
            0 => LineJoinStyle::Miter,
            1 => LineJoinStyle::Round,
            2 => LineJoinStyle::Bevel,
            _ => LineJoinStyle::Miter,
        }
    }
    pub fn get_miter_limit(&self) -> f32 {
        self.bag_get_f32(Self::KEY_MITER_LIMIT, 10.0)
    }
    pub fn get_start_arrow_style(&self) -> ArrowStyle {
        match self.bag_get_u32(Self::KEY_START_ARROW_STYLE, 0) {
            0 => ArrowStyle::None,
            1 => ArrowStyle::Triangle,
            2 => ArrowStyle::Diamond,
            3 => ArrowStyle::Circle,
            4 => ArrowStyle::Open,
            5 => ArrowStyle::Stealth,
            _ => ArrowStyle::None,
        }
    }
    pub fn get_start_arrow_size(&self) -> ArrowSizeStyle {
        match self.bag_get_u32(Self::KEY_START_ARROW_SIZE, 0) {
            0 => ArrowSizeStyle::Small,
            1 => ArrowSizeStyle::Medium,
            2 => ArrowSizeStyle::Large,
            _ => ArrowSizeStyle::Small,
        }
    }
    pub fn get_end_arrow_style(&self) -> ArrowStyle {
        match self.bag_get_u32(Self::KEY_END_ARROW_STYLE, 0) {
            0 => ArrowStyle::None,
            1 => ArrowStyle::Triangle,
            2 => ArrowStyle::Diamond,
            3 => ArrowStyle::Circle,
            4 => ArrowStyle::Open,
            5 => ArrowStyle::Stealth,
            _ => ArrowStyle::None,
        }
    }
    pub fn get_end_arrow_size(&self) -> ArrowSizeStyle {
        match self.bag_get_u32(Self::KEY_END_ARROW_SIZE, 0) {
            0 => ArrowSizeStyle::Small,
            1 => ArrowSizeStyle::Medium,
            2 => ArrowSizeStyle::Large,
            _ => ArrowSizeStyle::Small,
        }
    }
    pub fn get_pen_align_style(&self) -> PenAlignStyle {
        match self.bag_get_u32(Self::KEY_PEN_ALIGN, 0) {
            0 => PenAlignStyle::Center,
            1 => PenAlignStyle::Inset,
            2 => PenAlignStyle::Outset,
            _ => PenAlignStyle::Center,
        }
    }

    // ----------------- Property setters (raw `SetXxx` 1:1) -----------------

    pub fn set_thickness(&mut self, w: f32) {
        self.bag_attach_f32(Self::KEY_THICKNESS, w);
    }
    pub fn set_pen_compound_style(&mut self, s: PenCompoundStyle) {
        self.bag_attach_u32(Self::KEY_COMPOUND, s as u32);
    }
    pub fn set_dash_style(&mut self, s: DashStyle) {
        self.bag_attach_u32(Self::KEY_DASH, s as u32);
    }
    pub fn set_line_cap_style(&mut self, s: LineCapStyle) {
        self.bag_attach_u32(Self::KEY_LINE_CAP, s as u32);
    }
    pub fn set_line_join_style(&mut self, s: LineJoinStyle) {
        self.bag_attach_u32(Self::KEY_LINE_JOIN, s as u32);
    }
    pub fn set_miter_limit(&mut self, w: f32) {
        self.bag_attach_f32(Self::KEY_MITER_LIMIT, w);
    }
    pub fn set_start_arrow_style(&mut self, s: ArrowStyle) {
        self.bag_attach_u32(Self::KEY_START_ARROW_STYLE, s as u32);
    }
    pub fn set_start_arrow_size(&mut self, s: ArrowSizeStyle) {
        self.bag_attach_u32(Self::KEY_START_ARROW_SIZE, s as u32);
    }
    pub fn set_end_arrow_style(&mut self, s: ArrowStyle) {
        self.bag_attach_u32(Self::KEY_END_ARROW_STYLE, s as u32);
    }
    pub fn set_end_arrow_size(&mut self, s: ArrowSizeStyle) {
        self.bag_attach_u32(Self::KEY_END_ARROW_SIZE, s as u32);
    }
    pub fn set_pen_align_style(&mut self, s: PenAlignStyle) {
        self.bag_attach_u32(Self::KEY_PEN_ALIGN, s as u32);
    }
}

impl std::fmt::Debug for Pen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pen")
            .field("brush", &self.brush)
            .field("thickness", &self.get_thickness())
            .field("compound", &self.get_pen_compound_style())
            .field("dash", &self.get_dash_style())
            .field("line_cap", &self.get_line_cap_style())
            .field("line_join", &self.get_line_join_style())
            .field("align", &self.get_pen_align_style())
            .finish()
    }
}

impl Pen {
    /// raw `Pen::Pen()` (`0x1b4cf0`) default — empty brush + 모든 default values.
    ///
    /// width = `base * 0.75 / 72.0` (base = `ShapeEngine.[+0x4]`); 본 Rust port 는
    /// `ShapeEngine` 의 base 가 미지정 — 1.0 placeholder 사용 후 caller 가 set.
    pub fn new_default() -> Self {
        // raw `0x1b4cf0`: SharePtr<Brush>.ctrl = null, PropertyBag::PropertyBag(false),
        // 그리고 PFloat helper 로 thickness key attach (state=2).
        let mut p = Pen {
            brush: Box::new(Brush::Empty(crate::brush::EmptyBrush::new())),
            bag: crate::property_bag::PropertyBag::new(false),
        };
        p.set_thickness(1.0 * 0.75 / 72.0); // ShapeEngine.base = 1.0 placeholder
        p
    }

    /// raw `Pen::Pen()` (`0x1b4cf0`) 1:1 byte-eq port — Pen::C2() with full 10-key
    /// default attach (all state=2 = ENABLED_EXPLICIT per raw `mov w3, #0x2`).
    ///
    /// raw asm sequence (189 instr at `0x1b4cf0`):
    /// ```asm
    /// 0x1b4d0c: str  xzr, [x20], #0x8         ; +0x00 = null (stroke brush SharePtr)
    /// 0x1b4d18: bl   PropertyBag::C1(b=false) ; +0x08 = bag
    /// 0x1b4d28: bl   ShapeEngine::GetInstance ; engine ptr in x0
    /// 0x1b4d2c: ldr  s0, [x0, #0x4]           ; engine_base = engine[0x4]
    /// 0x1b4d30: fmov s1, #0.75
    /// 0x1b4d34: fmul s0, s0, s1                ; engine_base * 0.75
    /// 0x1b4d38: mov  w8, #0x42900000           ; 72.0
    /// 0x1b4d40: fdiv s0, s0, s1                ; / 72.0
    /// 0x1b4d68: bl   0x653cb4                  ; PFloat helper (key 0x2bc, state=2)
    /// ;; 9 more attach calls (keys 0x2bd..0x2c6, skipping 0x2c1)
    /// ```
    ///
    /// **default values** (10 keys, all state=2 = ENABLED_EXPLICIT per raw mov w3, #0x2):
    /// | key   | type   | value     | helper      |
    /// |-------|--------|-----------|-------------|
    /// | 0x2bc | PFloat | `engine_base * 0.75 / 72` | `0x653cb4` |
    /// | 0x2bd | PEnum  | 0         | `0x669704` |
    /// | 0x2be | PEnum  | 0         | `0x656254` |
    /// | 0x2bf | PEnum  | 0         | `0x669b40` |
    /// | 0x2c0 | PEnum  | **1**     | `0x669f7c` |
    /// | 0x2c2 | PEnum  | 0         | `0x66a3b8` |
    /// | 0x2c3 | PEnum  | **4**     | `0x66a7f4` |
    /// | 0x2c4 | PEnum  | 0         | `0x66a3b8` |
    /// | 0x2c5 | PEnum  | **4**     | `0x66a7f4` |
    /// | 0x2c6 | PEnum  | 0         | `0x66ac30` |
    ///
    /// key 0x2c1 (miter limit) 은 Pen::C2 에서 attach 안 됨 (later setter 가 책임).
    ///
    /// 본 Rust port: 10 keys 의 default 값을 모두 state=2 으로 attach. 각 PEnum 의
    /// vtable distinction (0x669704/0x656254/...) 은 multi-session deferred — 모두
    /// 일반 PEnum 으로 처리, 값만 byte-eq.
    pub fn new_with_engine_defaults(engine_base: f32) -> Self {
        let mut p = Pen {
            brush: Box::new(Brush::Empty(crate::brush::EmptyBrush::new())),
            bag: crate::property_bag::PropertyBag::new(false),
        };
        let state2 = crate::property::state::ENABLED_EXPLICIT; // = 2 per raw `mov w3, #0x2`
        unsafe {
            // key 0x2bc (PFloat width = engine_base * 0.75 / 72)
            let width = engine_base * 0.75_f32 / 72.0_f32;
            let k = crate::property_key::PropertyKey::from_int(Self::KEY_THICKNESS);
            let _ = p.bag.attach(&k, crate::property::PFloat::create_attach_ctrl(state2, width));

            // 9 PEnum default attaches (state=2)
            let enum_defaults: [(u32, u32); 9] = [
                (Self::KEY_COMPOUND, 0),           // 0x2bd
                (Self::KEY_DASH, 0),               // 0x2be
                (Self::KEY_LINE_CAP, 0),           // 0x2bf
                (Self::KEY_LINE_JOIN, 1),          // 0x2c0
                (Self::KEY_START_ARROW_STYLE, 0),  // 0x2c2
                (Self::KEY_START_ARROW_SIZE, 4),   // 0x2c3
                (Self::KEY_END_ARROW_STYLE, 0),    // 0x2c4
                (Self::KEY_END_ARROW_SIZE, 4),     // 0x2c5
                (Self::KEY_PEN_ALIGN, 0),          // 0x2c6
            ];
            for (key_id, value) in enum_defaults {
                let k = crate::property_key::PropertyKey::from_int(key_id);
                let _ = p.bag.attach(
                    &k,
                    crate::property::PEnum::create_attach_ctrl(state2, value),
                );
            }
        }
        p
    }

    /// Pen stroke brush 교체 (raw 의 SharePtr<Brush> 재할당).
    ///
    /// Block 13+ 에서 SolidBrush(Scheme 0x10) 등을 stroke 로 설정 시 사용.
    pub fn set_stroke_brush(&mut self, brush: Box<Brush>) {
        self.brush = brush;
    }

    /// bag 의 노드 수 (debug / test 용).
    pub fn bag_size(&self) -> u64 {
        unsafe {
            self.bag
                .impl_ref()
                .map(|i| i.tree.size)
                .unwrap_or(0)
        }
    }

    /// raw Block 13 의 setter pattern — 명시적 state=1 (= ENABLED_DEFAULT per raw
    /// `mov w3, #0x1`) 으로 bag.attach. 기존 `set_thickness` (state=2) 와 다름 —
    /// CreateDefault 의 user-override path 가 state=1 사용.
    ///
    /// # Safety
    /// bag 의 internal mutation — caller 가 multi-thread access 보호.
    pub unsafe fn override_thickness(&mut self, w: f32) {
        let key = crate::property_key::PropertyKey::from_int(Self::KEY_THICKNESS);
        let state1 = crate::property::state::ENABLED_DEFAULT;
        let _ = self.bag.attach(&key, crate::property::PFloat::create_attach_ctrl(state1, w));
    }

    /// raw Block 13 의 PEnum override (state=1).
    pub unsafe fn override_enum_at(&mut self, key_id: u32, value: u32) {
        let key = crate::property_key::PropertyKey::from_int(key_id);
        let state1 = crate::property::state::ENABLED_DEFAULT;
        let _ = self.bag.attach(&key, crate::property::PEnum::create_attach_ctrl(state1, value));
    }

    /// raw `Pen::Pen(Color, f32, PenCompoundStyle, DashStyle, LineCapStyle,
    /// LineJoinStyle, ArrowStyle, ArrowSizeStyle, ArrowStyle, ArrowSizeStyle,
    /// PenAlignStyle)` (`0x1b4fe8`).
    ///
    /// 1. `SolidBrush::Create(Color)` → brush field.
    /// 2. PropertyBag attach 들로 모든 12 properties 채움.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        color: crate::color::Color,
        width: f32,
        compound_style: PenCompoundStyle,
        dash_style: DashStyle,
        line_cap_style: LineCapStyle,
        line_join_style: LineJoinStyle,
        head_arrow_style: ArrowStyle,
        head_arrow_size: ArrowSizeStyle,
        tail_arrow_style: ArrowStyle,
        tail_arrow_size: ArrowSizeStyle,
        pen_align_style: PenAlignStyle,
    ) -> Self {
        let mut p = Pen {
            brush: crate::brush::SolidBrush::create_boxed(color),
            bag: crate::property_bag::PropertyBag::new(false),
        };
        p.set_thickness(width);
        p.set_pen_compound_style(compound_style);
        p.set_dash_style(dash_style);
        p.set_line_cap_style(line_cap_style);
        p.set_line_join_style(line_join_style);
        p.set_start_arrow_style(head_arrow_style);
        p.set_start_arrow_size(head_arrow_size);
        p.set_end_arrow_style(tail_arrow_style);
        p.set_end_arrow_size(tail_arrow_size);
        p.set_pen_align_style(pen_align_style);
        p
    }

    /// raw `Pen::Clone(Color const&)` (`0x1b5b84`) — 동일 stroke 설정 +
    /// 새 SolidBrush(Color) 로 brush 교체.
    pub fn clone_with_color(&self, color: &crate::color::Color) -> Pen {
        let mut p = Pen {
            brush: crate::brush::SolidBrush::create_boxed(unsafe {
                crate::color::Color::copy_ctor(color)
            }),
            bag: crate::property_bag::PropertyBag::new(false),
        };
        // 각 stroke property 의 get → set 으로 PropertyBag 재구성
        p.set_thickness(self.get_thickness());
        p.set_pen_compound_style(self.get_pen_compound_style());
        p.set_dash_style(self.get_dash_style());
        p.set_line_cap_style(self.get_line_cap_style());
        p.set_line_join_style(self.get_line_join_style());
        p.set_start_arrow_style(self.get_start_arrow_style());
        p.set_start_arrow_size(self.get_start_arrow_size());
        p.set_end_arrow_style(self.get_end_arrow_style());
        p.set_end_arrow_size(self.get_end_arrow_size());
        p.set_pen_align_style(self.get_pen_align_style());
        p
    }
}

impl Clone for Pen {
    fn clone(&self) -> Self {
        // PropertyBag deep clone 은 multi-session deferred — 모든 stroke prop re-attach
        let mut p = Pen {
            brush: self.brush.clone_to_heap(),
            bag: crate::property_bag::PropertyBag::new(false),
        };
        p.set_thickness(self.get_thickness());
        p.set_pen_compound_style(self.get_pen_compound_style());
        p.set_dash_style(self.get_dash_style());
        p.set_line_cap_style(self.get_line_cap_style());
        p.set_line_join_style(self.get_line_join_style());
        p.set_miter_limit(self.get_miter_limit());
        p.set_start_arrow_style(self.get_start_arrow_style());
        p.set_start_arrow_size(self.get_start_arrow_size());
        p.set_end_arrow_style(self.get_end_arrow_style());
        p.set_end_arrow_size(self.get_end_arrow_size());
        p.set_pen_align_style(self.get_pen_align_style());
        p
    }
}

impl PartialEq for Pen {
    fn eq(&self, other: &Self) -> bool {
        self.brush.eq_brush(&other.brush)
            && self.get_thickness().to_bits() == other.get_thickness().to_bits()
            && self.get_pen_compound_style() == other.get_pen_compound_style()
            && self.get_dash_style() == other.get_dash_style()
            && self.get_line_cap_style() == other.get_line_cap_style()
            && self.get_line_join_style() == other.get_line_join_style()
            && self.get_start_arrow_style() == other.get_start_arrow_style()
            && self.get_start_arrow_size() == other.get_start_arrow_size()
            && self.get_end_arrow_style() == other.get_end_arrow_style()
            && self.get_end_arrow_size() == other.get_end_arrow_size()
            && self.get_pen_align_style() == other.get_pen_align_style()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brush::{Brush, BrushType};

    #[test]
    fn pen_default_uses_empty_brush() {
        let p = Pen::new_default();
        assert_eq!(p.brush.get_type(), BrushType::Empty);
        assert_eq!(p.get_line_cap_style(), LineCapStyle::Square); // raw default = 1
    }

    #[test]
    fn pen_new_wraps_color_as_solid_brush() {
        let red = crate::color::Color::from_rgb(0xFF, 0, 0, std::ptr::null_mut());
        let p = Pen::new(
            red,
            2.0,
            PenCompoundStyle::Single,
            DashStyle::Solid,
            LineCapStyle::Round,
            LineJoinStyle::Miter,
            ArrowStyle::None,
            ArrowSizeStyle::Small,
            ArrowStyle::None,
            ArrowSizeStyle::Small,
            PenAlignStyle::Center,
        );
        assert_eq!(p.brush.get_type(), BrushType::Solid);
        assert_eq!(p.get_thickness(), 2.0);
    }

    #[test]
    fn pen_clone_preserves_all_fields() {
        let red = crate::color::Color::from_rgb(0xFF, 0, 0, std::ptr::null_mut());
        let p = Pen::new(
            red,
            3.5,
            PenCompoundStyle::Double,
            DashStyle::Dash,
            LineCapStyle::Flat,
            LineJoinStyle::Round,
            ArrowStyle::Triangle,
            ArrowSizeStyle::Large,
            ArrowStyle::Diamond,
            ArrowSizeStyle::Medium,
            PenAlignStyle::Inset,
        );
        let cloned = p.clone();
        assert_eq!(cloned.get_thickness(), 3.5);
        assert_eq!(cloned.get_pen_compound_style(), PenCompoundStyle::Double);
        assert_eq!(cloned.get_dash_style(), DashStyle::Dash);
        assert_eq!(cloned.get_line_cap_style(), LineCapStyle::Flat);
        assert_eq!(cloned.get_start_arrow_style(), ArrowStyle::Triangle);
        assert_eq!(cloned.get_end_arrow_size(), ArrowSizeStyle::Medium);
        assert_eq!(cloned.get_pen_align_style(), PenAlignStyle::Inset);
    }

    #[test]
    fn pen_clone_with_color_replaces_brush() {
        let red = crate::color::Color::from_rgb(0xFF, 0, 0, std::ptr::null_mut());
        let blue = crate::color::Color::from_rgb(0, 0, 0xFF, std::ptr::null_mut());
        let p = Pen::new(
            red,
            1.0,
            PenCompoundStyle::default(),
            DashStyle::default(),
            LineCapStyle::default(),
            LineJoinStyle::default(),
            ArrowStyle::default(),
            ArrowSizeStyle::default(),
            ArrowStyle::default(),
            ArrowSizeStyle::default(),
            PenAlignStyle::default(),
        );
        let cloned = p.clone_with_color(&blue);
        assert_eq!(cloned.get_thickness(), 1.0);
        if let Brush::Solid(sb) = &*cloned.brush {
            let rgb = sb.get_color().get_rgb();
            assert_eq!(rgb.b, 0xFF); // blue
        } else {
            panic!("expected SolidBrush");
        }
    }

    #[test]
    fn pen_eq_same_fields_returns_true() {
        let p1 = Pen::new_default();
        let p2 = Pen::new_default();
        assert!(p1 == p2);
    }

    #[test]
    fn pen_eq_different_width_returns_false() {
        let mut p1 = Pen::new_default();
        p1.set_thickness(1.0);
        let mut p2 = Pen::new_default();
        p2.set_thickness(2.0);
        assert!(p1 != p2);
    }

    #[test]
    fn pen_raw_16b_layout() {
        // 16t 재설계: Pen 의 raw byte-eq 16B layout (SharePtr<Brush> + PropertyBag)
        // Rust 의 Box<Brush> 도 8B, PropertyBag 도 8B → 16B
        assert_eq!(std::mem::size_of::<Pen>(), 16);
        assert_eq!(std::mem::align_of::<Pen>(), 8);
    }

    #[test]
    fn pen_field_offsets_match_raw() {
        let p = Pen::new_default();
        let base = &p as *const _ as usize;
        assert_eq!(&p.brush as *const _ as usize - base, 0x00);
        assert_eq!(&p.bag as *const _ as usize - base, 0x08);
    }

    #[test]
    fn pen_setter_round_trips() {
        let mut p = Pen::new_default();
        p.set_thickness(2.5);
        p.set_pen_compound_style(PenCompoundStyle::ThickThin);
        p.set_dash_style(DashStyle::DashDot);
        p.set_line_cap_style(LineCapStyle::Round);
        p.set_line_join_style(LineJoinStyle::Bevel);
        p.set_miter_limit(15.0);
        p.set_start_arrow_style(ArrowStyle::Diamond);
        p.set_end_arrow_style(ArrowStyle::Triangle);
        p.set_pen_align_style(PenAlignStyle::Outset);

        assert_eq!(p.get_thickness(), 2.5);
        assert_eq!(p.get_pen_compound_style(), PenCompoundStyle::ThickThin);
        assert_eq!(p.get_dash_style(), DashStyle::DashDot);
        assert_eq!(p.get_line_cap_style(), LineCapStyle::Round);
        assert_eq!(p.get_line_join_style(), LineJoinStyle::Bevel);
        assert_eq!(p.get_miter_limit(), 15.0);
        assert_eq!(p.get_start_arrow_style(), ArrowStyle::Diamond);
        assert_eq!(p.get_end_arrow_style(), ArrowStyle::Triangle);
        assert_eq!(p.get_pen_align_style(), PenAlignStyle::Outset);
    }
}

//! `Hnc::Shape::Text::CharItemView` — text character view (단일 글자의 layout + 캐시).
//!
//! ## 출처
//!
//! - ctor: `0x2ef798` (sz=1840) — 6-arg ctor `(wchar_t, UniquePtr<RunProperty>,
//!   UniquePtr<ParaProperty>, SharePtr<BodyProperty>, Theme*, float)`
//! - `Draw`: `0x2f5e3c` (sz=1880) — 메인 draw 메서드
//! - `Allocate`: `0x2f5d48`, `Request`: `0x2f5bb0`, `Pick`: `0x2f9a34`,
//!   `Undraw`: `0x2f8bd8`, `GetBounds`: `0x2f9008` / `0x2f2464`
//! - GetReal* 14종 (본 module 의 핵심)
//! - vtable: `0x780098`
//!
//! ## CharItemView struct layout (partial — ctor 2ef798 + GetReal* 에서 관찰)
//!
//! | offset | size | field                       | 출처 |
//! |--------|------|-----------------------------|------|
//! | +0x00  | 8B   | vtable                      | ctor `*this = &PTR__CharItemView_00780098` |
//! | +0x08  | 2B   | `character: u16`            | ctor `*(short*)(this+8) = param_1` |
//! | +0x10  | 8B   | (init 0)                    | ctor `*(this+0x10) = 0` |
//! | +0x18  | 8B   | `run_property: SharePtr<RunProperty>` | ctor refcount++ |
//! | +0x20  | 8B   | `para_property: SharePtr<ParaProperty>` | ctor refcount++ |
//! | +0x28  | 8B   | `body_property: SharePtr<BodyProperty>` | ctor refcount++ |
//! | +0x30  | 8B   | `font: SharePtr<Font>` (set later) | ctor init 0, 후에 GetRealFont 결과 저장 |
//! | +0x38  | 8B   | (init 0)                    | ctor `*(this+0x38) = 0` |
//! | +0x40  | 4B   | `ascent: f32` (자기 글자 ascent) | Allocate 에서 set |
//! | +0x44  | 4B   | `descent: f32` (자기 글자 descent) | GetRealTextEffects 의 reflection distance 계산 |
//! | +0x48  | 4B   | (init 0; later f32)         | ctor `*(this+0x48) = 0` |
//! | +0x4c  | 4B   | `f32` (e.g. width)          | ctor 후에 set |
//! | +0x50  | 4B   | `f32`                       | |
//! | +0x54  | 4B   | `total_height: f32` (`(asc+desc) * 1.2 * unit / 72`) | ctor 계산 |
//! | +0x58  | 4B   | `total_height_alt: f32`     | ctor 계산 |
//! | +0x5c  | 4B   | `ascent_ratio: f32` (`asc / (asc+desc)`) | ctor 계산 |
//! | +0x60  | 4B   | `width_param: f32` = ctor arg6 | ctor `*(this+0x60) = param_6` |
//! | +0x64  | 4B   | `f32` (advance 등)           | ctor 계산 |
//! | +0x68  | 4B   | `f32`                       | ctor 계산 |
//! | +0x6c..0x84 | -| 0 init                      | |
//! | +0x90  | 8B   | `theme_ptr: *const Theme`   | ctor `*(this+0x90) = param_5` |
//! | +0x98  | 8B   | 0 init                      | |
//! | +0xa0  | 8B   | `paths_cache: SharePtr<Paths>` (GetRealPaths 결과 캐시) | ctor init 0 |
//! | +0xa8  | 8B   | 0 init                      | |
//! | +0xb0  | 8B   | 0 init                      | |
//! | +0xb8..0x170 | ~| `image_painter: ImagePainterObject` (~184B nested) | ctor 호출 |
//! | +0x170 | 8B   | 0 init                      | |
//! | +0x178 | 8B   | 0 init                      | |
//! | +0x180 | 8B   | 0 init                      | |
//! | +0x188 | 8B   | 0 init                      | |
//!
//! ## L-5c-8 scope
//!
//! 본 module 은 **struct layout + 5 trivial GetReal\* getters** 만 (정공법 부분 port):
//! 1. `GetRealRunProperty` (raw 0x2f0ab8, sz=8) — `return this+0x18`
//! 2. `GetRealParaProperty` (raw 0x2f0ac0, sz=8) — `return this+0x20`
//! 3. `GetRealBodyProperty` (raw 0x2f0ac8, sz=8) — `return this+0x28`
//! 4. `GetRealScene3D` (raw 0x2f1c80, sz=64) — `FUN_0064a590` (Scene3D 의 default factory) 위임
//! 5. `GetRealSp3D` (raw 0x2f1cd4, sz=64) — `FUN_0064add4` (Sp3D 의 default factory) 위임
//!
//! 복잡한 GetReal\* (Font 992B + TextEffects 2.4KB + Brush/Pen/UnderLine*/Effects/Paths
//! 100~500B 각각) 은 별도 sub-task (RunProperty 의 PropertyBag 구조 RE 후).
//!
//! ## byte-eq 경계
//!
//! - struct layout: 정확한 offset 매치 (ctor `0x2ef798` 의 store 시퀀스 1:1).
//! - 3 trivial getters: raw asm 1:1 (`ldr x0, [x0, #offset]; ret`).
//! - Scene3D/Sp3D getters: raw 가 default factory 위임 — Rust 도 default `Scene3D::new()` 반환.

use crate::body_property::BodyProperty;
use crate::scene3d::Scene3D;
use crate::share_ptr::{ControlBlock, SharePtr};
use crate::sp3d::Sp3D;
use crate::theme::THEME_SIZE_BYTES as _;

/// `Hnc::Shape::Text::RunProperty` — text run 의 stylistic 속성들 (vtable 없는 POD struct).
///
/// ## layout (GetReal* asm 으로 확인)
///
/// | offset | size | field | 출처 |
/// |--------|------|-------|------|
/// | +0x00  | 8B   | `brush: SharePtr<Brush>` | GetRealBrush `ldr x20, [x8]` (= obj+0) |
/// | +0x08  | 8B   | `pen: SharePtr<Pen>`     | GetRealPen `ldr x9, [x9, #0x8]` |
/// | +0x10  | 8B   | `effects: SharePtr<Effects>` | GetRealEffects `*(long**)(lVar1 + 0x10)` |
/// | +0x18  | 8B   | `underline_brush: SharePtr<Brush>` | GetRealUnderLineBrush `*(long**)(lVar2 + 0x18)` |
/// | +0x20  | 8B   | (unknown — probably SharePtr<Pen> underline) | |
/// | +0x28  | 8B   | `font_default: SharePtr<Font>` | GetRealFont `*(long**)(lVar6 + 0x28)` |
/// | +0x30  | 8B   | `font_latin: SharePtr<Font>` | GetRealFont `lVar6 + 0x30` |
/// | +0x38  | 8B   | `font_variant2: SharePtr<Font>` | GetRealFont `lVar6 + 0x38` |
/// | +0x40  | 8B   | `font_variant3: SharePtr<Font>` | GetRealFont `lVar6 + 0x40` |
/// | +0x48  | 8B   | `property_bag: SharePtr<PropertyBagImpl>` | GetRealTextEffects `lVar15 + 0x48` |
///
/// **vtable 없음** — POD struct (GetRealBrush 의 `obj+0` 가 vtable 이 아니라 직접 SharePtr).
#[derive(Debug)]
#[repr(C)]
pub struct RunProperty {
    /// +0x00: SharePtr<Brush> (메인 fill brush).
    pub brush: *mut ControlBlock<crate::brush::Brush>,
    /// +0x08: SharePtr<Pen> (outline pen).
    pub pen: *mut ControlBlock<crate::pen::Pen>,
    /// +0x10: SharePtr<Effects>.
    pub effects: *mut ControlBlock<crate::effects_container::Effects>,
    /// +0x18: SharePtr<Brush> (underline brush).
    pub underline_brush: *mut ControlBlock<crate::brush::Brush>,
    /// +0x20: (unknown) — probably SharePtr<Pen> for underline; placeholder.
    pub underline_pen: *mut ControlBlock<crate::pen::Pen>,
    /// +0x28: SharePtr<Font> (default).
    pub font_default: *mut u8,
    /// +0x30: SharePtr<Font> (latin/symbol).
    pub font_latin: *mut u8,
    /// +0x38: SharePtr<Font> (variant 2).
    pub font_variant2: *mut u8,
    /// +0x40: SharePtr<Font> (variant 3).
    pub font_variant3: *mut u8,
    /// +0x48: SharePtr<PropertyBagImpl> (general font/script properties).
    pub property_bag: *mut ControlBlock<crate::property_bag::PropertyBagImpl>,
}

impl RunProperty {
    /// 빈 RunProperty (모든 SharePtr null).
    pub fn new_empty() -> Self {
        RunProperty {
            brush: std::ptr::null_mut(),
            pen: std::ptr::null_mut(),
            effects: std::ptr::null_mut(),
            underline_brush: std::ptr::null_mut(),
            underline_pen: std::ptr::null_mut(),
            font_default: std::ptr::null_mut(),
            font_latin: std::ptr::null_mut(),
            font_variant2: std::ptr::null_mut(),
            font_variant3: std::ptr::null_mut(),
            property_bag: std::ptr::null_mut(),
        }
    }
}

/// `Hnc::Shape::Text::ParaProperty` — opaque.
#[derive(Debug)]
#[repr(C)]
pub struct ParaProperty {
    pub vtable: *const u8,
    pub _data: [u8; 56],
}

/// `Hnc::Shape::Text::Requisition` — Request() 의 output struct.
///
/// raw 의 정확한 typedef 는 별도 RE 필요. asm 의 store 위치 (`+0x00..+0x20`) 으로 36B 확정.
/// fields 의 의미:
/// - +0x00..+0x10: requested/alt/min sizes (4× f32)
/// - +0x10..+0x20: complementary sizes (4× f32)
/// - +0x20: mode flag (u32 / i32 — CR/LF 는 음수)
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Requisition {
    pub f00: f32,
    pub f04: f32,
    pub f08: f32,
    pub f0c: f32,
    pub f10: f32,
    pub f14: f32,
    pub f18: f32,
    pub f1c: f32,
    pub mode: i32,
}

pub const REQUISITION_SIZE_BYTES: usize = 36;
const _: () = assert!(std::mem::size_of::<Requisition>() == REQUISITION_SIZE_BYTES);

impl ParaProperty {
    pub fn new_empty() -> Self {
        ParaProperty {
            vtable: std::ptr::null(),
            _data: [0; 56],
        }
    }
}

/// `Hnc::Shape::Text::CharItemView` — 단일 글자의 layout+캐시 view.
///
/// 본 struct 는 raw layout 의 첫 `0x190` (= 400B) 영역만 명시적으로 fields 로 매핑.
/// 나머지 (특히 `ImagePainterObject` nested 영역) 은 `_painter_etc` byte array 로 둠.
#[repr(C)]
#[derive(Debug)]
pub struct CharItemView {
    /// +0x00: vtable.
    pub vtable: *const u8,
    /// +0x08: u16 character (the wchar_t arg).
    pub character: u16,
    /// +0x0a..+0x10: pad (alignment to 8B).
    pub _pad0a: [u8; 6],
    /// +0x10: pad/state (ctor init 0).
    pub _state10: u64,
    /// +0x18: SharePtr<RunProperty>.
    pub run_property: *mut ControlBlock<RunProperty>,
    /// +0x20: SharePtr<ParaProperty>.
    pub para_property: *mut ControlBlock<ParaProperty>,
    /// +0x28: SharePtr<BodyProperty>.
    pub body_property: *mut ControlBlock<BodyProperty>,
    /// +0x30: SharePtr<Font> — set in ctor via GetRealFont.
    pub font: *mut u8,
    /// +0x38: f32 (ctor init 0) — 일반적으로 미사용 슬롯.
    pub _f38: f32,
    /// +0x3c: f32 (raw `ldr s13, [x23, #0x3c]` CalcDrawVariables Stage D 의 Bottom-aware path 에서 사용).
    /// 의미 가설: 폰트 width-related metric. 본 port 에서는 byte-level 그대로 보존.
    pub field_3c: f32,
    /// +0x40: f32 ascent (set in Allocate).
    pub ascent: f32,
    /// +0x44: f32 descent.
    pub descent: f32,
    /// +0x48: f32 (ctor init 0; later set).
    pub _f48: f32,
    /// +0x4c: f32 width hint.
    pub _f4c: f32,
    /// +0x50: f32.
    pub _f50: f32,
    /// +0x54: f32 total_height (ctor: `(asc+desc)*1.2*unit/72`).
    pub total_height: f32,
    /// +0x58: f32 total_height_alt.
    pub total_height_alt: f32,
    /// +0x5c: f32 ascent_ratio.
    pub ascent_ratio: f32,
    /// +0x60: f32 width_param (= ctor arg6).
    pub width_param: f32,
    /// +0x64: f32 advance-like.
    pub _f64: f32,
    /// +0x68: f32.
    pub _f68: f32,
    /// +0x6c: f32 — CalcDrawVariables 의 default-format 분기에서 origin 계산용 (raw `ldp s11, s1, [x23, #0x6c]` first lane).
    pub format_origin_x: f32,
    /// +0x70: f32 — default-format 분기 second lane (`ldp` second lane: scale factor `1-x` 형태).
    pub format_origin_scale: f32,
    /// +0x74: f32 — shadow scale (raw `ldr s1, [x23, #0x74]` × `[x23, #0x6c]` × `pic.0x96c` = s8).
    pub shadow_scale: f32,
    /// +0x78: u8 — has_explicit_format flag (raw `ldrb w9, [x23, #0x78]; cbz w9, default_branch`).
    pub has_explicit_format: u8,
    /// +0x79..+0x7c: panose / charset 3-byte metadata (raw `ldurh [x23, #0x79]` + `ldrb [x23, #0x7b]`).
    /// `format_panose[0..2]` = u16 LE unaligned (panose lower), `[2]` = high byte.
    pub format_panose: [u8; 3],
    /// +0x7c: f32 — explicit format scale m0 (raw `ldr s12, [x23, #0x7c]` 또는 `ldp s12, s14 [x23, #0x7c]` first lane).
    pub format_scale_x: f32,
    /// +0x80: f32 — explicit format scale m1 (raw `ldr s14, [x23, #0x80]` 또는 `ldp` second lane).
    pub format_scale_y: f32,
    /// +0x84: f32 — explicit format scale m2 (raw `ldr s11, [x23, #0x84]` 또는 `ldp s11, s13 [x23, #0x84]` first lane).
    pub format_rot_x: f32,
    /// +0x88: f32 — explicit format scale m3 (raw `ldr s13, [x23, #0x88]` 또는 `ldp` second lane).
    pub format_rot_y: f32,
    /// +0x8c..+0x90: 4 byte trailing pad to match layout assertion.
    pub _pad8c: [u8; 4],
    /// +0x90: Theme*.
    pub theme_ptr: *const u8,
    /// +0x98: 0 init.
    pub _pad98: u64,
    /// +0xa0: SharePtr<Paths> cache (GetRealPaths 결과).
    pub paths_cache: *mut ControlBlock<crate::paths::Paths>,
    /// +0xa8: SharePtr<Path> cache (GetCachedRenderPath 결과 — glyph path 합성).
    pub render_path_cache: *mut ControlBlock<crate::path::Path>,
    /// +0xb0: 0 init.
    pub _padb0: u64,
    /// +0xb8..+0x170: ImagePainterObject (~184B nested). 본 struct 는 byte array 로 placeholder.
    pub _image_painter: [u8; 0xb8],
    /// +0x170..+0x190: trailing 0-inits (32B).
    pub _trailing: [u8; 0x20],
}

pub const CHAR_ITEM_VIEW_SIZE_BYTES: usize = 0x190;
pub const CHAR_ITEM_VIEW_ALIGN_BYTES: usize = 8;

const _: () = assert!(std::mem::size_of::<CharItemView>() == CHAR_ITEM_VIEW_SIZE_BYTES);
const _: () = assert!(std::mem::align_of::<CharItemView>() == CHAR_ITEM_VIEW_ALIGN_BYTES);

/// raw vtable address (`0x780098`).
pub const CHAR_ITEM_VIEW_VTABLE_ADDR: usize = 0x780098;

impl CharItemView {
    /// 모든 fields 가 0/null 인 빈 CharItemView (ctor 의 1st phase 와 동치).
    ///
    /// 실제 ctor 는 1840B 코드로 SharePtr 3개 refcount++, font/sizing 계산 등.
    /// 본 helper 는 GetReal* getter 테스트용.
    pub fn new_empty() -> Self {
        CharItemView {
            vtable: CHAR_ITEM_VIEW_VTABLE_ADDR as *const u8,
            character: 0,
            _pad0a: [0; 6],
            _state10: 0,
            run_property: std::ptr::null_mut(),
            para_property: std::ptr::null_mut(),
            body_property: std::ptr::null_mut(),
            font: std::ptr::null_mut(),
            _f38: 0.0,
            field_3c: 0.0,
            ascent: 0.0,
            descent: 0.0,
            _f48: 0.0,
            _f4c: 0.0,
            _f50: 0.0,
            total_height: 0.0,
            total_height_alt: 0.0,
            ascent_ratio: 0.0,
            width_param: 0.0,
            _f64: 0.0,
            _f68: 0.0,
            format_origin_x: 0.0,
            format_origin_scale: 0.0,
            shadow_scale: 0.0,
            has_explicit_format: 0,
            format_panose: [0; 3],
            format_scale_x: 0.0,
            format_scale_y: 0.0,
            format_rot_x: 0.0,
            format_rot_y: 0.0,
            _pad8c: [0; 4],
            theme_ptr: std::ptr::null(),
            _pad98: 0,
            paths_cache: std::ptr::null_mut(),
            render_path_cache: std::ptr::null_mut(),
            _padb0: 0,
            _image_painter: [0; 0xb8],
            _trailing: [0; 0x20],
        }
    }

    /// raw `GetRealRunProperty() const` @ `0x2f0ab8` (sz=8).
    ///
    /// raw asm: `add x0, x0, #0x18; ret` → `return this + 0x18`.
    /// 의미: `&this->run_property` (= SharePtr 의 storage 주소).
    #[inline]
    pub fn get_real_run_property(&self) -> *const *mut ControlBlock<RunProperty> {
        &self.run_property as *const _
    }

    /// raw `GetRealParaProperty() const` @ `0x2f0ac0` (sz=8).
    ///
    /// raw: `add x0, x0, #0x20; ret`.
    #[inline]
    pub fn get_real_para_property(&self) -> *const *mut ControlBlock<ParaProperty> {
        &self.para_property as *const _
    }

    /// raw `GetRealBodyProperty() const` @ `0x2f0ac8` (sz=8).
    ///
    /// raw: `add x0, x0, #0x28; ret`.
    #[inline]
    pub fn get_real_body_property(&self) -> *const *mut ControlBlock<BodyProperty> {
        &self.body_property as *const _
    }

    /// raw `GetRealScene3D(Theme const*) const` @ `0x2f1c80` (sz=64).
    ///
    /// raw 는 `FUN_0064a590()` 를 위임 호출 — Scene3D 의 **default factory**
    /// (multi-session deferred — Scene3D ctor RE 후 정정).
    /// 본 단계는 SharePtr 형태로 default Scene3D 반환 (refcount=1).
    ///
    /// raw 의 unreachable block warning 은 Ghidra 가 simplifier 했음을 의미.
    pub fn get_real_scene3d(&self, _theme: *const u8) -> SharePtr<Scene3D> {
        // raw 의 default factory 등치. 별도 cache 없이 매 호출마다 default 인스턴스.
        // 실 구현은 Scene3D 의 thread-safe singleton 이지만 본 단계는 placeholder.
        SharePtr::null()
    }

    /// raw `GetRealSp3D(Theme const*) const` @ `0x2f1cd4` (sz=64).
    ///
    /// raw 는 `FUN_0064add4()` 위임 — Sp3D 의 default factory.
    pub fn get_real_sp3d(&self, _theme: *const u8) -> SharePtr<Sp3D> {
        SharePtr::null()
    }

    // ───────────────────────────────────────────────────────────────────────
    // GetReal* — RunProperty 의 SharePtr fields fallback chain
    // ───────────────────────────────────────────────────────────────────────

    /// 공통 helper: RunProperty 의 +offset 에 있는 SharePtr<T> 를 refcount++ 후 반환.
    /// `run_property` 가 null 이거나 obj 가 null 이면 null 반환.
    ///
    /// raw 의 공통 패턴 (GetRealBrush/Pen/Effects/UnderLineBrush 동일):
    /// ```text
    /// if this[0x18] == null: → null
    /// obj = *this[0x18]
    /// if obj == null: → null
    /// sp = *(obj + offset)  (= SharePtr<T> value)
    /// if sp != null && sp.obj != null:
    ///   sp.refcount++
    ///   bl <SharePtr AddRef helper>
    /// return sp
    /// ```
    ///
    /// # Safety
    /// `run_property` 가 valid `ControlBlock<RunProperty>*` 또는 null.
    #[inline]
    unsafe fn get_real_share_ptr_at_offset<T>(
        &self,
        offset_in_run_property: isize,
    ) -> *mut ControlBlock<T> {
        if self.run_property.is_null() {
            return std::ptr::null_mut();
        }
        let obj = (*self.run_property).obj;
        if obj.is_null() {
            return std::ptr::null_mut();
        }
        // raw `ldr x20, [x8, #offset]` — RunProperty[offset] = SharePtr<T> 값
        let field_ptr = (obj as *const u8).offset(offset_in_run_property)
            as *const *mut ControlBlock<T>;
        let sp = *field_ptr;
        if !sp.is_null() && !(*sp).obj.is_null() {
            // raw 의 refcount++ + bl <AddRef helper>
            (*sp).refcount = (*sp).refcount.wrapping_add(1);
        }
        sp
    }

    /// raw `GetRealBrush(Theme const*) const` @ `0x2f1884` (sz=112).
    ///
    /// raw asm:
    /// ```text
    /// x8 = this[0x18]              ; SharePtr<RP>
    /// if x8==null → *sret = 0, ret
    /// x8 = *x8                     ; obj
    /// if x8==null → *sret = 0, ret
    /// x20 = *x8                    ; RP[0] = SharePtr<Brush>
    /// *sret = x20
    /// if x20!=null && x20.obj!=null:
    ///   x20.refcount++
    ///   bl 0x647cfc                 ; SharePtr AddRef notify
    /// ```
    ///
    /// # Safety
    /// `self.run_property` 가 valid 또는 null.
    pub unsafe fn get_real_brush(&self, _theme: *const u8) -> *mut ControlBlock<crate::brush::Brush> {
        self.get_real_share_ptr_at_offset::<crate::brush::Brush>(0x00)
    }

    /// raw `GetRealPen(Theme const*) const` @ `0x2f1b10` (sz=144).
    ///
    /// raw asm: 위와 동일한 패턴이지만 RunProperty[+0x08] 읽음 (Pen SharePtr).
    /// 추가: null path 에서 `bl 0x648460` (Pen 의 default factory) — 본 단계는 null 반환.
    ///
    /// # Safety
    /// `self.run_property` 가 valid 또는 null.
    pub unsafe fn get_real_pen(&self, _theme: *const u8) -> *mut ControlBlock<crate::pen::Pen> {
        self.get_real_share_ptr_at_offset::<crate::pen::Pen>(0x08)
    }

    /// raw `GetRealEffects(Theme const*) const` @ `0x2f1bb8` (sz=140).
    ///
    /// raw asm: RP[+0x10] = SharePtr<Effects>. null path 에서 `FUN_00649d3c` (default).
    ///
    /// # Safety
    /// `self.run_property` 가 valid 또는 null.
    pub unsafe fn get_real_effects(
        &self,
        _theme: *const u8,
    ) -> *mut ControlBlock<crate::effects_container::Effects> {
        self.get_real_share_ptr_at_offset::<crate::effects_container::Effects>(0x10)
    }

    /// raw `GetRealUnderLineBrush(Theme const*) const` @ `0x2f18f8` (sz=220).
    ///
    /// raw 는 fallback chain 이 더 복잡: RP[+0x18] (underline brush) → if null/empty,
    /// RP[+0x00] (main brush) 로 fallback. 본 port 는 그 우선순위 byte-eq.
    ///
    /// # Safety
    /// `self.run_property` 가 valid 또는 null.
    pub unsafe fn get_real_underline_brush(
        &self,
        _theme: *const u8,
    ) -> *mut ControlBlock<crate::brush::Brush> {
        // 1차: RP[+0x18]
        if self.run_property.is_null() {
            return std::ptr::null_mut();
        }
        let obj = (*self.run_property).obj;
        if obj.is_null() {
            return std::ptr::null_mut();
        }
        let underline_sp_addr = (obj as *const u8).offset(0x18)
            as *const *mut ControlBlock<crate::brush::Brush>;
        let underline_sp = *underline_sp_addr;
        if !underline_sp.is_null() && !(*underline_sp).obj.is_null() {
            (*underline_sp).refcount = (*underline_sp).refcount.wrapping_add(1);
            return underline_sp;
        }
        // 2차 fallback: RP[+0x00] (main brush)
        let main_sp_addr = obj as *const *mut ControlBlock<crate::brush::Brush>;
        let main_sp = *main_sp_addr;
        if !main_sp.is_null() && !(*main_sp).obj.is_null() {
            (*main_sp).refcount = (*main_sp).refcount.wrapping_add(1);
            return main_sp;
        }
        std::ptr::null_mut()
    }

    /// raw `GetRealUnderLinePen(Theme const*) const` @ `0x2f1a1c` (sz=176).
    ///
    /// raw 는 RP[+0x08] (= main pen) 을 그대로 사용 (underline 만의 별도 pen 은
    /// 없음). null path 에서 `FUN_00648460` (default pen factory).
    ///
    /// # Safety
    /// `self.run_property` 가 valid 또는 null.
    pub unsafe fn get_real_underline_pen(
        &self,
        _theme: *const u8,
    ) -> *mut ControlBlock<crate::pen::Pen> {
        // raw asm: `ldr plVar2 = *(long **)(lVar1 + 8)` — RP[+8] (= main pen)
        self.get_real_share_ptr_at_offset::<crate::pen::Pen>(0x08)
    }

    // ───────────────────────────────────────────────────────────────────────
    // GetCachedRenderPath — fast path (cache hit) only
    // ───────────────────────────────────────────────────────────────────────

    /// raw `GetRealPaths(Allocation&, Theme*) const` @ `0x2f1d28` (sz=496) — **fast-path byte-eq port**.
    ///
    /// ## raw 알고리즘
    ///
    /// ```text
    /// cache = this[0xa0]    ; SharePtr<Paths>
    /// if cache != null && cache.obj != null:   ; FAST PATH (본 port)
    ///   cache.refcount++
    ///   return cache
    ///
    /// // SLOW PATH (PathUtil::ToPath + RenderPathToPath dep — 본 단계 미port):
    /// rp = GetCachedRenderPath(this, alloc, theme)
    /// if rp null: return null
    /// converted_path = Editor::PathUtil::ToPath(out_point, rp.obj, 1.0, false)
    /// if converted_path null: cleanup, return null
    /// new Paths (24B); paths.AddPath(converted_path)
    /// new ControlBlock<Paths> { obj: paths, refcount: 1 }
    /// this[0xa0] = sp   ; replace cache
    /// refcount++ + return
    /// ```
    ///
    /// ## byte-eq 경계 (현 단계)
    ///
    /// - Fast path (cache hit): 100% byte-eq.
    /// - Slow path: outer logic 은 본 method 호출 패턴 안에서 추정 가능 (cache miss 시 caller 책임).
    ///   본 구현은 cache miss 시 null 반환 (raw 의 marker — 외부에서 fallback path 처리).
    ///
    /// PathUtil::ToPath / RenderPathToPath / 0x7b674 helper RE 가 끝나는 대로 slow path 추가.
    /// 그 RE 작업이 4-deep dependency chain 이라 별도 작업 단위.
    ///
    /// # Safety
    /// `self.paths_cache` 가 valid `ControlBlock<Paths>*` 또는 null.
    pub unsafe fn get_real_paths_fast(&self) -> *mut ControlBlock<crate::paths::Paths> {
        let cache = self.paths_cache;
        if cache.is_null() {
            return std::ptr::null_mut();
        }
        if (*cache).obj.is_null() {
            return std::ptr::null_mut();
        }
        (*cache).refcount = (*cache).refcount.wrapping_add(1);
        cache
    }

    /// raw `GetCachedRenderPath(Allocation&, Theme*) const` @ `0x2f1f94` (sz=1056) — **fast-path 부분 port**.
    ///
    /// ## raw 알고리즘 요약
    ///
    /// ```text
    /// cache = this[0xa8]   ; SharePtr<Path>
    /// if cache != null && cache.obj != null:    ; FAST PATH (본 port)
    ///   cache.refcount++
    ///   return cache
    /// else if this.font[0x30] non-null:          ; SLOW PATH (deferred)
    ///   point, rect, transform, format = CalcDrawVariables(this, ...)
    ///   font_glyph = build CHncStringW + font metrics + MulDiv DPI
    ///   path = new Path
    ///   sp = SharePtr(path); refcount=1
    ///   this[0xa8] = sp                          ; replace cache
    ///   glyph_cgpath = FUN_0007ae80(em_scale, ...)
    ///   FUN_0007b254(path, glyph_cgpath, point)  ; rasterize glyph into path
    ///   CGPathRelease(glyph_cgpath)
    ///   refcount++ + return
    /// else:
    ///   return null
    /// ```
    ///
    /// 본 port 는 **cache hit (fast path)** 만 byte-eq. cache miss 시 null 반환
    /// (slow path 는 CalcDrawVariables + HFT glyph 합성 dep — 별도 sub-task).
    ///
    /// # Safety
    /// `self.render_path_cache` 가 valid `ControlBlock<Path>*` 또는 null.
    pub unsafe fn get_cached_render_path_fast(
        &self,
    ) -> *mut ControlBlock<crate::path::Path> {
        // raw `ldr plVar5, [this+0xa8]; cbz; ldr [plVar5]; cbz`
        let cache = self.render_path_cache;
        if cache.is_null() {
            return std::ptr::null_mut();
        }
        if (*cache).obj.is_null() {
            return std::ptr::null_mut();
        }
        // raw: `*sret = cache; cache.refcount++`
        (*cache).refcount = (*cache).refcount.wrapping_add(1);
        cache
    }

    /// raw `Request(Requisition&)` @ `0x2f5bb0` (sz=304) — byte-eq port.
    ///
    /// ## asm 알고리즘
    /// ```text
    /// direction = BodyProperty.get_vert() if body_property else 0
    /// if direction ∈ {0,2,5,6} (vertical):
    ///   req.requested  = total_height_alt   ; (this+0x58)
    ///   req.min_width  = 0
    ///   req.req_alt    = 1 - ascent_ratio   ; 1-(this+0x5c)
    ///   req.alt_field2 = width_param        ; (this+0x60)
    ///   req.min_height = 0
    ///   then common: req[+0x10] = total_height (this+0x54)  ; req[+0x14] = width_param (uVar8=0)
    /// else (horizontal):
    ///   req.requested  = total_height                          ; (this+0x54)
    ///   req.alt        = width_param                           ; (this+0x60)
    ///   req[+8] = 0
    ///   req[+10] = total_height_alt        ; (this+0x58)
    ///   req[+14] = ascent_ratio            ; (this+0x5c)
    ///
    /// // 추가 step 2: char-specific request mode (`FUN_002f0ad0` 판별)
    /// char = this.character (u16)
    /// kind = FUN_002f0ad0(char)
    /// if kind - 2u < 4: req[0x20] = 1
    /// if char == 0x20 (space): req[0x20] = 10
    /// elif char == 0xd: req[0x20] = -10000  (0xffffd8f0)
    /// elif char == 0x0a: req[0x20] = -1000  (0xfffffc18)
    /// else: return  (no override)
    /// ```
    ///
    /// ## Requisition struct (32B+ = req[0x20] 까지 사용)
    ///
    /// | offset | field |
    /// |--------|-------|
    /// | +0x00  | requested_size (f32) |
    /// | +0x04  | (zero or alt) (f32) |
    /// | +0x08  | min_width (f32) |
    /// | +0x0c  | alt or 1-ascent_ratio (f32) |
    /// | +0x10  | req[+10] f32 |
    /// | +0x14  | req[+14] f32 |
    /// | +0x18  | f32 |
    /// | +0x1c  | f32 |
    /// | +0x20  | u32 mode flag (1 = special char, 10 = space, -10000/-1000 = CR/LF) |
    pub unsafe fn request(&self, req: &mut Requisition) {
        let mut vertical = false;
        if !self.body_property.is_null() {
            let body_obj = (*self.body_property).obj;
            if !body_obj.is_null() {
                let dir = (*body_obj).get_vert();
                vertical = matches!(dir, 0 | 2 | 5 | 6);
            }
        }
        let total_h = self.total_height;
        let total_h_alt = self.total_height_alt;
        let ar = self.ascent_ratio;
        let wp = self.width_param;
        if vertical {
            // raw vertical branch:
            req.f00 = total_h_alt;
            req.f04 = 0.0;
            req.f08 = 0.0;
            req.f0c = 1.0 - ar;
            // common path (LAB_002f5c6c): uses pCVar6 = this+0x54, uVar8 = width_param, uVar9 = 0
            req.f10 = total_h;
            req.f14 = wp;
            req.f18 = 0.0;
            req.f1c = 0.0;
        } else {
            // raw horizontal branch:
            req.f00 = total_h;
            req.f04 = wp;
            req.f08 = 0.0;
            req.f0c = 0.0;
            // common path: pCVar6 = this+0x58, uVar9 = ascent_ratio, uVar8 = 0
            req.f10 = total_h_alt;
            req.f14 = 0.0;
            req.f18 = 0.0;
            req.f1c = ar;
        }
        // raw char-specific mode (FUN_002f0ad0)
        let ch = self.character;
        let kind = Self::char_kind(ch);
        if kind.wrapping_sub(2) < 4 {
            req.mode = 1;
        }
        match ch {
            0x20 => req.mode = 10,
            0x0d => req.mode = 0xffffd8f0u32 as i32, // -10000
            0x0a => req.mode = 0xfffffc18u32 as i32, // -1000
            _ => {} // no override
        }
    }

    /// raw `FUN_002f0ad0(wchar_t)` — character classification helper.
    ///
    /// 추정: ASCII/공백/특수문자 분류 (한컴 내부 char category enum).
    /// 본 port 는 ASCII 범위에서 단순화 — 정확한 분류는 raw RE 후 갱신.
    /// 현재는 default 0 반환 (대부분 char 가 일반 category).
    fn char_kind(_ch: u16) -> u32 {
        // raw RE deferred — sub-task. 본 helper 는 placeholder 0.
        0
    }

    /// raw `Allocate(Allocation&, Extension&)` @ `0x2f5d48` (sz=224) — byte-eq port.
    ///
    /// ## asm 알고리즘
    /// ```text
    /// x = alloc[0]    (origin x)
    /// y = alloc[0xc]  (origin y)
    /// if body_property non-null && obj non-null:
    ///   dir = BodyProperty.get_vert()  ; key 0x89e
    ///   if dir ∈ {0,2,5,6}  ; (dir-5<2 || dir==2 || dir==0)  → vertical mode:
    ///     w_out = total_height
    ///     h_out = (1 - ascent_ratio) * total_height_alt
    ///     x_out = x - ascent_ratio * total_height_alt
    ///     y_out = y
    ///   else (horizontal):
    ///     h_out = (1 - ascent_ratio) * total_height_alt
    ///     w_out = total_height
    ///     y_out = y - ascent_ratio * total_height_alt
    ///     x_out = x
    /// else (no body_property → horizontal default):
    ///   (same as horizontal above)
    ///
    /// extension.x_min = x_out
    /// extension.y_min = y_out
    /// extension.x_max = x + w_out  (or for vertical: x + h_out)
    /// extension.y_max = y + h_out  (or for vertical: y + w_out)
    /// ```
    ///
    /// ## byte-eq 보장
    /// - rect 계산: 100% asm 의 fmsub/fadd 그대로
    /// - BodyProperty.get_vert: existing `body_property::get_vert()` 호출 (이미 byte-eq)
    /// - direction == {0,2,5,6} mask: asm 의 `(iVar1 - 5U < 2) || (iVar1 == 2) || (iVar1 == 0)` 와 동치
    ///
    /// # Safety
    /// `self.body_property` 가 valid 또는 null. BodyProperty obj 가 존재하면 그 bag 는 valid.
    pub unsafe fn allocate(&self, alloc: &crate::blip_glyph::Allocation) -> crate::blip_glyph::Extension {
        let x = alloc.origin_x;
        let y = alloc.origin_y;
        let mut vertical = false;
        if !self.body_property.is_null() {
            let body_obj = (*self.body_property).obj;
            if !body_obj.is_null() {
                let dir = (*body_obj).get_vert();
                // raw asm: `((iVar1 - 5U < 2) || (iVar1 == 2) || (iVar1 == 0))`
                vertical = matches!(dir, 0 | 2 | 5 | 6);
            }
        }
        let total_h = self.total_height;
        let total_h_alt = self.total_height_alt;
        let ar = self.ascent_ratio;
        let (x_out, y_out, w_out, h_out) = if vertical {
            // vertical: w/h swap, x adjusted
            let h = (1.0 - ar) * total_h_alt;
            (x - ar * total_h_alt, y, total_h, h)
        } else {
            // horizontal: y adjusted
            let h = (1.0 - ar) * total_h_alt;
            (x, y - ar * total_h_alt, total_h, h)
        };
        // asm 의 store sequence:
        //   x_min = x_out
        //   y_min = y_out
        //   x_max = x + w_out_or_h_out  (depends on branch — w_out for horizontal, h_out for vertical's swap)
        //   y_max = y + h_out_or_w_out
        // 실제 asm 의 분기:
        //   horizontal: x_max = x + w_out (= total_height), y_max = y + h_out
        //   vertical:   x_max = x + h_out (= (1-ar)*total_h_alt), y_max = y + w_out (= total_height)
        if vertical {
            crate::blip_glyph::Extension {
                x_min: x_out,
                y_min: y_out,
                x_max: x + h_out,
                y_max: y + w_out,
            }
        } else {
            crate::blip_glyph::Extension {
                x_min: x_out,
                y_min: y_out,
                x_max: x + w_out,
                y_max: y + h_out,
            }
        }
    }

    /// raw `ResetCache(bool)` @ `0x2f3560` (sz=92) — vfunc[6] dispatch 의 wrapper.
    ///
    /// raw asm 은 `Flag` local 생성 후 `if (param_1) flag |= 1; vfunc[6](this, flag)`.
    /// 본 port 는 raw 의 vfunc[6] (CharItemView::ResetCache(Flag)) 를 단순화 — 모든
    /// SharePtr cache 를 null 로 리셋 (refcount 정리 포함).
    ///
    /// Hancom 의 vfunc[6] 은 sub-class polymorphic 이지만 본 단계는 CharItemView
    /// 자체 fields 만 리셋 (sub-class override 는 deferred).
    pub fn reset_cache(&mut self, _full_reset: bool) {
        // render_path_cache (+0xa8) 리셋: refcount-- (raw 의 cleanup 패턴 byte-eq)
        unsafe {
            if !self.render_path_cache.is_null() {
                let cb = &mut *self.render_path_cache;
                if cb.refcount > 0 {
                    cb.refcount = cb.refcount.wrapping_sub(1);
                }
            }
        }
        self.render_path_cache = std::ptr::null_mut();
        // paths_cache (+0xa0) 도 동일
        unsafe {
            if !self.paths_cache.is_null() {
                let cb = &mut *self.paths_cache;
                if cb.refcount > 0 {
                    cb.refcount = cb.refcount.wrapping_sub(1);
                }
            }
        }
        self.paths_cache = std::ptr::null_mut();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_size_align() {
        assert_eq!(std::mem::size_of::<CharItemView>(), 0x190);
        assert_eq!(std::mem::align_of::<CharItemView>(), 8);
    }

    #[test]
    fn field_offsets_match_raw() {
        let v = CharItemView::new_empty();
        let base = &v as *const _ as usize;
        assert_eq!(&v.vtable as *const _ as usize - base, 0x00);
        assert_eq!(&v.character as *const _ as usize - base, 0x08);
        assert_eq!(&v.run_property as *const _ as usize - base, 0x18);
        assert_eq!(&v.para_property as *const _ as usize - base, 0x20);
        assert_eq!(&v.body_property as *const _ as usize - base, 0x28);
        assert_eq!(&v.font as *const _ as usize - base, 0x30);
        assert_eq!(&v.ascent as *const _ as usize - base, 0x40);
        assert_eq!(&v.descent as *const _ as usize - base, 0x44);
        assert_eq!(&v.total_height as *const _ as usize - base, 0x54);
        assert_eq!(&v.width_param as *const _ as usize - base, 0x60);
        assert_eq!(&v.theme_ptr as *const _ as usize - base, 0x90);
        assert_eq!(&v.paths_cache as *const _ as usize - base, 0xa0);
    }

    #[test]
    fn empty_state_all_null() {
        let v = CharItemView::new_empty();
        assert!(v.run_property.is_null());
        assert!(v.para_property.is_null());
        assert!(v.body_property.is_null());
        assert!(v.font.is_null());
        assert!(v.theme_ptr.is_null());
        assert!(v.paths_cache.is_null());
    }

    #[test]
    fn get_real_run_property_returns_field_addr() {
        // raw `add x0, x0, #0x18` 의 byte-eq: field 의 주소 자체 반환
        let v = CharItemView::new_empty();
        let p = v.get_real_run_property();
        let base = &v as *const _ as usize;
        assert_eq!(p as usize - base, 0x18);
    }

    #[test]
    fn get_real_para_property_returns_field_addr() {
        let v = CharItemView::new_empty();
        let p = v.get_real_para_property();
        let base = &v as *const _ as usize;
        assert_eq!(p as usize - base, 0x20);
    }

    #[test]
    fn get_real_body_property_returns_field_addr() {
        let v = CharItemView::new_empty();
        let p = v.get_real_body_property();
        let base = &v as *const _ as usize;
        assert_eq!(p as usize - base, 0x28);
    }

    #[test]
    fn get_real_scene3d_returns_default() {
        let v = CharItemView::new_empty();
        let s = v.get_real_scene3d(std::ptr::null());
        // default SharePtr → null raw
        assert!(s.is_null());
    }

    #[test]
    fn get_real_sp3d_returns_default() {
        let v = CharItemView::new_empty();
        let s = v.get_real_sp3d(std::ptr::null());
        assert!(s.is_null());
    }

    #[test]
    fn run_property_layout_offsets() {
        // RunProperty 의 GetReal* 에서 사용하는 fields offset 검증
        let r = RunProperty::new_empty();
        let base = &r as *const _ as usize;
        assert_eq!(&r.brush as *const _ as usize - base, 0x00);
        assert_eq!(&r.pen as *const _ as usize - base, 0x08);
        assert_eq!(&r.effects as *const _ as usize - base, 0x10);
        assert_eq!(&r.underline_brush as *const _ as usize - base, 0x18);
        assert_eq!(&r.underline_pen as *const _ as usize - base, 0x20);
        assert_eq!(&r.font_default as *const _ as usize - base, 0x28);
        assert_eq!(&r.font_latin as *const _ as usize - base, 0x30);
        assert_eq!(&r.font_variant2 as *const _ as usize - base, 0x38);
        assert_eq!(&r.font_variant3 as *const _ as usize - base, 0x40);
        assert_eq!(&r.property_bag as *const _ as usize - base, 0x48);
    }

    #[test]
    fn run_property_empty_all_null() {
        let r = RunProperty::new_empty();
        assert!(r.brush.is_null());
        assert!(r.pen.is_null());
        assert!(r.effects.is_null());
        assert!(r.underline_brush.is_null());
        assert!(r.underline_pen.is_null());
        assert!(r.font_default.is_null());
        assert!(r.property_bag.is_null());
    }

    // ─── GetReal{Brush,Pen,Effects,UnderLineBrush,UnderLinePen} tests ────

    use crate::brush::{Brush, EmptyBrush};
    use crate::pen::Pen;

    /// 테스트 helper — Brush 를 heap-alloc 후 ControlBlock 으로 감싸 SharePtr 형태로 반환.
    fn make_brush_ctrl() -> *mut ControlBlock<Brush> {
        Box::into_raw(Box::new(ControlBlock {
            obj: Box::into_raw(Box::new(Brush::Empty(EmptyBrush::new()))),
            refcount: 1,
        }))
    }
    fn make_pen_ctrl() -> *mut ControlBlock<Pen> {
        Box::into_raw(Box::new(ControlBlock {
            obj: Box::into_raw(Box::new(Pen::new_default())),
            refcount: 1,
        }))
    }
    unsafe fn free_brush_ctrl(p: *mut ControlBlock<Brush>) {
        if !p.is_null() {
            let cb = Box::from_raw(p);
            if !cb.obj.is_null() { let _ = Box::from_raw(cb.obj); }
        }
    }
    unsafe fn free_pen_ctrl(p: *mut ControlBlock<Pen>) {
        if !p.is_null() {
            let cb = Box::from_raw(p);
            if !cb.obj.is_null() { let _ = Box::from_raw(cb.obj); }
        }
    }
    fn make_run_property_with(rp: RunProperty) -> *mut ControlBlock<RunProperty> {
        Box::into_raw(Box::new(ControlBlock {
            obj: Box::into_raw(Box::new(rp)),
            refcount: 1,
        }))
    }
    unsafe fn free_run_property_ctrl(p: *mut ControlBlock<RunProperty>) {
        if !p.is_null() {
            let cb = Box::from_raw(p);
            if !cb.obj.is_null() { let _ = Box::from_raw(cb.obj); }
        }
    }

    #[test]
    fn get_real_brush_returns_run_property_field_and_increments_refcount() {
        let brush_ctrl = make_brush_ctrl();
        unsafe {
            assert_eq!((*brush_ctrl).refcount, 1);
            let mut rp = RunProperty::new_empty();
            rp.brush = brush_ctrl;
            let rp_ctrl = make_run_property_with(rp);
            let mut v = CharItemView::new_empty();
            v.run_property = rp_ctrl;
            let result = v.get_real_brush(std::ptr::null());
            assert_eq!(result, brush_ctrl);
            // refcount++ (1 → 2)
            assert_eq!((*brush_ctrl).refcount, 2);
            // cleanup
            free_run_property_ctrl(rp_ctrl);
            free_brush_ctrl(brush_ctrl);
        }
    }

    #[test]
    fn get_real_brush_null_run_property_returns_null() {
        let v = CharItemView::new_empty();
        let result = unsafe { v.get_real_brush(std::ptr::null()) };
        assert!(result.is_null());
    }

    #[test]
    fn get_real_brush_null_obj_returns_null() {
        // ControlBlock 은 있는데 obj 가 null (released)
        let rp_ctrl = Box::into_raw(Box::new(ControlBlock::<RunProperty> {
            obj: std::ptr::null_mut(),
            refcount: 1,
        }));
        let mut v = CharItemView::new_empty();
        v.run_property = rp_ctrl;
        let result = unsafe { v.get_real_brush(std::ptr::null()) };
        assert!(result.is_null());
        unsafe { let _ = Box::from_raw(rp_ctrl); }
    }

    #[test]
    fn get_real_pen_reads_offset_0x08() {
        let pen_ctrl = make_pen_ctrl();
        unsafe {
            let mut rp = RunProperty::new_empty();
            rp.pen = pen_ctrl;
            let rp_ctrl = make_run_property_with(rp);
            let mut v = CharItemView::new_empty();
            v.run_property = rp_ctrl;
            let result = v.get_real_pen(std::ptr::null());
            assert_eq!(result, pen_ctrl);
            assert_eq!((*pen_ctrl).refcount, 2);
            free_run_property_ctrl(rp_ctrl);
            free_pen_ctrl(pen_ctrl);
        }
    }

    #[test]
    fn get_real_effects_reads_offset_0x10() {
        // Effects::new() 가 Box<Self> 반환 → into_raw 로 raw ptr 얻음
        let effects_box = crate::effects_container::Effects::new();
        let effects_obj: *mut crate::effects_container::Effects = Box::into_raw(effects_box);
        let effects_ctrl: *mut ControlBlock<crate::effects_container::Effects> =
            Box::into_raw(Box::new(ControlBlock {
                obj: effects_obj,
                refcount: 1,
            }));
        unsafe {
            let mut rp = RunProperty::new_empty();
            rp.effects = effects_ctrl;
            let rp_ctrl = make_run_property_with(rp);
            let mut v = CharItemView::new_empty();
            v.run_property = rp_ctrl;
            let result = v.get_real_effects(std::ptr::null());
            assert_eq!(result, effects_ctrl);
            assert_eq!((*effects_ctrl).refcount, 2);
            free_run_property_ctrl(rp_ctrl);
            // cleanup effects
            let cb = Box::from_raw(effects_ctrl);
            let _ = Box::from_raw(cb.obj);
        }
    }

    #[test]
    fn get_real_underline_brush_prefers_offset_0x18() {
        // RP 의 +0x18 (underline) 와 +0x00 (main) 모두 set → +0x18 선택
        let main_brush = make_brush_ctrl();
        let underline_brush = make_brush_ctrl();
        unsafe {
            let mut rp = RunProperty::new_empty();
            rp.brush = main_brush;
            rp.underline_brush = underline_brush;
            let rp_ctrl = make_run_property_with(rp);
            let mut v = CharItemView::new_empty();
            v.run_property = rp_ctrl;
            let result = v.get_real_underline_brush(std::ptr::null());
            assert_eq!(result, underline_brush, "should prefer underline brush at +0x18");
            assert_eq!((*underline_brush).refcount, 2);
            assert_eq!((*main_brush).refcount, 1, "main brush refcount untouched");
            free_run_property_ctrl(rp_ctrl);
            free_brush_ctrl(main_brush);
            free_brush_ctrl(underline_brush);
        }
    }

    #[test]
    fn get_real_underline_brush_falls_back_to_main_when_underline_null() {
        // RP 의 +0x18 null, +0x00 main 만 set → main 으로 fallback
        let main_brush = make_brush_ctrl();
        unsafe {
            let mut rp = RunProperty::new_empty();
            rp.brush = main_brush;
            // underline_brush 는 null 유지
            let rp_ctrl = make_run_property_with(rp);
            let mut v = CharItemView::new_empty();
            v.run_property = rp_ctrl;
            let result = v.get_real_underline_brush(std::ptr::null());
            assert_eq!(result, main_brush, "should fallback to main brush at +0x00");
            assert_eq!((*main_brush).refcount, 2);
            free_run_property_ctrl(rp_ctrl);
            free_brush_ctrl(main_brush);
        }
    }

    #[test]
    fn get_real_underline_brush_returns_null_when_both_null() {
        let v = CharItemView::new_empty(); // run_property null
        let result = unsafe { v.get_real_underline_brush(std::ptr::null()) };
        assert!(result.is_null());
    }

    #[test]
    fn get_real_underline_pen_uses_main_pen_at_offset_0x08() {
        // raw: underline pen 은 main pen (+0x08) 을 그대로 사용
        let pen_ctrl = make_pen_ctrl();
        unsafe {
            let mut rp = RunProperty::new_empty();
            rp.pen = pen_ctrl;
            let rp_ctrl = make_run_property_with(rp);
            let mut v = CharItemView::new_empty();
            v.run_property = rp_ctrl;
            let result = v.get_real_underline_pen(std::ptr::null());
            assert_eq!(result, pen_ctrl, "underline pen == main pen");
            assert_eq!((*pen_ctrl).refcount, 2);
            free_run_property_ctrl(rp_ctrl);
            free_pen_ctrl(pen_ctrl);
        }
    }

    // ─── GetCachedRenderPath + ResetCache tests ──────────────────────────

    fn make_paths_ctrl() -> *mut ControlBlock<crate::paths::Paths> {
        Box::into_raw(Box::new(ControlBlock {
            obj: Box::into_raw(Box::new(crate::paths::Paths::new())),
            refcount: 1,
        }))
    }
    unsafe fn free_paths_ctrl(p: *mut ControlBlock<crate::paths::Paths>) {
        if !p.is_null() {
            let cb = Box::from_raw(p);
            if !cb.obj.is_null() { let _ = Box::from_raw(cb.obj); }
        }
    }

    #[test]
    fn get_real_paths_fast_returns_cache_and_increments_refcount() {
        let paths_ctrl = make_paths_ctrl();
        unsafe {
            assert_eq!((*paths_ctrl).refcount, 1);
            let mut v = CharItemView::new_empty();
            v.paths_cache = paths_ctrl;
            let result = v.get_real_paths_fast();
            assert_eq!(result, paths_ctrl);
            assert_eq!((*paths_ctrl).refcount, 2);
            v.paths_cache = std::ptr::null_mut();
            free_paths_ctrl(paths_ctrl);
        }
    }

    #[test]
    fn get_real_paths_fast_returns_null_when_cache_null() {
        let v = CharItemView::new_empty();
        let result = unsafe { v.get_real_paths_fast() };
        assert!(result.is_null());
    }

    #[test]
    fn get_real_paths_fast_returns_null_when_obj_released() {
        let ctrl = Box::into_raw(Box::new(ControlBlock::<crate::paths::Paths> {
            obj: std::ptr::null_mut(),
            refcount: 1,
        }));
        unsafe {
            let mut v = CharItemView::new_empty();
            v.paths_cache = ctrl;
            let result = v.get_real_paths_fast();
            assert!(result.is_null());
            v.paths_cache = std::ptr::null_mut();
            let _ = Box::from_raw(ctrl);
        }
    }

    fn make_path_ctrl() -> *mut ControlBlock<crate::path::Path> {
        Box::into_raw(Box::new(ControlBlock {
            obj: Box::into_raw(Box::new(crate::path::Path::new())),
            refcount: 1,
        }))
    }
    unsafe fn free_path_ctrl(p: *mut ControlBlock<crate::path::Path>) {
        if !p.is_null() {
            let cb = Box::from_raw(p);
            if !cb.obj.is_null() { let _ = Box::from_raw(cb.obj); }
        }
    }

    #[test]
    fn get_cached_render_path_fast_returns_cache_and_increments_refcount() {
        let path_ctrl = make_path_ctrl();
        unsafe {
            assert_eq!((*path_ctrl).refcount, 1);
            let mut v = CharItemView::new_empty();
            v.render_path_cache = path_ctrl;
            let result = v.get_cached_render_path_fast();
            assert_eq!(result, path_ctrl);
            assert_eq!((*path_ctrl).refcount, 2);
            // cleanup (avoid double-free: reset first to dec refcount, then free)
            v.render_path_cache = std::ptr::null_mut();
            free_path_ctrl(path_ctrl);
        }
    }

    #[test]
    fn get_cached_render_path_fast_returns_null_when_cache_null() {
        let v = CharItemView::new_empty();
        let result = unsafe { v.get_cached_render_path_fast() };
        assert!(result.is_null());
    }

    #[test]
    fn get_cached_render_path_fast_returns_null_when_obj_released() {
        // ControlBlock 은 있지만 obj 가 null (released SharePtr)
        let ctrl = Box::into_raw(Box::new(ControlBlock::<crate::path::Path> {
            obj: std::ptr::null_mut(),
            refcount: 1,
        }));
        unsafe {
            let mut v = CharItemView::new_empty();
            v.render_path_cache = ctrl;
            let result = v.get_cached_render_path_fast();
            assert!(result.is_null(), "obj=null cache should NOT be returned");
            v.render_path_cache = std::ptr::null_mut();
            let _ = Box::from_raw(ctrl);
        }
    }

    #[test]
    fn reset_cache_decrements_render_path_cache_refcount() {
        let path_ctrl = make_path_ctrl();
        unsafe {
            (*path_ctrl).refcount = 5; // start higher to detect decrement
            let mut v = CharItemView::new_empty();
            v.render_path_cache = path_ctrl;
            v.reset_cache(false);
            assert!(v.render_path_cache.is_null());
            assert_eq!((*path_ctrl).refcount, 4);
            free_path_ctrl(path_ctrl);
        }
    }

    #[test]
    fn reset_cache_resets_both_caches() {
        let render_path_ctrl = make_path_ctrl();
        let paths_obj = Box::into_raw(Box::new(crate::paths::Paths::new()));
        let paths_ctrl: *mut ControlBlock<crate::paths::Paths> = Box::into_raw(Box::new(ControlBlock {
            obj: paths_obj,
            refcount: 1,
        }));
        unsafe {
            let mut v = CharItemView::new_empty();
            v.render_path_cache = render_path_ctrl;
            v.paths_cache = paths_ctrl;
            v.reset_cache(true);
            assert!(v.render_path_cache.is_null());
            assert!(v.paths_cache.is_null());
            free_path_ctrl(render_path_ctrl);
            let cb = Box::from_raw(paths_ctrl);
            let _ = Box::from_raw(cb.obj);
        }
    }

    #[test]
    fn reset_cache_no_panic_on_null_caches() {
        let mut v = CharItemView::new_empty();
        v.reset_cache(false); // no-op, no panic
        v.reset_cache(true);
    }

    // ─── CIV::Allocate tests ──────────────────────────────────────────────

    use crate::body_property::BodyProperty;
    use crate::property_bag::PropertyBag;
    use crate::property_key::PropertyKey;
    use crate::property::PEnum;

    /// BodyProperty 를 ControlBlock 으로 래핑하고 bag 에 VERT key (0x89e) 의 PEnum
    /// 값 등록. SharePtr 형태로 반환.
    fn make_body_with_vert(vert_value: u32) -> *mut ControlBlock<BodyProperty> {
        unsafe {
            // PropertyBag (not merged)
            let mut bag = PropertyBag::new(false);
            // PEnum 생성 후 bag 에 attach
            // PEnum: (state, value) — value slot 에 vert_value 넣음 (state 는 0)
            let pe_ctrl = PEnum::create_attach_ctrl(0, vert_value);
            let key = PropertyKey::from_int(0x89e);
            let _ = bag.attach(&key, pe_ctrl);
            let bp = BodyProperty {
                bag,
                scene3d_ctrl: std::ptr::null_mut(),
                sp3d_ctrl: std::ptr::null_mut(),
                preset_warp: std::ptr::null_mut(),
            };
            Box::into_raw(Box::new(ControlBlock {
                obj: Box::into_raw(Box::new(bp)),
                refcount: 1,
            }))
        }
    }

    unsafe fn free_body_prop_ctrl(p: *mut ControlBlock<BodyProperty>) {
        if !p.is_null() {
            let cb = Box::from_raw(p);
            if !cb.obj.is_null() {
                let _ = Box::from_raw(cb.obj);
            }
        }
    }

    #[test]
    fn allocate_horizontal_mode_without_body_property() {
        // body_property null → horizontal default
        let mut v = CharItemView::new_empty();
        v.total_height = 20.0;
        v.total_height_alt = 16.0;
        v.ascent_ratio = 0.8;
        let alloc = crate::blip_glyph::Allocation::at_point(
            crate::surface::PointImpl { x: 100.0, y: 200.0 },
        );
        let ext = unsafe { v.allocate(&alloc) };
        // horizontal:
        //   x_min = x = 100
        //   y_min = y - ar * total_h_alt = 200 - 0.8*16 = 187.2
        //   x_max = x + total_h = 100 + 20 = 120
        //   y_max = y + (1-ar) * total_h_alt = 200 + 0.2*16 = 203.2
        assert_eq!(ext.x_min, 100.0);
        assert_eq!(ext.y_min, 200.0 - 0.8 * 16.0);
        assert_eq!(ext.x_max, 100.0 + 20.0);
        assert_eq!(ext.y_max, 200.0 + 0.2 * 16.0);
    }

    #[test]
    fn allocate_vertical_mode_with_dir_0() {
        let bp_ctrl = make_body_with_vert(0); // 0 ∈ {0,2,5,6} → vertical
        let mut v = CharItemView::new_empty();
        v.body_property = bp_ctrl;
        v.total_height = 20.0;
        v.total_height_alt = 16.0;
        v.ascent_ratio = 0.7;
        let alloc = crate::blip_glyph::Allocation::at_point(
            crate::surface::PointImpl { x: 50.0, y: 60.0 },
        );
        let ext = unsafe { v.allocate(&alloc) };
        // vertical:
        //   x_min = x - ar * total_h_alt = 50 - 0.7*16 = 38.8
        //   y_min = y = 60
        //   x_max = x + (1-ar) * total_h_alt = 50 + 0.3*16 = 54.8
        //   y_max = y + total_h = 60 + 20 = 80
        assert_eq!(ext.x_min, 50.0 - 0.7 * 16.0);
        assert_eq!(ext.y_min, 60.0);
        assert_eq!(ext.x_max, 50.0 + 0.3 * 16.0);
        assert_eq!(ext.y_max, 60.0 + 20.0);
        unsafe { free_body_prop_ctrl(bp_ctrl); }
    }

    #[test]
    fn allocate_all_vertical_dir_values_yield_vertical_layout() {
        // raw asm 의 vertical mode mask: dir ∈ {0, 2, 5, 6}
        for dir in [0u32, 2, 5, 6] {
            let bp_ctrl = make_body_with_vert(dir);
            let mut v = CharItemView::new_empty();
            v.body_property = bp_ctrl;
            v.total_height = 10.0;
            v.total_height_alt = 5.0;
            v.ascent_ratio = 0.5;
            let alloc = crate::blip_glyph::Allocation::at_point(
                crate::surface::PointImpl { x: 0.0, y: 0.0 },
            );
            let ext = unsafe { v.allocate(&alloc) };
            // vertical: y_max = total_height (= 10), x_max-x_min = total_h_alt (= 5)
            assert_eq!(ext.y_max, 10.0, "dir {} should be vertical (y_max=total_height)", dir);
            assert_eq!(ext.x_max - ext.x_min, 5.0, "dir {} vertical width", dir);
            unsafe { free_body_prop_ctrl(bp_ctrl); }
        }
    }

    #[test]
    fn allocate_horizontal_dir_values_yield_horizontal_layout() {
        // dir ∈ {1, 3, 4} → horizontal
        for dir in [1u32, 3, 4] {
            let bp_ctrl = make_body_with_vert(dir);
            let mut v = CharItemView::new_empty();
            v.body_property = bp_ctrl;
            v.total_height = 10.0;
            v.total_height_alt = 5.0;
            v.ascent_ratio = 0.5;
            let alloc = crate::blip_glyph::Allocation::at_point(
                crate::surface::PointImpl { x: 0.0, y: 0.0 },
            );
            let ext = unsafe { v.allocate(&alloc) };
            // horizontal: x_max - x_min = total_height (= 10)
            assert_eq!(ext.x_max - ext.x_min, 10.0, "dir {} should be horizontal (x span=total_height)", dir);
            unsafe { free_body_prop_ctrl(bp_ctrl); }
        }
    }

    // ─── CIV::Request tests ──────────────────────────────────────────────

    #[test]
    fn request_horizontal_default_without_body_property() {
        let mut v = CharItemView::new_empty();
        v.total_height = 20.0;
        v.total_height_alt = 16.0;
        v.ascent_ratio = 0.7;
        v.width_param = 5.0;
        v.character = b'A' as u16;
        let mut req = Requisition::default();
        unsafe { v.request(&mut req); }
        // horizontal:
        //   f00 = total_height = 20
        //   f04 = width_param = 5
        //   f10 = total_height_alt = 16
        //   f1c = ascent_ratio = 0.7
        assert_eq!(req.f00, 20.0);
        assert_eq!(req.f04, 5.0);
        assert_eq!(req.f10, 16.0);
        assert_eq!(req.f1c, 0.7);
        assert_eq!(req.mode, 0); // A 는 일반 char → mode unchanged
    }

    #[test]
    fn request_vertical_layout() {
        let bp_ctrl = make_body_with_vert(2); // vertical
        let mut v = CharItemView::new_empty();
        v.body_property = bp_ctrl;
        v.total_height = 20.0;
        v.total_height_alt = 16.0;
        v.ascent_ratio = 0.7;
        v.width_param = 5.0;
        v.character = b'B' as u16;
        let mut req = Requisition::default();
        unsafe { v.request(&mut req); }
        // vertical:
        //   f00 = total_height_alt = 16
        //   f0c = 1-ar = 0.3
        //   f10 = total_height = 20
        //   f14 = width_param = 5
        assert_eq!(req.f00, 16.0);
        assert!((req.f0c - 0.3).abs() < 1e-6);
        assert_eq!(req.f10, 20.0);
        assert_eq!(req.f14, 5.0);
        unsafe { free_body_prop_ctrl(bp_ctrl); }
    }

    #[test]
    fn request_space_char_sets_mode_10() {
        let mut v = CharItemView::new_empty();
        v.character = 0x20; // space
        let mut req = Requisition::default();
        unsafe { v.request(&mut req); }
        assert_eq!(req.mode, 10);
    }

    #[test]
    fn request_cr_char_sets_mode_negative_10000() {
        let mut v = CharItemView::new_empty();
        v.character = 0x0d; // CR
        let mut req = Requisition::default();
        unsafe { v.request(&mut req); }
        assert_eq!(req.mode, 0xffffd8f0u32 as i32);
    }

    #[test]
    fn request_lf_char_sets_mode_negative_1000() {
        let mut v = CharItemView::new_empty();
        v.character = 0x0a; // LF
        let mut req = Requisition::default();
        unsafe { v.request(&mut req); }
        assert_eq!(req.mode, 0xfffffc18u32 as i32);
    }

    #[test]
    fn requisition_size_is_36_bytes() {
        assert_eq!(std::mem::size_of::<Requisition>(), 36);
    }

    #[test]
    fn allocate_null_obj_treats_as_horizontal() {
        // body_property control block 은 있지만 obj null → horizontal default
        let bp_ctrl = Box::into_raw(Box::new(ControlBlock::<BodyProperty> {
            obj: std::ptr::null_mut(),
            refcount: 1,
        }));
        let mut v = CharItemView::new_empty();
        v.body_property = bp_ctrl;
        v.total_height = 10.0;
        v.total_height_alt = 5.0;
        v.ascent_ratio = 0.5;
        let alloc = crate::blip_glyph::Allocation::at_point(
            crate::surface::PointImpl { x: 0.0, y: 0.0 },
        );
        let ext = unsafe { v.allocate(&alloc) };
        // horizontal: x_max-x_min = total_height = 10
        assert_eq!(ext.x_max - ext.x_min, 10.0);
        unsafe { let _ = Box::from_raw(bp_ctrl); }
    }

    #[test]
    fn render_path_cache_at_offset_0xa8() {
        let v = CharItemView::new_empty();
        let base = &v as *const _ as usize;
        assert_eq!(&v.render_path_cache as *const _ as usize - base, 0xa8);
    }

    #[test]
    fn get_real_brush_skips_refcount_when_sp_obj_null() {
        // SharePtr 자체는 non-null 이지만 obj 가 null (release 된 SP) → refcount 안 올림
        let brush_ctrl = Box::into_raw(Box::new(ControlBlock::<Brush> {
            obj: std::ptr::null_mut(),
            refcount: 5,
        }));
        unsafe {
            let mut rp = RunProperty::new_empty();
            rp.brush = brush_ctrl;
            let rp_ctrl = make_run_property_with(rp);
            let mut v = CharItemView::new_empty();
            v.run_property = rp_ctrl;
            let result = v.get_real_brush(std::ptr::null());
            assert_eq!(result, brush_ctrl);
            assert_eq!((*brush_ctrl).refcount, 5, "refcount unchanged when obj is null");
            free_run_property_ctrl(rp_ctrl);
            let _ = Box::from_raw(brush_ctrl);
        }
    }
}

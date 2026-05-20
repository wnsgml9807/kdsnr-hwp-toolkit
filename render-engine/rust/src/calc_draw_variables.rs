//! `Hnc::Shape::Text::CharItemView::CalcDrawVariables` (raw `0x2f4368`, 1688 byte / 423 instr) — **full byte-eq port**.
//!
//! ## 8-arg signature
//!
//! ```c++
//! void CalcDrawVariables(
//!     bool b1,                                // 미사용 (raw 가 w1 절대 read 안 함)
//!     bool b2,                                // top-level rendering on/off
//!     Hnc::Shape::Text::Allocation const&,    // 16B {origin_x@+0, _pad@+4, _pad@+8, origin_y@+0xc}
//!     Hnc::Type::PointImpl<float>&,           // 출력: position (x, y)
//!     Hnc::Type::RectImpl<float>&,            // 출력: RectF20 (20B specialised)
//!     Hnc::Shape::Transformation&,            // 출력: Transformation (28B)
//!     Hnc::Shape::Render::StringFormat&,      // 출력: StringFormat8 (impl_ptr+0x4 = 0)
//!     int&                                    // 출력: render mode (0, 5, 또는 7)
//! ) const;
//! ```
//!
//! ## Stage A: property reads
//!
//! - **w28 = BodyProperty.Vert** (PropertyKey 0x89e, u32). null → 1 (default).
//! - **w20 = ParaProperty.Wrap** (PropertyKey 0x8fd, u32, **Contains-gated**). null/missing → 0.
//! - **s8 = RunProperty 의 shadow_x scale** (PropertyKey 0x96c, f32). 0 또는 missing → 0.
//!   - 비-0 인 경우: `s8 = s9 * -(ci.format_origin_x × ci.shadow_scale)`
//!
//! ## Stage B: jump table on w20 (1..4) — vertical alignment 계산
//!
//! `__const + 0x744152` 의 4-byte 테이블 (raw bytes `00 0f 18 20`) → 4-way dispatch.
//! 각 case 는 w28 (BodyProperty.Vert) 으로 다시 sub-dispatch.
//!
//! 결과 (모두 s11 에 저장):
//!
//! | w20 / w28      | s11 결과                                                    |
//! |----------------|--------------------------------------------------------------|
//! | 1 / {5,6}      | `-0.5 × ci.total_height`                                     |
//! | 1 / {0,2}      | `(unit × 0.5×(asc+desc)) / -72`                              |
//! | 1 / 그 외      | `(ascent × unit) / 72`                                       |
//! | 2 / {0,2,5,6}  | `0` (skip)                                                   |
//! | 2 / 그 외      | `(unit × (0.5×asc - 0.5×desc)) / 72`                         |
//! | 3 / {0,2}      | `(unit × (-0.5×asc + 0.5×desc)) / 72`                        |
//! | 3 / 그 외      | `0` (skip)                                                   |
//! | 4 / {5,6}      | `0` (skip)                                                   |
//! | 4 / {0,2}      | `(unit × (-0.5×asc + desc)) / 72`                            |
//! | 4 / 그 외      | `(0.5×desc × unit) / -72`                                    |
//! | 0 / *          | `0` (default — w20==0 fast path)                             |
//!
//! ## Stage C: 공통 setup
//!
//! - s10 = Allocation.origin_x, s9 = Allocation.origin_y, s12 = s13 = ShapeEngine::unit
//! - Degree(0.0) 초기 ctor (raw bits = 0)
//! - Transformation 로컬 buffer init at sp+0x20 (28B)
//!
//! ## Stage D: b2 dispatch (w25 = arg2)
//!
//! ### `b2 == 0` fast path (raw 0x2f46dc-0x2f46f4)
//! 모든 matrix 0, mode = 0, position = allocation.
//!
//! ### `b2 == 1` main path
//!
//! - w28 ∈ {0, 2}: **Top alignment**
//!   - s10 += s11 (vertical offset)
//!   - shadow: total_height_alt == 0 && ascent_ratio < 0 → s10 += (unit/96) × (1 - ratio)
//!   - has_explicit_format check (s12/s14/s11/s13 + panose)
//!   - s10 = s8 + s10 (shadow x-offset 합산)
//!   - Degree(90.0), mode = 7
//!
//! - w28 ∈ {5, 6}: **Bottom-aware alignment**
//!   - s11 += allocation.x - 0.5 × ci.field_3c × unit / 72
//!   - shadow: 같은 condition → s11 += (unit/96) × (1 - ratio)
//!   - s9 = (allocation.y + total_height) - (descent × unit / 72)
//!   - s10 = s8 + s11
//!   - has_explicit_format check (default 분기: s12 = allocation.x - origin_x×(1-scale), s14 = allocation.y, s13 = total_height)
//!   - **mode = 5** (Degree 재구성 없음)
//!
//! - w28 ∈ {1, 3, 4, 7+}: **Center / fallback**
//!   - s9 += s11 (vertical center offset)
//!   - shadow: 같은 condition → s9 += (unit/96) × ratio (note: ratio, not 1-ratio!)
//!   - s9 = s8 + s9
//!   - has_explicit_format check (default 분기: s12 = allocation.x, s14 = -origin_x×scale + alloc.y, s11 = total_height, s13 unchanged = engine.unit)
//!   - **mode = 5**
//!
//! ## Stage E: 출력 write (raw 0x2f48a4-0x2f48e4)
//!
//! 1. `*out_position = (s10, s9)`
//! 2. `out_rect.write_raw(flag=w9, panose_lo, panose_hi, s12, s14, s11, s13)` — 20B
//! 3. `out_transformation = sp+0x20..0x3c 28B 복사` — flag0/flag1/panose/4 f32/degree
//! 4. `*out_string_format.impl_ptr + 0x4 = 0` (raw 의 temp alloc/free 는 0 만 전달)
//! 5. `*out_mode = mode` (0/5/7)

use crate::char_item_view::CharItemView;
use crate::shape_engine;
use crate::transformation::{RectF20, Transformation};

use crate::blip_glyph::Allocation;
use crate::surface::PointImpl;

/// `Hnc::Shape::Render::StringFormat` (byte-eq 8B). caller stack-frame
/// `0x2f3998..0x2f39c8` 에서 arg7 = sp+0x68..0x70 = 8B 확정.
#[repr(C)]
#[derive(Debug)]
pub struct StringFormat8 {
    /// +0..+8: `*mut StringFormatImpl`.
    pub impl_ptr: *mut StringFormatImpl,
}

/// CalcDrawVariables 가 write 하는 StringFormatImpl 의 minimum byte layout.
/// raw `0x2f48e0  str w9, [x11, #0x4]` 로 +0x4 byte 가 u32 write 됨.
#[repr(C)]
#[derive(Debug, Default)]
pub struct StringFormatImpl {
    pub _field_0: u32,
    /// +0x4: CalcDrawVariables 가 항상 0 으로 set.
    pub field_4: u32,
}

/// f32 IEEE 754 raw u32 representation of 72.0 (`0x42900000`).
const F32_72: f32 = 72.0;
/// `0x3c2aaaab` ≈ 1/96 (raw 의 `mov w8, #0xaaab; movk w8, #0x3c2a, lsl #16`).
const F32_RECIP_96: f32 = f32::from_bits(0x3c2aaaab);
/// `0x42b40000` = 90.0 (raw 의 `Degree::C1(90.0)` 인자).
const F32_90_BITS: u32 = 0x42b40000;

/// CalcDrawVariables byte-eq port.
#[allow(clippy::too_many_arguments)]
pub unsafe fn calc_draw_variables(
    ci: &CharItemView,
    _b1: bool, // raw 가 w1 절대 read 안 함
    b2: bool,
    allocation: &Allocation,
    out_position: &mut PointImpl<f32>,
    out_rect: &mut RectF20,
    out_transformation: &mut Transformation,
    out_string_format: &mut StringFormat8,
    out_mode: &mut i32,
) {
    // ─── Stage A1: BodyProperty.Vert (PropertyKey 0x89e, u32) → w28 ───
    let w28: u32 = read_property_u32(
        ci.body_property as *const u8,
        /*bag_offset*/ 0,
        /*key*/ 0x89e,
        /*default*/ 1,
    );

    // ─── Stage A2: ParaProperty.Wrap (PropertyKey 0x8fd, u32, Contains-gated) → w20 ───
    let w20: u32 = read_property_u32_with_contains(
        ci.para_property as *const u8,
        /*bag_offset*/ 0x18,
        /*key*/ 0x8fd,
        /*default*/ 0,
    );

    // ─── Stage A3: RunProperty.shadow (PropertyKey 0x96c, f32) → s8 ───
    let s8: f32 = read_run_property_shadow(ci, 0x96c);

    // ─── Stage B: jump table on w20 (1..4) → s11 ───
    let s11_after_b: f32 = stage_b_jump_table(w20, w28, ci);

    // ─── Stage C: 공통 setup ───
    let s10_init: f32 = allocation.origin_x;
    let s9_init: f32 = allocation.origin_y;
    let engine_unit: f32 = read_engine_unit();
    let s12_init: f32 = engine_unit;
    let s13_init: f32 = engine_unit;
    let mut degree_raw: u32 = 0; // Degree(0.0) initial

    // ─── Stage D: b2 dispatch ───
    let (final_s10, final_s9, final_s11, final_s12, final_s13, final_s14, has_panose, mode_w8, rect_flag) =
        if !b2 {
            // raw 0x2f46dc-0x2f46f4: w25 == 0 fast path
            (s10_init, s9_init, 0.0_f32, 0.0_f32, 0.0_f32, 0.0_f32, false, 0_i32, 0_u8)
        } else {
            // b2 == 1 main path. w28 으로 sub-dispatch.
            let mut s10 = s10_init;
            let mut s9 = s9_init;
            let mut s11 = s11_after_b;
            let s12;
            let s13;
            let s14;
            let mode;

            if matches!(w28, 5 | 6) {
                // ──── Path B (Bottom-aware), raw 0x2f46f8-0x2f47b4 ────
                let alloc_x = allocation.origin_x;
                let alloc_y = allocation.origin_y;
                let field_3c = ci.field_3c;
                let total_height = ci.total_height;
                let descent = ci.descent;

                // raw 0x2f4718-0x2f4730:
                //   s0 = -0.5 × field_3c × unit / 72
                //   s0 += allocation.x
                //   s11 += s0
                let inner = (-0.5_f32 * field_3c) * engine_unit / F32_72;
                let offset = alloc_x + inner;
                s11 += offset;

                // raw 0x2f473c-0x2f4778: shadow adjust if (total_height_alt == 0 && ascent_ratio < 0)
                let total_alt = ci.total_height_alt;
                let asc_ratio = ci.ascent_ratio;
                if total_alt == 0.0 && asc_ratio.is_sign_negative() && asc_ratio != 0.0 {
                    // raw 의 b.pl 은 "positive or zero" (= 부호 비트 0). negative non-zero 일 때만 진입.
                    // (fcmp #0.0 → s0 < 0 일 때 NF=1, b.pl 은 NF==0 일 때 분기 → not taken)
                    s11 += (engine_unit * F32_RECIP_96) * (1.0 - asc_ratio);
                }

                // raw 0x2f477c-0x2f4788: s9 = (alloc_y + total_height) - (descent × unit / 72)
                s9 = (alloc_y + total_height) - (descent * engine_unit / F32_72);

                // raw 0x2f478c: s10 = s8 + s11
                s10 = s8 + s11;

                // raw 0x2f4790-0x2f4814: has_explicit_format dispatch
                if ci.has_explicit_format != 0 {
                    // explicit (0x2f4800-0x2f4814):
                    s12 = ci.format_scale_x;
                    s14 = ci.format_scale_y;
                    s11 = ci.format_rot_x;
                    s13 = ci.format_rot_y;
                } else {
                    // not explicit (0x2f4798-0x2f47b0):
                    let origin_x = ci.format_origin_x;
                    let origin_scale = ci.format_origin_scale;
                    let f1 = 1.0 - origin_scale;
                    s12 = -(origin_x * f1) + alloc_x; // fmsub
                    s11 = ci.format_origin_x; // s11 was overwritten by ldp at 0x2f479c
                    s14 = alloc_y;
                    s13 = total_height;
                }
                mode = 5;
            } else if matches!(w28, 0 | 2) {
                // ──── Path A (Top alignment), raw 0x2f4668-0x2f47b4 ────
                // raw 0x2f4668: s10 = s11 + s10
                s10 = s11 + s10;

                // raw 0x2f466c-0x2f46a8: shadow adjust
                let total_alt = ci.total_height_alt;
                let asc_ratio = ci.ascent_ratio;
                if total_alt == 0.0 && asc_ratio.is_sign_negative() && asc_ratio != 0.0 {
                    s10 += (engine_unit * F32_RECIP_96) * (1.0 - asc_ratio);
                }

                // raw 0x2f46ac-0x2f4848 + 0x2f4880: has_explicit_format dispatch + s10 += s8 + Degree(90)
                if ci.has_explicit_format != 0 {
                    // explicit
                    s12 = ci.format_scale_x;
                    s14 = ci.format_scale_y;
                    s11 = ci.format_rot_x;
                    s13 = ci.format_rot_y;
                } else {
                    // not explicit (0x2f4830-0x2f4844 then merge to 0x2f4848):
                    let alloc_x = allocation.origin_x;
                    let alloc_y = allocation.origin_y;
                    let origin_x = ci.format_origin_x;
                    let origin_scale = ci.format_origin_scale;
                    let f1 = 1.0 - origin_scale;
                    // x24 was set to allocation+0xc in Stage C, and NOT overwritten here → s14 = allocation.origin_y
                    s11 = origin_x; // s11 was overwritten by ldp at 0x2f4834
                    s12 = -(origin_x * f1) + alloc_x; // fmsub
                    s14 = alloc_y;
                    s13 = ci.total_height;
                }

                // raw 0x2f4870-0x2f488c: Degree(90.0) reconstruct → degree_raw = 90.0's u32
                degree_raw = F32_90_BITS;

                // raw 0x2f4880: s10 = s8 + s10
                s10 = s8 + s10;

                mode = 7;
            } else {
                // ──── Path C (Center / fallback for w28 ∈ {1, 3, 4, 7+}), raw 0x2f47b8-0x2f4818 ────
                // raw 0x2f47b8: s9 = s11 + s9
                s9 = s11 + s9;

                // raw 0x2f47bc-0x2f47f0: shadow adjust (NOTE: uses asc_ratio directly, not 1-ratio)
                let total_alt = ci.total_height_alt;
                let asc_ratio = ci.ascent_ratio;
                if total_alt == 0.0 && asc_ratio.is_sign_negative() && asc_ratio != 0.0 {
                    s9 += (engine_unit * F32_RECIP_96) * asc_ratio;
                }

                // raw 0x2f47f4: s9 = s8 + s9
                s9 = s8 + s9;

                // raw 0x2f47f8-0x2f4814: has_explicit_format dispatch
                if ci.has_explicit_format != 0 {
                    // explicit
                    s12 = ci.format_scale_x;
                    s14 = ci.format_scale_y;
                    s11 = ci.format_rot_x;
                    s13 = ci.format_rot_y;
                } else {
                    // not explicit (0x2f495c-0x2f4970):
                    let alloc_x = allocation.origin_x;
                    let alloc_y = allocation.origin_y;
                    let origin_x = ci.format_origin_x;
                    let origin_scale = ci.format_origin_scale;
                    // raw 0x2f4964: ldp s13, s1, [x23, #0x6c] → s13 = origin_x, s1 = scale
                    // raw 0x2f4968: fmsub s14 = -(s13 * s1) + s0 (s0=alloc_y)
                    s12 = alloc_x;
                    s14 = -(origin_x * origin_scale) + alloc_y; // fmsub (note: scale not (1-scale))
                    s11 = ci.total_height;
                    s13 = s13_init; // unchanged from Stage C (= engine.unit)
                }
                mode = 5;
            }

            (s10, s9, s11, s12, s13, s14, ci.has_explicit_format != 0, mode, 1_u8)
        };

    // ─── Stage E: panose 처리 ───
    let (panose_lo, panose_hi, panose_bytes): (u16, u8, [u8; 3]) = if has_panose {
        let lo = u16::from_le_bytes([ci.format_panose[0], ci.format_panose[1]]);
        let hi = ci.format_panose[2];
        (lo, hi, ci.format_panose)
    } else {
        // raw 의 not-explicit 분기에서 sp+0x3c..0x3e 는 uninit (b2=true) 또는
        // 명시적으로 zero-init 안 함 (b2=false). byte-eq 어차피 caller 가 의미를 안 둠.
        // 본 port 는 deterministic 0 으로 set.
        (0, 0, [0; 3])
    };

    // ─── Stage F: output writes ───

    // raw 0x2f48a4: stp s10, s9, [x11]
    out_position.x = final_s10;
    out_position.y = final_s9;

    // raw 0x2f48a8-0x2f48c0: RectF20 (20B)
    // strb w9, [x21] — w9 = 1 (b2=true) 또는 0 (b2=false fast path)
    out_rect.write_raw(
        rect_flag,
        panose_lo,
        panose_hi,
        final_s12,
        final_s14,
        final_s11,
        final_s13,
    );

    // raw 0x2f48c4-0x2f48d4: Transformation (28B)
    // flag0 = 0 (initial strb wzr), flag1 = 1 (always; raw 0x2f4854: mov w9, #1; strb w9, [sp+0x24])
    out_transformation.write_raw(
        /*flag0*/ 0,
        /*flag1*/ 1, // raw 0x2f4618 setup 이 b2 분기 전에 mov w8,#1; strb w8 [sp+0x24]
        panose_bytes,
        final_s12,
        final_s14, // raw 0x2f4868: stp s12, s14, [sp+0x28]
        final_s11, // raw 0x2f486c: stp s11, s13, [sp+0x30]
        final_s13,
        degree_raw,
    );

    // raw 0x2f48d8-0x2f48e0: StringFormat impl_ptr+0x4 = 0
    if !out_string_format.impl_ptr.is_null() {
        (*out_string_format.impl_ptr).field_4 = 0;
    }

    // raw 0x2f48e4: str w8, [x10]
    *out_mode = mode_w8;
}

// ────────────────────────────────────────────────────────────────────────
// Helper: Stage B jump table — vertical alignment 계산
// ────────────────────────────────────────────────────────────────────────

fn stage_b_jump_table(w20: u32, w28: u32, ci: &CharItemView) -> f32 {
    if !matches!(w20, 1..=4) {
        return 0.0;
    }
    let asc = ci.ascent;
    let desc = ci.descent;
    let asc_plus_desc = asc + desc;
    let total_height = ci.total_height;
    let engine_unit = read_engine_unit();

    // 각 case 가 (s9_intermediate, divisor) 를 set 하고, 최종 s11 = (s9*unit)/divisor
    // 단 (w20=1, w28∈{5,6}) 는 별도 short-circuit (s11 = -0.5 × total_height).
    let (s9_intermediate, divisor) = match w20 {
        1 => {
            if matches!(w28, 5 | 6) {
                // raw 0x2f4820-0x2f482c: short-circuit return
                return -0.5 * total_height;
            } else if matches!(w28, 0 | 2) {
                // raw 0x2f4534-0x2f4554: s11 = (unit × 0.5×(asc+desc)) / -72
                (asc_plus_desc * 0.5, -F32_72)
            } else {
                // raw 0x2f4928-0x2f4938: s11 = (ascent × unit) / 72
                (asc, F32_72)
            }
        }
        2 => {
            // raw 0x2f4558-0x2f4568: bit-mask skip-set check
            //   mov w8, #0x65; lsr w8, w8, w28; tbnz w8, #0
            // 0x65 = 0110_0101 → bits {0, 2, 5, 6} set → skip when w28 ∈ {0, 2, 5, 6}
            // (단 w28 >= 7 → cmp/b.hs 가 skip-check 우회 → fall-through)
            let skip = w28 < 7 && ((0x65u32 >> w28) & 1) != 0;
            if skip {
                return 0.0;
            }
            // raw 0x2f456c-0x2f45c4 then 0x2f45c8-0x2f45dc:
            //   s9 = -0.5×(asc+desc) + asc = 0.5×asc - 0.5×desc
            //   s11 = (unit × s9) / 72
            let s9 = (-0.5_f32 * asc_plus_desc) + asc; // fmadd
            (s9, F32_72)
        }
        3 => {
            // raw 0x2f457c-0x2f4584: continue only if w28 ∈ {0, 2} (= `(w28 | 2) == 2`)
            if (w28 | 2) != 2 {
                return 0.0;
            }
            // raw 0x2f4588-0x2f4598: s9 = -(asc+desc)×0.5 + desc = -0.5×asc + 0.5×desc
            let s9 = -(asc_plus_desc * 0.5) + desc; // fnmsub
            (s9, F32_72)
        }
        4 => {
            if matches!(w28, 5 | 6) {
                // raw 0x2f459c-0x2f45a4: short-circuit skip
                return 0.0;
            }
            if matches!(w28, 0 | 2) {
                // raw 0x2f45b4-0x2f45c4: s0 = -(asc+desc)×0.5 + desc; s9 = desc×0.5 + s0
                //                       = -0.5×asc + desc
                let s0 = -(asc_plus_desc * 0.5) + desc;
                let s9 = desc * 0.5 + s0;
                (s9, F32_72)
            } else {
                // raw 0x2f493c-0x2f4958: s9 = descent × 0.5; s11 = (s9 × unit) / -72
                (desc * 0.5, -F32_72)
            }
        }
        _ => unreachable!(),
    };

    (engine_unit * s9_intermediate) / divisor
}

// ────────────────────────────────────────────────────────────────────────
// Helper: ShapeEngine.unit 안전 read
// ────────────────────────────────────────────────────────────────────────

fn read_engine_unit() -> f32 {
    shape_engine::read_instance().unit
}

// ────────────────────────────────────────────────────────────────────────
// Helper: property reads
// ────────────────────────────────────────────────────────────────────────

unsafe fn read_property_u32(
    sptr_storage: *const u8,
    bag_offset: usize,
    key_id: u32,
    default: u32,
) -> u32 {
    let ctrl = sptr_storage as *mut crate::share_ptr::ControlBlock<()>;
    if ctrl.is_null() {
        return default;
    }
    let obj_ptr = (*ctrl).obj as *mut u8;
    if obj_ptr.is_null() {
        return default;
    }
    let bag_ctrl_ptr = obj_ptr.add(bag_offset)
        as *mut *mut crate::share_ptr::ControlBlock<crate::property_bag::PropertyBagImpl>;
    let bag_ctrl = *bag_ctrl_ptr;
    let bag_impl_ptr: *const crate::property_bag::PropertyBagImpl = if bag_ctrl.is_null() {
        std::ptr::null()
    } else {
        (*bag_ctrl).obj
    };
    let key = crate::property_key::PropertyKey::from_int(key_id);
    let addr = crate::property_bag::PropertyBagImpl::get_value_addr(bag_impl_ptr, &key);
    *(addr as *const u32)
}

unsafe fn read_property_u32_with_contains(
    sptr_storage: *const u8,
    bag_offset: usize,
    key_id: u32,
    default: u32,
) -> u32 {
    let ctrl = sptr_storage as *mut crate::share_ptr::ControlBlock<()>;
    if ctrl.is_null() {
        return default;
    }
    let obj_ptr = (*ctrl).obj as *mut u8;
    if obj_ptr.is_null() {
        return default;
    }
    let bag_ptr = obj_ptr.add(bag_offset) as *const crate::property_bag::PropertyBag;
    let key = crate::property_key::PropertyKey::from_int(key_id);
    if !(*bag_ptr).contains(&key) {
        return default;
    }
    let bag_ctrl =
        *(obj_ptr.add(bag_offset)
            as *mut *mut crate::share_ptr::ControlBlock<crate::property_bag::PropertyBagImpl>);
    let bag_impl_ptr: *const crate::property_bag::PropertyBagImpl = if bag_ctrl.is_null() {
        std::ptr::null()
    } else {
        (*bag_ctrl).obj
    };
    let addr = crate::property_bag::PropertyBagImpl::get_value_addr(bag_impl_ptr, &key);
    *(addr as *const u32)
}

unsafe fn read_run_property_shadow(ci: &CharItemView, key_id: u32) -> f32 {
    let ctrl = ci.run_property;
    if ctrl.is_null() {
        return 0.0;
    }
    let rp = (*ctrl).obj;
    if rp.is_null() {
        return 0.0;
    }
    let bag_ctrl = (*rp).property_bag;
    let bag_impl_ptr: *const crate::property_bag::PropertyBagImpl = if bag_ctrl.is_null() {
        std::ptr::null()
    } else {
        (*bag_ctrl).obj
    };
    let key = crate::property_key::PropertyKey::from_int(key_id);
    let addr = crate::property_bag::PropertyBagImpl::get_value_addr(bag_impl_ptr, &key);
    let s9: f32 = *(addr as *const f32);
    if s9 == 0.0 {
        return 0.0;
    }
    // raw 0x2f44bc-0x2f44c8: s8 = s9 × -(ci.format_origin_x × ci.shadow_scale)
    let s0 = ci.format_origin_x;
    let s1 = ci.shadow_scale;
    let neg_prod = -(s0 * s1);
    s9 * neg_prod
}

// ────────────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::char_item_view::CharItemView;

    fn make_alloc(x: f32, y: f32) -> Allocation {
        Allocation {
            origin_x: x,
            _pad1: 0.0,
            _pad2: 0.0,
            origin_y: y,
        }
    }

    fn make_outputs() -> (PointImpl<f32>, RectF20, Transformation, StringFormatImpl, i32) {
        (
            PointImpl { x: 0.0, y: 0.0 },
            RectF20::ZERO,
            Transformation::ZERO,
            StringFormatImpl::default(),
            -1_i32,
        )
    }

    // ─── Stage A / b2=false fast path ───

    #[test]
    fn b2_false_fast_path_writes_position_from_allocation() {
        let ci = CharItemView::new_empty();
        let alloc = make_alloc(12.5, 34.75);
        let (mut pos, mut rect, mut trans, mut sf_impl, mut mode) = make_outputs();
        let mut sf = StringFormat8 { impl_ptr: &mut sf_impl as *mut _ };
        unsafe {
            calc_draw_variables(&ci, false, false, &alloc, &mut pos, &mut rect, &mut trans, &mut sf, &mut mode);
        }
        assert_eq!(pos.x, 12.5);
        assert_eq!(pos.y, 34.75);
        assert_eq!(mode, 0);
        assert_eq!(rect.flag(), 0); // b2=false → header byte = 0
        assert_eq!(trans.flag1(), 1);
        assert_eq!(trans.degree_raw, 0);
        assert_eq!(sf_impl.field_4, 0);
    }

    #[test]
    fn b1_param_is_ignored() {
        let ci = CharItemView::new_empty();
        let alloc = make_alloc(5.0, 10.0);
        let (mut p1, mut r1, mut t1, mut sf1, mut m1) = make_outputs();
        let mut sf1w = StringFormat8 { impl_ptr: &mut sf1 as *mut _ };
        let (mut p2, mut r2, mut t2, mut sf2, mut m2) = make_outputs();
        let mut sf2w = StringFormat8 { impl_ptr: &mut sf2 as *mut _ };
        unsafe {
            calc_draw_variables(&ci, true, false, &alloc, &mut p1, &mut r1, &mut t1, &mut sf1w, &mut m1);
            calc_draw_variables(&ci, false, false, &alloc, &mut p2, &mut r2, &mut t2, &mut sf2w, &mut m2);
        }
        assert_eq!(p1.x, p2.x);
        assert_eq!(p1.y, p2.y);
        assert_eq!(m1, m2);
    }

    #[test]
    fn null_string_format_impl_no_panic() {
        let ci = CharItemView::new_empty();
        let alloc = make_alloc(0.0, 0.0);
        let (mut pos, mut rect, mut trans, _, mut mode) = make_outputs();
        let mut sf = StringFormat8 { impl_ptr: std::ptr::null_mut() };
        unsafe {
            calc_draw_variables(&ci, false, false, &alloc, &mut pos, &mut rect, &mut trans, &mut sf, &mut mode);
        }
        assert_eq!(mode, 0);
    }

    // ─── Stage B jump table tests (16 sub-cases) ───
    // 모든 case 가 w20=0 default → s11=0 으로 시작. 의도된 path 만 호출되도록 verify.
    // helper: stage_b 만 격리해서 test.

    fn stage_b_for(w20: u32, w28: u32, ascent: f32, descent: f32, total_height: f32) -> f32 {
        let mut ci = CharItemView::new_empty();
        ci.ascent = ascent;
        ci.descent = descent;
        ci.total_height = total_height;
        stage_b_jump_table(w20, w28, &ci)
    }

    #[test]
    fn stage_b_w20_zero_returns_zero() {
        assert_eq!(stage_b_for(0, 0, 10.0, 4.0, 14.0), 0.0);
        assert_eq!(stage_b_for(0, 1, 10.0, 4.0, 14.0), 0.0);
    }

    #[test]
    fn stage_b_w20_out_of_range_returns_zero() {
        assert_eq!(stage_b_for(5, 0, 10.0, 4.0, 14.0), 0.0);
        assert_eq!(stage_b_for(99, 0, 10.0, 4.0, 14.0), 0.0);
    }

    #[test]
    fn stage_b_w20_1_w28_5or6_short_circuit() {
        // s11 = -0.5 × total_height (unit/72 도 안 거침)
        assert_eq!(stage_b_for(1, 5, 10.0, 4.0, 20.0), -10.0);
        assert_eq!(stage_b_for(1, 6, 10.0, 4.0, 20.0), -10.0);
    }

    #[test]
    fn stage_b_w20_1_w28_0_top_neg() {
        // s11 = (unit × 0.5×(asc+desc)) / -72
        let unit = read_engine_unit();
        let expected = (unit * 0.5 * (10.0 + 4.0)) / -72.0;
        assert_eq!(stage_b_for(1, 0, 10.0, 4.0, 20.0), expected);
    }

    #[test]
    fn stage_b_w20_1_w28_other_ascent_path() {
        // s11 = (ascent × unit) / 72
        let unit = read_engine_unit();
        let expected = (10.0 * unit) / 72.0;
        assert_eq!(stage_b_for(1, 1, 10.0, 4.0, 20.0), expected);
        assert_eq!(stage_b_for(1, 3, 10.0, 4.0, 20.0), expected);
        assert_eq!(stage_b_for(1, 7, 10.0, 4.0, 20.0), expected);
    }

    #[test]
    fn stage_b_w20_2_skip_set() {
        // w28 ∈ {0, 2, 5, 6} 은 skip → 0
        assert_eq!(stage_b_for(2, 0, 10.0, 4.0, 20.0), 0.0);
        assert_eq!(stage_b_for(2, 2, 10.0, 4.0, 20.0), 0.0);
        assert_eq!(stage_b_for(2, 5, 10.0, 4.0, 20.0), 0.0);
        assert_eq!(stage_b_for(2, 6, 10.0, 4.0, 20.0), 0.0);
    }

    #[test]
    fn stage_b_w20_2_other_center() {
        // s11 = (unit × (0.5×asc - 0.5×desc)) / 72
        let unit = read_engine_unit();
        let expected = (unit * (0.5 * 10.0 - 0.5 * 4.0)) / 72.0;
        assert_eq!(stage_b_for(2, 1, 10.0, 4.0, 20.0), expected);
        assert_eq!(stage_b_for(2, 3, 10.0, 4.0, 20.0), expected);
        // w28 >= 7 도 같은 path
        assert_eq!(stage_b_for(2, 7, 10.0, 4.0, 20.0), expected);
        assert_eq!(stage_b_for(2, 100, 10.0, 4.0, 20.0), expected);
    }

    #[test]
    fn stage_b_w20_3_0or2_only() {
        let unit = read_engine_unit();
        let expected = (unit * (-0.5 * 10.0 + 0.5 * 4.0)) / 72.0;
        assert_eq!(stage_b_for(3, 0, 10.0, 4.0, 20.0), expected);
        assert_eq!(stage_b_for(3, 2, 10.0, 4.0, 20.0), expected);
        // 그 외 모두 skip
        assert_eq!(stage_b_for(3, 1, 10.0, 4.0, 20.0), 0.0);
        assert_eq!(stage_b_for(3, 5, 10.0, 4.0, 20.0), 0.0);
        assert_eq!(stage_b_for(3, 7, 10.0, 4.0, 20.0), 0.0);
    }

    #[test]
    fn stage_b_w20_4_5or6_skip() {
        assert_eq!(stage_b_for(4, 5, 10.0, 4.0, 20.0), 0.0);
        assert_eq!(stage_b_for(4, 6, 10.0, 4.0, 20.0), 0.0);
    }

    #[test]
    fn stage_b_w20_4_0or2_descent_path() {
        let unit = read_engine_unit();
        let expected = (unit * (-0.5 * 10.0 + 4.0)) / 72.0;
        assert_eq!(stage_b_for(4, 0, 10.0, 4.0, 20.0), expected);
        assert_eq!(stage_b_for(4, 2, 10.0, 4.0, 20.0), expected);
    }

    #[test]
    fn stage_b_w20_4_other_neg72_path() {
        let unit = read_engine_unit();
        let expected = (0.5 * 4.0 * unit) / -72.0;
        assert_eq!(stage_b_for(4, 1, 10.0, 4.0, 20.0), expected);
        assert_eq!(stage_b_for(4, 7, 10.0, 4.0, 20.0), expected);
    }

    // ─── Stage D main path (b2=true) tests ───

    #[test]
    fn b2_true_w28_2_top_alignment_no_explicit_format() {
        // Default w28=1 (from null body_property), so set body... actually since ci.body_property
        // is null, w28=1, which is the "Path C / center" route. To test "Path A / top" (w28=2),
        // we need to mock body_property. For now, test the "Path C" (default w28=1) path.
        let mut ci = CharItemView::new_empty();
        ci.ascent = 10.0;
        ci.descent = 4.0;
        ci.total_height = 20.0;
        ci.format_origin_x = 0.0;
        ci.format_origin_scale = 0.0;
        // w28 = 1 (null body), w20 = 0 (null para) → s11_after_b = 0
        let alloc = make_alloc(5.0, 8.0);
        let (mut pos, mut rect, mut trans, mut sf, mut mode) = make_outputs();
        let mut sfw = StringFormat8 { impl_ptr: &mut sf as *mut _ };
        unsafe {
            calc_draw_variables(&ci, false, true, &alloc, &mut pos, &mut rect, &mut trans, &mut sfw, &mut mode);
        }
        // Path C: s9 = 0 + 8 = 8; s9 = 0 + 8 = 8 (s8=0). s10 unchanged = 5
        assert_eq!(pos.x, 5.0);
        assert_eq!(pos.y, 8.0);
        assert_eq!(mode, 5);
        assert_eq!(trans.degree_raw, 0); // Path C 는 Degree 재구성 없음
        assert_eq!(rect.flag(), 1); // b2=true
    }

    #[test]
    fn b2_true_default_w28_1_path_c_center_no_shadow() {
        // ascent_ratio = 0 (default) → no shadow adjust
        let mut ci = CharItemView::new_empty();
        ci.ascent = 10.0;
        ci.descent = 4.0;
        ci.total_height = 16.0;
        ci.ascent_ratio = 0.0; // no shadow trigger
        let alloc = make_alloc(2.0, 5.0);
        let (mut pos, mut rect, _, mut sf, mut mode) = make_outputs();
        let mut trans = Transformation::ZERO;
        let mut sfw = StringFormat8 { impl_ptr: &mut sf as *mut _ };
        unsafe {
            calc_draw_variables(&ci, false, true, &alloc, &mut pos, &mut rect, &mut trans, &mut sfw, &mut mode);
        }
        assert_eq!(pos.x, 2.0);
        assert_eq!(pos.y, 5.0);
        assert_eq!(mode, 5);
    }

    /// Helper: BodyProperty + ControlBlock 합성 (test 전용, leak 허용).
    unsafe fn make_body_property_ctrl(
        key_id: u32,
        value: u32,
    ) -> *mut crate::share_ptr::ControlBlock<crate::body_property::BodyProperty> {
        use crate::body_property::BodyProperty;
        use crate::property::{state, PEnum};
        use crate::property_bag::PropertyBag;
        use crate::property_key::PropertyKey;
        use crate::share_ptr::ControlBlock;

        let mut bag = PropertyBag::new(false);
        let key = PropertyKey::from_int(key_id);
        let ctrl = PEnum::create_attach_ctrl(state::ENABLED_DEFAULT, value);
        let _prev = bag.attach(&key, ctrl);

        let body = BodyProperty {
            bag,
            scene3d_ctrl: std::ptr::null_mut(),
            sp3d_ctrl: std::ptr::null_mut(),
            preset_warp: std::ptr::null_mut(),
        };
        let body_box = Box::new(body);
        let body_raw = Box::into_raw(body_box);
        let cb = Box::new(ControlBlock { obj: body_raw, refcount: 1 });
        Box::into_raw(cb)
    }

    #[test]
    fn b2_true_path_a_top_alignment_w28_2() {
        let mut ci = CharItemView::new_empty();
        ci.ascent = 10.0;
        ci.descent = 4.0;
        ci.total_height = 16.0;
        // Vert = 2 (Top) — Path A
        unsafe {
            ci.body_property = make_body_property_ctrl(0x89e, 2);
        }
        let alloc = make_alloc(0.0, 0.0);
        let (mut pos, mut rect, mut trans, mut sf, mut mode) = make_outputs();
        let mut sfw = StringFormat8 { impl_ptr: &mut sf as *mut _ };
        unsafe {
            calc_draw_variables(&ci, false, true, &alloc, &mut pos, &mut rect, &mut trans, &mut sfw, &mut mode);
        }
        // Path A: mode = 7, degree_raw = 90.0 bits
        assert_eq!(mode, 7);
        assert_eq!(trans.degree_raw, F32_90_BITS);
        assert_eq!(rect.flag(), 1);
        assert_eq!(trans.flag1(), 1);
        // s10 = s11_after_b + alloc.x + 0 (shadow) + 0 (s8) = 0 + 0 + 0 + 0 = 0
        assert_eq!(pos.x, 0.0);
        // s9 = alloc.y = 0 (Path A 는 s9 unchanged)
        assert_eq!(pos.y, 0.0);
    }

    #[test]
    fn b2_true_path_b_bottom_alignment_w28_5() {
        let mut ci = CharItemView::new_empty();
        ci.ascent = 10.0;
        ci.descent = 4.0;
        ci.total_height = 16.0;
        ci.field_3c = 0.0; // 단순화 — inner = 0
        unsafe {
            ci.body_property = make_body_property_ctrl(0x89e, 5);
        }
        let alloc = make_alloc(2.0, 5.0);
        let (mut pos, mut rect, mut trans, mut sf, mut mode) = make_outputs();
        let mut sfw = StringFormat8 { impl_ptr: &mut sf as *mut _ };
        unsafe {
            calc_draw_variables(&ci, false, true, &alloc, &mut pos, &mut rect, &mut trans, &mut sfw, &mut mode);
        }
        assert_eq!(mode, 5);
        assert_eq!(trans.degree_raw, 0);
        assert_eq!(rect.flag(), 1);
        // pos.y = (alloc.y + total_height) - (descent × unit / 72)
        let unit = read_engine_unit();
        let expected_y = (5.0 + 16.0) - (4.0 * unit / 72.0);
        assert_eq!(pos.y, expected_y);
    }

    #[test]
    fn b2_true_path_a_shadow_trigger_when_ratio_negative() {
        // shadow 발동 조건: total_height_alt == 0 && ascent_ratio < 0
        // Path A (w28=2): s10 += (unit/96) × (1 - ratio)
        let mut ci = CharItemView::new_empty();
        ci.total_height_alt = 0.0;
        ci.ascent_ratio = -0.5;
        unsafe {
            ci.body_property = make_body_property_ctrl(0x89e, 2);
        }
        let alloc = make_alloc(10.0, 0.0);
        let (mut pos, _, mut trans, mut sf, mut mode) = make_outputs();
        let mut rect = RectF20::ZERO;
        let mut sfw = StringFormat8 { impl_ptr: &mut sf as *mut _ };
        unsafe {
            calc_draw_variables(&ci, false, true, &alloc, &mut pos, &mut rect, &mut trans, &mut sfw, &mut mode);
        }
        assert_eq!(mode, 7);
        // s10 = 0 + 10 (alloc.x) + (unit × 1/96 × 1.5) + 0 (s8) = 10 + (1×0.0104166666... × 1.5)
        let unit = read_engine_unit();
        let expected_x = 10.0 + (unit * F32_RECIP_96) * 1.5 + 0.0;
        assert_eq!(pos.x, expected_x);
    }

    #[test]
    fn b2_true_path_c_shadow_uses_ratio_directly_not_one_minus() {
        // Path C (w28 ∈ {1, 3, ...}): s9 += (unit/96) × ratio  (NOT 1 - ratio!)
        let mut ci = CharItemView::new_empty();
        ci.total_height_alt = 0.0;
        ci.ascent_ratio = -0.25;
        // w28 = 1 (default — no body_property) → Path C
        let alloc = make_alloc(0.0, 0.0);
        let (mut pos, _, mut trans, mut sf, mut mode) = make_outputs();
        let mut rect = RectF20::ZERO;
        let mut sfw = StringFormat8 { impl_ptr: &mut sf as *mut _ };
        unsafe {
            calc_draw_variables(&ci, false, true, &alloc, &mut pos, &mut rect, &mut trans, &mut sfw, &mut mode);
        }
        assert_eq!(mode, 5);
        // s9 = 0 + 0 (s11) + (unit × 1/96 × -0.25) + 0 (s8) = unit × -0.0026...
        let unit = read_engine_unit();
        let expected_y = (unit * F32_RECIP_96) * -0.25;
        assert_eq!(pos.y, expected_y);
    }

    #[test]
    fn b2_true_path_c_with_explicit_format_copies_panose_and_scales() {
        let mut ci = CharItemView::new_empty();
        ci.has_explicit_format = 1;
        ci.format_panose = [0x12, 0x34, 0x56];
        ci.format_scale_x = 1.5;
        ci.format_scale_y = 2.5;
        ci.format_rot_x = 3.5;
        ci.format_rot_y = 4.5;
        let alloc = make_alloc(0.0, 0.0);
        let (mut pos, mut rect, mut trans, mut sf, mut mode) = make_outputs();
        let mut sfw = StringFormat8 { impl_ptr: &mut sf as *mut _ };
        unsafe {
            calc_draw_variables(&ci, false, true, &alloc, &mut pos, &mut rect, &mut trans, &mut sfw, &mut mode);
        }
        // Panose 가 explicit 경로로 RectF/Transformation 둘 다 들어가야 함
        assert_eq!(rect.panose_lo(), 0x3412);
        assert_eq!(rect.panose_hi(), 0x56);
        assert_eq!(rect.m0, 1.5); // format_scale_x → s12
        assert_eq!(rect.m1, 2.5); // format_scale_y → s14
        assert_eq!(rect.m2, 3.5); // format_rot_x → s11
        assert_eq!(rect.m3, 4.5); // format_rot_y → s13
        assert_eq!(trans.panose(), [0x12, 0x34, 0x56]);
        assert_eq!(mode, 5);
    }
}

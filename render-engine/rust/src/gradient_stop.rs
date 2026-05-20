//! `Hnc::Shape::GradientStop` + `GradientStops` (= std::vector<SharePtr<GradientStop>>)
//! — 1:1 byte-equivalent port.
//!
//! `FormatScheme::CreateDefault` (`0x16f628`) 의 **Block 3** 의 핵심 데이터 구조.
//! GradientBrush 의 stops (key 0x266) populating.
//!
//! # raw `GradientStop` 32B layout (확정 from `0x16f8ac-0x16f8d0` of CreateDefault)
//!
//! ```text
//! offset  field          type           의미
//! 0x00    value_first8   u8 [0..8]      Color value union 첫 8B
//! 0x08    value_last4    u8 [0..4]      Color value union 추가 4B
//! 0x0c    type_tag       u32            Color type_tag (0=Rgb, 2=Scheme, ...)
//! 0x10    color_effect   *mut ColorEffect (8B owned clone)
//! 0x18    position       f32            stop 의 위치 [0..1]
//! 0x1c    _pad           [u8; 4]        4B 정렬 패딩 (raw uninit)
//! ```
//!
//! 총 32B / 8B align (raw `mov w0, #0x20; bl __Znwm` @ `0x16f8ac`).
//!
//! **중요**: GradientStop 의 첫 16B (= +0x00..+0x10) 는 Color 의 첫 16B (value 12B +
//! type_tag 4B) 와 byte-identical. `ldur q0, [stack Color]; str q0, [stop]` 로 16B
//! memcpy. Color 의 `color_effect` (= +0x10..+0x18) 는 GradientStop 의 +0x10 에 별도
//! clone 으로 저장 (raw `bl 0x65411c` = `ColorEffect::clone_raw`).
//!
//! # raw `GradientStops` (= std::vector<SharePtr<GradientStop>>) 24B layout
//!
//! 확정 from `0x16f858-0x16f878` of CreateDefault.
//!
//! ```text
//! offset  field      type          의미
//! 0x00    begin      *mut Ctrl*    8B — buffer begin
//! 0x08    end        *mut Ctrl*    8B — buffer end (= begin + size_bytes)
//! 0x10    cap_end    *mut Ctrl*    8B — buffer cap_end (= begin + cap_bytes)
//! ```
//!
//! 총 24B (libc++ `std::vector` 표준 layout). 각 element = 8B `*mut ControlBlock<GradientStop>`.
//!
//! 초기 alloc: 160B = 20 element capacity (raw `mov w0, #0xa0; bl __Znwm` @ `0x16f86c`).
//!
//! # raw ControlBlock<GradientStop> 16B layout
//!
//! 확정 from `0x16f8dc-0x16f8ec`.
//!
//! ```text
//! offset  field      type                의미
//! 0x00    obj        *mut GradientStop   8B
//! 0x08    strong     u64                 8B refcount (ctor 에서 1, vector push 후 2)
//! ```
//!
//! 총 16B. **PropertyBag 의 ControlBlock<Property> (16B) 와 동일 layout** —
//! `Hnc::Memory::SharePtr<T>` 의 표준 instantiation. (Brush 의 24B
//! `Hnc::Memory::UniquePtr<Brush>` 와는 다름 — flag byte 없음.)

use crate::color::Color;
use crate::color_effect::ColorEffect;
use std::alloc::Layout;
use std::ptr;

/// raw 32B `Hnc::Shape::GradientStop`.
#[repr(C)]
pub struct GradientStop {
    /// raw +0x00..+0x0c: Color value union 12B.
    pub value: [u8; 12],
    /// raw +0x0c..+0x10: Color type_tag (u32).
    pub type_tag: u32,
    /// raw +0x10..+0x18: ColorEffect ptr (cloned from caller's effect).
    pub color_effect: *mut ColorEffect,
    /// raw +0x18..+0x1c: position [0..1].
    pub position: f32,
    /// raw +0x1c..+0x20: 4B alignment pad (uninit in raw).
    pub _pad: [u8; 4],
}

pub const GRADIENT_STOP_SIZE_BYTES: usize = 32;
pub const GRADIENT_STOP_ALIGN_BYTES: usize = 8;

const _: () = assert!(std::mem::size_of::<GradientStop>() == GRADIENT_STOP_SIZE_BYTES);
const _: () = assert!(std::mem::align_of::<GradientStop>() == GRADIENT_STOP_ALIGN_BYTES);

impl GradientStop {
    /// raw `0x16f8ac-0x16f8d0` 의 first-stop alloc + init sequence 1:1.
    ///
    /// 알고리즘:
    /// 1. alloc 32B
    /// 2. memcpy Color body 16B (= value + type_tag) from `color_in`
    /// 3. clone ColorEffect from `color_in.color_effect` (raw `bl 0x65411c`)
    /// 4. position = `position_in`
    /// 5. pad uninit (raw 의 4B uninit 와 일치)
    ///
    /// raw asm:
    /// ```asm
    /// 16f8ac: mov  w0, #0x20            ; alloc 32B
    /// 16f8b0: bl   __Znwm
    /// 16f8b4: mov  x25, x0
    /// 16f8b8: ldur q0, [x29, #-0xa0]    ; load 16B Color body
    /// 16f8bc: str  q0, [x0]              ; stop[0..16] = Color body
    /// 16f8c0: mov  x0, x24                ; effect1 ptr
    /// 16f8c4: bl   0x65411c               ; ColorEffect::clone_raw
    /// 16f8c8: mov  x26, x0
    /// 16f8cc: str  x0, [x25, #0x10]      ; stop[16..24] = cloned effect
    /// 16f8d0: str  wzr, [x25, #0x18]     ; stop[24..28] = position (0 for first)
    /// ```
    ///
    /// # Safety
    /// - `color_in` 은 valid `&Color` (24B layout).
    /// - 반환 ptr 은 caller 가 SharePtr ControlBlock 으로 wrap 또는 직접 dealloc.
    pub unsafe fn create_with_effect(color_in: &Color, position_in: f32) -> *mut GradientStop {
        let layout = Layout::new::<GradientStop>();
        let p = std::alloc::alloc(layout) as *mut GradientStop;
        if p.is_null() {
            std::alloc::handle_alloc_error(layout);
        }
        // raw `str q0, [x0]` — 16B memcpy of Color body (value 12B + type_tag 4B)
        let mut value = [0u8; 12];
        value.copy_from_slice(&color_in.value);
        let type_tag = color_in.type_tag;

        // raw `bl 0x65411c` — ColorEffect deep clone (null → null, else heap-copy)
        let cloned_effect = ColorEffect::clone_raw(color_in.color_effect);

        ptr::write(
            p,
            GradientStop {
                value,
                type_tag,
                color_effect: cloned_effect,
                position: position_in,
                _pad: [0u8; 4],
            },
        );
        p
    }

    /// `~GradientStop()` 등가 + heap dealloc.
    ///
    /// raw 의 stop dtor 는 ColorEffect 의 자체 release (raw `~Color()` 패턴
    /// 동일 — color_effect 의 raw_delete).
    ///
    /// # Safety
    /// `p` 는 `create_with_effect` 으로 얻은 ptr 또는 null.
    pub unsafe fn raw_delete(p: *mut GradientStop) {
        if p.is_null() {
            return;
        }
        // ColorEffect cleanup
        if !(*p).color_effect.is_null() {
            ColorEffect::raw_delete((*p).color_effect);
        }
        std::alloc::dealloc(p as *mut u8, Layout::new::<GradientStop>());
    }
}

/// raw `ControlBlock<GradientStop>` 16B (= `Hnc::Memory::SharePtr<GradientStop>::raw`).
///
/// 확정 from `0x16f8dc-0x16f8ec`:
/// ```asm
/// 16f8dc: mov  w0, #0x10
/// 16f8e0: bl   __Znwm                   ; alloc 16B
/// 16f8e4: mov  x8, x0
/// 16f8e8: mov  w9, #0x1
/// 16f8ec: stp  x25, x9, [x0]             ; obj = stop, strong = 1
/// ```
///
/// **NOTE**: 본 16B `ControlBlock<GradientStop>` 는 strong-only (weak / flag 없음) —
/// 기존 16B `ControlBlock<Property>` (PColor 의 SharePtr) 와 동일 layout. Brush 의
/// 24B `BrushControlBlock` (flag byte 있음) 과는 다른 instantiation.
#[repr(C)]
pub struct GradientStopCtrl {
    /// raw +0x00: `*mut GradientStop`.
    pub obj: *mut GradientStop,
    /// raw +0x08: strong refcount.
    pub strong: u64,
}

pub const GRADIENT_STOP_CTRL_SIZE_BYTES: usize = 16;
pub const GRADIENT_STOP_CTRL_ALIGN_BYTES: usize = 8;

const _: () = assert!(std::mem::size_of::<GradientStopCtrl>() == GRADIENT_STOP_CTRL_SIZE_BYTES);
const _: () = assert!(std::mem::align_of::<GradientStopCtrl>() == GRADIENT_STOP_CTRL_ALIGN_BYTES);

impl GradientStopCtrl {
    /// raw `0x16f8dc-0x16f8ec`: alloc 16B + obj + strong = 1.
    pub unsafe fn create_raw(obj: *mut GradientStop) -> *mut GradientStopCtrl {
        let layout = Layout::new::<GradientStopCtrl>();
        let p = std::alloc::alloc(layout) as *mut GradientStopCtrl;
        if p.is_null() {
            std::alloc::handle_alloc_error(layout);
        }
        ptr::write(p, GradientStopCtrl { obj, strong: 1 });
        p
    }

    /// SharePtr release path — strong-- (0 시 obj + ctrl dealloc).
    pub unsafe fn release(p: *mut GradientStopCtrl) {
        if p.is_null() {
            return;
        }
        if (*p).obj.is_null() {
            // bare ctrl, just dealloc
            std::alloc::dealloc(p as *mut u8, Layout::new::<GradientStopCtrl>());
            return;
        }
        let new_strong = (*p).strong.wrapping_sub(1);
        if new_strong == 0 {
            GradientStop::raw_delete((*p).obj);
            std::alloc::dealloc(p as *mut u8, Layout::new::<GradientStopCtrl>());
        } else {
            (*p).strong = new_strong;
        }
    }
}

/// raw 24B `GradientStops` (= libc++ `std::vector<SharePtr<GradientStop>>`).
///
/// 확정 from `0x16f858-0x16f878`:
/// ```asm
/// 16f858: stp  xzr, xzr, [x29, #-0xd8]   ; clear begin + end
/// 16f85c: stur xzr, [x29, #-0xc8]        ; clear cap_end
/// 16f86c: mov  w0, #0xa0                  ; alloc 160B
/// 16f870: bl   __Znwm
/// 16f874: add  x8, x0, #0xa0
/// 16f878: stp  x0, x0, [x29, #-0xd8]     ; begin = end = x0
/// 16f87c: stur x8, [x29, #-0xc8]         ; cap_end = x0 + 0xa0
/// ```
///
/// 초기 capacity: 160B / 8B/elem = **20 elements**. (각 elem = 8B `*mut Ctrl`).
///
/// **byte-eq layout**: ColorEffect 의 vector (`begin/end/cap_end`) 와 동일 패턴.
#[repr(C)]
pub struct GradientStopsVec {
    /// raw +0x00: buffer begin (= alloc result).
    pub begin: *mut *mut GradientStopCtrl,
    /// raw +0x08: buffer end (= begin + size_bytes).
    pub end: *mut *mut GradientStopCtrl,
    /// raw +0x10: buffer cap_end (= begin + cap_bytes).
    pub cap_end: *mut *mut GradientStopCtrl,
}

pub const GRADIENT_STOPS_VEC_SIZE_BYTES: usize = 24;
pub const GRADIENT_STOPS_VEC_ALIGN_BYTES: usize = 8;

const _: () = assert!(std::mem::size_of::<GradientStopsVec>() == GRADIENT_STOPS_VEC_SIZE_BYTES);
const _: () = assert!(std::mem::align_of::<GradientStopsVec>() == GRADIENT_STOPS_VEC_ALIGN_BYTES);

/// 초기 capacity (raw `mov w0, #0xa0`).
pub const GRADIENT_STOPS_INITIAL_CAPACITY_BYTES: usize = 0xa0;
pub const GRADIENT_STOPS_INITIAL_CAPACITY_ELEMS: usize = 0xa0 / 8;

impl GradientStopsVec {
    /// raw `0x16f858-0x16f87c` 1:1: empty vector + 160B initial alloc.
    pub unsafe fn new_with_initial_capacity() -> Self {
        let layout = Layout::from_size_align(GRADIENT_STOPS_INITIAL_CAPACITY_BYTES, 8)
            .expect("GradientStopsVec initial layout");
        let buf = std::alloc::alloc(layout) as *mut *mut GradientStopCtrl;
        if buf.is_null() {
            std::alloc::handle_alloc_error(layout);
        }
        GradientStopsVec {
            begin: buf,
            end: buf,
            cap_end: (buf as *mut u8).add(GRADIENT_STOPS_INITIAL_CAPACITY_BYTES)
                as *mut *mut GradientStopCtrl,
        }
    }

    /// Empty vector (no alloc) — for tests / null state.
    pub fn empty() -> Self {
        GradientStopsVec {
            begin: ptr::null_mut(),
            end: ptr::null_mut(),
            cap_end: ptr::null_mut(),
        }
    }

    /// element 수 (raw `size = (end - begin) / 8`).
    #[inline]
    pub fn len(&self) -> usize {
        if self.begin.is_null() {
            return 0;
        }
        unsafe { self.end.offset_from(self.begin) as usize }
    }

    /// capacity (raw `cap = (cap_end - begin) / 8`).
    #[inline]
    pub fn capacity(&self) -> usize {
        if self.begin.is_null() {
            return 0;
        }
        unsafe { self.cap_end.offset_from(self.begin) as usize }
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// raw `0x16f900-0x16f908` (fast path) + `0x16f918: bl 0x63010c` (slow realloc) 1:1.
    ///
    /// fast path: `if end < cap_end: *end++ = ctrl; ctrl.strong = 2`.
    ///
    /// slow path (raw `0x63010c`): new_cap = max(old_cap × 2, old_size+1), alloc
    /// new buffer, memcpy old → new (ctrl ptrs only — no refcount change), dealloc
    /// old, then push.
    ///
    /// **NOTE**: raw 의 push 후 strong = 2 인 점이 특이 — caller 가 만든 strong=1 의
    /// ctrl 을 vector 가 보유하면 +1 → 2. 이후 caller 가 local share_ptr 을 release
    /// 하면 strong → 1.
    ///
    /// # Safety
    /// `ctrl` 은 valid `*mut GradientStopCtrl` (caller 가 만든 strong=1). 호출 후
    /// strong 은 2 (vector + caller share). vector len += 1.
    pub unsafe fn push_back(&mut self, ctrl: *mut GradientStopCtrl) {
        if self.end < self.cap_end {
            // raw fast path
            ptr::write(self.end, ctrl);
            self.end = self.end.add(1);
            // raw 의 strong = 2 (refcount++ after push)
            if !ctrl.is_null() {
                (*ctrl).strong = (*ctrl).strong.wrapping_add(1);
            }
            return;
        }
        // raw slow path (bl 0x63010c)
        self.grow_and_push(ctrl);
    }

    /// raw `0x63010c` (slow realloc + push) 1:1.
    ///
    /// 알고리즘 (libc++ `std::vector::__push_back_slow_path`):
    /// 1. new_cap = max(old_cap × 2, old_size + 1)
    /// 2. alloc new buffer of new_cap × 8 bytes
    /// 3. memcpy old buffer → new (ctrl ptrs)
    /// 4. dealloc old buffer
    /// 5. push new ctrl at new_end
    /// 6. ctrl.strong++
    unsafe fn grow_and_push(&mut self, ctrl: *mut GradientStopCtrl) {
        let old_size_elems = self.len();
        let old_cap_elems = self.capacity();
        let req_elems = old_size_elems.checked_add(1).expect("GradientStopsVec overflow");
        let new_cap_elems = std::cmp::max(old_cap_elems.saturating_mul(2), req_elems);
        let new_cap_bytes = new_cap_elems
            .checked_mul(8)
            .expect("GradientStopsVec cap_bytes overflow");

        let new_layout = Layout::from_size_align(new_cap_bytes, 8)
            .expect("GradientStopsVec grow layout");
        let new_buf = std::alloc::alloc(new_layout) as *mut *mut GradientStopCtrl;
        if new_buf.is_null() {
            std::alloc::handle_alloc_error(new_layout);
        }

        // memcpy old ctrl ptrs → new (refcounts unchanged — just moving ownership of slot)
        if old_size_elems > 0 {
            ptr::copy_nonoverlapping(self.begin, new_buf, old_size_elems);
        }

        // dealloc old buffer
        let old_cap_bytes = old_cap_elems * 8;
        if old_cap_bytes > 0 {
            let old_layout = Layout::from_size_align(old_cap_bytes, 8)
                .expect("GradientStopsVec old layout");
            std::alloc::dealloc(self.begin as *mut u8, old_layout);
        }

        // push new ctrl
        let new_buf_u8 = new_buf as *mut u8;
        ptr::write(new_buf.add(old_size_elems), ctrl);
        if !ctrl.is_null() {
            (*ctrl).strong = (*ctrl).strong.wrapping_add(1);
        }

        self.begin = new_buf;
        self.end = new_buf_u8.add((old_size_elems + 1) * 8) as *mut *mut GradientStopCtrl;
        self.cap_end = new_buf_u8.add(new_cap_bytes) as *mut *mut GradientStopCtrl;
    }

    /// raw `0x62fd78` (`GradientStops::CopyFrom`) 1:1 — alloc new buffer of
    /// **same size_bytes as src** (tightly fit, NOT initial 160B cap) + element-by-
    /// element clone (each SharePtr ctrl refcount++).
    ///
    /// raw asm:
    /// ```asm
    /// 0x62fd94-0x62fd9c: dst.begin = dst.end = dst.cap_end = 0
    /// 0x62fda0: ldr x1, [src+8]                ; src.end
    /// 0x62fda4: ldr x0, [src]                  ; src.begin
    /// 0x62fda8: subs x21, x1, x0               ; size_bytes
    /// 0x62fdac: b.eq exit                      ; src empty → return
    /// 0x62fdbc: bl __Znwm(size_bytes)          ; alloc tight buffer
    /// 0x62fdc4: cap_end = new_buf + size_bytes (tight, no extra capacity)
    /// 0x62fdc8-0x62fdcc: dst.begin = dst.end = new_buf; dst.cap_end = end
    /// 0x62fde4-0x62fdf4: bl 0x62fe34 (element-by-element copy)
    /// ```
    ///
    /// 각 element 의 element-copy 는 SharePtr 의 copy_ctor (raw `0x62fe34`
    /// 의 functor) — `*dst = *src; if (*src) refcount++`.
    ///
    /// # Safety
    /// `self` 는 valid (begin/end/cap_end 정합). 반환 instance 는 caller 가
    /// drop / drop_in_place 로 해제 책임.
    pub unsafe fn clone_deep(&self) -> Self {
        // raw 0x62fda0-0x62fda8: size_bytes
        let size_bytes = (self.end as usize).wrapping_sub(self.begin as usize);

        // raw 0x62fdac: b.eq exit_empty
        if size_bytes == 0 {
            return GradientStopsVec::empty();
        }

        // raw 0x62fdbc: alloc tight buffer
        let layout = Layout::from_size_align(size_bytes, 8)
            .expect("GradientStopsVec::clone_deep layout");
        let new_buf = std::alloc::alloc(layout) as *mut *mut GradientStopCtrl;
        if new_buf.is_null() {
            std::alloc::handle_alloc_error(layout);
        }

        // raw 0x62fe34..: element-by-element copy + refcount++
        let count = size_bytes / std::mem::size_of::<*mut GradientStopCtrl>();
        for i in 0..count {
            let src_ctrl = *self.begin.add(i);
            // raw SharePtr copy_ctor: copy ptr + refcount++ if non-null
            if !src_ctrl.is_null() {
                (*src_ctrl).strong = (*src_ctrl).strong.wrapping_add(1);
            }
            ptr::write(new_buf.add(i), src_ctrl);
        }

        let new_buf_u8 = new_buf as *mut u8;
        GradientStopsVec {
            begin: new_buf,
            end: new_buf_u8.add(size_bytes) as *mut *mut GradientStopCtrl,
            cap_end: new_buf_u8.add(size_bytes) as *mut *mut GradientStopCtrl,
        }
    }

    /// Drop helper — 모든 stops 의 ctrl release + buffer dealloc.
    ///
    /// # Safety
    /// `self.begin..self.end` 는 valid stops ctrl ptrs.
    pub unsafe fn drop_in_place(&mut self) {
        if self.begin.is_null() {
            return;
        }
        let mut cur = self.begin;
        while cur < self.end {
            let ctrl = *cur;
            if !ctrl.is_null() {
                GradientStopCtrl::release(ctrl);
            }
            cur = cur.add(1);
        }
        let cap_bytes = (self.cap_end as usize).wrapping_sub(self.begin as usize);
        if cap_bytes > 0 {
            let layout = Layout::from_size_align(cap_bytes, 8)
                .expect("GradientStopsVec buf layout");
            std::alloc::dealloc(self.begin as *mut u8, layout);
        }
        self.begin = ptr::null_mut();
        self.end = ptr::null_mut();
        self.cap_end = ptr::null_mut();
    }
}

impl Drop for GradientStopsVec {
    fn drop(&mut self) {
        unsafe { self.drop_in_place() };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // GradientStop tests
    // ========================================================================

    #[test]
    fn gradient_stop_raw_32b_layout() {
        // raw `mov w0, #0x20` — 32B / 8B align
        assert_eq!(std::mem::size_of::<GradientStop>(), 32);
        assert_eq!(std::mem::align_of::<GradientStop>(), 8);
    }

    #[test]
    fn gradient_stop_field_offsets_match_raw() {
        let s = GradientStop {
            value: [0u8; 12],
            type_tag: 0,
            color_effect: ptr::null_mut(),
            position: 0.0,
            _pad: [0u8; 4],
        };
        let base = &s as *const _ as usize;
        assert_eq!(&s.value as *const _ as usize - base, 0x00);
        assert_eq!(&s.type_tag as *const _ as usize - base, 0x0c);
        assert_eq!(&s.color_effect as *const _ as usize - base, 0x10);
        assert_eq!(&s.position as *const _ as usize - base, 0x18);
        assert_eq!(&s._pad as *const _ as usize - base, 0x1c);
    }

    #[test]
    fn gradient_stop_first_16b_matches_color_body() {
        // raw `ldur q0, [stack Color]; str q0, [stop]` — 16B memcpy of value+type_tag
        let color = Color::from_scheme_raw_u32(0x10, ptr::null_mut());
        unsafe {
            let stop = GradientStop::create_with_effect(&color, 0.0);
            // First 12B = Color.value
            for i in 0..12 {
                assert_eq!((*stop).value[i], color.value[i]);
            }
            // +0x0c = type_tag
            assert_eq!((*stop).type_tag, color.type_tag);
            // +0x10 = color_effect (cloned, but src was null → null)
            assert_eq!((*stop).color_effect, ptr::null_mut());
            // +0x18 = position = 0
            assert_eq!((*stop).position, 0.0);
            GradientStop::raw_delete(stop);
        }
    }

    #[test]
    fn gradient_stop_clones_color_effect_independently() {
        // raw `bl 0x65411c (ColorEffect::clone_raw)` — clone the effect ptr.
        unsafe {
            let effect = ColorEffect::create();
            (*effect).add(0x20a, 0.5);
            // Color owns effect via Drop — let Color handle the original cleanup
            let color = Color {
                value: [0u8; 12],
                type_tag: crate::color::color_type::SCHEME,
                color_effect: effect,
            };
            let stop = GradientStop::create_with_effect(&color, 0.0);
            // cloned effect != original effect (deep clone via 0x65411c)
            assert_ne!((*stop).color_effect, effect);
            // cloned effect has same length (1 entry)
            assert_eq!((*(*stop).color_effect).len(), 1);
            // stop's raw_delete frees the CLONED effect (independent allocation)
            GradientStop::raw_delete(stop);
            // color goes out of scope here → Color::Drop frees the ORIGINAL effect.
            // No double-free since clone is independent.
            drop(color);
        }
    }

    #[test]
    fn gradient_stop_position_0_35_bit_pattern() {
        // raw `mov w8, #0x3333; movk w8, #0x3eb3, lsl #16` = 0x3EB33333 (= 0.35 float)
        let v = f32::from_bits(0x3EB33333);
        assert!((v - 0.35_f32).abs() < 1e-5, "value = {}", v);
    }

    // ========================================================================
    // GradientStopCtrl tests
    // ========================================================================

    #[test]
    fn gradient_stop_ctrl_raw_16b_layout() {
        // raw `mov w0, #0x10` — 16B / 8B align
        assert_eq!(std::mem::size_of::<GradientStopCtrl>(), 16);
        assert_eq!(std::mem::align_of::<GradientStopCtrl>(), 8);
    }

    #[test]
    fn gradient_stop_ctrl_field_offsets_match_raw() {
        let ctrl = GradientStopCtrl {
            obj: ptr::null_mut(),
            strong: 1,
        };
        let base = &ctrl as *const _ as usize;
        assert_eq!(&ctrl.obj as *const _ as usize - base, 0x00);
        assert_eq!(&ctrl.strong as *const _ as usize - base, 0x08);
    }

    #[test]
    fn gradient_stop_ctrl_create_and_release() {
        unsafe {
            let color = Color::from_scheme_raw_u32(0x10, ptr::null_mut());
            let stop = GradientStop::create_with_effect(&color, 0.0);
            let ctrl = GradientStopCtrl::create_raw(stop);
            assert_eq!((*ctrl).strong, 1);
            assert_eq!((*ctrl).obj, stop);
            GradientStopCtrl::release(ctrl);
            // After release: stop + ctrl both freed
        }
    }

    // ========================================================================
    // GradientStopsVec tests
    // ========================================================================

    #[test]
    fn gradient_stops_vec_raw_24b_layout() {
        assert_eq!(std::mem::size_of::<GradientStopsVec>(), 24);
        assert_eq!(std::mem::align_of::<GradientStopsVec>(), 8);
    }

    #[test]
    fn gradient_stops_vec_initial_capacity_20() {
        // raw `mov w0, #0xa0` — 160B / 8B per elem = 20 elements cap
        assert_eq!(GRADIENT_STOPS_INITIAL_CAPACITY_BYTES, 160);
        assert_eq!(GRADIENT_STOPS_INITIAL_CAPACITY_ELEMS, 20);
        unsafe {
            let v = GradientStopsVec::new_with_initial_capacity();
            assert_eq!(v.capacity(), 20);
            assert_eq!(v.len(), 0);
        }
    }

    #[test]
    fn gradient_stops_vec_push_back_fast_path() {
        unsafe {
            let mut v = GradientStopsVec::new_with_initial_capacity();
            let color = Color::from_scheme_raw_u32(0x10, ptr::null_mut());
            let stop = GradientStop::create_with_effect(&color, 0.0);
            let ctrl = GradientStopCtrl::create_raw(stop);
            assert_eq!((*ctrl).strong, 1);
            v.push_back(ctrl);
            // raw 의 strong = 2 after push
            assert_eq!((*ctrl).strong, 2);
            assert_eq!(v.len(), 1);
            assert_eq!(v.capacity(), 20);

            // release the caller's share (vector still owns one)
            GradientStopCtrl::release(ctrl);
            assert_eq!((*ctrl).strong, 1);
            // Drop vector → release the last share → ctrl + stop freed
        }
    }

    #[test]
    fn gradient_stops_vec_push_3_stops_block3_pattern() {
        // raw Block 3 의 pattern: 3 stops with different (effect, position)
        unsafe {
            let mut v = GradientStopsVec::new_with_initial_capacity();
            let color = Color::from_scheme_raw_u32(0x10, ptr::null_mut());

            // Stop 1: position 0.0 (raw `str wzr, [x25, #0x18]`)
            let stop1 = GradientStop::create_with_effect(&color, 0.0);
            let ctrl1 = GradientStopCtrl::create_raw(stop1);
            v.push_back(ctrl1);
            GradientStopCtrl::release(ctrl1);

            // Stop 2: position 0.35 (raw `mov w8, #0x3333; movk w8, #0x3eb3 → 0x3EB33333`)
            let stop2 = GradientStop::create_with_effect(&color, f32::from_bits(0x3EB33333));
            let ctrl2 = GradientStopCtrl::create_raw(stop2);
            v.push_back(ctrl2);
            GradientStopCtrl::release(ctrl2);

            // Stop 3: position 1.0 (hypothetical Block 3C)
            let stop3 = GradientStop::create_with_effect(&color, 1.0);
            let ctrl3 = GradientStopCtrl::create_raw(stop3);
            v.push_back(ctrl3);
            GradientStopCtrl::release(ctrl3);

            assert_eq!(v.len(), 3);

            // Walk vector and verify positions
            let s1 = *v.begin;
            let s2 = *v.begin.add(1);
            let s3 = *v.begin.add(2);
            assert_eq!((*(*s1).obj).position, 0.0);
            assert_eq!((*(*s2).obj).position.to_bits(), 0x3EB33333);
            assert_eq!((*(*s3).obj).position, 1.0);
        }
    }

    #[test]
    fn gradient_stops_vec_drop_releases_all_stops() {
        // Drop 이 모든 ctrl 의 release 호출 → strong 0 → stop + ctrl 모두 해제
        for _ in 0..10 {
            unsafe {
                let mut v = GradientStopsVec::new_with_initial_capacity();
                let color = Color::from_scheme_raw_u32(0x10, ptr::null_mut());
                for i in 0..5 {
                    let stop = GradientStop::create_with_effect(&color, i as f32 * 0.2);
                    let ctrl = GradientStopCtrl::create_raw(stop);
                    v.push_back(ctrl);
                    GradientStopCtrl::release(ctrl);
                }
                assert_eq!(v.len(), 5);
                drop(v);
                // No leak panic = success
            }
        }
    }

    #[test]
    fn gradient_stops_vec_empty_drop_no_panic() {
        let v = GradientStopsVec::empty();
        drop(v);
    }

    // ========================================================================
    // 16y: clone_deep + slow realloc tests
    // ========================================================================

    #[test]
    fn gradient_stops_vec_clone_deep_empty() {
        let v = GradientStopsVec::empty();
        unsafe {
            let cloned = v.clone_deep();
            assert_eq!(cloned.len(), 0);
            assert_eq!(cloned.capacity(), 0);
            assert!(cloned.begin.is_null());
        }
    }

    #[test]
    fn gradient_stops_vec_clone_deep_refcounts_increment() {
        unsafe {
            let mut v = GradientStopsVec::new_with_initial_capacity();
            let color = Color::from_scheme_raw_u32(0x10, ptr::null_mut());
            let stop = GradientStop::create_with_effect(&color, 0.5);
            let ctrl = GradientStopCtrl::create_raw(stop);
            assert_eq!((*ctrl).strong, 1);
            v.push_back(ctrl);
            GradientStopCtrl::release(ctrl);
            // strong = 1 (only vector owns)
            assert_eq!((*ctrl).strong, 1);

            // Clone deep — each ctrl gets refcount++
            let cloned = v.clone_deep();
            assert_eq!(cloned.len(), 1);
            // strong = 2 (vector + cloned vector)
            assert_eq!((*ctrl).strong, 2);
            // Both vectors point to SAME ctrl
            assert_eq!(*v.begin, *cloned.begin);
            // capacity is TIGHT (= size_bytes, not 160)
            assert_eq!(cloned.capacity(), 1);

            // Drop both → strong → 0 → ctrl + stop freed
            drop(cloned);
            assert_eq!((*ctrl).strong, 1);
            drop(v);
        }
    }

    #[test]
    fn gradient_stops_vec_slow_realloc_at_capacity_21() {
        // 20-element initial cap → push 21번째 시 realloc
        unsafe {
            let mut v = GradientStopsVec::new_with_initial_capacity();
            let color = Color::from_scheme_raw_u32(0x10, ptr::null_mut());

            // Fill 20 (fast path)
            for i in 0..20 {
                let s = GradientStop::create_with_effect(&color, i as f32 * 0.05);
                let c = GradientStopCtrl::create_raw(s);
                v.push_back(c);
                GradientStopCtrl::release(c);
            }
            assert_eq!(v.len(), 20);
            assert_eq!(v.capacity(), 20);

            // 21번째 push (slow path — realloc to 40)
            let s21 = GradientStop::create_with_effect(&color, 1.0);
            let c21 = GradientStopCtrl::create_raw(s21);
            v.push_back(c21);
            GradientStopCtrl::release(c21);

            assert_eq!(v.len(), 21);
            assert_eq!(v.capacity(), 40); // doubled

            // 모든 elements valid
            for i in 0..21 {
                let ctrl = *v.begin.add(i);
                assert!(!ctrl.is_null());
                assert!(!(*ctrl).obj.is_null());
            }
            // cleanup via drop
        }
    }

    // ========================================================================
    // 16y: PStops + GradientBrush::set_stops integration tests
    // ========================================================================

    #[test]
    fn pstops_raw_40b_layout() {
        // raw `0x655578: mov w0, #0x28` (= 40)
        assert_eq!(std::mem::size_of::<crate::property::PStops>(), 40);
        assert_eq!(std::mem::align_of::<crate::property::PStops>(), 8);
    }

    #[test]
    fn pstops_field_offsets_match_raw() {
        unsafe {
            let v = GradientStopsVec::empty();
            let ps = crate::property::PStops::new(2, &v);
            let base = &ps as *const _ as usize;
            assert_eq!(&ps.vtable as *const _ as usize - base, 0x00);
            assert_eq!(&ps.state as *const _ as usize - base, 0x08);
            assert_eq!(&ps.stops as *const _ as usize - base, 0x10);
        }
    }

    #[test]
    fn pstops_clone_deep_3_stops_preserves_data() {
        unsafe {
            let mut v = GradientStopsVec::new_with_initial_capacity();
            let color = Color::from_scheme_raw_u32(0x10, ptr::null_mut());
            for i in 0..3 {
                let s = GradientStop::create_with_effect(&color, i as f32 * 0.5);
                let c = GradientStopCtrl::create_raw(s);
                v.push_back(c);
                GradientStopCtrl::release(c);
            }

            let ps = crate::property::PStops::new(2, &v);
            assert_eq!(ps.len(), 3);

            // Each stop's position matches src
            for i in 0..3 {
                let cloned_ctrl = *ps.stops.begin.add(i);
                let src_ctrl = *v.begin.add(i);
                assert_eq!(cloned_ctrl, src_ctrl); // same ctrl (just refcount++)
                assert_eq!((*(*cloned_ctrl).obj).position, i as f32 * 0.5);
            }
            // src still owns first share, ps clone owns the second
            for i in 0..3 {
                let ctrl = *v.begin.add(i);
                assert_eq!((*ctrl).strong, 2);
            }
        }
    }

    #[test]
    fn gradient_brush_set_stops_attaches_key_0x266() {
        // raw `0x16fb9c-0x16fbd8` 의 SetStops sequence — bag.attach(0x266, PStops)
        unsafe {
            let mut gb = crate::brush::GradientBrush::new();
            assert_eq!(gb.bag_size(), 8); // 8 default keys

            // Build 3-stop vector
            let mut v = GradientStopsVec::new_with_initial_capacity();
            let color = Color::from_scheme_raw_u32(0x10, ptr::null_mut());
            for &pos in &[0.0_f32, 0.5, 1.0] {
                let s = GradientStop::create_with_effect(&color, pos);
                let c = GradientStopCtrl::create_raw(s);
                v.push_back(c);
                GradientStopCtrl::release(c);
            }

            gb.set_stops(&v);
            assert_eq!(gb.bag_size(), 9); // +1 (key 0x266)

            // Verify key 0x266 attached
            let pk = crate::property_key::PropertyKey::from_int(
                crate::brush::GradientBrush::KEY_STOPS,
            );
            let impl_ref = gb.bag.impl_ref().expect("bag impl");
            assert!(impl_ref.find_equal(&pk).is_ok());

            // get_stops returns 3 positions
            let stops = gb.get_stops();
            assert_eq!(stops.len(), 3);
            assert_eq!(stops[0].0, 0.0);
            assert_eq!(stops[1].0, 0.5);
            assert_eq!(stops[2].0, 1.0);
        }
    }

    #[test]
    fn gradient_brush_set_angle_270_overrides_default() {
        // raw `0x16fc50-0x16fc94` of Block 4: set angle to 270.0
        unsafe {
            let mut gb = crate::brush::GradientBrush::new();
            assert_eq!(gb.get_angle_degrees(), 0.0); // default
            gb.set_angle_degrees(270.0);
            assert_eq!(gb.get_angle_degrees(), 270.0);
            assert_eq!(gb.get_angle_degrees().to_bits(), 0x43870000);
        }
    }

    #[test]
    fn gradient_brush_block4_full_override_sequence() {
        // raw `0x16fb9c-0x16fcdc`: SetStops + 4 re-attaches
        unsafe {
            let mut gb = crate::brush::GradientBrush::new();

            // Build 3-stop vector
            let mut v = GradientStopsVec::new_with_initial_capacity();
            let color = Color::from_scheme_raw_u32(0x10, ptr::null_mut());
            for &pos in &[0.0_f32, f32::from_bits(0x3EB33333), 1.0] {
                let s = GradientStop::create_with_effect(&color, pos);
                let c = GradientStopCtrl::create_raw(s);
                v.push_back(c);
                GradientStopCtrl::release(c);
            }

            gb.set_stops(&v);
            gb.set_scaled(true);            // raw 0x16fbdc: key 0x265 = true
            gb.set_style(0);                // raw 0x16fc14: key 0x25f = 0
            gb.set_angle_degrees(270.0);    // raw 0x16fc50: key 0x260 = 270°
            gb.set_flip(true);              // raw 0x16fca0: key 0x261 = true

            // 9 keys total (8 default + 1 stops)
            assert_eq!(gb.bag_size(), 9);
            assert_eq!(gb.get_angle_degrees(), 270.0);
            assert_eq!(gb.get_stops().len(), 3);
        }
    }
}

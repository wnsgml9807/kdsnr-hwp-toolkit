//! `Hnc::Shape::Scene3D` (8B) — `PropertyBag` 단일 wrapper.
//!
//! libHncDrawingEngine 의 `0x1d0740 ~ 0x1d17cc` 영역 (~30+ getters/setters).
//! Shape `body.scene3d_ctrl` (BodyProperty +0x08) 가 가리키는 `SharePtr<Scene3D>`
//! 의 underlying type. 3D scene 의 camera/lightrig/backdrop 좌표를
//! `PropertyBag` 안에 typed property 로 보관.
//!
//! # raw layout (확정 from C2 copy ctor `0x1d0b10` + D2 `0x1d0b68`)
//!
//! ```text
//! Scene3D (8B):
//!   +0x00: PropertyBag bag         ; SharePtr<PropertyBagImpl> 의 single ptr
//! ```
//!
//! Scene3D 는 **PropertyBag 의 sole-member wrapper** — base class 가 아닌
//! composition. 모든 method 가 inline 으로 PropertyBag 에 위임:
//!
//! ## byte-eq method 매핑 (정공법, libHncDrawingEngine arm64)
//!
//! | Symbol                       | Raw addr   | 위임 대상                          |
//! |------------------------------|------------|----------------------------------|
//! | `Scene3D::C2(camera, lrigs, lrigds)` | `0x1d0740` | PropertyBag::C1(false) + 30+ attach |
//! | `Scene3D::C2(const Scene3D&)`        | `0x1d0b10` | PropertyBag::Clone                  |
//! | `Scene3D::C1(const Scene3D&)`        | `0x1d0b3c` | = C2(copy) (Itanium ABI)            |
//! | `Scene3D::D2()`                      | `0x1d0b68` | `b PropertyBag::D1`                 |
//! | `Scene3D::operator=(const&)`         | `0x1d0b6c` | copy-and-swap                       |
//! | `Scene3D::Swap(Scene3D&)`            | `0x1d0bc8` | `b PropertyBag::Swap`               |
//! | `Scene3D::operator==(const&) const`  | `0x1d0bcc` | `b PropertyBag::eq`                 |
//! | `Scene3D::operator!=(const&) const`  | `0x1d0bd0` | `eor` of eq                         |
//! | `Scene3D::operator<(const&) const`   | `0x1d0be8` | `b PropertyBag::lt`                 |
//!
//! # 본 단계 (L-5c-3 잔여) scope
//!
//! - struct 선언 + 작은 method (Swap/eq/ne/lt + copy ctor + D2)
//! - 3-arg main ctor `Scene3D::C2(camera, lrig, lrigd)` 는 30+ default property
//!   attach 의 sequence — `PropertyKey::C1` + `PropertyBag::Attach` 가 Property
//!   subclass 의 PEnum/PFloat 와 통합되어야 byte-eq port 가능. 다음 sub-task.
//! - PropertyBag::Clone (raw 0x4d928) tree iter — Property vfunc[4] 의존, deferred

use crate::property_bag::PropertyBag;

/// raw 8B `Hnc::Shape::Scene3D` — PropertyBag wrapper.
///
/// 단일 member 가 PropertyBag (8B) 이므로 layout 가 PropertyBag 와 정확히 동일.
/// `#[repr(C)]` 로 라인업 보장.
#[repr(C)]
pub struct Scene3D {
    /// raw +0x00..+0x08: PropertyBag (= ControlBlock<PropertyBagImpl>*).
    pub bag: PropertyBag,
}

pub const SCENE3D_SIZE_BYTES: usize = 8;
pub const SCENE3D_ALIGN_BYTES: usize = 8;

const _: () = assert!(std::mem::size_of::<Scene3D>() == SCENE3D_SIZE_BYTES);
const _: () = assert!(std::mem::align_of::<Scene3D>() == SCENE3D_ALIGN_BYTES);

impl Scene3D {
    /// Empty Scene3D — default-constructed PropertyBag (non-merged).
    ///
    /// raw 3-arg ctor `0x1d0740` 은 30+ default property attach 수행 → 미구현 (deferred).
    /// 본 fn 은 minimal helper: `Scene3D` struct 만 valid 인 empty bag 으로 init.
    pub fn new_empty() -> Self {
        Scene3D {
            bag: PropertyBag::new(false),
        }
    }

    /// raw `Scene3D::C2(const Scene3D&)` @ `0x1d0b10` (11 instr) 1:1.
    ///
    /// ```asm
    /// stp x20, x19, [sp, #-0x20]!
    /// stp x29, x30, [sp, #0x10]
    /// add x29, sp, #0x10
    /// mov x19, x0        ; this
    /// mov x8, x0          ; sret slot = this (Clone 의 result 가 self.bag 로 직접 직격)
    /// mov x0, x1          ; arg = other
    /// bl  PropertyBag::Clone
    /// mov x0, x19
    /// ret
    /// ```
    ///
    /// **현재 wrapper-level fallback**: PropertyBag::Clone (raw 0x4d928, tree iter)
    /// 미구현 → 보수적으로 empty bag 으로 새로 짓는 fallback. tree clone port 시 갱신.
    pub fn copy_of(other: &Scene3D) -> Self {
        // Ideal byte-eq: bag = PropertyBag::clone(&other.bag)
        // 현재 PropertyBag::Clone 미구현 (Property vfunc[4] 의존) → 빈 bag 으로 fallback.
        // **caller 가 비-empty bag 의 정확 deep copy 필요한 경우 panic 보다 empty 가 안전**.
        // — full Clone 은 별도 sub-task.
        let _ = other; // suppress unused warning until Clone port
        Scene3D::new_empty()
    }

    /// raw `Scene3D::Swap(Scene3D&)` @ `0x1d0bc8` (1 instr).
    ///
    /// `b PropertyBag::Swap` — pure tail call.
    #[inline]
    pub fn swap(&mut self, other: &mut Scene3D) {
        self.bag.swap(&mut other.bag);
    }

    /// raw `Scene3D::operator==(const Scene3D&) const` @ `0x1d0bcc` (1 instr).
    ///
    /// `b PropertyBag::operator==` — pure tail call.
    #[inline]
    pub fn eq_op(&self, other: &Scene3D) -> bool {
        self.bag.eq_op(&other.bag)
    }

    /// raw `Scene3D::operator!=(const Scene3D&) const` @ `0x1d0bd0` (6 instr).
    ///
    /// ```asm
    /// stp x29, x30, [sp, #-0x10]!
    /// mov x29, sp
    /// bl  PropertyBag::eq
    /// eor w0, w0, #0x1
    /// ldp x29, x30, [sp], #0x10
    /// ret
    /// ```
    #[inline]
    pub fn ne_op(&self, other: &Scene3D) -> bool {
        !self.eq_op(other)
    }

    /// raw `Scene3D::operator<(const Scene3D&) const` @ `0x1d0be8` (1 instr).
    ///
    /// `b PropertyBag::operator<` — tail call to libHncFoundation `0x4d798`.
    /// PropertyBag::lt 의 byte-eq port 는 sub-task (tree compare 의존).
    /// 현재 wrapper-level fallback: ctrl ptr lexicographic compare.
    pub fn lt_op(&self, other: &Scene3D) -> bool {
        // PropertyBag::lt port 미완 → 보수적 fallback.
        (self.bag.ctrl as usize) < (other.bag.ctrl as usize)
    }
}

impl PartialEq for Scene3D {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.eq_op(other)
    }
}

// D2 dtor 는 PropertyBag 의 Drop 으로 자동 (`b PropertyBag::D1`).

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_layout_size_align() {
        assert_eq!(std::mem::size_of::<Scene3D>(), 8);
        assert_eq!(std::mem::align_of::<Scene3D>(), 8);
    }

    #[test]
    fn raw_layout_bag_offset_zero() {
        let s = Scene3D::new_empty();
        let s_addr = &s as *const _ as usize;
        let bag_addr = &s.bag as *const _ as usize;
        assert_eq!(bag_addr - s_addr, 0x00);
    }

    #[test]
    fn empty_scene3d_drops_cleanly() {
        for _ in 0..100 {
            let s = Scene3D::new_empty();
            drop(s);
        }
    }

    #[test]
    fn eq_empty_pairs_are_equal() {
        let a = Scene3D::new_empty();
        let b = Scene3D::new_empty();
        assert!(a.eq_op(&b));
        assert!(!a.ne_op(&b));
    }

    #[test]
    fn swap_exchanges_ctrl_ptrs() {
        let mut a = Scene3D::new_empty();
        let mut b = Scene3D::new_empty();
        let a_ctrl = a.bag.ctrl as usize;
        let b_ctrl = b.bag.ctrl as usize;
        a.swap(&mut b);
        assert_eq!(a.bag.ctrl as usize, b_ctrl);
        assert_eq!(b.bag.ctrl as usize, a_ctrl);
    }

    #[test]
    fn copy_of_yields_independent_empty_bag() {
        let a = Scene3D::new_empty();
        let b = Scene3D::copy_of(&a);
        // 둘은 다른 ctrl ptr 을 가짐 (each new_empty alloc)
        assert_ne!(a.bag.ctrl as usize, b.bag.ctrl as usize);
        // 그러나 둘 다 empty → eq 는 true
        assert!(a.eq_op(&b));
    }

    #[test]
    fn partial_eq_trait_works() {
        let a = Scene3D::new_empty();
        let b = Scene3D::new_empty();
        assert!(a == b);
    }
}

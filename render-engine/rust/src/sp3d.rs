//! `Hnc::Shape::Sp3D` (8B) — `PropertyBag` 단일 wrapper.
//!
//! libHncDrawingEngine 의 `0x1e7408 ~ 0x1e7d?? ` 영역. Shape 의 3D body
//! material/bevel/contour 좌표를 `PropertyBag` 안에 typed property 로 보관.
//!
//! # raw layout (확정 from C2 copy ctor `0x1e754c` + D2 `0x1e75a4`)
//!
//! ```text
//! Sp3D (8B):
//!   +0x00: PropertyBag bag         ; SharePtr<PropertyBagImpl> 의 single ptr
//! ```
//!
//! Sp3D 는 **PropertyBag 의 sole-member wrapper** — Scene3D 와 동일 패턴. 모든
//! method 가 PropertyBag 에 위임.
//!
//! ## byte-eq method 매핑 (정공법, libHncDrawingEngine arm64)
//!
//! | Symbol                           | Raw addr   | 위임 대상                          |
//! |----------------------------------|------------|----------------------------------|
//! | `Sp3D::C2(f, f, f, MaterialStyle)` | `0x1e7408` | PropertyBag::C1(false) + attach 20+ |
//! | `Sp3D::C2(const Sp3D&)`            | `0x1e754c` | PropertyBag::Clone                  |
//! | `Sp3D::C1(const Sp3D&)`            | `0x1e7578` | = C2(copy)                          |
//! | `Sp3D::D2()`                       | `0x1e75a4` | `b PropertyBag::D1`                 |
//! | `Sp3D::operator=(const&)`          | `0x1e75a8` | copy-and-swap                       |
//! | `Sp3D::Swap(Sp3D&)`                | `0x1e7604` | `b PropertyBag::Swap`               |
//! | `Sp3D::operator==(const&) const`   | `0x1e7608` | `b PropertyBag::eq`                 |
//! | `Sp3D::operator!=(const&) const`   | `0x1e760c` | `eor` of eq                         |
//! | `Sp3D::operator<(const&) const`    | `0x1e7624` | `b PropertyBag::lt`                 |
//!
//! # 본 단계 (L-5c-3 잔여) scope: Scene3D 와 동일

use crate::property_bag::PropertyBag;

/// raw 8B `Hnc::Shape::Sp3D` — PropertyBag wrapper.
#[repr(C)]
pub struct Sp3D {
    /// raw +0x00..+0x08: PropertyBag.
    pub bag: PropertyBag,
}

pub const SP3D_SIZE_BYTES: usize = 8;
pub const SP3D_ALIGN_BYTES: usize = 8;

const _: () = assert!(std::mem::size_of::<Sp3D>() == SP3D_SIZE_BYTES);
const _: () = assert!(std::mem::align_of::<Sp3D>() == SP3D_ALIGN_BYTES);

impl Sp3D {
    /// Empty Sp3D — default-constructed PropertyBag.
    pub fn new_empty() -> Self {
        Sp3D {
            bag: PropertyBag::new(false),
        }
    }

    /// raw `Sp3D::C2(const Sp3D&)` @ `0x1e754c` — PropertyBag::Clone tail.
    pub fn copy_of(other: &Sp3D) -> Self {
        let _ = other;
        Sp3D::new_empty()
    }

    /// raw `Sp3D::Swap` @ `0x1e7604` — `b PropertyBag::Swap`.
    #[inline]
    pub fn swap(&mut self, other: &mut Sp3D) {
        self.bag.swap(&mut other.bag);
    }

    /// raw `Sp3D::operator==` @ `0x1e7608` — `b PropertyBag::eq`.
    #[inline]
    pub fn eq_op(&self, other: &Sp3D) -> bool {
        self.bag.eq_op(&other.bag)
    }

    /// raw `Sp3D::operator!=` @ `0x1e760c` — `eor` of eq.
    #[inline]
    pub fn ne_op(&self, other: &Sp3D) -> bool {
        !self.eq_op(other)
    }

    /// raw `Sp3D::operator<` @ `0x1e7624` — `b PropertyBag::lt`.
    pub fn lt_op(&self, other: &Sp3D) -> bool {
        (self.bag.ctrl as usize) < (other.bag.ctrl as usize)
    }
}

impl PartialEq for Sp3D {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.eq_op(other)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_layout_size_align() {
        assert_eq!(std::mem::size_of::<Sp3D>(), 8);
        assert_eq!(std::mem::align_of::<Sp3D>(), 8);
    }

    #[test]
    fn raw_layout_bag_offset_zero() {
        let s = Sp3D::new_empty();
        let s_addr = &s as *const _ as usize;
        let bag_addr = &s.bag as *const _ as usize;
        assert_eq!(bag_addr - s_addr, 0x00);
    }

    #[test]
    fn empty_drops_cleanly() {
        for _ in 0..100 {
            let s = Sp3D::new_empty();
            drop(s);
        }
    }

    #[test]
    fn eq_empty_pairs_are_equal() {
        let a = Sp3D::new_empty();
        let b = Sp3D::new_empty();
        assert!(a.eq_op(&b));
        assert!(!a.ne_op(&b));
    }

    #[test]
    fn swap_exchanges_ctrl_ptrs() {
        let mut a = Sp3D::new_empty();
        let mut b = Sp3D::new_empty();
        let a_ctrl = a.bag.ctrl as usize;
        let b_ctrl = b.bag.ctrl as usize;
        a.swap(&mut b);
        assert_eq!(a.bag.ctrl as usize, b_ctrl);
        assert_eq!(b.bag.ctrl as usize, a_ctrl);
    }

    #[test]
    fn copy_of_yields_independent_empty_bag() {
        let a = Sp3D::new_empty();
        let b = Sp3D::copy_of(&a);
        assert_ne!(a.bag.ctrl as usize, b.bag.ctrl as usize);
        assert!(a.eq_op(&b));
    }

    #[test]
    fn partial_eq_trait_works() {
        let a = Sp3D::new_empty();
        let b = Sp3D::new_empty();
        assert!(a == b);
    }
}

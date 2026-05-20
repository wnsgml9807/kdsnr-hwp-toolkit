//! `Hnc::Shape::EffectStyle` — 24B composite of 3 SharePtr.
//!
//! ## raw 구조 (확정 by ctor `0x16d8d4`)
//!
//! 24B layout:
//! - +0x00: `SharePtr<Scene3D>` (8B) — 3D scene parameters
//! - +0x08: `SharePtr<Sp3D>` (8B) — 3D shape properties
//! - +0x10: `SharePtr<Effects>` (8B) — 2D effects (shadow/glow/blur/reflection/softEdge)
//!
//! ctor signature: `EffectStyle(UniquePtr<Effects>, UniquePtr<Scene3D>, UniquePtr<Sp3D>)`
//! — args 순서와 layout 순서가 다름 (raw asm 의 store offsets 으로 확정).
//!
//! ## raw ctor (`0x16d8d4`) 알고리즘
//!
//! ```text
//! arg1 = UniquePtr<Effects>   (x1)
//! arg2 = UniquePtr<Scene3D>   (x2)
//! arg3 = UniquePtr<Sp3D>      (x3)
//!
//! self[+0x00] = arg2.ptr  ; Scene3D
//! if arg2.ptr non-null && arg2.ptr.T non-null:
//!   arg2.ptr.refcount++
//!   call vfunc (0x64a1d4) — Scene3D::AddRef 또는 similar
//!
//! self[+0x08] = arg3.ptr  ; Sp3D
//! ...
//!
//! self[+0x10] = arg1.ptr  ; Effects
//! ...
//! ```
//!
//! ## byte-eq 경계
//!
//! Effects/Scene3D/Sp3D 는 자체 복잡한 sub-class (각각 multi-session RE).
//! 본 단계는 opaque placeholder (ZST) — fields 는 `Option<Box<...>>` 로 모델링.
//! output PDF 의 effect 결과는 sub-class RE 완료 시 확정.

use std::ptr;

/// `Hnc::Shape::Effects` — 24B std::map<u32 effect_key, SharePtr<Effect>>.
///
/// 16-μ 단계에서 outer layout 완성 — 자세한 RE 는 `effects_container.rs` 참조.
/// 본 module 의 EffectStyle 의 +0x10 SharePtr field 가 이 타입의 `ControlBlock<Effects>*`.
pub use crate::effects_container::Effects;

/// L-5c-3 잔여 (2026-05-17): real Scene3D (8B PropertyBag wrapper) — placeholder 폐기.
pub use crate::scene3d::Scene3D;
/// L-5c-3 잔여 (2026-05-17): real Sp3D (8B PropertyBag wrapper) — placeholder 폐기.
pub use crate::sp3d::Sp3D;

impl std::fmt::Debug for Effects {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Effects(opaque)")
    }
}

/// raw 24B `Hnc::Shape::EffectStyle`.
///
/// 각 field 가 SharePtr 인 raw 와 달리, Rust 는 nullable owning ptr 사용 — 추후
/// SharePtr 도입 시 교체.
#[repr(C)]
#[derive(Debug)]
pub struct EffectStyle {
    /// raw +0x00: SharePtr<Scene3D> (= ControlBlock<Scene3D>*).
    pub scene3d: *mut crate::share_ptr::ControlBlock<Scene3D>,
    /// raw +0x08: SharePtr<Sp3D>.
    pub sp3d: *mut crate::share_ptr::ControlBlock<Sp3D>,
    /// raw +0x10: SharePtr<Effects>.
    pub effects: *mut crate::share_ptr::ControlBlock<Effects>,
}

pub const EFFECT_STYLE_SIZE_BYTES: usize = 24;
pub const EFFECT_STYLE_ALIGN_BYTES: usize = 8;

const _: () = assert!(std::mem::size_of::<EffectStyle>() == EFFECT_STYLE_SIZE_BYTES);
const _: () = assert!(std::mem::align_of::<EffectStyle>() == EFFECT_STYLE_ALIGN_BYTES);

impl EffectStyle {
    /// raw `EffectStyle::EffectStyle()` 추정 — 모든 SharePtr 가 null.
    ///
    /// raw 의 default ctor 는 별도 export 안 됨 — 단일 ctor (3-arg) 만 있음.
    /// 본 Rust port 의 `new_empty` 는 향후 default 가 export 되면 RE 후 정정.
    pub fn new_empty() -> Self {
        EffectStyle {
            scene3d: ptr::null_mut(),
            sp3d: ptr::null_mut(),
            effects: ptr::null_mut(),
        }
    }

    /// raw `EffectStyle::EffectStyle(UniquePtr<Effects>, UniquePtr<Scene3D>, UniquePtr<Sp3D>)`
    /// (`0x16d8d4`) — 3 ControlBlock 을 받아 ptr 복사 + refcount++ + 보조 vfunc 호출.
    ///
    /// **현재 scope**: ControlBlock raw addresses 만 처리; raw 의 보조 vfunc
    /// (`0x64a1d4`/`0x64aa18`/`0x649980`) 는 Scene3D/Sp3D/Effects 의 RE 가 끝난
    /// 후 정확한 호출. 본 단계는 refcount++ 만 byte-eq.
    ///
    /// # Safety
    /// 인자 ptrs 가 valid ControlBlock 또는 null.
    pub unsafe fn new(
        effects: *mut crate::share_ptr::ControlBlock<Effects>,
        scene3d: *mut crate::share_ptr::ControlBlock<Scene3D>,
        sp3d: *mut crate::share_ptr::ControlBlock<Sp3D>,
    ) -> Self {
        // raw `16d8f0-16d914`: scene3d field + refcount++ if non-null
        if !scene3d.is_null() && !(*scene3d).obj.is_null() {
            (*scene3d).refcount = (*scene3d).refcount.wrapping_add(1);
            // raw 보조 vfunc bl 0x64a1d4 — Scene3D AddRef 변종 (multi-session deferred)
        }
        // raw `16d918-16d940`: sp3d field + refcount++
        if !sp3d.is_null() && !(*sp3d).obj.is_null() {
            (*sp3d).refcount = (*sp3d).refcount.wrapping_add(1);
        }
        // raw `16d944-16d968`: effects field + refcount++
        if !effects.is_null() && !(*effects).obj.is_null() {
            (*effects).refcount = (*effects).refcount.wrapping_add(1);
        }
        EffectStyle {
            scene3d,
            sp3d,
            effects,
        }
    }

    /// 모든 3 SharePtr 가 null 인지.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.scene3d.is_null() && self.sp3d.is_null() && self.effects.is_null()
    }
}

impl Default for EffectStyle {
    fn default() -> Self {
        Self::new_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_layout_size_align() {
        assert_eq!(std::mem::size_of::<EffectStyle>(), 24);
        assert_eq!(std::mem::align_of::<EffectStyle>(), 8);
    }

    #[test]
    fn field_offsets_match_raw() {
        let es = EffectStyle::new_empty();
        let p = &es as *const EffectStyle as usize;
        assert_eq!(&es.scene3d as *const _ as usize - p, 0x00);
        assert_eq!(&es.sp3d as *const _ as usize - p, 0x08);
        assert_eq!(&es.effects as *const _ as usize - p, 0x10);
    }

    #[test]
    fn empty_state_all_null() {
        let es = EffectStyle::new_empty();
        assert!(es.is_empty());
        assert!(es.scene3d.is_null());
        assert!(es.sp3d.is_null());
        assert!(es.effects.is_null());
    }

    #[test]
    fn new_with_all_null_args_yields_empty() {
        unsafe {
            let es = EffectStyle::new(ptr::null_mut(), ptr::null_mut(), ptr::null_mut());
            assert!(es.is_empty());
        }
    }
}

//! `Hnc::Shape::ObjectDefaults` — 24B 1:1 byte-equivalent port.
//!
//! libHncDrawingEngine_arm64 의 `ObjectDefaults` 는 OOXML `<a:objectDefaults>`
//! 의 3 종류 SharePtr default (LineDefault / TextBoxDefault / ShapeDefault).
//!
//! # raw 24B layout (확정 from `ObjectDefaults::ObjectDefaults()` @ `0x1ae4ec`
//! + dtor `0x1ae6f4` 의 3-pattern teardown + `GetLineDefault` @ `0x1ae9b4`)
//!
//! ```text
//! offset  field            타입                                  의미
//! 0x00    line_default     SharePtr<DefaultProperty> (= CB*)    8B
//! 0x08    textbox_default  SharePtr<DefaultProperty>            8B
//! 0x10    shape_default    SharePtr<DefaultProperty>            8B
//! ```
//!
//! 총 24B / 8B align.
//!
//! # raw default ctor `0x1ae4ec`
//!
//! ```asm
//! 1ae4ec: stp xzr, xzr, [x0]       ; line_default=0, textbox_default=0
//! 1ae4f0: str xzr, [x0, #0x10]     ; shape_default=0
//! 1ae4f4: ret
//! ```
//!
//! # raw dtor `0x1ae6f4` (3 SharePtr drop iterations)
//!
//! offset 0x10 (shape) → 0x08 (textbox) → 0x00 (line) 순서:
//! ```asm
//! ldr x20, [x19, #offset]    ; x20 = CB*
//! cbz x20, skip
//! ldr x0, [x20]              ; x0 = CB.obj (DefaultProperty*, virtual)
//! cbz x0, skip
//! ldr x8, [x20, #0x8]; subs x8, x8, #1   ; refcount--
//! b.ne save_dec
//! ; refcount == 0 path:
//!   ldr x8, [x0]; ldr x8, [x8, #0x8]    ; vtable[+0x8] = ~DefaultProperty()
//!   blr x8                              ; virtual dtor call
//!   mov x0, x20; bl operator_delete    ; free CB
//! save_dec: str x8, [x20, #0x8]
//! str xzr, [x19, #offset]               ; clear field
//! ```
//!
//! # 본 R-1.5.7 단계 scope
//!
//! - 24B layout + field offsets 검증
//! - default ctor (3 fields = null)
//! - dtor (3 SharePtr drop)
//! - GetLineDefault / GetTextBoxDefault / GetShapeDefault accessors
//!   (raw 의 GetLineDefault 가 SharePtr.Clone semantic: refcount++ + return CB ptr)
//!
//! # 의도적 deferred
//!
//! - `SetLineDefault` / `SetTextBoxDefault` / `SetShapeDefault` (raw `0x1ae9d0` 등) —
//!   본 byte-eq 가 Theme empty 상태 (3 default 모두 null) 면 호출 안 됨.
//! - `Apply(Form&, bool)` (`0x1043ac`) — Form 종속.
//! - `DefaultProperty` 자체의 layout — virtual class (vtable[+8] dtor), Theme empty 상태엔
//!   인스턴스 발생 안 함.
//! - copy ctor / `operator=` / `Swap` — sub-objects 종속.

use crate::share_ptr::{ControlBlock, SharePtr};
use std::ptr;

/// `Hnc::Shape::DefaultProperty` — opaque placeholder (virtual class, RE 미완).
///
/// 본 R-1.5.7 단계의 ObjectDefaults byte-eq 에 인스턴스 발생 안 함. SharePtr<T>
/// drop 시점에 vtable[+8] dispatch 가 필요한데, ZST placeholder 로는 호출 불가
/// — empty 상태에서만 byte-eq 보장.
pub struct DefaultProperty {
    _opaque: [u8; 0],
}

/// raw 24B `Hnc::Shape::ObjectDefaults`.
#[repr(C)]
pub struct ObjectDefaults {
    /// raw +0x00: line_default SharePtr (= ControlBlock<DefaultProperty>*).
    pub line_default: *mut ControlBlock<DefaultProperty>,
    /// raw +0x08: textbox_default SharePtr.
    pub textbox_default: *mut ControlBlock<DefaultProperty>,
    /// raw +0x10: shape_default SharePtr.
    pub shape_default: *mut ControlBlock<DefaultProperty>,
}

pub const OBJECT_DEFAULTS_SIZE_BYTES: usize = 24;
pub const OBJECT_DEFAULTS_ALIGN_BYTES: usize = 8;

const _: () = assert!(std::mem::size_of::<ObjectDefaults>() == OBJECT_DEFAULTS_SIZE_BYTES);
const _: () = assert!(std::mem::align_of::<ObjectDefaults>() == OBJECT_DEFAULTS_ALIGN_BYTES);

impl ObjectDefaults {
    /// raw `ObjectDefaults::ObjectDefaults()` (`0x1ae4ec`) — 3 fields 모두 null.
    pub fn new() -> Self {
        ObjectDefaults {
            line_default: ptr::null_mut(),
            textbox_default: ptr::null_mut(),
            shape_default: ptr::null_mut(),
        }
    }

    /// raw `GetLineDefault() const` (`0x1ae9b4`) — sret SharePtr clone.
    ///
    /// ```asm
    /// 1ae9b4: ldr x9, [x0]           ; x9 = CB ptr
    /// 1ae9b8: str x9, [x8]           ; *sret = CB ptr
    /// 1ae9bc: cbz x9, exit
    /// 1ae9c0: ldr x8, [x9, #0x8]
    /// 1ae9c4: add x8, x8, #0x1      ; refcount++
    /// 1ae9c8: str x8, [x9, #0x8]
    /// ```
    ///
    /// SharePtr<DefaultProperty> 를 Clone semantic 으로 반환 — refcount 증가.
    pub fn get_line_default(&self) -> SharePtr<DefaultProperty> {
        unsafe { Self::clone_share_ptr(self.line_default) }
    }

    /// `GetTextBoxDefault()` — 동일 패턴 (offset 0x08).
    pub fn get_textbox_default(&self) -> SharePtr<DefaultProperty> {
        unsafe { Self::clone_share_ptr(self.textbox_default) }
    }

    /// `GetShapeDefault()` — 동일 패턴 (offset 0x10).
    pub fn get_shape_default(&self) -> SharePtr<DefaultProperty> {
        unsafe { Self::clone_share_ptr(self.shape_default) }
    }

    /// raw SharePtr clone 의 byte-equivalent — null safe, refcount++.
    ///
    /// # Safety
    /// `cb` 는 valid ControlBlock* 또는 null.
    unsafe fn clone_share_ptr(
        cb: *mut ControlBlock<DefaultProperty>,
    ) -> SharePtr<DefaultProperty> {
        if !cb.is_null() {
            (*cb).refcount = (*cb).refcount.wrapping_add(1);
        }
        SharePtr { raw: cb }
    }

    /// empty (모든 default 가 null)?
    pub fn is_empty(&self) -> bool {
        self.line_default.is_null()
            && self.textbox_default.is_null()
            && self.shape_default.is_null()
    }

    /// raw `ObjectDefaults::ObjectDefaults(const ObjectDefaults&)` (`0x1ae504`) 1:1.
    ///
    /// 알고리즘 (3 SharePtr<DefaultProperty> field 각 동일):
    /// 1. src.share_ptr (ControlBlock*) load.
    /// 2. null 이면 this.share_ptr = null.
    /// 3. non-null 이면:
    ///    - ControlBlock.T_ptr load
    ///    - null 이면 this.share_ptr = null
    ///    - non-null 이면: virtual `T->Clone()` 호출 (raw `vtable[+0x28]`)
    ///      - 결과 null 이면 this.share_ptr = null
    ///      - 결과 non-null: alloc 16B ControlBlock + {T_ptr: result, refcount: 1},
    ///        this.share_ptr = new_cb
    ///
    /// **현재 scope 제약**: DefaultProperty 의 virtual Clone vfunc (`vtable[+0x28]`)
    /// 는 본 세션 RE 안 됨 — `Theme(true)` 가 만드는 ObjectDefaults 는 3 SharePtr
    /// 모두 null (CreateDefault deferred) 이므로 도달 가능 input 에선 null path
    /// 만 사용됨. non-null path 도달 시 DefaultProperty 의 virtual class layout +
    /// Clone vfunc RE 필요.
    ///
    /// # Safety
    /// `src` 는 valid, `this` 는 uninit heap slot (24B).
    pub unsafe fn copy_from_raw(this: *mut ObjectDefaults, src: *const ObjectDefaults) {
        // raw `1ae51c..1ae564`: line_default (offset 0x00)
        // raw `1ae578..1ae5b8`: textbox_default (offset 0x08)
        // raw `1ae5cc..1ae604`: shape_default (offset 0x10)
        //
        // 본 함수는 SharePtr<DefaultProperty> 3 개 각각에 대해:
        //   if src.cb == null OR src.cb.T == null OR T->Clone() returns null:
        //       this.cb = null
        //   else:
        //       alloc new ControlBlock, fill, store

        // 3 fields 의 src ptrs
        let src_line = (*src).line_default;
        let src_textbox = (*src).textbox_default;
        let src_shape = (*src).shape_default;

        // line_default
        let new_line = clone_share_ptr_via_vfunc(src_line);
        // textbox_default
        let new_textbox = clone_share_ptr_via_vfunc(src_textbox);
        // shape_default
        let new_shape = clone_share_ptr_via_vfunc(src_shape);

        ptr::write(
            this,
            ObjectDefaults {
                line_default: new_line,
                textbox_default: new_textbox,
                shape_default: new_shape,
            },
        );
    }

    /// heap-alloc 24B + copy ctor.
    ///
    /// # Safety
    /// 반환 ptr 은 `raw_delete` 로 해제.
    pub unsafe fn clone_to_heap(&self) -> *mut ObjectDefaults {
        let layout = std::alloc::Layout::new::<ObjectDefaults>();
        let new_p = std::alloc::alloc(layout) as *mut ObjectDefaults;
        if new_p.is_null() {
            std::alloc::handle_alloc_error(layout);
        }
        Self::copy_from_raw(new_p, self as *const ObjectDefaults);
        new_p
    }

    /// heap-alloc ObjectDefaults* 해제.
    ///
    /// # Safety
    /// `p` 는 `clone_to_heap` 으로 얻은 ptr 또는 null.
    pub unsafe fn raw_delete(p: *mut ObjectDefaults) {
        if p.is_null() {
            return;
        }
        ptr::drop_in_place(p);
        std::alloc::dealloc(p as *mut u8, std::alloc::Layout::new::<ObjectDefaults>());
    }
}

/// raw 의 SharePtr Clone-via-Vfunc 패턴 (raw `1ae51c-1ae550` 등) 1:1.
///
/// 본 함수는 **null path 전용** — non-null path 는 DefaultProperty vtable[+0x28]
/// virtual Clone 호출 필요 (multi-session deferred).
unsafe fn clone_share_ptr_via_vfunc(
    src_cb: *mut ControlBlock<DefaultProperty>,
) -> *mut ControlBlock<DefaultProperty> {
    if src_cb.is_null() {
        // raw `1ae520: cbz x8, null_path` → this.share_ptr = null
        return ptr::null_mut();
    }
    let t_ptr = (*src_cb).obj;
    if t_ptr.is_null() {
        // raw `1ae528: cbz x0, null_path`
        return ptr::null_mut();
    }
    // raw `1ae52c-1ae534`: vtable[+0x28] (Clone vfunc) dispatch.
    //
    // **CURRENT SCOPE**: DefaultProperty 가 ZST placeholder — vtable 없음.
    // non-null path 도달 시 DefaultProperty 의 vtable layout + Clone signature
    // RE 필요.
    panic!(
        "ObjectDefaults::copy_from_raw: non-null SharePtr<DefaultProperty> requires vtable[+0x28] Clone vfunc RE — deferred"
    );
}

impl Default for ObjectDefaults {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for ObjectDefaults {
    /// raw `~ObjectDefaults()` (`0x1ae6f4`).
    ///
    /// 3 SharePtr drop in offset 0x10 → 0x08 → 0x00 order.
    fn drop(&mut self) {
        unsafe {
            // raw 0x1ae704: drop shape_default (offset 0x10)
            Self::drop_share_ptr_inplace(self.shape_default);
            self.shape_default = ptr::null_mut();
            // raw 0x1ae740: drop textbox_default (offset 0x08)
            Self::drop_share_ptr_inplace(self.textbox_default);
            self.textbox_default = ptr::null_mut();
            // raw 0x1ae77c: drop line_default (offset 0x00)
            Self::drop_share_ptr_inplace(self.line_default);
            self.line_default = ptr::null_mut();
        }
    }
}

impl ObjectDefaults {
    /// raw SharePtr drop pattern: refcount-- + free at 0 (virtual dtor).
    ///
    /// Empty placeholder (DefaultProperty = ZST) 상태에선 호출 안 됨. 향후
    /// DefaultProperty RE 완료 시 virtual dtor 호출 path 추가 필요.
    unsafe fn drop_share_ptr_inplace(cb: *mut ControlBlock<DefaultProperty>) {
        if cb.is_null() {
            return;
        }
        let obj = (*cb).obj;
        if obj.is_null() {
            return;
        }
        let new_rc = (*cb).refcount.wrapping_sub(1);
        if new_rc != 0 {
            (*cb).refcount = new_rc;
            return;
        }
        // refcount == 0: raw 의 virtual dtor call (vtable[+8])
        //
        // 본 단계는 DefaultProperty placeholder (ZST) — 실제 virtual call 시점에
        // 도달 시 별도 RE 필요.
        //
        // Rust SharePtr<T> 의 Box::from_raw 등가 path:
        drop(Box::from_raw(obj));
        std::alloc::dealloc(
            cb as *mut u8,
            std::alloc::Layout::new::<ControlBlock<DefaultProperty>>(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_layout_size_align() {
        assert_eq!(std::mem::size_of::<ObjectDefaults>(), 24);
        assert_eq!(std::mem::align_of::<ObjectDefaults>(), 8);
    }

    #[test]
    fn field_offsets_match_raw() {
        let od = ObjectDefaults::new();
        let p = &od as *const ObjectDefaults as usize;
        assert_eq!(&od.line_default as *const _ as usize - p, 0x00);
        assert_eq!(&od.textbox_default as *const _ as usize - p, 0x08);
        assert_eq!(&od.shape_default as *const _ as usize - p, 0x10);
    }

    #[test]
    fn default_ctor_all_null() {
        let od = ObjectDefaults::new();
        assert!(od.line_default.is_null());
        assert!(od.textbox_default.is_null());
        assert!(od.shape_default.is_null());
        assert!(od.is_empty());
    }

    #[test]
    fn get_line_default_on_empty_returns_null_share_ptr() {
        let od = ObjectDefaults::new();
        let sp = od.get_line_default();
        assert!(sp.is_null());
        // Also for the others:
        assert!(od.get_textbox_default().is_null());
        assert!(od.get_shape_default().is_null());
    }

    #[test]
    fn get_line_default_clones_increment_refcount() {
        // SharePtr<DefaultProperty> 를 외부에서 만들고 ObjectDefaults 의 field 에 직접 주입
        let mut od = ObjectDefaults::new();
        let sp = SharePtr::<DefaultProperty>::from_object(DefaultProperty { _opaque: [] });
        // Inject raw — refcount = 1
        od.line_default = sp.as_raw();
        let initial_rc = sp.refcount();
        assert_eq!(initial_rc, 1);

        // Get clones via ObjectDefaults
        let sp2 = od.get_line_default();
        assert_eq!(sp2.as_raw(), sp.as_raw());
        // refcount 2 (= 1 original + 1 cloned via get)
        assert_eq!(sp.refcount(), 2);

        // drop sp2 → refcount 1
        drop(sp2);
        assert_eq!(sp.refcount(), 1);

        // sp is in od.line_default and also held by `sp` itself. drop both:
        // First clear od.line_default to prevent double drop.
        od.line_default = ptr::null_mut();
        drop(sp);
        drop(od);
    }

    #[test]
    fn drop_empty_object_defaults_no_panic() {
        for _ in 0..50 {
            let od = ObjectDefaults::new();
            drop(od);
        }
    }

    #[test]
    fn drop_with_share_ptr_decrements_refcount() {
        #[allow(unused_unsafe)]
        unsafe {
            let sp = SharePtr::<DefaultProperty>::from_object(DefaultProperty { _opaque: [] });
            // Bump refcount externally so the drop doesn't reach 0
            let sp_clone = sp.clone();
            assert_eq!(sp.refcount(), 2);

            let mut od = ObjectDefaults::new();
            od.line_default = sp.as_raw();
            // Now refcount semantically belongs to: sp, sp_clone, od.line_default
            // (raw refcount value still 2 since we didn't bump for od)
            // For the test, we manually bump to model the ownership transfer:
            (*sp.as_raw()).refcount += 1; // model the original owner refcount
            assert_eq!(sp.refcount(), 3);

            // Drop od → decrements refcount from 3 to 2
            drop(od);
            assert_eq!(sp.refcount(), 2);

            // drop sp → 1, drop sp_clone → 0 (free)
            drop(sp);
            drop(sp_clone);
        }
    }

    #[test]
    fn default_property_is_zero_sized_placeholder() {
        assert_eq!(std::mem::size_of::<DefaultProperty>(), 0);
    }
}

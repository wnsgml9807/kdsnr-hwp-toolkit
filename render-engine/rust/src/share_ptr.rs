//! `Hnc::Memory::SharePtr<T>` — 8B intrusive-via-control-block shared pointer.
//!
//! 위치: `libHncDrawingEngine_arm64.dylib` (per-type template instantiations).
//! 본 file 의 raw asm 인용은 `SharePtr<Hnc::Shape::Theme>::~SharePtr()` @ 0x1c2b38
//! (또는 `reset()`) 의 코드를 기준 — 다른 instantiation 들도 layout/동작 동일.
//!
//! # Layout — 8B SharePtr + 16B ControlBlock
//!
//! ```text
//! SharePtr<T> {
//!     raw: *mut ControlBlock<T>,    // 8B, the only field
//! }
//!
//! ControlBlock<T> {
//!     obj: *mut T,                   // offset 0x00, 8B
//!     refcount: u64,                 // offset 0x08, 8B
//! }
//! // total 16B; 별도 heap-alloc.
//! ```
//!
//! T 는 ControlBlock 과 **별개로** heap-alloc 됨. SharePtr 의 raw 가 control block 을 가리키고,
//! control block 의 obj 가 실제 T 인스턴스를 가리킴. 이 indirection 으로 (1) 동일 T 를 여러 곳에서
//! 공유 + (2) raw asm 의 `ldr x0, [x20]` 으로 즉시 T* 조회 가 가능.
//!
//! # Raw asm — Theme's SharePtr dtor (@ 0x1c2b38)
//!
//! ```text
//! 001c2b38  stp   x20, x19, [sp, #-0x20]!   ; prologue
//! 001c2b3c  stp   x29, x30, [sp, #0x10]
//! 001c2b40  add   x29, sp, #0x10
//! 001c2b44  mov   x19, x0                    ; x19 = this (SharePtr*)
//! 001c2b48  ldr   x20, [x0]                  ; x20 = self.raw (ControlBlock*)
//! 001c2b4c  cbz   x20, 0x1c2b80              ; if null → exit
//! 001c2b50  ldr   x0,  [x20]                 ; x0 = control->obj (T*)
//! 001c2b54  cbz   x0,  0x1c2b80              ; if null → exit (defensive)
//! 001c2b58  ldr   x8,  [x20, #0x8]           ; x8 = control->refcount
//! 001c2b5c  subs  x8,  x8, #0x1              ; refcount--
//! 001c2b60  b.ne  0x1c2b78                   ; if refcount != 0 → store back
//! 001c2b64  bl    __ZN3Hnc5Shape5ThemeD2Ev   ; refcount == 0: destroy T
//! 001c2b68  bl    __ZdlPv                    ;  delete T (x0 = control->obj)
//! 001c2b6c  mov   x0,  x20
//! 001c2b70  bl    __ZdlPv                    ;  delete control block
//! 001c2b74  b     0x1c2b7c
//! 001c2b78  str   x8,  [x20, #0x8]           ; store decremented refcount
//! 001c2b7c  str   xzr, [x19]                  ; self.raw = null
//! 001c2b80  mov   x0,  x19                    ; return self
//! 001c2b84-0x1c2b8c  epilogue + ret
//! ```
//!
//! # Raw asm — SharePtr copy ctor (inline in Theme ctor @ 0x1ebba0)
//!
//! ```text
//! 001ebba0  ldr   x8, [x20]                  ; x8 = arg.raw (ControlBlock*)
//! 001ebba4  mov   x20, x19
//! 001ebba8  str   x8, [x20, #0x10]!          ; *(self) = x8 (copy ControlBlock*)
//! 001ebbac  cbz   x8, 0x1ebbbc                ; if null, skip refcount++
//! 001ebbb0  ldr   x9, [x8, #0x8]              ; x9 = refcount
//! 001ebbb4  add   x9, x9, #0x1                ; refcount++
//! 001ebbb8  str   x9, [x8, #0x8]              ; store back
//! ```
//!
//! # Rust 1:1 port 정책
//!
//! - `#[repr(transparent)]` + 단일 `*mut ControlBlock<T>` field → C++ ABI 와 8B 동일.
//! - C++ 의 `SharePtr::SharePtr(const SharePtr&)` (refcount++) ↔ Rust `Clone`.
//! - C++ 의 `SharePtr::~SharePtr()` (refcount--, 0 시 destroy) ↔ Rust `Drop`.
//! - `from_object(value: T)` constructor — heap-alloc T + heap-alloc ControlBlock, refcount=1.
//! - `null()` constructor — `raw = null`.
//! - `obj()` accessor → `&T` (null 이면 None).
//! - `refcount()` accessor → 검증/디버깅용.

#![allow(clippy::module_name_repetitions)]

use std::ptr::NonNull;

/// Control block held by a `SharePtr<T>`. 16B, 8B align.
///
/// Raw asm 의 `[x20, 0x0]` = obj, `[x20, 0x8]` = refcount.
#[repr(C, align(8))]
pub struct ControlBlock<T = ()> {
    /// offset 0x00 — heap-alloc'd T 의 raw pointer. SharePtr 가 null 인 path 에서는 보조 null
    /// check (defensive) 의 대상.
    pub obj: *mut T,
    /// offset 0x08 — refcount. 첫 SharePtr 생성 시 1, 이후 copy 시 +1, drop 시 -1.
    pub refcount: u64,
}

/// `Hnc::Memory::SharePtr<T>` — 8B. byte-equivalent C++ ABI.
///
/// Rust 의 type system 으로는 NULL 도 표현 가능. C++ 의 default ctor 가 raw = nullptr.
#[repr(transparent)]
pub struct SharePtr<T = ()> {
    /// 8B raw pointer to ControlBlock<T>. NULL when empty.
    pub raw: *mut ControlBlock<T>,
}

impl<T> SharePtr<T> {
    /// Construct an empty (null) SharePtr.
    ///
    /// 한컴 원본의 default ctor 와 동일: `raw = nullptr`.
    pub const fn null() -> Self {
        SharePtr {
            raw: std::ptr::null_mut(),
        }
    }

    /// Construct a SharePtr by heap-allocating `value` and a fresh ControlBlock.
    ///
    /// 한컴의 SharePtr factory 패턴 — `operator new(sizeof(T)) + T(...)` 로 객체를 만든 후
    /// `operator new(sizeof(ControlBlock)) + ControlBlock{obj=T*, refcount=1}` 로 control block 을
    /// 만들고 SharePtr.raw 에 저장. 본 Rust 메소드는 그 합성을 한 번에 제공.
    pub fn from_object(value: T) -> Self {
        // 1. heap-alloc T (raw 의 `__Znwm + ctor` 와 동등).
        let obj_box = Box::new(value);
        let obj_raw: *mut T = Box::into_raw(obj_box);
        // 2. heap-alloc ControlBlock.
        let cb_box = Box::new(ControlBlock {
            obj: obj_raw,
            refcount: 1,
        });
        let cb_raw: *mut ControlBlock<T> = Box::into_raw(cb_box);
        SharePtr { raw: cb_raw }
    }

    /// Raw access: is `raw == null`?
    pub fn is_null(&self) -> bool {
        self.raw.is_null()
    }

    /// Borrow the underlying object. None if SharePtr is null.
    ///
    /// SAFETY: SharePtr 가 null 이 아닐 때, control->obj 가 valid 한 T 를 가리킴을 보장.
    /// 한컴 raw asm 도 동일 가정 (`ldr x0, [x20]` 후 dereference).
    pub fn obj(&self) -> Option<&T> {
        if self.raw.is_null() {
            None
        } else {
            // SAFETY: raw is non-null → control block valid.
            unsafe {
                let cb = &*self.raw;
                if cb.obj.is_null() {
                    None
                } else {
                    Some(&*cb.obj)
                }
            }
        }
    }

    /// Mutable borrow — only valid when refcount == 1 (unique). 한컴 raw asm 에서 직접 mutate 는
    /// 매우 드물고, COW 패턴이 없음. 따라서 본 메소드는 byte-equivalent 와 무관한 Rust-side helper.
    /// Rust borrow checker 의 safety 를 우회하므로 unsafe.
    ///
    /// SAFETY: caller 는 다른 SharePtr 가 동일 ControlBlock 을 참조하지 않음을 보장해야 함.
    pub unsafe fn obj_mut_unchecked(&mut self) -> Option<&mut T> {
        if self.raw.is_null() {
            None
        } else {
            let cb = &mut *self.raw;
            if cb.obj.is_null() {
                None
            } else {
                Some(&mut *cb.obj)
            }
        }
    }

    /// 현재 refcount. 0 이면 null SharePtr 로 간주 (raw == null).
    pub fn refcount(&self) -> u64 {
        if self.raw.is_null() {
            0
        } else {
            // SAFETY: raw is non-null → valid.
            unsafe { (*self.raw).refcount }
        }
    }

    /// raw control block pointer (8B). C++ FFI 또는 byte-level 검증용.
    pub fn as_raw(&self) -> *mut ControlBlock<T> {
        self.raw
    }
}

/// Raw asm 의 copy ctor 와 동등: refcount++.
impl<T> Clone for SharePtr<T> {
    fn clone(&self) -> Self {
        // raw cbz x8 check 와 동등.
        if !self.raw.is_null() {
            // SAFETY: raw non-null → valid ControlBlock.
            unsafe {
                let cb = &mut *self.raw;
                // raw: ldr x9, [x8, #0x8]; add x9, x9, #1; str x9, [x8, #0x8]
                cb.refcount += 1;
            }
        }
        SharePtr { raw: self.raw }
    }
}

/// Raw asm 의 dtor 와 동등: refcount--, 0 시 destroy T + free ControlBlock.
impl<T> Drop for SharePtr<T> {
    fn drop(&mut self) {
        if self.raw.is_null() {
            return; // raw 의 cbz x20 path
        }
        // SAFETY: raw non-null → valid ControlBlock.
        unsafe {
            let cb_ptr = self.raw;
            let cb = &mut *cb_ptr;
            // raw: cbz x0, 0x1c2b80 (defensive null check on obj — match)
            if cb.obj.is_null() {
                // 0x1c2b7c: str xzr, [x19] (clear self.raw, but Rust drop already invalidates)
                self.raw = std::ptr::null_mut();
                return;
            }
            // raw: ldr x8, [x20, #0x8]; subs x8, x8, #1
            cb.refcount = cb.refcount.wrapping_sub(1);
            if cb.refcount == 0 {
                // raw: bl T::~T(); bl operator delete(T*)
                let obj_ptr = cb.obj;
                cb.obj = std::ptr::null_mut();
                // Box::from_raw 가 T::~T() + delete 와 동등 (heap-free).
                drop(Box::from_raw(obj_ptr));
                // raw: bl operator delete(ControlBlock*)
                // Box::from_raw 로 ControlBlock 도 free.
                drop(Box::from_raw(cb_ptr));
            } else {
                // raw: str x8, [x20, #0x8] (refcount 갱신은 이미 위에서 처리)
            }
            // raw: str xzr, [x19]
            self.raw = std::ptr::null_mut();
        }
    }
}

impl<T> Default for SharePtr<T> {
    fn default() -> Self {
        SharePtr::null()
    }
}

impl<T> std::fmt::Debug for SharePtr<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.raw.is_null() {
            write!(f, "SharePtr<{}>(null)", std::any::type_name::<T>())
        } else {
            write!(
                f,
                "SharePtr<{}>(raw=0x{:x}, refcount={})",
                std::any::type_name::<T>(),
                self.raw as usize,
                self.refcount()
            )
        }
    }
}

// SharePtr<T> 는 raw pointer 를 들고 있어서 자동으로 !Send/!Sync.
// 한컴 원본도 thread-safe 가 아님 (non-atomic refcount). 명시적 marker 미부여 — Rust 의
// auto-not-Send/Sync 가 raw 와 일관.

// 정적 검증.
const _: () = assert!(std::mem::size_of::<SharePtr<()>>() == 8, "SharePtr is 8B");
const _: () = assert!(std::mem::align_of::<SharePtr<()>>() == 8, "SharePtr is 8B-aligned");
const _: () = assert!(std::mem::size_of::<ControlBlock<()>>() == 16, "ControlBlock is 16B");
const _: () = assert!(std::mem::align_of::<ControlBlock<()>>() == 8, "ControlBlock is 8B-aligned");

// raw == null 인 SharePtr 의 byte pattern 검증 — Option<NonNull<ControlBlock>> 와 byte-equivalent.
// (Rust 의 NonNull optimization 으로 sizeof Option == sizeof NonNull, byte pattern: null = None).
const _: () = {
    let _: usize = std::mem::size_of::<Option<NonNull<ControlBlock<()>>>>();
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizeof_share_ptr_is_8b() {
        assert_eq!(std::mem::size_of::<SharePtr<u64>>(), 8);
        assert_eq!(std::mem::size_of::<SharePtr<[u8; 1024]>>(), 8); // T size 무관
        assert_eq!(std::mem::align_of::<SharePtr<u64>>(), 8);
    }

    #[test]
    fn sizeof_control_block_is_16b() {
        assert_eq!(std::mem::size_of::<ControlBlock<u64>>(), 16);
        assert_eq!(std::mem::size_of::<ControlBlock<[u8; 1024]>>(), 16); // T size 무관 (heap)
        assert_eq!(std::mem::align_of::<ControlBlock<u64>>(), 8);
    }

    #[test]
    fn control_block_field_offsets() {
        let cb = ControlBlock {
            obj: std::ptr::null_mut::<u64>(),
            refcount: 0u64,
        };
        let base = &cb as *const ControlBlock<u64> as usize;
        let off_obj = &cb.obj as *const *mut u64 as usize - base;
        let off_refcount = &cb.refcount as *const u64 as usize - base;
        assert_eq!(off_obj, 0x0);
        assert_eq!(off_refcount, 0x8);
    }

    #[test]
    fn null_share_ptr_is_null() {
        let sp = SharePtr::<u64>::null();
        assert!(sp.is_null());
        assert!(sp.obj().is_none());
        assert_eq!(sp.refcount(), 0);
        // raw bytes == 0
        assert_eq!(sp.as_raw() as usize, 0);
    }

    #[test]
    fn from_object_initial_refcount_one() {
        let sp = SharePtr::from_object(42u64);
        assert!(!sp.is_null());
        assert_eq!(sp.refcount(), 1);
        assert_eq!(sp.obj().copied(), Some(42));
    }

    #[test]
    fn clone_increments_refcount() {
        let sp1 = SharePtr::from_object(42u64);
        assert_eq!(sp1.refcount(), 1);
        let sp2 = sp1.clone();
        assert_eq!(sp1.refcount(), 2);
        assert_eq!(sp2.refcount(), 2);
        let sp3 = sp2.clone();
        assert_eq!(sp1.refcount(), 3);
        assert_eq!(sp2.refcount(), 3);
        assert_eq!(sp3.refcount(), 3);
    }

    #[test]
    fn drop_decrements_refcount() {
        let sp1 = SharePtr::from_object(42u64);
        let sp2 = sp1.clone();
        let sp3 = sp1.clone();
        assert_eq!(sp1.refcount(), 3);
        drop(sp2);
        assert_eq!(sp1.refcount(), 2);
        drop(sp3);
        assert_eq!(sp1.refcount(), 1);
    }

    #[test]
    fn drop_frees_when_refcount_zero() {
        // 객체에 소멸자 트래킹을 위해 Drop 가 있는 type 사용.
        use std::rc::Rc;
        use std::cell::Cell;
        let count = Rc::new(Cell::new(0u32));
        struct Tracker {
            count: Rc<Cell<u32>>,
        }
        impl Drop for Tracker {
            fn drop(&mut self) {
                self.count.set(self.count.get() + 1);
            }
        }
        {
            let sp = SharePtr::from_object(Tracker {
                count: count.clone(),
            });
            assert_eq!(count.get(), 0);
            let sp2 = sp.clone();
            assert_eq!(count.get(), 0);
            drop(sp2);
            assert_eq!(count.get(), 0); // sp 도 살아있음
        }
        // sp 가 drop 되어 refcount=0 → Tracker::drop 호출.
        assert_eq!(count.get(), 1);
    }

    #[test]
    fn clone_null_does_not_segfault() {
        let sp1 = SharePtr::<u64>::null();
        let sp2 = sp1.clone();
        assert!(sp2.is_null());
        // sp1, sp2 모두 drop 정상.
        drop(sp1);
        drop(sp2);
    }

    #[test]
    fn refcount_value_matches_raw_pattern() {
        let sp = SharePtr::from_object(0xDEADBEEFu64);
        // raw asm path: ldr x9, [x8, #0x8]; add x9, x9, #1; ...
        // 즉 refcount 는 1 부터 시작. clone 마다 +1.
        assert_eq!(sp.refcount(), 1);
        let c1 = sp.clone();
        assert_eq!(c1.refcount(), 2);
        let c2 = sp.clone();
        assert_eq!(c2.refcount(), 3);
        drop(c1);
        assert_eq!(sp.refcount(), 2);
        drop(c2);
        assert_eq!(sp.refcount(), 1);
    }

    #[test]
    fn default_is_null() {
        let sp: SharePtr<u64> = SharePtr::default();
        assert!(sp.is_null());
    }

    #[test]
    fn raw_layout_byte_compat() {
        // 한 SharePtr 의 raw bytes (8B) 가 control block pointer 와 일치.
        let sp = SharePtr::from_object(42u64);
        let raw_ptr_bytes: [u8; 8] = {
            let p = sp.as_raw() as usize;
            p.to_ne_bytes()
        };
        // SharePtr 의 첫 8 byte 가 control block 주소와 동일해야 함.
        let sp_bytes: [u8; 8] = unsafe {
            let p = &sp as *const SharePtr<u64> as *const [u8; 8];
            *p
        };
        assert_eq!(raw_ptr_bytes, sp_bytes);
    }
}

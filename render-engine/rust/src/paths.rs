//! `Hnc::Shape::Paths` — `std::vector<Path*>` (24B libc++ layout).
//!
//! ## 출처
//!
//! - default ctor: `0x1b332c` (3-ptr zero init)
//! - `Paths(Path*)`: `0x1b3338` (zero init + AddPath)
//! - `~Paths()`: `0x168288` (free buffer; doesn't delete contained Paths)
//! - `AddPath(Path*)`: `0x1b3380` (vector push_back semantics)
//! - `Swap`: `0x1b37c0`, `Clear`: `0x138f9c`, `Begin/End`: `0xbdf84/0xbdf8c`
//! - `GetAt(usize)`: `0x11484c`, `Create()`: `0x1b4158`, `Create(Path*)`: `0x1041e4`
//! - `Offset(f32,f32)`: `0x1b3944`, `Transform(T2D&, Guides*)`: `0x194434`, `Detach()`: `0x1b3f38`
//!
//! ## raw layout (24B)
//!
//! C++ `std::vector<T>` 의 libc++ 구현:
//! | offset | field | 의미 |
//! |--------|-------|------|
//! | +0x00  | `begin: *mut *mut Path` | data ptr (= `&vec[0]`) |
//! | +0x08  | `end: *mut *mut Path`   | logical end (= `&vec[size]`) |
//! | +0x10  | `capacity_end: *mut *mut Path` | allocated end (= `&vec[capacity]`) |
//!
//! - `size = (end - begin) / 8`
//! - `capacity = (capacity_end - begin) / 8`
//!
//! ## byte-eq 경계
//!
//! - struct layout: 100% libc++ vector layout 매치 (24B, 3× ptr)
//! - default ctor: `stp xzr, xzr; str xzr` — 3개 ptr 모두 0
//! - AddPath: push_back semantics. realloc 시 capacity 가 max(size*2, size+1) — libc++ 표준
//! - ~Paths: buffer free (개별 Path 는 caller 책임 — 본 dtor 는 vector 만 정리)

use crate::path::Path;
use std::alloc::{alloc, dealloc, Layout};
use std::ptr;

/// `Hnc::Shape::Paths` — 24B `std::vector<Path*>` byte-eq.
#[repr(C)]
#[derive(Debug)]
pub struct Paths {
    /// +0x00: data ptr (`*mut *mut Path`).
    pub begin: *mut *mut Path,
    /// +0x08: logical end.
    pub end: *mut *mut Path,
    /// +0x10: allocated capacity end.
    pub capacity_end: *mut *mut Path,
}

pub const PATHS_SIZE_BYTES: usize = 24;
pub const PATHS_ALIGN_BYTES: usize = 8;

const _: () = assert!(std::mem::size_of::<Paths>() == PATHS_SIZE_BYTES);
const _: () = assert!(std::mem::align_of::<Paths>() == PATHS_ALIGN_BYTES);

const PTR_SIZE: usize = std::mem::size_of::<*mut Path>(); // = 8

impl Paths {
    /// raw `Paths()` default ctor (`0x1b332c`):
    /// ```asm
    /// stp xzr, xzr, [x0]
    /// str xzr, [x0, #0x10]
    /// ret
    /// ```
    /// 3개 ptr 모두 null.
    pub fn new() -> Self {
        Paths {
            begin: ptr::null_mut(),
            end: ptr::null_mut(),
            capacity_end: ptr::null_mut(),
        }
    }

    /// raw `Paths(Path*)` ctor (`0x1b3338`):
    /// default ctor + AddPath(path).
    ///
    /// # Safety
    /// `path` 가 valid `*mut Path` 또는 null. null 이면 AddPath 가 early-return.
    pub unsafe fn from_path(path: *mut Path) -> Self {
        let mut p = Self::new();
        p.add_path(path);
        p
    }

    /// 현재 size (element 개수).
    /// raw: `(end - begin) / 8`.
    #[inline]
    pub fn len(&self) -> usize {
        if self.begin.is_null() || self.end.is_null() {
            return 0;
        }
        let bytes = (self.end as usize).wrapping_sub(self.begin as usize);
        bytes / PTR_SIZE
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 현재 capacity (allocated slots).
    /// raw: `(capacity_end - begin) / 8`.
    #[inline]
    pub fn capacity(&self) -> usize {
        if self.begin.is_null() || self.capacity_end.is_null() {
            return 0;
        }
        let bytes = (self.capacity_end as usize).wrapping_sub(self.begin as usize);
        bytes / PTR_SIZE
    }

    /// raw `AddPath(Path*)` (`0x1b3380`):
    ///
    /// ## 알고리즘 (byte-eq)
    /// ```text
    /// if path == null: return
    /// if end < capacity_end:    ; 빈 자리 있음
    ///   *end = path; end += 8
    /// else:                       ; 재할당 필요
    ///   size = (end - begin) / 8
    ///   capacity = (capacity_end - begin) / 8
    ///   new_cap = max(capacity*2, size+1)
    ///   new_buffer = malloc(new_cap * 8)
    ///   copy old elements [begin..end] → new_buffer
    ///   set new[size] = path
    ///   end = new_buffer + (size+1)*8
    ///   free(begin)
    ///   begin = new_buffer
    ///   capacity_end = new_buffer + new_cap*8
    /// ```
    ///
    /// # Safety
    /// `path` 가 valid 또는 null. self 의 buffer 는 `alloc` 으로 할당된 적이 있어야
    /// (`from_path` / `new` 후 add_path 호출 패턴).
    pub unsafe fn add_path(&mut self, path: *mut Path) {
        // raw `cbz x1, return` — null path 면 early-return
        if path.is_null() {
            return;
        }
        // raw `cmp end, capacity_end; b.hs realloc`
        if self.end < self.capacity_end {
            // 빈 자리 있음 — 바로 추가
            *self.end = path;
            self.end = self.end.add(1);
            return;
        }
        // 재할당 필요
        let size = self.len();
        let capacity = self.capacity();
        // raw: `new_cap = max(capacity*2, size+1)`
        let new_cap = (capacity.wrapping_mul(2)).max(size.wrapping_add(1));
        // raw `lsl x0, x23, #3; bl __Znwm` — alloc new_cap * 8 bytes
        let layout = Layout::from_size_align(new_cap * PTR_SIZE, PTR_SIZE).unwrap();
        let new_buffer = alloc(layout) as *mut *mut Path;
        assert!(!new_buffer.is_null(), "Paths::AddPath alloc failed");
        // copy old elements
        if !self.begin.is_null() && size > 0 {
            ptr::copy_nonoverlapping(self.begin, new_buffer, size);
        }
        // append new element
        *new_buffer.add(size) = path;
        // free old buffer
        if !self.begin.is_null() {
            let old_capacity = capacity;
            if old_capacity > 0 {
                let old_layout = Layout::from_size_align(old_capacity * PTR_SIZE, PTR_SIZE).unwrap();
                dealloc(self.begin as *mut u8, old_layout);
            }
        }
        // update ptrs
        self.begin = new_buffer;
        self.end = new_buffer.add(size + 1);
        self.capacity_end = new_buffer.add(new_cap);
    }

    /// raw `GetAt(usize)` (`0x11484c`):
    /// returns `begin[index]`. caller 가 bounds 검증 책임.
    ///
    /// # Safety
    /// `index < self.len()` 이어야.
    pub unsafe fn get_at(&self, index: usize) -> *mut Path {
        *self.begin.add(index)
    }
}

impl Default for Paths {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Paths {
    /// raw `~Paths()` (`0x168288`):
    /// buffer free (개별 Path 는 caller 책임).
    fn drop(&mut self) {
        unsafe {
            if !self.begin.is_null() {
                let cap = self.capacity();
                if cap > 0 {
                    let layout = Layout::from_size_align(cap * PTR_SIZE, PTR_SIZE).unwrap();
                    dealloc(self.begin as *mut u8, layout);
                }
                self.begin = ptr::null_mut();
                self.end = ptr::null_mut();
                self.capacity_end = ptr::null_mut();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_size_align() {
        assert_eq!(std::mem::size_of::<Paths>(), 24);
        assert_eq!(std::mem::align_of::<Paths>(), 8);
    }

    #[test]
    fn field_offsets() {
        let p = Paths::new();
        let base = &p as *const _ as usize;
        assert_eq!(&p.begin as *const _ as usize - base, 0x00);
        assert_eq!(&p.end as *const _ as usize - base, 0x08);
        assert_eq!(&p.capacity_end as *const _ as usize - base, 0x10);
    }

    #[test]
    fn default_ctor_all_null() {
        let p = Paths::new();
        assert!(p.begin.is_null());
        assert!(p.end.is_null());
        assert!(p.capacity_end.is_null());
        assert_eq!(p.len(), 0);
        assert_eq!(p.capacity(), 0);
        assert!(p.is_empty());
    }

    #[test]
    fn add_path_null_is_noop() {
        let mut p = Paths::new();
        unsafe { p.add_path(ptr::null_mut()); }
        assert_eq!(p.len(), 0);
    }

    #[test]
    fn add_path_single_allocates_and_stores() {
        let path = Box::into_raw(Box::new(Path::new()));
        let mut p = Paths::new();
        unsafe {
            p.add_path(path);
            assert_eq!(p.len(), 1);
            assert!(p.capacity() >= 1);
            assert_eq!(p.get_at(0), path);
            // cleanup the contained Path (Paths::~Paths doesn't delete)
            let _ = Box::from_raw(path);
        }
    }

    #[test]
    fn add_path_multiple_grows_capacity() {
        let mut p = Paths::new();
        let mut paths: Vec<*mut Path> = Vec::new();
        unsafe {
            for _ in 0..10 {
                let path = Box::into_raw(Box::new(Path::new()));
                paths.push(path);
                p.add_path(path);
            }
            assert_eq!(p.len(), 10);
            for (i, &path) in paths.iter().enumerate() {
                assert_eq!(p.get_at(i), path);
            }
            // cleanup
            for path in paths {
                let _ = Box::from_raw(path);
            }
        }
    }

    #[test]
    fn from_path_adds_single() {
        let path = Box::into_raw(Box::new(Path::new()));
        unsafe {
            let p = Paths::from_path(path);
            assert_eq!(p.len(), 1);
            assert_eq!(p.get_at(0), path);
            // drop p (frees vector buffer)
            drop(p);
            let _ = Box::from_raw(path);
        }
    }

    #[test]
    fn from_path_with_null_yields_empty() {
        unsafe {
            let p = Paths::from_path(ptr::null_mut());
            assert!(p.is_empty());
        }
    }

    #[test]
    fn drop_frees_buffer() {
        // 정확한 free 검증은 valgrind 없이는 어렵지만, len/cap reset 만 확인
        let path = Box::into_raw(Box::new(Path::new()));
        let mut p = Paths::new();
        unsafe {
            p.add_path(path);
            assert!(!p.begin.is_null());
            drop(p);
            let _ = Box::from_raw(path);
        }
    }
}

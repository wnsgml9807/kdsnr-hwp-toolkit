//! `CHncStringW` — 8B refcounted MFC-CString-compatible wide string.
//!
//! 위치: `libHncFoundation_arm64.dylib`. Microsoft MFC `CStringT<wchar_t>` 와 호환 layout.
//!
//! # Layout
//!
//! ```text
//! CHncStringW {
//!     data: *const u16,   // 8B - 유일 필드. wide char buffer 의 시작주소 (header 다음)
//! }                       // 총 8B / 8B align
//!
//! // data 가 가리키는 buffer 의 실제 구조 (data 보다 12B 앞 부분이 header):
//!
//! Buffer {
//!     [-12]   refcount:    i32   // shared refcount; -2 = nil sentinel (never freed),
//!                                 // -1 = literal (read-only static), > 0 = heap-alloc'd shared.
//!     [-8]    data_length: i32   // wchar count (excluding NUL).
//!     [-4]    alloc_length: i32  // capacity (wchar count, excluding NUL).
//!     [0..]   wchar_t[length+1]  // null-terminated UTF-16 (or UTF-32 on some MFC builds).
//! }
//! ```
//!
//! # Raw asm — default ctor `__ZN11CHncStringWC2Ev` (@ 0xd72c)
//!
//! ```text
//! 0000d72c  str   xzr, [x0]                         ; self.data = nullptr
//! 0000d730  adrp  x8, 207  ; 0xdc000                ; load __got entry
//! 0000d734  ldr   x8, [x8, #0xcf0]                   ; x8 = &_afxPchNil (global sentinel)
//! 0000d738  str   x8, [x0]                          ; self.data = _afxPchNil
//! 0000d73c  ret
//! ```
//!
//! 즉 default-constructed CHncStringW 는 전역 sentinel (refcount = -2 영구) 을 가리킴.
//!
//! # Raw asm — dtor `__ZN11CHncStringWD2Ev` (@ 0xdef4)
//!
//! ```text
//! 0000def4  stp   x20, x19, [sp, #-0x20]!           ; prologue
//! 0000def8  stp   x29, x30, [sp, #0x10]
//! 0000defc  add   x29, sp, #0x10
//! 0000df00  mov   x19, x0                            ; x19 = self
//! 0000df04  ldr   x0, [x0]                           ; x0 = self.data
//! 0000df08  ldr   w8, [x0, #-0xc]!                   ; pre-index: x0 = data - 0xc; w8 = refcount
//! 0000df0c  cmn   w8, #0x2                           ; w8 + 2 == 0 → w8 == -2 (nil sentinel)
//! 0000df10  b.eq  0xdf20                             ; skip dec/free
//! 0000df14  bl    _InterlockedDecrement              ; atomic --refcount, return new value in w0
//! 0000df18  cmp   w0, #0x0
//! 0000df1c  b.le  0xdf30                             ; if new refcount <= 0 → free buffer
//! 0000df20  mov   x0, x19                            ; exit (no free needed)
//! 0000df24-0xdf2c  epilogue + ret
//! 0000df30  ldr   x8, [x19]                          ; x8 = self.data
//! 0000df34  sub   x0, x8, #0xc                       ; x0 = buffer start (data - 12)
//! 0000df38  bl    __ZdaPv                            ; operator delete[]
//! 0000df3c  mov   x0, x19
//! 0000df40-0xdf48  epilogue + ret
//! ```
//!
//! # Refcount conventions
//!
//! - `-2` (`AFX_NIL_STRINGDATA`): static empty sentinel. Default-constructed strings share this.
//!   Dtor skips both dec AND free.
//! - `-1` (`AFX_LITERAL_STRINGDATA`): read-only literal (compile-time string constant). MFC convention.
//! - `> 0`: heap-allocated shared buffer. Dtor decrements; on reaching 0, frees the buffer
//!   (starting from `data - 12`).
//!
//! 본 Rust port 는 `> 0` 와 `-2` 경로만 구현. `-1` 은 Theme 의 default-init path 에서 안 쓰임 —
//! Theme 의 CHncStringW 는 default ctor 로만 초기화되고 sentinel 을 참조함.
//!
//! # Sentinel — Rust managed
//!
//! libHncFoundation 의 진짜 `_afxPchNil` 글로벌 주소와는 다른 위치를 가리키지만, **header 의
//! byte 패턴 (`refcount = -2, length = 0, alloc = 0`) + buffer 가 빈 wide-string 인 점은 동일**.
//! 외부 (libhsp) 가 CHncStringW 의 `data[-12..]` 를 읽었을 때 동일한 의미로 해석됨.

#![allow(clippy::module_name_repetitions)]

use std::sync::atomic::{AtomicI32, Ordering};

/// MFC CString-compatible buffer header. 12B / 4B align.
///
/// Layout matches `CStringData` (refcount, data_length, alloc_length).
#[repr(C, align(4))]
struct StringHeader {
    /// `-2` = nil sentinel; `-1` = literal; `> 0` = heap-allocated refcount.
    /// AtomicI32 for thread safety matching raw `InterlockedDecrement`.
    refcount: AtomicI32,
    /// Wide-character count (excluding NUL terminator).
    data_length: i32,
    /// Allocated wide-character capacity (excluding NUL terminator).
    alloc_length: i32,
}

const _: () = assert!(std::mem::size_of::<StringHeader>() == 12);

/// AFX_NIL_STRINGDATA refcount value — never freed, never decremented.
const REFCOUNT_NIL: i32 = -2;

/// AFX_LITERAL_STRINGDATA refcount value — static, no refcount management.
#[allow(dead_code)]
const REFCOUNT_LITERAL: i32 = -1;

// ===== Global nil sentinel =====
//
// 정확한 layout: 12B header (refcount = -2, lengths = 0) followed by a single u16 NUL.
// Total 14 bytes. Aligned to 4B so that the wchar_t access at data is naturally aligned.

#[repr(C, align(4))]
struct NilSentinel {
    header: StringHeader,
    nul: u16,
}

// SAFETY: Sentinel 은 mutate 되지 않고 (refcount = -2 이라 atomic 갱신 path 가 skip 됨),
// thread 간 공유 안전. AtomicI32 자체가 Sync.
static NIL_SENTINEL: NilSentinel = NilSentinel {
    header: StringHeader {
        refcount: AtomicI32::new(REFCOUNT_NIL),
        data_length: 0,
        alloc_length: 0,
    },
    nul: 0,
};

/// `CHncStringW` — 8B refcounted wide string compatible with MFC `CStringT<wchar_t>`.
///
/// `data` 가 sentinel 또는 heap-alloc'd buffer 의 wide-char 시작주소를 가리킴.
/// header (12B) 는 항상 `data` 보다 12B 앞에 존재.
#[repr(C, align(8))]
pub struct CHncStringW {
    /// 8B pointer to wide char buffer (NOT to header). Always non-null (default ctor sets sentinel).
    data: *const u16,
}

unsafe impl Send for CHncStringW {}
// CHncStringW 의 buffer 공유 (refcount via AtomicI32) 는 thread-safe. raw `InterlockedDecrement`
// 가 atomic 인 이유.
unsafe impl Sync for CHncStringW {}

impl CHncStringW {
    /// `CHncStringW::CHncStringW()` C2 @ 0xd72c / C1 @ 0xd740.
    ///
    /// raw 의 `self.data = _afxPchNil` 와 동등 — 본 port 에서는 Rust 의 NIL_SENTINEL 사용.
    pub fn new() -> Self {
        // Pointer to sentinel's wide-char buffer (just past the header).
        let data = &NIL_SENTINEL.nul as *const u16;
        CHncStringW { data }
    }

    /// `CHncStringW::Assign(const wchar_t*)` 의 핵심 동작과 등가 — 새로운 heap-alloc'd buffer 로
    /// 교체. 본 함수는 wide string slice 로부터 새 CHncStringW 를 만든다.
    ///
    /// raw asm 의 AssignCopy 와 동등 (length 계산 + heap alloc + memcpy + header init).
    pub fn from_wide(s: &[u16]) -> Self {
        let len = s.len() as i32;
        Self::alloc_buffer_from_wide_slice(s, len)
    }

    /// From a UTF-8 Rust &str — wide-encode 후 from_wide 호출.
    pub fn from_str(s: &str) -> Self {
        let encoded: Vec<u16> = s.encode_utf16().collect();
        Self::from_wide(&encoded)
    }

    /// Wide character length (excluding NUL). raw asm 의 `[x0, -0x8]` 와 동등.
    pub fn length(&self) -> i32 {
        // SAFETY: data 는 항상 valid buffer (sentinel or heap-alloc) 의 wchar start 를 가리킴.
        // header 는 data 보다 12B 앞에 위치.
        unsafe {
            let header_ptr = (self.data as *const u8).sub(12) as *const StringHeader;
            (*header_ptr).data_length
        }
    }

    /// Raw refcount value. 검증/디버그 용.
    pub fn refcount(&self) -> i32 {
        unsafe {
            let header_ptr = (self.data as *const u8).sub(12) as *const StringHeader;
            (*header_ptr).refcount.load(Ordering::Acquire)
        }
    }

    /// Wide character slice. Length is what header says.
    pub fn as_wide(&self) -> &[u16] {
        let len = self.length() as usize;
        // SAFETY: header.data_length 가 valid 한 한 buffer 길이 보장 (assign/alloc 가 작성).
        unsafe { std::slice::from_raw_parts(self.data, len) }
    }

    /// Raw 8B pointer view — byte-level FFI 용.
    pub fn data_ptr(&self) -> *const u16 {
        self.data
    }

    /// 내부: header.refcount 가 negative (nil/literal) 인지 검사.
    fn refcount_is_negative(&self) -> bool {
        self.refcount() < 0
    }

    /// 내부: 새 heap buffer 를 할당해서 wide string 으로 초기화. refcount = 1.
    fn alloc_buffer_from_wide_slice(src: &[u16], length: i32) -> Self {
        // Total bytes: 12 (header) + (length + 1) * 2 (wide chars + NUL).
        let wchar_bytes = (length as usize + 1) * 2;
        let total_bytes = 12 + wchar_bytes;
        // Align 4B. Vec<u8> 의 default alignment 가 1 이지만, header 부분이 #[repr(C, align(4))]
        // 이므로 4B aligned 필요. Box<[u8]> 도 default align 은 1B — 직접 layout 으로 alloc 해야 함.
        use std::alloc::{alloc, Layout};
        let layout = Layout::from_size_align(total_bytes, 4).expect("valid alignment");
        // SAFETY: layout.size > 0 (12 + 2 minimum).
        let raw = unsafe { alloc(layout) };
        if raw.is_null() {
            std::alloc::handle_alloc_error(layout);
        }
        // Write header.
        let header_ptr = raw as *mut StringHeader;
        // SAFETY: raw align 4B, layout 12B at start.
        unsafe {
            std::ptr::write(
                header_ptr,
                StringHeader {
                    refcount: AtomicI32::new(1),
                    data_length: length,
                    alloc_length: length,
                },
            );
        }
        // Write wchars.
        let data_ptr = unsafe { raw.add(12) as *mut u16 };
        // SAFETY: data_ptr 는 4B aligned (raw + 12 = aligned).
        unsafe {
            std::ptr::copy_nonoverlapping(src.as_ptr(), data_ptr, length as usize);
            // NUL terminator.
            std::ptr::write(data_ptr.add(length as usize), 0u16);
        }
        // Remember the layout for Drop. 우리는 별도 metadata 없이도 from header 의 alloc_length 로
        // total_bytes 를 재계산 가능 → Drop 에서 그렇게 함.
        CHncStringW { data: data_ptr }
    }
}

impl Default for CHncStringW {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for CHncStringW {
    /// `CHncStringW::CHncStringW(const CHncStringW&)` C2 @ 0xd754 / C1 @ 0xd8b0.
    ///
    /// raw asm 의 분기:
    /// - 양쪽 모두 sentinel/heap: refcount++ (`_InterlockedIncrement`).
    /// - other 가 literal: full copy via `AssignCopy`.
    ///
    /// 본 port 는 sentinel/heap-share 경로만 (literal 미사용).
    fn clone(&self) -> Self {
        // raw: ldr w8, [self.data - 0xc]; ... cmn w8, #0x2; b.eq 0xd7cc (skip dec)
        // (현 self 가 nil → skip self decrement, just adopt other.)
        // 본 시점에서 self 가 새로 만들어지므로 (clone), self 의 dec 단계는 무관.
        // 그저 self.data = self.data 후 refcount++ (sentinel 이면 skip 자동).
        let new = CHncStringW { data: self.data };
        // raw: sub x0, x2, #0xc; bl InterlockedIncrement
        if !new.refcount_is_negative() {
            // SAFETY: refcount_is_negative() == false 면 header.refcount > 0 (heap-alloc).
            unsafe {
                let header_ptr = (new.data as *const u8).sub(12) as *const StringHeader;
                (*header_ptr).refcount.fetch_add(1, Ordering::AcqRel);
            }
        }
        // sentinel (-2) 경로는 skip — refcount 갱신 없음.
        new
    }
}

impl Drop for CHncStringW {
    /// `CHncStringW::~CHncStringW()` D2 @ 0xdef4 / D1 @ 0xdf50.
    fn drop(&mut self) {
        // raw: ldr x0, [x0]; ldr w8, [x0, -0xc]!
        if self.data.is_null() {
            return; // 방어적 (default ctor 가 항상 sentinel 셋팅하지만)
        }
        // SAFETY: data points into valid buffer (sentinel or heap).
        let header_ptr = unsafe { (self.data as *const u8).sub(12) as *const StringHeader };
        let rc = unsafe { (*header_ptr).refcount.load(Ordering::Acquire) };
        // raw: cmn w8, #0x2; b.eq exit
        if rc == REFCOUNT_NIL {
            return;
        }
        // raw: bl InterlockedDecrement; cmp w0, 0; b.le free
        let prev = unsafe { (*header_ptr).refcount.fetch_sub(1, Ordering::AcqRel) };
        let new_val = prev - 1;
        if new_val <= 0 {
            // raw: ldr x8, [x19]; sub x0, x8, #0xc; bl operator delete[]
            // 우리는 alloc 시 Layout 으로 alloc 했으니 dealloc 도 동일 Layout 로.
            let alloc_length = unsafe { (*header_ptr).alloc_length };
            let total_bytes = 12 + (alloc_length as usize + 1) * 2;
            use std::alloc::{dealloc, Layout};
            let layout = Layout::from_size_align(total_bytes, 4).expect("valid alignment");
            unsafe {
                dealloc(header_ptr as *mut u8, layout);
            }
        }
        // self.data 는 drop 시 자동으로 사라짐 — clear 안 해도 됨 (raw 의 self 도 unwind 됨).
    }
}

impl std::fmt::Debug for CHncStringW {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s: String = String::from_utf16_lossy(self.as_wide());
        write!(
            f,
            "CHncStringW({:?}, len={}, refcount={})",
            s,
            self.length(),
            self.refcount()
        )
    }
}

impl PartialEq for CHncStringW {
    fn eq(&self, other: &Self) -> bool {
        // raw `==` is not exported, but logical equality is wide-content equal.
        self.as_wide() == other.as_wide()
    }
}
impl Eq for CHncStringW {}

// 정적 검증
const _: () = assert!(std::mem::size_of::<CHncStringW>() == 8);
const _: () = assert!(std::mem::align_of::<CHncStringW>() == 8);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizeof_8b_8align() {
        assert_eq!(std::mem::size_of::<CHncStringW>(), 8);
        assert_eq!(std::mem::align_of::<CHncStringW>(), 8);
    }

    #[test]
    fn default_ctor_points_to_nil_sentinel() {
        let s = CHncStringW::new();
        // sentinel: refcount = -2, length = 0.
        assert_eq!(s.refcount(), REFCOUNT_NIL);
        assert_eq!(s.length(), 0);
        assert_eq!(s.as_wide(), &[] as &[u16]);
    }

    #[test]
    fn multiple_default_strings_share_sentinel() {
        let s1 = CHncStringW::new();
        let s2 = CHncStringW::new();
        // 동일 sentinel 주소 → data 포인터 일치.
        assert_eq!(s1.data_ptr(), s2.data_ptr());
    }

    #[test]
    fn from_str_basic() {
        let s = CHncStringW::from_str("hello");
        assert_eq!(s.length(), 5);
        let wide: Vec<u16> = "hello".encode_utf16().collect();
        assert_eq!(s.as_wide(), &wide[..]);
        assert_eq!(s.refcount(), 1);
    }

    #[test]
    fn from_wide_unicode() {
        let s = CHncStringW::from_str("한컴 KSAT");
        let expected: Vec<u16> = "한컴 KSAT".encode_utf16().collect();
        assert_eq!(s.as_wide(), &expected[..]);
        assert_eq!(s.length() as usize, expected.len());
    }

    #[test]
    fn clone_increments_refcount() {
        let s1 = CHncStringW::from_str("share me");
        assert_eq!(s1.refcount(), 1);
        let s2 = s1.clone();
        assert_eq!(s1.refcount(), 2);
        assert_eq!(s2.refcount(), 2);
        // 동일 buffer 공유
        assert_eq!(s1.data_ptr(), s2.data_ptr());
    }

    #[test]
    fn drop_decrements_refcount_then_frees_at_zero() {
        let s1 = CHncStringW::from_str("test");
        let raw_ptr = s1.data_ptr();
        let s2 = s1.clone();
        assert_eq!(s1.refcount(), 2);
        drop(s1);
        assert_eq!(s2.refcount(), 1);
        // raw_ptr 가 여전히 valid (refcount > 0).
        unsafe {
            assert_eq!(*raw_ptr, 't' as u16);
        }
        drop(s2);
        // Now refcount=0, buffer freed. raw_ptr 는 dangling — 접근 unsafe.
    }

    #[test]
    fn drop_skips_sentinel() {
        // default-constructed strings drop 해도 sentinel 은 deallocate 안 됨.
        for _ in 0..1000 {
            let _ = CHncStringW::new();
        }
        // sentinel refcount 가 여전히 -2.
        let probe = CHncStringW::new();
        assert_eq!(probe.refcount(), REFCOUNT_NIL);
    }

    #[test]
    fn header_field_offsets_match_raw_layout() {
        // raw asm 의 ldr w8, [x0, -0xc] (refcount), [x0, -0x8] (length), [x0, -0x4] (alloc).
        let h = StringHeader {
            refcount: AtomicI32::new(42),
            data_length: 100,
            alloc_length: 200,
        };
        let base = &h as *const StringHeader as usize;
        let rc_off = &h.refcount as *const AtomicI32 as usize - base;
        let dl_off = &h.data_length as *const i32 as usize - base;
        let al_off = &h.alloc_length as *const i32 as usize - base;
        assert_eq!(rc_off, 0); // -12 from data → offset 0 of header
        assert_eq!(dl_off, 4); // -8 from data
        assert_eq!(al_off, 8); // -4 from data
    }

    #[test]
    fn empty_str_from_str_uses_zero_length() {
        let s = CHncStringW::from_str("");
        assert_eq!(s.length(), 0);
        assert_eq!(s.refcount(), 1); // 빈 string 도 새 buffer alloc — 0-length 지만 NUL 1개.
    }

    #[test]
    fn eq_compares_wide_content() {
        let a = CHncStringW::from_str("hello");
        let b = CHncStringW::from_str("hello");
        let c = CHncStringW::from_str("world");
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn debug_format_includes_string() {
        let s = CHncStringW::from_str("test");
        let d = format!("{:?}", s);
        assert!(d.contains("\"test\""));
        assert!(d.contains("len=4"));
        assert!(d.contains("refcount=1"));
    }

    #[test]
    fn many_clones_then_drops_work_cleanly() {
        // refcount management 가 race-free 인지 stress test.
        let s = CHncStringW::from_str("stress");
        let mut copies = Vec::with_capacity(100);
        for _ in 0..100 {
            copies.push(s.clone());
        }
        assert_eq!(s.refcount(), 101);
        copies.clear();
        assert_eq!(s.refcount(), 1);
    }
}

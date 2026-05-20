//! `Hnc::Shape::ColorEffect` — 24B `std::__1::vector<u64>` 1:1 port.
//!
//! libHncDrawingEngine_arm64.dylib 의 `Hnc::Shape::ColorEffect` 는 실제로
//! libc++ `std::vector<T>` 의 24B layout (begin/end/cap_end 의 3 raw pointer)
//! 이며, T 는 8B packed `(PKey: u32, float: u32)` 으로 인코딩된 u64 entry.
//!
//! # 본 R-1.5.4 단계 scope
//!
//! ColorScheme::ColorScheme() (12 SetAt) 의 byte-equivalent 에 필요한
//! 최소 동작만 port:
//! - `Create()` (raw @ `0xbec48`) — alloc 24B struct, zero-init.
//! - clone (raw @ `0x65411c`) — heap-alloc 새 struct + buffer memcpy.
//! - destructor inline (`~Color()` 내부의 `if begin: end=begin; delete begin; delete struct`).
//!
//! `Add(PKey, float)` (raw @ `0xbed4c`) / `operator=` (`0xbec78`) / `Apply` /
//! `Begin`/`End`/`operator==` 은 본 단계의 ColorScheme init 에 도달 안 함 —
//! [[rhwp-rendering-phase-grand-plan]] 의 후속 단계 (Color::Color(SchemeStyle,
//! float) 또는 다른 caller 가 도달 시) 에 추가 port.
//!
//! # raw 24B layout (libc++ std::vector)
//!
//! ```text
//! offset  field         type     의미
//! 0x00    begin         u64*     데이터 버퍼 시작
//! 0x08    end           u64*     데이터 버퍼 끝 (= begin + size)
//! 0x10    cap_end       u64*     할당된 버퍼 끝 (= begin + capacity)
//! ```
//!
//! entry = `u64 = (float_bits << 32) | PKey_u32`  (raw `Add` 의 `orr x24, x9, x8, lsl #32`).
//!
//! # raw `Create()` @ `0xbec48`
//!
//! ```asm
//! bec48: stp x20, x19, [sp, #-0x20]!
//! bec4c: stp x29, x30, [sp, #0x10]
//! bec50: add x29, sp, #0x10
//! bec54: mov x19, x8                ; x8 = sret pointer (caller-provided slot)
//! bec58: mov w0, #0x18              ; 24
//! bec5c: bl operator_new            ; x0 = malloc(24)
//! bec60: stp xzr, xzr, [x0, #0x8]   ; [0x08] = 0, [0x10] = 0
//! bec64: str xzr, [x0]              ; [0x00] = 0
//! bec68: str x0, [x19]              ; *sret = ptr
//! bec6c: ldp x29, x30, [sp, #0x10]
//! bec70: ldp x20, x19, [sp], #0x20
//! bec74: ret
//! ```
//!
//! Rust port: `unsafe fn create() -> *mut ColorEffect` allocates 24B, sets all to null.
//!
//! # raw clone @ `0x65411c`
//!
//! ```asm
//! 65411c: cbz x0, return_null
//! 654130: mov x21, x0               ; x21 = src
//! 654134: mov w0, #0x18; bl operator_new   ; x0 = malloc(24)  → new struct
//! 65413c: mov x19, x0
//! 654140: stp xzr, xzr, [x0, #0x8]  ; new[0x08..0x18] = 0,0
//! 654144: str xzr, [x0]             ; new[0x00] = 0
//! 654148: ldp x20, x8, [x21]        ; x20 = src.begin, x8 = src.end
//! 65414c: subs x21, x8, x20         ; x21 = size_bytes
//! 654150: b.eq exit_empty
//! 654154: tbnz x21, #0x3f, error   ; if size_bytes < 0: abort
//! 654158: mov x0, x21; bl operator_new      ; new buffer
//! 654160: mov x22, x0
//! 654164: asr x8, x21, #3           ; num_elem = size_bytes / 8
//! 654168: str x0, [x19]             ; new.begin = buffer
//! 65416c: add x8, x0, x8, lsl #3    ; aligned_end = buffer + num_elem*8
//! 654170: str x8, [x19, #0x10]      ; new.cap_end = aligned_end  ← cap is set first
//! 654174: and x21, x21, #0xfffffffffffffff8 ; size_bytes_aligned = size_bytes & ~7
//! 654178-654180: memcpy(buffer, src.begin, size_bytes_aligned)
//! 654184: add x8, x22, x21          ; aligned_end_v2 = buffer + size_bytes_aligned
//! 654188: str x8, [x19, #0x8]       ; new.end = aligned_end_v2
//! ```
//!
//! Note: cap_end = `buffer + (size_bytes/8)*8` 이며 end = `buffer + (size_bytes & ~7)` —
//! valid u64-multiple input 에서 두 값 동일.
//!
//! # ~ColorEffect 인라인 (raw `~Color()` 의 일부 @ `0x14c884..0x14c89c`)
//!
//! ```asm
//! 14c880: ldr x20, [x0, #0x10]      ; x20 = self.color_effect (Color [+0x10])
//! 14c884: cbz x20, exit
//! 14c888: ldr x0, [x20]             ; x0 = (*color_effect).begin
//! 14c88c: cbz x0, skip_buf
//! 14c890: str x0, [x20, #0x8]       ; (*color_effect).end = begin  ← libc++ __clear
//! 14c894: bl operator_delete        ; free(begin)
//! 14c898: mov x0, x20
//! 14c89c: bl operator_delete        ; free(color_effect struct)
//! ```

use std::alloc::Layout;
use std::ptr;

/// libc++ `std::vector<u64>` 의 raw 24B layout — `Hnc::Shape::ColorEffect`.
#[repr(C)]
#[derive(Debug)]
pub struct ColorEffect {
    /// `begin` — 데이터 버퍼 시작.
    pub begin: *mut u64,
    /// `end` — 데이터 버퍼 끝 (= begin + size).
    pub end: *mut u64,
    /// `cap_end` — 할당된 버퍼 끝 (= begin + capacity).
    pub cap_end: *mut u64,
}

/// Compile-time 사이즈 + align 검증 (raw 와 byte-equivalent).
pub const COLOR_EFFECT_SIZE_BYTES: usize = 24;
pub const COLOR_EFFECT_ALIGN_BYTES: usize = 8;

const _: () = assert!(std::mem::size_of::<ColorEffect>() == COLOR_EFFECT_SIZE_BYTES);
const _: () = assert!(std::mem::align_of::<ColorEffect>() == COLOR_EFFECT_ALIGN_BYTES);

impl ColorEffect {
    /// raw `Create()` @ `0xbec48` — heap-alloc 24B 새 ColorEffect, begin/end/cap_end=null.
    ///
    /// # Safety
    /// 반환된 ptr 은 `ColorEffect::raw_delete` 또는 동등 cleanup 으로 해제해야 함.
    /// Color 객체에 attach 시 `Color::~Color()` 가 cleanup 책임을 진다.
    pub unsafe fn create() -> *mut ColorEffect {
        let layout = Layout::new::<ColorEffect>();
        let p = std::alloc::alloc(layout) as *mut ColorEffect;
        if p.is_null() {
            std::alloc::handle_alloc_error(layout);
        }
        ptr::write(
            p,
            ColorEffect {
                begin: ptr::null_mut(),
                end: ptr::null_mut(),
                cap_end: ptr::null_mut(),
            },
        );
        p
    }

    /// raw clone @ `0x65411c` — null → null, 아니면 새 heap-alloc + buffer memcpy.
    ///
    /// Color copy ctor + ColorScheme::SetAt 에서 사용.
    ///
    /// # Safety
    /// `src` 는 valid 한 `ColorEffect*` 또는 null 이어야 함. 반환 ptr 은
    /// `raw_delete` 로 해제 필요.
    pub unsafe fn clone_raw(src: *const ColorEffect) -> *mut ColorEffect {
        // raw `65411c: cbz x0, return_null`
        if src.is_null() {
            return ptr::null_mut();
        }
        // raw `654134-654148`: alloc 24B + zero-init
        let struct_layout = Layout::new::<ColorEffect>();
        let new_p = std::alloc::alloc(struct_layout) as *mut ColorEffect;
        if new_p.is_null() {
            std::alloc::handle_alloc_error(struct_layout);
        }
        // Stage 1: zero-init the struct.
        ptr::write(
            new_p,
            ColorEffect {
                begin: ptr::null_mut(),
                end: ptr::null_mut(),
                cap_end: ptr::null_mut(),
            },
        );

        // raw `654148: ldp x20, x8, [x21]`
        let s_begin = (*src).begin;
        let s_end = (*src).end;
        // raw `65414c: subs x21, x8, x20`
        let size_bytes = (s_end as usize).wrapping_sub(s_begin as usize) as isize;
        // raw `654150: b.eq exit_empty` — 0 size: 빈 vector, struct 만 alloc 한 그대로
        if size_bytes == 0 {
            return new_p;
        }
        // raw `654154: tbnz x21, #0x3f, error` — 음수는 panic
        assert!(
            size_bytes > 0,
            "ColorEffect::clone_raw: end < begin (size_bytes={})",
            size_bytes
        );
        let size_bytes_u = size_bytes as usize;

        // raw `654158-65415c`: alloc buffer of size_bytes
        let buf_layout =
            Layout::from_size_align(size_bytes_u, 8).expect("ColorEffect buf layout");
        let buf = std::alloc::alloc(buf_layout) as *mut u64;
        if buf.is_null() {
            std::alloc::handle_alloc_error(buf_layout);
        }
        // raw `654168: str x0, [x19]` — new.begin = buffer
        (*new_p).begin = buf;
        // raw `654164: asr x8, x21, #3; 65416c: add x8, x0, x8, lsl #3`
        //  → cap_end = buffer + (size_bytes / 8) * 8
        let num_elem = size_bytes_u / 8;
        let cap_end_off = num_elem * 8;
        (*new_p).cap_end = (buf as *mut u8).add(cap_end_off) as *mut u64;
        // raw `654174: and x21, x21, #~7` — size_bytes_aligned
        let size_bytes_aligned = size_bytes_u & !7usize;
        // raw `654178-654180: memcpy(buffer, src.begin, size_bytes_aligned)`
        ptr::copy_nonoverlapping(s_begin as *const u8, buf as *mut u8, size_bytes_aligned);
        // raw `654184-654188`: new.end = buffer + size_bytes_aligned
        (*new_p).end = (buf as *mut u8).add(size_bytes_aligned) as *mut u64;
        new_p
    }

    /// raw `~ColorEffect()` (`~Color()` 에 inline) — buffer 만 free.
    ///
    /// 호출 후 begin/end/cap_end 모두 null. Struct 자체의 heap dealloc 은
    /// 호출자 책임 (`Color::~Color()` 가 `raw_delete` 로 결합 호출).
    ///
    /// # Safety
    /// `self` 는 valid mutable 참조. 호출 후 self 의 begin/end/cap_end 는 모두
    /// null 이 되어 idempotent.
    pub unsafe fn destruct_inplace(&mut self) {
        // raw `14c884: cbz x20, exit` — src null 이면 caller 가 미리 분기,
        // 이 함수 내부에선 self.begin 만 검사.
        if !self.begin.is_null() {
            // raw `14c890: str x0, [x20, #0x8]` — end = begin (libc++ __clear)
            self.end = self.begin;
            // raw `14c894: bl operator_delete` — free(begin)
            // libc++ vector<u64> 는 cap = (cap_end - begin) bytes 의 buffer 를
            // alloc 했음.
            let buf_size = (self.cap_end as usize).wrapping_sub(self.begin as usize);
            // operator_delete(p) 는 size 미요구 (sized-deallocation off) 이지만
            // Rust 의 dealloc 은 Layout 필요. cap_size 가 정확한 alloc size.
            if buf_size > 0 {
                let layout = Layout::from_size_align(buf_size, 8)
                    .expect("ColorEffect::destruct_inplace buf layout");
                std::alloc::dealloc(self.begin as *mut u8, layout);
            }
            self.begin = ptr::null_mut();
            self.end = ptr::null_mut();
            self.cap_end = ptr::null_mut();
        }
    }

    /// `~ColorEffect()` + struct heap dealloc 의 결합 — `~Color()` 의 inline
    /// 패턴 (`bl operator_delete(struct)` 포함) 과 1:1.
    ///
    /// # Safety
    /// `p` 는 `ColorEffect::create` 또는 `clone_raw` 로 alloc 된 ptr 또는 null.
    pub unsafe fn raw_delete(p: *mut ColorEffect) {
        // raw `14c884: cbz x20, exit`
        if p.is_null() {
            return;
        }
        // buffer 해제
        (*p).destruct_inplace();
        // raw `14c898-14c89c: mov x0, x20; bl operator_delete` — struct 자체 free
        std::alloc::dealloc(p as *mut u8, Layout::new::<ColorEffect>());
    }

    /// 현재 element 수 (= `(end - begin) / 8`).
    #[inline]
    pub fn len(&self) -> usize {
        if self.begin.is_null() {
            return 0;
        }
        let size_bytes = (self.end as usize).wrapping_sub(self.begin as usize);
        size_bytes / 8
    }

    /// 현재 capacity (= `(cap_end - begin) / 8`).
    #[inline]
    pub fn capacity(&self) -> usize {
        if self.begin.is_null() {
            return 0;
        }
        let cap_bytes = (self.cap_end as usize).wrapping_sub(self.begin as usize);
        cap_bytes / 8
    }

    /// empty 인가? (len == 0)
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// raw u64 entry (=packed PKey+float) 를 1개 push_back 하는 빠른 helper —
    /// **테스트용**. raw `Add(PKey, float)` (`0xbed4c`) 의 전체 jump table 동작은
    /// 본 파일의 [`ColorEffect::add`] 에 1:1 port.
    ///
    /// 본 helper 는 capacity 충분할 때 단순 `*end++ = entry` (raw `bedb4` line:
    /// `str x24, [x20], #0x8`) 만 수행 — capacity 부족 시 panic.
    ///
    /// # Safety
    /// `self.cap_end > self.end` (capacity ≥ 1 더 가능) 여야 함.
    pub unsafe fn push_unchecked_no_realloc(&mut self, entry: u64) {
        assert!(
            (self.end as usize) < (self.cap_end as usize),
            "push_unchecked_no_realloc: no capacity"
        );
        ptr::write(self.end, entry);
        self.end = self.end.add(1);
    }

    /// `Hnc::Shape::ColorEffect::Add(PKey, float)` (raw @ `0xbed4c`, 501 lines) 1:1.
    ///
    /// PKey ∈ [500..527] 만 처리, 그 외는 no-op (raw `bed70: b.hi epilogue`).
    /// 28-byte jump table @ `0x7431e7` 에 기반한 6 distinct branches:
    ///
    /// | PKey | byte | 동작                                           |
    /// |------|------|------------------------------------------------|
    /// | 500  | 0xcb | clamp [0, 1] + key=500 hardcode (raw `0xbf0bc`) |
    /// | 501  | 0x7a | clamp [-1, 1] + key=PKey (raw `0xbef78`)        |
    /// | 502  | 0x7a | clamp [-1, 1] + key=PKey (raw `0xbef78`)        |
    /// | 503..511, 513, 515..522 | 0x00 | default REPLACE (raw `0xbed90`)  |
    /// | 512  | 0x9b | Degree(value).GetValue() + key=512 (raw `0xbeffc`) |
    /// | 514  | 0xa8 | clamp [-16000, +16000] + key=514 (raw `0xbf030`) |
    /// | 523..527 | 0x23 | only-if-value==1.0 push (raw `0xbee1c`)    |
    ///
    /// push_back 은 libc++ `std::vector<u64>::__emplace_back_slow_path` 1:1 —
    /// new_cap = max(old_cap × 2, req), max_size = `0x1fffffffffffffff`.
    /// 자세한 raw 인용은 `kdsnr-hwp-toolkit/work/hft_re/render_re/COLOREFFECT_ADD_RE.txt`.
    ///
    /// # Safety
    /// `self` 는 valid mutable 참조. 내부 reallocate 시 `std::alloc::alloc` /
    /// `dealloc` 사용 (raw `operator_new` / `operator_delete` 와 동일 semantic).
    pub unsafe fn add(&mut self, pkey: u32, value: f32) {
        // raw `bed68-bed70`: PKey 범위 체크
        let offset = pkey.wrapping_sub(500);
        if offset > 27 {
            return;
        }

        // raw `bed78-bed8c`: jump table dispatch
        // 28-byte LUT @ binary 0x7431e7 (objdump -s -j __const 추출)
        const JUMP_TABLE: [u8; 28] = [
            0xcb, 0x7a, 0x7a, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x9b, 0x00,
            0xa8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x23, 0x23, 0x23, 0x23, 0x23,
        ];
        let lut_byte = JUMP_TABLE[offset as usize];

        // 각 분기마다 (out_value, out_pkey) 결정 후 push_back
        let (out_value, out_pkey) = match lut_byte {
            // Branch: default REPLACE (raw `0xbed90`)
            0x00 => (value, pkey),

            // Branch: only-if-value==1.0 (raw `0xbee1c`)
            0x23 => {
                // raw `bee20-bee24: fcmp s0, #1.0; b.ne epilogue`
                if value != 1.0 {
                    return;
                }
                // raw `bee2c: orr x24, x8, #0x3f80000000000000` — 명시적 1.0 비트
                (1.0_f32, pkey)
            }

            // Branch: clamp [-1, 1] + key=PKey (raw `0xbef78`)
            0x7a => (clamp_raw(value, -1.0, 1.0), pkey),

            // Branch: Degree normalize + key=512 (raw `0xbeffc`)
            0x9b => (degree_normalize(value), 512),

            // Branch: clamp [-16000, +16000] + key=514 (raw `0xbf030`)
            0xa8 => (clamp_raw(value, -16000.0, 16000.0), 514),

            // Branch: clamp [0, 1] + key=500 (raw `0xbf0bc`)
            0xcb => (clamp_raw(value, 0.0, 1.0), 500),

            _ => unreachable!("JUMP_TABLE byte {:#x} not handled", lut_byte),
        };

        // raw push_back common — packed u64 = pkey | (bits(value) << 32)
        // raw `bed98: orr x24, x9, x8, lsl #32`
        let packed: u64 = (out_pkey as u64) | ((out_value.to_bits() as u64) << 32);

        self.push_back_libcpp(packed);
    }

    /// libc++ `std::vector<u64>::push_back` 1:1 — fast path + reallocate-grow.
    ///
    /// raw fast `beda8: str x24, [x20], #0x8` + slow path `bedb4..bef58`.
    /// new_cap = `max(old_cap * 2, old_size + 1)`, max_size = `0x1fffffffffffffff`.
    unsafe fn push_back_libcpp(&mut self, entry: u64) {
        // raw `bed9c: ldp x20, x8, [x19, #0x8]` — end, cap_end
        if (self.end as usize) < (self.cap_end as usize) {
            // fast path: capacity 있음
            ptr::write(self.end, entry);
            self.end = self.end.add(1);
            return;
        }

        // slow path — reallocate (raw `bedb4..bef58`)
        let old_begin = self.begin;
        let old_end = self.end;
        let old_cap_end = self.cap_end;

        // raw `bedb8: sub x25, x20, x21` — old_size_bytes
        let old_size_bytes = (old_end as usize).wrapping_sub(old_begin as usize);
        let old_num_elem = old_size_bytes / 8;

        // raw `bedc0: add x9, x23, #0x1` — req_num_elem = old_num_elem + 1
        let req_num_elem = old_num_elem.checked_add(1).expect("ColorEffect::add overflow");

        // raw `bedc4: lsr x10, x9, #61; cbnz x10, throw` — max 2^61 elem check
        // (실제 도달 불가능 — assert 로 처리)
        assert!(
            req_num_elem >> 61 == 0,
            "ColorEffect::add req_num_elem too large"
        );

        // raw `bedcc-bedd4: doubled_in_elem = (cap_bytes/4) = cap_num_elem * 2`
        let old_cap_bytes = (old_cap_end as usize).wrapping_sub(old_begin as usize);
        let doubled = old_cap_bytes / 4; // = cap_num_elem * 2

        // raw `bedd8-beddc: csel x9, x11, x9, hi` — new_cap = max(doubled, req)
        let mut new_cap_elem = if doubled > req_num_elem {
            doubled
        } else {
            req_num_elem
        };

        // raw `bede0-bede8: cmp x8, x10 (0x7ffffffffffffff8); csel x22, x9, x8, lo`
        // → if old_cap_bytes < max_threshold: keep computed; else clamp to max_size
        const MAX_SIZE_ELEM: usize = 0x1fffffffffffffff;
        if old_cap_bytes >= 0x7ffffffffffffff8usize {
            new_cap_elem = MAX_SIZE_ELEM;
        }

        // raw `bedec: cbz x22, 0xbeea4` — new_cap == 0 (불가능: req≥1)
        debug_assert!(new_cap_elem > 0);

        // raw `bedf0-bedf4: overflow check (new_cap >> 61)`
        assert!(new_cap_elem >> 61 == 0, "ColorEffect::add new_cap too large");

        // raw `bedf8-bedfc: lsl x0, x22, #3; bl operator_new` — alloc new_cap * 8 bytes
        let new_cap_bytes = new_cap_elem * 8;
        let new_layout = Layout::from_size_align(new_cap_bytes, 8).expect("ColorEffect grow layout");
        let new_buf = std::alloc::alloc(new_layout) as *mut u64;
        if new_buf.is_null() {
            std::alloc::handle_alloc_error(new_layout);
        }

        // raw `bee00-bee0c: write_pos = new_buf + old_num_elem; *write_pos = entry; end_after = write_pos + 8`
        let write_pos = new_buf.add(old_num_elem);
        ptr::write(write_pos, entry);
        let new_end = write_pos.add(1);

        // raw `bee10: subs x10, x20, x21` — old non-empty?
        if old_size_bytes != 0 {
            // raw `beec0..bef40` — copy old → new (memmove down from high to low)
            // raw 는 SIMD-unrolled reverse copy (0x40 chunks); 의미상 memcpy
            ptr::copy_nonoverlapping(old_begin as *const u8, new_buf as *mut u8, old_size_bytes);
        }

        // raw `bef44-bef48: stp x9, x22, [x19]; str x8, [x19, #0x10]`
        //   → self.begin = new_buf, self.end = write_pos (will be re-set), self.cap_end = new_buf + cap*8
        // raw `bef4c-bef54: cbz/bl operator_delete` — free old
        // raw `bef58: str x22, [x19, #0x8]` — self.end = end_after (FINAL)
        self.begin = new_buf;
        self.end = new_end;
        self.cap_end = new_buf.add(new_cap_elem);

        if !old_begin.is_null() {
            // free old buffer (old_cap_bytes 로 layout 재생성)
            if old_cap_bytes > 0 {
                let old_layout = Layout::from_size_align(old_cap_bytes, 8)
                    .expect("ColorEffect old buf layout");
                std::alloc::dealloc(old_begin as *mut u8, old_layout);
            }
        }
    }

    /// `Hnc::Shape::ColorEffect::operator==(const ColorEffect&) const`
    /// (raw @ `0x14cab4..0x14cbb8`) 1:1.
    ///
    /// 알고리즘:
    /// 1. self.entries 의 fold(acc=1.0):
    ///    - PKey 500 → KEEP (acc 그대로)
    ///    - PKey 501 → MUL (acc *= value)
    ///    - PKey 502 → ADD (acc += value)
    ///    - else    → REPLACE (acc = value)
    /// 2. `fminnm(acc, 1.0)` (raw `14cb08: fminnm s1, s1, s0`).
    /// 3. other 도 동일하게 fold.
    /// 4. folded 값 다르면 → false (raw `14cb6c: mov w0, #0; ret`).
    /// 5. lengths 비교: 다르면 false.
    /// 6. 둘 다 empty: true.
    /// 7. element-wise (pkey_u32, value_bits) 정확히 같아야 true.
    pub fn operator_eq(&self, other: &Self) -> bool {
        // raw `14cab8-14cabc: fmov s0, s1, #1.0` — both accumulators init 1.0
        let self_folded = fold_alpha(self);
        let other_folded = fold_alpha(other);

        // raw `14cb64-14cb70: fcmp s1, s0; b.ne → ret w0=0`
        // f32 bit-pattern 비교 — NaN 도 정확히 같은 비트면 같다고 봐야 byte-eq
        if self_folded.to_bits() != other_folded.to_bits() {
            // fcmp 의 b.eq 는 NaN ≠ NaN 으로 처리 → 비트 같아도 false 반환
            // 표준 IEEE fcmp 동작: NaN any-comparison 은 false (eq 안 fire)
            // 따라서 raw 와 byte-eq 위해 표준 f32 비교 사용
            if !(self_folded == other_folded) {
                return false;
            }
        } else if self_folded.is_nan() {
            // 비트 같아도 NaN 이면 raw 는 b.eq fire 안 함 → false
            return false;
        }
        // f32 표준 비교로 처리
        if !(self_folded == other_folded) {
            return false;
        }

        // raw `14cb74-14cb84: 길이 비교`
        let self_size_bytes = (self.end as usize).wrapping_sub(self.begin as usize);
        let other_size_bytes = (other.end as usize).wrapping_sub(other.begin as usize);
        if self_size_bytes != other_size_bytes {
            return false;
        }

        // raw `14cb80: ccmp x10, x12, #0, ne; b.ne` — if 둘 다 0 (= self_size_bytes==0):
        //   subs x10, x8, x9 → x10=0 → Z=1 (= EQ from subs)
        //   ccmp x10, x12, #0, ne: ne flag 가 NOT 켜져 있으니 NZCV=#0 (Z=0,N=0)
        //   b.ne ⇒ Z=0 → fires → 14cbb8 (ret, w0 carries last cset)
        //   last cset 은 14cb78 의 `cset w0, eq` (lengths equal) → w0=1
        if self_size_bytes == 0 {
            return true;
        }

        // raw element-wise (14cb88..14cbb4):
        // (pkey_u32, value_bits) pairwise compare; mismatch → false
        let num_elem = self_size_bytes / 8;
        unsafe {
            for i in 0..num_elem {
                let s = *self.begin.add(i);
                let o = *other.begin.add(i);
                if s != o {
                    return false;
                }
            }
        }
        true
    }
}

/// raw `b.mi/b.le` 패턴 2-단계 clamp — NaN 보존.
///
/// raw 동작 (예: PKey 500 의 clamp [0, 1] @ `0xbf0bc`):
/// - `fcmp s0, lo; b.mi → clamp to lo` (NaN 은 b.mi fire 안 함)
/// - `fcmp s0, hi; b.le → keep s0` (NaN 은 V=1,N=0 → N≠V → b.le fires)
/// - 그 외: clamp to hi
///
/// 결과: `value < lo → lo`, `value > hi → hi`, NaN 또는 [lo, hi] 범위 → 그대로.
#[inline]
fn clamp_raw(value: f32, lo: f32, hi: f32) -> f32 {
    if value < lo {
        // raw b.mi fire (N=1)
        lo
    } else if value > hi {
        // raw b.le 안 fire (Z=0, N=0=V) → fall to clamp
        hi
    } else {
        // [lo, hi] 또는 NaN: raw 는 keep
        value
    }
}

/// `Hnc::Util::Degree::Degree(float)` (libHncFoundation @ `0x123d4`) +
/// `Degree::GetValue() const` (`0x12564`) 결합 1:1.
///
/// value 를 `[0, 360]` 범위로 normalize. raw asm 1:1:
///
/// ```text
/// if 0 <= value <= 360.0: return value  ; skip normalize
/// else:
///     q_int = (i32)(i64)trunc(value)    ; fcvtzs x8, s0 → use w8
///     prod = (i64)q_int * (i64)0xb60b60b7_i32
///     hi = (prod >> 32) as i32
///     acc = hi.wrapping_add(q_int)
///     q360 = (acc >> 8) + ((acc as u32) >> 31)  ; round-to-nearest
///     r = (q360 as f32).mul_add(-360.0, value)  ; fmadd single-rounding
///     if r < 0.0: r + 360.0 else r
/// ```
fn degree_normalize(value: f32) -> f32 {
    // raw `123d4-123e4: fcmp s0, #0.0; fccmp s0, 360.0, #0, pl; b.le skip`
    // PL = N=0 (s0 ≥ 0): 그 경우만 360 비교, 아니면 NZCV=0 (NE,LO,LT)
    // b.le fires when Z=1 OR N!=V
    //  - s0 < 0:        NZCV=0 (Z=0,N=0=V) → b.le 안 fire → normalize
    //  - s0 == 0:       Z=1 (from first fcmp) — but fccmp overrides flags
    //    실은 fccmp 가 cmp(0, 360) 실행 → s0 ≤ 360 → Z=0,N=1,V=0 → b.le fires
    //  - 0 < s0 ≤ 360:  fccmp(s0,360) → Z=1 or N=1 → b.le fires
    //  - s0 > 360:      fccmp → Z=0,N=0 → b.le 안 fire → normalize
    //  - NaN ≥ 0?       PL? NaN fcmp: Z=0,C=1,N=0,V=1 → PL (N=0) → fccmp(NaN,360):
    //                   NZCV unordered=0011 (N=0,V=1) → b.le (N!=V) fires → skip
    //  → in [0, 360] (or NaN): skip; else: normalize
    let need_normalize = !(value >= 0.0 && value <= 360.0);
    if !need_normalize {
        return value;
    }

    // raw `123e8: fcvtzs x8, s0` — saturating truncate to i64, then use w8 (low 32 signed)
    // Rust: `value as i32` 가 saturating 으로 i32::MIN/MAX 클램프. raw 와 byte-eq.
    //
    // 더 정확히는 raw 는 `fcvtzs x8` (64-bit) → smull w8 (low 32) — 즉
    // 64-bit fcvtzs 후 low 32-bit 사용. value 가 ±2^31 안 넘으면 동일.
    // Rust `value as i32` 는 ±2^31 saturating — raw 의 동작과 일치 (low 32 사용).
    let q_int: i32 = value as i32;

    // raw `123ec-12404: signed reciprocal magic for /360`
    // w9 = 0xb60b60b7 (signed -1240768841)
    let magic: i32 = 0xb60b60b7u32 as i32;
    let prod: i64 = (q_int as i64).wrapping_mul(magic as i64);
    let hi32: i32 = (prod >> 32) as i32;
    let acc: i32 = hi32.wrapping_add(q_int);
    // raw `12400: asr w9, w8, #8; 12404: add w8, w9, w8, lsr #31`
    let q360: i32 = (acc >> 8).wrapping_add(((acc as u32) >> 31) as i32);

    // raw `12408-12414: scvtf s1, w8; fmadd s0, s1, s2, s0` (s2 = -360.0)
    let q360_f = q360 as f32;
    let r = q360_f.mul_add(-360.0, value); // FMA — single rounding

    // raw `12418-12428: fadd s1, s0, 360.0; fcmp s0, #0; fcsel s0, s1, s0, mi`
    if r < 0.0 {
        r + 360.0
    } else {
        r
    }
}

/// `ColorEffect::operator==` 내부 alpha fold loop (raw `14cad8..14cb04`) 1:1.
///
/// acc = 1.0 시작, 각 entry 에 대해:
/// - PKey 500: acc 유지
/// - PKey 501: acc *= value
/// - PKey 502: acc += value
/// - else:    acc = value
///
/// 마지막에 `fminnm(acc, 1.0)` (raw `14cb08`).
fn fold_alpha(ce: &ColorEffect) -> f32 {
    // raw `fmov s1, #1.0` — 초기 acc
    let mut acc: f32 = 1.0;

    let size_bytes = (ce.end as usize).wrapping_sub(ce.begin as usize);
    if size_bytes == 0 {
        // raw `14cb08: fminnm s1, s1, s0` (s0 도 1.0 init)
        return fminnm(acc, 1.0);
    }

    let num_elem = size_bytes / 8;
    unsafe {
        // raw `14cad4: add x12, x9, #0x4` — point at first .float
        let mut ptr = ce.begin;
        for _ in 0..num_elem {
            let entry: u64 = *ptr;
            let pkey: u32 = (entry & 0xFFFF_FFFF) as u32;
            let value_bits: u32 = (entry >> 32) as u32;
            let value: f32 = f32::from_bits(value_bits);

            // raw `14cae0: fadd s3, s1, s2; 14cae4: fmul s4, s1, s2`
            let sum = acc + value;
            let mul = acc * value;

            // raw `fcsel` 3-단계: REPLACE/MUL/ADD/KEEP
            // 14caec: fcsel s1, s1, s2, ne (cmp PKey, 500): NE → s2 (REPLACE), EQ → s1 (KEEP)
            // 14caf4: fcsel s1, s4, s1, eq (cmp PKey, 501): EQ → s4 (MUL)
            // 14cafc: fcsel s1, s3, s1, eq (cmp PKey, 502): EQ → s3 (ADD)
            acc = if pkey == 500 {
                acc // KEEP
            } else if pkey == 501 {
                mul // MUL
            } else if pkey == 502 {
                sum // ADD
            } else {
                value // REPLACE
            };

            ptr = ptr.add(1);
        }
    }

    // raw `14cb08: fminnm s1, s1, s0` (s0 = 1.0)
    fminnm(acc, 1.0)
}

/// IEEE 754-2008 `fminnm` (minNum) — NaN one-operand 시 다른 operand 반환.
///
/// 두 값 모두 number: smaller 반환. 한 쪽 NaN: number 반환. 양쪽 NaN: NaN.
///
/// arm64 `fminnm` 명령어와 동일 semantics.
fn fminnm(a: f32, b: f32) -> f32 {
    if a.is_nan() && b.is_nan() {
        a // 양쪽 NaN: a 반환 (IEEE 명시 안 됨; arm 도 quiet NaN)
    } else if a.is_nan() {
        b
    } else if b.is_nan() {
        a
    } else if a <= b {
        a
    } else {
        b
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_layout_size_align() {
        assert_eq!(std::mem::size_of::<ColorEffect>(), 24);
        assert_eq!(std::mem::align_of::<ColorEffect>(), 8);
    }

    #[test]
    fn create_returns_empty_struct() {
        unsafe {
            let p = ColorEffect::create();
            assert!(!p.is_null());
            assert!((*p).begin.is_null());
            assert!((*p).end.is_null());
            assert!((*p).cap_end.is_null());
            assert_eq!((*p).len(), 0);
            assert_eq!((*p).capacity(), 0);
            assert!((*p).is_empty());
            ColorEffect::raw_delete(p);
        }
    }

    #[test]
    fn clone_raw_of_null_returns_null() {
        unsafe {
            let cloned = ColorEffect::clone_raw(ptr::null());
            assert!(cloned.is_null());
        }
    }

    #[test]
    fn clone_raw_of_empty_returns_empty_struct() {
        unsafe {
            let src = ColorEffect::create();
            let dst = ColorEffect::clone_raw(src);
            assert!(!dst.is_null());
            assert!((*dst).begin.is_null());
            assert!((*dst).end.is_null());
            assert!((*dst).cap_end.is_null());
            ColorEffect::raw_delete(src);
            ColorEffect::raw_delete(dst);
        }
    }

    #[test]
    fn clone_raw_with_data_copies_buffer() {
        unsafe {
            // Manually build a ColorEffect with 3 entries.
            let src = ColorEffect::create();
            // Allocate a buffer of 3 × 8B = 24B with align 8.
            let layout = Layout::from_size_align(24, 8).unwrap();
            let buf = std::alloc::alloc(layout) as *mut u64;
            *buf = 0x1234_5678_DEAD_BEEFu64;
            *buf.add(1) = 0x0F0F_0F0F_F0F0_F0F0u64;
            *buf.add(2) = 0x0123_4567_89AB_CDEFu64;
            (*src).begin = buf;
            (*src).end = buf.add(3);
            (*src).cap_end = buf.add(3);

            // Clone
            let dst = ColorEffect::clone_raw(src);
            assert!(!dst.is_null());
            assert!(!(*dst).begin.is_null());
            assert_eq!((*dst).len(), 3);
            assert_eq!(*((*dst).begin), 0x1234_5678_DEAD_BEEFu64);
            assert_eq!(*((*dst).begin.add(1)), 0x0F0F_0F0F_F0F0_F0F0u64);
            assert_eq!(*((*dst).begin.add(2)), 0x0123_4567_89AB_CDEFu64);
            // Independent buffer (no aliasing)
            assert_ne!((*dst).begin, (*src).begin);

            // Mutate src; dst unchanged
            *((*src).begin) = 0;
            assert_eq!(*((*dst).begin), 0x1234_5678_DEAD_BEEFu64);

            // cap_end == end (no spare capacity in clone)
            assert_eq!((*dst).cap_end, (*dst).end);

            ColorEffect::raw_delete(dst);
            ColorEffect::raw_delete(src);
        }
    }

    #[test]
    fn destruct_inplace_idempotent() {
        unsafe {
            let mut ce = ColorEffect {
                begin: ptr::null_mut(),
                end: ptr::null_mut(),
                cap_end: ptr::null_mut(),
            };
            ce.destruct_inplace(); // null begin: no-op
            assert!(ce.begin.is_null());

            // With actual buffer
            let layout = Layout::from_size_align(16, 8).unwrap();
            let buf = std::alloc::alloc(layout) as *mut u64;
            *buf = 1;
            *buf.add(1) = 2;
            ce.begin = buf;
            ce.end = buf.add(2);
            ce.cap_end = buf.add(2);
            ce.destruct_inplace();
            assert!(ce.begin.is_null());
            assert!(ce.end.is_null());
            assert!(ce.cap_end.is_null());
            // Double-call idempotent
            ce.destruct_inplace();
            assert!(ce.begin.is_null());
        }
    }

    #[test]
    fn raw_delete_of_null_is_noop() {
        unsafe {
            ColorEffect::raw_delete(ptr::null_mut());
        }
    }

    #[test]
    fn push_unchecked_no_realloc_basic() {
        unsafe {
            // Allocate buffer of 4 × 8B = 32B
            let layout = Layout::from_size_align(32, 8).unwrap();
            let buf = std::alloc::alloc(layout) as *mut u64;
            let mut ce = ColorEffect {
                begin: buf,
                end: buf, // empty
                cap_end: buf.add(4),
            };
            assert_eq!(ce.len(), 0);
            ce.push_unchecked_no_realloc(0xDEADBEEFu64);
            assert_eq!(ce.len(), 1);
            assert_eq!(*ce.begin, 0xDEADBEEFu64);
            ce.push_unchecked_no_realloc(0xCAFEBABEu64);
            assert_eq!(ce.len(), 2);
            assert_eq!(*ce.begin.add(1), 0xCAFEBABEu64);
            // cleanup
            ce.destruct_inplace();
        }
    }

    #[test]
    fn len_capacity_consistency() {
        unsafe {
            let src = ColorEffect::create();
            assert_eq!((*src).len(), 0);
            assert_eq!((*src).capacity(), 0);

            // populate
            let layout = Layout::from_size_align(16, 8).unwrap();
            let buf = std::alloc::alloc(layout) as *mut u64;
            *buf = 100;
            *buf.add(1) = 200;
            (*src).begin = buf;
            (*src).end = buf.add(2);
            (*src).cap_end = buf.add(2);
            assert_eq!((*src).len(), 2);
            assert_eq!((*src).capacity(), 2);
            assert!(!(*src).is_empty());

            ColorEffect::raw_delete(src);
        }
    }

    #[test]
    fn clone_of_multi_entry_preserves_byte_pattern() {
        unsafe {
            // PKey 0x1f4 + alpha 0.5 packed: low 32 = 0x1f4, high 32 = float bits
            let alpha_bits = 0.5f32.to_bits() as u64;
            let packed1 = 0x1f4u64 | (alpha_bits << 32);
            // PKey 0x205 + 1.25
            let packed2 = 0x205u64 | ((1.25f32.to_bits() as u64) << 32);

            let src = ColorEffect::create();
            let layout = Layout::from_size_align(16, 8).unwrap();
            let buf = std::alloc::alloc(layout) as *mut u64;
            *buf = packed1;
            *buf.add(1) = packed2;
            (*src).begin = buf;
            (*src).end = buf.add(2);
            (*src).cap_end = buf.add(2);

            let dst = ColorEffect::clone_raw(src);
            // Byte-equivalent: dst buffer has same bit patterns
            assert_eq!(*((*dst).begin), packed1);
            assert_eq!(*((*dst).begin.add(1)), packed2);
            // Decoded
            assert_eq!((*((*dst).begin)) & 0xFFFF_FFFFu64, 0x1f4u64);
            let high_bits = ((*((*dst).begin)) >> 32) as u32;
            assert_eq!(f32::from_bits(high_bits), 0.5);
            assert_eq!(((*((*dst).begin.add(1)) >> 32) as u32), 1.25f32.to_bits());

            ColorEffect::raw_delete(dst);
            ColorEffect::raw_delete(src);
        }
    }

    // ===== add() + operator_eq() + helper tests =====

    /// helper — empty ColorEffect on heap.
    unsafe fn make_empty() -> *mut ColorEffect {
        ColorEffect::create()
    }

    #[test]
    fn add_out_of_range_pkey_is_noop() {
        unsafe {
            let p = make_empty();
            // PKey 499 (< 500): no-op
            (*p).add(499, 0.5);
            assert_eq!((*p).len(), 0);
            // PKey 528 (> 527): no-op
            (*p).add(528, 0.5);
            assert_eq!((*p).len(), 0);
            // PKey 0
            (*p).add(0, 1.0);
            assert_eq!((*p).len(), 0);
            ColorEffect::raw_delete(p);
        }
    }

    #[test]
    fn add_pkey_500_clamps_to_0_1() {
        unsafe {
            let p = make_empty();
            // 음수 → 0.0
            (*p).add(500, -0.5);
            assert_eq!((*p).len(), 1);
            let e0 = *(*p).begin;
            assert_eq!((e0 & 0xFFFF_FFFF) as u32, 500);
            assert_eq!(f32::from_bits((e0 >> 32) as u32), 0.0);

            // >1 → 1.0
            (*p).add(500, 2.0);
            assert_eq!((*p).len(), 2);
            let e1 = *(*p).begin.add(1);
            assert_eq!(f32::from_bits((e1 >> 32) as u32), 1.0);

            // in-range → unchanged
            (*p).add(500, 0.5);
            assert_eq!((*p).len(), 3);
            let e2 = *(*p).begin.add(2);
            assert_eq!(f32::from_bits((e2 >> 32) as u32), 0.5);

            ColorEffect::raw_delete(p);
        }
    }

    #[test]
    fn add_pkey_500_hardcodes_key_500() {
        // PKey 500 분기 (0xcb) 는 key 를 500 으로 강제 (이미 500 이지만)
        unsafe {
            let p = make_empty();
            (*p).add(500, 0.5);
            let e = *(*p).begin;
            assert_eq!((e & 0xFFFF_FFFF) as u32, 500);
            ColorEffect::raw_delete(p);
        }
    }

    #[test]
    fn add_pkey_501_502_clamps_to_neg1_pos1() {
        unsafe {
            let p = make_empty();
            (*p).add(501, -2.0); // → -1.0
            let e0 = *(*p).begin;
            assert_eq!((e0 & 0xFFFF_FFFF) as u32, 501);
            assert_eq!(f32::from_bits((e0 >> 32) as u32), -1.0);

            (*p).add(502, 1.5); // → 1.0
            let e1 = *(*p).begin.add(1);
            assert_eq!((e1 & 0xFFFF_FFFF) as u32, 502);
            assert_eq!(f32::from_bits((e1 >> 32) as u32), 1.0);

            (*p).add(501, 0.3); // in-range
            let e2 = *(*p).begin.add(2);
            assert_eq!(f32::from_bits((e2 >> 32) as u32), 0.3);

            ColorEffect::raw_delete(p);
        }
    }

    #[test]
    fn add_pkey_503_to_511_replace_no_clamp() {
        // default REPLACE 분기 (0x00) — clamp 없이 그대로 push
        unsafe {
            let p = make_empty();
            for pkey in [503u32, 504, 510, 511] {
                (*p).add(pkey, 12345.0);
            }
            assert_eq!((*p).len(), 4);
            for i in 0..4 {
                let e = *(*p).begin.add(i);
                assert_eq!(f32::from_bits((e >> 32) as u32), 12345.0);
            }
            ColorEffect::raw_delete(p);
        }
    }

    #[test]
    fn add_pkey_513_515_522_replace_no_clamp() {
        unsafe {
            let p = make_empty();
            for pkey in [513u32, 515, 520, 522] {
                (*p).add(pkey, -999.5);
                let e = *(*p).end.offset(-1);
                assert_eq!((e & 0xFFFF_FFFF) as u32, pkey);
                assert_eq!(f32::from_bits((e >> 32) as u32), -999.5);
            }
            ColorEffect::raw_delete(p);
        }
    }

    #[test]
    fn add_pkey_512_degree_normalize_and_force_key_512() {
        unsafe {
            let p = make_empty();
            // 정확히 in-range
            (*p).add(512, 180.0);
            let e0 = *(*p).begin;
            assert_eq!((e0 & 0xFFFF_FFFF) as u32, 512);
            assert_eq!(f32::from_bits((e0 >> 32) as u32), 180.0);

            // PKey 입력 무시 — but offset only PKey 512 reaches this branch via LUT
            // (PKey 526 같은 게 0x9b 가 아니므로 다른 branch)

            // -90° → 270°
            (*p).add(512, -90.0);
            let e1 = *(*p).begin.add(1);
            assert_eq!(f32::from_bits((e1 >> 32) as u32), 270.0);

            // 370° → 10°
            (*p).add(512, 370.0);
            let e2 = *(*p).begin.add(2);
            // raw 의 magic /360 이 정확히 1 quotient 반환
            let v = f32::from_bits((e2 >> 32) as u32);
            assert!((v - 10.0).abs() < 1e-3, "got {}", v);

            ColorEffect::raw_delete(p);
        }
    }

    #[test]
    fn add_pkey_514_clamps_to_neg16k_pos16k_and_forces_key_514() {
        unsafe {
            let p = make_empty();
            (*p).add(514, -20000.0); // → -16000
            let e0 = *(*p).begin;
            assert_eq!((e0 & 0xFFFF_FFFF) as u32, 514);
            assert_eq!(f32::from_bits((e0 >> 32) as u32), -16000.0);

            (*p).add(514, 16001.0); // → 16000
            let e1 = *(*p).begin.add(1);
            assert_eq!(f32::from_bits((e1 >> 32) as u32), 16000.0);

            (*p).add(514, 5000.0); // in-range
            let e2 = *(*p).begin.add(2);
            assert_eq!(f32::from_bits((e2 >> 32) as u32), 5000.0);

            ColorEffect::raw_delete(p);
        }
    }

    #[test]
    fn add_pkey_523_to_527_only_pushes_if_value_eq_1() {
        unsafe {
            let p = make_empty();
            // value != 1.0 → no-op
            (*p).add(523, 0.5);
            assert_eq!((*p).len(), 0);
            (*p).add(527, -1.0);
            assert_eq!((*p).len(), 0);
            (*p).add(525, 100.0);
            assert_eq!((*p).len(), 0);

            // value == 1.0 → push with key=PKey
            (*p).add(523, 1.0);
            (*p).add(524, 1.0);
            (*p).add(525, 1.0);
            (*p).add(526, 1.0);
            (*p).add(527, 1.0);
            assert_eq!((*p).len(), 5);
            for (i, expected_pkey) in [523u32, 524, 525, 526, 527].iter().enumerate() {
                let e = *(*p).begin.add(i);
                assert_eq!((e & 0xFFFF_FFFF) as u32, *expected_pkey);
                assert_eq!(f32::from_bits((e >> 32) as u32), 1.0);
            }
            ColorEffect::raw_delete(p);
        }
    }

    #[test]
    fn add_triggers_libcpp_growth() {
        // 빈 vector → push 1: alloc cap=1
        // push 2 (size=1, cap=1): doubled=2, req=2, new_cap=2
        // push 3 (size=2, cap=2): doubled=4, req=3, new_cap=4
        // push 4 (size=3, cap=4): fast path
        // push 5 (size=4, cap=4): doubled=8, req=5, new_cap=8
        unsafe {
            let p = make_empty();
            assert_eq!((*p).capacity(), 0);

            (*p).add(503, 1.0);
            assert_eq!((*p).len(), 1);
            assert_eq!((*p).capacity(), 1);

            (*p).add(503, 2.0);
            assert_eq!((*p).len(), 2);
            assert_eq!((*p).capacity(), 2);

            (*p).add(503, 3.0);
            assert_eq!((*p).len(), 3);
            assert_eq!((*p).capacity(), 4);

            (*p).add(503, 4.0);
            assert_eq!((*p).len(), 4);
            assert_eq!((*p).capacity(), 4); // fast path, no growth

            (*p).add(503, 5.0);
            assert_eq!((*p).len(), 5);
            assert_eq!((*p).capacity(), 8);

            ColorEffect::raw_delete(p);
        }
    }

    #[test]
    fn add_preserves_byte_pattern() {
        // 다양한 float 값에 대해 push 후 bit-pattern 보존 확인
        unsafe {
            let p = make_empty();
            let values: Vec<f32> = vec![0.0, 0.5, -0.5, 1.5, f32::INFINITY, -3.14];
            for v in &values {
                // PKey 503 default REPLACE (no clamp)
                (*p).add(503, *v);
            }
            for (i, v) in values.iter().enumerate() {
                let e = *(*p).begin.add(i);
                assert_eq!((e & 0xFFFF_FFFF) as u32, 503);
                assert_eq!((e >> 32) as u32, v.to_bits(), "mismatch at i={}", i);
            }
            ColorEffect::raw_delete(p);
        }
    }

    #[test]
    fn operator_eq_both_empty_returns_true() {
        unsafe {
            let a = make_empty();
            let b = make_empty();
            assert!((*a).operator_eq(&*b));
            ColorEffect::raw_delete(a);
            ColorEffect::raw_delete(b);
        }
    }

    #[test]
    fn operator_eq_one_empty_other_with_alpha_500_eq_1() {
        // empty fold = 1.0
        // [(500, 1.0)] fold: KEEP → acc=1.0; fminnm(1.0, 1.0) = 1.0
        // 같은 fold 지만 length 다름 → false
        unsafe {
            let a = make_empty();
            let b = make_empty();
            (*b).add(500, 1.0);

            assert!(!(*a).operator_eq(&*b));
            assert!(!(*b).operator_eq(&*a));

            ColorEffect::raw_delete(a);
            ColorEffect::raw_delete(b);
        }
    }

    #[test]
    fn operator_eq_identical_buffers() {
        unsafe {
            let a = make_empty();
            let b = make_empty();
            (*a).add(500, 0.5);
            (*a).add(501, 0.25);
            (*b).add(500, 0.5);
            (*b).add(501, 0.25);

            assert!((*a).operator_eq(&*b));
            assert!((*b).operator_eq(&*a));

            ColorEffect::raw_delete(a);
            ColorEffect::raw_delete(b);
        }
    }

    #[test]
    fn operator_eq_different_fold_returns_false() {
        // [(500, 0.5)] fold = 1.0 (KEEP)
        // [(503, 0.5)] fold = 0.5 (REPLACE)
        // fminnm(1.0, 1.0) = 1.0; fminnm(0.5, 1.0) = 0.5 → 다르므로 false
        unsafe {
            let a = make_empty();
            let b = make_empty();
            (*a).add(500, 0.5);
            (*b).add(503, 0.5);

            assert!(!(*a).operator_eq(&*b));
            ColorEffect::raw_delete(a);
            ColorEffect::raw_delete(b);
        }
    }

    #[test]
    fn operator_eq_same_fold_different_entries_returns_false() {
        // [(503, 0.5), (501, 0.5)] fold:
        //  iter 0: REPLACE → acc=0.5
        //  iter 1: MUL → acc = 0.5 * 0.5 = 0.25
        //  fminnm(0.25, 1.0) = 0.25
        // [(503, 0.25)] fold:
        //  iter 0: REPLACE → acc=0.25
        //  fminnm(0.25, 1.0) = 0.25
        // Same fold but element-wise diff → false
        unsafe {
            let a = make_empty();
            let b = make_empty();
            (*a).add(503, 0.5);
            (*a).add(501, 0.5);
            (*b).add(503, 0.25);

            assert!(!(*a).operator_eq(&*b));
            ColorEffect::raw_delete(a);
            ColorEffect::raw_delete(b);
        }
    }

    #[test]
    fn operator_eq_fold_clamped_to_1() {
        // [(501, 2.0)] fold: MUL → acc=2.0; fminnm(2.0, 1.0) = 1.0
        // [] fold: 1.0; fminnm(1.0, 1.0) = 1.0
        // Same fold, but lengths differ → false
        // 그래도 fold 가 same 임을 확인
        unsafe {
            let a = make_empty();
            // 501 alone (no clamp on input — PKey 501 in Add() clamps [-1,1])
            // 우회: 직접 buffer 조작 (raw Add 가 clamp 하지만 fold 자체 test 목적)
            let layout = Layout::from_size_align(8, 8).unwrap();
            let buf = std::alloc::alloc(layout) as *mut u64;
            let packed = 501u64 | ((2.0_f32.to_bits() as u64) << 32);
            *buf = packed;
            (*a).begin = buf;
            (*a).end = buf.add(1);
            (*a).cap_end = buf.add(1);

            // fold(a) = fminnm(MUL(1.0, 2.0), 1.0) = fminnm(2.0, 1.0) = 1.0
            assert_eq!(fold_alpha(&*a), 1.0);

            ColorEffect::raw_delete(a);
        }
    }

    #[test]
    fn clamp_raw_nan_preserved() {
        // raw 의 NaN 동작: b.mi 안 fire, b.le fire → keep
        let nan = f32::NAN;
        let r = clamp_raw(nan, 0.0, 1.0);
        assert!(r.is_nan(), "expected NaN, got {}", r);
    }

    #[test]
    fn clamp_raw_basic() {
        assert_eq!(clamp_raw(-0.5, 0.0, 1.0), 0.0);
        assert_eq!(clamp_raw(0.5, 0.0, 1.0), 0.5);
        assert_eq!(clamp_raw(1.5, 0.0, 1.0), 1.0);
        assert_eq!(clamp_raw(0.0, 0.0, 1.0), 0.0);
        assert_eq!(clamp_raw(1.0, 0.0, 1.0), 1.0);

        assert_eq!(clamp_raw(-2.0, -1.0, 1.0), -1.0);
        assert_eq!(clamp_raw(2.0, -1.0, 1.0), 1.0);
    }

    #[test]
    fn degree_normalize_in_range() {
        assert_eq!(degree_normalize(0.0), 0.0);
        assert_eq!(degree_normalize(180.0), 180.0);
        assert_eq!(degree_normalize(360.0), 360.0);
        assert_eq!(degree_normalize(123.456), 123.456);
    }

    #[test]
    fn degree_normalize_negative() {
        assert!((degree_normalize(-90.0) - 270.0).abs() < 1e-3);
        assert!((degree_normalize(-180.0) - 180.0).abs() < 1e-3);
    }

    #[test]
    fn degree_normalize_above_360() {
        let r1 = degree_normalize(370.0);
        assert!((r1 - 10.0).abs() < 1e-3, "370 → {}", r1);
        let r2 = degree_normalize(720.0);
        assert!(r2.abs() < 1e-3 || (r2 - 360.0).abs() < 1e-3, "720 → {}", r2);
    }

    #[test]
    fn fminnm_basic() {
        assert_eq!(fminnm(0.5, 1.0), 0.5);
        assert_eq!(fminnm(2.0, 1.0), 1.0);
        assert_eq!(fminnm(-3.0, 1.0), -3.0);
        // NaN one-operand
        assert_eq!(fminnm(f32::NAN, 1.0), 1.0);
        assert_eq!(fminnm(1.0, f32::NAN), 1.0);
        // Both NaN
        let r = fminnm(f32::NAN, f32::NAN);
        assert!(r.is_nan());
    }

    #[test]
    fn add_then_destruct_no_leak() {
        // miri / asan 으로 확인할 수 있도록 alloc/dealloc 짝 검증
        unsafe {
            let p = make_empty();
            for i in 0..20 {
                (*p).add(503, i as f32);
            }
            assert_eq!((*p).len(), 20);
            ColorEffect::raw_delete(p);
        }
    }

    #[test]
    fn multiple_clones_independent() {
        unsafe {
            let src = ColorEffect::create();
            let layout = Layout::from_size_align(8, 8).unwrap();
            let buf = std::alloc::alloc(layout) as *mut u64;
            *buf = 0xFACE_FEED_DEAD_BEEFu64;
            (*src).begin = buf;
            (*src).end = buf.add(1);
            (*src).cap_end = buf.add(1);

            let d1 = ColorEffect::clone_raw(src);
            let d2 = ColorEffect::clone_raw(src);
            let d3 = ColorEffect::clone_raw(d1);

            // 모두 다른 buffer
            assert_ne!((*d1).begin, (*d2).begin);
            assert_ne!((*d1).begin, (*d3).begin);
            assert_ne!((*src).begin, (*d1).begin);

            // 동일 byte 패턴
            assert_eq!(*((*d1).begin), 0xFACE_FEED_DEAD_BEEFu64);
            assert_eq!(*((*d2).begin), 0xFACE_FEED_DEAD_BEEFu64);
            assert_eq!(*((*d3).begin), 0xFACE_FEED_DEAD_BEEFu64);

            // 한 개 dropp 한 후에도 다른 것 영향 없음
            ColorEffect::raw_delete(d2);
            assert_eq!(*((*d1).begin), 0xFACE_FEED_DEAD_BEEFu64);

            ColorEffect::raw_delete(d1);
            ColorEffect::raw_delete(d3);
            ColorEffect::raw_delete(src);
        }
    }
}

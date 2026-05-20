//! `Hnc::Shape::Text::Composition` + LRComposition / TBComposition.
//!
//! ## 한컴 메모리 layout (0x68 = 104 bytes; ctor `FUN_002fe308` 132B — raw asm 검증)
//!
//! ```text
//! +0x00  vptr
//! +0x08  parent Glyph* (SharePtr inner)
//! +0x10  doubly-linked-list head sentinel  (= this+0x10)
//! +0x18  doubly-linked-list tail sentinel  (= this+0x10)
//! +0x20  u64 count
//! +0x28  Vec<RowSegment>::begin (각 elem 24B)
//! +0x30  Vec<RowSegment>::end
//! +0x38  Vec<RowSegment>::cap
//! +0x40  separator Glyph* (SharePtr inner)
//! +0x48  u32 direction (LR=0, TB=1)
//! +0x4c  f32 span
//! +0x50  i32 damage_begin (init = -1)
//! +0x54  i32 damage_end   (init = -1)
//! +0x58  u8  has_damage   (init = 1)
//! +0x59  u8  ?            (init = 1)
//! +0x5c  u32 iter_cache
//! +0x60  Compositor* (SharePtr inner)
//! ```
//!
//! ## 1:1 포팅 정책
//!
//! 각 method 의 body 는 raw asm + decompile 인용을 주석에 포함시키고 정공법 1:1 포팅.
//! 스텁 / 단축 / "후속 단계 TODO" 패턴 **금지**. 누락된 정보는 RE 로 먼저 확보.
//!
//! ## 진척
//!
//! - B-5a (이 commit): struct + Composition trait shell + ctor. 메소드 body 는 모두 후속.

use std::cell::Cell;

use crate::compose_layout::{composition_compose_glyph, Break};
use crate::compositor::Compositor;
use crate::glyph::{Glue, Glyph, MonoGlyph};
use crate::layout::{Align, Layout, Tile, TileReverse};
use crate::layout_factory::LayoutFactory;
use crate::placement::PlaceNatural;
use crate::value_types::{Allocation, Allotment, BreakType, Dimension, Extension, Requirement, Requisition};

// ============================================================
// CompositionDirection
// ============================================================

/// `Hnc::Shape::Text::Dimension` enum subset used by Composition.
///
/// 한컴 ctor: `*(undefined4 *)(this + 0x48) = 0;` (LR) or `... = 1;` (TB).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum CompositionDirection {
    /// Left-right (horizontal) — Korean prose default.
    LR = 0,
    /// Top-bottom (vertical) — 세로쓰기.
    TB = 1,
}

impl Default for CompositionDirection {
    fn default() -> Self { CompositionDirection::LR }
}

// ============================================================
// RowSegment — per-row metadata in the +0x28 vector
// ============================================================

/// One row of laid-out items in `Composition::+0x28` vector (24B per elem).
///
/// raw asm 검증 (Insert/Remove/Replace/Change/GetAllotment 의 lVar15+24, +0x4 offset):
/// ```text
/// +0x00  i32 begin
/// +0x04  i32 end
/// +0x08  f32 origin_x
/// +0x0c  f32 origin_y
/// +0x10  u64 flag      (bit 0 = valid, bit 1 = damaged)
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct RowSegment {
    pub begin: i32,
    pub end: i32,
    pub origin_x: f32,
    pub origin_y: f32,
    pub flag: u64,
}

impl RowSegment {
    pub const VALID_BIT: u64 = 0x1;
    pub const DAMAGED_BIT: u64 = 0x2;
}

// ============================================================
// CompositionState — 한컴 Composition 의 raw struct
// ============================================================

/// `Composition` 의 raw fields. Composition trait method 의 storage.
///
/// `Option<Box<dyn Glyph>>` 가 SharePtr<Glyph> 와 대응 (null = None). refcount 는
/// Rust ownership 으로 대체.
pub struct CompositionState {
    /// `+0x08` — parent Glyph (output container, MonoGlyph base 가 들고 있음).
    pub parent_glyph: Option<Box<dyn Glyph>>,

    /// `+0x10..+0x20` — doubly-linked-list of items. Vec 변환 시 algorithm 동등성
    /// 보존 검증 필요 (B-5c port 단계에서 raw asm 1:1 따라가며 검증).
    pub items: Vec<Option<Box<dyn Glyph>>>,

    /// `+0x28..+0x38` — Vec<RowSegment>.
    pub rows: Vec<RowSegment>,

    /// `+0x40` — separator Glyph.
    pub separator: Option<Box<dyn Glyph>>,

    /// `+0x48` — direction.
    pub direction: CompositionDirection,

    /// `+0x4c` — span (column width for LR / column height for TB).
    pub span: f32,

    /// `+0x50/+0x54` — damaged range [begin, end].
    pub damage_begin: i32,
    pub damage_end: i32,

    /// `+0x58` — has_damage 비트.
    pub has_damage: bool,

    /// `+0x59` — ctor / `SetSpan` 가 1 로, `View` 가 0 으로 set (raw: ctor·SetSpan 의
    /// `*(u16*)(this+0x58) = 0x101` 의 상위 byte, View 의 `strb wzr,[this,#0x59]`).
    /// 정확한 의미는 미파악 — `View` 가 한 차례 처리 후 clear 하므로 "View 처리 필요" flag 로
    /// 추정 (DoRepair/Repair 가 읽을 가능성).
    pub flag_59: bool,

    /// `+0x5c` — iter cache (GetIndexOf/Insert/Remove/Replace/Change/GetAllotment
    /// 모두 이 cache 기반 binary-walk).
    pub iter_cache: Cell<i32>,

    /// `+0x60` — bound Compositor.
    pub compositor: Option<Box<dyn Compositor>>,
}

impl std::fmt::Debug for CompositionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompositionState")
            .field("items_count", &self.items.len())
            .field("rows_count", &self.rows.len())
            .field("direction", &self.direction)
            .field("span", &self.span)
            .field("damage", &(self.damage_begin, self.damage_end))
            .field("has_damage", &self.has_damage)
            .field("flag_59", &self.flag_59)
            .field("iter_cache", &self.iter_cache.get())
            .field("has_separator", &self.separator.is_some())
            .field("has_parent_glyph", &self.parent_glyph.is_some())
            .field("has_compositor", &self.compositor.is_some())
            .finish()
    }
}

impl CompositionState {
    /// `Hnc::Shape::Text::Composition::Composition(parent, compositor, separator, dim, float)`
    /// `FUN_002fe308` sz=132 — raw decompile 1:1:
    ///
    /// ```c
    /// // *(this + 8) = parent_glyph (refcount++)
    /// // vptr = pure_virtual (subclass override 가 swap)
    /// // *(this + 0x10) = this + 0x10; *(this + 0x18) = this + 0x10;  // list sentinel self-loop
    /// // *(this + 0x20) = 0;            // count
    /// // *(this + 0x28..0x38) = 0;      // rows vector empty
    /// // *(this + 0x40) = separator (refcount++)
    /// // *(this + 0x48) = direction enum
    /// // *(this + 0x4c) = float (span)
    /// // *(this + 0x50) = 0xff..ff;     // damage_begin = damage_end = -1
    /// // *(u16*)(this + 0x58) = 0x101;  // has_damage byte=1, +0x59 byte=1
    /// // *(this + 0x5c) = 0;            // iter_cache
    /// // *(this + 0x60) = compositor (refcount++)
    /// ```
    pub fn new(
        parent_glyph: Option<Box<dyn Glyph>>,
        compositor: Option<Box<dyn Compositor>>,
        separator: Option<Box<dyn Glyph>>,
        direction: CompositionDirection,
        span: f32,
    ) -> Self {
        Self {
            parent_glyph,
            items: Vec::new(),
            rows: Vec::new(),
            separator,
            direction,
            span,
            damage_begin: -1,
            damage_end: -1,
            has_damage: true,
            // raw: *(u16*)(this+0x58) = 0x101 → +0x58, +0x59 둘 다 1.
            flag_59: true,
            iter_cache: Cell::new(0),
            compositor,
        }
    }

    // -- Simple state accessors (raw asm 검증 완료) --------------

    /// `Composition::GetCount() -> usize` — `FUN_002ffbe8` sz=8.
    ///
    /// raw asm: `ldr x0, [x0, #0x20]; ret` — 단순히 `*(this+0x20)` 반환.
    /// 한컴 list count 는 +0x20. Rust 에서는 `items.len()` 로 매핑 (linked list 의
    /// count field 와 Vec.len() 둘 다 O(1)).
    pub fn get_count(&self) -> usize { self.items.len() }

    /// `Composition::GetComponent(unsigned long idx) const` — `FUN_002ffbf0` sz=228.
    ///
    /// raw decompile (한컴 doubly-linked list walk):
    /// ```c
    /// if (idx >= count) throw std::out_of_range("GetAt");
    /// node = list_head_sentinel;
    /// if (idx < 0) {
    ///     // walk via *node (prev) until idx wraps to 0 — practically: dead branch
    ///     // (Insert 의 `idx < count` 검사 unsigned 라 negative 는 else branch 로 가니까
    ///     //  GetComponent 의 negative 도 사실상 throw 위에서 잡힘 — but C++ 는 unsigned 라
    ///     //  caller 가 -1 → 0xff..ff 줘서 throw.)
    /// } else if (idx != 0) {
    ///     for i in 0..idx: node = node.next;
    /// }
    /// item = node.item;  // node[2] = SharePtr inner = Glyph*
    /// item.refcount++;
    /// return item;
    /// ```
    ///
    /// Rust: Vec 의 idx 직접 indexing. out-of-range 는 panic (한컴 `std::out_of_range`).
    pub fn get_component(&self, idx: usize) -> Option<&dyn Glyph> {
        if idx >= self.items.len() {
            panic!(
                "Composition::GetComponent — out_of_range \"GetAt\" (idx={}, count={})",
                idx,
                self.items.len()
            );
        }
        self.items[idx].as_deref()
    }

    /// mutable variant — 한컴 raw 에는 const 만 있으나 Insert/Remove/Replace/Change 등에서
    /// row 의 line container 에 mutating sub-op 호출 시 필요.
    pub fn get_component_mut(&mut self, idx: usize) -> Option<&mut dyn Glyph> {
        if idx >= self.items.len() {
            panic!(
                "Composition::GetComponent — out_of_range \"GetAt\" (idx={}, count={})",
                idx,
                self.items.len()
            );
        }
        self.items[idx].as_deref_mut()
    }

    /// `Composition::GetSpan() -> f32` — `FUN_002fe778` sz=8.
    ///
    /// raw asm: `ldr w0, [x0, #0x4c]; ret`.
    pub fn get_span(&self) -> f32 { self.span }

    /// `Composition::SetSpan(float)` — `FUN_002fe780` sz=28.
    ///
    /// raw decompile:
    /// ```c
    /// if (this->span != param_1) {
    ///     this->span = param_1;
    ///     *(u16*)(this + 0x58) = 0x101;   // has_damage byte = 1, +0x59 byte = 1
    /// }
    /// ```
    /// +0x59 는 미파악 비트 (initial 0x101 of ctor). 본 함수는 byte 단위로 둘 다 1
    /// 로 set — 우리는 `has_damage = true` 만 추적 (다른 byte 가 layout 결과에 영향 시
    /// 별도 field 추가 검토).
    pub fn set_span(&mut self, span: f32) {
        if self.span != span {
            self.span = span;
            // raw: *(u16*)(this+0x58) = 0x101 → has_damage 와 flag_59 둘 다 1.
            self.has_damage = true;
            self.flag_59 = true;
        }
    }

    /// `Composition::SetDamage(int begin, int end)` — `FUN_002fe744` sz=52.
    ///
    /// raw asm 검증:
    /// ```asm
    /// ldrb w8, [x0, #0x58]          ; w8 = has_damage byte
    /// cbz w8, NO_DAMAGE_YET         ; if has_damage == 0, jump
    /// ldp w8, w9, [x0, #0x50]       ; w8 = damage_begin, w9 = damage_end
    /// cmp w8, w1; csel w8, w8, w1, lt   ; w8 = min(damage_begin, begin)
    /// cmp w9, w2; csel w9, w9, w2, gt   ; w9 = max(damage_end, end)
    /// stp w8, w9, [x0, #0x50]
    /// ret
    /// NO_DAMAGE_YET:
    /// stp w1, w2, [x0, #0x50]       ; damage_begin = begin, damage_end = end
    /// mov w8, #0x1
    /// strb w8, [x0, #0x58]          ; has_damage = 1
    /// ret
    /// ```
    pub fn set_damage(&mut self, begin: i32, end: i32) {
        if self.has_damage {
            self.damage_begin = self.damage_begin.min(begin);
            self.damage_end = self.damage_end.max(end);
        } else {
            self.damage_begin = begin;
            self.damage_end = end;
            self.has_damage = true;
        }
    }

    // -- Row-segment indexing (raw asm 검증 완료) ----------------

    /// `Composition::GetBeginOf(int line_idx) -> i32` — `FUN_002fe5f4` sz=68.
    ///
    /// 한컴 외부 API 는 line_idx = `row_idx * 2` 로 받음 (한 row 가 짝수 boundary 2 개를
    /// 차지하는 시스템).
    ///
    /// raw decompile:
    /// ```c
    /// if (line_idx < 0) line_idx = line_idx + 1;     // signed adj
    /// row_count = (this+0x30 - this+0x28) / 24;
    /// last_row = row_count - 1;
    /// row_idx = line_idx >> 1;                        // arithmetic right shift (signed)
    /// if (row_idx > last_row) row_idx = last_row;     // clamp upper
    /// row_idx = max(row_idx, 0);                      // clamp lower (BIC w/ ASR)
    /// return *(int*)(rows_begin + row_idx * 24);     // rows[row_idx].begin
    /// ```
    ///
    /// edge: row_count == 0 → last_row = -1. clamp 후 row_idx = 0. C++ raw 는 `rows[0]`
    /// 를 invalid memory 에서 읽음 (UB). 우리는 0 반환 (safety; caller 가 empty rows 에서
    /// 호출하는 case 없음 — Repair 후에만 호출).
    pub fn get_begin_of(&self, line_idx: i32) -> i32 {
        if self.rows.is_empty() {
            return 0;
        }
        let adj = if line_idx < 0 { line_idx + 1 } else { line_idx };
        let row_idx = adj >> 1;
        let last_row = (self.rows.len() as i32) - 1;
        let clamped = row_idx.min(last_row).max(0);
        self.rows[clamped as usize].begin
    }

    /// `Composition::GetEndOf(int line_idx) -> i32` — `FUN_002fe638` sz=68.
    /// GetBeginOf 와 byte-identical 패턴, 다만 `.end` (+0x4) 반환.
    pub fn get_end_of(&self, line_idx: i32) -> i32 {
        if self.rows.is_empty() {
            return 0;
        }
        let adj = if line_idx < 0 { line_idx + 1 } else { line_idx };
        let row_idx = adj >> 1;
        let last_row = (self.rows.len() as i32) - 1;
        let clamped = row_idx.min(last_row).max(0);
        self.rows[clamped as usize].end
    }

    /// `Composition::GetIndexOf(int char_idx) -> i32` — `FUN_002fe548` sz=172.
    ///
    /// 한 char_idx 가 어느 row 에 속하는지 binary-walk. iter_cache 가 hint 로 사용됨 +
    /// 결과로 갱신. 반환값은 `row_idx * 2` (한컴 line index convention).
    ///
    /// raw asm 알고리즘 (1:1):
    /// ```text
    /// row_count = (rows_end - rows_begin) / 24
    /// last_row = row_count - 1                        (i32 ; rows 비어 있으면 -1)
    /// cache_pos = max(iter_cache, 0)                  (BIC w/ ASR 31)
    /// idx = min(last_row, cache_pos)                  (CSEL lt)
    ///
    /// // Forward walk (last_row > cache_pos 일 때만)
    /// if last_row > cache_pos:
    ///     loop_count = last_row - cache_pos           (= ~cache + row_count)
    ///     for _ in 0..loop_count:
    ///         if rows[idx].end >= char_idx: break
    ///         idx += 1
    ///     else:
    ///         idx = last_row                          (안 찾았으면 last 로 clamp)
    /// iter_cache = idx
    ///
    /// // Backward walk (idx >= 1 일 때만)
    /// if idx >= 1:
    ///     loop:
    ///         if rows[idx].begin <= char_idx: break
    ///         idx -= 1
    ///         iter_cache = idx
    ///         if idx == 0: idx = 0; break             (clamp + 종료)
    ///
    /// return max(idx, 0) << 1
    /// ```
    pub fn get_index_of(&self, char_idx: i32) -> i32 {
        let row_count = self.rows.len() as i32;
        let last_row = row_count - 1; // if row_count==0, -1
        let cache_raw = self.iter_cache.get();
        let cache_pos = if cache_raw < 0 { 0 } else { cache_raw };

        // start_idx = min(last_row, cache_pos)
        let mut idx: i32 = if last_row < cache_pos { last_row } else { cache_pos };

        // Forward walk only when last_row > cache_pos (asm: b.le SKIP_FORWARD)
        if last_row > cache_pos {
            let loop_count = last_row - cache_pos;
            let mut found = false;
            let mut remaining = loop_count;
            while remaining > 0 {
                if (idx as usize) >= self.rows.len() {
                    break;
                }
                if self.rows[idx as usize].end >= char_idx {
                    found = true;
                    break;
                }
                idx += 1;
                remaining -= 1;
            }
            if !found {
                idx = last_row;
            }
        }
        self.iter_cache.set(idx);

        // Backward walk only when idx >= 1
        if idx >= 1 {
            loop {
                if (idx as usize) >= self.rows.len() {
                    break;
                }
                if self.rows[idx as usize].begin <= char_idx {
                    break;
                }
                idx -= 1;
                self.iter_cache.set(idx);
                if idx <= 0 {
                    idx = 0;
                    break;
                }
            }
        }

        let final_idx = if idx < 0 { 0 } else { idx };
        final_idx << 1
    }

    // -- vfunc forwards (parent_glyph 로의 dispatch) ----------

    /// `Composition::Allocate(Allocation const& alloc, Extension& ext)` —
    /// `FUN_002fe7bc` sz=32.
    ///
    /// raw asm (full 8 instructions):
    /// ```asm
    /// ldr x8, [x0, #0x8]      ; x8 = SharePtr* @ this+8
    /// cbz x8, ret
    /// ldr x0, [x8]            ; x0 = SharePtr.inner_ptr = Glyph*
    /// cbz x0, ret
    /// ldr x8, [x0]            ; x8 = Glyph.vptr
    /// ldr x3, [x8, #0x20]     ; x3 = vfunc[4] = Allocate
    /// br x3                   ; tail call (x1=&alloc, x2=&ext preserved)
    /// ret
    /// ```
    /// parent_glyph 의 vfunc[4] (Allocate) 로 tail-call. Rust: 직접 trait method.
    pub fn allocate(&mut self, alloc: &Allocation, ext: &mut Extension) {
        if let Some(parent) = self.parent_glyph.as_mut() {
            parent.allocate(alloc, ext);
        }
    }

    /// `Composition::Request(Allocation const& avail, BoundsRect& out_bounds)` —
    /// `FUN_002fe79c` sz=32.
    ///
    /// raw asm (full 8 instructions): tail call to parent.vfunc[3] (Request) with
    /// x1 (req_out) preserved.
    ///
    /// **시그니처 정정** (raw vtable dump + Glyph trait audit 후):
    /// vfunc[3] = `Request(Requisition&)` (단일 인자). MonoGlyph::Request /
    /// CharItemView::Request / ItemView::Request 의 raw decompile 시그니처와 일치.
    ///
    /// 한컴 raw 의 "Glue::Request" Ghidra 라벨은 vfunc[4] (Allocate w/ Allocation+Extension)
    /// 에 mis-name 된 것 — 본 method 는 그게 아닌 vfunc[3] forward.
    pub fn request(&self, req_out: &mut Requisition) {
        if let Some(parent) = self.parent_glyph.as_ref() {
            parent.request(req_out);
        }
    }

    /// `Composition::SetMargin(int line_idx, float margin_a, float margin_b)` —
    /// `FUN_002fe67c` sz=200.
    ///
    /// 한컴 quirk (raw asm 검증): 메소드 이름은 SetMargin 이지만 실제로 row 의
    /// `origin_x`/`origin_y` 를 row struct 에 **갱신하지 않음**. 동작:
    /// 1. odd line_idx → no-op
    /// 2. row_idx = line_idx >> 1 (signed); out of range → no-op
    /// 3. row 의 (begin, end, origin_x, origin_y, flag) 을 stack 으로 snapshot
    /// 4. stack copy 의 origin_x == margin_a && origin_y == margin_b 이면 no-op
    ///    (NaN 비교는 ARM64 fcmp+fccmp(eq) 패턴 — NaN 시 b.eq not taken → update 진입)
    /// 5. 그 외:
    ///    - stack copy 의 origin_x = margin_a, origin_y = margin_b
    ///    - stack copy 의 flag &= ~0x2 (clear DAMAGED bit)
    ///    - composition damage range 에 (row.begin - 1, row.end + 1) merge
    ///    - stack copy 의 flag destructor 호출 (FUN_006b3bdc = `Hnc::Type::Flag::~Flag`)
    ///
    /// **row 가 실제로 바뀌지 않는다** — caller 가 SetMargin 후 Allocate/Repair 호출 시
    /// damage range 를 보고 행동하는 패턴 (이건 함수 이름과 동작 불일치하는 한컴 quirk).
    pub fn set_margin(&mut self, line_idx: i32, margin_a: f32, margin_b: f32) {
        // 한컴 asm `tbnz w1, #0x0, RET` — LSB set (odd) early return
        if (line_idx & 1) != 0 {
            return;
        }
        // 한컴 `cinc w8, w1, lt` — if < 0, w8 = w1 + 1; else w8 = w1
        let adj = if line_idx < 0 { line_idx + 1 } else { line_idx };
        let row_idx = adj >> 1; // arithmetic right shift (signed)
        let row_count = self.rows.len() as i32;
        if row_idx >= row_count || row_idx < 0 {
            return;
        }
        // snapshot row (RowSegment is Copy)
        let row = self.rows[row_idx as usize];
        // f32 `==` 의 NaN 처리 == 한컴 fcmp+fccmp(eq) 패턴 (NaN 시 결과 false → update path)
        let unchanged = row.origin_x == margin_a && row.origin_y == margin_b;
        if unchanged {
            return;
        }
        // 한컴 quirk: row 의 origin/flag 를 실제로 갱신하지 않음. 다만 composition damage 만.
        let begin_m1 = row.begin - 1;
        let end_p1 = row.end + 1;
        if self.has_damage {
            self.damage_begin = self.damage_begin.min(begin_m1);
            self.damage_end = self.damage_end.max(end_p1);
        } else {
            self.damage_begin = begin_m1;
            self.damage_end = end_p1;
            self.has_damage = true;
        }
        // 한컴은 stack copy 의 flag destructor 를 호출하지만 우리는 stack copy 자체가
        // Rust `let row = ...` 의 scope-bound copy 라서 자동으로 drop — 동등.
    }

    // -- list operations 1:1 (한컴 raw asm 검증) -----------

    /// row 검색 helper — Insert / Remove / Replace / Change / GetAllotment 가 공유하는
    /// iter_cache 기반 binary walk. 반환: clamped start_idx (`[0, row_count]`).
    ///
    /// raw asm: `0x2fe91c..0x2fe9dc` (Insert), 동일 패턴이 Remove / Replace / Change /
    /// GetAllotment 에 반복. 일관성을 위해 별도 helper 로 추출 — 한컴 raw 동작과 동등성
    /// 유지하기 위해 forward walk + iter_cache update + backward walk + clamp 모두 1:1.
    fn search_row_idx(&self, char_idx: i32) -> i32 {
        let row_count = self.rows.len() as i32;
        let last_row = row_count - 1;
        let cache_raw = self.iter_cache.get();
        let cache_pos = if cache_raw < 0 { 0 } else { cache_raw };
        let mut idx: i32 = if last_row < cache_pos { last_row } else { cache_pos };

        if last_row > cache_pos {
            let loop_count = last_row - cache_pos;
            let mut remaining = loop_count;
            let mut found = false;
            while remaining > 0 {
                if (idx as usize) >= self.rows.len() {
                    break;
                }
                if self.rows[idx as usize].end >= char_idx {
                    found = true;
                    break;
                }
                idx += 1;
                remaining -= 1;
            }
            if !found {
                idx = last_row;
            }
        }
        self.iter_cache.set(idx);

        if idx >= 1 {
            loop {
                if (idx as usize) >= self.rows.len() {
                    break;
                }
                if self.rows[idx as usize].begin <= char_idx {
                    break;
                }
                idx -= 1;
                self.iter_cache.set(idx);
                if idx <= 0 {
                    idx = 0;
                    break;
                }
            }
        }
        if idx < 0 {
            0
        } else {
            idx
        }
    }

    /// `Composition::Insert(unsigned long idx, SharePtr<Glyph> const&)` —
    /// `FUN_002fe804` sz=936.
    ///
    /// raw asm 전체 1:1:
    /// 1. List insertion:
    ///    - `cmp count, idx; b.ls APPEND` — unsigned `count <= idx` → append branch (else insert)
    ///    - operator_new(0x18) = 24B linked-list node (prev, next, item)
    ///    - item refcount inc
    ///    - link into doubly-linked list (sentinel at this+0x10)
    ///    - count++
    ///    Rust: `Vec::insert(idx, child)` for idx < count else `push` (algorithm 동등).
    ///
    /// 2. parent_glyph propagation (parent_glyph non-null + Glyph valid):
    ///    - row 검색 via search_row_idx
    ///    - for each row in [final_idx..row_count):
    ///        - if valid && begin <= idx <= end+1 (idx 가 row 에 포함):
    ///            - row.flag &= ~0x2 (clear DAMAGED bit on stack copy)
    ///            - sub = parent.GetComponent(row_line_idx)  (vfunc[17], +0x88)
    ///            - sub.Insert(idx + 3 - row.begin, null SharePtr)  (vfunc[12], +0x60)
    ///            - parent.Change(row_line_idx)  (vfunc[15], +0x78)
    ///        - if idx < begin: begin++
    ///        - if idx <= end+1: end++
    ///        - write back row
    ///
    /// 3. damage range = (idx-1, idx+1) merge.
    pub fn insert(&mut self, idx: usize, child: Option<Box<dyn Glyph>>) {
        // Step 1: items list insert
        let count = self.items.len();
        let idx_i32 = idx as i32;
        if idx < count {
            self.items.insert(idx, child);
        } else {
            self.items.push(child);
        }

        // Step 2: parent_glyph propagation + row shifts
        let has_parent = self.parent_glyph.is_some();
        if has_parent {
            let row_count = self.rows.len() as i32;
            let final_idx_initial = self.search_row_idx(idx_i32);

            if final_idx_initial < row_count {
                let mut i = final_idx_initial;
                while (i as usize) < self.rows.len() {
                    let row_line_idx = i * 2;
                    // Snapshot row
                    let mut row = self.rows[i as usize];

                    let valid = (row.flag & RowSegment::VALID_BIT) != 0;
                    let in_row = row.begin <= idx_i32 && idx_i32 <= row.end + 1;
                    if valid && in_row {
                        // Clear DAMAGED bit on stack copy
                        row.flag &= !RowSegment::DAMAGED_BIT;
                        let char_offset = (idx_i32 + 3 - row.begin) as usize;
                        // parent.GetComponent(row_line_idx).Insert(char_offset, null)
                        // + parent.Change(row_line_idx)
                        if let Some(parent) = self.parent_glyph.as_mut() {
                            if let Some(sub) = parent.get_component_mut(row_line_idx as usize) {
                                sub.insert(char_offset, None);
                            }
                            parent.change(row_line_idx as usize);
                        }
                    }
                    // Row shifts (always applied — 한컴 asm 0x2feaf4~0x2feb14)
                    if idx_i32 < row.begin {
                        row.begin += 1;
                    }
                    if idx_i32 <= row.end + 1 {
                        row.end += 1;
                    }
                    // Write back to vector
                    self.rows[i as usize] = row;
                    i += 1;
                }
            }
        }

        // Step 3: damage range (한컴 asm 0x2feb1c..0x2feb58)
        self.set_damage(idx_i32 - 1, idx_i32 + 1);
    }

    /// `Composition::Append(SharePtr<Glyph> const&)` — `FUN_002fe7dc` sz=20.
    ///
    /// raw asm (full):
    /// ```asm
    /// mov x2, x1                ; x2 = item
    /// ldr x1, [x0, #0x20]       ; x1 = count (Insert's idx)
    /// ldr x8, [x0]              ; vptr
    /// ldr x3, [x8, #0x60]       ; vfunc[12] = Insert
    /// br x3                     ; tail call this->Insert(count, item)
    /// ```
    /// virtual dispatch of Insert. LR/TB 가 Insert 를 override 하지 않으므로 base = Insert.
    pub fn append(&mut self, child: Option<Box<dyn Glyph>>) {
        let count = self.items.len();
        self.insert(count, child);
    }

    /// `Composition::Prepend(SharePtr<Glyph> const&)` — `FUN_002fe7f0` sz=20.
    /// raw asm: `mov x2, x1; mov x1, #0; ldr x3, [vptr+0x60]; br x3` — Insert(0, item).
    pub fn prepend(&mut self, child: Option<Box<dyn Glyph>>) {
        self.insert(0, child);
    }

    /// `Composition::Remove(unsigned long idx)` — `FUN_002febf4` sz=660.
    ///
    /// raw asm 1:1:
    /// 1. **List erase**: `FUN_002feea4(this + 0x10, idx)` — doubly-linked list node erase
    ///    at position idx (`std::list::erase` 동등). Rust: `Vec::remove(idx)`.
    /// 2. **parent_glyph propagation** (parent_glyph + Glyph 가 valid 일 때):
    ///    - row 검색 via search_row_idx
    ///    - for each row in `[final_idx..row_count)`:
    ///        - if valid && begin <= idx <= end+1: clear DAMAGED + sub.Remove(idx+3-begin) +
    ///          parent.Change(row_line_idx)
    ///        - if idx < begin: begin-- (row shift left)
    ///        - if idx <= end+1: end-- (row size shrinks)
    ///        - write back row
    /// 3. **damage range** = (idx-1, idx) merge. (Insert 와 달리 +1 이 아닌 idx.)
    pub fn remove(&mut self, idx: usize) {
        let idx_i32 = idx as i32;

        // Step 1: FUN_002feea4 = list node erase (with out-of-range throw)
        // 한컴 raw 는 idx >= count 시 std::out_of_range("Remove") throw — 우리도 panic.
        if idx >= self.items.len() {
            panic!(
                "Composition::Remove — out_of_range \"Remove\" (idx={}, count={})",
                idx,
                self.items.len()
            );
        }
        self.items.remove(idx);

        // Step 2: parent_glyph propagation
        let has_parent = self.parent_glyph.is_some();
        if has_parent {
            let row_count = self.rows.len() as i32;
            let final_idx_initial = self.search_row_idx(idx_i32);
            if final_idx_initial < row_count {
                let mut i = final_idx_initial;
                while (i as usize) < self.rows.len() {
                    let row_line_idx = i * 2;
                    let mut row = self.rows[i as usize];

                    let valid = (row.flag & RowSegment::VALID_BIT) != 0;
                    let in_row = row.begin <= idx_i32 && idx_i32 <= row.end + 1;
                    if valid && in_row {
                        row.flag &= !RowSegment::DAMAGED_BIT;
                        let char_offset = (idx_i32 + 3 - row.begin) as usize;
                        if let Some(parent) = self.parent_glyph.as_mut() {
                            if let Some(sub) = parent.get_component_mut(row_line_idx as usize) {
                                sub.remove(char_offset);
                            }
                            parent.change(row_line_idx as usize);
                        }
                    }
                    // Row shifts (DECREMENT for Remove):
                    if idx_i32 < row.begin {
                        row.begin -= 1;
                    }
                    if idx_i32 <= row.end + 1 {
                        row.end -= 1;
                    }
                    self.rows[i as usize] = row;
                    i += 1;
                }
            }
        }

        // Step 3: damage range (Remove 는 (idx-1, idx))
        self.set_damage(idx_i32 - 1, idx_i32);
    }

    /// `Composition::Replace(unsigned long idx, SharePtr<Glyph> const&)` —
    /// `FUN_002fef98` sz=1844.
    ///
    /// raw decompile + asm 1:1 (Glyph trait audit 후 시그니처 확정):
    ///
    /// 1. **idx out_of_range check** (한컴 asm 0x2ff020..028, `count <= idx` → throw).
    /// 2. **new_item.Request(&new_req)** — vfunc[3], 36B Requisition 측정.
    ///    - 한컴 init: X.Requirement = (DAT_00741f20 패턴 = (0,0,0,0)), Y.natural = -1e8, rest 0.
    /// 3. items[idx] 의 **old_item.Request(&old_req)** — 동일 init + vfunc[3].
    /// 4. **parent_glyph propagation** (parent_glyph non-null && inner valid):
    ///    - search_row_idx 로 시작 row 찾기 (iter_cache 사용)
    ///    - `for each row in [final_idx..row_count)`:
    ///        - snapshot row (begin/end/origin/flag)
    ///        - **if valid && (begin-1) <= idx <= (end+1)**: (±1 확장된 in-row 조건)
    ///            - **8 f32 bounds 비교**: new_req 와 old_req 의
    ///              X.natural / X.stretch / X.shrink / X.alignment /
    ///              Y.natural / Y.stretch / Y.shrink / Y.alignment 비교.
    ///              **하나라도 ABS >= 0.1 → DAMAGE 분기**.
    ///            - **DAMAGE**: row.flag &= ~0x2 (clear DAMAGED), composition damage range
    ///              merge `(idx-1, idx+1)`. replacement = null.
    ///            - **NO DAMAGE**: new_item non-null 이면 `ComposeGlyph(new_item, bt)` 호출 —
    ///              bt = (begin-1 == idx ? Penalty(3) : end+1 == idx ? Hint(1) : Normal(0)).
    ///              결과를 replacement 로 사용. null 이면 null.
    ///            - `sub = parent.GetComponent(row_line_idx)` (vfunc[17], +0x88)
    ///            - `sub.Replace(idx + 3 - row.begin, replacement)` (vfunc[14], +0x70)
    ///        - `parent.Change(row_line_idx)` (vfunc[15], +0x78) — 모든 path 에서 호출
    ///        - write back row (flag만 변경 가능, begin/end/origin 유지)
    ///        - **early exit**: `while (begin <= idx && remaining != 0)`
    /// 5. items list 자체는 **갱신하지 않음** — Composition::Replace 는 sub-layer
    ///    propagation 만. items list 의 element swap 은 caller 책임.
    ///    (한컴 raw asm `0x2ff060..0x2ff088` 가 node.item 을 단지 READ — node[+0x10] 에
    ///    write 하는 instruction 없음.)
    pub fn replace(&mut self, idx: usize, child: Option<Box<dyn Glyph>>) {
        use crate::compose_layout::composition_compose_glyph;
        use crate::value_types::{BreakType, Requirement};

        let idx_i32 = idx as i32;

        // Step 1: out_of_range check
        if idx >= self.items.len() {
            panic!(
                "Composition::Replace — out_of_range \"GetAt\" (idx={}, count={})",
                idx,
                self.items.len()
            );
        }

        // Step 2: 한컴 init pattern — INVALID_NATURAL Requirements, penalty=0
        // 한컴 raw: DAT_00741f20 (4 floats = 0,0,0,0) → X.Requirement
        //          Y.natural = -1e8, Y.stretch=Y.shrink=Y.alignment=0, penalty=0
        // (raw 0x2fefc8 의 `ldr q0, [x9, #0xf20]; stur q0, [x29, -0x90]` 16B 0 + 그 다음 16B init)
        let make_init_req = || {
            let mut r = Requisition::default();
            r.set_x(Requirement::new(0.0, 0.0, 0.0, 0.0));
            r.set_y(Requirement::new(Requirement::INVALID_NATURAL, 0.0, 0.0, 0.0));
            r.set_penalty(0);
            r
        };

        // Step 2a: new_item.Request(&new_req)
        let mut new_req = make_init_req();
        if let Some(new_g) = child.as_deref() {
            new_g.request(&mut new_req);
        }

        // Step 2b: old_item.Request(&old_req)
        let mut old_req = make_init_req();
        if let Some(old_g) = self.items[idx].as_deref() {
            old_g.request(&mut old_req);
        }

        // Step 3: parent_glyph propagation (parent_glyph non-null)
        let has_parent = self.parent_glyph.is_some();
        if !has_parent {
            // 한컴 raw: parent_glyph null 이면 propagation skip + damage 도 갱신 안 함
            // (Replace 의 damage 갱신은 loop body 안에서만 발생)
            return;
        }
        let row_count = self.rows.len() as i32;
        let final_idx_initial = self.search_row_idx(idx_i32);
        if final_idx_initial >= row_count {
            return;
        }

        // 8 f32 비교: ABS(new.field - old.field) >= 0.1 → DAMAGE
        let bounds_differ = {
            let nx = new_req.get_x();
            let ox = old_req.get_x();
            let ny = new_req.get_y();
            let oy = old_req.get_y();
            (nx.natural - ox.natural).abs() >= 0.1
                || (nx.stretch - ox.stretch).abs() >= 0.1
                || (nx.shrink - ox.shrink).abs() >= 0.1
                || (nx.alignment - ox.alignment).abs() >= 0.1
                || (ny.natural - oy.natural).abs() >= 0.1
                || (ny.stretch - oy.stretch).abs() >= 0.1
                || (ny.shrink - oy.shrink).abs() >= 0.1
                || (ny.alignment - oy.alignment).abs() >= 0.1
        };

        // damage 범위는 한컴 asm 0x2ff168 (loop 진입 전 set):
        //   iVar21 = idx - 1 (damage_begin if first damage)
        //   iVar13 = idx + 1 (damage_end if first damage)
        let dmg_begin = idx_i32 - 1;
        let dmg_end = idx_i32 + 1;

        let mut i = final_idx_initial;
        while (i as usize) < self.rows.len() {
            let row_line_idx = i * 2;
            let mut row = self.rows[i as usize]; // snapshot

            let valid = (row.flag & RowSegment::VALID_BIT) != 0;
            let in_extended_row =
                (row.begin - 1) <= idx_i32 && idx_i32 <= (row.end + 1);

            if valid && in_extended_row {
                let replacement: Option<Box<dyn Glyph>>;
                if bounds_differ {
                    // DAMAGE path (한컴 LAB_002ff37c)
                    row.flag &= !RowSegment::DAMAGED_BIT;
                    if self.has_damage {
                        self.damage_begin = self.damage_begin.min(dmg_begin);
                        self.damage_end = self.damage_end.max(dmg_end);
                    } else {
                        self.damage_begin = dmg_begin;
                        self.damage_end = dmg_end;
                        self.has_damage = true;
                    }
                    replacement = None; // sub.Replace 에 null SharePtr 전달
                } else {
                    // NO DAMAGE path (한컴 ELSE branch): ComposeGlyph 호출 → replacement glyph
                    replacement = if let Some(new_g) = child.as_deref() {
                        let bt = if (row.begin - 1) == idx_i32 {
                            BreakType::Penalty
                        } else if (row.end + 1) == idx_i32 {
                            BreakType::Hint
                        } else {
                            BreakType::Normal
                        };
                        composition_compose_glyph(new_g, bt)
                    } else {
                        None
                    };
                }

                // sub.Replace(char_offset, replacement) (한컴 LAB_002ff598)
                let char_offset = (idx_i32 + 3 - row.begin) as usize;
                if let Some(parent) = self.parent_glyph.as_mut() {
                    if let Some(sub) = parent.get_component_mut(row_line_idx as usize) {
                        sub.replace(char_offset, replacement);
                    }
                    // parent.Change(row_line_idx) — 모든 path 공통 (한컴 asm 0x2ff338)
                    parent.change(row_line_idx as usize);
                }
            }

            // Row write-back (flag 변경 가능, begin/end/origin 그대로)
            self.rows[i as usize] = row;

            // Early exit: while (begin <= idx && remaining != 0)
            // 한컴 asm 0x2ff352: `while (iVar5 <= iVar22 && bVar8)` — iVar5=row.begin (snapshot)
            let is_last_row = (i + 1) >= row_count;
            if row.begin > idx_i32 || is_last_row {
                break;
            }
            i += 1;
        }
    }

    /// `Composition::Change(unsigned long idx)` — `FUN_002ff9b4` sz=536.
    ///
    /// raw asm 1:1:
    /// - parent_glyph null 이면 no-op (한컴 0x2ff9dc: cbz x19, RET).
    /// - parent_sp refcount++, inner null 이면 dec + RET (skip).
    /// - row 검색 (search_row_idx).
    /// - **EARLY EXIT loop** (Insert/Remove 와 다름): row 의 row shifts 하지 않고, 첫
    ///   affected row 까지만 propagate 후 종료:
    ///     - for each row in [final_idx..row_count):
    ///         - if valid && begin <= idx <= end+1:
    ///             - sub = parent.GetComponent(row_line_idx)
    ///             - sub.Change(idx + 3 - row.begin)        (vfunc[15], +0x78)
    ///             - parent.Change(row_line_idx)            (vfunc[15], +0x78)
    ///         - flag dtor on stack copy (no row write-back)
    ///         - **early exit**: if begin > idx OR (begin <= idx AND last_row) → break
    /// - damage range 변경 없음 (한컴 raw 의 본 함수에서 SetDamage 호출 없음).
    pub fn change(&mut self, idx: usize) {
        let idx_i32 = idx as i32;
        if self.parent_glyph.is_none() {
            return;
        }
        let row_count_at_entry = self.rows.len() as i32;
        let final_idx_initial = self.search_row_idx(idx_i32);
        if final_idx_initial >= row_count_at_entry {
            return;
        }

        let mut i = final_idx_initial;
        while (i as usize) < self.rows.len() {
            let row_line_idx = i * 2;
            let row = self.rows[i as usize]; // snapshot (no write-back)

            let valid = (row.flag & RowSegment::VALID_BIT) != 0;
            let in_row = row.begin <= idx_i32 && idx_i32 <= row.end + 1;
            if valid && in_row {
                let char_offset = (idx_i32 + 3 - row.begin) as usize;
                if let Some(parent) = self.parent_glyph.as_mut() {
                    if let Some(sub) = parent.get_component_mut(row_line_idx as usize) {
                        sub.change(char_offset);
                    }
                    parent.change(row_line_idx as usize);
                }
            }

            // Early exit (한컴 asm 0x2ffae8..0x2ffafc):
            // - begin > idx → 더 이상 affected row 없음 → exit
            // - begin <= idx AND last row → 더 이상 iter 없음 → exit
            let is_last_row = (i + 1) >= row_count_at_entry;
            if row.begin > idx_i32 || is_last_row {
                break;
            }
            i += 1;
        }
    }

    /// `Composition::GetAllotment(unsigned long char_idx, Dimension dim, Allotment& out)` —
    /// `FUN_002ffce8` sz=420.
    ///
    /// 한 char_idx 의 row 를 찾아 sub-line container 에 GetAllotment 위임.
    ///
    /// raw asm 알고리즘 (1:1):
    /// ```text
    /// 1. GetIndexOf 와 동일한 패턴으로 row 검색 (forward+backward walk, iter_cache).
    /// 2. 찾은 idx 부터 row_count-1 까지 iterate:
    ///      x23 = i * 2 (= row_line_idx, 한컴 외부 line idx convention)
    ///      x25 = i * 24 (= row 의 byte offset)
    ///      x26 = remaining_count
    ///      for each iter:
    ///          if rows[i].begin > char_idx: continue
    ///          if rows[i].end + 1 < char_idx: continue
    ///          parent = *(this + 8) (SharePtr*)
    ///          parent_inner = *parent (Glyph*)
    ///          vptr = *parent_inner
    ///          if (rows[i].flag & 1) /* VALID */ :
    ///              sub = parent_inner.vfunc[17](row_line_idx)   // GetComponent
    ///              char_offset = char_idx + 3 - rows[i].begin
    ///              sub.vfunc[18](char_offset, dim, &out)        // sub.GetAllotment
    ///          else:
    ///              parent_inner.vfunc[18](row_line_idx, dim, &out)  // parent.GetAllotment
    ///          // continue to next row
    /// ```
    ///
    /// **주의**: caller 는 row 가 char_idx 포함하는 첫 row 부터 마지막 row 까지 모두 순회
    /// 하면서 sub.GetAllotment 호출 — 최종 out 값은 마지막 호출 결과. (한컴 raw asm 의
    /// for loop 가 break 없이 진행됨을 확인.)
    pub fn get_allotment(&self, char_idx: i32, dim: Dimension, out: &mut Allotment) {
        // 한컴 base default: `*out = ZERO`. parent_glyph None / out of range 시 ZERO 유지.
        *out = Allotment::ZERO;

        let row_count = self.rows.len() as i32;
        let last_row = row_count - 1;
        let cache_raw = self.iter_cache.get();
        let cache_pos = if cache_raw < 0 { 0 } else { cache_raw };
        let mut idx: i32 = if last_row < cache_pos { last_row } else { cache_pos };

        // Forward walk (GetIndexOf 와 동일 패턴)
        if last_row > cache_pos {
            let loop_count = last_row - cache_pos;
            let mut remaining = loop_count;
            let mut found = false;
            while remaining > 0 {
                if (idx as usize) >= self.rows.len() {
                    break;
                }
                if self.rows[idx as usize].end >= char_idx {
                    found = true;
                    break;
                }
                idx += 1;
                remaining -= 1;
            }
            if !found {
                idx = last_row;
            }
        }
        self.iter_cache.set(idx);

        // Backward walk
        if idx >= 1 {
            loop {
                if (idx as usize) >= self.rows.len() {
                    break;
                }
                if self.rows[idx as usize].begin <= char_idx {
                    break;
                }
                idx -= 1;
                self.iter_cache.set(idx);
                if idx <= 0 {
                    idx = 0;
                    break;
                }
            }
        }

        let mut final_idx = if idx < 0 { 0 } else { idx };
        if final_idx >= row_count {
            return;
        }

        // 2단계: row 들 순회
        let char_plus_3 = char_idx + 3;
        let parent = match self.parent_glyph.as_ref() {
            Some(p) => p,
            None => return,
        };

        while (final_idx as usize) < self.rows.len() {
            let row = self.rows[final_idx as usize];
            let row_line_idx = final_idx * 2;
            // 한컴 raw: if begin > char_idx → skip
            if row.begin > char_idx {
                final_idx += 1;
                continue;
            }
            // raw: if (end + 1) < char_idx → skip
            if (row.end + 1) < char_idx {
                final_idx += 1;
                continue;
            }
            if (row.flag & RowSegment::VALID_BIT) != 0 {
                // VALID: parent.GetComponent(row_line_idx).GetAllotment(char_offset, dim, out)
                let sub = parent.get_component(row_line_idx as usize);
                if let Some(sub_glyph) = sub {
                    let char_offset = char_plus_3 - row.begin;
                    sub_glyph.get_allotment(char_offset as usize, dim, out);
                }
                // sub == None: 한컴은 vptr null 검사 후 그냥 다음 row 로 — 동등.
            } else {
                // NOT VALID: parent.GetAllotment(row_line_idx, dim, out)
                parent.get_allotment(row_line_idx as usize, dim, out);
            }
            final_idx += 1;
        }
    }

    /// `Composition::FindPrevForcedBreak(int start, bool force_only) -> int`
    /// (`FUN_003012c4`, 408B). raw decompile + raw asm (`composition_asm/FindPrevForcedBreak_asm.txt`)
    /// 1:1 검증.
    ///
    /// 의미: `items[start]` 부터 역방향으로 walk 하며 forced break Glyph 를 찾음.
    ///
    /// - `start < 0` (raw 003012e8 `tbnz w1,#0x1f`) → start 그대로 반환.
    /// - `count <= start` (raw 003012f4-f8) → start 그대로 반환.
    /// - 각 idx 마다 `items[idx]` 의 holder 가 null 이면 idx-=1 (raw 00301354/003013ec).
    /// - 그 외 `glyph.request(req)` 호출. `req` 초기값:
    ///     `x = (natural=-1e8, 0, 0, 0)`, `y = (natural=-1e8, 0, 0, 0)`, `penalty = 0`.
    ///     (raw 의 `_DAT_00741f20` 8B = `20 bc be cc 00 00 00 00` = (f32 -1e8, f32 0), `_UNK_00741f28`
    ///     8B = 0. local_80 = `0xccbebc20` = -1e8 stored as `y.natural`. 모두 verified via Ghidra
    ///     mem dump `DumpDat741f20.py`.)
    /// - Request 가 채운 `req.penalty` 검사 (raw 00301388-9c):
    ///     - `force_only == false`: `penalty == -1000` 또는 `penalty == -10000` 시 FOUND.
    ///     - `force_only == true`:  `penalty == -10000` 시만 FOUND. (`force_only` 가 truthy 면 -1000
    ///       검사 건너뜀; -10000 만 인정.)
    /// - FOUND → idx 반환. else → idx-=1, bounds check (raw 003013fc).
    /// - idx < 0 시 idx (= -1, 즉 "앞쪽에 break 없음" 의미) 반환.
    ///
    /// 한컴 SharePtr refcount (`++/--`) 와 holder 의 NULL glyph 분기는 Rust `Option<Box<dyn Glyph>>`
    /// ownership 으로 의미 동등 — refcount는 Rust drop 가 알아서 처리, NULL glyph 케이스는 존재하지
    /// 않음 (Option::Some(Box) 는 무조건 valid).
    pub fn find_prev_forced_break(&self, start: i32, force_only: bool) -> i32 {
        let count = self.items.len() as i32;
        // raw 003012e8: 부호 검사 — bit 31 set → return start
        if start < 0 {
            return start;
        }
        // raw 003012f4-f8: cmp count, start; b.le → return start
        if count <= start {
            return start;
        }
        let mut idx = start;
        loop {
            // raw 00301324-2c: cmp count, idx (unsigned); b.ls throw out_of_range.
            // 진입은 `idx in [0, count)` 임이 LAB_003013fc 로 보장됨 → unreachable.
            debug_assert!(idx >= 0 && (idx as usize) < self.items.len());
            // raw 00301330-54: walk DLL to node at idx, holder = node[+0x10].
            // Rust 는 Vec 직접 indexing; holder NULL 은 items[idx].is_none().
            let walk_back = match &self.items[idx as usize] {
                None => true,
                Some(g) => {
                    // raw 00301368-84: Requisition init + vfunc[3] Request 호출.
                    let mut req = Requisition {
                        x: Requirement::new(-1.0e8, 0.0, 0.0, 0.0),
                        y: Requirement::new(-1.0e8, 0.0, 0.0, 0.0),
                        penalty: 0,
                    };
                    g.request(&mut req);
                    // raw 00301388-9c: penalty 검사.
                    let penalty = req.get_penalty();
                    ((force_only) || (penalty != -1000)) && (penalty != -10000)
                }
            };
            if !walk_back {
                // FOUND (raw LAB_003013a0)
                return idx;
            }
            // raw LAB_003013b4 / LAB_003013ec: idx -= 1
            idx -= 1;
            // raw LAB_003013fc: 부호 검사 → return idx (= -1, "no prev break")
            if idx < 0 {
                return idx;
            }
            // raw 00301400-04: idx < count 면 loop. 우리는 단조 감소이므로 항상 true,
            // 따라서 추가 check 불필요 (raw 의 `idx >= count` 케이스는 unreachable).
        }
    }

    /// `Composition::GetComponentPtr(int idx) const -> SharePtr<Glyph>`
    /// (`FUN_00302c5c`, 160B). raw decompile + raw asm (`composition_asm/GetComponentPtr_asm.txt`)
    /// 1:1 검증.
    ///
    /// 의미: `items[idx]` 의 holder(SharePtr) 을 반환 — `GetComponent` (vfunc[17]) 은 inner
    /// `Glyph*` 만 반환하지만 본 함수는 holder ownership 을 넘긴다 (refcount++ 후 return).
    ///
    /// raw 알고리즘:
    /// 1. `sxtw x10, w1` → idx 를 i32 → i64 signed-extend (raw 00302c68).
    /// 2. `cmp count, idx` unsigned → idx < 0 시 `count <= (large u64)` = true → throw out_of_range.
    ///    raw 00302c70-74. 우리 Rust 는 panic.
    /// 3. raw 00302c78-94: doubly-linked list walk to idx. Rust 는 Vec 직접 indexing.
    /// 4. raw 00302ca8-bc: `holder = node[+0x10]`; `*sret = holder`; if holder != NULL then
    ///    `holder.refcount += 1`.
    ///
    /// Rust 포팅: holder 의 inner Glyph 를 `clone_glyph` 로 새 Box 생성 (ownership transfer 의미
    /// 동등). holder None 이면 None. byte-equivalent LineSeg 출력 보장 (Glyph 자체 deterministic
    /// 이므로 clone 도 동일 동작).
    pub fn get_component_ptr(&self, idx: i32) -> Option<Box<dyn Glyph>> {
        // raw 00302c70-74: unsigned cmp — 음수 idx 시 throw.
        if idx < 0 {
            panic!(
                "Composition::GetComponentPtr: negative idx={} (raw throws std::out_of_range)",
                idx
            );
        }
        let count = self.items.len() as i32;
        if count <= idx {
            panic!(
                "Composition::GetComponentPtr: idx={} >= count={} (raw throws std::out_of_range)",
                idx, count
            );
        }
        // raw 00302ca8: holder = node[+0x10]
        // Rust: items[idx] is Option<Box<dyn Glyph>>. None == holder NULL, Some == valid holder.
        self.items[idx as usize].as_ref().map(|g| g.clone_glyph())
    }

    /// `Composition::GetSeparator(Break const& br) -> SharePtr<Glyph>`
    /// (`FUN_00301d50`, 652B). raw decompile + raw asm (`composition_asm/GetSeparator_asm.txt`)
    /// 1:1 검증.
    ///
    /// 의미: 라인 끝에 들어갈 separator Glyph 를 결정. paragraph 의 마지막 라인이면 stored
    /// `separator` (`+0x40`) 반환. 중간 라인이면 라인의 다음 item (idx = `to + 1`) 에 대해
    /// `ComposeGlyph(item, Forced)` 호출 — 그 결과가 valid 면 사용, 아니면 separator fallback.
    ///
    /// raw 알고리즘:
    /// 1. `to = br.to` (raw 00301d70 `ldrsw x10, [x1, #0x4]`).
    /// 2. `count = items.len()`.
    /// 3. raw 00301d7c-80: **`(int)count - 1 <= to`** 이면 `separator` 반환 (= 마지막 라인).
    /// 4. raw 00301d8c-90: `count <= (to + 1)` 면 `out_of_range` throw (LAB_003011fac).
    ///    실제로는 step 3 의 cmp 가 사전에 막아주므로 unreachable — debug_assert.
    /// 5. raw 00301d90-c8: items 의 doubly-linked list 를 walk 해서 idx=`to+1` 의 holder 추출.
    ///    Rust 는 Vec 직접 indexing 으로 대체.
    /// 6. raw 00301ddc-e8: `ComposeGlyph(item, bt=Forced=2)` 호출. raw `bl 0x002ff824`.
    ///    `mov w2, #0x2` 가 bt = `BreakType::Forced` (= 2) 설정. (`composition_compose_glyph`
    ///    free fn 으로 이미 ported.)
    /// 7. raw 00301df0-f44: 복잡한 bool 분기 — 핵심 SEMANTICS:
    ///    - `compose_result.is_some()` (raw `x22 != NULL && x22.glyph != 0`): **use compose_result**.
    ///      (raw 가 추가로 `item.glyph == result.glyph` 면 input 자신 사용; Rust 에선
    ///      `composition_compose_glyph` 가 이미 clone-of-input 반환하므로 byte-equivalent 출력.)
    ///    - `compose_result.is_none()` (raw `x22 == NULL || x22.glyph == 0`):
    ///      - `item.is_none()` 또는 `item.glyph == NULL` → 한컴 raw 는 separator fallback.
    ///      - `item.is_some()` AND `item.glyph != NULL` → 한컴 raw 도 separator fallback.
    ///      즉 compose 가 nothing 이면 항상 separator fallback. (Rust 의 `item.is_none()`
    ///      케이스는 `composition_compose_glyph` 에서 자동 None 반환 — 동등.)
    ///
    /// 한컴 SharePtr refcount 는 Rust ownership 으로 대체 (각 호출이 clone 으로 새 Box 생성 —
    /// 메모리 layout 은 다르지만 Glyph 의 BEHAVIORAL 출력은 1:1).
    pub fn get_separator(&self, br: &Break) -> Option<Box<dyn Glyph>> {
        let count = self.items.len() as i32;
        let to = br.to;
        // raw 00301d7c-80: `cmp w10, w9; b.ge` where w9 = count - 1. signed.
        if (count - 1) <= to {
            // 마지막 라인 — stored separator 반환 (raw LAB_00301e10).
            return self.separator.as_ref().map(|s| s.clone_glyph());
        }
        // raw 00301d8c-90: bounds check on (to + 1). step 3 가 to < count - 1 보장하므로
        // 항상 (to + 1) < count → unreachable throw.
        debug_assert!((to + 1) >= 0 && ((to + 1) as usize) < self.items.len());

        // raw 00301d90-c8: items list walk to idx (to + 1).
        let item_idx = (to + 1) as usize;
        // raw 00301de0-e8: ComposeGlyph(this, &item_sp, bt=2 Forced).
        // Composition::ComposeGlyph 가 input.glyph == NULL 케이스를 자체적으로 None 반환 처리하므로
        // 우리는 단순히 item.is_some() 일 때만 compose 호출.
        let compose_result = match &self.items[item_idx] {
            Some(g) => composition_compose_glyph(g.as_ref(), BreakType::Forced),
            None => None,
        };

        // raw 00301df0-f44 bool 분기 → SEMANTICS: compose 성공 → 그것 사용, 아니면 separator.
        match compose_result {
            Some(r) => Some(r),
            None => self.separator.as_ref().map(|s| s.clone_glyph()),
        }
    }

    /// `Composition::FindNextForcedBreak(int start, bool force_only) -> int`
    /// (`FUN_00301484`, 432B). raw decompile + raw asm (`composition_asm/FindNextForcedBreak_asm.txt`)
    /// 1:1 검증.
    ///
    /// 의미: `items[start]` 부터 정방향으로 walk 하며 forced break Glyph 를 찾음.
    ///
    /// - `count <= start` (raw 003014ac-b0) → epilogue 가 `min(idx, count-1)` 반환. 일반적으로
    ///   `count - 1` 또는 `-1` (count==0).
    /// - `start < 0` (raw 003014e0-e8 unsigned cmp) → out_of_range throw — Rust 에선 panic.
    ///   (raw 의 음수 idx walk 경로는 throw 가 사전에 막아서 dead code.)
    /// - 각 idx 마다 `items[idx]` 의 holder NULL 이면 idx+=1 (raw 003015b8/00301580 — bVar3=1 set).
    /// - 그 외 `glyph.request(req)` 호출 (req 초기화는 FindPrev 와 동일).
    /// - penalty 검사 동일 — FOUND 시 loop break, else idx+=1.
    /// - 최종: `return min(idx, count - 1)` (raw 003015d8-e0 `csel w0,w8,w19,lt`).
    ///   즉 break 못 찾고 끝에 도달 → `count - 1` 반환 ("뒤에 break 없음" 시 마지막 idx).
    pub fn find_next_forced_break(&self, start: i32, force_only: bool) -> i32 {
        let count = self.items.len() as i32;
        // raw 003014ac-b0: signed b.le — count <= start → epilogue path.
        // epilogue csel: count-1 < start ? count-1 : start. count <= start 이므로 count-1 < start,
        // 따라서 항상 count - 1 반환.
        if count <= start {
            return count - 1;
        }
        // raw 003014e0-e8: sxtw + unsigned cmp count, idx → throw if count <= idx_u64.
        // start < 0 sign-extend 시 large u64 → 항상 throw. raw 와 동등하게 panic.
        if start < 0 {
            panic!(
                "Composition::FindNextForcedBreak: negative start={} (raw throws std::out_of_range)",
                start
            );
        }
        let mut idx = start;
        loop {
            debug_assert!(idx >= 0 && (idx as usize) < self.items.len());
            let walk_forward = match &self.items[idx as usize] {
                None => true,
                Some(g) => {
                    let mut req = Requisition {
                        x: Requirement::new(-1.0e8, 0.0, 0.0, 0.0),
                        y: Requirement::new(-1.0e8, 0.0, 0.0, 0.0),
                        penalty: 0,
                    };
                    g.request(&mut req);
                    let penalty = req.get_penalty();
                    ((force_only) || (penalty != -1000)) && (penalty != -10000)
                }
            };
            if !walk_forward {
                // FOUND (raw LAB_00301570)
                break;
            }
            // raw 00301580 / 003015b8: idx += 1
            idx += 1;
            // raw 003015c4-c8: cmp idx, count; b.lt loop → if idx >= count break
            if idx >= count {
                break;
            }
        }
        // raw 003015d8-e0: w0 = (count-1 < idx) ? count-1 : idx
        let cap = count - 1;
        if cap < idx { cap } else { idx }
    }
}

// ============================================================
// Composition trait — virtual surface
// ============================================================

/// `Hnc::Shape::Text::Composition` virtual surface.
///
/// LRComposition / TBComposition 가 `create_line_item` 만 override (vfunc[19], vptr+0x98).
/// 다른 모든 method 는 base `Composition` 의 default 로 dispatch — Rust 에선
/// CompositionState 메소드로 구현.
pub trait Composition: std::fmt::Debug {
    fn state(&self) -> &CompositionState;
    fn state_mut(&mut self) -> &mut CompositionState;

    /// `self` 를 `&dyn Glyph` / `&mut dyn Glyph` 로 노출.
    ///
    /// `repair` (trait default method) 가 `Compositor::compose_numbering/bullet/break` 에
    /// `composition` 인자로 `self` 를 넘겨야 하는데, default method 안에서는 `Self: ?Sized`
    /// 라 `&Self → &dyn Glyph` unsizing coercion 이 불가하다. 각 concrete impl
    /// (`LRComposition`/`TBComposition`, 둘 다 `Glyph` 구현) 이 `{ self }` 로 제공.
    fn as_glyph(&self) -> &dyn Glyph;
    fn as_glyph_mut(&mut self) -> &mut dyn Glyph;

    /// `LRComposition::CreateLineItem(this, Break&, int from, int to) -> SharePtr<Glyph>`
    /// (`FUN_00302f68`, 164B) /
    /// `TBComposition::CreateLineItem` (`FUN_003037d0`, 164B).
    ///
    /// virtual dispatch (vfunc[19], vptr+0x98). LR=type 0 (HBox), TB=type 2 (VBox).
    /// raw asm 검증: `kdsnr-hwp-toolkit/work/hft_re/layout_re/line_item/*.asm.txt`.
    ///
    /// base `Composition::CreateLineItem` (`FUN_00302c54`, 8B) 는 `str xzr,[x8]; ret` — sret 에
    /// null 작성. 본 trait 은 abstract (default 없음) 이라 LR/TB 가 반드시 override 하므로
    /// base 의 null 반환은 도달 불가 (Composition 은 우리 모델에서 추상 — LR/TB 만 실체화).
    fn create_line_item(&mut self, br: &Break, from: i32, to: i32) -> Option<Box<dyn Glyph>>;

    /// `Composition::CreateItem(this, Break&, int from, int to, bool force) -> SharePtr<Glyph>`
    /// (`FUN_003000a8`, 2408B) — **non-virtual base method, LR/TB 공용** (default impl).
    ///
    /// 한 line range `[from, to]` 의 측정/배치 item 을 생성:
    /// - `force == false` (측정 경로): predecessor(bt=3)/main(bt=0)/successor(bt=1) 의 각
    ///   composed glyph 의 `Request` 를 모아 Requisition buffer 를 채우고, direction 별
    ///   `Tile`/`TileReverse` + `Align` 의 `Request_inner` 로 결합 → `Glue` 반환.
    /// - `force == true` (강제 경로): `CreateLineItem` 으로 실제 line HBox/VBox 를 만들고,
    ///   span 이 `(0, 1e8)` 면 `PlaceNatural{direction, span}` strategy 와 묶어 `Placement`
    ///   glyph (`MonoGlyph`) 로 반환, 아니면 line item 을 그대로 반환.
    ///
    /// 본 method 의 body 는 LR/TB 와 무관 (direction 은 `self.state()` 에서 읽음) 이므로
    /// trait default — `composition_create_item` 자유 함수로 위임.
    fn create_item(
        &mut self,
        br: &mut Break,
        from: i32,
        to: i32,
        force: bool,
    ) -> Option<Box<dyn Glyph>> {
        composition_create_item(self, br, from, to, force)
    }

    /// `Composition::View(int from, int to)` (`FUN_002ffe8c`, 468B) — **non-virtual base
    /// method, LR/TB 공용** (default impl).
    ///
    /// `rows` (`+0x28` Vec<RowSegment>) 를 순회하며 각 row 가 view range `[from, to]` 와
    /// 겹치는지 (`row.end >= from && row.begin <= to`) + valid bit (`flag & 1`) 상태에 따라
    /// `parent_glyph` (`+0x08` 출력 container) 의 `idx*2` 자식을 갱신:
    /// - **in view + invalid** → `CreateItem(force=true)` (실제 placed line) → `Replace(idx*2)`.
    /// - **out of view + valid** → `CreateItem(force=false)` (측정용 Glue) → `Replace(idx*2)`.
    /// - 그 외 (in+valid / out+invalid) → skip.
    /// 마지막에 `flag_59` (`+0x59`) 를 0 으로 clear. `parent_glyph` 가 null 이면 즉시 return
    /// (flag_59 미변경).
    ///
    /// raw 검증: `RowSegment` (24B `{begin@0, end@4, origin_x@8, origin_y@0xc, flag@0x10}`) 가
    /// `Break` (`{from@0, to@4, ..., flags@0x10}`) 와 동일 layout 이라 한컴은 row 를 stack 으로
    /// 복사해 `Break*` 로 `CreateItem` 에 넘김 — 원본 `rows[idx]` 는 수정 안 함. `CreateItem` 의
    /// `from`/`to` 인자는 둘 다 `0` (raw `mov w2,#0; mov w3,#0`).
    fn view(&mut self, from: i32, to: i32) {
        // raw 0x2ffeb8-0x2ffecc: parent_glyph holder + ptr null check → 둘 다 null 아닐 때만.
        //   Rust Option = (holder null || ptr null) 를 None 으로 통합. take → 끝에 put-back
        //   (한컴은 refcount++ 후 함수 끝에 refcount-- — net 0).
        let Some(mut parent) = self.state_mut().parent_glyph.take() else {
            return;
        };

        // raw 0x2ffee8: (int)rows.len() <= 0 이면 loop skip. rows.len() 는 시작 시 1회 capture.
        let row_count = self.state().rows.len();
        for idx in 0..row_count {
            // raw 0x2fff24-0x2fff38: rows[idx] (RowSegment, Copy) 를 매 iteration fresh read.
            let row = self.state().rows[idx];
            // raw 0x2fff2c / tbz/tbnz w8,#0: flag & 1 = VALID_BIT.
            let valid = (row.flag & RowSegment::VALID_BIT) != 0;
            // raw 0x2fff3c-0x2fff48: cmp end,from + ccmp begin,to,ge + b.le.
            let in_view = row.end >= from && row.begin <= to;
            // raw: in+invalid → force=1, out+valid → force=0, 그 외 → skip.
            let force = if in_view && !valid {
                true
            } else if !in_view && valid {
                false
            } else {
                continue;
            };
            // raw 0x2fff30-0x2fff38: Break = rows[idx] 복사 ({begin→from, end→to, flag→flags}).
            let mut br = Break {
                from: row.begin,
                to: row.end,
                flags: row.flag,
            };
            // raw 0x2fff5c-0x2fff6c / 0x2fff9c-0x2fffac: CreateItem(this, &Break, 0, 0, force).
            let item = self.create_item(&mut br, 0, 0, force);
            // raw 0x2fff70-0x2fff84: parent_glyph->Replace(idx*2, item) (vfunc[14], +0x70).
            parent.replace(idx * 2, item);
        }

        // raw 0x300000 / 0x30002c: this[+0x59] = 0.
        self.state_mut().flag_59 = false;
        self.state_mut().parent_glyph = Some(parent);
    }

    /// `Composition::DoRepair(int param_1, int param_2, int param_3,`
    /// `std::vector<int> const& param_4, int param_5)` (`FUN_00301664`, 1584B) —
    /// **non-virtual base method, LR/TB 공용** (default impl).
    ///
    /// `Repair` 가 한 paragraph 의 새 break 결과를 받아 `rows` (`+0x28` Vec<RowSegment>) 와
    /// `parent_glyph` (출력 container) 를 in-place 로 갱신. `count` 회 (= 처리할 line 수)
    /// 반복하며 line index `lvar12` (`from_line` 에서 시작, 매 iteration +1) 의 row 를
    /// 다음 중 하나로 처리:
    /// - **SKIP**: 기존 row 가 flag bit1 set 이고 `(begin, end)` 가 이미 원하는 값 → 그대로 둠.
    /// - **DELETE**: 뒤따르는 row 들이 새 line 범위 (`<= temp.to`) 안으로 흡수되면 그 row 들을
    ///   `rows` 와 `parent_glyph` (`Remove(idx*2|1)` → `Remove(idx*2)`) 양쪽에서 삭제.
    /// - **REPLACE**: 기존 row 를 새 `CreateItem` 결과로 교체 (`parent.Replace(idx*2)` +
    ///   `parent.Replace(idx*2|1, GetSeparator)` + `rows[idx] = temp`).
    /// - **INSERT**: 새 row 삽입 (`parent.Insert(idx*2)` + `parent.Insert(idx*2|1, GetSeparator)`
    ///   + `rows.insert(idx, temp)`).
    ///
    /// raw decompile/asm: `/tmp/hft_scripts/dorepair/helpers.txt`.
    ///
    /// 시그니처 매핑 (asm 기준): `x0=this, w1=param_1(from_line), w2=param_2(base),`
    /// `w3=param_3, x4=param_4(breaks&), w5=param_5(count)`. 각 line 의
    /// `temp.from = iVar6 + base`, `temp.to = (base-1) + breaks[uVar13]`,
    /// `iVar6 = (uVar13==0) ? 0 : breaks[uVar13-1] + 1`.
    /// `CreateItem` 호출 인자 (raw 0x301a50-68 / 0x301b34-4c): `create_item(&temp_break,`
    /// `base-1, param_3, force = this->flag_59)` — `from`/`to` 인자는 force path 일 때만 의미.
    ///
    /// `RowSegment` 의 `+0x10` flag (`Hnc::Type::Flag`, 8B) 는 libHncFoundation.dylib import:
    /// raw dump (`__ZN3Hnc4Type4FlagC1Ev` @ libHncFoundation 0xf3e0) 검증 결과 기본 ctor =
    /// `*this = 0` (zero-init), dtor (`D1`/`D2`) = pure no-op (`push/mov/pop/ret`). 따라서
    /// temp 의 flag 는 `0`, asm 의 모든 `Hnc::Type::Flag::~Flag` 호출은 무시 (관찰 효과 없음),
    /// RowSegment 의 raw 16B+8B SIMD 복사 = trivially-copyable → `Vec::insert`/`Vec::remove`
    /// 와 byte-equivalent.
    fn do_repair(
        &mut self,
        param_1: i32,
        param_2: i32,
        param_3: i32,
        param_4: &[i32],
        param_5: i32,
    ) {
        // raw 0x301690-98: plVar17 = *(this+8) (parent_glyph holder). NULL → 즉시 return.
        //   refcount++ 후 함수 끝에 refcount-- (take → put-back, net 0). plVar15 (inner) NULL
        //   케이스도 Option None 으로 통합 — 어느 쪽이든 loop 진입 안 함 → 관찰 동등.
        let Some(mut parent) = self.state_mut().parent_glyph.take() else {
            return;
        };

        // raw 0x3016ac-b4: `if (plVar15 != NULL && 0 < param_5)` — count <= 0 이면 loop skip.
        if param_5 > 0 {
            // raw 0x3016e4 / 0x3016d4-dc: lVar12 = (long)param_1; iVar4 = param_2 - 1.
            let mut lvar12: i32 = param_1;
            let ivar4 = param_2 - 1;

            // raw 0x301734..0x301730: do { ... } while (uVar13 != param_5).
            for uvar13 in 0..param_5 {
                // --- temp RowSegment 구성 (raw 0x301734-44) ---
                //   {from=-1, to=-1, ox=0, oy=0, flag=Flag()=0}.
                let mut temp = RowSegment {
                    begin: -1,
                    end: -1,
                    origin_x: 0.0,
                    origin_y: 0.0,
                    flag: 0,
                };

                // raw 0x301748-5c: row_count 를 iteration 시작 시 1회 capture (x19).
                let row_count = self.state().rows.len() as i32;

                // raw 0x301760-80: lVar12 < row_count 면 기존 row 의 origin 상속.
                //   (lVar12 >= row_count: origin 0 유지 — raw 0x3017a8 `str xzr,[sp,#0x78]`.)
                if lvar12 < row_count {
                    let r = self.state().rows[lvar12 as usize];
                    temp.origin_x = r.origin_x;
                    temp.origin_y = r.origin_y;
                }

                // raw 0x301794-bc: iVar6 = (uVar13 == 0) ? 0 : param_4[uVar13-1] + 1.
                let ivar6 = if uvar13 == 0 {
                    0
                } else {
                    param_4[(uvar13 - 1) as usize] + 1
                };

                // raw 0x3017c0-d0: temp.from = iVar6 + param_2;
                //                  temp.to = (param_2-1) + param_4[uVar13].
                temp.begin = ivar6 + param_2;
                temp.end = ivar4 + param_4[uvar13 as usize];

                // raw 0x3017d4-838: lVar12 != row_count 일 때 SKIP 검사 (lVar12 < row_count 보장).
                //   기존 row 의 flag bit1 set AND begin == temp.from AND end == temp.to → SKIP.
                if lvar12 != row_count {
                    let existing = self.state().rows[lvar12 as usize];
                    if (existing.flag & 0x2) != 0
                        && existing.begin == temp.begin
                        && existing.end == temp.end
                    {
                        // raw 0x301834 b.eq 0x301718: SKIP → advance.
                        lvar12 += 1;
                        continue;
                    }
                }

                // --- "do work": deletion loop (raw 0x301858-974) ---
                //   raw 0x3017dc-ec / 0x301844-54: lVar12 < row_count-1 일 때만 진입.
                //   raw 0x30186c-78: rows[lVar12+1].end <= temp.to 일 때만 loop.
                {
                    let mut rc = self.state().rows.len() as i32;
                    if lvar12 < rc - 1
                        && self.state().rows[(lvar12 + 1) as usize].end <= temp.end
                    {
                        loop {
                            // raw 0x301894-bc: parent.Remove((lVar12*2)|1) → parent.Remove(lVar12*2)
                            //   (vfunc[13]). 높은 index 먼저 — 낮은 index 가 유효하게 유지됨.
                            parent.remove(((lvar12 * 2) | 1) as usize);
                            parent.remove((lvar12 * 2) as usize);
                            // raw 0x3018c0-930: rows[lVar12] 삭제 (memmove tail down + shrink).
                            //   RowSegment 는 trivially-copyable, ~Flag 는 no-op → Vec::remove 와
                            //   byte-equivalent.
                            self.state_mut().rows.remove(lvar12 as usize);
                            // raw 0x301934-58: row_count 재계산 후 종료 조건.
                            rc = self.state().rows.len() as i32;
                            if lvar12 >= rc - 1 {
                                break;
                            }
                            // raw 0x30195c-6c: rows[lVar12+1].end <= temp.to 면 loop 계속.
                            if self.state().rows[(lvar12 + 1) as usize].end > temp.end {
                                break;
                            }
                        }
                    }
                }

                // --- INSERT vs REPLACE 결정 (raw 0x301978-a48) ---
                let use_replace: bool;
                let row_count = self.state().rows.len() as i32;
                if row_count == lvar12 {
                    // raw 0x301978-90: row_count == lVar12 (append) → INSERT.
                    use_replace = false;
                } else {
                    // raw 0x301994-a8: 기존 row 검사.
                    let existing = self.state().rows[lvar12 as usize];
                    // raw 0x3019b0-d8: no_damage = uVar13 < param_5-1
                    //                          && existing.end >= (param_2-1) + param_4[uVar13+1].
                    let no_damage = uvar13 < param_5 - 1
                        && existing.end >= ivar4 + param_4[(uvar13 + 1) as usize];
                    if no_damage {
                        // raw 0x301a3c → 0x301a50: INSERT.
                        use_replace = false;
                    } else {
                        // raw 0x3019dc: DAMAGE.
                        if uvar13 == param_5 - 1 {
                            // raw 0x3019e8-a04: existing.begin <= temp.to+1 ? REPLACE : INSERT.
                            use_replace = existing.begin <= temp.end + 1;
                        } else {
                            // raw 0x3019e4 b.ne 0x301a08 → REPLACE-prep.
                            use_replace = true;
                        }
                    }
                }

                // --- CreateItem (raw 0x301a50-68 / 0x301b34-4c) ---
                //   create_item(&temp_break, param_2-1, param_3, force = this->flag_59).
                //   temp 와 Break 는 동일 layout — CreateItem 이 br.flags 를 mutate 하므로
                //   호출 후 temp.flag 로 write-back (raw: 같은 메모리 &local_80).
                let force = self.state().flag_59;
                let mut br = Break {
                    from: temp.begin,
                    to: temp.end,
                    flags: temp.flag,
                };
                let item = self.create_item(&mut br, ivar4, param_3, force);
                temp.flag = br.flags;

                if use_replace {
                    // raw 0x301b50-68: parent.Replace(lVar12*2, item) (vfunc[14]).
                    parent.replace((lvar12 * 2) as usize, item);
                    // raw 0x301bb4-dc: sep = GetSeparator(&temp);
                    //                  parent.Replace((lVar12*2)|1, sep).
                    let sep = self.state().get_separator(&br);
                    parent.replace(((lvar12 * 2) | 1) as usize, sep);
                    // raw 0x301c18-30: rows[lVar12] = temp (in-place overwrite).
                    self.state_mut().rows[lvar12 as usize] = temp;
                } else {
                    // raw 0x301a6c-84: parent.Insert(lVar12*2, item) (vfunc[12]).
                    parent.insert((lvar12 * 2) as usize, item);
                    // raw 0x301ad0-f8: sep = GetSeparator(&temp);
                    //                  parent.Insert((lVar12*2)|1, sep).
                    let sep = self.state().get_separator(&br);
                    parent.insert(((lvar12 * 2) | 1) as usize, sep);
                    // raw 0x301704-14: FUN_00302004(rows, &rows[lVar12], &temp)
                    //   = std::vector<RowSegment>::insert(pos, value). RowSegment 는
                    //   trivially-copyable → Vec::insert 와 byte-equivalent.
                    self.state_mut().rows.insert(lvar12 as usize, temp);
                }

                // raw 0x301718-24: lVar12 += 1 (~Flag(&temp.flag) — no-op); uVar13 += 1.
                lvar12 += 1;
            }
        }

        // raw 0x301c40-74: refcount-- (parent 를 put-back; take 와 합쳐 net 0).
        self.state_mut().parent_glyph = Some(parent);
    }

    /// `Composition::Repair() -> bool` (`FUN_00300b14`, 1740B) — **non-virtual base method,
    /// LR/TB 공용** (default impl).
    ///
    /// damage 가 표시된 paragraph 를 한 번에 재배치하는 상위 driver. damaged char 범위를
    /// forced-break 단위 segment 로 쪼개고, 각 segment 마다:
    /// 1. segment 내 char 들의 측정값 (`widths`/`stretches`/`shrinks`/`penalties`) 을
    ///    각 `items[i].request()` 로 수집.
    /// 2. 라인별 column width (`heights`) 를 `rows` 의 origin 에서 계산 (`span - ox - oy`).
    /// 3. `compositor.ComposeNumbering`/`ComposeBullet` (ColCompositor 는 no-op) →
    ///    `compositor.ComposeBreak` 로 라인 분할 위치 (`breaks`) 결정.
    /// 4. `DoRepair` 로 `rows` + `parent_glyph` 를 in-place 갱신.
    /// 마지막에 `has_damage` 를 clear. 반환 = `!had_damage` (damage 없었으면 true).
    ///
    /// raw decompile/asm: `/tmp/hft_scripts/repair/repair.txt`.
    ///
    /// 시그니처: arg 없음 (`this` 만). `bool` 반환 = `(원래 has_damage == 0)`.
    /// 한컴 `local_80` (`ComposeNumbering`/`Bullet` 의 `vector<pair<...>>` 출력) 는
    /// ColCompositor 에선 두 함수가 raw `ret` no-op 이라 전혀 안 건드림 — Rust 도 동일.
    /// **PptCompositor** 가 compositor 로 들어오면 두 함수 모두 실제 body 호출 — `numbering`
    /// vector 가 push/consume 되어 byte-eq layout 산출 (B-Compositor F/G 완료, 14번째 세션).
    ///
    /// `compositor` 가 `None` 이면 raw 는 `this->compositor` null deref 로 crash —
    /// 본 port 는 명시적 panic (caller 가 has_damage 시 compositor 보장하는 계약).
    ///
    /// **provider 인자** (raw vtable 시그니처 외): PptCompositor 의 ComposeBullet 이
    /// CoreText 기반 측정을 필요로 함. raw 는 macOS 글로벌 `libhsp.dylib` shim 으로 호출;
    /// Rust 는 caller chain 으로 provider 전달. ColCompositor / Simple / Array 의
    /// `compose_bullet` 은 raw `ret` no-op (trait default) 이므로 provider 무시.
    fn repair(
        &mut self,
        ct_provider: &dyn crate::font_metric::CoreTextFontProvider,
        gm_provider: &dyn crate::font_metric::GlobalMetricProvider,
    ) -> bool {
        // raw 0x300b34-38: CVar8 = this->has_damage (this[0x58]); 0 이면 아무것도 안 함.
        let had_damage = self.state().has_damage;
        if had_damage {
            // raw 0x300b44: iVar7 = item count (this+0x20).
            let count = self.state().get_count() as i32;
            // raw 0x300b48-5c: uVar25 = row_count.
            let mut row_count = self.state().rows.len() as i32;
            // raw 0x300b60-6c: iVar13 = FindPrevForcedBreak(damage_begin, force_only=false).
            let mut seg_start = self
                .state()
                .find_prev_forced_break(self.state().damage_begin, false);

            // raw 0x300b70-bd0: starting row index `start_row` (uVar18).
            //   row_count < 1 → 0. else: 첫 row 에서 (seg_start < row.begin || seg_start <=
            //   row.end) 면 그 index, 못 찾으면 row_count.
            let mut start_row: i32;
            if row_count < 1 {
                start_row = 0;
            } else {
                start_row = row_count;
                let mut i = 0;
                while i < row_count {
                    let row = self.state().rows[i as usize];
                    if seg_start < row.begin || seg_start <= row.end {
                        start_row = i;
                        break;
                    }
                    i += 1;
                }
            }

            // raw 0x300b... `local_80 = 0` — numbering vector (`vector<NumberingEntry>`).
            //   segment loop **밖**에서 1회 init 되어 loop 전체에 누적된다 (raw 는 line 121
            //   에서 init, line 374 에서 해제 — segment loop 보다 shallow scope). ColCompositor
            //   에선 compose_numbering/bullet 가 no-op 이라 끝까지 비어있음. layout 출력에는
            //   영향 없음 — numbering/bullet 전용 내부 상태.
            let mut numbering: Vec<crate::compositor::NumberingEntry> = Vec::new();

            // raw 0x300bdc-e4 / 0x301164-6c: iVar13 < count-1 일 때만 main loop.
            if seg_start < count - 1 {
                // raw 0x300c30-1140: MAIN LOOP `do { ... } while (iVar22 < count-1)`.
                loop {
                    // raw 0x300c30-38: damage_end <= seg_start → exit.
                    if self.state().damage_end <= seg_start {
                        break;
                    }

                    // raw 0x300c3c-78: 6 vector (widths/stretches/shrinks/penalties/heights
                    //   /breaks) 를 빈 상태로 init. breaks 는 ComposeBreak 가 전부 덮어쓰므로
                    //   본 port 는 ComposeBreak 의 반환 Vec 으로 대체 (inner-loop 의 breaks
                    //   resize 는 관찰 효과 없음 — 생략).
                    let mut widths: Vec<f32> = Vec::new();
                    let mut stretches: Vec<f32> = Vec::new();
                    let mut shrinks: Vec<f32> = Vec::new();
                    let mut penalties: Vec<i32> = Vec::new();
                    let mut heights: Vec<f32> = Vec::new();

                    // raw 0x300c70-74: iVar1 = seg_start + 1; iVar22 = iVar1.
                    let i_var1 = seg_start + 1;
                    let mut i_var22 = i_var1;

                    // raw 0x300c7c-f48: INNER MEASURE LOOP (iVar1 < count 일 때만).
                    if i_var1 < count {
                        // raw 0x300c84: vec_size (iVar14) = 0.
                        let mut vec_size: i32 = 0;
                        loop {
                            // raw 0x300c9c-a4: uVar9 = iVar22 - iVar1; vec_size <= uVar9 면 resize.
                            let u_var9 = i_var22 - i_var1;
                            if vec_size <= u_var9 {
                                // raw 0x300ca8-c0: vec_size = FindNextForcedBreak(iVar22) - iVar1 + 1.
                                let nfb = self.state().find_next_forced_break(i_var22, false);
                                vec_size = (nfb - i_var1) + 1;
                                let ns = vec_size as usize;
                                // raw 0x300cc4-dc8: 6 vector 를 vec_size 로 resize (zero-fill /
                                //   truncate) — FUN_0067ecd4/FUN_0067ee78 = `Vec::resize`.
                                widths.resize(ns, 0.0);
                                stretches.resize(ns, 0.0);
                                shrinks.resize(ns, 0.0);
                                penalties.resize(ns, 0);
                                heights.resize(ns, 0.0);
                            }
                            // raw 0x300dcc-ec: widths/stretches/shrinks/penalties[uVar9] = 0.
                            widths[u_var9 as usize] = 0.0;
                            stretches[u_var9 as usize] = 0.0;
                            shrinks[u_var9 as usize] = 0.0;
                            penalties[u_var9 as usize] = 0;

                            // raw 0x300df0-fc: count <= iVar22 면 throw out_of_range("GetAt").
                            //   loop 구조상 iVar22 < count 가 보장됨 — 방어적 panic.
                            if count <= i_var22 {
                                panic!(
                                    "Composition::Repair — out_of_range \"GetAt\" (i_var22={}, count={})",
                                    i_var22, count
                                );
                            }

                            // raw 0x300e00-74: items[iVar22] 의 holder/glyph 추출 + Request.
                            //   borrow 분리: direction (Copy) 먼저, 그 다음 items 접근.
                            let direction = self.state().direction;
                            let measured: Option<Requisition> =
                                match &self.state().items[i_var22 as usize] {
                                    // raw 0x300e38 / LAB_00300c90: holder NULL → iVar22++.
                                    None => None,
                                    Some(glyph) => {
                                        // raw 0x300e4c-74: req = {x:(-1e8,0,0,0), y:(-1e8,0,0,0),
                                        //   penalty:0}; glyph.Request(&req) (vfunc[3]).
                                        let mut req = Requisition {
                                            x: Requirement::new(-1.0e8, 0.0, 0.0, 0.0),
                                            y: Requirement::new(-1.0e8, 0.0, 0.0, 0.0),
                                            penalty: 0,
                                        };
                                        glyph.request(&mut req);
                                        Some(req)
                                    }
                                };

                            match measured {
                                None => {
                                    // raw LAB_00300c90: iVar22++; iVar22 >= count → exit.
                                    i_var22 += 1;
                                    if i_var22 >= count {
                                        break;
                                    }
                                }
                                Some(req) => {
                                    // raw 0x300e78-edc: main axis = LR ? req.x : req.y.
                                    //   natural != -1e8 면 widths/stretches/shrinks 채움.
                                    let main = if direction == CompositionDirection::LR {
                                        req.x
                                    } else {
                                        req.y
                                    };
                                    if main.natural != -1.0e8 {
                                        widths[u_var9 as usize] = main.natural;
                                        stretches[u_var9 as usize] = main.stretch;
                                        shrinks[u_var9 as usize] = main.shrink;
                                    }
                                    // raw 0x300ee0-e8: penalties[uVar9] = req.penalty.
                                    penalties[u_var9 as usize] = req.penalty;
                                    // raw 0x300eec-f08: penalty 가 forced-break (-10000/-1000)
                                    //   가 아니면 iVar22++ 후 계속, 맞으면 inner loop 종료.
                                    let penalty = penalties[u_var9 as usize];
                                    if penalty != -10000 && penalty != -1000 {
                                        i_var22 += 1;
                                        // raw 0x300c94: iVar22 >= count → exit.
                                        if i_var22 >= count {
                                            break;
                                        }
                                    } else {
                                        // raw 0x300f38/f40: forced break → exit inner loop.
                                        break;
                                    }
                                }
                            }
                        }
                    }

                    // raw 0x300f4c-54: end_idx (iVar14) = min(iVar22, count-1).
                    let end_idx = i_var22.min(count - 1);
                    // raw 0x300f6c / 0x300c3c: iVar19 = start_row, iVar17 = row_count
                    //   (둘 다 loop iteration 시작 시점 값).
                    let start_row_i = start_row;
                    let row_count_i = row_count;

                    // raw 0x300f58-fe8: heights 채우기 (end_idx - seg_start > 0 일 때만).
                    if end_idx - seg_start > 0 {
                        // raw 0x300f70-78: iVar3 = max(start_row, row_count).
                        let i_var3 = start_row_i.max(row_count_i);
                        let mut k: i32 = 0;
                        loop {
                            // raw 0x300f8c-90 / 0x300fe0-e8: k == iVar3-start_row 도달 →
                            //   heights[iVar3-start_row] = span; break.
                            if (i_var3 - start_row_i) == k {
                                heights[(i_var3 - start_row_i) as usize] = self.state().span;
                                break;
                            }
                            // raw 0x300f94-c0: heights[k] = (span - rows[start_row+k].ox)
                            //   - rows[start_row+k].oy.
                            let row = self.state().rows[(start_row_i + k) as usize];
                            heights[k as usize] =
                                (self.state().span - row.origin_x) - row.origin_y;
                            k += 1;
                            // raw 0x300fd4-d8: k == end_idx - seg_start → exit.
                            if k == end_idx - seg_start {
                                break;
                            }
                        }
                    }

                    // raw 0x300fec-10b0: compositor 호출 (numbering/bullet → no-op for
                    //   ColCompositor; heights resize; ComposeBreak). compositor 는 take →
                    //   put-back (DoRepair 가 내부에서 다시 take 하므로 호출 전 복원 필수).
                    let breaks_u32: Vec<u32> = {
                        let mut compositor = self.state_mut().compositor.take().expect(
                            "Composition::Repair: compositor is None (raw derefs this->compositor)",
                        );
                        // raw 0x300fec-1010: ComposeNumbering(seg_start, end_idx, this,
                        //   &local_80). ColCompositor 는 raw `ret` no-op; PptCompositor 만
                        //   `numbering` 에 entry append. `compositor` 는 take 된 local 이라
                        //   `self` 와 alias 없음.
                        compositor.compose_numbering(
                            seg_start,
                            end_idx,
                            self.as_glyph(),
                            &mut numbering,
                        );
                        // raw 0x301014-1034: ComposeBullet(seg_start, end_idx, &local_80, this)
                        //   — ColCompositor no-op; PptCompositor 만 numbering 을 읽고
                        //   composition 의 CharItemView (+0x98) 를 mutate.
                        compositor.compose_bullet(
                            seg_start,
                            end_idx,
                            &numbering,
                            self.as_glyph_mut(),
                            ct_provider,
                            gm_provider,
                        );
                        // raw 0x301038-70: heights 를 (row_count - start_row) + 1 로 resize.
                        heights.resize(((row_count_i - start_row_i) + 1) as usize, 0.0);
                        // raw 0x301074-10b0: ComposeBreak(widths, stretches, shrinks,
                        //   penalties, heights, this, seg_start, end_idx, &out).
                        let b = compositor.compose_break(
                            &widths,
                            &stretches,
                            &shrinks,
                            &penalties,
                            &heights,
                            self.as_glyph(),
                            seg_start,
                            end_idx,
                        );
                        self.state_mut().compositor = Some(compositor);
                        b
                    };
                    // raw: ComposeBreak 반환값 = out vector 길이 = line count.
                    let line_count = breaks_u32.len() as i32;
                    let breaks_i32: Vec<i32> = breaks_u32.iter().map(|&b| b as i32).collect();

                    // raw 0x3010b4-cc: DoRepair(this, start_row, iVar1, end_idx, &breaks,
                    //   line_count).
                    self.do_repair(start_row_i, i_var1, end_idx, &breaks_i32, line_count);

                    // raw 0x300c0c-2c: uVar18 = start_row + line_count; uVar25 = 새 row_count;
                    //   iVar13 = end_idx; while (iVar22 < count-1).
                    start_row = start_row_i + line_count;
                    row_count = self.state().rows.len() as i32;
                    seg_start = end_idx;
                    if i_var22 >= count - 1 {
                        break;
                    }
                }
            }

            // raw 0x301140 / 0x301170: this->has_damage = false.
            self.state_mut().has_damage = false;
        }

        // raw 0x301174-7c: return (원래 has_damage == 0).
        !had_damage
    }
}

/// `Composition::CreateItem` (`FUN_003000a8`, 2408B) 의 1:1 포팅 본체.
///
/// raw decompile: `work/hft_re/layout_re/decompiles_v2/Composition__CreateItem_003000a8.txt`,
/// raw asm: `/tmp/hft_scripts/create_item/asm.txt`.
///
/// 시그니처 매핑 (Ghidra 의 param 추론은 어긋나 있음 — asm 기준): `x0=this`, `x1=Break&`,
/// `x2=from`, `x3=to`, `x4=force(bool)`, `x8=sret`. Ghidra 의 `param_3`/`param_4` (= `x2`/`x3`)
/// 는 normal path 에서 사실상 미사용 (from/to 는 Break struct 에서 다시 읽음). force path 의
/// `CreateLineItem` 호출은 `x2`/`x3` 를 안 건드려서 caller 가 넘긴 `from`/`to` 가 그대로 전달됨
/// → 본 port 는 force path 에서 `from`/`to` 인자를, normal path 에서 `br.from`/`br.to` 를 사용.
fn composition_create_item<C: Composition + ?Sized>(
    comp: &mut C,
    br: &mut Break,
    from: i32,
    to: i32,
    force: bool,
) -> Option<Box<dyn Glyph>> {
    // raw 0x3000d8-0x3000ec:
    //   *(ulong*)(Break + 0x10) = *(ulong*)(Break + 0x10) & ~3 | force | 2;
    br.flags = (br.flags & !3u64) | (force as u64) | 2u64;

    // ---- FORCE path (raw 0x3000f0 `cbz w4` fall-through) -------------
    if force {
        // raw 0x3000f4-0x300108: this->vfunc[0x98](this, Break) == CreateLineItem.
        //   asm 가 x2/x3 미설정 → CreateItem 의 incoming from/to 가 그대로 전달.
        let line_item = comp.create_line_item(br, from, to);
        // raw 0x30010c-0x300124: span 이 (0, 1e8) 범위 아니면 그대로 return
        //   (sret 에는 이미 CreateLineItem 결과가 들어 있음).
        let span = comp.state().span;
        if !(span > 0.0 && span < 1.0e8) {
            return line_item;
        }
        // raw 0x300128-0x300204: LayoutFactory::GetInstance() (stateless) +
        //   PlaceNatural{direction = this[+0x48], span = this[+0x4c]} 16B +
        //   Placement glyph (`MonoGlyph`, vtable 0x781168) 로 body+strategy 래핑 → sret.
        let direction = comp.state().direction as i32;
        return Some(Box::new(MonoGlyph {
            placement: Box::new(PlaceNatural { direction, span }),
            child: line_item,
        }));
    }

    // ---- NORMAL path (raw 0x300208 LAB_00300208) --------------------
    // raw 0x300208: iVar18 = *piVar12 (from); iVar17 = piVar12[1] (to).
    let from = br.from;
    let to = br.to;
    let count = comp.state().get_count() as i32;

    // raw 0x300214-0x300284: Requisition buffer ((to-from)+4 개, 전부 INVALID sentinel).
    //   uVar15 == 0 ((to-from) == -4) 이면 미할당 → 빈 Vec.
    //   raw 의 overflow check (uVar15 > 0x71c..) 는 buf_len < 0 → usize 변환 시 panic 으로 등가.
    let buf_len = (to - from) + 4;
    let mut buf: Vec<Requisition> = if buf_len != 0 {
        vec![Requisition::INVALID; buf_len as usize]
    } else {
        Vec::new()
    };
    // iVar19 — buffer 에 실제로 작성된 Requisition 개수.
    let mut written: usize = 0;

    // raw 0x300288-0x300434: predecessor (idx = from-1, bt = 3 = Penalty).
    //   `if (iVar18 < 1)` else branch — from >= 1 일 때만.
    //   raw 0x300294: `if (count <= from-1) throw out_of_range("GetAt")` — get_component 가 동일 panic.
    if from >= 1 {
        let pred_idx = (from - 1) as usize;
        let composed = comp
            .state()
            .get_component(pred_idx)
            .and_then(|item| composition_compose_glyph(item, BreakType::Penalty));
        if let Some(g) = composed {
            // raw 0x3003f8-0x300404: composed->vfunc[0x18](composed, &buf[0]) == Request.
            g.request(&mut buf[written]);
            written += 1;
        }
    }

    // raw 0x300504 LAB_00300504: main loop (from <= to), idx = from..=to, bt = 0 = Normal.
    if from <= to {
        for i in from..=to {
            // raw 0x300290: `if (count <= uVar20) throw` — i<0 이면 (ulong) huge → throw.
            //   Rust: i<0 → `i as usize` huge → get_component panic — 등가.
            let idx = i as usize;
            let composed = comp
                .state()
                .get_component(idx)
                .and_then(|item| composition_compose_glyph(item, BreakType::Normal));
            if let Some(g) = composed {
                // raw 0x300654-0x300664: composed->Request(&buf[iVar19 * 0x24]).
                g.request(&mut buf[written]);
                written += 1;
            }
        }
    }

    // raw 0x30045c LAB_0030045c: successor (idx = to+1, bt = 1 = Hint).
    //   `if ((int)count - 1 <= to) goto skip` → to < count-1 일 때만.
    if to < count - 1 {
        let succ_idx = (to + 1) as usize;
        let composed = comp
            .state()
            .get_component(succ_idx)
            .and_then(|item| composition_compose_glyph(item, BreakType::Hint));
        if let Some(g) = composed {
            // raw 0x300714-0x300728: composed->Request(&buf[iVar19 * 0x24]).
            g.request(&mut buf[written]);
            written += 1;
        }
    }

    // raw 0x300778-0x3007c4: buffer 를 written(iVar19) 개로 shrink.
    //   (written <= (to-from)+3 < buf_len 이라 항상 shrink — grow 분기는 도달 안 함.)
    buf.truncate(written);

    // raw 0x300760-0x300784: out Requisition = INVALID sentinel.
    let mut out = Requisition::INVALID;

    // raw 0x3007c8-0x3008a8: direction 별 main-axis 결합.
    let direction = comp.state().direction;
    let span = comp.state().span;
    // raw 0x3007d0-0x3007e8: span 이 (0, 1e8) 범위면 valid.
    let span_invalid = span >= 1.0e8 || span <= 0.0;

    let align_direction: i32;
    if direction == CompositionDirection::LR {
        // raw 0x30080c: `local_f0 == 0` (LR).
        if span_invalid {
            // raw 0x300868-0x3008a4: Tile{direction:0, cached_req:INVALID, trim:1}::Request_inner.
            let mut tile = Tile {
                direction: 0,
                cached_req: Requisition::INVALID,
                trim_trailing_hint: true,
            };
            tile.request_inner(&buf, &mut out);
        } else {
            // raw 0x300810-0x300818: out.x = {natural: span, stretch: 0, shrink: 0, alignment: 0}.
            out.x = Requirement::new(span, 0.0, 0.0, 0.0);
        }
        // raw: LR 은 0x300808 경유 후 0x3008a8 `mov w8,#1` → Align.direction = 1.
        align_direction = 1;
    } else {
        // raw 0x3007ec else: `local_f0 != 0` (TB).
        if span_invalid {
            // raw 0x300820-0x30085c: TileReverse{direction: this.direction, cached_req:INVALID,
            //   trim:1}::Request_inner.
            let mut tr = TileReverse {
                direction: direction as i32,
                cached_req: Requisition::INVALID,
                trim_trailing_hint: true,
            };
            tr.request_inner(&buf, &mut out);
        } else {
            // raw 0x3007f4-0x300808: out.y = {natural: span, stretch: 0, shrink: 0, alignment: 1.0}.
            out.y = Requirement::new(span, 0.0, 0.0, 1.0);
        }
        // raw 0x3007f4 / 0x300860 `mov w8,#0` → Align.direction = 0.
        align_direction = 0;
    }

    // raw 0x3008ac-0x3008c8: Align{direction = LR?1:0}::Request_inner(&buf, &out).
    let mut align = Align { direction: align_direction };
    align.request_inner(&buf, &mut out);

    // raw 0x3008cc-0x300910: LayoutFactory::GetInstance() (stateless) +
    //   Glue (vtable 0x780bd0, 48B) {req: out} 생성 → SharePtr → sret.
    Some(Box::new(Glue::new(out)))
}

// ============================================================
// LRComposition
// ============================================================

/// `Hnc::Shape::Text::LRComposition` — horizontal paragraph (`+0x48 = 0`).
///
/// - vtable @ 0x7801e0.
/// - ctor `FUN_00302d10` (144B): Composition base 의 인라인 layout 후 vptr → LRComposition.
/// - `Create` factory `FUN_00302e94` (212B): operator_new(0x68) + ctor + SharePtr 래핑.
#[derive(Debug)]
pub struct LRComposition {
    pub inner: CompositionState,
}

impl LRComposition {
    pub fn new(
        parent_glyph: Option<Box<dyn Glyph>>,
        compositor: Option<Box<dyn Compositor>>,
        separator: Option<Box<dyn Glyph>>,
        span: f32,
    ) -> Self {
        Self {
            inner: CompositionState::new(
                parent_glyph,
                compositor,
                separator,
                CompositionDirection::LR,
                span,
            ),
        }
    }
}

/// `Glyph` for `LRComposition` — `Composition::*` 메소드들로 forward.
impl Glyph for LRComposition {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    /// `LRComposition::Clone()` — `FUN_00302e4c` sz=52 → `FUN_0063e8bc` (copy ctor, sz=200).
    ///
    /// raw copy ctor 1:1 (한컴은 SharePtr refcount++ shared; Rust 는 deep-clone via
    /// `Glyph::clone_glyph` — layout output deterministic 이라 byte-equivalent 보장):
    /// - vptr → MonoGlyph (initial) → pure_virtual → LRComposition (final)
    /// - parent_glyph (this+0x08): shared/clone
    /// - items list (this+0x10): deep copy (`FUN_0063e3d0`)
    /// - rows vector (this+0x28): deep copy (`FUN_0063ea00`)
    /// - separator (this+0x40): shared/clone
    /// - direction (this+0x48), span (this+0x4c): byte copy
    /// - damage_begin/end (this+0x50): byte copy
    /// - has_damage byte (this+0x58), iter_cache (this+0x5c): byte copy
    /// - compositor (this+0x60): shared/clone
    fn clone_glyph(&self) -> Box<dyn Glyph> {
        Box::new(LRComposition {
            inner: clone_composition_state(&self.inner, CompositionDirection::LR),
        })
    }

    fn request(&self, req_out: &mut Requisition) {
        self.inner.request(req_out);
    }
    fn allocate(&mut self, alloc: &Allocation, ext: &mut Extension) {
        self.inner.allocate(alloc, ext);
    }
    fn get_count(&self) -> usize { self.inner.get_count() }
    fn get_component(&self, idx: usize) -> Option<&dyn Glyph> { self.inner.get_component(idx) }
    fn get_component_mut(&mut self, idx: usize) -> Option<&mut dyn Glyph> {
        self.inner.get_component_mut(idx)
    }
    fn get_allotment(&self, idx: usize, dim: Dimension, out: &mut Allotment) {
        self.inner.get_allotment(idx as i32, dim, out);
    }
}

// ============================================================
// TBComposition
// ============================================================

/// `Hnc::Shape::Text::TBComposition` — vertical paragraph (`+0x48 = 1`).
///
/// - vtable @ 0x780340.
/// - ctor `FUN_0030356c` (148B): `+0x48 = 1` 만 LR 과 다름.
#[derive(Debug)]
pub struct TBComposition {
    pub inner: CompositionState,
}

impl TBComposition {
    pub fn new(
        parent_glyph: Option<Box<dyn Glyph>>,
        compositor: Option<Box<dyn Compositor>>,
        separator: Option<Box<dyn Glyph>>,
        span: f32,
    ) -> Self {
        Self {
            inner: CompositionState::new(
                parent_glyph,
                compositor,
                separator,
                CompositionDirection::TB,
                span,
            ),
        }
    }
}

impl Glyph for TBComposition {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    /// `TBComposition::Clone()` — `FUN_003036b0` sz=52 → `FUN_0063ec24` (copy ctor).
    /// LRComposition::Clone 과 byte-identical except direction=TB.
    fn clone_glyph(&self) -> Box<dyn Glyph> {
        Box::new(TBComposition {
            inner: clone_composition_state(&self.inner, CompositionDirection::TB),
        })
    }

    fn request(&self, req_out: &mut Requisition) {
        self.inner.request(req_out);
    }
    fn allocate(&mut self, alloc: &Allocation, ext: &mut Extension) {
        self.inner.allocate(alloc, ext);
    }
    fn get_count(&self) -> usize { self.inner.get_count() }
    fn get_component(&self, idx: usize) -> Option<&dyn Glyph> { self.inner.get_component(idx) }
    fn get_component_mut(&mut self, idx: usize) -> Option<&mut dyn Glyph> {
        self.inner.get_component_mut(idx)
    }
    fn get_allotment(&self, idx: usize, dim: Dimension, out: &mut Allotment) {
        self.inner.get_allotment(idx as i32, dim, out);
    }
}

// ============================================================
// Composition trait impls — create_line_item (B-5g)
// ============================================================

/// `LRComposition::CreateLineItem` (`FUN_00302f68`, 164B).
///
/// raw asm 1:1 (`line_item/LRComposition__CreateLineItem_asm.txt`):
/// ```text
/// bl LayoutFactory::GetInstance()                  ; 0x302f94
/// bl LayoutFactory::CreateHBox()  → box            ; 0x302f9c (sret = sp+0x8)
/// *out = 0                                          ; 0x302fa0
/// x24 = box
/// if box != 0:
///     holder = operator_new(0x10)                   ; 0x302fb0
///     holder[0] = box; holder[1] = 1 (refcount)     ; 0x302fb8
/// else: holder = 0
/// *out = holder                                     ; 0x302fc4
/// ; compositor.ComposeLayout dispatch:
/// x8 = this[+0x60]                                  ; compositor holder
/// x0 = *x8                                          ; compositor
/// x8 = (*x0)[+0x30]                                 ; vfunc[6] = ComposeLayout
/// blr x8 (compositor, this, type=0, Break&, from, to, &out)   ; 0x302ff0
/// ```
///
/// 즉: HBox 생성 → SharePtr 래핑 → `compositor.compose_layout(this, type=0, br, from, to, &mut box)`.
impl Composition for LRComposition {
    fn state(&self) -> &CompositionState { &self.inner }
    fn state_mut(&mut self) -> &mut CompositionState { &mut self.inner }
    fn as_glyph(&self) -> &dyn Glyph { self }
    fn as_glyph_mut(&mut self) -> &mut dyn Glyph { self }

    fn create_line_item(&mut self, br: &Break, from: i32, to: i32) -> Option<Box<dyn Glyph>> {
        // raw 0x302f94: LayoutFactory::GetInstance() (stateless — no-op for Rust)
        let _ = LayoutFactory::get_instance();
        // raw 0x302f9c: box = LayoutFactory::CreateHBox()
        let mut hbox = LayoutFactory::create_h_box();

        // raw 0x302fc8-0x302ff0: compositor.ComposeLayout(this, type=0, br, from, to, &box)
        // Rust borrow: compositor 를 take 해서 local 로 옮긴 뒤 self(&dyn Glyph) 와 동시 사용.
        if let Some(mut compositor) = self.inner.compositor.take() {
            // LR = type 0 (HBox)
            compositor.compose_layout(self, 0, br, from, to, &mut hbox);
            self.inner.compositor = Some(compositor);
        }

        Some(Box::new(hbox))
    }
}

/// `TBComposition::CreateLineItem` (`FUN_003037d0`, 164B).
///
/// `LRComposition::CreateLineItem` 와 byte-identical except:
/// - `LayoutFactory::CreateVBox()` (HBox 대신)
/// - `compose_layout` 의 type = 2 (TB) (0 대신)
impl Composition for TBComposition {
    fn state(&self) -> &CompositionState { &self.inner }
    fn state_mut(&mut self) -> &mut CompositionState { &mut self.inner }
    fn as_glyph(&self) -> &dyn Glyph { self }
    fn as_glyph_mut(&mut self) -> &mut dyn Glyph { self }

    fn create_line_item(&mut self, br: &Break, from: i32, to: i32) -> Option<Box<dyn Glyph>> {
        let _ = LayoutFactory::get_instance();
        // raw 0x3037cc: box = LayoutFactory::CreateVBox()
        let mut vbox = LayoutFactory::create_v_box();

        // raw: compositor.ComposeLayout(this, type=2, br, from, to, &box)
        if let Some(mut compositor) = self.inner.compositor.take() {
            // TB = type 2 (VBox)
            compositor.compose_layout(self, 2, br, from, to, &mut vbox);
            self.inner.compositor = Some(compositor);
        }

        Some(Box::new(vbox))
    }
}

/// `Composition` copy ctor helper — `FUN_0063e8bc` (LR) / `FUN_0063ec24` (TB) 의 raw 1:1.
///
/// Direction 만 caller 가 결정 (LR vs TB). 나머지 모든 field 는 source 에서 그대로 복사:
/// - parent_glyph / separator / compositor: clone (Rust deep-clone via trait — 한컴 SharePtr
///   shared 와 layout output 동등)
/// - items: 각 item 의 clone_glyph
/// - rows: Copy (24B POD)
/// - span / damage_begin / damage_end / has_damage / iter_cache: scalar copy
fn clone_composition_state(src: &CompositionState, direction: CompositionDirection) -> CompositionState {
    CompositionState {
        parent_glyph: src.parent_glyph.as_ref().map(|g| g.clone_glyph()),
        items: src.items.iter().map(|opt| opt.as_ref().map(|g| g.clone_glyph())).collect(),
        rows: src.rows.clone(),
        separator: src.separator.as_ref().map(|g| g.clone_glyph()),
        direction,
        span: src.span,
        damage_begin: src.damage_begin,
        damage_end: src.damage_end,
        has_damage: src.has_damage,
        flag_59: src.flag_59,
        iter_cache: Cell::new(src.iter_cache.get()),
        compositor: src.compositor.as_ref().map(|c| c.clone_compositor()),
    }
}

// ============================================================
// Tests — ctor sanity only (method body 는 후속 단계에서 1:1 포팅 후 test 추가)
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font_metric::{
        CoreTextFontProvider, GlobalFontMetrics, GlobalMetricProvider, SystemFont,
    };

    /// Repair 의 ColCompositor / 빈 numbering path 에서 provider 가 호출 안 됨 — zero stub.
    ///
    /// PptCompositor wire 후 실제 measurement 가 필요한 통합 테스트는 별도 fixture (mock 의
    /// face/glyph 매핑) 가 필요하지만 본 composition.rs tests 는 ColCompositor 경로만 가짐.
    struct ZeroCt;
    impl CoreTextFontProvider for ZeroCt {
        fn glyph_for_character(&self, _font: SystemFont, _size_px: f64, _c: u16) -> u16 { 0 }
        fn advance_for_glyph(&self, _font: SystemFont, _size_px: f64, _glyph: u16) -> f64 { 0.0 }
    }
    struct ZeroGm;
    impl GlobalMetricProvider for ZeroGm {
        fn global_metrics(&self, _font_name: &str, _font_style: i32) -> GlobalFontMetrics {
            GlobalFontMetrics { em: 1000.0, ascent: 0.0, m7: 0.0, m8: 0.0 }
        }
    }

    #[test]
    fn lr_composition_initial_state() {
        let c = LRComposition::new(None, None, None, 100.0);
        assert_eq!(c.inner.direction, CompositionDirection::LR);
        assert_eq!(c.inner.span, 100.0);
        assert_eq!(c.inner.damage_begin, -1);
        assert_eq!(c.inner.damage_end, -1);
        assert!(c.inner.has_damage);
        assert_eq!(c.inner.iter_cache.get(), 0);
        assert_eq!(c.inner.items.len(), 0);
        assert_eq!(c.inner.rows.len(), 0);
    }

    #[test]
    fn tb_composition_initial_state() {
        let c = TBComposition::new(None, None, None, 50.0);
        assert_eq!(c.inner.direction, CompositionDirection::TB);
        assert_eq!(c.inner.span, 50.0);
    }

    // -- B-5g: create_line_item ----------------------------------

    #[test]
    fn lr_create_line_item_returns_hbox() {
        // compositor 없는 경우: HBox 만 생성하고 compose_layout 은 skip.
        let mut c = LRComposition::new(None, None, None, 100.0);
        let br = Break::default();
        let item = c.create_line_item(&br, 0, 5);
        assert!(item.is_some(), "create_line_item returns Some(HBox)");
        // 반환된 line item 은 Box_ — downcast 로 검증.
        let item = item.unwrap();
        let bx = item.as_any().downcast_ref::<crate::glyph::Box_>().unwrap();
        // HBox = Box + Superpose([Tile(0), Align(1)])
        assert!(bx.layout.is_some());
        assert_eq!(bx.layout.as_ref().unwrap().children.len(), 2);
        assert_eq!(bx.children.len(), 0, "fresh HBox has no children (compositor skipped)");
    }

    #[test]
    fn tb_create_line_item_returns_vbox() {
        let mut c = TBComposition::new(None, None, None, 50.0);
        let br = Break::default();
        let item = c.create_line_item(&br, 0, 3);
        assert!(item.is_some());
        let item = item.unwrap();
        let bx = item.as_any().downcast_ref::<crate::glyph::Box_>().unwrap();
        assert!(bx.layout.is_some());
        assert_eq!(bx.layout.as_ref().unwrap().children.len(), 2);
    }

    #[test]
    fn create_line_item_preserves_compositor_none() {
        // compositor None 이면 take 후 None 으로 복원 (== 그대로 None).
        let mut c = LRComposition::new(None, None, None, 100.0);
        let br = Break::default();
        let _ = c.create_line_item(&br, 0, 1);
        assert!(c.inner.compositor.is_none());
    }

    // -- B-5h: create_item (FUN_003000a8) ------------------------

    /// 테스트용 glyph — `compose` 가 모든 bt 에서 `can_break = true` 를 반환 (predecessor
    /// bt=3 포함 항상 composed Some), `request` 는 고정 Requisition 을 작성.
    #[derive(Debug, Clone)]
    struct AlwaysComposeGlyph {
        req: Requisition,
    }
    impl Glyph for AlwaysComposeGlyph {
        fn clone_glyph(&self) -> Box<dyn Glyph> { Box::new(self.clone()) }
        fn as_any(&self) -> &dyn std::any::Any { self }
        fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
        fn request(&self, out: &mut Requisition) { *out = self.req; }
        fn compose(&self, _bt: BreakType) -> crate::glyph::ComposeResult {
            crate::glyph::ComposeResult { replacement: None, can_break: true }
        }
    }

    fn always(nx: f32, sx: f32, kx: f32, ny: f32, sy: f32, ky: f32) -> Box<dyn Glyph> {
        Box::new(AlwaysComposeGlyph {
            req: Requisition {
                x: Requirement::new(nx, sx, kx, 0.0),
                y: Requirement::new(ny, sy, ky, 0.0),
                penalty: 0,
            },
        })
    }

    #[test]
    fn create_item_flag_write() {
        // raw 0x3000d8-0x3000ec: br.flags = (br.flags & ~3) | force | 2.
        let mut c = LRComposition::new(None, None, None, 100.0);
        // from=0, to=-1 → predecessor/main/successor 모두 skip (get_component 미접근).
        let mut br = Break::new(0, -1);
        br.flags = 0xFF;
        c.create_item(&mut br, 0, -1, false);
        assert_eq!(br.flags, 0xFE, "force=false → (0xFF & ~3) | 0 | 2");
        let mut br2 = Break::new(0, -1);
        br2.flags = 0xFF;
        c.create_item(&mut br2, 0, -1, true);
        assert_eq!(br2.flags, 0xFF, "force=true → (0xFF & ~3) | 1 | 2");
    }

    #[test]
    fn create_item_force_path_returns_placed_hbox() {
        // force + span ∈ (0, 1e8) → MonoGlyph{PlaceNatural{dir, span}, child = line HBox}.
        let mut c = LRComposition::new(None, None, None, 100.0);
        let mut br = Break::new(0, 3);
        let result = c.create_item(&mut br, 0, 3, true).expect("force path returns Some");
        let mg = result
            .as_any()
            .downcast_ref::<MonoGlyph>()
            .expect("force + valid span → Placement glyph (MonoGlyph)");
        let child = mg.child.as_ref().expect("body present");
        assert!(
            child.as_any().downcast_ref::<crate::glyph::Box_>().is_some(),
            "child = CreateLineItem 결과 HBox (Box_)"
        );
        let pn = mg
            .placement
            .as_any()
            .downcast_ref::<PlaceNatural>()
            .expect("strategy is PlaceNatural");
        assert_eq!(pn.direction, 0, "LR → PlaceNatural.direction = 0");
        assert_eq!(pn.span, 100.0);
    }

    #[test]
    fn create_item_force_path_bad_span_returns_line_item() {
        // force + span 무효 (>= 1e8) → CreateLineItem 결과(HBox)를 그대로 반환.
        let mut c = LRComposition::new(None, None, None, 1.0e8);
        let mut br = Break::new(0, 3);
        let result = c.create_item(&mut br, 0, 3, true).expect("returns line item");
        assert!(
            result.as_any().downcast_ref::<crate::glyph::Box_>().is_some(),
            "span 무효 → HBox(Box_)가 직접 반환"
        );
        assert!(result.as_any().downcast_ref::<MonoGlyph>().is_none());
    }

    #[test]
    fn create_item_normal_lr_span_invalid_combines_via_tile_align() {
        // LR + span 무효 → Tile(dir 0) 로 X 축 합산, Align(dir 1) 로 Y 축 결합 → Glue.
        let mut c = LRComposition::new(None, None, None, 1.0e8);
        // x = (10,2,1,0)/(20,3,1.5,0), y = INVALID → Align 이 skip.
        c.inner.items.push(Some(always(10.0, 2.0, 1.0, -1.0e8, 0.0, 0.0)));
        c.inner.items.push(Some(always(20.0, 3.0, 1.5, -1.0e8, 0.0, 0.0)));
        let mut br = Break::new(0, 1);
        let result = c.create_item(&mut br, 0, 1, false).expect("normal path returns Some(Glue)");
        let glue = result.as_any().downcast_ref::<Glue>().expect("normal path → Glue");
        // Tile(dir 0): natural=30, stretch=35-30=5, shrink=30-27.5=2.5, alignment=0.
        assert_eq!(glue.req.x, Requirement::new(30.0, 5.0, 2.5, 0.0));
        // Align(dir 1) on [y INVALID ×2]: 모두 skip → (0, 1e8, 1e8, 0).
        assert_eq!(glue.req.y, Requirement::new(0.0, 1.0e8, 1.0e8, 0.0));
        assert_eq!(glue.req.penalty, 0);
    }

    #[test]
    fn create_item_normal_lr_span_valid_skips_tile() {
        // LR + span 유효 → Tile 건너뛰고 out.x = {span,0,0,0} 직접 설정.
        let mut c = LRComposition::new(None, None, None, 50.0);
        c.inner.items.push(Some(always(10.0, 2.0, 1.0, -1.0e8, 0.0, 0.0)));
        c.inner.items.push(Some(always(20.0, 3.0, 1.5, -1.0e8, 0.0, 0.0)));
        let mut br = Break::new(0, 1);
        let result = c.create_item(&mut br, 0, 1, false).unwrap();
        let glue = result.as_any().downcast_ref::<Glue>().unwrap();
        // span 유효 → out.x = {natural: 50, 0, 0, 0} (Tile 합산 안 함).
        assert_eq!(glue.req.x, Requirement::new(50.0, 0.0, 0.0, 0.0));
        // Align 은 여전히 실행 → out.y = {0, 1e8, 1e8, 0}.
        assert_eq!(glue.req.y, Requirement::new(0.0, 1.0e8, 1.0e8, 0.0));
    }

    #[test]
    fn create_item_normal_pred_main_succ_counts() {
        // count=5, from=2, to=3 → predecessor(idx 1) + main(idx 2,3) + successor(idx 4) = 4 reqs.
        // 각 item x.natural=1 → Tile 합산 natural == 4.0 으로 결합 개수 확인.
        let mut c = LRComposition::new(None, None, None, 1.0e8);
        for _ in 0..5 {
            c.inner.items.push(Some(always(1.0, 0.0, 0.0, -1.0e8, 0.0, 0.0)));
        }
        let mut br = Break::new(2, 3);
        let result = c.create_item(&mut br, 2, 3, false).unwrap();
        let glue = result.as_any().downcast_ref::<Glue>().unwrap();
        assert_eq!(glue.req.x.natural, 4.0, "pred(1) + main(2) + succ(1) = 4 개 결합");
    }

    #[test]
    fn create_item_normal_tb_uses_tile_reverse() {
        // TB + span 무효 → TileReverse(dir 1) 로 Y 축 합산 (alignment=1.0), Align(dir 0) 로 X 결합.
        let mut c = TBComposition::new(None, None, None, 1.0e8);
        c.inner.items.push(Some(always(-1.0e8, 0.0, 0.0, 5.0, 1.0, 0.5)));
        c.inner.items.push(Some(always(-1.0e8, 0.0, 0.0, 8.0, 2.0, 1.0)));
        let mut br = Break::new(0, 1);
        let result = c.create_item(&mut br, 0, 1, false).unwrap();
        let glue = result.as_any().downcast_ref::<Glue>().unwrap();
        // TileReverse(dir 1): natural=13, stretch=16-13=3, shrink=13-11.5=1.5, alignment=1.0.
        assert_eq!(glue.req.y, Requirement::new(13.0, 3.0, 1.5, 1.0));
        // Align(dir 0) on [x INVALID ×2]: 모두 skip → (0, 1e8, 1e8, 0).
        assert_eq!(glue.req.x, Requirement::new(0.0, 1.0e8, 1.0e8, 0.0));
    }

    #[test]
    #[should_panic(expected = "out_of_range")]
    fn create_item_normal_predecessor_out_of_range_panics() {
        // from >= 1 인데 from-1 >= count → raw `throw out_of_range("GetAt")` 등가 panic.
        let mut c = LRComposition::new(None, None, None, 1.0e8);
        let mut br = Break::new(3, 5); // from=3 → predecessor idx=2, count=0 → panic.
        let _ = c.create_item(&mut br, 3, 5, false);
    }

    // -- B-5e: view (FUN_002ffe8c) -------------------------------

    /// 테스트용 parent_glyph — `replace(idx, child)` 호출을 (idx, child.is_some()) 로 기록.
    #[derive(Debug, Default)]
    struct RecordingParent {
        replace_calls: Vec<(usize, bool)>,
    }
    impl Glyph for RecordingParent {
        fn clone_glyph(&self) -> Box<dyn Glyph> {
            Box::new(RecordingParent { replace_calls: self.replace_calls.clone() })
        }
        fn as_any(&self) -> &dyn std::any::Any { self }
        fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
        fn replace(&mut self, idx: usize, child: Option<Box<dyn Glyph>>) {
            self.replace_calls.push((idx, child.is_some()));
        }
    }

    fn row_seg(begin: i32, end: i32, valid: bool) -> RowSegment {
        RowSegment {
            begin,
            end,
            origin_x: 0.0,
            origin_y: 0.0,
            flag: if valid { RowSegment::VALID_BIT } else { 0 },
        }
    }

    #[test]
    fn view_no_parent_glyph_returns_early() {
        // parent_glyph 없으면 즉시 return — flag_59 미변경 (ctor 의 true 유지).
        let mut c = LRComposition::new(None, None, None, 100.0);
        assert!(c.inner.flag_59, "ctor → flag_59 = true");
        c.view(0, 100);
        assert!(c.inner.flag_59, "parent 없음 → flag_59 그대로 true");
    }

    #[test]
    fn view_empty_rows_clears_flag_59() {
        // rows 비어 있으면 loop skip, flag_59 만 clear, parent_glyph 보존.
        let mut c = LRComposition::new(Some(Box::new(RecordingParent::default())), None, None, 100.0);
        c.view(0, 100);
        assert!(!c.inner.flag_59, "View → flag_59 = false");
        assert!(c.inner.parent_glyph.is_some(), "parent_glyph 보존 (refcount net 0)");
    }

    #[test]
    fn view_replaces_per_row_state() {
        // 4 종 row 상태를 한 view(0,5) 호출로 검증:
        //   row0 in-view+invalid  → CreateItem(force=true)  → Replace(0)
        //   row1 in-view+valid    → skip
        //   row2 out-of-view+valid → CreateItem(force=false) → Replace(4)
        //   row3 out-of-view+invalid → skip
        let mut c = LRComposition::new(Some(Box::new(RecordingParent::default())), None, None, 100.0);
        c.inner.rows.push(row_seg(0, 0, false)); // in view, invalid
        c.inner.rows.push(row_seg(0, 0, true)); // in view, valid
        c.inner.rows.push(row_seg(0, -1, true)); // out of view (end -1 < from 0), valid
        c.inner.rows.push(row_seg(0, -1, false)); // out of view, invalid
        c.view(0, 5);

        let parent = c.inner.parent_glyph.as_ref().unwrap();
        let rp = parent.as_any().downcast_ref::<RecordingParent>().unwrap();
        // row0 → Replace(0*2=0), row2 → Replace(2*2=4). 둘 다 child Some.
        assert_eq!(rp.replace_calls, vec![(0, true), (4, true)]);
        assert!(!c.inner.flag_59);
    }

    #[test]
    fn view_out_of_view_valid_uses_force_false_glue() {
        // out-of-view + valid 한 row → CreateItem(force=false) → Glue 가 Replace 됨.
        let mut c = LRComposition::new(Some(Box::new(RecordingParent::default())), None, None, 50.0);
        c.inner.rows.push(row_seg(0, -1, true)); // out of view for view(10, 20)
        c.view(10, 20);
        let rp = c.inner.parent_glyph.as_ref().unwrap()
            .as_any().downcast_ref::<RecordingParent>().unwrap();
        assert_eq!(rp.replace_calls, vec![(0, true)]);
    }

    // -- B-5i: do_repair (FUN_00301664) --------------------------

    /// 테스트용 parent_glyph — `remove`/`insert`/`replace` 호출을 (op, idx) 순서대로 기록.
    #[derive(Debug, Default)]
    struct RepairParent {
        ops: Vec<(&'static str, usize)>,
    }
    impl Glyph for RepairParent {
        fn clone_glyph(&self) -> Box<dyn Glyph> {
            Box::new(RepairParent { ops: self.ops.clone() })
        }
        fn as_any(&self) -> &dyn std::any::Any { self }
        fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
        fn remove(&mut self, idx: usize) {
            self.ops.push(("remove", idx));
        }
        fn insert(&mut self, idx: usize, _child: Option<Box<dyn Glyph>>) {
            self.ops.push(("insert", idx));
        }
        fn replace(&mut self, idx: usize, _child: Option<Box<dyn Glyph>>) {
            self.ops.push(("replace", idx));
        }
    }

    fn repair_row(begin: i32, end: i32, flag: u64) -> RowSegment {
        RowSegment { begin, end, origin_x: 0.0, origin_y: 0.0, flag }
    }

    fn repair_ops(c: &LRComposition) -> Vec<(&'static str, usize)> {
        c.inner
            .parent_glyph
            .as_ref()
            .unwrap()
            .as_any()
            .downcast_ref::<RepairParent>()
            .unwrap()
            .ops
            .clone()
    }

    #[test]
    fn do_repair_no_parent_returns_early() {
        // parent_glyph None → take() None → 즉시 return, rows 미변경.
        let mut c = LRComposition::new(None, None, None, 100.0);
        c.inner.rows.push(repair_row(0, 4, 0x1));
        c.do_repair(0, 0, 0, &[5], 1);
        assert_eq!(c.inner.rows.len(), 1);
        assert_eq!(c.inner.rows[0].end, 4);
    }

    #[test]
    fn do_repair_count_zero_noop() {
        // param_5 == 0 → loop skip, parent put-back, rows 미변경, parent op 없음.
        let mut c = LRComposition::new(Some(Box::new(RepairParent::default())), None, None, 100.0);
        c.inner.rows.push(repair_row(0, 4, 0x1));
        c.do_repair(0, 0, 0, &[], 0);
        assert_eq!(c.inner.rows.len(), 1);
        assert!(c.inner.parent_glyph.is_some());
        assert_eq!(repair_ops(&c), Vec::<(&'static str, usize)>::new());
    }

    #[test]
    fn do_repair_append_inserts_new_row() {
        // 빈 rows + lVar12 == row_count (== 0) → INSERT path.
        let mut c = LRComposition::new(Some(Box::new(RepairParent::default())), None, None, 100.0);
        c.do_repair(0, 0, 0, &[5], 1);
        // INSERT: parent.Insert(0, item), parent.Insert(1, separator).
        assert_eq!(repair_ops(&c), vec![("insert", 0), ("insert", 1)]);
        // rows.insert(0, temp): temp.begin = 0+0, temp.end = (0-1)+5 = 4.
        assert_eq!(c.inner.rows.len(), 1);
        assert_eq!(c.inner.rows[0].begin, 0);
        assert_eq!(c.inner.rows[0].end, 4);
        // CreateItem force path (flag_59 ctor default = true): br.flags = (0 & ~3) | 1 | 2 = 3.
        assert_eq!(c.inner.rows[0].flag, 3);
    }

    #[test]
    fn do_repair_skip_unchanged_row() {
        // 기존 row 가 flag bit1 set + begin/end 일치 → SKIP, parent op 없음, row 미변경.
        let mut c = LRComposition::new(Some(Box::new(RepairParent::default())), None, None, 100.0);
        c.inner.rows.push(repair_row(0, 4, 0x2)); // bit1 set, begin=0, end=4
        // temp.begin = 0, temp.end = (0-1)+5 = 4 → 일치.
        c.do_repair(0, 0, 0, &[5], 1);
        assert_eq!(repair_ops(&c), Vec::<(&'static str, usize)>::new());
        assert_eq!(c.inner.rows.len(), 1);
        assert_eq!(c.inner.rows[0].flag, 0x2);
    }

    #[test]
    fn do_repair_replace_damaged_last_row() {
        // 기존 row (flag bit1 미set) → SKIP 안 함; uVar13 == param_5-1 + DAMAGE +
        // existing.begin <= temp.to+1 → REPLACE path.
        let mut c = LRComposition::new(Some(Box::new(RepairParent::default())), None, None, 100.0);
        c.inner.rows.push(repair_row(0, 2, 0x0));
        c.do_repair(0, 0, 0, &[5], 1);
        // REPLACE: parent.Replace(0, item), parent.Replace(1, separator).
        assert_eq!(repair_ops(&c), vec![("replace", 0), ("replace", 1)]);
        // rows[0] = temp: begin=0, end=4, flag=3 (CreateItem force path).
        assert_eq!(c.inner.rows.len(), 1);
        assert_eq!(c.inner.rows[0].begin, 0);
        assert_eq!(c.inner.rows[0].end, 4);
        assert_eq!(c.inner.rows[0].flag, 3);
    }

    #[test]
    fn do_repair_deletion_absorbs_following_rows() {
        // rows[lVar12+1].end <= temp.to → deletion loop 가 rows[lVar12] 를 흡수 삭제.
        let mut c = LRComposition::new(Some(Box::new(RepairParent::default())), None, None, 100.0);
        c.inner.rows.push(repair_row(0, 2, 0x0));
        c.inner.rows.push(repair_row(3, 4, 0x0));
        c.inner.rows.push(repair_row(8, 10, 0x0));
        // temp.end = (0-1)+6 = 5. rows[1].end = 4 <= 5 → 1회 삭제. 그 후 rows[1].end = 10 > 5 → stop.
        c.do_repair(0, 0, 0, &[6], 1);
        // 삭제: parent.Remove((0*2)|1=1) → parent.Remove(0); 그 후 REPLACE (DAMAGE, 마지막 iter).
        assert_eq!(
            repair_ops(&c),
            vec![("remove", 1), ("remove", 0), ("replace", 0), ("replace", 1)]
        );
        // 삭제 후 rows = [{3,4},{8,10}] → REPLACE 로 rows[0] = temp {0,5,flag:3}.
        assert_eq!(c.inner.rows.len(), 2);
        assert_eq!((c.inner.rows[0].begin, c.inner.rows[0].end, c.inner.rows[0].flag), (0, 5, 3));
        assert_eq!((c.inner.rows[1].begin, c.inner.rows[1].end), (8, 10));
    }

    #[test]
    fn do_repair_no_damage_then_replace_two_iters() {
        // param_5 = 2: iter0 = no_damage(INSERT), iter1 = DAMAGE 마지막(REPLACE).
        let mut c = LRComposition::new(Some(Box::new(RepairParent::default())), None, None, 100.0);
        c.inner.rows.push(repair_row(0, 10, 0x0));
        // iter0 (uVar13=0): temp.begin=0, temp.end=(0-1)+3=2.
        //   no_damage = 0<1 && rows[0].end(10) >= (0-1)+breaks[1](8) → 10>=7 true → INSERT.
        //   rows.insert(0, {0,2}) → rows = [{0,2},{0,10}], lVar12=1.
        // iter1 (uVar13=1): row_count=2, lVar12=1. iVar6 = breaks[0]+1 = 4.
        //   temp.begin = 4, temp.end = (0-1)+8 = 7. existing = rows[1] = {0,10}.
        //   no_damage = 1<1 false → DAMAGE; uVar13==1==param_5-1 → existing.begin(0)<=temp.to+1(8)
        //   → REPLACE. rows[1] = {4,7}.
        c.do_repair(0, 0, 0, &[3, 8], 2);
        assert_eq!(
            repair_ops(&c),
            vec![("insert", 0), ("insert", 1), ("replace", 2), ("replace", 3)]
        );
        assert_eq!(c.inner.rows.len(), 2);
        assert_eq!((c.inner.rows[0].begin, c.inner.rows[0].end), (0, 2));
        assert_eq!((c.inner.rows[1].begin, c.inner.rows[1].end), (4, 7));
    }

    // -- B-5j: repair (FUN_00300b14) -----------------------------

    #[test]
    fn repair_no_damage_returns_true() {
        // has_damage == false → 아무것도 안 하고 return true (!had_damage).
        let mut c = LRComposition::new(None, None, None, 100.0);
        c.inner.has_damage = false;
        c.inner.rows.push(repair_row(0, 4, 0x1));
        assert!(c.repair(&ZeroCt, &ZeroGm), "no damage → returns true");
        // rows / has_damage 미변경.
        assert_eq!(c.inner.rows.len(), 1);
        assert!(!c.inner.has_damage);
    }

    #[test]
    fn repair_no_items_clears_damage() {
        // count == 0 → FindPrevForcedBreak(damage_begin=0) = 0 (count <= start).
        //   gate `seg_start < count-1` = `0 < -1` = false → main loop skip.
        //   has_damage clear, return false.
        let mut c = LRComposition::new(None, None, None, 100.0);
        c.inner.has_damage = true;
        c.inner.damage_begin = 0;
        c.inner.damage_end = 100;
        assert!(!c.repair(&ZeroCt, &ZeroGm), "had damage → returns false");
        assert!(!c.inner.has_damage, "repair clears has_damage");
        assert!(c.inner.rows.is_empty());
    }

    #[test]
    fn repair_main_loop_lays_out_lines() {
        // 통합 테스트: damage 있는 2-item paragraph 를 repair.
        //   seg_start = FindPrevForcedBreak(0,false) = -1 (forced break 없음).
        //   gate -1 < count-1(1) → main loop 1회. 측정 → ComposeBreak → DoRepair.
        let mut c = LRComposition::new(
            Some(Box::new(RepairParent::default())),
            Some(Box::new(crate::compositor::ColCompositor::new(100.0, 0))),
            None,
            1000.0, // span 큼 → ComposeBreak fit path
        );
        // 2 items: x.natural=10, y INVALID, penalty 0 (forced break 아님).
        c.inner.items.push(Some(always(10.0, 0.0, 0.0, -1.0e8, 0.0, 0.0)));
        c.inner.items.push(Some(always(10.0, 0.0, 0.0, -1.0e8, 0.0, 0.0)));
        c.inner.has_damage = true;
        c.inner.damage_begin = 0;
        c.inner.damage_end = 100;

        let result = c.repair(&ZeroCt, &ZeroGm);
        assert!(!result, "had damage → returns false");
        assert!(!c.inner.has_damage, "repair clears has_damage");
        // compositor 는 compose_break 동안 take 됐다가 복원돼야 함.
        assert!(c.inner.compositor.is_some(), "compositor restored after repair");
        // DoRepair 가 ComposeBreak 결과(>=1 line)를 rows 에 insert.
        assert!(!c.inner.rows.is_empty(), "do_repair inserted >= 1 row");
        // parent_glyph 에 Insert 호출 기록 (append 경로: idx*2, idx*2+1).
        let ops = repair_ops(&c);
        assert!(
            ops.iter().any(|(op, _)| *op == "insert"),
            "parent_glyph got Insert ops from do_repair, got {:?}",
            ops
        );
    }

    #[test]
    fn repair_returns_true_only_when_clean() {
        // had_damage == true 이면 main loop 진입 여부와 무관하게 항상 false 반환.
        let mut c = LRComposition::new(None, None, None, 100.0);
        c.inner.has_damage = true;
        c.inner.damage_begin = 0;
        c.inner.damage_end = 0;
        assert!(!c.repair(&ZeroCt, &ZeroGm));
        // 두 번째 호출: 이제 has_damage = false → true.
        assert!(c.repair(&ZeroCt, &ZeroGm));
    }

    // -- accessor 1:1 tests (raw asm 동등성 검증) -----------------

    fn empty_state() -> CompositionState {
        CompositionState::new(None, None, None, CompositionDirection::LR, 1.0)
    }

    fn mk_state_with_rows(rows: Vec<RowSegment>) -> CompositionState {
        let mut s = empty_state();
        s.rows = rows;
        s
    }

    fn row(begin: i32, end: i32) -> RowSegment {
        RowSegment { begin, end, origin_x: 0.0, origin_y: 0.0, flag: RowSegment::VALID_BIT }
    }

    #[test]
    fn get_count_returns_items_len() {
        let mut s = empty_state();
        assert_eq!(s.get_count(), 0);
        s.items.push(None);
        s.items.push(None);
        assert_eq!(s.get_count(), 2);
    }

    #[test]
    fn set_span_changes_and_damage_set() {
        let mut s = empty_state();
        s.has_damage = false;
        s.set_span(99.5);
        assert_eq!(s.get_span(), 99.5);
        assert!(s.has_damage);
    }

    #[test]
    fn set_span_unchanged_keeps_no_damage() {
        let mut s = empty_state();
        s.has_damage = false;
        s.set_span(1.0); // ctor default = 1.0
        assert!(!s.has_damage);
    }

    #[test]
    fn set_damage_first_time_initializes() {
        let mut s = empty_state();
        s.has_damage = false;
        s.set_damage(5, 10);
        assert_eq!(s.damage_begin, 5);
        assert_eq!(s.damage_end, 10);
        assert!(s.has_damage);
    }

    #[test]
    fn set_damage_with_existing_merges_min_max() {
        let mut s = empty_state();
        s.has_damage = true;
        s.damage_begin = 3;
        s.damage_end = 8;
        s.set_damage(1, 10);
        assert_eq!(s.damage_begin, 1);
        assert_eq!(s.damage_end, 10);

        s.set_damage(5, 6);
        assert_eq!(s.damage_begin, 1);
        assert_eq!(s.damage_end, 10);
    }

    #[test]
    fn get_begin_end_of_basic() {
        let s = mk_state_with_rows(vec![row(0, 4), row(5, 9)]);
        assert_eq!(s.get_begin_of(0), 0);
        assert_eq!(s.get_end_of(0), 4);
        assert_eq!(s.get_begin_of(2), 5); // row_idx*2 = 2 → row 1
        assert_eq!(s.get_end_of(2), 9);
    }

    #[test]
    fn get_begin_of_clamps_to_last() {
        let s = mk_state_with_rows(vec![row(0, 4), row(5, 9)]);
        // line_idx=10 → row_idx=5 → clamped to last_row=1
        assert_eq!(s.get_begin_of(10), 5);
    }

    #[test]
    fn get_begin_of_odd_line_idx_truncates() {
        let s = mk_state_with_rows(vec![row(0, 4), row(5, 9)]);
        // line_idx=1, signed shift right → 0
        assert_eq!(s.get_begin_of(1), 0);
        // line_idx=3 → 1
        assert_eq!(s.get_begin_of(3), 5);
    }

    #[test]
    fn get_begin_of_negative_line_idx_adj() {
        let s = mk_state_with_rows(vec![row(0, 4), row(5, 9)]);
        // -1 → -1+1=0 → 0>>1=0 → row 0
        assert_eq!(s.get_begin_of(-1), 0);
        // -2 → -1 → -1>>1=-1 (signed) → clamped to 0
        assert_eq!(s.get_begin_of(-2), 0);
    }

    #[test]
    fn get_begin_of_empty_rows_returns_zero() {
        let s = empty_state();
        assert_eq!(s.get_begin_of(0), 0);
        assert_eq!(s.get_end_of(2), 0);
    }

    #[test]
    fn get_index_of_forward_walk_finds_row() {
        let s = mk_state_with_rows(vec![row(0, 4), row(5, 9), row(10, 14)]);
        // iter_cache=0, char_idx=10 → walks forward to row 2
        assert_eq!(s.get_index_of(10), 4); // row_idx 2 * 2
    }

    #[test]
    fn get_index_of_backward_walk_finds_row() {
        let s = mk_state_with_rows(vec![row(0, 4), row(5, 9), row(10, 14)]);
        s.iter_cache.set(2);
        // char_idx=0 — backward walk from 2 to 0
        assert_eq!(s.get_index_of(0), 0);
    }

    #[test]
    fn get_index_of_exact_row_no_walk() {
        let s = mk_state_with_rows(vec![row(0, 4), row(5, 9)]);
        s.iter_cache.set(0);
        // char_idx=4 — rows[0].end=4 >= 4, found immediately
        assert_eq!(s.get_index_of(4), 0);
        s.iter_cache.set(1);
        // char_idx=7 — rows[1].end=9 >= 7, found
        assert_eq!(s.get_index_of(7), 2);
    }

    #[test]
    fn get_index_of_empty_rows() {
        let s = empty_state();
        assert_eq!(s.get_index_of(0), 0);
        assert_eq!(s.get_index_of(100), 0);
    }

    #[test]
    fn get_index_of_updates_iter_cache() {
        let s = mk_state_with_rows(vec![row(0, 4), row(5, 9), row(10, 14)]);
        s.iter_cache.set(0);
        let _ = s.get_index_of(12);
        // forward walk advances iter_cache to row containing 12 = row 2
        assert_eq!(s.iter_cache.get(), 2);
    }

    // -- SetMargin 1:1 tests (한컴 quirk: row 자체는 안 바꾸고 damage 만) -----

    fn row_full(begin: i32, end: i32, ox: f32, oy: f32) -> RowSegment {
        RowSegment { begin, end, origin_x: ox, origin_y: oy, flag: RowSegment::VALID_BIT }
    }

    #[test]
    fn set_margin_odd_line_idx_noop() {
        let mut s = mk_state_with_rows(vec![row_full(0, 5, 1.0, 2.0)]);
        s.has_damage = false;
        s.damage_begin = 100;
        s.damage_end = 200;
        s.set_margin(1, 3.0, 4.0); // odd → no-op
        assert!(!s.has_damage);
        assert_eq!(s.damage_begin, 100);
    }

    #[test]
    fn set_margin_unchanged_origin_noop() {
        let mut s = mk_state_with_rows(vec![row_full(2, 7, 1.0, 2.0)]);
        s.has_damage = false;
        s.set_margin(0, 1.0, 2.0); // same margins → no-op
        assert!(!s.has_damage);
    }

    #[test]
    fn set_margin_changed_origin_sets_damage() {
        let mut s = mk_state_with_rows(vec![row_full(2, 7, 1.0, 2.0)]);
        s.has_damage = false;
        s.set_margin(0, 3.0, 4.0);
        assert!(s.has_damage);
        assert_eq!(s.damage_begin, 1); // begin - 1 = 2 - 1
        assert_eq!(s.damage_end, 8);   // end + 1 = 7 + 1
        // 한컴 quirk: row 자체는 안 변함
        assert_eq!(s.rows[0].origin_x, 1.0);
        assert_eq!(s.rows[0].origin_y, 2.0);
    }

    #[test]
    fn set_margin_nan_treated_as_changed() {
        // ARM64 fcmp+fccmp(eq) — NaN 비교는 b.eq not taken → update 진입
        // Rust f32 `==` 도 NaN 시 false → 동등 동작
        let mut s = mk_state_with_rows(vec![row_full(0, 5, f32::NAN, 2.0)]);
        s.has_damage = false;
        s.set_margin(0, 1.0, 2.0);
        assert!(s.has_damage); // NaN 비교는 != 로 처리되어 update 진입
    }

    #[test]
    fn set_margin_out_of_range_noop() {
        let mut s = mk_state_with_rows(vec![row_full(0, 5, 1.0, 2.0)]);
        s.has_damage = false;
        s.set_margin(10, 99.0, 99.0); // row_idx = 5, out of range
        assert!(!s.has_damage);
    }

    // -- Composition::Clone 1:1 tests (deep copy semantics) ----

    #[test]
    fn lr_composition_clone_preserves_fields() {
        let mut original = LRComposition::new(None, None, None, 42.5);
        original.inner.damage_begin = 5;
        original.inner.damage_end = 10;
        original.inner.has_damage = true;
        original.inner.iter_cache.set(3);
        original.inner.rows = vec![row_full(1, 4, 0.5, 1.5), row_full(5, 9, 2.5, 3.5)];

        let cloned_box = original.clone_glyph();
        let cloned = cloned_box.as_any().downcast_ref::<LRComposition>().unwrap();
        assert_eq!(cloned.inner.direction, CompositionDirection::LR);
        assert_eq!(cloned.inner.span, 42.5);
        assert_eq!(cloned.inner.damage_begin, 5);
        assert_eq!(cloned.inner.damage_end, 10);
        assert!(cloned.inner.has_damage);
        assert_eq!(cloned.inner.iter_cache.get(), 3);
        assert_eq!(cloned.inner.rows.len(), 2);
        assert_eq!(cloned.inner.rows[0].begin, 1);
        assert_eq!(cloned.inner.rows[1].end, 9);
    }

    #[test]
    fn tb_composition_clone_keeps_tb_direction() {
        let original = TBComposition::new(None, None, None, 7.0);
        let cloned_box = original.clone_glyph();
        let cloned = cloned_box.as_any().downcast_ref::<TBComposition>().unwrap();
        assert_eq!(cloned.inner.direction, CompositionDirection::TB);
        assert_eq!(cloned.inner.span, 7.0);
    }

    // -- GetAllotment with no parent_glyph ----

    #[test]
    fn get_allotment_no_parent_returns_zero() {
        let s = mk_state_with_rows(vec![row_full(0, 5, 0.0, 0.0)]);
        let mut out = Allotment { origin: 99.0, span: 99.0, alignment: 99.0 };
        s.get_allotment(3, Dimension::X, &mut out);
        // parent_glyph None → out 은 ZERO (한컴 base default)
        assert_eq!(out, Allotment::ZERO);
    }

    // -- FindPrev/NextForcedBreak 1:1 tests ----------------------
    //
    // 한컴 raw asm 의 forced-break penalty 값:
    //   - -10000: Forced break (paragraph end 등). 항상 break.
    //   - -1000:  Penalty break. `force_only == false` 시만 break.
    //   - 그 외:  normal item.

    /// 테스트용 Glyph — `request()` 가 설정된 penalty 를 그대로 출력.
    #[derive(Debug)]
    struct PenaltyGlyph {
        penalty: i32,
    }
    impl Glyph for PenaltyGlyph {
        fn clone_glyph(&self) -> Box<dyn Glyph> {
            Box::new(PenaltyGlyph { penalty: self.penalty })
        }
        fn as_any(&self) -> &dyn std::any::Any { self }
        fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
        fn request(&self, req_out: &mut Requisition) {
            // raw forced-break Glyph (e.g., ParaItemView::Request) 는 penalty 만 채움.
            req_out.set_penalty(self.penalty);
        }
    }

    fn make_state_with_penalties(penalties: &[Option<i32>]) -> CompositionState {
        let mut s = empty_state();
        for p in penalties {
            match p {
                None => s.items.push(None),
                Some(pp) => s.items.push(Some(Box::new(PenaltyGlyph { penalty: *pp }))),
            }
        }
        s
    }

    #[test]
    fn find_prev_negative_start_returns_start() {
        let s = make_state_with_penalties(&[Some(0), Some(-10000)]);
        // raw 003012e8: tbnz w1,#0x1f → return start
        assert_eq!(s.find_prev_forced_break(-1, false), -1);
        assert_eq!(s.find_prev_forced_break(-5, true), -5);
    }

    #[test]
    fn find_prev_start_ge_count_returns_start() {
        let s = make_state_with_penalties(&[Some(0)]);
        // raw 003012f4-f8: b.le epilogue
        assert_eq!(s.find_prev_forced_break(1, false), 1);
        assert_eq!(s.find_prev_forced_break(5, false), 5);
    }

    #[test]
    fn find_prev_finds_forced_at_start() {
        let s = make_state_with_penalties(&[Some(-10000), Some(0), Some(0)]);
        // start=0 에 -10000 → 즉시 FOUND
        assert_eq!(s.find_prev_forced_break(0, false), 0);
        assert_eq!(s.find_prev_forced_break(0, true), 0);
    }

    #[test]
    fn find_prev_walks_back_to_forced() {
        let s = make_state_with_penalties(&[Some(-10000), Some(0), Some(0), Some(0)]);
        // start=3 → walk back: 3 → 2 → 1 → 0 (FOUND -10000)
        assert_eq!(s.find_prev_forced_break(3, false), 0);
        assert_eq!(s.find_prev_forced_break(3, true), 0);
    }

    #[test]
    fn find_prev_walks_off_front_returns_neg_one() {
        let s = make_state_with_penalties(&[Some(0), Some(0), Some(0)]);
        // 모든 item 이 normal → idx 0 까지 walk back → -1
        assert_eq!(s.find_prev_forced_break(2, false), -1);
    }

    #[test]
    fn find_prev_penalty_minus_1000_force_only_skipped() {
        let s = make_state_with_penalties(&[Some(0), Some(-1000), Some(0)]);
        // start=2, force_only=true → -1000 무시, walk off front → -1
        assert_eq!(s.find_prev_forced_break(2, true), -1);
        // force_only=false → -1000 break → idx 1
        assert_eq!(s.find_prev_forced_break(2, false), 1);
    }

    #[test]
    fn find_prev_null_holder_skipped() {
        let s = make_state_with_penalties(&[Some(-10000), None, None, Some(0)]);
        // start=3 → 3(normal) → 2(null) → 1(null) → 0(-10000 FOUND)
        assert_eq!(s.find_prev_forced_break(3, false), 0);
    }

    #[test]
    fn find_next_count_le_start_returns_count_minus_one() {
        let s = make_state_with_penalties(&[Some(0), Some(0)]);
        // start=2 == count → 즉시 epilogue → count - 1 = 1
        assert_eq!(s.find_next_forced_break(2, false), 1);
        // start=5 > count → 동일
        assert_eq!(s.find_next_forced_break(5, false), 1);
    }

    #[test]
    fn find_next_empty_returns_neg_one() {
        let s = empty_state();
        // count = 0, start = 0 → cmp 0 <= 0 → epilogue → count - 1 = -1
        assert_eq!(s.find_next_forced_break(0, false), -1);
    }

    #[test]
    fn find_next_finds_forced_at_start() {
        let s = make_state_with_penalties(&[Some(-10000), Some(0), Some(0)]);
        assert_eq!(s.find_next_forced_break(0, false), 0);
        assert_eq!(s.find_next_forced_break(0, true), 0);
    }

    #[test]
    fn find_next_walks_forward_to_forced() {
        let s = make_state_with_penalties(&[Some(0), Some(0), Some(0), Some(-10000)]);
        // start=0 → walk forward: 0 → 1 → 2 → 3 (FOUND)
        assert_eq!(s.find_next_forced_break(0, false), 3);
    }

    #[test]
    fn find_next_walks_off_end_returns_count_minus_one() {
        let s = make_state_with_penalties(&[Some(0), Some(0), Some(0)]);
        // 모든 normal → idx 가 count=3 까지 → return min(3, 2) = 2 = count - 1
        assert_eq!(s.find_next_forced_break(0, false), 2);
    }

    #[test]
    fn find_next_penalty_minus_1000_force_only_skipped() {
        let s = make_state_with_penalties(&[Some(0), Some(-1000), Some(0)]);
        // force_only=true → -1000 skip, walk off end → count - 1 = 2
        assert_eq!(s.find_next_forced_break(0, true), 2);
        // force_only=false → -1000 break → idx 1
        assert_eq!(s.find_next_forced_break(0, false), 1);
    }

    #[test]
    fn find_next_null_holder_skipped() {
        let s = make_state_with_penalties(&[Some(0), None, None, Some(-10000)]);
        // 0(normal) → 1(null) → 2(null) → 3(-10000 FOUND)
        assert_eq!(s.find_next_forced_break(0, false), 3);
    }

    #[test]
    #[should_panic(expected = "FindNextForcedBreak: negative start")]
    fn find_next_negative_start_panics() {
        let s = make_state_with_penalties(&[Some(0), Some(0)]);
        // count > start signed (2 > -1) → 통과 → unsigned check 발사 → panic
        let _ = s.find_next_forced_break(-1, false);
    }

    #[test]
    fn find_prev_request_does_not_pollute_state() {
        // Glyph::request 가 받은 req 의 penalty 만 변경 — x/y 의 -1e8 sentinel 은 그대로.
        // FindPrev/Next 는 매 iter 마다 req 를 새로 초기화하므로 누적 오염 없음.
        let s = make_state_with_penalties(&[Some(0), Some(0), Some(-10000)]);
        let r = s.find_prev_forced_break(2, false);
        assert_eq!(r, 2); // immediate FOUND at start
    }

    // -- GetSeparator 1:1 tests ----------------------------------
    //
    // 라인 끝의 separator 결정 알고리즘:
    //   - 마지막 라인 (to >= count - 1) → 저장된 separator 반환
    //   - 중간 라인 → 다음 item 에 Compose(Forced) 호출, 결과가 있으면 그것, 없으면 separator

    use crate::glyph::ComposeResult;

    /// 테스트용 Glyph — `compose()` 가 설정된 (replacement, can_break) 반환.
    #[derive(Debug)]
    struct ComposingGlyph {
        replacement_id: Option<i32>,
        can_break: bool,
        marker: i32,
    }
    impl Glyph for ComposingGlyph {
        fn clone_glyph(&self) -> Box<dyn Glyph> {
            Box::new(ComposingGlyph {
                replacement_id: self.replacement_id,
                can_break: self.can_break,
                marker: self.marker,
            })
        }
        fn as_any(&self) -> &dyn std::any::Any { self }
        fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
        fn compose(&self, _bt: BreakType) -> ComposeResult {
            ComposeResult {
                replacement: self.replacement_id.map(|id| {
                    Box::new(ComposingGlyph {
                        replacement_id: None,
                        can_break: false,
                        marker: id,
                    }) as Box<dyn Glyph>
                }),
                can_break: self.can_break,
            }
        }
    }

    fn make_state_with_separator_and_items(
        sep_marker: Option<i32>,
        items_spec: &[Option<(Option<i32>, bool, i32)>],
    ) -> CompositionState {
        let separator: Option<Box<dyn Glyph>> = sep_marker.map(|m| {
            Box::new(ComposingGlyph {
                replacement_id: None,
                can_break: false,
                marker: m,
            }) as Box<dyn Glyph>
        });
        let mut s = CompositionState::new(None, None, separator, CompositionDirection::LR, 1.0);
        for spec in items_spec {
            match spec {
                None => s.items.push(None),
                Some((rep, cb, m)) => {
                    s.items.push(Some(Box::new(ComposingGlyph {
                        replacement_id: *rep,
                        can_break: *cb,
                        marker: *m,
                    })));
                }
            }
        }
        s
    }

    fn marker_of(g: &dyn Glyph) -> i32 {
        g.as_any().downcast_ref::<ComposingGlyph>().unwrap().marker
    }

    #[test]
    fn get_separator_last_line_returns_separator() {
        // count = 3, to = 2 (마지막 line) → separator 반환
        let s = make_state_with_separator_and_items(
            Some(99),
            &[Some((None, false, 1)), Some((None, false, 2)), Some((None, false, 3))],
        );
        let br = Break::new(0, 2);
        let r = s.get_separator(&br).unwrap();
        assert_eq!(marker_of(&*r), 99); // separator's marker
    }

    #[test]
    fn get_separator_to_equals_count_minus_one_returns_separator() {
        // to = count - 1 (마지막 line) → separator
        let s = make_state_with_separator_and_items(Some(99), &[Some((None, false, 1)), Some((None, false, 2))]);
        let br = Break::new(0, 1);
        let r = s.get_separator(&br).unwrap();
        assert_eq!(marker_of(&*r), 99);
    }

    #[test]
    fn get_separator_compose_returns_replacement_used() {
        // 중간 라인: to=0, items[1] 의 compose 가 replacement (marker=42) 반환 → 42 사용
        let s = make_state_with_separator_and_items(
            Some(99),
            &[
                Some((None, false, 1)),
                Some((Some(42), false, 2)), // compose → replacement marker=42
                Some((None, false, 3)),
            ],
        );
        let br = Break::new(0, 0);
        let r = s.get_separator(&br).unwrap();
        assert_eq!(marker_of(&*r), 42);
    }

    #[test]
    fn get_separator_compose_no_replacement_can_break_uses_item() {
        // 중간 라인: items[1] 의 compose returns (None, can_break=true) → input 자신 사용
        // bt = Forced (2). can_break=true 이지만 forced 일 때 compose 어떻게?
        // composition_compose_glyph: replacement None + can_break true → Some(input.clone())
        let s = make_state_with_separator_and_items(
            Some(99),
            &[
                Some((None, false, 1)),
                Some((None, true, 7)),  // marker=7, can_break=true → use input (clone w/ marker=7)
                Some((None, false, 3)),
            ],
        );
        let br = Break::new(0, 0);
        let r = s.get_separator(&br).unwrap();
        assert_eq!(marker_of(&*r), 7);
    }

    #[test]
    fn get_separator_compose_none_falls_back_to_separator() {
        // items[1] 의 compose: replacement None, can_break false → returns None → fallback separator
        let s = make_state_with_separator_and_items(
            Some(99),
            &[
                Some((None, false, 1)),
                Some((None, false, 2)),
                Some((None, false, 3)),
            ],
        );
        let br = Break::new(0, 0);
        let r = s.get_separator(&br).unwrap();
        assert_eq!(marker_of(&*r), 99); // separator
    }

    #[test]
    fn get_separator_null_item_at_to_plus_one_falls_back() {
        // items[1] is None → compose returns None → separator
        let s = make_state_with_separator_and_items(
            Some(99),
            &[Some((None, false, 1)), None, Some((None, false, 3))],
        );
        let br = Break::new(0, 0);
        let r = s.get_separator(&br).unwrap();
        assert_eq!(marker_of(&*r), 99);
    }

    #[test]
    fn get_separator_no_separator_set_returns_none() {
        // separator None + compose None → None
        let s = make_state_with_separator_and_items(
            None,
            &[Some((None, false, 1)), Some((None, false, 2)), Some((None, false, 3))],
        );
        let br = Break::new(0, 0); // 중간 line
        let r = s.get_separator(&br);
        assert!(r.is_none());
    }

    #[test]
    fn get_separator_empty_composition() {
        // count=0, to=anything ≥ -1 → 항상 separator (count - 1 = -1 <= to for to >= -1)
        let s = make_state_with_separator_and_items(Some(99), &[]);
        let br = Break::new(0, 0);
        let r = s.get_separator(&br).unwrap();
        assert_eq!(marker_of(&*r), 99);
    }

    // -- GetComponentPtr 1:1 tests --------------------------------

    #[test]
    fn get_component_ptr_returns_clone_of_item() {
        let s = make_state_with_separator_and_items(
            None,
            &[Some((None, false, 11)), Some((None, false, 22)), Some((None, false, 33))],
        );
        let r = s.get_component_ptr(1).unwrap();
        assert_eq!(marker_of(&*r), 22);
    }

    #[test]
    fn get_component_ptr_null_item_returns_none() {
        let s = make_state_with_separator_and_items(None, &[Some((None, false, 1)), None, Some((None, false, 3))]);
        assert!(s.get_component_ptr(1).is_none());
    }

    #[test]
    #[should_panic(expected = "GetComponentPtr: negative idx")]
    fn get_component_ptr_negative_idx_panics() {
        let s = make_state_with_separator_and_items(None, &[Some((None, false, 1))]);
        let _ = s.get_component_ptr(-1);
    }

    #[test]
    #[should_panic(expected = "GetComponentPtr: idx=5 >= count=2")]
    fn get_component_ptr_out_of_range_panics() {
        let s = make_state_with_separator_and_items(None, &[Some((None, false, 1)), Some((None, false, 2))]);
        let _ = s.get_component_ptr(5);
    }

    #[test]
    fn get_component_ptr_clone_is_independent_box() {
        // 두 번 호출하면 두 개의 별도 Box 가 나옴 (refcount 의미는 ownership 으로 대체).
        let s = make_state_with_separator_and_items(None, &[Some((None, false, 42))]);
        let r1 = s.get_component_ptr(0).unwrap();
        let r2 = s.get_component_ptr(0).unwrap();
        assert_eq!(marker_of(&*r1), 42);
        assert_eq!(marker_of(&*r2), 42);
    }
}

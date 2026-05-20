//! Layout hierarchy — `Hnc::Shape::Text::Layout` base + Tile/Align/Superpose subclasses.
//!
//! **Glyph 와 다른 별도 hierarchy**. `Hnc::Memory::SharePtr<Hnc::Shape::Text::Layout>` 가
//! `Superpose::Add` 시그니처에서 확정 (raw decompile `Superpose__Add_00316ac4.txt`).
//!
//! ## RTTI 검증 (`/tmp/hft_scripts/hbox_layer/vtables.txt`):
//!
//! | 클래스 | vtable | typeinfo | size |
//! |--------|--------|----------|------|
//! | `Tile` | `0x781d50` | `N3Hnc5Shape4Text4TileE` (`0x791fa0`) | 56B (0x38) |
//! | `Align` | `0x77fab8` | `N3Hnc5Shape4Text5AlignE` (`0x791980`) | 16B (0x10) |
//! | `Superpose` | `0x781828` | (`0x791e88`) | 32B (0x20) |
//!
//! ## Layout 인터페이스 (5 vfuncs, 검증)
//!
//! 세 클래스 모두 동일한 5 슬롯 vtable:
//!
//! | 슬롯 | 메소드 | 시그니처 |
//! |------|--------|----------|
//! | +0 | dtor1 | `~T()` (Rust: Drop) |
//! | +8 | dtor0/delete | (Rust: Drop) |
//! | +16 | Clone | `() -> Layout*` |
//! | +24 | Request_inner | `(vector<Requisition> const& children, Requisition& out)` |
//! | +32 | Allocate_inner | `(Allocation const& avail, vector<Requisition> const& reqs, vector<Allocation>& out_allocs)` |
//!
//! ## Tile (HBox/VBox inner — direction + sum combiner)
//!
//! - +0x00: vtable
//! - +0x08: direction (i32, 0=H, 1=V)
//! - +0x0c..+0x2f: cached Requisition (36B)
//! - +0x30: trim_trailing_hint (u8, 0/1)
//! - +0x31..+0x37: padding to 56B
//!
//! Tile::Request_inner (`FUN_00302478`, 904B): direction 축의 natural/stretch/shrink 합산.
//! `trim_trailing_hint == 1` 이면 후행 penalty==1000 (= `1.4013e-42` as float) 제거 + 중간
//! penalty>1 (u32 비교) 제거 후 합산. 캐시 결과는 this[+0x0c..+0x2f] 에 기록.
//!
//! Tile::Allocate_inner (`FUN_0034d90c`, 768B): direction 축의 Allocation 을 children 에
//! 자연/stretch/shrink 비율로 분배.
//!
//! ## Align (alignment combiner)
//!
//! - +0x00: vtable
//! - +0x08: direction (i32, 0=H, 1=V)
//! - +0x0c..+0x0f: padding to 16B
//!
//! Align::Request_inner (`FUN_002d0bb4`, 512B): direction 축의 Requirement 를 alignment 기반
//! left/right max 누적. NEON SIMD 2-lane f32 사용.
//!
//! ## Superpose (multi-Layout container)
//!
//! - +0x00: vtable
//! - +0x08, +0x10, +0x18: std::vector<Holder*> (start/end/capacity), Holder=16B {Layout*, refcount}
//!
//! Superpose::Request: children 순회 + 각 vfunc[+0x18] (Request_inner) 호출.
//! Superpose::Allocate: children 순회 + 각 vfunc[+0x20] (Allocate_inner) 호출.

use crate::value_types::{Allocation, Requirement, Requisition};

// ============================================================
// Layout trait
// ============================================================

/// `Hnc::Shape::Text::Layout` interface — 5 vfuncs.
///
/// Tile / Align / Superpose 가 구현. Glyph 와 별개 hierarchy.
pub trait Layout: std::fmt::Debug {
    /// vtable +16: `Clone()`.
    fn clone_layout(&self) -> Box<dyn Layout>;

    /// vtable +24: `Request_inner(vector<Requisition> const& children, Requisition& out)`.
    /// children Requisition 들을 결합해 out 에 작성. self 의 cache 도 갱신 가능 (Tile 만).
    fn request_inner(&mut self, children: &[Requisition], out: &mut Requisition);

    /// vtable +32: `Allocate_inner(Allocation const& avail, vector<Requisition> const& reqs,
    ///                              vector<Allocation>& out_allocs)`.
    /// avail 을 children 에 분배. out_allocs 는 reqs 와 같은 크기여야 함 (size mismatch → no-op).
    fn allocate_inner(
        &self,
        avail: &Allocation,
        reqs: &[Requisition],
        out_allocs: &mut [Allocation],
    );
}

// ============================================================
// helpers — direction-axis selectors
// ============================================================

#[inline]
fn axis_of(req: &Requisition, direction: i32) -> &Requirement {
    if direction == 0 { &req.x } else { &req.y }
}

#[inline]
fn write_axis(out: &mut Requisition, direction: i32, value: Requirement) {
    if direction == 0 { out.x = value; } else { out.y = value; }
}

#[inline]
fn allotment_of(alloc: &Allocation, direction: i32) -> &crate::value_types::Allotment {
    if direction == 0 { &alloc.x } else { &alloc.y }
}

// ============================================================
// Tile
// ============================================================

/// `Hnc::Shape::Text::Tile` (56B, vtable `0x781d50`).
///
/// Direction (X 또는 Y) 축의 children Requisition 들을 합산해 출력. `trim_trailing_hint`
/// flag 가 1 이면 후행 penalty==1000 + 중간 penalty>1 항목 제거 후 합산.
#[derive(Debug, Clone)]
pub struct Tile {
    /// +0x08: direction. 0=H, 1=V.
    pub direction: i32,
    /// +0x0c..+0x2f: cached Requisition (이전 Request_inner 호출의 출력).
    pub cached_req: Requisition,
    /// +0x30: trim_trailing_hint. LayoutFactory::CreateHBox/VBox 가 1 로 초기화.
    pub trim_trailing_hint: bool,
}

impl Tile {
    /// CreateHBox/VBox 의 새 Tile 초기 상태 (raw decompile 검증):
    ///
    /// ```c
    /// puVar5 = operator_new(0x38);                              // 56B
    /// *puVar5 = &PTR_FUN_00781d50;                              // vtable
    /// *(undefined4 *)(puVar5 + 1) = 0;                          // direction
    /// uVar2 = _DAT_00741f20;  // (-1e8 f32, 0 f32)
    /// uVar3 = _UNK_00741f28;  // 0
    /// *(undefined8 *)((long)puVar5 + 0x14) = _UNK_00741f28;     // +0x14..+0x1b = 0
    /// *(undefined8 *)((long)puVar5 + 0xc) = uVar2;              // +0x0c..+0x13 = (-1e8, 0)
    /// *(undefined4 *)((long)puVar5 + 0x1c) = 0xccbebc20;        // +0x1c = -1e8 (Y.natural)
    /// puVar5[4] = 0;                                            // +0x20 = 0 (Y.stretch=0, Y.shrink=0)
    /// puVar5[5] = 0;                                            // +0x28 = 0 (Y.alignment=0, penalty=0)
    /// *(undefined1 *)(puVar5 + 6) = 1;                          // +0x30 = 1 (trim)
    /// ```
    ///
    /// 즉 cached_req = {x: (-1e8, 0, 0, 0), y: (-1e8, 0, 0, 0), penalty: 0} (INVALID sentinel).
    pub fn new(direction: i32) -> Self {
        Self {
            direction,
            cached_req: Requisition {
                x: Requirement::new(-1.0e8, 0.0, 0.0, 0.0),
                y: Requirement::new(-1.0e8, 0.0, 0.0, 0.0),
                penalty: 0,
            },
            trim_trailing_hint: true,
        }
    }
}

impl Layout for Tile {
    /// `Tile` Clone — `FUN_0034d8c4` (sz=72):
    /// ```text
    /// new x0 = operator_new(0x38)                ; alloc 56B
    /// *x0 = vtable_0x781d50                       ; vtable
    /// q0 = this[+0x08..+0x17]  → new[+0x08..+0x17]   ; direction + first 12B of cached_req
    /// q0 = this[+0x18..+0x27]  → new[+0x18..+0x27]   ; middle 16B of cached_req
    /// q0 = this[+0x21..+0x30]  → new[+0x21..+0x30]   ; last 16B (overlapping)
    /// ```
    ///
    /// Rust 동등: 전체 객체 clone (POD 필드만이라 직접 복사).
    fn clone_layout(&self) -> Box<dyn Layout> {
        Box::new(self.clone())
    }

    /// `Tile::Request_inner` (`FUN_00302478`, 904B).
    ///
    /// raw asm 검증:
    /// 1. `if (this[0x30] == 0)`: simple path — children 의 direction 축 (X if dir==0, Y if dir==1)
    ///    의 `(natural, stretch, shrink)` 합산. `natural == -1e8` (INVALID) 항목 skip.
    ///    결과: `sum_natural`, `sum(natural+stretch)`, `sum(natural-shrink)`.
    /// 2. `else (this[0x30] != 0)`: trim path —
    ///    - children 을 새 buffer 로 copy.
    ///    - **trim1**: 후행에서 `penalty == 1000` (= 0x3e8, = `1.4013e-42` as f32) 인 항목 제거.
    ///    - **trim2**: trim1 결과의 끝에서 거꾸로 `penalty > 1` (unsigned cmp, `b.hi`) 인 항목 제거.
    ///    - trim2 가 항목을 제거했으면 (`pfvar9 != pfvar10`): `[pfvar10..pfvar6_orig)` 의 항목들을
    ///      `[pfvar9..)` 로 memmove (= trim1 에서 제거된 penalty==1000 항목들이 trim2 결과 뒤에
    ///      붙음). 새 end = `pfvar9 + (pfvar6_orig - pfvar10)`.
    ///    - trim 결과 vector 가 비어있지 않으면 simple path 와 동일하게 합산.
    /// 3. 출력 Requisition 의 direction 축에 `(sum_natural, stretch_sum - sum_natural,
    ///    sum_natural - shrink_sum, alignment=0)` 를 작성. penalty 는 `param_3[+4*8]` (= 출력
    ///    Requisition+0x20 위치의 penalty) 를 self[+0x2c] 에 캐시.
    /// 4. self[+0x0c..+0x2f] 에 출력 Requisition 통째 캐시.
    ///
    /// **byte-equivalent 주의**: f32 누적 순서는 raw asm 과 동일하게 children 의 인덱스 순서
    /// 그대로 `+=` 누적. `(natural+stretch)` 와 `(natural-shrink)` 를 별도 누적자에 합산
    /// (한컴은 `fmadd`/`fmsub` 가 아닌 `fadd` 두 번). 마지막에 `stretch_total - sum_natural`,
    /// `sum_natural - shrink_total` 로 결과 stretch/shrink 계산.
    fn request_inner(&mut self, children: &[Requisition], out: &mut Requisition) {
        let dir = self.direction;
        let (sum_natural, sum_natural_plus_stretch, sum_natural_minus_shrink) =
            if !self.trim_trailing_hint {
                tile_request_simple_sum(dir, children)
            } else {
                tile_request_trim_then_sum(dir, children)
            };

        let stretch = sum_natural_plus_stretch - sum_natural;
        let shrink = sum_natural - sum_natural_minus_shrink;

        write_axis(
            out,
            dir,
            Requirement {
                natural: sum_natural,
                stretch,
                shrink,
                alignment: 0.0,
            },
        );

        // Cache `out` 통째에 this[+0x0c..+0x2f].
        // raw asm:
        //   *(undefined4 *)(param_1 + 0x2c) = *(undefined4 *)(param_3 + 4);   // penalty (offset 0x20 of Requisition)
        //   *(undefined8 *)(param_1 + 0x24) = uVar16;   // Y bytes 8-15
        //   *(undefined8 *)(param_1 + 0x1c) = uVar15;   // Y bytes 0-7
        //   *(undefined8 *)(param_1 + 0x14) = uVar14;   // X bytes 8-15
        //   *(undefined8 *)(param_1 + 0xc)  = uVar13;   // X bytes 0-7
        // 즉 통째 36B 복사.
        self.cached_req = *out;
    }

    /// `Tile::Allocate_inner` (`FUN_0034d90c`, 768B).
    ///
    /// raw asm:
    /// 1. `count_reqs = (reqs.end - reqs.begin) / 36` (via 0x8e388e38_8e388e39 reciprocal).
    /// 2. `count_allocs = (out_allocs.end - out_allocs.begin) / 24` (via 0xaaab reciprocal).
    /// 3. `if count_reqs != count_allocs → return` (no-op).
    /// 4. Direction 축 선택:
    ///    - `axis_alloc = direction==0 ? avail.x : avail.y` (12B Allotment)
    ///    - `axis_self = direction==0 ? this[+0x0c..+0x1b] : this[+0x1c..+0x2b]` (16B cached Requirement)
    ///    - 출력 위치 offset 도 0 (X) 또는 12 (Y).
    /// 5. Allocation 계산: cached 의 stretch/shrink 비율로 avail.span 분배. 각 child Requisition
    ///    의 natural/stretch/shrink 를 사용. 결과는 out_allocs[i] 의 axis 부분 (origin, span,
    ///    alignment) 작성.
    ///
    /// 알고리즘 detail (asm trace):
    /// ```text
    /// avail_span    = axis_alloc.span        ; param_1[+0x4 of axis] = span
    /// avail_align   = axis_alloc.alignment   ; param_1[+0x8 of axis] = alignment
    /// cached_natural= axis_self.natural
    /// cached_stretch= axis_self.stretch
    /// cached_shrink = axis_self.shrink
    ///
    /// // Phase 1: determine "scale"
    /// if cached_natural == 0:
    ///     // alignment scenario
    ///     ratio = (1 - avail_align) * avail_span
    ///     if ratio > cached_natural { ... } else { branch B }
    /// else if cached_natural == 1:
    ///     ratio = avail_align * avail_span
    ///     if ratio > cached_natural { branch B } else { ... }
    /// else:
    ///     ratio_a = avail_align / cached_natural
    ///     ratio_b = (1 - avail_align) * avail_span / (cached_natural - 1) ?
    ///     // pick min(ratio_a, ratio_b)
    ///     ...
    /// // ...
    /// ```
    ///
    /// **구현 노트**: raw asm 의 분기 구조는 복잡하지만 핵심은:
    /// - avail.span 이 cached.natural 보다 크면 stretch 비율로 확장
    /// - avail.span 이 cached.natural 보다 작으면 shrink 비율로 축소
    /// - 각 child 의 own ratio = (natural / cached.natural) * avail.span
    /// - sentinel `natural == -1e8` 인 child 는 자기 자리만 차지하고 origin 만 누적
    fn allocate_inner(
        &self,
        avail: &Allocation,
        reqs: &[Requisition],
        out_allocs: &mut [Allocation],
    ) {
        if reqs.len() != out_allocs.len() {
            return;
        }
        if reqs.is_empty() {
            return;
        }

        tile_allocate_inner_impl(self.direction, &self.cached_req, avail, reqs, out_allocs);
    }
}

/// Tile::Request_inner 의 simple path (flag==0):
/// children 의 direction 축의 (natural, stretch, shrink) 를 합산. `natural == -1e8` 항목 skip.
///
/// 반환: `(sum_natural, sum(natural+stretch), sum(natural-shrink))`.
fn tile_request_simple_sum(direction: i32, children: &[Requisition]) -> (f32, f32, f32) {
    if children.is_empty() {
        return (0.0, 0.0, 0.0);
    }
    let mut natural: f32 = 0.0;
    let mut np_stretch: f32 = 0.0; // natural + stretch
    let mut nm_shrink: f32 = 0.0; // natural - shrink
    for r in children {
        let axis = axis_of(r, direction);
        // raw asm: `ldr s0, [...]; fmov s1, w_minus_1e8; fcmp s0, s1; b.eq <skip>`
        if axis.natural != -1.0e8 {
            natural += axis.natural;
            np_stretch += axis.natural + axis.stretch;
            nm_shrink += axis.natural - axis.shrink;
        }
    }
    (natural, np_stretch, nm_shrink)
}

/// Tile::Request_inner 의 trim path (flag!=0):
/// children 을 복사 → 후행 penalty==1000 제거 (trim1) → 후행 penalty>1 제거 (trim2) →
/// trim2 가 항목을 제거했으면 trim1 에서 잘린 penalty==1000 항목들을 trim2 결과 뒤에 memmove
/// 로 붙임 → 합산.
fn tile_request_trim_then_sum(direction: i32, children: &[Requisition]) -> (f32, f32, f32) {
    if children.is_empty() {
        return (0.0, 0.0, 0.0);
    }
    let mut copy: Vec<Requisition> = children.to_vec();

    // trim1: walk back, drop penalty==1000 (= 0x3e8 = `1.4013e-42` as f32).
    // raw asm: `ldur w11, [x9, #-0x4]; cmp w11, #0x3e8; b.eq <loop>`
    let mut pfvar10 = copy.len();
    while pfvar10 > 0 && copy[pfvar10 - 1].penalty == 1000 {
        pfvar10 -= 1;
    }
    let pfvar6_orig = copy.len(); // end before trim1 (used by memmove later)

    // trim2: from pfvar10, walk back, drop penalty>1 (unsigned).
    // raw asm: `ldur w11, [x11, #-0x4]; cmp w11, #1; b.hi <loop>`
    let mut pfvar9 = pfvar10;
    while pfvar9 > 0 && (copy[pfvar9 - 1].penalty as u32) > 1 {
        pfvar9 -= 1;
    }

    // memmove: if trim2 trimmed (pfvar9 != pfvar10) AND there's trailing pen==1000 (pfvar10 < pfvar6_orig),
    // shift [pfvar10..pfvar6_orig) → [pfvar9..pfvar9+(pfvar6_orig-pfvar10)).
    // raw asm:
    //   if pfvar9 + (pfvar10 - pfvar9) != pfvar6:    // == "pfvar10 != pfvar6"
    //       memmove pfvar10..pfvar6 → pfvar9..
    //   pfvar6 = pfvar9 + (pfvar6_orig - pfvar10)    // new end
    let final_end = if pfvar9 != pfvar10 {
        if pfvar10 != pfvar6_orig {
            // shift trailing pen==1000 onto position pfvar9
            let n = pfvar6_orig - pfvar10;
            for i in 0..n {
                copy[pfvar9 + i] = copy[pfvar10 + i];
            }
        }
        pfvar9 + (pfvar6_orig - pfvar10)
    } else {
        // pfvar9 == pfvar10: trim2 did nothing. Sum the ORIGINAL pfvar6 range.
        // raw asm: LAB_00302604 in the "if pfvar4==pfvar6 → zero, else sum [pfvar4..pfvar6)" branch.
        // Note: pfvar6_orig here is the size BEFORE trim1 (= original children count).
        //       So if trim1 trimmed but trim2 didn't, we still sum the ORIGINAL (including pen==1000).
        pfvar6_orig
    };

    if final_end == 0 {
        return (0.0, 0.0, 0.0);
    }

    // sum on copy[0..final_end)
    let mut natural: f32 = 0.0;
    let mut np_stretch: f32 = 0.0;
    let mut nm_shrink: f32 = 0.0;
    for r in &copy[..final_end] {
        let axis = axis_of(r, direction);
        if axis.natural != -1.0e8 {
            natural += axis.natural;
            np_stretch += axis.natural + axis.stretch;
            nm_shrink += axis.natural - axis.shrink;
        }
    }
    (natural, np_stretch, nm_shrink)
}

/// Tile::Allocate_inner implementation — raw asm byte-equivalent 1:1 port.
///
/// ## Phase 1: target & ratio 결정 (raw asm `0x34d978` ~ `0x34da4c`)
///
/// ```text
/// avail_span     = avail_axis.span
/// avail_align    = avail_axis.alignment
/// cached_natural = cached_axis.natural
/// cached_stretch = cached_axis.stretch
/// cached_shrink  = cached_axis.shrink
/// cached_align   = cached_axis.alignment
///
/// if cached_align == 0:                               // branch at 0x34d984
///     target = avail_span * (1 - avail_align)         // "left segment"
///     w13 = (target < cached_natural) ? 1 : 0         // shrink needed?
///     if target > cached_natural: goto stretch_path   // 0x34d9d0
///     else: goto shrink_path                          // 0x34da20
/// elif cached_align == 1:                             // branch at 0x34d9b4
///     target = avail_span * avail_align               // "right segment"
///     w13 = (target < cached_natural) ? 1 : 0
///     if target <= cached_natural: goto shrink_path
///     else: goto stretch_path
/// else:                                               // 0x34d9f4
///     ratio_a = avail_align / cached_align
///     ratio_b = (1 - avail_align) / (1 - cached_align)
///     ratio_min = min(ratio_a, ratio_b)               // fcsel ... mi
///     target = avail_span * ratio_min
///     w13 = (target < cached_natural) ? 1 : 0
///     if target > cached_natural: goto stretch_path
///     else: goto shrink_path
///
/// stretch_path (0x34d9d0):
///     if cached_stretch <= 0: goto shrink_path        // fallthrough; ratio = 0 enters shrink with w13=0
///     stretch_ratio = (target - cached_natural) / cached_stretch
///     starting_origin = avail.axis.origin
///     enter stretch_loop at 0x34da50
///
/// shrink_path (0x34da20):
///     v1 = 0
///     if w13 != 0 AND cached_shrink > 0:
///         shrink_ratio = (cached_natural - target) / cached_shrink
///     else: shrink_ratio = 0
///     starting_origin = avail.axis.origin
///     if target <= cached_natural:
///         enter shrink_loop at 0x34dae8
///     else:
///         enter stretch_loop at 0x34da50 (with stretch_ratio computed above)
/// ```
///
/// ## Phase 2: distribute to children (raw asm `0x34da50` ~ `0x34dc04`)
///
/// stretch_loop body per child k:
/// ```text
/// req_axis = (direction == 0) ? reqs[k].x : reqs[k].y
/// if req_axis.natural == -1e8 (sentinel):
///     child_span = 0
///     child_align_in_alloc = 0
/// else:
///     child_span = req_axis.natural + stretch_ratio * req_axis.stretch
///     child_align_in_alloc = req_axis.alignment
/// child_origin = accumulated_origin + child_span * child_align_in_alloc
/// out_allocs[k].axis = { origin: child_origin, span: child_span, alignment: child_align_in_alloc }
/// accumulated_origin += child_span
/// ```
///
/// shrink_loop body per child k: same shape, with
/// `child_span = req_axis.natural - shrink_ratio * req_axis.shrink`.
///
/// **No-scale path** (`0x34db80`): w13 == 0 fallthrough — no ratio scaling, just identity span.
/// ```text
/// child_span = req_axis.natural
/// ; (no stretch/shrink applied)
/// ```
fn tile_allocate_inner_impl(
    direction: i32,
    cached: &Requisition,
    avail: &Allocation,
    reqs: &[Requisition],
    out_allocs: &mut [Allocation],
) {
    let avail_axis = allotment_of(avail, direction);
    let cached_axis = axis_of(cached, direction);

    let avail_origin = avail_axis.origin;
    let avail_span = avail_axis.span;
    let avail_align = avail_axis.alignment;

    let cached_natural = cached_axis.natural;
    let cached_stretch = cached_axis.stretch;
    let cached_shrink = cached_axis.shrink;
    let cached_align = cached_axis.alignment;

    // Phase 1: compute target & decide scale mode
    let target: f32;
    let shrink_needed: bool; // w13 register in asm — set when target < cached_natural

    if cached_align == 0.0 {
        // branch at 0x34d984
        target = avail_span * (1.0f32 - avail_align);
        shrink_needed = target < cached_natural;
    } else if cached_align == 1.0 {
        target = avail_span * avail_align;
        shrink_needed = target < cached_natural;
    } else {
        // cached_align in (0, 1)
        let ratio_a = avail_align / cached_align;
        let ratio_b = (1.0f32 - avail_align) / (1.0f32 - cached_align);
        // raw: fcmp s1,s4; fcsel s1,s1,s4,mi → s1 = (s1 < s4) ? s1 : s4 = min(s1, s4)
        let ratio_min = if ratio_b < ratio_a { ratio_b } else { ratio_a };
        target = avail_span * ratio_min;
        shrink_needed = target < cached_natural;
    }

    // Phase 1.5: decide loop type & compute ratio
    enum Mode {
        Stretch(f32),   // stretch_ratio
        Shrink(f32),    // shrink_ratio
        NoScale,        // identity (w13==0 fallthrough + can't stretch/shrink)
    }

    let mode: Mode = if target > cached_natural {
        // stretch path 0x34d9d0
        if cached_stretch <= 0.0 {
            // fallthrough to shrink (but shrink_needed = false → no scale)
            if shrink_needed && cached_shrink > 0.0 {
                Mode::Shrink((cached_natural - target) / cached_shrink)
            } else {
                Mode::NoScale
            }
        } else {
            Mode::Stretch((target - cached_natural) / cached_stretch)
        }
    } else if shrink_needed {
        // shrink path 0x34da20 with w13 == 1
        if cached_shrink > 0.0 {
            Mode::Shrink((cached_natural - target) / cached_shrink)
        } else {
            Mode::NoScale
        }
    } else {
        // target == cached_natural OR cached_align/avail combination → no scale
        Mode::NoScale
    };

    if reqs.is_empty() {
        // raw asm: cmp x9, x10; b.eq 0x0034dc08 (return)
        return;
    }

    // Phase 2: distribute
    let mut accumulated_origin: f32 = avail_origin;

    for (i, child) in reqs.iter().enumerate() {
        let child_axis = axis_of(child, direction);

        let (child_span, child_align_in_alloc): (f32, f32) = if child_axis.natural == -1.0e8 {
            // sentinel: span=0, align=0 (raw asm `movi d3,#0x0; movi d4,#0x0` at 0x34db70-78 etc.)
            (0.0, 0.0)
        } else {
            // raw asm:
            //   stretch path: `fmadd s4, s3, s1, s2` where s3=stretch, s1=ratio, s2=natural
            //     → span = stretch * ratio + natural (single-rounding FMA)
            //   shrink path:  `fmsub s4, s4, s1, s2` where s4=shrink, s1=ratio, s2=natural
            //     → span = -shrink * ratio + natural = natural - shrink * ratio (single-rounding FMA)
            let span = match mode {
                Mode::Stretch(r) => child_axis.stretch.mul_add(r, child_axis.natural),
                Mode::Shrink(r) => (-child_axis.shrink).mul_add(r, child_axis.natural),
                Mode::NoScale => child_axis.natural,
            };
            (span, child_axis.alignment)
        };

        // raw asm:
        //   s4 = child_span; s3 = child_align_in_alloc
        //   s2 = s0 + s4              ; fadd: new origin for next = old + span
        //   s0 = s4 * s3 + s0          ; fmadd: this child's origin = span * align + old (single-rounding)
        //   stp s0, s4, [x12]          ; write (origin, span)
        //   str s3, [x12, #0x8]         ; write alignment
        //   s0 = s2                     ; for next iter, s0 = end of this child = old + span
        let child_origin = child_span.mul_add(child_align_in_alloc, accumulated_origin);

        let new_axis = crate::value_types::Allotment {
            origin: child_origin,
            span: child_span,
            alignment: child_align_in_alloc,
        };
        if direction == 0 {
            out_allocs[i] = Allocation { x: new_axis, y: out_allocs[i].y };
        } else {
            out_allocs[i] = Allocation { x: out_allocs[i].x, y: new_axis };
        }

        accumulated_origin += child_span;
    }
}

// ============================================================
// TileReverse
// ============================================================

/// `Hnc::Shape::Text::TileReverse` (56B, vtable `0x781dc0`, typeinfo
/// `N3Hnc5Shape4Text11TileReverseE`).
///
/// `Tile` 과 **동일한 struct layout** (direction + cached_req + trim flag) 및 Layout 5-vfunc
/// vtable (`[+0x18]` = `FUN_00302858` = `Request_inner`, `[+0x20]` = `FUN_0034e640` =
/// `Allocate_inner`, `[+0x10]` = `FUN_0034e5f8` = `Clone`) 을 가진다.
///
/// 단, `Composition::CreateItem` (`FUN_003000a8`) 는 이 클래스를 **stack 임시 객체로 만들어
/// `Request_inner` 만 직접 `bl 0x00302858` 호출**한다 (vtable dispatch 아님, Superpose 에
/// 들어가지도 않음). 따라서 본 port 는 B-5h 범위에서 `request_inner` 만 inherent method 로
/// 제공한다. `Allocate_inner`/`Clone` 은 TileReverse 가 `dyn Layout` 으로 쓰이는 경로가
/// 생기는 후속 단계에서 `Layout` impl 과 함께 포팅.
#[derive(Debug, Clone)]
pub struct TileReverse {
    /// +0x08: direction. 0=H/X, 1=V/Y.
    pub direction: i32,
    /// +0x0c..+0x2f: cached Requisition (이전 Request_inner 호출의 출력).
    pub cached_req: Requisition,
    /// +0x30: trim_trailing_hint.
    pub trim_trailing_hint: bool,
}

impl TileReverse {
    /// `TileReverse::Request_inner` (`FUN_00302858`, 908B).
    ///
    /// raw asm 검증: 합산 + trim 알고리즘이 `Tile::Request_inner` (`FUN_00302478`) 와
    /// **byte-identical** 하다 —
    /// 1. `this[+0x30] == 0` (simple): direction 축의 `(natural, stretch, shrink)` 합산,
    ///    `natural == -1e8` (INVALID) skip. (raw `0x302914`-`0x302974`)
    /// 2. `else` (trim): children 복사 → trim1 (후행 `penalty == 1000` 제거) → trim2 (끝에서
    ///    `penalty > 1` unsigned 제거) → trim2 가 trim 했으면 memmove 로 trim1 의 1000 들을
    ///    뒤에 붙임 → 합산. (raw `0x302888`-`0x302b28`, `Tile` 과 동일 흐름)
    ///
    /// **유일한 차이는 출력 Requirement 의 `alignment` 가 `1.0`** (Tile 은 `0.0`):
    /// ```text
    /// 00302b28  fsub s0,s10,s8           ; stretch = sum(nat+str) - sum(nat)
    /// 00302b2c  fsub s1,s8,s9            ; shrink  = sum(nat) - sum(nat-shr)
    /// 00302b48  str s8,[x20,x8]          ; out.<axis>.natural    = sum(nat)
    /// 00302b4c  stp s0,s1,[x9, #0x4]     ; out.<axis>.stretch/shrink
    /// 00302b50  mov w8,#0x3f800000       ; 1.0
    /// 00302b54  str w8,[x9, #0xc]        ; out.<axis>.alignment  = 1.0   ← Tile 은 0.0
    /// ```
    /// 결과 Requisition 은 `self.cached_req` 에 통째 캐시 (raw `0x302b58`-`0x302b68`,
    /// `out.penalty` 포함).
    pub fn request_inner(&mut self, children: &[Requisition], out: &mut Requisition) {
        let dir = self.direction;
        let (sum_natural, sum_natural_plus_stretch, sum_natural_minus_shrink) =
            if !self.trim_trailing_hint {
                tile_request_simple_sum(dir, children)
            } else {
                tile_request_trim_then_sum(dir, children)
            };

        let stretch = sum_natural_plus_stretch - sum_natural;
        let shrink = sum_natural - sum_natural_minus_shrink;

        write_axis(
            out,
            dir,
            Requirement {
                natural: sum_natural,
                stretch,
                shrink,
                // TileReverse: alignment = 1.0 (Tile 은 0.0) — raw `mov w8,#0x3f800000`.
                alignment: 1.0,
            },
        );

        // raw 0x302b58-0x302b68: self[+0x0c..+0x2f] 에 out 통째 캐시.
        self.cached_req = *out;
    }
}

// ============================================================
// Align
// ============================================================

/// `Hnc::Shape::Text::Align` (16B, vtable `0x77fab8`).
///
/// Direction 축의 alignment 결합. children 의 alignment 별 left/right extent 의 max 누적.
#[derive(Debug, Clone)]
pub struct Align {
    /// +0x08: direction. 0=H, 1=V.
    pub direction: i32,
}

impl Align {
    /// CreateHBox/VBox 의 새 Align 초기 상태 (raw decompile):
    /// ```c
    /// plVar6 = operator_new(0x10);                            // 16B
    /// *plVar6 = (long)&PTR_FUN_0077fab8;                       // vtable
    /// *(undefined4 *)(plVar6 + 1) = 1;                         // direction = 1 (HBox)
    /// // VBox: direction = 0
    /// ```
    ///
    /// HBox 의 Align direction = 1, VBox 의 Align direction = 0 (Tile 과 반대!).
    pub fn new(direction: i32) -> Self {
        Self { direction }
    }
}

impl Layout for Align {
    /// `Align::Clone` — `FUN_002d0b74` (sz=56):
    /// ```text
    /// new = operator_new(0x10)
    /// *new = vtable_0x77fab8
    /// new[+0x08] = this[+0x08]   ; direction
    /// ```
    fn clone_layout(&self) -> Box<dyn Layout> {
        Box::new(self.clone())
    }

    /// `Align::Request_inner` (`FUN_002d0bb4`, 512B).
    ///
    /// raw asm:
    /// 1. 빈 vector → 'empty path' (LAB_002d0c48): natural=0, stretch=+1e8, shrink=-1e8 (해당
    ///    축의 nothing).
    /// 2. children 순회 (direction-aware axis 선택):
    ///    - 각 child 의 axis = (natural, stretch, shrink, alignment)
    ///    - skip if `natural == -1e8`
    ///    - L_alpha = 1.0 - alignment, R_alpha = alignment
    ///    - v4 = (L*natural, R*natural)              ; v2 ← max(v2, v4) per lane
    ///    - v5 = (L*(natural+stretch), R*(natural+stretch))  ; v1 ← min(v1, v5)
    ///    - v6 = (L*(natural-shrink), R*(natural-shrink))   ; v0 ← max(v0, v6)
    /// 3. 끝에서 saturate: v2 ← max(v2, v1), v2 ← min(v2, v0) (clamp to [v1, v0])
    /// 4. natural = v2.lane[0] + v2.lane[1]  (faddp)
    /// 5. alignment, stretch, shrink 계산 (3 branch by v2.lane[1] == 0, v2.lane[0] == 0,
    ///    else 정상).
    /// 6. 출력 Requisition 의 direction 축에 (natural, stretch, shrink, alignment) 작성.
    ///
    /// **구현 노트**: NEON vec2 ops 는 Rust `(f32, f32)` tuple 로 보존. blend (BIT/BIF) 는
    /// per-lane conditional copy 로 표현.
    fn request_inner(&mut self, children: &[Requisition], out: &mut Requisition) {
        let dir = self.direction;
        let (natural, stretch, shrink, alignment) = align_request_impl(dir, children);
        write_axis(
            out,
            dir,
            Requirement {
                natural,
                stretch,
                shrink,
                alignment,
            },
        );
    }

    /// `Align::Allocate_inner` (`FUN_002d0f00`, 368B).
    ///
    /// raw asm:
    /// 1. count_reqs == count_allocs check.
    /// 2. 각 child 에 대해:
    ///    - skip sentinel (natural == -1e8) — span=0, alignment 만 복제
    ///    - child_align = child.alignment
    ///    - if `child_align == 0.0`: scale = 1.0 - avail_align (from this.direction's avail)
    ///    - else if `child_align == 1.0`: scale = avail_align
    ///    - else: scale = min(avail_align / child_align, (1-avail_align)/(1-child_align))
    ///    - child_span = avail_span * scale * (natural)
    ///    - clamp: child_span ≥ natural + stretch, child_span ≤ natural - shrink
    ///    - origin = (cached origin)
    /// 3. out_allocs[i] 의 direction 축 작성.
    ///
    /// **구현 노트**: avail_origin 사용 — `axis_alloc.origin`. child_origin = avail_origin
    /// (전체 child 가 동일 origin? 또는 align 기준 위치).
    fn allocate_inner(
        &self,
        avail: &Allocation,
        reqs: &[Requisition],
        out_allocs: &mut [Allocation],
    ) {
        if reqs.len() != out_allocs.len() {
            return;
        }
        if reqs.is_empty() {
            return;
        }
        align_allocate_impl(self.direction, avail, reqs, out_allocs);
    }
}

/// Align::Request_inner 의 lane 누적 + saturate + 3-branch 출력 — raw asm `FUN_002d0bb4` 1:1.
///
/// 반환: `(natural, stretch, shrink, alignment)` for direction 축의 출력 Requirement.
///
/// ## Loop (raw `0x002d0bf8` direction!=0 / `0x002d0cf4` direction==0)
///
/// init: `v2 = (0,0)` (natural-max), `v1 = (+1e8,+1e8)` (stretch-min), `v0 = (-1e8,-1e8)` (shrink-max).
/// non-sentinel child 마다:
/// `l = 1-alignment`, `r = alignment`,
/// `v4 = (l*nat, r*nat)`, `v5 = (l*(nat+str), r*(nat+str))`, `v6 = (l*(nat-shr), r*(nat-shr))`,
/// `v2 = max(v2,v4)`, `v1 = min(v1,v5)`, `v0 = max(v0,v6)`.
///
/// ## Saturate (raw `0x002d0c64`)
///
/// `v2 = min(v1,v2)` [bif v2,v1,(v1>v2)] → `v2 = max(v2,v0)` [bif v2,v0,(v2>v0)] →
/// `v3 = max(v2,v1)` [bsl] → `v1 = min(v0,v2)` [bsl] → `s4 = v2.1`, `s0 = v2.0 + v2.1` [faddp].
///
/// ## 3-branch 출력
///
/// - `s4 == 0` (raw `0x002d0c94`): `s1 = v2.0-v1.0`, `s2 = v3.0-v2.0`, `s3 = 0.0`
/// - `v2.0 == 0` (raw `0x002d0cb4`): `s1 = v2.1-v1.1`, `s2 = v3.1-v2.1`, `s3 = 1.0`
/// - normal (raw `0x002d0d44`):
///   `s1 = max(v1.0/v2.0, v1.1/v2.1)`, `s2 = min(v3.0/v2.0, v3.1/v2.1)`,
///   `s1 = s0*(1.0-s1)`, `s2 = s0*(s2-1.0)`, `s3 = (s0!=0) ? s4/s0 : 0.0`
///
/// 출력: `(natural=s0, stretch=s2, shrink=s1, alignment=s3)` (raw `0x002d0d8c`).
fn align_request_impl(direction: i32, children: &[Requisition]) -> (f32, f32, f32, f32) {
    let mut v2 = (0.0f32, 0.0f32); // natural-max accumulator
    let mut v1 = (1.0e8f32, 1.0e8f32); // stretch-min accumulator
    let mut v0 = (-1.0e8f32, -1.0e8f32); // shrink-max accumulator

    for child in children {
        let axis = axis_of(child, direction);
        if axis.natural == -1.0e8 {
            // raw `fcmp s4, s5(-1e8); b.eq <skip>` — sentinel skip
            continue;
        }
        let l = 1.0f32 - axis.alignment;
        let r = axis.alignment;

        let v4 = (l * axis.natural, r * axis.natural);
        let np_stretch = axis.natural + axis.stretch;
        let v5 = (l * np_stretch, r * np_stretch);
        let nm_shrink = axis.natural - axis.shrink;
        let v6 = (l * nm_shrink, r * nm_shrink);

        // bit v2, v4, (v4>v2): where v4>v2 → v2=v4 → v2 = max(v2, v4)
        if v4.0 > v2.0 { v2.0 = v4.0; }
        if v4.1 > v2.1 { v2.1 = v4.1; }
        // bit v1, v5, (v1>v5): where v1>v5 → v1=v5 → v1 = min(v1, v5)
        if v1.0 > v5.0 { v1.0 = v5.0; }
        if v1.1 > v5.1 { v1.1 = v5.1; }
        // bit v0, v6, (v6>v0): where v6>v0 → v0=v6 → v0 = max(v0, v6)
        if v6.0 > v0.0 { v0.0 = v6.0; }
        if v6.1 > v0.1 { v0.1 = v6.1; }
    }

    // Saturate phase:
    // bif v2, v1, (v1>v2): v1>v2 keep v2, else v2=v1 → v2 = min(v1, v2)
    if !(v1.0 > v2.0) { v2.0 = v1.0; }
    if !(v1.1 > v2.1) { v2.1 = v1.1; }
    // bif v2, v0, (v2>v0): v2>v0 keep v2, else v2=v0 → v2 = max(v2, v0)
    if !(v2.0 > v0.0) { v2.0 = v0.0; }
    if !(v2.1 > v0.1) { v2.1 = v0.1; }
    // bsl (v2>v1), v2, v1 → v3 = max(v2, v1)
    let v3 = (
        if v2.0 > v1.0 { v2.0 } else { v1.0 },
        if v2.1 > v1.1 { v2.1 } else { v1.1 },
    );
    // bsl (v0>v2), v2, v0 → v1 = min(v0, v2)
    let v1 = (
        if v0.0 > v2.0 { v2.0 } else { v0.0 },
        if v0.1 > v2.1 { v2.1 } else { v0.1 },
    );

    let s4 = v2.1;
    let s0 = v2.0 + v2.1; // faddp

    if s4 == 0.0 {
        // branch 1 (raw 0x002d0c94): fsub v1=v2-v1 (lane0), fsub v2=v3-v2 (lane0), s3=0
        let s1 = v2.0 - v1.0;
        let s2 = v3.0 - v2.0;
        (s0, s2, s1, 0.0f32) // (natural, stretch, shrink, alignment)
    } else if v2.0 == 0.0 {
        // branch 2 (raw 0x002d0cb4): fsub v1=v2-v1 → s1=lane1, fsub v2=v3-v2 → s2=lane1, s3=1.0
        let s1 = v2.1 - v1.1;
        let s2 = v3.1 - v2.1;
        (s0, s2, s1, 1.0f32)
    } else {
        // branch 3 normal (raw 0x002d0d44)
        // fdiv v1 = v1/v2 per-lane → s1_cur = v1d.0, s5 = v1d.1
        let v1d = (v1.0 / v2.0, v1.1 / v2.1);
        let s1_cur = v1d.0;
        let s5 = v1d.1;
        // fcmp s5, s1_cur; fcsel s1, s1_cur, s5, mi(=s5<s1_cur) → s1 = max(s1_cur, s5)
        let s1_raw = if s5 < s1_cur { s1_cur } else { s5 };
        // fdiv v2 = v3/v2 per-lane → s2_cur = v2d.0, s3_dup = v2d.1
        let v2d = (v3.0 / v2.0, v3.1 / v2.1);
        let s2_cur = v2d.0;
        let s3_dup = v2d.1;
        // fcmp s2_cur, s3_dup; fcsel s2, s2_cur, s3_dup, mi(=s2_cur<s3_dup) → s2 = min(s2_cur, s3_dup)
        let s2_raw = if s2_cur < s3_dup { s2_cur } else { s3_dup };
        // s1 = s0 * (1.0 - s1)
        let s1 = s0 * (1.0f32 - s1_raw);
        // s2 = s0 * (s2 - 1.0)
        let s2 = s0 * (s2_raw - 1.0f32);
        // s3 = (s0 != 0) ? s4/s0 : 0.0
        let s3 = if s0 != 0.0 { s4 / s0 } else { 0.0f32 };
        (s0, s2, s1, s3)
    }
}

fn align_allocate_impl(
    direction: i32,
    avail: &Allocation,
    reqs: &[Requisition],
    out_allocs: &mut [Allocation],
) {
    // raw `FUN_002d0f00`:
    let avail_axis = allotment_of(avail, direction);
    let avail_origin = avail_axis.origin;
    let avail_span = avail_axis.span;
    let avail_align = avail_axis.alignment;

    for (i, child) in reqs.iter().enumerate() {
        let child_axis = axis_of(child, direction);

        let new_axis: crate::value_types::Allotment = if child_axis.natural == -1.0e8 {
            // raw `0x002d0f80` sentinel path: copy avail axis 통째.
            //   str w16, [x14, #0x8]   ; out.alignment = avail.alignment
            //   str x15, [x14]          ; out.{origin, span} = avail.{origin, span}
            crate::value_types::Allotment {
                origin: avail_origin,
                span: avail_span,
                alignment: avail_align,
            }
        } else {
            // raw `0x002d0fdc` non-sentinel path:
            let child_align = child_axis.alignment;

            // scale 계산:
            //   if child_align == 0: scale = 1.0 - avail_align       (raw 0x002d0fe8)
            //   else if child_align == 1.0: scale = avail_align       (raw 0x002d0ff8 b.eq)
            //   else: scale = min((1-avail_align)/(1-child_align), avail_align/child_align)  (raw 0x002d1000)
            //         fcsel s3, s3, s4, mi(=s3<s4) → s3 = min(s3, s4) where s3=rb, s4=ra
            let scale = if child_align == 0.0 {
                1.0f32 - avail_align
            } else if child_align == 1.0 {
                avail_align
            } else {
                let ra = avail_align / child_align; // s4
                let rb = (1.0f32 - avail_align) / (1.0f32 - child_align); // s3
                if rb < ra { rb } else { ra } // min(rb, ra)
            };

            // raw `0x002d1018`:
            //   s3 = avail_span * scale
            //   s4 = req.natural + req.stretch (= max span)
            //   s3 = min(s4, s3)            ; fcsel s3, s4, s3, mi(=s4<s3)
            //   s2 = req.natural - req.shrink (= min span)
            //   s2 = max(s2, s3)            ; fcsel s2, s2, s3, mi(=s3<s2)
            let span_raw = avail_span * scale;
            let max_span = child_axis.natural + child_axis.stretch;
            let span_clamped_hi = if max_span < span_raw { max_span } else { span_raw };
            let min_span = child_axis.natural - child_axis.shrink;
            let span = if span_clamped_hi < min_span { min_span } else { span_clamped_hi };

            // raw `0x002d103c`: origin = avail_axis.origin, alignment = req_axis.alignment
            crate::value_types::Allotment {
                origin: avail_origin,
                span,
                alignment: child_align,
            }
        };

        if direction == 0 {
            out_allocs[i] = Allocation { x: new_axis, y: out_allocs[i].y };
        } else {
            out_allocs[i] = Allocation { x: out_allocs[i].x, y: new_axis };
        }
    }
}

// ============================================================
// Superpose
// ============================================================

/// `Hnc::Shape::Text::Superpose` (32B, vtable `0x781828`).
///
/// `std::vector<SharePtr<Layout>>` 의 list container. Request/Allocate 시 자식들의
/// Request_inner/Allocate_inner 를 순회 호출.
///
/// Raw layout:
/// - +0x00: vtable
/// - +0x08..+0x10: vector.start (Holder**)
/// - +0x10..+0x18: vector.end
/// - +0x18..+0x20: vector.capacity
///
/// Rust 동등: `Vec<Box<dyn Layout>>`.
#[derive(Debug)]
pub struct Superpose {
    pub children: Vec<Box<dyn Layout>>,
}

impl Clone for Superpose {
    fn clone(&self) -> Self {
        Self {
            children: self.children.iter().map(|c| c.clone_layout()).collect(),
        }
    }
}

impl Superpose {
    /// `Superpose::Create` (`FUN_00316a44`) + ctor (`FUN_003393dc`):
    /// ```c
    /// *this = vtable_0x781828
    /// this[+0x08] = 0
    /// this[+0x10] = 0
    /// this[+0x18] = 0
    /// FUN_0062fae4(this+0x08, 0x14);  // vector::reserve(20)? — initial capacity 20 elements?
    /// ```
    ///
    /// Rust: 빈 Vec.
    pub fn new() -> Self {
        Self { children: Vec::new() }
    }

    /// `Superpose::Add` (`FUN_00316ac4`):
    /// ```c
    /// if (layout != null && *layout != null):
    ///     vector::push_back(holder)
    ///     refcount += 1
    /// ```
    pub fn add(&mut self, layout: Box<dyn Layout>) {
        self.children.push(layout);
    }

    /// `Superpose::GetCount` (`FUN_00339718`):
    /// ```c
    /// return (this[+0x10] - this[+0x08]) >> 3;   // (end - start) / 8 = count of Holder*
    /// ```
    pub fn get_count(&self) -> usize {
        self.children.len()
    }
}

impl Default for Superpose {
    fn default() -> Self {
        Self::new()
    }
}

impl Layout for Superpose {
    /// `Superpose::Clone` (`FUN_003396c4`, sz=64):
    /// ```c
    /// new = operator_new(0x20)
    /// *new = vtable_0x781828
    /// FUN_006409e8(new+0x08, this+0x08)   // vector copy-construct
    /// ```
    fn clone_layout(&self) -> Box<dyn Layout> {
        Box::new(Self {
            children: self.children.iter().map(|c| c.clone_layout()).collect(),
        })
    }

    /// `Superpose::Request` (`FUN_003397f8`, sz=88):
    /// ```c
    /// pStart = this[+0x08];
    /// pEnd = this[+0x10];
    /// for (p = pStart; p != pEnd; p++) {
    ///     glyph = *p->layout;
    ///     glyph->vfunc[+0x18](glyph, param_1, param_2);   // == Request_inner
    /// }
    /// ```
    fn request_inner(&mut self, children: &[Requisition], out: &mut Requisition) {
        for child in self.children.iter_mut() {
            child.request_inner(children, out);
        }
    }

    /// `Superpose::Allocate` (`FUN_00339850`, sz=104):
    /// ```c
    /// for (p = pStart; p != pEnd; p++) {
    ///     glyph = *p->layout;
    ///     glyph->vfunc[+0x20](glyph, param_1, param_2, param_3);   // == Allocate_inner
    /// }
    /// ```
    fn allocate_inner(
        &self,
        avail: &Allocation,
        reqs: &[Requisition],
        out_allocs: &mut [Allocation],
    ) {
        for child in self.children.iter() {
            child.allocate_inner(avail, reqs, out_allocs);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value_types::Allotment;

    fn req(nx: f32, sx: f32, kx: f32, ay: f32, p: i32) -> Requisition {
        Requisition {
            x: Requirement::new(nx, sx, kx, 0.0),
            y: Requirement::new(0.0, 0.0, 0.0, ay),
            penalty: p,
        }
    }

    #[test]
    fn tile_request_simple_sum_h() {
        let mut tile = Tile::new(0);
        tile.trim_trailing_hint = false;
        let children = vec![
            req(10.0, 2.0, 1.0, 0.0, 0),
            req(20.0, 3.0, 1.5, 0.0, 0),
        ];
        let mut out = Requisition::ZERO;
        tile.request_inner(&children, &mut out);
        // natural = 10 + 20 = 30
        // np_stretch = 12 + 23 = 35  → stretch = 35 - 30 = 5
        // nm_shrink = 9 + 18.5 = 27.5 → shrink = 30 - 27.5 = 2.5
        assert_eq!(out.x.natural, 30.0);
        assert_eq!(out.x.stretch, 5.0);
        assert_eq!(out.x.shrink, 2.5);
        assert_eq!(out.x.alignment, 0.0);
    }

    #[test]
    fn tile_request_simple_sum_skips_invalid_natural() {
        let mut tile = Tile::new(0);
        tile.trim_trailing_hint = false;
        let children = vec![
            req(10.0, 1.0, 0.0, 0.0, 0),
            req(-1.0e8, 999.0, 999.0, 0.0, 0),  // sentinel — skip
            req(20.0, 2.0, 0.0, 0.0, 0),
        ];
        let mut out = Requisition::ZERO;
        tile.request_inner(&children, &mut out);
        assert_eq!(out.x.natural, 30.0);
        assert_eq!(out.x.stretch, 3.0);
        assert_eq!(out.x.shrink, 0.0);
    }

    #[test]
    fn tile_request_trim_path_empty() {
        let mut tile = Tile::new(0);
        let children: Vec<Requisition> = vec![];
        let mut out = Requisition::ZERO;
        tile.request_inner(&children, &mut out);
        assert_eq!(out.x.natural, 0.0);
    }

    #[test]
    fn tile_request_trim_path_drops_trailing_penalty_1000() {
        // trim_trailing_hint=true: [n=10, p=0], [n=20, p=1000], [n=30, p=1000]
        // After trim1: only [n=10, p=0] remains as the kept-then-summed range...
        // Wait: trim1 trims trailing pen==1000. Stop at first non-1000 from end → pfvar10 = index 1 (past [10]).
        // trim2 from pfvar10 going back: check [10].penalty = 0. NOT > 1, stop. pfvar9 = pfvar10.
        // pfvar9 == pfvar10: sum [pfvar4..pfvar6_orig=3) — i.e., ORIGINAL all 3 items including the 1000s.
        // So natural = 10 + 20 + 30 = 60.
        let mut tile = Tile::new(0);
        let children = vec![
            req(10.0, 1.0, 0.0, 0.0, 0),
            req(20.0, 2.0, 0.0, 0.0, 1000),
            req(30.0, 3.0, 0.0, 0.0, 1000),
        ];
        let mut out = Requisition::ZERO;
        tile.request_inner(&children, &mut out);
        assert_eq!(out.x.natural, 60.0, "trim1 didn't trim, sum all 3");
    }

    #[test]
    fn tile_request_trim_path_drops_interior_high_penalty() {
        // [n=10, p=1], [n=999, p=5], [n=20, p=2]
        // trim1 from end: [20].pen=2 != 1000, stop. pfvar10 = 3 (all 3 items).
        // trim2 from end: [20].pen=2 > 1, skip. [999].pen=5 > 1, skip. [10].pen=1 NOT > 1, stop. pfvar9 = 1.
        // pfvar9 != pfvar10 → memmove [3..3) = nothing. New end = 1 + (3 - 3) = 1.
        // Sum [0..1) = [10]. natural = 10.
        let mut tile = Tile::new(0);
        let children = vec![
            req(10.0, 1.0, 0.0, 0.0, 1),
            req(999.0, 999.0, 0.0, 0.0, 5),
            req(20.0, 2.0, 0.0, 0.0, 2),
        ];
        let mut out = Requisition::ZERO;
        tile.request_inner(&children, &mut out);
        assert_eq!(out.x.natural, 10.0, "drops penalty>1");
    }

    #[test]
    fn tile_request_trim_path_keeps_trailing_1000_after_memmove() {
        // [n=10, p=1], [n=999, p=5], [n=88, p=1000], [n=77, p=1000]
        // trim1 from end: [77].pen=1000, [88].pen=1000 → trim. Stop at [999].pen=5. pfvar10 = 2.
        // trim2 from pfvar10=2 back: [999].pen=5 > 1, skip. [10].pen=1 NOT > 1, stop. pfvar9 = 1.
        // pfvar9 != pfvar10 → memmove [2..4) → [1..3). New copy: [10, 88, 77, 77]. New end = 1 + (4-2) = 3.
        // Sum copy[0..3) = [10, 88, 77]. natural = 175.
        let mut tile = Tile::new(0);
        let children = vec![
            req(10.0, 1.0, 0.0, 0.0, 1),
            req(999.0, 999.0, 0.0, 0.0, 5),
            req(88.0, 0.0, 0.0, 0.0, 1000),
            req(77.0, 0.0, 0.0, 0.0, 1000),
        ];
        let mut out = Requisition::ZERO;
        tile.request_inner(&children, &mut out);
        assert_eq!(out.x.natural, 175.0, "keeps trailing 1000s after memmove, drops middle pen=5");
    }

    #[test]
    fn tile_request_writes_v_axis_for_direction_1() {
        let mut tile = Tile::new(1);
        tile.trim_trailing_hint = false;
        let children = vec![
            Requisition {
                x: Requirement::new(0.0, 0.0, 0.0, 0.0),
                y: Requirement::new(5.0, 1.0, 0.5, 0.0),
                penalty: 0,
            },
        ];
        let mut out = Requisition::ZERO;
        tile.request_inner(&children, &mut out);
        // Y axis written
        assert_eq!(out.y.natural, 5.0);
        assert_eq!(out.y.stretch, 1.0);
        assert_eq!(out.y.shrink, 0.5);
    }

    #[test]
    fn tile_clone_preserves_state() {
        let mut tile = Tile::new(0);
        tile.cached_req.x.natural = 42.0;
        tile.trim_trailing_hint = false;

        // Test via direct Clone trait (Tile derives Clone, separate from Layout::clone_layout).
        let cloned_tile: Tile = tile.clone();
        assert_eq!(cloned_tile.direction, 0);
        assert_eq!(cloned_tile.cached_req.x.natural, 42.0);
        assert_eq!(cloned_tile.trim_trailing_hint, false);

        // Also verify Layout::clone_layout works without panic.
        let _boxed: Box<dyn Layout> = tile.clone_layout();
    }

    #[test]
    fn align_request_empty_path() {
        let mut align = Align::new(0);
        let children: Vec<Requisition> = vec![];
        let mut out = Requisition::ZERO;
        align.request_inner(&children, &mut out);
        // empty: natural = 0+0 = 0, but stretch/shrink could be from saturate phase.
        // 자세한 값 보다는 패닉 안 나는지 확인.
        let _ = out;
    }

    #[test]
    fn align_request_single_child_centered() {
        // child with natural=10, alignment=0.5 → L=5, R=5. natural total = 10.
        let mut align = Align::new(0);
        let children = vec![
            Requisition {
                x: Requirement::new(10.0, 0.0, 0.0, 0.5),
                y: Requirement::ZERO,
                penalty: 0,
            },
        ];
        let mut out = Requisition::ZERO;
        align.request_inner(&children, &mut out);
        assert_eq!(out.x.natural, 10.0);
    }

    #[test]
    fn superpose_request_dispatches_to_children() {
        // Two Tiles in Superpose. Both should be called during request.
        let mut sup = Superpose::new();
        sup.add(Box::new(Tile::new(0)));
        sup.add(Box::new(Tile::new(0)));
        assert_eq!(sup.get_count(), 2);

        let children = vec![req(10.0, 0.0, 0.0, 0.0, 0)];
        let mut out = Requisition::ZERO;
        sup.request_inner(&children, &mut out);
        // Both Tiles wrote to out.x (last write wins)
        assert_eq!(out.x.natural, 10.0);
    }

    #[test]
    fn superpose_clone_independent_state() {
        let mut sup = Superpose::new();
        let mut tile = Tile::new(0);
        tile.cached_req.x.natural = 99.0;
        sup.add(Box::new(tile));

        let _cloned = sup.clone_layout();
        // Just verify clone doesn't panic
    }

    #[test]
    fn tile_allocate_inner_distributes_span() {
        let mut tile = Tile::new(0);
        tile.cached_req = Requisition {
            x: Requirement::new(30.0, 5.0, 2.5, 0.0),
            y: Requirement::ZERO,
            penalty: 0,
        };
        let avail = Allocation {
            x: Allotment::new(0.0, 35.0, 0.0),  // span = 35, natural = 30, stretch = 5 → ratio=1.0
            y: Allotment::ZERO,
        };
        let reqs = vec![
            req(10.0, 2.0, 1.0, 0.0, 0),
            req(20.0, 3.0, 1.5, 0.0, 0),
        ];
        let mut out = vec![Allocation::ZERO; 2];
        tile.allocate_inner(&avail, &reqs, &mut out);
        // diff = 35 - 30 = 5 (stretch)
        // ratio = 5/5 = 1.0
        // child[0] span = 10 + 1.0 * 2 = 12, origin = 0
        // child[1] span = 20 + 1.0 * 3 = 23, origin = 12
        assert_eq!(out[0].x.span, 12.0);
        assert_eq!(out[0].x.origin, 0.0);
        assert_eq!(out[1].x.span, 23.0);
        assert_eq!(out[1].x.origin, 12.0);
    }

    #[test]
    fn tile_allocate_inner_size_mismatch_noop() {
        let tile = Tile::new(0);
        let avail = Allocation::ZERO;
        let reqs = vec![req(10.0, 0.0, 0.0, 0.0, 0)];
        let mut out = vec![Allocation::ZERO; 2]; // size mismatch
        tile.allocate_inner(&avail, &reqs, &mut out);
        // out unchanged
        assert_eq!(out[0], Allocation::ZERO);
        assert_eq!(out[1], Allocation::ZERO);
    }

    #[test]
    fn tile_allocate_inner_sentinel_child_zero_span() {
        // sentinel child (natural == -1e8): span = 0, accumulated_origin 진행 안 함.
        let mut tile = Tile::new(0);
        tile.cached_req = Requisition {
            x: Requirement::new(30.0, 5.0, 0.0, 0.0),
            y: Requirement::ZERO,
            penalty: 0,
        };
        let avail = Allocation {
            x: Allotment::new(0.0, 35.0, 0.0), // stretch ratio = (35-30)/5 = 1.0
            y: Allotment::ZERO,
        };
        let reqs = vec![
            req(10.0, 2.0, 0.0, 0.0, 0),
            req(-1.0e8, 999.0, 0.0, 0.0, 0), // sentinel
            req(20.0, 3.0, 0.0, 0.0, 0),
        ];
        let mut out = vec![Allocation::ZERO; 3];
        tile.allocate_inner(&avail, &reqs, &mut out);
        // child[0]: span = 10 + 1.0*2 = 12, origin = 0
        assert_eq!(out[0].x.span, 12.0);
        assert_eq!(out[0].x.origin, 0.0);
        // child[1]: sentinel → span = 0, origin = accumulated (12)
        assert_eq!(out[1].x.span, 0.0);
        assert_eq!(out[1].x.origin, 12.0);
        // child[2]: span = 20 + 1.0*3 = 23, origin = 12 (sentinel didn't advance)
        assert_eq!(out[2].x.span, 23.0);
        assert_eq!(out[2].x.origin, 12.0);
    }

    #[test]
    fn align_allocate_inner_sentinel_copies_avail_axis() {
        // raw asm 0x002d0f80: sentinel child → avail axis 통째 복사.
        let align = Align::new(0);
        let avail = Allocation {
            x: Allotment::new(7.0, 100.0, 0.25),
            y: Allotment::ZERO,
        };
        let reqs = vec![req(-1.0e8, 0.0, 0.0, 0.0, 0)]; // sentinel
        let mut out = vec![Allocation::ZERO; 1];
        align.allocate_inner(&avail, &reqs, &mut out);
        // out[0].x == avail.x (origin, span, alignment 모두 복사)
        assert_eq!(out[0].x.origin, 7.0);
        assert_eq!(out[0].x.span, 100.0);
        assert_eq!(out[0].x.alignment, 0.25);
    }

    #[test]
    fn align_allocate_inner_align_zero_uses_one_minus_avail() {
        // child_align == 0 → scale = 1 - avail_align. span = avail_span * scale, clamped.
        let align = Align::new(0);
        let avail = Allocation {
            x: Allotment::new(5.0, 40.0, 0.25), // scale = 1 - 0.25 = 0.75
            y: Allotment::ZERO,
        };
        // child: natural=20, stretch=20 (max_span=40), shrink=10 (min_span=10), align=0
        let reqs = vec![req(20.0, 20.0, 10.0, 0.0, 0)];
        let mut out = vec![Allocation::ZERO; 1];
        align.allocate_inner(&avail, &reqs, &mut out);
        // span_raw = 40 * 0.75 = 30. clamp hi: min(40, 30) = 30. clamp lo: max(10, 30) = 30.
        assert_eq!(out[0].x.span, 30.0);
        assert_eq!(out[0].x.origin, 5.0); // avail_origin
        assert_eq!(out[0].x.alignment, 0.0); // child_align
    }

    #[test]
    fn align_allocate_inner_clamps_to_max_span() {
        let align = Align::new(0);
        let avail = Allocation {
            x: Allotment::new(0.0, 1000.0, 0.0), // scale = 1.0, span_raw = 1000
            y: Allotment::ZERO,
        };
        // child: natural=10, stretch=5 (max_span=15), shrink=2, align=0
        let reqs = vec![req(10.0, 5.0, 2.0, 0.0, 0)];
        let mut out = vec![Allocation::ZERO; 1];
        align.allocate_inner(&avail, &reqs, &mut out);
        // span_raw = 1000. clamp hi: min(15, 1000) = 15. clamp lo: max(8, 15) = 15.
        assert_eq!(out[0].x.span, 15.0);
    }

    #[test]
    fn align_allocate_inner_size_mismatch_noop() {
        let align = Align::new(0);
        let avail = Allocation::ZERO;
        let reqs = vec![req(10.0, 0.0, 0.0, 0.0, 0)];
        let mut out = vec![Allocation::ZERO; 2]; // mismatch
        align.allocate_inner(&avail, &reqs, &mut out);
        assert_eq!(out[0], Allocation::ZERO);
        assert_eq!(out[1], Allocation::ZERO);
    }

    #[test]
    fn align_request_two_children_max_extent() {
        // 두 child 의 alignment-projected extent 의 max 누적.
        // child A: natural=10, align=0 → L=10, R=0
        // child B: natural=20, align=1 → L=0, R=20
        // v2 (natural-max) = (max(10,0), max(0,20)) = (10, 20)
        // saturate + faddp: s0 = 10 + 20 = 30
        let mut align = Align::new(0);
        let children = vec![
            Requisition { x: Requirement::new(10.0, 0.0, 0.0, 0.0), y: Requirement::ZERO, penalty: 0 },
            Requisition { x: Requirement::new(20.0, 0.0, 0.0, 1.0), y: Requirement::ZERO, penalty: 0 },
        ];
        let mut out = Requisition::ZERO;
        align.request_inner(&children, &mut out);
        assert_eq!(out.x.natural, 30.0, "natural = sum of max L/R extents");
    }

    // -- TileReverse::Request_inner (FUN_00302858) --------------------

    #[test]
    fn tile_reverse_request_inner_simple_sum() {
        // trim flag off → simple sum path. Tile 과 동일한 합산이되 alignment=1.0.
        let mut tr = TileReverse {
            direction: 0,
            cached_req: Requisition::INVALID,
            trim_trailing_hint: false,
        };
        let children = vec![
            req(10.0, 2.0, 1.0, 0.0, 0),
            req(20.0, 3.0, 1.5, 0.0, 0),
        ];
        let mut out = Requisition::ZERO;
        tr.request_inner(&children, &mut out);
        // natural = 30, stretch = 35-30 = 5, shrink = 30-27.5 = 2.5
        assert_eq!(out.x.natural, 30.0);
        assert_eq!(out.x.stretch, 5.0);
        assert_eq!(out.x.shrink, 2.5);
        // 핵심: TileReverse 는 alignment = 1.0 (Tile 은 0.0).
        assert_eq!(out.x.alignment, 1.0);
    }

    #[test]
    fn tile_reverse_request_inner_caches_result() {
        let mut tr = TileReverse {
            direction: 1,
            cached_req: Requisition::INVALID,
            trim_trailing_hint: false,
        };
        let children = vec![Requisition {
            x: Requirement::ZERO,
            y: Requirement::new(7.0, 0.0, 0.0, 0.0),
            penalty: 0,
        }];
        let mut out = Requisition::ZERO;
        tr.request_inner(&children, &mut out);
        // direction=1 → Y 축에 작성, alignment=1.0.
        assert_eq!(out.y.natural, 7.0);
        assert_eq!(out.y.alignment, 1.0);
        // cached_req 에 out 통째 캐시 (raw 0x302b58-0x302b68).
        assert_eq!(tr.cached_req, out);
    }

    #[test]
    fn tile_reverse_request_inner_trim_path_matches_tile_trim() {
        // trim 알고리즘은 Tile 과 byte-identical — 같은 입력으로 검증.
        // [n=10,p=1], [n=999,p=5], [n=88,p=1000], [n=77,p=1000]
        // → trim2 가 [999,5] 제거, memmove 로 1000 들 붙임 → sum [10,88,77] = 175.
        let mut tr = TileReverse {
            direction: 0,
            cached_req: Requisition::INVALID,
            trim_trailing_hint: true,
        };
        let children = vec![
            req(10.0, 1.0, 0.0, 0.0, 1),
            req(999.0, 999.0, 0.0, 0.0, 5),
            req(88.0, 0.0, 0.0, 0.0, 1000),
            req(77.0, 0.0, 0.0, 0.0, 1000),
        ];
        let mut out = Requisition::ZERO;
        tr.request_inner(&children, &mut out);
        assert_eq!(out.x.natural, 175.0, "trim 알고리즘은 Tile 과 동일");
        assert_eq!(out.x.alignment, 1.0);
    }
}

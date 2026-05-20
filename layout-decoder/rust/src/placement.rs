//! Placement layout strategies — `Hnc::Shape::Text::Place{Center,Fix,Margin,Natural}`.
//!
//! 본 모듈은 4 개의 *strategy* 클래스를 모델한다. 이들은 `Placement` glyph (= `MonoGlyph`
//! 의 Rust 표현) 의 `+0x10` 슬롯에 `SharePtr<Layout>` 으로 주입되어, `Placement::Request`
//! / `Placement::Allocate` dispatch (`CalcPlacement`) 에서 실제 layout 결정을 담당한다.
//!
//! ## vtable 검증 (`vtables.txt` data_dump)
//!
//! | strategy     | vtable addr | size | typeinfo            |
//! |--------------|-------------|------|---------------------|
//! | PlaceCenter  | 0x7810c0    | 16B  | `N3...PlaceCenterE` |
//! | PlaceFix     | 0x7810f8    | 16B  | `N3...PlaceFixE`    |
//! | PlaceMargin  | 0x781130    | 96B  | `N3...PlaceMarginE` |
//! | PlaceNatural | 0x781210    | 16B  | `N3...PlaceNaturalE`|
//!
//! PlaceCenter/PlaceFix/PlaceNatural 의 primary vtable 슬롯 (각 vtable [+0..+40]):
//!
//! | slot | 의미   | PlaceCenter        | PlaceFix          | PlaceNatural       |
//! |------|--------|--------------------|-------------------|--------------------|
//! | +0   | dtor1  | `0x33076c`         | `0x330858` no-op  | `0x331714` no-op   |
//! | +8   | dtor2  | `0x330770`         | operator_delete   | operator_delete    |
//! | +16  | Clone  | `0x330774`         | `0x330860`        | `0x33171c`         |
//! | +24  | Request| `0x3307c8`         | `0x3308a4` no-op  | `0x331750`         |
//! | +32  | Allocate|`0x3307e4`         | `0x3308a8`        | `0x33176c` no-op   |
//!
//! PlaceCenter vtable 의 [+40] 이후 (secondary RTTI block) 는 PlaceFix typeinfo + 그 다음
//! PlaceMargin typeinfo 가 들어가지만, **데이터 상속 없음** — `CreateCenter` /
//! `CreateFix` / `CreateNatural` 모두 `operator_new(0x10) = 16B` 만 할당. secondary
//! vtable 의 method 슬롯들은 `dynamic_cast` 가 사용하는 RTTI 메타데이터일 뿐, 실제
//! `this` 포인터를 받지 않는다.
//!
//! PlaceMargin 의 vtable 만 96B 객체에 대응, secondary 로 Placement (8B base) 의 typeinfo
//! 만 가짐 — PlaceMargin 은 Placement 의 base 가 아니라 (Placement 는 별도 24B 객체) ABI
//! RTTI 만 공유.
//!
//! ## CalcPlacement 흐름 (Placement::Allocate 의 핵심)
//!
//! `Placement::CalcPlacement` (`FUN_00331324` sz=280) 가 `Placement::Allocate` / `Draw`
//! 내부에서 호출하는 패턴:
//!
//! ```c
//! Requisition local_60 = INVALID;
//! child.Request(&local_60);                   // 자식 글리프 자기 size
//! layout.Allocate(parent_alloc, [local_60], [parent_alloc_copy]);
//!                                              // strategy 가 parent_alloc_copy 를 수정
//! *out_alloc = parent_alloc_copy;             // = 자식 sub-allocation
//! ```
//!
//! 주의: CalcPlacement 는 **strategy.Request 를 호출하지 않는다**. strategy.Request 는
//! Phase 1 (parent → `Placement::Request` dispatch) 에서 이미 한 번 호출되어 (`PlaceMargin`
//! 의 경우) 자기 객체 +0x38..+0x58 cache 슬롯에 post-margin Requisition 을 저장해 둠.
//! Phase 2 (parent → `Placement::Allocate`) 에서는 그 cache 를 strategy.Allocate 가
//! 직접 읽는다.
//!
//! ## Placement glyph wrapper 의 vfunc 5-8 (Draw/Undraw/GetBounds/Pick)
//!
//! `Placement` glyph (= Rust `MonoGlyph`) 의 vtable `0x781168` 의 추가 메소드들 — 모두
//! rendering layer 에 속하며 layout 단계에선 호출 안 됨. byte-equivalent 보장을 위한 raw
//! decompile 인용 + dispatch 패턴은 문서화만 (Rust 구현은 trait default 가 byte-eq).
//!
//! | vfunc | raw addr      | size | child vtable slot | CalcPlacement? | 시그니처                                            |
//! |-------|---------------|------|-------------------|----------------|-----------------------------------------------------|
//! | 5 Draw     | `0x331488` | 144B | `+0x28`           | YES            | `(Surface&, Allocation&, Flag&, BWMode&)`            |
//! | 6 Undraw   | `0x331518` | 32B  | `+0x30`           | **NO (direct forward)** | `(Flag&)`                                  |
//! | 7 GetBounds| `0x331538` | 176B | `+0x38`           | YES            | `(Theme*, Allocation&, Glyph*) → Allocation` (sret) |
//! | 8 Pick     | `0x3315e8` | 160B | `+0x40`           | YES            | `(Allocation&, Theme*, Hit&, int) → bool`            |
//!
//! ### Placement::Draw (`FUN_00331488`)
//! ```c
//! void Placement::Draw(Placement *this, Surface *s, Allocation *avail, Flag *flag, BWMode *bw) {
//!   if (child != NULL && *child != NULL) {
//!       Allocation local = *avail;
//!       CalcPlacement(this+8, avail, this+0x10, &local);
//!       child.vtable[+0x28](child, s, &local, flag, bw);   // child.Draw(...)
//!   }
//! }
//! ```
//!
//! ### Placement::Undraw (`FUN_00331518`) — **CalcPlacement 없이 direct tail-call**
//! ```c
//! void Placement::Undraw(Placement *this, Flag *flag) {
//!   if (child != NULL && *child != NULL) {
//!       child.vtable[+0x30](child, flag);   // tail-call, x1=flag preserved
//!   }
//! }
//! ```
//! Undraw 는 sub-allocation 이 필요 없음 — child 가 자기 cached_alloc 사용.
//!
//! ### Placement::GetBounds (`FUN_00331538`)
//! ```c
//! void Placement::GetBounds(Placement *this, Theme *th, Allocation *avail, Glyph *g,
//!                            /* sret */ Allocation *out) {
//!   if (child != NULL && *child != NULL) {
//!       Allocation local = *avail;
//!       CalcPlacement(this+8, avail, this+0x10, &local);
//!       child.vtable[+0x38](child, th, &local, g, /* sret */ out);
//!   } else {
//!       *out = {0, 0, 0, 0, 0, 0};   // zero Allocation
//!   }
//! }
//! ```
//! sret 출력 — child 가 null 이면 zero Allocation 반환 (24B 의 +0/+4/+0xc 슬롯 명시적 zero).
//!
//! ### Placement::Pick (`FUN_003315e8`)
//! ```c
//! u64 Placement::Pick(Placement *this, Allocation *avail, Theme *th, Hit *hit, int depth) {
//!   if (child != NULL && *child != NULL) {
//!       Allocation local = *avail;
//!       CalcPlacement(this+8, avail, this+0x10, &local);
//!       return child.vtable[+0x40](child, &local, th, hit, depth);
//!   }
//!   return 0;   // miss
//! }
//! ```
//!
//! ## 본 layout-decoder 모듈에서 vfunc 5-8 의 영향
//!
//! **layout output (LineSeg coord) byte-equiv 에 영향 없음** — Composition::repair 의
//! 흐름은 Request → Allocate 만 사용. Draw/Undraw 는 rendering phase, GetBounds/Pick 는
//! interactive UI (hit-test) 용도.
//!
//! 본 layout-decoder Rust 모듈에서 `Glyph::draw` / `undraw` / `get_bounds` / `pick` 의
//! override 는 (현재 시점) `BulletRenderGlyph` 뿐 — `self.layout.draw()` 로 layout (Box)
//! 에 forward. `Box::draw` 도 trait default no-op. 즉 본 모듈 안에서 호출 chain 끝의
//! 산출은 모두 zero/None — **child=null 인 raw Placement 의 sret zero 동작과
//! byte-equivalent**.
//!
//! **rendering phase 의 byte-equiv 가 필요할 때 별도 모듈로 진행해야 할 의존 type**:
//! - `Hnc::Shape::Surface` — PDF/screen rendering surface (CoreGraphics CGContext wrapper)
//! - `Hnc::Shape::Theme` — color/font theme
//! - `Hnc::Type::Flag` — bitset flag
//! - `Hnc::Shape::BWMode` — black/white mode
//! - `Hnc::Shape::Text::Hit` — hit-test result
//!
//! 이 5개 type 의 RE 가 prerequisite. 본 layout-decoder 의 vfunc 3-4 (Request/Allocate)
//! byte-equiv 완료와 독립.
//!
//! ## Rust trait 시그니처 결정
//!
//! - `request(&self, &mut Requisition)` — raw `XxxPlacement::Request(this, vec, req)` 의
//!   1:1. `vec` (`std::vector<Requisition>`) 인자는 raw 의 `PlaceCenter`/`PlaceFix`/
//!   `PlaceMargin`/`PlaceNatural` 의 decompile 어디에서도 사용되지 않음 (호출자 빈 vec
//!   전달) — Rust 시그니처에서 제거.
//! - `allocate(&self, &mut Allocation, &Requisition)` — raw `XxxPlacement::Allocate(this,
//!   parent_alloc, req_vec, alloc_vec)` 의 1:1. raw 의 `parent_alloc` 인자는 어느
//!   strategy 도 사용하지 않음 (PlaceMargin 은 `alloc_vec[0]` 으로 작업, PlaceCenter 는
//!   `req_vec[0]` + `alloc_vec[0]`, PlaceFix 는 `alloc_vec[0]` 만, PlaceNatural 은 no-op)
//!   — Rust 시그니처에서 제거. `alloc_vec[0]` 은 Rust 의 `alloc: &mut Allocation` 로
//!   대체 (Phase 2 의 CalcPlacement 가 그 vec 의 begin 원소를 parent_alloc 복사로 초기화).
//!   `req_vec[0]` 은 Rust 의 `req: &Requisition` (= child 의 자연 request 결과).
//! - `&self` 가 가능한 이유: cache (PlaceMargin 한정) 는 `Cell<Requisition>` 으로 interior
//!   mutability. 나머지 strategy 는 cache 없음.

use std::cell::Cell;

use crate::value_types::{Allocation, Requirement, Requisition};

// ============================================================
// trait Placement — 4 strategy 공통 interface
// ============================================================

/// `Hnc::Shape::Text::Layout` virtual interface — strategy 4 종이 구현하는 vtable 메소드.
///
/// raw 의 `Layout` 추상 base 는 vtable 만 갖고 데이터 없음. `+0`/`+8` dtor1/dtor2,
/// `+16` Clone, `+24` Request, `+32` Allocate, `+40..` Draw/Pick/GetBounds 등 — 본 단계엔
/// Request/Allocate 만 byte-equivalent 로 포팅 (Draw 등은 layout output 무관).
pub trait Placement: std::fmt::Debug {
    /// `Clone() const` — 자기 자신을 새 heap 객체로 복제.
    fn clone_placement(&self) -> Box<dyn Placement>;

    /// `dynamic_cast<T*>` 대응 — 각 strategy 가 `{ self }` 로 override.
    fn as_any(&self) -> &dyn std::any::Any;

    /// `Request(vector<Requisition> const& /*unused*/, Requisition& out_req)` — vfunc[3].
    ///
    /// raw 의 4 strategy decompile 모두 첫 인자 `vector` 는 미사용 (Layout abstract base
    /// 의 시그니처 매칭용). 두 번째 인자 `out_req` 만 사용.
    ///
    /// **base default no-op** — PlaceNatural/PlaceFix 외 strategy 는 본인 구현으로 override.
    /// (PlaceFix::Request 는 raw 도 4B `return` 인 진짜 no-op 이므로 default 그대로 사용.)
    fn request(&self, _out_req: &mut Requisition) {}

    /// `Allocate(Allocation const& /*parent_alloc, unused by strategies*/,
    ///   vector<Requisition> const& req_vec, vector<Allocation>& alloc_vec)` — vfunc[4].
    ///
    /// raw 의 strategy.Allocate 는 `alloc_vec[0]` (size 1 의 vector) 를 in-place 로 수정
    /// 하여 child sub-allocation 을 만들어 출력함 — Rust 는 `alloc: &mut Allocation` 로
    /// 단순화. `req_vec[0]` 는 PlaceCenter 만 사용 (`req.alignment`); 나머지는 자기 cache
    /// 또는 자기 필드만 사용.
    ///
    /// **base default no-op** — PlaceNatural 의 raw Allocate (`FUN_0033176c`) 가 4B `return`
    /// 인 진짜 no-op 이므로 default 그대로 사용.
    fn allocate(&self, _alloc: &mut Allocation, _req: &Requisition) {}
}

// ============================================================
// PlaceCenter — 16B, vtable @ 0x7810c0
// ============================================================

/// `Hnc::Shape::Text::PlaceCenter` — alignment slot 으로 자식 정렬.
///
/// Object layout (16B, raw 검증: `LayoutFactory::CreateCenter` `FUN_00317d0c`
/// line 22-23 / 52-53 — Superpose 가 HCenter + VCenter 두 개 생성):
/// - `+0x00` vtable = `PTR__PlaceCenter_007810c0`
/// - `+0x08` `dimension: i32` — 0=H (X 슬롯 사용) / 1=V (Y 슬롯 사용). `CreateCenter` 가
///   첫 번째 객체에 immediate `0`, 두 번째에 immediate `1` 로 set.
/// - `+0x0c` `alignment: f32` — `CreateCenter` 의 `param_2` (H) / `param_3` (V) 인자.
///   `GetAlignment()` (`0x3307b8`) 가 +0xc 반환.
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct PlaceCenter {
    /// `+0x08` — dimension. 0=Horizontal / 1=Vertical.
    pub dimension: i32,
    /// `+0x0c` — alignment ratio (0..1).
    pub alignment: f32,
}

impl Placement for PlaceCenter {
    fn clone_placement(&self) -> Box<dyn Placement> { Box::new(*self) }
    fn as_any(&self) -> &dyn std::any::Any { self }

    /// `PlaceCenter::Request` (`FUN_003307c8`, 28B) 의 1:1 포팅.
    ///
    /// raw decompile:
    /// ```c
    /// void PlaceCenter::Request(PlaceCenter *this, vector* /*unused*/, Requisition *req) {
    ///   if (*(int *)(this + 8) != 0) {        // dimension != 0 (= Vertical)
    ///       req = req + 0x10;                  // → y 슬롯 시작
    ///   }
    ///   *(undefined4 *)(req + 0xc) = *(undefined4 *)(this + 0xc);  // *.alignment = self.alignment
    /// }
    /// ```
    fn request(&self, req: &mut Requisition) {
        if self.dimension != 0 {
            req.y.alignment = self.alignment;
        } else {
            req.x.alignment = self.alignment;
        }
    }

    /// `PlaceCenter::Allocate` (`FUN_003307e4`, 92B) 의 1:1 포팅.
    ///
    /// raw decompile:
    /// ```c
    /// void PlaceCenter::Allocate(PlaceCenter *this, Allocation *parent_alloc,
    ///                            vector<Requisition> *req_vec, vector<Allocation> *alloc_vec) {
    ///   lVar3 = *(long *)req_vec;                       // req_vec.begin
    ///   if (lVar3 != *(long *)(req_vec + 8) &&          // req_vec not empty AND
    ///       (lVar4 = *(long *)alloc_vec,
    ///        lVar4 != *(long *)(alloc_vec + 8))) {      // alloc_vec not empty
    ///     bVar5 = *(int *)(this + 8) != 0;              // dimension != 0
    ///     lVar1 = 0;
    ///     if (bVar5) lVar1 = 0xc;                       // alloc.y.origin offset
    ///     lVar2 = lVar4;
    ///     if (bVar5) lVar2 = lVar4 + 0xc;               // alloc.{x|y} Allotment base
    ///     if (bVar5) lVar3 = lVar3 + 0x10;              // req.{x|y} Requirement base
    ///     fVar6 = *(float *)(lVar3 + 0xc);              // req.{x|y}.alignment
    ///     // alloc.{x|y}.origin += alloc.{x|y}.span * (req.alignment - alloc.alignment)
    ///     *(float *)(lVar4 + lVar1) =
    ///         *(float *)(lVar4 + lVar1) +
    ///         *(float *)(lVar2 + 4) * (fVar6 - *(float *)(lVar2 + 8));
    ///     *(float *)(lVar2 + 8) = fVar6;                // alloc.{x|y}.alignment = req.alignment
    ///   }
    /// }
    /// ```
    ///
    /// 의미: 자식의 자연 alignment 를 alloc 의 alignment 슬롯으로 옮기되, origin 도 같이
    /// 보정해 alignment-point 의 절대 좌표는 보존. `begin = origin - alignment*span`
    /// 으로 새/구 alignment 에 대해 동일하게 산출되도록 유지.
    ///
    /// Rust 의 `Allotment` field layout 일치 (`value_types.rs` 검증):
    /// - `Allotment.origin` = `+0`, `Allotment.span` = `+4`, `Allotment.alignment` = `+8`.
    fn allocate(&self, alloc: &mut Allocation, req: &Requisition) {
        // raw `req_vec.begin != end` + `alloc_vec.begin != end` 가드는 Rust 의 `req: &Requisition`
        // 과 `alloc: &mut Allocation` 가 무조건 valid 라 도달 불가능 — 생략.
        if self.dimension != 0 {
            let align = req.y.alignment;
            alloc.y.origin += alloc.y.span * (align - alloc.y.alignment);
            alloc.y.alignment = align;
        } else {
            let align = req.x.alignment;
            alloc.x.origin += alloc.x.span * (align - alloc.x.alignment);
            alloc.x.alignment = align;
        }
    }
}

// ============================================================
// PlaceFix — 16B, vtable @ 0x7810f8
// ============================================================

/// `Hnc::Shape::Text::PlaceFix` — 자식에게 고정 span (width 또는 height) 할당.
///
/// Object layout (16B, raw 검증: `LayoutFactory::CreateHFix` `FUN_00318628`
/// line 19-23 + `CreateFix` `FUN_00318290` line 25-27 — Superpose 가 HFix + VFix 두 개
/// 생성):
/// - `+0x00` vtable = `PTR_FUN_007810f8`
/// - `+0x08` `dimension: i32` — 0=H / 1=V. `CreateFix` 가 첫 번째에 immediate `0`,
///   두 번째에 immediate `1` 로 set.
/// - `+0x0c` `fix_size: f32` — fixed span 값. `CreateFix` 의 `param_2`(H) / `param_3`(V).
///
/// **PlaceFix 는 PlaceMargin 의 base 가 아니다** — vtable @ 0x7810f8 의 secondary
/// sub-vtable 들 (`+48 PlaceMargin typeinfo`, `+104 Placement typeinfo`) 는 Itanium ABI
/// 의 RTTI 메타데이터일 뿐이고, `CreateHFix` 가 `operator_new(0x10) = 16B` 만 할당하므로
/// 데이터 상속 없음. secondary 의 PlaceMargin method 슬롯들 (`+72 Clone`, `+80 Request`,
/// `+88 Allocate` 가 PlaceMargin 의 진짜 메소드 주소) 는 PlaceFix 객체에 절대 호출되지
/// 않음 (호출되면 OOB read 로 UB).
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct PlaceFix {
    /// `+0x08` — dimension. 0=Horizontal / 1=Vertical.
    pub dimension: i32,
    /// `+0x0c` — fixed span size.
    pub fix_size: f32,
}

impl Placement for PlaceFix {
    fn clone_placement(&self) -> Box<dyn Placement> { Box::new(*self) }
    fn as_any(&self) -> &dyn std::any::Any { self }

    /// `PlaceFix::Request` (`FUN_003308a4`, 4B) — raw 가 `return` 만 있는 진짜 no-op.
    /// trait default 그대로 사용 — override 안 함.
    // (no `fn request` override — base default no-op 사용)

    /// `PlaceFix::Allocate` (`FUN_003308a8`, 40B) 의 1:1 포팅.
    ///
    /// raw decompile:
    /// ```c
    /// void PlaceFix::Allocate(PlaceFix *this, Allocation*, vector*, long *alloc_vec) {
    ///   lVar1 = *alloc_vec;                          // alloc_vec.begin
    ///   if (lVar1 != alloc_vec[1]) {                 // not empty
    ///     if (*(int *)(this + 8) != 0) {              // dimension != 0
    ///         lVar1 = lVar1 + 0xc;                    // y Allotment base
    ///     }
    ///     *(undefined4 *)(lVar1 + 4) = *(undefined4 *)(this + 0xc);
    ///     // alloc.{x|y}.span = self.fix_size
    ///   }
    /// }
    /// ```
    ///
    /// 의미: 자식이 받게 될 sub-allocation 의 해당 축 span 을 `self.fix_size` 로 강제.
    /// origin/alignment 는 그대로 유지.
    fn allocate(&self, alloc: &mut Allocation, _req: &Requisition) {
        if self.dimension != 0 {
            alloc.y.span = self.fix_size;
        } else {
            alloc.x.span = self.fix_size;
        }
    }
}

// ============================================================
// PlaceNatural — 16B, vtable @ 0x781210
// ============================================================

/// `Hnc::Shape::Text::PlaceNatural` — alignment 슬롯에 span 비트 패턴을 set (raw 의
/// alignment 슬롯 재활용 패턴).
///
/// Object layout (16B, raw 검증: `LayoutFactory::CreateHNatural` `FUN_00318fe0`
/// line 19-23):
/// - `+0x00` vtable = `PTR_FUN_00781210`
/// - `+0x08` `direction: i32` — 0=H/X, 1=V/Y.
/// - `+0x0c` `span: f32` — natural ratio.
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct PlaceNatural {
    /// `+0x08` — direction. 0=X/H, 1=Y/V.
    pub direction: i32,
    /// `+0x0c` — span (자연 크기 ratio).
    pub span: f32,
}

impl Placement for PlaceNatural {
    fn clone_placement(&self) -> Box<dyn Placement> { Box::new(*self) }
    fn as_any(&self) -> &dyn std::any::Any { self }

    /// `PlaceNatural::Request` (`FUN_00331750`, 16B) 의 1:1 포팅.
    ///
    /// raw decompile:
    /// ```c
    /// void PlaceNatural::Request(this, vector* /*unused*/, Requisition *req) {
    ///   *(undefined4 *)(req + (ulong)(*(int *)(this + 8) != 0) * 0x10 + 0xc)
    ///       = *(undefined4 *)(this + 0xc);
    /// }
    /// ```
    /// → `direction == 0`: req.x.alignment 에 span 비트 set; `direction != 0`: req.y.alignment.
    ///
    /// 의미상 미묘 — raw 가 span 을 alignment 슬롯에 (f32 bit pattern 으로) write.
    /// Hancom 의 PlaceNatural 가 alignment 슬롯을 "natural ratio" 의 storage 로 재활용
    /// (subsequent consumer 의 해석 영역).
    fn request(&self, req: &mut Requisition) {
        if self.direction != 0 {
            req.y.alignment = self.span;
        } else {
            req.x.alignment = self.span;
        }
    }

    // `PlaceNatural::Allocate` (`FUN_0033176c`, 4B) — raw 가 `return` 만 있는 진짜 no-op.
    // trait default 그대로 사용. (no `fn allocate` override)
}

// ============================================================
// PlaceMargin — 96B, vtable @ 0x781130
// ============================================================

/// `Hnc::Shape::Text::PlaceMargin` — 자식 둘레로 margin 추가 (HMargin/VMargin/LMargin/
/// RMargin/TMargin/BMargin).
///
/// Object layout (96B, raw 검증: ctor `PlaceMargin::PlaceMargin(float)` `FUN_003308d0`
/// sz=84 line 14-29 + Clone `FUN_00330b64` sz=96 `operator_new(0x60)` line 14):
///
/// ```text
/// +0x00  ptr  vtable = PTR__PlaceMargin_00781130
/// +0x08  f32  left.natural        ; ctor 가 param_1 으로 init
/// +0x0c  f32  left.stretch        ; ctor 0
/// +0x10  f32  left.shrink         ; ctor 0
/// +0x14  f32  top.natural         ; ctor param_1
/// +0x18  f32  top.stretch         ; ctor 0
/// +0x1c  f32  top.shrink          ; ctor 0
/// +0x20  f32  right.natural       ; ctor param_1
/// +0x24  f32  right.stretch       ; ctor 0
/// +0x28  f32  right.shrink        ; ctor 0
/// +0x2c  f32  bottom.natural      ; ctor param_1
/// +0x30  f32  bottom.stretch      ; ctor 0 (`_DAT_00741f70` lo-32 = 0)
/// +0x34  f32  bottom.shrink       ; ctor 0 (`_DAT_00741f70` hi-32 = 0)
/// +0x38  f32  cache.x.natural     ; ctor -1e8 (`_UNK_00741f78` = INVALID)
/// +0x3c  f32  cache.x.stretch     ; ctor 0
/// +0x40  f32  cache.x.shrink      ; ctor 0
/// +0x44  f32  cache.x.alignment   ; ctor 0
/// +0x48  f32  cache.y.natural     ; ctor -1e8 (0xccbebc20)
/// +0x4c  f32  cache.y.stretch     ; ctor 0
/// +0x50  f32  cache.y.shrink      ; ctor 0
/// +0x54  f32  cache.y.alignment   ; ctor 0
/// +0x58  i32  cache.penalty       ; ctor 0
/// +0x5c  pad  4 byte (Clone 이 0x60 만 alloc, 마지막 4B 미작성 — 0 padding)
/// ```
///
/// **DAT 상수 의미** (ctor `FUN_003308d0` line 22-23):
/// - `_DAT_00741f70` (8B) — `(0.0, 0.0)` = `(bottom.stretch, bottom.shrink)` 초기값.
/// - `_UNK_00741f78` (8B) — `(-1e8, 0.0)` = `(cache.x.natural, cache.x.stretch)` 초기값.
///   `0xccbebc20` 가 -1e8 의 f32 bit pattern (= `Requirement::INVALID_NATURAL`).
///   PlaceMargin::Request 의 `if (cache.x.natural != -1e8)` 가드의 sentinel 와 일치.
///
/// **cache 의 역할** (raw `PlaceMargin::Request` `0x330cfc` + `Allocate` `0x330dc0`):
/// - Phase 1: parent → `Placement::Request` → child.Request + `PlaceMargin::Request`.
///   `PlaceMargin::Request` 가 (1) req → cache 로 복사, (2) cache 에 margin 더함
///   (only if natural valid), (3) cache → req 로 write back. 그래서 parent 가 받는 req
///   는 post-margin, cache 는 post-margin 으로 영구 보존.
/// - Phase 2: parent → `Placement::Allocate` → `CalcPlacement` → strategy.Allocate.
///   strategy.Allocate 는 cache (+0x38..+0x58) 를 사용해 alloc 의 origin/span 을 inset.
///   `CalcPlacement` 는 strategy.Request 를 다시 호출하지 않음 — cache 만 의지.
///
/// Rust 의 cache 는 `Cell<Requisition>` 으로 interior mutability — trait `request(&self)`
/// 시그니처를 유지하면서 `request` 시 cache 갱신 가능.
#[derive(Debug)]
#[repr(C)]
pub struct PlaceMargin {
    /// `+0x08` — left margin natural width.
    pub left_natural: f32,
    /// `+0x0c` — left margin stretch.
    pub left_stretch: f32,
    /// `+0x10` — left margin shrink.
    pub left_shrink: f32,
    /// `+0x14` — top margin natural height.
    pub top_natural: f32,
    /// `+0x18` — top margin stretch.
    pub top_stretch: f32,
    /// `+0x1c` — top margin shrink.
    pub top_shrink: f32,
    /// `+0x20` — right margin natural width.
    pub right_natural: f32,
    /// `+0x24` — right margin stretch.
    pub right_stretch: f32,
    /// `+0x28` — right margin shrink.
    pub right_shrink: f32,
    /// `+0x2c` — bottom margin natural height.
    pub bottom_natural: f32,
    /// `+0x30` — bottom margin stretch.
    pub bottom_stretch: f32,
    /// `+0x34` — bottom margin shrink.
    pub bottom_shrink: f32,
    /// `+0x38..+0x5c` — post-margin Requisition cache. `Cell` 로 interior mutability.
    /// Phase 1 `request` 가 write, Phase 2 `allocate` 가 read.
    pub cache: Cell<Requisition>,
}

impl Default for PlaceMargin {
    /// raw ctor `PlaceMargin::PlaceMargin(float param_1)` (`FUN_003308d0`, sz=84) 의 1:1.
    ///
    /// raw 의 single-arg ctor 는 param_1 을 left/top/right/bottom 의 natural 에 모두 set,
    /// stretch/shrink 는 0. cache 의 x.natural / y.natural 은 -1e8 (INVALID), 나머지 0.
    ///
    /// Rust 의 `Default::default()` 는 raw 의 param_1 = 0.0 호출에 해당.
    fn default() -> Self {
        Self::with_uniform_margin(0.0)
    }
}

impl Clone for PlaceMargin {
    fn clone(&self) -> Self {
        Self {
            left_natural: self.left_natural,
            left_stretch: self.left_stretch,
            left_shrink: self.left_shrink,
            top_natural: self.top_natural,
            top_stretch: self.top_stretch,
            top_shrink: self.top_shrink,
            right_natural: self.right_natural,
            right_stretch: self.right_stretch,
            right_shrink: self.right_shrink,
            bottom_natural: self.bottom_natural,
            bottom_stretch: self.bottom_stretch,
            bottom_shrink: self.bottom_shrink,
            cache: Cell::new(self.cache.get()),
        }
    }
}

impl PlaceMargin {
    /// raw 의 1-arg ctor `FUN_003308d0` 와 동등: 4 변에 같은 natural margin, stretch/shrink=0.
    pub fn with_uniform_margin(margin: f32) -> Self {
        Self {
            left_natural: margin,
            left_stretch: 0.0,
            left_shrink: 0.0,
            top_natural: margin,
            top_stretch: 0.0,
            top_shrink: 0.0,
            right_natural: margin,
            right_stretch: 0.0,
            right_shrink: 0.0,
            bottom_natural: margin,
            bottom_stretch: 0.0,
            bottom_shrink: 0.0,
            cache: Cell::new(Requisition::INVALID),
        }
    }

    /// raw 의 4-arg ctor `FUN_003309cc` / `FUN_00330a20` (left, top, right, bottom 의 natural
    /// margin, stretch/shrink=0) 와 동등. (4-arg ctor 의 정확한 매핑은 후속 RE 가 필요하나
    /// 본 단계에선 layout 영향 없음.)
    pub fn with_natural(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self {
            left_natural: left,
            left_stretch: 0.0,
            left_shrink: 0.0,
            top_natural: top,
            top_stretch: 0.0,
            top_shrink: 0.0,
            right_natural: right,
            right_stretch: 0.0,
            right_shrink: 0.0,
            bottom_natural: bottom,
            bottom_stretch: 0.0,
            bottom_shrink: 0.0,
            cache: Cell::new(Requisition::INVALID),
        }
    }

    /// raw 의 12-arg ctor `FUN_00330ac8` 와 동등 — 각 변의 natural/stretch/shrink 모두 set.
    /// 인자 순서는 raw 의 `(left_n, left_s, left_sh, top_n, top_s, top_sh, right_n, right_s,
    /// right_sh, bottom_n, bottom_s, bottom_sh)` (후속 RE 로 정확한 매핑 확정 필요 — 본
    /// 12-arg ctor 의 fully decompile 은 후속 단계).
    #[allow(clippy::too_many_arguments)]
    pub fn with_full(
        left_natural: f32, left_stretch: f32, left_shrink: f32,
        top_natural: f32, top_stretch: f32, top_shrink: f32,
        right_natural: f32, right_stretch: f32, right_shrink: f32,
        bottom_natural: f32, bottom_stretch: f32, bottom_shrink: f32,
    ) -> Self {
        Self {
            left_natural, left_stretch, left_shrink,
            top_natural, top_stretch, top_shrink,
            right_natural, right_stretch, right_shrink,
            bottom_natural, bottom_stretch, bottom_shrink,
            cache: Cell::new(Requisition::INVALID),
        }
    }

    /// `PlaceMargin::CalcSpan(float total, Requirement const&, float natural_part,
    ///   float stretch_factor, float shrink_factor)` (`FUN_00330f08`, 76B) 의 1:1 포팅.
    ///
    /// raw decompile:
    /// ```c
    /// float PlaceMargin::CalcSpan(float total, Requirement *req, float natural_part,
    ///                              float stretch_factor, float shrink_factor) {
    ///   float diff = total - req.natural;
    ///   if (0.0 < diff && 0.0 < req.stretch) {
    ///       return natural_part + (stretch_factor / req.stretch) * diff;
    ///   }
    ///   float ratio = 0.0;
    ///   if (diff < 0.0 && 0.0 < req.shrink) {
    ///       ratio = shrink_factor / req.shrink;
    ///   }
    ///   return natural_part + ratio * diff;
    /// }
    /// ```
    ///
    /// 의미: 자식이 받을 axis 의 effective margin 계산 — 자연 margin 에 stretch/shrink
    /// 비율로 추가 보정. 본 헬퍼는 Allocate 안에서 inlined 로 사용되지만 향후 동일 로직의
    /// 재사용 / 검증 용도로 노출.
    pub fn calc_span(
        total: f32,
        req: &Requirement,
        natural_part: f32,
        stretch_factor: f32,
        shrink_factor: f32,
    ) -> f32 {
        let diff = total - req.natural;
        if 0.0 < diff && 0.0 < req.stretch {
            return natural_part + (stretch_factor / req.stretch) * diff;
        }
        let mut ratio = 0.0;
        if diff < 0.0 && 0.0 < req.shrink {
            ratio = shrink_factor / req.shrink;
        }
        natural_part + ratio * diff
    }
}

impl Placement for PlaceMargin {
    fn clone_placement(&self) -> Box<dyn Placement> { Box::new(self.clone()) }
    fn as_any(&self) -> &dyn std::any::Any { self }

    /// `PlaceMargin::Request` (`FUN_00330cfc`, sz=196) 의 1:1 포팅.
    ///
    /// raw decompile (변수 alias 정리):
    /// ```c
    /// void PlaceMargin::Request(PlaceMargin *this, vector* /*unused*/, Requisition *req) {
    ///   // step 1: req → cache 로 8 float (+ penalty) 복사.
    ///   cache.x.natural   = req.x.natural;        // +0x38 ← req+0
    ///   cache.x.stretch   = req.x.stretch;        // +0x3c ← req+4
    ///   cache.x.shrink    = req.x.shrink;         // +0x40 ← req+8
    ///   cache.x.alignment = req.x.alignment;      // +0x44 ← req+0xc
    ///   cache.y.natural   = req.y.natural;        // +0x48 ← req+0x10
    ///   cache.y.stretch   = req.y.stretch;        // +0x4c ← req+0x14
    ///   cache.y.shrink    = req.y.shrink;         // +0x50 ← req+0x18
    ///   cache.y.alignment = req.y.alignment;      // +0x54 ← req+0x1c
    ///   cache.penalty     = req.penalty;          // +0x58 ← req+0x20
    ///   // step 2a: x axis margin 추가 (only if natural valid).
    ///   if (cache.x.natural != -1e8) {
    ///       cache.x.natural += left.natural + right.natural;
    ///       cache.x.stretch += left.stretch + right.stretch;
    ///       cache.x.shrink  += left.shrink  + right.shrink;
    ///   }
    ///   // step 2b: y axis margin 추가.
    ///   if (cache.y.natural != -1e8) {
    ///       cache.y.natural += top.natural + bottom.natural;
    ///       cache.y.stretch += top.stretch + bottom.stretch;
    ///       cache.y.shrink  += top.shrink  + bottom.shrink;
    ///   }
    ///   // step 3: cache → req 로 write back.
    ///   req.x = cache.x;
    ///   req.y = cache.y;
    ///   req.penalty = cache.penalty;
    /// }
    /// ```
    ///
    /// alignment 는 modification 대상 아님 — pre/post 동일. INVALID natural 인 axis 는
    /// margin 적용 안 함 (raw 의 sentinel 처리).
    fn request(&self, req: &mut Requisition) {
        // step 1: req → local cache snapshot.
        let mut c = *req;

        // step 2a: x axis margin (only if natural valid).
        if c.x.natural != Requirement::INVALID_NATURAL {
            c.x.natural += self.left_natural + self.right_natural;
            c.x.stretch += self.left_stretch + self.right_stretch;
            c.x.shrink  += self.left_shrink  + self.right_shrink;
        }
        // step 2b: y axis margin.
        if c.y.natural != Requirement::INVALID_NATURAL {
            c.y.natural += self.top_natural + self.bottom_natural;
            c.y.stretch += self.top_stretch + self.bottom_stretch;
            c.y.shrink  += self.top_shrink  + self.bottom_shrink;
        }

        // step 3: cache 영구 저장 + req write back.
        self.cache.set(c);
        *req = c;
    }

    /// `PlaceMargin::Allocate` (`FUN_00330dc0`, sz=328) 의 1:1 포팅.
    ///
    /// raw decompile (변수 alias 정리; axes 처리 X / Y 가 대칭):
    /// ```c
    /// void PlaceMargin::Allocate(PlaceMargin *this, Allocation* /*unused*/,
    ///                            vector* /*req, unused*/, vector<Allocation>* alloc_vec) {
    ///   pfVar1 = alloc_vec.begin;
    ///   if (pfVar1 == alloc_vec.end) return;
    ///
    ///   // === X axis ===
    ///   float cx_shrink = cache.x.shrink;      // (this+0x40)
    ///   float diff_x = pfVar1[1] /*alloc.x.span*/ - cache.x.natural;
    ///   float lspan, rspan;
    ///   if (diff_x <= 0.0 || cache.x.stretch <= 0.0) {
    ///       // shrink path (or no-stretch)
    ///       float lr = left.shrink / cx_shrink;
    ///       float rr = 0.0;
    ///       if (diff_x >= 0.0 || cx_shrink <= 0.0) {
    ///           lr = 0.0;
    ///       }
    ///       lspan = left.natural + lr * diff_x;
    ///       float rn = right.natural;          // (this+0x20)
    ///       if (diff_x < 0.0 && cx_shrink > 0.0) {
    ///           rr = right.shrink / cx_shrink;
    ///       }
    ///       rspan = rn + rr * diff_x;
    ///   } else {
    ///       // stretch path
    ///       lspan = left.natural + (left.stretch / cache.x.stretch) * diff_x;
    ///       rspan = right.natural + (right.stretch / cache.x.stretch) * diff_x;
    ///   }
    ///   // origin update — alignment is post-margin (cache.x.alignment).
    ///   alloc.x.origin =
    ///       alloc.x.origin + (-(cache.x.alignment * rspan)) +
    ///                        (lspan * (1.0 - cache.x.alignment));
    ///   alloc.x.span = alloc.x.span - (lspan + rspan);
    ///
    ///   // === Y axis (대칭) ===
    ///   // ... (top/bottom)
    /// }
    /// ```
    ///
    /// 의미: parent_alloc 에서 margin 만큼 inset 한 sub-allocation 을 계산하고 alloc 을
    /// in-place 갱신. raw 의 `pfVar1[*]` 는 첫 번째 Allocation 의 f32 array view:
    /// `[0]=x.origin, [1]=x.span, [2]=x.alignment, [3]=y.origin, [4]=y.span, [5]=y.alignment`.
    fn allocate(&self, alloc: &mut Allocation, _req: &Requisition) {
        let cache = self.cache.get();

        // === X axis ===
        let cx_shrink = cache.x.shrink;
        let cx_stretch = cache.x.stretch;
        let cx_natural = cache.x.natural;
        let cx_align = cache.x.alignment;
        let diff_x = alloc.x.span - cx_natural;

        let (lspan, rspan) = if diff_x <= 0.0 || cx_stretch <= 0.0 {
            // raw 의 shrink/no-stretch 경로.
            let mut lr = self.left_shrink / cx_shrink;
            let mut rr = 0.0;
            if diff_x >= 0.0 || cx_shrink <= 0.0 {
                lr = 0.0;
            }
            let lspan = self.left_natural + lr * diff_x;
            if diff_x < 0.0 && cx_shrink > 0.0 {
                rr = self.right_shrink / cx_shrink;
            }
            let rspan = self.right_natural + rr * diff_x;
            (lspan, rspan)
        } else {
            // raw 의 stretch 경로 (diff > 0 && stretch > 0).
            let lspan = self.left_natural + (self.left_stretch / cx_stretch) * diff_x;
            let rspan = self.right_natural + (self.right_stretch / cx_stretch) * diff_x;
            (lspan, rspan)
        };
        alloc.x.origin = alloc.x.origin + (-(cx_align * rspan)) + lspan * (1.0 - cx_align);
        alloc.x.span -= lspan + rspan;

        // === Y axis === (raw 의 line 48-70 — X 와 대칭 구조)
        let cy_shrink = cache.y.shrink;
        let cy_stretch = cache.y.stretch;
        let cy_natural = cache.y.natural;
        let cy_align = cache.y.alignment;
        let diff_y = alloc.y.span - cy_natural;

        let (tspan, bspan) = if diff_y <= 0.0 || cy_stretch <= 0.0 {
            let mut tr = self.top_shrink / cy_shrink;
            let mut br = 0.0;
            if diff_y >= 0.0 || cy_shrink <= 0.0 {
                tr = 0.0;
            }
            let tspan = self.top_natural + tr * diff_y;
            if diff_y < 0.0 && cy_shrink > 0.0 {
                br = self.bottom_shrink / cy_shrink;
            }
            let bspan = self.bottom_natural + br * diff_y;
            (tspan, bspan)
        } else {
            let tspan = self.top_natural + (self.top_stretch / cy_stretch) * diff_y;
            let bspan = self.bottom_natural + (self.bottom_stretch / cy_stretch) * diff_y;
            (tspan, bspan)
        };
        alloc.y.origin = alloc.y.origin + (-(cy_align * bspan)) + tspan * (1.0 - cy_align);
        alloc.y.span -= tspan + bspan;
    }
}

// ============================================================
// Tests — Placement strategy request/allocate 검증
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value_types::{Allotment, Requisition};

    // ---- PlaceCenter ----

    #[test]
    fn place_center_request_horizontal_sets_x_alignment() {
        let pc = PlaceCenter { dimension: 0, alignment: 0.5 };
        let mut req = Requisition::INVALID;
        pc.request(&mut req);
        assert_eq!(req.x.alignment, 0.5);
        assert_eq!(req.y.alignment, 0.0);
    }

    #[test]
    fn place_center_request_vertical_sets_y_alignment() {
        let pc = PlaceCenter { dimension: 1, alignment: 0.75 };
        let mut req = Requisition::INVALID;
        pc.request(&mut req);
        assert_eq!(req.y.alignment, 0.75);
    }

    /// raw `FUN_003307e4` line 39-41 검증: alloc.x.origin 갱신 + alignment overwrite.
    /// alloc=(origin=10, span=100, align=0.0), req.x.alignment=0.5 → origin += 100*(0.5-0)
    /// = 60, alignment=0.5.
    #[test]
    fn place_center_allocate_horizontal() {
        let pc = PlaceCenter { dimension: 0, alignment: 0.5 };
        let mut alloc = Allocation::new(
            Allotment::new(10.0, 100.0, 0.0),
            Allotment::new(20.0, 200.0, 0.0),
        );
        // req.x.alignment 가 0.5 라 가정.
        let mut req = Requisition::ZERO;
        req.x.alignment = 0.5;
        pc.allocate(&mut alloc, &req);
        assert_eq!(alloc.x.origin, 10.0 + 100.0 * (0.5 - 0.0));  // = 60.0
        assert_eq!(alloc.x.alignment, 0.5);
        // y 축 unchanged.
        assert_eq!(alloc.y.origin, 20.0);
        assert_eq!(alloc.y.alignment, 0.0);
    }

    #[test]
    fn place_center_allocate_vertical_preserves_x() {
        let pc = PlaceCenter { dimension: 1, alignment: 0.25 };
        let mut alloc = Allocation::new(
            Allotment::new(0.0, 50.0, 0.0),
            Allotment::new(100.0, 80.0, 0.0),
        );
        let mut req = Requisition::ZERO;
        req.y.alignment = 0.25;
        pc.allocate(&mut alloc, &req);
        assert_eq!(alloc.x.origin, 0.0);  // unchanged
        assert_eq!(alloc.x.alignment, 0.0);
        assert_eq!(alloc.y.origin, 100.0 + 80.0 * (0.25 - 0.0));  // = 120.0
        assert_eq!(alloc.y.alignment, 0.25);
    }

    /// alignment-point 보존 검증: begin/end 절대 좌표가 유지되어야 함.
    #[test]
    fn place_center_allocate_preserves_geometric_extents() {
        let pc = PlaceCenter { dimension: 0, alignment: 0.7 };
        let mut alloc = Allocation::new(
            Allotment::new(10.0, 40.0, 0.2),  // begin = 10 - 0.2*40 = 2; end = 10 + 0.8*40 = 42
            Allotment::new(0.0, 0.0, 0.0),
        );
        let begin_before = alloc.x.get_begin();
        let end_before = alloc.x.get_end();
        let mut req = Requisition::ZERO;
        req.x.alignment = 0.7;
        pc.allocate(&mut alloc, &req);
        // span 변하지 않음 (PlaceCenter 는 origin/alignment 만 변경).
        // alignment-point (origin) 의 절대 위치가 보존되어야 begin/end 일치.
        let begin_after = alloc.x.get_begin();
        let end_after = alloc.x.get_end();
        assert!((begin_before - begin_after).abs() < 1e-4,
                "begin: {} != {}", begin_before, begin_after);
        assert!((end_before - end_after).abs() < 1e-4,
                "end: {} != {}", end_before, end_after);
    }

    // ---- PlaceFix ----

    /// PlaceFix::Request 는 raw 가 4B `return` 인 no-op — req 미변경 확인.
    #[test]
    fn place_fix_request_is_noop() {
        let pf = PlaceFix { dimension: 0, fix_size: 42.0 };
        let mut req = Requisition::ZERO;
        req.x.natural = 99.0;
        req.x.alignment = 0.3;
        pf.request(&mut req);
        assert_eq!(req.x.natural, 99.0);   // unchanged
        assert_eq!(req.x.alignment, 0.3);  // unchanged
    }

    /// raw `FUN_003308a8`: alloc.{x|y}.span = self.fix_size.
    #[test]
    fn place_fix_allocate_horizontal_sets_x_span() {
        let pf = PlaceFix { dimension: 0, fix_size: 42.0 };
        let mut alloc = Allocation::new(
            Allotment::new(10.0, 100.0, 0.5),
            Allotment::new(20.0, 200.0, 0.6),
        );
        pf.allocate(&mut alloc, &Requisition::ZERO);
        assert_eq!(alloc.x.span, 42.0);
        // 다른 필드 unchanged.
        assert_eq!(alloc.x.origin, 10.0);
        assert_eq!(alloc.x.alignment, 0.5);
        assert_eq!(alloc.y.span, 200.0);
    }

    #[test]
    fn place_fix_allocate_vertical_sets_y_span() {
        let pf = PlaceFix { dimension: 1, fix_size: 80.0 };
        let mut alloc = Allocation::new(
            Allotment::new(0.0, 100.0, 0.0),
            Allotment::new(0.0, 200.0, 0.0),
        );
        pf.allocate(&mut alloc, &Requisition::ZERO);
        assert_eq!(alloc.x.span, 100.0);  // unchanged
        assert_eq!(alloc.y.span, 80.0);   // set
    }

    // ---- PlaceNatural ----

    #[test]
    fn place_natural_request_horizontal_sets_x_alignment_to_span() {
        let pn = PlaceNatural { direction: 0, span: 10.0 };
        let mut req = Requisition::INVALID;
        pn.request(&mut req);
        assert_eq!(req.x.alignment, 10.0);
    }

    #[test]
    fn place_natural_request_vertical_sets_y_alignment_to_span() {
        let pn = PlaceNatural { direction: 1, span: 7.5 };
        let mut req = Requisition::INVALID;
        pn.request(&mut req);
        assert_eq!(req.y.alignment, 7.5);
    }

    /// PlaceNatural::Allocate 는 raw 가 4B `return` 인 no-op — alloc 미변경.
    #[test]
    fn place_natural_allocate_is_noop() {
        let pn = PlaceNatural { direction: 0, span: 5.0 };
        let mut alloc = Allocation::new(
            Allotment::new(10.0, 20.0, 0.3),
            Allotment::new(40.0, 50.0, 0.6),
        );
        let before = alloc;
        pn.allocate(&mut alloc, &Requisition::ZERO);
        assert_eq!(alloc, before);
    }

    // ---- PlaceMargin ----

    /// PlaceMargin::with_uniform_margin(5.0) → 4 변 모두 natural=5, stretch/shrink=0,
    /// cache 는 INVALID.
    #[test]
    fn place_margin_ctor_uniform() {
        let pm = PlaceMargin::with_uniform_margin(5.0);
        assert_eq!(pm.left_natural, 5.0);
        assert_eq!(pm.top_natural, 5.0);
        assert_eq!(pm.right_natural, 5.0);
        assert_eq!(pm.bottom_natural, 5.0);
        assert_eq!(pm.left_stretch, 0.0);
        assert_eq!(pm.left_shrink, 0.0);
        let cache = pm.cache.get();
        assert_eq!(cache.x.natural, Requirement::INVALID_NATURAL);
        assert_eq!(cache.y.natural, Requirement::INVALID_NATURAL);
    }

    /// raw `PlaceMargin::Request`: x.natural valid → x += left+right; y.natural INVALID
    /// → y unchanged. cache 도 같은 값으로 갱신.
    #[test]
    fn place_margin_request_adds_x_margins_only_when_valid() {
        let pm = PlaceMargin::with_full(
            1.0, 0.5, 0.25,    // left  (n/s/sh)
            2.0, 1.0, 0.5,     // top
            3.0, 1.5, 0.75,    // right
            4.0, 2.0, 1.0,     // bottom
        );
        let mut req = Requisition {
            x: Requirement::new(100.0, 10.0, 5.0, 0.5),
            y: Requirement::new(Requirement::INVALID_NATURAL, 0.0, 0.0, 0.0),
            penalty: 7,
        };
        pm.request(&mut req);
        // x: natural += 1+3=4, stretch += 0.5+1.5=2, shrink += 0.25+0.75=1, alignment unchanged.
        assert_eq!(req.x.natural, 104.0);
        assert_eq!(req.x.stretch, 12.0);
        assert_eq!(req.x.shrink, 6.0);
        assert_eq!(req.x.alignment, 0.5);
        // y: INVALID → unchanged.
        assert_eq!(req.y.natural, Requirement::INVALID_NATURAL);
        assert_eq!(req.y.stretch, 0.0);
        assert_eq!(req.y.shrink, 0.0);
        // penalty preserved.
        assert_eq!(req.penalty, 7);
        // cache equals output.
        let cache = pm.cache.get();
        assert_eq!(cache.x.natural, 104.0);
        assert_eq!(cache.x.stretch, 12.0);
        assert_eq!(cache.y.natural, Requirement::INVALID_NATURAL);
    }

    #[test]
    fn place_margin_request_adds_both_axes_when_both_valid() {
        let pm = PlaceMargin::with_natural(2.0, 3.0, 4.0, 5.0);
        let mut req = Requisition {
            x: Requirement::new(50.0, 0.0, 0.0, 0.0),
            y: Requirement::new(60.0, 0.0, 0.0, 0.0),
            penalty: 0,
        };
        pm.request(&mut req);
        assert_eq!(req.x.natural, 50.0 + 2.0 + 4.0);  // = 56
        assert_eq!(req.y.natural, 60.0 + 3.0 + 5.0);  // = 68
    }

    /// PlaceMargin::Allocate — stretch path: diff > 0, stretch > 0.
    /// 좌우 margin 비례 분배. cache 미리 set 후 alloc 검증.
    #[test]
    fn place_margin_allocate_stretch_distributes_extra_space() {
        let pm = PlaceMargin::with_full(
            10.0, 1.0, 0.0,   // left  (natural=10, stretch=1)
            0.0,  0.0, 0.0,
            20.0, 3.0, 0.0,   // right (natural=20, stretch=3)
            0.0,  0.0, 0.0,
        );
        // request 호출하여 cache populate. child req: x.natural=100, stretch=4
        let mut req = Requisition {
            x: Requirement::new(100.0, 4.0, 0.0, 0.0),
            y: Requirement::new(Requirement::INVALID_NATURAL, 0.0, 0.0, 0.0),
            penalty: 0,
        };
        pm.request(&mut req);
        // 이제 cache.x.natural = 100 + 10 + 20 = 130, stretch = 4 + 1 + 3 = 8.
        let cache = pm.cache.get();
        assert_eq!(cache.x.natural, 130.0);
        assert_eq!(cache.x.stretch, 8.0);

        // alloc.x.span = 200 → diff_x = 200 - 130 = 70.
        // stretch path: lspan = 10 + (1/8)*70 = 10 + 8.75 = 18.75
        //               rspan = 20 + (3/8)*70 = 20 + 26.25 = 46.25
        // alloc.x.span -= (18.75 + 46.25) = 65 → 200 - 65 = 135.
        let mut alloc = Allocation::new(
            Allotment::new(0.0, 200.0, 0.0),
            Allotment::new(0.0, 0.0, 0.0),
        );
        pm.allocate(&mut alloc, &Requisition::ZERO);
        assert!((alloc.x.span - 135.0).abs() < 1e-4, "span: {}", alloc.x.span);
        // origin: 0 + -(0 * 46.25) + 18.75 * (1 - 0) = 18.75.
        assert!((alloc.x.origin - 18.75).abs() < 1e-4, "origin: {}", alloc.x.origin);
    }

    /// PlaceMargin::Allocate — shrink path: diff < 0, shrink > 0.
    #[test]
    fn place_margin_allocate_shrink_path() {
        let pm = PlaceMargin::with_full(
            10.0, 0.0, 2.0,   // left  (shrink=2)
            0.0,  0.0, 0.0,
            20.0, 0.0, 3.0,   // right (shrink=3)
            0.0,  0.0, 0.0,
        );
        let mut req = Requisition {
            x: Requirement::new(100.0, 0.0, 5.0, 0.0),
            y: Requirement::new(Requirement::INVALID_NATURAL, 0.0, 0.0, 0.0),
            penalty: 0,
        };
        pm.request(&mut req);
        let cache = pm.cache.get();
        assert_eq!(cache.x.shrink, 10.0);  // 5 + 2 + 3

        // alloc.x.span = 80 → diff_x = 80 - 130 = -50. cache.x.stretch = 0, so shrink path.
        // lr = left.shrink / cache.shrink = 2/10 = 0.2
        // lspan = 10 + 0.2 * (-50) = 10 - 10 = 0
        // rr = right.shrink / cache.shrink = 3/10 = 0.3
        // rspan = 20 + 0.3 * (-50) = 20 - 15 = 5
        // alloc.x.span -= (0 + 5) = -5 → 80 - 5 = 75
        let mut alloc = Allocation::new(
            Allotment::new(0.0, 80.0, 0.0),
            Allotment::new(0.0, 0.0, 0.0),
        );
        pm.allocate(&mut alloc, &Requisition::ZERO);
        assert!((alloc.x.span - 75.0).abs() < 1e-4, "span: {}", alloc.x.span);
        // origin: 0 + -(0 * 5) + 0 * (1 - 0) = 0.
        assert!(alloc.x.origin.abs() < 1e-4, "origin: {}", alloc.x.origin);
    }

    /// CalcSpan helper 의 stretch path 검증.
    #[test]
    fn place_margin_calc_span_stretch_path() {
        let req = Requirement::new(100.0, 4.0, 1.0, 0.0);
        // total=120, diff=20 > 0, stretch>0 → natural_part + (stretch/req.stretch)*diff
        let r = PlaceMargin::calc_span(120.0, &req, 10.0, 2.0, 0.0);
        assert!((r - (10.0 + (2.0 / 4.0) * 20.0)).abs() < 1e-4);  // = 20.0
    }

    #[test]
    fn place_margin_calc_span_shrink_path() {
        let req = Requirement::new(100.0, 0.0, 4.0, 0.0);
        // total=80, diff=-20 < 0, stretch=0 → shrink path.
        // ratio = shrink_factor / req.shrink = 2/4 = 0.5
        // return = natural_part + 0.5*(-20) = 10 - 10 = 0
        let r = PlaceMargin::calc_span(80.0, &req, 10.0, 0.0, 2.0);
        assert!((r - 0.0).abs() < 1e-4);
    }

    #[test]
    fn place_margin_calc_span_no_diff_returns_natural() {
        let req = Requirement::new(100.0, 4.0, 4.0, 0.0);
        let r = PlaceMargin::calc_span(100.0, &req, 10.0, 2.0, 2.0);
        // diff = 0 → not stretch (need 0.0 < diff), not shrink (need diff < 0.0).
        // ratio = 0, return = natural_part + 0 = 10.
        assert!((r - 10.0).abs() < 1e-4);
    }
}

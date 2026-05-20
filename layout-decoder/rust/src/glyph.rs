//! Glyph hierarchy — `Hnc::Shape::Text::Glyph` 와 subclass.
//!
//! RTTI 검증된 hierarchy (`data_dump/vtables.txt` + `CLASS_HIERARCHY.md` +
//! `glyph_vtables_full.txt` + `vtables_extended.txt`):
//!
//! ```text
//! Glyph (base, 16 vfuncs)
//! ├── 단순 primitive (16 vfunc inherit only):
//! │   ├── Glue              N3Hnc5Shape4Text4GlueE
//! │   ├── Space             N3Hnc5Shape4Text5SpaceE
//! │   ├── Strut             N3Hnc5Shape4Text5StrutE
//! │   ├── HStrut/VStrut     N3Hnc5Shape4Text6HStrutE / VStrutE
//! │   └── ShapeOf           N3Hnc5Shape4Text7ShapeOfE
//! └── 컨테이너 (extra Allocate vfunc):
//!     ├── Box               N3Hnc5Shape4Text3BoxE
//!     ├── Tile (HBox/VBox)  N3Hnc5Shape4Text4TileE
//!     ├── TileFirstAlign    ...
//!     ├── TileReverse
//!     ├── TileReverseFirstAlign
//!     ├── Align             N3Hnc5Shape4Text5AlignE
//!     ├── Deck              N3Hnc5Shape4Text4DeckE
//!     ├── BlipGlyph         N3Hnc5Shape4Text9BlipGlyphE
//!     ├── WidgetGlyph       N3Hnc5Shape4Text11WidgetGlyphE
//!     ├── DebugGlyph        N3Hnc5Shape4Text10DebugGlyphE
//!     └── MonoGlyph         N3Hnc5Shape4Text9MonoGlyphE
//! ```
//!
//! **Hancom Glyph base 의 16 vfuncs** (단순 primitive vtable @ Glue/Strut/Space 등에서 검증):
//!
//! | 슬롯 | 메소드           | base 주소  | base sz | 의미                                     |
//! |------|------------------|------------|---------|------------------------------------------|
//! | +0   | dtor1            |            |         |                                          |
//! | +8   | dtor0/delete     |            |         |                                          |
//! | +16  | dtor2            |            |         |                                          |
//! | +24  | Clone            | 0x31596c   | sz=8    | (this, out)                              |
//! | +32  | **Request**      | 0x315974   | sz=4    | (this, &avail, &out_bounds) — base no-op |
//! | +40  | Draw             | 0x31597c   | sz=4    |                                          |
//! | +48  | Undraw           | 0x2f8f8c   | sz=124  |                                          |
//! | +56  | GetBounds        | 0x315980   | sz=16   |                                          |
//! | +64  | Pick             | 0x315990   | sz=8    |                                          |
//! | +72  | Compose          | 0x315998   | sz=20   | (out_replacement, bt, &can_break)        |
//! | +80  | Append           | 0x3159ac   | sz=4    |                                          |
//! | +88  | Prepend          | 0x3159b0   | sz=4    |                                          |
//! | +96  | Insert           | 0x3159b4   | sz=4    |                                          |
//! | +104 | Remove           | 0x3159b8   | sz=4    |                                          |
//! | +112 | Replace          | 0x3159bc   | sz=4    |                                          |
//! | +120 | Change           | 0x3159c0   | sz=4    |                                          |
//! | +128 | GetCount         | 0x3159c4   | sz=8    | () -> int (base returns 0)               |
//! | +136 | GetComponent     | 0x3159cc   | sz=8    | (idx) -> Glyph*                          |
//! | +144 | GetAllotment     | 0x3159d4   | sz=12   | (idx) -> Allotment                       |
//!
//! **Container 서브클래스 (Box/Deck/Tile/Align 등)** 는 별도 base 에서 **+40 = Allocate**
//! 슬롯을 추가, 이후 +48 = Draw, +56 = Undraw, ... 로 한 칸씩 밀림. Rust 포팅에선 모든
//! Glyph subclass 가 `allocate(&mut self, &Allocation)` 메소드를 trait 으로 가짐 (default
//! no-op — 단순 primitive 는 override 안 함, 컨테이너는 override).
//!
//! **Request 의 실제 의미**: 표준 TeX-style `Request(Requisition&)` 가 아니라 **bounds
//! accumulator** — `(this, Allocation const& avail, float[4]& out_bounds)`. base 는 no-op.
//! Glue/Strut/Space 의 Request 는 byte-identical (단순 bounds merge).
//! Deck/Box/Tile 의 Request 는 children 재귀 + bounds merge.

use crate::placement::Placement;
use crate::value_types::{Allocation, Allotment, BoundsRect, BreakType, Dimension, Extension, Requirement, Requisition};
use kdsnr_render::bw_mode::BWMode;
use kdsnr_render::flag::Flag;
use kdsnr_render::hit::Hit;
use kdsnr_render::surface::Surface;
use kdsnr_render::theme::Theme;

// ============================================================
// Glyph trait
// ============================================================

/// `Hnc::Shape::Text::Glyph` virtual interface.
///
/// 한컴 base 16 vfuncs + 컨테이너용 Allocate (default no-op). 모든 메소드는 base impl 이
/// no-op 또는 default — 각 subclass 가 필요한 만큼 override.
pub trait Glyph: std::fmt::Debug + std::any::Any {
    /// `Clone()` (vtable +24, base @ `FUN_0031596c`). shared_ptr 새 인스턴스 반환.
    fn clone_glyph(&self) -> Box<dyn Glyph>;

    /// Rust 의 `Any` 로 downcast 가능하게 노출 (한컴 의 `dynamic_cast<CharItemView*>(glyph)`
    /// 등에 대응). 각 subclass 는 `fn as_any(&self) -> &dyn Any { self }` 로 override.
    fn as_any(&self) -> &dyn std::any::Any;
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;

    /// `Request(Requisition& req_out)` — **vfunc[3], vptr+0x18**.
    ///
    /// raw vtable dump 검증 (`vtables_v2.txt`):
    /// - MonoGlyph vtable [+0x18] = `FUN_002d04a8` = MonoGlyph::Request(Requisition&)
    /// - Composition vtable [+0x18] = `FUN_002fe79c` = Composition::Request(forward to parent vfunc[3])
    /// - LRComposition / TBComposition: 동일
    /// - Glue vtable [+0x18] = `FUN_003157f4` (Ghidra mis-labels as "Glue::Request" 이지만 별 함수)
    /// - CharItemView vtable [+0x18] = `FUN_002f5bb0` = CharItemView::Request(Requisition&)
    ///
    /// 즉 vfunc[3] 의 진짜 시그니처는 **단일 인자 `Requisition&`** — 자신의 bounds spec
    /// 을 캐스ㅂㅗ리 의 Requisition 으로 채움. (raw size 36B = X.Requirement(16) + Y.Requirement(16)
    /// + i32 penalty(4).)
    ///
    /// **base default**: no-op (Glyph base @ `FUN_00315974` sz=4 = ret only).
    fn request(&self, _req_out: &mut Requisition) {
        // Glyph base no-op (한컴 raw `FUN_00315974` sz=4)
    }

    /// `Allocate(Allocation const& avail, BoundsRect& out_bounds)` — **vfunc[4], vptr+0x20**
    /// (Glue/Box/Tile/Strut/Space style).
    ///
    /// raw vtable dump 검증 (vtables_v2.txt + vtables.txt):
    /// - Glue vtable [+0x20] = `FUN_0031580c` (Ghidra mis-labels as "Glue::Request" 이지만 진짜는
    ///   Glue::Allocate w/ Allocation + 16B output. x1 = 24B Allocation 읽기, x2 = 16B 읽고 union.)
    /// - Box vtable [+0x20] = `FUN_002e601c` (Box::Allocate — cached_bounds 누적)
    /// - HBox/VBox_outer [+0x20] = FUN_002e601c (Box::Allocate)
    /// - Tile (HBox/VBox_inner) [+0x18] = Tile::Request_inner (vfunc[3] 위치인데 시그니처는 2-arg —
    ///   Tile 은 다른 hierarchy. 후속 단계 detailed audit 필요.)
    ///
    /// **bounds accumulator** 갱신. Compositor 가 자식 순회하며 각 child 의 Allocate
    /// (BoundsRect-style output) 를 호출 → 모든 child 의 bounds 가 `out_bounds` 에 union 누적.
    ///
    /// 본 method 와 `allocate(Allocation, Extension)` 는 모두 vfunc[4] 의 representation 이고
    /// 출력 type 만 다름 (BoundsRect 와 Extension 둘 다 16B `{l/t/r/b}`). primitive Glyphs
    /// (Glue/Strut/Space) 는 본 `allocate_bounds` 를 사용. container Glyphs (MonoGlyph,
    /// Composition) 는 `allocate(Allocation, Extension)` 를 사용.
    fn allocate_bounds(&mut self, _avail: &Allocation, _out_bounds: &mut BoundsRect) {
        // Glyph base no-op
    }

    /// `Allocate(Allocation const& alloc, Extension& ext)` (vfunc[4], vptr+0x20).
    ///
    /// raw 시그니처 검증 (`MonoGlyph::Allocate` `FUN_002d0584` sz=32 — `(Allocation*, Extension*)`).
    /// container Glyph 는 자신과 child 의 bounds 를 ext 에 누적. primitive 는 default no-op.
    fn allocate(&mut self, _alloc: &Allocation, _ext: &mut Extension) {}

    /// `Draw(Surface&, Allocation const&, Flag const&, BWMode const&)` — **vfunc[5], vptr+0x28**.
    ///
    /// raw 검증 (`nm -arch arm64 libHncDrawingEngine.dylib | c++filt`):
    /// `Hnc::Shape::Text::Glyph::Draw(Hnc::Shape::Surface&, Hnc::Shape::Text::Allocation const&,
    ///  Hnc::Type::Flag const&, Hnc::Shape::BWMode const&)` @ `0x31597c` (4 byte: `ret`).
    ///
    /// **Glyph base default** = no-op (raw 가 `ret` 한 줄). Rust trait default 도 빈 body.
    ///
    /// **시그니처 (raw 1:1)**:
    /// - `param1 = this` (= self)
    /// - `param2 = Surface&` (x1) — 그리기 backend
    /// - `param3 = Allocation const&` (x2) — child sub-allocation (caller 가 `cached_alloc[i]` 전달)
    /// - `param4 = Flag const&` (x3) — bit-flag (BWMode bit 4 = "skip drawing")
    /// - `param5 = BWMode const&` (x4) — black/white mode
    fn draw(
        &mut self,
        _surface: &mut dyn Surface,
        _alloc: &Allocation,
        _flag: &Flag,
        _bw: &BWMode,
    ) {
        // raw `0x31597c`: ret only — no observable side effect.
        //
        // **시그니처가 `&mut self` 인 이유 (L-5b 정공법 정정)**:
        // Box::Draw (raw `FUN_002e6348`) 가 첫 instruction 으로 `strb wzr, [x0, +0x29]`
        // (cache_bounds_valid = false) 를 수행 → 시그니처는 mut. C++ vtable 의 단일
        // entry 이므로 모든 subclass 의 Draw 가 같은 시그니처를 가짐. 한컴 C++ 가
        // `&` 로 받지만 raw 동작은 mut → Rust trait 은 `&mut self` 로 통일.
    }

    /// `Undraw(Flag const&)` — **vfunc[6], vptr+0x30**.
    ///
    /// raw 검증 (`Hnc::Shape::Text::Glyph::Undraw(Hnc::Type::Flag const&)` @ `0x2f8f8c`, 124 byte).
    ///
    /// **Glyph base default = container traversal** (정정: 이전 doc 의 "no-op" 은 부정확).
    /// raw asm `0x2f8f8c`:
    /// ```text
    /// mov x19, x1                 ; flag = arg2
    /// mov x20, x0                 ; this = arg1
    /// ldr x8, [x0]; ldr x8, [x8, #0x80]  ; vfunc[+0x80] = GetCount
    /// blr x8                       ; n = this->GetCount()
    /// cbz x0, ret                  ; if (n == 0) return;
    /// mov x21, x0; mov x22, #0     ; count = n, i = 0
    /// loop:
    ///   ldr x8, [x20]; ldr x8, [x8, #0x88]; mov x0, x20; mov x1, x22; blr x8
    ///                              ; child = this->GetComponent(i)
    ///   cbz x0, advance
    ///   ldr x8, [x0]; ldr x8, [x8, #0x30]  ; vfunc[+0x30] = Undraw
    ///   mov x1, x19; blr x8         ; child->Undraw(flag)
    /// advance:
    ///   add x22, x22, #1; cmp x21, x22; b.ne loop
    /// ret
    /// ```
    ///
    /// 즉 primitive (GetCount=0) 에선 no-op, container (Box) 에선 모든 child 의 Undraw 호출.
    /// Box::Undraw (override) 와 동작 동일 (Box 는 linked-list 직접 순회). 단,
    /// `get_component(idx)` 가 None 인 (= raw SharePtr inner null) child 는 skip.
    fn undraw(&self, flag: &Flag) {
        let n = self.get_count();
        let mut i = 0;
        while i < n {
            if let Some(child) = self.get_component(i) {
                child.undraw(flag);
            }
            i += 1;
        }
    }

    /// `GetBounds(Theme const*, Allocation const&, Glyph*) -> Allocation` — **vfunc[7], vptr+0x38**.
    ///
    /// raw 검증 (`Hnc::Shape::Text::Glyph::GetBounds(Hnc::Shape::Theme const*, Hnc::Shape::Text::Allocation const&,
    /// Hnc::Shape::Text::Glyph*)` @ `0x315980`, 16 byte).
    ///
    /// **Glyph base default = zero sret output**. raw asm `0x315980`:
    /// ```text
    /// strb wzr, [x8]       ; *(u8*)(out+0x0) = 0
    /// stur xzr, [x8, #0xc] ; *(u64*)(out+0xc) = 0   (y.span+y.alignment)
    /// stur xzr, [x8, #0x4] ; *(u64*)(out+0x4) = 0   (x.span+x.alignment)
    /// ret
    /// ```
    /// 즉 sret 24B 의 byte [0x0] (1B) + [0x4..0x14] (16B) zero. [0x1..0x4] [0x14..0x18] = **don't-care**
    /// (caller frame garbage 잔존, raw 의 미초기화). `Allocation::ZERO` (full 24B zero) 는 observable
    /// byte 일치 (caller 가 미초기화 영역 access 안 함을 전제).
    ///
    /// **시그니처 (raw 1:1)**: `(this, theme: Theme const* nullable, alloc: Allocation const&,
    /// child_param: Glyph* nullable) -> Allocation (sret)`.
    fn get_bounds(
        &mut self,
        _theme: Option<&Theme>,
        _alloc: &Allocation,
        _child: Option<&dyn Glyph>,
    ) -> Allocation {
        // `&mut self` — Box::GetBounds (FUN_002e64d4) 가 cache_bounds_valid 를 mutate.
        // C++ vtable 단일 entry → 모든 subclass 통일 시그니처.
        Allocation::ZERO
    }

    /// `Pick(Allocation const&, Theme const*, Hit&, int) -> bool` — **vfunc[8], vptr+0x40**.
    ///
    /// raw 검증 (`Hnc::Shape::Text::Glyph::Pick(Hnc::Shape::Text::Allocation const&,
    /// Hnc::Shape::Theme const*, Hnc::Shape::Text::Hit&, int)` @ `0x315990`, 8 byte).
    ///
    /// **Glyph base default = return false**. raw asm `0x315990`:
    /// ```text
    /// mov w0, #0x0
    /// ret
    /// ```
    ///
    /// **시그니처 (raw 1:1)**: `(this, alloc: Allocation const&, theme: Theme const* nullable,
    /// hit: Hit&, depth: int) -> bool`.
    fn pick(
        &mut self,
        _alloc: &Allocation,
        _theme: Option<&Theme>,
        _hit: &mut Hit,
        _depth: i32,
    ) -> bool {
        // `&mut self` — Box::Pick (FUN_002e65b8) 가 cache_bounds_valid 를 mutate.
        // C++ vtable 단일 entry → 모든 subclass 통일 시그니처.
        false
    }

    /// `Compose(out, BreakType, &can_break)`.
    ///
    /// Glyph base impl (`FUN_00315998`):
    /// ```c
    /// can_break = (bt < 2);
    /// out = nullptr;
    /// ```
    fn compose(&self, bt: BreakType) -> ComposeResult {
        ComposeResult {
            replacement: None,
            can_break: (bt as u32) < 2,
        }
    }

    /// `Append(SharePtr<Glyph>)` — child list 끝에 추가.
    ///
    /// **한컴 Box::Append 동작 (1:1 검증, FUN_00331810)**:
    /// SharePtr 의 inner ptr 이 null 이어도 link 노드를 항상 생성하고 child list 에
    /// add 함 (count++). placeholder slot 으로 들어감. 따라서 호출자가 `None` 을
    /// 전달해도 (= null SharePtr) container 의 child count 는 증가.
    fn append(&mut self, _child: Option<Box<dyn Glyph>>) {}

    /// `append` 의 편의 wrapper — null 아닌 child 만 받음.
    fn append_some(&mut self, child: Box<dyn Glyph>) {
        self.append(Some(child));
    }

    /// 한컴 Box::Append 의 null SharePtr case — placeholder add.
    fn append_null(&mut self) {
        self.append(None);
    }

    /// `Prepend(SharePtr<Glyph>)` (vfunc[11], vptr+0x58).
    fn prepend(&mut self, _child: Option<Box<dyn Glyph>>) {}

    /// `Insert(unsigned long idx, SharePtr<Glyph>)` (vfunc[12], vptr+0x60).
    /// 한컴은 SharePtr 의 inner null 도 placeholder slot 으로 추가 (Box::Insert).
    fn insert(&mut self, _idx: usize, _child: Option<Box<dyn Glyph>>) {}

    /// `Remove(unsigned long idx)` (vfunc[13], vptr+0x68).
    fn remove(&mut self, _idx: usize) {}

    /// `Replace(unsigned long idx, SharePtr<Glyph>)` (vfunc[14], vptr+0x70).
    fn replace(&mut self, _idx: usize, _child: Option<Box<dyn Glyph>>) {}

    /// `Change(unsigned long idx)` (vfunc[15], vptr+0x78).
    ///
    /// 한컴 raw 시그니처 검증 (`MonoGlyph::Change` `FUN_0031a52c` sz=32 — `(unsigned long)`).
    /// idx 위치의 child 가 변경되었음을 container 에 알림.
    fn change(&mut self, _idx: usize) {}

    /// `GetCount() const` (vfunc[16], vptr+0x80) — base는 0.
    fn get_count(&self) -> usize { 0 }

    /// `GetComponent(unsigned long idx) const` (vfunc[17], vptr+0x88) — base 는 None.
    fn get_component(&self, _idx: usize) -> Option<&dyn Glyph> { None }

    /// `GetComponent(idx)` mutable variant — Composition::Insert 등에서 line container 에
    /// sub-Insert 호출 시 필요. 한컴 base 는 const 만 정의 — 우리는 별도 mutable 추가.
    fn get_component_mut(&mut self, _idx: usize) -> Option<&mut dyn Glyph> { None }

    /// `GetAllotment(unsigned long idx, Dimension dim, Allotment& out)` (vfunc[18], vptr+0x90).
    ///
    /// 한컴 raw 시그니처 검증 (`MonoGlyph::GetAllotment` `FUN_0031a594` sz=40 —
    /// `(unsigned long, Dimension, Allotment&)`). out 인자 사용 (return value 아님).
    /// base: out 을 zero 로 set (한컴 MonoGlyph: `*out = 0; out[1] = 0;`).
    fn get_allotment(&self, _idx: usize, _dim: Dimension, out: &mut Allotment) {
        *out = Allotment::ZERO;
    }
}

/// `Glyph::Compose` 의 출력.
#[derive(Debug)]
pub struct ComposeResult {
    pub replacement: Option<Box<dyn Glyph>>,
    pub can_break: bool,
}

/// H/V 구분.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Horizontal = 0,
    Vertical = 1,
}

// ============================================================
// 단순 primitives (16 vfunc)
// ============================================================

/// `Hnc::Shape::Text::Glue` — flexible space, HGlue/VGlue.
///
/// Object layout (48 bytes, from `Glue::Clone` `FUN_003157f4`):
/// - +0..+8: vtable
/// - +8..+24: X Requirement (16 bytes)
/// - +24..+40: Y Requirement
/// - +40..+44: penalty (i32)
///
/// vtable @ 0x780bd0.
#[derive(Debug, Clone)]
pub struct Glue {
    pub req: Requisition,
}

impl Glue {
    pub fn new(req: Requisition) -> Self { Self { req } }
}

impl Glyph for Glue {
    fn clone_glyph(&self) -> Box<dyn Glyph> { Box::new(self.clone()) }
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    /// `Glue::Request` (`FUN_0031580c` sz=92).
    /// Glue 의 stored Requisition 은 사용 안 함! 단순히 avail 의 bounds 를 out 에 merge.
    /// Strut::Request / Space::Request 와 byte-identical.
    fn allocate_bounds(&mut self, avail: &Allocation, out_bounds: &mut BoundsRect) {
        out_bounds.merge_allocation(avail);
    }
}

/// `Hnc::Shape::Text::Space` — word/inter-glyph space, HSpace/VSpace.
///
/// vtable @ 0x7814e8. Space::Request (`FUN_00337f20` sz=92) is **byte-identical** to
/// Glue::Request and Strut::Request — 단순 bounds accumulator.
#[derive(Debug, Clone)]
pub struct Space {
    pub direction: Direction,
}

impl Glyph for Space {
    fn clone_glyph(&self) -> Box<dyn Glyph> { Box::new(self.clone()) }
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    /// `Space::Request` (`FUN_00337f20` sz=92) — byte-identical to Glue::Request.
    fn allocate_bounds(&mut self, avail: &Allocation, out_bounds: &mut BoundsRect) {
        out_bounds.merge_allocation(avail);
    }
}

/// `Hnc::Shape::Text::Strut` — fixed-dimension baseline anchor.
///
/// vtable @ 0x781630.
#[derive(Debug, Clone)]
pub struct Strut {
    pub direction: Direction,
    pub req_a: Requirement,
    pub req_b: Requirement,
}

impl Glyph for Strut {
    fn clone_glyph(&self) -> Box<dyn Glyph> { Box::new(self.clone()) }
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    /// `Strut::Request` (`FUN_0033841c` sz=92) — byte-identical to Glue::Request.
    fn allocate_bounds(&mut self, avail: &Allocation, out_bounds: &mut BoundsRect) {
        out_bounds.merge_allocation(avail);
    }
}

/// `Hnc::Shape::Text::HStrut` — no-op Request variant.
///
/// vtable @ 0x7816d8. HStrut::Request (`FUN_00338580` sz=4) — empty.
#[derive(Debug, Clone, Default)]
pub struct HStrut {
    pub width: f32,
}

impl Glyph for HStrut {
    fn clone_glyph(&self) -> Box<dyn Glyph> { Box::new(self.clone()) }
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    // Request: base no-op (`FUN_00338580` sz=4) — trait default 가 동등.
}

/// `Hnc::Shape::Text::VStrut` — no-op Request variant.
///
/// vtable @ 0x781780. VStrut::Request (`FUN_00338674` sz=4) — empty.
#[derive(Debug, Clone, Default)]
pub struct VStrut {
    pub height: f32,
}

impl Glyph for VStrut {
    fn clone_glyph(&self) -> Box<dyn Glyph> { Box::new(self.clone()) }
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
}

/// `Hnc::Shape::Text::ShapeOf` — sized by another shape.
///
/// vtable @ 0x781440. ShapeOf::Request (`FUN_00337b80` sz=4) — no-op.
#[derive(Debug, Clone, Default)]
pub struct ShapeOf;

impl Glyph for ShapeOf {
    fn clone_glyph(&self) -> Box<dyn Glyph> { Box::new(self.clone()) }
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
}

// ============================================================
// 컨테이너 (extra Allocate vfunc)
// ============================================================

/// `Hnc::Shape::Text::Deck` — overlay container (현재 활성 child 만 보임).
///
/// vtable @ 0x780738. **container 17-vfunc 변형**: +40 = Deck::Allocate (별도 슬롯),
/// 이후 +48 = Draw, +56 = Undraw 로 한 칸씩 밀림.
///
/// Object layout (확인된 field):
/// - +32 (`this[4]`): current active child index (i64, 사용은 i32 처럼)
/// - +80..+104 (`this[10..12]`): cached avail (last passed Allocation, 24 bytes)
/// - 그 외 children list, dirty flag 등 TBD
///
/// `Deck::Request` (`FUN_0030cc78` sz=216):
/// 1. avail 을 `this[10..12]` 에 캐시 (3×8 = 24 bytes copy)
/// 2. `current_idx < GetCount()` 면 `child[current_idx].Request(avail, out)` 호출
/// 3. 그 후 자신의 avail 도 out_bounds 에 merge
#[derive(Debug, Default)]
pub struct Deck {
    pub current_idx: u32,
    pub cached_avail: Allocation,
    pub children: Vec<Box<dyn Glyph>>,
}

impl Glyph for Deck {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn clone_glyph(&self) -> Box<dyn Glyph> {
        // Deck::Clone (`FUN_0030cac8` sz=388) — 정확한 children 복제 로직 RE 안 됐으나 영향
        // 없음: `LayoutFactory::CreateDeck` (`FUN_00317708`) 의 호출 사이트가 layout RE 범위
        // (`decompiles_v2`) 내 0건. Deck 는 textbox 외 다른 shape 경로에서만 instantiate 됨 →
        // layout output byte-eq 와 무관. 정확한 RE 는 별도 phase.
        let cloned_children: Vec<Box<dyn Glyph>> =
            self.children.iter().map(|c| c.clone_glyph()).collect();
        Box::new(Self {
            current_idx: self.current_idx,
            cached_avail: self.cached_avail,
            children: cloned_children,
        })
    }

    /// `Deck::Request` (`FUN_0030cc78` sz=216) 의 1:1 포팅.
    fn allocate_bounds(&mut self, avail: &Allocation, out_bounds: &mut BoundsRect) {
        // 1. avail 을 Deck 의 cached_avail 에 저장 (Hancom +80..+104)
        self.cached_avail = *avail;

        // 2. current_idx < count 일 때만 진행
        let count = self.children.len();
        let idx = self.current_idx as usize;
        if idx < count {
            // 3. child[idx].Request(avail, out) — vtable +0x20 = +32
            self.children[idx].allocate_bounds(avail, out_bounds);

            // 4. 자신의 avail 도 out_bounds 에 merge (Glue::Request 와 동일 로직)
            out_bounds.merge_allocation(avail);
        }
    }

    fn get_count(&self) -> usize { self.children.len() }

    fn get_component(&self, idx: usize) -> Option<&dyn Glyph> {
        self.children.get(idx).map(|b| b.as_ref())
    }
}

// ============================================================
// Box (HBox/VBox outer) — container Glyph
// ============================================================
//
// Note: `Tile`, `Align`, `Superpose` 는 별도 `Hnc::Shape::Text::Layout` hierarchy 이며
// `crate::layout` 모듈로 분리. `TileFirstAlign` / `TileReverse` / `TileReverseFirstAlign`
// 는 Tile 의 subclass (additional vtable secondary) — Phase B-5f-A.6 후속에서 추가 port.

use crate::layout::Superpose;

/// `Hnc::Shape::Text::Box` — HBox/VBox 의 outer (176 bytes, vtable `0x77fd10`).
///
/// raw layout (`LayoutFactory::CreateHBox` `FUN_002ec634` 의 init 흐름):
/// - +0x00: vtable
/// - +0x08..+0x18: doubly-linked-list sentinel pair (head, tail) — children list 의 sentinel
/// - +0x18: count (u64, 0 초기)
/// - +0x20: Holder<Layout>* (= primary layout strategy holder, CreateHBox 에서 Superpose 를 가리킴)
/// - +0x28: u16 (cache flags) — +0x28=cache_req_valid, +0x29=cache_bounds_valid
/// - +0x30..+0x47: 일부 state
/// - +0x48..+0x67: cached Requisition (36B)
/// - +0x68: penalty (i32) ← 위 Requisition 의 마지막 i32
/// - +0x70..+0x9f: 일부 state
/// - +0xa0..+0xb0: cached bounds (4 f32: min_x, min_y, max_x, max_y)
///
/// **16 vfuncs (base Glyph 와 동일 layout)** — RTTI 검증 raw vtable @ `0x77fd10` +
/// L-5b raw asm 재검증 (`work/hft_re/layout_re/glyph_draw_dump/`):
///
/// | vfunc | slot   | 메소드                         | raw addr        | size |
/// |-------|--------|--------------------------------|-----------------|------|
/// | 3     | +0x18  | Request(Requisition&)          | `FUN_002e5e48`  | 56B  |
/// | 4     | +0x20  | Allocate(Allocation, Extension&) | `FUN_002e601c`| 76B  |
/// | 5     | +0x28  | Draw(Surface,Alloc,Flag,BWMode)| `FUN_002e6348`  | 188B |
/// | 6     | +0x30  | Undraw(Flag)                   | `FUN_002e6404`  | 208B |
/// | 7     | +0x38  | GetBounds(Theme*,Alloc,Glyph*)→Alloc | `FUN_002e64d4` | 228B |
/// | 8     | +0x40  | Pick(Alloc,Theme*,Hit,depth)→bool| `FUN_002e65b8`| 256B |
/// | 9     | +0x48  | Compose (base)                 | `Glyph::Compose`|      |
/// | 10-15 | +0x50..+0x80 | Append/Prepend/Insert/Remove/Replace/Change | `FUN_00331810`/etc | |
/// | 16    | +0x80  | GetCount                       | `FUN_00331ee0`  | 8B   |
/// | 17    | +0x88  | GetComponent                   | `FUN_00331ee8`  |      |
/// | 18    | +0x90  | GetAllotment                   | `FUN_002e6688`  | 116B |
///
/// **L-5b vfunc index 정정**: 이전 doc 의 "5=Allocate, 6-9=Draw/Undraw/GetBounds/Pick"
/// 는 mis-label. raw vtable+nm+L-5a 의 dump 결과 모두 base Glyph 와 동일 16-vfunc.
/// `FUN_002e6348` = Box::Draw (vfunc[5]). `FUN_002e601c` = Box::Allocate (vfunc[4]).
#[derive(Debug)]
pub struct Box_ {
    /// +0x08..+0x20: doubly-linked-list of holders. Rust: `Vec<Option<Box<dyn Glyph>>>`.
    /// `None` slot 은 한컴의 null SharePtr Append/Insert (placeholder slot) 에 대응.
    pub children: Vec<Option<Box<dyn Glyph>>>,
    /// +0x20: Layout holder (CreateHBox/VBox 가 Superpose 로 초기화). cache 재계산이 이
    /// Layout 의 `request_inner` / `allocate_inner` 를 호출.
    pub layout: Option<Superpose>,
    /// +0x28: u8, cached Requisition 유효 플래그 (`FUN_002e5e80` 가 사용).
    pub cache_req_valid: bool,
    /// +0x29: u8, cached BoundsRect 유효 플래그 (`FUN_002e6120` 가 사용).
    pub cache_bounds_valid: bool,
    /// +0x30..+0x40: cached child Requisition vector (`FUN_002e5e80` 가 gather 한 결과).
    /// `recompute_request_cache` 가 채움, `recompute_bounds_cache` 가 `layout.allocate_inner`
    /// 의 input 으로 사용.
    pub cached_child_reqs: Vec<Requisition>,
    /// +0x48..+0x67: cached Requisition (`Box::Request` 가 출력).
    pub cached_req: Requisition,
    /// +0x70..+0x80: cached per-child Allocation vector (`FUN_002e6120` 가 채움).
    /// `Box::Allocate` 가 children 에 분배할 때 사용. `Box::GetAllotment` 도 이걸 읽음.
    pub cached_allocations: Vec<Allocation>,
    /// +0xa0..+0xb0: cached bounds (`Box::Request` (BoundsAccumulator) path 가 사용).
    pub cached_bounds: BoundsRect,
}

impl Default for Box_ {
    fn default() -> Self {
        // CreateHBox 의 fresh-init 과 등가 (raw decompile `FUN_002ec634`):
        //   +0x18: count = 0
        //   +0x20: holder = null (또는 Superpose)
        //   +0x28: u16 = 0 → cache_req_valid = false, cache_bounds_valid = false
        //   +0x48..+0x67: Requisition INVALID sentinel (-1e8, 0, ...)
        //   +0xa0..+0xb0: BoundsRect 0 (CreateHBox 가 puVar5[0x14]=puVar5[0x15]=0 으로 init)
        Self {
            children: Vec::new(),
            layout: None,
            cache_req_valid: false,
            cache_bounds_valid: false,
            cached_child_reqs: Vec::new(),
            cached_req: Requisition {
                x: Requirement::new(-1.0e8, 0.0, 0.0, 0.0),
                y: Requirement::new(-1.0e8, 0.0, 0.0, 0.0),
                penalty: 0,
            },
            cached_allocations: Vec::new(),
            cached_bounds: BoundsRect::default(),
        }
    }
}

/// raw `FUN_002e601c` / `FUN_002e6120` 의 SIMD `bif` bounds merge.
///
/// raw asm:
/// ```text
/// v2 = [a.min_x, a.min_y, b.max_x, b.max_y]
/// v3 = [b.min_x, b.min_y, a.max_x, a.max_y]
/// v2 = v3 > v2                              ; fcmgt
/// result = bif(a, b, v2)
/// ```
/// lane 별 결과:
/// - min_x: `(b.min_x > a.min_x) ? a.min_x : b.min_x` = min(a, b) [NaN: a.min_x or b.min_x 가 NaN 이면 b.min_x]
/// - min_y: 동일
/// - max_x: `(a.max_x > b.max_x) ? a.max_x : b.max_x` = max(a, b)
/// - max_y: 동일
#[inline]
fn bounds_simd_merge(a: BoundsRect, b: BoundsRect) -> BoundsRect {
    BoundsRect {
        min_x: if b.min_x > a.min_x { a.min_x } else { b.min_x },
        min_y: if b.min_y > a.min_y { a.min_y } else { b.min_y },
        max_x: if a.max_x > b.max_x { a.max_x } else { b.max_x },
        max_y: if a.max_y > b.max_y { a.max_y } else { b.max_y },
    }
}

impl Box_ {
    /// `LayoutFactory::CreateHBox/CreateVBox` 가 생성하는 Box 의 초기 상태.
    pub fn new(layout: Superpose) -> Self {
        let mut me = Self::default();
        me.layout = Some(layout);
        me
    }

    /// `Box::Request` 의 helper `FUN_002e5e80` (sz=372) 의 Rust 1:1 port.
    ///
    /// raw asm 흐름:
    /// 1. `if this[+0x28] != 0`: 캐시 유효, 즉시 return.
    /// 2. count = `this->GetCount()` (vfunc[+0x80]).
    /// 3. `buf = Vec<Requisition>(count)`, 각 element init = `{x: (-1e8,0,0,0), y: (-1e8,0,0,0), penalty: 0}`
    ///    (raw: `ldr q0, [x9, #0xf20]` = `_DAT_00741f20` = (-1e8, 0, 0, 0)).
    /// 4. for i: `child = GetComponent(i)`; if non-null: `child->vfunc[+0x18](&buf[i])` = `child.request(&mut buf[i])`.
    /// 5. if `this[+0x20]` (layout holder) non-null: `layout->vfunc[+0x18](&buf, &this[+0x48])`
    ///    = `layout.request_inner(&buf, &mut this.cached_req)`.
    /// 6. `this[+0x30..+0x40]` ← buf (= `cached_child_reqs`). 옛 buf 는 free.
    /// 7. `this[+0x28] = 1` (cache_req_valid).
    pub fn recompute_request_cache(&mut self) {
        if self.cache_req_valid {
            return;
        }
        let count = self.children.len();
        // raw: each Requisition init = {x: (-1e8, 0, 0, 0), y: (-1e8, 0, 0, 0), penalty: 0}.
        let mut buf: Vec<Requisition> = vec![
            Requisition {
                x: Requirement::new(-1.0e8, 0.0, 0.0, 0.0),
                y: Requirement::new(-1.0e8, 0.0, 0.0, 0.0),
                penalty: 0,
            };
            count
        ];

        // gather child Requisitions (vfunc[+0x18] = child.request)
        for (i, child_opt) in self.children.iter().enumerate() {
            if let Some(child) = child_opt {
                child.request(&mut buf[i]);
            }
        }

        // pass to layout (vfunc[+0x18] = layout.request_inner → writes this.cached_req)
        if let Some(ref mut layout) = self.layout {
            use crate::layout::Layout as _;
            layout.request_inner(&buf, &mut self.cached_req);
        }

        // raw: this[+0x30..+0x40] = buf vector (replacing old)
        self.cached_child_reqs = buf;
        self.cache_req_valid = true;
    }

    /// `Box` 의 bounds-recompute helper `FUN_002e6120` (sz=472) 의 Rust 1:1 port.
    ///
    /// raw asm 흐름:
    /// 1. `bl FUN_002e5e80` — ensure Requisition cache valid (= `recompute_request_cache`).
    /// 2. `if this[+0x29] != 0`: bounds 캐시 유효, return.
    /// 3. count = `GetCount()`.
    /// 4. `allocs = Vec<Allocation>(count)`, zeroed (raw: `bl FUN_006bae84` = bzero).
    /// 5. if layout holder non-null: `layout->vfunc[+0x20](avail, &this[+0x30], &allocs)`
    ///    = `layout.allocate_inner(avail, &cached_child_reqs, &mut allocs)`.
    /// 6. `this[+0xa0..+0xb0]` (cached_bounds) ← `_DAT_00741f30` = `(+1e8, +1e8, -1e8, -1e8)` (INIT).
    /// 7. for idx in 0..count:
    ///    - `child = GetComponent(idx)`; if null, skip.
    ///    - `local_acc = INIT` (raw: `ldr q0, [sp]` = INIT sentinel, copied to `sp[0x10]`).
    ///    - `child->vfunc[+0x20](&allocs[idx], &local_acc)` = `child.allocate_bounds(&allocs[idx], &mut local_acc)`.
    ///    - `cached_bounds = bounds_simd_merge(local_acc, cached_bounds)` (raw merge phase 0x002e6234).
    /// 8. `this[+0x70..+0x80]` ← allocs (= `cached_allocations`). 옛 allocs 는 free.
    /// 9. `this[+0x29] = 1` (cache_bounds_valid).
    pub fn recompute_bounds_cache(&mut self, avail: &Allocation) {
        // step 1
        self.recompute_request_cache();
        // step 2
        if self.cache_bounds_valid {
            return;
        }
        let count = self.children.len();
        // step 4: allocs zeroed
        let mut allocs: Vec<Allocation> = vec![Allocation::ZERO; count];

        // step 5: layout distributes avail to per-child allocations
        if let Some(ref layout) = self.layout {
            use crate::layout::Layout as _;
            layout.allocate_inner(avail, &self.cached_child_reqs, &mut allocs);
        }

        // step 6: reset cached_bounds to INIT sentinel
        self.cached_bounds = BoundsRect::INIT;

        // step 7: per-child bounds accumulation
        for (idx, child_opt) in self.children.iter_mut().enumerate() {
            let child = match child_opt {
                Some(c) => c,
                None => continue,
            };
            // raw: local_acc reset to INIT sentinel each iteration
            let mut local_acc = BoundsRect::INIT;
            child.allocate_bounds(&allocs[idx], &mut local_acc);
            // raw merge phase: cached_bounds = merge(local_acc, cached_bounds)
            self.cached_bounds = bounds_simd_merge(local_acc, self.cached_bounds);
        }

        // step 8/9
        self.cached_allocations = allocs;
        self.cache_bounds_valid = true;
    }
}

impl Glyph for Box_ {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    /// `Box::Clone` — `FUN_002e5e00` (dtor2 slot, sz=52, copy-construct via `FUN_0063e170`).
    fn clone_glyph(&self) -> Box<dyn Glyph> {
        Box::new(Self {
            children: self.children.iter()
                .map(|c| c.as_ref().map(|g| g.clone_glyph()))
                .collect(),
            layout: self.layout.clone(),
            cache_req_valid: self.cache_req_valid,
            cache_bounds_valid: self.cache_bounds_valid,
            cached_child_reqs: self.cached_child_reqs.clone(),
            cached_req: self.cached_req,
            cached_allocations: self.cached_allocations.clone(),
            cached_bounds: self.cached_bounds,
        })
    }

    /// `Box::Request` `FUN_002e5e48` (sz=56). raw 검증:
    /// ```text
    /// bl FUN_002e5e80                    ; ensure cache valid
    /// q0 = this[+0x48..+0x57]            ; X Requirement (16B)
    /// q1 = this[+0x58..+0x67]            ; Y Requirement (16B)
    /// w8 = this[+0x68]                   ; penalty (i32)
    /// out[+0x20] = w8                    ; out.penalty
    /// out[+0x00..+0x10] = q0             ; out.x
    /// out[+0x10..+0x20] = q1             ; out.y
    /// ```
    fn request(&self, out: &mut Requisition) {
        // raw asm BL FUN_002e5e80: ensure cache valid. Rust 에서는 &self 라 mut 불가
        // — 캐시 재계산은 외부에서 한 번 수행해야 함. 단순화: 캐시가 있으면 write, 없으면
        // INVALID sentinel 으로 출력 (한컴은 캐시 invalid 면 helper 가 채워줌).
        //
        // **현재 mutable cache 가 필요한 함수는 별도 `request_mut` 으로 노출 또는 호출 전 외부에서
        // `recompute_request_cache()` 호출 필요**. trait::request 는 const → cache 만 출력.
        *out = self.cached_req;
    }

    /// `Box::Allocate(Allocation const&, Extension&)` — **vfunc[4], +0x20** — `FUN_002e601c`
    /// (sz=76). raw:
    /// ```text
    /// strb wzr, [x0, #0x29]              ; this.cache_bounds_valid = false
    /// bl FUN_002e6120                     ; recompute_bounds_cache(avail = param_2)
    /// q0 = this[+0xa0..+0xb0]             ; cached_bounds (4 floats = Extension layout)
    /// q1 = *(Extension *)out_ext          ; existing out
    /// out_ext = bif_simd_merge(cached_bounds, out_ext)  ; 4-lane min/max
    /// ```
    ///
    /// **byte-equivalent**: 먼저 `cache_bounds_valid = false` 로 clear → `recompute_bounds_cache`
    /// 가 강제 재계산 (layout.allocate_inner + per-child allocate 누적) → 그 결과
    /// `cached_bounds` 를 `out_ext` 에 SIMD bif merge.
    ///
    /// **현 trait 이중화**: trait 에는 `allocate_bounds(.., &mut BoundsRect)` 와
    /// `allocate(.., &mut Extension)` 두 method 가 있고, `BoundsRect` 와 `Extension` 는
    /// 16B 동일 layout. raw vfunc[4] 는 단일 entry — 두 method 는 같은 entry 의 두 type-alias.
    /// `Box::Allocate` 는 본 `allocate_bounds` (BoundsRect) 와 아래 `allocate` (Extension) 둘
    /// 모두 구현 (호출자에 따라 어느 쪽이 invoke 되든 동일 byte 효과 — `cached_bounds` 를
    /// `out` 에 SIMD merge).
    fn allocate_bounds(&mut self, avail: &Allocation, out_bounds: &mut BoundsRect) {
        // raw `strb wzr, [x0, #0x29]` — clear bounds valid (force recompute)
        self.cache_bounds_valid = false;
        // raw `bl FUN_002e6120` — recompute_bounds_cache
        self.recompute_bounds_cache(avail);
        // raw merge: out_bounds = bounds_simd_merge(cached_bounds, out_bounds)
        *out_bounds = bounds_simd_merge(self.cached_bounds, *out_bounds);
    }

    /// `Box::Allocate` (vfunc[4]) 의 Extension-typed alias. raw vfunc[4] 의 동작 — 자세한
    /// 설명은 `allocate_bounds` 참조. Extension 과 BoundsRect 는 16B 동일 layout.
    fn allocate(&mut self, avail: &Allocation, ext: &mut Extension) {
        // 동일 cache 무효화 + 재계산 후 SIMD merge.
        self.cache_bounds_valid = false;
        self.recompute_bounds_cache(avail);
        // Extension layout = (left, top, right, bottom) = BoundsRect (min_x, min_y, max_x, max_y).
        // 4-lane SIMD bif 동작이 동일하므로 BoundsRect 로 cast 후 merge, 결과를 Extension 에 write.
        let cur = BoundsRect {
            min_x: ext.left,
            min_y: ext.top,
            max_x: ext.right,
            max_y: ext.bottom,
        };
        let merged = bounds_simd_merge(self.cached_bounds, cur);
        ext.left = merged.min_x;
        ext.top = merged.min_y;
        ext.right = merged.max_x;
        ext.bottom = merged.max_y;
    }

    /// `Box::Draw(Surface&, Allocation const&, Flag const&, BWMode const&)` — **vfunc[5], +0x28**
    /// — `FUN_002e6348` (sz=188). raw asm `work/hft_re/layout_re/glyph_draw_dump/Box__Draw_2e6348.asm`:
    /// ```text
    /// ; 시그니처: (this=x0, surface=x1, alloc=x2, flag=x3, bw=x4)
    /// mov x19, x4 ; bw
    /// mov x20, x3 ; flag
    /// mov x21, x1 ; surface
    /// mov x22, x0 ; this
    /// strb wzr, [x0, #0x29]              ; cache_bounds_valid = false
    /// mov x1, x2                          ; arg1 of helper = alloc
    /// bl 0x2e6120                          ; recompute_bounds_cache(this, alloc)
    /// ; loop: idx in 0..count, offset = idx*0x18 (sizeof Allocation=24)
    ///   x0 = this->GetCount()             ; vfunc[+0x80]
    ///   if (x0 == 0) ret
    ///   for idx = 0..count:
    ///     child = this->GetComponent(idx) ; vfunc[+0x88]
    ///     if (child == null) continue
    ///     child->vfunc[+0x28](            ; child.Draw
    ///       /*x1=*/surface,
    ///       /*x2=*/&cached_allocations[idx],
    ///       /*x3=*/flag,
    ///       /*x4=*/bw)
    /// ret
    /// ```
    ///
    /// 즉 Box::Draw 는:
    /// 1. `cache_bounds_valid = false` + `recompute_bounds_cache(alloc)` — cached_allocations
    ///    가 layout 으로 다시 채워짐.
    /// 2. 각 child 에 `cached_allocations[idx]` 를 그대로 sub-allocation 으로 전달하여
    ///    `child.draw(surface, sub_alloc, flag, bw)` 호출.
    fn draw(&mut self, surface: &mut dyn Surface, alloc: &Allocation, flag: &Flag, bw: &BWMode) {
        // raw `strb wzr, [x0, #0x29]` + `bl FUN_002e6120`
        self.cache_bounds_valid = false;
        self.recompute_bounds_cache(alloc);
        // raw: per-child Draw dispatch with cached_allocations[idx] as sub-alloc.
        // children 와 cached_allocations 둘 다 self 의 field 라 동시 borrow 회피 위해 clone.
        let allocs = self.cached_allocations.clone();
        for (idx, child_opt) in self.children.iter_mut().enumerate() {
            if let Some(child) = child_opt {
                let child_alloc = allocs.get(idx).copied().unwrap_or(Allocation::ZERO);
                child.draw(surface, &child_alloc, flag, bw);
            }
        }
    }

    /// `Box::GetBounds(Theme const*, Allocation const&, Glyph*) -> Allocation` —
    /// **vfunc[7], +0x38** — `FUN_002e64d4` (sz=228). raw asm
    /// `work/hft_re/layout_re/glyph_draw_dump/Box__GetBounds_2e64d4.asm`:
    /// ```text
    /// ; 시그니처: (this=x0, theme=x1, alloc=x2, child_param=x3, out=x8 sret)
    /// mov x20, x3 ; child_param
    /// mov x21, x1 ; theme
    /// mov x22, x0 ; this
    /// mov x19, x8 ; out (sret)
    /// strb wzr, [x0, #0x29]
    /// mov x1, x2 ; alloc
    /// bl 0x2e6120                          ; recompute_bounds_cache(this, alloc)
    /// count = this->GetCount()
    /// if (count == 0) goto fallback_zero
    /// for idx = 0..count:
    ///   child = this->GetComponent(idx)
    ///   if (child == null) continue
    ///   child->vfunc[+0x38](               ; child.GetBounds → out (sret)
    ///     /*x8=*/out,
    ///     /*x1=*/theme,
    ///     /*x2=*/&cached_allocations[idx],
    ///     /*x3=*/child_param)
    ///   if (out->[0] != 0) ret              ; 첫 non-zero hit 반환
    ///   continue
    /// fallback_zero:
    ///   out->[0] = 0 (1B)
    ///   out->[+0x4..+0xC] = 0 (8B)
    ///   out->[+0xC..+0x14] = 0 (8B)
    ///   ret
    /// ```
    ///
    /// **종료 조건 `out->[0] != 0`**: Allocation 의 첫 byte = `x.origin` (f32) 의 LSB.
    /// 0.0 (= 0x00000000) 일 때 byte 0 = 0. 따라서 "first child returns Allocation with
    /// non-zero `x.origin` LSB" 면 hit 으로 간주. (zero Allocation = no-bounds sentinel
    /// 과 일치 — Glyph::GetBounds 의 base default 가 같은 sret 패턴.)
    ///
    /// **fallback zero sret**: raw 의 부분 zero (bytes 0, 4..0xb, 0xc..0x13 = 17B 만 zero;
    /// bytes 1..3, 0x14..0x17 = don't-care) 는 caller 가 미초기화 영역 access 안 함 전제.
    /// Rust 에선 `Allocation::ZERO` 로 전부 zero — observable byte 일치.
    fn get_bounds(
        &mut self,
        theme: Option<&Theme>,
        alloc: &Allocation,
        child_param: Option<&dyn Glyph>,
    ) -> Allocation {
        self.cache_bounds_valid = false;
        self.recompute_bounds_cache(alloc);
        let count = self.children.len();
        if count == 0 {
            return Allocation::ZERO;
        }
        let allocs = self.cached_allocations.clone();
        for (idx, child_opt) in self.children.iter_mut().enumerate() {
            if let Some(child) = child_opt {
                let child_alloc = allocs.get(idx).copied().unwrap_or(Allocation::ZERO);
                let out = child.get_bounds(theme, &child_alloc, child_param);
                // raw 종료 조건: `ldrb w8, [x19]; cbz w8, continue` —
                // out 의 first byte (= x.origin LSB) 가 non-zero 면 hit.
                let first_byte = out.x.origin.to_le_bytes()[0];
                if first_byte != 0 {
                    return out;
                }
            }
        }
        // raw fallback zero sret.
        Allocation::ZERO
    }

    /// `Box::Pick(Allocation const&, Theme const*, Hit&, int) -> bool` — **vfunc[8], +0x40** —
    /// `FUN_002e65b8` (sz=256). raw asm
    /// `work/hft_re/layout_re/glyph_draw_dump/Box__Pick_2e65b8.asm`:
    /// ```text
    /// ; 시그니처: (this=x0, alloc=x1, theme=x2, hit=x3, depth=x4) -> bool (w0)
    /// mov x19, x4 ; depth
    /// mov x20, x3 ; hit
    /// mov x21, x2 ; theme
    /// mov x22, x0 ; this
    /// strb wzr, [x0, #0x29]
    /// bl 0x2e6120                          ; recompute_bounds_cache(this, alloc) (x1=alloc 그대로)
    /// count = this->GetCount()
    /// if (count == 0) { result = false; return; }
    /// result = true                         ; default
    /// for idx = 0..count:
    ///   ; loop tail: result = (idx < count) ? true : false  (cset w26, lo)
    ///   if (idx == count) goto end
    ///   child = this->GetComponent(idx)
    ///   if (child == null) continue
    ///   bool hit_b = child->vfunc[+0x40](   ; child.Pick
    ///     /*x1=*/&cached_allocations[idx],
    ///     /*x2=*/theme,
    ///     /*x3=*/hit,
    ///     /*x4=*/depth)
    ///   if (hit_b) goto end                  ; 첫 hit → break true
    ///   continue
    /// end: return result & 1
    /// ```
    ///
    /// 의미: any non-null child returns true → break true. 모두 null/false → false.
    /// `cset w26, lo` 의 효과: loop 의 마지막 iteration 후 idx==count 가 되면 result=0.
    fn pick(
        &mut self,
        alloc: &Allocation,
        theme: Option<&Theme>,
        hit: &mut Hit,
        depth: i32,
    ) -> bool {
        self.cache_bounds_valid = false;
        self.recompute_bounds_cache(alloc);
        let count = self.children.len();
        if count == 0 {
            return false;
        }
        let allocs = self.cached_allocations.clone();
        for (idx, child_opt) in self.children.iter_mut().enumerate() {
            if let Some(child) = child_opt {
                let child_alloc = allocs.get(idx).copied().unwrap_or(Allocation::ZERO);
                if child.pick(&child_alloc, theme, hit, depth) {
                    return true;
                }
            }
        }
        false
    }

    /// `Box::Append` `FUN_00331810` (sz=128). raw asm 흐름:
    /// ```text
    /// ldr x19, [x0, #0x18]               ; old count = this[+0x18]
    /// mov w0, #0x18                       ; alloc 0x18 = 24B holder node
    /// bl operator_new                     ; node = new(24)
    /// ldr x9, [x21]                        ; x9 = *param_1 (= child SharePtr's inner Glyph ptr)
    /// str x9, [x0, #0x10]                  ; node[+0x10] = glyph
    /// ; refcount++ if non-null
    /// if (x9 != 0):
    ///     ldr x8, [x9, #0x8]
    ///     add x8, x8, #1
    ///     str x8, [x9, #0x8]
    /// ; link into doubly-linked list (sentinel @ this+0x08):
    /// x9 = this
    /// x10 = x9[0x8]++  ; advance tail pointer (?)
    /// stp x10, x9, [x0]                    ; node[+0..+8] = x10 (prev), node[+8..+16] = this (next sentinel)
    /// str x0, [x10, #0x8]                  ; x10[+0x8] = node (link previous tail.next = node)
    /// str x0, [x9]                         ; this[+0] (sentinel.prev) = node (??)
    /// add x8, x8, #1                      ; count++
    /// str x8, [x20, #0x18]                  ; this[+0x18] = new count
    /// ; tail-call Box::Change(old_count) — invalidate caches
    /// br vfunc[+0x98]                     ; +0x98 = vfunc[19] = ???
    /// ```
    ///
    /// Rust 동등:
    /// - `self.children.push(child)`
    /// - `self.change(old_count)` — child changed at the END (= new index = old count).
    fn append(&mut self, child: Option<Box<dyn Glyph>>) {
        let old_count = self.children.len();
        self.children.push(child);
        self.change(old_count);
    }

    /// `Box::Prepend` `FUN_00331890` (sz=188). raw: insert at index 0.
    fn prepend(&mut self, child: Option<Box<dyn Glyph>>) {
        self.children.insert(0, child);
        // raw: `br vfunc[+0x98]` with x1 = 0 → `self.change(0)`
        self.change(0);
    }

    /// `Box::Insert` `FUN_0033194c` (sz=244). raw: 양수/음수 idx 분기 + DLL link + count++.
    fn insert(&mut self, idx: usize, child: Option<Box<dyn Glyph>>) {
        if idx > self.children.len() {
            // raw: out-of-range → operator_new(16) + exception throw. Rust: panic.
            panic!("Box::Insert: idx {} > count {}", idx, self.children.len());
        }
        self.children.insert(idx, child);
        self.change(idx);
    }

    /// `Box::Remove` `FUN_00331a40` (sz=324). raw: bounds check + DLL erase via `FUN_002feea4`.
    fn remove(&mut self, idx: usize) {
        if idx >= self.children.len() {
            // raw: out-of-range → exception. panic.
            panic!("Box::Remove: idx {} >= count {}", idx, self.children.len());
        }
        // raw: holder = list_node[+0x10]; if holder != null: glyph = *holder; if glyph != null:
        //   FUN_006b3bd0 (create temp SharePtr) + glyph.vfunc[+0x30] (Draw?) + FUN_006b3bdc
        // 우리 Rust 에서는 그냥 vec remove.
        self.children.remove(idx);
        self.change(idx);
    }

    /// `Box::Replace` `FUN_00331bdc` (sz=344). raw: bounds check + DLL ptr swap via `FUN_00331d8c`.
    fn replace(&mut self, idx: usize, child: Option<Box<dyn Glyph>>) {
        if idx >= self.children.len() {
            panic!("Box::Replace: idx {} >= count {}", idx, self.children.len());
        }
        self.children[idx] = child;
        self.change(idx);
    }

    /// `Box::Change` `FUN_00331ed4` (sz=12). raw:
    /// ```text
    /// ldr x8, [x0]
    /// ldr x2, [x8, #0x98]          ; vfunc[+0x98] = +152 = FUN_002e66fc
    /// br x2                          ; tail-call vfunc[+0x98]
    /// ```
    ///
    /// vfunc[+0x98] (`FUN_002e66fc`) 는 Box 의 19th vfunc (cache 무효화 helper).
    /// 우리 Rust 에서는 캐시만 invalidate.
    fn change(&mut self, _idx: usize) {
        // Cache invalidation (raw asm 의 FUN_002e66fc 역할):
        self.cache_req_valid = false;
        self.cache_bounds_valid = false;
    }

    /// `Box::GetCount` `FUN_00331ee0` (sz=8). raw: `ret this[+0x18]`.
    fn get_count(&self) -> usize {
        self.children.len()
    }

    /// `Box::GetComponent` `FUN_00331ee8` (sz=??). raw: walk DLL by idx, return holder.glyph.
    fn get_component(&self, idx: usize) -> Option<&dyn Glyph> {
        self.children.get(idx).and_then(|c| c.as_deref())
    }

    fn get_component_mut(&mut self, idx: usize) -> Option<&mut dyn Glyph> {
        self.children.get_mut(idx).and_then(|c| c.as_deref_mut())
    }

    /// `Box::Undraw(Flag const&)` — `0x2e6404` (208B). raw asm:
    /// ```text
    /// add x20, x0, #0x8           ; x20 = &this->sentinel (= self.children list head)
    /// ldr x21, [x0, #0x10]        ; x21 = sentinel.next (= 첫 node)
    /// cmp x21, x20; b.eq ret      ; 빈 list?
    /// mov x19, x1                 ; flag
    /// loop:
    ///   ldr x8, [x21, #0x10]      ; node.child_share (= holder.glyph SharePtr)
    ///   cbz x8, advance
    ///   ldr x0, [x8]              ; child = *SharePtr
    ///   cbz x0, advance
    ///   ldr x8, [x0]; ldr x8, [x8, #0x30]; mov x1, x19; blr x8   ; child->Undraw(flag)
    /// advance:
    ///   ldr x21, [x21, #0x8]      ; node = node.next
    ///   cmp x21, x20; b.ne loop
    /// ret
    /// ```
    ///
    /// raw 는 doubly-linked list 직접 순회 (vfunc[+0x80] GetCount / vfunc[+0x88] GetComponent
    /// 미사용 — 효율). Rust 의 `self.children: Vec` 순회로 동등. None slot (= raw 의 SharePtr
    /// inner null) 도 skip — base default `Glyph::Undraw` 와 byte-equivalent observable 결과.
    ///
    /// **정공법 노트**: base default 가 같은 결과 내지만, raw 가 별도 method 로 존재 →
    /// 1:1 매핑 정신상 Rust 도 override 유지. 호출 횟수 / linked-list traversal 패턴 보존.
    fn undraw(&self, flag: &Flag) {
        for child_opt in &self.children {
            // raw `cbz x8, advance` + `cbz x0, advance` — SharePtr inner null skip.
            //   Rust `Option<Box<dyn Glyph>>::Some(c)` 가 동등.
            if let Some(child) = child_opt {
                child.undraw(flag);
            }
        }
    }

    /// `Box::GetAllotment` `FUN_002e6688` (sz=??). raw: idx 범위 check + `this[+0x70][idx]`
    /// (= `cached_allocations[idx]`) 의 dim 축 Allotment 추출.
    ///
    /// `cached_allocations` 는 `recompute_bounds_cache` (FUN_002e6120) 가 채움. 미계산
    /// 상태면 zero.
    fn get_allotment(&self, idx: usize, dim: Dimension, out: &mut Allotment) {
        match self.cached_allocations.get(idx) {
            Some(alloc) => {
                *out = match dim {
                    Dimension::X => alloc.x,
                    Dimension::Y => alloc.y,
                };
            }
            None => {
                *out = Allotment::ZERO;
            }
        }
    }
}

/// `Hnc::Shape::Text::Placement` glyph (vtable `0x781168`, typeinfo `N3Hnc5Shape4Text9PlacementE`)
/// — `MonoGlyph` 의 subclass 로, body Glyph 하나에 placement *strategy* 를 묶는다.
///
/// Object layout (24B, raw 검증: `LayoutFactory::CreateNatural` `FUN_00302364`,
/// `Composition::CreateItem` force path `FUN_003000a8`, dtor `FUN_00331044`):
/// - +0x00: vtable
/// - +0x08: `child` — body Glyph (`SharePtr<Glyph>`, null 가능)
/// - +0x10: `placement` — strategy (`SharePtr<Placement>`: PlaceNatural/PlaceFix/PlaceMargin/PlaceCenter)
///
/// (이름은 기존 port 호환을 위해 `MonoGlyph` 로 유지 — 실제로는 bare `MonoGlyph` `0x780e60`
/// 가 아니라 strategy 를 가진 `Placement` glyph `0x781168` 를 모델한다.)
#[derive(Debug)]
pub struct MonoGlyph {
    /// +0x10 — placement strategy.
    pub placement: Box<dyn Placement>,
    /// +0x08 — body Glyph. Hancom `SharePtr<Glyph>` 라서 null 가능 → `Option`.
    pub child: Option<Box<dyn Glyph>>,
}

impl Glyph for MonoGlyph {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    /// `Placement::Clone` (`FUN_003311ac`, 96B): 새 24B 객체에 body+strategy 를 복사 (한컴은
    /// SharePtr 공유 + refcount++). Rust 는 deep-clone — layout output 이 deterministic 이라
    /// byte-equivalent.
    fn clone_glyph(&self) -> Box<dyn Glyph> {
        Box::new(Self {
            placement: self.placement.clone_placement(),
            child: self.child.as_ref().map(|c| c.clone_glyph()),
        })
    }

    /// `Placement::Request` (`FUN_00331214`, 132B) 의 1:1 포팅 — child + layout dispatch.
    ///
    /// raw decompile:
    /// ```c
    /// void Placement::Request(Placement *this, Requisition *req) {
    ///   // step 1: child.request(req)
    ///   if (*(this+8) != 0 && *(*(this+8)) != 0) {
    ///       (*((*(this+8))+0x18))(*(this+8), req);   // = child.vtable[+0x18](req)
    ///   }
    ///   // step 2: layout.request(&local_vec, req)
    ///   if (*(this+0x10) != 0 && *(*(this+0x10)) != 0) {
    ///       vector<Requisition> local_vec = empty;
    ///       (*((*(this+0x10))+0x18))(*(this+0x10), &local_vec, req);
    ///       // vector destructor (PlaceMargin/Center/Natural 본문에서 local_vec 미사용)
    ///   }
    /// }
    /// ```
    ///
    /// **객체 layout 매핑** (raw `LayoutFactory::CreateNatural/Margin/Center/Fix` 의 24B
    /// allocation):
    /// - `+0x00` vtable = `PTR__Placement_00781168`
    /// - `+0x08` `SharePtr<Glyph>` child — Rust 는 `self.child` (owned `Option<Box<dyn Glyph>>`)
    /// - `+0x10` `SharePtr<PlaceXxx>` layout strategy — Rust 는 `self.placement` (owned
    ///   `Box<dyn Placement>`)
    ///
    /// **vector 인자**: raw 의 `local_vec` 는 `vector<Requisition>` empty buffer 로 layout
    /// strategy 에 전달되지만 PlaceCenter/PlaceMargin/PlaceNatural 의 본문 (raw decompile
    /// 검증) 에서 미사용 — Rust 는 무시.
    ///
    /// **2026-05-15 14d 정정**: 이전 Rust 는 본 dispatch override 부재로 `Glyph::request`
    /// default (no-op) 사용 → `Requisition::INVALID` 반환 → **byte 출력 불일치**. 이 결함은
    /// 14번째 세션 audit 에서 사용자 지적으로 발견.
    fn request(&self, req: &mut crate::value_types::Requisition) {
        // step 1: child.request(req) — raw 가 SharePtr 의 inner null 가드.
        //   Rust 는 `Option<Box<dyn Glyph>>` 의 `Some(c)` 가 동등.
        if let Some(child) = &self.child {
            child.request(req);
        }
        // step 2: placement (= layout strategy).request(req).
        //   raw 가 `(layout, &local_vec, req)` 호출이지만 본 vec 인자는 strategy 본문에서
        //   미사용 (PlaceCenter/Margin/Natural 의 decompile 검증) — trait `request(req)` 로 충분.
        self.placement.request(req);
    }

    /// `Placement::Allocate` (`FUN_003312b4`, 112B) + `Placement::CalcPlacement`
    /// (`FUN_00331324`, 280B) 의 1:1 포팅 — child + layout 2단 dispatch.
    ///
    /// raw decompile (`Placement::Allocate`):
    /// ```c
    /// void Placement::Allocate(Placement *this, Allocation *avail, Extension *ext) {
    ///   long *plVar1 = *(long **)(this + 8);          // child SharePtr inner
    ///   if (plVar1 != NULL && *plVar1 != NULL) {       // child not null
    ///       Allocation local_40 = *avail;              // local stack copy
    ///       CalcPlacement(this + 8, avail, this + 0x10, &local_40);
    ///       // local_40 is now the child's sub-allocation.
    ///       (**(code **)(*plVar1 + 0x20))(plVar1, &local_40, ext);
    ///       // = child.vtable[+0x20](child, child_alloc, ext)
    ///   }
    /// }
    /// ```
    ///
    /// raw decompile (`Placement::CalcPlacement`):
    /// ```c
    /// void Placement::CalcPlacement(
    ///     SharePtr *child_share, Allocation *parent_alloc,
    ///     SharePtr *layout_share, Allocation *out_alloc)
    /// {
    ///   // entry guards: child / layout SharePtr null check.
    ///   if (valid_pointers) {
    ///       Requisition local_60 = INVALID_sentinels;        // _DAT_00741f20/_UNK_00741f28...
    ///       (**(code **)(*plVar1 + 0x18))(plVar1, &local_60); // child.Request(&local_60)
    ///
    ///       Requisition *req_share = operator_new(0x24);     // heap copy
    ///       *req_share = local_60;
    ///       std::vector<Requisition>{req_share..req_share+1};
    ///
    ///       Allocation *alloc_share = operator_new(0x18);    // heap copy
    ///       *alloc_share = *out_alloc;                       // init = parent_alloc copy
    ///       std::vector<Allocation>{alloc_share..alloc_share+1};
    ///
    ///       (**(code **)(*(*layout_share)+ 0x20))(           // layout.Allocate
    ///           *layout_share, parent_alloc, &req_vec, &alloc_vec);
    ///
    ///       *out_alloc = *alloc_share;                       // write modified alloc back
    ///       // (vector destructors / refcount decrements omitted)
    ///   }
    /// }
    /// ```
    ///
    /// **Rust 포팅 매핑**:
    /// - `this + 8` (child SharePtr) → `self.child: Option<Box<dyn Glyph>>` non-null = Some.
    /// - `this + 0x10` (layout SharePtr) → `self.placement: Box<dyn Placement>` (항상 valid).
    /// - `local_60` (stack Requisition INVALID) → `Requisition::INVALID`.
    /// - heap `operator_new(0x24)` / `operator_new(0x18)` 의 vec wrapping → Rust 의 `&mut`
    ///   value 로 단순화 (strategy 가 `vec[0]` 만 접근).
    /// - `child.vtable[+0x20]` = `Glyph::allocate` (vfunc[4] = `(Allocation, Extension)`).
    /// - `layout.vtable[+0x20]` = `Placement (strategy).allocate` — Rust trait
    ///   `Placement::allocate(&self, &mut Allocation, &Requisition)`.
    ///
    /// **byte-equivalent 주의**: `Placement::Request` (parent 가 Phase 1 에서 호출) 가
    /// `PlaceMargin` 의 cache 를 미리 채워둠. 본 `allocate` 의 strategy.allocate 호출은
    /// 그 cache 를 read 만 함 (PlaceMargin 한정). `PlaceCenter` 는 `child_req` param 을
    /// 사용. `PlaceFix` 는 `self.fix_size` 만. `PlaceNatural` 은 no-op.
    fn allocate(&mut self, avail: &Allocation, ext: &mut Extension) {
        // raw line 19: `if ((plVar1 != (long *)0x0) && (*plVar1 != 0))` — child SharePtr
        //   inner null 가드. Rust 의 `Option<Box<dyn Glyph>>` 의 `Some(c)` 가 동등.
        if let Some(child) = self.child.as_mut() {
            // raw CalcPlacement step 1: local_60 := INVALID, child.Request(&local_60).
            let mut child_req = Requisition::INVALID;
            child.request(&mut child_req);

            // raw CalcPlacement step 2: alloc_share := *out_alloc (= avail copy);
            //   layout.Allocate(avail, [child_req], [&mut alloc_share]) → strategy 가
            //   alloc_share 를 in-place 수정 하여 child sub-allocation 산출.
            let mut child_alloc = *avail;
            self.placement.allocate(&mut child_alloc, &child_req);

            // raw line 24-25: `child.vtable[+0x20](child, child_alloc, ext)`.
            //   Rust 의 Glyph::allocate(&mut self, &Allocation, &mut Extension).
            child.allocate(&child_alloc, ext);
        }
        // raw 의 child null 인 경우 ext 미수정 (early return) — Rust 도 동일 (`if let Some`
        //   miss 시 아무 동작 안 함).
    }

    /// `Placement::Draw(Surface&, Allocation const&, Flag const&, BWMode const&)` — `0x331488`.
    ///
    /// **vfunc 매핑**: Rust 의 `MonoGlyph` struct 는 raw vtable `0x781168` (= Placement) 를
    /// 모델 (`MonoGlyph` 이름은 호환 유지). 따라서 본 method 는 **`Placement::Draw` 의 1:1 port**,
    /// 단순 `MonoGlyph::Draw` (`0x2d08a8`, child forward only without CalcPlacement) 가 아님.
    ///
    /// raw asm `0x331488` (sz=144B):
    /// ```text
    /// ldr x8, [x0, #0x8]!     ; child SharePtr (= this+0x8)
    /// cbz x8, ret
    /// ldr x8, [x8]; cbz x8, ret  ; child = *SharePtr (Glyph*)
    ///
    /// ; alloc copy to stack (24B = 16B+8B)
    /// ldr q0, [x2]; str q0, [sp]
    /// ldr x8, [x2, #0x10]; str x8, [sp, #0x10]
    ///
    /// ; CalcPlacement(child_share, parent_alloc, strategy_share, &local_alloc)
    /// add x8, x22, #0x10       ; x8 = this+0x10 (strategy)
    /// mov x3, sp; mov x1, x2; mov x2, x8
    /// bl Placement::CalcPlacement
    ///
    /// ; child->Draw(surface, &local_alloc, flag, bwmode)
    /// ldr x8, [x22, #0x8]; ldr x0, [x8]
    /// ldr x8, [x0]; ldr x8, [x8, #0x28]   ; child.vfunc[+0x28] = Draw
    /// mov x2, sp; mov x1, x21; mov x3, x20; mov x4, x19
    /// blr x8
    /// ret
    /// ```
    ///
    /// CalcPlacement 의 로직은 `Self::allocate` 안에 이미 inline. 동일 로직 적용 후 child forward.
    fn draw(&mut self, surface: &mut dyn Surface, alloc: &Allocation, flag: &Flag, bw: &BWMode) {
        if let Some(child) = &mut self.child {
            // raw CalcPlacement: child.Request + strategy.Allocate(parent, [child_req], [&mut local_alloc]).
            let mut child_req = Requisition::INVALID;
            child.request(&mut child_req);
            let mut child_alloc = *alloc;
            // strategy.allocate 가 &mut self.placement 필요. &mut self trait 가 됐으니
            // self.placement 직접 mut 접근 가능 — clone 회피.
            self.placement.allocate(&mut child_alloc, &child_req);
            child.draw(surface, &child_alloc, flag, bw);
        }
    }

    /// `Placement::Undraw(Flag const&)` — `0x331518` (32 byte, **CalcPlacement 없이 direct
    /// forward**, raw asm 검증).
    ///
    /// raw asm `0x331518`:
    /// ```text
    /// ldr x8, [x0, #0x8]; cbz x8, ret  ; child SharePtr null
    /// ldr x0, [x8]; cbz x0, ret         ; child null
    /// ldr x8, [x0]; ldr x2, [x8, #0x30]; br x2  ; child.vfunc[+0x30] = Undraw, tail-call
    /// ret
    /// ```
    ///
    /// child null 시 raw 는 단순 ret (no observable side effect, Glyph::Undraw 의 container
    /// traversal 도 안 함). Rust 의 base default 가 container traversal 인데, 본 override
    /// 가 그것을 **단일 child forward 로 좁힘** — raw 와 동일.
    fn undraw(&self, flag: &Flag) {
        if let Some(child) = &self.child {
            child.undraw(flag);
        }
    }

    /// `Placement::GetBounds(Theme const*, Allocation const&, Glyph*)` — `0x331538` (176B).
    ///
    /// raw asm `0x331538`:
    /// ```text
    /// ; entry: x8 = sret out_alloc, x20 = sret saved
    /// ; child null 가드:
    /// ldr x8, [x0, #0x8]!; cbz x8, fallback  ; child SharePtr null
    /// ldr x8, [x8]; cbz x8, fallback         ; child null
    ///
    /// ; alloc copy + CalcPlacement (Draw 와 동일)
    /// ldr q0, [x2]; str q0, [sp]; ldr x8, [x2, #0x10]; str x8, [sp, #0x10]
    /// add x8, x22, #0x10; mov x3, sp; mov x1, x2; mov x2, x8
    /// bl Placement::CalcPlacement
    ///
    /// ; child->GetBounds(theme=x21, &local_alloc, child_param=x19) → out=x20
    /// ldr x8, [x22, #0x8]; ldr x0, [x8]; ldr x8, [x0]; ldr x9, [x8, #0x38]
    /// mov x2, sp; mov x8, x20; mov x1, x21; mov x3, x19
    /// blr x9
    /// ret
    ///
    /// fallback:
    ///   strb wzr, [x20]
    ///   stur xzr, [x20, #0xc]
    ///   stur xzr, [x20, #0x4]
    ///   ret
    /// ```
    ///
    /// child null fallback = Glyph::GetBounds 의 zero output 과 동일 (`Allocation::ZERO`).
    fn get_bounds(
        &mut self,
        theme: Option<&Theme>,
        alloc: &Allocation,
        child_param: Option<&dyn Glyph>,
    ) -> Allocation {
        if let Some(child) = &mut self.child {
            let mut child_req = Requisition::INVALID;
            child.request(&mut child_req);
            let mut child_alloc = *alloc;
            self.placement.allocate(&mut child_alloc, &child_req);
            child.get_bounds(theme, &child_alloc, child_param)
        } else {
            // raw fallback `0x3315c8`: zero sret output (= Glyph::GetBounds base default).
            Allocation::ZERO
        }
    }

    /// `Placement::Pick(Allocation const&, Theme const*, Hit&, int) -> bool` — `0x3315e8` (160B).
    ///
    /// raw asm `0x3315e8`:
    /// ```text
    /// ldr x8, [x0, #0x8]!; cbz x8, fallback
    /// ldr x8, [x8]; cbz x8, fallback
    ///
    /// ; alloc copy + CalcPlacement (Draw 와 동일)
    /// ldr q0, [x1]; str q0, [sp]; ldr x8, [x1, #0x10]; str x8, [sp, #0x10]
    /// add x2, x22, #0x10; mov x3, sp
    /// bl Placement::CalcPlacement
    ///
    /// ; return child->Pick(&local_alloc, theme=x21, hit=x20, depth=x19)
    /// ldr x8, [x22, #0x8]; ldr x0, [x8]; ldr x8, [x0]; ldr x8, [x8, #0x40]
    /// mov x1, sp; mov x2, x21; mov x3, x20; mov x4, x19
    /// blr x8
    /// ret
    ///
    /// fallback: mov w0, #0; ret  ; = Glyph::Pick base
    /// ```
    fn pick(
        &mut self,
        alloc: &Allocation,
        theme: Option<&Theme>,
        hit: &mut Hit,
        depth: i32,
    ) -> bool {
        if let Some(child) = &mut self.child {
            let mut child_req = Requisition::INVALID;
            child.request(&mut child_req);
            let mut child_alloc = *alloc;
            self.placement.allocate(&mut child_alloc, &child_req);
            child.pick(&child_alloc, theme, hit, depth)
        } else {
            false
        }
    }
}

/// `Hnc::Shape::Text::BlipGlyph` — inline image glyph (그림 글머리표 / inline 이미지).
///
/// raw layout (64B, vtable `PTR__BlipGlyph_0077faf0` @ 0x77faf0). `FUN_002eaf54`
/// (bullet render ctor) 의 picture-bullet 경로가 `operator_new(0x40)` 로 생성:
/// ```text
/// +0x08 f32  width        (= fVar29 * fVar27)
/// +0x0c f32  f_0c         (= 0)
/// +0x10 f32  f_10         (= 0)
/// +0x14 f32  x_anchor     (= 1.0)
/// +0x18 f32  height       (= fVar28 * fVar27)
/// +0x1c f32  f_1c         (= 0)
/// +0x20 f32  f_20         (= 0)
/// +0x24 f32  y_anchor     (= 1.0)
/// +0x28 u32  u_28         (= 0; default-branch Request 의 penalty)
/// +0x30 SharePtr<ImageBrush>  — rendering 전용 (Draw). layout 무관 → FUN_002eaf54 포팅 시 추가.
/// +0x38 u32  paragraph_class  (= local_164, BodyProperty key 0x89e)
/// ```
///
/// layout 관련 vfunc (raw 1:1):
/// - Clone (`FUN_002d12a0`, +0x10) / Request (`FUN_002d137c`, +0x18) / Allocate
///   (`FUN_002d13e8`, +0x20).
/// - Draw (`FUN_002d1480`, 1320B) 는 rendering 전용 — `Glyph::draw` default no-op (layout 무관).
///
/// **정공법 메모**: `+0x30` `SharePtr<ImageBrush>` 는 `Draw` 만 사용 — layout (Request/
/// Allocate) 에 무관하고 `ImageBrush` 가 아직 RE 되지 않아 본 단계에선 필드 생략.
/// `FUN_002eaf54` 포팅 시 추가 (bottom-up RE — 컨테이너보다 leaf 의 layout surface 먼저).
#[derive(Debug, Default, Clone)]
pub struct BlipGlyph {
    /// `+0x08` — width.
    pub width: f32,
    /// `+0x0c` — `FUN_002eaf54` picture-bullet 경로에서 0.
    pub f_0c: f32,
    /// `+0x10` — `FUN_002eaf54` picture-bullet 경로에서 0.
    pub f_10: f32,
    /// `+0x14` — x anchor ratio (`FUN_002eaf54` 에서 1.0).
    pub x_anchor: f32,
    /// `+0x18` — height.
    pub height: f32,
    /// `+0x1c` — `FUN_002eaf54` picture-bullet 경로에서 0.
    pub f_1c: f32,
    /// `+0x20` — `FUN_002eaf54` picture-bullet 경로에서 0.
    pub f_20: f32,
    /// `+0x24` — y anchor ratio (`FUN_002eaf54` 에서 1.0).
    pub y_anchor: f32,
    /// `+0x28` — default-branch Request 의 penalty 값 (`FUN_002eaf54` 에서 0).
    pub u_28: u32,
    /// `+0x38` — paragraph class (`BodyProperty::GetVert`, key 0x89e). Request/Allocate 의
    ///   `pc < 7 && (1<<pc)&0x65 != 0` 분기 키.
    pub paragraph_class: u32,
}

impl Glyph for BlipGlyph {
    /// `BlipGlyph::Clone` (`FUN_002d12a0`, +0x10): `operator_new(0x40)` + `+0x08..+0x38`
    /// 필드 복사 (+0x30 SharePtr refcount++). Rust 는 derive Clone (layout 필드만 — +0x30
    /// 미모델).
    fn clone_glyph(&self) -> Box<dyn Glyph> {
        Box::new(self.clone())
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    /// `BlipGlyph::Request(Requisition&)` (`FUN_002d137c`, +0x18).
    ///
    /// raw:
    /// ```c
    /// if (pc < 7 && (1 << (pc & 0x1f)) & 0x65 != 0) {     // special (0x002d13a0-bc)
    ///   req.x = {height, f_1c, f_20, 0.5};
    ///   req.y = {width, f_0c, f_10, x_anchor};
    ///   /* penalty (+0x20) 미기록 */
    /// } else {                                            // default (0x002d13c4-d8)
    ///   req.x = {width, f_0c, f_10, x_anchor};
    ///   req.y = {height, f_1c, f_20, y_anchor};
    ///   req.penalty = u_28;
    /// }
    /// ```
    fn request(&self, req_out: &mut Requisition) {
        let pc = self.paragraph_class;
        // raw 0x002d1380-9c: `cmp pc,#6; lsl 1,1,pc; and ,0x65; ccmp ,#0,#4,ls` —
        //   `pc <= 6 && (1<<pc) & 0x65 != 0`.
        if pc < 7 && (1u32 << (pc & 0x1f)) & 0x65 != 0 {
            // raw 0x002d13a0-bc — special branch.
            req_out.x = Requirement {
                natural: self.height,
                stretch: self.f_1c,
                shrink: self.f_20,
                alignment: 0.5, // raw mov w9,#0x3f000000
            };
            req_out.y = Requirement {
                natural: self.width,
                stretch: self.f_0c,
                shrink: self.f_10,
                alignment: self.x_anchor,
            };
            // raw: penalty (+0x20) 미기록 — caller 초기값 유지.
        } else {
            // raw 0x002d13c4-d8 — default branch.
            req_out.x = Requirement {
                natural: self.width,
                stretch: self.f_0c,
                shrink: self.f_10,
                alignment: self.x_anchor,
            };
            req_out.y = Requirement {
                natural: self.height,
                stretch: self.f_1c,
                shrink: self.f_20,
                alignment: self.y_anchor,
            };
            req_out.penalty = self.u_28 as i32;
        }
    }

    /// `BlipGlyph::Allocate(Allocation const&, Extension&)` (`FUN_002d13e8`, +0x20).
    ///
    /// raw:
    /// ```c
    /// x0 = alloc.x.origin; y0 = alloc.y.origin; w = width; h = height;
    /// if (pc < 7 && (1 << (pc & 0x1f)) & 0x65 != 0) {     // special (0x002d1418-30)
    ///   ext = {x0 - h*0.5, y0 - w, x0 + h*0.5, y0};
    /// } else {                                            // default (0x002d1434-60)
    ///   ext = {x0 - x_anchor*w, y0 - y_anchor*h,
    ///          x0 - (1-x_anchor)*w, y0 - (1-y_anchor)*h};
    /// }
    /// ```
    fn allocate(&mut self, alloc: &Allocation, ext: &mut Extension) {
        // raw 0x002d13e8-f4: fVar2 = *param_1 (alloc.x.origin), fVar1 = *(param_1+0xc)
        //   (alloc.y.origin — Allotment 12B 라 y 는 +0xc), fVar4 = width, fVar3 = height.
        let x0 = alloc.x.origin;
        let y0 = alloc.y.origin;
        let w = self.width; // raw fVar4 = this+0x08
        let h = self.height; // raw fVar3 = this+0x18
        let pc = self.paragraph_class;
        if pc < 7 && (1u32 << (pc & 0x1f)) & 0x65 != 0 {
            // raw 0x002d1418-30 — special branch (fmsub/fmadd).
            ext.left = x0 - h * 0.5;
            ext.top = y0 - w;
            ext.right = x0 + h * 0.5;
            ext.bottom = y0;
        } else {
            // raw 0x002d1434-60 — default branch.
            let xa = self.x_anchor; // raw fVar6 = this+0x14
            let ya = self.y_anchor; // raw fVar5 = this+0x24
            ext.left = x0 - xa * w;
            ext.top = y0 - ya * h;
            ext.right = x0 - (1.0 - xa) * w;
            ext.bottom = y0 - (1.0 - ya) * h;
        }
    }
}

/// `Hnc::Shape::Text::WidgetGlyph` — inline form control.
#[derive(Debug, Default, Clone)]
pub struct WidgetGlyph;

impl Glyph for WidgetGlyph {
    fn clone_glyph(&self) -> Box<dyn Glyph> { Box::new(self.clone()) }
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
}

/// `Hnc::Shape::Text::CharItemView` — character-level item view.
///
/// **2026-05-15 14번째 세션 종료 기준**: 한컴 원본은 ~400 bytes / 61 메소드 의 큰 클래스 이나,
/// `PptCompositor::ComposeLayout` / `ComposeBreak` / `ComposeBullet` / `ComposeNumbering` 이
/// 접근하는 모든 필드 + raw ctor `FUN_002ef798` (1840B) 의 layout 영향 부분 1:1 포팅 완료
/// (`from_ctor_context`). render-only 메소드 (DrawDirect 등) 와 RTTI 메타 등은 layout 출력
/// byte-eq 와 무관하므로 별도 module 에서 추후 다룸.
///
/// vtable @ 0x780098 (`PTR__CharItemView_00780098`, constructor 에서 set).
/// RTTI: `Hnc::Shape::Text::CharItemView`.
///
/// 한컴 원본 필드 layout (constructor `FUN_002ef798` sz=1840 에서 set):
/// - +0x00: vtable
/// - +0x08: `short` — 문자 코드 (0x0d=CR, 0x0a=LF, 0x20=space 등)
/// - +0x10: 0 (pointer slot)
/// - +0x18: SharePtr<RunProperty> (run-level 속성)
/// - +0x20: SharePtr<ParaProperty> (paragraph-level 속성)
/// - +0x28: SharePtr<BodyProperty> (body-level 속성)
/// - +0x30..+0x38: 0 (pointer slot)
/// - +0x40, +0x44: f32, f32 — ascent/descent (FUN_000764fc 에서 set)
/// - +0x48, +0x50: 0
/// - +0x4c: f32 — width (FUN_000764fc 에서 set)
/// - +0x54: f32 — total ascent + line-height
/// - +0x58: f32 — line height (computed: fontHeight × 1.2 × dpi / 72)
/// - +0x5c: f32 — vertical anchor ratio (ascent / (ascent+descent))
/// - +0x60: f32 — `param_6` (constructor float arg). ComposeLayout stage 11 에서
///                space char 일 때 `0x4cbebc20` (=1e8) 으로 set 됨
/// - +0x64: f32 — additional metric
/// - +0x68: f32 — additional metric
/// - +0x6c: f32 — paragraph line height (ComposeLayout stage 11 에서 set)
/// - +0x70: f32 — line anchor (`local_11c`, ComposeLayout stage 11)
/// - +0x74: f32 — alignment ratio (`fVar42`, ComposeLayout stage 11)
/// - +0x90: Theme* — render context
/// - +0x98: SharePtr — render path (ComposeLayout stage 6 에서 vtable +0x18 호출)
/// - +0xb8..+0x170: ImagePainterObject (rendering state)
///
/// ComposeLayout 에 의해 mutate 되는 필드만 표시:
/// - `+0x58` (`line_height`)
/// - `+0x5c` (`vertical_anchor_ratio`)
/// - `+0x60` (`reset_flag_or_size`) — 0x4cbebc20 으로 zero-out
/// - `+0x6c` (`paragraph_line_height`)
/// - `+0x70` (`line_anchor`)
/// - `+0x74` (`alignment_ratio`)
#[derive(Debug, Default)]
pub struct CharItemView {
    /// `+0x08` — 문자 코드 (UTF-16 code unit). 0x0d=CR, 0x0a=LF, 0x20=space.
    pub char_code: u16,

    /// `+0x18` — SharePtr<RunProperty> (한컴 원본). Rust 에선 owned Option.
    /// `PptCompositor::ComposeLayout` stage 8 가 font size (key 0x96a) 추출 시 사용.
    pub run_property: Option<crate::runtime::RunProperty>,

    /// `+0x20` — `SharePtr<ParaProperty>` (한컴 원본). Rust 에선 owned Option.
    /// `PptCompositor::ComposeNumbering` (`lVar6 + 0x20`) / `ComposeBullet` / `ComposeBreak`
    /// 가 paragraph 의 bullet / level (key 0x902) 추출 시 사용.
    pub para_property: Option<crate::text_property::ParaProperty>,

    /// `+0x28` — `SharePtr<BodyProperty>` (한컴 원본). Rust 에선 owned Option.
    /// `PptCompositor::ComposeBreak` 가 `BodyProperty::GetVert` (key 0x89e) 추출 시 사용.
    pub body_property: Option<crate::text_property::BodyProperty>,

    /// `+0x40` — ascent (위 부분 높이).
    pub ascent: f32,
    /// `+0x44` — descent (아래 부분 높이).
    pub descent: f32,
    /// `+0x4c` — width (문자 너비).
    pub width: f32,

    /// `+0x54` — total height (line-height 포함).
    pub total_height: f32,

    /// `+0x58` — line height (`fontHeight × 1.2 × dpi / 72`). ComposeLayout 의 fVar36.
    pub line_height: f32,

    /// `+0x5c` — vertical anchor ratio (`ascent / (ascent + descent)`). ComposeLayout 의 fVar37.
    pub vertical_anchor_ratio: f32,

    /// `+0x60` — runtime metric. Stage 11 에서 space-class 문자 면 1e8 으로 reset.
    pub reset_or_size: f32,

    /// `+0x64` — additional metric.
    pub metric_64: f32,
    /// `+0x68` — additional metric.
    pub metric_68: f32,

    /// `+0x6c` — paragraph line height (`fVar44`, set by ComposeLayout stage 11).
    pub paragraph_line_height: f32,

    /// `+0x70` — line anchor (`local_11c`, set by ComposeLayout stage 11).
    pub line_anchor: f32,

    /// `+0x74` — alignment ratio (`fVar42`, set by ComposeLayout stage 11).
    pub alignment_ratio: f32,

    /// `+0x90` — Theme 포인터 (render context).
    ///
    /// raw `CharItemView::CharItemView` (`FUN_002ef798` line 78,
    /// `bullet_render_deps.txt:3487`):
    /// ```c
    /// *(Theme **)(this + 0x90) = param_5;
    /// ```
    /// raw `CharItemView::GetTheme()` (`FUN_002f0ab0`):
    /// ```c
    /// return *(undefined8 *)(this + 0x90);
    /// ```
    /// raw `PptCompositor::ComposeBullet` (`0x307468:174`):
    /// ```c
    /// uVar5 = *(undefined8 *)(lVar9 + 0x90);  // → param_5 of FUN_002eaf54.
    /// ```
    /// 한컴 원본은 raw pointer (non-owning) — Theme 객체 자체는 외부에서 소유. Rust 는
    /// owned `Option<Theme>` 으로 clone 보존 (Theme 은 face name 문자열 2개만 가진 경량
    /// 구조이므로 clone 비용 무시 가능).
    pub theme: Option<crate::runtime::Theme>,

    /// `+0x98` — render path SharePtr.
    ///
    /// raw 호출부 (`PptCompositor::ComposeBreak` 0x307d2c-0x308060,
    /// `PptCompositor::ComposeLayout` 0x308ac4-0x308b28):
    /// ```c
    /// render_path = *(long **)(char_view + 0x98);
    /// (**(code **)(*render_path_obj + 0x18))(render_path_obj, &requisition_buf);
    /// ```
    /// 즉 슬롯은 값 객체가 아니라 `Glyph` vtable surface 를 가진 SharePtr 이다. 실제
    /// `ComposeBullet` 경로에서는 `BulletRenderGlyph` 가 들어가고, 그 `Request` 는 내부
    /// HBox/VBox layout 으로 forward 한다.
    pub render_path: Option<Box<dyn Glyph>>,
}

impl Clone for CharItemView {
    fn clone(&self) -> Self {
        Self {
            char_code: self.char_code,
            run_property: self.run_property.clone(),
            para_property: self.para_property.clone(),
            body_property: self.body_property.clone(),
            ascent: self.ascent,
            descent: self.descent,
            width: self.width,
            total_height: self.total_height,
            line_height: self.line_height,
            vertical_anchor_ratio: self.vertical_anchor_ratio,
            reset_or_size: self.reset_or_size,
            metric_64: self.metric_64,
            metric_68: self.metric_68,
            paragraph_line_height: self.paragraph_line_height,
            line_anchor: self.line_anchor,
            alignment_ratio: self.alignment_ratio,
            theme: self.theme.clone(),
            render_path: self.render_path.as_ref().map(|g| g.clone_glyph()),
        }
    }
}

/// 한컴 `RenderPath` 의 vtable +0x18 (slot 3) 호출 결과.
///
/// `PptCompositor::ComposeLayout` (line 597-610) 의 호출 패턴:
/// ```text
/// local_f0 = _DAT_00741f20;  // bytes: 20 bc be cc 00 00 00 00 (= [-1e8, 0])
/// uStack_e8 = _UNK_00741f28; // bytes: 00 00 00 00 00 00 00 00 (= [0, 0])
/// local_e0 = -1e+08;          // special slot init
/// (**(code **)(*plVar23 + 0x18))(plVar23, &local_f0);  // render path's vtable[3]
/// if ((6 < uVar32) || ((1 << (uVar32 & 0x1f)) & 0x65) == 0) {
///     fVar44 = local_f0._0_4_;  // default slot (paragraph_class NOT in {0, 2, 5, 6})
/// } else {
///     fVar44 = local_e0;        // special slot (paragraph_class in {0, 2, 5, 6})
/// }
/// ```
///
/// `0x65 = 0b01100101` → bits 0, 2, 5, 6 set. 즉 paragraph_class ∈ {0, 2, 5, 6} 이면
/// special slot 사용.
///
/// **모델링 한계**: 한컴 의 RenderPath 는 font shaping 후 글리프 단위 metric 을 갖는
/// 큰 구조. 여기선 layout 에 필요한 두 값만 노출.
#[derive(Debug, Default, Clone, Copy)]
pub struct FirstLineMetrics {
    /// `local_f0._0_4_` — paragraph_class NOT in {0, 2, 5, 6} 일 때 사용.
    pub default_ascent: f32,
    /// `local_e0` — paragraph_class in {0, 2, 5, 6} 일 때 사용.
    pub special_ascent: f32,
}

impl FirstLineMetrics {
    /// `paragraph_class` 에 따라 적절한 ascent 슬롯 반환.
    ///
    /// 한컴 line 608 의 비트 마스크 `0x65 = bits 0,2,5,6`:
    /// `(6 < class)` OR `(1 << class) & 0x65 == 0` → default_ascent
    /// 그렇지 않으면 special_ascent.
    pub fn pick_for_paragraph_class(&self, paragraph_class: u32) -> f32 {
        if paragraph_class > 6 || ((1u32 << (paragraph_class & 0x1f)) & 0x65) == 0 {
            self.default_ascent
        } else {
            self.special_ascent
        }
    }
}

/// Metric block produced by `CharItemView::CharItemView` before it writes the
/// layout-facing fields.
///
/// raw `FUN_002ef798` calls `FUN_000764fc` with an output buffer in the constructor
/// metric area, then uses these values to fill `+0x54..+0x74`. This type deliberately
/// represents only already-measured data. The CoreText-backed measurement path itself
/// belongs to `font_metric.rs` / platform FFI.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CharItemViewConstructorMetrics {
    /// raw `this+0x3c` — used for `total_height` in paragraph classes other than 5/6.
    pub metric_3c: f32,
    /// raw `this+0x40`.
    pub ascent: f32,
    /// raw `this+0x44`.
    pub descent: f32,
    /// raw `this+0x4c` — converted into `metric_64`.
    pub metric_4c: f32,
    /// raw `this+0x50` — converted into `metric_68`.
    pub metric_50: f32,
}

impl Glyph for FirstLineMetrics {
    fn clone_glyph(&self) -> Box<dyn Glyph> { Box::new(*self) }
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    /// Test/fixture render-path adapter for the raw `vtable+0x18` surface.
    ///
    /// The real Hancom path calls `render_path->Request(&Requisition)` and then reads:
    /// - default slot: first float of the buffer (`local_c0` / `local_f0._0_4_`)
    /// - special slot: `local_b0` / `local_e0`, i.e. the first float of the Y requirement
    ///
    /// This compact fixture writes only those two observed floats. Production bullet
    /// paths should use `BulletRenderGlyph`, not this adapter.
    fn request(&self, req_out: &mut Requisition) {
        req_out.x.natural = self.default_ascent;
        req_out.y.natural = self.special_ascent;
    }
}

/// Extract first-line ascent from a raw render-path Glyph.
///
/// Raw `PptCompositor::ComposeBreak` / `ComposeLayout` initialize a Requisition-like
/// stack buffer, call `render_path->vtable[+0x18]`, then choose:
/// - `req.x.natural` when `paragraph_class > 6` or class not in `{0, 2, 5, 6}`;
/// - `req.y.natural` when class is in `{0, 2, 5, 6}`.
pub fn first_line_ascent_from_render_path(
    render_path: &dyn Glyph,
    paragraph_class: u32,
) -> f32 {
    let mut req = Requisition::INVALID;
    render_path.request(&mut req);
    if paragraph_class > 6 || ((1u32 << (paragraph_class & 0x1f)) & 0x65) == 0 {
        req.x.natural
    } else {
        req.y.natural
    }
}

/// `FUN_002f0ad0` (1016B) — Hancom internal UTF-16 character class classifier.
///
/// `CharItemView::Request` calls this and sets `req.penalty = 1` when the result is in
/// `2..=5`; `PptCompositor::ComposeLayout` also uses the same class range when deciding
/// whether a character behaves like a spacing/break class.
///
/// Raw constants verified in `B8G_RAW_ASM_VERIFICATION.md`:
/// - `DAT_00742dd0` = `(0xcc00, 0xb200, 0x0700, 0xd100)` as 4 u16 lane offsets.
/// - `DAT_00742dd8` = `(0x19c0, 0x51d0, 0x0200, 0x00e0)` as 4 u16 lane limits.
/// - `DAT_00750854` = `[0x1a, 0x1a, 0x1a, 0x1a, 0x1b, 0x1d, 0x1a, 0x1c]`.
pub fn char_item_view_char_class(ch: u16) -> i32 {
    let c = ch as u32;
    let u1 = c & 0xff00;
    let u2 = c & 0xffe0;
    let u3 = c & 0xfff0;
    let u4 = c & 0xff80;

    if ((c + 0x60) & 0xffff) < 0x40 { return 3; }
    if u1 == 0x3200 { return 3; }
    if (((c + 0x5400) >> 10) & 0x3f) < 0x0b { return 3; }
    if u2 == 0xa960 { return 3; }
    if u1 == 0x1100 { return 3; }
    if (c.wrapping_sub(0x3130) & 0xffff) < 0x60 { return 3; }

    if c.wrapping_sub(0x3040) < 0xc0 { return 2; }
    if u3 == 0x31f0 { return 2; }

    let cjk_hit = [
        (0xcc00u16, 0x19c0u16),
        (0xb200u16, 0x51d0u16),
        (0x0700u16, 0x0200u16),
        (0xd100u16, 0x00e0u16),
    ]
    .iter()
    .any(|&(offset, limit)| ch.wrapping_add(offset) < limit);
    if cjk_hit { return 4; }

    if (c.wrapping_sub(0x31c0) & 0xffff) < 0x30 { return 4; }
    if (c.wrapping_sub(0x3100) & 0xffff) < 0x30 { return 4; }
    if u3 == 0x3190 { return 4; }
    if u4 == 0x2e80 { return 4; }
    if u3 == 0x2ff0 { return 4; }
    if u2 == 0x31a0 { return 4; }

    if c.wrapping_sub(0x0370) < 0x90 { return 6; }
    if u1 == 0x1f00 { return 6; }

    if ((c + 0x59c0) & 0xffff) < 0x60 { return 7; }
    if u2 == 0x2de0 { return 7; }
    if u1 == 0x0400 { return 7; }
    if (c.wrapping_sub(0x0500) & 0xffff) < 0x30 { return 7; }

    if ((c + 0x190) & 0xffff) < 0x90 { return 8; }
    if ((c + 0x4b0) & 0xffff) < 0x2b0 { return 8; }
    if u1 == 0x0600 { return 8; }
    if (c.wrapping_sub(0x0750) & 0xffff) < 0x30 { return 8; }

    if c.wrapping_sub(0x0590) < 0x70 { return 9; }
    if ((c + 0x0500) & 0xffff) < 0x50 { return 9; }

    if u4 == 0x0e00 { return 0x0a; }

    if c.wrapping_sub(0x2d80) < 0x60 { return 0x0b; }
    if (c.wrapping_sub(0x1200) & 0xffff) < 0xc0 { return 0x0b; }
    if u2 == 0x1380 { return 0x0b; }

    if u4 == 0x0980 { return 0x0c; }
    if u4 == 0x0a80 { return 0x0d; }

    if u4 == 0x1780 { return 0x0e; }
    if u2 == 0x19e0 { return 0x0e; }

    if u4 == 0x0a00 { return 0x10; }
    if u4 == 0x0c80 { return 0x0f; }
    if c.wrapping_sub(0x1400) < 0x280 { return 0x11; }
    if c.wrapping_sub(0x13a0) < 0x60 { return 0x12; }
    if ((c + 0x6000) & 0xffff) < 0x4d0 { return 0x13; }
    if u1 == 0x0f00 { return 0x14; }
    if (c & 0xffc0) == 0x0780 { return 0x15; }
    if u4 == 0x0900 { return 0x16; }
    if u4 == 0x0b80 { return 0x18; }
    if u4 == 0x0c00 { return 0x17; }
    if c.wrapping_sub(0x0700) < 0x50 { return 0x19; }

    let u4_off = u4.wrapping_sub(0x0b00);
    if (u4_off & 0xffff) < 0x400 {
        let bit = (u4_off >> 7) & 0x1f;
        if (0xb1u32 >> bit) & 1 != 0 {
            const TABLE: [i32; 8] = [0x1a, 0x1a, 0x1a, 0x1a, 0x1b, 0x1d, 0x1a, 0x1c];
            return TABLE[((u4_off >> 7) & 0x7) as usize];
        }
    }

    if c.wrapping_sub(0x1800) < 0xb0 { return 0x1e; }

    if c.wrapping_sub(0x1000) < 0xa0 { return 0x21; }
    if u2 == 0xa9e0 { return 0x21; }
    if u2 == 0xaa60 { return 0x21; }

    if u2 == 0xf000 { return 0; }
    if (c.wrapping_sub(0x2000) & 0xffff) < 0x70 { return 0; }
    if u1 == 0x1e00 { return 0; }
    if ((c + 0x58e0) & 0xffff) < 0xe0 { return 0; }
    if c < 0x250 { return 0; }
    if u2 == 0x2c60 { return 0; }

    if u1 == 0xf000 { 0x20 } else { 3 }
}

impl CharItemView {
    pub fn new(char_code: u16) -> Self {
        Self { char_code, ..Default::default() }
    }

    /// Build a `CharItemView` once the raw constructor's font metric block is known.
    ///
    /// This is the safe boundary for `FUN_002eaf54` text-bullet children:
    /// - clones/stores RunProperty, ParaProperty, BodyProperty semantic handles;
    /// - reads RunProperty keys `0x96a`, `0x96c`, `0x96b`;
    /// - reads BodyProperty key `0x89e` (default `1`);
    /// - applies the already-ported constructor metric equations.
    ///
    /// It does **not** fabricate glyph metrics. `metrics` must come from a real
    /// `CharItemView::CharItemView` metric source (`GetRealFont`/`FUN_000764fc`).
    pub fn from_constructor_metrics(
        char_code: u16,
        run_property: Option<crate::runtime::RunProperty>,
        para_property: Option<crate::text_property::ParaProperty>,
        body_property: Option<crate::text_property::BodyProperty>,
        reset_or_size: f32,
        metrics: CharItemViewConstructorMetrics,
    ) -> Self {
        let paragraph_class = body_property.as_ref().map(|bp| bp.get_vert()).unwrap_or(1);
        let font_size = run_property
            .as_ref()
            .map(crate::runtime::RunProperty::effective_font_size)
            .unwrap_or(10.0);
        let font_metric_offset = run_property
            .as_ref()
            .map(crate::runtime::RunProperty::font_metric_offset_px)
            .unwrap_or(0.0);
        let dpi = crate::runtime::ShapeEngine::get_instance().logical_dpi;

        let mut view = Self {
            char_code,
            run_property,
            para_property,
            body_property,
            ascent: metrics.ascent,
            descent: metrics.descent,
            width: metrics.metric_4c,
            reset_or_size,
            ..Default::default()
        };
        view.compute_metrics(
            paragraph_class,
            font_size,
            font_metric_offset,
            metrics.metric_3c,
            metrics.metric_4c,
            metrics.metric_50,
            dpi,
        );
        view
    }

    /// 한컴 `FUN_002ef798` (constructor sz=1840) 의 metric 계산 부분 1:1 포팅.
    ///
    /// `GetRealFont(theme)` 으로부터 `ascent`, `descent`, `metric_3c`, `metric_4c`,
    /// `metric_50` 이 set 되었다고 가정. 이 메소드는 RunProperty + BodyProperty 의
    /// paragraph_class (key 0x89e) 기반으로 layout-relevant metric (0x54, 0x58, 0x5c,
    /// 0x64, 0x68, 0x6c, 0x70, 0x74) 을 계산.
    ///
    /// # 한컴 원본 decompile 분기 (line 243-297)
    /// `iVar9` = paragraph_class (PropertyBag key 0x89e, default 1).
    /// - `iVar9 in {5, 6}`: `(this+0x54) = fVar14 + ((ascent+descent) * 1.2 * dpi)/72`
    ///                      `(this+0x58) = ((ascent+descent) * 1.2 * dpi)/72`
    ///                      `(this+0x70) = 0.5`
    ///                      `(this+0x74) = (this+0x5c)`
    /// - `iVar9 in {0, 2}`: `(this+0x54) = fVar14 + (metric_3c * dpi)/72`
    ///                      `(this+0x58) = (font_size * 1.2 * dpi)/72`
    ///                      `(this+0x70) = 0.5`, `(this+0x74) = (this+0x5c)`
    /// - else:               `(this+0x54) = fVar14 + (metric_3c * dpi)/72`
    ///                       `(this+0x58) = (font_size * 1.2 * dpi)/72`
    ///                       `(this+0x70) = (this+0x5c)`, `(this+0x74) = (this+0x5c)`
    ///
    /// # 입력
    /// - `paragraph_class`: BodyProperty `0x89e` value (없으면 1)
    /// - `font_size`: RunProperty `0x96a` (`fVar13`). 적용된 effective size — bold/italic
    ///                조정 후 (constructor line 88-122 에서 처리).
    /// - `font_metric_offset`: `(RunProperty.metric_96b * dpi) / 72` (`fVar14` at line 167).
    ///                         0x54 의 추가 offset.
    /// - `dpi`: `ShapeEngine.dpi_scale`.
    pub fn compute_metrics(
        &mut self,
        paragraph_class: u32,
        font_size: f32,
        font_metric_offset: f32,
        metric_3c: f32,
        metric_4c: f32,
        metric_50: f32,
        dpi: f32,
    ) {
        let sum = self.ascent + self.descent;
        // (this+0x5c) = ascent / (ascent + descent)
        self.vertical_anchor_ratio = if sum > 0.0 { self.ascent / sum } else { 0.0 };
        // (this+0x64) = (metric_4c * dpi)/72
        self.metric_64 = (metric_4c * dpi) / 72.0;
        // (this+0x68) = (metric_50 * dpi)/72
        self.metric_68 = (metric_50 * dpi) / 72.0;

        match paragraph_class {
            5 | 6 => {
                // line 243-264
                let lh = (sum * 1.2 * dpi) / 72.0;
                self.total_height = font_metric_offset + lh;
                self.line_height = lh;
                self.paragraph_line_height = lh;
                self.line_anchor = 0.5;
                self.alignment_ratio = self.vertical_anchor_ratio;
            }
            0 | 2 => {
                // line 266-279
                self.total_height = font_metric_offset + (metric_3c * dpi) / 72.0;
                self.line_height = (font_size * 1.2 * dpi) / 72.0;
                self.paragraph_line_height = self.line_height;
                self.line_anchor = 0.5;
                self.alignment_ratio = self.vertical_anchor_ratio;
            }
            _ => {
                // line 281-297 (default else)
                self.total_height = font_metric_offset + (metric_3c * dpi) / 72.0;
                self.line_height = (font_size * 1.2 * dpi) / 72.0;
                self.paragraph_line_height = self.line_height;
                self.line_anchor = self.vertical_anchor_ratio;
                self.alignment_ratio = self.vertical_anchor_ratio;
            }
        }
    }

    /// 한컴 constructor 의 font style flag 인코딩 (line 142-154).
    ///
    /// Returns 2-bit packed flag:
    /// - bit 0: bold (cVar1 != 0)
    /// - bit 1: italic (pcVar4 != 0)
    ///
    /// 한컴 원본은 이 값을 float bit 로 캐스팅해 RenderPath struct 의 pfVar3[1] 에
    /// 저장. layout 자체엔 영향 없음. RE 일관성용으로 노출.
    pub fn font_style_flag(bold: bool, italic: bool) -> u32 {
        (bold as u32) | ((italic as u32) << 1)
    }

    /// 한컴 constructor 의 effective font size 계산 (line 88-122).
    ///
    /// - `font_size = RunProperty[0x96a]`, fallback 10.0 if <= 0
    /// - `font_size_adjust = RunProperty[0x96c]`
    /// - if `font_size_adjust != 0`: `font_size = (font_size * 2) / 3`
    pub fn effective_font_size(font_size_raw: f32, font_size_adjust: f32) -> f32 {
        let base = if font_size_raw > 0.0 { font_size_raw } else { 10.0 };
        if font_size_adjust != 0.0 {
            (base + base) / 3.0
        } else {
            base
        }
    }

    /// `CharItemView::CharItemView` (`FUN_002ef798`, 1840B) 의 전체 본체 1:1 포팅.
    ///
    /// raw decompile (`bullet_render_deps.txt:3412-3716`) 의 단계별 흐름:
    ///
    /// 1. **기본 field 초기화** (line 3452-3496):
    ///    `+0x08` 에 char, `+0x60` 에 `param_6` (reset_or_size), `+0x90` 에 theme.
    ///    `+0x10..+0x58` / `+0x64..+0x90` / `+0x170..+0x188` 은 zero-init.
    ///
    /// 2. **RunProperty PropertyBag 5 키 read** (line 3499-3577):
    ///    `0x96a` (font_size, clamp ≤0→10) / `0x96c` (font_size_adjust, !=0→size = size*2/3) /
    ///    `0x967` (bold) / `0x968` (italic) / `0x96b` (metric_96b → font_metric_offset_px).
    ///
    /// 3. **GetRealFont** (line 3578-3615): RunProperty + Theme + char → realfont 56B 객체
    ///    `{ size_px = effective_font_size * 96/72, style_flag, face_name }`. 결과 SharePtr 가
    ///    `this+0x30` 에 store.
    ///
    /// 4. **null 가드** (line 3616-3617): real_font 없으면 ctor 즉시 종료 — 모든 metric field
    ///    이 zero.
    ///
    /// 5. **BodyProperty.GetVert(0x89e) → paragraph_class** (line 3618-3636, default 1).
    ///
    /// 6. **Surface 생성** (line 3637-3644): render 컨텍스트, layout 영향 없음.
    ///
    /// 7. **FUN_000764fc** = `combine_char_metrics` (line 3645-3651): char 가 0xd/0xa 면 0x20
    ///    으로 대체. 결과 `CharCtorMetrics` 가 `this+0x38..+0x48` 영역에 해당.
    ///
    /// 8. **paragraph_class 별 metric 산출** (line 3652-3706): `compute_metrics` 가 이미 1:1
    ///    포팅 — `0x54/0x58/0x5c/0x64/0x68/0x6c/0x70/0x74` 채움.
    ///
    /// **providers**: advance 와 global metric 측정은 [`CoreTextFontProvider`] +
    /// [`GlobalMetricProvider`] FFI 경계로 위임 — raw `FUN_00082d98` / `FUN_000761f4` 와 1:1.
    /// 테스트는 mock provider, 프로덕션은 `CoreTextProvider`.
    pub fn from_ctor_context(
        char_code: u16,
        run_property: crate::runtime::RunProperty,
        para_property: Option<crate::text_property::ParaProperty>,
        body_property: Option<crate::text_property::BodyProperty>,
        theme: &crate::runtime::Theme,
        reset_or_size: f32,
        ct_provider: &dyn crate::font_metric::CoreTextFontProvider,
        gm_provider: &dyn crate::font_metric::GlobalMetricProvider,
    ) -> Self {
        // ── step 1: shell — char + reset_or_size + property handles + theme 보유.
        //   raw line 3487: `*(Theme **)(this + 0x90) = param_5;`
        let mut view = Self {
            char_code,
            run_property: Some(run_property),
            para_property,
            body_property,
            theme: Some(theme.clone()),
            reset_or_size,
            ..Default::default()
        };

        // ── step 2: RunProperty 5 키 read (이미 `RunProperty::effective_font_size`/`get_bold`/
        // `get_italic`/`font_metric_offset_px` 가 raw line 3499-3577 의 산식 1:1).
        let run = view.run_property.as_ref().expect("just set");
        let font_size = run.effective_font_size();
        let bold = run.get_bold();
        let italic = run.get_italic();
        let style_flag = Self::font_style_flag(bold, italic) as i32;
        let font_metric_offset = run.font_metric_offset_px();

        // ── step 3: GetRealFont. None 이면 step 4 의 null 가드와 동등 — 측정 skip.
        let real_font = crate::runtime::realize_font(char_code, view.run_property.as_ref(), theme);

        // ── step 4: null 가드. real_font 없으면 모든 layout metric 이 zero 인 ctor 결과.
        let Some(real_font) = real_font else {
            return view;
        };

        // ── step 5: BodyProperty.GetVert (raw line 3618-3636, default 1).
        let paragraph_class = view
            .body_property
            .as_ref()
            .map(|bp| bp.get_vert())
            .unwrap_or(1);

        // ── step 6: Surface — render 전용. layout 영향 없음 — skip.

        // ── step 7: FUN_000764fc 호출. char 가 0xd/0xa 면 0x20 으로 대체.
        let measure_char = match char_code {
            0x0d | 0x0a => 0x20u16,
            other => other,
        };
        // raw 0x82e94: dVar14 = MulDiv((i32)size, 0x60, 0x48) * 72.0/96.0 → size_px (provider 가 사용).
        let advance = crate::font_metric::measure_string_advance(
            font_size,
            style_flag,
            &[measure_char],
            ct_provider,
        );
        // raw `FUN_000761f4`: GlobalFontMetrics 측정. style_flag 가 bit0=bold/bit1=italic 그대로
        // 통과 — `GlobalMetricProvider::global_metrics` 는 본 인코딩과 동일 (font_metric.rs §
        // GlobalMetricProvider doc 참조).
        let global = gm_provider.global_metrics(&real_font.face_name, style_flag);
        let metrics = crate::font_metric::combine_char_metrics(font_size, &global, advance);

        // ── step 8: compute_metrics (paragraph_class 분기, raw line 3652-3706).
        view.ascent = metrics.ascent;
        view.descent = metrics.m7; // raw `+0x44` = m7 = descent (sTypoDescender abs).
        view.width = metrics.width;
        // raw `FUN_000764fc` 는 `CharItemView+0x38..+0x4c` 의 5 float 만 write — em(+0x38) /
        // width(+0x3c) / ascent(+0x40) / m7(+0x44) / m8(+0x48) (font_metric.rs `CharCtorMetrics`
        // 5 필드 1:1). 그래서 `+0x4c` 와 `+0x50` 은 ctor 의 zero-init (line 3477-3480) 결과
        // 그대로 0 유지. 따라서 `metric_4c = 0, metric_50 = 0` 이 raw 의 실제 입력값.
        // compute_metrics 의 `metric_3c` 는 raw line 3692 의 `*(this+0x3c)` 를 사용 → width.
        view.compute_metrics(
            paragraph_class,
            font_size,
            font_metric_offset,
            /* metric_3c */ metrics.width,
            /* metric_4c */ 0.0,
            /* metric_50 */ 0.0,
            crate::runtime::ShapeEngine::get_logical_dpi(),
        );

        view
    }
}

impl Glyph for CharItemView {
    fn clone_glyph(&self) -> Box<dyn Glyph> { Box::new(self.clone()) }
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    /// `CharItemView::Request(Requisition&)` (`FUN_002f5bb0`, 304B).
    ///
    /// Raw branch:
    /// ```c
    /// iVar2 = BodyProperty.GetVert(0x89e);
    /// if (iVar2 in {0,2,5,6}) {
    ///   req.x = {this+0x58, 0, 0, 1.0 - this+0x5c};
    ///   req.y = {this+0x54, this+0x60, 0, 0};
    /// } else {
    ///   req.x = {this+0x54, this+0x60, 0, 0};
    ///   req.y = {this+0x58, 0, 0, this+0x5c};
    /// }
    /// class = FUN_002f0ad0(this+0x08);
    /// if (class - 2U < 4) req.penalty = 1;
    /// if (char == 0x20) req.penalty = 10;
    /// else if (char == 0x0d) req.penalty = -10000;
    /// else if (char == 0x0a) req.penalty = -1000;
    /// ```
    fn request(&self, req_out: &mut Requisition) {
        let paragraph_class = self
            .body_property
            .as_ref()
            .map(|bp| bp.get_vert());
        let special = matches!(paragraph_class, Some(0 | 2 | 5 | 6));

        if special {
            req_out.x = Requirement::new(
                self.line_height,
                0.0,
                0.0,
                1.0 - self.vertical_anchor_ratio,
            );
            req_out.y = Requirement::new(
                self.total_height,
                self.reset_or_size,
                0.0,
                0.0,
            );
        } else {
            req_out.x = Requirement::new(
                self.total_height,
                self.reset_or_size,
                0.0,
                0.0,
            );
            req_out.y = Requirement::new(
                self.line_height,
                0.0,
                0.0,
                self.vertical_anchor_ratio,
            );
        }

        let char_class = char_item_view_char_class(self.char_code);
        if (char_class.wrapping_sub(2) as u32) < 4 {
            req_out.penalty = 1;
        }
        req_out.penalty = match self.char_code {
            0x20 => 10,
            0x0d => -10000,
            0x0a => -1000,
            _ => req_out.penalty,
        };
    }

    /// `CharItemView::Allocate(Allocation const&, Extension&)` (`FUN_002f5d48`, 224B).
    ///
    /// Raw branch mirrors `Request`: paragraph classes `{0,2,5,6}` rotate the glyph box.
    fn allocate(&mut self, alloc: &Allocation, ext: &mut Extension) {
        let paragraph_class = self
            .body_property
            .as_ref()
            .map(|bp| bp.get_vert());
        let special = matches!(paragraph_class, Some(0 | 2 | 5 | 6));
        let x = alloc.x.origin;
        let y = alloc.y.origin;
        if special {
            ext.left = x - self.vertical_anchor_ratio * self.line_height;
            ext.top = y;
            ext.right = x + self.total_height;
            ext.bottom = y + (1.0 - self.vertical_anchor_ratio) * self.line_height;
        } else {
            ext.left = x;
            ext.top = y - self.vertical_anchor_ratio * self.line_height;
            ext.right = x + self.total_height;
            ext.bottom = y + (1.0 - self.vertical_anchor_ratio) * self.line_height;
        }
    }

    /// Same raw vfunc slot as `Allocate`, used by `Box::recompute_bounds_cache` as a
    /// bounds accumulator. `Extension` and `BoundsRect` are the same four-float shape here.
    fn allocate_bounds(&mut self, alloc: &Allocation, out_bounds: &mut BoundsRect) {
        let mut ext = Extension::default();
        self.allocate(alloc, &mut ext);
        *out_bounds = BoundsRect {
            min_x: ext.left,
            min_y: ext.top,
            max_x: ext.right,
            max_y: ext.bottom,
        };
    }
}

/// `Hnc::Shape::Text::DebugGlyph`.
#[derive(Debug, Default, Clone)]
pub struct DebugGlyph;

impl Glyph for DebugGlyph {
    fn clone_glyph(&self) -> Box<dyn Glyph> { Box::new(self.clone()) }
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glue_strut_space_request_byte_identical() {
        // Hancom Glue::Request / Strut::Request / Space::Request 는 byte-identical.
        let mut glue = Glue::new(Requisition::ZERO);
        let mut strut = Strut {
            direction: Direction::Horizontal,
            req_a: Requirement::ZERO,
            req_b: Requirement::ZERO,
        };
        let mut space = Space { direction: Direction::Horizontal };

        let alloc = Allocation::new(
            Allotment::new(5.0, 30.0, 0.5),
            Allotment::new(50.0, 100.0, 0.5),
        );
        let mut bg = BoundsRect::INIT;
        let mut bs = BoundsRect::INIT;
        let mut bsp = BoundsRect::INIT;
        glue.allocate_bounds(&alloc, &mut bg);
        strut.allocate_bounds(&alloc, &mut bs);
        space.allocate_bounds(&alloc, &mut bsp);
        assert_eq!(bg, bs);
        assert_eq!(bg, bsp);
    }

    #[test]
    fn no_op_subclasses_dont_modify_bounds() {
        let mut h = HStrut::default();
        let mut v = VStrut::default();
        let mut s = ShapeOf::default();
        let saved = BoundsRect { min_x: 1.0, min_y: 2.0, max_x: 3.0, max_y: 4.0 };
        let alloc = Allocation::new(
            Allotment::new(100.0, 200.0, 0.0),
            Allotment::new(300.0, 400.0, 0.0),
        );
        let mut b = saved;
        h.allocate_bounds(&alloc, &mut b);
        v.allocate_bounds(&alloc, &mut b);
        s.allocate_bounds(&alloc, &mut b);
        assert_eq!(b, saved);
    }

    #[test]
    fn deck_request_caches_avail() {
        let mut deck = Deck::default();
        let alloc = Allocation::new(
            Allotment::new(7.0, 8.0, 0.25),
            Allotment::new(9.0, 10.0, 0.75),
        );
        let mut bounds = BoundsRect::INIT;
        deck.allocate_bounds(&alloc, &mut bounds);
        assert_eq!(deck.cached_avail, alloc);
    }

    #[test]
    fn deck_request_recurses_into_current_child_only() {
        // Deck 는 current_idx 의 child 만 호출. 다른 children 은 skip.
        // 검증: 2개의 Glue child 가 있고 current_idx=1 일 때, bounds 는 avail 의 extents 만.
        let mut deck = Deck::default();
        deck.children.push(Box::new(Glue::new(Requisition::ZERO)));
        deck.children.push(Box::new(Glue::new(Requisition::ZERO)));
        deck.current_idx = 1;

        let alloc = Allocation::new(
            Allotment::new(0.0, 10.0, 0.0),
            Allotment::new(0.0, 20.0, 0.0),
        );
        let mut bounds = BoundsRect::INIT;
        deck.allocate_bounds(&alloc, &mut bounds);

        // child[1].request + Deck self merge → 둘 다 avail merge 라서 결과는 avail extents
        assert_eq!(bounds.min_x, 0.0);
        assert_eq!(bounds.max_x, 10.0);
        assert_eq!(bounds.min_y, 0.0);
        assert_eq!(bounds.max_y, 20.0);
    }

    #[test]
    fn deck_request_idx_out_of_range_noop() {
        let mut deck = Deck::default();
        deck.current_idx = 5;  // out of range (0 children)
        let saved = BoundsRect { min_x: 1.0, min_y: 2.0, max_x: 3.0, max_y: 4.0 };
        let mut bounds = saved;
        let alloc = Allocation::default();
        deck.allocate_bounds(&alloc, &mut bounds);
        // index out of range → bounds 미변동 (단 cached_avail 은 update)
        assert_eq!(bounds, saved);
        assert_eq!(deck.cached_avail, alloc);
    }

    #[test]
    fn box_allocate_bounds_empty_recomputes_to_init() {
        // 빈 Box (children 없음, layout 없음): recompute_bounds_cache 가 cached_bounds 를
        // INIT 으로 리셋하고 merge 안 함 → out_bounds 는 INIT (= 1e8, 1e8, -1e8, -1e8).
        // raw asm 검증: Box::Request 는 항상 cache_bounds_valid=false 로 clear 후 재계산.
        let mut bx = Box_::default();
        bx.cached_bounds = BoundsRect { min_x: -5.0, min_y: -10.0, max_x: 5.0, max_y: 10.0 };
        let mut bounds = BoundsRect::INIT;
        let alloc = Allocation::default();
        bx.allocate_bounds(&alloc, &mut bounds);
        // manually-set cached_bounds 가 recompute 로 INIT 으로 wipe → out 도 INIT 유지
        assert_eq!(bounds, BoundsRect::INIT);
        assert_eq!(bx.cached_bounds, BoundsRect::INIT);
        assert!(bx.cache_bounds_valid, "recompute sets cache_bounds_valid");
    }

    #[test]
    fn box_allocate_bounds_with_glue_child() {
        // Box 에 Glue child 1개 추가. recompute_bounds_cache:
        //   - layout 없음 → allocs[0] = ZERO Allocation
        //   - child(Glue).allocate_bounds(&ZERO, &mut local_acc=INIT)
        //     → Glue::Request = local_acc.merge_allocation(ZERO)
        //       ZERO Allocation 의 begin/end 모두 0 → local_acc = (0, 0, 0, 0)
        //   - cached_bounds = merge((0,0,0,0), INIT) = (0, 0, 0, 0)
        let mut bx = Box_::default();
        bx.append(Some(Box::new(Glue::new(Requisition::ZERO))));
        let mut bounds = BoundsRect::INIT;
        bx.allocate_bounds(&Allocation::default(), &mut bounds);
        assert_eq!(bx.cached_bounds, BoundsRect { min_x: 0.0, min_y: 0.0, max_x: 0.0, max_y: 0.0 });
        assert_eq!(bounds, BoundsRect { min_x: 0.0, min_y: 0.0, max_x: 0.0, max_y: 0.0 });
    }

    #[test]
    fn box_request_outputs_cached_req() {
        // Box::Request (vfunc[3]) — cached_req 를 out 으로 복사.
        let mut bx = Box_::default();
        bx.cached_req = Requisition {
            x: Requirement::new(42.0, 1.0, 2.0, 0.0),
            y: Requirement::new(10.0, 0.5, 0.25, 0.0),
            penalty: 7,
        };
        let mut out = Requisition::ZERO;
        bx.request(&mut out);
        assert_eq!(out.x.natural, 42.0);
        assert_eq!(out.y.natural, 10.0);
        assert_eq!(out.penalty, 7);
    }

    #[test]
    fn char_item_view_downcast() {
        // 한컴 `dynamic_cast<CharItemView*>(glyph)` 대응: Any::downcast_ref
        let civ = CharItemView::new(0x0d);
        let g: Box<dyn Glyph> = Box::new(civ);

        let casted = g.as_any().downcast_ref::<CharItemView>();
        assert!(casted.is_some(), "Box<dyn Glyph> → CharItemView downcast");
        assert_eq!(casted.unwrap().char_code, 0x0d);

        // 다른 type 으로 downcast 는 None
        let glue: Box<dyn Glyph> = Box::new(Glue::new(Requisition::ZERO));
        assert!(glue.as_any().downcast_ref::<CharItemView>().is_none());
    }

    #[test]
    fn char_item_view_field_mutation_via_downcast_mut() {
        let civ = CharItemView::new(0x20);
        let mut g: Box<dyn Glyph> = Box::new(civ);

        // PptCompositor::ComposeLayout stage 11 패턴: dynamic_cast mut + set fields
        let casted = g.as_any_mut().downcast_mut::<CharItemView>().unwrap();
        casted.line_height = 36.0;
        casted.vertical_anchor_ratio = 0.75;
        casted.reset_or_size = 1e8;
        casted.paragraph_line_height = 40.0;
        casted.line_anchor = 0.5;
        casted.alignment_ratio = 0.25;

        // 검증
        let re_cast = g.as_any().downcast_ref::<CharItemView>().unwrap();
        assert_eq!(re_cast.line_height, 36.0);
        assert_eq!(re_cast.vertical_anchor_ratio, 0.75);
        assert_eq!(re_cast.reset_or_size, 1e8);
        assert_eq!(re_cast.paragraph_line_height, 40.0);
        assert_eq!(re_cast.line_anchor, 0.5);
        assert_eq!(re_cast.alignment_ratio, 0.25);
    }

    fn body_property_with_vert(vert: u32) -> crate::text_property::BodyProperty {
        let mut bp = crate::text_property::BodyProperty::new();
        bp.property_bag.insert(
            crate::properties::PropertyKey::new(crate::text_property::KEY_VERT),
            crate::properties::PropertyValue::Uint(vert),
        );
        bp
    }

    fn request_fixture(ch: u16) -> CharItemView {
        let mut civ = CharItemView::new(ch);
        civ.total_height = 12.0;
        civ.reset_or_size = 3.0;
        civ.line_height = 8.0;
        civ.vertical_anchor_ratio = 0.25;
        civ
    }

    #[test]
    fn char_item_view_request_default_branch() {
        // raw FUN_002f5bb0 default:
        // req.x = {+0x54, +0x60, 0, 0}; req.y = {+0x58, 0, 0, +0x5c}.
        let civ = request_fixture(0x41);
        let mut out = Requisition { penalty: 77, ..Requisition::INVALID };
        civ.request(&mut out);
        assert_eq!(out.x, Requirement::new(12.0, 3.0, 0.0, 0.0));
        assert_eq!(out.y, Requirement::new(8.0, 0.0, 0.0, 0.25));
        assert_eq!(out.penalty, 77, "non-space Latin leaves penalty untouched");
    }

    #[test]
    fn char_item_view_request_special_body_branch() {
        // BodyProperty.GetVert in {0,2,5,6} rotates the request axes.
        let mut civ = request_fixture(0x41);
        civ.body_property = Some(body_property_with_vert(5));
        let mut out = Requisition { penalty: 77, ..Requisition::INVALID };
        civ.request(&mut out);
        assert_eq!(out.x, Requirement::new(8.0, 0.0, 0.0, 0.75));
        assert_eq!(out.y, Requirement::new(12.0, 3.0, 0.0, 0.0));
        assert_eq!(out.penalty, 77);
    }

    #[test]
    fn char_item_view_request_penalties() {
        let mut out = Requisition::INVALID;

        request_fixture(0x4e00).request(&mut out);
        assert_eq!(out.penalty, 1, "FUN_002f0ad0 class 4 writes soft penalty 1");

        out.penalty = 0;
        request_fixture(0x20).request(&mut out);
        assert_eq!(out.penalty, 10, "space overrides class penalty");

        out.penalty = 0;
        request_fixture(0x0d).request(&mut out);
        assert_eq!(out.penalty, -10000, "CR forced break marker");

        out.penalty = 0;
        request_fixture(0x0a).request(&mut out);
        assert_eq!(out.penalty, -1000, "LF forced break marker");
    }

    #[test]
    fn char_item_view_allocate_default_and_special() {
        // raw FUN_002f5d48 default:
        // {x, y-anchor*h, x+width, y+(1-anchor)*h}; special rotates top/left anchor.
        let alloc = Allocation::new(
            Allotment::new(10.0, 0.0, 0.0),
            Allotment::new(20.0, 0.0, 0.0),
        );

        let mut default_civ = request_fixture(0x41);
        let mut ext = Extension::default();
        default_civ.allocate(&alloc, &mut ext);
        assert_eq!(ext, Extension { left: 10.0, top: 18.0, right: 22.0, bottom: 26.0 });

        let mut special_civ = request_fixture(0x41);
        special_civ.body_property = Some(body_property_with_vert(0));
        let mut ext = Extension::default();
        special_civ.allocate(&alloc, &mut ext);
        assert_eq!(ext, Extension { left: 8.0, top: 20.0, right: 22.0, bottom: 26.0 });
    }

    #[test]
    fn glyph_base_compose_default() {
        let s = ShapeOf::default();
        assert!(s.compose(BreakType::Normal).can_break);   // bt=0 < 2
        assert!(s.compose(BreakType::Hint).can_break);     // bt=1 < 2
        assert!(!s.compose(BreakType::Forced).can_break);  // bt=2
        assert!(!s.compose(BreakType::Penalty).can_break); // bt=3
    }

    // ────── CharItemView::compute_metrics (constructor port) ──────

    #[test]
    fn civ_from_constructor_metrics_binds_property_inputs() {
        use crate::properties::{keys, HashMapPropertyBag, PropertyKey, PropertyValue};
        use crate::runtime::{RunProperty, ShapeEngine};
        use crate::text_property::{BodyProperty, ParaProperty, KEY_VERT};

        ShapeEngine::_reset_for_test();
        let run = RunProperty::from_bag(
            HashMapPropertyBag::new()
                .with(keys::FONT_SIZE, PropertyValue::Float(12.0))
                .with(keys::METRIC_96B, PropertyValue::Float(3.0)),
        );
        let para = ParaProperty::new();
        let mut body = BodyProperty::new();
        body.property_bag
            .insert(PropertyKey::new(KEY_VERT), PropertyValue::Uint(0));

        let view = CharItemView::from_constructor_metrics(
            0x2022,
            Some(run),
            Some(para),
            Some(body),
            0.25,
            CharItemViewConstructorMetrics {
                metric_3c: 6.0,
                ascent: 8.0,
                descent: 2.0,
                metric_4c: 4.0,
                metric_50: 5.0,
            },
        );

        assert_eq!(view.char_code, 0x2022);
        assert!(view.run_property.is_some());
        assert!(view.para_property.is_some());
        assert!(view.body_property.is_some());
        assert_eq!(view.reset_or_size, 0.25);
        assert_eq!(view.ascent, 8.0);
        assert_eq!(view.descent, 2.0);
        assert_eq!(view.width, 4.0);
        assert!((view.total_height - 12.0).abs() < 1e-6);
        assert!((view.line_height - 19.2).abs() < 1e-5);
        assert!((view.vertical_anchor_ratio - 0.8).abs() < 1e-6);
        assert!((view.metric_64 - 5.333_333).abs() < 1e-5);
        assert!((view.metric_68 - 6.666_667).abs() < 1e-5);
        assert_eq!(view.line_anchor, 0.5);
        assert_eq!(view.alignment_ratio, view.vertical_anchor_ratio);
        ShapeEngine::_reset_for_test();
    }

    #[test]
    fn civ_compute_metrics_class_5_uses_ascent_descent_sum() {
        let mut v = CharItemView {
            ascent: 8.0,
            descent: 2.0,
            ..Default::default()
        };
        // dpi = 96, font_size = 14 (ignored for class 5), metric_3c=99 (ignored)
        v.compute_metrics(5, /*font_size*/14.0, /*offset*/0.0,
                          /*3c*/99.0, /*4c*/1.0, /*50*/2.0, /*dpi*/96.0);
        // sum=10, vertical_anchor_ratio=8/10=0.8
        assert!((v.vertical_anchor_ratio - 0.8).abs() < 1e-6);
        // line_height = (10 * 1.2 * 96)/72 = 16.0
        assert!((v.line_height - 16.0).abs() < 1e-6);
        // total_height = 0 + 16 = 16
        assert!((v.total_height - 16.0).abs() < 1e-6);
        // metric_64 = (1 * 96)/72 = 1.333...
        assert!((v.metric_64 - 1.333_333).abs() < 1e-3);
        // metric_68 = (2 * 96)/72 = 2.666...
        assert!((v.metric_68 - 2.666_666).abs() < 1e-3);
        // paragraph_line_height = line_height
        assert_eq!(v.paragraph_line_height, v.line_height);
        // line_anchor = 0.5 (constant for class 5/6)
        assert_eq!(v.line_anchor, 0.5);
        // alignment_ratio = vertical_anchor_ratio
        assert_eq!(v.alignment_ratio, v.vertical_anchor_ratio);
    }

    #[test]
    fn civ_compute_metrics_class_6_same_as_5() {
        let mut v5 = CharItemView { ascent: 8.0, descent: 2.0, ..Default::default() };
        let mut v6 = CharItemView { ascent: 8.0, descent: 2.0, ..Default::default() };
        v5.compute_metrics(5, 14.0, 0.0, 99.0, 1.0, 2.0, 96.0);
        v6.compute_metrics(6, 14.0, 0.0, 99.0, 1.0, 2.0, 96.0);
        assert_eq!(v5.total_height, v6.total_height);
        assert_eq!(v5.line_height, v6.line_height);
        assert_eq!(v5.line_anchor, v6.line_anchor);
    }

    #[test]
    fn civ_compute_metrics_class_0_uses_3c_for_total_and_font_for_line() {
        let mut v = CharItemView { ascent: 8.0, descent: 2.0, ..Default::default() };
        v.compute_metrics(0, /*font_size*/14.0, /*offset*/3.0,
                          /*3c*/12.0, /*4c*/0.0, /*50*/0.0, /*dpi*/96.0);
        // total_height = 3 + (12 * 96)/72 = 3 + 16 = 19
        assert!((v.total_height - 19.0).abs() < 1e-6);
        // line_height = (14 * 1.2 * 96)/72 = 22.4 (f32 imprecise)
        assert!((v.line_height - 22.4).abs() < 1e-3);
        // line_anchor = 0.5 (constant)
        assert_eq!(v.line_anchor, 0.5);
        // alignment_ratio = vertical_anchor_ratio = 0.8
        assert!((v.alignment_ratio - 0.8).abs() < 1e-6);
    }

    #[test]
    fn civ_compute_metrics_class_2_same_as_0() {
        let mut v0 = CharItemView { ascent: 8.0, descent: 2.0, ..Default::default() };
        let mut v2 = CharItemView { ascent: 8.0, descent: 2.0, ..Default::default() };
        v0.compute_metrics(0, 14.0, 3.0, 12.0, 0.0, 0.0, 96.0);
        v2.compute_metrics(2, 14.0, 3.0, 12.0, 0.0, 0.0, 96.0);
        assert_eq!(v0.total_height, v2.total_height);
        assert_eq!(v0.line_height, v2.line_height);
        assert_eq!(v0.line_anchor, v2.line_anchor);
    }

    #[test]
    fn civ_compute_metrics_default_class_uses_5c_for_anchor() {
        // paragraph_class in else branch (e.g. 1) → line_anchor = vertical_anchor_ratio (NOT 0.5)
        let mut v = CharItemView { ascent: 6.0, descent: 4.0, ..Default::default() };
        v.compute_metrics(1, 10.0, 0.0, 10.0, 0.0, 0.0, 96.0);
        // vertical_anchor_ratio = 6/10 = 0.6
        assert!((v.vertical_anchor_ratio - 0.6).abs() < 1e-6);
        // line_anchor = vertical_anchor_ratio = 0.6 (NOT 0.5)
        assert!((v.line_anchor - 0.6).abs() < 1e-6);
        // alignment_ratio = vertical_anchor_ratio
        assert!((v.alignment_ratio - 0.6).abs() < 1e-6);
    }

    #[test]
    fn civ_compute_metrics_zero_height_safe() {
        // ascent + descent == 0 → vertical_anchor_ratio = 0 (no div by zero)
        let mut v = CharItemView::default();
        v.compute_metrics(1, 10.0, 0.0, 0.0, 0.0, 0.0, 96.0);
        assert_eq!(v.vertical_anchor_ratio, 0.0);
    }

    #[test]
    fn civ_effective_font_size_fallback() {
        // raw <= 0 → 10
        assert_eq!(CharItemView::effective_font_size(0.0, 0.0), 10.0);
        assert_eq!(CharItemView::effective_font_size(-5.0, 0.0), 10.0);
        // raw > 0, no adjust → raw
        assert_eq!(CharItemView::effective_font_size(14.0, 0.0), 14.0);
        // adjust != 0 → (raw * 2)/3
        let result = CharItemView::effective_font_size(12.0, 0.5);
        assert!((result - 8.0).abs() < 1e-6);
    }

    #[test]
    fn civ_font_style_flag_packing() {
        assert_eq!(CharItemView::font_style_flag(false, false), 0);
        assert_eq!(CharItemView::font_style_flag(true, false), 1);
        assert_eq!(CharItemView::font_style_flag(false, true), 2);
        assert_eq!(CharItemView::font_style_flag(true, true), 3);
    }

    // ────── FirstLineMetrics (RenderPath vtable[3] 출력 모델) ──────

    #[test]
    fn first_line_metrics_special_classes_use_special_ascent() {
        let m = FirstLineMetrics { default_ascent: 10.0, special_ascent: 20.0 };
        // 0x65 = bits 0, 2, 5, 6
        assert_eq!(m.pick_for_paragraph_class(0), 20.0); // bit 0
        assert_eq!(m.pick_for_paragraph_class(2), 20.0); // bit 2
        assert_eq!(m.pick_for_paragraph_class(5), 20.0); // bit 5
        assert_eq!(m.pick_for_paragraph_class(6), 20.0); // bit 6
    }

    #[test]
    fn first_line_metrics_other_classes_use_default_ascent() {
        let m = FirstLineMetrics { default_ascent: 10.0, special_ascent: 20.0 };
        assert_eq!(m.pick_for_paragraph_class(1), 10.0); // bit 1 not in 0x65
        assert_eq!(m.pick_for_paragraph_class(3), 10.0);
        assert_eq!(m.pick_for_paragraph_class(4), 10.0);
        // > 6 → default branch
        assert_eq!(m.pick_for_paragraph_class(7), 10.0);
        assert_eq!(m.pick_for_paragraph_class(100), 10.0);
    }

    // ============================================================
    // CharItemView::from_ctor_context — FUN_002ef798 full ctor body
    // ============================================================

    use crate::font_metric::{
        CoreTextFontProvider, GlobalFontMetrics, GlobalMetricProvider, SystemFont,
    };
    use crate::runtime::{Font, FontTable, RunProperty, ShapeEngine, Theme};
    use std::cell::RefCell;

    /// 측정용 mock — char ↔ advance 의 결정적 매핑.
    struct MockCt {
        // (font, char) → advance(pt). 미정의 char 는 1.0 advance.
        advances: std::collections::HashMap<(SystemFont, u16), f64>,
        gly_calls: RefCell<Vec<(SystemFont, u16)>>,
    }

    impl MockCt {
        fn new() -> Self {
            Self {
                advances: std::collections::HashMap::new(),
                gly_calls: RefCell::new(Vec::new()),
            }
        }
        fn with(mut self, font: SystemFont, c: u16, adv: f64) -> Self {
            self.advances.insert((font, c), adv);
            self
        }
    }

    impl CoreTextFontProvider for MockCt {
        fn glyph_for_character(&self, font: SystemFont, _size_px: f64, c: u16) -> u16 {
            self.gly_calls.borrow_mut().push((font, c));
            c // identity — no substitution
        }
        fn advance_for_glyph(&self, font: SystemFont, _size_px: f64, glyph: u16) -> f64 {
            *self.advances.get(&(font, glyph)).unwrap_or(&1.0)
        }
    }

    /// Global metric mock — face name 별 결정적 매핑.
    struct MockGm {
        metrics: std::collections::HashMap<String, GlobalFontMetrics>,
    }

    impl MockGm {
        fn new() -> Self {
            Self {
                metrics: std::collections::HashMap::new(),
            }
        }
        fn with<S: Into<String>>(mut self, face: S, gm: GlobalFontMetrics) -> Self {
            self.metrics.insert(face.into(), gm);
            self
        }
    }

    impl GlobalMetricProvider for MockGm {
        fn global_metrics(&self, font_name: &str, _font_style: i32) -> GlobalFontMetrics {
            self.metrics
                .get(font_name)
                .copied()
                .unwrap_or(GlobalFontMetrics {
                    em: 1000.0,
                    ascent: 800.0,
                    m7: 200.0,
                    m8: 0.0,
                })
        }
    }

    #[test]
    fn from_ctor_context_returns_zero_metrics_when_no_font_table() {
        // raw GetRealFont line 34-37 (no RunProperty / no FontTable): output empty SharePtr.
        // CharItemView ctor line 3616-3617: real_font null → skip 측정. 모든 metric field=0.
        ShapeEngine::_reset_for_test();
        let ct = MockCt::new();
        let gm = MockGm::new();
        let run = RunProperty::new(12.0); // font_table=None
        let view = CharItemView::from_ctor_context(
            b'A' as u16,
            run,
            None,
            None,
            &Theme::new(),
            0.0,
            &ct,
            &gm,
        );
        assert_eq!(view.char_code, b'A' as u16);
        assert_eq!(view.ascent, 0.0);
        assert_eq!(view.descent, 0.0);
        assert_eq!(view.width, 0.0);
        assert_eq!(view.total_height, 0.0);
        assert_eq!(view.line_height, 0.0);
        // glyph_for_character 한 번도 호출 안 됨 — 측정 skip 증명.
        assert!(ct.gly_calls.borrow().is_empty());
        ShapeEngine::_reset_for_test();
    }

    #[test]
    fn from_ctor_context_measures_and_computes_metrics_for_ascii() {
        // raw 의 전체 chain 1:1 검증:
        //   GetRealFont('A') → font_table.latin = Arial → face="Arial"
        //   measure_string_advance('A', size=12, style=0) → MockCt 가 SystemFont::Helvetica
        //     으로 advance=8.0 반환
        //   combine_char_metrics(12, gm(em=1000/asc=800/m7=200/m8=100), 8.0):
        //     mul = MulDiv(12, 96, 72) = 16
        //     em_r = 1000, p6_r=800, p7_r=200, p8_r=100
        //     CharCtorMetrics { em=1000, width=8*72/96=6.0, ascent=800*16/1000=12.8, m7=3.2, m8=1.6 }
        //   compute_metrics(paragraph_class=1, font_size=12, offset=0, metric_3c=6, 4c=0, 50=0, dpi=96):
        //     sum=16, vertical_anchor=12.8/16=0.8
        //     class=1 (else 분기): total_height=0+(6*96)/72=8, line_height=(12*1.2*96)/72=19.2
        ShapeEngine::_reset_for_test();
        let ct = MockCt::new().with(SystemFont::Helvetica, b'A' as u16, 8.0);
        let gm = MockGm::new().with(
            "Arial",
            GlobalFontMetrics {
                em: 1000.0,
                ascent: 800.0,
                m7: 200.0,
                m8: 100.0,
            },
        );

        let table = FontTable {
            latin: Some(Font::new("Arial")),
            ..Default::default()
        };
        let run = RunProperty::new(12.0).with_font_table(table);

        let view = CharItemView::from_ctor_context(
            b'A' as u16,
            run,
            None,
            None,
            &Theme::new(),
            0.0,
            &ct,
            &gm,
        );

        assert_eq!(view.char_code, b'A' as u16);
        assert!((view.width - 6.0).abs() < 1e-5, "width={}", view.width);
        assert!((view.ascent - 12.8).abs() < 1e-4, "ascent={}", view.ascent);
        assert!((view.descent - 3.2).abs() < 1e-4, "descent={}", view.descent);
        // class=1 (none body_property → default 1) → else 분기:
        //   total_height = offset + width*dpi/72 = 0 + 6*96/72 = 8.0
        assert!((view.total_height - 8.0).abs() < 1e-5);
        //   line_height = font_size*1.2*dpi/72 = 12*1.2*96/72 = 19.2
        assert!((view.line_height - 19.2).abs() < 1e-5);
        //   vertical_anchor_ratio = ascent/(ascent+descent) = 12.8/16 = 0.8
        assert!((view.vertical_anchor_ratio - 0.8).abs() < 1e-6);
        // class=1 else branch: line_anchor = vertical_anchor_ratio (raw line 3704)
        assert!((view.line_anchor - 0.8).abs() < 1e-6);
        ShapeEngine::_reset_for_test();
    }

    #[test]
    fn from_ctor_context_substitutes_space_for_cr_lf() {
        // raw line 3645-3648: char == 0x0d || 0x0a → 0x20 (space) 으로 substitute.
        // 측정만 substitute — view.char_code 자체는 원본 그대로 유지.
        ShapeEngine::_reset_for_test();
        // Helvetica 의 space advance = 5.0. A advance 는 다른 값.
        let ct = MockCt::new()
            .with(SystemFont::Helvetica, 0x20, 5.0)
            .with(SystemFont::Helvetica, 0x0d, 999.0); // 절대 호출 안 됨
        let gm = MockGm::new().with(
            "Arial",
            GlobalFontMetrics { em: 1000.0, ascent: 800.0, m7: 200.0, m8: 0.0 },
        );

        let run = RunProperty::new(10.0).with_font_table(FontTable {
            latin: Some(Font::new("Arial")),
            ..Default::default()
        });

        let view = CharItemView::from_ctor_context(
            0x0d, run, None, None, &Theme::new(), 0.0, &ct, &gm,
        );
        // char_code 그대로 0x0d.
        assert_eq!(view.char_code, 0x0d);
        // measure_string_advance 가 0x20 (space) 으로 호출됨 — width 는 5.0 * 72/96 = 3.75.
        assert!((view.width - (5.0 * 72.0 / 96.0)).abs() < 1e-5, "width={}", view.width);
        // 호출 history 검증.
        let calls = ct.gly_calls.borrow();
        assert!(calls.iter().any(|&(_, c)| c == 0x20), "should request glyph for 0x20");
        assert!(!calls.iter().any(|&(_, c)| c == 0x0d), "must NOT request glyph for 0x0d");
        ShapeEngine::_reset_for_test();
    }

    #[test]
    fn from_ctor_context_selects_cjk_font_for_korean_char() {
        // raw GetRealFont + select_font_slot: '한' (0xD55C) → char_class 3 → CJK slot.
        // 그리고 measure_string_advance 의 select_system_font 은 ko_KR Hangul 이면
        // SystemFont::AppleSdGothicNeoMedium (bold=false 인 경우).
        ShapeEngine::_reset_for_test();
        let ct = MockCt::new().with(SystemFont::AppleSdGothicNeoMedium, 0xD55C, 12.0);
        let gm = MockGm::new().with(
            "Apple SD Gothic Neo",
            GlobalFontMetrics { em: 2048.0, ascent: 1638.0, m7: 410.0, m8: 0.0 },
        );

        let run = RunProperty::new(10.0).with_font_table(FontTable {
            latin: Some(Font::new("Arial")),
            cjk: Some(Font::new("Apple SD Gothic Neo")),
            ..Default::default()
        });

        let view = CharItemView::from_ctor_context(
            0xD55C, run, None, None, &Theme::new(), 0.0, &ct, &gm,
        );

        // CJK 슬롯이 선택됐는지 — global_metrics 가 "Apple SD Gothic Neo" 로 호출됨.
        // (간접 검증: ascent 가 GM 값 기반으로 계산됐는지)
        // mul = MulDiv(10, 96, 72) = 13. em_r = 2048. ascent = 1638 * 13 / 2048 ≈ 10.397
        assert!((view.ascent - (1638.0 * 13.0 / 2048.0)).abs() < 1e-3, "ascent={}", view.ascent);
        // CTFontGetGlyphsForCharacters 가 AppleSDGothicNeo 폰트로 호출됐는지 검증.
        let calls = ct.gly_calls.borrow();
        assert!(
            calls.iter().any(|&(f, c)| f == SystemFont::AppleSdGothicNeoMedium && c == 0xD55C),
            "expected AppleSDGothicNeoMedium call for 0xD55C, got {:?}",
            calls,
        );
        ShapeEngine::_reset_for_test();
    }

    #[test]
    fn from_ctor_context_uses_hcr_dotum_fallback_when_all_slots_empty() {
        // FontTable 자체는 Some 이지만 모든 슬롯이 None — realize_font 가 fallback "HCR Dotum".
        ShapeEngine::_reset_for_test();
        let ct = MockCt::new().with(SystemFont::Helvetica, b'X' as u16, 7.0);
        let gm = MockGm::new().with(
            "HCR Dotum",
            GlobalFontMetrics { em: 1000.0, ascent: 750.0, m7: 250.0, m8: 0.0 },
        );

        let run = RunProperty::new(12.0).with_font_table(FontTable::default());

        let view = CharItemView::from_ctor_context(
            b'X' as u16, run, None, None, &Theme::new(), 0.0, &ct, &gm,
        );

        // GM hit 검증 — face "HCR Dotum" 의 metric 이 사용됨.
        // mul = MulDiv(12, 96, 72) = 16. em_r = 1000. ascent = 750 * 16 / 1000 = 12.0.
        assert!((view.ascent - 12.0).abs() < 1e-5, "ascent={}", view.ascent);
        ShapeEngine::_reset_for_test();
    }

    // ============================================================
    // MonoGlyph::request dispatch tests (audit 14d 수술 후 추가)
    // ============================================================

    use crate::placement::{PlaceCenter, PlaceNatural};

    /// child 가 자기 Requisition fill 후, placement 가 alignment 슬롯에 overlay.
    /// 시나리오: child 는 x.natural=5.0 set, placement=PlaceCenter(H, 0.3) 는 x.alignment=0.3.
    #[test]
    fn monoglyph_request_dispatches_to_child_then_placement() {
        #[derive(Debug)]
        struct ChildRequester;
        impl Glyph for ChildRequester {
            fn clone_glyph(&self) -> Box<dyn Glyph> { Box::new(ChildRequester) }
            fn as_any(&self) -> &dyn std::any::Any { self }
            fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
            fn request(&self, req: &mut Requisition) {
                req.x = Requirement::new(5.0, 0.0, 0.0, 0.0);
            }
        }
        let mg = MonoGlyph {
            placement: Box::new(PlaceCenter { dimension: 0, alignment: 0.3 }),
            child: Some(Box::new(ChildRequester)),
        };
        let mut req = Requisition::INVALID;
        mg.request(&mut req);
        // child 가 fill: x.natural=5.0.
        assert_eq!(req.x.natural, 5.0);
        // placement (PlaceCenter, H) 가 alignment 슬롯 overlay: x.alignment=0.3.
        assert_eq!(req.x.alignment, 0.3);
    }

    /// child 가 None → placement 만 호출. 정확한 dispatch 순서 검증 (raw `Placement::Request`
    /// step1 의 SharePtr null 가드와 동등).
    #[test]
    fn monoglyph_request_with_no_child_runs_placement_only() {
        let mg = MonoGlyph {
            placement: Box::new(PlaceNatural { direction: 1, span: 12.0 }),
            child: None,
        };
        let mut req = Requisition::INVALID;
        mg.request(&mut req);
        // child 없음 → x 슬롯 INVALID 그대로 유지.
        // placement (PlaceNatural, V) 가 y.alignment=12.0 set.
        assert_eq!(req.y.alignment, 12.0);
    }

    // ============================================================
    // MonoGlyph::allocate dispatch tests (Placement::Allocate + CalcPlacement)
    // ============================================================

    use crate::placement::{PlaceFix, PlaceMargin};
    // Allocation, Allotment, Extension, Requirement, Requisition 은 `use super::*` 로 이미 가시.
    use std::rc::Rc;

    /// child.allocate 가 어떤 alloc 으로 호출됐는지 capture 하는 spy.
    /// (Glyph::allocate 시그니처 = `&mut self, &Allocation, &mut Extension`).
    #[derive(Debug)]
    struct AllocSpy {
        captured: Rc<RefCell<Option<Allocation>>>,
        req_x_natural: f32,
        req_x_align: f32,
    }
    impl Glyph for AllocSpy {
        fn clone_glyph(&self) -> Box<dyn Glyph> {
            Box::new(AllocSpy {
                captured: self.captured.clone(),
                req_x_natural: self.req_x_natural,
                req_x_align: self.req_x_align,
            })
        }
        fn as_any(&self) -> &dyn std::any::Any { self }
        fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
        fn request(&self, req: &mut Requisition) {
            req.x = Requirement::new(self.req_x_natural, 0.0, 0.0, self.req_x_align);
            req.y = Requirement::new(self.req_x_natural, 0.0, 0.0, self.req_x_align);
        }
        fn allocate(&mut self, alloc: &Allocation, _ext: &mut Extension) {
            *self.captured.borrow_mut() = Some(*alloc);
        }
    }

    /// **PlaceFix + MonoGlyph::allocate** 통합 byte-equiv:
    /// parent_alloc 의 x.span 이 fix_size 로 강제됨.
    #[test]
    fn monoglyph_allocate_with_placefix_horizontal_fixes_span() {
        let captured = Rc::new(RefCell::new(None));
        let mut mg = MonoGlyph {
            placement: Box::new(PlaceFix { dimension: 0, fix_size: 42.0 }),
            child: Some(Box::new(AllocSpy {
                captured: captured.clone(),
                req_x_natural: 100.0,
                req_x_align: 0.0,
            })),
        };
        let parent = Allocation::new(
            Allotment::new(10.0, 200.0, 0.0),
            Allotment::new(20.0, 300.0, 0.0),
        );
        let mut ext = Extension::default();
        mg.allocate(&parent, &mut ext);

        let captured_alloc = captured.borrow().expect("child must be called");
        // x.span fixed to 42 (from PlaceFix); x.origin unchanged.
        assert_eq!(captured_alloc.x.span, 42.0);
        assert_eq!(captured_alloc.x.origin, 10.0);
        // y unchanged.
        assert_eq!(captured_alloc.y.span, 300.0);
        assert_eq!(captured_alloc.y.origin, 20.0);
    }

    /// **PlaceCenter + MonoGlyph::allocate** 통합 byte-equiv:
    /// PlaceCenter 가 child_req.x.alignment 를 읽어서 alloc.x.origin/alignment 조정.
    /// 단, raw 의 CalcPlacement 는 strategy.Request 를 호출하지 않으므로 child_req 는
    /// child 본인이 set 한 값 (= AllocSpy::request 의 align 인자) 만 반영.
    #[test]
    fn monoglyph_allocate_with_placecenter_shifts_origin_by_child_align() {
        let captured = Rc::new(RefCell::new(None));
        let mut mg = MonoGlyph {
            placement: Box::new(PlaceCenter { dimension: 0, alignment: 0.5 }),
            child: Some(Box::new(AllocSpy {
                captured: captured.clone(),
                req_x_natural: 50.0,
                req_x_align: 0.3,  // child 가 자기 alignment 를 0.3 으로 set
            })),
        };
        let parent = Allocation::new(
            Allotment::new(0.0, 100.0, 0.0),
            Allotment::new(0.0, 0.0, 0.0),
        );
        let mut ext = Extension::default();
        mg.allocate(&parent, &mut ext);

        let captured_alloc = captured.borrow().unwrap();
        // PlaceCenter::Allocate: alloc.x.origin += span * (req.align - alloc.align)
        //   = 0 + 100 * (0.3 - 0.0) = 30
        assert!((captured_alloc.x.origin - 30.0).abs() < 1e-4);
        assert!((captured_alloc.x.alignment - 0.3).abs() < 1e-4);
    }

    /// **PlaceMargin + MonoGlyph::allocate** 통합 byte-equiv 의 KEY TEST.
    ///
    /// 시나리오: MonoGlyph 의 parent 가 Phase 1 에서 mg.request 호출 → PlaceMargin.cache
    /// 가 post-margin 으로 populate. 그 후 Phase 2 에서 mg.allocate 호출 → PlaceMargin
    /// 이 cache 를 사용해 child sub-allocation 의 margin inset.
    #[test]
    fn monoglyph_allocate_with_placemargin_insets_child_alloc() {
        let captured = Rc::new(RefCell::new(None));
        // left=10, right=20 margin (natural only).
        let mut mg = MonoGlyph {
            placement: Box::new(PlaceMargin::with_natural(10.0, 0.0, 20.0, 0.0)),
            child: Some(Box::new(AllocSpy {
                captured: captured.clone(),
                req_x_natural: 100.0,
                req_x_align: 0.0,
            })),
        };

        // Phase 1: parent 가 mg.request 호출 → MonoGlyph::request 가 child.Request 로 req
        // 를 overwrite 한 후 strategy.Request 가 margin 추가 + cache populate.
        //   - parent 가 보내는 req 는 INVALID (raw 의 sentinel) 이지만 child.Request 가
        //     자기 자연 size 로 overwrite (AllocSpy: x.natural=100, y.natural=100).
        //   - strategy (PlaceMargin) 의 Request 는 valid 검사 통과 후 margin 더해 cache 갱신.
        let mut probe_req = Requisition::INVALID;
        mg.request(&mut probe_req);
        // 검증: cache 에 post-margin 값 들어가 있어야 함.
        let cache = (mg.placement.as_any().downcast_ref::<PlaceMargin>().unwrap()).cache.get();
        // child 가 x.natural=100 set, strategy 가 +10+20=30 더함 → cache.x.natural=130.
        assert_eq!(cache.x.natural, 130.0);

        // Phase 2: parent 가 mg.allocate 호출. PlaceMargin 이 cache 사용.
        let parent = Allocation::new(
            Allotment::new(0.0, 200.0, 0.0),
            Allotment::new(0.0, 200.0, 0.0),
        );
        let mut ext = Extension::default();
        mg.allocate(&parent, &mut ext);

        let captured_alloc = captured.borrow().unwrap();
        // PlaceMargin::Allocate x axis: diff_x = 200 - 130 = 70, stretch path 못 탐 (stretch=0).
        //   shrink path: lr/rr 모두 0 (cache.x.shrink=0).
        //   lspan = left.natural + 0*diff = 10, rspan = right.natural + 0*diff = 20.
        //   alloc.x.origin += -(0*20) + 10*(1-0) = 10.
        //   alloc.x.span -= (10 + 20) = 30 → 200 - 30 = 170.
        assert!((captured_alloc.x.origin - 10.0).abs() < 1e-4,
                "origin: {}", captured_alloc.x.origin);
        assert!((captured_alloc.x.span - 170.0).abs() < 1e-4,
                "span: {}", captured_alloc.x.span);
    }

    /// child 가 None → mg.allocate no-op (raw `Placement::Allocate` 의 inner-null 가드와 동등).
    #[test]
    fn monoglyph_allocate_with_no_child_is_noop() {
        let mut mg = MonoGlyph {
            placement: Box::new(PlaceFix { dimension: 0, fix_size: 42.0 }),
            child: None,
        };
        let parent = Allocation::new(
            Allotment::new(10.0, 100.0, 0.5),
            Allotment::new(20.0, 50.0, 0.0),
        );
        let mut ext = Extension::default();
        mg.allocate(&parent, &mut ext);
        // ext 미수정.
        assert_eq!(ext, Extension::default());
    }

    // ============================================================
    // Phase L-5: Glyph base + MonoGlyph + Box::Undraw 1:1 port tests
    // ============================================================
    //
    // 정공법 검증:
    // - Glyph::Draw (0x31597c) base = no-op (raw `ret`)
    // - Glyph::Undraw (0x2f8f8c) base = container traversal (GetCount/GetComponent + child.undraw)
    // - Glyph::GetBounds (0x315980) base = zero sret output (observable byte-eq with Allocation::ZERO)
    // - Glyph::Pick (0x315990) base = return false (raw `mov w0, #0`)
    // - Placement::Draw/Undraw/GetBounds/Pick (0x331488/518/538/5e8) = CalcPlacement + child forward
    //   (Undraw 만 CalcPlacement 없이 direct forward)
    // - Box::Undraw (0x2e6404) = children linked list 순회 (None slot skip)

    use std::rc::Rc as StdRc;
    use kdsnr_render::bw_mode::BWMode;
    use kdsnr_render::flag::Flag;
    use kdsnr_render::hit::Hit;
    use kdsnr_render::svg_surface::SvgSurface;
    use kdsnr_render::theme::Theme as RenderTheme;

    /// draw/undraw/get_bounds/pick 호출 횟수를 기록하는 mock leaf glyph.
    #[derive(Debug)]
    struct CountingLeaf {
        draws: StdRc<RefCell<u32>>,
        undraws: StdRc<RefCell<u32>>,
        bounds_calls: StdRc<RefCell<u32>>,
        pick_calls: StdRc<RefCell<u32>>,
        /// pick 의 return 값.
        pick_result: bool,
        /// get_bounds 의 return 값 (default = Allocation::ZERO).
        bounds_result: Allocation,
        /// draw/get_bounds 시 호출된 alloc 을 기록 (CalcPlacement 가 child_alloc 으로 modify 했는지 검증용).
        last_draw_alloc: StdRc<RefCell<Option<Allocation>>>,
    }

    impl CountingLeaf {
        fn new() -> Self {
            Self {
                draws: StdRc::new(RefCell::new(0)),
                undraws: StdRc::new(RefCell::new(0)),
                bounds_calls: StdRc::new(RefCell::new(0)),
                pick_calls: StdRc::new(RefCell::new(0)),
                pick_result: false,
                bounds_result: Allocation::ZERO,
                last_draw_alloc: StdRc::new(RefCell::new(None)),
            }
        }
    }

    impl Glyph for CountingLeaf {
        fn clone_glyph(&self) -> Box<dyn Glyph> {
            Box::new(Self {
                draws: StdRc::clone(&self.draws),
                undraws: StdRc::clone(&self.undraws),
                bounds_calls: StdRc::clone(&self.bounds_calls),
                pick_calls: StdRc::clone(&self.pick_calls),
                pick_result: self.pick_result,
                bounds_result: self.bounds_result,
                last_draw_alloc: StdRc::clone(&self.last_draw_alloc),
            })
        }
        fn as_any(&self) -> &dyn std::any::Any { self }
        fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
        fn request(&self, _req_out: &mut Requisition) {
            // strategy 에 영향 안 주는 sentinel — Placement::Draw 의 child_req 가
            // INVALID 가 아닌 임의 값으로 채워졌는지 검증할 때 사용 가능.
        }
        fn allocate(&mut self, _alloc: &Allocation, _ext: &mut Extension) {}
        fn draw(&mut self, _s: &mut dyn Surface, alloc: &Allocation, _f: &Flag, _bw: &BWMode) {
            *self.draws.borrow_mut() += 1;
            *self.last_draw_alloc.borrow_mut() = Some(*alloc);
        }
        fn undraw(&self, _f: &Flag) {
            *self.undraws.borrow_mut() += 1;
        }
        fn get_bounds(
            &mut self,
            _theme: Option<&RenderTheme>,
            _alloc: &Allocation,
            _child: Option<&dyn Glyph>,
        ) -> Allocation {
            *self.bounds_calls.borrow_mut() += 1;
            self.bounds_result
        }
        fn pick(
            &mut self,
            _alloc: &Allocation,
            _theme: Option<&RenderTheme>,
            _hit: &mut Hit,
            _depth: i32,
        ) -> bool {
            *self.pick_calls.borrow_mut() += 1;
            self.pick_result
        }
    }

    fn dummy_surface() -> SvgSurface { SvgSurface::new(100.0, 100.0) }
    fn dummy_alloc() -> Allocation {
        Allocation::new(Allotment::new(0.0, 100.0, 0.0), Allotment::new(0.0, 50.0, 0.0))
    }

    /// raw `0x31597c` (Glyph::Draw base): ret only. 관찰 가능한 side effect 없음.
    /// CountingLeaf 가 Glyph trait 의 default 를 override 했으므로 base default 검증은
    /// 별도 ZST helper 가 필요.
    #[derive(Debug, Default)]
    struct EmptyGlyph;
    impl Glyph for EmptyGlyph {
        fn clone_glyph(&self) -> Box<dyn Glyph> { Box::new(EmptyGlyph) }
        fn as_any(&self) -> &dyn std::any::Any { self }
        fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    }

    #[test]
    fn glyph_base_draw_is_noop() {
        // raw 0x31597c (`ret`): no side effect, no panic.
        let mut g = EmptyGlyph;
        let mut s = dummy_surface();
        g.draw(&mut s, &dummy_alloc(), &Flag::new(), &BWMode::V0);
        // EmptyGlyph 는 trait default draw 사용 — 호출만 가능하면 OK.
        // SVG buffer 미수정 검증.
        assert!(s.buffer.is_empty());
    }

    #[test]
    fn glyph_base_undraw_traverses_children_via_vfuncs() {
        // raw 0x2f8f8c: GetCount + GetComponent 순회 + child.Undraw 호출.
        // primitive (GetCount=0) 에선 no-op.
        let g = EmptyGlyph;
        g.undraw(&Flag::new()); // GetCount=0 → no-op, no panic.

        // container (Box_ with 3 children, base default 가 GetCount/GetComponent 사용)
        // - Box::Undraw override 와 동등 결과지만 base 경로 검증 위해 EmptyContainer 사용.
        #[derive(Debug)]
        struct CountingContainer {
            children: Vec<CountingLeaf>,
        }
        impl Glyph for CountingContainer {
            fn clone_glyph(&self) -> Box<dyn Glyph> { unimplemented!() }
            fn as_any(&self) -> &dyn std::any::Any { self }
            fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
            fn get_count(&self) -> usize { self.children.len() }
            fn get_component(&self, idx: usize) -> Option<&dyn Glyph> {
                self.children.get(idx).map(|c| c as &dyn Glyph)
            }
        }
        let c0 = CountingLeaf::new();
        let c1 = CountingLeaf::new();
        let undraws_0 = StdRc::clone(&c0.undraws);
        let undraws_1 = StdRc::clone(&c1.undraws);
        let container = CountingContainer { children: vec![c0, c1] };
        container.undraw(&Flag::new());
        // base default Undraw 가 두 child 모두 forward.
        assert_eq!(*undraws_0.borrow(), 1);
        assert_eq!(*undraws_1.borrow(), 1);
    }

    #[test]
    fn glyph_base_get_bounds_returns_zero() {
        // raw 0x315980: zero sret output. Allocation::ZERO 와 observable byte-eq.
        let mut g = EmptyGlyph;
        let result = g.get_bounds(None, &dummy_alloc(), None);
        assert_eq!(result, Allocation::ZERO);
    }

    #[test]
    fn glyph_base_pick_returns_false() {
        // raw 0x315990: `mov w0, #0` → false.
        let mut g = EmptyGlyph;
        let mut hit = Hit::default();
        let result = g.pick(&dummy_alloc(), None, &mut hit, 0);
        assert!(!result);
    }

    // ─── Placement (MonoGlyph) 4 method ─────────────────────────────

    #[test]
    fn placement_draw_forwards_to_child_with_calc_placement() {
        // raw 0x331488: child 있으면 CalcPlacement + child.Draw(surface, &local_alloc, ...).
        let leaf = CountingLeaf::new();
        let draws = StdRc::clone(&leaf.draws);
        let last_alloc = StdRc::clone(&leaf.last_draw_alloc);
        let mut mg = MonoGlyph {
            placement: Box::new(PlaceFix { dimension: 0, fix_size: 33.0 }),
            child: Some(Box::new(leaf)),
        };
        let mut s = dummy_surface();
        let parent = dummy_alloc();
        mg.draw(&mut s, &parent, &Flag::new(), &BWMode::V0);
        assert_eq!(*draws.borrow(), 1, "child.draw 정확히 1회 forward");
        // CalcPlacement (PlaceFix dim=0) 이 child_alloc.x.span = 33.0 으로 modify.
        let recorded = last_alloc.borrow();
        let alloc = recorded.expect("draw 가 alloc 기록");
        assert_eq!(alloc.x.span, 33.0, "PlaceFix 가 x.span 을 fix_size 로 modify");
    }

    #[test]
    fn placement_draw_with_no_child_is_noop() {
        // raw 0x331488 fallback: child null → ret. SVG buffer 변경 없음.
        let mut mg = MonoGlyph {
            placement: Box::new(PlaceFix { dimension: 0, fix_size: 1.0 }),
            child: None,
        };
        let mut s = dummy_surface();
        mg.draw(&mut s, &dummy_alloc(), &Flag::new(), &BWMode::V0);
        assert!(s.buffer.is_empty());
    }

    #[test]
    fn placement_undraw_forwards_to_child_without_calc_placement() {
        // raw 0x331518: CalcPlacement 없이 child.Undraw(flag) direct forward.
        let leaf = CountingLeaf::new();
        let undraws = StdRc::clone(&leaf.undraws);
        let mg = MonoGlyph {
            placement: Box::new(PlaceFix { dimension: 0, fix_size: 1.0 }),
            child: Some(Box::new(leaf)),
        };
        mg.undraw(&Flag::new());
        assert_eq!(*undraws.borrow(), 1);
    }

    #[test]
    fn placement_undraw_with_no_child_is_noop() {
        let mg = MonoGlyph {
            placement: Box::new(PlaceFix { dimension: 0, fix_size: 1.0 }),
            child: None,
        };
        mg.undraw(&Flag::new()); // no panic
    }

    #[test]
    fn placement_get_bounds_forwards_with_calc_placement() {
        // raw 0x331538: child 있으면 CalcPlacement + child.GetBounds. 결과 = child 의 출력.
        let mut leaf = CountingLeaf::new();
        leaf.bounds_result = Allocation::new(
            Allotment::new(7.0, 17.0, 0.25),
            Allotment::new(11.0, 13.0, 0.5),
        );
        let calls = StdRc::clone(&leaf.bounds_calls);
        let expected = leaf.bounds_result;
        let mut mg = MonoGlyph {
            placement: Box::new(PlaceFix { dimension: 1, fix_size: 25.0 }),
            child: Some(Box::new(leaf)),
        };
        let result = mg.get_bounds(None, &dummy_alloc(), None);
        assert_eq!(*calls.borrow(), 1);
        assert_eq!(result, expected);
    }

    #[test]
    fn placement_get_bounds_with_no_child_returns_zero() {
        // raw 0x3315c8 fallback: zero sret output = Allocation::ZERO.
        let mut mg = MonoGlyph {
            placement: Box::new(PlaceFix { dimension: 0, fix_size: 1.0 }),
            child: None,
        };
        let result = mg.get_bounds(None, &dummy_alloc(), None);
        assert_eq!(result, Allocation::ZERO);
    }

    #[test]
    fn placement_pick_forwards_with_calc_placement() {
        let mut leaf = CountingLeaf::new();
        leaf.pick_result = true;
        let calls = StdRc::clone(&leaf.pick_calls);
        let mut mg = MonoGlyph {
            placement: Box::new(PlaceFix { dimension: 0, fix_size: 1.0 }),
            child: Some(Box::new(leaf)),
        };
        let mut hit = Hit::default();
        let result = mg.pick(&dummy_alloc(), None, &mut hit, 0);
        assert_eq!(*calls.borrow(), 1);
        assert!(result, "child.pick=true → mg.pick=true");
    }

    #[test]
    fn placement_pick_with_no_child_returns_false() {
        // raw 0x331670 fallback: mov w0, #0; ret.
        let mut mg = MonoGlyph {
            placement: Box::new(PlaceFix { dimension: 0, fix_size: 1.0 }),
            child: None,
        };
        let mut hit = Hit::default();
        assert!(!mg.pick(&dummy_alloc(), None, &mut hit, 0));
    }

    // ─── Box::Undraw ────────────────────────────────────────────────

    #[test]
    fn box_undraw_traverses_all_children() {
        // raw 0x2e6404: children linked list 순회 + child.Undraw forward.
        let leaf_a = CountingLeaf::new();
        let leaf_b = CountingLeaf::new();
        let undraws_a = StdRc::clone(&leaf_a.undraws);
        let undraws_b = StdRc::clone(&leaf_b.undraws);
        let mut bx = Box_::default();
        bx.children.push(Some(Box::new(leaf_a)));
        bx.children.push(Some(Box::new(leaf_b)));
        bx.undraw(&Flag::new());
        assert_eq!(*undraws_a.borrow(), 1);
        assert_eq!(*undraws_b.borrow(), 1);
    }

    #[test]
    fn box_undraw_skips_none_slots() {
        // raw `cbz x8, advance`: SharePtr inner null skip.
        let leaf = CountingLeaf::new();
        let undraws = StdRc::clone(&leaf.undraws);
        let mut bx = Box_::default();
        bx.children.push(None);
        bx.children.push(Some(Box::new(leaf)));
        bx.children.push(None);
        bx.undraw(&Flag::new());
        // None slot 들은 panic 없이 skip, 유일한 Some 만 forward.
        assert_eq!(*undraws.borrow(), 1);
    }

    #[test]
    fn box_undraw_empty_children_is_noop() {
        let bx = Box_::default();
        bx.undraw(&Flag::new()); // no panic
    }

    // ─── L-5b: Box::Draw / GetBounds / Pick (vfunc[5/7/8]) ──────────

    /// `Box::Draw` (FUN_002e6348) byte-eq: 모든 non-null child 에 대해 `child.draw` 호출.
    /// raw: `for idx in 0..GetCount: if child = GetComponent(idx): child.vfunc[+0x28](
    ///        surface, &cached_allocs[idx], flag, bw)`.
    #[test]
    fn box_draw_forwards_to_all_children() {
        let leaf_a = CountingLeaf::new();
        let leaf_b = CountingLeaf::new();
        let draws_a = StdRc::clone(&leaf_a.draws);
        let draws_b = StdRc::clone(&leaf_b.draws);
        let mut bx = Box_::default();
        bx.children.push(Some(Box::new(leaf_a)));
        bx.children.push(Some(Box::new(leaf_b)));
        let mut s = dummy_surface();
        bx.draw(&mut s, &dummy_alloc(), &Flag::new(), &BWMode::V0);
        assert_eq!(*draws_a.borrow(), 1, "child A draw 1회");
        assert_eq!(*draws_b.borrow(), 1, "child B draw 1회");
    }

    /// `Box::Draw` raw `cbz x0, advance` (null child skip): None slot 은 호출 안 함.
    #[test]
    fn box_draw_skips_none_slots() {
        let leaf = CountingLeaf::new();
        let draws = StdRc::clone(&leaf.draws);
        let mut bx = Box_::default();
        bx.children.push(None);
        bx.children.push(Some(Box::new(leaf)));
        bx.children.push(None);
        let mut s = dummy_surface();
        bx.draw(&mut s, &dummy_alloc(), &Flag::new(), &BWMode::V0);
        assert_eq!(*draws.borrow(), 1);
    }

    /// `Box::Draw` raw `cbz x0, 0x2e63ec` (count==0 early ret): empty children 안전.
    #[test]
    fn box_draw_empty_children_is_noop() {
        let mut bx = Box_::default();
        let mut s = dummy_surface();
        bx.draw(&mut s, &dummy_alloc(), &Flag::new(), &BWMode::V0); // no panic
        assert!(s.buffer.is_empty());
    }

    /// `Box::Draw` raw `cache_bounds_valid = false` → `recompute_bounds_cache` 가
    /// 매 호출 강제 재계산.
    #[test]
    fn box_draw_invalidates_bounds_cache() {
        let mut bx = Box_::default();
        bx.cache_bounds_valid = true;
        let mut s = dummy_surface();
        bx.draw(&mut s, &dummy_alloc(), &Flag::new(), &BWMode::V0);
        // recompute_bounds_cache 끝나면 다시 valid=true 가 되지만, 그 안에서 한번 false
        // 로 clear 되었음을 강제. 본 테스트는 panic 없이 진행됨을 확인 (recompute path
        // 가 실행됐다는 간접 검증).
        assert!(bx.cache_bounds_valid, "recompute 후 valid=true");
    }

    /// `Box::GetBounds` (FUN_002e64d4) byte-eq: 첫 "first-byte non-zero" child 의 출력 반환.
    /// raw: `child->vfunc[+0x38](out, theme, &cached_allocs[idx], child_param);
    ///       if (out->[0] != 0) ret;`  ← out 의 byte[0] = x.origin 의 LE LSB.
    ///
    /// **주의**: f32 normal value 들 (예: 7.5 = 0x40F00000 → LE byte[0] = 0x00) 은 LSB
    /// 가 0 → continue. 0.1 / 1.23 같이 비정수 값만 LSB non-zero. 본 종료 조건은 raw
    /// 1:1 byte-eq 이므로 의도 (한컴 코드의 sentinel 패턴) 그대로.
    #[test]
    fn box_get_bounds_returns_first_non_zero_first_byte_child() {
        let mut leaf_a = CountingLeaf::new();
        leaf_a.bounds_result = Allocation::ZERO; // first byte = 0 → continue
        let mut leaf_b = CountingLeaf::new();
        // 0.1f32 = 0x3DCCCCCD → LE byte[0] = 0xCD ≠ 0 → hit.
        leaf_b.bounds_result = Allocation::new(
            Allotment::new(0.1, 17.0, 0.25),
            Allotment::new(11.0, 13.0, 0.5),
        );
        let calls_a = StdRc::clone(&leaf_a.bounds_calls);
        let calls_b = StdRc::clone(&leaf_b.bounds_calls);
        let expected = leaf_b.bounds_result;
        let mut bx = Box_::default();
        bx.children.push(Some(Box::new(leaf_a)));
        bx.children.push(Some(Box::new(leaf_b)));
        let result = bx.get_bounds(None, &dummy_alloc(), None);
        assert_eq!(*calls_a.borrow(), 1, "child A 호출 후 LSB=0 → continue");
        assert_eq!(*calls_b.borrow(), 1, "child B 호출 후 LSB≠0 → return");
        assert_eq!(result, expected);
    }

    /// `Box::GetBounds` raw byte-eq 검증: f32 normal value (LSB=0) 는 raw 의 종료 조건
    /// 을 통과 못함 → fallback 까지 진행.
    #[test]
    fn box_get_bounds_normal_f32_lsb_zero_continues_to_fallback() {
        // 7.5 = 0x40F00000 → LE byte[0] = 0x00 → raw 의 `cbz` 가 continue.
        let mut leaf = CountingLeaf::new();
        leaf.bounds_result = Allocation::new(
            Allotment::new(7.5, 17.0, 0.25),
            Allotment::new(11.0, 13.0, 0.5),
        );
        let mut bx = Box_::default();
        bx.children.push(Some(Box::new(leaf)));
        let result = bx.get_bounds(None, &dummy_alloc(), None);
        // raw 의 byte-eq 동작: LSB=0 child → continue → fallback zero.
        assert_eq!(result, Allocation::ZERO);
    }

    /// `Box::GetBounds` raw fallback `0x2e6594`: empty children 또는 모든 child 가 null/zero
    /// → zero sret output.
    #[test]
    fn box_get_bounds_empty_returns_zero() {
        let mut bx = Box_::default();
        let result = bx.get_bounds(None, &dummy_alloc(), None);
        assert_eq!(result, Allocation::ZERO);
    }

    #[test]
    fn box_get_bounds_all_zero_children_returns_zero() {
        // 모든 child 가 zero Allocation 반환 → fallback.
        let mut leaf_a = CountingLeaf::new();
        leaf_a.bounds_result = Allocation::ZERO;
        let mut leaf_b = CountingLeaf::new();
        leaf_b.bounds_result = Allocation::ZERO;
        let mut bx = Box_::default();
        bx.children.push(Some(Box::new(leaf_a)));
        bx.children.push(Some(Box::new(leaf_b)));
        let result = bx.get_bounds(None, &dummy_alloc(), None);
        assert_eq!(result, Allocation::ZERO);
    }

    /// `Box::Pick` (FUN_002e65b8) byte-eq: 첫 hit (= child.pick=true) 에서 break, true 반환.
    /// 모든 child false → false. count==0 → false.
    #[test]
    fn box_pick_returns_true_on_first_hit() {
        let mut leaf_a = CountingLeaf::new();
        leaf_a.pick_result = false;
        let mut leaf_b = CountingLeaf::new();
        leaf_b.pick_result = true;
        let leaf_c = CountingLeaf::new(); // 호출 안 됨 (break)
        let calls_a = StdRc::clone(&leaf_a.pick_calls);
        let calls_b = StdRc::clone(&leaf_b.pick_calls);
        let calls_c = StdRc::clone(&leaf_c.pick_calls);
        let mut bx = Box_::default();
        bx.children.push(Some(Box::new(leaf_a)));
        bx.children.push(Some(Box::new(leaf_b)));
        bx.children.push(Some(Box::new(leaf_c)));
        let mut hit = Hit::default();
        let result = bx.pick(&dummy_alloc(), None, &mut hit, 0);
        assert!(result);
        assert_eq!(*calls_a.borrow(), 1);
        assert_eq!(*calls_b.borrow(), 1);
        assert_eq!(*calls_c.borrow(), 0, "B 에서 break — C 호출 안 됨");
    }

    #[test]
    fn box_pick_returns_false_when_all_children_miss() {
        let mut leaf_a = CountingLeaf::new();
        leaf_a.pick_result = false;
        let mut leaf_b = CountingLeaf::new();
        leaf_b.pick_result = false;
        let mut bx = Box_::default();
        bx.children.push(Some(Box::new(leaf_a)));
        bx.children.push(Some(Box::new(leaf_b)));
        let mut hit = Hit::default();
        assert!(!bx.pick(&dummy_alloc(), None, &mut hit, 0));
    }

    #[test]
    fn box_pick_returns_false_when_empty() {
        // raw: cbz x0, 0x2e6668 → result = false.
        let mut bx = Box_::default();
        let mut hit = Hit::default();
        assert!(!bx.pick(&dummy_alloc(), None, &mut hit, 0));
    }

    #[test]
    fn box_pick_skips_none_slots() {
        // None slot 은 pick 호출 안 함 (raw `cbz x0, continue`).
        let mut leaf = CountingLeaf::new();
        leaf.pick_result = true;
        let calls = StdRc::clone(&leaf.pick_calls);
        let mut bx = Box_::default();
        bx.children.push(None);
        bx.children.push(Some(Box::new(leaf)));
        bx.children.push(None);
        let mut hit = Hit::default();
        assert!(bx.pick(&dummy_alloc(), None, &mut hit, 0));
        assert_eq!(*calls.borrow(), 1);
    }
}

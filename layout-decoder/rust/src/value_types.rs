//! Value types — POD-like records that flow between Glyph / Placement / Compositor.
//!
//! 모든 field layout 과 메소드는 `libHncDrawingEngine_arm64.dylib` decompile 에서
//! 1:1 포팅. 출처 함수 주소는 각 메소드 위에 doc-comment 로 표기.
//!
//! Hancom 원본 class:
//! - `Hnc::Shape::Text::Requirement` (16 bytes, 4 floats)
//! - `Hnc::Shape::Text::Requisition` (36 bytes, 2×Requirement + penalty i32)
//! - `Hnc::Shape::Text::Allotment` (12 bytes, 3 floats)
//! - `Hnc::Shape::Text::Allocation` (24 bytes, 2×Allotment)

// ============================================================
// Requirement
// ============================================================

/// One-axis size request — `Hnc::Shape::Text::Requirement` (16 bytes).
///
/// Field offsets (from `GetNatural`/`GetStretch`/`GetShrink`/`GetAlignment` decompiles):
/// - +0x0: natural   (`FUN_002d0de0`)
/// - +0x4: stretch   (`FUN_002d0de8`)
/// - +0x8: shrink    (`FUN_002d0df0`)
/// - +0xc: alignment (`FUN_002d0df8`)
#[derive(Debug, Clone, Copy, Default, PartialEq)]
#[repr(C)]
pub struct Requirement {
    pub natural: f32,
    pub stretch: f32,
    pub shrink: f32,
    pub alignment: f32,
}

impl Requirement {
    pub const ZERO: Self = Self { natural: 0.0, stretch: 0.0, shrink: 0.0, alignment: 0.0 };

    /// Sentinel used by `IsValid()` 로 invalid 표시.
    pub const INVALID_NATURAL: f32 = -1e8;

    /// `Requirement::Requirement(float natural, float stretch, float shrink, float alignment)`
    /// — `FUN_002e5c04`. 단순 필드 복사.
    #[inline]
    pub const fn new(natural: f32, stretch: f32, shrink: f32, alignment: f32) -> Self {
        Self { natural, stretch, shrink, alignment }
    }

    /// `Requirement::Requirement(float max1, float natural1, float min1, float max2, float natural2, float min2)`
    /// — `FUN_002d0e00`.
    ///
    /// 두 Requirement 를 결합. param 순서는 (각각 max, natural, min) × 2. 분기:
    /// - 두 natural 모두 0 → alignment = 0 (첫 axis 만)
    /// - 첫 natural 만 0 → alignment = 1.0
    /// - 그 외 → 비율 기반 stretch/shrink + alignment = `natural1 / (natural1 + natural2)`
    ///
    /// 참고: param naming 은 decompile 의 raw 명. 의미는 max/natural/min 의 3-tuple 두 개.
    pub fn from_two_ranges(
        mut max1: f32, natural1: f32, min1: f32,
        mut max2: f32, natural2: f32, min2: f32,
    ) -> Self {
        // 첫 axis 의 max 를 (max1, natural1, min1) 중 가장 가까운 자체 값으로 클램프
        if natural1 <= max1 { max1 = natural1; }
        if max1 <= min1 { max1 = min1; }
        let fvar5 = if max1 <= natural1 { natural1 } else { max1 };
        let fvar6 = if min1 <= max1 { min1 } else { max1 };

        if natural2 <= max2 { max2 = natural2; }
        if max2 <= min2 { max2 = min2; }
        let fvar2 = if max2 <= natural2 { natural2 } else { max2 };
        let fvar3 = if min2 <= max2 { min2 } else { max2 };

        let fvar1 = max1 + max2;
        let natural = fvar1;

        if max1 == 0.0 {
            return Self {
                natural,
                stretch: fvar2 - max2,
                shrink: max2 - fvar3,
                alignment: 0.0,
            };
        }
        if max2 == 0.0 {
            return Self {
                natural,
                stretch: fvar5 - max1,
                shrink: max1 - fvar6,
                alignment: 1.0,  // 0x3f800000
            };
        }

        // ratio-based combination
        let mut fvar4 = fvar3 / max2;
        if fvar3 / max2 <= fvar6 / max1 {
            fvar4 = fvar6 / max1;
        }
        let mut fvar6_var = fvar2 / max2;
        if fvar5 / max1 <= fvar2 / max2 {
            fvar6_var = fvar5 / max1;
        }
        let stretch = fvar1 * (fvar6_var - 1.0);
        let shrink = fvar1 * (1.0 - fvar4);
        let alignment = if fvar1 == 0.0 { 0.0 } else { max1 / fvar1 };

        Self { natural, stretch, shrink, alignment }
    }

    /// `Requirement::Create(float, float, float, float)` (`FUN_00315278`) — heap-allocated
    /// factory. Rust 에선 stack 으로 그대로.
    #[inline]
    pub fn create(natural: f32, stretch: f32, shrink: f32, alignment: f32) -> Self {
        Self::new(natural, stretch, shrink, alignment)
    }

    /// `Requirement::Create(float, float, float, float, float, float)` (`FUN_003152c8`).
    /// 6-arg combine.
    #[inline]
    pub fn create_from_two(max1: f32, n1: f32, min1: f32, max2: f32, n2: f32, min2: f32) -> Self {
        Self::from_two_ranges(max1, n1, min1, max2, n2, min2)
    }

    /// `IsValid()` (`FUN_002d0dc4`). `natural != -1e8`.
    #[inline]
    pub fn is_valid(&self) -> bool {
        self.natural != Self::INVALID_NATURAL
    }

    /// `Equals(other, epsilon)` (`FUN_003151e0`). 4-component within `epsilon`.
    pub fn equals(&self, other: &Self, eps: f32) -> bool {
        (self.natural - other.natural).abs() < eps
            && (self.stretch - other.stretch).abs() < eps
            && (self.shrink - other.shrink).abs() < eps
            && (self.alignment - other.alignment).abs() < eps
    }

    // accessors (1:1 with Get*)
    #[inline] pub fn get_natural(&self) -> f32 { self.natural }
    #[inline] pub fn get_stretch(&self) -> f32 { self.stretch }
    #[inline] pub fn get_shrink(&self) -> f32 { self.shrink }
    #[inline] pub fn get_alignment(&self) -> f32 { self.alignment }
}

// ============================================================
// Requisition
// ============================================================

/// Two-axis size request + penalty — `Hnc::Shape::Text::Requisition` (36 bytes).
///
/// Field layout (verified `Get` / `GetY` / `GetPenalty`):
/// - +0x00..0x10: X Requirement
/// - +0x10..0x20: Y Requirement
/// - +0x20: penalty (i32)
#[derive(Debug, Clone, Copy, Default, PartialEq)]
#[repr(C)]
pub struct Requisition {
    pub x: Requirement,
    pub y: Requirement,
    pub penalty: i32,
}

/// `Hnc::Shape::Text::Dimension` (axis selector) — 0=X, 1=Y.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum Dimension {
    X = 0,
    Y = 1,
}

impl Requisition {
    pub const ZERO: Self = Self { x: Requirement::ZERO, y: Requirement::ZERO, penalty: 0 };

    /// INVALID sentinel — `_DAT_00741f20` (`(-1e8, 0)`) + `_UNK_00741f28` (`(0, 0)`) 기반.
    /// X/Y 의 `natural` 이 `-1e8` (`Requirement::INVALID_NATURAL`), 나머지 필드 0.
    ///
    /// `Composition::CreateItem` (`FUN_003000a8`) 의 Requisition buffer 각 원소 초기값,
    /// `out` Requisition 초기값, Tile/TileReverse 의 `cached_req` 초기값 등에 쓰임.
    pub const INVALID: Self = Self {
        x: Requirement::new(Requirement::INVALID_NATURAL, 0.0, 0.0, 0.0),
        y: Requirement::new(Requirement::INVALID_NATURAL, 0.0, 0.0, 0.0),
        penalty: 0,
    };

    /// `Requisition::Requisition(Requirement const& x, Requirement const& y, int penalty)`
    /// — `FUN_002d1174` / `FUN_003153f0`.
    #[inline]
    pub fn new(x: Requirement, y: Requirement, penalty: i32) -> Self {
        Self { x, y, penalty }
    }

    /// `Requisition::Create(...)` (`FUN_0031540c`) — heap factory. Rust stack 동등.
    #[inline]
    pub fn create(x: Requirement, y: Requirement, penalty: i32) -> Self {
        Self::new(x, y, penalty)
    }

    /// `Get(Dimension)` (`FUN_002d0db4`).
    #[inline]
    pub fn get(&self, dim: Dimension) -> &Requirement {
        match dim {
            Dimension::X => &self.x,
            Dimension::Y => &self.y,
        }
    }

    /// `GetY()` (`FUN_002d135c`) — Y axis 의 alias.
    #[inline] pub fn get_y(&self) -> &Requirement { &self.y }
    #[inline] pub fn get_x(&self) -> &Requirement { &self.x }

    /// `GetPenalty()` (`FUN_0030165c`).
    #[inline] pub fn get_penalty(&self) -> i32 { self.penalty }

    /// `Set(Dimension, Requirement)` (`FUN_002d0ee4`).
    #[inline]
    pub fn set(&mut self, dim: Dimension, req: Requirement) {
        match dim {
            Dimension::X => self.x = req,
            Dimension::Y => self.y = req,
        }
    }

    /// `SetX(Requirement)` (`FUN_002d1348`).
    #[inline] pub fn set_x(&mut self, req: Requirement) { self.x = req; }

    /// `SetY(Requirement)` (`FUN_002d1370`).
    #[inline] pub fn set_y(&mut self, req: Requirement) { self.y = req; }

    /// `SetPenalty(int)` (`FUN_002f5d34`).
    #[inline] pub fn set_penalty(&mut self, p: i32) { self.penalty = p; }

    /// `Equals(other, eps)` (`FUN_002ff734`). 8 floats (2×Requirement) within eps.
    /// 주의: decompile 은 penalty 비교 안 함 — float 컴포넌트 8개만 검사.
    pub fn equals(&self, other: &Self, eps: f32) -> bool {
        self.x.equals(&other.x, eps) && self.y.equals(&other.y, eps)
    }
}

// ============================================================
// Allotment
// ============================================================

/// One-axis allotted slot — `Hnc::Shape::Text::Allotment` (12 bytes).
///
/// Field layout (검증 — `GetOrigin`/`GetSpan`/`GetAlignment`):
/// - +0x0: origin    (`FUN_002d1090`)
/// - +0x4: span      (`FUN_002d1080`)
/// - +0x8: alignment (`FUN_002d1088`)
///
/// 의미:
/// - `origin` 은 alignment 기준점의 절대 좌표
/// - `span` 은 영역 길이
/// - `alignment` 는 0.0=영역 시작점에 origin 위치, 1.0=영역 끝점에 origin 위치
/// - `begin = origin - alignment*span`, `end = origin + (1-alignment)*span`
#[derive(Debug, Clone, Copy, Default, PartialEq)]
#[repr(C)]
pub struct Allotment {
    pub origin: f32,
    pub span: f32,
    pub alignment: f32,
}

impl Allotment {
    pub const ZERO: Self = Self { origin: 0.0, span: 0.0, alignment: 0.0 };

    /// `Allotment::Allotment(float origin, float span, float alignment)`
    /// — `FUN_002d1098` / `FUN_00315458`.
    #[inline]
    pub const fn new(origin: f32, span: f32, alignment: f32) -> Self {
        Self { origin, span, alignment }
    }

    /// `Allotment::Create(...)` (`FUN_0031550c`) — heap factory.
    #[inline]
    pub fn create(origin: f32, span: f32, alignment: f32) -> Self {
        Self::new(origin, span, alignment)
    }

    // accessors (1:1)
    #[inline] pub fn get_origin(&self) -> f32 { self.origin }
    #[inline] pub fn get_span(&self) -> f32 { self.span }
    #[inline] pub fn get_alignment(&self) -> f32 { self.alignment }

    /// `SetAlignment(float)` (`FUN_003154dc`).
    #[inline] pub fn set_alignment(&mut self, v: f32) { self.alignment = v; }

    /// `GetBegin()` (`FUN_003154e4`) = `origin - alignment * span`.
    #[inline]
    pub fn get_begin(&self) -> f32 {
        self.origin - self.alignment * self.span
    }

    /// `GetEnd()` (`FUN_003154f4`) = `origin + (1 - alignment) * span`.
    #[inline]
    pub fn get_end(&self) -> f32 {
        self.origin + (1.0 - self.alignment) * self.span
    }

    /// `Equals(other, eps)` (`FUN_00315468`).
    pub fn equals(&self, other: &Self, eps: f32) -> bool {
        (self.origin - other.origin).abs() < eps
            && (self.span - other.span).abs() < eps
            && (self.alignment - other.alignment).abs() < eps
    }
}

// ============================================================
// Allocation
// ============================================================

/// 2D allocated area — `Hnc::Shape::Text::Allocation` (24 bytes).
///
/// Field layout (검증 — `GetOriginX`/`GetOriginY`/`GetY`):
/// - +0x00..0x0c: X Allotment
/// - +0x0c..0x18: Y Allotment
#[derive(Debug, Clone, Copy, Default, PartialEq)]
#[repr(C)]
pub struct Allocation {
    pub x: Allotment,
    pub y: Allotment,
}

impl Allocation {
    pub const ZERO: Self = Self { x: Allotment::ZERO, y: Allotment::ZERO };

    /// `Allocation::Allocation(Allotment const& x, Allotment const& y)` — `FUN_00315558`.
    #[inline]
    pub const fn new(x: Allotment, y: Allotment) -> Self {
        Self { x, y }
    }

    /// `Allocation::Create(x, y)` — `FUN_00315580`. heap factory.
    #[inline]
    pub fn create(x: Allotment, y: Allotment) -> Self {
        Self::new(x, y)
    }

    /// `Get(Dimension)` (`FUN_002d1070`).
    #[inline]
    pub fn get(&self, dim: Dimension) -> &Allotment {
        match dim {
            Dimension::X => &self.x,
            Dimension::Y => &self.y,
        }
    }

    /// `GetY()` (`FUN_002fd030`).
    #[inline] pub fn get_y(&self) -> &Allotment { &self.y }
    #[inline] pub fn get_x(&self) -> &Allotment { &self.x }

    /// `GetOriginX()` (`FUN_002d1464`).
    #[inline] pub fn get_origin_x(&self) -> f32 { self.x.origin }
    /// `GetOriginY()` (`FUN_002d146c`).
    #[inline] pub fn get_origin_y(&self) -> f32 { self.y.origin }

    /// `GetLeft()` (`FUN_002d05d8`) = X allotment 의 `begin`.
    #[inline] pub fn get_left(&self) -> f32 { self.x.get_begin() }
    /// `GetRight()` (`FUN_002d05f8`) = X allotment 의 `end`.
    #[inline] pub fn get_right(&self) -> f32 { self.x.get_end() }
    /// `GetTop()` (`FUN_002d05e8`) = Y allotment 의 `begin`.
    #[inline] pub fn get_top(&self) -> f32 { self.y.get_begin() }
    /// `GetBottom()` (`FUN_002d0610`) = Y allotment 의 `end`.
    #[inline] pub fn get_bottom(&self) -> f32 { self.y.get_end() }

    /// `Set(Dimension, Allotment)` (`FUN_002d10a4`).
    #[inline]
    pub fn set(&mut self, dim: Dimension, a: Allotment) {
        match dim {
            Dimension::X => self.x = a,
            Dimension::Y => self.y = a,
        }
    }

    /// `SetX(Allotment)` (`FUN_002fd060`).
    #[inline] pub fn set_x(&mut self, a: Allotment) { self.x = a; }
    /// `SetY(Allotment)` (`FUN_002fd048`).
    #[inline] pub fn set_y(&mut self, a: Allotment) { self.y = a; }

    /// `Equals(other, eps)` (`FUN_002e6068`). 6 floats (2×Allotment) within eps.
    pub fn equals(&self, other: &Self, eps: f32) -> bool {
        self.x.equals(&other.x, eps) && self.y.equals(&other.y, eps)
    }
}

// ============================================================
// BoundsRect — Glyph::Request 의 출력
// ============================================================

/// 2D min/max bounding box accumulator.
///
/// Hancom 의 `Glyph::Request` (vtable +24, base @ `FUN_00315974`, sz=4 no-op) 가 채우는
/// 출력 타입. 4 floats:
/// - +0: X.min
/// - +4: Y.min
/// - +8: X.max
/// - +12: Y.max
///
/// Compositor 가 자식들을 순회하며 `Request(allocation, &mut bounds)` 호출 →
/// 모든 자식의 bounds 가 누적되어 전체 composition 의 bounding box 가 산출됨.
///
/// Glue / Strut 의 Request 는 byte-identical: 주어진 Allocation 의 X/Y begin/end 를
/// bounds 에 min/max merge.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[repr(C)]
pub struct BoundsRect {
    pub min_x: f32,
    pub min_y: f32,
    pub max_x: f32,
    pub max_y: f32,
}

impl BoundsRect {
    /// Compositor 가 시작 시 사용. min=+1e8, max=-1e8 로 두면 첫 child 가 bounds 를 만든 뒤
    /// 후속 child 가 union 으로 확장됨.
    ///
    /// **byte-equivalent 검증**: raw `_DAT_00741f30` (16B) = `20 bc be 4c | 20 bc be 4c |
    /// 20 bc be cc | 20 bc be cc` = `(+1e8, +1e8, -1e8, -1e8)` as f32x4. 한컴은
    /// `f32::INFINITY` 가 아니라 `±1e8` sentinel 을 사용 (`FUN_002e6120` 의
    /// `ldr q0, [x8, #0xf30]` + `Box::recompute_bounds_cache` 가 cached_bounds 초기화에 사용).
    pub const INIT: Self = Self {
        min_x: 1.0e8,
        min_y: 1.0e8,
        max_x: -1.0e8,
        max_y: -1.0e8,
    };

    pub const ZERO: Self = Self { min_x: 0.0, min_y: 0.0, max_x: 0.0, max_y: 0.0 };

    /// Allocation 의 begin/end 를 bounds 에 min/max merge.
    /// Glue / Strut 의 Request 와 동등.
    ///
    /// 한컴 원본 (Glue::Request `FUN_0031580c`):
    /// ```c
    /// begin_x = avail.x.origin - avail.x.alignment * avail.x.span;
    /// end_x   = avail.x.origin + (1 - avail.x.alignment) * avail.x.span;
    /// (begin_y, end_y similar)
    /// if (bounds.min_x <= begin_x) begin_x = bounds.min_x;  // min(prev, begin)
    /// if (bounds.min_y <= begin_y) begin_y = bounds.min_y;
    /// bounds.min_x = begin_x; bounds.min_y = begin_y;
    /// if (end_x <= bounds.max_x) end_x = bounds.max_x;     // max(prev, end)
    /// if (end_y <= bounds.max_y) end_y = bounds.max_y;
    /// bounds.max_x = end_x; bounds.max_y = end_y;
    /// ```
    pub fn merge_allocation(&mut self, avail: &Allocation) {
        let begin_x = avail.x.get_begin();
        let begin_y = avail.y.get_begin();
        let end_x = avail.x.get_end();
        let end_y = avail.y.get_end();

        // min-merge (decompile: if prev <= new, keep prev; else use new — so prev=min)
        let nx = if self.min_x <= begin_x { self.min_x } else { begin_x };
        let ny = if self.min_y <= begin_y { self.min_y } else { begin_y };
        self.min_x = nx;
        self.min_y = ny;

        // max-merge (if new <= prev, keep prev — so prev=max)
        let mx = if end_x <= self.max_x { self.max_x } else { end_x };
        let my = if end_y <= self.max_y { self.max_y } else { end_y };
        self.max_x = mx;
        self.max_y = my;
    }
}

// ============================================================
// Extension — `Hnc::Shape::Text::Extension` (16 bytes; bounding box)
// ============================================================

/// `Hnc::Shape::Text::Extension` — 16-byte POD bounding box `(left, top, right, bottom)`.
///
/// raw decompile + asm 검증 (`FUN_003155d8` ctor + `FUN_002fe18c/194/19c/1a4` getter):
/// ```text
/// +0x00  f32 left
/// +0x04  f32 top
/// +0x08  f32 right
/// +0x0c  f32 bottom
/// ```
///
/// Glyph::Allocate (vfunc[4]) 가 `Allocation const&` 와 `Extension&` 두 인자를 받음 —
/// container Glyph 들이 child 의 `Set(Allocation&)` 으로 자신의 bounding box 누적.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[repr(C)]
pub struct Extension {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl Extension {
    /// `Extension::Extension(float left, float top, float right, float bottom)` —
    /// `FUN_003155d8` sz=12 (또는 `FUN_002d8d60` byte-identical).
    /// raw: 4 float 인자를 +0x00..0x0c 에 그대로 저장.
    #[inline]
    pub const fn new(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self { left, top, right, bottom }
    }

    /// `Extension::Create(l, t, r, b)` — `FUN_003156d4` (heap factory). value-by-value 동등.
    #[inline]
    pub fn create(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self::new(left, top, right, bottom)
    }

    /// `Extension::GetLeft() const` — `FUN_002fe18c` sz=8.
    #[inline] pub fn get_left(&self) -> f32 { self.left }
    /// `Extension::GetTop() const` — `FUN_002fe194` sz=8.
    #[inline] pub fn get_top(&self) -> f32 { self.top }
    /// `Extension::GetRight() const` — `FUN_002fe19c` sz=8.
    #[inline] pub fn get_right(&self) -> f32 { self.right }
    /// `Extension::GetBottom() const` — `FUN_002fe1a4` sz=8.
    #[inline] pub fn get_bottom(&self) -> f32 { self.bottom }

    /// `Extension::Set(float, float, float, float)` — `FUN_002d1474` sz=12.
    /// raw: 4 float 을 그대로 덮어쓰기. ctor 와 동등.
    #[inline]
    pub fn set(&mut self, left: f32, top: f32, right: f32, bottom: f32) {
        self.left = left;
        self.top = top;
        self.right = right;
        self.bottom = bottom;
    }

    /// `Extension::Set(Allocation const&)` — `FUN_003156a0` sz=52.
    ///
    /// raw decompile:
    /// ```c
    /// // alloc layout (verified via field offsets):
    /// // +0=x.origin, +4=x.span, +8=x.alignment, +0xc=y.origin, +0x10=y.span, +0x14=y.alignment
    /// this->left   = origin_x - alignment_x * span_x;
    /// this->top    = origin_y - alignment_y * span_y;
    /// this->right  = origin_x + (1.0 - alignment_x) * span_x;
    /// this->bottom = origin_y + (1.0 - alignment_y) * span_y;
    /// ```
    /// 즉 `(x.get_begin(), y.get_begin(), x.get_end(), y.get_end())` 와 동등.
    pub fn set_from_allocation(&mut self, alloc: &Allocation) {
        self.left = alloc.x.get_begin();
        self.top = alloc.y.get_begin();
        self.right = alloc.x.get_end();
        self.bottom = alloc.y.get_end();
    }

    /// `Extension::Reset()` — `FUN_002e679c` sz=16.
    ///
    /// raw asm: DAT_00741f30 / UNK_00741f38 둘 다 8-byte zero. 즉 4 float 모두 0.0 으로 set.
    /// (한컴 raw 의 `_DAT_00741f30` 값 검증됨: `00 00 00 00 00 00 00 00`.)
    #[inline]
    pub fn reset(&mut self) {
        self.left = 0.0;
        self.top = 0.0;
        self.right = 0.0;
        self.bottom = 0.0;
    }

    /// `Extension::Merge(float left, float top, float right, float bottom)` —
    /// `FUN_002d05a4` sz=52. SIMD bounding-box union.
    ///
    /// raw decompile (mask logic):
    /// ```text
    /// this.left   = (new.l < this.l) ? new.l : this.l   // smaller wins (expand left)
    /// this.top    = (new.t < this.t) ? new.t : this.t   // smaller wins (expand top)
    /// this.right  = (this.r < new.r) ? new.r : this.r   // larger wins (expand right)
    /// this.bottom = (this.b < new.b) ? new.b : this.b   // larger wins (expand bottom)
    /// ```
    pub fn merge(&mut self, left: f32, top: f32, right: f32, bottom: f32) {
        if left < self.left { self.left = left; }
        if top < self.top { self.top = top; }
        if self.right < right { self.right = right; }
        if self.bottom < bottom { self.bottom = bottom; }
    }

    /// `Extension::Merge(Extension const&)` — `FUN_002e6320` sz=40.
    /// raw 와 동등 — 4 float 을 `Merge(l,t,r,b)` 로 호출.
    #[inline]
    pub fn merge_with(&mut self, other: &Self) {
        self.merge(other.left, other.top, other.right, other.bottom);
    }

    /// `Extension::Merge(Extension const&, ...)` — `FUN_0030b07c` (third overload, sz=TBD).
    /// 본 메소드는 후속 B-단계에서 raw decompile 검증 후 추가.
    /// (현재 단계에서 호출처 없음.)

    /// `Extension::operator==(Extension const&)` — `FUN_003155e8` sz=92.
    /// raw 와 동등 — 4 float 모두 정확히 동일해야 true (epsilon 없음).
    pub fn equals_exact(&self, other: &Self) -> bool {
        self.left == other.left
            && self.top == other.top
            && self.right == other.right
            && self.bottom == other.bottom
    }
}

// ============================================================
// BreakType
// ============================================================

/// Break point classification.
///
/// Glyph::Compose 의 분기 `bt < 2` 에서 파악된 enum 값. ColCompositor::ComposeBreak
/// 의 penalty 처리 코드 1:1 포팅 후 정확한 값 의미 확정 (현재는 추정).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum BreakType {
    Normal = 0,
    Hint = 1,
    Forced = 2,
    Penalty = 3,
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::size_of;

    #[test]
    fn sizes_match_hancom() {
        assert_eq!(size_of::<Requirement>(), 16);
        assert_eq!(size_of::<Requisition>(), 36);
        assert_eq!(size_of::<Allotment>(), 12);
        assert_eq!(size_of::<Allocation>(), 24);
        assert_eq!(size_of::<Extension>(), 16);
    }

    #[test]
    fn extension_ctor_and_getters() {
        let e = Extension::new(1.0, 2.0, 3.0, 4.0);
        assert_eq!(e.get_left(), 1.0);
        assert_eq!(e.get_top(), 2.0);
        assert_eq!(e.get_right(), 3.0);
        assert_eq!(e.get_bottom(), 4.0);
    }

    #[test]
    fn extension_set_4_floats() {
        let mut e = Extension::new(0.0, 0.0, 0.0, 0.0);
        e.set(5.0, 6.0, 7.0, 8.0);
        assert_eq!(e, Extension::new(5.0, 6.0, 7.0, 8.0));
    }

    #[test]
    fn extension_set_from_allocation() {
        let mut e = Extension::default();
        let a = Allocation::new(
            Allotment::new(100.0, 50.0, 0.0),   // X: begin=100, end=150
            Allotment::new(200.0, 30.0, 1.0),   // Y: begin=170, end=200
        );
        e.set_from_allocation(&a);
        assert_eq!(e.left, 100.0);
        assert_eq!(e.top, 170.0);
        assert_eq!(e.right, 150.0);
        assert_eq!(e.bottom, 200.0);
    }

    #[test]
    fn extension_reset() {
        let mut e = Extension::new(1.0, 2.0, 3.0, 4.0);
        e.reset();
        assert_eq!(e, Extension::new(0.0, 0.0, 0.0, 0.0));
    }

    #[test]
    fn extension_merge_expand_all_sides() {
        // initial small box (10, 20, 30, 40)
        let mut e = Extension::new(10.0, 20.0, 30.0, 40.0);
        // merge with wider box (5, 15, 50, 60)
        e.merge(5.0, 15.0, 50.0, 60.0);
        assert_eq!(e, Extension::new(5.0, 15.0, 50.0, 60.0));
    }

    #[test]
    fn extension_merge_no_change_when_inside() {
        // initial wide box
        let mut e = Extension::new(0.0, 0.0, 100.0, 100.0);
        // merge with smaller box inside
        e.merge(20.0, 30.0, 50.0, 60.0);
        assert_eq!(e, Extension::new(0.0, 0.0, 100.0, 100.0));
    }

    #[test]
    fn extension_merge_with_extension() {
        let mut a = Extension::new(0.0, 0.0, 100.0, 100.0);
        let b = Extension::new(-10.0, 50.0, 120.0, 60.0);
        a.merge_with(&b);
        assert_eq!(a, Extension::new(-10.0, 0.0, 120.0, 100.0));
    }

    #[test]
    fn extension_equals_exact() {
        let a = Extension::new(1.0, 2.0, 3.0, 4.0);
        let b = Extension::new(1.0, 2.0, 3.0, 4.0);
        let c = Extension::new(1.0, 2.0, 3.0, 4.0001);
        assert!(a.equals_exact(&b));
        assert!(!a.equals_exact(&c));
    }

    #[test]
    fn allotment_begin_end() {
        // alignment=0.0: begin = origin
        let a = Allotment::new(10.0, 4.0, 0.0);
        assert_eq!(a.get_begin(), 10.0);
        assert_eq!(a.get_end(), 14.0);
        // alignment=1.0: end = origin
        let a = Allotment::new(10.0, 4.0, 1.0);
        assert_eq!(a.get_begin(), 6.0);
        assert_eq!(a.get_end(), 10.0);
        // alignment=0.5: origin == midpoint
        let a = Allotment::new(10.0, 4.0, 0.5);
        assert_eq!(a.get_begin(), 8.0);
        assert_eq!(a.get_end(), 12.0);
    }

    #[test]
    fn allocation_extents() {
        let a = Allocation::new(
            Allotment::new(100.0, 50.0, 0.0),
            Allotment::new(200.0, 30.0, 1.0),
        );
        assert_eq!(a.get_left(), 100.0);
        assert_eq!(a.get_right(), 150.0);
        assert_eq!(a.get_top(), 170.0);
        assert_eq!(a.get_bottom(), 200.0);
        assert_eq!(a.get_origin_x(), 100.0);
        assert_eq!(a.get_origin_y(), 200.0);
    }

    #[test]
    fn requirement_is_valid() {
        let r = Requirement::new(0.0, 0.0, 0.0, 0.0);
        assert!(r.is_valid());
        let r = Requirement::new(Requirement::INVALID_NATURAL, 0.0, 0.0, 0.0);
        assert!(!r.is_valid());
    }

    #[test]
    fn requirement_equals() {
        let a = Requirement::new(10.0, 2.0, 1.0, 0.5);
        let b = Requirement::new(10.0001, 2.0, 1.0, 0.5);
        assert!(a.equals(&b, 0.001));
        assert!(!a.equals(&b, 0.00001));
    }

    #[test]
    fn requisition_get_set() {
        let mut r = Requisition::default();
        r.set_x(Requirement::new(10.0, 0.0, 0.0, 0.0));
        r.set_y(Requirement::new(20.0, 0.0, 0.0, 0.0));
        r.set_penalty(5);
        assert_eq!(r.get_x().natural, 10.0);
        assert_eq!(r.get_y().natural, 20.0);
        assert_eq!(r.get_penalty(), 5);
        assert_eq!(r.get(Dimension::X).natural, 10.0);
        assert_eq!(r.get(Dimension::Y).natural, 20.0);
    }

    #[test]
    fn allocation_get_set_dim() {
        let mut a = Allocation::default();
        a.set(Dimension::X, Allotment::new(1.0, 2.0, 0.3));
        a.set(Dimension::Y, Allotment::new(4.0, 5.0, 0.7));
        assert_eq!(a.get(Dimension::X).origin, 1.0);
        assert_eq!(a.get(Dimension::Y).origin, 4.0);
    }

    #[test]
    fn requirement_from_two_zero_first() {
        // first natural == 0 → use second axis only, alignment = 0
        let r = Requirement::from_two_ranges(0.0, 0.0, 0.0, 10.0, 8.0, 6.0);
        assert_eq!(r.alignment, 0.0);
        // natural = max1 + max2 = 0 + 8 = 8 (since max2 clamped to natural2=8)
        assert_eq!(r.natural, 8.0);
    }

    #[test]
    fn bounds_rect_size() {
        assert_eq!(size_of::<BoundsRect>(), 16);
    }

    #[test]
    fn bounds_merge_allocation_first_child() {
        // Empty bounds (INIT) + first allocation → bounds == allocation extents
        let mut b = BoundsRect::INIT;
        let a = Allocation::new(
            Allotment::new(10.0, 20.0, 0.0),  // X: origin=10, span=20, align=0 → begin=10, end=30
            Allotment::new(100.0, 50.0, 1.0), // Y: origin=100, span=50, align=1 → begin=50, end=100
        );
        b.merge_allocation(&a);
        assert_eq!(b.min_x, 10.0);
        assert_eq!(b.min_y, 50.0);
        assert_eq!(b.max_x, 30.0);
        assert_eq!(b.max_y, 100.0);
    }

    #[test]
    fn bounds_merge_allocation_extends_bounds() {
        // Existing bounds (10..30 x 50..100), merge wider allocation (5..40 x 60..200)
        let mut b = BoundsRect {
            min_x: 10.0, min_y: 50.0, max_x: 30.0, max_y: 100.0,
        };
        let a = Allocation::new(
            Allotment::new(5.0, 35.0, 0.0),    // begin=5, end=40
            Allotment::new(60.0, 140.0, 0.0),  // begin=60, end=200
        );
        b.merge_allocation(&a);
        assert_eq!(b.min_x, 5.0);   // extended left
        assert_eq!(b.min_y, 50.0);  // unchanged (50 < 60)
        assert_eq!(b.max_x, 40.0);  // extended right
        assert_eq!(b.max_y, 200.0); // extended down
    }

    #[test]
    fn bounds_merge_allocation_no_change_when_inside() {
        // Existing bounds wider than allocation → no change
        let mut b = BoundsRect {
            min_x: 0.0, min_y: 0.0, max_x: 100.0, max_y: 100.0,
        };
        let a = Allocation::new(
            Allotment::new(50.0, 20.0, 0.0),
            Allotment::new(50.0, 20.0, 0.0),
        );
        b.merge_allocation(&a);
        assert_eq!(b.min_x, 0.0);
        assert_eq!(b.min_y, 0.0);
        assert_eq!(b.max_x, 100.0);
        assert_eq!(b.max_y, 100.0);
    }

    #[test]
    fn requirement_from_two_zero_second() {
        // second natural == 0 → use first axis only, alignment = 1.0
        let r = Requirement::from_two_ranges(10.0, 8.0, 6.0, 0.0, 0.0, 0.0);
        assert_eq!(r.alignment, 1.0);
        assert_eq!(r.natural, 8.0);
    }
}

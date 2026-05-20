//! `Hnc::Shape::Text::Compositor` virtual interface + subclass stubs.
//!
//! 한컴 RTTI hierarchy:
//! ```text
//! Compositor (abstract base)
//!   ├─ SimpleCompositor                vtable @ 0x7804a0
//!   │    ├─ ColCompositor              vtable @ 0x780558
//!   │    │    └─ PptCompositor         vtable @ 0x780578
//!   │    └─ ArrayCompositor            vtable (TBD)
//! ```
//!
//! 모든 Compositor vfunc (vptr+0x18 / +0x20 / +0x28 / +0x30):
//! - `ComposeNumbering` / `ComposeBullet` / `ComposeBreak` / `ComposeLayout`
//!
//! ## 1:1 포팅 정책
//!
//! - 각 subclass 의 ComposeLayout/Break/Numbering/Bullet 는 **별도 함수** — 한컴 raw
//!   address 별로 1:1 port. SimpleCompositor 와 ColCompositor 와 ArrayCompositor 의
//!   ComposeLayout 이 동일하다고 가정 금지. 각각 0x303f80 / 0x305d60 / 0x304d5c 의
//!   별도 decompile 을 확인 후 별도 포팅.
//! - PptCompositor::ComposeLayout 는 이미 `ppt_compose_layout.rs` 에 ported —
//!   trait method 에서 그것을 호출.
//!
//! ## 진척
//!
//! - B-5a (이 commit): Compositor trait shell + 4 subclass struct. method body 는
//!   각 subclass 의 vfunc 별 별도 단계에서 1:1 포팅.

use crate::compose_break::ComposeBreakInput;
use crate::compose_layout::Break;
use crate::glyph::Glyph;

// ============================================================
// NumberingEntry — ComposeNumbering 의 출력 / ComposeBullet 의 입력
// ============================================================

/// `std::vector<std::pair<CharItemView const*, std::pair<unsigned int, bool>>>` 의 한 원소.
///
/// `PptCompositor::ComposeNumbering` 이 paragraph 마다 하나씩 append 하고,
/// `PptCompositor::ComposeBullet` 이 이를 읽어 글머리표 render 객체를 만든다.
/// `Composition::Repair` 에서 한 repair 호출당 하나의 `Vec<NumberingEntry>` (raw `local_80`)
/// 가 segment loop 전체에 걸쳐 누적된다. **layout 출력 (line break / row 위치) 에는 영향
/// 없음** — numbering/bullet 전용 내부 상태.
///
/// raw element layout (16B): `+0x00` = `CharItemView const*`, `+0x08` = `pair<uint,bool>`
/// (`+0x08` uint = number, `+0x0c` byte = bool).
///
/// ## Rust schema 차이 — `view: Box<CharItemView>` 에서 `key: usize` 로 전환 (2026-05-15 수정)
///
/// 이전 schema 는 `view: Box<CharItemView>` 였으나 이는 `get_para_item_view` 의 owned clone
/// 이라 raw 의 pointer identity 가 보존되지 않아 `ComposeBullet` 의 numbering vector lookup
/// (`PptCompositor__ComposeBullet_00307468.txt:165-172`) 가 영원히 fail → byte 출력 불일치.
///
/// 새 schema 는 raw 와 동일하게 **composition 내부 CR CharItemView 의 raw pointer cast**
/// (`&CharItemView as *const _ as usize`) 를 키로 사용. `find_para_cr_view` 가 producer 측
/// 키 derivation, `lookup_starting_numbering` 가 consumer 측 lookup — 동일 cast 메소드라
/// raw `lVar3` (composition 내부 ptr) 와 byte-equivalent.
///
/// `level` / `bullet_start` 캐시: raw `ComposeNumbering` 의 backward scan
/// (`0x306cf8-e14`) 은 매 iteration 마다 `view.+0x20.ParaProperty` 를 deref 해서
/// `Contains(0x902)?GetLevel():..` 와 `Bullet.GetType()==3?startAt:..` 를 추출. ParaProperty
/// 는 paragraph 의 immutable property bag (push 시점=scan 시점 동일 값) 이므로 push 시점에
/// `Option<i32>` 로 cache → byte-equivalent. `Some(v)` = raw 의 "추출 가능" 분기, `None` =
/// raw 의 "갱신 안 함 (carry-over)" 분기.
#[derive(Debug, Clone)]
pub struct NumberingEntry {
    /// `+0x00` — raw 의 `CharItemView const*` 의 `as *const _ as usize`. composition 내부
    /// CR CharItemView 의 안정 주소. `ComposeBullet` 의 lookup key.
    pub key: usize,
    /// `+0x08` (uint) — 이 paragraph 의 numbering 번호.
    pub number: u32,
    /// `+0x0c` (bool) — `(to - from) < 2` (단문 라인 여부). raw `uVar11` bit 32.
    pub is_short_line: bool,
    /// raw scan 의 `if (Contains(0x902)) iVar4 = GetLevel()` 분기 캐시. `None` = ParaProperty
    /// 가 0x902 없음 (raw 의 "갱신 안 함" 분기 = carry-over 유지).
    pub level: Option<i32>,
    /// raw scan 의 `if (Bullet.GetType()==3) local_84 = startAt` 분기 캐시. `None` = bullet 이
    /// AutoNumber 아님 (raw 의 "갱신 안 함" 분기 = carry-over 유지).
    pub bullet_start: Option<i32>,
}

/// `Hnc::Shape::Text::Compositor` virtual interface.
///
/// 한컴 base `Compositor` 는 abstract. RTTI hierarchy 의 각 subclass vtable (raw dump
/// `/tmp/hft_scripts/repair/compositor_vtables.txt`) 의 object-vptr-relative slot:
/// ```text
/// vptr+0x00 = ~D1     vptr+0x08 = ~D0      vptr+0x10 = Clone
/// vptr+0x18 = ComposeNumbering             vptr+0x20 = ComposeBullet
/// vptr+0x28 = ComposeBreak                 vptr+0x30 = ComposeLayout
/// ```
pub trait Compositor: std::fmt::Debug {
    /// `Clone()` (vfunc[2], vptr+0x10).
    fn clone_compositor(&self) -> Box<dyn Compositor>;

    /// `ComposeNumbering(int from, int to, Composition const*, vector&)` (vfunc[3], vptr+0x18).
    ///
    /// SimpleCompositor (`0x303e30`) / ColCompositor (`0x305904`) / ArrayCompositor
    /// (`0x304bf4`) 의 raw body 는 **단일 `ret`** (`return param_1` = `from`) — 즉
    /// 완전 no-op. PptCompositor (`0x306b40`, 1056B) 만 실제 body 를 가지며 override.
    ///
    /// raw signature 의 3·4번째 인자 (`Composition const*`, `vector<NumberingEntry>&`) 는
    /// no-op subclass 에선 미사용이지만 PptCompositor 가 필요로 함. 반환값 (`param_1` 또는
    /// PptCompositor 의 garbage) 은 호출처 `Composition::Repair` 가 무시 → `()` 로 모델.
    /// default impl = no-op.
    fn compose_numbering(
        &mut self,
        _from: i32,
        _to: i32,
        _composition: &dyn Glyph,
        _numbering: &mut Vec<NumberingEntry>,
    ) {
    }

    /// `ComposeBullet(int from, int to, vector const&, Composition*)` (vfunc[4], vptr+0x20).
    ///
    /// SimpleCompositor (`0x303e34`) / ColCompositor (`0x305908`) / ArrayCompositor
    /// (`0x304bf8`) 의 raw body 는 **단일 `ret`** (`return param_1`) — no-op.
    /// PptCompositor (`0x307468`, 1072B) 만 실제 body — `numbering` 을 읽고 `composition`
    /// 의 CharItemView `+0x98` (render path) 를 mutate. default impl = no-op.
    ///
    /// **Provider 인자**: raw 는 macOS CoreText 를 글로벌 함수 (`libhsp.dylib` 의 GDI shim)
    /// 로 직접 호출. Rust 는 mock/real provider 의 dispatch 를 위해 caller 가 주입. no-op
    /// impl 은 무시. raw 의 vtable 시그니처에는 없는 추가 인자이나, layout 출력 byte-eq 와는
    /// 무관 (provider 자체는 한컴 PDF 와 동일한 측정 결과를 반환하도록 구현).
    fn compose_bullet(
        &mut self,
        _from: i32,
        _to: i32,
        _numbering: &[NumberingEntry],
        _composition: &mut dyn Glyph,
        _ct_provider: &dyn crate::font_metric::CoreTextFontProvider,
        _gm_provider: &dyn crate::font_metric::GlobalMetricProvider,
    ) {
    }

    /// `ComposeBreak(widths&, stretches&, shrinks&, penalties&, heights&, Composition*, from, to,`
    /// `&out)` (vfunc[5], vptr+0x28) — 한 paragraph 의 line break char index 들 결정.
    ///
    /// raw signature 의 `Composition*` 는 `ColCompositor`/`Simple`/`Array` 의 ComposeBreak
    /// body 에서 미사용 (RE 검증) 이나 `PptCompositor::ComposeBreak` (`0x307af4`) 가 필요로
    /// 함 — 인자로 유지. `from`/`to` 도 동일. 반환 = 각 line 의 마지막 char index
    /// (`breaks.len()` = line count).
    fn compose_break(
        &mut self,
        widths: &[f32],
        stretches: &[f32],
        shrinks: &[f32],
        penalties: &[i32],
        heights: &[f32],
        composition: &dyn Glyph,
        from: i32,
        to: i32,
    ) -> Vec<u32>;

    /// `ComposeLayout(Composition*, Type, Break&, from, to, &output)` (vfunc[6], vptr+0x30).
    /// 한 라인의 children 을 output 컨테이너에 append.
    fn compose_layout(
        &mut self,
        composition: &dyn Glyph,
        composition_type: u32,
        break_range: &Break,
        from: i32,
        to: i32,
        output: &mut dyn Glyph,
    );
}

// ============================================================
// SimpleCompositor — base intermediate (vtable @ 0x7804a0)
// ============================================================

/// `Hnc::Shape::Text::SimpleCompositor` — 8B 빈 객체 (vtable only).
///
/// raw ctor `FUN_00303de0` (sz=16):
/// ```c
/// SimpleCompositor::SimpleCompositor(SimpleCompositor *this) {
///     *(undefined ***)this = &PTR__SimpleCompositor_007804b0;  // vtable+0x10
/// }
/// ```
/// raw `Create` (`FUN_00304b60`, sz=52): `operator_new(8)` + ctor.
/// → **field 없음** (`operator_new(8)` 가 vtable ptr 만). Rust 빈 struct 가 byte-eq.
///
/// 메소드:
/// - `+0x18 ComposeNumbering` (`0x303e30`) / `+0x20 ComposeBullet` (`0x303e34`) — raw
///   `ret` no-op. trait default 사용.
/// - `+0x28 ComposeBreak` (`FUN_00303e38`, sz=328) — `simple_compose_break`.
/// - `+0x30 ComposeLayout` (`FUN_00303f80`, sz=2440) — `simple_compose_layout`.
#[derive(Debug, Clone, Default)]
pub struct SimpleCompositor {}

impl Compositor for SimpleCompositor {
    /// `SimpleCompositor::Clone` — raw 는 `operator_new(8)` + vtable copy (data 없음).
    /// Rust `#[derive(Clone)]` 가 동일.
    fn clone_compositor(&self) -> Box<dyn Compositor> {
        Box::new(self.clone())
    }

    // compose_numbering / compose_bullet: trait default (raw `ret` no-op).

    /// `SimpleCompositor::ComposeBreak` (`FUN_00303e38`, 328B) — `simple_compose_break` 로
    /// 위임. raw body 가 widths + heights 만 사용 — `stretches`/`shrinks`/`penalties`/
    /// `composition`/`from`/`to` 는 raw 에서 미사용.
    fn compose_break(
        &mut self,
        widths: &[f32],
        _stretches: &[f32],
        _shrinks: &[f32],
        _penalties: &[i32],
        heights: &[f32],
        _composition: &dyn Glyph,
        _from: i32,
        _to: i32,
    ) -> Vec<u32> {
        crate::simple_compositor::simple_compose_break(widths, heights)
    }

    /// `SimpleCompositor::ComposeLayout` (`FUN_00303f80`, 2440B) — `simple_compose_layout`
    /// 로 위임. raw body 가 `from`/`to` 를 break_range 와 별도로 사용 안 함 — trait 의
    /// `from`/`to` 인자는 break_range 의 중복 정보. (raw param 5/6 도 stack 인자로 들어오나
    /// body 미사용 — RE 검증 simple_compositor.rs doc).
    fn compose_layout(
        &mut self,
        composition: &dyn Glyph,
        composition_type: u32,
        break_range: &Break,
        _from: i32,
        _to: i32,
        output: &mut dyn Glyph,
    ) {
        crate::simple_compositor::simple_compose_layout(
            composition,
            composition_type,
            *break_range,
            output,
        );
    }
}

// ============================================================
// ColCompositor — multi-column (vtable @ 0x780530)
// ============================================================

/// `Hnc::Shape::Text::ColCompositor` — 24B `{vptr@0, col_width@0x08, line_count@0x10}`.
///
/// ctor `ColCompositor(float param_1, unsigned long param_2)` (`0x305890` C1 / `0x3058a8`
/// C2) — raw:
/// ```c
/// *this = &PTR__ColCompositor_00780540;   // vptr
/// *(float*)(this + 0x08) = param_1;       // col_width
/// *(ulong*)(this + 0x10) = param_2;       // line_count
/// ```
/// 메소드:
/// - `FUN_0030590c` — `ColCompositor::ComposeBreak` (ported: `compose_break::compose_break`,
///   line_count = `this+0x10` in/out).
/// - `FUN_00305d60` — `ColCompositor::ComposeLayout` (ported: `compose_layout::compose_layout`,
///   `col_natural_size` = `this+0x08`).
/// - `ComposeNumbering` (`0x305904`) / `ComposeBullet` (`0x305908`) — raw `ret` no-op.
/// - `Clone` (`0x3058cc`) — `operator_new(0x18)` + vptr + 16B copy of `+0x08`/`+0x10`.
/// - `~ColCompositor` (`0x3058c4`) — raw `ret` (no-op).
#[derive(Debug, Clone, Default)]
pub struct ColCompositor {
    /// `+0x08` — ctor `param_1`. `ColCompositor::ComposeLayout` 의 `col_natural_size`
    /// (empty-range extra glue 의 cross-axis 크기).
    pub col_width: f32,
    /// `+0x10` — ctor `param_2`. `ColCompositor::ComposeBreak` 의 line-count member.
    /// in/out: `0` 으로 들어오면 `1` 로 set 되고, 최종 line count 로 갱신됨.
    pub line_count: u64,
}

impl ColCompositor {
    /// raw ctor `ColCompositor(float col_width, unsigned long line_count)`.
    pub fn new(col_width: f32, line_count: u64) -> Self {
        Self {
            col_width,
            line_count,
        }
    }
}

impl Compositor for ColCompositor {
    /// `ColCompositor::Clone` (`0x3058cc`) — `operator_new(0x18)` + vptr + 16B `+0x08`/`+0x10`
    /// 복사. Rust `#[derive(Clone)]` 가 동일 (col_width + line_count 복사).
    fn clone_compositor(&self) -> Box<dyn Compositor> {
        Box::new(self.clone())
    }

    // compose_numbering / compose_bullet: trait default (raw `ret` no-op).

    /// `ColCompositor::ComposeBreak` (`FUN_0030590c`) — `compose_break::compose_break` 로 위임.
    ///
    /// raw param 매핑 (Ghidra `param_N` = x0=this 포함 번호): `widths`=param_2(x1),
    /// `stretches`=param_3(x2), `shrinks`=param_4(x3), `penalties`=param_5(x4, `vector<int>`),
    /// `heights`=param_6(x5), `Composition*`=param_7(x6), `from`=param_8(x7), `to`=stack0,
    /// `&out`=stack1. body 에서 **실제 사용은 `widths`(param_2) + `heights`(param_6, =라인별
    /// column width) + `this+0x10`(line_count) 뿐** — stretches/shrinks/penalties/Composition*/
    /// from/to 는 미사용 (`compose_break.rs` 검증). `ComposeBreakInput` 의 `composition_widths`
    /// 가 곧 raw param_6 (= 이 함수의 `heights` 인자).
    fn compose_break(
        &mut self,
        widths: &[f32],
        stretches: &[f32],
        shrinks: &[f32],
        _penalties: &[i32],
        heights: &[f32],
        _composition: &dyn Glyph,
        from: i32,
        to: i32,
    ) -> Vec<u32> {
        let input = ComposeBreakInput {
            widths,
            _stretches: stretches,
            _shrinks: shrinks,
            // raw param_5 (penalties, `vector<int>`) 는 ColCompositor::ComposeBreak body 에서
            // 미사용 — `ComposeBreakInput._heights` 는 `&[f32]` 타입이라 빈 slice 전달.
            _heights: &[],
            // raw param_6 = 라인별 column width 배열 (Repair 가 `span - origins` 로 채운 것).
            composition_widths: heights,
            _from: from,
            _to: to,
        };
        // `compose_break.rs` 는 `&mut u32` 를 받음 — `+0x10` (u64) 와 값 동등 (작은 line count).
        let mut lc = self.line_count as u32;
        let out = crate::compose_break::compose_break(&input, &mut lc);
        self.line_count = lc as u64;
        out.breaks
    }

    /// `ColCompositor::ComposeLayout` (`FUN_00305d60`) — `compose_layout::compose_layout` 로
    /// 위임. `col_natural_size` = `self.col_width` (`+0x08`). `from`/`to` 인자는 `break_range`
    /// (`{from, to}`) 와 중복이므로 free fn 에 `break_range` 만 전달.
    fn compose_layout(
        &mut self,
        composition: &dyn Glyph,
        composition_type: u32,
        break_range: &Break,
        _from: i32,
        _to: i32,
        output: &mut dyn Glyph,
    ) {
        crate::compose_layout::compose_layout(
            composition,
            composition_type,
            *break_range,
            output,
            self.col_width,
        );
    }
}

// ============================================================
// PptCompositor — Powerpoint-style (vtable @ 0x780578)
// ============================================================

/// `Hnc::Shape::Text::PptCompositor`.
///
/// 메소드 별:
/// - `FUN_00306b40` (sz=1056) — `PptCompositor::ComposeNumbering`.
/// - `FUN_00307468` (sz=1072) — `PptCompositor::ComposeBullet`.
/// - `FUN_00307af4` (sz=1520) — `PptCompositor::ComposeBreak`.
/// - `FUN_00308248` (sz=9712) — `PptCompositor::ComposeLayout` (이미 ported as
///   `ppt_compose_layout::ppt_compose_layout`).
/// - Helpers: `IsFirstLineOnPara` (0x306ffc), `GetParaItemView` (0x3071d8),
///   `GetFirstCharItemViewOnPara` (0x30794c) — `ppt_compositor.rs` 에 ported.
#[derive(Debug, Clone, Default)]
pub struct PptCompositor {
    // TODO B-5 후속: ctor decompile 추출 후 정확한 field layout.
}

// ============================================================
// ArrayCompositor — fixed-array layout
// ============================================================

/// `Hnc::Shape::Text::ArrayCompositor` — 16B 객체 `{vtable@0, divisor: u64@0x08}`.
///
/// raw ctor `FUN_00304b94` / `FUN_00304ba4` (sz=16, C1/C2 동일 body):
/// ```c
/// ArrayCompositor::ArrayCompositor(ArrayCompositor *this, ulong param_1) {
///     *(undefined ***)this = &PTR__ArrayCompositor_007804f8;  // vtable+0x10
///     *(ulong *)(this + 8) = param_1;                          // divisor
/// }
/// ```
///
/// 메소드:
/// - `+0x18 ComposeNumbering` (`0x304bf4`) / `+0x20 ComposeBullet` (`0x304bf8`) — raw
///   `ret` no-op. trait default 사용.
/// - `+0x28 ComposeBreak` (`FUN_00304bfc`, sz=352) — `array_compose_break`. raw body 가
///   `widths` 와 `self.divisor` (+0x08) 만 사용.
/// - `+0x30 ComposeLayout` (`FUN_00304d5c`, sz=2440) — `SimpleCompositor::ComposeLayout`
///   (`FUN_00303f80`) 와 raw decompile **完全 동일** (LAB_ 주소만 다름). `simple_compose_layout`
///   재사용 — byte-equiv.
#[derive(Debug, Clone, Default)]
pub struct ArrayCompositor {
    /// `+0x08` — raw ctor `param_1`. `ArrayCompositor::ComposeBreak` 의 divisor.
    pub divisor: u64,
}

impl ArrayCompositor {
    /// raw ctor `ArrayCompositor(unsigned long divisor)`.
    pub fn new(divisor: u64) -> Self {
        Self { divisor }
    }
}

impl Compositor for ArrayCompositor {
    /// `ArrayCompositor::Clone` — raw `operator_new(0x10)` + vtable + `+0x08` byte copy.
    /// Rust `#[derive(Clone)]` 가 동일 (divisor 복사).
    fn clone_compositor(&self) -> Box<dyn Compositor> {
        Box::new(self.clone())
    }

    // compose_numbering / compose_bullet: trait default (raw `ret` no-op).

    /// `ArrayCompositor::ComposeBreak` (`FUN_00304bfc`, 352B) — `array_compose_break` 로
    /// 위임. raw body 는 `widths` 와 `self.divisor` 만 사용 — 다른 인자 미사용
    /// (`array_compositor.rs` doc 검증).
    fn compose_break(
        &mut self,
        widths: &[f32],
        _stretches: &[f32],
        _shrinks: &[f32],
        _penalties: &[i32],
        _heights: &[f32],
        _composition: &dyn Glyph,
        _from: i32,
        _to: i32,
    ) -> Vec<u32> {
        crate::array_compositor::array_compose_break(widths, self.divisor)
    }

    /// `ArrayCompositor::ComposeLayout` (`FUN_00304d5c`, 2440B) — `simple_compose_layout`
    /// 로 위임 (raw decompile 完全 동일).
    fn compose_layout(
        &mut self,
        composition: &dyn Glyph,
        composition_type: u32,
        break_range: &Break,
        _from: i32,
        _to: i32,
        output: &mut dyn Glyph,
    ) {
        crate::simple_compositor::simple_compose_layout(
            composition,
            composition_type,
            *break_range,
            output,
        );
    }
}

// ============================================================
// PptCompositor — trait Compositor impl (B-Compositor-G)
// ============================================================
//
// 4 메소드 모두 raw 1:1 outer (`ppt_compose_break` / `ppt_compose_layout` /
// `ppt_compose_numbering` / `ppt_compose_bullet`) 로 forward.
//
// raw vtable 매핑 (compositor doc table):
//   vptr+0x18 = ComposeNumbering  (FUN_00306b40, 1056B)
//   vptr+0x20 = ComposeBullet     (FUN_00307468, 1072B)
//   vptr+0x28 = ComposeBreak      (FUN_00307af4, 1520B)
//   vptr+0x30 = ComposeLayout     (FUN_00308248, 9712B)
//
// 각 메소드는 raw 시그니처를 trait Compositor 의 시그니처로 mapping. trait 시그니처는
// raw 의 4-method 의 union (각 method 가 필요로 하는 인자 집합).

impl Compositor for PptCompositor {
    /// `PptCompositor::Clone` — raw 는 빈 객체 alloc + vtable copy (data 없음). Rust:
    /// `#[derive(Clone)]` 가 동일 (PptCompositor 가 빈 struct).
    fn clone_compositor(&self) -> Box<dyn Compositor> {
        Box::new(self.clone())
    }

    /// `PptCompositor::ComposeNumbering` (`FUN_00306b40`) — outer 로 forward.
    fn compose_numbering(
        &mut self,
        from: i32,
        to: i32,
        composition: &dyn Glyph,
        numbering: &mut Vec<NumberingEntry>,
    ) {
        crate::ppt_compose_numbering::ppt_compose_numbering(from, to, composition, numbering);
    }

    /// `PptCompositor::ComposeBullet` (`FUN_00307468`) — outer 로 forward.
    /// raw 의 (from=param_1, to=param_2, &numbering=param_3, composition=param_4) 매핑.
    fn compose_bullet(
        &mut self,
        from: i32,
        to: i32,
        numbering: &[NumberingEntry],
        composition: &mut dyn Glyph,
        ct_provider: &dyn crate::font_metric::CoreTextFontProvider,
        gm_provider: &dyn crate::font_metric::GlobalMetricProvider,
    ) {
        crate::ppt_compose_bullet::ppt_compose_bullet(
            from,
            to,
            numbering,
            composition,
            ct_provider,
            gm_provider,
        );
    }

    /// `PptCompositor::ComposeBreak` (`FUN_00307af4`) — outer 로 forward.
    /// trait 의 `Vec<u32>` 반환 = `ppt_compose_break` 의 반환과 일치 (raw return value).
    fn compose_break(
        &mut self,
        widths: &[f32],
        stretches: &[f32],
        shrinks: &[f32],
        penalties: &[i32],
        heights: &[f32],
        composition: &dyn Glyph,
        from: i32,
        to: i32,
    ) -> Vec<u32> {
        crate::ppt_compose_break::ppt_compose_break(
            widths, stretches, shrinks, penalties, heights, composition, from, to,
        )
    }

    /// `PptCompositor::ComposeLayout` (`FUN_00308248`) — outer 로 forward.
    /// raw 의 (composition=param_1, type=param_3, break=*param_4, p5=param_5, p6=param_6,
    /// output=*param_7) 매핑.
    fn compose_layout(
        &mut self,
        composition: &dyn Glyph,
        composition_type: u32,
        break_range: &Break,
        from: i32,
        to: i32,
        output: &mut dyn Glyph,
    ) {
        crate::ppt_compose_layout::ppt_compose_layout(
            composition,
            composition_type,
            *break_range,
            from,
            to,
            output,
        );
    }
}

// ============================================================
// Tests — PptCompositor trait wire smoke
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font_metric::{
        CoreTextFontProvider, GlobalFontMetrics, GlobalMetricProvider, SystemFont,
    };

    /// Zero stub provider — ColCompositor/Simple/Array 의 no-op path 와 PptCompositor 의
    /// no-bullet path 모두 측정 호출 안 함. PptCompositor 의 실제 measurement 검증은
    /// `ppt_compose_bullet::tests` 의 e2e 가 담당.
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

    /// `PptCompositor::Clone` (raw `FUN_00306b1c`) — 빈 객체 alloc + vtable copy. Rust 의
    /// `#[derive(Clone)]` 가 동일.
    #[test]
    fn ppt_compositor_clone_compositor_returns_boxed() {
        let mut ppt = PptCompositor::default();
        let cloned: Box<dyn Compositor> = ppt.clone_compositor();
        // smoke: cloned 가 valid Compositor (no-op 메소드 호출 panic-free).
        let _: Box<dyn Compositor> = cloned.clone_compositor();
        // original 도 여전히 valid — trait method 호출 가능.
        let _ = ppt.clone_compositor();
    }

    /// `compose_numbering` — outer `ppt_compose_numbering` 으로 forward.
    /// 빈 composition → IsFirstLineOnPara=true (idx<0) but GetParaItemView=None → early return,
    /// numbering 미변경.
    #[test]
    fn ppt_compositor_compose_numbering_forwards_to_outer() {
        let mut ppt = PptCompositor::default();
        let comp = crate::composition::LRComposition::new(None, None, None, 100.0);
        let mut numbering: Vec<NumberingEntry> = Vec::new();
        ppt.compose_numbering(-1, 0, &comp, &mut numbering);
        // 빈 composition → outer 가 find_para_cr_view None → push 안 함. panic-free.
        assert!(numbering.is_empty());
    }

    /// `compose_bullet` — outer `ppt_compose_bullet` 으로 forward.
    /// 빈 composition → IsFirstLineOnPara(0) = false (composition 비어 있어 panic),
    ///   안전하게 idx<0 으로 호출 → outer 가 early return.
    #[test]
    fn ppt_compositor_compose_bullet_forwards_to_outer() {
        let mut ppt = PptCompositor::default();
        let mut comp = crate::composition::LRComposition::new(None, None, None, 100.0);
        let numbering: Vec<NumberingEntry> = Vec::new();
        // idx=-1 / para_idx=0 / empty composition → IsFirstLineOnPara(-1)=true →
        //   find_para_cr_view(0) on empty comp → None (count==0 → idx>=count → None) → return.
        ppt.compose_bullet(-1, 0, &numbering, &mut comp, &ZeroCt, &ZeroGm);
        // panic-free. composition 미변경.
    }

    /// `compose_break` — outer `ppt_compose_break` 으로 forward.
    /// `widths.len() != penalties.len()` → 빈 Vec 반환 (raw 0x307b1c-38 의 가드).
    #[test]
    fn ppt_compositor_compose_break_forwards_to_outer() {
        let mut ppt = PptCompositor::default();
        let comp = crate::composition::LRComposition::new(None, None, None, 100.0);
        // widths.len()=2, penalties.len()=0 → 가드 트리거.
        let result = ppt.compose_break(
            &[10.0, 10.0], &[0.0, 0.0], &[0.0, 0.0],
            &[], &[100.0],
            &comp, 0, 1,
        );
        assert!(result.is_empty(), "widths/penalties length mismatch → empty");
    }

    /// `compose_layout` — outer `ppt_compose_layout` 으로 forward.
    /// 최소 1-CR composition (raw 의 GetComponent 가 idx>=count 면 throw 라 빈 composition
    /// 은 자체가 invalid input). Break(0, 0) — 1-item range, p5=-1, p6=0, type=1.
    /// 모든 stage default path, panic-free.
    #[test]
    fn ppt_compositor_compose_layout_forwards_to_outer() {
        use crate::glyph::CharItemView;
        let mut ppt = PptCompositor::default();
        let mut comp = crate::composition::LRComposition::new(None, None, None, 100.0);
        comp.inner
            .items
            .push(Some(Box::new(CharItemView { char_code: 0x0d, ..Default::default() })));
        let mut output = crate::composition::LRComposition::new(None, None, None, 100.0);
        ppt.compose_layout(
            &comp, 1, &Break::new(0, 0), -1, 0, &mut output,
        );
        // panic-free.
    }

    // ============================================================
    // SimpleCompositor / ArrayCompositor trait wire smoke tests
    // ============================================================

    /// `SimpleCompositor::Clone` — `#[derive(Clone)]` = raw `operator_new(8)` + vtable copy.
    #[test]
    fn simple_compositor_clone_compositor_returns_boxed() {
        let mut s = SimpleCompositor::default();
        let _: Box<dyn Compositor> = s.clone_compositor();
    }

    /// `SimpleCompositor::compose_numbering` / `compose_bullet` — raw `ret` no-op (trait
    /// default). numbering 미변경, composition 미변경.
    #[test]
    fn simple_compositor_numbering_bullet_are_noop() {
        let mut s = SimpleCompositor::default();
        let comp = crate::composition::LRComposition::new(None, None, None, 100.0);
        let mut numbering: Vec<NumberingEntry> = Vec::new();
        s.compose_numbering(0, 0, &comp, &mut numbering);
        assert!(numbering.is_empty(), "Simple compose_numbering = raw ret no-op");

        let mut mut_comp = crate::composition::LRComposition::new(None, None, None, 100.0);
        s.compose_bullet(0, 0, &numbering, &mut mut_comp, &ZeroCt, &ZeroGm);
        // raw `ret` no-op — composition 미변경 (panic-free).
    }

    /// `SimpleCompositor::compose_break` — `simple_compose_break(widths, heights)` 로 forward.
    /// 빈 widths → 빈 결과.
    #[test]
    fn simple_compositor_compose_break_forwards_to_free_fn() {
        let mut s = SimpleCompositor::default();
        let comp = crate::composition::LRComposition::new(None, None, None, 100.0);
        let result = s.compose_break(&[], &[], &[], &[], &[], &comp, 0, 0);
        assert!(result.is_empty(), "empty widths → no breaks");
    }

    /// `SimpleCompositor::compose_layout` — `simple_compose_layout` 로 forward.
    /// 1-CR composition, panic-free.
    #[test]
    fn simple_compositor_compose_layout_forwards_to_free_fn() {
        use crate::glyph::CharItemView;
        let mut s = SimpleCompositor::default();
        let mut comp = crate::composition::LRComposition::new(None, None, None, 100.0);
        comp.inner
            .items
            .push(Some(Box::new(CharItemView { char_code: 0x0d, ..Default::default() })));
        let mut output = crate::composition::LRComposition::new(None, None, None, 100.0);
        s.compose_layout(&comp, 1, &Break::new(0, 0), 0, 0, &mut output);
    }

    /// `ArrayCompositor::new(divisor)` + Clone — `+0x08` field 보존.
    #[test]
    fn array_compositor_new_and_clone() {
        let a = ArrayCompositor::new(7);
        assert_eq!(a.divisor, 7);
        let cloned = a.clone();
        assert_eq!(cloned.divisor, 7);
    }

    /// `ArrayCompositor::compose_break` — `array_compose_break(widths, divisor)` 로 forward.
    /// 빈 widths → 빈 결과.
    #[test]
    fn array_compositor_compose_break_forwards_to_free_fn() {
        let mut a = ArrayCompositor::new(2);
        let comp = crate::composition::LRComposition::new(None, None, None, 100.0);
        let result = a.compose_break(&[], &[], &[], &[], &[], &comp, 0, 0);
        assert!(result.is_empty());
    }

    /// `ArrayCompositor::compose_layout` — `simple_compose_layout` 와 동일 (raw decompile
    /// 完全 동일). panic-free.
    #[test]
    fn array_compositor_compose_layout_forwards_to_free_fn() {
        use crate::glyph::CharItemView;
        let mut a = ArrayCompositor::new(3);
        let mut comp = crate::composition::LRComposition::new(None, None, None, 100.0);
        comp.inner
            .items
            .push(Some(Box::new(CharItemView { char_code: 0x0d, ..Default::default() })));
        let mut output = crate::composition::LRComposition::new(None, None, None, 100.0);
        a.compose_layout(&comp, 1, &Break::new(0, 0), 0, 0, &mut output);
    }
}

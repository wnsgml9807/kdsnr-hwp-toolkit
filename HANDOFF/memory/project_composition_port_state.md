---
name: Composition port 상세 진행 상태 (B-5j Repair 완료)
description: composition.rs / compositor.rs / glyph.rs / layout.rs / placement.rs 의 함수별 port 상태 + 다음 세션 진입점. 8번째 세션 종료
type: project
originSessionId: 8번째 세션 (2026-05-14)
---
`kdsnr-hwp-toolkit/layout-decoder/rust/src/` 의 전체 port 상태. **289 tests pass** (8번째 세션 종료, B-5i DoRepair + B-5j Repair 완료).

## -1. 8번째 세션 변경 요약 (B-5i + B-5j)

- **`Composition::DoRepair`** (`FUN_00301664`, 1584B) — `composition.rs` 의 `Composition` trait default method `do_repair(&mut self, param_1, param_2, param_3, param_4: &[i32], param_5)`. raw decompile + asm 1:1. §5 상세 참조. +7 tests.
- **`FUN_00302004`** = `std::vector<RowSegment>::insert` → Rust `Vec::insert` 등가 (별도 함수 안 만들고 do_repair 내부에서 직접 `rows.insert`).
- **`Hnc::Type::Flag` raw 확인**: `@rpath/libHncFoundation.dylib` import. libHncFoundation.dylib 임포트 후 dump — 8B, ctor=zero-init, dtor=no-op, trivially-copyable. `RowSegment.flag: u64` 모델 정확 확인 완료.
- **`Compositor` trait 재설계 + `impl Compositor for ColCompositor`** (`compositor.rs`) — vtable RTTI (Simple `0x7804a0` / Array `0x7804e8` / Col `0x780530` / Ppt `0x780578`, obj vptr = +0x10). ComposeNumbering(+0x18)/ComposeBullet(+0x20) 은 Simple/Col/Array 모두 raw `ret` no-op (trait default). `ColCompositor` = 24B `{vptr, col_width:f32@0x08, line_count:u64@0x10}`, ctor `0x305890 (float, ulong)`. compose_break → `compose_break::compose_break`, compose_layout → `compose_layout::compose_layout` 위임.
- **`Composition::Repair`** (`FUN_00300b14`, 1740B) — `composition.rs` 의 `Composition` trait default method `repair(&mut self) -> bool`. §5 상세 참조. +4 tests (289 total).

## 0. 7번째 세션 변경 요약 (B-5h + B-5e)

- **`TileReverse::Request_inner`** (`FUN_00302858`, 908B) — `layout.rs` 에 `TileReverse` plain struct + inherent `request_inner`. `Tile::Request_inner` 와 trim+sum **byte-identical**, 유일 차이 = 출력 `alignment` 1.0 (Tile 0.0). `tile_request_simple_sum`/`tile_request_trim_then_sum` helper 재사용. `Composition::CreateItem` 이 stack 임시객체로 `bl 0x00302858` 직접 호출하므로 Layout trait impl 은 안 함 (vtable dispatch 경로 없음 — Allocate_inner `FUN_0034e640` / Clone `FUN_0034e5f8` 은 후속).
- **`Composition::CreateItem`** (`FUN_003000a8`, 2408B) — `composition.rs` 의 `Composition` trait default method `create_item(&mut self, &mut Break, from, to, force)` + 자유함수 `composition_create_item<C: Composition + ?Sized>`. raw decompile + asm 1:1. §5 상세 참조.
- **`Composition::View`** (`FUN_002ffe8c`, 468B) — `composition.rs` 의 `Composition` trait default method `view(&mut self, from, to)`. `rows` 순회 → in-view+invalid `create_item(force=true)` / out-of-view+valid `create_item(force=false)` → `parent_glyph.replace(idx*2, item)` (take/put-back) → `flag_59` clear. `RowSegment`(24B `{begin@0,end@4,origin_x@8,origin_y@0xc,flag@0x10}`) 가 `Break` 와 동일 layout — 한컴은 row 복사해 `Break*` 로 CreateItem 호출, 원본 rows 수정 안 함. CreateItem from/to 인자 = `0,0` (asm `mov w2/w3,#0`).
- **부수 보정**:
  - `placement.rs`: `PlaceNatural` 에 `{direction: i32, span: f32}` 필드 (16B layout). `Placement` trait 에 `as_any` (4 impl).
  - `glyph.rs`: `MonoGlyph.child` → `Option<Box<dyn Glyph>>` (SharePtr null 가능). doc 정정 — `MonoGlyph` struct 는 사실 bare MonoGlyph(0x780e60) 가 아니라 vtable 0x781168 `Placement` glyph (`{body@+8, strategy@+0x10}`) 를 모델.
  - `compose_layout.rs`: `Break` 에 `flags: u64` (Hancom `+0x10` tagged flags).
  - `value_types.rs`: `Requisition::INVALID` 상수 (`{x:(-1e8,0,0,0), y:(-1e8,0,0,0), penalty:0}`).
  - `composition.rs`: `CompositionState` 에 `flag_59: bool` (`+0x59`, ctor/SetSpan=true / View=false).

### 6번째 세션 요약 (참고)
- **B-5f-A byte-eq 완료**: Layout/Tile/Align/Superpose/Box/LayoutFactory. Tile::Allocate_inner FMA `mul_add`, Align saturate/3-branch 정정, Box recompute helpers, `BoundsRect::INIT` = `(+1e8,+1e8,-1e8,-1e8)`.
- **B-5g 완료**: `impl Composition for LR/TBComposition` + `create_line_item`.

## 1. 정공법 정책 (사용자 명시)

- **stub / `unimplemented!()` / `// TODO 후속 단계` 우회 금지** — 두 번 화남.
- **추측 금지** — raw decompile / raw asm / vtable dump 만 신뢰.
- byte-equivalent LineSeg 출력 목표 (한컴 macOS HncDrawingEngine 와 1:1).
- 매 함수 doc comment 에 raw 인용 필수.
- **B-5f-A 옵션 A** (full byte-equivalent) 선택됨. semantic-equiv 우회 금지.

## 2. 파일 구조 + 진행 상태 (6번째 세션 종료)

```text
layout-decoder/rust/src/
├── lib.rs               — module 선언 + re-exports (TileReverse 추가)
├── value_types.rs       — Requirement/Requisition(+INVALID 상수)/Allotment/Allocation/BoundsRect/...
├── placement.rs         — Placement trait(+as_any) + PlaceFix/PlaceMargin/PlaceNatural(+direction,span)/PlaceCenter
├── glyph.rs             — Glyph trait + 13 subclass + Box_ + MonoGlyph(child→Option; vtable 0x781168 Placement glyph)
├── layout.rs            — Layout trait + Tile + Align + Superpose + TileReverse (NEW 7번째)
├── layout_factory.rs    — LayoutFactory ZST + CreateHBox/CreateVBox
├── runtime.rs / properties.rs
├── compose_break.rs     — ColCompositor::ComposeBreak (free fn)
├── compose_layout.rs    — ColCompositor::ComposeLayout (free fn) + Break(+flags) + helper
├── ppt_compositor.rs / ppt_compose_layout.rs
├── compositor.rs        — Compositor trait + 4 subclass struct shells (body 대기)
└── composition.rs       — CompositionState + Composition trait(+create_item) + LR/TBComposition
```

## 3. Layout hierarchy

### Layout trait — 5 vfuncs

| vfunc | offset | method | raw |
|-------|--------|--------|-----|
| 0 | +0 | dtor1 | (no-op for PODs) |
| 1 | +8 | dtor0 / delete | |
| 2 | +16 | Clone | |
| 3 | +24 | Request_inner | `(self, &[Requisition], &mut Requisition)` |
| 4 | +32 | Allocate_inner | `(self, &Allocation, &[Requisition], &mut [Allocation])` |

Tile/Align/Superpose 모두 동일 인터페이스. raw vtable @ `/tmp/hft_scripts/hbox_layer/vtables.txt`.

### Tile (56B, vtable 0x781d50) — 1:1 port 상태

- struct: `direction: i32, cached_req: Requisition, trim_trailing_hint: bool`
- `Tile::Request_inner` (`FUN_00302478`, 904B):
  - simple path (flag==0): direction 축의 natural/stretch/shrink 합산, INVALID sentinel skip ✅ byte-eq
  - trim path (flag==1): trim1 (penalty==1000) + trim2 (penalty>1 unsigned) + memmove (trim1 의 1000s 를 trim2 결과 뒤에 붙임) + 합산 ✅ byte-eq
  - 캐시 결과: `self.cached_req` 갱신 ✅
- `Tile::Allocate_inner` (`FUN_0034d90c`, 768B):
  - Phase 1: target 계산 (cached_align 0/1/between 분기 + ratio_a/ratio_b min) ✅ byte-eq
  - Phase 2: stretch/shrink/no-scale loop (sentinel skip, child_origin=acc+span*align, advance by span) ✅ algorithm-eq, **byte-eq 완전 검증 필요** (fmadd vs `*`+`+` 차이)
- `Tile::Clone` (`FUN_0034d8c4`, 72B): 56B 복사 ✅ via #[derive(Clone)]

### Align (16B, vtable 0x77fab8) — partial byte-eq

- struct: `direction: i32`
- `Align::Request_inner` (`FUN_002d0bb4`, 512B):
  - SIMD 2-lane lane-max 누적 (v2/v1/v0): natural-max, stretch-min, shrink-max ✅ 1:1
  - saturate phase: v2 ← max(v2, v1), v2 ← min(v2, v0) ✅ 1:1
  - 3-branch (s4==0, s2==0, normal) 마지막 단계의 stretch/shrink 계산 — **algorithm-eq, byte-eq 검증 필요**
- `Align::Allocate_inner` (`FUN_002d0f00`, 368B):
  - child_align 0/1/between scale 계산 ✅ 1:1
  - child_span clamp to [natural-shrink, natural+stretch] ✅ algorithm-eq
  - **byte-eq: raw asm 의 정확한 분기 순서 + fcsel 비교 검증 필요**
- `Align::Clone` (`FUN_002d0b74`, 56B): 16B 복사 ✅

### Superpose (32B, vtable 0x781828) — ✅ byte-eq

- struct: `Vec<Box<dyn Layout>>`
- `Superpose::Add`/`GetCount`/`Get` ✅ 1:1
- `Superpose::Request_inner`: children iterate + each.request_inner(reqs, out) ✅
- `Superpose::Allocate_inner`: children iterate + each.allocate_inner(avail, reqs, out) ✅
- `Superpose::Clone` ✅ via manual Clone impl

### TileReverse (56B, vtable 0x781dc0, `N3Hnc5Shape4Text11TileReverseE`) — 7번째 세션 NEW

- struct: `direction: i32, cached_req: Requisition, trim_trailing_hint: bool` (Tile 과 동일 layout)
- `TileReverse::Request_inner` (`FUN_00302858`, 908B): `Tile::Request_inner` 와 trim+sum **byte-identical**, 유일 차이 = 출력 `alignment` **1.0** (Tile 0.0). `tile_request_simple_sum`/`tile_request_trim_then_sum` helper 재사용. ✅ byte-eq
- plain struct + inherent `request_inner` 만 — `Composition::CreateItem` 이 stack 임시객체로 `bl 0x00302858` 직접 호출하므로 `Layout` trait impl 안 함. `Allocate_inner` (`FUN_0034e640`), `Clone` (`FUN_0034e5f8`) 는 vtable dispatch 경로 생기면 후속 (asm dump 완료: `/tmp/hft_scripts/item_class/methods.txt`).

### Box_ (176B, vtable 0x77fd10) — 19 vfunc Glyph

- struct: `children: Vec<Option<Box<dyn Glyph>>>`, `layout: Option<Superpose>`, `cache_req_valid: bool`, `cache_bounds_valid: bool`, `cached_req: Requisition`, `cached_bounds: BoundsRect`
- vfuncs:
  - `request(&self, &mut Requisition)` ✅ writes `cached_req` to out
  - `allocate_bounds(&mut self, &Allocation, &mut BoundsRect)` ✅ 4-lane SIMD bif merge mimicked (min/max combine)
  - `allocate(&mut self, &Allocation, &mut Extension)` **TODO** — children iterate, child.allocate per RAW
  - `append/prepend/insert/remove/replace` ✅ list mutations + change()
  - `change(&mut self, idx)` ✅ invalidates both caches
  - `get_count/get_component/get_component_mut` ✅
  - `get_allotment` — **TODO** per-child cached allotment array at this[+0x70]
  - `clone_glyph` ✅ via Superpose Clone

- `recompute_request_cache` helper (raw `FUN_002e5e80`):
  - for each child: child.request → buf[i]
  - layout.request_inner(&buf, &mut cached_req)
  - cache_req_valid = true
  ✅ algorithm-eq, **byte-eq 보완 필요** (helper recompute 흐름의 vector init 정확성)
- `Box::Request` (FUN_002e601c) bounds-accumulator: cached_bounds 와 out_bounds 의 min/max merge ✅ algorithm 이해 + Rust 동등
- `Box::Allocate` (FUN_002e6348) child iterator: **TODO** byte-eq impl

### LayoutFactory (ZST, singleton)

- `get_instance()` ZST 반환 ✅
- `create_h_box()` → Box + Superpose([Tile(dir=0, trim=1), Align(dir=1)]) ✅
- `create_v_box()` → Box + Superpose([Tile(dir=1, trim=1), Align(dir=0)]) ✅

## 4. Composition

- 5번째 세션까지: 모든 list ops/accessor/scan helpers/Clone 1:1 ported.
- 6번째 세션 (B-5g): `Composition` trait + `create_line_item` (LR/TB override).
- 7번째 세션 (B-5h): `Composition` trait 에 `create_item` default method + 자유함수 `composition_create_item`. (B-5e): `Composition` trait 에 `view` default method. §5 참조.

## 5. Composition::CreateItem / View / DoRepair (B-5h/B-5e/B-5i 완료) + 다음 진입점

### B-5h — `Composition::CreateItem` (`FUN_003000a8`, 2408B) ✅ 완료

`composition.rs`: `Composition` trait default method `create_item(&mut self, &mut Break, from, to, force)` + 자유함수 `composition_create_item<C: Composition + ?Sized>`.

시그니처 (asm 검증): `x0=this, x1=Break&, x2=from, x3=to, x4=force(bool), x8=sret`. normal path 는 from/to 를 Break struct (`br.from`/`br.to`) 에서 다시 읽음, force path 는 from/to 인자를 `create_line_item` 에 그대로 전달 (asm 가 x2/x3 미설정).

- **flag write (항상)**: `br.flags = (br.flags & ~3) | force | 2`.
- **force 경로**: `create_line_item(br, from, to)` → `span ∈ (0,1e8)` 면 `MonoGlyph{placement: PlaceNatural{direction, span}, child: line_item}` (vtable 0x781168 Placement glyph), 아니면 line_item 그대로 반환.
- **일반 경로**: `buf` = `Vec<Requisition>` `(to-from)+4` 개 `Requisition::INVALID` (0 이면 빈 Vec) → predecessor(`from>=1`, bt=Penalty)/main(`from..=to`, bt=Normal)/successor(`to<count-1`, bt=Hint) 각 `composition_compose_glyph` → `Some` 면 `g.request(&mut buf[written])`, `written++` → `buf.truncate(written)` → `out = Requisition::INVALID` → LR: span 무효 `Tile{0,INVALID,trim}.request_inner` / 유효 `out.x={span,0,0,0}`; TB: span 무효 `TileReverse{dir,INVALID,trim}.request_inner` / 유효 `out.y={span,0,0,1.0}` → `Align{LR?1:0}.request_inner` → `Glue::new(out)`.
- bounds check: `get_component(idx)` 가 `idx>=len` 시 panic ("GetAt") — raw `throw out_of_range` 등가.

### B-5e — `Composition::View` (`FUN_002ffe8c`, 468B) ✅ 완료

`composition.rs` 의 `Composition` trait default method `view(&mut self, from, to)`.
- raw asm: `/tmp/hft_scripts/view/view_asm.txt`. decompile: `decompiles_v2/Composition__View_002ffe8c.txt`.
- parent_glyph null → 즉시 return. rows 순회: `valid = flag & 1`, `in_view = end >= from && begin <= to`.
  - in-view + invalid → `create_item(&mut Break{begin,end,flag}, 0, 0, force=true)` → `parent.replace(idx*2, item)`.
  - out-of-view + valid → `create_item(..., force=false)` → `parent.replace(idx*2, item)`.
  - 그 외 → skip.
- 끝에 `flag_59 = false`. parent_glyph 는 take→put-back (한컴 refcount++/-- net 0).

### B-5i — `Composition::DoRepair` (`FUN_00301664`, 1584B) ✅ 완료

`composition.rs` 의 `Composition` trait default method
`do_repair(&mut self, param_1, param_2, param_3, param_4: &[i32], param_5)`.
raw asm + decompile: `/tmp/hft_scripts/dorepair/helpers.txt`.

- **시그니처** (asm 검증): `x0=this, w1=param_1(시작 line idx), w2=param_2(base char), w3=param_3, x4=param_4(breaks &Vec<int>), w5=param_5(count)`.
- **알고리즘**: `param_5` 회 loop. line index `lvar12`(=param_1 시작, 매 iter 끝 +1) 마다:
  1. temp RowSegment 구성: `{begin:-1, end:-1, ox:0, oy:0, flag:0}`. `lvar12 < row_count` 면 기존 row 의 origin 상속. `iVar6 = uVar13==0 ? 0 : param_4[uVar13-1]+1`. `temp.begin = iVar6 + param_2`, `temp.end = (param_2-1) + param_4[uVar13]`.
  2. **SKIP**: `lvar12 != row_count` 이고 기존 row 가 `flag bit1 set && begin==temp.begin && end==temp.end` → advance only.
  3. **DELETE loop**: `lvar12 < row_count-1 && rows[lvar12+1].end <= temp.end` 면 → `parent.remove((lvar12*2)|1)` + `parent.remove(lvar12*2)` + `rows.remove(lvar12)` 반복 (다음 row 의 end > temp.end 거나 끝나면 stop).
  4. **결정**: `row_count == lvar12` → INSERT. else `no_damage = uVar13 < param_5-1 && existing.end >= (param_2-1)+param_4[uVar13+1]` → INSERT. else (DAMAGE): `uVar13 == param_5-1` 이면 `existing.begin <= temp.end+1 ? REPLACE : INSERT`, 아니면 REPLACE.
  5. `create_item(&temp_break, param_2-1, param_3, force=flag_59)` → `temp.flag = br.flags` write-back.
  6. **REPLACE**: `parent.replace(lvar12*2, item)` + `parent.replace((lvar12*2)|1, get_separator(&br))` + `rows[lvar12] = temp`. **INSERT**: `parent.insert(lvar12*2, item)` + `parent.insert((lvar12*2)|1, sep)` + `rows.insert(lvar12, temp)` (= `FUN_00302004`).
- parent 는 `take()` → put-back (refcount++/-- net 0). parent None 이면 즉시 return.
- **`Hnc::Type::Flag` 확인**: libHncFoundation.dylib import. ctor=zero-init, dtor=no-op, trivially-copyable → `RowSegment.flag: u64` 정확.

### B-5j — `Composition::Repair` (`FUN_00300b14`, 1740B) ✅ 완료

`composition.rs` 의 `Composition` trait default method `repair(&mut self) -> bool`. raw decompile/asm: `/tmp/hft_scripts/repair/`.

- **알고리즘**: `had_damage = has_damage` (캡처). damage 없으면 즉시 `return true`. 있으면:
  `count`=item count, `row_count`=rows.len(), `seg_start = find_prev_forced_break(damage_begin, false)`.
  `start_row` = `seg_start < row.begin || seg_start <= row.end` 인 첫 row index (없으면 row_count).
  `if seg_start < count-1`: **main loop** `while i_var22 < count-1`:
  1. `damage_end <= seg_start` → break.
  2. `i_var1 = seg_start+1`, `i_var22 = i_var1`. **inner measure loop** (`i_var1 < count`): `uVar9 = i_var22-i_var1`; `vec_size <= uVar9` 면 `vec_size = find_next_forced_break(i_var22)-i_var1+1` 로 6 vector resize; `items[i_var22].request(&req)` (req init `{(-1e8,..),(-1e8,..),0}`); main axis (`LR?x:y`) `.natural != -1e8` 면 `widths/stretches/shrinks[uVar9]` 채움; `penalties[uVar9]=req.penalty`; penalty 가 -10000/-1000 (forced) 면 break, 아니면 `i_var22++`.
  3. `end_idx = min(i_var22, count-1)`. **height-fill** (`end_idx-seg_start>0`): `heights[k] = span - rows[start_row+k].ox - rows[start_row+k].oy`, 단 `k == max(start_row,row_count)-start_row` 면 `heights[k]=span` 후 break, `k == end_idx-seg_start` 면 break.
  4. compositor take → `compose_numbering`/`compose_bullet` (no-op) → `heights.resize((row_count-start_row)+1)` → `compose_break(widths,stretches,shrinks,penalties,heights,seg_start,end_idx)` → put back. `line_count = breaks.len()`.
  5. `do_repair(start_row, i_var1, end_idx, &breaks_i32, line_count)`.
  6. `start_row += line_count`; `row_count = rows.len()`; `seg_start = end_idx`.
  끝에 `has_damage = false`, `return !had_damage`.
- compositor `None` 이면 panic (raw 는 null deref crash — caller 계약 위반).

### Placement glyph (0x781168 = `MonoGlyph` struct) method body — B-Compositor/B-5k 에서 필요

- 현재 `MonoGlyph` 의 `request`/`allocate`/`draw` 등은 trait-default no-op (B-8 부터의 상태). B-5i 에는 불필요하지만 Repair/Compositor 가 호출.
- 전체 method asm dump: `/tmp/hft_scripts/item_class/placement_glyph_methods.txt` + `calc_placement.txt`.
  - `Request` (`FUN_00331214`) = `body.request(out)` + `strategy.request(scratch, out)`.
  - `Allocate` (`FUN_003312b4`) = `CalcPlacement` → `body.allocate(placed_alloc, ext)`.
  - `CalcPlacement` (`FUN_00331324`) = `body.request(&req)` → 1-elem `vec<Requisition>`/`vec<Allocation>` → `strategy.Allocate(avail, &reqs, &allocs)` → `out_alloc = allocs[0]`. PlaceNatural 의 strategy `Allocate` = no-op → PlaceNatural-strategy CalcPlacement 은 identity.
  - strategy 실제 인터페이스 (현 `Placement` trait 의 1-arg `request`/`allocate` 와 다름 — 재설계 필요): `Request(scratch, Requisition&)`, `Allocate(Allocation&, vector<Requisition>&, vector<Allocation>&)`. PlaceFix 는 16B `{direction, value}` (현 Rust `{width, height}` 는 오류).

### 완료됨 (참조용)

- B-5f-A: byte-eq 완료. B-5g: `impl Composition for LR/TBComposition` + create_line_item.
- B-5h: `TileReverse::Request_inner` + `Composition::CreateItem`. B-5e: `Composition::View`.
- B-5i: `Composition::DoRepair` + `FUN_00302004`(=Vec::insert) + `Hnc::Type::Flag` raw 확인.
- B-5j: `Composition::Repair` + `Compositor` trait 재설계 + `impl Compositor for ColCompositor`.
- 7번째 세션 부수: PlaceNatural {direction,span}, MonoGlyph child→Option, Break flags, Requisition::INVALID, Placement::as_any, CompositionState::flag_59.

## 8. Compositor 상태

- `compositor.rs`: `Compositor` trait (RE-검증 signature) + `impl Compositor for ColCompositor` 완료.
  - `ColCompositor` = 24B `{vptr, col_width:f32@0x08, line_count:u64@0x10}`. `ColCompositor::new(col_width, line_count)`.
  - `compose_numbering`/`compose_bullet` = trait default no-op (`return from`) — Simple/Col/Array raw 가 `ret`.
  - `compose_break` → `compose_break::compose_break` 위임. `compose_layout` → `compose_layout::compose_layout` 위임.
- **미완 (B-Compositor)**: `SimpleCompositor`/`ArrayCompositor`/`PptCompositor` 는 빈 struct shell — `impl Compositor` 없음. PptCompositor 의 ComposeNumbering(`0x306b40`)/ComposeBullet(`0x307468`) 은 실제 body (~1KB 각). Simple/Array 의 ComposeBreak/ComposeLayout 도 별도 raw.
- vtable RTTI: Simple `0x7804a0` / Array `0x7804e8` / Col `0x780530` / Ppt `0x780578`. obj vptr = vtable + 0x10. slot: +0x18 ComposeNumbering, +0x20 ComposeBullet, +0x28 ComposeBreak, +0x30 ComposeLayout. raw dump: `/tmp/hft_scripts/repair/compositor_vtables.txt`, `compose_nb_bodies.txt`, `compositor_ctors.txt`, `ctors2.txt`.

## 6. RE artifact 위치

### decompile (Ghidra C 출력)
- `kdsnr-hwp-toolkit/work/hft_re/layout_re/decompiles_v2/` — 모든 layout-관련 클래스 (CreateItem 포함)
- `kdsnr-hwp-toolkit/work/hft_re/layout_re/decomp_by_address/` — vtable slot 별 decompile

### raw asm dump
- `/tmp/hft_scripts/dorepair/` (8번째) — `helpers.txt` (DoRepair 1584B + FUN_00302004 840B), `foundation_flag_bodies.txt` (libHncFoundation `Hnc::Type::Flag` ctor/dtor/operator). Ghidra 스크립트: `DumpFoundationFlagAddr.py` (libHncFoundation 임포트 후 0xf3d0~0xf560 dump).
- `/tmp/hft_scripts/create_item/asm.txt` (7번째) — CreateItem 2408B + TileReverse::Request_inner 908B + CreateLineItem_base 8B
- `/tmp/hft_scripts/item_class/` (7번째) — `item_vtable.txt` (0x781168 19-slot), `methods.txt` (TileReverse aux + PlaceNatural + Item dtor), `placement_glyph_methods.txt` (Placement glyph 전체 method)
- `/tmp/hft_scripts/hbox_layer/` (6번째) — Tile/Align/Box/Superpose RTTI + asm
- `/tmp/hft_scripts/composition_asm/`, `line_item/`, `request_full_asm/`
- Ghidra dump 스크립트: `/tmp/hft_scripts/DumpItemClass.py`, `DumpPlacementGlyph.py`, `DumpCreateItem.py`

### vtable dumps
- `kdsnr-hwp-toolkit/work/hft_re/layout_re/data_dump/vtables.txt` — 전체 클래스 vtable RTTI
- `/tmp/hft_scripts/hbox_layer/vtables.txt` — Tile/Align/Box/Superpose (6번째 세션 NEW)

## 7. 한컴 codebase 특이 model

1. **Layout vs Glyph 별도 hierarchy** (6번째 세션 NEW 발견): Tile/Align/Superpose 는 `Hnc::Shape::Text::Layout` (5 vfuncs). Glyph 와 다른 base.
2. **HBox = Box(Glyph) + Superpose(Layout) + [Tile(direction), Align(opposite_direction)]**: 4-layer 구조.
3. **Tile direction vs Align direction INVERTED**: HBox 에서 Tile.dir=0 (H), Align.dir=1 (V).
4. **Tile trim algorithm**: penalty==1000 후행 + penalty>1 중간 제거 + penalty==1000 후행을 트림된 위치 뒤에 다시 추가. 이상한 알고리즘이지만 raw asm 검증됨.
5. **Box vtable 19 vfuncs**: GetCount/GetComponent/GetAllotment at +0x80/+0x88/+0x90, plus +0x98 = 19th (cache invalidation helper FUN_002e66fc, Box::Change 가 tail-call).
6. **3-deref vfunc dispatch**: `Holder<Glyph> → Glyph → vtable → vfunc`. Rust: `Option<Box<dyn Glyph>>` (refcount 무시).
7. **vfunc[3] (+0x18) = Request(Requisition&)**: writes self spec to out. For Box, returns cached_req (= Requisition cache).
8. **vfunc[4] (+0x20) = BoundsAccumulator (=`allocate_bounds` in Rust trait)**: bounds min/max merge.
9. **vfunc[5] (+0x28) = container Allocate**: iterates children, calls each child's allocate. Leaves don't have this (their +0x28 = Draw).

## 8. 정공법 정책 (사용자 명시)

- stub 처리는 사용자 신뢰 위반. "// TODO 후속 단계" 등 우회 패턴 절대 금지.
- raw decompile / raw asm / vtable dump 만 신뢰. "보통/아마/일반적으로" 같은 추측 표현 금지.
- 작업 진행 시 매 함수마다: (1) decompile/asm 출처 명시, (2) doc comment 에 raw 인용, (3) 알고리즘 1:1 mapping 설명, (4) test 추가, (5) cargo test green 확인.
- 컨텍스트 1M 활용 — 한 세션 큰 작업 진행 가능. 자주 묻거나 작게 자르지 말 것.
- **B-5f-A 옵션 A** (full byte-equivalent) — semantic-eq 우회 금지.

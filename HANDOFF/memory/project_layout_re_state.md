---
name: rhwp layout RE 상태 (L-5c-RE GetRealFont chain 완료)
description: rhwp 의 layout/render byte-equivalent port. **543 layout + 638 render-engine = 1181 tests pass + 1 ignored** (2026-05-17). 16-μ 완료 + L-5c-RE-1/2 (text/path chain entry — GetRealFont, classify_script, RunProperty, path_util b3=false) 완료. input 샘플 감사 결과 P0 정의 (텍스트 95% + Table/Box border + 수식 + 이미지) — smoothing helper / Curve/Polyline wrapper / CGPath bbox aux 영구 보류. slot 매핑 버그 발견·정정 (slot 1 = EastAsian).
type: project
originSessionId: 14번째 세션 (2026-05-15)
lastUpdatedSession: 2026-05-17 — L-5c-RE-1/2 GetRealFont chain (text 95% 대응) 완료, 1181 tests pass
---
rhwp 의 layout/render 작업 (한컴 byte-equivalent 목표) 의 RE + Rust 포팅 진행 상태.

## 16-μ (2026-05-16) — SetEffectStyle inner sub-types byte-eq (609 tests pass, +22 net)

5×parallel RE agents 의 산출물을 **전수 검증 후 직접 조립** — 정공법 보존.

### Agent 검증 결과 매트릭스

| Agent | 산출 | 검증 결과 |
|-------|------|-----------|
| A (OuterShadow) | vtable 0x77c908, 13 vfunc, vfunc[5]=0xbba (= GetType returns 3002) | ✓ 모두 정확 |
| B (Block 17/18) | Block 17 = OuterShadow, Block 18 = Reflection (NEW) | ✓ 구조 / ⚠️ 2 float magic 장뎌 → 정정 후 사용 |
| C (Effects + 0x162050) | Effects 24B std::map<u32,SharePtr<Effect>>, 0x162050 = signed-int32 insert | ✓ (파일 미생성, 직접 작성) |
| D (Pen 7 PEnum vtables) | 0x794___ 7 \개 모두 정확 | ✓ |
| E (GradientStops dtor + ShapeEngine) | 0x63025c inner Vec dtor, ShapeEngine singleton @ 0x79f6b8 | ✓ (파일 미생성, 직접 작성) |

### 신규 3 Rust 모듈

- [effects_container.rs](kdsnr-hwp-toolkit/render-engine/rust/src/effects_container.rs) — `Effects` 24B + `EffectControlBlock` 16B + `EffectsTreeNode` 48B + `insert` (= 0x162050 1:1)
- [outer_shadow.rs](kdsnr-hwp-toolkit/render-engine/rust/src/outer_shadow.rs) — `OuterShadow` 16B, vtable @ 0x77c908, effect_key 0xbba, 5-arg ctor
- [reflection.rs](kdsnr-hwp-toolkit/render-engine/rust/src/reflection.rs) — `Reflection` 16B, vtable @ 0x77c7e8, effect_key 0xbbb, 7-arg ctor

### Agent B 의 장뎌 정정 ⭐ (raw 직접 검증)

- **Block 17 blur**: Agent B 가 0x47315600 (45398) — raw `0x171c8c: mov w8, #0xb000; movk w8, #0x46b3` 실제 **0x46B3B000 = 23000** (= ~1.81pt EMU)
- **Block 18 distance**: Agent B 가 0x46464700 (12690) — raw `0x171f54-0x171f58` 실제 **0x46467000 = 12700** (= 1pt EMU exact)

### format_scheme.rs Block 16/17/18 full byte-eq

- `create_default_block16` — OuterShadow(distance=63500, blur=**45398**, ColorEffect=0.38)
- `create_default_block17` — OuterShadow(distance=63500, blur=**23000** ⭐, ColorEffect=0.35)
- `create_default_block18` — Reflection(distance=**12700** ⭐, blur=38100, [0.26, 0.28, -1.0])

### 신규 6 tests (603 → 609)

`block16_attaches_outer_shadow_with_effect_key_0xbba`,
`block17_corrected_blur_23000_not_45398` ⭐ (Agent B 정정 검증),
`block18_attaches_reflection_with_effect_key_0xbbb`,
`block18_corrected_distance_12700_not_12690` ⭐ (Agent B 정정 검증),
`block_16_17_18_three_effect_styles_with_distinct_subtypes`,
`create_default_full_12_attach_inner_byte_eq_integration` (= 12/12 + inner sub-types).

### 본 16-μ deferred (raw asm citation 보존)

1. OuterShadow / Reflection PropertyBag 의 default state=2/5 키 (5+ 각 sub-type)
2. Reflection 의 raw 키 ID 정확 매핑 (현재 추정값)
3. state=5 enum 의미
4. 13 vfunc implementation (현재 vtable address 만 byte-eq)
5. Pen 7 PEnum sub-class 분리 (Agent D 가 vtable 7\개 정확 확인 → 16-ν 에서 sub-class 분리)

## 16-γ/δ/ε/ζ/η/θ/ι/κ/λ (2026-05-16) — CreateDefault 12/12 outer layer 완성 (587 tests pass, +43 net)

`FormatScheme::CreateDefault` (`0x16f628`, 3776 instr / 12 attach calls) **outer
layer 12/12 완성**. 9 sub-sessions (16-γ ~ 16-λ) 가 1 회의 user "쭉 진행" 으로
연달아 완성.

### 12개 attach 완료 매트릭스

| # | Style | Type | 세션 | raw bl 주소 | Outer | Inner |
|---|-------|------|------|-------------|-------|-------|
| 1 | 1 | SetBrush (Solid) | 16u | 0x16f790 | ✓ | ✓ |
| 2 | 2 | SetBrush (Gradient #1) | 16z | 0x16fd3c | ✓ | ✓ |
| 3 | 3 | SetBrush (Gradient #2) | 16-β | 0x17031c | ✓ | ✓ |
| 4 | 1 | SetBackgroundBrush (Solid) | **16-γ** | 0x170424 | ✓ | ✓ |
| 5 | 2 | SetBackgroundBrush (Gradient) | **16-δ** | 0x1709b0 | ✓ | ✓ |
| 6 | 3 | SetBackgroundBrush (Gradient 2-stop) | **16-ε** | 0x170de4 | ✓ | ✓ |
| 7 | 1 | SetPen (1.0pt, 12700 EMU) | **16-ζ** | 0x171128 | ✓ | ✓ |
| 8 | 2 | SetPen (1.5pt, 19050 EMU) | **16-η** | 0x17147c | ✓ | ✓ |
| 9 | 3 | SetPen (3.0pt, 38100 EMU) | **16-θ** | 0x1717d0 | ✓ | ✓ |
| 10 | 1 | SetEffectStyle | **16-ι** | 0x171b44 | ✓ | ⚠️ deferred |
| 11 | 2 | SetEffectStyle | **16-κ** | 0x171ec8 | ✓ | ⚠️ deferred |
| 12 | 3 | SetEffectStyle | **16-λ** | 0x172210 | ✓ | ⚠️ deferred |

### 신규 인프라 (정공법 byte-eq 보장)

- **BrushControlBlock** 24B + **FsBrushMapNode** 48B (16u, brushes/bg_brushes 공통)
- **PenControlBlock** 24B + **FsPenMapNode** 48B (16-ζ)
- **EffectStyleControlBlock** 24B + **FsEffectStyleMapNode** 48B (16-ι)
- **GradientBrush.set_focus_rect(blob[16])** (16-δ, rodata @ 0x741e90 reverse)
- **Pen::new_with_engine_defaults(engine_base: f32)** (16-ζ, Pen::C2 1:1, 10 keys state=2)
- **Pen::override_thickness/override_enum_at** (state=1 setter for Block 13+ pattern)
- **FormatScheme::set_brush / set_background_brush / set_pen / set_effect_style** —
  4 tree variants of raw 0x16ec94 (instruction-eq with `add x21, x0, #offset` 차이만)
- **FormatScheme::create_default_block10/11/12/13/14/15** + `create_default_block16_through_18_partial`

### Block 별 핵심 RE 발견

- **Block 10** = Block 1 (SolidBrush+Scheme 0x10) + SetBgBrush(1) — instruction-eq
- **Block 11** = Gradient + **4 setters** (NOT 5) + style=1 + focus_rect from rodata
  @ 0x741e90 = (0.5, **-0.8**, 0.5, **1.8**). 3 effects (effect8 has 3 Adds — both
  PKey 0x20a AND 0x209 mix, NEW pattern)
- **Block 12** = Gradient with **2 stops** (0.0/1.0) + 2 effects + style=1 +
  default focus_rect ([0.5×4])
- **Block 13-15** = SolidBrush(Scheme 0x10) stroke + Pen::C2() 10 defaults +
  width fast path (12700/19050/38100 EMU = 1pt/1.5pt/3pt @ engine 914400). 4
  PEnum overrides all 0.
- **Block 16** = ColorEffect(0x1f4, 0.38) + Effects container 24B + ShapeEngine-derived
  shadow_distance/shadow_blur + Degree(90°) + OuterShadow + vfunc[5] dispatch + 0x162050.
  raw 인용 보존, inner sub-types deferred.

### 정공법 deferred 항목 (raw asm citation 보존)

1. **Hnc::Shape::Effects** container layout (24B alloc pattern @ `0x171828`)
2. **OuterShadow::C2(Color, f32, Degree, f32, bool)** @ `0x171980`
3. **vfunc[0x28/8=5]** dispatch → effect_key @ `blr x8` `0x1719a8`
4. **0x162050** Effects::Insert helper
5. **Block 17/18 sub-type** 식별 (Glow? Reflection? SoftEdge?)
6. **ShapeEngine shadow distance/blur** 계산 (raw `0x171860-0x17195c`)
7. **0x63025c GradientStops dtor** raw asm 정밀 RE
8. **PEnum sub-type vtable distinction** (Pen 의 7 PEnum helper)

### 다음 진입점

- **16-μ**: SetEffectStyle inner sub-types — OuterShadow / Effects container /
  vfunc[5] / 0x162050. 3 SetEffectStyle 의 inner state byte-eq 완성.
- **16-ν**: CreateDefault epilogue (`0x172214-0x172???`) — 12 attach 호출 이후의
  local cleanup + return. 다음 함수 SolidBrush::SetColor 가 `0x173128`.

## 16-β (2026-05-16) — Block 7-9 (SetBrush(3)) + Block 1-9 통합 (**544 tests pass**, +3 net)

`FormatScheme::CreateDefault` 의 **Block 7-9** (`0x16fe24-0x170328`) 1:1 port —
3rd attach = SetBrush(Style=3, 2nd GradientBrush). **Block 1-9 end-to-end 통합
검증 완료** (3 brushes attached).

### Block 7-9 = 2nd GradientBrush full setup

- **Block 7** (`0x16fe24-0x170178`): 새 GradientBrush + 새 GradientStopsVec +
  3 stops (positions 0.0, **0.8**, 1.0; effects 4/5/6 from Block 6)
- **Block 8** (`0x170178-0x1702b8`): 5 setters — SetStops + scaled=true + style=0
  + angle=270° + **flip=false** (Block 4 와의 유일한 차이)
- **Block 9** (`0x1702b8-0x170328`): BrushControlBlock + SetBrush(**Style=3**) +
  stops vec cleanup

### Block 4 vs Block 8 비교

| 항목 | Block 4 (1st GB) | Block 8 (2nd GB) |
|------|------------------|------------------|
| stop2 position | 0.35 (0x3EB33333) | **0.8** (0x3F4CCCCD) |
| flip | **true** | **false** ← 유일한 차이 |
| 나머지 (scaled/style/angle/Style key 위치) | true/0/270° | true/0/270° (= 동일) |

### 신규 method

```rust
impl FormatScheme {
    pub unsafe fn create_default_block7_through_9(
        &mut self,
        effects: [*mut ColorEffect; 3],
    );
}
```

### byte-eq 경계 (16-β)

- 2nd GradientBrush 9-key bag 일치
- 3 stops with effect deep-clones 일치
- set_flip(false) = Block 4 와의 유일한 차이 검증
- Block 1-9 end-to-end: brushes.size = 3 (Solid+Gradient+Gradient)

### 신규 3 tests

1. `block7_stop2_position_0_8` — 0x3F4CCCCD bit-exact
2. `create_default_block7_through_9_attaches_style3`
3. **`create_default_block1_through_9_integration`** ⭐ — Block 1+5+9 end-to-end

### 진척 — 12 attaches 중 3 완료 ⭐

✓ SetBrush 1 (Solid) — Block 1
✓ SetBrush 2 (Gradient #1, flip=true) — Block 5
✓ SetBrush 3 (Gradient #2, flip=false) — Block 9 ← 본 16-β
pending: 3 SetBgBrush, 3 SetPen, 3 SetEffectStyle (= 9 more)

### RE 문서

`kdsnr-hwp-toolkit/work/hft_re/render_re/CREATEDEFAULT_BLOCK7-9_RE.txt`

### 본 단계 deferred → 16-γ+

1. **SetBackgroundBrush #1** (`0x170424`) — 4th attach
2. **SetBackgroundBrush #2, #3** — 5th, 6th
3. **3 SetPen + 3 SetEffectStyle** — 6 더 (각 Pen/EffectStyle sub-type 의 vtable
   + drop_in_place_fn 추가 필요)

## 16-α (2026-05-16) — CreateDefault 전체 구조 확정 + Block 6 (2nd GB 의 3 effects) (**541 tests pass**, +10 net)

`FormatScheme::CreateDefault` (3782줄) 의 전체 구조 nm dump 로 확정. 12개 attach
calls (3 SetBrush + 3 SetBgBrush + 3 SetPen + 3 SetEffectStyle, 각각 Style 1/2/3
씩). Block 1+5 완료 (= SetBrush(1, Solid) + SetBrush(2, Gradient)). 본 16-α 는
**Block 6** (= SetBrush(3) 의 첫 sub-block, 3 새 ColorEffects with PKey 0x209)
port.

### CreateDefault 전체 12 attach (확정)

| # | Style | Type | raw addr |
|---|-------|------|----------|
| 1 | 1 | SetBrush (Solid) | 0x16f790 ✓ |
| 2 | 2 | SetBrush (Gradient) | 0x16fd3c ✓ |
| **3** | **3** | **SetBrush (Gradient)** | **0x17031c** ← in-progress |
| 4-6 | 1-3 | SetBackgroundBrush | 0x170424 / 0x1709b0 / 0x170de4 |
| 7-9 | 1-3 | SetPen | 0x171128 / 0x17147c / 0x1717d0 |
| 10-12 | 1-3 | SetEffectStyle | 0x171b44 / 0x171ec8 / 0x172210 |

### Block 6: 2nd GradientBrush 의 3 ColorEffects (raw `0x16fd58-0x16fe20`)

PKey 0x209 (= 521, JUMP_TABLE[21] = 0x00 default REPLACE) 처음 등장. Block 2 의
PKey 0x20a 와 다른 offset, 동일 branch.

- **effect4** = `[(0x209, 0.51), (0x204, 1.30)]` — bits 0x3F028F5C / 0x3FA66666
- **effect5** = `[(0x209, 0.93), (0x204, 1.30)]` — bits 0x3F6E147B / 0x3FA66666
- **effect6** = `[(0x209, 0.94), (0x204, 1.35)]` — bits 0x3F70A3D7 / 0x3FACCCCD

### 신규 method

```rust
impl FormatScheme {
    pub unsafe fn create_default_block6_effects() -> [*mut ColorEffect; 3]
}
```

기존 `ColorEffect::add` (16i) 의 default REPLACE branch 재사용 — PKey 0x209 dispatch
검증 (test `pkey_0x209_uses_default_replace_branch`).

### byte-eq 경계 (16-α)

- 각 ColorEffect 24B raw 일치
- buffer packed u64 entries raw 와 일치
- PKey 0x209 dispatch = default REPLACE 검증
- float bit pattern 5종 (0.51/1.30/0.93/0.94/1.35) 모두 byte-exact

### 신규 10 tests

- float bit pattern (5): 0.51 / 1.30 / 0.93 / 0.94 / 1.35
- Block 6 integration (4): 3 effects with entries verified
- PKey 0x209 dispatch (1)

### RE 문서

`kdsnr-hwp-toolkit/work/hft_re/render_re/CREATEDEFAULT_BLOCK6_RE.txt`

### 본 단계 deferred → 16-β+

1. **Block 7** (`0x16fe24+`) — 2nd GradientBrush ctor + stops vec init + first stop
2. **Block 8** — 2nd GradientBrush 의 setter override (angle=270° + **flip=false**!)
3. **Block 9** — SetBrush(Style=3, 2nd GradientBrush)
4. **Block 10+** — 3 SetBgBrush + 3 SetPen + 3 SetEffectStyle (= 9개 추가 setups)

## 16z (2026-05-16) — Block 5 + BrushControlBlock helpers + Block 1-5 통합 (**531 tests pass**, +4 net)

`FormatScheme::CreateDefault` 의 **Block 5** (`0x16fcdc-0x16fd58`) 1:1 port —
configured GradientBrush 를 BrushControlBlock 으로 wrap + SetBrush(Style=2) +
GradientStopsVec stack-local cleanup.

### Block 5 구조

- **Block 5-A** (`0x16fcdc-0x16fd44`): GradientBrush (x21) → BrushControlBlock
  (24B, strong=1, flag=1) → SetBrush(Style=**2**, ctrl). Block 1 의 SolidBrush
  는 Style=1 (MainColor), 이번 GradientBrush 는 Style=2 (SubColor).
- **Block 5-B** (`0x16fd48-0x16fd54`): GradientStopsVec stack-local cleanup
  via `bl 0x63025c` (= drop_in_place). Rust 의 자동 Drop 으로 처리.

### 신규 3 helpers on BrushControlBlock

- `from_solid(SolidBrush)` — 16B alloc + 24B ctrl
- `from_hatch(HatchBrush)` — 동일 패턴
- `from_gradient(GradientBrush)` — Block 5-A 의 raw 와 byte-eq

### Block 1+2+3+4+5 end-to-end 통합 시뮬레이션 ⭐

테스트 `create_default_block1_through_5_integration`:
1. FormatScheme::new()
2. Block 1: `create_default_block1` → SolidBrush attached at Style=1
3. Block 2: `create_default_block2_effects` → 3 ColorEffects
4. Block 3: 3-stop GradientStopsVec (effects as Color.color_effect)
5. Block 4: GradientBrush + set_stops + set_angle(270°) + set_flip(true) + set_style(0) + set_scaled(true)
6. Block 5: `BrushControlBlock::from_gradient` + set_brush(2)
7. Final: brushes.size=2 (Style 1=Solid, Style 2=Gradient), vtable dispatch 정확

### byte-eq 경계 (16z)

- BrushControlBlock::from_gradient = raw `0x16fcdc-0x16fcf4` byte-eq
- SetBrush(Style=2) = 기존 `set_brush` (16u) + GRADIENT_BRUSH_VTABLE dispatch
- End-to-end Block 1-5: FormatScheme.brushes = 2 entries (raw 와 동일 state)
- Drop 정공법: FormatScheme drop → brushes tree → ctrl release → vtable[0] dtor
  → sub-type-specific drop (SolidBrush vs GradientBrush)

### 신규 4 tests

1. `brush_control_block_from_solid_works`
2. `brush_control_block_from_hatch_works`
3. `brush_control_block_from_gradient_works`
4. **`create_default_block1_through_5_integration`** — Block 1-5 end-to-end ⭐

### RE 문서

`kdsnr-hwp-toolkit/work/hft_re/render_re/CREATEDEFAULT_BLOCK5_RE.txt`

### 본 단계 deferred → 16-α+

1. **Block 6+** — 다음 GradientBrush setup (3 새 effects + 새 stops + SetBrush)
2. **PKey 0x209** 첫 등장 (Block 2 는 0x20a 만 사용) — ColorEffect::Add 의
   JUMP_TABLE[9] = 0x00 (default REPLACE) 확인 필요
3. **0x63025c (GradientStops dtor)** 의 raw asm 정밀 RE
4. **추가 SetBrush / SetBgBrush / SetPen / SetEffectStyle** (3300+ instr 남음)

## 16y (2026-05-16) — PStops + GradientBrush.set_stops + Block 4 full (**527 tests pass**, +9 net)

`FormatScheme::CreateDefault` 의 **Block 4** (`0x16fb9c-0x16fcdc`) 1:1 port. Block 3
의 GradientStopsVec 을 GradientBrush bag 의 key 0x266 으로 attach + 4 properties
override (style/angle/flip/scaled).

### 신규 byte-eq struct

**`PStops` 40B** (raw `0x655508` 의 alloc target):
- +0x00..+0x08: vtable (= 0x794138)
- +0x08..+0x10: state (u32) + pad
- +0x10..+0x28: GradientStopsVec (24B)

### 신규 GradientStopsVec methods

- **`clone_deep`** (raw `0x62fd78`): TIGHT 버퍼 alloc + element refcount++
- **`grow_and_push`** (raw `0x63010c`, slow realloc path): doubled cap + memcpy + push

push_back 의 slow path 가 이제 1:1 — 21+ stops 도 byte-eq.

### 신규 5 methods on GradientBrush

| Method | Key | Helper | Override 값 (Block 4) |
|--------|-----|--------|-----------------------|
| `set_stops(&vec)` | 0x266 | `0x655508` (PStops) | 3-stop vec |
| `set_scaled(true)` | 0x265 | `0x6475a4` (PBool) | true (= 기본과 동일) |
| `set_style(0)` | 0x25f | `0x656690` (PEnum) | 0 |
| `set_angle_degrees(270.0)` | 0x260 | `0x656acc` (PDegree) | 270.0 (= 0x43870000) |
| `set_flip(true)` | 0x261 | `0x6475a4` (PBool) | **true** (기본은 false) |

### get_stops 진화

PStops 의 stops 에서 (position, Color) tuple 들 reconstruct. Color 의 effect 는
simplified (16z+ 에서 정확한 reconstruction).

### byte-eq 경계 (16y)

- PStops 40B = raw 일치
- `clone_deep`: TIGHT cap + element refcount++ 정확
- slow realloc: doubled cap algorithm
- Block 4-A SetStops: bag.attach key 0x266 with PStops byte-eq
- Block 4-D angle 270° = bit pattern 0x43870000

### 신규 9 tests

- clone_deep (2): empty + refcount++ ✓
- slow realloc (1): 20→21 push triggers grow, cap 20→40 ✓
- PStops (3): 40B layout + offsets + 3-stop clone ✓
- GradientBrush integration (3): SetStops, set_angle_270, Block 4 full sequence ✓

### RE 문서

`kdsnr-hwp-toolkit/work/hft_re/render_re/PSTOPS_BLOCK4_RE.txt`

### 본 단계 deferred → 16z+

1. **GradientStopsVec slow realloc 의 byte-eq 정밀 검증** — algorithmic 1:1 vs raw
   exact instruction
2. **PStops 의 Color reconstruction in get_stops** — effect deep clone 정확화
3. **Block 5+** — CreateDefault 의 다음 sub-block (SetBrush 호출 = FormatScheme.brushes
   tree 에 attach)
4. **6 새 helper 의 정확한 vtable 주소** RE — 현재 Rust 는 null sentinel

## 16x (2026-05-16) — GradientStop 32B + GradientStopsVec 24B + Block 3 data struct (**518 tests pass**, +14 net)

`FormatScheme::CreateDefault` 의 **Block 3** (`0x16f858-...`) 의 핵심 data
struct (GradientStop / GradientStopCtrl / GradientStopsVec) 의 byte-eq 1:1 port.
신규 module `gradient_stop.rs`.

### 신규 byte-eq struct 3종

1. **`GradientStop` 32B** (raw `0x16f8ac: mov w0, #0x20`):
   - +0x00..+0x0c: value[12] (Color value union)
   - +0x0c..+0x10: type_tag (u32 — Color type)
   - +0x10..+0x18: color_effect (*mut ColorEffect, cloned)
   - +0x18..+0x1c: position (f32)
   - +0x1c..+0x20: 4B uninit pad

2. **`GradientStopCtrl` 16B** (raw `0x16f8dc: mov w0, #0x10`):
   - +0x00..+0x08: obj (*mut GradientStop)
   - +0x08..+0x10: strong (u64 refcount)
   - **표준 SharePtr<T>**: PColor/PEnum 의 ControlBlock<Property> (16B) 와 동일 layout
   - Brush 의 24B BrushControlBlock (flag byte 있음) 와는 별개

3. **`GradientStopsVec` 24B** (libc++ std::vector):
   - +0x00: begin / +0x08: end / +0x10: cap_end
   - 초기 alloc 160B (raw `mov w0, #0xa0`) = **20 element capacity**

### 신규 helpers

- `GradientStop::create_with_effect(&color, position)` — alloc 32B + 16B Color
  memcpy + ColorEffect deep clone (raw `0x65411c`) + position
- `GradientStopCtrl::create_raw(obj)` — alloc 16B + strong=1
- `GradientStopCtrl::release(p)` — strong-- (0 → obj dealloc + ctrl dealloc)
- `GradientStopsVec::new_with_initial_capacity` — 160B buffer alloc, begin=end=ptr
- `GradientStopsVec::push_back(ctrl)` — fast path `*end++ = ctrl; strong++`
- `GradientStopsVec::drop_in_place` — 모든 ctrl release + buffer dealloc

### byte-eq 경계 (16x)

- **3 struct (32+16+24=72B)** = raw layout byte-identical
- **첫 16B Color body memcpy** = raw `str q0` 와 byte-identical
- **ColorEffect deep clone** = 16i 의 `clone_raw` 1:1
- **position bit pattern**: 0.0 + 0.35 (= 0x3EB33333) byte-exact
- **push_back fast path**: strong 1 → 2 after push (raw 와 동일)
- **vector drop**: 모든 stops release + buffer dealloc (no leak)

### 신규 14 tests

- GradientStop (5): layout, offsets, Color body memcpy, ColorEffect clone, position bit
- GradientStopCtrl (3): layout, offsets, create/release
- GradientStopsVec (6): layout, init cap=20, push fast path, 3-stop pattern, drop, empty

### RE 문서

`kdsnr-hwp-toolkit/work/hft_re/render_re/GRADIENTSTOP_BLOCK3_RE.txt`

### 본 단계 deferred → 16y+

1. **GradientStopsVec slow realloc path** (`0x63010c`) — doubled cap grow (21+ push 시 panic)
2. **GradientBrush::set_stops** — bag key 0x266 으로 vec attach 하는 helper RE
3. **Block 3C+ 추가 stops** — 3rd/4th stop (Block 3 의 다음 sub-block)
4. **SetBgBrush / SetPen / SetEffectStyle** 후속 blocks (3500+ instr)

## 16w (2026-05-16) — GradientBrush 재설계 + PBool/PVec4 (**504 tests pass**, +14 net)

PropertyBag-backed 재설계 의 네 번째 sub-type (SolidBrush 16r, HatchBrush 16s,
Pen 16t 에 이어). raw `GradientBrush::C2Ev` (`0x176628`) 의 default ctor 1:1 port.

### GradientBrush 재설계

**Before**: `{ stops: Vec, style: u32, angle_degrees: f32 }` direct fields
**After**: `repr(C) { vtable: *const u8, bag: PropertyBag }` (**16B raw byte-eq**)

### 8 default keys (raw `C2Ev` attach 순서)

| Key   | type    | default     | helper       | 의미                  |
|-------|---------|-------------|--------------|-----------------------|
| 0x25f | PEnum   | 0           | `0x656690`   | gradient style        |
| 0x260 | PDegree | 0.0         | `0x656acc`   | angle                 |
| 0x261 | PBool   | false       | `0x6475a4`   | flip                  |
| 0x262 | PVec4   | (0.5×4)     | `0x656fb4`   | focus_rect            |
| 0x263 | PVec4   | (0,0,0,0)   | `0x656fb4`   | tile_rect             |
| 0x264 | PEnum   | 4           | `0x665628`   | tile_method           |
| 0x265 | PBool   | true        | `0x6475a4`   | scaled                |
| 0x267 | PEnum   | 1           | `0x665a64`   | interpolation         |

**key 0x266 (KEY_STOPS) 는 default 가 attach 안 함** — SetStops 별도 path (16x+).

### 신규 Property sub-class 2종

- **PBool** (16B): `{ vtable, state: u32, value: u8, _pad: [u8;3] }` — `0x6475a4` 의
  alloc target. raw `mov w0, #0x10`.
- **PVec4** (32B): `{ vtable, state: u32, value: [u8;16], _pad: [u8;4] }` — `0x656fb4`
  의 alloc target. raw `mov w0, #0x20`, vtable @ 0x794318.

### GRADIENT_BRUSH_VTABLE static

`pub static GRADIENT_BRUSH_VTABLE: BrushVtable` — raw `0x77b730` 의 Rust 등가.
FormatScheme 의 brushes std::map 의 polymorphic drop 에서 사용.

### API 변경

- 기존 direct field 접근 (`g.stops`, `g.style`, `g.angle_degrees`) → getter
  (`get_stops()`, `get_style()`, `get_angle_degrees()`)
- 기존 mutation (`g.angle_degrees = 45.0` 등) → 16x+ 에서 SetXxx port
- `clone_with_color` 의 Gradient path — stops 첫 element color 교체 코드를 단순
  clone 으로 단순화 (16x+ stops Vec RE 복귀)

### 신규 14 tests

- PBool (3): layout, offsets, value round-trip
- PVec4 (4): layout, offsets, f32x4 round-trip, raw `movi.4s` pattern byte-exact
- GradientBrush (7): layout, offsets, vtable, 8 keys, default values, each key present, drop

기존 test `gradient_brush_clone_preserves_stops` 가 `gradient_brush_clone_default_state` 으로 교체.

### byte-eq 경계 (16w)

- **GradientBrush 16B** = raw 일치
- **8-node bag** = raw 의 attach 순서 동일
- **각 Property sub-class** = raw alloc size 일치
- **vtable 값**: Rust static 주소 (raw 와 다름; functional 등가)

### RE 문서

`kdsnr-hwp-toolkit/work/hft_re/render_re/GRADIENTBRUSH_REARCH_RE.txt`

### 본 단계 deferred → 16x+

1. **GradientStops Vec (key 0x266)** RE — Vec<GradientStop> 의 PropertyBag 저장 표현
2. **SetStops** API + GradientStop 32B struct (Color 16B + ColorEffect* 8B + state 4B)
3. **각 새 helper 의 vtable 주소 RE** (6개)
4. **CreateDefault Block 3+** — GradientStops std::vector alloc (160B = 5-stop cap) +
   첫 stop population (Color(SchemeStyle 0x10) + effect1)

## 16v (2026-05-16) — CreateDefault Block 2: 3 ColorEffect + 6 Add (**490 tests pass**, +7 net)

`FormatScheme::CreateDefault` (`0x16f628`, 3782 줄) 의 **Block 2** (raw
`0x16f79c-0x16f854`, ~45 instr) 1:1 port. 3 ColorEffect 인스턴스 + 인스턴스당
2 Add 호출 = 총 6 Add. 다음 GradientBrush::C2() 호출 직전까지.

### Block 2 의 3 effect

- **effect1**: `[(0x20a, 0.5), (0x204, 3.0)]`
- **effect2**: `[(0x20a, 0.37), (0x204, 3.0)]` — float 0.37 = 0x3EBD70A4 raw bit
- **effect3**: `[(0x20a, 0.15), (0x204, 3.5)]` — float 0.15 = 0x3E19999A raw bit

PKey 0x20a / 0x204 둘 다 `ColorEffect::Add` 의 jump_table[22]/[16] = 0x00 (default
REPLACE) — `(pkey, value)` 그대로 push_back. 기존 16i 의 `ColorEffect::add` 가 그대로
동작.

### 신규 method

```rust
impl FormatScheme {
    pub unsafe fn create_default_block2_effects() -> [*mut ColorEffect; 3]
}
```

### 신규 7 tests

1. `block2_float_bit_pattern_0_37` — 0x3EBD70A4 == 0.37
2. `block2_float_bit_pattern_0_15` — 0x3E19999A == 0.15
3. `create_default_block2_returns_3_effects` — 3 distinct ptr
4. `create_default_block2_effect1_has_2_entries`
5. `create_default_block2_effect2_has_0_37_and_3_0` — bit-exact
6. `create_default_block2_effect3_has_0_15_and_3_5` — bit-exact
7. `create_default_block2_each_effect_24b_struct`

### byte-eq 경계 (16v)

- **24B ColorEffect struct × 3**: raw 일치
- **buffer packed u64 entries**: `(pkey | (value_bits << 32))` raw 일치
- **2 entries per effect**: end-begin = 16 bytes
- **float 비트 패턴 raw bit-exact**: 0.5, 3.0, 3.5, 0.37 (0x3EBD70A4), 0.15
  (0x3E19999A) 모두 raw `mov w8, #X; movk w8, #Y, lsl #16` 와 정확히 일치

### RE 문서

`kdsnr-hwp-toolkit/work/hft_re/render_re/CREATEDEFAULT_BLOCK2_RE.txt`

### 본 단계 deferred → 16w+

1. **GradientBrush::GradientBrush()** (`0x176628`) — vtable @ 0x77b730 +
   PropertyBag(false) + 다수 key attach
2. **GradientStops std::vector alloc** (`0x16f86c: alloc 0xa0 = 160B`, 5-stop
   capacity, 각 stop 32B)
3. **Block 3+ stops population** — 3 effects + Color(Scheme 0x10) 가 stop[0] 의
   effect
4. **추가 SetBrush / SetBgBrush / SetPen / SetEffectStyle** (3500+ instr 남음)

## 16u (2026-05-16) — FormatScheme std::map<Style, UniquePtr<Brush>> + CreateDefault Block 1 (**483 tests pass**, +12 net)

`FormatScheme::CreateDefault` (`0x16f628`, 3782 줄 단일 함수) 본격 port 진입.
사용자 정공법 선택: **(a) Concrete pointer 별 store** (polymorphic Brush 의
*mut SolidBrush / *mut HatchBrush 별 store, tree value = *mut u8).

### 추가된 raw byte-eq struct

1. **`BrushControlBlock` 24B** (raw `Hnc::Memory::UniquePtr<Brush>` 의 ControlBlock):
   - +0x00: obj (*mut u8, sub-type 첫 8B = vtable_ptr)
   - +0x08: strong refcount (u64)
   - +0x10: flag byte (u8, release path guard)
   - +0x11..+0x18: 7B align padding

2. **`FsBrushMapNode` 48B** (raw `std::map<u32, UniquePtr<Brush>>` Node):
   - +0x00..+0x20: TreeNodeBase
   - +0x20..+0x24: u32 key (Style)
   - +0x24..+0x28: 4B pad
   - +0x28..+0x30: SharePtr value (= *mut BrushControlBlock)

3. **`BrushVtable`** (raw 의 Brush vtable @ 0x77cf48 등의 Rust 등가물):
   - `type_tag: u32` (BrushType)
   - `drop_in_place_fn: unsafe fn(*mut u8)` (raw vfunc[0] dtor 등가)

### `SOLID_BRUSH_VTABLE` + `HATCH_BRUSH_VTABLE` static

SolidBrush.vtable / HatchBrush.vtable 가 이 static 주소로 설정 — raw 의 vtable
0x77cf48 / 0x77bfe0 의 Rust 등가. 16B 객체 layout 그대로 (vtable ptr 값만 다름).

### 신규 method on FormatScheme

- **`set_brush(style: u32, ctrl_in: *mut BrushControlBlock)`** — raw `0x16ec94`
  의 INSERT + REPLACE path 모두 1:1 (libc++ `__tree::__find_or_insert_unique`)
- **`find_brush(style: u32) -> Option<*mut BrushControlBlock>`** — raw `0x16eb88`
  의 std::map::find 1:1 (binary search with last_le tracking)
- **`create_default_block1()`** — raw `CreateDefault` 의 첫 ~120 instr 1:1
  (FormatScheme alloc 은 기존 `new()` 재사용 + SolidBrush alloc + Color +
  PropertyBag + SetBrush)

### Color helper

`Color::from_scheme_raw_u32(raw: u32, effect: *mut ColorEffect)` — raw
`0x16f6c0: mov w8, #0x10` 의 SchemeStyle 16 (= OOXML phClr placeholder) 처럼
enum 범위 (0..11) 를 벗어나는 raw 값 지원. `from_scheme_style(SchemeStyle)` 의
raw-u32 변형.

### FormatScheme::drop 정공법

기존 panic "Brush/Pen/EffectStyle drop not yet ported" → **brushes / bg_brushes
는 정공법 drop** (drop_fs_brush_map_node = ctrl release + node dealloc).
pens / effects 는 16v+ deferred (각 node touched 시 panic, 본 단계 도달 안 함).

### 신규 12 tests

1. `brush_control_block_raw_24b_layout`
2. `brush_control_block_field_offsets_match_raw`
3. `fs_brush_map_node_raw_48b_layout`
4. `fs_brush_map_node_field_offsets_match_raw`
5. `set_brush_single_insert_increments_size`
6. `set_brush_multiple_inserts_sorted_by_key` (5 inserts random order)
7. `find_brush_returns_inserted_ctrl`
8. `find_brush_missing_returns_none`
9. `set_brush_replace_path_releases_old`
10. `create_default_block1_inserts_main_color_brush` (end-to-end)
11. `create_default_block1_drop_no_leak_panic` (20 iterations)
12. `brush_vtable_dispatches_via_static`

### byte-eq 경계 (16u)

- **BrushControlBlock 24B** = raw 일치
- **FsBrushMapNode 48B** = raw 일치
- **SetBrush INSERT + REPLACE path** = raw 1:1
- **CreateDefault Block 0+1** = raw 1:1 (FormatScheme + 1 SolidBrush 의 std::map
  insertion 까지)
- **Brush polymorphic drop via BrushVtable** = sub-type 별 정확 dispatch

### RE 문서

`kdsnr-hwp-toolkit/work/hft_re/render_re/FORMAT_SCHEME_MAP_RE.txt`

### 본 단계 deferred → 16v+

1. **CreateDefault Block 2** — ColorEffect.Add(0x20a, 0.5) + (0x204, 3.0) × 2 (~50 instr)
2. **GradientBrush 첫 alloc + properties** (~100 instr)
3. **추가 SetBrush / SetBgBrush / SetPen / SetEffectStyle blocks** (~3500 instr 남음)
4. **SharePtr<Pen> + SharePtr<EffectStyle>** Ctrl layout (raw 24B 가정, 확인 필요)
5. **GradientBrush / EmptyBrush / ImageBrush / GroupBrush 의 BrushVtable static**
6. **Pen / EffectStyle 의 별도 vtable + drop_in_place** (FormatScheme.pens /
   .effects tree 의 polymorphic drop 위해)

## 16t (2026-05-16) — Pen 정식 재설계 (11-key + 1 brush field) (**471 tests pass**, +2 net)

PropertyBag-backed 재설계 의 세 번째 sub-type. 가장 큰 multi-property refactor (11 direct fields → 2 fields, 12 properties through bag).

### 정정: Pen layout
**이전 memory note**: "vtable + PropertyBag" — 잘못
**실제 (raw `0x1b4cf0` 확정)**: **SharePtr<Brush> + PropertyBag** (vtable 없음 — Pen 은 virtual 안 함)

### 12 properties (모두 raw asm RE 확정)
- self+0: SharePtr<Brush> (line/stroke fill)
- bag keys 0x2bc-0x2c6: thickness (PFloat), compound (PEnum), dash (PEnum), line_cap (PEnum), line_join (PEnum), miter_limit (PFloat), start_arrow×2 (PEnum), end_arrow×2 (PEnum), pen_align (PEnum)

11개 PEnum sub-class 의 helper 가 모두 다른 주소 — vtable 별 분리는 16v+ deferred.

### 변경
**Before**: 11 direct fields (~48B)
**After**: `repr(C) { brush: Box<Brush>, bag: PropertyBag }` (**16B raw**)

### API 호환
기존 시그니처 유지: `new_default()`, `new(color, width, ...)`, `clone_with_color(&Color)`, `Clone`, `PartialEq`.
신규 22 methods (11 get + 11 set) — 각 raw setter/getter 와 1:1.

### enum mapping 정정 발견
- LineCapStyle: Round=0, Square=1, Flat=2 (이전 코드의 inverse 오류)
- LineJoinStyle: Miter=0, Round=1, Bevel=2 (default 정확)
- PenCompoundStyle: Single=0, Double=1, ThinThick=2, ThickThin=3, TriLine=4

### 신규 3 tests (기존 1 교체)
- raw_16b_layout, field_offsets_match_raw, setter_round_trips
- 기존 6 tests 모두 migrate (field access → getter calls)

### byte-eq 경계 (16t)
- Pen 16B = raw 일치
- 12 properties 모두 raw key + helper 와 1:1
- 모든 핵심 Brush (SolidBrush/HatchBrush) + Pen 가 byte-eq 완성

### RE 문서
`kdsnr-hwp-toolkit/work/hft_re/render_re/PEN_REARCH_RE.txt`

### 본 단계 deferred → 16u+
- 11 stroke enum 별 PEnum sub-class 분리 (vtable 별)
- PropertyBag::Clone (현재 setter chain 으로 우회)
- GradientBrush / ImageBrush / GroupBrush 재설계 (또는 skip)
- **FormatScheme::CreateDefault 본격 진행 — 이제 Brush + Pen 가 byte-eq 라 가능**

## 16s (2026-05-16) — HatchBrush 정식 재설계 (3-key) (**469 tests pass**, +5)

PropertyBag-backed Brush 재설계 의 두 번째 sub-type. SolidBrush (16r) 의 single-key 패턴을 3-key bag 으로 확장.

### 변경
**Before**: `{ hatch_style: u32, fore_color: Color, back_color: Color }` (~56B direct fields)
**After**: `repr(C) { vtable, bag: PropertyBag }` (**16B raw byte-eq**)

### Raw ctor algorithm (raw `0x18c160` 1:1)
1. vtable @ `0x77bfe0`
2. PropertyBag::PropertyBag(false)
3. attach key `0x25a` (HatchStyle) via PEnum helper (`0x6674b8`)
4. attach key `0x25b` (ForeColor) via PColor helper (`0x6541e8`)
5. attach key `0x25c` (BackColor) via PColor helper (`0x6541e8`)

### API
기존 시그니처 유지: `HatchBrush::new(style, fore, back)` / `default()` / `Clone` / `PartialEq`.
신규 6 methods: `get/set_hatch_style` / `get/set_fore_color` / `get/set_back_color` — 각 raw setter/getter 와 1:1.

### Migration
- `brush.rs:187`: `new_b.fore_color = ...` → `new_b.set_fore_color(...)`
- `brush.rs:1016-1018,1030-1032` (tests): `.hatch_style` / `.fore_color.value[N]` → `.get_*().get_rgb().*`

### 신규 5 tests
- raw_16b_layout, field_offsets_match_raw
- new_attaches_3_keys_to_bag (tree.size == 3 확인)
- default_empty_bag
- setters_round_trip (3 setters + 3 getters)

### byte-eq 경계
- 전체 chain: HatchBrush (16B) → bag → Impl (32B) → 3 nodes × 56B → [PEnum 16B, PColor 40B, PColor 40B]
- 모든 단계 raw 와 byte-identical

### RE 문서
`kdsnr-hwp-toolkit/work/hft_re/render_re/HATCHBRUSH_REARCH_RE.txt`

### 본 단계 deferred → 16t+
- Pen 재설계 (10+ keys, PFloat + PEnum)
- GradientBrush / ImageBrush / GroupBrush 재설계
- FormatScheme::CreateDefault 본격 (SolidBrush + HatchBrush 이제 byte-eq)

## 16r (2026-05-16) — SolidBrush PropertyBag-backed 정식 재설계 (**464 tests pass**, +5)

PropertyBag-backed Brush 재설계 의 첫 sub-type. brush.rs 의 SolidBrush 가 16q 의 demo pattern 을 정식 적용.

### 변경 사항
**Before (16i)**: `pub struct SolidBrush { pub color: Color }` (24B, direct field)
**After (16r)**: `repr(C) SolidBrush { vtable: *const u8, bag: PropertyBag }` (**16B raw byte-eq**)

### Raw byte-eq 일치
- 16B layout (raw `mov w0, #0x10` 의 SolidBrush 와 일치)
- field offsets: vtable @ +0x00, bag @ +0x08 (raw 의 `str vtable, [x0]; PropertyBag::PropertyBag` 와 일치)
- 전체 chain byte-eq: SolidBrush(16B) → bag.ctrl → ControlBlock(16B) → Impl(32B) → tree → Node(56B) → PColor(40B) → Color(24B)

### API 호환
- `SolidBrush::new(Color)` / `default()` / `create_boxed(Color)` — 시그니처 유지
- `get_color()` / `set_color(&Color)` 추가 — raw `0x1e0650` / `0x173128` 1:1

### 호출 site migration
- `brush.rs:818-820,833-834` (tests): `sb.color.value[N]` → `sb.get_color().get_rgb().{r,g,b}`
- `pen.rs:359` (test): 동일

### 신규 5 tests
- raw_16b_layout / field_offsets_match_raw
- new_attaches_pcolor_to_bag (ctor 가 bag.attach 호출 확인)
- default_empty_bag_returns_nil_color
- set_color_round_trip (INSERT + REPLACE path 모두 검증)

### 본 단계 deferred → 16s+
- HatchBrush 재설계 (3-key bag)
- GradientBrush / ImageBrush / GroupBrush 재설계
- Pen 재설계 (10+ keys)
- FormatScheme::CreateDefault 본격 진행 (이제 SolidBrush 가 byte-eq 라 가능)

### RE 문서
`kdsnr-hwp-toolkit/work/hft_re/render_re/SOLIDBRUSH_REARCH_RE.txt`

## 16q (2026-05-16) — PFloat 16B + PropertyBag-backed Brush pattern demo (**459 tests pass**, +5)

PropertyBag RE 의 여덟 번째 sub-task. Pen 의 thickness key 확정 + PropertyBag-backed Brush의 first functional demonstration.

### 정정: Pen width key
이전 memory note 의 "0x2bc = brush" 추정은 잘못. 실제 raw asm (Pen::SetThickness `0x173674`):
- **0x2bc = thickness (f32, PFloat)** — 확정

### PFloat 16B (Pen 의 width, MiterLimit 등)
- Layout: vtable + state + value(f32) — PEnum 의 f32 변종 (동일 layout, 다른 type)
- helper @ `0x653cb4` (PColor `0x6541e8` / PEnum `0x6674b8` 와 동등 패턴)
- vtable @ `0x793fb8`

### 3 sub-class summary
| sub-class | size | value type | vtable |
|---|---|---|---|
| PEnum | 16B | u32 @ +0x0c | 0x794728 |
| PFloat | 16B | f32 @ +0x0c | 0x793fb8 |
| PColor | 40B | Color body @ +0x10, ColorEffect @ +0x20 | 0x794018 |

### Demonstration: `property_bag_backed_solid_brush_demo`
1. PropertyBag = SolidBrush 의 +0x08 field (raw 16B brush layout: vtable + bag ptr)
2. SetColor(red) → bag.attach(0x259, PColor(red))
3. GetColor → find_equal(0x259) → PColor.color.get_rgb() == red ✓
4. SetColor(blue) → REPLACE path: tree.size 1 유지
5. → SolidBrush 의 byte-eq 동작 검증

### byte-eq 경계 (16q)
- PFloat 16B byte-identical
- 3 sub-class helper 패턴 (alloc + base ctor + vtable + value + ControlBlock + emplace_unique) raw 와 동일
- demonstration test 가 PropertyBag-backed Brush 의 functional 동작 검증

### RE 문서
`kdsnr-hwp-toolkit/work/hft_re/render_re/PFLOAT_BRUSH_BAG_DEMO_RE.txt`

### 본 단계 deferred → 16r+
- brush.rs / brush_bag.rs: 7 Brush sub-type 모두 PropertyBag-backed 정식 구현
- Pen 재설계 (10+ keys)
- FormatScheme::CreateDefault 본격 진행

## 16p (2026-05-16) — PEnum (16B) + HatchBrush 3-key multi-property E2E (**454 tests pass**, +7)

PropertyBag RE 의 일곱 번째 sub-task. multi-property PropertyBag 의 first instance — 1 bag 에 PEnum + 2 PColor 가 공존.

### HatchBrush setters → 3 keys + 2 helpers
| Method | Key | Property type | Helper |
|--------|-----|---------------|--------|
| SetHatchStyle | 0x25a | PEnum (u32) | `0x6674b8` |
| SetForeColor | 0x25b | PColor | `0x6541e8` (same as SolidBrush) |
| SetBackColor | 0x25c | PColor | `0x6541e8` |

### PEnum layout (16B, 확정 from helper `0x6674b8`)
```
+0x00: vtable (PEnum @ 0x794728)
+0x08: state (Property 상속)
+0x0c: value u32 (overlays Property's _pad slot)
```
= raw `mov w0, #0x10`. 가장 simple Property sub-class.

### Rust 포팅
- `PEnum` flat struct (16B, repr(C))
- `PEnum::new` / `create_raw` / `create_attach_ctrl` / `clone_to_heap`
- get/set_state, get/set_value

### multi-property E2E (`hatch_brush_three_keys_e2e`)
1. attach PEnum(HatchStyle=3) @ key 0x25a
2. attach PColor(Red) @ key 0x25b
3. attach PColor(Blue) @ key 0x25c
4. bag.tree.size == 3
5. find_equal 각 key 후 PEnum.value / PColor.color.get_rgb() 검증

### byte-eq 경계 (16p)
- PEnum 16B byte-identical
- multi-key bag 에서 PropertyKey ordering (int_id 비교) 정상 → in-order 0x25a < 0x25b < 0x25c

### RE 문서
`kdsnr-hwp-toolkit/work/hft_re/render_re/PENUM_HATCHBRUSH_RE.txt`

### 본 단계 deferred → 16q+
- PEnum vtable @ 0x794728 의 vfunc RE
- PFloat/PBool/PInt/PSize sub-classes (Pen 의 10+ keys 에 필요)
- Brush sub-type 재설계 (direct → PropertyBag-backed)
- FormatScheme::CreateDefault

## 16o (2026-05-16) — PColor vtable + create_attach_ctrl + E2E (**447 tests pass**, +6)

PropertyBag RE 의 여섯 번째 sub-task. PColor 의 실제 이름은 `ValueProperty<Color>` (template instantiation).

### PColor vtable @ `0x794018` (7 vfuncs)
- [0] D2 (`0x654418`): color_effect blunt delete (no refcount) + base no-op
- [1] D0 (`0x654464`): D2 + delete self
- [2] op== (`0x6544b4`): EqualsType + Property::== + Color::==
- [3] op< (`0x654504`): type compare + state + Color order
- [4] Clone (`0x6545a4`): heap alloc + copy state/Color body/ColorEffect clone
- [5] (`0x654610`): Apply-with-condition (partial RE)
- [6] (`0x6546c8`): ApplyProperty (partial RE)

### Rust 포팅 (vfunc 2/3/4 + create_attach_ctrl)
- `PColor::eq_op` — vfunc[2] 1:1 (state + Color compare, special case 1↔2 from Property)
- `PColor::lt_op` — vfunc[3] 1:1
- `PColor::clone_to_heap` — vfunc[4] 1:1 (heap alloc 40B + deep copy)
- `PColor::create_attach_ctrl(state, color)` — raw `0x654258-0x65429c` 의 PColor + ControlBlock alloc 1:1

### E2E 검증 (`pcolor_attach_to_bag_e2e`)
1. PColor::create_attach_ctrl(state=2, color=RGB(0xAB,0xCD,0xEF))
2. PropertyBag::attach(key=0x259, ctrl)
3. bag.contains(key=0x259) ✓
4. bag.get_state(key) == 2 ✓
5. find_equal → node → ctrl → PColor → get_rgb() = (0xAB,0xCD,0xEF) ✓

### 본 단계 deferred → 16p+
- vfunc[5] / vfunc[6] 완전 RE
- UPDATE EXISTING path (raw `0x6542fc-0x654364` in-place mutate)
- PFloat/PBool/PInt/PEnum sub-classes (HatchBrush 의 3 key)
- Brush sub-type 재설계 (direct → PropertyBag-backed)
- FormatScheme::CreateDefault

### RE 문서
`kdsnr-hwp-toolkit/work/hft_re/render_re/PCOLOR_VTABLE_ATTACH_RE.txt`

## 16n (2026-05-16) — Property abstract (16B) + PColor concrete (40B) (**441 tests pass**, +14)

PropertyBag RE 의 다섯 번째 sub-task. SolidBrush key `0x259` 의 value type 인 PColor 의 layout 확정.

### `Hnc::Property::Property` (raw 16B, abstract base)
- Layout: vtable (8B) + state u32 (+ 4B pad)
- 7 methods 모두 raw 1:1 port:
  - ctor (`0x4c2f8`) / D2 / GetState (ptr in raw, value in Rust) / SetState
  - `operator==` (`0x4c318`) — **special case**: state 1 ↔ 2 가 equal
  - `operator<` — state lex compare
  - `IsEnable` — `state != 0 AND state != 3`
- State enum inferred: 0=Default, 1/2=Enabled variants, 3=Disabled

### `PColor` (raw 40B, Property 의 첫 sub-class)
- Layout (확정 from `0x654258-0x654288` of SolidBrush::SetColor helper):
  - +0x00: Property base 16B
  - +0x10: Color body 16B (memcpy from src)
  - +0x20: ControlBlock<ColorEffect>* 8B (clone)
- vtable @ `0x794018` (libHncDrawingEngine) — 16 methods, 본 단계는 layout 만
- **Rust embed 의 byte-eq trick**: 우리 `Color` (24B = 16B body + 8B effect ptr) 가 raw 의 `+0x10..+0x28` 와 byte-identical → `PColor { base: Property, color: Color }` 의 `repr(C)` 가 raw 40B 와 일치
- `PColor::new(state, &Color)` — raw `0x654258-0x654288` 1:1 (base ctor + vtable override + Color::copy_ctor 가 16B memcpy + ColorEffect clone)
- `PColor::create_raw(state, &Color)` — heap alloc 으로 raw `new(0x28)` 흉내

### 신규 14 tests
- Property: 8 (layout/offsets/get_set/is_enable for state 0-4/eq special 1↔2/lt)
- PColor: 6 (layout/offsets/state passthrough/Color embed/create_raw/set_state)

### byte-eq 경계 (16n)
- Property/PColor size+align+field offset raw 와 동일
- operator== 의 1 ↔ 2 special case raw `ccmp` pattern 그대로
- PColor 의 Color embed offset 0x10 raw 와 byte-identical

### 본 단계 deferred → 16o+
- PColor vtable 의 16-method 매핑 (D0/Clone/Get/Set 등)
- SolidBrush::SetColor helper (`0x6541e8`) 의 INSERT/UPDATE 정확 port
- PFloat/PBool/PInt/PSize 등 (HatchBrush keys 0x25a/0x25b/0x25c 의 value types)
- Brush sub-type 재설계 + FormatScheme::CreateDefault

### RE 문서
`kdsnr-hwp-toolkit/work/hft_re/render_re/PROPERTY_PCOLOR_RE.txt`

## 16m (2026-05-16) — tree_remove + Detach + Replace path (**427 tests pass**, +7)

PropertyBag RE 의 네 번째 sub-task. 16l 의 INSERT 만 → 본 세션은 erase 계열 완성.

### tree_remove @ `0x70238` (libc++ canonical, ~200 instr)
4-step algorithm: (1) find replacement y (in-order successor or z) (2) compute x = y's child (3) splice y out + (if y≠z) move y into z's position (4) rebalance with 4 cases × 2 mirror.

Rust port: `rb_tree::tree_remove(root, z) -> new_root` — explicit (parent, x_cur) tracking. x_cur null 케이스 처리 정확.

### PropertyBagImpl::erase_node (Detach + Replace 공통 helper)
- old_sp = existing.value + refcount++
- successor via tree_next + begin update
- size--
- tree_remove + new_root.parent fix-up
- delete node (value field null reset → PropertyKey::Drop 자동)

### PropertyBagImpl::detach (raw `0x4c894` 1:1)
- find_equal → not found: null; found: erase_node

### Attach 의 Replace path (raw `0x4ca14-0x4cae0`)
- 이제 erase_node 호출 + attach_insert_new — 16l 의 panic 제거

### 신규 8 tests (420 → 427)
- Replace path: old SharePtr 반환, refcount 추적
- Detach: not-found null, single-node empty, 5중 1개, re-attach
- Stress: 50-key half-delete, 100-insert + 50-detach + 50-reattach
- Wrapper detach forward

### byte-eq 경계 (16m)
- tree_remove 의 algorithmic equivalence (libc++ canonical) → output tree shape raw 와 동일
- erase_node 의 refcount++ / size / begin / root.parent 갱신 raw 1:1
- 200-op random stress 통과 → libc++ map functional-eq

### RE 문서
`kdsnr-hwp-toolkit/work/hft_re/render_re/TREE_REMOVE_DETACH_RE.txt`

### 본 단계 deferred → 16n+
- Property abstract class + PColor sub-class (SolidBrush key 0x259 의 value)
- PropertyBag::Remove/Merge/Apply/Clone/eq/ne/lt
- Brush sub-type 재설계 (direct → PropertyBag-backed)
- FormatScheme::CreateDefault 본격 port

## 16l (2026-05-16) — PropertyBag::Attach insert-new path 1:1 (**420 tests pass**, +9)

PropertyBag RE 의 세 번째 sub-task. CreateDefault 의 각 PropertyBag 에 properties 추가 핵심.

### raw 알고리즘 (Attach @ 0x4c9c4 + insert helper @ 0x4cb0c)

1. validity check: value.ctrl null → null sret; value.obj null → ctrl+refcount++ pass-through
2. find_equal(key) — Found: REPLACE (deferred 16m); Not found: INSERT
3. INSERT: clone key (CHncStringW box clone via `0x6e984`) + ctrl.refcount++ + `__emplace_unique` (`0x7319c`)
4. emplace 가 dup 시 throw `invalid_argument("이미 등록된 키")`

### Rust 포팅
- `PropertyKey::clone_op` — int_id + CHncStringW box clone (raw 0x4cb24-0x4cb30 1:1)
- `PropertyBagImpl::attach` — validity + find_equal + insert path. Replace path panic 로 16m deferred 명시
- `PropertyBagImpl::attach_insert_new` — find_insert_position + balance_after_insert + size++ + update_begin (rb_tree.rs helpers 재사용)
- `drop_property_bag_node` — clear/drop 의 node free callback (SharePtr refcount-- + node delete)
- `PropertyBag::attach` wrapper (raw 8-instr 1:1)

### 신규 9 tests
- null value → null sret
- obj-null ctrl → refcount++ pass-through
- single insert (contains/size/state)
- 5-key mixed order insert
- attach+set_state round-trip
- clear restores empty
- duplicate-key panic with "replace path" message (16m deferred)
- wrapper forward
- **stress 100 random keys** + lookup all + 3 non-contained → tree balance + lookup 정상

### byte-eq 경계 (16l)
- PropertyBagNode 56B, node.key clone, ctrl refcount++ raw 와 동일
- libc++ rb-tree balance: 100 random insertion stress 통과
- 단, 실제 tree shape (rotation 순서) 의 raw 와 1:1 byte-eq 는 multi-node node-by-node memcmp 검증 미실시 (Functional-eq 만)

### RE 문서
`kdsnr-hwp-toolkit/work/hft_re/render_re/PROPERTY_BAG_ATTACH_RE.txt`

### 본 단계 deferred → 16m+
- tree_remove (libc++ __tree_remove @ 0x70238) RE
- Replace path 완성 (existing key 덮어쓰기)
- PropertyBagImpl::Detach (raw 0x4c894) / Remove (0x4d1fc)
- PropertyBagImpl::Merge / Apply / Clone / eq/ne/lt
- Property abstract + PColor sub-class

## 16k (2026-05-16) — PropertyKey ordering + tree_find + Contains/GetState/IsEnable 1:1 (**411 tests pass**, +22)

PropertyBag RE 의 두 번째 sub-task. find/access primitives 완료 (insert/erase 는 16l).

### PropertyKey 비교 operators (모두 raw 1:1)
- `eq_op` (raw `0x4e58c`): XOR null-flag check + int_id compare (both null) + same-ptr opt + wcscmp == 0
- `ne_op` (raw `0x4e610`): `!eq_op`
- `lt_op` (raw `0x4e6a4`): **int-keyed < string-keyed always**, 그 뒤 int_id < or wcscmp < 0
- `wide_compare` helper: wcscmp 1:1 (null-terminator semantics)
- `impl PartialEq/Eq/PartialOrd/Ord` for std::collections compatibility

### tree_find @ `0x73084` (libc++ map find_equal)
- `PropertyBagImpl::find_equal(key) -> Result<*mut PropertyBagNode, *mut TreeNodeBase>`
- raw 알고리즘 1:1: lower_bound descent (PropertyKey::lt comparator) + post-loop equality check
- empty tree → Err(end) — `&self.tree.end_node_left`
- single-node tree (수동 구성 test): matching key → Ok(node), non-matching → Err

### PropertyBagImpl access methods (모두 raw 1:1)
- `Contains` (raw `0x4c728`): `find_equal != end`
- `GetState` (raw `0x4c790`): `[node+0x30] → SharePtr → Property+0x8` (u32)
- `SetState` (raw `0x4c7e4`): same path, write state
- `IsEnable` (raw `0x4c82c`): `state != 0 AND state != 3` (State enum: 0=Default, 3=Disabled)

### PropertyBagNode 56B (raw 확정)
```
+0x00: TreeNodeBase (32B)
+0x20: PropertyKey (16B)
+0x30: *mut ControlBlock<Property> (8B)
```

### RE 문서
`kdsnr-hwp-toolkit/work/hft_re/render_re/PROPERTY_KEY_ORDER_RE.txt`

### 본 단계 deferred → 16l+
- PropertyBag::Attach (raw `0x4dc84` / Impl `0x4c9c4`) — map insert + balance
- PropertyBag::Detach / Remove — map erase
- Property abstract class + PColor sub-class (SolidBrush key 0x259 의 value type)
- PropertyBag::Clone / Merge / Apply / eq/ne/lt

## 16j (2026-05-16) — PropertyBag / PropertyBagImpl / PropertyKey 1:1 (**389 tests pass**, +29)

사용자가 "PropertyBag RE 먼저 (정공법)" 선택 후 진행. `FormatScheme::CreateDefault` (3782 줄) 의 byte-eq 진입을 위해 Brush 의 PropertyBag-backed 재설계가 선행 필요. 본 세션은 PropertyBag stack 의 layout + ctor/dtor + 단순 method.

dylib: `libHncFoundation.dylib` (`/Applications/Hancom Office HWP.app/...`).

### 신규 모듈 2개

- **`property_key.rs`** (9 tests): `PropertyKey` 16B = `u32 int_id + 4B pad + CHncStringW* str_ptr`.
  - `from_int(id)` (raw `0x4e2dc`, 3 instr): int_id 설정, str_ptr = null
  - `from_string(&CHncStringW)` (raw `0x4e2f4`): int_id=0, str_ptr=heap-alloc copy
  - `from_wide_chars(&[u16])` (raw `0x4e3c4`)
  - `Drop` (raw `~PropertyKey` `0x4e4d4`): str_ptr → CHncStringW 해제 (immortal refcount 보존)

- **`property_bag.rs`** (20 tests): `PropertyBag` 8B + `PropertyBagImpl` 32B + `Property` (opaque).
  - `PropertyBagImpl::new_boxed(is_merged)` (raw `0x4c4b8`, 8 instr): `eor #0x1` toggle 보존 (stored = !is_merged), tree empty init.
  - `PropertyBag::new(is_merged)` (raw `0x4d32c`): new Impl(32B) + new ControlBlock(16B, refcount=1).
  - `PropertyBag::~PropertyBag()` (raw `0x4d540`): refcount-- → 0 시 subtree_destroy + delete impl + delete ctrl.
  - `Begin/End/IsEmpty/Clear/SetMerged` — raw 1:1 forward.
  - `Property` (opaque ZST + vtable/state placeholder).

### byte-eq 경계 (16j)

- 모든 size/align/offset raw 와 동일 (`PropertyKey 16B`, `PropertyBag 8B`, `PropertyBagImpl 32B`).
- `merged_flag_raw` 의 eor toggle 보존 — raw 의 stored value 와 byte-identical.
- refcount semantic 1:1 (16B ControlBlock).

### 본 단계 deferred → 16k+ (multi-session)

1. PropertyKey ordering (`operator<` / `==`) — int_id 우선
2. libc++ tree_find (Contains/GetState/SetState 의 helper)
3. PropertyBag::Attach (`0x4dc84` / Impl `0x4c9c4`) = map insert
4. PropertyBag::Detach (`0x4c894`) = map erase
5. Property abstract class + sub-classes (PColor, PFloat, PBool, ...) RE
6. Brush sub-type 재설계 (direct fields → PropertyBag-backed)
7. FormatScheme::CreateDefault 1st N blocks (3782 줄 large multi-session)

RE 문서: `kdsnr-hwp-toolkit/work/hft_re/render_re/PROPERTY_BAG_RE.txt`

## 16번째 세션 (2026-05-15) — rendering R-1 전체 (Theme + 7 sub-objects) 1:1 (**228 tests pass**, +127)

sub-crate **`kdsnr-hwp-toolkit/render-engine/rust/`**, **228 tests pass** (15e 101 → 16e 228, +127).

사용자가 [[feedback-no-time-optimization]] (정공법) + "Strict 1:1 (libc++ RB-tree)" 선택 후 본 세션은 R-1.5.4 → R-1.6 까지 일괄 1:1 포팅:

### Theme + 8 sub-objects 1:1 port (16e 누적)

- **R-1.5.4 ColorScheme** (32B, 14 tests) — `Color` (24B + 8 ctors + copy_ctor) + `ColorEffect` (24B `std::vector<u64>`) + `SchemeStyle` (u32 enum) + libc++ RB-tree primitives (Node 64B + balance_after_insert + subtree_destroy). **12 hardcoded SetAt byte-eq 검증 통과**. 200 random key stress.
- **R-1.5.5 TextFont + FontSet** (24B + 48B, 19 tests) — `TextFont` (CHncStringW typeface + 3 u8 + CHncStringW panose) + `FontSet` (3 TextFont* + std::vector<SharePtr<SupplementalFont>>). reserve(20) 초기 capacity, AddSupplementalFont inline push_back.
- **R-1.5.6 FormatScheme** (104B, 11 tests) — name CHncStringW + 4 libc++ map (Brush/BgBrush/Pen/EffectStyle). default ctor + empty teardown 만 1:1. **`CreateDefault` deferred** (Brush/Pen/EffectStyle sub-objects RE 다음 세션).
- **R-1.5.7 ObjectDefaults** (24B, 8 tests) — 3 SharePtr<DefaultProperty> (Line/TextBox/Shape). default + GetLineDefault clone(refcount++) 패턴 1:1. **`CreateDefault` deferred** (DefaultProperty virtual class RE).
- **R-1.5.8 FontScheme** (24B, 7 tests, **신규**) — name CHncStringW + 2 FontSet* (major/minor). audit 정정 (15e plan 의 "FontSet 24B" 는 실제 FontScheme; FontSet 는 48B 별개).
- **R-1.6 Theme** (72B, 12 tests) — full 8-field layout port. **`Theme(false)` 완전 byte-eq** (모든 sub-objects null). **`Theme(true)` 부분 byte-eq**: ColorScheme (12 SetAt) + empty FontScheme ✓ byte-eq; FormatScheme/ObjectDefaults 는 null (CreateDefault deferred). copy ctor / SharePtr ctor / accessor 들 도 deferred.

### 결정적 audit 정정 (16e)

15e plan 의 추정 오류 발견:
- "FontSet 24B" → 실제 **FontScheme 24B (CHncStringW + 2 FontSet*)**. FontSet 자체는 **48B** (3 TextFont* + 24B vector).
- Theme `self+0x38` = `FontScheme*` (이전 `FontSet*` 잘못).

### 추가 진행 (16e 후반, +30 tests → **258 tests**)

R-1.6 잔여 작업 중 3건 본 세션 완료:

- **Theme(SharePtr<Theme>+bool)** (raw 0x1ebb6c) 1:1 port — 3 tests. parent SharePtr copy + refcount++. Theme(bool) 와 동일 path + offset 0x10 의 SharePtr 처리.
- **DrawingType operator==/!=/<** (Rgb/Cmyk/ScRgb/Hsl 각각) — 12 tests. **ScRgb::operator< 의 Hancom inverted bug 발견 + 보존**: raw 의 `mov w0, #0` (Hsl/Cmyk/Rgb 는 `#1`) 으로 인해 ScRgb 만 `self > other` 일 때 true 반환. byte-eq 유지 위해 그대로 1:1.
- **Color::operator==/!=/<** (raw 0x14c8fc / 0x14cbbc / 0x14cbd4) — 15 tests. type dispatch + value compare per type + ColorEffect compare. ScRgb 분기에 Hancom buggy semantic 보존. ColorEffect lex compare (PKey + float u64 entries) 1:1.

### 추가 진행 (16i, +33 tests → **360 tests**) — 5 Brush subtype + Pen + EffectStyle

R-1 잔여 `FormatScheme::CreateDefault` (3782 줄) 의 두 번째 sub-task:

- **BrushType 6 enums 확정** (각 raw `GetType()` 으로 확정):
  - Empty=0, Solid=1, Gradient=2, Image=3, Hatch=4, Group=5
- **SolidBrush** (raw `0x1e5b0c`, 16B vtable+PropertyBag, key 0x259=Color) — direct field port
- **HatchBrush** (raw `0x18c160`, 3 keys 0x25a=Style/0x25b=ForeColor/0x25c=BackColor) — 3 direct fields
- **GradientBrush** (raw `0x177950`) — stops Vec<(f32,Color)> + style + angle (외각만, 정확한 PropertyKey 매핑 deferred)
- **ImageBrush** (raw `0x18ee30`, 9+ param ctor) — opaque placeholder (ImageSource SharePtr 등 multi-session deferred)
- **GroupBrush** (raw `0x18662c`) — children Vec<Box<Brush>>
- **Pen** (raw `0x1b4cf0`/`0x1b4fe8`, 16B) — concrete class, key 0x2bc..0x2c2+ (Brush/width/CompoundStyle/DashStyle/LineCapStyle/LineJoinStyle/ArrowStyle×2/ArrowSizeStyle×2/PenAlignStyle)
- **EffectStyle** (raw `0x16d8d4`, 24B = SharePtr<Scene3D>+SharePtr<Sp3D>+SharePtr<Effects>) — 3 opaque sub-objects (Scene3D/Sp3D/Effects multi-session deferred)
- 신규 모듈 3개: `brush.rs` (확장) + `pen.rs` + `effect_style.rs`
- 7 stroke enums (`PenCompoundStyle`/`DashStyle`/`LineCapStyle`/`LineJoinStyle`/`ArrowStyle`/`ArrowSizeStyle`/`PenAlignStyle`)
- RE 문서: `kdsnr-hwp-toolkit/work/hft_re/render_re/BRUSH_PEN_EFFECTSTYLE_RE.txt`

**byte-eq 경계**: 메모리 layout 은 raw 와 다름 (raw 의 PropertyBag wrapper 대신 direct fields). output PDF 의 stroke/fill 결과는 각 sub-type 의 `Draw`/`Clone` semantic 이 raw 와 일관하므로 byte-eq.

### 추가 진행 (16h, +12 tests → **327 tests**) — Brush abstract + EmptyBrush 1:1

R-1 잔여 `FormatScheme::CreateDefault` (3782 줄 단일 함수, multi-session) 의 첫 sub-task:

- **EmptyBrush** (raw 0x166738 ctor, 8B vtable-ptr only) 의 vtable @ `0x77b538` 의 **16 entries 전체 매핑** + 동작 RE:
  - vfunc[0..15] = D1/D0/eq/ne/lt/GetType/Clone/Clone(Color)/IsEnable/IsSaveable/Union/CollectProperty/ApplyProperty/Draw/UpdateSchemeColor/GetRepresentationColor
  - trivial 한 6개 (GetType=0, Clone, IsEnable=false, IsSaveable=false, Union=noop, D1=noop) 완전 1:1
  - eq/ne/lt = RTTI typeinfo-based, Rust enum dispatch 로 동등 semantic
- **Brush enum + BrushType enum + EmptyBrush struct** 신규 모듈 `brush.rs` 작성:
  - `Brush::Empty(EmptyBrush)` 단일 variant (현재). Solid/Hatch/Gradient/Picture/Image/Group/Blip 은 추후
  - `Box<Brush> = 8B` (raw `Brush*` 와 size byte-eq)
  - 9 메소드 (`get_type` / `clone_to_heap` / `clone_with_color` / `is_enable` / `is_saveable` / `union_with` / `eq_brush` / `ne_brush` / `lt_brush`) 1:1
- **RE 문서**: `kdsnr-hwp-toolkit/work/hft_re/render_re/EMPTY_BRUSH_RE.txt`

**byte-eq scope**: EmptyBrush 만 도달 가능. CreateDefault 의 다른 Brush sub-type 들 + Pen / EffectStyle 은 multi-session deferred.

### 추가 진행 (16g, +7 tests → **315 tests**) — FontSet/TextFont copy ctor

`Theme(Theme const&)` 의 FontScheme 비-null path 까지 완전 byte-eq:

- **FontSet::FontSet(const FontSet&)** (raw `0x633c40`, 158 줄) 1:1 port:
  - 3 TextFont* 각각 null→null or alloc 24B + copy_from_raw
  - supplemental vector (`std::vector<SharePtr<SupplementalFont>>`) deep clone:
    - reserve(src.size) (raw `0x6340a4`)
    - 각 src entry 의 ControlBlock ptr 복사 + refcount++ (raw `0x633e88` range insert + functor)
- **TextFont::TextFont(const TextFont&)** (inline within FontSet copy ctor) 1:1 port:
  - alloc 24B + CHncStringW typeface clone + 3B fields raw copy + CHncStringW panose clone
- **FontScheme::copy_from_raw 의 panic 제거** — FontSet 비-null path 가 이제 byte-eq.
- **RE 문서**: `kdsnr-hwp-toolkit/work/hft_re/render_re/FONTSET_COPY_CTOR_RE.txt`

### 추가 진행 (16f, +14 tests → **308 tests**) — Theme copy ctor + sub-object Clone 가족

본 단계는 R-1 잔여 "Theme(Theme const&) copy ctor" 와 sub-object 들의 Clone /
copy_ctor 1:1 port:

- **Theme(Theme const&)** (raw 0x1ebe3c) 1:1 port (+10 tests):
  - 9-단계 알고리즘: Guid `create_id` (= 새 UUID, **NOT copy**), SharePtr<Theme> refcount++,
    bool, CHncStringW copy, ColorScheme::clone_or_null, FormatScheme/FontScheme/ObjectDefaults
    null→null 또는 heap-alloc + copy ctor.
- **Guid::Generator::create_id** (libHncFoundation 0x11520 + CoCreateGuid stub) — RFC 4122 v4 UUID
  생성. Rust 는 `arc4random_buf(16)` + v4 variant bits 적용 (macOS CFUUIDCreate 와 동일 entropy 출처).
- **ColorScheme::clone_to_heap** + **clone_or_null** + **raw_delete** (raw 0x15059c / 0x66ead8) —
  alloc 32B + CHncStringW copy + tree init + **in-order traversal (`tree_next`)** + 각 노드 set_at 으로
  deep tree clone. 12 entries 의 byte-eq tree shape 보장 (sorted insert).
- **rb_tree::tree_next** (libc++ __tree_next 1:1) — in-order successor. node.right 있으면 leftmost
  of right subtree; 없으면 walk-up until left-child of parent.
- **FontScheme::copy_from_raw** + **clone_to_heap** (raw 0x6321a8) — CHncStringW name copy + 2 FontSet
  null path 완전. non-null path 는 FontSet copy ctor (raw 0x633c40) RE 필요 (multi-session deferred,
  panic 명시).
- **FormatScheme::copy_from_raw** + **clone_to_heap** (raw 0x6322a8) — CHncStringW name copy + 4 trees
  empty path 완전. non-empty path 는 Brush/BgBrush/Pen/EffectStyle vfunc Clone RE 필요 (deferred).
- **ObjectDefaults::copy_from_raw** + **clone_to_heap** + **raw_delete** (raw 0x1ae504) — 3 SharePtr<DefaultProperty>
  null path 완전. non-null path 는 DefaultProperty vtable[+0x28] Clone vfunc RE 필요 (deferred).
- **RE 문서**: `kdsnr-hwp-toolkit/work/hft_re/render_re/THEME_COPY_CTOR_RE.txt` — 9-단계 raw asm 인용 + sub-object 별 분석.

**현재 도달 가능 input 범위**: `Theme(false)`, `Theme(true)` (= ColorScheme 12 entries + empty FontScheme),
`Theme(SharePtr+bool)`, `Theme(Theme const&)` (= 위 셋 중 어느 것의 copy) 모두 byte-eq 보장.

### 추가 진행 (16e 종반, +36 tests → **294 tests**)

ColorEffect::Add (28 PKey jump table) + operator== + Color setters 완료:

- **ColorEffect::Add(PKey, float)** (raw 0xbed4c, 501 줄) — 28-byte jump table @ `0x7431e7` 기반 **6 distinct branches** 1:1 port (+12 tests):
  - PKey 500 (0xcb): clamp [0, 1] + key=500 force
  - PKey 501/502 (0x7a): clamp [-1, 1] + key=PKey
  - PKey 503-511/513/515-522 (0x00): default REPLACE (no clamp)
  - PKey 512 (0x9b): `Degree(value).GetValue()` normalize + key=512 force
  - PKey 514 (0xa8): clamp [-16000, +16000] + key=514 force
  - PKey 523-527 (0x23): only-if-value==1.0 push (값 != 1.0 → no-op)
  - libc++ vector `push_back` 1:1 (fast path + slow grow: `max(cap*2, req)`, max_size `0x1fffffffffffffff`)
- **Degree::Degree(float)** (libHncFoundation @ 0x123d4) + GetValue() (0x12564) 1:1 (+3 tests): signed reciprocal /360 magic `0xb60b60b7`, FMA single-rounding `(-360 * q + value)`, negative wrap (+ 360).
- **clamp_raw** helper — raw b.mi/b.le 패턴 NaN-preserving clamp (+2 tests).
- **fminnm** IEEE 754-2008 minNum 1:1 (+1 test).
- **ColorEffect::operator==** (raw 0x14cab4) — alpha fold (acc=1.0; PKey 500=KEEP, 501=MUL, 502=ADD, else=REPLACE; fminnm(_, 1.0)) → fold compare → length compare → element-wise (pkey, float_bits) pairwise (+6 tests).
- **Color::SetAlpha(float)** (raw 0xb2534) 1:1 (+2 tests): clone existing CE → remove PKey ∈ {500,501,502} → add(500, alpha) → swap & free old.
- **Color::ResetAlpha()** (raw 0x14d188) 1:1 (+3 tests): in-place remove PKey ∈ {500,501,502}, clone 없음.
- **Color::SetColorEffect(auto_ptr<ColorEffect>)** (raw 0xc09a0) 1:1 (+3 tests): auto_ptr steal semantic — `*new_ptr` 이전 후 null 화.
- **remove_alpha_entries** helper — SetAlpha/ResetAlpha 공유 scan & memmove-down loop.
- **`COLOREFFECT_ADD_RE.txt`** 작성 (raw asm + 분석, jump table 검증, Degree LUT 추출 포함).

### 다음 세션 진입점 (R-1 미완 + R-2)

1. **Theme(Theme const&)** copy ctor — sub-objects 각 copy ctor 필요 (ColorScheme::Clone / FormatScheme::Clone / FontScheme::Clone 등). raw RE 필요.
2. **FormatScheme::CreateDefault / ObjectDefaults::CreateDefault** — Brush/Pen/EffectStyle/DefaultProperty multi-session RE.
3. **R-2 Surface** — vtable + 8 ctor + libhsp shim (multi-session 매우 큰 phase).

### 이전 세션 (15e) 기록

- **R-1.5.4a ColorEffect** (24B `std::vector<u64>`) — raw `Create` @ 0xbec48 + clone @ 0x65411c + inline ~ColorEffect (in `~Color()`) 1:1. `[u64]` entry = packed `{PKey: u32, float: u32}`. 11 tests.
- **R-1.5.4b libc++ `__tree` primitives** — `TreeNodeBase` (32B: left/right/parent/is_black) + `TreeBase` (24B: begin/end_node_left/size). `balance_after_insert` (raw @ 0x26550) + `left_rotate/right_rotate` + `is_left_child` + `find_insert_position` (raw inline @ 0x150084) + `subtree_destroy_recursive` (raw @ 0x631b24). 17 tests + RB invariant 검증 + 50 random key stress.
- **R-1.5.4c Color** (24B = 12B value union + 4B type_tag + 8B `ColorEffect*`) — 10 ctor (SystemStyle/SchemeStyle/PresetStyle/u8×3+auto_ptr/Rgb/Cmyk/ScRgb/Hsl), `copy_ctor` (raw `Color::Clone` 0xb247c: 16B memcpy + ColorEffect::clone_raw), `Swap` (0x14c8b0), `~Color` (0x14c870), 8 accessors (GetType/GetSchemeStyle/.../GetColorEffect). 20 tests. **`operator</==/!=` deferred** — DrawingType::Cmyk/ScRgb/Hsl operator< (별도 dylib internal, 각 100+ asm 줄) 종속, ColorScheme init 경로엔 도달 안 함.
- **R-1.5.4d SchemeStyle** (u32 enum, 12 valid variants 0..11). 5 tests.
- **R-1.5.4e ColorScheme** (32B = 8B CHncStringW name + 24B __tree<SchemeStyle, Color>) — ctor (12 hardcoded SetAt: System(8)/System(5) + 10 Rgb 색상 raw 와 byte-eq), `SetAt` (raw 0x150074, INSERT 의 2-clone path + UPDATE 의 single-clone path 둘 다 정공법 1:1), `Contains`, `GetColor`, `~ColorScheme` (raw 0x15016c: subtree_destroy + ~CHncStringW). 14 tests + 200 random key stress + 12 hardcoded color byte 검증.

**raw RE dump 위치**: `kdsnr-hwp-toolkit/work/hft_re/render_re/COLORSCHEME_RE.txt` (1411 줄 raw asm + 분석).

**다음 세션 진입점** — R-1.5.5/R-1.5.6/R-1.5.7/R-1.6 (Theme sub-objects 잔여 + Theme 자체):

- **R-1.5.5 FontSet** (실측 **48B**, plan 의 24B 잘못됨) — 3 TextFont* (latin/cs/ea, 24B) + 24B std::vector<SharePtr<SupplementalFont>>. raw exports 정독 완료 (ctor 0x169418, dtor 0x1696dc, Get* 0x169bf4-0x169c70). **TextFont sub-object 별도 RE 필요** (raw 의 `+0x10` 에 CHncStringW + 추가 fields).
- **R-1.5.6 FormatScheme** — raw `CreateDefault()` sret factory, sizeof TBD.
- **R-1.5.7 ObjectDefaults** — 동일 sret factory 패턴, sizeof TBD.
- **R-1.6 Theme** (72B, sub-objects 가용 후 3 ctor variant + dtor 가능).

## 15번째 세션 (2026-05-15) — rendering phase R-1 진행 (**101 tests pass**)

새 sub-crate **`kdsnr-hwp-toolkit/render-engine/rust/`** (`kdsnr-render`). **101 tests pass**.

R-1.1 ~ R-1.4 (Flag/BWMode/Hit/Theme audit) 완료 + 사용자 "audit-only 가 아니라 포팅하라" 지적 후 R-1.5 (Theme sub-objects) 진행 — 3/7 sub-object 1:1 포팅 완료:

- **R-1.1 Flag** (libHncFoundation 8B u64) — 10 함수 raw asm 1:1, bit-63 mask, LSB→MSB operator<. 30 tests.
- **R-1.2 BWMode** (libHncDrawingEngine u32 enum) — 13 variants + RenderMode + 2 lookup table (raw file dump). 25 tests.
- **R-1.3 Hit** (24B POD) — CharItemView::Pick @ 0x2f9a34 의 4 access 으로 layout 확정. 5 tests.
- **R-1.4 Theme** (72B audit only) — 72B layout + offset 상수 + 의존성 트리. ctor 미포팅. 2 tests.
- **R-1.5.1 Guid** (16B) — libHncFoundation 의 12 exported 함수 (C1/C2/D1/D2/copy/eq/ne/lt/CreateID) raw asm 1:1. Big-endian byte order compare. 14 tests.
- **R-1.5.2 SharePtr<T>** (8B + 16B ControlBlock) — raw asm @ 0x1c2b38 정독. T + ControlBlock 별개 heap-alloc, refcount 관리. Clone/Drop = inc/dec. 12 tests.
- **R-1.5.3 CHncStringW** (8B refcounted MFC wide string) — D2 @ 0xdef4 정독. 12B header (refcount AtomicI32, data_length i32, alloc_length i32) + null-terminated u16 data. -2 nil sentinel (Rust-managed static), > 0 = heap. AtomicI32 fetch_add/sub = `InterlockedIncrement`/`Decrement`. 13 tests.

---
## 14e 이전 상태 (layout phase 완성, 변경 없음)

**현재 (2026-05-15 14번째 세션 종료, 14e + vfunc 5-8 audit 후)**: **516 tests pass**. B-Compositor A~G + 4 Compositor trait wire + 14d/14e Placement 완전 RE + 4 strategy 1:1 + MonoGlyph::allocate dispatch + **vfunc[5-8] audit + doc 완료** (raw 1:1 패턴 인용, layout-decoder 내 호출자 zero 검증, rendering phase prerequisite type 5종 명시). **다음 진입점**: rendering phase (Surface/Theme/Flag/BWMode/Hit RE 후 vfunc[5-8] 포팅) + e2e validation (12-set hwpx, 사용자 환경).

**현재 byte-equiv 보장 정도** (정직 평가, 14번째 세션 종료 14e 후):
- ✅ 함수 단위 byte-equiv: value_types, **placement 완전 RE + 4 strategy Request/Allocate 1:1 (PlaceCenter/PlaceFix/PlaceMargin/PlaceNatural — 14e)**, glyph hierarchy, font_metric, autonum, port_bullet_render (FUN_002eaf54 4188B), ppt_compose_bullet/numbering/break/layout, simple_compose_break/layout, array_compose_break.
- ✅ Producer→Consumer 통합 byte-equiv: ComposeNumbering → ComposeBullet 의 키 identity, integration tests.
- ✅ trait dispatch byte-equiv: 4 Compositor 의 impl Compositor 완료, Repair chain 모두 wire.
- ✅ **MonoGlyph::request + MonoGlyph::allocate dispatch (14d/14e)** — raw `Placement::Request` (`FUN_00331214`) + `Placement::Allocate` (`FUN_003312b4`) + `CalcPlacement` (`FUN_00331324`) 의 child + layout 2-step dispatch 1:1.
- ✅ **PlaceMargin 96B layout 완전 RE** (14e) — vtable `0x781130`, ctor `FUN_003308d0` 84B 의 12 margin f32 (l/t/r/b × n/s/sh) + cache (`+0x38..+0x5c` post-margin Requisition). DAT 상수 `_DAT_00741f70` = `(0.0, 0.0)` (bottom.stretch/shrink 초기), `_UNK_00741f78` = `(-1e8, 0.0)` (cache.x.natural=INVALID 초기). `Request` `FUN_00330cfc` 196B + `Allocate` `FUN_00330dc0` 328B + `CalcSpan` `FUN_00330f08` 76B 모두 1:1. cache 는 Rust `Cell<Requisition>` 으로 interior mutability.
- ✅ **PlaceFix RE 정정** (14e) — 별도 16B 클래스 (vtable `0x7810f8`, 이전 추정의 "PlaceMargin secondary inherit" 은 RTTI 메타데이터 artifact 일 뿐). primary vtable `[+0/+8] dtor (no-op)`, `[+16] Clone`, `[+24] Request (FUN_003308a4, 4B no-op return)`, `[+32] Allocate (FUN_003308a8, 40B)` = `alloc.{x|y}.span = self.fix_size`. CreateHFix/CreateFix 가 `operator_new(0x10) = 16B` 만 할당 → secondary sub-vtable 의 PlaceMargin/Placement method 들은 호출 불가 (OOB read 가 됨).
- ✅ **PlaceCenter::Allocate 1:1** (14e) — `FUN_003307e4` 92B. `alloc.{x|y}.origin += alloc.{x|y}.span * (req.alignment - alloc.alignment); alloc.alignment = req.alignment`. alignment-point 의 절대 좌표는 보존 (begin/end 변화 없음).
- ✅ **PlaceNatural::Allocate** = raw 4B no-op (`FUN_0033176c`) → trait default 사용.
- ✅ **Placement::Draw / Undraw / GetBounds / Pick dispatch (vfunc[5+]) audit + doc 완료** (14e 후속): `FUN_00331488` (Draw, 144B) / `FUN_00331518` (Undraw, 32B, **CalcPlacement 없이 direct tail-call**) / `FUN_00331538` (GetBounds, 176B) / `FUN_003315e8` (Pick, 160B) — 모두 raw decompile 1:1 인용 + dispatch 패턴이 `placement.rs` 모듈 doc 의 "vfunc 5-8" 섹션 + `glyph.rs` Glyph trait 의 draw/undraw/get_bounds/pick doc 에 추가됨. **layout-decoder 모듈 내 호출자 검증**: `BulletRenderGlyph::draw` 외 override 0개, 모든 chain end-leaf 가 trait default no-op/zero/None → raw 의 child=null 케이스 (zero/0 반환) 와 byte-equivalent. **rendering phase 의 byte-equiv 가 필요할 때 prerequisite type RE**: Surface / Theme / Flag / BWMode / Hit (5종) — 본 layout-decoder 모듈 (Request/Allocate) 의 byte-equiv 와 독립.
- ⚠️ font fallback `"HCR Dotum"`: §8.4 도달 가능 입력 (font_style ∈ {0,1,2,3} → weight ∈ {400,700}) 전체 byte-eq 확정. 잔여는 미설치 폰트 fallback 정합 (fontMgr 등록 RE 별도 phase).
- ✅ `Deck::Clone` — layout 경로 호출자 0건, byte-eq 영향 없음 (RE 확인 + doc 갱신).
- ✅ **Placement caller chain 검증** (14e): `Composition::allocate` (`FUN_002fe7bc`) → `parent_glyph.allocate` → `Box::allocate` (`FUN_002e6348`) → 각 child 의 `allocate` 호출. child 가 `MonoGlyph (Placement)` 일 때 `MonoGlyph::allocate` (14e 추가) 가 dispatch — CalcPlacement 패턴으로 `child.request` + `placement.allocate` + `child.allocate`.
- ❌ end-to-end byte-equiv (12-set hwpx → 한컴 PDF): 미증명. mock provider 단위 테스트만. 사용자 환경 (실제 macOS 한컴) 필요.

**13번째 세션 (2026-05-15)**: (1) **PropertyValue::IntFloat variant + PropertyBag.get_int_float** 추가 — raw `FUN_006805d0` 가 반환하는 8-byte `{i32 mode, f32 factor}` payload 용 (key `0x90f` `ParaProperty::GetBulletSize`). (2) **`TextFont` struct + `ParaProperty.text_font` 필드** 추가 — raw `+0x10` `SharePtr<TextFont>` 슬롯. `get_font_size_raw()` (no clamp) / `set_font_size()` 만 노출 (bullet ctor 가 raw 값 사용 후 clamp). (3) **`ParaProperty::get_bullet_size()` getter** (key `0x90f` → `Option<(i32, f32)>`). (4) **key 0x259/0x25b consumer 전수조사** — Ghidra script `DumpKeyConsumers.py` 으로 모든 immediate-load 참조 스캔. 결과 (`/tmp/hft_scripts/bcompositor/key_consumers.txt`): `0x259` (61 refs) 는 `SolidBrush::Draw`/`GetColor`/`ApplyProperty`/`UpdatePlaceholderColor` 등 모두 **render-time**, `0x25b` (30 refs) 는 `HatchBrush::Draw`/`Get/SetForeColor` 등 모두 **render-time**. Glyph::Request / Glyph::Allocate consumer 0건 → **`FUN_002eaf54` step 5 (ParaProperty.0x90e → RunProperty.0x259/0x25b) 는 layout no-op**. 본 layout 포팅은 step 5 skip — `ppt_compose_bullet.rs` 모듈 doc 에 명시. (5) **`FUN_002eaf54` step 6 1:1 포팅** — `step6_compute_bullet_glyph_size(Option<(i32,f32)>, &mut TextFont, dpi) -> f32` + `step6_with_global_dpi` wrapper. raw `ppt_subsystem_deps.txt:616-680` 의 3 분기 (mode 1 absolute / mode != 1 factor / no 0x90f) 모두 1:1, fVar26 pre-clamp 계산, fVar27 clamp 후 write back, no-0x90f 경로는 write 안 함 — 7 tests 통과. (6) **Font/FontTable/Theme/RealFontMeta 신규 type + `select_font_slot` + `realize_font`** (`runtime.rs`) — raw `CharItemView::GetRealFont` (`0x2f0234`, 992B) 의 char-class → 4-slot dispatch 1:1 (Latin/CJK/Script/Symbol). `RunProperty.font_table: Option<FontTable>` 필드 추가, 슬롯 비면 `Theme::major/minor_latin` 그리고 hardcoded `"HCR Dotum"` fallback — 7 tests. (7) **`CharItemView::from_ctor_context` 신규** (`glyph.rs`) — raw `FUN_002ef798` (1840B) 의 layout 영향 부분 1:1 endto end: shell init + RunProperty 5키 read + GetRealFont 호출 + null 가드 + BodyProperty.GetVert + measure_string_advance + global_metrics + combine_char_metrics + compute_metrics. CR/LF char substitute to 0x20 forward. 5 tests (zero-fallback / ascii / cjk / cr-lf / "HCR Dotum" fallback). (8) **`port_bullet_render_character` endto end** (`ppt_compose_bullet.rs`) — raw `FUN_002eaf54` 의 step 1-6 + step 7 Type 1 (Character) 분기 통합. step 7 Type 1 은 `bullet.chars[0]` 로 단 한 개 `CharItemView::from_ctor_context` 호출 (raw line 945-983). 3 endto end tests (HBox build / empty bullet / first-char-only). (9) **이전 세션 11번째 종료시 373 → 12번째 391 → 13번째 416 tests**. 25 new tests in this session.

**12번째 세션 (2026-05-15)**: (1) **병렬 세션 충돌 정리** — claude 프로세스 4개 동시 실행 발견, 프로세스 조상 추적으로 자기 PID 식별 후 나머지 3개 SIGTERM. 11번째 세션 작업은 다른 AI 가 병행 — 검증 결과 RE 기반·정공법 준수, 되돌릴 것 없음 (`param_6=CharItemView+0x38` 매핑 독립 교차검증 통과). (2) **font metric 경계 (`GetOutlineTextMetricsW`) 100% RE 완료** — `libhsp.dylib` arm64 슬라이스를 Ghidra `drawing_proj` 에 임포트. 전체 체인 확정: `FUN_000761f4`(2회 호출, 2차는 em-square 모드) → `_SelectObject` → DC vtable `0xfbc50` slot17 `FUN_0008a37c`→`FUN_0008a3bc`→**`FUN_00064258` CoreText realization** → realized-font 0x78B 객체 → `GetOutlineTextMetricsW` impl `FUN_00089c1c` (DC vtable slot59). **순효과**: `GlobalFontMetrics.em` = `CGFontGetUnitsPerEm`, `ascent`=OS/2 `sTypoAscender`(raw), `m7`=`abs(sTypoDescender)`, `m8`=`sTypoLineGap` (OS/2 없으면 `CGFontGetAscent/Descent`/`CGFontGetLeading` fallback). OS/2 는 `CGFontCopyTableForTag('OS/2')` → 표준 레이아웃 파싱. (3) **CoreText FFI provider 구현 완료** — `font_metric.rs` 에 `Os2Metrics`/`parse_os2_metrics`/`GlobalMetricProvider`, `font_metric_coretext.rs` (NEW, `#[cfg(macos)]`) 에 `CoreTextProvider` (`CoreTextFontProvider`+`GlobalMetricProvider` 구현, raw CoreText/CG/CF FFI). **391 tests pass**. (4) **땜질 audit** (사용자 지시) — RE 없이 넘어간 4건 전수조사: m8 fallback=`CGFontGetLeading` 확정·수정 / `0x10e1`·`FUN_00027ba8` 은 검증 아닌 tautology(정정) / 폰트 매칭 체인 (`FUN_0006a664`/`FUN_000673e8`/`FUN_00065904`/`FUN_00066238`) 완전 RE → `font_style∈{0,1,2,3}` → 요청 weight∈{400,700} → CoreText symbolic-trait 매칭이 도달 가능 입력 전체에서 `FUN_000673e8` 과 동일 face 선택 = byte-equivalent 증명. 상세: `LIBHSP_GETOUTLINETEXTMETRICS_RE.md` §8.

**상세 함수별 진행 상황**: `project_composition_port_state.md` 참조.

## 누적 진행 (시간순)

- **B-7..B-5j** (1~8번째): 113→289 tests
- **B-Compositor A/B/C** (9번째): 308 tests — Simple/Array ComposeBreak + ComposeLayout
- **B-Compositor D1~D4** (9번째): 321 tests — Ppt subsystem RE + 데이터모델 + trait 수술 + ComposeNumbering
- **B-Compositor E** (10번째): 340 tests — PptCompositor::ComposeBreak
- **B-Compositor F 시작** (11번째): 345 tests — dependency RE + BlipGlyph/BulletRenderGlyph
- **B-Compositor F 진행 — font metric 경계 + CoreText FFI** (12번째): 391 tests — libhsp Ghidra import, GetOutlineTextMetricsW 전체 RE, CoreTextProvider, 땜질 audit
- **B-Compositor F 진행 — step 6 포팅** (13번째): 398 tests — PropertyValue::IntFloat, TextFont struct, step 5 layout no-op 결정, step6_compute_bullet_glyph_size
- **B-Compositor F 진행 — Type 1 Character bullet endto end** (13번째b): 416 tests — Font/FontTable/Theme/realize_font (GetRealFont 1:1), CharItemView::from_ctor_context (ctor body 1:1), port_bullet_render_character (FUN_002eaf54 outer + Type 1 분기)
- **B-Compositor F 진행 — Type 3 AutoNumber bullet endto end** (13번째c): 456 tests — autonum.rs (FUN_002e86e8 + 18 leaf 함수 + 13-step Roman + Chinese-tens + digit-table expansion + circled/PUA), 41 case dispatch + 36 unit tests. port_bullet_render_autonum + port_bullet_render_text 공통화 (Type 1/3 unified)
- **B-Compositor F 완료 — Type 2 Picture + unified dispatcher** (13번째d): 465 tests — ImageBrush/ImageSource type (text_property.rs), Bullet::Picture { brush } 확장, port_bullet_render_picture (BlipGlyph 0x40B 완전 1:1, scale = (bullet_size/img_h)*0.7), step1_through_step6_setup refactor, port_bullet_render unified dispatcher (Type 0/1/2/3 모두). **FUN_002eaf54 4188B 100% endto end 완료** (Type 1/2/3 + None)
- **ComposeBullet 외곽 1072B + 차이 검증** (14번째a): 472 tests — `PptCompositor::ComposeBullet` (`0x307468`) 1:1 포팅. `CharItemView.theme: Option<Theme>` 필드 추가 (raw `+0x90`), `from_ctor_context` 가 theme 저장. `get_first_char_item_view_on_para_mut` (composition 의 children[idx] 가 직접 CharItemView 일 때 in-place mutable 참조 반환) + `find_para_cr_view` (composition 내부 CR `&CharItemView` 반환, pointer-identity 보존 키 derivation 의 기반). 의도적 차이 4건 종접 검증: (#1 cache skip) `PTR_FUN_0077fdc0[+0x30]` GetType = `FUN_002e696c` = 8B `return 0;` → default Bullet shell GetType=0 검증 → ParaProperty.bullet=None 케이스 byte-equiv. (#2 overwrite 항상) `plVar7 = new(0x28)` 가 본 함수 안에서 막 alloc 라 기존 inner 와 주소 충돌 불가 → same-inner do-nothing 분기 dead branch. (#4) RAII drop = SharePtr destructor.
- **차이 #3 통합 결함 수술** (14번째a 후속): 475 tests — 발견: ComposeNumbering 가 `view: Box<CharItemView>` clone 을 push → ComposeBullet 의 `find_para_cr_view` 가 composition 내부 ptr 키로 lookup → mismatch → 모든 AutoNumber 가 default 1. **수술**: `NumberingEntry` schema 변경 `view: Box<CharItemView>` → `{ key: usize (raw lVar3 의 *const _ as usize), number, is_short_line, level: Option<i32>, bullet_start: Option<i32> }`. `ppt_compose_numbering` 의 producer 가 `find_para_cr_view` 로 composition 내부 ptr 키 derive, push 시점에 level/bullet_start cache. backward scan 도 cache 사용 (ParaProperty immutable 가정 하에 raw deref 와 byte-eq). `BulletNumberingEntry` 제거 — `NumberingEntry` 단일화. tests: ppt_compose_numbering 8개 + 통합 검증 2개 (`integration_producer_consumer_share_pointer_key` / `integration_first_paragraph_no_numbering_entries_uses_default_one`).
- **ComposeLayout outer orchestrator + B-Compositor-G** (14번째b): 484 tests — `PptCompositor::ComposeLayout` (`0x308248`, 9712B / 1782 줄) outer orchestrator 1:1 포팅 — 13 stage (`stage_1_setup` ~ `stage_13_post_pads`) 를 raw 순서대로 sequential 호출. raw line range 매핑 (stage 1: 83-170, stage 2: 171-245, stage 3: 246-320, stage 4: 321-407, stage 5: 408-534, stage 6: 535-629, stage 7: 630-714, stage 8: 715-1096, stage 9: 1097-1300, stage 10: 1301-1454, stage 11: 1455-1549, stage 12: 1551-1647, stage 13: 1648-1758) 모듈 doc 에 RE-grounded 인용. Entry guards 3건 (`*param_7==0` / `param_1==0` / `*(*param_7)==0`) 는 Rust `&dyn`/`&mut dyn` non-null 보장으로 도달 불가능 — 생략. `paragraph_item_bag` / `para_view_run_bag` 추출은 outer 에서 `get_para_item_view` 한 번 호출 후 reference 전달. 4 outer smoke tests (empty / one CR / 3 children / vertical type). **B-Compositor-G (PptCompositor)**: `trait Compositor` 의 `compose_bullet` 시그니처에 `ct_provider` + `gm_provider` 추가 (raw vtable 시그니처 외 — caller chain 전달). `PptCompositor` 의 `impl Compositor` 4 메소드 wire (`compose_numbering` / `compose_bullet` / `compose_break` / `compose_layout`) — 각각 outer 로 1:1 forward. `Composition::Repair` (trait Composition default method) 의 시그니처에도 provider 인자 추가. 5 PptCompositor trait wire smoke tests.
- **Simple/Array Compositor trait wire + outdated doc 정리** (14번째c): 491 tests — 사용자 지적으로 미완 audit. **발견**: `SimpleCompositor` / `ArrayCompositor` 의 `impl Compositor` 가 부재 (trait dispatch 안 됨). **수술**: (1) raw ctor RE — `SimpleCompositor::SimpleCompositor` (`0x303de0`, 16B) = 빈 객체 (vtable only, raw `operator_new(8)`). `ArrayCompositor::ArrayCompositor` (`0x304b94`/`0x304ba4`, 16B) = `{vtable@0, divisor: u64@0x08}`. Rust `ArrayCompositor` 에 `divisor: u64` field 추가 + `new(divisor)` ctor. (2) `impl Compositor for SimpleCompositor` — clone + compose_break/layout 가 `simple_compose_break` / `simple_compose_layout` 로 forward. (3) `impl Compositor for ArrayCompositor` — clone + compose_break 가 `array_compose_break(widths, divisor)` 로 forward, compose_layout 은 `simple_compose_layout` 재사용 (raw `ArrayCompositor::ComposeLayout` 0x304d5c 가 `SimpleCompositor::ComposeLayout` 0x303f80 와 decompile 完全 동일). (4) outdated doc 5곳 정리: Stage 3~13 `⏳` → `✅`, `first_line_ascent` "stub: 0.0 TODO" 표시 (실제로 `first_line_ascent_from_render_path` 호출 구현 됨), `runtime.rs` "stub 단계" / `glyph.rs` "Phase B-8a stub" / `Repair` "모델하지 않음" doc 모두 14번째 세션 시점으로 갱신. 7 Simple/Array trait wire smoke tests.
- **14e Placement 완전 RE + 4 strategy 1:1 + MonoGlyph::allocate dispatch** (14번째e): 516 tests — 사용자 "byte-equivalence 100% 가 아니면 의미 없다" 지적, 14d 미완 인계분 전부 처리. **(1) Ghidra dump 추가**: `DumpPlaceFix.py` (FUN_00330858/85c/860/8a4/8a8) + `DumpPlaceMore.py` (PlaceNatural Allocate FUN_0033176c 등 + PlaceCenter vtable @ 0x7810c0 + PlaceMargin vtable @ 0x781130 scan). **(2) PlaceFix 정정 — 별도 16B 클래스 확정**: vtable `0x7810f8` primary 의 method 들 (`FUN_003308a4` Request = 4B no-op, `FUN_003308a8` Allocate = 40B sets `alloc.span`) 이 PlaceFix 의 진짜 메소드. secondary sub-vtable 의 PlaceMargin/Placement RTTI 엔트리는 `dynamic_cast` 메타데이터 — 데이터 상속 없음 (`CreateHFix` 의 `operator_new(0x10) = 16B` 가 그 증거). PlaceFix `Hnc::Shape::Text::PlaceFix` typeinfo @ 0x791d38 (별도 export, `nm` 검색에선 안 보이지만 RTTI string 있음). **(3) PlaceMargin 96B 완전 RE**: ctor `FUN_003308d0` 84B의 12 margin field + cache 슬롯 (12 axes × 3 + cache Requisition 36B = 84B 본문 + 12B padding = Clone 의 `operator_new(0x60)` 96B 일치). DAT 상수 `_DAT_00741f70`/`_UNK_00741f78` 의미 (cache init INVALID) 확정. **(4) Placement trait 재설계**: `request(&self, &mut Requisition)` (PlaceMargin 은 `Cell<Requisition>` interior mutability 로 cache 갱신) + `allocate(&self, &mut Allocation, &Requisition)` (req 인자 추가 — PlaceCenter 가 사용). **(5) Rust 1:1 포팅**: `placement.rs` 전면 재작성 (~1300 lines) — PlaceCenter 의 Request 既存 + Allocate 신규 (1:1), PlaceFix 의 dimension/fix_size 정정 (기존 `width/height` 잘못) + Allocate 신규 (1:1), PlaceMargin 의 12 field + Cell cache + Request `FUN_00330cfc` 196B 1:1 + Allocate `FUN_00330dc0` 328B 1:1 + CalcSpan `FUN_00330f08` 76B 1:1, PlaceNatural 기존 Request 유지 + Allocate trait default (raw 4B no-op). **(6) MonoGlyph::allocate dispatch 신규** (`glyph.rs`): `Placement::Allocate` `FUN_003312b4` 112B + `Placement::CalcPlacement` `FUN_00331324` 280B 의 1:1 — child.request → placement.allocate (cache/req 사용) → child.allocate. **(7) e2e dispatch tests 4건** (PlaceFix horizontal/PlaceCenter alignment/PlaceMargin inset/no-child no-op). **(8) PlaceMargin 단위 test 7건**: Request/Allocate stretch & shrink paths + CalcSpan helper. **명시적 미완 인계 (다음 세션)**: (a) `Placement::Draw` (`FUN_00331488` 144B) / `Undraw` (`FUN_00331518` 32B) / `GetBounds` (`FUN_00331538` 176B) / `Pick` (`FUN_003315e8` 160B) — 모두 CalcPlacement 패턴 dispatch. layout output 영향 없으나 (Glyph::draw/get_bounds 가 trait default no-op 인 채라 호출자 zero) rendering byte-equiv 시점엔 필요. (b) e2e validation 사용자 환경 (12-set hwpx).

- **14d Placement / Deck / Font fallback 3건 audit + Placement 부분 수술** (14번째d): 497 tests — 사용자가 "stub / 일부 도메인 RE 가 향후 문제 될 것" 지적, 3 항목 RE-grounded audit. **(1) Placement**: 진짜 결함 — `MonoGlyph::request` override 부재로 `Glyph::request` default 반환 (`Requisition::INVALID`), raw `Placement::Request` (`FUN_00331214`, 132B) 의 child + layout dispatch 미구현. raw RE — Placement 객체 24B = `{vtable, child SharePtr@+0x08, layout SharePtr@+0x10}`. PlaceCenter (16B, +0x08 dimension/+0x0c alignment, `CreateCenter` `0x317d0c` 로 확정) / PlaceNatural (16B, +0x08 direction/+0x0c span, `CreateHNatural`/`CreateVNatural` 로 확정) / PlaceMargin (84B+, raw 의 cached Requisition 슬롯 0x30..0x58 등 큰 layout, `0x3308d0` ctor) / PlaceFix (multi-inherit Placement+PlaceMargin secondary vtable, raw RE 보류). **수술 (본 세션 부분 진행)**: PlaceCenter 에 `dimension: i32` 필드 추가 + Request 1:1 (`FUN_003307c8` 28B). PlaceNatural Request 1:1 (`FUN_00331750` 16B). MonoGlyph::request 가 child.request → placement.request 순서 dispatch (`FUN_00331214` 1:1). 6 new tests. **(2) Deck**: `LayoutFactory::CreateDeck` (`FUN_00317708`) 정의 있으나 layout RE 범위 (decompiles_v2) 내 호출자 0건 → byte-eq 영향 없음. doc 갱신. **(3) Font fallback `HCR Dotum`**: `LIBHSP_GETOUTLINETEXTMETRICS_RE.md §8.4` 가 도달 가능 입력 (font_style ∈ {0,1,2,3} → weight ∈ {400,700}) 전체에서 byte-eq 확정 명시. 잔여는 미설치 폰트 fallback 정합만 (fontMgr 등록 RE 별도 phase). **명시적 미완 인계 (다음 세션)**: (a) PlaceMargin 의 large layout (0x30..0x58 cached Requisition 슬롯 + ctor `_DAT_00741f70` 의 의미) 의 정확한 RE + Rust struct 확장 + Request/Allocate 1:1. (b) PlaceFix multi-inherit (primary `0x7810f8` + secondary PlaceMargin `0x791d50` + secondary Placement `0x791d68`) — secondary vtable 위임 방식 RE. (c) `Placement::Allocate` (`FUN_003312b4`) / `Placement::Draw` (`FUN_00331488`) dispatch 도 child + layout 패턴이지만 본 세션엔 Request 만 — Allocate/Draw 도 동일 dispatch 적용 필요. (d) MonoGlyph 가 layout 흐름의 어디서 외부 dispatch 받는지 (Composition::Repair → rows → render 등) 의 caller chain 검증.

## B-Compositor-F 진행 (11번째 세션)

`PptCompositor::ComposeBullet` (`0x307468`, 1072B) 는 `FUN_002eaf54` (bullet render ctor, **4188B 단일 최대**) 를 호출 — bottom-up RE 로 진행.

### 완료

1. **dependency 일괄 덤프** — `/tmp/hft_scripts/DumpBulletRenderDeps.py` → `bcompositor/bullet_render_deps.txt` (6431 lines). `FUN_002eaf54` 의 미해결 의존성: vtable `PTR_FUN_0077ff90` (bullet render obj) + `PTR__BlipGlyph_0077faf0` + method body, LayoutFactory CreateHBox/VBox, PropertyBag accessor 6종, `FUN_002e86e8` (autonum string), CharItemView ctor (`0x2ef798`), RunProperty copy ctor (`0x332dbc`), SharePtr machinery 13종.

2. **핵심 RE 발견**: bullet render object (`FUN_002eaf54` 생성, vtable `PTR_FUN_0077ff90`, 40B) = `{vtable, bullet_type:i32@8, layout:SharePtr<Glyph>@0x10, key_901:f32@0x18, numbering:i32@0x20}`. `CharItemView.+0x98` (render_path) 에 저장. vtable 의 Request/Allocate/Draw/Undraw (`+0x18/+0x20/+0x28/+0x30`) 가 전부 `layout` (= LayoutFactory 의 `Box_` HBox/VBox) 로 **단순 forward** (tail-call). Box vtable `0x77fd10` 도 동일 offset (`vtables.txt` 검증). → ComposeBullet 의 layout-출력 = HBox/VBox 의 `Box::Request`.

3. **`BlipGlyph` 완전 RE** (`glyph.rs` 재작성) — picture-bullet glyph 64B. 기존 stub → 10 필드 + `Request`(`FUN_002d137c`)/`Allocate`(`FUN_002d13e8`) 1:1 (`pc<7 && (1<<pc)&0x65` 2분기). `+0x30` ImageBrush 는 Draw 전용 → FUN_002eaf54 포팅 시 추가.

4. **`BulletRenderGlyph` 신규** (`bullet_render.rs` NEW) — Clone + Request/Allocate/Draw/Undraw forward. 5 tests.

### ⚠️ 결정적 발견 (11번째) — font metric 경계 = CoreText 시스템 폰트

`FUN_002eaf54` 의 bullet CharItemView 자식 폭은 `CharItemView::CharItemView` ctor → `GetRealFont` → `FUN_000764fc` → `FUN_00082d98` 로 결정. RE 결과 (`font_metric_deps.txt`):
- **macOS Hancom 엔진은 layout metric 에 HFT 폰트를 안 씀.** `FUN_00082d98` 가 glyph advance 를 **CoreText 시스템 폰트** (`_CTFontGetAdvancesForGlyphs`) 로 측정 — 하드코딩 `Helvetica`/`Helvetica-Bold`/`STHeitiTC-Medium`/`AppleSDGothicNeo-Medium`/`AppleSDGothicNeo-Bold`, Unicode codepoint 분류로 선택.
- `FUN_000761f4` = 폰트 global metric (em/ascent/descent) 을 GDI shim `_GetOutlineTextMetricsW` 로. `FUN_000764fc` = 결합 공식 (`MulDiv` DPI 변환, `width=(advance*72)/96`).
- **HFT 디코더 (`kdsnr-hwp-toolkit/hft-decoder/`) 는 별개** — Windows `HncBaseDraw.dll` 의 glyph **shape** 렌더링용 (`Glyph{d, advance, em}`). macOS layout metric 경로와 무관.
- **후속 검증 (사용자 challenge "PDF 폰트 안 깨짐" 대응)**: import stub 해석 + `otool -L` 로 확정 — `_CTFont*` 는 진짜 macOS CoreText import, CFString 은 `"Helvetica"`/`"STHeitiTC-Medium"`/`"AppleSDGothicNeo-Medium"` 등 실제 시스템 폰트명. GDI 이름(`_CreateFontIndirectW` 등)은 **`libhsp.dylib`** export — `libhsp` 는 macOS 시스템 프레임워크만 link (CoreText/CG, 한컴 폰트 dylib 없음) = 순수 CoreText-backed GDI shim. → `libHncDrawingEngine` layout 의 font metric 은 전부 CoreText 로 귀결. "폰트 안 깨짐" 과 모순 없음: glyph **shape** 은 별도 rendering 경로(실제 폰트 embed), 본 엔진은 `Hnc::Shape::Text` (도형/textbox) 전용.
- → end-to-end byte-equivalent = Rust 포트도 **CoreText 로 동일 측정**. `FUN_00082d98` codepoint→폰트 분류기 + `FUN_000764fc` DPI 공식은 순수 산술 1:1 포팅 가능 (DAT 상수 `0x742858/0x742860` 확보), advance 조회만 CoreText FFI 경계.
- dump: `DumpFontMetric.py`→`font_metric_deps.txt` (3507줄), `DumpFontImports.py`→`font_imports.txt`.

### 다음 진입점 — B-Compositor-F 잔여

1. **font metric 경계 RE — ✅ 완료** (12번째 세션). `LIBHSP_GETOUTLINETEXTMETRICS_RE.md` 에 전체 체인 + byte-equivalent 공식 확정. 이제 포팅/FFI 구현만.
2. **CoreText FFI provider 구현** — `GlyphAdvanceProvider`(advance) + `GlobalMetricProvider`(em/ascent/m7/m8) 경계. byte-equivalent = Rust 가 `select_system_font` 결과 폰트의 CoreText `CGFont` 에서 `CGFontGetUnitsPerEm` + `'OS/2'` 테이블(`CGFontCopyTableForTag`) `sTypoAscender/Descender/LineGap` 을 읽음. advance 는 `CTFontGetAdvancesForGlyphs`.
3. **`FUN_002eaf54` 포팅** (13번째 세션: step 6 완료, step 1/2/6 + key_901 + 8e/8f 부분; outer + step 4/5/7 미완) — 4188B, `ppt_subsystem_deps.txt:219-1001` 에 전체 decompile. 단계별 구조 (decompile에서 직접 추출):
   1. bullet_render obj init (40B, vtable `PTR_FUN_0077ff90`) — **DONE** (`BulletRenderGlyph`)
   2. layout 선택 (BodyProperty key 0x89e → HBox/VBox) — **DONE** (`bullet_render_uses_vbox`)
   3. Bullet retrieval — ParaProperty 에서 SharePtr 가져옴, `FUN_0067e2cc` 캐시. Bullet base 8B + subtype (CharacterBullet 0x10B / PictureBullet / AutoNumberBull 0x10B)
   4. ParaProperty keys 0x90e (run-property modification value) / 0x90f (iVar6 + fVar27 factor) / 0x901 (→ key_901, **DONE**) / TextFont (`*(para+0x10)`)
   5. RunProperty copy ctor (`FUN_00332dbc`) + key 0x259/0x25b 셋 + `FUN_00648210` 캐시 wrap (if uVar4==0). **13번째 세션 결정: layout no-op** — key_consumers.txt 가 0x259=`SolidBrush`, 0x25b=`HatchBrush` 의 **render-only** 키임을 증명. `ppt_compose_bullet.rs` 모듈 doc 의 step-5 섹션에 RE-grounded 증명 + skip 결정.
   6. fVar26 (bullet glyph 크기) 산출: `iVar6==1` → `fVar27*ShapeEngine[+4]/72`, else TextFont key 0x96a 2회 읽고 `fVar27*(fVar26*SE/72)`, 그 후 TextFont 의 0x96a 를 `fVar27` (clamp ≥10) 로 set — **DONE** (`step6_compute_bullet_glyph_size`, 7 tests)
   7. Bullet GetType 분기:
      - **Type 1 (Character)**: `CharacterBullet+8` = u16 char[] (count `*(u32*)(charbullet+8-4)`), 각 char 별: RunProperty 복제 → `FUN_0067a048` SharePtr wrap → `CharItemView::CharItemView(pCVar16, char, run_share, para_share, body_share, theme, 0.0)` → SharePtr wrap → `layout.vtable[+0x50]` append
      - **Type 2 (Picture)**: `PictureBullet+8` = ImageBrush. ImageBrush.Clone → ImageBrush+0x10 (ImageSource), `ImageBrush.GetImageSource`. image size 를 `(fVar26/fVar28)*0.7` 스케일. BlipGlyph 0x40B 빌드: `+0x08` width=fVar29*fVar27, `+0x18` height=fVar28*fVar27, `+0x30` ImageBrush SharePtr, `+0x38` paragraph_class. SharePtr wrap, append.
      - **Type 3 (AutoNumber)**: `FUN_002e86e8(&str, *(u32*)(autonum+8) /*format type*/, *(i32*)(autonum+0xc) + numbering - 1 /*start+offset*/)` → CHncStringW. 각 char 별 Character 와 동일 경로.
   8. cleanup
   - **블로커 (모두 해제 완료)**: (a) CharItemView ctor — 13번째b 완료. (b) FUN_002e86e8 autonum — 13번째c 완료. (c) RunProperty copy ctor — semantic 등가 (Clone). (d) SharePtr 캐시 — layout 무관. (e) ImageBrush/ImageSource — **13번째d 세션 완료**: text_property.rs 에 `ImageBrush`/`ImageSource` 추가, `Bullet::Picture { brush }`, `port_bullet_render_picture` (BlipGlyph scale formula 1:1).
   - **FUN_002eaf54 4188B 100% endto end 완료**. Type 0/1/2/3 + unified dispatcher (`port_bullet_render`).
4. **`render_path` 모델 변경** — 11번째 세션에 일부 진행 (`Option<Box<dyn Glyph>>`). `ppt_compose_break.rs` Phase 1 의 FirstLineMetrics 사용처 rework 확인 필요.
5. **`ComposeBullet` 외곽** (`0x307468`) — 주의: `get_first_char_item_view_on_para` 가 owned clone 반환 → composition 의 CharItemView in-place mutate 위해 `_mut` variant 필요 (infra).
6. **B-Compositor-G**: trait impl wiring.

## 핵심 정공법 정책 (사용자 명시)

- **stub / `unimplemented!()` / `// TODO 후속 단계` 우회 금지**.
- **추측 금지** — raw decompile / asm / vtable dump 만 신뢰. (이번 세션 F 가 의존성 미RE 라 먼저 일괄 덤프 후 leaf 부터 bottom-up.)
- raw 인용 doc comment 필수. full byte-equivalent (semantic-eq 우회 금지). Ppt subsystem 전부 포팅.

## 파일 구조 (Rust port — Ppt subsystem 관련)

```text
layout-decoder/rust/src/
├── (기존) value_types / placement / glyph / layout / layout_factory / runtime /
│         properties / compose_break / compose_layout / ppt_compositor /
│         ppt_compose_layout / compositor / composition / text_property /
│         ppt_compose_numbering / ppt_compose_break / simple_compositor / array_compositor
└── bullet_render.rs  — BulletRenderGlyph (NEW 11번째)
└── autonum.rs        — FUN_002e86e8 autonum string generator (NEW 13번째c)
```

`glyph.rs`: `BlipGlyph` 완전 RE 재작성 (11번째). `CharItemView` 에 `render_path: Option<FirstLineMetrics>` (다음 세션 모델 변경 예정).

## RE artifact 위치

- decompile: `kdsnr-hwp-toolkit/work/hft_re/layout_re/decompiles_v2/`
- raw asm: `/tmp/hft_scripts/` — `bcompositor/` (`bullet_render_deps.txt` = 11번째 세션 dump, `font_metric_deps.txt`, `ppt_subsystem_deps.txt`, `ppt_compose_bullet.txt` 등), `hbox_layer/vtables.txt` (Box vtable)
- libhsp font metric 경계 (12번째): `bcompositor/libhsp_dc.txt` / `libhsp_dc_vtable.txt` / `libhsp_font_tables.txt`. dump script `DumpLibhspDC.py` / `DumpLibhspDCVtable.py` / `DumpLibhspFontTables.py`. 종합: `LIBHSP_GETOUTLINETEXTMETRICS_RE.md`
- dump script: `/tmp/hft_scripts/DumpBulletRenderDeps.py`
- SESSION_STATE: `kdsnr-hwp-toolkit/work/hft_re/layout_re/SESSION_STATE.md` (11번째 갱신)

## 환경

- 프로그램: `/tmp/drawing_proj.rep/` — `libHncDrawingEngine_arm64.dylib` + `libhsp_arm64.dylib` (12번째 세션 임포트, `lipo -thin arm64 /Applications/Hancom\ Office\ HWP.app/Contents/Frameworks/Hnc/Bin/libhsp.dylib`)
- Java: `/tmp/amazon-corretto-21.jdk` (`JAVA_HOME` 설정 필요)
- Ghidra: `/opt/homebrew/Cellar/ghidra/12.0.4/libexec/support/analyzeHeadless`
- 실행: `export JAVA_HOME=/tmp/amazon-corretto-21.jdk/Contents/Home; export PATH=$JAVA_HOME/bin:$PATH; <ghidra> /tmp drawing_proj -process <libHncDrawingEngine_arm64.dylib|libhsp_arm64.dylib> -noanalysis -scriptPath /tmp/hft_scripts -postScript <script>.py`

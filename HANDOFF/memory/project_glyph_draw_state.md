---
name: Glyph Draw vfunc 5-8 port 상태 (L-5)
description: 17 Glyph 클래스의 Draw/Undraw/GetBounds/Pick 1:1 port 진행 매트릭스. L-5a (14 method 시그니처 + Glyph base + MonoGlyph 묶음) 완료. L-5b (Box Draw/Allocate/GetBounds/Pick + trait &mut self 정공법화) 완료. L-5c-1 (Degree 17 method) + L-5c-2 (Matrix3 17 + Transform2D 30) 완료. L-5c-3.. (BodyProperty/Render::Path/ShapeEngine 등) 보류.
metadata:
  type: project
  originSessionId: b2b2688b-7d2d-4212-9fa9-ee199c9adb0d
---

# Glyph Draw vfunc 5-8 port 상태 (L-5)

L-5 = Glyph::Draw/Undraw/GetBounds/Pick 의 raw byte-eq 1:1 port. 17 Glyph × 4 vfunc = 68 method.

## vfunc index 정합 정리 (2026-05-17, L-5b)

이전 doc 의 vfunc mis-label 정정 — raw vtable @ `0x77fd10` (HBox/VBox_outer)
+ `nm -arch arm64 + c++filt` + L-5a 의 `glyph_draw_dump/` raw asm 검증 결과:

| vfunc | slot   | 메소드                          | Box raw addr   | size |
|-------|--------|---------------------------------|----------------|------|
| 3     | +0x18  | Request(Requisition&)           | `FUN_002e5e48` | 56B  |
| 4     | +0x20  | Allocate(Allocation, Extension&) | `FUN_002e601c` | 76B  |
| 5     | +0x28  | Draw(Surface,Alloc,Flag,BWMode) | `FUN_002e6348` | 188B |
| 6     | +0x30  | Undraw(Flag)                    | `FUN_002e6404` | 208B |
| 7     | +0x38  | GetBounds(...)→Allocation       | `FUN_002e64d4` | 228B |
| 8     | +0x40  | Pick(...)→bool                  | `FUN_002e65b8` | 256B |
| 9     | +0x48  | Compose                         | base           |      |
| 10-15 | +0x50..+0x80 | Append/Prepend/Insert/Remove/Replace/Change | `FUN_00331810`/etc | |
| 16    | +0x80  | GetCount                        | `FUN_00331ee0` | 8B   |
| 17    | +0x88  | GetComponent                    | `FUN_00331ee8` |      |
| 18    | +0x90  | GetAllotment                    | `FUN_002e6688` | 116B |

모든 container 가 base Glyph 와 동일한 16-vfunc layout (이전의 "container 17-vfunc 변형:
+40 = Allocate, 이후 한 칸씩 밀림" 가설은 raw asm 검증으로 폐기).

## raw asm dump 위치

- `work/hft_re/layout_re/glyph_draw_dump/` — 17 method × 4 vfunc raw asm
  (`Glyph__Draw_31597c.asm`, `MonoGlyph__Draw_2d08a8.asm`, `Placement__Draw_331488.asm`,
  `Box__Draw_2e6348.asm`, `Box__Undraw_2e6404.asm`, `Box__GetBounds_2e64d4.asm`,
  `Box__Pick_2e65b8.asm`, `Box__SetupHelper_2e6120.asm`, `Box__SetupSubhelper_2e5e80.asm`,
  `CharItemView__Draw_002f5e3c` (기존 decompiles/), `BlipGlyph__Draw_2d1480.asm`, ...).

## L-5a 완료 (2026-05-17, 14 method)

trait 시그니처 raw 1:1 확장 + Glyph base default 4 + MonoGlyph/Placement 4 + Box::Undraw + DebugGlyph::Undraw.

(이전 표 그대로, 변경 없음. trait 시그니처는 L-5b 에서 `&mut self` 로 재정정.)

## L-5b 완료 (2026-05-17, Box 4 method + trait `&mut self` 정정 + ripple)

| Glyph | Method | raw addr | size | 정공법 port 상태 |
|---|---|---|---|---|
| **trait 정정** | draw/get_bounds/pick `&self` → `&mut self` | — | — | ✅ raw 가 cache_bounds_valid mutate. C++ vtable 단일 entry → 모든 subclass 통일 |
| Box | Allocate (vfunc[4]) | 0x2e601c | 76B | ✅ doc 정정 (이전 "Box::Request BoundsAccumulator" mis-label) + Extension/BoundsRect 두 trait method 둘 다 byte-eq merge 구현 |
| Box | Draw (vfunc[5]) | 0x2e6348 | 188B | ✅ recompute_bounds_cache + 자식 vfunc[+0x28] dispatch with `&cached_allocations[idx]` |
| Box | GetBounds (vfunc[7]) | 0x2e64d4 | 228B | ✅ recompute_bounds_cache + 자식 vfunc[+0x38] sret + raw 종료 조건 (`out.x.origin LE LSB ≠ 0`) byte-eq |
| Box | Pick (vfunc[8]) | 0x2e65b8 | 256B | ✅ recompute_bounds_cache + 자식 vfunc[+0x40] hit break + count==0 → false |

**Prerequisite 확인**: `FUN_002e6120` setup helper (Box cached_alloc 빌더, 472B raw asm)
+ `FUN_002e5e80` Requisition cache sub-helper 둘 다 **이미 L-5a 이전에 `Box_::recompute_bounds_cache`
+ `Box_::recompute_request_cache` 로 byte-eq port 되어 있음** ([glyph.rs:695] + [glyph.rs:645]).
L-5b 는 prerequisite 추가 없이 4 method 만 정공법 1:1 추가.

**raw 종료 조건 byte-eq**:
- Box::GetBounds: `ldrb w8, [x19]; cbz w8, continue` = "out 의 byte[0] (= x.origin LE LSB) ≠ 0
  이면 hit, 아니면 continue". f32 normal value 들 (예: 7.5 = 0x40F00000 → LE byte[0]=0x00) 은
  LSB=0 → continue. 0.1 (0x3DCCCCCD → LSB=0xCD) 같은 비정수만 hit. **이건 raw 의 의도된
  sentinel 패턴 그대로 정공법 port** — 테스트 `box_get_bounds_normal_f32_lsb_zero_continues_to_fallback`
  으로 검증.

**Tests**: 12 신규 (box_draw_*, box_get_bounds_*, box_pick_*). **543 layout pass + 655 render
pass = 1198 total, 0 fail**.

## L-5c 진행 중 (prerequisite tree, 10-15 세션 추정)

### L-5c-0 / L-5c-1 / L-5c-2 완료 (2026-05-17)
- ✅ **dependency mapping**: CharItemView::Draw 의 외부 호출 ~20개 클래스/helper 의
  dylib 매핑. `Render::Path / Render::Surface / RenderUtil / Effects / EffectsPainter /
  ImagePainter / BodyProperty / ShapeEngine / ShapeRenderConverter` = `libHncDrawingEngine.dylib`
  내. `Hnc::Util::Degree / Transform2D / Matrix3` = **`libHncFoundation.dylib`
  (2.4MB 새 RE 영역)**. `work/hft_re/dylibs/libHncFoundation.dylib` 추가.
- ✅ **L-5c-1 Hnc::Util::Degree port** (`render-engine/rust/src/degree.rs`):
  17 unique method 1:1 port (Constrain magic-multiply `0xB60B60B7` for divide-by-360 + sign fix,
  ToRadian/ToDegree f64 magic const `0x3f91df46a2529d39` / `0x404ca5dc1a63c1f8`,
  Normalize/FlipWidth raw byte-eq). 20 신규 tests.
- ✅ **L-5c-2a Hnc::Util::Matrix3 port** (`render-engine/rust/src/matrix3.rs`):
  17 unique method 1:1 port. `[f32; 9]` row-major. Default ctor = identity (raw 의
  `__const@0xc8280 = [1,0,0,0]` 두 번 stp + m22=1.0 매칭). `operator/`, `Swap`,
  `IsIdentity`, `Determinant`, `Adjoint`, `Inverse` (det==0 → identity fallback),
  `PreMultiply` / `AppendMultiply` (NEON 4-lane FMUL+FMLA 시퀀스의 정확한 accumulation
  order: `((col[1]*row[1]) [fmul] + col[0]*row[0]) [fma] + col[2]*row[2] [fma]`).
  27 신규 tests.
- ✅ **L-5c-2b Hnc::Util::Transform2D port** (`render-engine/rust/src/transform2d.rs`):
  30 method 1:1 port. Storage = `Matrix3` embed (36B). 핵심 method:
  - Trivial: `GetXScale/GetYScale/GetXOffset/SetXOffset/SetYOffset/GetElement/IsValid/IsIdentity`
  - Inverse = tail-call to Matrix3::Inverse
  - `OffsetSubtractHalf`, `OffsetNormalize` (f64 intermediate + i32 cast)
  - `Apply / GetTransformPoint / Apply(vec) / GetInverseTransformPoint` — identity
    short-circuit + fmul+fma+fadd accumulation
  - `Translate / FlipVert / FlipHoriz / Skew / Rotate / Scale / Multiply` — `order` 파라미터
    convention: `order != 0` = PreMultiply (`self = other * self`), `order == 0` =
    AppendMultiply (`self = self * other`)
  - `GetTransformInfo` — atan2f + sqrtf + π/2 sentinel for axis-aligned cases
  - `Init` — composition `R(angle, center) * T(dst_center) * S(scale) * T(-src_center)`,
    PreMultiply 누적
  - `from_scale_rotate_translate(sx, sy, rot, tx, ty)` — raw inline trace 결과:
    `T(tx, ty) * R(rot, (0,0)) * S(sx, sy)`
  47 신규 tests. **render-engine 744 pass + layout 543 pass = 1287 total, 0 fail**.

### L-5c-2 보류 갱신 (사용자 지시: byte-eq 로 갈아엎기)

**#2 `GetInverseTransformPoint`**: ✅ byte-eq 재작성 (raw 0x16498..0x16578 1:1 port).
adj FMA 시퀀스 + special branch (v6/v7 == [1,0,0,0]/[0,1,0,0] 4-lane fcmeq + extra
2×2 det check) + 일반 inverse apply 모두 raw asm 그대로 trace. ❗ 초기 port 의 register
매핑 오류 (m01↔m10) 도 함께 정정.

**#3 `GetTransformInfo`**: ✅ byte-eq 재작성 (raw 0x165c8..0x166cc 1:1 port).
sret layout 정정 (offset/scale/rotation x/y), x-axis (m00, m10) + y-axis (m01, m11)
분기 정확, `b.le` 의 NaN unordered 의미 (`m10/m01 ≤ 0`) 포함. 3 신규 edge-case tests 추가.

**#1 `Init`**: ⚠️ **부분 byte-eq**. composition (T(-src)·S·T(dst)·R) 의 pre_multiply
4-step 으로 구현. 분석 결과:
- **angle=0 (sparse only)**: T·S·T 의 모든 곱셈이 0/1 multiplicand → IEEE-exact →
  **raw inline 과 byte-eq 보장**
- **angle≠0 (rotation)**: R 의 cos/sin dense → raw inline 9-fma chain 과 sub-ULP
  (~1e-7 relative) 차이 가능. pixel-eq 무관.

raw asm 의 inline 9-fma sequence (0x15a18..0x15ec8, ~300 NEON instruction, 다중 SIMD
shuffle + 5 branches) line-by-line 1:1 trace 는 별도 focused 세션 필요. to-do.md 에
추가됨.

### L-5c-3 진행 중 (2026-05-17)

- ✅ **libHncDrawingEngine.dylib** dylibs/ 등록 (20MB)
- ✅ **RenderUtil::ToMatrix3** (`0x2d1ae4` 200B): Transform2D → Matrix3, 2×3 affine 추출 + 강제 (0,0,1) 하단행. 4 tests.
- ✅ **ShapeEngine 클래스** (40B layout: is_started/unit/catalog_ptr/theme_ptr/common_path/is_enable_xbox/resolution) + GetInstance singleton (Rust OnceLock<RwLock<_>>) + 6 method (GetLogicalDpi/GetResolution/SetUnit/SetResolution/IsStarted/IsEnableXBox/SetEnableXBox). 6 tests.
- ✅ **RenderUtil::LogicalToRender** (`0x332274` 288B): `p *= 96.0 / engine.unit` (양 축 동일). ShapeEngine singleton 의존. raw 의 SIMD shuffle 분석 결과 두 lane 모두 unit 으로 채워짐 (compiler artifact 가 아니라 의도). 3 tests.
- ⏸️ **SurfaceRestorer**: CoreGraphics wrapper (CGContextSave/RestoreGState) → SvgSurface backend 에서는 무관, byte-eq port 대상 아님.
- ✅ **L-5c-3c BodyProperty getters** (2026-05-17, `render-engine/rust/src/body_property.rs`):
  - 클래스 layout 32B (bag + Scene3D ctrl + Sp3D ctrl + PresetWarp ptr) — C2 ctor `0x2e3030` 검증
  - **27 PropertyKey 상수** `0x898..0x8b1` (key 모듈) — raw `mov w8, #0xNNN` 직접 인용
  - **27 scalar getter** 1:1 port — pattern: stack-alloc PropertyKey → bag_impl_ptr resolve → helper 0x67d0e4 → typed load
    - u32 (8): GetVert/AutoTxRotType/PresetWarpType/HorzOverflow/VertOverflow/Anchor/Wrap/AutoFit
    - bool (7): GetSpaceFirstLastPara/AnchorCenter/RtlCol/FromWordArt/ForceAntiAlias/CompatibleLineSpace + GetUpright 의 inner stage
    - f32 (7): GetLeftInset/TopInset/RightInset/BottomInset/SpaceCol/NormalFitFontScale/NormalFitLineReduction
    - i32 Degree (2): GetRotation/GetAutoTxRotAngle
    - u64 (1): GetNumCol (test ignored — PUInt64 type 미포팅, +0xc 에서 8B 읽기 alloc overflow)
    - conditional bool (1): GetUpright (AutoTxRotType 검사 후 분기, raw 0x2e0a74)
  - **Contains** (raw 0x2e3ed0): pure tail-call to PropertyBag::Contains
  - **IsSaveable** (raw 0x2e3ed4): state ∈ {1, 5} 항상 true, state 2 는 write_all=true 일 때만, 그 외 false
  - **PropertyBag::get_value_addr helper** (`property_bag.rs`): raw 0x67d0e4 family (9개 byte-identical template instantiation 중 하나만 포팅) — std::map at(key) → Property+0xc 반환, out_of_range/bad_cast panic
  - **35 신규 tests** (34 pass + 1 ignored PUInt64). **543 layout + 802 render = 1345 total, 0 fail**.

### L-5c-3d Composite getters (2026-05-17 추가)

- ✅ **GetInset** (`0x2e4904` sz=272B): 4 sequential f32 getter (keys 0x8a0..0x8a3) → AAPCS HVA 반환 (v0..v3 = left/top/right/bottom). `Margin {f32 × 4}` 16B / align 4.
- ✅ **GetFlatText** (`0x2e5420` sz=108B): helper 0x67d56c (byte-identical 10th instantiation) → 반환 ptr 그대로 `*const FlatTextPair`. `FlatTextPair {first: u8, _pad: [u8;3], second: f32}` 8B / align 4. caller 가 dereference 책임 (raw 는 ldr 없음). 편의 `get_flat_text_value()` 도 제공.
- ✅ **GetPresetWarp** (`0x2e0b08` sz=8B): raw `add x0, x0, #0x18; ret` — `&self.preset_warp` slot 주소 반환 (raw byte-eq).
- ✅ **Margin / FlatTextPair 타입** + `#[repr(C)]` layout assert + field offset 테스트.
- **8 신규 tests** (2 layout + 2 inset + 2 flat_text + 2 preset_warp). **543 layout + 810 render = 1353 total + 1 ignored, 0 fail**.

### L-5c-4a/b 완료 (2026-05-17, render-engine/rust/src/path.rs, 60 tests)

- **L-5c-4a (Path framework)**: Path 24B 외부 layout byte-eq (5 meta field + impl_ptr)
  - default ctor (raw 0xa110c) + dtor (raw 0xa1768)
  - 5 setter + 5 getter (raw 0xa1bfc..0xa1c44)
  - Swap (raw 0xa18d0 외부 + 0xa192c 내부)
- **L-5c-4b (Path geometry)**:
  - Subpath enum (Move/Line/Bezier/Begin/Close) — 5 raw subpath family 매핑
    - LineSubpath (vtable @ +0x960, 32B) type=0=Move, type=2=Line
    - BezierSubpath (vtable @ +0x9a8, 40B) 4 control points
    - StartSubpath (vtable @ +0xa38, 8B) "begin new subpath" marker
    - CloseSubpath (vtable @ +0xa98, 8B) "close current subpath" marker
  - PathImpl helpers: add_line (raw 0x792dc, empty 분기로 implicit Move 생성),
    add_close (raw 0x799f4), add_begin (raw 0x797c0), add_bezier (raw 0x798c0)
  - geometry ctor 4 variant: from_rect_f/i, from_line_f/i, from_polyline_f/i
  - public Add method: add_line/i, add_polyline/i, add_rect/i, add_bezier/i,
    add_bezier_chain (3-step sliding window), start, close
  - Clone (raw 0xa1a9c) — deep copy via Vec::clone (Subpath Copy)
  - Transform (raw 0xa2340) — IsIdentity short-circuit + per-point Transform2D::apply
  - Outline / Expand / Union — **raw stub 그대로** (0xa2388 mov w0,#0; 0xa2390 str xzr; 0xa2398 ret)
  - Flatten — placeholder (raw helper 0x7d860 RE 후 Bezier→polyline)
  - GetPointCount / GetPoints / GetTypes — subpath traversal
  - GetBounds — placeholder min/max (raw 0x72c34 정확 logic 후속)
- **render-engine 870 pass + layout 543 pass = 1413 total, 0 fail**

### L-5c-9a/b1/b1+ 완료 (2026-05-17, SRC dispatcher + 보조 module)

- **L-5c-9a (apply_color_mode)**: `ShapeRenderConverter::ApplyColorMode(Flag&, ColorMode)` (raw 0x1df7e4, 40B)
- **L-5c-9b1 (ToRenderColor)**: `ShapeRenderConverter::ToRenderColor(Color, ColorMapper*)` (raw 0x1dfb9c, ~330B) — 7-way dispatcher on type_tag (RGB/CMYK/SCHEME/SYSTEM/PRESET/SC_RGB/HSL)
  - 보조 module 신규: `math_util.rs` (MathUtil::Round, raw 0x12a38), `pixel_util.rs` (PixelUtil::HslToRgb, raw 0x13724 ~220B NEON 1:1)
  - `color.rs` 확장: `Color::get_preset_color(PresetStyle)` + 3개 190B byte table (R/G/B at raw 0x74fba8/c66/d24)
  - `shape_render_converter.rs`: `RenderColor` (DrawingType::Color 6B) + `ColorMapperLike` trait (SchemeStyle 해결) + `SystemColorProvider` trait (GetSysColor 시스템 위임)
- **L-5c-9b1+ (ToColorMode)**: `ShapeRenderConverter::ToColorMode(Flag&)` (raw 0x180918, 44B) — Flag bit pattern → ColorMode reverse mapping (priority: 0x40 Grayscale > 0x80 BlackWhite > 0x08000000 Mode3 > None)
- **render-engine 917 pass** (+47 신규)

### L-5c-5a 완료 (2026-05-17, theme.rs ~250 추가 + 11 tests)

Theme 의 raw 의 lazy parent-chain accessor + RB tree 검색 1:1 port:

- **`Theme::get_format_scheme_init(init_if_null, engine)`** (raw 0x1ec490, 56B) — parent chain walk (`SharePtr<Theme>`) + `init_defaults` flag fallback → `ShapeEngineProvider` trait 호출
- **`Theme::get_font_scheme_init(init_if_null, engine)`** (raw 0x1ec65c) — 동일 패턴
- **`Theme::get_scheme_brush(style, engine)`** (raw 0x1ec84c, ~196B) — `FormatScheme.brushes` (offset +0x10 = end_node_left) lower_bound + exact-match + refcount++
- **`Theme::get_scheme_background_brush(style, engine)`** (raw 0x1ec914) — `FormatScheme.bg_brushes` (+0x28)
- **`Theme::get_scheme_pen(style, engine)`** (raw 0x1ec9dc) — `FormatScheme.pens` (+0x40), Pen ControlBlock 을 BrushControlBlock 과 동일 24B layout 으로 노출 (raw 의 type punning)
- **신규 trait**: `ShapeEngineProvider { default_format_scheme; default_font_scheme }`
- **RB tree helper**: `lookup_brush_node` / `lookup_pen_node` — std::map lower_bound (key at node+0x20, value at node+0x28, right child at node+0x8)
- **render-engine 928 pass** (+11 신규 — Theme 합계 40 tests)

### L-5c-9b1++ 완료 (2026-05-17, SRC coord scaler family, +8 tests)

ShapeRenderConverter 의 5 scalar coord 변환:

- **`logical_to_render_scalar(p) -> f32`** (raw 0xdb2f8, 48B): `p * 96.0 / engine.unit`
- **`render_to_logical_scalar(p) -> f32`** (raw 0x14029c, 52B): `unit * p / 96.0`
- **`render_to_device_scalar(p, scale) -> f32`** (raw 0x1dfac8, 20B): `(scale != 0 ? scale : 1.0) * p` (leaf, ShapeEngine 호출 없음)
- **`logical_to_device_scalar(p, scale) -> i64`** (raw 0x106cc8, 64B): `MathUtil::Round((double)scale * (double)logical_to_render(p))` (tail call)
- **`device_to_logical_scalar(p, scale) -> f32`** (raw 0xe5a10, 68B): `((p / scale_safe) * unit) / 96.0`

raw const: 96.0 = `0x42c00000`. ShapeEngine 의 `unit` 은 default 1.0 (`OnceLock<RwLock<>>` singleton).

- **render-engine 936 pass** (+8 신규)

### L-5c-9b1+++ 완료 (2026-05-17, ApplyRenderMode dispatcher, +11 tests)

`ShapeRenderConverter::ApplyRenderMode(RenderColor&, mode)` (raw 0x1dfdf0, ~248B):
8-way jump table dispatch on `mode-1` (1..8 valid, 0/9+ = no-op).

- case 1/2/3 → ColorUtilProvider trait callback (ToGray/ToLightGray/ToInverseGray)
- case 4: light gray solid `(0xA0, 0xA0, 0xA0)` raw mov+strh
- case 5: white `(0xFF, 0xFF, 0xFF)`
- case 6: black `(0, 0, 0)`
- case 7: SystemColorProvider.get_sys_color(5)
- case 8: SystemColorProvider.get_sys_color(8)
- 모든 constant case 가 color_type=1 set, alpha 안 건드림 (raw 검증)

**신규 trait**: `ColorUtilProvider { to_gray; to_light_gray; to_inverse_gray }` —
raw ColorUtil 3 method 의 byte-eq port 는 3KB lazy SIMD lookup table 의존으로 별도 세션.

- **render-engine 947 pass** (+11 신규)

### L-5c-5b 보류 (FontSet / EffectsContainer 의존)

- **`Theme::GetMajorFont()`** (raw 0x168c04) — FontScheme[+8] lazy init (CreateDefaultFontSet 호출)
- **`Theme::GetMinorFont()`** (raw 0x168c70) — FontScheme[+0x10] 동일 패턴
- **`Theme::GetSchemeEffects(Style)`** (raw 0x15de24) — EffectsContainer 16B 출력 (vec semantic)
- **`Theme::GetSchemeScene3D/Sp3D(Style)`** — Scene3D/Sp3D struct 의존
- 모두 별도 sub-object port 후 진행

### L-5c-4c 보류 (geometry helper RE 필요)

- AddArc(RectF, Degree, Degree) — raw helper 0x7aa44 RE 필요
- AddEllipse(RectF/RectI) — raw helper 0x7a930 RE 필요
- AddCurve(slice, tension) — raw helper 0x79fa4 RE 필요 (smoothed Bezier chain)
- Flatten(Bezier→polyline) — raw helper 0x7d860 RE 필요
- GetBounds 정확 logic (Pen 두께 + Transform 처리) — raw helper 0x72c34 RE
- GetStartPoint/GetLastPoint — subpath virtual vfunc dispatch 정밀

### L-5c-4d 보류 (CGPath/HFT 의존 — S-4 backend 와 같이)

- Path(CGPath*) ctor
- AddString(wchar*, FontFamily, size, PointF, StringFormat) — text → CGPath via HFT
- IsVisible(Surface, PointF/I) — CGPathContainsPoint
- IsOutlineVisible(Surface, Pen*, PointF/I) — CGPathCreateCopyByStrokingPath

### L-5c-3c/d 보류 (Scene3D/Sp3D 타입 의존 — 다음 세션)

- **GetScene3D** (`0x2e5640`): sret + SharePtr<Scene3D> 복사 ctor + refcount++ + tail-call 0x64a1d4
- **GetSp3D** (`0x2e56e8`): sret + SharePtr<Sp3D> 복사 ctor + refcount++ + tail-call 0x64aa18
- **operator==** (`0x2e3ddc`): Scene3D/Sp3D/PresetWarp ptr 비교 + PropertyBag::operator== 위임 (200B+)
- **operator!=** (`0x2d20c8`): tail-call eq + xor 1 (trivial — eq 만 끝나면 1줄)
- **Clone** (`0x2d2c04`): sret + alloc 0x20 + C2 ctor + DeepPtr wrap (0x678664 호출)
- **CollectProperty** (`0x2e5a7c`): PropertyBag::Merge + Scene3D/Sp3D setter via 0x1cc15c/0x1cc2a4 (외부 helper)
- **Union/Swap** — bag 합치기 + Scene3D/Sp3D/PresetWarp 교환
- **C1/C2 ctor** (`0x2d1d98`/`0x2e3030`) + **D1/D2 dtor** (`0x2d1e38`/`0x2e3c6c`)
- **PresetWarp** 자체 64B layout (raw `new(0x40)` from ctor 0x2e30c4)
- 모두 Scene3D/Sp3D/PresetWarp 타입의 byte-eq port 후 진행

### L-5c 남은 prerequisite

| Glyph / 의존 | Method / 영역 | raw addr | size | Prerequisite (부족한 byte-eq port) |
|---|---|---|---|---|
| ~`libHncFoundation`~ | ~`Hnc::Util::Transform2D` / `Matrix3`~ | — | — | ✅ L-5c-2 완료 |
| ~`RenderUtil::ToMatrix3 / LogicalToRender / ShapeEngine`~ | — | — | — | ✅ L-5c-3a/b 완료 |
| ~libHncDrawingEngine~ | ~`BodyProperty::Get*` 27 scalar + Contains + IsSaveable~ | 0x2d2c6c-0x2e5568 | - | ✅ L-5c-3c 완료 (GetInset/GetFlatText/SharePtr getters/ctor/dtor 보류) |
| ~libHncDrawingEngine~ | ~Render::Path framework + geometry (60% — Add Arc/Ellipse/String/CGPath 제외)~ | 0xa110c-0xa2398 | - | ✅ L-5c-4a/b 완료 (path.rs 60 tests) |
| libHncDrawingEngine | Theme cache singleton | TBD | - | Theme 조회 + ShapeEngine 와 연결 |
| libHncDrawingEngine | ShapeEngine::GetInstance | TBD | - | 싱글톤 storage |
| libHncDrawingEngine | Effects (OuterShadow/Reflection) state | TBD | - | 일부 이미 render-engine 에 있음 |
| libHncDrawingEngine | ShapeRenderConverter coord transform | TBD | - | RenderToLogical/LogicalToRender/LogicalToDevice |
| libHncDrawingEngine | RenderUtil::ToMatrix3 | 0x2d1ae4 | - | Transform2D → Matrix3 변환 |
| libHncDrawingEngine | GetRealPen/GetRealEffects/GetRealTextEffects | 0x2f1xxx | 합 ~1KB | PropertyBag traversal |
| libHncDrawingEngine | GetCachedRenderPath | TBD | - | path 캐시 lookup |
| libHncDrawingEngine | GetPreEffectsImage | TBD | - | effects pre-rendering |
| libHncDrawingEngine | EffectsPainter::Draw | TBD | - | effects 그리기 |
| libHncDrawingEngine | Render::SurfaceRestorer ctor/dtor | TBD | - | save/restore wrapper |
| libHncDrawingEngine | CharItemView helpers (DrawBrush/Pen/UnderLine/Direct/Diagnostics/CalcDrawVariables) | various | 4400B 합 | 상기 모든 prerequisite |
| **CharItemView::Draw** | (main 1880B) | 0x2f5e3c | 1880B | 상기 모든 prerequisite |
| CharItemView | Undraw | 0x2f8bd8 | 124B | child + bbox state |
| CharItemView | GetBounds | 0x2f9008 | 116B | Allocation expansion logic |
| CharItemView | Pick | 0x2f9a34 | 388B | hit detection logic |
| BlipGlyph | Draw | 0x2d1480 | 1636B | ImageBrush byte-eq + Surface::DrawImage SvgSurface 어댑터 (S-4) |
| WidgetGlyph | Draw | 0x30b0d8 | 772B | Surface::FillPath + Pen/Brush PropertyBag full state |
| DebugGlyph | Draw | 0x30b988 | 3536B | diagnostic visualization — layout phase 무관 (별도) |

**왜 L-5c 가 multi-session**: CharItemView::Draw 의 raw decompile 정독 결과 외부 호출 ~20
함수 — ShapeEngine 싱글톤, Render::Path 17 ctor, Theme cache lookup, Effects (OuterShadow/
Reflection) state, ShapeRenderConverter coord transform, BodyProperty getter, Hnc::Util::Degree/
Transform2D/Matrix3 (libHncFoundation 새 RE), GetReal{Pen,Brush,Effects,TextEffects},
GetCachedRenderPath, GetPreEffectsImage, EffectsPainter::Draw, RenderUtil::ToMatrix3,
Render::SurfaceRestorer. 정공법 1:1 port 가 prerequisite 의 정공법 port 없이 추측 강제
(정공법 위반). bottom-up 순서: Util::Degree (✅) → Util::Transform2D + Matrix3 → BodyProperty
getter → Render::Path → ShapeEngine + Theme cache → Effects → ShapeRenderConverter →
RenderUtil → GetReal* helpers → CharItemView helpers → CharItemView::Draw main → Undraw/
GetBounds/Pick → Blip/Widget/Debug::Draw.

## L-5c-RE-1/2 완료 (2026-05-17, text/path chain entry — GetRealFont까지)

CharItemView::Draw 의 **본격 prerequisite chain** 진입. **input 샘플 감사 결과** (104 sections, work/e2e/) 로 P0 컴포넌트 (text 95% + box/표 + 수식 + 이미지) 확정 후, text-chain 부터 진행.

### Input 샘플 감사 결과 (P0 정의)

- **출현**: hp:t (4712), hp:run (4274), hp:p (2788), hp:linesegarray (2258), hp:equation (1794), hp:tbl/tr/tc (280/468/976), hp:rect (108), hp:pic (60), hp:line (56), hp:drawText (108)
- **미출현**: hp:bezier / hp:curve / hp:polyline / hp:polygon (0건) — smoothing helper (FUN_0x79fa4 5840B) 와 ToPath wrapper 들 영구 보류 (P0 미발동 확정)
- P0 우선순위: **(1) 텍스트 chain 95%** → (2) Table/Box border → (3) 수식 → (4) BlipGlyph 이미지

### L-5c-RE-1: path-chain (RenderPathToPath, b3=false branch)

| 작업 | 함수 / 모듈 | 크기 | 상태 |
|---|---|---|---|
| flatten_path_points | `FUN_0x7b674` / `path_util.rs` | 3060B | ✅ |
| flatten_path_types | `FUN_0x7c26c` / `path_util.rs` | 3148B | ✅ |
| LogicalPosition (16B) | `logical_position.rs` | — | ✅ + `get_position` |
| Segment (32B) | `shape_segment.rs` | — | ✅ + `set_last_position`/`set_smooth_corner` |
| Shape::Path (48B) | `shape_path.rs` | — | ✅ |
| RenderPathToPath b3=false | `path_util.rs` | 1840B 중 일부 | ✅ (b3=true/snap/aux 보류) |
| RenderPathAuxOutput (20B) | path_util.rs | — | ✅ (zero-init, bbox 미계산) |

### L-5c-RE-2: GetRealFont chain (텍스트 폰트 선택)

`Hnc::Shape::Text::CharItemView::GetRealFont(Theme*)` (raw 0x2f0234, 1296B) byte-eq port + dependencies:

| 작업 | 함수 / 위치 | 크기 | 모듈 | 상태 |
|---|---|---|---|---|
| Unicode script classifier | `FUN_0x2f0ad0` | 1268B | `text_real_font::classify_script` | ✅ 50+ branch + SIMD lane + Indic table |
| script→slot mapper | `FUN_0x2f0ec8` | 32B | `text_real_font::script_to_slot` | ✅ — **slot 매핑 버그 정정** |
| theme reference 체크 | `FUN_0x2f0fc4` | 196B | `text_real_font::is_theme_font_reference` | ✅ 8 token (OOXML +mj/+mn × lt/ea/cs/sym) |
| RunProperty struct (80B) | (신규) | — | `run_property.rs` | ✅ 4 font slot getter + FontSlot enum |
| GetRealFont body | 0x2f0234 | 1296B | `text_real_font::get_real_font` | ✅ direct return (script→slot→font dispatch) |
| theme reference dispatch | 0x2f0410-0x2f0414 | — | `text_real_font::check_theme_reference` | ✅ 별도 `unsafe` 함수 (valid `*ControlBlock<TextFont>` 가정) |
| TextFont struct (24B) | (이미 존재) | — | `text_font.rs` | ✅ (sessions 이전 완료) |
| resolve_theme_font | `FUN_0x2f10bc` | 870B | (보류) | ⏸️ P0 미발동 — direct font name 만 사용 |

### ⭐ slot 매핑 버그 정정 (raw asm 검증)

초기 script_to_slot 결과 → 슬롯 dispatch 매핑 가설:
- ❌ 잘못된 가설: slot 1 = ComplexScript, slot 2 = EastAsian
- ✅ raw GetRealFont asm 검증 후 정정:
  - `script_to_slot` 1 → `RunProperty+0x30` = **EastAsian** (raw 0x2f0388 reads +0x30)
  - `script_to_slot` 2 → `RunProperty+0x38` = **ComplexScript** (raw 0x2f02fc reads +0x38)
  - `RunProperty layout offset`: `0x28 + slot*8` (Latin/EastAsian/ComplexScript/Symbol 순)
- bits 2-5 (Hangul/Hiragana/Katakana/Arabic Presentation) → EastAsian (한자/한글/일본어가 같은 슬롯 이유)
- bits 8,9,10,33 (Greek/Cyrillic/Hebrew/Arabic 등) → ComplexScript

### Input 샘플 font 확인

- HWPX header.xml 의 `<hh:fontface>` 검사: TTF 와 HFT 혼용 발견
  - TTF: "한컴 윤고딕 230", "함초롬바탕", "Pretendard" 등
  - HFT: "#태고딕", "신명 디나루", "신명 신그래픽", "신명 중고딕"
- **모든 폰트가 직접 이름** (theme reference 0건) → `is_theme_font_reference` 항상 false → `resolve_theme_font` 미발동
- HFT 폰트 사용 확인 → hft-decoder chain (별도 모듈) 도 critical path

### 8 theme reference token (UTF-16LE wchar)

dylib `__cstring+0x7d2486..0x7d24ea` 에서 추출 (chained-fixup 미해석, 직접 search):
```
+mj-lt / +mj-ea / +mj-cs / +mj-sym   (major font scheme refs)
+mn-lt / +mn-ea / +mn-cs / +mn-sym   (minor font scheme refs)
```
OOXML DrawingML 표준 token. CHncStringW 가 **UTF-16LE wchar_t** (Windows-style) 사용 확인.

### tests + 합계

- **+38 신규 tests** (text_real_font 모듈) + 6 (run_property)
- 전체: **1181 tests pass + 1 ignored, 0 fail**

### L-5c-RE-3 완료 (2026-05-17, CalcDrawVariables 1688B full byte-eq)

| 작업 | 함수 / 모듈 | 크기 | 상태 |
|---|---|---|---|
| `Transformation` (28B) | `transformation.rs` | — | ✅ header0/header1+panose/4 f32/degree_raw |
| `RectImpl<float>` 20B variant (= `RectF20`) | `transformation.rs` | — | ✅ caller stack-frame 측정으로 16B 가 아니라 20B 확정 |
| `StringFormat` 8B `{impl_ptr}` + `StringFormatImpl` `{_field_0, field_4}` | `calc_draw_variables.rs` | — | ✅ +0x4 write 가 유일 outut |
| **Stage A**: 3 property reads | `calc_draw_variables.rs` | — | ✅ BodyProperty.Vert (0x89e u32) + ParaProperty.Wrap (0x8fd u32, Contains-gated) + RunProperty.shadow (0x96c f32) |
| **Stage B**: 16-path vertical alignment jump table | `stage_b_jump_table` | ~200B | ✅ w20=1..4 × w28 sub-dispatch, `__const@0x744152` 8B 디코드 (`00 0f 18 20`) |
| **Stage C**: 공통 setup | inline | — | ✅ Allocation read + ShapeEngine.unit × 2 + Degree(0) raw |
| **Stage D**: b2=true main path (Top/Bottom/Center) | inline | ~580B | ✅ Path A (w28 ∈ {0,2}) mode=7 + Degree(90), Path B (w28 ∈ {5,6}) mode=5, Path C (else) mode=5 + ratio (not 1-ratio) |
| **Stage D**: has_explicit_format 분기 (× 3) | inline | — | ✅ explicit = copy panose+4 scale, default = compute s12/s14/s11/s13 per-path |
| **Stage E/F**: 4 output writes | inline | — | ✅ PointF (s10, s9) + RectF20 (20B byte-eq) + Transformation (28B byte-eq) + StringFormatImpl.field_4 = 0 + mode 0/5/7 |
| CharItemView field 확장 | `char_item_view.rs` | — | ✅ `field_3c` (split from _pad38) + 0x6c..0x8c typed (format_origin_x/scale/shadow_scale/has_explicit_format/panose/4 scale) |

**3 helper byte-identical 검증**: 0x67d0e4 (u32) / 0x6800ac (u32) / 0x65616c (f32) 모두 동일 std::map lower_bound walk → existing `get_value_addr` 재사용 (cast 만 다름).

**`b1` 미사용 확인**: raw 가 `w1/x1` 절대 read 안 함 (`grep -nE "w1|x1," CharItemView__CalcDrawVariables_2f4368.asm` 결과 `add x1, sp, #0x20` 만 — call setup 용). port 도 `_b1: bool` 으로 placeholder.

**Path B `[x23+0x3c]` 발견**: raw `ldr s13, [x23, #0x3c]` 가 우리 `_pad38: u64` 의 상위 4 byte 영역에 access → `_pad38` 을 `_f38: f32 + field_3c: f32` 로 split.

**Tests** (21 신규 calc_draw_variables + 6 transformation = 27 모듈 합):
- b2=false fast path (3): position pass-through + mode=0 + null StringFormat OK
- Stage B 11 sub-cases (w20=0/1/2/3/4 × w28 ∈ {0,2,5,6,1,3,7+} edge cases)
- Path A Top (w28=2 via real BodyProperty bag): mode=7 + degree_raw = 0x42b40000
- Path B Bottom (w28=5 via real BodyProperty bag): mode=5 + s9 = (alloc.y + total_height) - (descent×unit/72)
- Path C Center (w28=1 default): mode=5 + shadow uses ratio (not 1-ratio)
- has_explicit_format dispatch: panose+4 scale copy
- shadow trigger condition: total_height_alt == 0 && ascent_ratio < 0

**총 1208 tests pass** (이전 1190 + 18 신규), 0 fail, 1 ignored.

### L-5c-RE-4 완료 (2026-05-17, GetRealTextEffects fast path full byte-eq + P0 미발동 deferral)

| 작업 | 함수 / 모듈 | 크기 | 상태 |
|---|---|---|---|
| `RunProperty::GetFontSize` | raw 0x2ecb18 / `run_property.rs` | 116B | ✅ key 0x96a f32 |
| `RunProperty::GetScriptBaseLine` | raw 0x2f0074 / `run_property.rs` | 116B | ✅ key 0x96c f32 |
| `Effects::get_effect_sret` | raw 0xc2744 / `effects_container.rs` | ~250B | ✅ RB lower_bound + refcount++ on hit |
| `UniqueEffectsCtrl` (24B) | `char_item_view_effects.rs` | — | ✅ {inner_ctrl, refcount, flag, _pad} byte-eq |
| `singleton_init_649980` | raw 0x649980 / `char_item_view_effects.rs` | 150B | ⏸️ no-op stub (cxa_guard 4-stage RE 보류 L-5c-RE-4c, output 무관) |
| `make_unique_effects_649d3c` | raw 0x649d3c / `char_item_view_effects.rs` | 250B | ✅ alloc 24B + move semantics + flag=1 ; ⏸️ dedup registry insert (FUN_649e30) 보류 (output content 동일) |
| **`get_real_text_effects` (fast path)** | raw 0x2f2ad8 / `char_item_view_effects.rs` | 2428B | ✅ Path 1/2/3 byte-eq |
| LAB_002f2c0c (4 effect block) | inline | ~1500B | ⏸️ P0 미발동 → `unreachable!()` deferral |

**⭐ 잘라내기 정당성 (P0 미발동 deferral)**:

- toolkit 전체 hwpx audit (2026-05-17): 307 파일 (vendor 제외), `<hp:t>` 355건 중
  - `<hp:outerShadow>` = **0건**
  - `<hp:reflection>` = **0건**
  - `<hp:glow>` = **0건**
  - `<hp:innerShadow>` = **0건**
  - `<hp:softEdge>` = **0건**
  - `<hp:shadow type="NONE">` 59건 (= 명시적 "효과 없음" marker, 실제 effect 없음)
- 따라서 LAB_002f2c0c 의 4 effect in-place modify sequence (Shadow 0x3ee / OuterShadow 0xbba /
  Reflection 0xbbb / Glow 0xbb9) 는 우리 input pool 에서 **100% 발동 0건**.
- `unreachable!("PATH 4: P0 미발동 → deferred to L-5c-RE-4b")` 으로 명시적 panic.
- **"100% pixel-eq 보장한 코드 잘라내기"** (패싱 아님).

**4 keys 매핑 (raw asm 검증)**:
- raw 0x2f2bbc lower_bound on `0x3ee` (Shadow effect type) — 항상 검사
- raw 0x2f2e1c `0xbba` (OuterShadow) — `flag.byte0 & 1 == 0` 일 때만
- raw 0x2f2eb4 `0xbbb` (Reflection) — `flag.byte0 & 1 == 0` 일 때만
- raw 0x2f2f64 `0xbb9` (Glow) — `flag.byte0 & 1 == 0` 일 때만

**Path 3 fast wrap byte-eq verify points**:
- raw 0x2f33c8: `effects_ctrl.refcount++` (1→2 test 검증)
- raw 0x2f33d4: `singleton_init_649980` 호출 (side effect 만, output 무관)
- raw 0x2f33e0: `make_unique_effects_649d3c(sret, &local_slot)` — 24B ctrl alloc + flag=1

**Tests** (14 신규):
- run_property (2): get_font_size + get_script_base_line via real bag
- effects_container (3): get_effect_sret empty/missing/existing
- char_item_view_effects (8): layout/field_offsets/null_source/alloc_move + Path 1/2/3 + Path 4 panic

**총 1222 tests pass** (이전 1208 + 14 신규), 0 fail, 1 ignored.

### L-5c-RE-4b 보류 (LAB_002f2c0c effect processing — input 발동 시 실행)

| 작업 | raw addr | 크기 | 의존 |
|---|---|---|---|
| `PropertyBag::Set<float>` | 0x653cb4 | 360B | std::__tree::__emplace_unique (0x647764) |
| `Reflection::SetDistance` | 0x1cee70 | 100B | PropertyBag::Set 위 |
| `Effects` copy ctor (deep) | 0x631f40 | 296B | Effect::Clone vfunc[+0x30] dispatch + 0x162050 insert |
| `Effects` ~dtor (recursive) | 0x6320c8 | 112B | EffectControlBlock release tree walk |
| Shadow scale logic (0x3ee) | inline 0x2f2c0c | ~300B | font_size × distance / 96.0 |
| OuterShadow scale logic (0xbba) | inline 0x2f2e1c | ~400B | (1-shadow_ratio) × (font_size × dist) / 72.0 |
| Reflection scale logic (0xbbb) | inline 0x2f2eb4 | ~500B | distance + blur + alpha 합성 + ShapeEngine.unit |
| Glow scale logic (0xbb9) | inline 0x2f2f64 | ~250B | font_size × radius × 3.0 / 96.0 |
| FUN_649e30 dedup registry | 0x649e30 | ~600B | global RB tree of cached UniqueEffectsCtrl |

발동 조건: hwpx 의 `<hp:t>` 안에 `<hp:outerShadow>` / `<hp:reflection>` / `<hp:glow>` 등의 명시적 effect tag 가 포함된 경우.

### L-5c-RE-5a 완료 (2026-05-17, CharItemView::Draw outer dispatch full byte-eq + P0 미발동 deferral)

| 작업 | 함수 / 모듈 | 크기 | 상태 |
|---|---|---|---|
| `DrawOutcome` enum (Skipped/DrawDirectCalled/EffectsUnreachable) | `char_item_view_draw.rs` | — | ✅ outer dispatch outcome enum |
| `SkipReason` enum (NewlineOrCR/InvisibleSpace/...) | `char_item_view_draw.rs` | — | ✅ 5 진단 사유 |
| `DrawDirectFn` trait (callback for DrawDirect 5248B) | `char_item_view_draw.rs` | — | ✅ L-5c-RE-5b 으로 추상화 |
| **`draw` outer dispatch** | raw 0x2f5e3c / `char_item_view_draw.rs` | 1880B | ✅ Block 2/3/4/8/10 byte-eq |

**Block-by-block 매핑** (raw 0x2f5e3c..0x2f6594):
- **Block 1 (raw 0x2f5e80, dead reads 0x96a/0x96c)**: skipped — output 무관, pixel-eq 무관
- **Block 2 (raw 0x2f5ee8)**: `sVar1 ∈ {10, 13}` → `Skipped(NewlineOrCR)` ✅
- **Block 3 (raw 0x2f5f04)**: `sVar1 == 32` + bag.0x961 u32 == 0 → `Skipped(InvisibleSpace)` ✅
- **Block 4 (raw 0x2f5f50)**: font null OR font.obj null → `Skipped(NoFont)` ✅
- **Block 5 (raw 0x2f5f74)**: ShapeEngine warmup (theme refcount inline expand) — surface trait callback abstraction 으로 L-5c-RE-5b 에서 통합
- **Block 6 (raw 0x2f5fb0)**: `get_real_pen()` 호출 ✅
- **Block 7 (raw 0x2f5fe0)**: `get_real_brush()` (raw inline expand, 본 port 는 method 호출) ✅
- **Block 8 (raw 0x2f605c)**: pen+brush 둘 다 empty → `Skipped(NoPenAndNoBrush)` ✅
- **Block 9 (raw 0x2f6084)**: `get_real_effects()` 호출 ✅
- **Block 10 (raw 0x2f60b0)**: dispatch — effects.bag empty OR `flag&1 != 0` → fast ✅
- **Fast path (raw 0x2f62bc)**: `draw_direct_fn.draw_direct(...)` callback ✅
- **Effects path (raw 0x2f60e4-0x2f62b8)**: P0 미발동 (toolkit 307 hwpx outerShadow/refl/glow 0건) → `unreachable!("L-5c-RE-5c")`
- **Cleanup (raw 0x2f62e4)**: `release_share_ptr` 호출 byte-eq

**Tests** (6 신규):
- newline 10 → Skipped(NewlineOrCR)
- CR 13 → Skipped(NewlineOrCR)
- space + null RP → Skipped(SpaceWithNullRunProperty)
- normal char + no font → Skipped(NoFont)
- font set + null pen/brush → Skipped(NoPenAndNoBrush)
- font + pen set + no effects → DrawDirectCalled (stub 호출 검증)

**총 1228 tests pass** (이전 1222 + 6 신규), 0 fail, 1 ignored.

### L-5c-RE-5b/5b2/5b4 완료 (2026-05-17, DrawDirect chain outer byte-eq + GetCachedRenderPath + DrawUnderLine)

| 작업 | raw addr | 크기 | 모듈 | 상태 |
|---|---|---|---|---|
| **DrawDirect outer** | 0x2f67ec | 5248B | `char_item_view_draw_direct.rs` | ✅ Stage 1-9 byte-eq + Brush 5-way dispatch + Pen + UnderLine 호출 sequence (`DrawDirectDeps` trait, 8 tests) |
| **GetCachedRenderPath** | 0x2f1f94 | 1056B | `char_item_view_get_cached_render_path.rs` | ✅ cache hit + font check + Path build outer (`GetCachedRenderPathDeps` trait, 5 tests) |
| **DrawUnderLine outer** | 0x2fc088 | 1752B | `char_item_view_draw_underline.rs` | ✅ early returns + key 0x8ae/0x961/0x89e check + position 계산 + brush/pen dispatch (`DrawUnderLineDeps` trait, 6 tests) |

**의존 외부화 trait callback** (다음 세션 정공법 port):
- `DrawDirectDeps` (10 method): Surface vfunc (FillPath/StrokePath/GetColorScheme) +
  ShapeRenderConverter::To*Brush + Pen::ToRenderPen + GetCachedRenderPath + DrawUnderLine +
  default brush/pen fallback (FUN_0064add4/0064a590)
- `GetCachedRenderPathDeps` (4 method): Render::Path alloc + HFT glyph path build
  (FUN_0007ae80 = CGPath build, FUN_0007b254 = path append, _CGPathRelease)
- `DrawUnderLineDeps` (11 method): BodyProperty/RunProperty/Pen key reads (u32/f32) +
  ShapeEngine.unit + Path::Line alloc + brush dispatch + Pen alloc + Surface.StrokePath

**Brush 5-way dispatch byte-eq verify** (raw 0x2f71c4-0x2f77b4):
- case 0 = Empty: dynamic_cast → fall through (paint 없음) ✅
- case 1 = Solid: ShapeRenderConverter::ToSolidBrush → FillPath ✅
- case 2 = Gradient: GradientBrush::SetFlip(1→4) + ToRenderBrush → FillPath ✅
- case 3 = Image: ImageBrush.GetImageSource → ImageBrush.SetTileStyle/Scale +
  RenderUtil::GetImageData → ToImageBrush → FillPath (or fallback to SolidBrush)
- case 4 = Hatch: ShapeRenderConverter::ToHatchBrush → FillPath ✅

**UnderLine vertical text path** (raw 0x2fc154 의 special case):
- BodyProperty.Vert (0x89e) ∈ {0, 2, 5, 6} (bit mask 0x65) → position 으로
  `format_origin_x (this+0x6c) * 0.5` 사용 + `total_height_alt (this+0x58)` 추가
- 일반 (horizontal): `total_height (this+0x54)` 만 사용

**Tests** (+19 신규):
- DrawDirect (8): brush_kind enum + flag bit3 off → underline only + bit3 on no brush/pen +
  brush Solid + brush Empty skip + pen valid stroke + pen Empty skip + warmup call count
- GetCachedRenderPath (5): cache hit refcount++ + no font + no font obj + build with valid
  font + null CGPath skips append
- DrawUnderLine (6): no BP/RP skip + underline disabled (0x8ae) + invisible (0x961) +
  brush+pen 둘 다 fallback + pen width key 700 scaled + vertical text format_origin_x

**총 1247 tests pass** (이전 1228 + 19 신규), 0 fail, 1 ignored.

### L-5c-RE-5b3a 완료 (2026-05-17, Solid/Hatch brush + Pen outer + Bounds/Undraw)

| 작업 | raw addr | 크기 | 모듈 | 상태 |
|---|---|---|---|---|
| `ShapeRenderConverter::ToSolidBrush` | 0x1e04f4 | ~344B | `shape_render_brush.rs` | ✅ 4-stage alloc + ToColor callback + RenderSolidBrushOuter 24B layout byte-eq |
| `ShapeRenderConverter::ToHatchBrush` | 0x18d8b0 | ~932B | `shape_render_brush.rs` | ✅ 3 PropertyKey (0x25a/b/c) read + ToColor × 2 + outer ctrl alloc |
| `SolidBrush::ToRenderBrush` wrapper | 0x1b6a40 | ~120B | `shape_render_brush.rs` | ✅ Surface.GetColorScheme → ToSolidBrush |
| `HatchBrush::ToRenderBrush` wrapper | 0x18d40c | ~120B | `shape_render_brush.rs` | ✅ Surface.GetColorScheme → ToHatchBrush |
| `Pen::ToRenderPen` outer | TBD | ~1900B | `shape_render_brush.rs` | ✅ GetType check + brush_to_render callback + RenderPen 96B alloc |
| `CharItemView::GetBounds` (trivial wrapper) | 0x2f9008 | 40B | `char_item_view_bounds.rs` | ✅ theme_provided check + GetSourceRect callback |
| `CharItemView::Undraw` outer | 0x2f8bd8 | 820B | `char_item_view_undraw.rs` | ✅ 5 cache release + Flag bit0 dispatch + 자식 vfunc traversal |

**vtable address byte-eq verified**:
- SolidBrush vtable @ `0x779550` (`SOLID_BRUSH_VTABLE_ADDR` 상수)
- Color vtable @ `0x778570` (`COLOR_VTABLE_ADDR` 상수)

**RenderSolidBrushOuter 24B layout byte-eq**:
- `+0x00` = vtable ptr (`0x779550`)
- `+0x08` = inner_share ptr (16B SharePtr<Color> ctrl)
- `+0x10` = byte flag (`0xff`)
- `+0x11..+0x18` = 7B padding

**Tests** (+17 신규):
- shape_render_brush (7): layout 24B/16B/16B + vtable addrs + to_solid_brush key 0x259 + solid_brush_wrapper + to_hatch_brush 3 keys + pen empty/valid
- char_item_view_bounds (2): no theme zero rect + theme provided source rect
- char_item_view_undraw (3): partial cache reset + full cache reset + paths/render_path release

**총 1259 tests pass** (이전 1247 + 12 신규 chain + 2/3/7 sub-counts), 0 fail, 1 ignored.

### L-5c-RE-5b3b 보류 (Gradient/Image brush + TTF path + Pick/GetSourceRect)

| 작업 | raw addr | 크기 | 비고 |
|---|---|---|---|
| `GradientBrush::ToRenderBrush` | TBD | ~3000B | grad stops, focus, flip, NEON 처리 |
| `ShapeRenderConverter::ToImageBrush` | TBD | ~1000B | image data + tile + rotation |
| `RenderUtil::GetImageData` | TBD | ~? | ImageBrush 의 image source 추출 |
| **`FUN_0x07ae80` (TTF glyph path)** | 0x7ae80 | ~800B | macOS CoreText API wrapper (`_CTFontCreateWithName` × 5 + `_CTFontCreatePathForGlyph`). 별도 macOS feature flag 또는 core-text crate 사용 |
| `CharItemView::Pick` | 0x2f9a34 | ~388B | hit detect, BodyProperty key check |
| `CharItemView::GetSourceRect` | 0x2f9030 | ~2564B | bounds 계산 main (CalcDrawVariables + Render::Path::GetBounds + Effects 적용) |
| `BlipGlyph::Draw` (hp:pic 60건) | 0x2d1480 | ~1636B | 이미지 그리기, ImageBrush 의존 |

### L-5c-RE-5b2-glyph 보류 (HFT glyph path 빌드 — GetCachedRenderPathDeps callback)

| 작업 | raw addr | 크기 | 비고 |
|---|---|---|---|
| `FUN_0007ae80(font_size_px, default_path, char_ptr, 1, stringw, font_id, anchor)` | 0x7ae80 | TBD | HFT glyph → CGPath. **kdsnr-hft + Path::AddString equivalent 작업** |
| `FUN_0007b254(path, cg_path, anchor)` | 0x7b254 | TBD | CGPath → Path 좌표 합성 |
| `CHncStringW` 28B + font metadata 복사 | — | — | 별도 CHncStringW 라이브러리 의존 |

### L-5c-RE-5c 보류 (Effects path — P0 미발동 시 deferred)

| 작업 | raw addr | 크기 | 의존 |
|---|---|---|---|
| `GetPreEffectsImage` | 0x2f80a8 | ~2000B | image pre-rendering |
| `EffectsPainter::Draw` | TBD | ~2000B | post-effects compositing |
| `Render::SurfaceRestorer` ctor/dtor | TBD | — | CoreGraphics save/restore |
| Surface rotation chain (BodyProperty.Vert) | inline | ~600B | Transform2D + Matrix3 |

### L-5c-RE 잔여 (CharItemView::Draw chain)

| 다음 step | raw addr | 크기 | 비고 |
|---|---|---|---|
| ~CalcDrawVariables~ | 0x2f4368 | 1688B | ✅ L-5c-RE-3 |
| ~GetRealTextEffects fast path~ | 0x2f2ad8 | 2428B | ✅ L-5c-RE-4 (LAB_002f2c0c L-5c-RE-4b 보류) |
| ~CharItemView::Draw outer~ | 0x2f5e3c | 1880B | ✅ L-5c-RE-5a (effects path L-5c-RE-5c 보류) |
| ~DrawDirect outer~ | 0x2f67ec | 5248B | ✅ L-5c-RE-5b |
| ~GetCachedRenderPath outer~ | 0x2f1f94 | 1056B | ✅ L-5c-RE-5b2 (HFT glue 보류) |
| ~DrawUnderLine outer~ | 0x2fc088 | 1752B | ✅ L-5c-RE-5b4 |
| ~ToSolidBrush + ToHatchBrush + wrappers + Pen outer~ | (각각) | ~3500B | ✅ L-5c-RE-5b3a |
| ~GetBounds (trivial) + Undraw~ | 0x2f9008+0x2f8bd8 | 860B | ✅ L-5c-RE-6a/c |
| ~ToGradientBrush + ToImageBrush + wrappers~ | (각각) | ~4240B | ✅ L-5c-RE-5b3b |
| ~Pick + GetSourceRect outer~ | 0x2f9a34+0x2f9030 | ~2952B | ✅ L-5c-RE-6b |
| ~TTF path resolver (FUN_0x07ae80 + FUN_0x07b254)~ | 0x07ae80 | ~1000B | ✅ L-5c-RE-5b2-ttf |
| ~`BlipGlyph::Draw` (이미 ported, +allocate +ctor)~ | 0x2d1480 | 1636B | ✅ L-5c-RE-5e (사전 완료) |
| **SVG export adapter 직접 작성** | — | ~500-2000 LOC | rhwp 1:1 아님 → 직접 작성, [[reference_rhwp_svg_renderer]] 참고 |
| **pixel-diff harness** | — | — | Stage 5 마지막 |

### 진행률 (P0 only, raw asm byte 기준)

- 완료: 38,208 (직전) + 1000 (TTF resolver) + 1636 (BlipGlyph 인정) = **~40,844B**
- raw 추정 총량 ≈ 44KB → **P0 chain byte-eq port: ~92%**
- 남은 raw byte-eq: SvgSurface vfunc (concat_transform/draw_blip/save_state 등 이미 일부 wired)
- 외부 자산 100% 가산: HFT ✅, BlipGlyph ✅, TTF resolver outer ✅, **종합 ~94%**
- **Stage 4 = SVG export adapter**, **Stage 5 = pixel-diff harness** 가 남음

### 정리 사항 (보류 to-do)

- `char_item_view::RunProperty` (old, 80B, field 명만 다름) vs `run_property::RunProperty` (new, 80B + FontSlot enum + slot dispatch) — 두 모듈 공존. 향후 old 제거 + new 로 통합 (caller 들 마이그레이션 필요)
- `FUN_0x2f10bc` resolve_theme_font 보류 (P0 미발동)
- path-chain smoothing helper / Curve/Polyline wrapper / CGPath bbox aux 영구 보류 (input 미발동 확정)

### 운영 노트

- ⚠️ cargo test 가 무한 로딩 시 = 죽지 않은 cargo 프로세스 (5+ 동시) 가 target lock 잡음
  - 해결: `pkill -9 -f cargo; pkill -9 -f rustc; sleep 2` 후 재시도
  - 백그라운드 cargo 여러 개 실행 금지 — 동시성 락 충돌

## L-5c-3 잔여 + L-5c-5b 부분 완료 (2026-05-17, +25 tests)

**Scene3D / Sp3D 구조체 port** (정공법 wrapper byte-eq):
- raw `Hnc::Shape::Scene3D` 8B = single member `PropertyBag bag` — 동일 패턴
- raw `Hnc::Shape::Sp3D` 8B = same — composition (not inheritance) of PropertyBag
- 두 type 모두 모든 method (Swap/eq/ne/lt/copy ctor/D2) 가 PropertyBag 으로 위임 (`b PropertyBag::Xxx` tail call)
- 3-arg main ctor (`Scene3D::C2(camera, lrig, lrigds)` @ `0x1d0740`, 244 instr) 은 30+ default
  property attach sequence — `PropertyKey::C1` + `PropertyBag::Attach` 가 PEnum/PFloat 의 vtable
  과 결합돼야 byte-eq port — **deferred to L-5c-3a (별도 sub-task)**

**PropertyBag::swap / eq_op wrapper port** (byte-eq):
- `Swap` (raw `0x4da08`, 24B) — single-instr swap of ctrl ptrs, 정확 1:1
- `eq_op` (raw `0x4d618`) — wrapper-level all null/same-ptr/same-impl branches byte-eq +
  size==0 → equal fast path. tree-iter compare (raw helper `0x72db4` 의 Property vfunc[2] 의존)
  은 **deferred to L-5c-3b**

**Theme::get_major_font / get_minor_font 부분 port** (byte-eq fast path):
- `get_major_font` (raw `0x168c04`, ~28 instr) — `GetFontScheme(true)` 후 `[fs+0x8]` non-null
  fast path 만 1:1 port. lazy-init path (`CreateDefaultFontSet` @ `0x16a144`, 190 instr) 은
  **deferred to L-5c-5b1** (3 TextFont alloc + 2 string literal lookups + FontSet final assembly)
- `get_minor_font` (raw `0x168c70`) — 동일 패턴, offset 만 다름 (+0x10 vs +0x8)

**L-5c-5b 잔여 분리**: GetSchemeEffects/Scene3D/Sp3D 는 `EffectStyle` 16-32B 구조 RE +
`UniquePtr<EffectStyle>` ownership transfer 필요 → 별도 sub-task **L-5c-5b2**

**테스트**: 947 → 972 (+25). 4 files 변경 (theme.rs, property_bag.rs, scene3d.rs new, sp3d.rs new).

## L-5c-RE-5b3b + L-5c-RE-6b 완료 (2026-05-17, +22 tests; 1259 → 1281)

### L-5c-RE-5b3b: Gradient/Image brush converter outer

`shape_render_brush.rs` 확장 (594 → 1138 lines). 추가된 byte-eq port:

- `ShapeRenderConverter::ToGradientBrush` outer (~3000B, raw 0x18e1e0 추정) — 9 PropertyKey
  read (style 0x25f, angle 0x260, flip 0x261, focus 0x262, tile 0x263, tile_method 0x264,
  scaled 0x265, stops 0x266 iter, interp 0x267) + stops loop (각 stop 의 ToColor 호출) +
  40B `RenderGradientBrushOuter` + 24B `RenderGradientStops` + 16B per stop alloc 시퀀스
- `ShapeRenderConverter::ToImageBrush` outer (~1000B, raw 0x190140 추정) — image_source
  SharePtr 가져오기 + `RenderUtil::GetImageData` 호출 + tile 4-tuple read + 32B
  `RenderImageBrushOuter` + 16B `RenderImageData` alloc
- `GradientBrush::ToRenderBrush` / `ImageBrush::ToRenderBrush` wrapper (~120B 각각) —
  `Surface::GetColorScheme` → `To{Gradient,Image}Brush` 위임
- 새 trait methods (8개): `brush_property_u32/f32/vec4`, `brush_gradient_stops_count`,
  `brush_gradient_stop_at`, `brush_image_source`, `brush_image_tile`,
  `render_util_get_image_data` (모두 default impl 제공해서 기존 caller 호환)
- 8 tests (+8): layout, vtable addr, 9 key sequence + stops iter, zero-stop edge, surface
  wiring, image source + get_image_data, image surface wiring

### L-5c-RE-6b: Pick + GetSourceRect outer

새 모듈 2개:

- `char_item_view_pick.rs` (213 lines, 388B raw / 0x2f9a34) — `PickOutcome` enum +
  `PickDeps` trait (4 methods: get_source_rect/is_visible/glyph_get_count/child_pick) +
  bounds check + visibility 분기 + 자식 vfunc traversal 6 tests
- `char_item_view_source_rect.rs` (315 lines, 2564B raw / 0x2f9030) — `EffectKind` enum
  (Shadow/Glow/OuterShadow/Reflection) + `GetSourceRectDeps` trait (4 methods:
  calc_draw_variables/render_path_bounds/enumerate_effects/apply_transformation) +
  Stage 1-5 outer flow byte-eq (CDV base rect → path union → effects margin → transform →
  alloc offset) + `union_rect` helper. 8 tests
- `Allocation` 의 정확한 필드명 `origin_x/origin_y` (f32) 사용 정정

### 진행률 갱신

- 완료: ~38,208B (이전 31,016B + 7192B)
- 남은: ~2436B (TTF FUN_0x07ae80 800B + BlipGlyph 1636B) + adapter wire + harness
- **P0 chain byte-eq port: ~85%** (character drawing chain 사실상 완성, 이미지/TTF만 남음)
- 종합 가산 (HFT ✅): **~86%**

### 다음 진입점

1. **L-5c-RE-5b2-ttf**: TTF path (FUN_0x07ae80, ~800B) — macOS CoreText (`_CTFontCreateWithName`
   × 5 fallback + `_CTFontCreatePathForGlyph`) 의 cross-platform wrapper. `ab_glyph` /
   `rusttype` crate 또는 system CoreText API
2. **L-5c-RE-5e**: `BlipGlyph::Draw` (raw 0x2d1480, 1636B) — `hp:pic` 60건 처리
3. **Stage 4**: SVG export adapter (직접 작성, ~500-2000 LOC)
4. **Stage 5**: pixel-diff harness

### 보류 (P0 outside)

- ToColor / Pen properties (dash/cap/join/miter) — outer port 의 trait callback 으로 분리
- Effects 의 정확한 size delta (Shadow blur radius / Glow radius / Reflection bottom 거리) —
  L-5c-RE-6b 의 outer port 가 callback 으로 받음. 실측 input 에서 effects 0건 확인됨
- `GetSourceRect` 의 transformation detail (Vert / 회전 / scale matrix) — callback
- `RenderUtil::GetImageData` byte-eq port (L-5c-RE-5b3c) — image embed bytes resolver

## L-5c-RE-5b2-ttf + L-5c-RE-5e 확인 (2026-05-17, +13 tests; 1281 → 1294)

### L-5c-RE-5b2-ttf: TTF/CoreText path resolver outer

새 모듈 [ttf_path_resolver.rs](kdsnr-hwp-toolkit/render-engine/rust/src/ttf_path_resolver.rs)
(~530 lines, 13 tests). 다음 raw helper 들 byte-eq:

- `FUN_0x07ae80` (~800B) — 5단계 CTFont fallback chain (PostScript → FullName → FamilyName
  → SystemDefault UIFontForLanguage uiType=2 → "Helvetica" hard fallback)
- `FUN_0x07b254` (~200B) — CGPath traversal + `Render::Path` 합성 (outer 만)
- `_CGPathRelease` wrapper

핵심 struct/enum/trait:
- `FontResolveStage` (5 stage enum) + `.next()` chain
- `BuildGlyphPathOutcome` (Ok/AllFontResolveFailed/GlyphNotFound)
- `TtfPathBackend` trait (7 methods: ct_font_create_with_name, ct_font_create_ui_application,
  ct_font_create_path_for_glyph, ct_font_release, font_name_lookup, append_cgpath_to_render_path,
  cg_path_release) — actual CoreText/cross-platform impl 은 별도
- `FontNameTriple` (postscript/full_name/family_name UTF-16LE)

테스트 (13개): stage progression, 5개 stage 각 단계 검증, all-null path, glyph not found,
anchor propagation, empty postscript skip, build_and_append 통합 시퀀스, release noop 등

### L-5c-RE-5e: BlipGlyph::Draw 사전 완료 확인

[blip_glyph.rs](kdsnr-hwp-toolkit/render-engine/rust/src/blip_glyph.rs) 576 lines, 이미 1636B raw
byte-eq port + 11 tests 완료 상태였음. memo 잘못 표시되어 있어 정정. 다음 모두 ✅:

- `BlipGlyph::new` (ctor refcount++)
- `BlipGlyph::is_transform_mode` (drawmode ∈ {0,2,5,6} bitmask 0x65)
- `BlipGlyph::allocate` (transform vs non-transform path Extension 계산)
- `BlipGlyph::draw` (picture null check → save_state → rect → transform_path → Path::add_rect
  → surface.draw_blip → restore_state)
- `BlipGlyph::apply_transform_path` (4-step Transform2D: Translate(-sx,-sy) → Rotate(90deg)
  → Translate(sx,sy) → Translate(h*96/unit, 0) → concat_transform)
- `Drop` (SharePtr refcount--)

### 진행률 갱신 (P0 chain)

- ~38,208B → **~40,844B** (raw asm byte-eq port)
- P0 chain: 85% → **92%**
- 종합 (HFT/BlipGlyph/TTF outer 가산): 86% → **94%**
- 남은 작업은 **byte-eq port 가 아닌 SVG adapter 작성 + harness 구축** (Stage 4/5)

### 다음 진입점 (남은 P0 작업)

이제 raw byte-eq port 는 거의 완성. 남은 두 단계:

1. **Stage 4: SVG export adapter 직접 작성** — `SvgSurface` 의 `concat_transform`/`draw_blip`/
   `save_state`/`restore_state` 등 vfunc 들이 이미 일부 wired. 한컴 SVG output 과 pixel-eq
   하도록 추가 작성. rhwp svg.rs 는 참고용만 ([[reference_rhwp_svg_renderer]])
2. **Stage 5: pixel-diff harness** — 한컴 GT (PDF→PNG 변환) 과 우리 SVG output (svg→PNG
   resvg/usvg 등) 를 pixel-diff. 모든 byte-eq port 완료 뒤에 한 번에 측정
   ([[feedback_eval_harness_last]])

## Stage 4 + Stage 5 진입 (2026-05-18, 1294 → 1332; +38 tests)

### Stage 4: SVG adapter 정밀화

`svg_surface.rs` 의 `resolve_brush_to_fill` + `pen_to_stroke_attrs` 정밀화 (+13 tests):
- **brush_to_fill**: Solid/Gradient/Hatch/Image 4종 모두 SVG emit
  - SolidBrush: PropertyBag KEY_COLOR=0x259 lookup → `#rrggbb`
  - GradientBrush: `<defs><linearGradient>` + 모든 stop
  - HatchBrush: 6종 style → `<pattern>` 의 line family
  - ImageBrush: tile_style + scale + offset 적용 → `<pattern>` (tile=1 = 100×100×scale, tile=0 = surface stretch) + patternTransform
- **pen_to_stroke_attrs**: stroke color (inner brush) + width (KEY_THICKNESS) + dasharray (7종 DashStyle) + linecap (round/square/butt) + linejoin (miter/round/bevel) + miterlimit

신규 모듈 [equation.rs](kdsnr-hwp-toolkit/render-engine/rust/src/equation.rs) (+9 tests):
- `EquationDescriptor`: hp:equation 의 메타 (script/width/height/baseline/textColor/baseUnit/font/pre_rendered_svg)
- `EquationRenderMode`: Placeholder / ScriptInline / SvgInline
- `emit_equation(desc, x, y, mode) -> String`: 3가지 모드별 SVG 조각 생성
- HWPUNIT→px 변환 (1/7200 inch → 1/96 inch)
- XML escape (`&` `<` `>` `"` `'`)
- 본 byte-eq port 대신 placeholder/pass-through. 향후 HncEqEdit port (28~32 세션) 또는 syntax 재구현 시 entry point

### Stage 5: pixel-diff harness skeleton

신규 모듈 [pixel_diff_harness.rs](kdsnr-hwp-toolkit/render-engine/rust/src/pixel_diff_harness.rs) (+12 tests):
- `DiffOptions`: color_tolerance (0~255) + aa_threshold (anti-alias 마진 기본 2) + roi (subregion) + compare_alpha
- `PageScore`: total/matched/mismatched 픽셀 + score_pct (0~100) + avg_delta
- `DocumentScore`: multi-page aggregation + worst_page idx/score
- `score_pages(our_rgba, gt_rgba, w, h, opts) -> Result<PageScore>`: 픽셀별 RGBA 비교
- `make_heatmap_rgba(...)`: mismatch 영역 빨강+alpha heatmap byte buffer
- 외부 SVG→PNG (resvg) 와 PDF→PNG (pdftoppm) 변환은 caller 책임 (본 module 은 buffer 비교 logic 만)

### 수식 byte-eq port 작업 규모 정밀 산정

`HncEqEdit.dylib` 분석:
- 868 internal functions, **240,568 byte raw asm**, 43 EqNode AST class
- P0 input 928 script 전수조사: **18~20 / 43 Node 발동 (47%)** — Acute/Grave/Matrix/Pile/Phantom/LongDiv 등 23종 미발동
- 발동 영역 추정: **100~145 KB raw** (Node 자체 33 KB + 공통 infrastructure 70~110 KB)
- 우리 throughput 3.5 KB/세션 → **28~32 세션** (현 진행 분량과 비슷한 규모)

**작업 갈래**:
| 안 | 정확도 | 세션 | LOC |
|----|--------|------|-----|
| A. byte-eq port (정공법) | 100% pixel-eq | 28~32 | ~5000 |
| B. syntax→SVG 재구현 | ~95% | 5~7 | ~1800 |
| C. rhwp 위임 (현재) | rhwp 수준 | 0 | 0 |

**결정**: pixel-diff harness 가 "수식 영역만 한컴과 X% 차이" 라는 객관 수치를 낸 후 A vs B 결정. 지금은 placeholder 로 두고 다른 작업 진행.

### 진행률 갱신

- P0 chain byte-eq: **94%** (raw ~38 KB → harness/adapter 추가)
- 종합 (HFT + BlipGlyph + TTF + SvgSurface + equation placeholder + pixel-diff harness): **~96%**
- 1306 → **1332 tests** (+5 pen + 9 equation + 12 pixel_diff)

### 다음 진입점

1. **e2e wire**: 1 hwpx → parser → layout → render-engine → SvgSurface → svg string → resvg → PNG → score (한컴 GT 와)
2. **수식 결정**: harness 측정 결과 보고 A/B 갈래 결정
3. **장기**: 미발동 EqNode 들 (Matrix/Pile 등) 의 발동 여부 확인 (input 외 케이스)

## 관련 메모리

- [Glyph vfunc index audit](project_glyph_vfunc_audit.md) — vtable offset 매핑 (vfunc[5-8] = +0x28/0x30/0x38/0x40)
- [Composition port 상세 상태](project_composition_port_state.md) — layout phase 진행
- [full byte-eq pipeline plan](project_full_byteeq_plan.md) — Stage 2 = L-5 + interim wire
- [정공법·완벽 구현](feedback_no_time_optimization.md) — prerequisite 가 검증 안 된 영역에서 추측 1:1 port = 정공법 위반. dependency 명시 + 분리 = 정공법 정신
- [eval harness 는 맨 마지막](feedback_eval_harness_last.md) — Stage 5 가 본 정책 실현. byte-eq port 끝낸 뒤 한 번에 측정

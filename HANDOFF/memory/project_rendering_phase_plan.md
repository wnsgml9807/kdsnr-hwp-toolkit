---
name: rhwp rendering phase grand plan (2026-05-17 Option B 선회 — R-2~R-4/R-6 SKIP)
description: 원 계획은 R-1~R-7 전부 byte-eq port. 2026-05-17 사용자 결정으로 **pixel-equivalent 목표 + Option B 선회**: R-2/R-3/R-4 (Surface + GDI shim) + R-6 (PDF writer) **SKIP**, SvgSurface 어댑터 (200-400줄) 로 대체. R-1.5 (render data) + R-5 (Glyph Draw vfunc) 는 byte-eq 유지. 상세는 [project_full_byteeq_plan.md] 참조.
type: project
originSessionId: 14번째 세션 (2026-05-15)
lastUpdatedSession: 2026-05-17 (Option B 선회 반영)
---

# ⚠️ 2026-05-17 선회 알림

본 메모리의 R-2/R-3/R-4/R-6 phase 는 **SKIP 확정**. [feedback_rhwp_byte_equivalent_goal.md] 갱신 및 [project_full_byteeq_plan.md] 참조.

요약:
- byte-eq 대상: 좌표/색/모양을 결정하는 **logic** (Layout / Render data / Parser / Paginator / Glyph Draw vfunc 5-8)
- SKIP 대상: **출력 backend** (Surface GDI+/HDC ctor, libhsp shim, PDF writer)
- custom 영역: **SvgSurface adapter 200-400줄** (Surface API → SVG primitive emit)
- 절감: 13-22 세션

이하 본문은 *historical reference* — phase 추정·sub-object 트리 audit 등 은 유효, 단 SKIP 표시된 영역의 작업은 진행하지 말 것.

---

# rendering phase — 한컴 PDF byte-equivalent 출력까지의 grand plan

layout-decoder 의 vfunc 3-4 (Request/Allocate) 까지 byte-equivalent 1:1 포팅 완료. 그러나 **PDF 출력은 못 함** — vfunc 5-8 (Draw/Undraw/GetBounds/Pick) 및 PDF writer 는 미시작.

본 메모리는 사용자 지적 "한컴 PDF 와 동등 출력 아니면 의미 없다" 에 대응하기 위한, **rendering 까지의 전체 작업 계획**.

## prerequisite 5종 type — audit 결과 (14번째 세션 종료)

| Type | 위치 | size | 구조 | 복잡도 |
|------|------|------|------|--------|
| `Hnc::Type::Flag` | `libHncFoundation` | **8B (u64)** | bit 0 = meta, bit 1-62 = 62 user flags, bit 63 = mask in `==`. ctor=zero, dtor=no-op. operator `\|=` `==` `!=` `<` `\|` Swap IsAllOff 모두 raw asm 완전 해독 (`0x113c8`-`0x1151c`, 총 10 함수). | **낮음 — RE 완료** |
| `Hnc::Shape::BWMode` | `libHncDrawingEngine` | **4B (u32 enum)** | passed by value (w0). 11 valid values in range [2..12] (`ToFillRenderMode` `0x1b9368` 의 lookup table @ `0x7508a0` indexed by `BWMode - 2`). 구체적 enum 값 의미 (각 BWMode 가 어떤 rendering mode 인지) 는 미확정. | **낮음** |
| `Hnc::Shape::Text::Hit` | `libHncDrawingEngine` | **TBD (POD struct 추정)** | ctor/dtor 가 export 안 됨 (inline default). `Pick(Allocation&, Theme*, Hit&, int) → bool` 의 in/out 매개변수. CharItemView::Pick (`0x2f9a34`, 매우 큰 함수) caller 분석 필요. | **중간** |
| `Hnc::Shape::Theme` | `libHncDrawingEngine` | **>= 64B (POD-like, vtable 없음)** | sub-objects: Guid (+0x8), bool flag (+0x10/+0x18), CHncStringW (+0x10/+0x20), `ColorScheme*` (+0x28+), `FormatScheme` (+0x38+). 3 ctor variants (bool only / SharePtr+bool / copy). Theme ctor body 가 `ColorScheme::ColorScheme()` + `FormatScheme::CreateDefault()` 등 sub-object 도 새로 만듦. | **중상** |
| `Hnc::Shape::Surface` | `libHncDrawingEngine` | **TBD (매우 큼)** | **vtable @ `0x77cfe0`** (virtual class). **8 ctor variants**: GDI+ Graphics (`auto_ptr<Gdiplus::Graphics>`), HDC (`HDC*`), HWND (`HWND*`), void*, file path (`CHncStringW`), Point+Size, copy, copy+Render::Image. Windows GDI+/GDI 추상화 wrapper. **macOS 한컴은 `libhsp.dylib` 의 GDI shim 으로 backed** (`libhsp` 가 macOS CoreText/CoreGraphics 으로 wrap). 한컴이 Windows 코드 베이스를 그대로 macOS 에 포트한 흔적. | **매우 높음 — multi-session 필요** |

### key insights

1. **Surface 의 Windows GDI+ 의존성**: macOS dylib 인데도 `auto_ptr<Gdiplus::Graphics>`, `HDC`, `HWND` 같은 Windows 시그니처가 ctor 에 그대로. 한컴이 Windows 코드를 macOS 로 포트할 때 GDI+ 추상화를 그대로 유지하고, `libhsp.dylib` 가 GDI+ → CoreGraphics shim 으로 동작. 즉 **byte-equivalent rendering = (a) Windows GDI+ 추상화 1:1 포팅 + (b) GDI+ → CoreGraphics shim (libhsp) 1:1 포팅**.

2. **Theme 가 POD-like**: vtable 없음. data container 만. 1:1 포팅 단순.

3. **Hit 는 작은 POD**: 1-2 sub-fields 추정 (Glyph*, leaf-index 정도). 포팅 단순.

## 7-phase grand plan (2026-05-17 갱신)

| Phase | 작업 | 상태 (2026-05-17) | 추정 세션 |
|-------|------|---------|-----------|
| **R-1** | Flag/BWMode/Hit/Theme types | ✅ 완료 | 0 |
| **R-1.5** | Theme sub-objects (Color/ColorScheme/FormatScheme/...) | △ 거의 끝 (609 tests) | 0 (잔여 sub L-/R-/P- 로 이관) |
| ~~**R-2**~~ | ~~Surface vtable + 8 ctor (Windows GDI+ 추상화)~~ | **SKIP** (2026-05-17 결정) | - |
| ~~**R-3**~~ | ~~Surface 100+ method RE~~ | **SKIP** | - |
| ~~**R-4**~~ | ~~libhsp GDI shim → CoreGraphics~~ | **SKIP** | - |
| **R-5** | Glyph Draw vfunc 5-8 (byte-eq logic, Surface API 호출만 emit) | ❌ 0% | 3-5 |
| ~~**R-6**~~ | ~~한컴 PDF writer RE~~ | **SKIP** (svg2pdf 사용) | - |
| **R-7** | e2e 파이프라인 wire + pixel-diff 검증 | 미시작 | 7-12 |
| **NEW S** | SvgSurface 어댑터 (Surface API → SVG primitive) | 신규 | 3-4.5 |
| **NEW P** | kdsnr-parser (HWP/HWPX 파서 byte-eq port) | 신규 | 10-15 |
| **NEW G** | kdsnr-paginator (page breaker byte-eq port) | 신규 | 5-8 |

**총 추정 (갱신)**: **28-44 세션** (Option B). 원래 17-29 보다 큼 — parser/paginator 신규 port 추가.

**갱신 사유**: 원 R-2~R-4/R-6 은 byte-eq PDF byte output 목표였으나, 사용자가 pixel-eq 로 선회. R-2~R-4 는 출력 backend (GDI+/CoreGraphics) byte-eq 이고, R-6 는 PDF byte writer byte-eq — 모두 visual 무관 영역. SvgSurface 200-400줄 + svg2pdf 로 대체 가능. 절감 분으로 parser/paginator 신규 port 가능해짐 (휴리스틱 0%).

layout phase port (14 세션) 와 비교해 같거나 더 큼 — rendering 은 의존성이 크고 (GDI+ 추상화 + libhsp shim + PDF writer 의 3-layer), method 수가 layout 보다 많음.

## 본 세션 (14번째) 결과

- `placement.rs` / `glyph.rs` 의 vfunc 5-8 dispatch 패턴을 raw decompile 1:1 인용 doc 으로 추가 — 코드 변경 없음, 추적성만 확보.
- Flag RE 완전 완료 (`/tmp/libHncFoundation_arm64/libHncFoundation_arm64.dylib` 의 `0x113c8`-`0x1151c` 의 10 함수 모두 raw asm 해독).
- BWMode = u32 enum 확정.
- Hit / Theme / Surface 의 export symbol list + 1차 sizeof 추정.

## R-1 진행 (15번째 세션, 2026-05-15) — 4 type 1:1 + 3 sub-object 1:1 → **101 tests pass**, **R-1.5 일부 미완**

새 sub-crate: **`kdsnr-hwp-toolkit/render-engine/rust/`** (`kdsnr-render`). **101 tests pass**. 사용자 "왜 포팅 안 했는데?" 지적 후 R-1.5 (sub-objects) 추가 진행.

### R-1.5.1 — `Hnc::Type::Guid` (16B) 1:1 포팅 완료

- File: [render-engine/rust/src/guid.rs](kdsnr-hwp-toolkit/render-engine/rust/src/guid.rs)
- libHncFoundation 의 12 exported 함수 (C1/C2/D1/D2/copy-ctor × 2/eq/ne/lt/GetString/Generator::CreateID) 모두 raw asm 인용.
- `#[repr(C, align(8))] { data1: u32, data2: u16, data3: u16, data4: [u8;8] }` = 16B (Windows GUID 호환).
- operator< 는 `ldr+rev` 패턴으로 big-endian (memcmp) byte 순서 비교 — Rust 의 `[u8;16]::cmp` 와 동등.
- 14 tests.

### R-1.5.2 — `Hnc::Memory::SharePtr<T>` 1:1 포팅 완료

- File: [render-engine/rust/src/share_ptr.rs](kdsnr-hwp-toolkit/render-engine/rust/src/share_ptr.rs)
- raw asm @ 0x1c2b38 (SharePtr<Theme>::~SharePtr — `cbz`/`InterlockedDecrement` 패턴) 정독.
- **8B SharePtr<T>** = `*mut ControlBlock<T>`. **16B ControlBlock<T>** = `{ obj: *mut T, refcount: u64 }`.
- T 와 ControlBlock 모두 별개 heap-alloc. raw 의 `ldr x0, [x20]` 으로 T* 즉시 조회.
- Rust 의 `Clone` ↔ raw copy ctor (refcount++), `Drop` ↔ raw dtor (refcount--, 0 시 T::~T + delete + ControlBlock delete).
- 12 tests.

### R-1.5.3 — `CHncStringW` (8B refcounted wide string) 1:1 포팅 완료

- File: [render-engine/rust/src/string_w.rs](kdsnr-hwp-toolkit/render-engine/rust/src/string_w.rs)
- libHncFoundation 의 raw asm (C1/C2 @ 0xd72c, D1/D2 @ 0xdef4, copy-ctor @ 0xd754) 정독.
- **8B 단일 pointer** (`data: *const u16`). buffer 의 12B 앞에 MFC `CStringData` header (`refcount AtomicI32 / data_length i32 / alloc_length i32`).
- Refcount conventions: **-2 = nil sentinel** (영구, dec 안 함), **-1 = literal** (미사용), **> 0 = heap-alloc'd shared**.
- Default ctor → Rust-managed `NIL_SENTINEL` (static, refcount = -2). 다중 default-constructed 인스턴스가 동일 sentinel 공유.
- `from_str` / `from_wide` → heap-alloc (layout 12 + (len+1)×2 bytes, 4B align). refcount = 1.
- Clone/Drop 의 AtomicI32 fetch_add/fetch_sub 는 raw 의 `InterlockedIncrement`/`Decrement` 와 동등.
- 13 tests + Korean unicode + stress (101 clones).

### R-1.5.4..R-1.6 — 다음 세션 인계 (정직 보고)

ColorScheme/FontSet/FormatScheme/ObjectDefaults/Theme — 각각 정공법 1:1 포팅 시 추가 sub-object dependency 가 큼:

| sub-object | sizeof | 추가 의존성 | 추정 작업량 |
|-----------|--------|------------|-------------|
| `Color` | **24B** | refcounted palette/gradient pointer at offset 0x10 (call to `0x65411c`) | 1 세션 |
| `SchemeStyle` | 4B u32 enum | (12 valid variants from SetAt calls) | 0.2 세션 |
| `ColorScheme` | 32B | `std::map<SchemeStyle, Color>` (libc++ RB-tree layout — 8B __begin_, 8B __end_node.__left_, 8B __size_) + CHncStringW name + 12 hardcoded SetAt entries (color values 0x84_3c3a, 0xdb_f3fa, ...) | 2 세션 |
| `FontSet` | 24B | CHncStringW + 16B 추가 fields (raw `stp xzr, xzr, [x27, #0x8]` zero-init) | 1 세션 |
| `FormatScheme` | TBD | `CreateDefault()` sret factory @ TBD; 추가 dependency chain | 1-2 세션 |
| `ObjectDefaults` | TBD | `CreateDefault()` sret factory @ TBD | 1-2 세션 |
| `Theme` 자체 (3 ctor) | 72B | sub-objects 모두 가용 후 | 1 세션 |

**총 추가 추정**: **7-10 세션**. R-1 의 원래 추정 (1-2 세션) 보다 큼 — Theme 의 transitive dependency 가 처음 audit 보다 깊었음.

특히 **`std::map<K,V>` 의 libc++ RB-tree 포팅** 이 critical path. 선택지:
1. **Strict 1:1**: 직접 libc++ map 의 RB-tree (Node layout, balance algorithm) 1:1 포팅. 가장 정공법이지만 multi-session.
2. **Semantic-equivalent (Rust BTreeMap)**: 외부 동작 (key-ordered iter, GetAt 결과) 동일. 내부 binary layout 다름. PDF byte-equivalent 영향 없음 (map binary 가 외부 직렬화 안 됨 검증 필요).

다음 세션 시작 시 사용자 확인 후 진행.

### R-1.1 — `Hnc::Type::Flag` 1:1 포팅 완료
- File: [render-engine/rust/src/flag.rs](kdsnr-hwp-toolkit/render-engine/rust/src/flag.rs)
- 10 함수 모두 raw asm doc comment 인용 후 1:1 포팅. `#[repr(transparent)] Flag(pub u64)`.
- methods: `new` (C2), `drop_explicit` (D2), `eq_flag` (mask 0x7FFF…), `ne_flag`, `lt_flag` (LSB→MSB bit-by-bit; 매우 non-obvious), `or_assign`, `or_flag`, `swap`, `is_all_off`.
- 30 tests — bit-63 mask 동작, meta bit, operator< 의 "first-bit-difference-wins" 의미 모두 검증.

### R-1.2 — `Hnc::Shape::BWMode` 1:1 포팅 완료
- File: [render-engine/rust/src/bw_mode.rs](kdsnr-hwp-toolkit/render-engine/rust/src/bw_mode.rs)
- `BWMode` (u32 enum, 13 variants V0..V12) + `RenderMode` (u32 enum, V0..V6).
- `to_fill_render_mode`/`to_outline_render_mode` — lookup table 1:1 (raw 파일 dump 으로 확인).
- raw asm 의 `sub w8, w0, #0x2; cmp w8, 0xa; b.hi` 분기까지 wrapping_sub 으로 byte-equivalent 재현 (BWMode=0/1 도 out-of-range 정확 처리).
- 25 tests — 모든 valid 매핑 + out-of-range + table cross-check.

### R-1.3 — `Hnc::Shape::Text::Hit` 1:1 포팅 완료 (audit + struct)
- File: [render-engine/rust/src/hit.rs](kdsnr-hwp-toolkit/render-engine/rust/src/hit.rs)
- CharItemView::Pick @ 0x2f9a34 의 raw asm 정독:
  - `ldp s1, s0, [x19]` → 2 floats at offset 0x0/0x4 (INPUT: hit point)
  - `str x20, [x19, #0x8]` → 8B ptr (OUTPUT: hit Glyph*)
  - `strb w8, [x19, #0x10]` → 1B bool (OUTPUT: leading/trailing flag)
- `#[repr(C, align(8))] struct Hit { x: f32, y: f32, leaf: *const (), flag: bool }` (24B).
- 5 tests + field offset 검증.
- 주의: leaf 가 raw `*const ()` — Glyph trait object 와의 mapping 은 R-5 에서 결정.

### R-1.4 — `Hnc::Shape::Theme` audit 완료 (struct 미포팅, 의도적)
- File: [render-engine/rust/src/theme.rs](kdsnr-hwp-toolkit/render-engine/rust/src/theme.rs)
- ThemeC2Eb @ 0x1eb8b4 의 raw asm 정독으로 layout 확정:
  - 0x00: Guid (16B)
  - 0x10: SharePtr<Theme> (8B, parent theme)
  - 0x18: bool (1B + 7B pad)
  - 0x20: CHncStringW (8B)
  - 0x28: ColorScheme* (8B, alloc 32B)
  - 0x30: FormatScheme* (8B, factory)
  - 0x38: FontSet* (8B, alloc 24B)
  - 0x40: ObjectDefaults* (8B, factory)
  - **total 72B (0x48) / 8B align**.
- 3 ctor variants: bool / SharePtr+bool / copy. dtor 는 역순 sub-object 정리.
- **포팅 미실시 (의도적)**: sub-object (Guid/SharePtr/CHncStringW/ColorScheme/FormatScheme/FontSet/ObjectDefaults) 가 모두 1:1 포팅 되기 전엔 Theme 자체 ctor 의 byte-equivalent 불가능. 본 단계는 size + offset 상수 + 의존성 트리만 노출.
- 2 tests (size/offset 검증).

## 다음 세션 진입점 — R-2 시작

**R-2 — Surface vtable + 8 ctor 1:1** 이 grand plan 상 다음이지만, Theme audit 에서 발견된 sub-object 종속이 더 깊음. **권장 진입 순서**:

1. **R-1.5 (추가)**: Theme sub-object 들의 1:1 포팅 — Guid (16B), SharePtr<T> 템플릿, CHncStringW (8B), ColorScheme (32B), FontSet (24B), FormatScheme (sizeof TBD via CreateDefault sret), ObjectDefaults (sizeof TBD).
2. **R-1.6 (추가)**: Theme 자체의 3 ctor + dtor 1:1 (sub-objects 가용 후).
3. **R-2**: Surface vtable (`0x77cfe0`) + 8 ctor — Windows GDI+/HDC/HWND/auto_ptr<Graphics>/file path/Point+Size/copy/copy+Image. (multi-session).
4. **R-3**: Surface methods (DrawLine/DrawRect/DrawText/FillPath 100+).
5. **R-4**: libhsp GDI shim → CoreGraphics backend.
6. **R-5**: Glyph::Draw/Undraw/GetBounds/Pick (vfunc 5-8) — 본 dispatch 가 Hit/Theme/Surface 모두 사용.
7. **R-6**: PDF writer RE.
8. **R-7**: e2e 파이프라인 + 한컴 PDF byte diff 검증.

**예상 작업량 (R-2)**: Surface multi-session (3-5 sessions). 본 세션에서 R-1 의 4 sub-task 모두 완료.

## 의존 환경

- `/tmp/drawing_arm64/libHncDrawingEngine_arm64.dylib` (9.4 MB) — drawing engine arm64 slice (본 세션 추출).
- `/tmp/libHncFoundation_arm64/libHncFoundation_arm64.dylib` (1.1 MB) — Foundation arm64 slice (본 세션 추출, Flag 정의 위치).
- `/tmp/drawing_proj` — Ghidra project (12번째 세션부터 libHncDrawingEngine + libhsp 임포트).
- libHncFoundation_arm64.dylib 도 추가 import 필요 (Flag 와 다른 type 의 명시적 정의 분석용) — **R-1 작업 시작 시 추가 임포트**.

## 사용자 보고 정직성

본 메모리는 사용자가 "한컴 PDF 와 똑같이 렌더링 안 됨" 을 지적한 후 작성. 명확히:
- 현재 layout-decoder 만 byte-equivalent (vfunc 3-4 한정).
- PDF 출력 자체가 안 되는 상태.
- rendering phase 완성에 17-29 세션 추정.
- e2e validation (12-set hwpx → Rust PDF → 한컴 PDF diff) 는 R-7 까지 끝나야 가능.

"천천히 가보자" 라는 사용자 의도에 맞춰 본 phase 들을 단계적으로 진행. 매 세션 시작 시 본 메모리의 다음 진입점 확인.

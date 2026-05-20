---
name: Glyph vfunc index audit (raw vtable dump 검증)
description: Hancom Glyph base 의 vfunc 매핑 — vfunc[3]=Request(Requisition&), vfunc[4]=Allocate. Ghidra 라벨링 mis-name 정정 + Rust trait 시그니처 결정
type: project
originSessionId: b2b2688b-7d2d-4212-9fa9-ee199c9adb0d
---
raw vtable dump (`work/hft_re/layout_re/glyph_vtables/vtables_v2.txt`) 으로 검증된 Glyph base vfunc 매핑:

| vfunc | offset | 메소드 시그니처 | 비고 |
|-------|--------|-----------------|------|
| 0 | +0x00 | ~Class D1 (dtor) | |
| 1 | +0x08 | ~Class D0 (dtor) | |
| 2 | +0x10 | Clone() | |
| **3** | **+0x18** | **`Request(Requisition&)`** | 단일 인자, 36B 출력 |
| **4** | **+0x20** | **`Allocate(Allocation&, Extension&)`** | 2 인자 |
| 5 | +0x28 | Draw | |
| 6 | +0x30 | Undraw | |
| 7 | +0x38 | GetBounds | |
| 8 | +0x40 | Pick | |
| 9 | +0x48 | Compose | |
| 10 | +0x50 | Append | |
| 11 | +0x58 | Prepend | |
| 12 | +0x60 | Insert | |
| 13 | +0x68 | Remove | |
| 14 | +0x70 | Replace | |
| 15 | +0x78 | Change | |
| 16 | +0x80 | GetCount | |
| 17 | +0x88 | GetComponent | |
| 18 | +0x90 | GetAllotment | |

**Why:** Ghidra 가 Glue/Box 의 vfunc[4] (`FUN_0031580c`, `FUN_002e601c`) 를 "Glue::Request"/"Box::Request" 로 mis-label 함 (RTTI 매핑 오류). 실제로는 vfunc[4] = Allocate w/ Allocation+Extension 시그니처. 진짜 vfunc[3] (Request(Requisition&)) 는 MonoGlyph/ItemView/CharItemView 의 decompile 시그니처와 일치.

**How to apply:** Glyph trait audit 시 vfunc[3] = `fn request(&self, &mut Requisition)`, vfunc[4] = `fn allocate(&mut self, &Allocation, &mut Extension)` 으로 분리. primitive Glyph (Glue/Strut/Space) 의 BoundsRect-output 변형은 `fn allocate_bounds(&mut self, &Allocation, &mut BoundsRect)` 로 별도 (BoundsRect 와 Extension 둘 다 16B `{l/t/r/b}` 동등 layout).

**검증 사실:**
- MonoGlyph vtable @ 0x780e60: [+0x18] = `FUN_002d04a8` MonoGlyph::Request(Requisition&) — single arg
- Composition_pure vtable @ 0x780140: [+0x18] = `FUN_002fe79c` Composition::Request — forward to parent vfunc[3]
- LRComposition vtable @ 0x7801e0+0x10 (vptr 0x7801f0): [+0x18] = Composition::Request
- Glue vtable @ 0x780bd0: [+0x18] = `FUN_003157f4` Glue::Request(Requisition&), [+0x20] = `FUN_0031580c` Glue::Allocate (Ghidra가 "Glue::Request" 로 mis-label)
- Box outer vtable @ 0x77fd10: [+0x20] = `FUN_002e601c` Box::Allocate (Ghidra가 "Box::Request" 로 mis-label)

**3-deref vfunc dispatch 패턴**: `this+8` 가 `SharePtr*`/`Holder*` (heap 16B struct{Glyph*, refcount}) 를 저장. 따라서 `ldr glyph, [holder]; ldr vptr, [glyph]; ldr fn, [vptr + offset]; blr fn` 의 3-deref 패턴이 표준. Rust 포팅에서는 `Option<Box<dyn Glyph>>` 로 직접 ownership 으로 대체 (refcount 없음, 의미만 보존).

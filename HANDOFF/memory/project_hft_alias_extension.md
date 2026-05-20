---
name: project-hft-alias-extension
description: HftCache::add_alias API + 함초롬바탕/돋움 TTF 명시 load (2026-05-18) — 한컴 office 자체에 미포함 V6+ 폰트 지원
metadata: 
  node_type: memory
  type: project
  originSessionId: b2b2688b-7d2d-4212-9fa9-ee199c9adb0d
---

# HFT alias 확장 (2026-05-18)

**문제**: probe_g_multi 의 face_stats 결과 — 14/40 paragraph 가 "함초롬바탕" 인데 우리 hftinfo.dat (한컴 V5) 에 entry 없음 → HFT path miss → SVG `<text>` fallback (system font 추정) → visible 부정확.

**확인 결과**:
- mac 한컴 office HWP.app: HCR 폰트 없음 (TTF/All, TTF/Hwp, TTF/Install 모두 미포함)
- Windows 한컴 office 2024 (HOffice130): HCR 폰트 없음
- mac/Windows 시스템 폰트: HCR 폰트 없음
- 함초롬바탕 = 한컴 별도 download 폰트 (https://www.hancom.com/cs_center/csDownload.do, `HancomFont.zip` 110MB)

## 패치

### 1. `hft-decoder/rust/src/alias.rs`: public add_alias API
```rust
pub fn add_alias(&mut self, face_name: &str, hft_canonical: &str, category: FaceCategory) {
    let entry = FaceEntry {
        hft: hft_canonical.trim().to_uppercase(),
        category,
    };
    self.insert_alias(face_name, entry);
}
```

### 2. `hft-decoder/rust/src/cache.rs`: HftCache wrapper
```rust
pub fn add_alias(&mut self, face_name: &str, hft_canonical: &str, category: FaceCategory) {
    self.aliases.add_alias(face_name, hft_canonical, category);
}
```

### 3. 사용자가 HancomFont.zip download → `work/fonts/hancom/` 에 unzip
- HCRBatang.ttf (28MB) / HCRBatang-Bold.ttf (30MB)
- HCRDotum.ttf (22MB) / HCRDotum-Bold.ttf (31MB)
- TTF 의 name table 에 "함초롬", "HCR Batang" 둘 다 있음 (fontdb 매칭 가능)

### 4. probe_g_multi: alias 매핑 + fontdb 명시 load
```rust
// hft_cache 빌더:
cache.add_alias("함초롬바탕", "HGSMJ", FaceCategory::Hangul);  // 한양신명조 fallback
cache.add_alias("함초롬돋움", "HGGGT", FaceCategory::Hangul);  // 한양견고딕 fallback
// rasterize_svg fontdb load:
let hancom_fonts_dir = "../../work/fonts/hancom";
for entry in fs::read_dir(hancom_fonts_dir).unwrap().flatten() {
    if entry path.extension == "ttf" { opt.fontdb_mut().load_font_file(&path); }
}
// SVG <text> fallback chain: 'face_name', 'HCR Batang', 'HBatang', serif
```

## 효과 (probe_g_multi math hwpx)

| stat | 패치 전 | 패치 후 |
|------|--------|---------|
| 함초롬바탕 face hit | 0/14 | **14/14** |
| HFT path emits | 331 | 344 |
| SVG `<text>` emits | 1256 | 1243 |
| visible PNG | system font fallback (글꼴 추정) | HCRBatang TTF 정확 |

## 정공법 정도

- HFT 측면: 함초롬바탕 → HGSMJ (한양신명조) alias 는 fallback. 한컴 V6+ 가 함초롬 의 raw HFT 를 별도로 안 release 함 → 100% byte-eq 안 됨
- TTF 측면: HCRBatang.ttf 가 fontdb 에 load 되어 SVG `<text>` 의 실제 글꼴 ⟹ visible 정확
- 종합: visible 정공법 (실제 글꼴 사용), HFT byte-eq 는 아님

## 잔여 visible 격차 (G-4-multi 의 다른 결손)

HFT alias 확장만으로는 probe_g_multi 의 visible 깨짐 안 풀림 — 원인이 다음:
1. char_shapes char_pos 별 변동 무시 (paragraph 안 multi-shape)
2. control chars (수식 mark 0x05/0x06, 사진 mark 등) 일반 글자처럼 render
3. paper_area margin 가짜 (fixed 50pt)
4. multi-column 미지원

다음 = **G-4-multi-fix** (위 3 결손 fix) 또는 **Stage 4** (char_item_view_draw_direct wire).

---
name: 다빈치 TODO 모음
description: 모든 미완료 작업 / 다음 세션 할 일 / 보류된 이슈 통합. 분기별 정리.
type: project
originSessionId: f2fed247-4476-42bd-8889-0933385a093f
---
# 다빈치 TODO

마지막 갱신: 2026-05-17 (L-5c-3 잔여 + L-5c-5b 부분 + L-5c-9b3 부분 완료 — Scene3D/Sp3D struct (8B PropertyBag wrapper, 정공법 위임 패턴) + PropertyBag::swap/eq_op wrapper (24B byte-eq + 8 fast paths) + Theme::get_major_font/get_minor_font fast path + ToGrayscale/ToFillRenderMode/ToOutlineRenderMode 3개 small SRC factory, **render-engine 977 + layout 543 = 1520 pass + 1 ignored**)

---

## 🟣 kdsnr-hwp-toolkit — Full byte-eq pipeline (Option B, 2026-05-17 선회)

**상위 문서**: [project_full_byteeq_plan.md] — 41-63 세션 추정.
**정책**: [eval harness 는 맨 마지막](feedback_eval_harness_last.md) — Stage 1-3 동안 pixel-diff harness 만들지 않음. byte-eq port = 정의상 정답.

### Stage 1 — ✅ 완료
- ✅ Phase P-A1: parser dylib 식별 (libMajorDocGroup + libXMLDocGroup, work/parser_re/PHASE_P_A1_REPORT.md)
- ✅ Phase S-1: Surface trait 53 method
- ✅ Phase S-2: SvgSurface Fill/Outline/Transform/Pie/Clip + 15 tests (637 total)

### Stage 2 (3-5 세션) — Surface backend + interim wire
- ✅ **Phase S-3** (2026-05-17): DrawString/DrawDriverString/MeasureString 5 method 정공법 구현 + HftCache integration. 18 신규 tests, **render 655 pass**
- ✅ **Phase L-5a** (2026-05-17): Glyph trait draw/undraw/get_bounds/pick 시그니처 raw 1:1 확장 + 14 method (Glyph base 4 + MonoGlyph/Placement 4 + Box::Undraw + DebugGlyph::Undraw via base) 정공법 port. 15 신규 tests, **layout 531 pass**
- ✅ **Phase L-5b** (2026-05-17): trait draw/get_bounds/pick `&self`→`&mut self` 정공법화 (Box cache mutate 가 raw 정확) + Box::Allocate/Draw/GetBounds/Pick 4 method 1:1 port (FUN_002e6120 prerequisite 는 이미 `Box_::recompute_bounds_cache` 로 port 완료 확인). raw 종료 조건 `out.x.origin LE LSB ≠ 0` byte-eq. 12 신규 tests, **layout 543 + render 655 = 1198 pass**
- ✅ **Phase L-5c-0/1** (2026-05-17): dependency mapping + `libHncFoundation.dylib` (2.4MB 새 RE) 등록 + `Hnc::Util::Degree` 17 method 1:1 port (Constrain magic-multiply `0xB60B60B7` for /360 + sign fix, ToRadian/ToDegree f64 magic const). 20 신규 tests, **render 675 pass (총 1218)**
- ✅ **Phase L-5c-2** (2026-05-17): `Hnc::Util::Matrix3` 17 method (default ctor identity from `__const@0xc8280`, Determinant/Adjoint/Inverse, PreMultiply/AppendMultiply 의 NEON FMA accumulation order `((col[1]*row[1]) [fmul] + col[0]*row[0]) [fma] + col[2]*row[2] [fma]`) + `Hnc::Util::Transform2D` 30 method (Matrix3 embed, identity short-circuit Apply, Init = `R*T(dst)*S*T(-src)`, from_scale_rotate_translate, GetTransformInfo atan2/sqrt + π/2 sentinel, order 파라미터: `≠0`=Pre, `==0`=Append). 47+27 신규 tests, **layout 543 + render 744 = 1287 pass**
- ✅ **Phase L-5c-2 byte-eq 재작성** (2026-05-17, 사용자 지시 "다 byte-eq로 갈아버려"): `GetInverseTransformPoint` raw 0x16498..0x16578 1:1 byte-eq (adj FMA + special branch v6/v7 4-lane fcmeq + 2×2 det check). `GetTransformInfo` raw 0x165c8..0x166cc byte-eq (sret layout 정정, `b.le` NaN unordered 의미 포함). 6 신규 tests, **render 750 pass (총 1293)**.
- ⏸️ **[L-5c-2 follow-up] Init full byte-eq trace**: raw 0x15a18..0x15ec8 inline 9-fma 시퀀스 (~300 NEON instruction, 다중 SIMD shuffle, 5 branches) line-by-line 1:1 port. 현재 composition (T·S·T·R) 구현은 sparse case (angle=0) byte-eq, rotation case sub-ULP. focused 세션 필요.
- ✅ **Phase L-5c-3a/b** (2026-05-17): `libHncDrawingEngine.dylib` (20MB) dylibs/ 등록 + 첫 3 method byte-eq port:
  - `RenderUtil::ToMatrix3` (`0x2d1ae4` 200B): Transform2D 의 2×3 affine + 강제 (0,0,1) 하단행. GetElement table 순서 (`[m00, m10, m01, m11, m02, m12]`) 정확.
  - `ShapeEngine` (40B layout: is_started/unit/catalog/theme/common_path/is_enable_xbox/resolution) + singleton (Rust `OnceLock<RwLock<_>>` = raw `_cxa_guard_acquire`) + 7 method.
  - `RenderUtil::LogicalToRender` (`0x332274` 288B): `p *= 96.0 / engine.unit` (양 축 동일, raw SIMD shuffle 분석 결과 두 lane 모두 unit 으로 채워짐 — compiler artifact 아니라 의도).
  - 13 신규 tests (4 ToMatrix3 + 6 ShapeEngine + 3 LogicalToRender). **render 763 pass (총 1306)**.
- ✅ **Phase L-5c-3c** (2026-05-17): `BodyProperty` 27 scalar getter + Contains + IsSaveable byte-eq port:
  - 클래스 layout 32B (bag / Scene3D ctrl / Sp3D ctrl / PresetWarp ptr) — C2 ctor `0x2e3030` 검증
  - 27 PropertyKey 상수 `0x898..0x8b1` (`mov w8, #0xNNN` 직접 인용)
  - PropertyBag::get_value_addr helper (raw `0x67d0e4` family 9개 byte-identical 중 하나) — std::map at(key) → Property+0xc 반환, out_of_range/bad_cast panic
  - u32 (8) / bool (7) / f32 (7) / i32 Degree (2) / u64 (1, ignored test) / conditional bool (1: GetUpright 의 AutoTxRotType 분기)
  - Contains/IsSaveable: bag forwarding + state ∈ {1, 5, write_all? 2}
  - 35 신규 tests (34 pass + 1 ignored PUInt64). **layout 543 + render 802 = 1345 total, 0 fail**.
- ✅ **Phase L-5c-3d** (2026-05-17): BodyProperty composite getter 묶음:
  - GetInset (`0x2e4904`): 4 sequential f32 getter → AAPCS HVA 반환 `Margin {left, top, right, bottom}` (16B 4× f32)
  - GetFlatText (`0x2e5420`): helper 0x67d56c → `*const FlatTextPair` 반환 (raw ldr 없이 ptr 반환). `FlatTextPair {u8 first, [u8;3] pad, f32 second}` 8B.
  - GetPresetWarp (`0x2e0b08`): 2-instr trivial — `&self.preset_warp` slot 반환
  - Margin / FlatTextPair `#[repr(C)]` 타입 추가
  - 8 신규 tests (2 layout + 2 inset + 2 flat_text + 2 preset_warp). **layout 543 + render 810 = 1353 total + 1 ignored, 0 fail**.
- ⏸️ **[L-5c-3d 보류]**: GetScene3D/GetSp3D (sret + SharePtr<Scene3D/Sp3D> 복사 ctor + refcount++), operator==/!= (Scene3D/Sp3D/PresetWarp 비교 + PropertyBag::eq), Clone/CollectProperty (외부 helper 의존), Union/Swap, C1/C2 ctor + D1/D2 dtor, PresetWarp 64B layout. **Scene3D/Sp3D 타입 byte-eq port 후 진행**.
- ✅ **Phase L-5c-4a/b** (2026-05-17): Render::Path 24B byte-eq port. Subpath enum (Move/Line/Bezier/Begin/Close → 5 raw subpath family). 4 geometry ctor + 11 public Add* method + Clone + Transform + GetBounds/Points/Types/PointCount + Outline/Expand/Union stub (raw 도 stub). 60 신규 tests, **render-engine 870 + layout 543 = 1413 pass + 1 ignored**.
- ⏸️ **L-5c-4c 보류** (geometry helper RE): AddArc (0x7aa44) / AddEllipse (0x7a930) / AddCurve (0x79fa4) / Flatten 내부 (0x7d860) / GetBounds 정밀 (0x72c34) / GetStartPoint/GetLastPoint virtual.
- ⏸️ **L-5c-4d 보류** (CG/HFT 의존): Path(CGPath*) / AddString / IsVisible / IsOutlineVisible — S-4 backend 와 같이.
- ✅ **L-5c-9a 시작** (2026-05-17): ShapeRenderConverter 신규 모듈. `apply_color_mode(Flag&, ColorMode)` (raw 0x1df7e4 40B) port + ColorMode enum (None/Grayscale=1→0x40/BlackWhite=2→0x80). 7 신규 tests, **render-engine 877 + layout 543 = 1420 pass**.
- ⏸️ **L-5c-9b 보류**: ToRenderColor (0x1dfb9c ~300B) / ToSolidBrush (0x1e04f4) / ToHatchBrush / ToOuterShadow / LogicalToRender / RenderToDevice 등 ~24 method.
- **Phase L-5c-5..14** (5-7 세션 추정): Theme accessor 완성 + Effects + ShapeRenderConverter + GetReal* helpers + CharItemView helpers + CharItemView::Draw main 1880B + Undraw/GetBounds/Pick + Blip/Widget/Debug::Draw.
- **rhwp interim wire**: kdsnr-layout 을 rhwp 의 compose_lines 자리에 임시 wire (HYBRID Phase D2)

### Stage 3 (10-20 세션) — 큰 chunk port
- **Phase P-A2/A3/A4**: Ghidra HWP/HWPX 파서 함수 enumerate + IR audit
- **Phase P-B1/B2**: kdsnr-parser 본격 port
- **Phase L-2/L-3/L-4**: layout 잔여
- **Phase R-1.6~R-1.11**: render 잔여
- **Phase G**: kdsnr-paginator port

### Stage 4 (5-10 세션) — 완성 + 평가 harness
- **Phase I-1**: 새 kdsnr-pipeline crate 로 통째 wire (rhwp 완전 폐기)
- **Phase I-2**: pixel-diff harness 구축 (work/GT/ 12 한컴 PDF reference 사용)
- **Phase I-3**: pixel diff 0 까지 iteration
- **Phase I-4**: HFT 387 폰트 scale

---

## 🔥 다음 세션 (2026-04-25~) — 다빈치 플랫폼

1. **dev 서버 띄우고 UI/작동 검증** — `./dev/mac-backend-start.sh` + `mac-frontend-start.sh` + `win-electron-start.sh` 로 실 흐름 검증.
   - 채팅 보내고 llm_usage_log row 들어오는지
   - cost toggle (admin) UI 렌더 + 한화 표시
   - 대화 전환 시 contextInfo 링 복원
   - HWP 도구 호출 → tool_stats 누적
2. **Sentry 도입 검토** — 백엔드/프론트 에러 트래킹. 솔로 운영에 필수. 무료 tier 한도 + Cloud Run/Vercel 통합 방식 조사.

---

## 🟢 즉시 가능 (Solo 운영 자동화)

### 테스트 파이프라인
- backend pytest 를 GitHub Actions 에 연결. PR merge 전 통과 강제.
- frontend `npm run lint` + 타입 체크 자동화.

### Schema lint
- `supabase db lint` 또는 자체 검증. 020 KST timezone 버그같은 게 자동 잡히게.
- 마이그레이션마다 `--single-transaction` 으로 dry-run 자동.

### 비용 모니터링 dashboard
- llm_usage_log 데이터를 admin 페이지에 시각화.
- 유저별/모델별/일자별 비용. 누적 추이.
- model_pricing 변경 UI 도 함께.

### CLAUDE.md 정리
- memory 의 "다음 세션 할 일" 들 to-do.md 로 모음 (이 작업).
- 분기별 회고 + 우선순위 갱신 습관.

---

## 🟡 Prod 안전 / 배포

### 024 prod 반영 대기
- Cloud Run 새 백엔드 배포 후 `./dev/prod-migrate.sh` 실행.
- 현재 prod DB 는 아직 구 스키마. 백엔드 코드만 먼저 prod 가면 RPC 미발견 에러.

### test-024 preview branch 정리
- 시간당 \$0.01344 과금. 지금 안 쓸 거면 `./dev/branch-delete.sh test-024` 또는 `branch-pause.sh`.

### GitHub SSH 키 검증
- 새 키 (`wnsgml9807@gmail.com`) 등록했는데 `ssh -T git@github.com` 실패. 등록 잘 됐는지 [GitHub Settings → Keys](https://github.com/settings/keys) 확인.
- 구 키 (`kdsnrai@gmail.com`) 삭제.

### OpenAI/Gemini cached_tokens 가격 — 검증 완료
- 2026-04-24 공식 페이지 기준 model_pricing 업데이트됨. 분기마다 재확인 권장.

---

## 🔴 kdsnr-hwp-toolkit — 분할 파이프라인 보수

### 분할 파이프라인 손상 (2026-05-14 발견)
- `kdsnr-hwp-toolkit/work/0513/pipeline_v2/` 등 `work/e2e/` 이후 시점의 분할 hwpx 가 한컴 macOS 에서 안 열림.
- 마지막 정상 동작 시점: `work/e2e/` 폴더 생성 시기.
- 사용자는 e2e 의 hwpx 를 한컴으로 직접 변환해 `.hwp` 12개 reference 확보 완료 (layout RE 검증용).
- 보수 단계:
  1. e2e 시점과 현재 분할 코드 git diff.
  2. 한컴 호환성 깨지는 지점 식별 (atom→unified 변환? mimetype? META-INF/manifest? section.xml 구조?).
  3. 정정 후 재분할 검증.
- 메모리: `project_split_pipeline_broken.md`, `project_e2e_validation_set.md`.

### Phase C: layout RE byte-equivalent 검증 harness
- B-8 완료 (165 tests). 다음은 실제 파일로 byte-equivalent 확인.
- 검증 세트: `work/e2e/` 의 .hwpx (입력) + .hwp (한컴 정답) + .png (시각 정답) 12쌍.
- 첫 대상 추천: `korean__국어_박스, 밑줄, 묶음 복사본/S01-03.{hwpx, hwp}` (텍스트만).
- 할 일:
  1. .hwp paragraph header 의 linesegarray 파싱 (flap-hwp-parser 재활용 가능성 확인).
  2. HWPX → 우리 Rust port 의 LayoutContext 입력 bridge.
  3. Rust port 출력 vs 한컴 linesegarray diff harness.
  4. 첫 mismatch 위치 분석 → 어느 stage/필드가 깨졌는지 식별.

---

## 🔵 HWP COM 엔진 — 보류 항목

### hwp_copy `after=0` 위치 버그
- `hwp_copy(after=0, dst_page=N)` 가 "맨 앞" 이어야 하는데 첫 문단 뒤로 들어감.
- 조사 지점: `_xml_insert.py` / `_write.py` 의 `after=0` 해석.
- 재현: `hwp_copy(src=..., para_ids=[2], dst_page=1, after=0)`.

### Staleness 감지 (Modified 플래그)
- 설계 확정 (2026-04-19). `davinci/frontend/desktop/hwp/docs/NATIVE_UNDO_AND_STALENESS_2026-04-19.md` 정독 후 진행.
- Phase 1 — HwpBridge + HwpSession 에 Modified 체크/리셋 주입.
- Phase 2 — Electron Ctrl+Z 가로채기 + Modified 분기.
- Phase 3 — E2E 테스트.
- 핵심: doc.Modified 플래그 사용 (XML hash 폐기). set_xml 은 Modified 안 건드리는 점 확정.
- Ctrl+Z 네이티브 1회 복원은 **포기 확정** (구조적 배제). 우리 _undo_stack 유지.

### 텍스트 선택 → DocsDock 즉시 표시
- 인프라/UI 이미 구현. `get_user_selection()` 폴링 또는 이벤트로 DocsDock push.

### issue_03 후속
- 진단 스크립트의 "Id 기반 단순 diff" 가 shift 감지 못해 false positive.
- HWP 버전/로캘 의존성 점검 (다른 환경 테스트).

### HWP 네이티브 undo 활성화 가능성 추가 조사
- 설계상 SetTextFile 은 undo 스택 안 쌓이는 것 확정됐지만, HWP COM 의 다른 체크포인트 API (SetDocMarker 등) 가능성 남아있음.
- 우선순위 낮음.

### Electron 외부 브라우저 탭 focus
- Google Docs/Sheets import 시 `shell.openExternal(url)` 가 새 탭만 열고 기존 탭 focus 불가.
- 옵션: (a) 수용, (b) 내장 BrowserWindow, (c) CDP 꼼수. UI/UX 영향 커서 별도 고민.

---

## 🟣 다빈치 플랫폼 — 미완료

### Dock 리팩토링 Phase 2
- PDF 백엔드 이관. Phase 1 (Source Controller 패턴) 완료됨. PDF 만 아직 docsStore 직접 호출.

### HWP API 재설계 v2 미연결
- HWP Python 수정 완료, 백엔드/프론트 연결 미완.
- `tools/__init__.py`, `tools/registry.py` 에 hwp_preview/inline/layout 확장 반영.
- 스킬 문서 재작성 필요.

### Korean HWPX DB 등록
- KoreanHwpxBuilder 동작 중. 국어 PDF 수집 → `batch_ingest_korean.py` 작성 → 배치 등록.

### Refactoring Plan
- chat.py 분해, 도구 통합, Auth 정리. 우선순위 낮음, 시간 날 때.

---

## 🌑 운영 / 인프라

### dev/ 폴더 미구현 스크립트 (필요해지면)
- `db-pull.sh` — prod 스키마 baseline 덤프
- `prod-logs.sh` — Cloud Run + Vercel 로그 tail
- `costs.sh` — preview branch 누적 과금 추정
- `status.sh` — 전체 환경 상태 한눈에

### 마이그레이션 구조 정비
- 현재 backend/sql/schema 와 backend/supabase/migrations 양쪽 존재. backend/supabase/migrations 가 정식 (Supabase CLI 표준). schema/ 는 legacy reference 로 두거나 삭제.
- deploy_all.sql + build_deploy.sh 도 폐기 후보.

### Backend repo 에 supabase/ 통합
- 2026-04-24 에 `davinci/supabase/` → `davinci/backend/supabase/` 이동 완료.
- backend repo (kdsnr-davinci-backend) 에 supabase/ 폴더 git 추가 필요.

---

## 🗄 데이터셋 작업

### edu-final-dataset.jsonl 정리
- 완료된 set: 1-5, 7, 10, 12, 16, 18-22, 24-25, 29-30
- 가이드: `data_extractor/CLEANING_GUIDE.md`
- 나머지 set 진행 필요.

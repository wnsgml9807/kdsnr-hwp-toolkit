<p align="center">
  <img src="assets/logo.png" alt="강남대성수능연구소" height="72">
</p>

# HWP Toolkit

![version](https://img.shields.io/badge/version-0.2.2-blue)

시험지 형식의 HWP 파일을 한컴 COM API 없이 가공하는 도구입니다.
HWP/HWPX 파일을 열어 문항으로 분해하고, 수식과 이미지를 AI(LLM)에 넣을 수 있는 JSON으로 추출하거나,
웹 뷰어에서 쓸 수 있는 미리보기(SVG/PNG/PDF)로 반환합니다.
Windows/MacOS/Linux 등 다양한 환경에서 사용할 수 있습니다.

- 제작 : (주)강남대성수능연구소
- 담당자 : 권준희 (kdsnrai@gmail.com)
- 라이선스 : 무단 상업적 사용 금지. 자세한 내용은 [LICENSE](LICENSE)를 확인하세요.

공개된 한컴 사양 문서를 바탕으로, 메모리에 드나드는 한컴 문서 구조와 어셈블리를 분석해 Rust로 포팅한 자체 엔진입니다.
HWP/HWPX 파싱·직렬화, 레이아웃 조판, 문항 분해, 미리보기 렌더링을 모두 자체 구현합니다. (외부 렌더러 의존 없음)

- HWP 시험지 파일을 HWPX로 변환
- 시험지 형태의 한글 파일을 개별 문항으로 분해 (국어는 세트 단위)
- 한글 수식을 AI가 정확하게 읽을 수 있는 LaTeX로 변환
- 문항 속 이미지를 그대로 추출해 base64로 임베드
- 문항을 (수식·이미지 포함) AI 입력용 JSON으로 추출
- 페이지/문항 단위 미리보기를 SVG/PNG/PDF로 반환
- 한컴이 아닌 툴로 손상된 문서 자동 감지

## 설치

- 운영체제 : Windows / macOS / Linux
- Python 3.10 이상

OS에 따라 아래 커맨드를 터미널에서 복사하여 실행하세요. 빌드 도구(Rust 등)는 필요 없습니다.

**Windows (x64)**

```bash
pip install https://github.com/wnsgml9807/kdsnr-hwp-toolkit/releases/download/v0.2.2/kdsnr_hwp_toolkit-0.2.2-cp310-abi3-win_amd64.whl
```

**macOS (Apple Silicon)**

```bash
pip install https://github.com/wnsgml9807/kdsnr-hwp-toolkit/releases/download/v0.2.2/kdsnr_hwp_toolkit-0.2.2-cp310-abi3-macosx_11_0_arm64.whl
```

**Linux (x64)** — 한컴 오피스가 없어 글꼴 자동 수집이 안 되므로 [폰트](#폰트) 파일을 수동으로 복사해야 합니다.

```bash
pip install https://github.com/wnsgml9807/kdsnr-hwp-toolkit/releases/download/v0.2.2/kdsnr_hwp_toolkit-0.2.2-cp310-abi3-manylinux_2_17_x86_64.manylinux2014_x86_64.whl
```

### 폰트

렌더링에는 한컴 글꼴이 필요합니다. 글꼴은 **재배포하지 않으며**, 실행 시 사용자의
한컴 오피스 설치본에서 수집합니다. 글리프는 처음 한 번만 디코딩되어 사용자 캐시에
저장됩니다(이후 실행은 즉시 로드).

글꼴은 패키지에 동봉된 **`.fonts/` 폴더**(설치 위치 기준 `kdsnr_hwp_toolkit/.fonts/`)에
모입니다. 이 폴더가 기본 글꼴 폴더이며, 폴더는 비어 있는 상태로 배포됩니다(글꼴 파일은
미동봉). 다른 위치를 쓰려면 `FONT_DIR` 환경변수로 덮어쓸 수 있습니다.

- `FONT_DIR` : 글꼴 폴더 (미설정 시 동봉된 `.fonts/`)
- `HANCOM_PATH` : 한컴 오피스 설치 경로 (글꼴 수집 원본)

`export_preview`는 렌더 전 필요한 글꼴을 확인하고, 없으면 **Windows/macOS에서는
한컴 설치본에서 자동 수집**해 `.fonts/`에 채웁니다. 끝내 찾지 못하면 누락 글꼴 목록과
함께 `ValueError`를 반환합니다.

**수동 추가 (Linux 등)** — 한컴이 없어 자동 수집이 안 되면, 오류 메시지에 나온 글꼴
파일을 `.fonts/` 폴더(또는 `FONT_DIR`)에 직접 복사하세요.

```bash
# 동봉된 .fonts/ 위치는 설치된 패키지 기준입니다.
FONT_DIR=$(python -c "import kdsnr_hwp_toolkit, pathlib; print(pathlib.Path(kdsnr_hwp_toolkit.__file__).parent / '.fonts')")
cp HCRBatang.ttf HCRDotum.ttf  ...  "$FONT_DIR"/   # 누락 목록의 파일들
```

## 모델

모든 API는 단일 모델 `Document` 하나를 주고받습니다. `import_file`로 만들고,
변환·분해·렌더·저장에 그대로 넘깁니다.

```python
import kdsnr_hwp_toolkit as k

doc = k.import_file("exam.hwpx")     # -> Document (손상 문서면 ValueError)
doc.source_format                    # "hwp" | "hwpx"
```

| 속성 | 타입 | 설명 |
| --- | --- | --- |
| `source_format` | `str` | 불러온 컨테이너: `"hwp"` \| `"hwpx"` \| `"unknown"` |
| `section_count` | `int` | 본문 구역 수 |
| `label` | `str \| None` | 분할 문항일 때의 라벨, 아니면 `None` |

## 사용법

### 1. 불러오기 / 저장

**`import_file(path)`** — 파일을 열어 문서 모델(`Document`)로 만듭니다.

컨테이너 종류(HWP/HWPX)는 확장자가 아니라 바이트로 판별하므로, 확장자가 틀려도 올바르게 읽습니다.

| 인자 | 타입 | 설명 |
| --- | --- | --- |
| `path` | `str \| Path` | 입력 파일 경로 |
| **반환** | `Document` | 문서 모델 |

**`save_file(doc, path, file_type=None)`** — 문서 모델을 파일로 저장합니다.

형식은 `file_type`으로 지정하고, 주지 않으면 경로의 확장자(`.hwp` / `.hwpx`)로 추론합니다.

| 인자 | 타입 | 설명 |
| --- | --- | --- |
| `doc` | `Document` | 저장할 문서 |
| `path` | `str \| Path` | 출력 경로 |
| `file_type` | `"hwp" \| "hwpx" \| None` | `None`이면 확장자로 추론 |
| **반환** | `str` | 저장된 경로 |

**`is_corrupt(doc)`** — 문서가 손상된 형태인지 검사합니다.

한컴이 아닌 툴로 변형·편집되어 레이아웃이 깨진 파일이면 `True`를 돌려줍니다. 이런 문서는
`import_file` 단계에서 이미 `ValueError`로 막히며, 이 함수로 예외 없이 직접 확인할 수 있습니다.

| 인자 | 타입 | 설명 |
| --- | --- | --- |
| `doc` | `Document` | 검사할 문서 |
| **반환** | `bool` | 손상 형태면 `True` (예외 없음) |

```python
doc = k.import_file("exam.hwp")          # 컨테이너(hwp/hwpx)는 자동 판별
k.save_file(doc, "out.hwpx")             # 확장자로 형식 추론

# 손상 문서는 import_file 이 ValueError:
# "[KDSNR-HWP-TOOLKIT] 한컴이 아닌 다른 툴에 의해 변형되거나 편집되어 손상된 문서입니다. 변환이 불가능합니다."
```

### 2. HWP → HWPX 변환

**`hwp_to_hwpx(doc)`** — HWP 문서를 HWPX 형식으로 바꿉니다.

내용(모델)은 그대로 두고 포맷 태그만 전환하며, 실제 컨테이너 변환은 `save_file` 시점에
일어납니다. `save_file`에 `file_type`이나 확장자를 직접 주면 이 전환 없이 바로 저장할 수도 있습니다.

| 인자 | 타입 | 설명 |
| --- | --- | --- |
| `doc` | `Document` | 원본 HWP 문서 |
| **반환** | `Document` | HWPX 태그로 전환된 새 문서 (내용 동일) |

```python
doc = k.import_file("input.hwp")
hwpx_doc = k.hwp_to_hwpx(doc)            # HWPX 태그로 전환
k.save_file(hwpx_doc, "out.hwpx")
```

> **HWPX → HWP 저장은 비활성화되어 있습니다.** 일부 그림/표 레코드가 한컴의 HWP
> 열기 검증을 아직 통과하지 못하는 케이스가 있어, 현재 버전에서는 `hwpx_to_hwp(doc)`와
> HWPX 원본의 `.hwp` 저장을 `ValueError`로 막습니다. HWPX 저장과 문항 분해/렌더링을
> 사용하세요.

### 3. 개별 문항 분해

**`split_set_to_question(doc)`** — 시험지 한 부를 문항별 문서로 나눕니다.

원본 순서를 유지하며, 각 결과는 바로 저장·렌더할 수 있습니다. 지원 과목은
수학/과학/사회/국어입니다. 국어는 문항 하나가 아니라 세트 발문·지문·부속 문항을 묶은
세트 단위로 분해합니다.

| 인자 | 타입 | 설명 |
| --- | --- | --- |
| `doc` | `Document` | 시험지 한 부 |
| **반환** | `list[Document]` | 문항별 문서 (각 `label`에 문항 라벨) |

```python
questions = k.split_set_to_question(doc)   # -> list[Document]
for q in questions:
    print(q.label)                         # 문항 라벨
    k.save_file(q, f"out/{q.label}.hwpx")

# 국어 시험지:
# label 예: "S01-03" (세트 발문 + 지문 + 1~3번 문항)
```

### 4. 미리보기 렌더링

**`export_preview(docs, save_path, preview_type="page", media_types=None, dpi=200)`** — 문서를 이미지/PDF 미리보기로 내보냅니다.

페이지 단위 또는 문항 단위로 SVG/PNG/PDF를 생성합니다. 렌더 전 필요한 글꼴을 먼저 확인·수집하고,
없으면 누락 목록과 함께 `ValueError`를 냅니다.

| 인자 | 타입 | 설명 |
| --- | --- | --- |
| `docs` | `list[Document]` | 렌더할 문서들 |
| `save_path` | `str \| Path` | 출력 폴더 |
| `preview_type` | `"page" \| "question"` | `page`=조판된 페이지마다 한 장, `question`=문항 분해 후 문항별 렌더 |
| `media_types` | `list[str]` | `"svg" \| "png" \| "pdf"` 택일·복수 (기본 `["png"]`) |
| `dpi` | `float` | PNG 해상도 (기본 200, SVG/PDF는 무관) |
| **반환** | `list[list[str]]` | 확장자별 경로. 바깥=`media_types` 순서, 안쪽=그 확장자의 전체 경로(문서·페이지 평탄화) |

`dpi`는 PNG 래스터 해상도입니다. 벡터 트리를 해당 해상도로 직접 렌더하므로
확대로 인한 화질 저하가 없습니다(SVG/PDF는 벡터라 `dpi`와 무관). 기본값 200은
화면·인쇄 미리보기에 충분하며, 더 선명한 출력이 필요하면 300을 씁니다.

```python
# 페이지 단위
k.export_preview([doc], "out/", preview_type="page", media_types=["png"])

# 문항 단위 (입력을 문항으로 분해한 뒤 문항별로 렌더)
paths = k.export_preview(
    [doc],
    "out/",
    preview_type="question",         # "page" | "question"
    media_types=["png", "pdf"],      # "svg" | "png" | "pdf"
    dpi=200,                          # PNG 해상도 (기본 200)
)
# 반환: 확장자별 경로 리스트 (바깥 = media_types 순서)
pngs, pdfs = paths
```

### 5. 문항 JSON 추출 (AI 입력용)

**`extract_questions(doc, image_max_px=1024)`** — 시험지를 문항별 JSON으로 추출합니다.

각 문항을 `{label, subject, text, images}` 딕셔너리로 돌려줍니다. 본문의 수식은 LaTeX(`$...$`)로
변환되어 제 위치에 들어가고, 문항 안의 이미지는 축소되어 base64 `data:` URI로 담깁니다.
`json.dumps`로 직렬화해 LLM 입력으로 쓸 수 있습니다.

| 인자 | 타입 | 설명 |
| --- | --- | --- |
| `doc` | `Document` | 시험지 한 부 |
| `image_max_px` | `int` | 이미지 최대 변 길이(px). 더 길면 비율 유지하며 축소 (기본 1024) |
| **반환** | `list[dict]` | 문항별 `{label, subject, text, images}` |

| 키 | 타입 | 설명 |
| --- | --- | --- |
| `label` | `str` | 문항 라벨 (예: `"Q01"`) |
| `subject` | `str` | `"math" \| "science" \| "social" \| "korean"` |
| `text` | `str` | 문항 본문. 수식은 `$LaTeX$`로 인라인 변환 |
| `images` | `list[str]` | 문항 내 이미지의 base64 `data:` URI 목록 |

```python
import json

questions = k.extract_questions(doc)        # -> list[dict]
for q in questions:
    print(q["label"], q["subject"], len(q["images"]))
    print(q["text"])                        # 수식이 $LaTeX$로 들어간 본문

json.dumps(questions, ensure_ascii=False)   # LLM 입력으로

# 국어 시험지는 세트 단위로 추출됩니다.
```

분해·미리보기와 동일하게 수학/과학/사회/국어를 지원하며, 국어는 세트 단위로 반환합니다.

<p align="center">
  <img src="assets/logo.png" alt="강남대성수능연구소" height="72">
</p>

# HWP 툴킷

시험지 형식의 HWP 파일을 한컴 COM API 없이 가공하는 도구입니다.
HWP/HWPX 파일을 읽어 문항으로 분해하고, 웹 뷰어에서 쓸 수 있는 미리보기(SVG/PNG/PDF)를 반환합니다.
Windows/MacOS/Linux 등 다양한 환경에서 사용할 수 있습니다.

- 제작 : (주)강남대성수능연구소
- 담당자 : 권준희

공개된 한컴 사양 문서를 바탕으로, 메모리에 드나드는 한컴 문서 구조와 어셈블리를 분석해 Rust로 포팅한 자체 엔진입니다.
HWP/HWPX 파싱·직렬화, 레이아웃 조판, 문항 분해, 미리보기 렌더링을 모두 자체 구현합니다. (외부 렌더러 의존 없음)

- HWP 시험지 파일을 HWPX로 변환
- 시험지 형태의 한글 파일을 개별 문항으로 분해
- 페이지/문항 단위 미리보기를 SVG/PNG/PDF로 반환
- 한컴이 아닌 툴로 손상된 문서 자동 감지

## 설치

Python 3.10 이상이 필요합니다.

```bash
git clone https://github.com/wnsgml9807/kdsnr-hwp-toolkit.git
cd kdsnr-hwp-toolkit
python -m pip install -e .
```

## 모델

모든 API는 단일 모델 `Document` 하나를 주고받습니다. `import_file`로 만들고,
변환·분해·렌더·저장에 그대로 넘깁니다.

```python
import kdsnr_hwp_toolkit as k

doc = k.import_file("exam.hwpx")     # -> Document (손상 문서면 ValueError)
doc.source_format                    # "hwp" | "hwpx"
```

## 사용법

### 1. 불러오기 / 저장

```python
doc = k.import_file("exam.hwp")          # 컨테이너(hwp/hwpx)는 자동 판별
k.save_file(doc, "out.hwpx")             # 확장자로 형식 추론 (.hwp / .hwpx)
k.save_file(doc, "out.hwp", file_type="hwp")
```

손상된 문서(한컴이 아닌 툴로 변형·편집되어 레이아웃이 깨진 파일)는 불러올 때
다음 오류를 반환합니다.

```python
ValueError("[KDSNR-HWP-TOOLKIT] 한컴이 아닌 다른 툴에 의해 변형되거나 편집되어 손상된 문서입니다. 변환이 불가능합니다.")
```

### 2. HWP → HWPX

```python
doc = k.import_file("input.hwp")
hwpx_doc = k.hwp_to_hwpx(doc)            # 포맷 태그 전환 (실제 직렬화는 save_file에서)
k.save_file(hwpx_doc, "out.hwpx")
```

### 3. 개별 문항 분해

```python
questions = k.split_set_to_question(doc)   # -> list[Document]
for q in questions:
    print(q.label)                         # 문항 라벨
    k.save_file(q, f"out/{q.label}.hwpx")
```

현재 지원 과목은 수학/과학/사회입니다. 국어 시험지는 다음 오류를 반환합니다.

```python
ValueError("[KDSNR-HWP-TOOLKIT] 국어 과목은 문항별 분할과 미리보기를 지원하지 않습니다. (다음 버전 예정)")
```

### 4. 미리보기 렌더링

```python
# 페이지 단위
k.export_preview([doc], "out/", preview_type="page", media_types=["png"])

# 문항 단위 (입력을 문항으로 분해한 뒤 문항별로 렌더)
paths = k.export_preview(
    [doc],
    "out/",
    preview_type="question",         # "page" | "question"
    media_types=["png", "pdf"],      # "svg" | "png" | "pdf"
    scale=1.5,                        # PNG 래스터 배율
)
# 반환: 확장자별 경로 리스트 (바깥 = media_types 순서)
pngs, pdfs = paths
```

## 폰트

렌더링에는 한컴 글꼴이 필요합니다. 글꼴은 **재배포하지 않으며**, 실행 시 사용자의
한컴 오피스 설치본에서 수집합니다. 글리프는 처음 한 번만 디코딩되어 사용자 캐시에
저장됩니다(이후 실행은 즉시 로드).

- `FONT_DIR` : 글꼴/매핑이 위치한 폴더
- `HANCOM_PATH` : 한컴 오피스 설치 경로 (글꼴 수집 원본)

필요한 글꼴 파일이 없으면 Windows/macOS에서는 설치본에서 자동 수집하고,
끝내 찾지 못하면 누락 글꼴 목록과 함께 `ValueError`를 반환합니다.

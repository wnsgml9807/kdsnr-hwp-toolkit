#  HWP 툴킷 

시험지 형식의 HWP파일을 한컴 COM API 없이 가공하는 도구입니다.
HWP 파일에서 문항을 추출하여, 웹 뷰어에서 사용할 수 있는 미리보기 파일을  반환합니다.
Windows/MacOS/Linux 등 다양한 환경에서 사용할 수 있습니다.
한컴 어셈블리를 메모리에서 추출해 Rust로 포팅하여 만들었습니다.
외부 유출에 각별히 유의해 주시길 부탁드립니다. 

- HWP 시험지 파일을 HWPX 파일로 변환
- 시험지 형태의 한글 파일을 개별 문항으로 분해 + 개별 HWPX/PNG/PDF로 반환

## 사용법

Python 3.10 이상이 필요합니다.

```bash
pip install kdsnr-hwp-toolkit
```

### 1. HWP → HWPX

```python
from kdsnr_hwp_toolkit import hwp_to_hwpx

hwpx_path = hwp_to_hwpx(
    input_hwp_path="input.hwp",
    output_hwpx_dir="out/hwpx",
)

print(hwpx_path)
```

### 2. 개별 문항 분해 + 미리보기 파일

```python
from kdsnr_hwp_toolkit import split_set_to_question

question_paths = split_set_to_question(
    "math_input.hwp",              # HWP가 들어오면 내부에서 자동으로 HWPX 변환
    output_dir="out/questions",
    preview_type=["png", "pdf"],
    crop=True,                     # False면 모든 미리보기를 전체 페이지로 생성
    preview_workers=64,            # 기본값
)

print(question_paths)
```

`output_dir`는 필수입니다. `preview_type`은 `"png"`, `"pdf"` 중 하나 또는 리스트를 받습니다.
같은 형식이 중복되어도 한 번만 생성됩니다.

현재 지원 과목은 수학/과학/사회입니다. 국어 시험지는 자동 감지 후 다음 오류를 반환합니다.

```python
ValueError("국어 과목은 아직 지원하지 않습니다")
```

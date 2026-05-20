"""paragraph.linesegs_xml 채움 — splitter 가 변형한 paragraph 의 lineseg 생성.

원칙:
  - 한컴 원본 lineseg 가 살아 있으면 보존 (정답 그대로)
  - splitter 가 paragraph 를 분리/변형해서 linesegs_xml 비어 있는 경우만 새로 생성
  - cell.height 결정은 책임 X — cell_height.py 가 담당
  - inline 보정 책임 X — inline_correction.py 가 담당

현재 thin wrapper. 내부는 기존 lineseg_gen.py 의 검증된 함수 호출.
점진적으로 lineseg_gen.py 내부 정리해서 이 모듈로 흡수.
"""
from .. import lineseg_gen as _legacy


def fill_missing(doc):
    """doc 의 모든 paragraph (top-level + cell 안 재귀) 의 lineseg 를 채움.

    기존 동작 보존:
      - 한컴 원본 linesegs_xml 있으면 보존
      - splitter 가 비운 paragraph 만 새로 생성
      - cell paragraph 의 lineseg 는 cell 내부 width 기반으로 생성

    Side effect: 기존 enrich_linesegs 가 cell.height 와 inline 보정도 같이 함.
    그 책임들은 cell_height.py / inline_correction.py 가 enrich.py 에서 별도로
    호출. 점진적 마이그레이션 중에는 enrich_linesegs 내부의 cell_height/correction
    도 함께 실행됨 (idempotent — 다시 호출해도 같은 결과).
    """
    return _legacy.enrich_linesegs(doc)

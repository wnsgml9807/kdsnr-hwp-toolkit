"""layout 단계 entry point — splitter 이후 IR 의 lineseg/보정 일괄 처리.

흐름:
    enrich_doc(doc)
        ├── inline_correction.apply(doc) — rhwp quirk 보정 (affectLSpacing 등)
        ├── linesegs.fill_missing(doc)   — paragraph.linesegs_xml 채움
        └── inline_correction.apply(doc) — fill_missing 후 다시 보정 (idempotent)

cell.height 결정은 호출하지 않는다. cell_height.resolve 가 stored cellSz 를
content_bottom + margin 으로 부풀리면 (예: 282 → 1586) rhwp 가 동일 cell 에
서 동일 vertAlign=CENTER 콘텐츠를 다른 행 높이로 잘라 렌더한다 (사이언스
Q14 (가)(나)(다) 박스 마지막 행 잘림 회귀). 한컴 원본 placeholder cellSz 는
rhwp 의 런타임 reflow 가 정확히 처리하므로 손대지 않는다.
"""
from . import linesegs, inline_correction


def enrich_doc(doc):
    doc = inline_correction.apply(doc)
    doc = linesegs.fill_missing(doc)
    doc = inline_correction.apply(doc)
    return doc

"""Layout 단계 — splitter 이후 IR 의 lineseg/cell.height/inline 보정 책임 분리.

Pipeline 흐름:
    codec.read → splitter → layout.enrich_doc → codec.write → rhwp

layout.enrich_doc(doc) 의 단계:
    1. linesegs.fill_missing(doc)     — paragraph.linesegs_xml 비어있는 것만 채움
    2. cell_height.resolve(doc)       — cell.cellSz.height 결정 (row max 통일)
    3. inline_correction.apply(doc)   — rhwp quirk 보정 (필요시만)

각 모듈은 단일 책임. 다른 모듈의 출력에만 의존.
"""
from .enrich import enrich_doc

__all__ = ["enrich_doc"]

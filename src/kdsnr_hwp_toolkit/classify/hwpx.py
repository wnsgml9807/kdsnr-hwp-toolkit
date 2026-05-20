from __future__ import annotations

import re

from kdsnr_hwp_toolkit.codec.schema import (
    CharItem,
    LineBreakItem,
    OpaqueInlineItem,
    Paragraph,
    TableItem,
)
from kdsnr_hwp_toolkit.core.model import Atom, ClassifiedAtom, Role

_BALMUN_RE = re.compile(r"^\s*(\d+)\s*\.(?!\d)")
_SEONJI_RE = re.compile(r"^\s*[①-⑤]")


def paragraph_text(p: Paragraph) -> str:
    return "".join(it.text for it in p.items if isinstance(it, CharItem))


def is_bogi_table(tbl: TableItem) -> bool:
    text = ""
    for c in tbl.cells:
        for p in c.paragraphs:
            text += paragraph_text(p)
    normalized = re.sub(r"\s+", "", text)
    if "보기" not in normalized:
        return False
    row_count = int(tbl.table_attrs.get("rowCnt", "0"))
    col_count = int(tbl.table_attrs.get("colCnt", "0"))
    return row_count >= 3 and col_count >= 3


def classify_hwpx_atom(atom: Atom) -> ClassifiedAtom:
    p = atom.payload
    if not isinstance(p, Paragraph):
        return ClassifiedAtom(atom=atom, role=Role.UNKNOWN, confidence=0.0)

    text = paragraph_text(p).strip()
    tables = [it for it in p.items if isinstance(it, TableItem)]

    if any(is_bogi_table(t) for t in tables):
        return ClassifiedAtom(atom=atom, role=Role.BOGI_BOX, reasons=("bogi-table",))

    if len(tables) == 1 and str(tables[0].table_attrs.get("rowCnt")) == "1" and str(tables[0].table_attrs.get("colCnt")) == "1":
        return ClassifiedAtom(atom=atom, role=Role.DATA_BOX, reasons=("single-cell-table",))

    if _BALMUN_RE.match(text):
        return ClassifiedAtom(atom=atom, role=Role.STEM, reasons=("balmun-regex",))

    if _SEONJI_RE.match(text):
        return ClassifiedAtom(atom=atom, role=Role.CHOICES, reasons=("choice-regex",))

    if any(isinstance(it, OpaqueInlineItem) and "pic" in it.tag for it in p.items):
        return ClassifiedAtom(atom=atom, role=Role.PICTURE_BLOCK, reasons=("picture",))

    if any(isinstance(it, OpaqueInlineItem) and "equation" in it.tag for it in p.items):
        return ClassifiedAtom(atom=atom, role=Role.EQUATION_BLOCK, reasons=("equation",))

    if any(isinstance(it, LineBreakItem) for it in p.items):
        return ClassifiedAtom(atom=atom, role=Role.STEM_CONTINUATION, reasons=("linebreak-text",))

    if text:
        return ClassifiedAtom(atom=atom, role=Role.STEM_CONTINUATION, reasons=("text",))

    return ClassifiedAtom(atom=atom, role=Role.UNKNOWN, confidence=0.5)

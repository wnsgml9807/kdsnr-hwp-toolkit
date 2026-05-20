"""Atom classifier — content-based, with paraPr fallback hints.

Classifies one src paragraph into an Atom kind. Needs the previous atom for
context (balmun_cont detection, korean dialog vs jimun, etc).

Rules verified against templet/{math,science,social,korean}.hwpx audit
(2026-05-11). See work/atom_slot_audit.txt for evidence.
"""
from __future__ import annotations

import re

from kdsnr_hwp_toolkit.codec.schema import (
    CharItem,
    OpaqueInlineItem,
    Paragraph,
    TableItem,
)

from .atoms import Atom


_BALMUN_RE = re.compile(r"^\s*(\d+)\s*\.(?!\d)")
_SEONJI_RE = re.compile(r"^\s*[①-⑤]")
_SET_HEADER_RE = re.compile(r"\[\s*\d+\s*[~∼～]\s*\d+\s*\]")
_DIALOG_RE = re.compile(r'^\s*[“"]')
_FOOTNOTE_RE = re.compile(r"^\s*\*[^\d]")
_AUTHOR_RE = re.compile(r"-\s*[^,]+,\s*「")
_JUNGRYAK_RE = re.compile(r"\(중략\)")

# korean balmun in templet has no "N." prefix — paraPr (43,3) hint
_KOREAN_BALMUN_PARAPR = (43, 3)


def _text(p: Paragraph) -> str:
    return "".join(it.text for it in p.items if isinstance(it, CharItem))


def _has_equation(p: Paragraph) -> bool:
    return any(
        isinstance(it, OpaqueInlineItem) and "equation" in it.tag
        for it in p.items
    )


def _has_picture(p: Paragraph) -> bool:
    return any(
        isinstance(it, OpaqueInlineItem) and "pic" in it.tag for it in p.items
    )


def _first_table(p: Paragraph) -> TableItem | None:
    for it in p.items:
        if isinstance(it, TableItem):
            return it
    return None


def _is_empty(p: Paragraph) -> bool:
    txt = _text(p).strip()
    if txt:
        return False
    if _first_table(p) is not None:
        return False
    if _has_equation(p) or _has_picture(p):
        return False
    return True


def _is_bogi_table(tbl: TableItem) -> bool:
    rc = int(tbl.table_attrs.get("rowCnt", "0"))
    cc = int(tbl.table_attrs.get("colCnt", "0"))
    if rc < 3 or cc < 3:
        return False
    for cell in tbl.cells:
        text = "".join(
            "".join(it.text for it in cp.items if isinstance(it, CharItem))
            for cp in cell.paragraphs
        )
        if "보기" in re.sub(r"\s+", "", text):
            return True
    return False


def classify(p: Paragraph, *, prev_atom: Atom | None, subject: str) -> Atom:
    """Classify a paragraph into an Atom.

    `prev_atom` is the most recent non-empty/non-unknown atom in the same
    question unit. Used for balmun_cont, jimun_dialog vs jimun, and
    jimun_data_box (table inside passage) detection.
    """
    if _is_empty(p):
        return Atom.EMPTY

    txt = _text(p)
    txt_strip = txt.strip()
    tbl = _first_table(p)
    pp_st = (p.para_shape_id, p.style_id)

    # ── Box-shaped paragraphs ────────────────────────────────
    if tbl is not None:
        if _is_bogi_table(tbl):
            return Atom.BOGI_BOX
        if subject == "korean":
            if _SET_HEADER_RE.search(txt):
                return Atom.SET_HEADER
            if prev_atom in (
                Atom.JIMUN, Atom.JIMUN_DIALOG, Atom.JIMUN_BRACKET,
                Atom.JIMUN_DATA_BOX, Atom.JIMUN_INLINE_TABLE,
            ):
                rc = int(tbl.table_attrs.get("rowCnt", "0"))
                cc = int(tbl.table_attrs.get("colCnt", "0"))
                if rc == 1 and cc == 1:
                    return Atom.JIMUN_DATA_BOX
                return Atom.JIMUN_INLINE_TABLE
        return Atom.DATA_BOX

    if _has_picture(p) and not txt_strip:
        # picture-only paragraph — uses dedicated centered slot
        return Atom.PIC_BLOCK

    # ── Korean text paragraphs ───────────────────────────────
    if subject == "korean":
        if _SET_HEADER_RE.search(txt):
            return Atom.SET_HEADER
        if _JUNGRYAK_RE.search(txt):
            return Atom.JUNGRYAK
        if _AUTHOR_RE.search(txt):
            return Atom.AUTHOR_CREDIT
        if _FOOTNOTE_RE.match(txt):
            return Atom.FOOTNOTE
        if _BALMUN_RE.match(txt) or pp_st == _KOREAN_BALMUN_PARAPR:
            return Atom.BALMUN
        if _SEONJI_RE.match(txt):
            return Atom.SEONJI
        if _DIALOG_RE.match(txt):
            return Atom.JIMUN_DIALOG
        if p.para_shape_id == 47:  # bracket interior style
            return Atom.JIMUN_BRACKET
        return Atom.JIMUN

    # ── math / science / social ──────────────────────────────
    if _BALMUN_RE.match(txt):
        return Atom.BALMUN
    if _SEONJI_RE.match(txt):
        # 2-row seonji: contains a LineBreak (한컴 자동 줄바꿈으론 정렬이
        # 왼쪽으로 치우치므로 명시적으로 2행 슬롯을 사용해야 함)
        from kdsnr_hwp_toolkit.codec.schema import LineBreakItem
        if any(isinstance(it, LineBreakItem) for it in p.items):
            return Atom.SEONJI_2ROW
        return Atom.SEONJI
    # eq_block: equation present, text only punctuation/whitespace/no Hangul/Latin
    if _has_equation(p) and not re.search(r"[가-힣A-Za-z]", txt):
        return Atom.EQ_BLOCK
    # eq_block paraPr hint (math): pp=6 st=2 → eq_block even if has Hangul ("(는 상수)")
    if subject == "math" and pp_st == (6, 2) and _has_equation(p):
        return Atom.EQ_BLOCK
    if _has_equation(p) or txt_strip:
        if prev_atom in (
            Atom.EQ_BLOCK, Atom.DATA_BOX, Atom.BOGI_BOX,
            Atom.BALMUN, Atom.BALMUN_CONT,
        ):
            return Atom.BALMUN_CONT
    return Atom.UNKNOWN


__all__ = ["classify"]

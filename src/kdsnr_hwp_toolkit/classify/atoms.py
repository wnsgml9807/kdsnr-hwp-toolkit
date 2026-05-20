"""Atom kind taxonomy.

17 kinds confirmed with user. The classifier returns one of these for every
src paragraph. The slot table (slot_catalog.py) maps (subject, atom) →
templet (pp, st, cs).

Kinds split into two transform paths:
  - BOX_ATOMS: templet box paragraph cloned wholesale, src content swapped
    into cell. Source content style preserved (sacred).
  - All others: src paragraph reused, templet (pp,st,cs) applied to outer +
    char_shape applied to runs.
"""
from __future__ import annotations

from enum import Enum


class Atom(str, Enum):
    # common (math/science/social)
    BALMUN = "balmun"
    BALMUN_CONT = "balmun_cont"
    EQ_BLOCK = "eq_block"
    DATA_BOX = "data_box"
    BOGI_BOX = "bogi_box"
    SEONJI = "seonji"
    SEONJI_2ROW = "seonji_2row"
    PIC_BLOCK = "pic_block"

    # korean
    SET_HEADER = "set_header"
    JIMUN = "jimun"
    JIMUN_DIALOG = "jimun_dialog"
    JIMUN_BRACKET = "jimun_bracket"
    JIMUN_DATA_BOX = "jimun_data_box"
    JIMUN_INLINE_TABLE = "jimun_inline_table"
    JUNGRYAK = "jungryak"
    AUTHOR_CREDIT = "author_credit"
    FOOTNOTE = "footnote"

    # internal markers (don't appear in output)
    EMPTY = "empty"
    UNKNOWN = "unknown"


# Only BOGI_BOX uses the templet wrapper-clone path (templet provides the
# 〈보 기〉 framed shell; src content cell goes inside).
# DATA_BOX / JIMUN_* keep their src table/picture as-is and only re-stamp
# the outer paragraph's paraPr/styleId/charPr to the templet slot —
# nesting them inside a templet wrapper produced "박스 안에 박스" artifacts.
BOX_ATOMS = frozenset({
    Atom.BOGI_BOX,
})


__all__ = ["Atom", "BOX_ATOMS"]

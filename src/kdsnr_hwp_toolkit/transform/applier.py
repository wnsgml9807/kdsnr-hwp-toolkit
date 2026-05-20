"""Apply slot styles to classified atoms.

Two paths (per ARCHITECTURE.md non-negotiable rule):

  1. BOX_ATOMS (data_box, bogi_box, jimun_data_box, jimun_inline_table):
     Clone the templet's box paragraph wholesale. Replace inner table cells
     with src content. Source content style preserved untouched.

  2. All other atoms:
     Take src paragraph as-is. Re-stamp outer (paraPr, styleId, char_shape)
     to the templet slot. Re-stamp every item's char_shape_id to slot.cs_id.

  Special case (balmun): templet's balmun paragraph carries TWO char_shapes
  — one for the question number ("N.") and one for the body text. We split
  the leading "N." run (or first char) into cs_id (number style) and apply
  body_cs_id to everything after.
"""
from __future__ import annotations

import re
from dataclasses import replace

from kdsnr_hwp_toolkit.codec.schema import (
    CharItem, EmptyRunItem, LineBreakItem, OpaqueInlineItem, Paragraph,
    TabItem, TableItem, CellItem,
)
from kdsnr_hwp_toolkit.classify.atoms import Atom, BOX_ATOMS

from .slot_catalog import Slot


_QNUM_RE = re.compile(r"^(\s*\d+\s*\.\s*)")
# Strips full-width spaces (　) and any leading whitespace from the very
# first char of a balmun paragraph — src often carries leading 전각 공백 that
# breaks the templet's negative-indent (내어쓰기) for question numbers.
_BALMUN_LEADING_WS_RE = re.compile(r"^[\s　]+")


def _restamp_char_shape(item, cs_id: int):
    """Replace the char_shape_id on any item kind that carries one."""
    if isinstance(item, (CharItem, TabItem, LineBreakItem,
                         EmptyRunItem, OpaqueInlineItem, TableItem)):
        return replace(item, char_shape_id=cs_id)
    return item


_SEONJI_MARKER_RE = re.compile(r"^[①②③④⑤⑥⑦⑧⑨⑩]")


def _strip_seonji_tab(items: tuple, slot: Slot) -> tuple:
    """If items start with `[①…] + TabItem + …`, replace that TabItem with
    a single space char so the marker and content are visually separated
    but don't jump to the next tab stop column.

    src puts a tab between the marker and content. Templet's seonji slot has
    its own tab stops that align ① ② ③ ④ ⑤ as separate columns — keeping
    the tab makes the content jump to the ② column. Replacing with a single
    space gives "① ㉠은…" tight spacing.
    Multi-marker rows (templet-style "① ② ③ ④ ⑤") are not affected — their
    first marker is followed by another marker, not by content.
    """
    if len(items) < 2:
        return items
    if not isinstance(items[0], CharItem):
        return items
    if not _SEONJI_MARKER_RE.match(items[0].text):
        return items
    if not isinstance(items[1], TabItem):
        return items
    # Skip if this is "marker + tab + marker" (multi-column seonji row)
    if len(items) >= 3 and isinstance(items[2], CharItem):
        if _SEONJI_MARKER_RE.match(items[2].text):
            return items
    space = CharItem(
        text=" ",
        char_shape_id=items[1].char_shape_id,
        starts_new_run=True,
    )
    return (items[0], space) + tuple(items[2:])


def _apply_role_style(src: Paragraph, slot: Slot) -> Paragraph:
    """Non-box path: templet OWNS paragraph paraPr/style/cs and item cs.

    User policy (2026-05-11): outside box atoms, src's char_shape is replaced
    with templet's slot.cs_id (font + size unified). Items whose src cs
    differs from src's default cs are preserved untouched — those carry
    char-level emphasis (밑줄/bold/italic). normalize_styles still forces
    textColor=black + shadeColor=white globally.

    Box-internal font diversity is handled separately in _box_clone /
    _normalize_data_box_cells.
    """
    src_default_cs = src.char_shape_id_first
    items = src.items
    # Single-marker seonji ("① + tab + content") — drop the tab so the
    # marker doesn't jump to the ② tab-stop column.
    items = _strip_seonji_tab(items, slot)
    new_items = tuple(
        _restamp_char_shape(it, slot.cs_id)
        if getattr(it, "char_shape_id", None) == src_default_cs
        else it
        for it in items
    )
    # Preserve src linesegs_xml when the paragraph carries an inline visual
    # whose height must be honoured by the renderer (picture/equation/table
    # via OpaqueInlineItem). Hanword 12+ uses the cached lineseg vertsize for
    # the row height; clearing it collapses the row to default text height,
    # cropping pictures (e.g. Q13 그림 잘림). For text-only paragraphs we still
    # clear so the renderer recomputes against the new templet width.
    has_visual = any(
        isinstance(it, (OpaqueInlineItem, TableItem)) for it in items
    )
    # Visual paragraphs (picture/equation containers) keep src's paraPr as
    # well: templet slot paraPr (e.g. math PIC_BLOCK with `lineSpacing=60%`)
    # constrains line height too tight for the visual's actual vertsize.
    # Src's paraPr was authored to fit the visual it carries.
    if has_visual:
        return replace(
            src,
            items=new_items,
            starts_new_page=False,
            starts_new_column=False,
        )
    return replace(
        src,
        items=new_items,
        para_shape_id=slot.pp_id,
        style_id=slot.st_id,
        char_shape_id_first=slot.cs_id,
        starts_new_page=False,
        starts_new_column=False,
        linesegs_xml="",
    )


def _apply_balmun_style(src: Paragraph, slot: Slot) -> Paragraph:
    """Balmun path: split leading 'N.' run into qnum cs, rest into body cs.

    cs_id = number style (templet's first char_shape)
    body_cs_id = body style (templet's later char_shape, if present)

    If body_cs_id is None (single-cs templet), falls back to plain
    _apply_role_style behavior.
    """
    if slot.body_cs_id is None:
        return _apply_role_style(src, slot)

    qnum_cs = slot.cs_id
    body_cs = slot.body_cs_id
    src_default_cs = src.char_shape_id_first

    # Leading CharItems → match qnum across combined text (src may split
    # "10" / "." / " " into multiple CharItems, so single-item match fails).
    leading_indices: list[int] = []
    combined = ""
    for j, it in enumerate(src.items):
        if isinstance(it, CharItem):
            txt = it.text
            if not leading_indices:
                txt = _BALMUN_LEADING_WS_RE.sub("", txt)
            combined += txt
            leading_indices.append(j)
            if _QNUM_RE.match(combined):
                break
            if len(combined) > 12:
                break
        else:
            break

    qnum_match = _QNUM_RE.match(combined) if leading_indices else None
    qnum_consumed: set = set()
    new_items: list = []

    if qnum_match:
        qnum_text = qnum_match.group(1)
        first_it = src.items[leading_indices[0]]
        new_items.append(replace(
            first_it,
            text=qnum_text,
            char_shape_id=qnum_cs,
            starts_new_run=True,
        ))
        rest = combined[len(qnum_text):]
        if rest:
            new_items.append(replace(
                first_it,
                text=rest,
                char_shape_id=body_cs,
                starts_new_run=True,
            ))
        qnum_consumed = set(leading_indices)

    # Subsequent items: src cs == default → restamp with body_cs;
    # otherwise (emphasis cs) preserve as-is.
    # Also collapse TabItems immediately following the qnum — replace the
    # FIRST tab with a single space (so number and body are visually
    # separated), drop the rest. Templet's 내어쓰기 takes care of the
    # left margin alignment.
    after_qnum = qnum_match is not None
    inserted_separator = False
    for j, it in enumerate(src.items):
        if j in qnum_consumed:
            continue
        if after_qnum and isinstance(it, TabItem):
            if not inserted_separator:
                new_items.append(CharItem(
                    text=" ",
                    char_shape_id=body_cs if body_cs is not None else it.char_shape_id,
                    starts_new_run=True,
                ))
                inserted_separator = True
            continue
        if after_qnum and not isinstance(it, TabItem):
            after_qnum = False
        cs = getattr(it, "char_shape_id", None)
        if cs is not None and cs != src_default_cs:
            new_items.append(it)
        else:
            new_items.append(_restamp_char_shape(it, body_cs))

    return replace(
        src,
        items=tuple(new_items),
        para_shape_id=slot.pp_id,
        style_id=slot.st_id,
        char_shape_id_first=qnum_cs,
        starts_new_page=False,
        starts_new_column=False,
        linesegs_xml="",
    )


def _src_table(src: Paragraph) -> TableItem | None:
    for it in src.items:
        if isinstance(it, TableItem):
            return it
    return None


def _box_clone(src: Paragraph, slot: Slot) -> Paragraph:
    """Box path: clone templet box paragraph, swap cell content with src.

    Strategy:
      - Take templet's box paragraph (slot.template_paragraph) as the shell.
      - Find the templet's TableItem; find the cell that holds the largest
        text content (= the "content cell" — others are labels/spacers).
      - Replace that cell's paragraphs with src's table cell paragraphs OR
        with src's non-table paragraphs (image-only cases).
      - Preserve src content style.

    For 1×1 boxes (data_box wrapper, jimun_data_box), the only cell IS the
    content cell. Easy.

    For 3×3 bogi boxes, the content cell is typically cell index 5 (bottom
    row, last col) — verified from templet/{science,social,korean}.hwpx.
    """
    template_p = slot.template_paragraph
    assert template_p is not None, "BOX atom missing template_paragraph"

    template_tbl = next(
        (it for it in template_p.items if isinstance(it, TableItem)), None
    )
    assert template_tbl is not None, "BOX template paragraph has no table"

    # bogi_box is the only atom that uses this clone path (per BOX_ATOMS).
    # src is always an N×M table (3×3 typical). Unwrap its largest cell —
    # those are the actual 보기 항목 (ㄱ/ㄴ/ㄷ paragraphs). Nesting the whole
    # src table here would produce "보기 안에 보기" doubled-frame output.
    src_tbl = _src_table(src)
    if src_tbl is not None:
        src_content_paras = _largest_cell_paragraphs(src_tbl)
    else:
        src_content_paras = (src,)

    # Force src cell paragraphs to use templet's content-cell paraPr/style/
    # cs_first (paragraph layout). Per-item char_shape stays src (font
    # diversity preserved). Without paraPr normalization, src's paraPr
    # (e.g. pp=105 with TAB after ㄱ.) bleeds in and breaks 보기 spacing.
    template_content_cell = template_tbl.cells[_content_cell_index(template_tbl)]
    if template_content_cell.paragraphs:
        ref_p = template_content_cell.paragraphs[0]
        src_content_paras = tuple(
            replace(
                p,
                para_shape_id=ref_p.para_shape_id,
                style_id=ref_p.style_id,
                char_shape_id_first=ref_p.char_shape_id_first,
            )
            for p in src_content_paras
        )

    # Find the content cell in template table — heuristic: cell with the
    # most paragraphs OR the largest non-empty text.
    content_idx = _content_cell_index(template_tbl)

    new_cells = []
    for ci, cell in enumerate(template_tbl.cells):
        if ci == content_idx:
            new_cells.append(replace(cell, paragraphs=tuple(src_content_paras)))
        else:
            new_cells.append(cell)

    new_table = replace(template_tbl, cells=tuple(new_cells))
    new_items = tuple(
        new_table if isinstance(it, TableItem) else it
        for it in template_p.items
    )
    return template_p.with_items(new_items)


def _largest_cell_paragraphs(tbl: TableItem) -> tuple[Paragraph, ...]:
    """Pick the cell with the most non-empty content."""
    def cell_weight(cell: CellItem) -> int:
        return sum(
            len(it.text)
            for p in cell.paragraphs
            for it in p.items
            if isinstance(it, CharItem)
        ) + sum(
            10  # any non-text item is heavy
            for p in cell.paragraphs
            for it in p.items
            if not isinstance(it, (CharItem, EmptyRunItem))
        )

    if not tbl.cells:
        return ()
    best = max(tbl.cells, key=cell_weight)
    return tuple(best.paragraphs)


def _content_cell_index(tbl: TableItem) -> int:
    """Pick the cell that should hold src content."""
    if not tbl.cells:
        return 0
    weights = []
    for cell in tbl.cells:
        w = sum(
            len(it.text)
            for p in cell.paragraphs
            for it in p.items
            if isinstance(it, CharItem)
        ) + sum(
            10
            for p in cell.paragraphs
            for it in p.items
            if not isinstance(it, (CharItem, EmptyRunItem))
        )
        weights.append(w)
    return max(range(len(weights)), key=lambda i: weights[i])


def apply_atom(src: Paragraph, atom: Atom, slot: Slot) -> Paragraph:
    """Transform src paragraph into output paragraph using slot.

    DATA_BOX cells are NOT normalized — src paraPr/style/cs all preserved
    inside the box (per "박스 안 폰트만 src" policy: paraPr likewise).
    Only the OUTER paragraph wrapper (paraPr/style/cs_first) gets templet.
    """
    if atom in BOX_ATOMS:
        return _box_clone(src, slot)
    if atom == Atom.BALMUN:
        return _apply_balmun_style(src, slot)
    return _apply_role_style(src, slot)


__all__ = ["apply_atom"]

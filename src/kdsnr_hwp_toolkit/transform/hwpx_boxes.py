from __future__ import annotations

import re
from dataclasses import replace
from functools import lru_cache
from typing import Sequence

from kdsnr_hwp_toolkit.codec.read import _parse_table
from kdsnr_hwp_toolkit.codec.schema import (
    CellItem,
    CharItem,
    EmptyRunItem,
    Paragraph,
    TableItem,
)
from kdsnr_hwp_toolkit.core.model import Role
from kdsnr_hwp_toolkit.resources import template_text
from kdsnr_hwp_toolkit.transform.boxes import BoxShellTemplate, build_box_with_source_content


DATA_BOX_WIDTH = 31040
DATA_BOX_OUTMARGIN = (0, 0, 0, 566)
DATA_BOX_INMARGIN = (0, 0, 0, 0)
DATA_BOX_CELLMARGIN = (510, 510, 141, 141)

BOX_WRAP_PP = 1
BOX_WRAP_STYLE = 4
BOX_WRAP_CS = 18

BOGI_BOX_WIDTH = 30608
BOGI_BOX_OUTMARGIN = (0, 0, 0, 850)
BOGI_BOX_INMARGIN = (19, 0, 0, 0)
BOGI_FILLER_CM = (141, 141, 141, 141)
BOGI_CONTENT_CM = (850, 850, 708, 850)
BOGI_LEFT_W = 13355
BOGI_HEADER_W = 4189
BOGI_RIGHT_W = 13064
BOGI_ROW0_H = 574
BOGI_ROW1_H = 574
BOGI_HEADER_H = 1148

BOGI_BF_OUTER = 21
BOGI_BF_TL = 19
BOGI_BF_HEADER = 26
BOGI_BF_BL = 27
BOGI_BF_BR = 28
BOGI_BF_CONTENT = 29

BOGI_FILLER_PP = 77
BOGI_FILLER_STYLE = 46
BOGI_FILLER_CS = 113
BOGI_HEADER_PP = 78
BOGI_HEADER_STYLE = 46
BOGI_HEADER_CS = 112

CANONICAL_LINESEGS = (
    '<hp:linesegarray><hp:lineseg textpos="0" vertpos="0" '
    'vertsize="1000" textheight="1000" baseline="850" spacing="600" '
    'horzpos="0" horzsize="12964" flags="393216"/>'
    '</hp:linesegarray>'
)


def _table_attrs(row_count: int, col_count: int, border_fill_id: int) -> dict[str, str]:
    return {
        "id": "0",
        "zOrder": "0",
        "numberingType": "TABLE",
        "textWrap": "TOP_AND_BOTTOM",
        "textFlow": "BOTH_SIDES",
        "lock": "0",
        "dropcapstyle": "None",
        "pageBreak": "NONE",
        "repeatHeader": "1",
        "rowCnt": str(row_count),
        "colCnt": str(col_count),
        "cellSpacing": "0",
        "borderFillIDRef": str(border_fill_id),
        "noAdjust": "0",
    }


def _pre_rows_xml(width: int, height: int, outmargin: tuple[int, int, int, int], inmargin: tuple[int, int, int, int]) -> str:
    om_l, om_r, om_t, om_b = outmargin
    im_l, im_r, im_t, im_b = inmargin
    return (
        f'<hp:sz width="{width}" widthRelTo="ABSOLUTE" '
        f'height="{height}" heightRelTo="ABSOLUTE" protect="0"/>'
        '<hp:pos treatAsChar="1" affectLSpacing="0" flowWithText="1" '
        'allowOverlap="0" holdAnchorAndSO="0" vertRelTo="PARA" '
        'horzRelTo="PARA" vertAlign="TOP" horzAlign="LEFT" '
        'vertOffset="0" horzOffset="0"/>'
        f'<hp:outMargin left="{om_l}" right="{om_r}" top="{om_t}" bottom="{om_b}"/>'
        f'<hp:inMargin left="{im_l}" right="{im_r}" top="{im_t}" bottom="{im_b}"/>'
    )


def _cell_meta(col: int, row: int, col_span: int, row_span: int, width: int, height: int, margin: tuple[int, int, int, int]) -> str:
    ml, mr, mt, mb = margin
    return (
        f'<hp:cellAddr colAddr="{col}" rowAddr="{row}"/>'
        f'<hp:cellSpan colSpan="{col_span}" rowSpan="{row_span}"/>'
        f'<hp:cellSz width="{width}" height="{height}"/>'
        f'<hp:cellMargin left="{ml}" right="{mr}" top="{mt}" bottom="{mb}"/>'
    )


def _cell_attrs(border_fill_id: int) -> dict[str, str]:
    return {
        "name": "",
        "header": "0",
        "hasMargin": "0",
        "protect": "0",
        "editable": "0",
        "dirty": "0",
        "borderFillIDRef": str(border_fill_id),
    }


def _sublist(align: str = "TOP") -> dict[str, str]:
    return {
        "id": "",
        "textDirection": "HORIZONTAL",
        "lineWrap": "BREAK",
        "vertAlign": align,
        "linkListIDRef": "0",
        "linkListNextIDRef": "0",
        "textWidth": "0",
        "textHeight": "0",
        "hasTextRef": "0",
        "hasNumRef": "0",
    }


def _filler() -> Paragraph:
    return Paragraph(
        items=(CharItem(text="", char_shape_id=BOGI_FILLER_CS, starts_new_run=True),),
        para_shape_id=BOGI_FILLER_PP,
        style_id=BOGI_FILLER_STYLE,
        char_shape_id_first=BOGI_FILLER_CS,
        para_id_attr="0",
        linesegs_xml=CANONICAL_LINESEGS,
    )


def _bogi_label() -> Paragraph:
    return Paragraph(
        items=(CharItem(text="<보 기>", char_shape_id=BOGI_HEADER_CS, starts_new_run=True),),
        para_shape_id=BOGI_HEADER_PP,
        style_id=BOGI_HEADER_STYLE,
        char_shape_id_first=BOGI_HEADER_CS,
        para_id_attr="0",
        linesegs_xml=CANONICAL_LINESEGS,
    )


def _empty_source_content() -> Paragraph:
    return Paragraph(
        items=(EmptyRunItem(char_shape_id=0, starts_new_run=True),),
        para_shape_id=0,
        style_id=0,
        char_shape_id_first=0,
        para_id_attr="0",
        linesegs_xml="",
    )


def _content_height(paragraphs: Sequence[Paragraph], minimum: int = 1150) -> int:
    max_bottom = 0
    has_accumulated_vertpos = False
    sum_heights = 0
    for p in paragraphs:
        para_height = 0
        for vp, th in re.findall(
            r'<hp:lineseg\s+[^/]*?vertpos="(\d+)"[^/]*?(?:textheight|vertsize)="(\d+)"',
            p.linesegs_xml or "",
        ):
            top = int(vp)
            height = int(th)
            if top > 0:
                has_accumulated_vertpos = True
            max_bottom = max(max_bottom, top + height)
            para_height = max(para_height, height)
        sum_heights += max(para_height, minimum)
    if has_accumulated_vertpos:
        return max(max_bottom, minimum)
    return max(sum_heights, max(len(paragraphs), 1) * minimum)


def _int_attr(xml: str, tag: str, attr: str) -> int | None:
    m = re.search(rf"<{tag}\b[^>]*\b{attr}=\"(-?\d+)\"", xml)
    return int(m.group(1)) if m else None


def _replace_attr(xml: str, tag: str, attr: str, value: int) -> str:
    pattern = rf"(<{tag}\b[^>]*\b{attr}=\")-?\d+(\")"
    return re.sub(pattern, rf"\g<1>{value}\2", xml, count=1)


def _cell_size(cell: CellItem) -> tuple[int, int]:
    width = _int_attr(cell.cell_meta_xml, "hp:cellSz", "width") or 0
    height = _int_attr(cell.cell_meta_xml, "hp:cellSz", "height") or 0
    return width, height


def _cell_margin(cell: CellItem) -> tuple[int, int, int, int]:
    m = re.search(
        r'<hp:cellMargin\b[^>]*\bleft="(-?\d+)"[^>]*\bright="(-?\d+)"[^>]*\btop="(-?\d+)"[^>]*\bbottom="(-?\d+)"',
        cell.cell_meta_xml,
    )
    return tuple(map(int, m.groups())) if m else (0, 0, 0, 0)


def _with_cell_height(cell: CellItem, height: int) -> CellItem:
    return replace(cell, cell_meta_xml=_replace_attr(cell.cell_meta_xml, "hp:cellSz", "height", height))


def _with_table_height(tbl: TableItem, height: int) -> TableItem:
    return replace(tbl, pre_rows_xml=_replace_attr(tbl.pre_rows_xml, "hp:sz", "height", height))


def _content_cell_index(tbl: TableItem) -> int:
    best_idx = 0
    best_area = -1
    for idx, cell in enumerate(tbl.cells):
        text = "".join(
            item.text
            for p in cell.paragraphs
            for item in p.items
            if isinstance(item, CharItem)
        )
        if "보기" in text:
            continue
        width, height = _cell_size(cell)
        area = width * height
        if area > best_area:
            best_idx = idx
            best_area = area
    return best_idx


@lru_cache(maxsize=1)
def _bogi_shell_template() -> TableItem:
    return _parse_table(template_text("bogi_box_unified.xml"), BOX_WRAP_CS)


def build_bogi_shell_with_source_content(content: Sequence[Paragraph]) -> Paragraph:
    source_content = tuple(content) or (_empty_source_content(),)
    shell = _bogi_shell_template()
    content_idx = _content_cell_index(shell)
    content_cell = shell.cells[content_idx]
    _, base_content_h = _cell_size(content_cell)
    _, _, mt, mb = _cell_margin(content_cell)
    content_cell_h = max(base_content_h, _content_height(source_content) + mt + mb)
    base_table_h = _int_attr(shell.pre_rows_xml, "hp:sz", "height") or base_content_h
    total_h = base_table_h - base_content_h + content_cell_h

    cells = list(shell.cells)
    content_sublist = dict(content_cell.sublist_attrs)
    content_sublist["vertAlign"] = "TOP"
    cells[content_idx] = _with_cell_height(
        replace(content_cell, paragraphs=source_content, sublist_attrs=content_sublist),
        content_cell_h,
    )
    table = replace(
        _with_table_height(shell, total_h),
        cells=tuple(cells),
        starts_new_run=True,
    )
    return Paragraph(items=(table,), para_shape_id=BOX_WRAP_PP, style_id=BOX_WRAP_STYLE, char_shape_id_first=BOX_WRAP_CS, para_id_attr="0")


def build_data_shell_with_source_content(
    content: Sequence[Paragraph],
    outer_border_fill_id: int,
    source_cell_height: int | None = None,
) -> Paragraph:
    source_content = tuple(content) or (_empty_source_content(),)
    ml, mr, mt, mb = DATA_BOX_CELLMARGIN
    measured_h = _content_height(source_content) + mt + mb
    cell_h = max(measured_h, source_cell_height or 0)
    cell = CellItem(
        cell_attrs=_cell_attrs(outer_border_fill_id),
        sublist_attrs=_sublist("CENTER"),
        paragraphs=source_content,
        cell_meta_xml=_cell_meta(0, 0, 1, 1, DATA_BOX_WIDTH, cell_h, DATA_BOX_CELLMARGIN),
    )
    table = TableItem(
        table_attrs=_table_attrs(1, 1, outer_border_fill_id),
        pre_rows_xml=_pre_rows_xml(DATA_BOX_WIDTH, cell_h, DATA_BOX_OUTMARGIN, DATA_BOX_INMARGIN),
        cells=(cell,),
        char_shape_id=BOX_WRAP_CS,
        starts_new_run=True,
    )
    return Paragraph(items=(table,), para_shape_id=BOX_WRAP_PP, style_id=BOX_WRAP_STYLE, char_shape_id_first=BOX_WRAP_CS, para_id_attr="0")


def build_data_shell_from_source_table(
    source_table: TableItem,
    outer_border_fill_id: int,
) -> Paragraph:
    """Use the source data table's geometry while applying template shell ids.

    Data boxes often contain anchored pictures/graphs whose positions are
    relative to the original table/cell geometry. Rebuilding only the cell
    paragraphs into a fresh shell loses that geometry and causes PNG overflow.
    The template owns the border shell, but the source table owns the internal
    layout frame for its content.
    """

    table_attrs = dict(source_table.table_attrs)
    table_attrs.update({
        "id": "0",
        "zOrder": "0",
        "borderFillIDRef": str(outer_border_fill_id),
        "pageBreak": "NONE",
    })
    cells = []
    for cell in source_table.cells:
        cell_attrs = dict(cell.cell_attrs)
        cell_attrs["borderFillIDRef"] = str(outer_border_fill_id)
        cells.append(replace(cell, cell_attrs=cell_attrs))
    table = replace(
        source_table,
        table_attrs=table_attrs,
        cells=tuple(cells),
        char_shape_id=BOX_WRAP_CS,
        starts_new_run=True,
    )
    return Paragraph(
        items=(table,),
        para_shape_id=BOX_WRAP_PP,
        style_id=BOX_WRAP_STYLE,
        char_shape_id_first=BOX_WRAP_CS,
        para_id_attr="0",
    )


BOGI_SHELL = BoxShellTemplate(
    role=Role.BOGI_BOX,
    name="bogi_box_unified",
    build=build_bogi_shell_with_source_content,
)


def build_bogi_box(content: Sequence[Paragraph]) -> Paragraph:
    return build_box_with_source_content(BOGI_SHELL, content)

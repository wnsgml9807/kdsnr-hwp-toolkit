"""cell.cellSz.height 결정 + row 내 max 통일.

전제: paragraph.linesegs_xml 이 이미 채워져 있어야 한다 (`linesegs.fill_missing`
호출 후). 이 모듈은 채워진 lineseg 의 visual bottom 을 합산해서 cell.height 를
결정한다.

원칙:
  - 같은 row 의 모든 cell.cellSz.height 는 동일해야 한다 (한컴 spec).
  - 우리 splitter 가 cell 별로 다른 height 로 만들 수 있으므로 row max 로 통일.
  - cell content (paragraph + inline) 가 stored cell.height 보다 크면 늘림.
  - rowSpan > 1 cell 은 cover 하는 row 들의 max sum 으로 결정.
  - canonical 보기 박스 (3x3 + "보기" 텍스트) 는 별도 special-case
    (label/filler row 와 content row 가 혼합되어 row max 통일이 깨뜨림).
"""
import re as _re
from typing import Optional
from dataclasses import replace as _dc_replace

from ..codec.schema import TableItem, CellItem


def resolve(doc):
    """doc 의 모든 paragraph 의 TableItem 의 cell.height + table.height 결정."""
    def visit_para(p):
        return _resolve_in_paragraph(p)

    new_sections = []
    sec_changed = False
    for sec in doc.sections:
        new_body = tuple(visit_para(p) for p in sec.body)
        if any(np is not op for np, op in zip(new_body, sec.body)):
            new_sections.append(_dc_replace(sec, body=new_body))
            sec_changed = True
        else:
            new_sections.append(sec)
    if sec_changed:
        return _dc_replace(doc, sections=tuple(new_sections))
    return doc


def _resolve_in_paragraph(p):
    """단일 paragraph 안의 TableItem 들에 대해 cell.height + table.height 결정.
    nested cell 안 paragraph 도 재귀.
    """
    if not any(isinstance(it, TableItem) for it in p.items):
        return p

    new_items = []
    changed = False
    for it in p.items:
        if not isinstance(it, TableItem):
            new_items.append(it)
            continue

        new_it, it_changed = _resolve_in_table(it)
        if it_changed:
            changed = True
        new_items.append(new_it)

    if not changed:
        return p
    return p.with_items(tuple(new_items))


def _resolve_in_table(it: TableItem):
    """단일 TableItem 안 cell.height + table.height 결정.
    cell 안 paragraph 가 nested table 을 포함하면 재귀.
    """
    from .. import lineseg_gen as _lg  # 헬퍼 함수 재사용 (점진적 분리)

    canonical_bogi = _lg._is_canonical_bogi_table(it)
    bogi_content_idx = _lg._canonical_bogi_content_index(it) if canonical_bogi else -1
    one_row_table = str(it.table_attrs.get("rowCnt")) == "1"
    outer_table_h = _lg._xml_int_attr(it.pre_rows_xml, "height") or 0
    bogi_old_content_h = 0
    bogi_required_content_h = 0

    new_cells = []
    required_cell_heights: list[int] = []
    changed = False

    for ci, c in enumerate(it.cells):
        sz_m = _re.search(r'<hp:cellSz\b[^/>]*\bwidth="(\d+)"', c.cell_meta_xml)
        old_h = _lg._xml_int_attr(c.cell_meta_xml, "height") or 0

        # 보기 shell cells: 그대로 보존
        if canonical_bogi and ci != bogi_content_idx:
            new_cells.append(c)
            required_cell_heights.append(old_h)
            continue

        mg_m = _re.search(
            r'<hp:cellMargin\b[^/>]*\bleft="(\d+)"\s+right="(\d+)".*?\btop="(\d+)"\s+bottom="(\d+)"',
            c.cell_meta_xml,
        )
        top_m = int(mg_m.group(3)) if mg_m else 0
        bottom_m = int(mg_m.group(4)) if mg_m else 0

        # 셀 안 paragraph 의 nested table 재귀 처리
        resolved_paras = tuple(_resolve_in_paragraph(cp) for cp in c.paragraphs)
        if any(rp is not op for rp, op in zip(resolved_paras, c.paragraphs)):
            changed = True

        # 비-빈 paragraph 의 visual_bottom 합산 (이미 채워진 lineseg 기반)
        def _is_empty(p):
            t, ins, _, _ = _lg.extract_text_and_inlines(p.items)
            return not t and not ins
        non_empty_paras = [cp for cp in resolved_paras if not _is_empty(cp)]
        content_bottom = max(
            (_lg._paragraph_visual_bottom(cp) for cp in non_empty_paras), default=0,
        )

        required_h = max(old_h, content_bottom + top_m + bottom_m)
        if one_row_table:
            required_h = max(required_h, outer_table_h)
        if canonical_bogi and ci == bogi_content_idx:
            bogi_old_content_h = old_h
            bogi_required_content_h = required_h
        required_cell_heights.append(required_h)

        # cell paragraphs 만 갱신 (cell_meta_xml/sublist_attrs.height 는 row-level post 에서)
        new_cells.append(CellItem(
            cell_attrs=dict(c.cell_attrs),
            sublist_attrs=dict(c.sublist_attrs),
            paragraphs=resolved_paras,
            cell_meta_xml=c.cell_meta_xml,
        ))

    pre_rows_xml = it.pre_rows_xml

    if canonical_bogi:
        # 보기 박스: 셀별 height 그대로 적용 (row max 통일 X)
        adjusted = list(new_cells)
        for ci, c in enumerate(new_cells):
            old_h = _lg._xml_int_attr(c.cell_meta_xml, "height") or 0
            req = required_cell_heights[ci]
            if req > old_h and old_h > 0:
                new_meta = _lg._replace_cell_height(c.cell_meta_xml, req)
                new_subattrs = dict(c.sublist_attrs)
                new_subattrs["textHeight"] = str(req)
                adjusted[ci] = CellItem(
                    cell_attrs=dict(c.cell_attrs),
                    sublist_attrs=new_subattrs,
                    paragraphs=c.paragraphs,
                    cell_meta_xml=new_meta,
                )
                changed = True
        new_cells = adjusted
        if bogi_required_content_h > bogi_old_content_h:
            old_table_h = _lg._xml_int_attr(pre_rows_xml, "height") or 0
            if old_table_h > 0:
                pre_rows_xml = _lg._replace_table_height(
                    pre_rows_xml,
                    old_table_h - bogi_old_content_h + bogi_required_content_h,
                )
                changed = True
    elif required_cell_heights:
        # 일반 표: row 내 max(required_h) 로 통일
        cell_addr: list[Optional[tuple[int, int]]] = []
        for c in new_cells:
            addr_m = _re.search(r'<hp:cellAddr\b[^>]*\browAddr="(\d+)"', c.cell_meta_xml)
            span_m = _re.search(r'<hp:cellSpan\b[^>]*\browSpan="(\d+)"', c.cell_meta_xml)
            if addr_m:
                cell_addr.append((
                    int(addr_m.group(1)),
                    int(span_m.group(1)) if span_m else 1,
                ))
            else:
                cell_addr.append(None)

        row_max_h: dict[int, int] = {}
        for ci, info in enumerate(cell_addr):
            if info is None:
                continue
            row, rs = info
            if rs == 1:
                row_max_h[row] = max(row_max_h.get(row, 0), required_cell_heights[ci])

        # rowSpan>1 cell 이 자기 cover range 합보다 크면 last_row 에 추가
        for ci, info in enumerate(cell_addr):
            if info is None:
                continue
            row, rs = info
            if rs <= 1:
                continue
            cur_sum = sum(row_max_h.get(r, 0) for r in range(row, row + rs))
            req = required_cell_heights[ci]
            if req > cur_sum:
                last_row = row + rs - 1
                row_max_h[last_row] = row_max_h.get(last_row, 0) + (req - cur_sum)

        # row_max 를 각 cell 에 적용
        adjusted = list(new_cells)
        for ci, info in enumerate(cell_addr):
            if info is None:
                continue
            row, rs = info
            target_h = sum(row_max_h.get(r, 0) for r in range(row, row + rs))
            if target_h <= 0:
                continue
            cur_h = _lg._xml_int_attr(adjusted[ci].cell_meta_xml, "height") or 0
            if target_h > cur_h and cur_h > 0:
                new_meta = _lg._replace_cell_height(adjusted[ci].cell_meta_xml, target_h)
                new_subattrs = dict(adjusted[ci].sublist_attrs)
                new_subattrs["textHeight"] = str(target_h)
                adjusted[ci] = CellItem(
                    cell_attrs=dict(adjusted[ci].cell_attrs),
                    sublist_attrs=new_subattrs,
                    paragraphs=adjusted[ci].paragraphs,
                    cell_meta_xml=new_meta,
                )
                changed = True
        new_cells = adjusted

        if row_max_h:
            old_table_h = _lg._xml_int_attr(pre_rows_xml, "height") or 0
            required_table_h = sum(row_max_h.values())
            if required_table_h > old_table_h and old_table_h > 0:
                pre_rows_xml = _lg._replace_table_height(pre_rows_xml, required_table_h)
                changed = True

    new_it = TableItem(
        table_attrs=dict(it.table_attrs),
        pre_rows_xml=pre_rows_xml,
        cells=tuple(new_cells),
        char_shape_id=it.char_shape_id,
        starts_new_run=it.starts_new_run,
    )
    return new_it, changed

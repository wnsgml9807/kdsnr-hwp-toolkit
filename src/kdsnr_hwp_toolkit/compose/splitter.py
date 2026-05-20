"""Compose — orchestrates extract → classify → transform → write per question.

Pipeline (one .hwpx output per detected question/set):

    1. read src .hwp/.hwpx
    2. unwrap_wrappers + split_fused_in_body  (extract)
    3. detect_units → list of question/set ranges  (extract)
    4. for each unit:
         a. read templet for subject (cached via slot_catalog)
         b. merge_styles(template ← src) → id_maps for src refs
         c. for each src paragraph in unit:
              - classify (atom + prev_atom context)
              - if EMPTY/UNKNOWN → drop
              - rewrite_paragraph(p, id_maps)
              - apply_atom(p, atom, slot)
         d. replace_section_body(template, body=transformed_paragraphs)
         e. write → bytes

Output: list of (label, hwpx_bytes).
"""
from __future__ import annotations

import re

from kdsnr_hwp_toolkit.adapters import hwp_to_hwpx
from kdsnr_hwp_toolkit.classify import Atom, classify
from kdsnr_hwp_toolkit.codec import read, write
from kdsnr_hwp_toolkit.codec.operations.copy import merge_styles, rewrite_paragraph
from kdsnr_hwp_toolkit.codec.operations.paragraphs import replace_section_body
from kdsnr_hwp_toolkit.codec.schema import (
    CharItem, EmptyRunItem, Paragraph, SectionMeta, StyleEntry, TableItem,
)
from kdsnr_hwp_toolkit.extract.boundary import (
    detect_units, disambiguate_labels, split_fused_in_body,
    split_fused_paragraph, unwrap_meta_tables, unwrap_wrappers,
)
from kdsnr_hwp_toolkit.layout import enrich_doc
from kdsnr_hwp_toolkit.resources import template_bytes
from kdsnr_hwp_toolkit.transform import apply_atom, catalog_for, normalize_styles


def _refuse_jimun_inline_box(triples):
    """Re-fuse korean inline-box paragraphs with adjacent jimun based on
    src origin paragraph index.

    Two src layouts split_fused_in_body produces:
      A) src had [TBL, text]                   → split: (TBL only) (text)
         → re-fuse box+next so box stays as the paragraph's first item
            (first-line indent applies to the box)
      B) src had [text, TBL, text]             → split: (text) (TBL only) (text)
         → re-fuse prev+box+next into one paragraph (mid-paragraph box)

    Disambiguation: prev/next are merged with the box only when they share
    the same origin paragraph index (= they came from the same src para).
    `triples` items are (paragraph, atom, origin_idx). `origin_idx` is None
    for inserted spacers etc.
    """
    fused_box_atoms = (Atom.JIMUN_DATA_BOX, Atom.JIMUN_INLINE_TABLE)
    out: list[Paragraph] = []
    i = 0
    while i < len(triples):
        p, a, oid = triples[i]
        if a in fused_box_atoms:
            prev_idx = i - 1
            next_idx = i + 1
            prev = triples[prev_idx] if prev_idx >= 0 else None
            nxt = triples[next_idx] if next_idx < len(triples) else None

            # prev only merges if same origin (= src had text BEFORE box in same paragraph)
            include_prev = (
                prev is not None
                and prev[1] == Atom.JIMUN
                and prev[2] is not None
                and prev[2] == oid
                and out  # prev is already in out
            )
            # next merges either when same origin (text after box in same src para)
            # OR when src layout was [TBL, text] (origin paragraph was just box+next)
            include_next = (
                nxt is not None
                and nxt[1] == Atom.JIMUN
                and nxt[2] is not None
                and nxt[2] == oid
            )

            new_items: list = []
            ref_p = p
            if include_prev:
                prev_p = out.pop()
                new_items.extend(prev_p.items)
                ref_p = prev_p
            new_items.extend(p.items)
            consumed_next = False
            if include_next:
                new_items.extend(nxt[0].items)
                consumed_next = True
            out.append(ref_p.with_items(tuple(new_items)))
            i += 2 if consumed_next else 1
            continue
        out.append(p)
        i += 1
    return out


def _merge_leading_structural_into_next(doc):
    """Move templet's leading structural paragraph (SectionMeta etc) into the
    next paragraph's items so the page starts directly with content.

    Otherwise korean output begins with a blank line (the templet placeholder
    that carries section meta) before the set_header. User explicitly asked
    to remove that opening empty line.
    """
    from dataclasses import replace as _replace
    sec = doc.sections[0]
    if len(sec.body) < 2:
        return doc
    first = sec.body[0]
    # Only merge if first is "structural-only" (no real text content).
    if any(isinstance(it, CharItem) and it.text.strip() for it in first.items):
        return doc
    second = sec.body[1]
    new_second = second.with_items(tuple(first.items) + tuple(second.items))
    new_body = (new_second,) + tuple(sec.body[2:])
    new_sec = _replace(sec, body=new_body)
    new_sections = (new_sec,) + tuple(doc.sections[1:])
    return _replace(doc, sections=new_sections)


def _find_spacer_after_set_header(template) -> Paragraph | None:
    """Find the spacer paragraph that follows the set_header in korean templet.

    User confirmed (2026-05-11): "세트 발문 사이~지문 첫 줄 사이에 반줄이
    들어가야 한다". templet/korean.hwpx body[1] is an EmptyRunItem-only
    paragraph (pp=7 st=2 cs=52) that supplies that 반줄. We auto-insert it
    after every classified SET_HEADER atom.
    """
    import re as _re
    set_header_re = _re.compile(r"\[\s*\d+\s*[~∼～]\s*\d+\s*\]")
    body = template.sections[0].body
    for i in range(len(body) - 1):
        text = "".join(it.text for it in body[i].items if isinstance(it, CharItem))
        if set_header_re.search(text):
            spacer = body[i + 1]
            if not any(isinstance(it, CharItem) and it.text.strip()
                       for it in spacer.items):
                return spacer
    return None


def _load_canonical_meta(subject: str) -> tuple[bytes, bytes]:
    """Pull (version.xml, settings.xml) out of the templet zip.

    rhwp's HWP→HWPX emits version.xml with the source HWP's older
    xmlVersion / comma-separated appVersion that Hanword 12+ rejects.
    We swap in the templet's known-good copies so the output opens.
    """
    import io as _io
    import zipfile as _zip
    raw = template_bytes(f"{subject}.hwpx")
    with _zip.ZipFile(_io.BytesIO(raw)) as z:
        return z.read("version.xml"), z.read("settings.xml")


def _replace_meta_in_zip(hwpx_bytes: bytes, version_xml: bytes, settings_xml: bytes) -> bytes:
    import io as _io
    import zipfile as _zip
    src = _io.BytesIO(hwpx_bytes)
    out = _io.BytesIO()
    with _zip.ZipFile(src, "r") as zin, \
         _zip.ZipFile(out, "w", _zip.ZIP_DEFLATED) as zout:
        for item in zin.infolist():
            if item.filename == "version.xml":
                zout.writestr(item, version_xml)
            elif item.filename == "settings.xml":
                zout.writestr(item, settings_xml)
            else:
                zout.writestr(item, zin.read(item.filename))
    return out.getvalue()


def _strip_incomplete_shapes(doc):
    """Remove `<hp:rect>` / `<hp:line>` / `<hp:ellipse>` OpaqueInlineItems
    whose rhwp serializer emits without the required `lineShape` /
    `fillBrush` / `inMargin` children — Hanword refuses the entire .hwpx
    when any such element is encountered.

    Diagnostic toggle to confirm shape XML completeness is the cause of
    open-failure (Q17). Once rhwp's write_rect/write_line emits the full
    required schema this function becomes a no-op.
    """
    from dataclasses import replace as _replace
    from kdsnr_hwp_toolkit.codec.schema import (
        TableItem, OpaqueInlineItem, CellItem,
    )
    _BAD_PREFIXES = ("<hp:rect", "<hp:line ", "<hp:ellipse")

    def _filter_items(items):
        out = []
        changed = False
        for it in items:
            if isinstance(it, OpaqueInlineItem) and it.xml.startswith(_BAD_PREFIXES):
                changed = True
                continue
            if isinstance(it, TableItem):
                new_cells = []
                tbl_changed = False
                for c in it.cells:
                    new_paras = []
                    cell_changed = False
                    for cp in c.paragraphs:
                        new_p_items, p_changed = _filter_items(cp.items)
                        if p_changed:
                            new_paras.append(cp.with_items(new_p_items))
                            cell_changed = True
                        else:
                            new_paras.append(cp)
                    if cell_changed:
                        new_cells.append(_replace(c, paragraphs=tuple(new_paras)))
                        tbl_changed = True
                    else:
                        new_cells.append(c)
                if tbl_changed:
                    out.append(_replace(it, cells=tuple(new_cells)))
                    changed = True
                    continue
            out.append(it)
        return tuple(out), changed

    new_sections = []
    sections_changed = False
    for sec in doc.sections:
        new_body = []
        body_changed = False
        for p in sec.body:
            new_items, p_changed = _filter_items(p.items)
            if p_changed:
                new_body.append(p.with_items(new_items))
                body_changed = True
            else:
                new_body.append(p)
        if body_changed:
            new_sections.append(_replace(sec, body=tuple(new_body)))
            sections_changed = True
        else:
            new_sections.append(sec)
    if not sections_changed:
        return doc
    return _replace(doc, sections=tuple(new_sections))


def _autofit_zero_height_cells(doc):
    """Set explicit per-row cell heights when binary has `height="0"`.

    HWP binary stores height=0 for cells whose row should auto-fit content
    height. Hanword's .hwp renderer computes the row height at draw time,
    but its .hwpx renderer leaves them at 0, collapsing rows on top of one
    another. Fix: distribute the table's `<hp:sz height>` evenly across rows
    that report height=0.
    """
    from dataclasses import replace as _replace
    import re as _re
    from kdsnr_hwp_toolkit.codec.schema import TableItem, CellItem

    _SZ_HEIGHT_RE = _re.compile(r'<hp:sz[^/>]*\bheight="(\d+)"')
    _CELLSZ_HEIGHT_RE = _re.compile(r'(<hp:cellSz[^/>]*\bheight=")(\d+)(")')
    _CELL_ROW_RE = _re.compile(r'<hp:cellAddr[^/>]*\browAddr="(\d+)"')

    def _fix_table(tbl: TableItem) -> TableItem:
        m = _SZ_HEIGHT_RE.search(tbl.pre_rows_xml)
        if not m:
            return tbl
        table_height = int(m.group(1))
        if table_height <= 0:
            return tbl

        # Determine per-row required height. Distribute evenly across distinct
        # rows when binary heights are 0; rows with non-zero heights are kept.
        row_max_h: dict[int, int] = {}
        for c in tbl.cells:
            rm = _CELL_ROW_RE.search(c.cell_meta_xml)
            if not rm:
                continue
            row = int(rm.group(1))
            hm = _CELLSZ_HEIGHT_RE.search(c.cell_meta_xml)
            h = int(hm.group(2)) if hm else 0
            row_max_h[row] = max(row_max_h.get(row, 0), h)
        if not row_max_h:
            return tbl

        zero_rows = [r for r, h in row_max_h.items() if h == 0]
        if not zero_rows:
            return tbl

        known_total = sum(h for h in row_max_h.values() if h > 0)
        remaining = table_height - known_total
        if remaining <= 0:
            return tbl
        per_zero_row = max(remaining // len(zero_rows), 1)

        new_cells = []
        for c in tbl.cells:
            rm = _CELL_ROW_RE.search(c.cell_meta_xml)
            if not rm:
                new_cells.append(c); continue
            row = int(rm.group(1))
            if row not in zero_rows:
                # also recurse into nested tables
                new_paras = tuple(_fix_para(p) for p in c.paragraphs)
                new_cells.append(_replace(c, paragraphs=new_paras))
                continue
            new_meta = _CELLSZ_HEIGHT_RE.sub(
                lambda m: f'{m.group(1)}{per_zero_row}{m.group(3)}',
                c.cell_meta_xml,
            )
            new_paras = tuple(_fix_para(p) for p in c.paragraphs)
            new_cells.append(_replace(c, cell_meta_xml=new_meta, paragraphs=new_paras))
        return _replace(tbl, cells=tuple(new_cells))

    def _fix_para(p):
        new_items = []
        changed = False
        for it in p.items:
            if isinstance(it, TableItem):
                new_tbl = _fix_table(it)
                if new_tbl is not it:
                    changed = True
                new_items.append(new_tbl)
            else:
                new_items.append(it)
        if not changed:
            return p
        return p.with_items(tuple(new_items))

    new_sections = []
    sections_changed = False
    for sec in doc.sections:
        new_body = []
        body_changed = False
        for p in sec.body:
            np = _fix_para(p)
            if np is not p:
                body_changed = True
            new_body.append(np)
        if body_changed:
            new_sections.append(_replace(sec, body=tuple(new_body)))
            sections_changed = True
        else:
            new_sections.append(sec)
    if not sections_changed:
        return doc
    return _replace(doc, sections=tuple(new_sections))


_NULL_REF = "4294967295"  # HWPX uint sentinel = "no override"


def _distribute_to_justify(hwpx_bytes: bytes) -> bytes:
    """Replace `<hh:align horizontal="DISTRIBUTE_SPACE"/>` with `JUSTIFY`.

    Many .hwp authors break a logical paragraph into per-visual-line
    paragraphs and pair them with DISTRIBUTE_SPACE alignment. Hanword's
    binary renderer detects this and skips the inter-word stretch on
    short last-lines; the HWPX renderer applies stretch uniformly,
    causing 2-word lines like "세계 ⋯ 최고" to spread across the cell.
    JUSTIFY uses the same inter-word stretch but exempts the last
    (or only) line of each paragraph, which matches the binary
    renderer's behaviour without changing layout for true multi-line
    justified paragraphs.
    """
    import io as _io
    import zipfile as _zip
    src = _io.BytesIO(hwpx_bytes)
    out = _io.BytesIO()
    with _zip.ZipFile(src, "r") as zin, \
         _zip.ZipFile(out, "w", _zip.ZIP_DEFLATED) as zout:
        for item in zin.infolist():
            data = zin.read(item.filename)
            if item.filename == "Contents/header.xml":
                text = data.decode("utf-8")
                text = text.replace(
                    'horizontal="DISTRIBUTE_SPACE"',
                    'horizontal="JUSTIFY"',
                )
                data = text.encode("utf-8")
            zout.writestr(item, data)
    return out.getvalue()


def _shorttabs_to_space(hwpx_bytes: bytes) -> bytes:
    """Replace `<hp:tab width="N"/>` elements with a single space character
    when N < 1000 HWPUNIT (i.e. small inline gap tabs).

    Hanword 12+ HWPX renderer ignores hp:tab width and snaps each tab to
    the next paragraph tabPr stop. .hwp binaries that author intended a
    small inline gap (e.g. "교사 :", "*** 총부양비＝") store small widths
    (190–500 range) but the renderer expands them to 4040+ unit gaps,
    pushing the trailing text to the wrong column or wrapping it. A bare
    space is the simplest substitute that preserves the visual gap without
    triggering tab-stop snap.
    """
    import io as _io
    import zipfile as _zip
    src = _io.BytesIO(hwpx_bytes)
    out = _io.BytesIO()
    with _zip.ZipFile(src, "r") as zin, \
         _zip.ZipFile(out, "w", _zip.ZIP_DEFLATED) as zout:
        for item in zin.infolist():
            data = zin.read(item.filename)
            if item.filename == "Contents/section0.xml":
                text = data.decode("utf-8")
                def _repl(m):
                    w = int(m.group(1))
                    return " " if w < 1000 else m.group(0)
                text = re.sub(
                    r'<hp:tab\b[^>]*\bwidth="(\d+)"[^/]*/>',
                    _repl,
                    text,
                )
                data = text.encode("utf-8")
            zout.writestr(item, data)
    return out.getvalue()


def _zero_inline_tab_widths(hwpx_bytes: bytes) -> bytes:
    """Set every <hp:tab width="N"/> in section0.xml to width="0".

    The HWP binary stores a width hint per tab control (`<hp:tab width="220"/>`
    etc) that Hanword's binary renderer treats as an internal hint and
    recomputes the actual advance from tabPr stops at render time. Hanword's
    HWPX renderer, however, treats the stored width as authoritative — so
    rhwp's verbatim emission of the binary value (220, 482, etc.) shows tabs
    far narrower than the original .hwp. Hancom-saved HWPX files store the
    pre-computed advance instead (1676–24594 range observed).

    Setting width to 0 makes Hanword snap to the next tab stop on render,
    matching the binary renderer's behaviour and producing the visual gap
    the author intended.
    """
    import io as _io
    import zipfile as _zip
    src = _io.BytesIO(hwpx_bytes)
    out = _io.BytesIO()
    with _zip.ZipFile(src, "r") as zin, \
         _zip.ZipFile(out, "w", _zip.ZIP_DEFLATED) as zout:
        for item in zin.infolist():
            data = zin.read(item.filename)
            if item.filename == "Contents/section0.xml":
                text = data.decode("utf-8")
                text = re.sub(
                    r'(<hp:tab\b[^>]*\bwidth=")\d+(")',
                    r'\g<1>0\g<2>',
                    text,
                )
                data = text.encode("utf-8")
            zout.writestr(item, data)
    return out.getvalue()


def _drop_tabpr_for_cell_tabs(hwpx_bytes: bytes) -> bytes:
    """Remove tabPrIDRef from paraPrs that are referenced by cell paragraphs
    containing explicit-width <hp:tab> elements.

    Hanword 12+ reflowing a cell paragraph honours tabPr stops and ignores
    the stored hp:tab width — so a paragraph with `tabPr stops 4040, 5500`
    and an inline `<hp:tab width="220"/>` jumps to 4040 (large gap) instead
    of advancing 220 units. The original .hwp does not exhibit this because
    the binary renderer uses the stored tab width directly.

    Setting tabPrIDRef on those paraPrs to the empty tabPr (id 0 — present
    by default in Hancom's catalog) lets Hanword fall back to the hp:tab
    width attribute for cell paragraph layout.
    """
    import io as _io
    import zipfile as _zip
    src = _io.BytesIO(hwpx_bytes)

    # First pass: read section0 to collect cell paragraph paraPr IDs that
    # reference explicit-width hp:tab elements.
    target_pp_ids: set[str] = set()
    with _zip.ZipFile(src, "r") as zin:
        try:
            section_xml = zin.read("Contents/section0.xml").decode("utf-8")
        except KeyError:
            return hwpx_bytes
        # Walk hp:tc blocks; for each cell paragraph that has an hp:tab with
        # non-zero width, remember its paraPrIDRef.
        for tc_m in re.finditer(r"<hp:tc\b[^>]*>(.*?)</hp:tc>", section_xml, re.DOTALL):
            tc_body = tc_m.group(1)
            for p_m in re.finditer(r"<hp:p\b([^>]*)>(.*?)</hp:p>", tc_body, re.DOTALL):
                attrs = p_m.group(1)
                body = p_m.group(2)
                if not re.search(r'<hp:tab\b[^>]*\bwidth="(?!0")\d+"', body):
                    continue
                pp_m = re.search(r'paraPrIDRef="(\d+)"', attrs)
                if pp_m:
                    target_pp_ids.add(pp_m.group(1))

    if not target_pp_ids:
        return hwpx_bytes

    src.seek(0)
    out = _io.BytesIO()
    with _zip.ZipFile(src, "r") as zin, \
         _zip.ZipFile(out, "w", _zip.ZIP_DEFLATED) as zout:
        for item in zin.infolist():
            data = zin.read(item.filename)
            if item.filename == "Contents/header.xml":
                text = data.decode("utf-8")
                # For each target paraPr, override tabPrIDRef to 0 (the empty
                # tabPr that Hancom always provides). Pattern is anchored on
                # `id="N"` to scope the rewrite to the matching paraPr only.
                for pid in target_pp_ids:
                    text = re.sub(
                        rf'(<hh:paraPr\b[^>]*\bid="{pid}"[^>]*\btabPrIDRef=")(\d+)(")',
                        rf'\g<1>0\g<3>',
                        text,
                    )
                data = text.encode("utf-8")
            zout.writestr(item, data)
    return out.getvalue()



def _sanitize_style_refs(doc):
    """Replace dangling paraPrIDRef/charPrIDRef/nextStyleIDRef in style
    entries with the HWPX null sentinel.

    rhwp's HWP→HWPX emits values like ``paraPrIDRef="1042"`` (the langID
    leaking into the paraPr slot). Hanword applies the dangling ref when
    rendering and falls back to default paragraph properties — losing
    indent/intent/alignment specified on the actual paragraph. Sweeping
    bad refs to the null sentinel makes the renderer use the paragraph's
    own paraPr instead.
    """
    from dataclasses import replace as _replace
    valid_pp = {e.id for e in doc.styles.para_shapes}
    valid_cs = {e.id for e in doc.styles.char_shapes}
    valid_st = {e.id for e in doc.styles.styles}

    def _fix(xml: str, attr: str, valid: set) -> str:
        def repl(m):
            v = int(m.group(1))
            return f'{attr}="{_NULL_REF}"' if v not in valid else m.group(0)
        return re.sub(rf'{attr}="(\d+)"', repl, xml)

    new_styles = []
    for e in doc.styles.styles:
        x = e.xml
        x = _fix(x, "paraPrIDRef", valid_pp)
        x = _fix(x, "charPrIDRef", valid_cs)
        x = _fix(x, "nextStyleIDRef", valid_st)
        new_styles.append(StyleEntry(id=e.id, xml=x))
    return _replace(
        doc, styles=_replace(doc.styles, styles=tuple(new_styles))
    )


def _strip_templet_tabstopval(doc):
    """Remove templet's `tabStopVal`/`tabStopUnit` from <hp:secPr>.

    Templet sets section-default tab interval to 4000 hwpunit via these
    attributes; src files don't set them and rely on the implicit default
    derived from `tabStop="8000"`. When we wrap src content in templet's
    section meta, dialog/footnote paragraphs that depend on the implicit
    default render with wider/different tab spacing.

    Strip both attrs so cell paragraphs whose tabPr has no explicit stops
    fall back to src-style behavior.
    """
    from dataclasses import replace as _replace
    import re as _re
    sec = doc.sections[0]
    if not sec.body:
        return doc
    first = sec.body[0]
    new_items = []
    changed = False
    for it in first.items:
        if isinstance(it, SectionMeta):
            new_xml = _re.sub(r'\s*tabStopVal="[^"]*"', '', it.raw_xml)
            new_xml = _re.sub(r'\s*tabStopUnit="[^"]*"', '', new_xml)
            if new_xml != it.raw_xml:
                new_items.append(_replace(it, raw_xml=new_xml))
                changed = True
                continue
        new_items.append(it)
    if not changed:
        return doc
    new_first = first.with_items(tuple(new_items))
    new_body = (new_first,) + tuple(sec.body[1:])
    new_sec = _replace(sec, body=new_body)
    return _replace(doc, sections=(new_sec,) + tuple(doc.sections[1:]))


_UNSUPPORTED_KOREAN_MESSAGE = "국어 과목은 아직 지원하지 않습니다"

_KOREAN_INPUT_MARKERS = (
    "국어 영역",
    "다음 글을 읽고",
    "다음 글을 읽고 물음에 답하시오",
    "윗글",
    "보기의 ⓐ",
)


def _doc_text_for_subject_guard(doc) -> str:
    parts: list[str] = []

    def visit_paragraph(p):
        for item in p.items:
            if isinstance(item, CharItem):
                parts.append(item.text)
            elif isinstance(item, TableItem):
                for cell in item.cells:
                    for cp in cell.paragraphs:
                        visit_paragraph(cp)

    for sec in doc.sections:
        for p in sec.body:
            visit_paragraph(p)
    return "".join(parts)


def _contains_unsupported_korean_marker(text: str) -> bool:
    compact = "".join(text.split())
    if any(marker in text for marker in _KOREAN_INPUT_MARKERS):
        return True
    return "국어영역" in compact or "다음글을읽고" in compact


def _raise_if_unsupported_korean_input(doc) -> None:
    if _contains_unsupported_korean_marker(_doc_text_for_subject_guard(doc)):
        raise ValueError(_UNSUPPORTED_KOREAN_MESSAGE)


def _detect_subject_from_text(text: str) -> str:
    compact = "".join(text.split())
    if _contains_unsupported_korean_marker(text):
        raise ValueError(_UNSUPPORTED_KOREAN_MESSAGE)
    if "과학탐구" in compact or "통합과학" in compact:
        return "science"
    if "수학영역" in compact or ("5지선다형" in compact and "단답형" in compact):
        return "math"
    if "사회탐구" in compact:
        return "social"
    social_markers = (
        "사회문제",
        "개인과사회",
        "문화",
        "가치함축",
        "사회현상",
        "당위법칙",
    )
    if sum(1 for marker in social_markers if marker in compact) >= 2:
        return "social"
    raise ValueError("시험지 과목을 자동 인식하지 못했습니다")


def _detect_subject(doc) -> str:
    return _detect_subject_from_text(_doc_text_for_subject_guard(doc))


def _split_paper_to_hwpx_units_for_subject(
    src, subject: str
) -> list[tuple[str, bytes]]:
    _raise_if_unsupported_korean_input(src)

    catalog = catalog_for(subject)
    canonical_version, canonical_settings = _load_canonical_meta(subject)
    # Note: cell-height auto-fit and incomplete-shape strip are now handled
    # in rhwp's serializer (write_cell_sz fallback + write_rect/write_line
    # full child emission). Earlier post-processes removed.
    src = unwrap_wrappers(src, korean=(subject == "korean"))
    src = unwrap_meta_tables(src)
    # Track origin index for each split paragraph so re-fuse can recognize
    # which split paragraphs came from the same original src paragraph.
    src_orig_body = src.sections[0].body
    origin_for_split: list[int] = []
    new_split_body: list[Paragraph] = []
    for orig_idx, op in enumerate(src_orig_body):
        for sp in split_fused_paragraph(op):
            new_split_body.append(sp)
            origin_for_split.append(orig_idx)
    from dataclasses import replace as _dc_replace
    src = _dc_replace(
        src,
        sections=(_dc_replace(src.sections[0], body=tuple(new_split_body)),)
        + tuple(src.sections[1:]),
    )
    units = detect_units(src, subject)
    labels = disambiguate_labels(units)

    src_body = src.sections[0].body
    outputs: list[tuple[str, bytes]] = []

    for unit, label in zip(units, labels):
        # Fresh template per unit (replace_section_body returns new doc, but
        # we want a clean copy of styles for each unit's merge_styles)
        template = read(
            template_bytes(f"{subject}.hwpx"),
            doc_id=f"template:{subject}:{label}",
        )

        # Merge src styles into template
        merged_template, id_maps = merge_styles(
            template, src, src_doc_id="src"
        )
        # Sweep dangling paraPrIDRef/charPrIDRef inside <hh:style> entries
        # to the null sentinel — rhwp emits langID (e.g. 1042) in paraPrIDRef
        # which Hanword applies blindly, overriding the paragraph's actual
        # paraPr (loses indent/intent/align).
        merged_template = _sanitize_style_refs(merged_template)

        # Classify + transform each src paragraph in the unit
        out_triples: list[tuple[Paragraph, Atom, int | None]] = []
        prev_atom = None
        spacer = (
            _find_spacer_after_set_header(catalog.template)
            if subject == "korean"
            else None
        )
        for src_idx in unit.para_indices:
            sp = src_body[src_idx]
            atom = classify(sp, prev_atom=prev_atom, subject=subject)
            if atom in (Atom.EMPTY, Atom.UNKNOWN):
                continue
            slot = catalog.atom_to_slot.get(atom)
            if slot is None:
                continue
            remapped = rewrite_paragraph(sp, id_maps)
            transformed = apply_atom(remapped, atom, slot)
            origin = origin_for_split[src_idx]
            out_triples.append((transformed, atom, origin))
            # Korean: insert templet's 반줄 spacer right after each set_header
            if subject == "korean" and atom == Atom.SET_HEADER and spacer is not None:
                out_triples.append((spacer, Atom.EMPTY, None))
            prev_atom = atom

        # Korean: re-fuse jimun_data_box with adjacent jimun paragraphs so the
        # passage text flows around the inline box. Origin index decides
        # whether prev/next belongs to the same src paragraph.
        if subject == "korean":
            out_paras = _refuse_jimun_inline_box(out_triples)
        else:
            out_paras = [p for p, _, _ in out_triples]

        # keep_scope_defs=True: preserve templet's hp:header / hp:footer
        # (머리말/바닥글 — page header/footer content). Dropping these strips
        # subject headers like "사회탐구 영역" + horizontal rule.
        out_doc = replace_section_body(
            merged_template, 0, tuple(out_paras), keep_scope_defs=True
        )
        # Korean: drop the leading blank carrier paragraph by merging its
        # structural items into the first content paragraph (set_header).
        if subject == "korean":
            out_doc = _merge_leading_structural_into_next(out_doc)
        # Global policy: black text, transparent fill, drop memos.
        out_doc = normalize_styles(out_doc)
        # Strip templet's tabStopVal so explicit-stop-less tabs match src.
        out_doc = _strip_templet_tabstopval(out_doc)
        # Splitter clears linesegs_xml in 3 places (split_fused_paragraph,
        # _apply_role_style text-only, _apply_balmun_style): src's cached
        # lineseg vpos was anchored to src's column width and is invalid
        # after retargeting to templet slot. layout.enrich_doc recomputes
        # linesegs against templet's paraShape/charShape/column metrics so
        # rhwp doesn't fall back to a self-accumulating y that conflicts
        # with the preserved absolute vpos in adjacent paragraphs
        # (root cause of Q28 huge gap above display equation).
        # 2026-05-19: enrich_linesegs → layout.enrich_doc. 책임 분리:
        #   linesegs.fill_missing → cell_height.resolve → inline_correction.apply
        out_doc = enrich_doc(out_doc)
        out_bytes = write(out_doc)
        # Replace version.xml/settings.xml with templet's canonical copies
        # — rhwp's are too old for Hanword 12+ to accept.
        out_bytes = _replace_meta_in_zip(
            out_bytes, canonical_version, canonical_settings
        )
        # NOTE: paraPr margin <hp:switch> wrapping AND tabPr stop pairing are
        # now emitted natively by rhwp's HWPX serializer (write_para_pr +
        # write_tab_item_switch). Earlier post-processes (_wrap_header_paraPr_switch,
        # _pair_tabpr_stops, _merge_visually_continuous_cell_paragraphs) removed —
        # they were all workarounds for the same root cause: rhwp emitting
        # default-branch-only values that Hanword 12+ misreads at 2× scale.
        outputs.append((label, out_bytes))

    return outputs


def split_paper_to_hwpx_units_with_subject(input_data: bytes) -> tuple[str, list[tuple[str, bytes]]]:
    src = read(hwp_to_hwpx(input_data), doc_id="src")
    subject = _detect_subject(src)
    return subject, _split_paper_to_hwpx_units_for_subject(src, subject)


def split_paper_to_hwpx_units(input_data: bytes) -> list[tuple[str, bytes]]:
    """Split a source HWP/HWPX into per-question .hwpx files.

    The subject is detected from the source document. Korean inputs are
    intentionally rejected until the Korean pipeline is production-ready.
    """

    _, units = split_paper_to_hwpx_units_with_subject(input_data)
    return units


__all__ = ["split_paper_to_hwpx_units"]

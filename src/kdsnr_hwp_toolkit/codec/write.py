"""HwpxDocument → HWPX zip bytes.

Strategy: write back EVERY raw_xml_files entry verbatim, regenerate the
files we own (header.xml, section[N].xml, content.hpf), and write
BinData/* from bin_items. Order is preserved as much as possible to keep
zip diff minimal.

`mimetype` MUST be the first zip entry, uncompressed (HWPX/EPUB convention).
"""

from __future__ import annotations

import io
import re
import zipfile

from ._xml import emit_open_tag, escape_attr, escape_text
from .schema import (
    BinItem,
    CellItem,
    CharItem,
    ColumnDef,
    EmptyRunItem,
    FaceNameList,
    HwpxDocument,
    Item,
    LayoutDef,
    LineBreakItem,
    NoteDef,
    OpaqueInlineItem,
    Paragraph,
    ScopeDef,
    Section,
    SectionMeta,
    StyleEntry,
    StyleTable,
    TabItem,
    TableItem,
)
from .read import MARKER_OPEN, MARKER_CLOSE


# ============================================================
# Public entry
# ============================================================

def write(doc: HwpxDocument) -> bytes:
    """Serialize HwpxDocument to HWPX zip bytes."""
    buf = io.BytesIO()
    # mimetype must be first and uncompressed (epub-style).
    mt = doc.raw_xml_files.get("mimetype", b"application/hwp+zip")
    with zipfile.ZipFile(buf, "w") as zf:
        zi = zipfile.ZipInfo("mimetype")
        zi.compress_type = zipfile.ZIP_STORED
        zf.writestr(zi, mt)

        # Track names already written to avoid duplicates.
        written: set[str] = {"mimetype"}

        # Other raw passthrough files (preserve original order if possible).
        for name, payload in doc.raw_xml_files.items():
            if name in written:
                continue
            zf.writestr(name, payload, zipfile.ZIP_DEFLATED)
            written.add(name)

        # Contents/header.xml — regenerate
        header_xml = _emit_header_xml(doc)
        zf.writestr("Contents/header.xml", header_xml.encode("utf-8"),
                    zipfile.ZIP_DEFLATED)
        written.add("Contents/header.xml")

        # Contents/section[N].xml
        for sec in doc.sections:
            name = f"Contents/section{sec.section_index}.xml"
            zf.writestr(name, _emit_section_xml(sec).encode("utf-8"),
                        zipfile.ZIP_DEFLATED)
            written.add(name)

        # Contents/content.hpf — regenerate manifest with current bin_items.
        zf.writestr("Contents/content.hpf",
                    _emit_content_hpf(doc).encode("utf-8"),
                    zipfile.ZIP_DEFLATED)
        written.add("Contents/content.hpf")

        # BinData/* — bytes from bin_items
        for bi in doc.bin_items:
            if bi.href in written:
                continue
            zf.writestr(bi.href, bi.data, zipfile.ZIP_DEFLATED)
            written.add(bi.href)

    return buf.getvalue()


# ============================================================
# header.xml emission
# ============================================================

def _emit_header_xml(doc: HwpxDocument) -> str:
    parts: list[str] = [doc.header_xml_decl]
    parts.append(f"<hh:head{doc.header_root_attrs}>")
    parts.append(doc.header_pre_lists_xml)
    # Lists in canonical HWPX order
    parts.append(_emit_fontfaces(doc.styles.face_names))
    parts.append(_emit_list("hh:borderFills", doc.styles.border_fills))
    parts.append(_emit_list("hh:charProperties", doc.styles.char_shapes))
    parts.append(_emit_list("hh:tabProperties", doc.styles.tab_defs))
    parts.append(_emit_list("hh:numberings", doc.styles.numberings))
    if doc.styles.bullets:
        parts.append(_emit_list("hh:bullets", doc.styles.bullets))
    parts.append(_emit_list("hh:paraProperties", doc.styles.para_shapes))
    parts.append(_emit_list("hh:styles", doc.styles.styles))
    parts.append(doc.header_post_lists_xml)
    parts.append("</hh:head>")
    return "".join(parts)


def _emit_list(list_tag: str, entries: tuple[StyleEntry, ...]) -> str:
    if not entries:
        # Emit empty list with itemCnt=0 — HWPX accepts.
        return f'<{list_tag} itemCnt="0"></{list_tag}>'
    inner = "".join(e.xml for e in entries)
    return f'<{list_tag} itemCnt="{len(entries)}">{inner}</{list_tag}>'


def _emit_fontfaces(blocks: tuple[FaceNameList, ...]) -> str:
    if not blocks:
        return '<hh:fontfaces itemCnt="0"></hh:fontfaces>'
    inner_parts: list[str] = []
    for fl in blocks:
        attrs = dict(fl.raw_attrs)
        attrs["lang"] = fl.lang
        attrs["fontCnt"] = str(len(fl.fonts))
        # Place lang first, then fontCnt, then any others (HWPX convention)
        ordered = {"lang": attrs.pop("lang"), "fontCnt": attrs.pop("fontCnt")}
        ordered.update(attrs)
        attrs_str = "".join(
            f' {k}="{escape_attr(str(v))}"' for k, v in ordered.items()
        )
        if not fl.fonts:
            inner_parts.append(f"<hh:fontface{attrs_str}/>")
        else:
            fonts_xml = "".join(f.xml for f in fl.fonts)
            inner_parts.append(f"<hh:fontface{attrs_str}>{fonts_xml}</hh:fontface>")
    return (
        f'<hh:fontfaces itemCnt="{len(blocks)}">'
        + "".join(inner_parts)
        + "</hh:fontfaces>"
    )


# ============================================================
# section[N].xml emission
# ============================================================

def _emit_section_xml(sec: Section) -> str:
    decl = sec.raw_xml_decl or '<?xml version="1.0" encoding="UTF-8" standalone="yes" ?>'
    body_xml = "".join(_emit_paragraph(p) for p in sec.body)
    return f'{decl}<hs:sec{sec.raw_root_attrs}>{body_xml}</hs:sec>'


def _emit_paragraph(p: Paragraph) -> str:
    """<hp:p attrs>...runs...</hp:p>"""
    attrs = dict(p.raw_attrs)
    # Update with current values (preserves order if key exists)
    attrs["paraPrIDRef"] = str(p.para_shape_id)
    attrs["styleIDRef"] = str(p.style_id)
    if "id" in attrs:
        attrs["id"] = p.para_id_attr
    elif p.para_id_attr and p.para_id_attr != "0":
        attrs["id"] = p.para_id_attr
    if "pageBreak" in attrs or p.starts_new_page:
        attrs["pageBreak"] = "1" if p.starts_new_page else "0"
    if "columnBreak" in attrs or p.starts_new_column:
        attrs["columnBreak"] = "1" if p.starts_new_column else "0"
    if "merged" in attrs or p.merged:
        attrs["merged"] = "1" if p.merged else "0"

    open_tag = emit_open_tag("hp:p", attrs)
    linesegs = p.linesegs_xml or ""
    if not p.items:
        # Empty paragraph — emit a minimal empty run so HWPX has at least
        # one charPrIDRef anchor. Hanword rejects <hp:p></hp:p> empty.
        return f'{open_tag}<hp:run charPrIDRef="{p.char_shape_id_first}"/>{linesegs}</hp:p>'
    runs_xml = _emit_runs(p.items)
    return f"{open_tag}{runs_xml}{linesegs}</hp:p>"


def _emit_runs(items: tuple[Item, ...]) -> str:
    """Group consecutive items into <hp:run> blocks.

    Break a new <hp:run> when EITHER:
      - the item's char_shape_id differs from current run's, OR
      - the item carries `starts_new_run=True` (parser preserved an
        explicit run boundary even with same char_shape_id).
    EmptyRunItem always emits its own self-close <hp:run charPrIDRef=N/>.

    Inside each run, contiguous CharItem/TabItem/LineBreakItem with a
    compatible charStyleIDRef are coalesced into a SINGLE <hp:t> element:
      <hp:t>① <hp:tab .../>② <hp:tab .../>③ </hp:t>
    Hanword renders inline tabs reliably only when wrapped inside hp:t —
    standalone <hp:tab> emitted as an hp:run sibling collapses visually.
    Source HWPX (and the math/korean templates) all use the wrapped form.
    """
    parts: list[str] = []
    cur_cs: int | None = None
    cur_children: list[Item] = []

    def flush():
        if cur_children:
            parts.append(f'<hp:run charPrIDRef="{cur_cs}">')
            parts.append(_emit_run_children(cur_children))
            parts.append("</hp:run>")

    for item in items:
        if isinstance(item, EmptyRunItem):
            flush()
            cur_children = []
            cur_cs = None
            parts.append(f'<hp:run charPrIDRef="{item.char_shape_id}"/>')
            continue
        item_cs = _item_cs(item)
        force_break = bool(getattr(item, "starts_new_run", False)) and cur_cs is not None
        if cur_cs is None:
            cur_cs = item_cs
        elif item_cs != cur_cs or force_break:
            flush()
            cur_children = []
            cur_cs = item_cs
        cur_children.append(item)

    if cur_children:
        parts.append(f'<hp:run charPrIDRef="{cur_cs}">')
        parts.append(_emit_run_children(cur_children))
        parts.append("</hp:run>")
    return "".join(parts)


def _is_text_inline(item: Item) -> bool:
    """Items that can live inside the same <hp:t> wrapper."""
    return isinstance(item, (CharItem, TabItem, LineBreakItem))


def _emit_inline_child(item: Item) -> str:
    """Emit ONE hp:t-inner child (no surrounding <hp:t>)."""
    if isinstance(item, CharItem):
        if not item.text:
            return ""
        if MARKER_OPEN not in item.text:
            return escape_text(item.text)
        # sentinel-wrapped raw markers — same logic as _emit_char_item body
        out: list[str] = []
        pos = 0
        n = len(item.text)
        while pos < n:
            op = item.text.find(MARKER_OPEN, pos)
            if op < 0:
                out.append(escape_text(item.text[pos:]))
                break
            if op > pos:
                out.append(escape_text(item.text[pos:op]))
            cl = item.text.find(MARKER_CLOSE, op + 1)
            if cl < 0:
                out.append(escape_text(item.text[op + 1:]))
                break
            out.append(item.text[op + 1:cl])
            pos = cl + 1
        return "".join(out)
    if isinstance(item, TabItem):
        if item.tab_attrs:
            attrs_str = "".join(
                f' {k}="{escape_attr(str(v))}"' for k, v in item.tab_attrs.items()
            )
            return f"<hp:tab{attrs_str}/>"
        return "<hp:tab/>"
    if isinstance(item, LineBreakItem):
        return "<hp:lineBreak/>"
    return ""


def _emit_run_children(items: list[Item]) -> str:
    """Coalesce contiguous text-inline items into one <hp:t> per
    charStyleIDRef block. Non-inline items emit as before.
    """
    parts: list[str] = []
    pending: list[Item] = []
    pending_style: object = "__unset__"

    def flush_pending():
        nonlocal pending, pending_style
        if not pending:
            return
        # Build hp:t opening attrs
        if isinstance(pending_style, str) and pending_style != "__unset__":
            attrs = f' charStyleIDRef="{escape_attr(pending_style)}"'
        else:
            attrs = ""
        body = "".join(_emit_inline_child(it) for it in pending)
        # If only one CharItem with empty text → emit self-close hp:t
        if not body:
            parts.append(f"<hp:t{attrs}/>")
        else:
            parts.append(f"<hp:t{attrs}>{body}</hp:t>")
        pending = []
        pending_style = "__unset__"

    for it in items:
        if _is_text_inline(it):
            it_style = it.char_style if isinstance(it, CharItem) else None
            if pending_style == "__unset__":
                pending_style = it_style
                pending.append(it)
            elif it_style == pending_style or it_style is None:
                # same style or tab/lineBreak (no own style) → keep merging
                pending.append(it)
            else:
                # style change → flush + start new group
                flush_pending()
                pending_style = it_style
                pending.append(it)
        else:
            flush_pending()
            parts.append(_emit_item(it))
    flush_pending()
    return "".join(parts)


def _item_cs(item: Item) -> int:
    return getattr(item, "char_shape_id", 0) or 0


def _emit_item(item: Item) -> str:
    if isinstance(item, CharItem):
        return _emit_char_item(item)
    if isinstance(item, TabItem):
        if item.tab_attrs:
            attrs_str = "".join(
                f' {k}="{escape_attr(str(v))}"' for k, v in item.tab_attrs.items()
            )
            return f"<hp:tab{attrs_str}/>"
        return "<hp:tab/>"
    if isinstance(item, LineBreakItem):
        return "<hp:lineBreak/>"
    if isinstance(item, SectionMeta):
        return item.raw_xml
    if isinstance(item, (ColumnDef, LayoutDef, ScopeDef, NoteDef)):
        return item.raw_xml
    if isinstance(item, OpaqueInlineItem):
        return item.xml
    if isinstance(item, TableItem):
        return _emit_table(item)
    return ""


def _emit_char_item(c: CharItem) -> str:
    """<hp:t> with optional charStyleIDRef. Sentinel raw markers pass through."""
    attrs_str = ""
    if c.char_style is not None:
        attrs_str = f' charStyleIDRef="{escape_attr(c.char_style)}"'
    if not c.text:
        return f"<hp:t{attrs_str}/>"

    # Handle sentinel-wrapped raw markers
    if MARKER_OPEN not in c.text:
        body = escape_text(c.text)
        return f"<hp:t{attrs_str}>{body}</hp:t>"

    parts: list[str] = []
    pos = 0
    n = len(c.text)
    while pos < n:
        op = c.text.find(MARKER_OPEN, pos)
        if op < 0:
            parts.append(escape_text(c.text[pos:]))
            break
        if op > pos:
            parts.append(escape_text(c.text[pos:op]))
        cl = c.text.find(MARKER_CLOSE, op + 1)
        if cl < 0:
            parts.append(escape_text(c.text[op + 1:]))
            break
        parts.append(c.text[op + 1:cl])  # raw XML — no escape
        pos = cl + 1
    body = "".join(parts)
    return f"<hp:t{attrs_str}>{body}</hp:t>"


def _emit_table(t: TableItem) -> str:
    open_tag = emit_open_tag("hp:tbl", t.table_attrs)
    parts = [open_tag, t.pre_rows_xml]
    # Group cells into rows by RowAddr (preserve cells order).
    current_row: str | None = None
    for cell in t.cells:
        row_addr = cell.cell_attrs.get("rowAddr") or _extract_row_addr(cell)
        if row_addr != current_row:
            if current_row is not None:
                parts.append("</hp:tr>")
            parts.append("<hp:tr>")
            current_row = row_addr
        parts.append(_emit_cell(cell))
    if current_row is not None:
        parts.append("</hp:tr>")
    parts.append("</hp:tbl>")
    return "".join(parts)


def _extract_row_addr(cell: CellItem) -> str:
    """Fallback: dig <hp:cellAddr rowAddr="..."> from cell_meta_xml."""
    m = re.search(r'<hp:cellAddr\b[^>]*\browAddr="([^"]+)"', cell.cell_meta_xml)
    return m.group(1) if m else "0"


def _emit_cell(c: CellItem) -> str:
    open_tag = emit_open_tag("hp:tc", c.cell_attrs)
    sublist_open = emit_open_tag("hp:subList", c.sublist_attrs)
    inner_paras = "".join(_emit_paragraph(p) for p in c.paragraphs)
    return (
        f"{open_tag}{sublist_open}{inner_paras}</hp:subList>"
        f"{c.cell_meta_xml}</hp:tc>"
    )


# ============================================================
# content.hpf emission — patch <opf:item> entries
# ============================================================

def _emit_content_hpf(doc: HwpxDocument) -> str:
    """Patch the manifest's <opf:item> list to reflect doc.bin_items.

    Strategy: take the original content.hpf as template, locate the
    <opf:manifest> block, and replace ONLY the BinData/* item entries.
    Preserve metadata, spine, and non-binary items (header, masterpages,
    settings) untouched.
    """
    text = doc.content_hpf_template
    if not text:
        # Synthesize minimal manifest if template missing
        return _synthesize_content_hpf(doc)

    # Locate <opf:manifest>...</opf:manifest>
    mf_open_m = re.search(r"<opf:manifest\b[^>]*>", text)
    if not mf_open_m:
        return text  # Can't patch — return as-is
    mf_body_start = mf_open_m.end()
    mf_close_idx = text.find("</opf:manifest>", mf_body_start)
    if mf_close_idx < 0:
        return text
    mf_body = text[mf_body_start:mf_close_idx]

    # Strip existing BinData/* items from manifest body. Use quote-aware
    # tag-end scan because media-type / href contain '/'.
    from ._xml import find_tag_end
    surviving_items: list[str] = []
    pos = 0
    while pos < len(mf_body):
        lt = mf_body.find("<opf:item", pos)
        if lt < 0:
            surviving_items.append(mf_body[pos:])
            break
        if pos < lt:
            surviving_items.append(mf_body[pos:lt])
        nx = mf_body[lt + len("<opf:item"):lt + len("<opf:item") + 1]
        if nx not in (" ", "\t", "\n", ">"):
            surviving_items.append(mf_body[lt:lt + 1])
            pos = lt + 1
            continue
        gt = find_tag_end(mf_body, lt)
        if gt < 0:
            surviving_items.append(mf_body[lt:])
            break
        seg = mf_body[lt:gt + 1]
        if 'href="BinData/' not in seg:
            surviving_items.append(seg)
        pos = gt + 1

    # Build new BinData entries from doc.bin_items
    new_bd_items: list[str] = []
    for bi in doc.bin_items:
        new_bd_items.append(
            f'<opf:item id="{escape_attr(bi.manifest_id)}" '
            f'href="{escape_attr(bi.href)}" '
            f'media-type="{escape_attr(bi.media_type)}" '
            f'isEmbeded="{escape_attr(bi.is_embedded)}"/>'
        )

    new_mf_body = "".join(surviving_items + new_bd_items)
    new_text = (
        text[:mf_body_start] + new_mf_body + text[mf_close_idx:]
    )
    return new_text


def _synthesize_content_hpf(doc: HwpxDocument) -> str:
    """Minimal HWPX content.hpf when template is unavailable."""
    parts = [
        '<?xml version="1.0" encoding="UTF-8" standalone="yes" ?>',
        '<opf:package xmlns:opf="http://www.idpf.org/2007/opf/" '
        'xmlns:dc="http://purl.org/dc/elements/1.1/" version="" '
        'unique-identifier="" id="">',
        '<opf:metadata></opf:metadata>',
        '<opf:manifest>',
        '<opf:item id="header" href="Contents/header.xml" media-type="application/xml"/>',
    ]
    for sec in doc.sections:
        parts.append(
            f'<opf:item id="section{sec.section_index}" '
            f'href="Contents/section{sec.section_index}.xml" '
            f'media-type="application/xml"/>'
        )
    for bi in doc.bin_items:
        parts.append(
            f'<opf:item id="{escape_attr(bi.manifest_id)}" '
            f'href="{escape_attr(bi.href)}" '
            f'media-type="{escape_attr(bi.media_type)}" '
            f'isEmbeded="{escape_attr(bi.is_embedded)}"/>'
        )
    parts.append('</opf:manifest>')
    parts.append('<opf:spine>')
    parts.append('<opf:itemref idref="header" linear="yes"/>')
    for sec in doc.sections:
        parts.append(
            f'<opf:itemref idref="section{sec.section_index}" linear="yes"/>'
        )
    parts.append('</opf:spine>')
    parts.append('</opf:package>')
    return "".join(parts)

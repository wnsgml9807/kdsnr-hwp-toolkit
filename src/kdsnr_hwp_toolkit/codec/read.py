"""HWPX zip → HwpxDocument.

Strategy: parse the few XML files we need to mutate (header.xml,
section[N].xml, content.hpf manifest items) and capture EVERY other zip
entry as raw bytes for byte-perfect passthrough on write.

This guarantees that things we don't touch (mimetype, version.xml,
settings.xml, masterpage[N].xml, META-INF/*, Preview/*) survive the
round-trip identically to the source — which is exactly what we need for
Hanword compatibility.
"""

from __future__ import annotations

import io
import re
import zipfile
from typing import Optional

from ._xml import (
    TAG_NAME,
    find_matching_close,
    find_tag_end,
    iter_direct_children,
    parse_attrs,
    parse_open_tag,
)
from .schema import (
    BinItem,
    CellItem,
    CharItem,
    ColumnDef,
    EmptyRunItem,
    FaceNameList,
    FontFace,
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


# hp:ctrl inner-tag → typed structural class. Probed across 4 templates +
# sample HWP→HWPX outputs: only colPr/header/footer/autoNum/newNum seen,
# but listing the full davinci-parity set so future docs are routed cleanly.
_CTRL_LAYOUT_TAGS = frozenset({
    "hp:colPr", "hp:autoNum", "hp:newNum",
    "hp:pageNumPos", "hp:pageHiding", "hp:pageNumCtrl",
})
_CTRL_SCOPE_TAGS = frozenset({
    "hp:header", "hp:footer", "hp:masterPage",
})
_CTRL_NOTE_TAGS = frozenset({
    "hp:footNote", "hp:endNote",
})


def _ctrl_inner_first_tag(ctrl_xml: str) -> str:
    """Given a full <hp:ctrl>…</hp:ctrl> string, return the first child tag
    name (e.g. 'hp:header'). Returns '' if no inner element found.
    """
    after_open = ctrl_xml.find(">")
    if after_open < 0:
        return ""
    m = re.search(rf"<({TAG_NAME})", ctrl_xml[after_open + 1:])
    return m.group(1) if m else ""


# ============================================================
# Public entry
# ============================================================

def read(data: bytes, doc_id: Optional[str] = None) -> HwpxDocument:
    """Parse HWPX zip bytes into HwpxDocument."""
    zf = zipfile.ZipFile(io.BytesIO(data))
    names = zf.namelist()

    # Categorize zip entries
    raw_xml_files: dict[str, bytes] = {}
    section_files: dict[int, bytes] = {}   # section_idx → bytes
    bin_data_files: dict[str, bytes] = {}  # filename (BinData/...) → bytes
    header_bytes: bytes | None = None
    content_hpf_text: str = ""

    section_re = re.compile(r"^Contents/section(\d+)\.xml$")

    for name in names:
        if name.endswith("/"):
            continue
        with zf.open(name) as fh:
            payload = fh.read()
        if name == "Contents/header.xml":
            header_bytes = payload
        elif name == "Contents/content.hpf":
            content_hpf_text = payload.decode("utf-8")
        elif (m := section_re.match(name)):
            section_files[int(m.group(1))] = payload
        elif name.startswith("BinData/"):
            bin_data_files[name] = payload
        else:
            raw_xml_files[name] = payload

    if header_bytes is None:
        raise ValueError("HWPX 누락: Contents/header.xml")
    if not section_files:
        raise ValueError("HWPX 누락: Contents/sectionN.xml")
    if not content_hpf_text:
        raise ValueError("HWPX 누락: Contents/content.hpf")

    # Parse header.xml → StyleTable
    header_text = header_bytes.decode("utf-8")
    (
        styles,
        header_xml_decl,
        header_root_attrs,
        header_pre_lists_xml,
        header_post_lists_xml,
    ) = _parse_header_xml(header_text)

    # Parse content.hpf manifest → list of bin item descriptors
    bin_items = _parse_content_hpf(content_hpf_text, bin_data_files)

    # Parse each section
    sections: list[Section] = []
    for idx in sorted(section_files.keys()):
        sections.append(_parse_section(
            section_files[idx].decode("utf-8"), section_index=idx,
        ))

    return HwpxDocument(
        sections=tuple(sections),
        styles=styles,
        bin_items=tuple(bin_items),
        raw_xml_files=raw_xml_files,
        content_hpf_template=content_hpf_text,
        header_xml_decl=header_xml_decl,
        header_root_attrs=header_root_attrs,
        header_pre_lists_xml=header_pre_lists_xml,
        header_post_lists_xml=header_post_lists_xml,
        doc_id=doc_id,
    )


# ============================================================
# header.xml — catalog parsing
# ============================================================

# Mapping of header list-container tag → (entry_tag, schema_field).
# Order matters for emit; read accepts any order (we scan whole doc).
_HEADER_LISTS: tuple[tuple[str, str, str], ...] = (
    ("hh:borderFills", "hh:borderFill", "border_fills"),
    ("hh:charProperties", "hh:charPr", "char_shapes"),
    ("hh:tabProperties", "hh:tabPr", "tab_defs"),
    ("hh:numberings", "hh:numbering", "numberings"),
    ("hh:bullets", "hh:bullet", "bullets"),
    ("hh:paraProperties", "hh:paraPr", "para_shapes"),
    ("hh:styles", "hh:style", "styles"),
)


def _parse_header_xml(text: str) -> tuple[StyleTable, str, str, str, str]:
    """header.xml → (StyleTable, xml_decl, root_attrs_str, pre_lists, post_lists).

    pre_lists is the raw XML between <hh:head> open tag and the FIRST list
    container we recognize (typically holds <hh:beginNum/> + <hh:refList>).
    post_lists is from the end of the LAST list to </hh:head>
    (trackchageConfig, compatibleDocument, etc).
    """
    # XML declaration
    decl_m = re.match(r"\s*<\?xml[^?]*\?>\s*", text)
    xml_decl = decl_m.group(0) if decl_m else ""

    # <hh:head ...> open tag
    head_m = re.search(r"<hh:head\b([^>]*)>", text)
    if not head_m:
        raise ValueError("Contents/header.xml: <hh:head> 누락")
    head_open_end = head_m.end()
    head_close_idx = text.rfind("</hh:head>")
    if head_close_idx < 0:
        raise ValueError("Contents/header.xml: </hh:head> 누락")
    root_attrs_str = head_m.group(1)
    inner = text[head_open_end:head_close_idx]

    # Find first list container start position
    list_starts: list[tuple[int, int, str, str, str]] = []
    for list_tag, entry_tag, field_name in _HEADER_LISTS:
        m = re.search(rf"<{re.escape(list_tag)}\b[^>]*>", inner)
        if m:
            close_idx = inner.find(f"</{list_tag}>", m.end())
            if close_idx < 0:
                raise ValueError(f"header.xml: </{list_tag}> 누락")
            list_starts.append(
                (m.start(), close_idx + len(f"</{list_tag}>"),
                 list_tag, entry_tag, field_name)
            )
    # Also handle <hh:fontfaces>
    ff_m = re.search(r"<hh:fontfaces\b[^>]*>", inner)
    ff_close_end = -1
    if ff_m:
        ff_close = inner.find("</hh:fontfaces>", ff_m.end())
        if ff_close < 0:
            raise ValueError("header.xml: </hh:fontfaces> 누락")
        ff_close_end = ff_close + len("</hh:fontfaces>")
        list_starts.append(
            (ff_m.start(), ff_close_end, "hh:fontfaces", "hh:fontface", "face_names")
        )

    if not list_starts:
        # No catalog lists at all — unusual but handle gracefully
        return StyleTable(), xml_decl, root_attrs_str, inner, ""

    list_starts.sort(key=lambda t: t[0])
    pre_lists = inner[:list_starts[0][0]]
    post_lists = inner[list_starts[-1][1]:]

    # Extract entries
    fields: dict[str, tuple] = {
        "border_fills": (), "char_shapes": (), "tab_defs": (),
        "numberings": (), "bullets": (), "para_shapes": (), "styles": (),
        "face_names": (),
    }
    for ls_start, ls_end, list_tag, entry_tag, field_name in list_starts:
        if field_name == "face_names":
            fields["face_names"] = _parse_fontfaces(inner[ls_start:ls_end])
        else:
            body_start = inner.find(">", ls_start) + 1
            body_end = inner.find(f"</{list_tag}>", body_start)
            entries = _parse_entry_list(
                inner[body_start:body_end], entry_tag,
            )
            fields[field_name] = entries

    return (
        StyleTable(
            face_names=fields["face_names"],
            border_fills=fields["border_fills"],
            char_shapes=fields["char_shapes"],
            tab_defs=fields["tab_defs"],
            numberings=fields["numberings"],
            bullets=fields["bullets"],
            para_shapes=fields["para_shapes"],
            styles=fields["styles"],
        ),
        xml_decl,
        root_attrs_str,
        pre_lists,
        post_lists,
    )


def _parse_entry_list(body: str, entry_tag: str) -> tuple[StyleEntry, ...]:
    """Parse a flat list of <ENTRY id="N" .../> or <ENTRY id="N" ...>...</ENTRY>."""
    entries: list[StyleEntry] = []
    pos = 0
    open_re = rf"<{re.escape(entry_tag)}\b"
    while pos < len(body):
        m = re.search(open_re, body[pos:])
        if not m:
            break
        lt = pos + m.start()
        gt = find_tag_end(body, lt)
        if gt < 0:
            break
        open_tag = body[lt:gt + 1]
        id_m = re.search(r'\bid="(\d+)"', open_tag)
        if not id_m:
            pos = gt + 1
            continue
        eid = int(id_m.group(1))
        if open_tag.endswith("/>"):
            entries.append(StyleEntry(id=eid, xml=open_tag))
            pos = gt + 1
            continue
        close = find_matching_close(body, lt, entry_tag)
        if close <= 0:
            break
        entries.append(StyleEntry(id=eid, xml=body[lt:close]))
        pos = close
    return tuple(entries)


def _parse_fontfaces(xml: str) -> tuple[FaceNameList, ...]:
    """<hh:fontfaces itemCnt="7"><hh:fontface lang="X" fontCnt="N">FONT*</hh:fontface>+</hh:fontfaces>"""
    list_open = re.match(r"<hh:fontfaces\b[^>]*>", xml)
    if not list_open:
        return ()
    body_start = list_open.end()
    body_end = xml.rfind("</hh:fontfaces>")
    body = xml[body_start:body_end]

    blocks: list[FaceNameList] = []
    pos = 0
    while pos < len(body):
        m = re.search(r"<hh:fontface\b", body[pos:])
        if not m:
            break
        lt = pos + m.start()
        gt = find_tag_end(body, lt)
        if gt < 0:
            break
        open_tag = body[lt:gt + 1]
        attrs = parse_attrs(open_tag)
        lang = attrs.pop("lang", "")
        if open_tag.endswith("/>"):
            blocks.append(FaceNameList(lang=lang, fonts=(), raw_attrs=attrs))
            pos = gt + 1
            continue
        close = find_matching_close(body, lt, "hh:fontface")
        if close <= 0:
            break
        inner = body[gt + 1:close - len("</hh:fontface>")]
        fonts = _parse_font_entries(inner)
        blocks.append(FaceNameList(lang=lang, fonts=fonts, raw_attrs=attrs))
        pos = close
    return tuple(blocks)


def _parse_font_entries(inner: str) -> tuple[FontFace, ...]:
    fonts: list[FontFace] = []
    pos = 0
    while pos < len(inner):
        m = re.search(r"<hh:font\b", inner[pos:])
        if not m:
            break
        lt = pos + m.start()
        gt = find_tag_end(inner, lt)
        if gt < 0:
            break
        open_tag = inner[lt:gt + 1]
        id_m = re.search(r'\bid="(\d+)"', open_tag)
        if not id_m:
            pos = gt + 1
            continue
        fid = int(id_m.group(1))
        if open_tag.endswith("/>"):
            fonts.append(FontFace(id=fid, xml=open_tag))
            pos = gt + 1
            continue
        close = find_matching_close(inner, lt, "hh:font")
        if close <= 0:
            break
        fonts.append(FontFace(id=fid, xml=inner[lt:close]))
        pos = close
    return tuple(fonts)


# ============================================================
# content.hpf — manifest items
# ============================================================

def _parse_content_hpf(text: str, bin_data_files: dict[str, bytes]) -> list[BinItem]:
    """Extract <opf:item> entries that point to BinData/* files.

    Cannot use a simple regex like `<opf:item[^/]*/>` because attribute
    values legitimately contain `/` (media-type="image/png", href contains
    path slashes). Use quote-aware tag-end scanning instead.
    """
    bin_items: list[BinItem] = []
    pos = 0
    while pos < len(text):
        lt = text.find("<opf:item", pos)
        if lt < 0:
            break
        # boundary check
        nx = text[lt + len("<opf:item"):lt + len("<opf:item") + 1]
        if nx not in (" ", "\t", "\n", ">"):
            pos = lt + 1
            continue
        gt = find_tag_end(text, lt)
        if gt < 0:
            break
        seg = text[lt:gt + 1]
        attrs = parse_attrs(seg)
        href = attrs.get("href", "")
        pos = gt + 1
        if not href.startswith("BinData/"):
            continue
        data = bin_data_files.get(href)
        if data is None:
            continue
        bin_items.append(BinItem(
            manifest_id=attrs.get("id", ""),
            href=href,
            media_type=attrs.get("media-type", ""),
            # Default to "1" (embedded): file IS present in the zip per
            # the data check above. rhwp's HWP→HWPX manifest omits the
            # attribute entirely, and a missing attr must NOT mean
            # external — that breaks Hanword image rendering.
            is_embedded=attrs.get("isEmbeded", "1"),
            data=data,
        ))
    return bin_items


# ============================================================
# Section parsing
# ============================================================

def _parse_section(text: str, *, section_index: int) -> Section:
    decl_m = re.match(r"\s*<\?xml[^?]*\?>\s*", text)
    xml_decl = decl_m.group(0) if decl_m else ""

    # Root may be <hs:sec ...> in standard HWPX
    root_m = re.search(rf"<({TAG_NAME})\b([^>]*)>", text[len(xml_decl):])
    if not root_m:
        raise ValueError(f"section{section_index}.xml: 루트 태그 누락")
    root_tag = root_m.group(1)
    root_attrs_str = root_m.group(2)
    body_start_abs = len(xml_decl) + root_m.end()
    body_close_idx = text.rfind(f"</{root_tag}>")

    paragraphs: list[Paragraph] = []
    for ps, pe, tag in iter_direct_children(text, body_start_abs, body_close_idx):
        if tag == "hp:p":
            paragraphs.append(_parse_paragraph(text[ps:pe]))

    return Section(
        body=tuple(paragraphs),
        raw_root_attrs=root_attrs_str,
        raw_xml_decl=xml_decl,
        section_index=section_index,
    )


def _parse_paragraph(p_xml: str) -> Paragraph:
    open_m = re.match(r"<hp:p\b([^>]*?)(/)?>", p_xml)
    if not open_m:
        raise ValueError(f"<hp:p> 파싱 실패: {p_xml[:80]!r}")
    attrs = parse_attrs(open_m.group(1))
    is_self_close = open_m.group(2) == "/"

    para_shape_id = int(attrs.get("paraPrIDRef", "0"))
    style_id = int(attrs.get("styleIDRef", "0"))
    starts_new_page = attrs.get("pageBreak", "0") == "1"
    starts_new_column = attrs.get("columnBreak", "0") == "1"
    merged = attrs.get("merged", "0") == "1"
    para_id_attr = attrs.get("id", "0")

    if is_self_close:
        return Paragraph(
            items=(),
            para_shape_id=para_shape_id,
            style_id=style_id,
            starts_new_page=starts_new_page,
            starts_new_column=starts_new_column,
            merged=merged,
            para_id_attr=para_id_attr,
            raw_attrs=attrs,
        )

    inner_start = open_m.end()
    inner_end = p_xml.rfind("</hp:p>")
    items, first_cs, linesegs_xml = _parse_p_inner(p_xml[inner_start:inner_end])

    return Paragraph(
        items=items,
        para_shape_id=para_shape_id,
        style_id=style_id,
        char_shape_id_first=first_cs,
        starts_new_page=starts_new_page,
        starts_new_column=starts_new_column,
        merged=merged,
        para_id_attr=para_id_attr,
        raw_attrs=attrs,
        linesegs_xml=linesegs_xml,
    )


def _parse_p_inner(inner: str) -> tuple[tuple[Item, ...], int, str]:
    """Parse <hp:p>'s inner content. <hp:p> contains <hp:run>+ at top level
    (per HWPX spec). Each run wraps zero or more children.

    Also captures the trailing <hp:linesegarray> (layout cache) verbatim
    so split_paper can preserve Hancom's authoritative line-break positions
    on paragraphs whose content didn't change.
    """
    items: list[Item] = []
    first_cs = -1
    linesegs_xml = ""

    pos = 0
    n = len(inner)
    while pos < n:
        # Skip whitespace and find next element
        lt = inner.find("<", pos)
        if lt < 0:
            break
        gt = find_tag_end(inner, lt)
        if gt < 0:
            break
        seg = inner[lt:gt + 1]
        m = re.match(rf"<({TAG_NAME})", seg)
        if not m:
            pos = gt + 1
            continue
        tag = m.group(1)
        is_self = seg.endswith("/>")

        if tag == "hp:run":
            attrs = parse_attrs(seg)
            cs_id = int(attrs.get("charPrIDRef", "0"))
            if first_cs < 0:
                first_cs = cs_id
            if is_self:
                items.append(EmptyRunItem(char_shape_id=cs_id, starts_new_run=True))
                pos = gt + 1
                continue
            close_end = find_matching_close(inner, lt, "hp:run")
            if close_end <= 0:
                break
            run_inner = inner[gt + 1:close_end - len("</hp:run>")]
            run_items = _parse_run_inner(run_inner, cs_id)
            # Mark first item of this run so writer can preserve the boundary
            # even when adjacent items share char_shape_id.
            if run_items:
                from dataclasses import replace as _dc_replace
                run_items[0] = _dc_replace(run_items[0], starts_new_run=True)
            elif not run_items:
                # Empty run with no parseable children — emit placeholder
                items.append(EmptyRunItem(char_shape_id=cs_id, starts_new_run=True))
            items.extend(run_items)
            pos = close_end
        elif tag == "hp:linesegarray":
            # Capture verbatim. Hancom's wrap positions are authoritative
            # for unchanged paragraphs; split_paper passes this through.
            if is_self:
                linesegs_xml = seg
                pos = gt + 1
            else:
                close_end = find_matching_close(inner, lt, "hp:linesegarray")
                if close_end > 0:
                    linesegs_xml = inner[lt:close_end]
                    pos = close_end
                else:
                    pos = gt + 1
        else:
            # Unexpected child of <hp:p>. Skip (preserved as opaque inside
            # synthetic run? for now, treat as opaque without char_shape).
            if is_self:
                pos = gt + 1
            else:
                close_end = find_matching_close(inner, lt, tag)
                pos = close_end if close_end > 0 else gt + 1

    if first_cs < 0:
        first_cs = 0
    return tuple(items), first_cs, linesegs_xml


def _parse_run_inner(inner: str, char_shape_id: int) -> list[Item]:
    """Children of <hp:run>. Per probe, the universe is large:
      hp:t (text), hp:tab (sibling form), hp:lineBreak,
      hp:tbl, hp:pic, hp:equation, hp:rect/ellipse/line/gso/textart,
      hp:secPr, hp:colPr, hp:newNum, hp:autoNum, hp:autoNumFormat,
      hp:fwSpace, hp:nbSpace, hp:hyphen,
      hp:bookmark, hp:fieldBegin, hp:fieldEnd, hp:hiddenComment,
      hp:footNote, hp:endNote, hp:footNotePr, hp:endNotePr,
      hp:ctrl, hp:masterPage, ...

    Strategy:
      hp:t → CharItem (parse text + nested hp:tab as inline)
      hp:tab (sibling) → TabItem
      hp:lineBreak → LineBreakItem
      hp:tbl → TableItem (recurse cells)
      hp:secPr → SectionMeta (raw_xml preserved — section-level metadata)
      hp:colPr (inline) → ColumnDef (raw_xml preserved — column section)
      everything else → OpaqueInlineItem (raw xml preserved)
    """
    items: list[Item] = []
    pos = 0
    n = len(inner)
    while pos < n:
        lt = inner.find("<", pos)
        if lt < 0:
            break
        gt = find_tag_end(inner, lt)
        if gt < 0:
            break
        seg = inner[lt:gt + 1]
        m = re.match(rf"<({TAG_NAME})", seg)
        if not m:
            pos = gt + 1
            continue
        tag = m.group(1)
        is_self = seg.endswith("/>")

        if tag == "hp:t":
            if is_self:
                # Empty <hp:t/> — preserve as zero-length CharItem
                items.append(CharItem(text="", char_shape_id=char_shape_id))
                pos = gt + 1
                continue
            close_end = find_matching_close(inner, lt, "hp:t")
            if close_end <= 0:
                break
            attrs = parse_attrs(seg)
            cs_style = attrs.get("charStyleIDRef")
            t_inner = inner[gt + 1:close_end - len("</hp:t>")]
            for ci in _decode_t_inner(t_inner, char_shape_id, cs_style):
                items.append(ci)
            pos = close_end

        elif tag == "hp:tab":
            attrs = parse_attrs(seg)
            items.append(TabItem(char_shape_id=char_shape_id, tab_attrs=attrs))
            pos = gt + 1 if is_self else _skip_full(inner, lt, tag, gt)

        elif tag == "hp:lineBreak":
            items.append(LineBreakItem(char_shape_id=char_shape_id))
            pos = gt + 1 if is_self else _skip_full(inner, lt, tag, gt)

        elif tag == "hp:secPr":
            full = _full_element(inner, lt, gt, is_self, "hp:secPr")
            items.append(SectionMeta(raw_xml=full, char_shape_id=char_shape_id))
            pos = lt + len(full)

        elif tag == "hp:colPr":
            full = _full_element(inner, lt, gt, is_self, "hp:colPr")
            items.append(ColumnDef(raw_xml=full, char_shape_id=char_shape_id))
            pos = lt + len(full)

        elif tag == "hp:tbl":
            close_end = find_matching_close(inner, lt, "hp:tbl")
            if close_end <= 0:
                break
            items.append(_parse_table(inner[lt:close_end], char_shape_id))
            pos = close_end

        elif tag == "hp:ctrl":
            # Route hp:ctrl by inner tag into typed structural classes
            # (LayoutDef / ScopeDef / NoteDef). Davinci AttachedDef parity.
            full = _full_element(inner, lt, gt, is_self, "hp:ctrl")
            inner_tag = _ctrl_inner_first_tag(full)
            if inner_tag in _CTRL_LAYOUT_TAGS:
                items.append(LayoutDef(
                    raw_xml=full, inner_tag=inner_tag, char_shape_id=char_shape_id,
                ))
            elif inner_tag in _CTRL_SCOPE_TAGS:
                items.append(ScopeDef(
                    raw_xml=full, inner_tag=inner_tag, char_shape_id=char_shape_id,
                ))
            elif inner_tag in _CTRL_NOTE_TAGS:
                items.append(NoteDef(
                    raw_xml=full, inner_tag=inner_tag, char_shape_id=char_shape_id,
                ))
            else:
                # Unknown ctrl — keep opaque so round-trip stays byte-equal
                items.append(OpaqueInlineItem(
                    tag="hp:ctrl", xml=full, char_shape_id=char_shape_id,
                ))
            pos = lt + len(full)

        else:
            # Catch-all: opaque preserved
            full = _full_element(inner, lt, gt, is_self, tag)
            items.append(OpaqueInlineItem(
                tag=tag, xml=full, char_shape_id=char_shape_id,
            ))
            pos = lt + len(full)

    return items


def _full_element(inner: str, lt: int, gt: int,
                  is_self: bool, tag: str) -> str:
    if is_self:
        return inner[lt:gt + 1]
    close_end = find_matching_close(inner, lt, tag)
    if close_end <= 0:
        return inner[lt:gt + 1]
    return inner[lt:close_end]


def _skip_full(inner: str, lt: int, tag: str, gt: int) -> int:
    close_end = find_matching_close(inner, lt, tag)
    return close_end if close_end > 0 else gt + 1


# ============================================================
# <hp:t> inner → CharItem(s)
# ============================================================

# Sentinel marks for in-text raw markers (rarely used — HWPX text rarely has
# nested unknown markers, but kept for symmetry with davinci codec).
MARKER_OPEN = ""
MARKER_CLOSE = ""


def _decode_t_inner(t_inner: str, cs_id: int,
                    cs_style: Optional[str]) -> list[Item]:
    """Decode a <hp:t> inner. Splits on nested <hp:tab>/<hp:lineBreak> etc.

    HWPX <hp:t> can contain plain text + nested <hp:tab .../>, as observed
    in samples. We split into CharItem segments + TabItem/LineBreakItem
    between them, all sharing the parent run's char_shape_id.
    """
    out: list[Item] = []
    if "<" not in t_inner:
        # Pure text — single CharItem.
        if t_inner:
            out.append(CharItem(
                text=_xml_unescape(t_inner),
                char_shape_id=cs_id,
                char_style=cs_style,
            ))
        return out

    pos = 0
    n = len(t_inner)
    pending: list[str] = []

    def flush():
        if pending:
            out.append(CharItem(
                text="".join(pending),
                char_shape_id=cs_id,
                char_style=cs_style,
            ))
            pending.clear()

    while pos < n:
        lt = t_inner.find("<", pos)
        if lt < 0:
            pending.append(_xml_unescape(t_inner[pos:]))
            break
        if lt > pos:
            pending.append(_xml_unescape(t_inner[pos:lt]))
        gt = find_tag_end(t_inner, lt)
        if gt < 0:
            pending.append(_xml_unescape(t_inner[lt:]))
            break
        seg = t_inner[lt:gt + 1]
        m = re.match(rf"<({TAG_NAME})", seg)
        if not m:
            pending.append(seg)
            pos = gt + 1
            continue
        tag = m.group(1)
        is_self = seg.endswith("/>")
        if tag == "hp:tab":
            flush()
            attrs = parse_attrs(seg)
            out.append(TabItem(char_shape_id=cs_id, tab_attrs=attrs))
            pos = gt + 1 if is_self else _skip_full(t_inner, lt, tag, gt)
        elif tag == "hp:lineBreak":
            flush()
            out.append(LineBreakItem(char_shape_id=cs_id))
            pos = gt + 1 if is_self else _skip_full(t_inner, lt, tag, gt)
        else:
            # Unknown nested marker — preserve raw XML inside CharItem text
            # via sentinel pair. Writer emits it as raw bytes.
            flush()
            full = _full_element(t_inner, lt, gt, is_self, tag)
            out.append(CharItem(
                text=f"{MARKER_OPEN}{full}{MARKER_CLOSE}",
                char_shape_id=cs_id,
                char_style=cs_style,
            ))
            pos = lt + len(full)
    flush()
    return out


def _xml_unescape(s: str) -> str:
    return (
        s.replace("&lt;", "<")
         .replace("&gt;", ">")
         .replace("&quot;", '"')
         .replace("&apos;", "'")
         .replace("&amp;", "&")
    )


# ============================================================
# Table parsing
# ============================================================

def _parse_table(xml: str, char_shape_id: int) -> TableItem:
    """<hp:tbl ...>(pre_rows)(<hp:tr><hp:tc>+</hp:tr>)+</hp:tbl>"""
    om = re.match(r"<hp:tbl\b([^>]*)>", xml)
    if not om:
        return TableItem(
            table_attrs={}, pre_rows_xml="", cells=(), char_shape_id=char_shape_id,
        )
    table_attrs = parse_attrs(om.group(1))
    inner_start = om.end()
    inner_end = xml.rfind("</hp:tbl>")
    inner = xml[inner_start:inner_end]

    # First <hp:tr> — everything before is pre_rows_xml
    first_tr = re.search(r"<hp:tr\b", inner)
    if not first_tr:
        return TableItem(
            table_attrs=table_attrs, pre_rows_xml=inner, cells=(),
            char_shape_id=char_shape_id,
        )
    pre_rows = inner[:first_tr.start()]

    cells: list[CellItem] = []
    pos = first_tr.start()
    while pos < len(inner):
        m = re.search(r"<hp:tr\b", inner[pos:])
        if not m:
            break
        tr_lt = pos + m.start()
        tr_close_end = find_matching_close(inner, tr_lt, "hp:tr")
        if tr_close_end <= 0:
            break
        tr_open_gt = find_tag_end(inner, tr_lt)
        tr_inner_start = tr_open_gt + 1
        tr_inner_end = tr_close_end - len("</hp:tr>")
        tr_inner = inner[tr_inner_start:tr_inner_end]
        # Each <hp:tc>
        c_pos = 0
        while c_pos < len(tr_inner):
            cm = re.search(r"<hp:tc\b", tr_inner[c_pos:])
            if not cm:
                break
            c_lt = c_pos + cm.start()
            c_close_end = find_matching_close(tr_inner, c_lt, "hp:tc")
            if c_close_end <= 0:
                break
            cells.append(_parse_cell(tr_inner[c_lt:c_close_end]))
            c_pos = c_close_end
        pos = tr_close_end

    return TableItem(
        table_attrs=table_attrs,
        pre_rows_xml=pre_rows,
        cells=tuple(cells),
        char_shape_id=char_shape_id,
    )


def _parse_cell(cell_xml: str) -> CellItem:
    """<hp:tc attrs><hp:subList attrs>P+</hp:subList>(meta)*</hp:tc>"""
    om = re.match(r"<hp:tc\b([^>]*)>", cell_xml)
    cell_attrs = parse_attrs(om.group(1)) if om else {}
    inner_start = om.end() if om else 0
    inner_end = cell_xml.rfind("</hp:tc>")
    inner = cell_xml[inner_start:inner_end]

    # <hp:subList ...>P+</hp:subList>
    sl_m = re.search(r"<hp:subList\b([^>]*)>", inner)
    sublist_attrs: dict = {}
    paragraphs: tuple[Paragraph, ...] = ()
    cell_meta_xml = inner

    if sl_m:
        sublist_attrs = parse_attrs(sl_m.group(1))
        sl_open_end = sl_m.end()
        # Use depth-aware match — a cell paragraph may contain a nested
        # <hp:tbl> whose cells have their own <hp:subList>, so a plain
        # find() would match the inner close.
        sl_lt = sl_m.start()
        sl_close_end = find_matching_close(inner, sl_lt, "hp:subList")
        if sl_close_end <= 0:
            sl_close_end = len(inner)
            sl_close = len(inner)
        else:
            sl_close = sl_close_end - len("</hp:subList>")
        sl_body = inner[sl_open_end:sl_close]
        # Direct <hp:p> children
        paras: list[Paragraph] = []
        for ps, pe, t in iter_direct_children(sl_body, 0, len(sl_body)):
            if t == "hp:p":
                paras.append(_parse_paragraph(sl_body[ps:pe]))
        paragraphs = tuple(paras)
        # Anything before the subList + after the </hp:subList> is cell meta
        cell_meta_xml = inner[:sl_m.start()] + inner[sl_close_end:]

    return CellItem(
        cell_attrs=cell_attrs,
        sublist_attrs=sublist_attrs,
        paragraphs=paragraphs,
        cell_meta_xml=cell_meta_xml,
    )

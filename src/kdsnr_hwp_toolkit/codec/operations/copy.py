"""Cross-document paragraph copy with catalog merge — HWPX adaptation of
davinci HWPML codec's copy.py.

Algorithm (transitive normalized dedupe — proven correct on davinci's 4
samples per probe cx_3b/cx_5/cx_7):

  Dependency order (canonicalize leaves → roots, dst-side):
    1. fontfaces (per-Lang), borderFills, tabPr, numberings, bin_items  [leaves]
    2. charPr     (refs fontfaces + borderFills)
    3. paraPr     (refs borderFills + tabPr + numberings)
    4. style      (refs paraPr + charPr + nextStyleIDRef)

For each src entry:
  - canonicalize (substitute inner refs with dst-side canon hash)
  - if dst has matching canon → REUSE (id_map[src] = matched_dst_id)
  - else → APPEND with rewritten inner refs + new id; if entry has Name,
    append " (copied from {src_doc_id})" suffix on Name collision

Then for each src paragraph, recursively rewrite all ref attributes via
id_maps to point at dst-side ids.

HWPX ref attribute catalog (cross-checked vs probe — strictly different
from HWPML names):
  paraPrIDRef        — used in <hp:p>, <hh:style>
  charPrIDRef        — used in <hp:run>, <hh:style>
  styleIDRef         — used in <hp:p>
  charStyleIDRef     — used in <hp:t> (HWPML <CHAR Style=>)
  nextStyleIDRef     — used in <hh:style>
  borderFillIDRef    — used in <hh:charPr>, <hh:paraPr>'s <hh:border> child,
                       <hp:tbl>, <hp:tc>, <hp:pageBorderFill>
  tabPrIDRef         — used in <hh:paraPr>
  numberingIDRef     — used in <hh:paraPr>'s <hh:heading>, <hh:numbering>'s heads
  linkListIDRef      — used in <hp:subList>
  linkListNextIDRef  — used in <hp:subList>
  binaryItemIDRef    — used in <hc:img> (string id like "image2", remapped)
  outlineShapeIDRef  — used in <hp:secPr>; references numberingIDRef space
  memoShapeIDRef     — used in <hp:secPr>; references memoProperties (we don't model)
  hh:fontRef hangul=, latin=, hanja=, japanese=, other=, symbol=, user=
                      — used in <hh:charPr> (lowercase Lang attrs)

Differences from HWPML codec:
  - bin_items use string ids ("imageN") not int
  - 1-level binary indirection (no separate BinRef list)
  - All numeric refs are 0-based (HWPX serializer registers idx as u16)
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field

from .._xml import find_tag_end, parse_attrs, TAG_NAME
from ..schema import (
    BinItem,
    CellItem,
    CharItem,
    ColumnDef,
    EmptyRunItem,
    FaceNameList,
    FontFace,
    HwpxDocument,
    Item,
    LANGS_LOWER,
    LayoutDef,
    LineBreakItem,
    NoteDef,
    OpaqueInlineItem,
    Paragraph,
    ScopeDef,
    SectionMeta,
    StyleEntry,
    StyleTable,
    TabItem,
    TableItem,
)


# ============================================================
# IdMaps — remap registry per (src→dst) migration
# ============================================================

@dataclass
class IdMaps:
    para_shapes: dict[int, int] = field(default_factory=dict)
    char_shapes: dict[int, int] = field(default_factory=dict)
    styles: dict[int, int] = field(default_factory=dict)
    border_fills: dict[int, int] = field(default_factory=dict)
    tab_defs: dict[int, int] = field(default_factory=dict)
    numberings: dict[int, int] = field(default_factory=dict)
    bullets: dict[int, int] = field(default_factory=dict)
    # face_names: lang(uppercase) → {src_id: dst_id}
    face_names: dict[str, dict[int, int]] = field(default_factory=dict)
    # bin_items: src manifest_id ("imageN") → dst manifest_id
    bin_items: dict[str, str] = field(default_factory=dict)


# ============================================================
# Canonicalization helpers — used to detect logical equality
# ============================================================

_WS_RE = re.compile(r"\s+")
_ATTR_FULL_RE = re.compile(rf'({TAG_NAME})="([^"]*)"')


def _normalize_attrs(attrs: dict[str, str], exclude: tuple[str, ...] = ()) -> tuple:
    return tuple(sorted((k, v) for k, v in attrs.items() if k not in exclude))


def _shallow_canon(xml: str, exclude_outer_attr: tuple[str, ...] = ("id",)) -> str:
    """Canonicalize an entry: outer attr-sorted (excluding id), inner ws-collapsed."""
    m = re.match(rf"<({TAG_NAME})\b([^>]*?)(/)?>", xml)
    if not m:
        return _WS_RE.sub(" ", xml).strip()
    tag = m.group(1)
    attrs = parse_attrs(m.group(2))
    self_close = m.group(3) == "/"
    filtered = {k: v for k, v in attrs.items() if k not in exclude_outer_attr}
    attrs_str = " ".join(f'{k}="{v}"' for k, v in sorted(filtered.items()))
    if self_close:
        return _WS_RE.sub(" ", f"<{tag} {attrs_str}/>" if attrs_str else f"<{tag}/>").strip()
    inner = xml[m.end():xml.rfind(f"</{tag}>")]
    inner_norm = _WS_RE.sub(" ", inner).strip()
    head = f"<{tag} {attrs_str}>" if attrs_str else f"<{tag}>"
    return _WS_RE.sub(" ", f"{head}{inner_norm}</{tag}>").strip()


def _find_entry(entries: tuple[StyleEntry, ...], eid: int) -> StyleEntry | None:
    for e in entries:
        if e.id == eid:
            return e
    return None


def _find_font(face_names: tuple[FaceNameList, ...],
               lang: str, fid: int) -> FontFace | None:
    for fl in face_names:
        if fl.lang == lang:
            for f in fl.fonts:
                if f.id == fid:
                    return f
    return None


def _alloc_next_id(entries) -> int:
    """0-based: return max id + 1, or 0 if empty (HWPX uses 0-based ids)."""
    if not entries:
        return 0
    return max(e.id for e in entries) + 1


# ============================================================
# Per-catalog canon
# ============================================================

CanonKey = tuple


def _canon_bf(e: StyleEntry, doc: HwpxDocument) -> CanonKey:
    return ("BF", _shallow_canon(e.xml))


def _canon_td(e: StyleEntry, doc: HwpxDocument) -> CanonKey:
    return ("TD", _shallow_canon(e.xml))


def _canon_nb(e: StyleEntry, doc: HwpxDocument) -> CanonKey:
    return ("NB", _shallow_canon(e.xml))


def _canon_ff(f: FontFace, lang: str, doc: HwpxDocument) -> CanonKey:
    """FontFace identity: lang + face name + type only.

    typeInfo (familyType/weight/proportion 등) 와 substFont 등 inner metadata 는
    canon에서 제외. 이유: rhwp HWP→HWPX는 typeInfo를 emit하지 않지만, 한컴이
    저장한 HWPX 양식에는 typeInfo가 있음. 같은 face name인데 typeInfo 차이로
    다른 entry로 판정되면 append + 이름 suffix(" (copied from src)") 가 붙어
    HWP가 시스템 폰트 lookup을 실패함. 렌더링 동일성은 face name + type 으로
    결정되므로 그것만 비교해서 dedupe.
    """
    m = re.match(rf"<({TAG_NAME})\b([^>]*?)(/)?>", f.xml)
    if not m:
        return ("FF", lang, f.xml)
    attrs = parse_attrs(m.group(2))
    face = attrs.get("face", "")
    ftype = attrs.get("type", "")
    return ("FF", lang, face, ftype)


def _canon_bin(b: BinItem, doc: HwpxDocument) -> CanonKey:
    """BinItem identity: media_type + sha256 of bytes (data fully fingerprinted)."""
    import hashlib
    h = hashlib.sha256(b.data).hexdigest()
    return ("BIN", b.media_type, h)


def _canon_cs(e: StyleEntry, doc: HwpxDocument) -> CanonKey:
    """charPr canon: substitute inner borderFillIDRef + <hh:fontRef> Lang attrs
    with hashes of the referenced entries."""
    m = re.match(rf"<({TAG_NAME})\b([^>]*?)(/)?>", e.xml)
    if not m:
        return ("CS", _shallow_canon(e.xml))
    attrs = parse_attrs(m.group(2))
    attrs.pop("id", None)
    bf_id_str = attrs.pop("borderFillIDRef", None)
    bf_canon: CanonKey | None = None
    if bf_id_str:
        try:
            bf_id = int(bf_id_str)
            bf_e = _find_entry(doc.styles.border_fills, bf_id)
            bf_canon = _canon_bf(bf_e, doc) if bf_e else ("BF-missing", bf_id)
        except ValueError:
            pass

    # <hh:fontRef hangul=N latin=N ...>
    fontref_m = re.search(r"<hh:fontRef\b([^/>]*)/?>", e.xml)
    fontref_canons: dict[str, CanonKey] = {}
    fontref_raw = ""
    if fontref_m:
        fontref_raw = fontref_m.group(0)
        fr_attrs = parse_attrs(fontref_m.group(1))
        for lang_low, lang_up in zip(LANGS_LOWER, _LANGS_UPPER):
            if lang_low in fr_attrs:
                try:
                    fid = int(fr_attrs[lang_low])
                    ff = _find_font(doc.styles.face_names, lang_up, fid)
                    fontref_canons[lang_low] = (
                        _canon_ff(ff, lang_up, doc) if ff
                        else ("FF-missing", lang_up, fid)
                    )
                except ValueError:
                    pass

    inner = e.xml[m.end():e.xml.rfind(f"</{m.group(1)}>")] if m.group(3) != "/" else ""
    if fontref_raw:
        inner = inner.replace(fontref_raw, "", 1)
    inner_norm = _WS_RE.sub(" ", inner).strip()
    return (
        "CS",
        _normalize_attrs(attrs),
        bf_canon,
        tuple(sorted(fontref_canons.items())),
        inner_norm,
    )


_LANGS_UPPER: tuple[str, ...] = (
    "HANGUL", "LATIN", "HANJA", "JAPANESE", "OTHER", "SYMBOL", "USER",
)


def _canon_ps(e: StyleEntry, doc: HwpxDocument) -> CanonKey:
    """paraPr canon: substitute tabPrIDRef + numberingIDRef + paraPr's
    <hh:border borderFillIDRef> with hashes of the referenced entries."""
    m = re.match(rf"<({TAG_NAME})\b([^>]*?)(/)?>", e.xml)
    if not m:
        return ("PS", _shallow_canon(e.xml))
    attrs = parse_attrs(m.group(2))
    attrs.pop("id", None)
    self_close = m.group(3) == "/"

    td_id_str = attrs.pop("tabPrIDRef", None)
    td_canon: CanonKey | None = None
    if td_id_str:
        try:
            td_id = int(td_id_str)
            td_e = _find_entry(doc.styles.tab_defs, td_id)
            td_canon = _canon_td(td_e, doc) if td_e else ("TD-missing", td_id)
        except ValueError:
            pass

    # <hh:border borderFillIDRef="N">
    bf_canon: CanonKey | None = None
    bf_raw = ""
    bm = re.search(r'<hh:border\b[^>]*\bborderFillIDRef="(\d+)"[^/>]*/?>', e.xml)
    if bm:
        bf_raw = bm.group(0)
        try:
            bf_id = int(bm.group(1))
            bf_e = _find_entry(doc.styles.border_fills, bf_id)
            bf_canon = _canon_bf(bf_e, doc) if bf_e else ("BF-missing", bf_id)
        except ValueError:
            pass

    # <hh:heading numberingIDRef="N">
    nb_canon: CanonKey | None = None
    nb_raw = ""
    nm = re.search(r'<hh:heading\b[^>]*\bnumberingIDRef="(\d+)"[^/>]*/?>', e.xml)
    if nm:
        nb_raw = nm.group(0)
        try:
            nb_id = int(nm.group(1))
            nb_e = _find_entry(doc.styles.numberings, nb_id)
            nb_canon = _canon_nb(nb_e, doc) if nb_e else ("NB-missing", nb_id)
        except ValueError:
            pass

    inner = e.xml[m.end():e.xml.rfind(f"</{m.group(1)}>")] if not self_close else ""
    for raw in (bf_raw, nb_raw):
        if raw:
            inner = inner.replace(raw, "", 1)
    inner_norm = _WS_RE.sub(" ", inner).strip()
    return (
        "PS",
        _normalize_attrs(attrs),
        td_canon,
        bf_canon,
        nb_canon,
        inner_norm,
    )


def _canon_style(e: StyleEntry, doc: HwpxDocument) -> CanonKey:
    """style canon: substitute paraPrIDRef + charPrIDRef with subordinate canon;
    nextStyleIDRef via name resolution to break cycles."""
    m = re.match(rf"<({TAG_NAME})\b([^>]*?)(/)?>", e.xml)
    if not m:
        return ("STYLE", _shallow_canon(e.xml))
    attrs = parse_attrs(m.group(2))
    attrs.pop("id", None)
    ps_id_str = attrs.pop("paraPrIDRef", None)
    cs_id_str = attrs.pop("charPrIDRef", None)
    next_id_str = attrs.pop("nextStyleIDRef", None)
    self_close = m.group(3) == "/"

    ps_canon: CanonKey | None = None
    if ps_id_str:
        try:
            ps_e = _find_entry(doc.styles.para_shapes, int(ps_id_str))
            ps_canon = _canon_ps(ps_e, doc) if ps_e else ("PS-missing", ps_id_str)
        except ValueError:
            pass
    cs_canon: CanonKey | None = None
    if cs_id_str:
        try:
            cs_e = _find_entry(doc.styles.char_shapes, int(cs_id_str))
            cs_canon = _canon_cs(cs_e, doc) if cs_e else ("CS-missing", cs_id_str)
        except ValueError:
            pass
    next_name: str | None = None
    if next_id_str:
        try:
            next_e = _find_entry(doc.styles.styles, int(next_id_str))
            if next_e:
                nm = re.search(r'\bname="([^"]*)"', next_e.xml)
                next_name = nm.group(1) if nm else None
        except ValueError:
            pass
    inner = e.xml[m.end():e.xml.rfind(f"</{m.group(1)}>")] if not self_close else ""
    inner_norm = _WS_RE.sub(" ", inner).strip()
    return (
        "STYLE",
        _normalize_attrs(attrs),
        ps_canon,
        cs_canon,
        next_name,
        inner_norm,
    )


# ============================================================
# Ref rewriter
# ============================================================

def _rewrite_attr(xml: str, attr_name: str, id_map: dict[int, int]) -> str:
    """Replace `\\bATTR="N"` values per id_map. Only numeric values."""
    if not id_map:
        return xml

    def repl(m):
        try:
            old = int(m.group(1))
        except ValueError:
            return m.group(0)
        new = id_map.get(old, old)
        return f'{attr_name}="{new}"'

    return re.sub(rf'\b{attr_name}="(\d+)"', repl, xml)


def _rewrite_fontref_in_xml(xml: str,
                             ff_maps: dict[str, dict[int, int]]) -> str:
    """Replace <hh:fontRef hangul=N latin=N ...> Lang attrs per per-Lang maps.

    ff_maps is keyed by uppercase Lang (HANGUL/LATIN/...). The XML attribute
    names are lowercase (hangul/latin/...).
    """
    if not any(ff_maps.values()):
        return xml

    def repl_fontref(m):
        inner = m.group(1)
        new_inner = inner
        for lang_low, lang_up in zip(LANGS_LOWER, _LANGS_UPPER):
            mp = ff_maps.get(lang_up)
            if not mp:
                continue
            def lang_repl(mm, _mp=mp, _l=lang_low):
                try:
                    old = int(mm.group(1))
                except ValueError:
                    return mm.group(0)
                new = _mp.get(old, old)
                return f'{_l}="{new}"'
            new_inner = re.sub(rf'\b{lang_low}="(\d+)"', lang_repl, new_inner)
        return f'<hh:fontRef{new_inner}/>'

    return re.sub(r'<hh:fontRef\b([^/>]*)/>', repl_fontref, xml)


def rewrite_refs_in_xml(xml: str, id_maps: IdMaps) -> str:
    """Apply all ref remaps to an XML fragment.

    Catalog of remapped attribute names (HWPX, see davinci probe cx_1
    for HWPML equivalents we consolidated):
      paraPrIDRef, charPrIDRef, styleIDRef, charStyleIDRef, nextStyleIDRef,
      borderFillIDRef, tabPrIDRef,
      numberingIDRef (paraPr.heading), outlineShapeIDRef (secPr),
      linkListIDRef, linkListNextIDRef,
      binaryItemIDRef (string ids), <hh:fontRef> Lang attrs.
    """
    xml = _rewrite_attr(xml, "paraPrIDRef", id_maps.para_shapes)
    xml = _rewrite_attr(xml, "charPrIDRef", id_maps.char_shapes)
    xml = _rewrite_attr(xml, "styleIDRef", id_maps.styles)
    xml = _rewrite_attr(xml, "charStyleIDRef", id_maps.styles)
    xml = _rewrite_attr(xml, "nextStyleIDRef", id_maps.styles)
    xml = _rewrite_attr(xml, "borderFillIDRef", id_maps.border_fills)
    xml = _rewrite_attr(xml, "tabPrIDRef", id_maps.tab_defs)
    xml = _rewrite_attr(xml, "numberingIDRef", id_maps.numberings)
    xml = _rewrite_attr(xml, "outlineShapeIDRef", id_maps.numberings)
    xml = _rewrite_attr(xml, "linkListIDRef", id_maps.numberings)
    xml = _rewrite_attr(xml, "linkListNextIDRef", id_maps.numberings)
    if id_maps.bin_items:
        def bi_repl(m):
            old = m.group(1)
            new = id_maps.bin_items.get(old, old)
            return f'binaryItemIDRef="{new}"'
        xml = re.sub(r'\bbinaryItemIDRef="([^"]+)"', bi_repl, xml)
    xml = _rewrite_fontref_in_xml(xml, id_maps.face_names)
    return xml


# ============================================================
# Entry self-id rewriter
# ============================================================

def _rewrite_self_id(xml: str, new_id: int) -> str:
    """Replace the FIRST `\\bid="N"` in the entry XML with new_id."""
    return re.sub(r'\bid="\d+"', f'id="{new_id}"', xml, count=1)


def _rename_in_xml(xml: str, orig: str, new: str) -> str:
    """Replace name="orig" with name="new" — first occurrence only."""
    def attr_escape(s):
        return s.replace("&", "&amp;").replace('"', "&quot;").replace("<", "&lt;")
    return xml.replace(f'name="{orig}"', f'name="{attr_escape(new)}"', 1)


# ============================================================
# merge_styles — main entry
# ============================================================

def merge_styles(
    dst_doc: HwpxDocument,
    src_doc: HwpxDocument,
    src_doc_id: str | None = None,
) -> tuple[HwpxDocument, IdMaps]:
    """Merge src_doc's styles + bin_items into dst_doc.

    Returns (new_dst_doc, id_maps).

    For each src entry:
      - if dst has a logically-equivalent entry → reuse its id (id_map[src]=dst)
      - else → append a copy with rewritten internal refs + a fresh id
        (HWPX uses 0-based ids; we use max+1)

    On Name collision (Style/FontFace), append " (copied from {src_doc_id})"
    to disambiguate.
    """
    if src_doc_id is None:
        src_doc_id = src_doc.doc_id or "src"
    suffix = f" (copied from {src_doc_id})"

    id_maps = IdMaps()

    # ── Leaf 1: BinItem (unique by content sha256) ──
    dst_bin_items: list[BinItem] = list(dst_doc.bin_items)
    dst_bin_canons = {b.manifest_id: _canon_bin(b, dst_doc) for b in dst_bin_items}

    def _next_bin_id() -> tuple[str, str]:
        """Allocate a fresh manifest_id ("imageN"). Returns (manifest_id, href base)."""
        existing_nums = set()
        for b in dst_bin_items:
            m = re.match(r"image(\d+)", b.manifest_id)
            if m:
                existing_nums.add(int(m.group(1)))
        n = max(existing_nums, default=0) + 1
        return f"image{n}", n

    for b in src_doc.bin_items:
        sc = _canon_bin(b, src_doc)
        match_id = next((mid for mid, c in dst_bin_canons.items() if c == sc), None)
        if match_id is not None:
            id_maps.bin_items[b.manifest_id] = match_id
            continue
        # Allocate new id; preserve extension via href
        new_id, new_n = _next_bin_id()
        ext = b.href.rsplit(".", 1)[-1] if "." in b.href else "bin"
        new_href = f"BinData/{new_id}.{ext}"
        nb = BinItem(
            manifest_id=new_id,
            href=new_href,
            media_type=b.media_type,
            is_embedded=b.is_embedded,
            data=b.data,
        )
        dst_bin_items.append(nb)
        dst_bin_canons[new_id] = _canon_bin(nb, dst_doc)
        id_maps.bin_items[b.manifest_id] = new_id

    # ── Leaf 2: borderFill ──
    dst_bf = list(dst_doc.styles.border_fills)
    dst_bf_canons = {e.id: _canon_bf(e, dst_doc) for e in dst_bf}
    for e in src_doc.styles.border_fills:
        sc = _canon_bf(e, src_doc)
        match = next((d for d, c in dst_bf_canons.items() if c == sc), None)
        if match is not None:
            id_maps.border_fills[e.id] = match
            continue
        new_id = _alloc_next_id(dst_bf)
        ne = StyleEntry(id=new_id, xml=_rewrite_self_id(e.xml, new_id))
        dst_bf.append(ne)
        dst_bf_canons[new_id] = _canon_bf(ne, dst_doc)
        id_maps.border_fills[e.id] = new_id

    # ── Leaf 3: tabPr ──
    dst_td = list(dst_doc.styles.tab_defs)
    dst_td_canons = {e.id: _canon_td(e, dst_doc) for e in dst_td}
    for e in src_doc.styles.tab_defs:
        sc = _canon_td(e, src_doc)
        match = next((d for d, c in dst_td_canons.items() if c == sc), None)
        if match is not None:
            id_maps.tab_defs[e.id] = match
            continue
        new_id = _alloc_next_id(dst_td)
        ne = StyleEntry(id=new_id, xml=_rewrite_self_id(e.xml, new_id))
        dst_td.append(ne)
        dst_td_canons[new_id] = _canon_td(ne, dst_doc)
        id_maps.tab_defs[e.id] = new_id

    # ── Leaf 4: numbering ──
    dst_nb = list(dst_doc.styles.numberings)
    dst_nb_canons = {e.id: _canon_nb(e, dst_doc) for e in dst_nb}
    for e in src_doc.styles.numberings:
        sc = _canon_nb(e, src_doc)
        match = next((d for d, c in dst_nb_canons.items() if c == sc), None)
        if match is not None:
            id_maps.numberings[e.id] = match
            continue
        new_id = _alloc_next_id(dst_nb)
        ne = StyleEntry(id=new_id, xml=_rewrite_self_id(e.xml, new_id))
        dst_nb.append(ne)
        dst_nb_canons[new_id] = _canon_nb(ne, dst_doc)
        id_maps.numberings[e.id] = new_id

    # ── Leaf 5: bullet ──
    dst_bullets = list(dst_doc.styles.bullets)
    dst_bullet_canons = {e.id: _shallow_canon(e.xml) for e in dst_bullets}
    for e in src_doc.styles.bullets:
        sc = _shallow_canon(e.xml)
        match = next((d for d, c in dst_bullet_canons.items() if c == sc), None)
        if match is not None:
            id_maps.bullets[e.id] = match
            continue
        new_id = _alloc_next_id(dst_bullets)
        ne = StyleEntry(id=new_id, xml=_rewrite_self_id(e.xml, new_id))
        dst_bullets.append(ne)
        dst_bullet_canons[new_id] = _shallow_canon(ne.xml)
        id_maps.bullets[e.id] = new_id

    # ── Leaf 6: fontfaces (per-Lang) ──
    dst_face_names: list[FaceNameList] = list(dst_doc.styles.face_names)
    for lang in _LANGS_UPPER:
        id_maps.face_names[lang] = {}

    def _find_lang_block(lang: str) -> tuple[int | None, FaceNameList | None]:
        for i, fl in enumerate(dst_face_names):
            if fl.lang == lang:
                return i, fl
        return None, None

    for src_fl in src_doc.styles.face_names:
        lang = src_fl.lang
        idx, dst_fl = _find_lang_block(lang)
        if dst_fl is None:
            dst_fl = FaceNameList(lang=lang, fonts=(), raw_attrs=dict(src_fl.raw_attrs))
            dst_face_names.append(dst_fl)
            idx = len(dst_face_names) - 1

        dst_fonts = list(dst_fl.fonts)
        dst_font_canons = {f.id: _canon_ff(f, lang, dst_doc) for f in dst_fonts}
        dst_name_to_id: dict[str, int] = {}
        for f in dst_fonts:
            nm_m = re.search(r'\bface="([^"]*)"', f.xml)
            if nm_m:
                dst_name_to_id[nm_m.group(1)] = f.id

        for f in src_fl.fonts:
            sc = _canon_ff(f, lang, src_doc)
            match = next((d for d, c in dst_font_canons.items() if c == sc), None)
            if match is not None:
                id_maps.face_names.setdefault(lang, {})[f.id] = match
                continue
            nm_m = re.search(r'\bface="([^"]*)"', f.xml)
            orig_name = nm_m.group(1) if nm_m else ""
            new_id = _alloc_next_id(dst_fonts)
            new_xml = _rewrite_self_id(f.xml, new_id)
            if orig_name and orig_name in dst_name_to_id:
                new_name = orig_name + suffix
                k = 2
                while new_name in dst_name_to_id:
                    new_name = f"{orig_name}{suffix} ({k})"
                    k += 1
                new_xml = new_xml.replace(
                    f'face="{orig_name}"', f'face="{new_name}"', 1,
                )
            nf = FontFace(id=new_id, xml=new_xml)
            dst_fonts.append(nf)
            dst_font_canons[new_id] = _canon_ff(nf, lang, dst_doc)
            final_nm = re.search(r'\bface="([^"]*)"', new_xml)
            if final_nm:
                dst_name_to_id[final_nm.group(1)] = new_id
            id_maps.face_names.setdefault(lang, {})[f.id] = new_id

        dst_face_names[idx] = FaceNameList(
            lang=lang, fonts=tuple(dst_fonts), raw_attrs=dst_fl.raw_attrs,
        )

    # Intermediate dst (for canon recomputation in CS/PS/STYLE phases)
    intermediate_styles = StyleTable(
        face_names=tuple(dst_face_names),
        border_fills=tuple(dst_bf),
        char_shapes=dst_doc.styles.char_shapes,
        tab_defs=tuple(dst_td),
        numberings=tuple(dst_nb),
        bullets=tuple(dst_bullets),
        para_shapes=dst_doc.styles.para_shapes,
        styles=dst_doc.styles.styles,
    )
    intermediate_dst = HwpxDocument(
        sections=dst_doc.sections,
        styles=intermediate_styles,
        bin_items=tuple(dst_bin_items),
        raw_xml_files=dst_doc.raw_xml_files,
        content_hpf_template=dst_doc.content_hpf_template,
        header_xml_decl=dst_doc.header_xml_decl,
        header_root_attrs=dst_doc.header_root_attrs,
        header_pre_lists_xml=dst_doc.header_pre_lists_xml,
        header_post_lists_xml=dst_doc.header_post_lists_xml,
        doc_id=dst_doc.doc_id,
    )

    # ── charPr ──
    dst_cs = list(dst_doc.styles.char_shapes)
    dst_cs_canons = {e.id: _canon_cs(e, intermediate_dst) for e in dst_cs}
    for e in src_doc.styles.char_shapes:
        sc = _canon_cs(e, src_doc)
        match = next((d for d, c in dst_cs_canons.items() if c == sc), None)
        if match is not None:
            id_maps.char_shapes[e.id] = match
            continue
        new_id = _alloc_next_id(dst_cs)
        rewritten = _rewrite_attr(e.xml, "borderFillIDRef", id_maps.border_fills)
        rewritten = _rewrite_fontref_in_xml(rewritten, id_maps.face_names)
        rewritten = _rewrite_self_id(rewritten, new_id)
        ne = StyleEntry(id=new_id, xml=rewritten)
        dst_cs.append(ne)
        dst_cs_canons[new_id] = _canon_cs(ne, intermediate_dst)
        id_maps.char_shapes[e.id] = new_id

    # ── paraPr ──
    dst_ps = list(dst_doc.styles.para_shapes)
    dst_ps_canons = {e.id: _canon_ps(e, intermediate_dst) for e in dst_ps}
    for e in src_doc.styles.para_shapes:
        sc = _canon_ps(e, src_doc)
        match = next((d for d, c in dst_ps_canons.items() if c == sc), None)
        if match is not None:
            id_maps.para_shapes[e.id] = match
            continue
        new_id = _alloc_next_id(dst_ps)
        rewritten = _rewrite_attr(e.xml, "tabPrIDRef", id_maps.tab_defs)
        rewritten = _rewrite_attr(rewritten, "borderFillIDRef", id_maps.border_fills)
        rewritten = _rewrite_attr(rewritten, "numberingIDRef", id_maps.numberings)
        rewritten = _rewrite_self_id(rewritten, new_id)
        ne = StyleEntry(id=new_id, xml=rewritten)
        dst_ps.append(ne)
        dst_ps_canons[new_id] = _canon_ps(ne, intermediate_dst)
        id_maps.para_shapes[e.id] = new_id

    # Re-compute intermediate to reflect updated CS/PS for STYLE canon
    intermediate_styles = StyleTable(
        face_names=intermediate_styles.face_names,
        border_fills=intermediate_styles.border_fills,
        char_shapes=tuple(dst_cs),
        tab_defs=intermediate_styles.tab_defs,
        numberings=intermediate_styles.numberings,
        bullets=intermediate_styles.bullets,
        para_shapes=tuple(dst_ps),
        styles=dst_doc.styles.styles,
    )
    intermediate_dst = HwpxDocument(
        sections=intermediate_dst.sections,
        styles=intermediate_styles,
        bin_items=intermediate_dst.bin_items,
        raw_xml_files=intermediate_dst.raw_xml_files,
        content_hpf_template=intermediate_dst.content_hpf_template,
        header_xml_decl=intermediate_dst.header_xml_decl,
        header_root_attrs=intermediate_dst.header_root_attrs,
        header_pre_lists_xml=intermediate_dst.header_pre_lists_xml,
        header_post_lists_xml=intermediate_dst.header_post_lists_xml,
        doc_id=intermediate_dst.doc_id,
    )

    # ── style ──
    dst_st = list(dst_doc.styles.styles)
    dst_st_canons = {e.id: _canon_style(e, intermediate_dst) for e in dst_st}
    dst_st_name_to_id: dict[str, int] = {}
    for e in dst_st:
        nm = re.search(r'\bname="([^"]*)"', e.xml)
        if nm:
            dst_st_name_to_id[nm.group(1)] = e.id

    for e in src_doc.styles.styles:
        sc = _canon_style(e, src_doc)
        match = next((d for d, c in dst_st_canons.items() if c == sc), None)
        if match is not None:
            id_maps.styles[e.id] = match
            continue
        new_id = _alloc_next_id(dst_st)
        nm_m = re.search(r'\bname="([^"]*)"', e.xml)
        orig_name = nm_m.group(1) if nm_m else ""
        rewritten = _rewrite_attr(e.xml, "paraPrIDRef", id_maps.para_shapes)
        rewritten = _rewrite_attr(rewritten, "charPrIDRef", id_maps.char_shapes)
        rewritten = _rewrite_attr(rewritten, "nextStyleIDRef", id_maps.styles)
        rewritten = _rewrite_self_id(rewritten, new_id)
        if orig_name and orig_name in dst_st_name_to_id:
            new_name = orig_name + suffix
            k = 2
            while new_name in dst_st_name_to_id:
                new_name = f"{orig_name}{suffix} ({k})"
                k += 1
            rewritten = _rename_in_xml(rewritten, orig_name, new_name)
        ne = StyleEntry(id=new_id, xml=rewritten)
        dst_st.append(ne)
        dst_st_canons[new_id] = _canon_style(ne, intermediate_dst)
        final_nm = re.search(r'\bname="([^"]*)"', rewritten)
        if final_nm:
            dst_st_name_to_id[final_nm.group(1)] = new_id
        id_maps.styles[e.id] = new_id

    # Final styles
    final_styles = StyleTable(
        face_names=tuple(dst_face_names),
        border_fills=tuple(dst_bf),
        char_shapes=tuple(dst_cs),
        tab_defs=tuple(dst_td),
        numberings=tuple(dst_nb),
        bullets=tuple(dst_bullets),
        para_shapes=tuple(dst_ps),
        styles=tuple(dst_st),
    )
    final_doc = HwpxDocument(
        sections=dst_doc.sections,
        styles=final_styles,
        bin_items=tuple(dst_bin_items),
        raw_xml_files=dst_doc.raw_xml_files,
        content_hpf_template=dst_doc.content_hpf_template,
        header_xml_decl=dst_doc.header_xml_decl,
        header_root_attrs=dst_doc.header_root_attrs,
        header_pre_lists_xml=dst_doc.header_pre_lists_xml,
        header_post_lists_xml=dst_doc.header_post_lists_xml,
        doc_id=dst_doc.doc_id,
    )
    return final_doc, id_maps


# ============================================================
# Per-item rewrite (used by rewrite_paragraph)
# ============================================================

def _rewrite_table_item(t: TableItem, id_maps: IdMaps) -> TableItem:
    new_attrs = dict(t.table_attrs)
    if "borderFillIDRef" in new_attrs:
        try:
            old = int(new_attrs["borderFillIDRef"])
            new_attrs["borderFillIDRef"] = str(id_maps.border_fills.get(old, old))
        except ValueError:
            pass
    new_pre_rows = rewrite_refs_in_xml(t.pre_rows_xml, id_maps)
    new_cells = tuple(_rewrite_cell_item(c, id_maps) for c in t.cells)
    return TableItem(
        table_attrs=new_attrs,
        pre_rows_xml=new_pre_rows,
        cells=new_cells,
        char_shape_id=id_maps.char_shapes.get(t.char_shape_id, t.char_shape_id),
    )


def _rewrite_cell_item(c: CellItem, id_maps: IdMaps) -> CellItem:
    new_attrs = dict(c.cell_attrs)
    if "borderFillIDRef" in new_attrs:
        try:
            old = int(new_attrs["borderFillIDRef"])
            new_attrs["borderFillIDRef"] = str(id_maps.border_fills.get(old, old))
        except ValueError:
            pass
    new_sublist = dict(c.sublist_attrs)
    for attr in ("linkListIDRef", "linkListNextIDRef"):
        if attr in new_sublist:
            try:
                old = int(new_sublist[attr])
                new_sublist[attr] = str(id_maps.numberings.get(old, old))
            except ValueError:
                pass
    new_meta = rewrite_refs_in_xml(c.cell_meta_xml, id_maps)
    return CellItem(
        cell_attrs=new_attrs,
        sublist_attrs=new_sublist,
        paragraphs=tuple(rewrite_paragraph(p, id_maps) for p in c.paragraphs),
        cell_meta_xml=new_meta,
    )


def rewrite_paragraph(p: Paragraph, id_maps: IdMaps) -> Paragraph:
    """Recursively rewrite all catalog refs inside p.

    Every item's `starts_new_run` marker is preserved (writer needs it to
    keep run boundaries that same-cs grouping would otherwise collapse).
    """
    from dataclasses import replace as _dc_replace
    new_items: list[Item] = []
    for item in p.items:
        snr = getattr(item, "starts_new_run", False)
        if isinstance(item, CharItem):
            new_cs = id_maps.char_shapes.get(item.char_shape_id, item.char_shape_id)
            new_style = item.char_style
            if item.char_style is not None:
                try:
                    sid = int(item.char_style)
                    new_style = str(id_maps.styles.get(sid, sid))
                except ValueError:
                    pass
            new_items.append(CharItem(
                text=item.text, char_shape_id=new_cs, char_style=new_style,
                starts_new_run=snr,
            ))
        elif isinstance(item, TabItem):
            new_items.append(TabItem(
                char_shape_id=id_maps.char_shapes.get(item.char_shape_id, item.char_shape_id),
                tab_attrs=dict(item.tab_attrs),
                starts_new_run=snr,
            ))
        elif isinstance(item, LineBreakItem):
            new_items.append(LineBreakItem(
                char_shape_id=id_maps.char_shapes.get(item.char_shape_id, item.char_shape_id),
                starts_new_run=snr,
            ))
        elif isinstance(item, EmptyRunItem):
            new_items.append(EmptyRunItem(
                char_shape_id=id_maps.char_shapes.get(item.char_shape_id, item.char_shape_id),
                starts_new_run=snr,
            ))
        elif isinstance(item, OpaqueInlineItem):
            new_items.append(OpaqueInlineItem(
                tag=item.tag,
                xml=rewrite_refs_in_xml(item.xml, id_maps),
                char_shape_id=id_maps.char_shapes.get(item.char_shape_id, item.char_shape_id),
                starts_new_run=snr,
            ))
        elif isinstance(item, SectionMeta):
            new_items.append(SectionMeta(
                raw_xml=rewrite_refs_in_xml(item.raw_xml, id_maps),
                char_shape_id=id_maps.char_shapes.get(item.char_shape_id, item.char_shape_id),
                starts_new_run=snr,
            ))
        elif isinstance(item, ColumnDef):
            new_items.append(ColumnDef(
                raw_xml=rewrite_refs_in_xml(item.raw_xml, id_maps),
                char_shape_id=id_maps.char_shapes.get(item.char_shape_id, item.char_shape_id),
                starts_new_run=snr,
            ))
        elif isinstance(item, LayoutDef):
            new_items.append(LayoutDef(
                raw_xml=rewrite_refs_in_xml(item.raw_xml, id_maps),
                inner_tag=item.inner_tag,
                char_shape_id=id_maps.char_shapes.get(item.char_shape_id, item.char_shape_id),
                starts_new_run=snr,
            ))
        elif isinstance(item, ScopeDef):
            new_items.append(ScopeDef(
                raw_xml=rewrite_refs_in_xml(item.raw_xml, id_maps),
                inner_tag=item.inner_tag,
                char_shape_id=id_maps.char_shapes.get(item.char_shape_id, item.char_shape_id),
                starts_new_run=snr,
            ))
        elif isinstance(item, NoteDef):
            new_items.append(NoteDef(
                raw_xml=rewrite_refs_in_xml(item.raw_xml, id_maps),
                inner_tag=item.inner_tag,
                char_shape_id=id_maps.char_shapes.get(item.char_shape_id, item.char_shape_id),
                starts_new_run=snr,
            ))
        elif isinstance(item, TableItem):
            new_items.append(_rewrite_table_item(item, id_maps))
        else:
            new_items.append(item)

    return Paragraph(
        items=tuple(new_items),
        para_shape_id=id_maps.para_shapes.get(p.para_shape_id, p.para_shape_id),
        style_id=id_maps.styles.get(p.style_id, p.style_id),
        char_shape_id_first=id_maps.char_shapes.get(p.char_shape_id_first, p.char_shape_id_first),
        starts_new_page=p.starts_new_page,
        starts_new_column=p.starts_new_column,
        merged=p.merged,
        para_id_attr=p.para_id_attr,
        raw_attrs=dict(p.raw_attrs),
        linesegs_xml=p.linesegs_xml,
    )


# ============================================================
# Public copy entry
# ============================================================

def _strip_attached(p: Paragraph) -> Paragraph:
    """Remove ALL section-level structural items from paragraph.

    Mirrors davinci AttachedDef: SectionMeta + ColumnDef + LayoutDef +
    ScopeDef + NoteDef. Used when copying src paragraphs into dst — dst's
    structure should win, not src's.

    After stripping, the next remaining item gets its starts_new_run flag
    cleared (the run boundary it marked was relative to the now-removed
    structural items).
    """
    from dataclasses import replace as _dc_replace
    new_items = []
    stripped_any = False
    for it in p.items:
        if isinstance(it, (SectionMeta, ColumnDef, LayoutDef, ScopeDef, NoteDef)):
            stripped_any = True
            continue
        new_items.append(it)
    if stripped_any and new_items and getattr(new_items[0], "starts_new_run", False):
        new_items[0] = _dc_replace(new_items[0], starts_new_run=False)
    return p.with_items(tuple(new_items))


def copy_paragraphs(
    src_doc: HwpxDocument,
    dst_doc: HwpxDocument,
    src_section_idx: int,
    src_para_indices: tuple[int, ...] | list[int],
    *,
    include_attached: bool = False,
) -> tuple[HwpxDocument, tuple[Paragraph, ...]]:
    """Take src paragraphs (by section + 0-based indices), merge styles into
    dst, and return (new_dst_doc, remapped_paragraphs).

    Caller is responsible for splicing the returned paragraphs into the new
    dst doc's section body (typically via insert_paragraphs / replace).

    `include_attached`: if False (default), strip src paragraphs' SectionMeta
    and ColumnDef items — preserve dst's section meta intact.
    """
    if src_section_idx < 0 or src_section_idx >= len(src_doc.sections):
        raise IndexError(f"src_section_idx={src_section_idx} 범위 초과")

    src_paras = src_doc.sections[src_section_idx].body
    selected: list[Paragraph] = []
    for i in src_para_indices:
        if i < 0 or i >= len(src_paras):
            raise IndexError(f"src para index {i} out of range (have {len(src_paras)})")
        p = src_paras[i]
        if not include_attached:
            p = _strip_attached(p)
        selected.append(p)

    merged_doc, id_maps = merge_styles(dst_doc, src_doc, src_doc_id=src_doc.doc_id)
    rewritten = tuple(rewrite_paragraph(p, id_maps) for p in selected)
    return merged_doc, rewritten

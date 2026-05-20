"""HWPX domain model — adapted from davinci HWPML codec for HWPX format.

Key structural differences from HWPML:
  - Container: ZIP (multiple XML files + binaries) vs single XML.
  - Catalogs: Contents/header.xml's <hh:head> children
    (fontfaces/borderFills/charProperties/tabProperties/numberings/bullets/
     paraProperties/styles).
  - Body: Contents/section[N].xml with root <hs:sec>.
  - Binaries: BinData/imageN.ext zip entries + manifest <opf:item> in
    Contents/content.hpf (one-level indirection vs HWPML's two-level
    BINITEM→BINDATA base64 chain).
  - Master pages: Contents/masterpage[N].xml separate files, referenced via
    <hp:masterPage idRef="masterpageN"/> inside <hp:secPr>.
  - Section meta (SECDEF analog): <hp:secPr> inside the FIRST <hp:run> of
    the FIRST <hp:p> of the section.

Tag mapping (HWPML → HWPX), used throughout codec:
  P / TEXT / CHAR / TAB / LINEBREAK / FWSPACE / NBSPACE / HYPHEN
    → hp:p / hp:run / hp:t / hp:tab / hp:lineBreak / hp:fwSpace / hp:nbSpace / hp:hyphen
  TABLE / ROW / CELL / CELLMARGIN / PARALIST
    → hp:tbl / hp:tr / hp:tc / hp:cellMargin / hp:subList
  PICTURE / IMAGE / EQUATION / SHAPECOMMENT
    → hp:pic / hc:img / hp:equation / hp:shapeComment
  SECDEF / COLDEF / NEWNUM / FOOTNOTE / ENDNOTE
    → hp:secPr / hp:colPr / hp:newNum / hp:footNote / hp:endNote
  MASTERPAGE
    → masterpage*.xml file + <hp:masterPage idRef="..."/> ref

Ref attribute mapping (HWPML → HWPX):
  ParaShape         → paraPrIDRef
  CharShape (TEXT)  → charPrIDRef
  CharShape (PARAHEAD/STYLE) → charPrIDRef
  Style (P)         → styleIDRef
  Style (CHAR)      → charStyleIDRef    (note: <hp:t charStyleIDRef="N">)
  BorderFill        → borderFillIDRef
  BorderFillId      → borderFillIDRef   (HWPX collapsed both)
  BorferFill (typo) → borderFillIDRef   (HWPX cleaned up)
  TabDef            → tabPrIDRef
  LinkListID        → linkListIDRef
  LinkListIDNext    → linkListNextIDRef
  BinItem           → binaryItemIDRef   (string "imageN", not int)
  NextStyle         → nextStyleIDRef
  FONTID Lang attrs → hh:fontRef hangul=, latin=, hanja=, japanese=,
                                other=, symbol=, user=  (lowercase Lang)

Floating-detection axes (per davinci probe cx_10): identical to HWPX.
  HWPML TreatAsChar → HWPX hp:pos@treatAsChar
  HWPML FlowWithText → HWPX hp:pos@flowWithText
  HWPML HorzRelTo / VertRelTo → HWPX hp:pos@horzRelTo / vertRelTo
  HWPML SHAPECOMMENT → HWPX <hp:shapeComment>

A floating shape (page-anchored decoration) satisfies one of:
  - has <hp:shapeComment> → user-explicit content → keep
  - all of: treatAsChar="1" + flowWithText="1" + horzRelTo/vertRelTo not in
    {Paper, Page} → keep
  - otherwise → drop (page-anchored layout decoration)
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Optional, Union


# ============================================================
# Languages — used in fontface dedupe (per-Lang Id space)
# ============================================================

# HWPX uses lowercase lang codes in <hh:fontface lang="..."> and in
# <hh:fontRef hangul="..."> attribute names.
LANGS_LOWER: tuple[str, ...] = (
    "hangul", "latin", "hanja", "japanese", "other", "symbol", "user",
)
# But the <hh:fontface lang="..."> attribute is uppercase per HWPX convention.
LANGS_UPPER: tuple[str, ...] = (
    "HANGUL", "LATIN", "HANJA", "JAPANESE", "OTHER", "SYMBOL", "USER",
)


# ============================================================
# Item — paragraph 내부 원자
# ============================================================

@dataclass(frozen=True)
class CharItem:
    """A run of plain text. char_shape_id is the run's charPrIDRef.

    char_style holds the optional `charStyleIDRef` of the underlying <hp:t>
    (HWPX equivalent of HWPML <CHAR Style="N">). Most <hp:t> have no
    charStyleIDRef and this is None.

    starts_new_run: True if this item is the first inside a fresh <hp:run>
    open tag (writer must break a new <hp:run> here even if char_shape_id
    matches the previous item — preserves source's run boundaries that
    same-cs grouping would otherwise collapse).
    """
    text: str
    char_shape_id: int
    char_style: Optional[str] = None
    starts_new_run: bool = False


@dataclass(frozen=True)
class TabItem:
    """<hp:tab/> inside a run."""
    char_shape_id: int
    tab_attrs: dict = field(default_factory=dict)
    starts_new_run: bool = False


@dataclass(frozen=True)
class LineBreakItem:
    """<hp:lineBreak/>."""
    char_shape_id: int
    starts_new_run: bool = False


@dataclass(frozen=True)
class EmptyRunItem:
    """`<hp:run charPrIDRef="N"/>` — content-less run, charPr marker only."""
    char_shape_id: int
    starts_new_run: bool = False


@dataclass(frozen=True)
class OpaqueInlineItem:
    """Any inline run-level child whose internals we don't model.

    Targets: hp:pic, hp:equation, hp:rect, hp:ellipse, hp:line, hp:gso,
    hp:textart, hp:container, hp:fieldBegin, hp:fieldEnd, hp:bookmark,
    hp:hiddenComment, hp:fwSpace, hp:nbSpace, hp:hyphen, hp:markpenBegin/End,
    and bare inline ctrls not classified into LayoutDef/NoteDef/ScopeDef.
    """
    tag: str
    xml: str
    char_shape_id: int
    starts_new_run: bool = False


# ============================================================
# Table — first-class because we need to remap cell.borderFillIDRef
# and walk into nested cell paragraphs for ID rewriting.
# ============================================================

@dataclass(frozen=True)
class CellItem:
    """<hp:tc ...><hp:subList ...>P*</hp:subList>...children</hp:tc>.

    HWPX cell layout (probed):
      <hp:tc name="" header="0" hasMargin="0" protect="0" editable="0"
              dirty="0" borderFillIDRef="N">
        <hp:subList id="" textDirection="..." linkListIDRef="0"
                    linkListNextIDRef="0" textWidth="N" textHeight="N"
                    hasTextRef="0" hasNumRef="0">
          <hp:p .../>+
        </hp:subList>
        <hp:cellAddr colAddr="C" rowAddr="R"/>
        <hp:cellSpan colSpan="N" rowSpan="N"/>
        <hp:cellSz width="N" height="N"/>
        <hp:cellMargin left="N" right="N" top="N" bottom="N"/>
      </hp:tc>

    Children after subList (cellAddr/cellSpan/cellSz/cellMargin) are kept
    as raw XML (cell_meta_xml) — they have no ID refs to remap.
    """
    cell_attrs: dict
    sublist_attrs: dict
    paragraphs: tuple["Paragraph", ...]
    cell_meta_xml: str  # raw <hp:cellAddr/Span/Sz/Margin/...> sequence


@dataclass(frozen=True)
class TableItem:
    """<hp:tbl ...>(<hp:sz/><hp:pos/><hp:outMargin/><hp:inMargin/>)?
                   (<hp:tr><hp:tc>+</hp:tr>)+</hp:tbl>.

    table_attrs: outer <hp:tbl> attrs (id/rowCnt/colCnt/borderFillIDRef/...)
    pre_rows_xml: raw XML between <hp:tbl> open and the first <hp:tr>
                  (sz/pos/outMargin/inMargin/shapeComment/etc — no ID refs
                  except borderFillIDRef which is on <hp:tbl> itself).
    cells: flat ordered list (by row then col), recursively walked.
    char_shape_id: charPrIDRef of the wrapping <hp:run>.
    """
    table_attrs: dict
    pre_rows_xml: str
    cells: tuple[CellItem, ...]
    char_shape_id: int = 0
    starts_new_run: bool = False


# ============================================================
# Attached structural items — body[0] section-level scaffolding.
# Mirror davinci codec's AttachedDef classification (SectionDef/LayoutDef/
# NoteDef/ScopeDef) ported to HWPX tag namespace.
#
# Probed inventory across all 4 templates + sample HWP→HWPX outputs:
#   inside <hp:ctrl> we see only: hp:colPr, hp:header, hp:footer,
#                                 hp:autoNum, hp:newNum
#   bare in run (not via ctrl):  hp:secPr (= SectionMeta)
# ============================================================

@dataclass(frozen=True)
class SectionMeta:
    """<hp:secPr>...</hp:secPr> — section properties (= davinci SectionDef).

    pagePr/footNotePr/endNotePr/pageBorderFill (multiple)/masterPage refs/
    grid/startNum/visibility/lineNumberShape/etc. Stored as raw XML.
    Catalog refs inside (borderFillIDRef, outlineShapeIDRef, memoShapeIDRef,
    masterPage idRef) are remapped by rewrite_refs_in_xml().
    """
    raw_xml: str
    char_shape_id: int = 0
    starts_new_run: bool = False


@dataclass(frozen=True)
class ColumnDef:
    """<hp:colPr .../> appearing INLINE in run (not wrapped by hp:ctrl).
    Kept for back-compat. New parser routes hp:ctrl-wrapped colPr to LayoutDef.
    """
    raw_xml: str
    char_shape_id: int = 0
    starts_new_run: bool = False


@dataclass(frozen=True)
class LayoutDef:
    """<hp:ctrl>{<hp:colPr>|<hp:autoNum>|<hp:newNum>}</hp:ctrl> — column /
    auto-number / new-number layout directive (= davinci LayoutDef).

    raw_xml preserves the entire <hp:ctrl>...</hp:ctrl> wrapper.
    inner_tag holds the inner element name (e.g. "hp:colPr") for fast
    classification.
    """
    raw_xml: str
    inner_tag: str
    char_shape_id: int = 0
    starts_new_run: bool = False


@dataclass(frozen=True)
class ScopeDef:
    """<hp:ctrl>{<hp:header>|<hp:footer>|<hp:masterPage>}</hp:ctrl> —
    scope-bearing layout (= davinci ScopeDef).

    These contain their own <hp:subList><hp:p>...</hp:p>+ children with
    paragraph references that need ID remapping when migrated cross-doc.
    """
    raw_xml: str
    inner_tag: str
    char_shape_id: int = 0
    starts_new_run: bool = False


@dataclass(frozen=True)
class NoteDef:
    """<hp:ctrl>{<hp:footNote>|<hp:endNote>}</hp:ctrl> — footnote/endnote
    (= davinci NoteDef). Has internal paragraphs like ScopeDef.

    Reserved for completeness. Not seen in current sample corpus but the
    parser routes here if encountered.
    """
    raw_xml: str
    inner_tag: str
    char_shape_id: int = 0
    starts_new_run: bool = False


Item = Union[
    CharItem, TabItem, LineBreakItem, EmptyRunItem,
    OpaqueInlineItem, TableItem,
    SectionMeta, ColumnDef, LayoutDef, ScopeDef, NoteDef,
]

# AttachedItem = section-level scaffolding items, migrated together when
# body[0] is replaced. Davinci's AttachedDef parity.
AttachedItem = Union[SectionMeta, ColumnDef, LayoutDef, ScopeDef, NoteDef]


# ============================================================
# Paragraph
# ============================================================

@dataclass(frozen=True)
class Paragraph:
    """<hp:p paraPrIDRef=N styleIDRef=N pageBreak=0 columnBreak=0 merged=0>.

    items: sequence of (run-grouped) items. Same conceptual model as
    davinci codec's Paragraph — items are flat; the writer groups
    consecutive items by char_shape_id into <hp:run> blocks.

    raw_attrs: original <hp:p> attribute order (id/paraPrIDRef/styleIDRef/
               pageBreak/columnBreak/merged) — preserved for round-trip
               byte-equivalence.
    """
    items: tuple[Item, ...]
    para_shape_id: int
    style_id: int = 0
    char_shape_id_first: int = 0
    starts_new_page: bool = False
    starts_new_column: bool = False
    merged: bool = False
    para_id_attr: str = "0"   # <hp:p id="..."> — string because some HWPX
                              # files use very large ints exceeding u32.
    raw_attrs: dict = field(default_factory=dict)
    linesegs_xml: str = ""    # <hp:linesegarray>...</hp:linesegarray> — pre-computed.
                              # Empty = writer omits (rhwp/Hancom may regenerate).

    def with_items(self, items: tuple[Item, ...]) -> "Paragraph":
        return Paragraph(
            items=items,
            para_shape_id=self.para_shape_id,
            style_id=self.style_id,
            char_shape_id_first=self.char_shape_id_first,
            starts_new_page=self.starts_new_page,
            starts_new_column=self.starts_new_column,
            merged=self.merged,
            para_id_attr=self.para_id_attr,
            raw_attrs=dict(self.raw_attrs),
            linesegs_xml=self.linesegs_xml,
        )


# ============================================================
# Section
# ============================================================

@dataclass(frozen=True)
class Section:
    """One Contents/section[N].xml file's content.

    body: top-level paragraphs. The first body paragraph carries section
          metadata as the first <hp:run>'s first child (<hp:secPr>).
    raw_root_attrs: <hs:sec> root element attributes (xmlns: declarations
                    + any others) — preserved verbatim.
    raw_xml_decl: the leading "<?xml ... ?>" declaration of the file.
    section_index: 0-based index (matches the section[N].xml suffix).
    """
    body: tuple[Paragraph, ...] = ()
    raw_root_attrs: str = ""
    raw_xml_decl: str = ""
    section_index: int = 0


# ============================================================
# Style table — header.xml catalogs
# ============================================================

@dataclass(frozen=True)
class StyleEntry:
    """An entry in any of the catalog lists (charPr/paraPr/style/borderFill/
    tabPr/numbering/bullet). Stored as raw XML for fidelity; id is also
    extracted to the field for fast lookup.

    NOTE: HWPX entry IDs are 0-based array indices (the serializer registers
    `idx as u16` per rhwp probe). Same for borderFill — HWPX uses 0-based,
    NOT HWP-binary's 1-based convention.
    """
    id: int
    xml: str


@dataclass(frozen=True)
class FontFace:
    """<hh:font id="N" face="..." type="..." isEmbedded="0">
        <hh:typeInfo .../>?<hh:substFont .../>?</hh:font>"""
    id: int
    xml: str


@dataclass(frozen=True)
class FaceNameList:
    """<hh:fontface lang="HANGUL" fontCnt="N">FONT*</hh:fontface>.

    HWPX uses 7 lang blocks (HANGUL/LATIN/HANJA/JAPANESE/OTHER/SYMBOL/USER)
    in the parent <hh:fontfaces itemCnt="7">. Each lang has its own Id
    space starting from 0.
    """
    lang: str                     # uppercase (HANGUL, LATIN, ...)
    fonts: tuple[FontFace, ...]
    raw_attrs: dict = field(default_factory=dict)


@dataclass(frozen=True)
class StyleTable:
    """The catalog lists from Contents/header.xml's <hh:head>.

    Read order in HWPX header.xml (probed):
      hh:beginNum, hh:refList, hh:fontfaces, hh:borderFills,
      hh:charProperties, hh:tabProperties, hh:numberings, hh:bullets?,
      hh:paraProperties, hh:styles, hh:memoProperties?, ...

    refList contains things like <hh:fieldList>, <hh:bookmark> we treat as
    opaque preserved bytes (refList_xml).
    """
    face_names: tuple[FaceNameList, ...] = ()
    border_fills: tuple[StyleEntry, ...] = ()
    char_shapes: tuple[StyleEntry, ...] = ()
    tab_defs: tuple[StyleEntry, ...] = ()
    numberings: tuple[StyleEntry, ...] = ()
    bullets: tuple[StyleEntry, ...] = ()
    para_shapes: tuple[StyleEntry, ...] = ()
    styles: tuple[StyleEntry, ...] = ()


# ============================================================
# Binary items — content.hpf manifest + zip entries
# ============================================================

@dataclass(frozen=True)
class BinItem:
    """A binary asset: manifest entry + raw bytes from zip.

    manifest_id: the opf:item @id (e.g. "image2") — this is what
                 binaryItemIDRef references from <hc:img>.
    href: the opf:item @href (e.g. "BinData/image2.PNG") — also the
          zip entry name.
    media_type: opf:item @media-type (e.g. "image/png").
    is_embedded: opf:item @isEmbeded (sic — HWPX has a typo).
    data: raw bytes from the zip entry.

    BinItem is the HWPX consolidation of HWPML's two-level
    BinRef + BinItem. Cleaner — single object per binary.
    """
    manifest_id: str
    href: str
    media_type: str
    is_embedded: str   # "0" or "1"
    data: bytes


# ============================================================
# Document
# ============================================================

@dataclass(frozen=True)
class HwpxDocument:
    """A parsed HWPX zip.

    File-level layout (per probe):
      mimetype                                 — raw bytes preserved
      version.xml                              — raw bytes preserved
      settings.xml                             — raw bytes preserved
      Preview/PrvImage.png, Preview/PrvText.txt — raw bytes preserved
      META-INF/container.xml, container.rdf, manifest.xml — raw preserved
      Contents/content.hpf                     — manifest (parsed for binItems)
      Contents/header.xml                      — parsed → styles
      Contents/section[N].xml                  — parsed → sections
      Contents/masterpage[N].xml               — preserved as raw_xml_files
      BinData/imageN.{ext}                     — bin_items

    raw_xml_files: every zip entry NOT in {mimetype, settings.xml,
                   version.xml, content.hpf, header.xml, section*.xml,
                   BinData/*} is captured as raw bytes for byte-perfect
                   passthrough. masterpage*.xml live here.
    content_hpf_template: the original content.hpf as raw text — write
                          patches in updated <opf:item> entries while
                          preserving everything else (metadata, spine).
    """
    sections: tuple[Section, ...] = ()
    styles: StyleTable = field(default_factory=StyleTable)
    bin_items: tuple[BinItem, ...] = ()
    raw_xml_files: dict = field(default_factory=dict)   # name → bytes
    content_hpf_template: str = ""
    header_xml_decl: str = ""        # leading "<?xml ... ?>"
    header_root_attrs: str = ""      # <hh:head ...> attrs string
    header_pre_lists_xml: str = ""   # raw XML between <hh:head> open and
                                     # first list (hh:beginNum, hh:refList)
    header_post_lists_xml: str = ""  # raw XML after the last catalog list
                                     # (hh:trackchageConfig, etc) until </hh:head>
    doc_id: Optional[str] = None

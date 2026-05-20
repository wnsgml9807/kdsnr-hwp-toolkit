"""Question boundary detection — Korean SAT exam paper paragraph classification.

Ported from the original Rust boundary.rs. Operates on a parsed HwpxDocument.
Each unit is a contiguous range of body paragraphs forming one question (for
science/math/social) or one passage-set (for korean).
"""

from __future__ import annotations

import re
from dataclasses import dataclass, replace as _dc_replace
from enum import Enum
from typing import Callable, Iterable, Optional

from ..codec.schema import (
    CharItem,
    HwpxDocument,
    Item,
    OpaqueInlineItem,
    Paragraph,
    SectionMeta,
    TableItem,
)


class Role(Enum):
    BALMUN = "발문"      # question stem ("1.", "12." etc)
    SEONJI = "선지"      # answer choices (①〜⑤)
    MIDDLE = "중간"      # everything in between (boxes, tables, pictures)
    SET_HEADER = "세트헤더"   # passage set header for korean ("[1~3] 다음...")
    JIMUN = "지문"       # passage body for korean


class ParaKind(Enum):
    EMPTY = 0
    BALMUN = 1
    SEONJI = 2
    SET_HEADER = 3
    OTHER = 4


@dataclass
class DetectedUnit:
    label: str                 # "Q01", "S01-04", etc.
    para_indices: tuple[int, ...]
    roles: tuple[Role, ...]


# Regex (HWPML-tested in original Rust impl, work identically here).
_BALMUN_RE = re.compile(r"^\s*(\d+)\s*\.(?!\d)")
_SEONJI_RE = re.compile(r"^\s*[①-⑤]")  # ①②③④⑤
_SET_HEADER_RE = re.compile(
    r"\[\s*(\d+)\s*[~～∼∽]\s*(\d+)\s*\]"
)


_XL_STYLE_RE = re.compile(r"^xl\d+", re.IGNORECASE)
# Excel-paste imports HWPX styles named xl65, xl66, ... — these always carry
# tabular metadata (점수표, 이해도 코드 등), never real exam text.

# Korean Hancom convention — review memos sit on top of the default body
# style 바탕글 with a non-black textColor (red/blue/green). Real exam content
# is always #000000 on a typed exam style (문제, 01-문제, 일반지문, 선택지N행
# etc.), never on 바탕글.
_MEMO_STYLE_NAMES = {"바탕글"}


def _attr(xml: str, attr: str) -> Optional[str]:
    m = re.search(rf'\b{attr}="([^"]*)"', xml or "")
    return m.group(1) if m else None


def _style_name(xml: str) -> str:
    m = re.search(r'\bname="([^"]*)"', xml or "")
    return m.group(1) if m else ""


# ============================================================
# Atomic paragraph split (block-level items get their own paragraph)
# ============================================================

def _is_block_level(it: Item) -> bool:
    return isinstance(it, TableItem)


def split_fused_paragraph(p: Paragraph) -> list[Paragraph]:
    """If the paragraph contains a block-level item (TableItem) mixed with
    other content, split it into multiple paragraphs.

    Each sub-paragraph inherits paraPr/style/cs from the original. linesegs
    are dropped on split — the original lineseg cache covered the full text
    flow which no longer applies after split. starts_new_page / column flags
    are preserved on the FIRST sub-paragraph only.
    """
    items = list(p.items)
    n_blocks = sum(1 for it in items if _is_block_level(it))
    if n_blocks == 0:
        return [p]
    if n_blocks == 1 and len(items) == 1:
        return [p]
    if n_blocks == 1:
        text = "".join(it.text for it in items if isinstance(it, CharItem))
        if not text.strip():
            non_block_meaningful = any(
                isinstance(it, OpaqueInlineItem) for it in items
            )
            if not non_block_meaningful:
                return [p]

    out: list[Paragraph] = []
    bucket: list[Item] = []

    def _flush(is_first: bool):
        if not bucket:
            return
        sub = _dc_replace(
            p,
            items=tuple(bucket),
            linesegs_xml="",
            starts_new_page=p.starts_new_page if is_first else False,
            starts_new_column=p.starts_new_column if is_first else False,
        )
        out.append(sub)
        bucket.clear()

    for it in items:
        if _is_block_level(it):
            _flush(is_first=(not out))
            block_p = _dc_replace(
                p,
                items=(it,),
                linesegs_xml="",
                starts_new_page=False,
                starts_new_column=False,
            )
            out.append(block_p)
        else:
            bucket.append(it)
    _flush(is_first=(not out))
    return out


def split_fused_in_body(doc: HwpxDocument) -> HwpxDocument:
    """Walk section[0].body and apply split_fused_paragraph to each. The
    resulting body has one block-level item per paragraph at most — boundary
    detection can then run on a clean atomic stream."""
    if not doc.sections:
        return doc
    body = doc.sections[0].body
    new_body: list[Paragraph] = []
    changed = False
    for p in body:
        sub = split_fused_paragraph(p)
        if len(sub) != 1 or sub[0] is not p:
            changed = True
        new_body.extend(sub)
    if not changed:
        return doc
    new_sec = _dc_replace(doc.sections[0], body=tuple(new_body))
    return _dc_replace(doc, sections=(new_sec,) + tuple(doc.sections[1:]))


def unwrap_meta_tables(doc: HwpxDocument) -> HwpxDocument:
    """Unwrap '출제진 메타 표' that hides whole-question text in cells.

    Some src files use a meta table layout: each row holds question metadata
    (number/category/score/code) plus one cell that contains the full balmun +
    cont + box + seonji content. detect_units works on top-level body text and
    misses these.

    Heuristic: a TableItem cell with text matching `^N. ` (balmun marker) and
    length > 15 chars is treated as an embedded question. Such cells'
    paragraphs are spliced into body in order; the wrapping table paragraph
    is dropped. Other (metadata-only) cells are discarded.
    """
    if not doc.sections:
        return doc
    body = doc.sections[0].body
    new_body: list[Paragraph] = []
    changed = False
    for p in body:
        embedded_paragraphs: list[Paragraph] = []
        for it in p.items:
            if not isinstance(it, TableItem):
                continue
            if len(it.cells) < 4:
                continue
            for cell in it.cells:
                ctext = "".join(
                    itc.text
                    for cp in cell.paragraphs
                    for itc in cp.items
                    if isinstance(itc, CharItem)
                )
                stripped = ctext.lstrip("　 ")
                if _BALMUN_RE.match(stripped) and len(stripped) > 15:
                    embedded_paragraphs.extend(cell.paragraphs)
        if embedded_paragraphs:
            new_body.extend(embedded_paragraphs)
            changed = True
        else:
            new_body.append(p)
    if not changed:
        return doc
    new_sec = _dc_replace(doc.sections[0], body=tuple(new_body))
    return _dc_replace(doc, sections=(new_sec,) + tuple(doc.sections[1:]))


def paragraph_text(p: Paragraph) -> str:
    """Concatenate all CharItem.text from p (ignoring nested cell content,
    SectionMeta, etc — boundary detection only uses top-level run text)."""
    parts: list[str] = []
    for it in p.items:
        if isinstance(it, CharItem):
            parts.append(it.text)
    return "".join(parts)


def classify_paragraph(text: str, *, korean: bool) -> ParaKind:
    trimmed = text.strip()
    if not trimmed:
        return ParaKind.EMPTY
    if korean and _SET_HEADER_RE.search(text):
        return ParaKind.SET_HEADER
    if _BALMUN_RE.match(trimmed):
        return ParaKind.BALMUN
    if _SEONJI_RE.match(trimmed):
        return ParaKind.SEONJI
    return ParaKind.OTHER


def _all_cell_paragraphs(tbl: TableItem) -> list[Paragraph]:
    """모든 cell.paragraphs 를 row/col 순서대로 평탄화."""
    out: list[Paragraph] = []
    for c in tbl.cells:
        out.extend(c.paragraphs)
    return out


def _has_boundary_marker(paragraphs: list[Paragraph], *, korean: bool) -> bool:
    """paragraphs 중 어느 하나라도 BALMUN/SET_HEADER 마커를 가짐."""
    for p in paragraphs:
        kind = classify_paragraph(paragraph_text(p), korean=korean)
        if kind == ParaKind.BALMUN:
            return True
        if korean and kind == ParaKind.SET_HEADER:
            return True
    return False


def _is_wrapper_paragraph(p: Paragraph, *, korean: bool) -> bool:
    """wrapper 후보 판정.

    조건:
    - paragraph 가 단독 TableItem 만 의미 있는 item 으로 가짐 (CharItem 텍스트 비거나 공백뿐)
    - 그 표 안 cell paragraphs (모든 depth 1) 중 어느 하나라도 BALMUN/SET_HEADER 마커
      를 가짐. → 본문이 박스 안에 갇힌 형태로 판정.

    보기 박스 / 자료 박스는 cell 안 paragraphs 에 BALMUN 마커가 없으므로 자동으로
    wrapper 판정에서 제외됨 (false-positive 방지).
    """
    tbls = [it for it in p.items if isinstance(it, TableItem)]
    if len(tbls) != 1:
        return False
    other_text = "".join(
        it.text for it in p.items if isinstance(it, CharItem)
    )
    if other_text.strip():
        return False  # paragraph 자체에 텍스트가 있으면 wrapper 아님
    cell_paras = _all_cell_paragraphs(tbls[0])
    return _has_boundary_marker(cell_paras, korean=korean)


def unwrap_wrappers(doc: HwpxDocument, *, korean: bool) -> HwpxDocument:
    """body 안 wrapper paragraph 들을 cell 안 paragraphs 로 평탄화.

    재귀: 평탄화한 paragraph 가 또 wrapper 면 한 번 더 unwrap.
    header/메타 cell paragraphs 는 그대로 흘려보내짐 — 짧은 텍스트라
    boundary detector 가 unit 시작으로 잡지 않음.
    """
    if not doc.sections:
        return doc
    body = list(doc.sections[0].body)

    def _pass(paragraphs):
        out = []
        changed = False
        for p in paragraphs:
            if _is_wrapper_paragraph(p, korean=korean):
                tbl = next(it for it in p.items if isinstance(it, TableItem))
                out.extend(_all_cell_paragraphs(tbl))
                changed = True
            else:
                out.append(p)
        return out, changed

    # 재귀 fixed-point — 안에 또 wrapper 가 있으면 한 번 더
    while True:
        body, changed = _pass(body)
        if not changed:
            break

    if tuple(body) == doc.sections[0].body:
        return doc
    from dataclasses import replace as _r
    new_sec = _r(doc.sections[0], body=tuple(body))
    return _r(doc, sections=(new_sec,) + tuple(doc.sections[1:]))


def _build_memo_mask(doc: HwpxDocument) -> tuple[bool, ...]:
    """Per-body-paragraph boolean: True ⇒ paragraph is a review memo / Excel
    paste / non-content meta block, NOT real exam content.

    Three independent XML signals (any one triggers True):
      S1. textColor on the paragraph's first charPr is non-black (#FF0000
          red, #0000FF blue, #008000 green — Hancom convention for review
          markup, never used by real exam paragraphs which are #000000).
      S2. style name == "바탕글" (default body style; review memos are typed
          on 바탕글, real exam paragraphs use exam-typed styles like 문제 /
          01-문제 / 일반지문 / 선택지N행).
      S3. style name matches ^xl\\d+ (Excel paste — 점수표/이해도 메타).

    Real exam content survives all three checks; non-content meta fails at
    least one. Verified against 12 src probes covering normal exams, math
    검토 input, social 검토 input, science 검토 input.
    """
    if not doc.sections:
        return ()
    body = doc.sections[0].body
    cs_xml = {cs.id: cs.xml for cs in doc.styles.char_shapes}
    style_xml = {s.id: s.xml for s in doc.styles.styles}

    mask: list[bool] = []
    for p in body:
        first_cs = None
        for it in p.items:
            if hasattr(it, "char_shape_id"):
                first_cs = it.char_shape_id
                break
        if first_cs is None:
            first_cs = p.char_shape_id_first
        csx = cs_xml.get(first_cs, "") or ""
        stx = style_xml.get(p.style_id, "") or ""
        text_color = (_attr(csx, "textColor") or "").upper()
        style_name = _style_name(stx)

        s1 = bool(text_color) and text_color not in ("#000000", "#NONE", "")
        s2 = style_name in _MEMO_STYLE_NAMES
        s3 = bool(_XL_STYLE_RE.match(style_name))
        mask.append(s1 or s2 or s3)
    return tuple(mask)


def detect_units(doc: HwpxDocument, subject: str) -> list[DetectedUnit]:
    """Detect question/set boundaries in the first section of doc.

    Subjects:
      "science" / "math" / "social" — boundary = each balmun start.
      "korean" — boundary = each set-header start.
    """
    if not doc.sections:
        raise ValueError("doc has no sections")
    body = doc.sections[0].body
    memo_mask = _build_memo_mask(doc)

    if subject == "korean":
        return _detect_korean_sets(body, memo_mask)
    elif subject in ("science", "math", "social"):
        return _detect_questions(body, memo_mask)
    else:
        raise ValueError(f"unknown subject: {subject!r}")


def _has_visual_content(p: Paragraph) -> bool:
    """A paragraph that classifies as EMPTY by text alone may still carry a
    floating table (condition box, distribution table, etc.) as its sole
    item. These belong to the current question; without this check the
    boundary detector trims them off as separator whitespace."""
    return any(isinstance(it, TableItem) for it in p.items)


def _detect_questions(body: tuple[Paragraph, ...],
                      memo_mask: tuple[bool, ...] = ()) -> list[DetectedUnit]:
    kinds = [
        classify_paragraph(paragraph_text(p), korean=False)
        for p in body
    ]
    # Demote memo-classified paragraphs from BALMUN/SEONJI to OTHER so they
    # neither start units nor terminate questions.
    if memo_mask:
        kinds = [
            ParaKind.OTHER if memo_mask[i] and k in (ParaKind.BALMUN, ParaKind.SEONJI)
            else k
            for i, k in enumerate(kinds)
        ]
    balmun_starts = [i for i, k in enumerate(kinds) if k == ParaKind.BALMUN]

    units: list[DetectedUnit] = []
    for idx, start in enumerate(balmun_starts):
        next_start = balmun_starts[idx + 1] if idx + 1 < len(balmun_starts) else len(body)
        # Trim to the last meaningful paragraph in the question's range.
        # EMPTY paragraphs that hold a TableItem (floating condition box)
        # count as content while we're still INSIDE the question stem. A
        # TableItem found after the question has terminated (SEONJI seen, or
        # the "[4점]" point-marker text encountered) is a section divider
        # — 단답형 / 5지선다형 label, 확인사항 footer, etc. — and belongs
        # to whatever comes next, not the current question.
        last = start
        question_terminated = False  # SEONJI seen OR [4점] in non-stem para
        for j in range(start, next_start):
            if kinds[j] == ParaKind.SEONJI:
                last = j
                question_terminated = True
            elif kinds[j] in (ParaKind.BALMUN, ParaKind.OTHER):
                last = j
                # [4점] in the stem paragraph itself (e.g. 단답형 questions
                # like Q20: full stem inline including "[4점]" followed by a
                # condition box) does NOT terminate the question — the box
                # paragraph is still ahead. [4점] in a later paragraph (a
                # continuation OTHER row) is the true termination marker.
                if j != start and "[4점]" in paragraph_text(body[j]):
                    question_terminated = True
            elif (not question_terminated
                  and kinds[j] == ParaKind.EMPTY
                  and _has_visual_content(body[j])):
                last = j
        para_indices = tuple(
            i for i in range(start, last + 1)
            if not (memo_mask and memo_mask[i])
        )
        roles = tuple(
            Role.BALMUN if kinds[i] == ParaKind.BALMUN
            else Role.SEONJI if kinds[i] == ParaKind.SEONJI
            else Role.MIDDLE
            for i in para_indices
        )
        m = _BALMUN_RE.match(paragraph_text(body[start]).strip())
        q_num = int(m.group(1)) if m else (idx + 1)
        units.append(DetectedUnit(
            label=f"Q{q_num:02d}",
            para_indices=para_indices,
            roles=roles,
        ))
    return units


def _detect_korean_sets(body: tuple[Paragraph, ...],
                        memo_mask: tuple[bool, ...] = ()) -> list[DetectedUnit]:
    headers: list[tuple[int, int, int]] = []   # (para_idx, from_q, to_q)
    for i, p in enumerate(body):
        if memo_mask and memo_mask[i]:
            continue
        m = _SET_HEADER_RE.search(paragraph_text(p))
        if m:
            try:
                from_q = int(m.group(1))
                to_q = int(m.group(2))
            except ValueError:
                from_q = to_q = 0
            headers.append((i, from_q, to_q))

    units: list[DetectedUnit] = []
    for idx, (start, from_q, to_q) in enumerate(headers):
        next_start = headers[idx + 1][0] if idx + 1 < len(headers) else len(body)
        para_indices = tuple(
            i for i in range(start, next_start)
            if not (memo_mask and memo_mask[i])
        )
        roles = tuple(
            Role.SET_HEADER if i == start
            else Role.BALMUN if classify_paragraph(paragraph_text(body[i]), korean=True) == ParaKind.BALMUN
            else Role.SEONJI if classify_paragraph(paragraph_text(body[i]), korean=True) == ParaKind.SEONJI
            else Role.JIMUN
            for i in para_indices
        )
        units.append(DetectedUnit(
            label=f"S{from_q:02d}-{to_q:02d}",
            para_indices=para_indices,
            roles=roles,
        ))
    return units


def disambiguate_labels(units: Iterable[DetectedUnit]) -> list[str]:
    """If multiple units have the same label, append _2/_3 etc."""
    seen: dict[str, int] = {}
    out: list[str] = []
    for u in units:
        n = seen.get(u.label, 0) + 1
        seen[u.label] = n
        if n == 1:
            out.append(u.label)
        else:
            out.append(f"{u.label}_{n}")
    return out

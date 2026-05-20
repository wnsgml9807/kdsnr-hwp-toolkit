"""Paragraph-level operations on HwpxDocument.

Simple insert/delete plus the special case we actually need for the
question splitter: replace_section_body.

The HWPX equivalent of HWPML's "section meta migration on body[0] change"
is centered on <hp:secPr>. SectionMeta lives inside the FIRST <hp:run> of
the FIRST <hp:p>. When we replace the section body, we keep the first
paragraph (the one carrying SectionMeta + ColumnDef) and append the new
paragraphs after it. This preserves the section configuration intact.
"""

from __future__ import annotations

from dataclasses import replace as _replace

from dataclasses import replace as _dc_replace

from ..schema import (
    CharItem,
    ColumnDef,
    HwpxDocument,
    Item,
    LayoutDef,
    LineBreakItem,
    NoteDef,
    Paragraph,
    ScopeDef,
    Section,
    SectionMeta,
    TabItem,
)


def _is_structural(it: Item) -> bool:
    """Davinci AttachedDef parity: SectionDef + LayoutDef + ScopeDef + NoteDef.
    Routed to typed classes by read.py so we just isinstance-check.
    """
    return isinstance(it, (SectionMeta, ColumnDef, LayoutDef, ScopeDef, NoteDef))


def _is_text_content(it: Item) -> bool:
    """Plain-text content items (= example text inside template body[0]).
    Stripping these from template body[0] removes the "[18~21] 다음 글을…"
    type example text that templates carry in their structural carrier
    paragraph, while preserving structure + decorative tables/shapes.
    """
    return isinstance(it, (CharItem, TabItem, LineBreakItem))


def insert_paragraphs(
    doc: HwpxDocument,
    section_idx: int,
    at_para_idx: int,
    new_paras: tuple[Paragraph, ...] | list[Paragraph],
    *,
    position: str = "before",
) -> HwpxDocument:
    """Insert new_paras at section[section_idx].body[at_para_idx]."""
    if section_idx < 0 or section_idx >= len(doc.sections):
        raise IndexError(f"section {section_idx} out of range")
    sec = doc.sections[section_idx]
    n = len(sec.body)
    if position == "before":
        if at_para_idx < 0 or at_para_idx > n:
            raise IndexError(f"at_para_idx {at_para_idx} out of range")
        idx = at_para_idx
    elif position == "after":
        if at_para_idx < -1 or at_para_idx > n:
            raise IndexError(f"at_para_idx {at_para_idx} out of range")
        idx = at_para_idx + 1
    else:
        raise ValueError(f"position must be 'before' or 'after', got {position!r}")

    new_body = tuple(sec.body[:idx]) + tuple(new_paras) + tuple(sec.body[idx:])

    # Body[0] section-meta migration: if we just inserted before idx 0 of an
    # existing body that has SectionMeta on its (now-shifted) old first
    # paragraph, the meta must move to the new body[0].
    if idx == 0 and len(sec.body) > 0:
        old_first = sec.body[0]
        attached = tuple(
            it for it in old_first.items
            if isinstance(it, (SectionMeta, ColumnDef))
        )
        if attached:
            cleaned_old = old_first.with_items(tuple(
                it for it in old_first.items
                if not isinstance(it, (SectionMeta, ColumnDef))
            ))
            new_first = new_body[0].with_items(attached + new_body[0].items)
            new_body_list = list(new_body)
            new_body_list[0] = new_first
            new_body_list[len(new_paras)] = cleaned_old
            new_body = tuple(new_body_list)

    new_sec = _replace(sec, body=new_body)
    new_sections = tuple(
        new_sec if i == section_idx else s for i, s in enumerate(doc.sections)
    )
    return _replace(doc, sections=new_sections)


def delete_paragraphs(
    doc: HwpxDocument,
    section_idx: int,
    para_indices: tuple[int, ...] | list[int],
) -> HwpxDocument:
    """Delete paragraphs by 0-based index in section body.

    If body[0] is being deleted and contained SectionMeta/ColumnDef, those
    are migrated to the new body[0] (which becomes the surviving first).
    Refuses to leave an empty body.
    """
    if section_idx < 0 or section_idx >= len(doc.sections):
        raise IndexError(f"section {section_idx} out of range")
    sec = doc.sections[section_idx]
    n = len(sec.body)
    to_remove = set(para_indices)
    for i in to_remove:
        if i < 0 or i >= n:
            raise IndexError(f"index {i} out of range")
    if len(to_remove) >= n:
        raise ValueError("cannot delete all paragraphs in a section")

    new_body = tuple(p for i, p in enumerate(sec.body) if i not in to_remove)

    if 0 in to_remove:
        old_first = sec.body[0]
        attached = tuple(
            it for it in old_first.items
            if isinstance(it, (SectionMeta, ColumnDef))
        )
        if attached:
            new_first = new_body[0]
            new_body = (
                new_first.with_items(attached + new_first.items),
            ) + new_body[1:]

    new_sec = _replace(sec, body=new_body)
    new_sections = tuple(
        new_sec if i == section_idx else s for i, s in enumerate(doc.sections)
    )
    return _replace(doc, sections=new_sections)


def replace_section_body(
    doc: HwpxDocument,
    section_idx: int,
    new_body_paras: tuple[Paragraph, ...] | list[Paragraph],
    *,
    keep_attached_from_template: bool = True,
    keep_scope_defs: bool = False,
) -> HwpxDocument:
    """Replace section body, migrating template body[0]'s structure items.

    HWP/HWPX 계약: section의 모든 레이아웃 메타 (<hp:secPr>, <hp:colPr>,
    <hp:header>/<hp:footer>, <hp:masterPage>, <hp:footNote>/<hp:endNote>,
    <hp:newNum>/<hp:autoNum>/<hp:pageNumPos>) 는 body[0] 첫 run의 inline
    item으로 살아있어야 함. 양식 body[0]의 content (CharItem 등) 는 양식
    예제일 뿐이므로 버리고, 구조 item만 src body[0] 앞에 prepend.

    new_body_paras가 비면 양식 body[0]의 구조만 남긴 빈 단락을 둠.

    다빈치 codec/operations/paragraphs.py의 attached 이전 패턴을 HWPX로 옮긴 것.
    """
    if section_idx < 0 or section_idx >= len(doc.sections):
        raise IndexError(f"section {section_idx} out of range")
    sec = doc.sections[section_idx]

    if keep_attached_from_template and sec.body:
        # 양식 body[0] paragraph wrapper 그대로 보존 — paraPrIDRef/styleIDRef
        # /char_shape_id_first 등 양식이 의도한 "구조 전용 빈 paragraph"
        # 속성을 유지해야 1페이지 레이아웃이 정상. src paragraph 속성으로
        # body[0]을 만들면 HWP가 섹션 루트에 src의 발문용 margin/border/
        # lineSpacing을 적용해서 레이아웃 깨짐.
        #
        # 양식 body[0]에서 CharItem/TabItem/LineBreakItem만 제거 — 국어
        # 양식의 "[18~21] 다음 글을…" 같은 예제 텍스트가 첫 줄에 새서
        # 보이는 것을 막음. 구조 item + decorative table/shape 는 보존.
        template_first = sec.body[0]
        kept_items = tuple(
            it for it in template_first.items
            if not _is_text_content(it)
            and (keep_scope_defs or not isinstance(it, ScopeDef))
        )
        # text content 뒤에 따라오던 starts_new_run 마커는 의미 없어졌을 수
        # 있으나 그대로 둬도 emit 영향 없음 (cs 다르면 어차피 break).
        new_first = template_first.with_items(kept_items)
        new_body = (new_first,) + tuple(new_body_paras)
    else:
        new_body = tuple(new_body_paras)
        if not new_body:
            raise ValueError("empty section body and no template first para")

    new_sec = _replace(sec, body=new_body)
    new_sections = tuple(
        new_sec if i == section_idx else s for i, s in enumerate(doc.sections)
    )
    return _replace(doc, sections=new_sections)

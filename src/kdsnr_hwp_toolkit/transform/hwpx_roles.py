from __future__ import annotations

import re
from dataclasses import replace
from typing import Mapping

from kdsnr_hwp_toolkit.codec.schema import (
    CharItem,
    EmptyRunItem,
    LineBreakItem,
    OpaqueInlineItem,
    Paragraph,
    TabItem,
    TableItem,
)
from kdsnr_hwp_toolkit.core.model import Role


ROLE_TO_TEMPLATE_KEY = {
    Role.STEM: ("balmun_paraPr_id", "balmun_style_id", "balmun_charPr_id"),
    Role.STEM_CONTINUATION: ("balmun_cont_paraPr_id", "balmun_cont_style_id", "balmun_cont_charPr_id"),
    Role.CHOICES: ("seonji_paraPr_id", "seonji_style_id", "seonji_charPr_id"),
    Role.EQUATION_BLOCK: ("eq_block_paraPr_id", "eq_block_style_id", "eq_block_charPr_id"),
    Role.PICTURE_BLOCK: ("pic_block_paraPr_id", "pic_block_style_id", "pic_block_charPr_id"),
    Role.INLINE_TABLE: ("box_wrap_paraPr_id", "box_wrap_style_id", "box_wrap_charPr_id"),
    Role.UNKNOWN: ("balmun_cont_paraPr_id", "balmun_cont_style_id", "balmun_cont_charPr_id"),
}

CHOICE_TAB_ATTRS = (
    {"width": "4601", "leader": "0", "type": "1"},
    {"width": "4589", "leader": "0", "type": "1"},
    {"width": "4639", "leader": "0", "type": "1"},
    {"width": "4614", "leader": "0", "type": "1"},
)


def _with_char_shape(item, char_shape_id: int):
    if isinstance(item, CharItem):
        return replace(item, char_shape_id=char_shape_id)
    if isinstance(item, TabItem):
        return replace(item, char_shape_id=char_shape_id)
    if isinstance(item, LineBreakItem):
        return replace(item, char_shape_id=char_shape_id)
    if isinstance(item, EmptyRunItem):
        return replace(item, char_shape_id=char_shape_id)
    if isinstance(item, OpaqueInlineItem):
        return replace(item, char_shape_id=char_shape_id)
    if isinstance(item, TableItem):
        return replace(item, char_shape_id=char_shape_id)
    return item


def _paragraph_text(paragraph: Paragraph) -> str:
    return "".join(it.text for it in paragraph.items if isinstance(it, CharItem))


def _template_choice_items(paragraph: Paragraph, char_shape_id: int):
    text = _paragraph_text(paragraph)
    chunks = [
        re.sub(r"\s+", " ", m.group(0)).strip()
        for m in re.finditer(r"[①-⑤][^①-⑤]*", text)
    ]
    if len(chunks) != 5:
        return tuple(_with_char_shape(it, char_shape_id) for it in paragraph.items)

    items = []
    for idx, chunk in enumerate(chunks):
        items.append(CharItem(text=chunk, char_shape_id=char_shape_id, starts_new_run=(idx == 0)))
        if idx < 4:
            items.append(TabItem(char_shape_id=char_shape_id, tab_attrs=CHOICE_TAB_ATTRS[idx]))
    return tuple(items)


def apply_hwpx_template_role_style(
    paragraph: Paragraph,
    role: Role,
    role_map: Mapping[str, int],
) -> Paragraph:
    """Apply template-owned role style to a non-box paragraph."""

    pp_key, st_key, cs_key = ROLE_TO_TEMPLATE_KEY.get(role, ROLE_TO_TEMPLATE_KEY[Role.UNKNOWN])
    pp_id = int(role_map[pp_key])
    st_id = int(role_map[st_key])
    cs_id = int(role_map[cs_key])
    items = (
        _template_choice_items(paragraph, cs_id)
        if role == Role.CHOICES
        else tuple(_with_char_shape(it, cs_id) for it in paragraph.items)
    )
    return replace(
        paragraph,
        items=items,
        para_shape_id=pp_id,
        style_id=st_id,
        char_shape_id_first=cs_id,
        starts_new_page=False,
        starts_new_column=False,
        linesegs_xml="",
    )

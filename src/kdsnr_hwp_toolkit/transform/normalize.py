"""Post-merge style normalization — global policy enforcement.

User policy (2026-05-11):
  - 색깔은 무조건 검정 통일 (text color → #000000)
  - 배경색 제거 (shadeColor → white, fillBrush faceColor → none)
  - 메모 등 제거 (hp:memo controls dropped from src body)

Other char-shape attributes (font, size, bold, italic, underline) are NOT
touched — only color and background. So underlines and emphasis survive.
"""
from __future__ import annotations

import re
from dataclasses import replace

from kdsnr_hwp_toolkit.codec.schema import (
    HwpxDocument, StyleEntry, StyleTable,
)


_TEXT_COLOR_RE = re.compile(r'textColor="[^"]*"')
_SHADE_COLOR_RE = re.compile(r'shadeColor="[^"]*"')
_FACE_COLOR_RE = re.compile(r'faceColor="[^"]*"')
_HATCH_COLOR_RE = re.compile(r'hatchColor="[^"]*"')
_FILL_ALPHA_RE = re.compile(r'(<hc:winBrush\b[^>]*?)\balpha="[^"]*"')


def _force_black_charPr(entry: StyleEntry) -> StyleEntry:
    xml = entry.xml
    xml = _TEXT_COLOR_RE.sub('textColor="#000000"', xml)
    xml = _SHADE_COLOR_RE.sub('shadeColor="#FFFFFF"', xml)
    return StyleEntry(id=entry.id, xml=xml)


def _strip_borderFill_fill(entry: StyleEntry) -> StyleEntry:
    """Force any fillBrush face color to 'none' (transparent)."""
    xml = entry.xml
    xml = _FACE_COLOR_RE.sub('faceColor="none"', xml)
    # also reset hatchColor to a neutral value (kept for schema validity)
    xml = _HATCH_COLOR_RE.sub('hatchColor="#FF000000"', xml)
    # set alpha=0 if winBrush has alpha attr
    xml = _FILL_ALPHA_RE.sub(r'\1 alpha="0"', xml)
    return StyleEntry(id=entry.id, xml=xml)


def normalize_styles(doc: HwpxDocument) -> HwpxDocument:
    """Apply global black-color + transparent-fill policy."""
    new_charPrs = tuple(_force_black_charPr(e) for e in doc.styles.char_shapes)
    new_borderFills = tuple(
        _strip_borderFill_fill(e) for e in doc.styles.border_fills
    )
    new_styles = replace(
        doc.styles,
        char_shapes=new_charPrs,
        border_fills=new_borderFills,
    )
    return replace(doc, styles=new_styles)


__all__ = ["normalize_styles"]

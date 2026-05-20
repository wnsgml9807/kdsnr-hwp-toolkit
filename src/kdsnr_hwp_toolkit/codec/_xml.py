"""Quote-aware XML scanning helpers — shared by read/write/classify.

We avoid lxml because:
  - it normalizes attribute order (we need byte-level round-trip fidelity).
  - it rewrites namespace declarations (HWPX has 14 namespaces, all needed).
  - it adds whitespace HWP doesn't tolerate in some contexts.

Instead we use raw text manipulation with quote-aware tag-end scanning.
The davinci codec uses the same approach for HWPML and it works in
production for the team's real samples.

Tag names in HWPX include namespace prefixes (hp:, hh:, hc:, hpf:, opf:,
hm:, hs:, ha:, hp10:, hhs:, dc:, ooxmlchart:, hwpunitchar:, epub:, config:).
Standard \\w doesn't include ":", so all tag-name regex uses explicit
character class [a-zA-Z][\\w-]*(?::[\\w-]+)?.
"""

from __future__ import annotations

import re
from typing import Iterator


# Tag name pattern: optional namespace prefix + local name. Matches things
# like "hp:p", "hh:charPr", "hc:img", "opf:item", and bare "masterPage".
TAG_NAME = r"[a-zA-Z][\w.-]*(?::[a-zA-Z][\w.-]*)?"

# Attribute pattern: name="value" with non-quote value (HWPX never embeds
# escaped quotes per probe; raw " is the only quote char).
_ATTR_RE = re.compile(rf'({TAG_NAME})="([^"]*)"')


def find_tag_end(xml: str, start: int) -> int:
    """quote-aware `>` scan. start should be at or after the opening `<`.

    Returns the index of the closing `>` of the tag at `start`, or -1 if
    not found. Skips `>` characters inside `"..."` attribute values.
    """
    in_quote = False
    i = start
    n = len(xml)
    while i < n:
        c = xml[i]
        if in_quote:
            if c == '"':
                in_quote = False
        else:
            if c == '"':
                in_quote = True
            elif c == ">":
                return i
        i += 1
    return -1


def find_matching_close(xml: str, open_start: int, tag: str,
                        limit: int | None = None) -> int:
    """Return the exclusive end of the </tag> that matches the <tag ...>
    starting at open_start. -1 if unmatched within `limit`.

    Handles self-closing siblings of the same tag (does not increment depth)
    and quote-aware `>` matching.
    """
    if limit is None:
        limit = len(xml)
    depth = 0
    pos = open_start
    while pos < limit:
        lt = xml.find("<", pos)
        if lt < 0 or lt >= limit:
            return -1
        gt = find_tag_end(xml, lt)
        if gt < 0:
            return -1
        seg = xml[lt:gt + 1]
        m = re.match(rf"<(/?)({TAG_NAME})", seg)
        if not m:
            pos = gt + 1
            continue
        is_close = bool(m.group(1))
        this_tag = m.group(2)
        is_self = seg.endswith("/>")
        if this_tag == tag:
            if is_close:
                depth -= 1
                if depth == 0:
                    return gt + 1
            elif not is_self:
                depth += 1
        pos = gt + 1
    return -1


def parse_attrs(s: str) -> dict:
    """Parse attribute string into ordered dict (insertion = source order)."""
    return {m.group(1): m.group(2) for m in _ATTR_RE.finditer(s)}


def parse_open_tag(xml: str) -> tuple[str, dict, int, bool]:
    """Returns (tag_name, attrs, open_tag_end_pos, is_self_close).

    open_tag_end_pos is the position after the '>' (exclusive end of the
    open tag). For self-close `<tag .../>`, is_self_close is True.

    Raises ValueError if the input doesn't start with a valid open tag.
    """
    m = re.match(rf"<({TAG_NAME})\b([^>]*)>", xml)
    if not m:
        raise ValueError(f"bad open tag: {xml[:80]!r}")
    tag = m.group(1)
    attrs_str = m.group(2)
    is_self = attrs_str.endswith("/")
    if is_self:
        attrs_str = attrs_str.rstrip("/")
    return tag, parse_attrs(attrs_str), m.end(), is_self


def iter_direct_children(
    xml: str, start: int, end: int,
) -> Iterator[tuple[int, int, str]]:
    """Yield (child_start, child_end, tag) for every direct child element
    inside xml[start:end], skipping nested content.

    Whitespace and text nodes are silently skipped. A close tag at depth 0
    terminates iteration (we treat it as the parent's closing tag).
    """
    pos = start
    while pos < end:
        lt = xml.find("<", pos)
        if lt < 0 or lt >= end:
            return
        if xml[lt:lt + 2] == "</":
            return  # parent close
        gt = find_tag_end(xml, lt)
        if gt < 0:
            return
        seg = xml[lt:gt + 1]
        m = re.match(rf"<({TAG_NAME})", seg)
        if not m:
            pos = gt + 1
            continue
        tag = m.group(1)
        if seg.endswith("/>"):
            yield (lt, gt + 1, tag)
            pos = gt + 1
            continue
        close_end = find_matching_close(xml, lt, tag, end)
        if close_end <= 0:
            return
        yield (lt, close_end, tag)
        pos = close_end


def escape_attr(s: str) -> str:
    return (
        s.replace("&", "&amp;")
         .replace('"', "&quot;")
         .replace("<", "&lt;")
    )


def escape_text(s: str) -> str:
    """Minimal escape for content of <hp:t> and similar leaf text nodes."""
    return s.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")


def emit_open_tag(tag: str, attrs: dict, self_close: bool = False) -> str:
    """Emit `<tag a="x" b="y">` or `<tag a="x"/>` preserving attr order."""
    if attrs:
        attr_str = "".join(
            f' {k}="{escape_attr(str(v))}"' for k, v in attrs.items()
        )
    else:
        attr_str = ""
    if self_close:
        return f"<{tag}{attr_str}/>"
    return f"<{tag}{attr_str}>"

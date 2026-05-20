from __future__ import annotations

import io
import re
import zipfile

from .. import _native as _native_mod


_CURSZ_RE = re.compile(r'<hp:curSz\s+width="(\d+)"\s+height="(\d+)"\s*/>')
_ORGSZ_ZERO_RE = re.compile(r'<hp:orgSz\s+width="0"\s+height="0"\s*/>')


def hwp_to_hwpx(input_data: bytes) -> bytes:
    """Convert HWP/HWPX bytes to HWPX bytes via the bundled rhwp binding."""

    if input_data.startswith(b"PK\x03\x04"):
        return input_data
    hwpx = bytes(_native_mod.hwp_to_hwpx(input_data))
    hwpx = _fix_rhwp_picture_orgsz_keep_clip(hwpx)
    hwpx = _fix_rhwp_equation_attrs(hwpx)
    return hwpx


_EQ_TAG_RE = re.compile(
    r'(<hp:equation\b[^>]*\bversion=)"([^"]*)"([^>]*\bfont=)"([^"]*)"'
)


def _patch_equation_font_swap(section_xml: str) -> str:
    """rhwp swaps equation version/font attrs — restore the canonical pair."""
    def _swap(m: re.Match) -> str:
        version_val = m.group(2)
        font_val = m.group(4)
        if version_val == "" and font_val == "Equation Version 60":
            return f'{m.group(1)}"Equation Version 60"{m.group(3)}"HYhwpEQ"'
        return m.group(0)
    return _EQ_TAG_RE.sub(_swap, section_xml)


def _fix_rhwp_equation_attrs(hwpx_bytes: bytes) -> bytes:
    """Repack section XMLs with corrected equation attributes."""
    src_buf = io.BytesIO(hwpx_bytes)
    out_buf = io.BytesIO()
    with zipfile.ZipFile(src_buf, "r") as zin, \
         zipfile.ZipFile(out_buf, "w", zipfile.ZIP_DEFLATED) as zout:
        for item in zin.infolist():
            data = zin.read(item.filename)
            if item.filename.startswith("Contents/section") and item.filename.endswith(".xml"):
                text = data.decode("utf-8")
                text = _patch_equation_font_swap(text)
                data = text.encode("utf-8")
            zout.writestr(item, data)
    return out_buf.getvalue()


def _fix_rhwp_picture_orgsz_keep_clip(hwpx_bytes: bytes) -> bytes:
    """Set zero hp:orgSz to hp:curSz while preserving source imgClip."""
    out_buf = io.BytesIO()
    with zipfile.ZipFile(io.BytesIO(hwpx_bytes), "r") as zin, zipfile.ZipFile(
        out_buf, "w", zipfile.ZIP_DEFLATED
    ) as zout:
        for item in zin.infolist():
            data = zin.read(item.filename)
            if item.filename.startswith("Contents/section") and item.filename.endswith(".xml"):
                text = data.decode("utf-8")
                text = _patch_pic_orgsz_keep_clip(text)
                data = text.encode("utf-8")
            zout.writestr(item, data)
    return out_buf.getvalue()


def _patch_pic_orgsz_keep_clip(section_xml: str) -> str:
    out: list[str] = []
    pos = 0
    while True:
        start = section_xml.find("<hp:pic", pos)
        if start < 0:
            out.append(section_xml[pos:])
            break
        end = section_xml.find("</hp:pic>", start)
        if end < 0:
            out.append(section_xml[pos:])
            break
        end += len("</hp:pic>")
        out.append(section_xml[pos:start])
        chunk = section_xml[start:end]
        m = _CURSZ_RE.search(chunk)
        if m:
            w, h = m.group(1), m.group(2)
            chunk = _ORGSZ_ZERO_RE.sub(f'<hp:orgSz width="{w}" height="{h}"/>', chunk, count=1)
        out.append(chunk)
        pos = end
    return "".join(out)

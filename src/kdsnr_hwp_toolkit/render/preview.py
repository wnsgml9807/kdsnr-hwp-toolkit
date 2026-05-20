from __future__ import annotations

import os
import subprocess
import tempfile
from pathlib import Path
from shutil import copyfile


_CROP_LEFT_X_FRACTION = 0.04
_CROP_RIGHT_X_FRACTION = 0.515
# Padding above the detected top edge (divider line OR first content row).
_CROP_HEADER_PADDING_PX = 20
# Maximum allowed top crop expressed as fraction of page height — guards
# against a divider line accidentally clipping below real body content. Below
# this fraction the detector falls back to first-content-row detection.
_CROP_HEADER_MAX_FRACTION = 0.10
_CROP_DPI = 200
_CROP_CONTENT_MARGIN_PX = 44
_CROP_VERTICAL_RULE_MIN_FRACTION = 0.55
_CROP_HORIZONTAL_RULE_MIN_FRACTION = 0.55
_CROP_VERTICAL_GROUP_GAP_PX = 120


def _default_rhwp_bin() -> Path:
    env = os.environ.get("RHWP_BIN")
    if env:
        return Path(env)
    # parents[3] = kdsnr-hwp-toolkit (이 패키지의 source 와 함께 vendor 된 rhwp).
    # 옛 flap-hwp-parser/vendor/rhwp 는 더 이상 사용하지 않음.
    toolkit_root = Path(__file__).resolve().parents[3]
    return toolkit_root / "vendor" / "rhwp" / "target" / "release" / "rhwp"


def _hft_args() -> list[str]:
    hft_path = os.environ.get("RHWP_HFT_PATH")
    if not hft_path and os.environ.get("RHWP_USE_SYSTEM_HFT") == "1":
        default_dir = "/Applications/Hancom Office HWP.app/Contents/Resources/Hnc/Shared/Fonts"
        if Path(default_dir).is_dir():
            hft_path = default_dir
    return ["--hft-path", hft_path] if hft_path else []


def _render_env() -> dict[str, str]:
    return os.environ.copy()


def render_pdf(hwpx_path: str | os.PathLike, pdf_path: str | os.PathLike, *, rhwp_bin: str | os.PathLike | None = None) -> Path:
    """Render an HWPX file to PDF.

    `--reflow` 는 더 이상 필요하지 않다. rhwp 의 parse 시점 자동 보정
    (`DocumentCore::reflow_zero_height_paragraphs` + `reflow_zero_height_table_cells`)
    이 본문/셀 단락의 placeholder lineseg 를 self-encoded segment_width
    기준으로 자동 reflow 한다.
    """

    hwpx = Path(hwpx_path)
    pdf = Path(pdf_path)
    rhwp = Path(rhwp_bin) if rhwp_bin is not None else _default_rhwp_bin()
    if not rhwp.exists():
        raise FileNotFoundError(f"rhwp binary not found: {rhwp}")
    pdf.parent.mkdir(parents=True, exist_ok=True)
    cmd = [str(rhwp), "export-pdf", str(hwpx), "-o", str(pdf)]
    # rhwp 는 embedded HFT cache 를 기본 사용한다. RHWP_HFT_PATH 는
    # 로컬 한컴 HFT 디렉터리로 강제 override 해야 할 때만 전달한다.
    cmd.extend(_hft_args())
    subprocess.run(cmd, check=True, capture_output=True, env=_render_env())
    return pdf


def render_svg(hwpx_path: str | os.PathLike, svg_path: str | os.PathLike, *, rhwp_bin: str | os.PathLike | None = None) -> Path:
    """Render the first page of an HWPX file to SVG."""

    hwpx = Path(hwpx_path)
    svg = Path(svg_path)
    rhwp = Path(rhwp_bin) if rhwp_bin is not None else _default_rhwp_bin()
    if not rhwp.exists():
        raise FileNotFoundError(f"rhwp binary not found: {rhwp}")
    svg.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory() as td:
        out_dir = Path(td)
        cmd = [str(rhwp), "export-svg", str(hwpx), "-o", str(out_dir), "-p", "0"]
        cmd.extend(_hft_args())
        subprocess.run(cmd, check=True, capture_output=True, env=_render_env())
        produced = out_dir / f"{hwpx.stem}.svg"
        if not produced.exists():
            candidates = sorted(out_dir.glob("*.svg"))
            if not candidates:
                raise RuntimeError(f"rhwp produced no SVG (hwpx={hwpx})")
            produced = candidates[0]
        copyfile(produced, svg)
    return svg


def _detect_header_divider_y(img) -> int:
    """Find the top Y to crop above. Priority:

    1. Horizontal divider line in the top quarter (page-header underline).
    2. First row containing ink (= top of real body content) minus padding.

    Earlier behavior fell back to a fixed `0.17 * h` fraction, which clipped
    body content when the template no longer drew a header divider — the
    current ksat templates start the body around y≈0.12·h, well above 0.17·h.
    """
    import numpy as np

    arr = np.asarray(img.convert("L"))
    h, w = arr.shape
    ink_mask = arr < 200
    if not ink_mask.any():
        return 0

    # 1) horizontal divider in the top quarter — but only accept it if it
    #    sits *above* the content top, otherwise it's part of the body.
    search_h = h // 4
    threshold = int(w * 0.5)
    max_divider_y = int(h * _CROP_HEADER_MAX_FRACTION)
    for y in range(min(search_h, max_divider_y)):
        if int((arr[y] < 200).sum()) > threshold:
            return min(h, y + _CROP_HEADER_PADDING_PX)

    # 2) fallback: first content row minus padding.
    rows_with_ink = np.where(ink_mask.any(axis=1))[0]
    first_content_y = int(rows_with_ink[0])
    return max(0, first_content_y - _CROP_HEADER_PADDING_PX)


def pdf_to_question_png(
    pdf_path: str | os.PathLike,
    png_path: str | os.PathLike,
    *,
    subject: str | None = None,
) -> Path:
    """Render first PDF page and crop to the left-column question region."""

    from PIL import Image

    pdf = Path(pdf_path)
    png = Path(png_path)
    with tempfile.TemporaryDirectory() as td:
        prefix = Path(td) / "raw"
        subprocess.run(
            [
                "pdftoppm",
                "-r",
                str(_CROP_DPI),
                "-png",
                "-f",
                "1",
                "-l",
                "1",
                str(pdf),
                str(prefix),
            ],
            check=True,
            capture_output=True,
        )
        produced = list(prefix.parent.glob(f"{prefix.name}-*.png"))
        if not produced:
            candidate = prefix.parent / f"{prefix.name}.png"
            if candidate.exists():
                produced = [candidate]
        if not produced:
            raise RuntimeError(f"pdftoppm produced no PNG (pdf={pdf})")
        img = Image.open(produced[0])

    w, h = img.size
    header_y = max(0, _detect_header_divider_y(img) - _CROP_CONTENT_MARGIN_PX)
    left_x = int(w * _CROP_LEFT_X_FRACTION)
    right_x = int(w * _CROP_RIGHT_X_FRACTION)
    body = img.crop((left_x, header_y, right_x, h))

    content_bbox = _content_bbox(body)
    if content_bbox is not None:
        left, top, right, bottom = content_bbox
        content = body.crop((left, top, right, bottom))
        padded = Image.new(
            body.mode,
            (
                content.width + _CROP_CONTENT_MARGIN_PX * 2,
                content.height + _CROP_CONTENT_MARGIN_PX * 2,
            ),
            "white",
        )
        padded.paste(content, (_CROP_CONTENT_MARGIN_PX, _CROP_CONTENT_MARGIN_PX))
        body = padded

    png.parent.mkdir(parents=True, exist_ok=True)
    body.save(png, optimize=True)
    return png


def _content_bbox(img) -> tuple[int, int, int, int] | None:
    """Return the first content block bbox inside the initial page crop.

    Page footers and column divider strokes can otherwise make a naive bbox
    span the full page. Drop near-full-height vertical rule columns, then keep
    the first ink group before a large blank gap. The caller adds a uniform
    margin around the returned ink bbox.
    """

    import numpy as np

    arr = np.asarray(img.convert("L"))
    mask = arr < 245
    if not mask.any():
        return None

    col_counts = mask.sum(axis=0)
    long_rule_cols = col_counts > int(mask.shape[0] * _CROP_VERTICAL_RULE_MIN_FRACTION)
    if long_rule_cols.any():
        mask[:, long_rule_cols] = False
    row_counts = mask.sum(axis=1)
    long_rule_rows = row_counts > int(mask.shape[1] * _CROP_HORIZONTAL_RULE_MIN_FRACTION)
    if long_rule_rows.any():
        mask[long_rule_rows, :] = False
    rows = np.where(mask.any(axis=1))[0]
    if len(rows) == 0:
        return None

    start = prev = int(rows[0])
    for row in rows[1:]:
        row = int(row)
        if row - prev > _CROP_VERTICAL_GROUP_GAP_PX:
            break
        prev = row
    block = mask[start:prev + 1, :]
    cols = np.where(block.any(axis=0))[0]
    if len(cols) == 0:
        return None
    return int(cols[0]), start, int(cols[-1]) + 1, prev + 1


def render_question_png(
    hwpx_path: str | os.PathLike,
    png_path: str | os.PathLike,
    *,
    rhwp_bin: str | os.PathLike | None = None,
    subject: str | None = None,
) -> Path:
    png = Path(png_path)
    with tempfile.TemporaryDirectory() as td:
        pdf = Path(td) / "question.pdf"
        render_pdf(hwpx_path, pdf, rhwp_bin=rhwp_bin)
        return pdf_to_question_png(pdf, png, subject=subject)

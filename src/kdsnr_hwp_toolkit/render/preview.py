from __future__ import annotations

import os
import re
import subprocess
import tempfile
import lzma
from pathlib import Path
from shutil import copyfile, copyfileobj


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
    exe_name = "rhwp.exe" if os.name == "nt" else "rhwp"
    # parents[3] = kdsnr-hwp-toolkit (이 패키지의 source 와 함께 vendor 된 rhwp).
    # 옛 flap-hwp-parser/vendor/rhwp 는 더 이상 사용하지 않음.
    toolkit_root = Path(__file__).resolve().parents[3]
    source_bin = toolkit_root / "vendor" / "rhwp" / "target" / "release" / exe_name
    if source_bin.exists():
        return source_bin
    package_bin = Path(__file__).resolve().parents[1] / "bin" / exe_name
    if package_bin.exists():
        return package_bin
    package_xz = package_bin.with_name(f"{exe_name}.xz")
    if package_xz.exists():
        return _extract_packaged_rhwp(package_xz, exe_name)
    return source_bin


def _extract_packaged_rhwp(package_xz: Path, exe_name: str) -> Path:
    cache_root = Path(os.environ.get("KDSNR_HWP_TOOLKIT_CACHE", Path.home() / ".cache" / "kdsnr-hwp-toolkit"))
    cache_root.mkdir(parents=True, exist_ok=True)
    stamp = f"{package_xz.stat().st_size}-{int(package_xz.stat().st_mtime)}"
    out_dir = cache_root / stamp
    out_dir.mkdir(parents=True, exist_ok=True)
    target = out_dir / exe_name
    if not target.exists():
        tmp = target.with_suffix(target.suffix + ".tmp")
        with lzma.open(package_xz, "rb") as src, tmp.open("wb") as dst:
            copyfileobj(src, dst)
        tmp.replace(target)
    _ensure_executable(target)
    return target


def _ensure_executable(path: Path) -> None:
    if os.name == "nt" or not path.exists():
        return
    mode = path.stat().st_mode
    if mode & 0o111:
        return
    path.chmod(mode | 0o755)


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
    _ensure_executable(rhwp)
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
    _ensure_executable(rhwp)
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


def _render_pdf_first_page(pdf_path: str | os.PathLike):
    import fitz
    from PIL import Image

    doc = fitz.open(Path(pdf_path))
    page = doc[0]
    pix = page.get_pixmap(matrix=fitz.Matrix(_CROP_DPI / 72, _CROP_DPI / 72), alpha=False)
    return Image.frombytes("RGB", (pix.width, pix.height), pix.samples)


def _render_svg_page(svg_path: str | os.PathLike):
    import fitz
    from PIL import Image

    svg = Path(svg_path)
    doc = fitz.open("svg", svg.read_bytes())
    page = doc[0]
    pix = page.get_pixmap(matrix=fitz.Matrix(_CROP_DPI / 72, _CROP_DPI / 72), alpha=False)
    return Image.frombytes("RGB", (pix.width, pix.height), pix.samples)


def _question_crop_box(img) -> tuple[int, int, int, int]:
    w, h = img.size
    header_y = max(0, _detect_header_divider_y(img) - _CROP_CONTENT_MARGIN_PX)
    left_x = int(w * _CROP_LEFT_X_FRACTION)
    right_x = int(w * _CROP_RIGHT_X_FRACTION)
    body = img.crop((left_x, header_y, right_x, h))

    content_bbox = _content_bbox(body)
    if content_bbox is not None:
        left, top, right, bottom = content_bbox
        return (
            left_x + left - _CROP_CONTENT_MARGIN_PX,
            header_y + top - _CROP_CONTENT_MARGIN_PX,
            left_x + right + _CROP_CONTENT_MARGIN_PX,
            header_y + bottom + _CROP_CONTENT_MARGIN_PX,
        )
    return left_x, header_y, right_x, h


def _crop_image_with_padding(img, box: tuple[int, int, int, int]):
    from PIL import Image

    left, top, right, bottom = box
    width = max(1, right - left)
    height = max(1, bottom - top)
    canvas = Image.new(img.mode, (width, height), "white")
    src_box = (max(0, left), max(0, top), min(img.width, right), min(img.height, bottom))
    if src_box[2] > src_box[0] and src_box[3] > src_box[1]:
        content = img.crop(src_box)
        canvas.paste(content, (src_box[0] - left, src_box[1] - top))
    return canvas


def pdf_to_question_png(
    pdf_path: str | os.PathLike,
    png_path: str | os.PathLike,
    *,
    subject: str | None = None,
    crop: bool = True,
) -> Path:
    """Render first PDF page to PNG, optionally cropped to the question."""

    img = _render_pdf_first_page(pdf_path)
    body = _crop_image_with_padding(img, _question_crop_box(img)) if crop else img

    png = Path(png_path)
    png.parent.mkdir(parents=True, exist_ok=True)
    body.save(png, optimize=True)
    return png


def crop_pdf_to_question_pdf(
    pdf_path: str | os.PathLike,
    cropped_pdf_path: str | os.PathLike,
    *,
    subject: str | None = None,
) -> Path:
    """Create a cropped PDF preview from the first page."""

    img = _render_pdf_first_page(pdf_path)
    body = _crop_image_with_padding(img, _question_crop_box(img)).convert("RGB")
    out = Path(cropped_pdf_path)
    out.parent.mkdir(parents=True, exist_ok=True)
    body.save(out, "PDF", resolution=_CROP_DPI)
    return out


def crop_svg_to_question_svg(
    svg_path: str | os.PathLike,
    cropped_svg_path: str | os.PathLike,
    *,
    subject: str | None = None,
) -> Path:
    """Crop an SVG preview by rewriting its root viewport/viewBox."""

    svg = Path(svg_path)
    text = svg.read_text(encoding="utf-8")
    root_match = re.search(r"<svg\b[^>]*>", text)
    if not root_match:
        raise RuntimeError(f"invalid SVG: {svg}")
    root = root_match.group(0)
    width_match = re.search(r'\bwidth="([0-9.]+)"', root)
    height_match = re.search(r'\bheight="([0-9.]+)"', root)
    if not width_match or not height_match:
        raise RuntimeError(f"SVG root has no width/height: {svg}")
    svg_w = float(width_match.group(1))
    svg_h = float(height_match.group(1))

    img = _render_svg_page(svg)
    box = _question_crop_box(img)

    scale_x = svg_w / img.width
    scale_y = svg_h / img.height
    x = box[0] * scale_x
    y = box[1] * scale_y
    w = max(1.0, (box[2] - box[0]) * scale_x)
    h = max(1.0, (box[3] - box[1]) * scale_y)
    new_root = re.sub(r'\bwidth="[^"]*"', f'width="{w}"', root)
    new_root = re.sub(r'\bheight="[^"]*"', f'height="{h}"', new_root)
    if re.search(r'\bviewBox="[^"]*"', new_root):
        new_root = re.sub(r'\bviewBox="[^"]*"', f'viewBox="{x} {y} {w} {h}"', new_root)
    else:
        new_root = new_root[:-1] + f' viewBox="{x} {y} {w} {h}">'
    out = Path(cropped_svg_path)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(text[: root_match.start()] + new_root + text[root_match.end() :], encoding="utf-8")
    return out


def _content_bbox(img) -> tuple[int, int, int, int] | None:
    """Return the content bbox inside the initial page crop.

    Page footers and column divider strokes can otherwise make a naive bbox
    span the full page. Drop near-full-height vertical rule columns and
    horizontal rules, then keep the full remaining ink bbox. Some math
    questions contain large intentional gaps before diagrams; stopping at the
    first row gap clips those diagrams.
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

    start = int(rows[0])
    end = int(rows[-1]) + 1
    block = mask[start:end, :]
    cols = np.where(block.any(axis=0))[0]
    if len(cols) == 0:
        return None
    return int(cols[0]), start, int(cols[-1]) + 1, end


def render_question_png(
    hwpx_path: str | os.PathLike,
    png_path: str | os.PathLike,
    *,
    rhwp_bin: str | os.PathLike | None = None,
    subject: str | None = None,
    crop: bool = True,
) -> Path:
    png = Path(png_path)
    with tempfile.TemporaryDirectory() as td:
        pdf = Path(td) / "question.pdf"
        render_pdf(hwpx_path, pdf, rhwp_bin=rhwp_bin)
        return pdf_to_question_png(pdf, png, subject=subject, crop=crop)

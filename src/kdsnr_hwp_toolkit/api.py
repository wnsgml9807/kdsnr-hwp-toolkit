from __future__ import annotations

import os
import tempfile
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path
from collections.abc import Iterable
from contextlib import nullcontext

from .adapters import hwp_to_hwpx as _hwp_to_hwpx_bytes
from .compose.splitter import split_paper_to_hwpx_units_with_subject
from .render import (
    crop_pdf_to_question_pdf,
    pdf_to_question_png,
    render_pdf,
)


InputLike = bytes | bytearray | str | os.PathLike
PreviewType = str | Iterable[str] | None
_SUPPORTED_PREVIEW_TYPES = ("png", "pdf")
_LOG_PREFIX = "[KDSNR-HWP-TOOLKIT]"


def _tqdm(iterable=None, **kwargs):
    try:
        from tqdm.auto import tqdm
    except Exception:
        if iterable is None:
            return nullcontext()
        return iterable
    return tqdm(iterable, **kwargs)


def _read_input(input_data: InputLike) -> bytes:
    if isinstance(input_data, (bytes, bytearray)):
        return bytes(input_data)
    return Path(input_data).read_bytes()


def _read_split_input_as_hwpx(input_data: InputLike) -> bytes:
    if isinstance(input_data, (bytes, bytearray)):
        return _hwp_to_hwpx_bytes(bytes(input_data))
    path = Path(input_data)
    if path.suffix.lower() == ".hwp":
        with tempfile.TemporaryDirectory() as td:
            hwpx_path = hwp_to_hwpx(input_hwp_path=path, output_hwpx_dir=td)
            return hwpx_path.read_bytes()
    return _hwp_to_hwpx_bytes(path.read_bytes())


def _normalize_preview_types(preview_type: PreviewType) -> tuple[str, ...]:
    if preview_type is None:
        return ()
    if isinstance(preview_type, str):
        requested = [preview_type]
    else:
        requested = list(preview_type)
    normalized: list[str] = []
    for item in requested:
        if item not in _SUPPORTED_PREVIEW_TYPES:
            raise ValueError(
                "preview_type items must be one of "
                f"{_SUPPORTED_PREVIEW_TYPES}, got {item!r}"
            )
        if item not in normalized:
            normalized.append(item)
    return tuple(normalized)


def hwp_to_hwpx(
    *,
    input_hwp_path: str | os.PathLike,
    output_hwpx_dir: str | os.PathLike,
) -> Path:
    """Convert one HWP/HWPX file to HWPX and return the output path."""

    src = Path(input_hwp_path)
    out_dir = Path(output_hwpx_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    out = out_dir / f"{src.stem}.hwpx"
    hwpx = _hwp_to_hwpx_bytes(src.read_bytes())
    out.write_bytes(hwpx)
    return out


def split_set_to_question(
    input_data: InputLike,
    output_dir: str | os.PathLike,
    *,
    preview_type: PreviewType = None,
    preview_workers: int | None = None,
    crop: bool = True,
) -> list[Path]:
    """Split a supported exam set into per-question HWPX files.

    The subject is detected from the source document. Math, science, and
    social studies are supported. Korean inputs raise
    `ValueError("국어 과목은 아직 지원하지 않습니다")`.

    Writes `<label>.hwpx` files into `output_dir` and returns their paths.
    `preview_type` may be a list containing "png" and/or "pdf"
    (duplicates are ignored). Preview rendering runs in parallel by default;
    pass `preview_workers=1` for deterministic sequential rendering.
    `crop=True` applies the same question crop to every requested preview;
    `crop=False` keeps full-page previews.
    """

    preview_types = _normalize_preview_types(preview_type)
    if preview_workers is None:
        preview_workers = 64
    if preview_workers < 1:
        raise ValueError("preview_workers must be >= 1")

    subject, units = split_paper_to_hwpx_units_with_subject(_read_split_input_as_hwpx(input_data))
    out = Path(output_dir)
    out.mkdir(parents=True, exist_ok=True)
    written: list[Path] = []
    previews: list[Path] = []
    write_iter = _tqdm(units, desc=f"{_LOG_PREFIX} Writing HWPX", unit="question")
    for label, hwpx in write_iter:
        hwpx_path = out / f"{label}.hwpx"
        hwpx_path.write_bytes(hwpx)
        written.append(hwpx_path)
        if preview_types:
            previews.append(hwpx_path)

    if preview_types:
        if preview_workers == 1:
            render_iter = _tqdm(
                previews,
                desc=f"{_LOG_PREFIX} Rendering previews",
                unit="question",
            )
            for hwpx_path in render_iter:
                _render_previews(hwpx_path, preview_types, subject, crop)
        else:
            with ThreadPoolExecutor(max_workers=preview_workers) as executor:
                futures = [
                    executor.submit(_render_previews, hwpx_path, preview_types, subject, crop)
                    for hwpx_path in previews
                ]
                done_iter = as_completed(futures)
                done_iter = _tqdm(
                    done_iter,
                    total=len(futures),
                    desc=f"{_LOG_PREFIX} Rendering previews ({preview_workers} workers)",
                    unit="question",
                )
                for future in done_iter:
                    future.result()
    return written


def _render_previews(
    hwpx_path: Path,
    preview_types: tuple[str, ...],
    subject: str | None = None,
    crop: bool = True,
) -> None:
    """Render requested previews with at most one PDF export per question."""

    pdf_path = hwpx_path.with_suffix(".pdf")
    need_pdf = "pdf" in preview_types or "png" in preview_types
    keep_pdf = "pdf" in preview_types
    if need_pdf:
        with tempfile.TemporaryDirectory() as td:
            source_pdf = Path(td) / f"{hwpx_path.stem}.pdf"
            render_pdf(hwpx_path, source_pdf)
            if "png" in preview_types:
                pdf_to_question_png(
                    source_pdf,
                    hwpx_path.with_suffix(".png"),
                    subject=subject,
                    crop=crop,
                )
            if keep_pdf:
                if crop:
                    crop_pdf_to_question_pdf(source_pdf, pdf_path, subject=subject)
                else:
                    pdf_path.write_bytes(source_pdf.read_bytes())
    else:
        source_pdf = None


__all__ = ["hwp_to_hwpx", "split_set_to_question"]

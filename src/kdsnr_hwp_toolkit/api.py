from __future__ import annotations

import os
import tempfile
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path
from collections.abc import Iterable
from contextlib import nullcontext
from typing import overload

from .adapters import hwp_to_hwpx as _hwp_to_hwpx_bytes
from .compose.splitter import split_paper_to_hwpx_units_with_subject
from .render import pdf_to_question_png, render_pdf, render_svg


InputLike = bytes | bytearray | str | os.PathLike
PreviewType = str | Iterable[str] | None
_SUPPORTED_PREVIEW_TYPES = ("svg", "png", "pdf")


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


@overload
def hwp_to_hwpx(input_data: InputLike, output_path: None = None) -> bytes: ...


@overload
def hwp_to_hwpx(input_data: InputLike, output_path: str | os.PathLike) -> Path: ...


def hwp_to_hwpx(
    input_data: InputLike,
    output_path: str | os.PathLike | None = None,
) -> bytes | Path:
    """Convert HWP/HWPX input to HWPX bytes.

    If `input_data` is already HWPX bytes/path, the HWPX payload is returned
    unchanged. When `output_path` is provided, the converted HWPX is written
    there and the output path is returned.
    """

    hwpx = _hwp_to_hwpx_bytes(_read_input(input_data))
    if output_path is None:
        return hwpx
    out = Path(output_path)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_bytes(hwpx)
    return out


def split_set_to_question(
    input_data: InputLike,
    output_dir: str | os.PathLike,
    *,
    preview_type: PreviewType = None,
    preview_workers: int | None = None,
) -> list[Path]:
    """Split a supported exam set into per-question HWPX files.

    The subject is detected from the source document. Math, science, and
    social studies are supported. Korean inputs raise
    `ValueError("국어 과목은 아직 지원하지 않습니다")`.

    Writes `<label>.hwpx` files into `output_dir` and returns their paths.
    `preview_type` may be a list containing "svg", "png", and/or "pdf"
    (duplicates are ignored). Preview rendering runs in parallel by default;
    pass `preview_workers=1` for deterministic sequential rendering.
    """

    preview_types = _normalize_preview_types(preview_type)
    if preview_workers is None:
        preview_workers = max(1, min(5, os.cpu_count() or 1))
    if preview_workers < 1:
        raise ValueError("preview_workers must be >= 1")

    subject, units = split_paper_to_hwpx_units_with_subject(_read_input(input_data))
    out = Path(output_dir)
    out.mkdir(parents=True, exist_ok=True)
    written: list[Path] = []
    previews: list[Path] = []
    write_iter = _tqdm(units, desc="Writing HWPX", unit="question")
    for label, hwpx in write_iter:
        hwpx_path = out / f"{label}.hwpx"
        hwpx_path.write_bytes(hwpx)
        written.append(hwpx_path)
        if preview_types:
            previews.append(hwpx_path)

    if preview_types:
        if preview_workers == 1:
            render_iter = _tqdm(previews, desc="Rendering previews", unit="question")
            for hwpx_path in render_iter:
                _render_previews(hwpx_path, preview_types, subject)
        else:
            with ThreadPoolExecutor(max_workers=preview_workers) as executor:
                futures = [
                    executor.submit(_render_previews, hwpx_path, preview_types, subject)
                    for hwpx_path in previews
                ]
                done_iter = as_completed(futures)
                done_iter = _tqdm(
                    done_iter,
                    total=len(futures),
                    desc=f"Rendering previews ({preview_workers} workers)",
                    unit="question",
                )
                for future in done_iter:
                    future.result()
    return written


def _render_previews(hwpx_path: Path, preview_types: tuple[str, ...], subject: str | None = None) -> None:
    """Render requested previews with at most one PDF export per question."""

    pdf_path = hwpx_path.with_suffix(".pdf")
    need_pdf = "pdf" in preview_types or "png" in preview_types
    keep_pdf = "pdf" in preview_types
    if need_pdf and keep_pdf:
        render_pdf(hwpx_path, pdf_path)
        source_pdf = pdf_path
    elif need_pdf:
        with tempfile.TemporaryDirectory() as td:
            source_pdf = Path(td) / f"{hwpx_path.stem}.pdf"
            render_pdf(hwpx_path, source_pdf)
            pdf_to_question_png(source_pdf, hwpx_path.with_suffix(".png"), subject=subject)
            source_pdf = None
    else:
        source_pdf = None

    if "png" in preview_types and source_pdf is not None:
        pdf_to_question_png(source_pdf, hwpx_path.with_suffix(".png"), subject=subject)
    if "svg" in preview_types:
        render_svg(hwpx_path, hwpx_path.with_suffix(".svg"))


__all__ = ["hwp_to_hwpx", "split_set_to_question"]

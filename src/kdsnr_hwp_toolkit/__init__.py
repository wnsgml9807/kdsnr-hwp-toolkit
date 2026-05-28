"""KDSNR HWP/HWPX toolkit.

A small, model-centric API over the native engine. Read a document, convert,
split it into questions, render previews, and save — all around one type,
``Document``.

    from kdsnr_hwp_toolkit import import_file, split_set_to_question, export_preview

    doc = import_file("exam.hwpx")           # -> Document (raises on corruption)
    questions = split_set_to_question(doc)   # -> list[Document]
    export_preview(questions, "out/", media_types=["png"])

Tool-corrupted documents (edited by a non-Hancom tool) raise ``ValueError``.
"""

import os as _os
from pathlib import Path as _Path

# Default the managed font directory to the package's bundled ``.fonts/`` folder
# unless the caller already set ``FONT_DIR``. Fonts are collected/copied here at
# runtime (Windows/macOS auto-collect from a Hancom install; on Linux copy the
# font files in manually). The folder ships empty — fonts are not redistributed.
_FONT_DIR = _Path(__file__).resolve().parent / ".fonts"
try:
    _FONT_DIR.mkdir(parents=True, exist_ok=True)
except OSError:
    pass
_os.environ.setdefault("FONT_DIR", str(_FONT_DIR))

from ._native import (
    Document,
    hwp_to_hwpx,
    hwpx_to_hwp,
    import_file,
    is_corrupt,
    save_file,
    split_set_to_question,
)
# export_preview is wrapped Python-side to show the one-time glyph-cache bar.
from .api import export_preview

__all__ = [
    "Document",
    "import_file",
    "save_file",
    "hwp_to_hwpx",
    "hwpx_to_hwp",
    "split_set_to_question",
    "export_preview",
    "is_corrupt",
]

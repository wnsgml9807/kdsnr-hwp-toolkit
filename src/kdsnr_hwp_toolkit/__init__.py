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

from ._native import (
    Document,
    hwp_to_hwpx,
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
    "split_set_to_question",
    "export_preview",
    "is_corrupt",
]

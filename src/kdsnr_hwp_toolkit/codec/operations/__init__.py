"""Codec operations — paragraph manipulation, cross-doc copy."""

from .copy import IdMaps, merge_styles, rewrite_paragraph, copy_paragraphs, rewrite_refs_in_xml
from .paragraphs import insert_paragraphs, delete_paragraphs, replace_section_body

__all__ = [
    "IdMaps", "merge_styles", "rewrite_paragraph", "copy_paragraphs",
    "rewrite_refs_in_xml",
    "insert_paragraphs", "delete_paragraphs", "replace_section_body",
]

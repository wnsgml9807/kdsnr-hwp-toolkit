from __future__ import annotations

from dataclasses import dataclass
from enum import Enum
from typing import Any, Mapping, Sequence


class Role(str, Enum):
    """Semantic role assigned by classification.

    A role says what an atom is. It does not say who owns its style.
    Ownership is handled by TransformPolicy.
    """

    UNKNOWN = "unknown"
    KOREAN_SET = "korean_set"
    QUESTION_NUMBER = "question_number"
    STEM = "stem"
    STEM_CONTINUATION = "stem_continuation"
    DATA_BOX = "data_box"
    BOGI_BOX = "bogi_box"
    INLINE_TABLE = "inline_table"
    CHOICES = "choices"
    EQUATION_BLOCK = "equation_block"
    PICTURE_BLOCK = "picture_block"


@dataclass(frozen=True)
class DocumentBundle:
    """A parsed source document plus metadata needed downstream."""

    document: Any
    subject: str
    source_name: str = ""


@dataclass(frozen=True)
class Atom:
    """A physical unit extracted from the source.

    `payload` intentionally stays opaque here. The codec model can evolve
    without forcing classifier/policy code to know every HWPX detail.
    """

    payload: Any
    source_paragraph_index: int | None = None
    metadata: Mapping[str, Any] | None = None


@dataclass(frozen=True)
class ClassifiedAtom:
    atom: Atom
    role: Role
    confidence: float = 1.0
    reasons: Sequence[str] = ()

from __future__ import annotations

from dataclasses import dataclass
from enum import Enum

from .model import Role


class Ownership(str, Enum):
    TEMPLATE = "template"
    SOURCE = "source"
    MIXED = "mixed"


@dataclass(frozen=True)
class TransformPolicy:
    """Defines which document owns each layer of an atom."""

    role: Role
    shell_owner: Ownership
    content_owner: Ownership
    restyle_outer_paragraph: bool
    preserve_content_paragraph_style: bool
    preserve_content_char_style: bool
    preserve_content_linesegs: bool

    @property
    def source_content_is_sacred(self) -> bool:
        return (
            self.content_owner == Ownership.SOURCE
            and self.preserve_content_paragraph_style
            and self.preserve_content_char_style
            and self.preserve_content_linesegs
        )


_TEMPLATE_ROLE = TransformPolicy(
    role=Role.UNKNOWN,
    shell_owner=Ownership.TEMPLATE,
    content_owner=Ownership.TEMPLATE,
    restyle_outer_paragraph=True,
    preserve_content_paragraph_style=False,
    preserve_content_char_style=False,
    preserve_content_linesegs=False,
)


_BOX_CONTENT_SOURCE = TransformPolicy(
    role=Role.UNKNOWN,
    shell_owner=Ownership.TEMPLATE,
    content_owner=Ownership.SOURCE,
    restyle_outer_paragraph=False,
    preserve_content_paragraph_style=True,
    preserve_content_char_style=True,
    preserve_content_linesegs=True,
)


def policy_for_role(role: Role) -> TransformPolicy:
    """Return the non-negotiable transform policy for a semantic role."""

    if role in {Role.BOGI_BOX, Role.DATA_BOX}:
        return TransformPolicy(role=role, **{
            k: v for k, v in _BOX_CONTENT_SOURCE.__dict__.items()
            if k != "role"
        })

    if role in {
        Role.KOREAN_SET,
        Role.QUESTION_NUMBER,
        Role.STEM,
        Role.STEM_CONTINUATION,
        Role.INLINE_TABLE,
        Role.CHOICES,
        Role.EQUATION_BLOCK,
        Role.PICTURE_BLOCK,
        Role.UNKNOWN,
    }:
        return TransformPolicy(role=role, **{
            k: v for k, v in _TEMPLATE_ROLE.__dict__.items()
            if k != "role"
        })

    raise ValueError(f"Unhandled role: {role!r}")

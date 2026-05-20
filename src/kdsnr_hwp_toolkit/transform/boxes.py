from __future__ import annotations

from dataclasses import dataclass, replace
from typing import Any, Callable, Sequence

from kdsnr_hwp_toolkit.core.model import Role
from kdsnr_hwp_toolkit.core.policy import policy_for_role


@dataclass(frozen=True)
class BoxShellTemplate:
    """Template-owned shell for a source-owned box content payload.

    A shell owns border, label, outer geometry, row/column layout, and cell
    margins. It does not own paragraphs inserted into its content slot.
    """

    role: Role
    name: str
    build: Callable[[Sequence[Any]], Any]


def preserve_source_content_paragraphs(content_paragraphs: Sequence[Any]) -> tuple[Any, ...]:
    """Return source content paragraphs without style/layout mutation.

    This small function is intentionally boring. It is the guardrail: if a
    future change wants to alter charPr, paraPr, tab structure, picture anchor,
    equation order, or lineSeg, it should not happen in the shell wrapper.
    """

    return tuple(content_paragraphs)


def build_box_with_source_content(
    shell: BoxShellTemplate,
    content_paragraphs: Sequence[Any],
) -> Any:
    """Build a template shell while preserving source-owned content."""

    policy = policy_for_role(shell.role)
    if not policy.source_content_is_sacred:
        raise AssertionError(f"{shell.role} does not preserve source content")
    return shell.build(preserve_source_content_paragraphs(content_paragraphs))


def clone_cell_with_source_paragraphs(cell: Any, paragraphs: Sequence[Any]) -> Any:
    """Codec-agnostic helper used by concrete HWPX shell builders."""

    return replace(cell, paragraphs=tuple(paragraphs))

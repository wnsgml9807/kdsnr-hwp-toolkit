from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class RenderContract:
    """Declares that HWPX output is the rendering source of truth."""

    hwpx_is_canonical: bool = True
    fix_renderer_instead_of_mutating_content: bool = True

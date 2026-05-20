from __future__ import annotations

from collections.abc import Iterable
from typing import Any

from kdsnr_hwp_toolkit.core.model import Atom


def extract_atoms(paragraphs: Iterable[Any]) -> list[Atom]:
    """Create extraction atoms without mutation.

    Boundary detection and fused-paragraph splitting will live here. The key
    invariant is that extracted payloads are source objects or exact clones,
    never restyled template objects.
    """

    return [
        Atom(payload=p, source_paragraph_index=i)
        for i, p in enumerate(paragraphs)
    ]

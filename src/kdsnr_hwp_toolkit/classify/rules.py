from __future__ import annotations

from kdsnr_hwp_toolkit.core.model import Atom, ClassifiedAtom, Role


def classify_atom(atom: Atom) -> ClassifiedAtom:
    """Classify an atom semantically.

    This placeholder deliberately returns UNKNOWN until the legacy classifier
    is ported into this module. The important design point is that this layer
    has no dependency on templates or rendering.
    """

    return ClassifiedAtom(atom=atom, role=Role.UNKNOWN, confidence=0.0)

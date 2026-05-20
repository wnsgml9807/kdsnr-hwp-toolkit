"""KDSNR HWP Toolkit.

This package is organized around explicit ownership boundaries:
template-owned role styling, template-owned box shells, and source-owned box
content.
"""

from .core.model import Atom, ClassifiedAtom, DocumentBundle, Role
from .core.policy import Ownership, TransformPolicy, policy_for_role
from .api import hwp_to_hwpx, split_set_to_question

__all__ = [
    "Atom",
    "ClassifiedAtom",
    "DocumentBundle",
    "Ownership",
    "Role",
    "TransformPolicy",
    "policy_for_role",
    "hwp_to_hwpx",
    "split_set_to_question",
]

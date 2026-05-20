from __future__ import annotations

from typing import Any, Protocol

from kdsnr_hwp_toolkit.core.model import ClassifiedAtom
from kdsnr_hwp_toolkit.core.policy import Ownership, policy_for_role


class RoleStyleApplier(Protocol):
    def __call__(self, payload: Any, role_name: str) -> Any:
        ...


def apply_template_role_style(
    classified: ClassifiedAtom,
    apply_style: RoleStyleApplier,
) -> Any:
    """Apply unified role style to non-box atoms.

    Box atoms must go through `transform.boxes`; this keeps source-owned box
    content from accidentally passing through role restyling.
    """

    policy = policy_for_role(classified.role)
    if policy.content_owner == Ownership.SOURCE:
        raise AssertionError(
            f"{classified.role.value} contains source-owned content; "
            "use a box shell transform instead"
        )
    return apply_style(classified.atom.payload, classified.role.value)

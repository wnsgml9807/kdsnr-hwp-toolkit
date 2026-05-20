from __future__ import annotations

from collections.abc import Sequence
from typing import Any


def compose_body(template_document: Any, transformed_payloads: Sequence[Any]) -> Any:
    """Compose transformed payloads into the unified template.

    Concrete HWPX splice logic will be implemented after the box/role
    transform contracts are stable. This function exists so orchestration has
    one place where template-body replacement happens.
    """

    raise NotImplementedError("HWPX body composition is not ported yet")

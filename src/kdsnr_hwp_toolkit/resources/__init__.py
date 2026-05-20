from __future__ import annotations

from importlib import resources


def template_bytes(name: str) -> bytes:
    return resources.files(__package__).joinpath("templates", name).read_bytes()


def template_text(name: str) -> str:
    return resources.files(__package__).joinpath("templates", name).read_text(
        encoding="utf-8"
    )

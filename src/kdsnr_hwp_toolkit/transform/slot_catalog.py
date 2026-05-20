"""Slot catalog — derives atom→(pp,st,cs) mapping from each subject's templet.

Single source of truth: templet/{subject}.hwpx. We walk paragraphs, classify
each, and pick the most-common slot per atom (single-unification rule
confirmed with user 2026-05-11). Box atoms also store the box paragraph
itself for clone-and-swap transforms.
"""
from __future__ import annotations

from collections import Counter, defaultdict
from dataclasses import dataclass
from functools import lru_cache

from kdsnr_hwp_toolkit.codec import read
from kdsnr_hwp_toolkit.codec.schema import (
    CharItem, HwpxDocument, Paragraph, TableItem,
)
from kdsnr_hwp_toolkit.resources import template_bytes
from kdsnr_hwp_toolkit.classify.atoms import Atom, BOX_ATOMS
from kdsnr_hwp_toolkit.classify.classifier import classify


SubjectName = str  # "math" | "science" | "social" | "korean"


@dataclass(frozen=True)
class Slot:
    """A templet slot — paragraph properties + optional reference paragraph.

    body_cs_id: for balmun (and any other atom whose templet paragraph carries
    two distinct char_shapes — first char = number/marker style, rest = body
    style). When set, transform applies cs_id only to the leading "N." run
    and body_cs_id to everything after. None = single cs across the paragraph.
    """

    pp_id: int
    st_id: int
    cs_id: int
    template_paragraph: Paragraph | None = None  # for box atoms (clone source)
    body_cs_id: int | None = None


@dataclass(frozen=True)
class SubjectCatalog:
    subject: SubjectName
    template: HwpxDocument
    atom_to_slot: dict  # Atom → Slot


def _first_table(p: Paragraph) -> TableItem | None:
    for it in p.items:
        if isinstance(it, TableItem):
            return it
    return None


def _is_1x1(tbl: TableItem) -> bool:
    return (
        tbl.table_attrs.get("rowCnt") == "1"
        and tbl.table_attrs.get("colCnt") == "1"
    )


def _has_bogi_marker(tbl: TableItem) -> bool:
    import re as _re
    for cell in tbl.cells:
        text = "".join(
            it.text
            for cp in cell.paragraphs
            for it in cp.items
            if isinstance(it, CharItem)
        )
        if "보기" in _re.sub(r"\s+", "", text):
            return True
    return False


def _prefer_wrapper(atom: Atom, candidates: list[Paragraph]) -> Paragraph:
    """Pick canonical wrapper paragraph for box atoms.

    DATA_BOX / JIMUN_DATA_BOX → 1×1 shell preferred (avoid leaking inline
    table labels like math's '합계' from confusion-matrix wrappers).
    BOGI_BOX → paragraph whose table carries '보기' marker.
    Others → first match.
    """
    if atom in (Atom.DATA_BOX, Atom.JIMUN_DATA_BOX):
        for p in candidates:
            tbl = _first_table(p)
            if tbl is not None and _is_1x1(tbl):
                return p
    if atom == Atom.BOGI_BOX:
        for p in candidates:
            tbl = _first_table(p)
            if tbl is not None and _has_bogi_marker(tbl):
                return p
    return candidates[0]


def _detect_body_cs(p: Paragraph, first_cs: int) -> int | None:
    """Find the first char_shape that differs from first_cs in this paragraph.

    Returns None if every item shares first_cs (= single-cs paragraph).
    Used to detect templet's "marker char + body char" pattern (balmun's
    "N." in number style, body in body style).
    """
    for it in p.items:
        cs = getattr(it, "char_shape_id", None)
        if cs is not None and cs != first_cs:
            return cs
    return None


def _classify_template(template: HwpxDocument, subject: str):
    """Walk template paragraphs, returning [(idx, atom, paragraph), ...]."""
    body = template.sections[0].body
    out = []
    prev = None
    for i, p in enumerate(body):
        atom = classify(p, prev_atom=prev, subject=subject)
        out.append((i, atom, p))
        if atom not in (Atom.EMPTY, Atom.UNKNOWN):
            prev = atom
    return out


def _build_catalog(subject: SubjectName) -> SubjectCatalog:
    raw = template_bytes(f"{subject}.hwpx")
    template = read(raw, doc_id=f"template:{subject}")
    classified = _classify_template(template, subject)

    # Group paragraphs by atom
    by_atom: dict[Atom, list[Paragraph]] = defaultdict(list)
    for _, atom, p in classified:
        if atom in (Atom.EMPTY, Atom.UNKNOWN):
            continue
        by_atom[atom].append(p)

    # For each atom, pick the most common (pp, st, cs) — user confirmed
    # "차이 미미하면 더 많이 쓰인 슬롯으로 단일 통일"
    atom_to_slot: dict[Atom, Slot] = {}
    for atom, paras in by_atom.items():
        triple_counter: Counter = Counter(
            (p.para_shape_id, p.style_id, p.char_shape_id_first) for p in paras
        )
        chosen_triple, _ = triple_counter.most_common(1)[0]

        # Special case for ⑪ (social data_box widths): pick FIRST encountered
        # (user explicit: "첫번째로 통일").
        if atom == Atom.DATA_BOX and subject == "social":
            chosen_triple = (
                paras[0].para_shape_id,
                paras[0].style_id,
                paras[0].char_shape_id_first,
            )

        # BOGI_BOX: prefer slots whose representative is a CLEAN wrapper
        # (table only, no fused balmun-cont text). Otherwise the cloned
        # paragraph carries fused leading text into output → "발문 두 번
        # 반복" artifact.
        if atom == Atom.BOGI_BOX:
            clean_paras = [
                p for p in paras
                if not any(
                    isinstance(it, CharItem) and it.text.strip()
                    for it in p.items
                )
            ]
            if clean_paras:
                clean_triples = Counter(
                    (p.para_shape_id, p.style_id, p.char_shape_id_first)
                    for p in clean_paras
                )
                chosen_triple, _ = clean_triples.most_common(1)[0]

        # Pick a representative paragraph that uses the chosen triple.
        # For box atoms prefer the canonical wrapper shape (1×1 / bogi-marked)
        # — otherwise the first inline-table paragraph would be picked and
        # _box_clone would leak its labels into output.
        candidates = [
            p for p in paras
            if (p.para_shape_id, p.style_id, p.char_shape_id_first) == chosen_triple
        ]
        rep_p = _prefer_wrapper(atom, candidates)

        # Detect body_cs (different char_shape after the first char run).
        # Used by balmun to keep "N." in number style and body in body style.
        body_cs = _detect_body_cs(rep_p, chosen_triple[2])

        # Always keep the representative paragraph — needed not just for the
        # BOX_ATOMS clone path but also for data_box (cell paragraph paraPr
        # normalization) and any future per-atom inspection.
        atom_to_slot[atom] = Slot(
            pp_id=chosen_triple[0],
            st_id=chosen_triple[1],
            cs_id=chosen_triple[2],
            template_paragraph=rep_p,
            body_cs_id=body_cs,
        )

    return SubjectCatalog(subject=subject, template=template, atom_to_slot=atom_to_slot)


# Atoms that any subject may legitimately need even if its templet doesn't
# carry an example — borrowed from `math` (the most complete catalog) when
# missing. PIC_BLOCK is the canonical case: social Q13 has a centered
# picture but social.hwpx templet doesn't include a picture-only paragraph.
_FALLBACK_ATOMS = (Atom.PIC_BLOCK, Atom.EQ_BLOCK, Atom.SEONJI_2ROW, Atom.BALMUN_CONT)


@lru_cache(maxsize=8)
def catalog_for(subject: SubjectName) -> SubjectCatalog:
    """Cached catalog. Templet rarely changes during a run.

    After building, fill in cross-subject fallback slots so atoms classified
    in src never silently drop. Subjects whose templet has no example of a
    given atom (e.g. social has no picture-only paragraph) inherit the slot
    from the math catalog, which carries every atom.
    """
    cat = _build_catalog(subject)
    if subject == "math":
        return cat
    math_cat = _build_catalog("math")
    new_slots = dict(cat.atom_to_slot)
    for atom in _FALLBACK_ATOMS:
        if atom not in new_slots and atom in math_cat.atom_to_slot:
            new_slots[atom] = math_cat.atom_to_slot[atom]
    if len(new_slots) == len(cat.atom_to_slot):
        return cat
    from dataclasses import replace as _replace
    return _replace(cat, atom_to_slot=new_slots)


__all__ = ["Slot", "SubjectCatalog", "catalog_for"]

#!/usr/bin/env python3
"""Compare glyph relative positions between a Hancom GT PDF and our PDF.

The comparison intentionally ignores absolute page placement.  For each side
it extracts text glyph bounding boxes, computes the union glyph bbox, and
compares every matched glyph in that local coordinate system:

    rel_x = glyph_x - origin_x
    rel_y = glyph_y - origin_y

This answers "which glyph moved inside the question layout?" rather than
"did the whole question move on the page?".

By default the origin is derived from the matched glyphs themselves: we subtract
the median candidate-vs-GT translation, so a whole-question page shift is
removed even if one PDF exposes extra text-layer glyphs.

Limitations:
- PDF paths without a text layer cannot be assigned a character label.
- Matching is sequence based.  If one renderer emits extra text-layer glyphs,
  non-equal spans are reported as unmatched.
"""

from __future__ import annotations

import argparse
import csv
import difflib
import json
import math
from dataclasses import dataclass, asdict
from pathlib import Path
from typing import Iterable

import fitz  # PyMuPDF


@dataclass(frozen=True)
class Glyph:
    side: str
    page: int
    block: int
    line: int
    span: int
    index: int
    char: str
    font: str
    size: float
    x0: float
    y0: float
    x1: float
    y1: float

    @property
    def cx(self) -> float:
        return (self.x0 + self.x1) / 2.0

    @property
    def cy(self) -> float:
        return (self.y0 + self.y1) / 2.0


def _char_label(ch: str) -> str:
    cp = ord(ch)
    if ch == " ":
        return "SPACE"
    if ch == "\t":
        return "TAB"
    if cp < 0x20 or cp == 0x7F:
        return f"U+{cp:04X}"
    if 0xE000 <= cp <= 0xF8FF:
        return f"PUA U+{cp:04X}"
    return ch


def _include_char(ch: str, mode: str) -> bool:
    if mode == "all":
        return True
    if mode == "nonspace":
        return not ch.isspace()
    if mode == "pua":
        return 0xE000 <= ord(ch) <= 0xF8FF
    raise ValueError(f"unknown filter mode: {mode}")


def _parse_bbox(value: str | None) -> tuple[float, float, float, float] | None:
    if value is None:
        return None
    parts = [p.strip() for p in value.split(",")]
    if len(parts) != 4:
        raise ValueError("bbox must be x0,y0,x1,y1")
    x0, y0, x1, y1 = map(float, parts)
    return (x0, y0, x1, y1)


def _inside_bbox(cx: float, cy: float, bbox: tuple[float, float, float, float] | None) -> bool:
    if bbox is None:
        return True
    x0, y0, x1, y1 = bbox
    return x0 <= cx <= x1 and y0 <= cy <= y1


def extract_glyphs(
    pdf: Path,
    side: str,
    *,
    page_index: int = 0,
    char_filter: str = "nonspace",
    bbox: tuple[float, float, float, float] | None = None,
    max_page_y_ratio: float | None = None,
) -> list[Glyph]:
    doc = fitz.open(pdf)
    page = doc[page_index]
    max_page_y = page.rect.height * max_page_y_ratio if max_page_y_ratio is not None else None
    raw = page.get_text("rawdict")
    glyphs: list[Glyph] = []
    idx = 0
    for bi, block in enumerate(raw.get("blocks", [])):
        for li, line in enumerate(block.get("lines", [])):
            for si, span in enumerate(line.get("spans", [])):
                font = span.get("font", "")
                size = float(span.get("size", 0.0))
                for ch_info in span.get("chars", []):
                    ch = ch_info.get("c", "")
                    if not ch or not _include_char(ch, char_filter):
                        continue
                    x0, y0, x1, y1 = map(float, ch_info["bbox"])
                    cx = (x0 + x1) / 2.0
                    cy = (y0 + y1) / 2.0
                    if max_page_y is not None and cy > max_page_y:
                        continue
                    if not _inside_bbox(cx, cy, bbox):
                        continue
                    glyphs.append(
                        Glyph(
                            side=side,
                            page=page_index,
                            block=bi,
                            line=li,
                            span=si,
                            index=idx,
                            char=ch,
                            font=font,
                            size=size,
                            x0=x0,
                            y0=y0,
                            x1=x1,
                            y1=y1,
                        )
                    )
                    idx += 1
    return glyphs


def glyph_union(glyphs: Iterable[Glyph]) -> tuple[float, float, float, float]:
    gs = list(glyphs)
    if not gs:
        return (0.0, 0.0, 0.0, 0.0)
    return (
        min(g.x0 for g in gs),
        min(g.y0 for g in gs),
        max(g.x1 for g in gs),
        max(g.y1 for g in gs),
    )


def _match_by_sequence(gt: list[Glyph], cand: list[Glyph]) -> tuple[list[tuple[int, int]], list[int], list[int]]:
    gt_seq = [g.char for g in gt]
    cand_seq = [g.char for g in cand]
    matcher = difflib.SequenceMatcher(a=gt_seq, b=cand_seq, autojunk=False)
    pairs: list[tuple[int, int]] = []
    gt_unmatched: list[int] = []
    cand_unmatched: list[int] = []
    for tag, i1, i2, j1, j2 in matcher.get_opcodes():
        if tag == "equal":
            pairs.extend(zip(range(i1, i2), range(j1, j2)))
        else:
            gt_unmatched.extend(range(i1, i2))
            cand_unmatched.extend(range(j1, j2))
    return pairs, gt_unmatched, cand_unmatched


def _rms(values: list[float]) -> float:
    if not values:
        return 0.0
    return math.sqrt(sum(v * v for v in values) / len(values))


def _median(values: list[float]) -> float:
    if not values:
        return 0.0
    xs = sorted(values)
    mid = len(xs) // 2
    if len(xs) % 2:
        return xs[mid]
    return (xs[mid - 1] + xs[mid]) / 2.0


def write_overlay(
    out_path: Path,
    rows: list[dict],
    gt_box: tuple[float, float, float, float],
    cand_box: tuple[float, float, float, float],
    *,
    top_n: int,
) -> None:
    try:
        from PIL import Image, ImageDraw, ImageFont
    except Exception:
        return

    w = max(gt_box[2] - gt_box[0], cand_box[2] - cand_box[0], 1.0)
    h = max(gt_box[3] - gt_box[1], cand_box[3] - cand_box[1], 1.0)
    margin = 36
    scale = min(3.0, 900.0 / w, 900.0 / h)
    canvas_w = int(w * scale + margin * 2)
    canvas_h = int(h * scale + margin * 2 + 34)
    img = Image.new("RGB", (canvas_w, canvas_h), "white")
    draw = ImageDraw.Draw(img)
    font = ImageFont.load_default()

    def xy(x: float, y: float) -> tuple[float, float]:
        return (margin + x * scale, margin + y * scale)

    draw.text((margin, 8), "blue=GT  red=candidate  gray=line shows movement", fill=(40, 40, 40), font=font)
    rows_sorted = sorted(rows, key=lambda r: float(r["distance"]), reverse=True)
    top_keys = {(r["rank"], r["codepoint"]) for r in rows_sorted[:top_n]}

    for row in rows:
        gt_x = float(row["gt_rel_cx"])
        gt_y = float(row["gt_rel_cy"])
        ca_x = float(row["candidate_rel_cx"])
        ca_y = float(row["candidate_rel_cy"])
        gx, gy = xy(gt_x, gt_y)
        cx, cy = xy(ca_x, ca_y)
        important = (row["rank"], row["codepoint"]) in top_keys
        draw.line((gx, gy, cx, cy), fill=(180, 180, 180), width=2 if important else 1)
        r = 4 if important else 2
        draw.ellipse((gx - r, gy - r, gx + r, gy + r), outline=(0, 80, 220), width=2, fill=(210, 228, 255))
        draw.rectangle((cx - r, cy - r, cx + r, cy + r), outline=(220, 40, 40), width=2, fill=(255, 220, 220))
        if important:
            draw.text((cx + 5, cy - 5), row["char"], fill=(150, 0, 0), font=font)

    img.save(out_path)


def write_report(
    gt_pdf: Path,
    cand_pdf: Path,
    out_dir: Path,
    *,
    page_index: int,
    char_filter: str,
    gt_bbox: tuple[float, float, float, float] | None,
    candidate_bbox: tuple[float, float, float, float] | None,
    max_page_y_ratio: float | None,
    alignment: str,
    top_n: int,
) -> dict:
    out_dir.mkdir(parents=True, exist_ok=True)
    gt = extract_glyphs(
        gt_pdf,
        "gt",
        page_index=page_index,
        char_filter=char_filter,
        bbox=gt_bbox,
        max_page_y_ratio=max_page_y_ratio,
    )
    cand = extract_glyphs(
        cand_pdf,
        "candidate",
        page_index=page_index,
        char_filter=char_filter,
        bbox=candidate_bbox,
        max_page_y_ratio=max_page_y_ratio,
    )
    pairs, gt_unmatched, cand_unmatched = _match_by_sequence(gt, cand)

    gt_box = glyph_union(gt)
    cand_box = glyph_union(cand)

    if alignment == "bbox":
        gt_ox, gt_oy = gt_box[0], gt_box[1]
        cand_ox, cand_oy = cand_box[0], cand_box[1]
        removed_translation = [round(cand_ox - gt_ox, 4), round(cand_oy - gt_oy, 4)]
    elif alignment == "matched-median":
        raw_dx = [cand[ci].cx - gt[gi].cx for gi, ci in pairs]
        raw_dy = [cand[ci].cy - gt[gi].cy for gi, ci in pairs]
        tx = _median(raw_dx)
        ty = _median(raw_dy)
        gt_ox, gt_oy = 0.0, 0.0
        cand_ox, cand_oy = tx, ty
        removed_translation = [round(tx, 4), round(ty, 4)]
    elif alignment == "matched-first":
        if pairs:
            gi0, ci0 = pairs[0]
            tx = cand[ci0].cx - gt[gi0].cx
            ty = cand[ci0].cy - gt[gi0].cy
        else:
            tx = ty = 0.0
        gt_ox, gt_oy = 0.0, 0.0
        cand_ox, cand_oy = tx, ty
        removed_translation = [round(tx, 4), round(ty, 4)]
    else:
        raise ValueError(f"unknown alignment mode: {alignment}")

    rows: list[dict] = []
    dxs: list[float] = []
    dys: list[float] = []
    dists: list[float] = []
    for rank, (gi, ci) in enumerate(pairs):
        a = gt[gi]
        b = cand[ci]
        gt_rx = a.cx - gt_ox
        gt_ry = a.cy - gt_oy
        cand_rx = b.cx - cand_ox
        cand_ry = b.cy - cand_oy
        dx = cand_rx - gt_rx
        dy = cand_ry - gt_ry
        dist = math.hypot(dx, dy)
        dxs.append(dx)
        dys.append(dy)
        dists.append(dist)
        rows.append(
            {
                "rank": rank,
                "char": _char_label(a.char),
                "codepoint": f"U+{ord(a.char):04X}",
                "gt_layout": f"b{a.block}:l{a.line}:s{a.span}:g{a.index}",
                "candidate_layout": f"b{b.block}:l{b.line}:s{b.span}:g{b.index}",
                "gt_font": a.font,
                "candidate_font": b.font,
                "gt_rel_cx": round(gt_rx, 4),
                "gt_rel_cy": round(gt_ry, 4),
                "candidate_rel_cx": round(cand_rx, 4),
                "candidate_rel_cy": round(cand_ry, 4),
                "dx": round(dx, 4),
                "dy": round(dy, 4),
                "distance": round(dist, 4),
            }
        )

    rows_sorted = sorted(rows, key=lambda r: float(r["distance"]), reverse=True)

    csv_path = out_dir / "glyph_relative_diff.csv"
    fieldnames = [
        "rank",
        "char",
        "codepoint",
        "gt_layout",
        "candidate_layout",
        "gt_font",
        "candidate_font",
        "gt_rel_cx",
        "gt_rel_cy",
        "candidate_rel_cx",
        "candidate_rel_cy",
        "dx",
        "dy",
        "distance",
    ]
    with csv_path.open("w", newline="", encoding="utf-8") as f:
        writer = csv.DictWriter(f, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(rows_sorted)

    unmatched_path = out_dir / "unmatched.json"
    unmatched = {
        "gt": [asdict(gt[i]) | {"label": _char_label(gt[i].char)} for i in gt_unmatched],
        "candidate": [asdict(cand[i]) | {"label": _char_label(cand[i].char)} for i in cand_unmatched],
    }
    unmatched_path.write_text(json.dumps(unmatched, ensure_ascii=False, indent=2), encoding="utf-8")

    overlay_path = out_dir / "relative_overlay.png"
    write_overlay(overlay_path, rows, gt_box, cand_box, top_n=top_n)

    summary = {
        "gt_pdf": str(gt_pdf),
        "candidate_pdf": str(cand_pdf),
        "page": page_index,
        "filter": char_filter,
        "alignment": alignment,
        "removed_translation": removed_translation,
        "gt_input_bbox": [round(v, 4) for v in gt_bbox] if gt_bbox else None,
        "candidate_input_bbox": [round(v, 4) for v in candidate_bbox] if candidate_bbox else None,
        "max_page_y_ratio": max_page_y_ratio,
        "gt_glyph_count": len(gt),
        "candidate_glyph_count": len(cand),
        "matched_count": len(pairs),
        "gt_unmatched_count": len(gt_unmatched),
        "candidate_unmatched_count": len(cand_unmatched),
        "gt_bbox": [round(v, 4) for v in gt_box],
        "candidate_bbox": [round(v, 4) for v in cand_box],
        "mean_abs_dx": round(sum(abs(v) for v in dxs) / len(dxs), 4) if dxs else 0.0,
        "mean_abs_dy": round(sum(abs(v) for v in dys) / len(dys), 4) if dys else 0.0,
        "rms_distance": round(_rms(dists), 4),
        "max_distance": round(max(dists), 4) if dists else 0.0,
        "top_moved": rows_sorted[:top_n],
        "outputs": {
            "csv": str(csv_path),
            "unmatched": str(unmatched_path),
            "overlay": str(overlay_path) if overlay_path.exists() else None,
        },
    }
    summary_path = out_dir / "summary.json"
    summary_path.write_text(json.dumps(summary, ensure_ascii=False, indent=2), encoding="utf-8")
    summary["outputs"]["summary"] = str(summary_path)
    return summary


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--gt", required=True, type=Path, help="Hancom GT PDF")
    parser.add_argument("--candidate", required=True, type=Path, help="Candidate PDF rendered by our pipeline")
    parser.add_argument("--out-dir", required=True, type=Path, help="Directory for CSV/JSON report")
    parser.add_argument("--page", type=int, default=0, help="0-based page index")
    parser.add_argument(
        "--filter",
        choices=["all", "nonspace", "pua"],
        default="nonspace",
        help="Glyph subset to compare",
    )
    parser.add_argument("--gt-bbox", help="GT crop bbox in PDF points: x0,y0,x1,y1")
    parser.add_argument("--candidate-bbox", help="Candidate crop bbox in PDF points: x0,y0,x1,y1")
    parser.add_argument(
        "--max-page-y-ratio",
        type=float,
        help="Drop glyphs whose center y is below this page-height ratio, useful for footer/page numbers",
    )
    parser.add_argument(
        "--alignment",
        choices=["matched-median", "matched-first", "bbox"],
        default="matched-median",
        help="How to remove whole-question translation before reporting relative drift",
    )
    parser.add_argument("--top", type=int, default=20, help="Number of largest moves to print")
    args = parser.parse_args()

    summary = write_report(
        args.gt,
        args.candidate,
        args.out_dir,
        page_index=args.page,
        char_filter=args.filter,
        gt_bbox=_parse_bbox(args.gt_bbox),
        candidate_bbox=_parse_bbox(args.candidate_bbox),
        max_page_y_ratio=args.max_page_y_ratio,
        alignment=args.alignment,
        top_n=args.top,
    )
    print(json.dumps(summary, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

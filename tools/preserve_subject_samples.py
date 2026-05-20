#!/usr/bin/env python3
"""Run the supported subject pipeline and preserve the exact input samples.

Outputs:
  work/preserved_subject_inputs/
    manifest.json
    math/input/...
    math/split/*.hwpx
    math/preview/*.{pdf,png,svg}
    science/...
    social/...
    korean_unsupported/input/...
"""
from __future__ import annotations

import argparse
import json
import shutil
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SRC = ROOT / "src"
if str(SRC) not in sys.path:
    sys.path.insert(0, str(SRC))

from kdsnr_hwp_toolkit.api import split_set_to_question  # noqa: E402
from kdsnr_hwp_toolkit.render.preview import (  # noqa: E402
    pdf_to_question_png,
    render_pdf,
    render_svg,
)


SUPPORTED_SAMPLES = {
    "math": {
        "input": ROOT / "templet/original/math_input_sample_2.hwp",
        "gt_pdf": ROOT / "templet/original/math_input_sample_2.pdf",
    },
    "science": {
        "input": ROOT / "templet/original/science_input_example_2.hwp",
        "gt_pdf": ROOT / "templet/original/science_input_example_2.pdf",
    },
    "social": {
        "input": ROOT / "templet/original/social_test_input_2.hwp",
        "gt_pdf": None,
    },
}

UNSUPPORTED_SAMPLES = {
    "korean": {
        "input": ROOT / "templet/original/국어_박스, 밑줄, 묶음.hwpx",
        "gt_pdf": ROOT / "templet/original/국어_박스, 밑줄, 묶음.pdf",
    },
}


def _copy_sample(src: Path, dst_dir: Path) -> Path:
    dst_dir.mkdir(parents=True, exist_ok=True)
    dst = dst_dir / src.name
    shutil.copy2(src, dst)
    return dst


def _render_previews(hwpx_paths: list[Path], out_dir: Path, subject: str, limit: int) -> list[dict]:
    out_dir.mkdir(parents=True, exist_ok=True)
    previews = []
    for hwpx_path in hwpx_paths[:limit]:
        stem = hwpx_path.stem
        pdf = out_dir / f"{stem}.pdf"
        png = out_dir / f"{stem}.png"
        svg = out_dir / f"{stem}.svg"
        render_pdf(hwpx_path, pdf)
        pdf_to_question_png(pdf, png, subject=subject)
        render_svg(hwpx_path, svg)
        previews.append({
            "label": stem,
            "pdf": str(pdf.relative_to(ROOT)),
            "png": str(png.relative_to(ROOT)),
            "svg": str(svg.relative_to(ROOT)),
        })
    return previews


def preserve(out_dir: Path, preview_count: int) -> dict:
    if not out_dir.is_absolute():
        out_dir = ROOT / out_dir
    out_dir = out_dir.resolve()
    out_dir.mkdir(parents=True, exist_ok=True)
    manifest: dict = {
        "description": "Supported subject pipeline run with preserved input samples.",
        "output_dir": str(out_dir.relative_to(ROOT)),
        "supported_subjects": {},
        "unsupported_subjects": {},
    }

    for subject, sample in SUPPORTED_SAMPLES.items():
        subject_dir = out_dir / subject
        preserved_input = _copy_sample(sample["input"], subject_dir / "input")
        preserved_gt = None
        if sample.get("gt_pdf") and sample["gt_pdf"].exists():
            preserved_gt = _copy_sample(sample["gt_pdf"], subject_dir / "input")

        split_dir = subject_dir / "split"
        hwpx_paths = split_set_to_question(preserved_input, split_dir)
        previews = _render_previews(hwpx_paths, subject_dir / "preview", subject, preview_count)

        manifest["supported_subjects"][subject] = {
            "input": str(preserved_input.relative_to(ROOT)),
            "gt_pdf": str(preserved_gt.relative_to(ROOT)) if preserved_gt else None,
            "split_dir": str(split_dir.relative_to(ROOT)),
            "question_count": len(hwpx_paths),
            "preview_count": len(previews),
            "previews": previews,
        }

    for subject, sample in UNSUPPORTED_SAMPLES.items():
        subject_dir = out_dir / f"{subject}_unsupported"
        preserved_input = _copy_sample(sample["input"], subject_dir / "input")
        preserved_gt = None
        if sample.get("gt_pdf") and sample["gt_pdf"].exists():
            preserved_gt = _copy_sample(sample["gt_pdf"], subject_dir / "input")
        manifest["unsupported_subjects"][subject] = {
            "input": str(preserved_input.relative_to(ROOT)),
            "gt_pdf": str(preserved_gt.relative_to(ROOT)) if preserved_gt else None,
            "expected_error": "국어 과목은 아직 지원하지 않습니다",
        }

    manifest_path = out_dir / "manifest.json"
    manifest_path.write_text(
        json.dumps(manifest, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
    )
    return manifest


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--out-dir",
        type=Path,
        default=ROOT / "work/preserved_subject_inputs",
    )
    parser.add_argument("--preview-count", type=int, default=3)
    args = parser.parse_args()
    if args.preview_count < 0:
        raise SystemExit("--preview-count must be >= 0")
    manifest = preserve(args.out_dir, args.preview_count)
    print(json.dumps(manifest, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()

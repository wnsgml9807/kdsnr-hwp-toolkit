"""Python-side conveniences over the native API.

``export_preview`` is wrapped to (1) verify/collect the fonts the documents need
before rendering and (2) show progress bars while glyphs are cached and pages
are rendered. Everything else is re-exported from ``_native`` unchanged.
"""

from __future__ import annotations

import base64
import io
import re

from . import _native
from .hwpeq_to_latex import hwpeq_to_latex

_TAG = "[KDSNR-HWP-TOOLKIT]"

# Equation scripts are wrapped by the native side in STX/ETX sentinels at their
# inline position; convert exactly those spans to LaTeX.
_EQ_SPAN = re.compile("\x02(.*?)\x03", re.DOTALL)

_IMAGE_MIME = {
    "png": "image/png",
    "jpg": "image/jpeg",
    "jpeg": "image/jpeg",
    "bmp": "image/bmp",
    "gif": "image/gif",
}


def _image_data_uri(data: bytes, ext: str, max_px: int) -> str:
    """Downscale an embedded image (longest side ``<= max_px``) and return it as a
    base64 ``data:`` URI. Falls back to the original bytes if Pillow is missing or
    the image cannot be decoded.
    """
    try:
        from PIL import Image

        img = Image.open(io.BytesIO(data))
        img.load()
        w, h = img.size
        longest = max(w, h)
        if longest > max_px > 0:
            scale = max_px / longest
            img = img.resize((max(1, round(w * scale)), max(1, round(h * scale))))
        if img.mode not in ("RGB", "RGBA", "L"):
            img = img.convert("RGB")
        buf = io.BytesIO()
        img.save(buf, format="PNG")
        payload, mime = buf.getvalue(), "image/png"
    except Exception:
        payload = data
        mime = _IMAGE_MIME.get(ext.lower(), "application/octet-stream")
    return f"data:{mime};base64,{base64.b64encode(payload).decode('ascii')}"


def extract_questions(doc, image_max_px: int = 1024):
    """Extract a problem set's questions as JSON-ready dicts, in order.

    Each dict is ``{"label", "subject", "text", "images"}``. Equation scripts in
    ``text`` are converted to LaTeX (``$...$``); ``images`` are the question's
    embedded rasters, downscaled (longest side ``<= image_max_px``) and base64
    ``data:`` URIs — feed straight to a vision language model via ``json.dumps``.
    Korean raises ``ValueError`` (per-question support is a later version).
    """
    out = []
    for it in _native.extract_questions(doc):
        text = _EQ_SPAN.sub(lambda m: f"${hwpeq_to_latex(m.group(1))}$", it["text"])
        images = [_image_data_uri(data, ext, image_max_px) for (data, ext) in it["images"]]
        out.append({"label": it["label"], "subject": it["subject"], "text": text, "images": images})
    return out


def _tqdm(total, desc, unit):
    try:
        from tqdm import tqdm
        return tqdm(total=(total or None), desc=desc, unit=unit)
    except Exception:
        return None


def _ensure_fonts(docs) -> None:
    """Verify the font directory has every needed font; collect missing ones from
    an installed Hancom Office (Windows/macOS). Raise ``ValueError`` if any remain.
    """
    report = _native.prepare_fonts(docs)
    font_dir = report["font_dir"]
    missing0 = report["collected"] + report["missing"]  # what was absent this run
    if not missing0:
        return  # all present — say nothing

    print(f"{_TAG} 폰트 폴더 확인: {font_dir}")
    print(f"{_TAG} 필요 폰트 {report['required']}개 중 {len(missing0)}개 파일이 폴더에 없습니다.")

    if report["collected"]:
        os_name = "Windows" if report["os"] == "windows" else "macOS"
        print(f"{_TAG} {os_name} 환경 감지. 한컴 설치 폴더에서 폰트를 수집합니다...")
        print(f"{_TAG} 폰트 {len(report['collected'])}개 수집·복사 완료.")
        for i, (face, file) in enumerate(report["collected"], 1):
            print(f"  {i} : {face} -> {file}")

    if report["missing"]:
        lines = "\n".join(
            f"  {i} : {face} -> {file}" for i, (face, file) in enumerate(report["missing"], 1)
        )
        raise ValueError(
            f"{_TAG} 일부 폰트 파일을 찾을 수 없습니다. 폰트 폴더(FONT_DIR)를 확인하세요.\n# 누락 폰트\n{lines}"
        )
    print(f"{_TAG} 필요 폰트 모두 확보 완료.")


def export_preview(
    docs,
    save_path,
    preview_type: str = "page",
    media_types=None,
    dpi: float = 200.0,
):
    """Render previews to ``save_path``; see the native docs for arguments.

    ``dpi`` is the PNG raster resolution (vector-accurate; SVG/PDF ignore it).
    Verifies fonts first (collecting missing ones from a Hancom install on
    Windows/macOS, else raising ``ValueError``). On a cold glyph cache a bar
    tracks glyph decoding; a second bar tracks page rendering.
    """
    _ensure_fonts(docs)

    bars: dict = {"glyph": None, "render": None}

    def progress(phase: str, done: int, total: int) -> None:
        bar = bars[phase]
        if bar is None:
            if phase == "glyph":
                print(f"{_TAG} 글리프 캐시를 생성합니다... (최초 1회)")
                bar = _tqdm(total, f"{_TAG} 글리프 생성·캐싱", "glyph")
            else:
                bar = _tqdm(total, f"{_TAG} 미리보기 렌더링", "page")
            bars[phase] = bar if bar is not None else False
            bar = bars[phase]
        if bar:
            bar.update(max(0, done - bar.n))
            if total and done >= total:
                bar.close()

    out = _native.export_preview(docs, save_path, preview_type, media_types, dpi, progress)
    n = sum(len(group) for group in out)
    print(f"{_TAG} 파일 {n}개 내보내기 완료.")
    return out

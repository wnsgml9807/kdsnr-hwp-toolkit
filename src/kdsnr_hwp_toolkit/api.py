"""Python-side conveniences over the native API.

``export_preview`` is wrapped to (1) verify/collect the fonts the documents need
before rendering and (2) show progress bars while glyphs are cached and pages
are rendered. Everything else is re-exported from ``_native`` unchanged.
"""

from __future__ import annotations

from . import _native

_TAG = "[KDSNR-HWP-TOOLKIT]"


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

"""
lineSeg 생성기 — codec 통합용.

설계:
1. CharWidthTable: (charPr_id, char) → width_hwpunit
   - 캘리브레이션 데이터(calib_charpr_widths.json) 로드
   - 미측정 글자는 한글 monospace / Latin 평균 / fallback
2. tokenize: 텍스트 + 인라인 객체 → 토큰 (어절 단위)
3. fill_lines: 토큰을 greedy fit (한국어 금칙 적용)
4. generate_linesegs: 줄별 lineSeg (textpos, vp, vs, th, bl, sp, hp, hs, flags)

charPr별로 글자 폭이 다르므로 paragraph의 첫 charPr_id를 키로 사용 (단일 charPr 가정).
혼합 charPr는 추후 대응.
"""
from __future__ import annotations
from dataclasses import dataclass, field
from typing import Optional
import json
from pathlib import Path
from collections import defaultdict

# ──────────────────────────────────────────────────────────
# 한국어 줄 머리 / 줄 꼬리 금칙
# ──────────────────────────────────────────────────────────

LINE_START_FORBIDDEN = set(
    ")]},.!?;:'\""
    "、。…·―"
    "ー》」』】"
    "）｝〕〉＞"
    "≫］﹞〞’"
    "”，．！？"
    "；："
    "%‰℃°％"
)

LINE_END_FORBIDDEN = set(
    "([{'\""
    "《「『【"
    "（｛〔〈"
    "＜≪［〝"
    "‘“"
    "$₩£€¥"
    "＄￥"
)


def is_hangul(ch: str) -> bool:
    cp = ord(ch)
    return (0xAC00 <= cp <= 0xD7A3) or (0x1100 <= cp <= 0x11FF) or \
           (0x3130 <= cp <= 0x318F) or (0xA960 <= cp <= 0xA97F) or \
           (0xD7B0 <= cp <= 0xD7FF)


def is_latin(ch: str) -> bool:
    cp = ord(ch)
    return (0x0030 <= cp <= 0x0039) or \
           (0x0041 <= cp <= 0x005A) or \
           (0x0061 <= cp <= 0x007A)


def is_space(ch: str) -> bool:
    return ch == ' '


def is_lang_neutral(ch: str) -> bool:
    return not (is_hangul(ch) or is_latin(ch))


# ──────────────────────────────────────────────────────────
# 데이터 모델
# ──────────────────────────────────────────────────────────

@dataclass
class InlineObj:
    char_pos: int
    utf16_len: int      # HWPX char_count 기여 (보통 8 for inline objects)
    width_hwp: int
    height_hwp: int
    treat_as_char: bool = True


@dataclass
class CharStyle:
    """charPr 메트릭 — 캘리브레이션 데이터에 이미 반영됨"""
    char_shape_id: int
    font_size_hwp: int
    ratio_pct: int = 100
    spacing_pct: int = 0


@dataclass
class ParaStyle:
    line_spacing_pct: int
    indent_hwp: int = 0
    margin_left_hwp: int = 0
    margin_right_hwp: int = 0
    condense: int = 0


@dataclass
class LineSeg:
    text_pos: int
    vert_pos: int
    vert_size: int
    text_height: int
    baseline: int
    spacing: int
    horz_pos: int
    horz_size: int
    flags: int = 393216

    def to_xml(self) -> str:
        return (f'<hp:lineseg textpos="{self.text_pos}" vertpos="{self.vert_pos}" '
                f'vertsize="{self.vert_size}" textheight="{self.text_height}" '
                f'baseline="{self.baseline}" spacing="{self.spacing}" '
                f'horzpos="{self.horz_pos}" horzsize="{self.horz_size}" '
                f'flags="{self.flags}"/>')


# ──────────────────────────────────────────────────────────
# 글자 폭 테이블 — charPr별 룩업
# ──────────────────────────────────────────────────────────

class CharWidthTable:
    """
    캘리브레이션된 (unified_charPr_id, char) → width 매핑 + 메트릭 시그니처 룩업.

    소스: charpr_widths.json (intended_charPr = unified.hwpx의 charPr id)
    임의 문서의 charPr id를 받으면 해당 charPr의 메트릭 시그니처(height, ratio, spacing, fontRef)
    를 unified.hwpx의 모든 charPr와 매칭하여 동일 시그니처를 가진 unified_id를 찾고
    그 id의 캘리브레이션 데이터를 사용한다.
    """
    def __init__(self, calib_path: Path, unified_template_path: Optional[Path] = None):
        data = json.loads(Path(calib_path).read_text())
        self.by_charpr: dict[int, dict[str, float]] = defaultdict(dict)
        for r in data:
            cpr = r.get('intended_charPr')
            if cpr is None: continue
            self.by_charpr[cpr][r['char']] = r['char_width_hwpunit']

        self.hangul_default = {}
        for cpr, widths in self.by_charpr.items():
            for c, w in widths.items():
                if is_hangul(c):
                    self.hangul_default[cpr] = w
                    break

        self.global_avg = {}
        for cpr, widths in self.by_charpr.items():
            self.global_avg[cpr] = sum(widths.values()) / len(widths) if widths else 800.0

        # signature → unified_id 인덱스. calibration intended_charPr 는 subject templet 의
        # charPr id 기준 (resources/templates/{subject}.hwpx). 명시 path 없으면 4 subject union.
        self.unified_metrics: dict[int, CharMetric] = {}
        import zipfile as _zf
        templates_dir = Path(__file__).parent / "resources" / "templates"
        if unified_template_path is not None:
            paths = [unified_template_path]
        else:
            paths = sorted(templates_dir.glob("*.hwpx"))
        for tpath in paths:
            try:
                z = _zf.ZipFile(str(tpath))
                hdr = z.read('Contents/header.xml').decode('utf-8')
                for m in _re.finditer(r'<hh:charPr\b[^>]*\bid="(\d+)"[^>]*>.*?</hh:charPr>', hdr, _re.DOTALL):
                    cid = int(m.group(1))
                    if cid not in self.unified_metrics:
                        self.unified_metrics[cid] = parse_char_metric(m.group(0))
            except Exception:
                pass

    def _signature(self, m: CharMetric) -> tuple:
        return (m.height, m.ratio_hangul, m.ratio_latin, m.spacing_hangul, m.fontref_hangul, m.fontref_latin)

    def map_to_unified(self, doc_metric: CharMetric) -> Optional[int]:
        """문서의 CharMetric을 받아 동일 시그니처를 가진 unified.hwpx charPr id 반환."""
        sig = self._signature(doc_metric)
        for uid, um in self.unified_metrics.items():
            if uid not in self.by_charpr: continue  # only consider calibrated ones
            if self._signature(um) == sig:
                return uid
        # height만 일치해도 fallback
        for uid, um in self.unified_metrics.items():
            if uid not in self.by_charpr: continue
            if um.height == doc_metric.height and um.ratio_hangul == doc_metric.ratio_hangul:
                return uid
        return None

    def register_doc_metric(self, doc_cpr_id: int, char_metric: 'CharMetric') -> None:
        """한국어 폰트는 fixed-em advance (HFT probe). 한컴 식: width = em × ratio.
        spacing 은 char 간 자간 (advance 자체에 미적용)."""
        if doc_cpr_id in self.by_charpr:
            return
        em = char_metric.height
        ratio = char_metric.ratio_hangul / 100.0
        hangul_w = em * ratio
        self.hangul_default[doc_cpr_id] = hangul_w
        self.global_avg[doc_cpr_id] = hangul_w * 0.6

    def width(self, unified_charPr_id: int, ch: str) -> float:
        widths = self.by_charpr.get(unified_charPr_id, {})
        if ch in widths:
            return widths[ch]
        if is_hangul(ch):
            return self.hangul_default.get(unified_charPr_id, 1100.0)
        cp = ord(ch)
        if 0x4E00 <= cp <= 0x9FFF:
            return self.hangul_default.get(unified_charPr_id, 1100.0)
        if ch == ' ':
            return self.hangul_default.get(unified_charPr_id, 1100.0) / 2.7
        # Digit fallback: '0' is calibrated; other digits share the same
        # tabular width in monospaced-digit Korean fonts (신명 중명조 family
        # included). global_avg averages all alphanums incl. wide caps and
        # narrow punctuation, giving a way-too-wide value for digits.
        if '0' <= ch <= '9' and '0' in widths:
            return widths['0']
        # Lowercase Latin fallback to 'a' if available; uppercase to 'A'.
        if ch.isascii() and ch.islower() and 'a' in widths:
            return widths['a']
        if ch.isascii() and ch.isupper() and 'A' in widths:
            return widths['A']
        return self.global_avg.get(unified_charPr_id, 800.0)


# ──────────────────────────────────────────────────────────
# 토크나이저
# ──────────────────────────────────────────────────────────

@dataclass
class Token:
    kind: str   # 'text', 'space', 'tab', 'newline', 'inline'
    start_idx: int
    end_idx: int
    width: float
    text: str = ""
    inline: Optional[InlineObj] = None


def tokenize(
    text: str,
    inline_objs: list[InlineObj],
    char_style: CharStyle,
    width_tbl: CharWidthTable,
    tab_widths: Optional[list[int]] = None,
    char_cs_uids: Optional[list[int]] = None,
) -> list[Token]:
    """char_cs_uids: per-char unified-charPr id (length = len(text)). When
    provided, each character's width is looked up against its OWN charPr —
    crucial for paragraphs that mix QNUM (e.g. "29.") with BALMUN body, where
    a single dominant cs would over-estimate the body's width."""
    tokens: list[Token] = []
    n = len(text)
    inlines_by_pos = {obj.char_pos: obj for obj in inline_objs}
    default_cpr = char_style.char_shape_id
    tab_widths = tab_widths or []
    tab_idx = 0

    def w_at(idx: int, ch: str) -> float:
        cpr = char_cs_uids[idx] if char_cs_uids and 0 <= idx < len(char_cs_uids) else default_cpr
        return width_tbl.width(cpr, ch)

    i = 0
    while i <= n:
        if i in inlines_by_pos:
            obj = inlines_by_pos[i]
            tokens.append(Token(kind='inline', start_idx=i, end_idx=i,
                                width=float(obj.width_hwp), inline=obj))
        if i >= n: break
        ch = text[i]

        if ch == '\n':
            tokens.append(Token(kind='newline', start_idx=i, end_idx=i+1, width=0))
            i += 1
            continue

        if ch == '\t':
            # tab 의 advance 는 인라인 width 로 처리. wide tab 이 line 폭 초과하면 자연 wrap.
            tw = tab_widths[tab_idx] if tab_idx < len(tab_widths) else 0
            tab_idx += 1
            tokens.append(Token(kind='tab', start_idx=i, end_idx=i+1, width=float(tw)))
            i += 1
            continue

        if is_space(ch):
            w = w_at(i, ' ')
            tokens.append(Token(kind='space', start_idx=i, end_idx=i+1, width=w, text=ch))
            i += 1
            continue

        # 한글 어절: 연속 한글 + 후행 줄머리 금칙 흡수
        if is_hangul(ch):
            start = i
            buf = ""
            while i < n:
                c = text[i]
                if is_space(c) or c == '\n' or c == '\t': break
                if not is_hangul(c) and is_latin(c): break
                if i in inlines_by_pos and i > start: break
                buf += c
                i += 1
            while i < n and text[i] in LINE_START_FORBIDDEN \
                  and text[i] != '\n' and text[i] != '\t':
                buf += text[i]
                i += 1
            w = sum(w_at(start + k, c) for k, c in enumerate(buf))
            tokens.append(Token(kind='text', start_idx=start, end_idx=i, width=w, text=buf))
            continue

        # 라틴 단어
        if is_latin(ch):
            start = i
            buf = ""
            while i < n:
                c = text[i]
                if is_space(c) or c == '\n' or c == '\t': break
                if not is_latin(c) and not is_lang_neutral(c): break
                if i in inlines_by_pos and i > start: break
                buf += c
                i += 1
            w = sum(w_at(start + k, c) for k, c in enumerate(buf))
            tokens.append(Token(kind='text', start_idx=start, end_idx=i, width=w, text=buf))
            continue

        # 단일 기호/구두점
        w = w_at(i, ch)
        tokens.append(Token(kind='text', start_idx=i, end_idx=i+1, width=w, text=ch))
        i += 1

    return tokens


# ──────────────────────────────────────────────────────────
# 줄 채우기
# ──────────────────────────────────────────────────────────

@dataclass
class Line:
    start_token_idx: int
    end_token_idx: int
    text_start: int
    text_end: int
    used_width: int
    contains_inline: bool = False
    max_inline_h: int = 0


def fill_lines(
    tokens: list[Token],
    available_width_hwp: int,
    indent_hwp: int = 0,
    condense_pct: int = 0,
) -> list[Line]:
    """condense_pct: paraPr.condense (0~100). Hancom compresses chars by up to
    this percentage to keep a token on the current line instead of wrapping.
    Effective max content width per line = eff_w / (1 - condense_pct/100).
    """
    lines: list[Line] = []
    n = len(tokens)
    i = 0
    is_first_line = True
    condense_factor = max(0.01, 1.0 - condense_pct / 100.0)

    while i < n:
        line_start_token = i
        first_tok = tokens[i]
        line_text_start = first_tok.inline.char_pos if first_tok.kind == 'inline' else first_tok.start_idx
        used = 0
        last_break_token: Optional[int] = None
        last_break_used = 0
        line_max_inline_h = 0
        contains_inline = False

        eff_w = available_width_hwp - (indent_hwp if is_first_line else 0)
        # Allow content up to eff_w / (1 - condense) HU; actual width still
        # tracked unscaled — the renderer is the one that physically squeezes.
        eff_w_max = int(eff_w / condense_factor)
        line_complete = False

        while i < n:
            tok = tokens[i]

            if tok.kind == 'newline':
                lines.append(Line(
                    start_token_idx=line_start_token, end_token_idx=i+1,
                    text_start=line_text_start, text_end=tok.end_idx,
                    used_width=used,
                    contains_inline=contains_inline, max_inline_h=line_max_inline_h,
                ))
                i += 1
                is_first_line = False
                line_complete = True
                break

            tok_w = int(tok.width)

            if used + tok_w > eff_w_max:
                # tab 자체가 overflow를 일으키면 tab 직전(현재 i)까지 한 줄로 묶고 tab 부터 새 줄.
                # → "구하시오. \t[4점]" 같이 tab 직전 어절이 한 줄에 살아남는다.
                if tok.kind == 'tab' and i > line_start_token:
                    prev = tokens[i-1]
                    end_idx = prev.inline.char_pos if prev.kind == 'inline' else prev.end_idx
                    lines.append(Line(
                        start_token_idx=line_start_token, end_token_idx=i,
                        text_start=line_text_start, text_end=end_idx,
                        used_width=used,
                        contains_inline=contains_inline, max_inline_h=line_max_inline_h,
                    ))
                    is_first_line = False
                    line_complete = True
                    break
                if last_break_token is not None:
                    last_tok_before = tokens[last_break_token-1]
                    end_idx = last_tok_before.inline.char_pos if last_tok_before.kind == 'inline' else last_tok_before.end_idx
                    lines.append(Line(
                        start_token_idx=line_start_token, end_token_idx=last_break_token,
                        text_start=line_text_start, text_end=end_idx,
                        used_width=last_break_used,
                        contains_inline=contains_inline, max_inline_h=line_max_inline_h,
                    ))
                    i = last_break_token
                    is_first_line = False
                    line_complete = True
                    break
                else:
                    if i == line_start_token:
                        # 단일 토큰이 라인 폭 초과 — 그래도 넣고 다음 줄
                        end_idx = tok.inline.char_pos if tok.kind == 'inline' else tok.end_idx
                        lines.append(Line(
                            start_token_idx=line_start_token, end_token_idx=i+1,
                            text_start=line_text_start, text_end=end_idx,
                            used_width=used + tok_w,
                            contains_inline=contains_inline or (tok.kind == 'inline'),
                            max_inline_h=max(line_max_inline_h, tok.inline.height_hwp if tok.kind == 'inline' else 0),
                        ))
                        i += 1
                        is_first_line = False
                        line_complete = True
                        break
                    else:
                        # break 없는데 overflow — 직전까지 잘라낸다
                        prev = tokens[i-1]
                        end_idx = prev.inline.char_pos if prev.kind == 'inline' else prev.end_idx
                        lines.append(Line(
                            start_token_idx=line_start_token, end_token_idx=i,
                            text_start=line_text_start, text_end=end_idx,
                            used_width=used,
                            contains_inline=contains_inline, max_inline_h=line_max_inline_h,
                        ))
                        is_first_line = False
                        line_complete = True
                        break
            else:
                used += tok_w
                if tok.kind == 'space' or tok.kind == 'tab':
                    last_break_token = i + 1
                    last_break_used = used
                if tok.kind == 'inline':
                    contains_inline = True
                    line_max_inline_h = max(line_max_inline_h, tok.inline.height_hwp)
                i += 1

        if not line_complete:
            # 모든 토큰 소진
            last = tokens[-1]
            end_idx = last.inline.char_pos if last.kind == 'inline' else last.end_idx
            lines.append(Line(
                start_token_idx=line_start_token, end_token_idx=n,
                text_start=line_text_start, text_end=end_idx,
                used_width=used,
                contains_inline=contains_inline, max_inline_h=line_max_inline_h,
            ))
            break

    return lines


# ──────────────────────────────────────────────────────────
# UTF-16 위치 변환
# ──────────────────────────────────────────────────────────

def text_pos_to_utf16(text: str, char_pos: int, inline_objs: list[InlineObj]) -> int:
    """text의 char_pos까지의 UTF-16 단위 수 (HWPX/rhwp 규칙).
    inline 객체: 8 단위. tab '\\t': 8 단위. linebreak '\\n': 1. 일반 BMP: 1."""
    utf16 = 0
    text_idx = 0
    inlines_sorted = sorted(inline_objs, key=lambda x: x.char_pos)
    inline_iter = iter(inlines_sorted)
    next_inline = next(inline_iter, None)

    while text_idx < char_pos and text_idx < len(text):
        while next_inline is not None and next_inline.char_pos == text_idx:
            utf16 += next_inline.utf16_len
            next_inline = next(inline_iter, None)
        ch = text[text_idx]
        if ch == '\t':
            utf16 += 8
        elif ord(ch) < 0x10000:
            utf16 += 1
        else:
            utf16 += 2
        text_idx += 1
    return utf16


# ──────────────────────────────────────────────────────────
# lineSeg 생성 메인
# ──────────────────────────────────────────────────────────

def generate_linesegs(
    text: str,
    inline_objs: list[InlineObj],
    char_style: CharStyle,
    para_style: ParaStyle,
    width_tbl: CharWidthTable,
    available_width_hwp: int,
    horz_pos_hwp: int = 0,
    base_vert_pos: int = 0,
    baseline_ratio: float = 0.85,
    tab_widths: Optional[list[int]] = None,
    char_cs_uids: Optional[list[int]] = None,
    per_char_heights: Optional[list[int]] = None,
) -> list[LineSeg]:
    """
    문단 → lineSeg 리스트.

    horz_pos_hwp: 한컴이 paragraph margin/indent를 흡수해서 도출한 최종 inset.
                  fill_lines의 eff_w 계산: available_width - horz_pos.
    baseline_ratio: 일반 텍스트 0.85, eq_block 디스플레이 수식 ~0.58.
    char_cs_uids: per-char unified charPr id (e.g. "29." 발문 prefix 와 본문이
                  다른 charPr 일 때 정확히 폭을 분리 측정하기 위함).
    """
    tokens = tokenize(text, inline_objs, char_style, width_tbl,
                      tab_widths=tab_widths, char_cs_uids=char_cs_uids)
    eff_w = available_width_hwp - horz_pos_hwp
    # Hanging indent: paraPr.intent < 0 means the first line starts out-denting
    # to the left of the column, giving it extra effective width by |intent|.
    # fill_lines treats indent_hwp as a SUBTRACTION from eff_w on first line,
    # so a negative intent (out-dent) becomes a positive add. Without this
    # the first line wraps too early in BALMUN paragraphs (e.g. "29. 다음
    # 조건..." where "29." sits in the out-dent slot left of column edge).
    lines = fill_lines(tokens, eff_w,
                       indent_hwp=para_style.indent_hwp,
                       condense_pct=0)  # condense 적용 logic RE 필요 (width 압축 vs eff_w 확장)

    fs = char_style.font_size_hwp
    text_h = fs

    # spacing — 한컴 정수 산술 (round half up 방식이지만 일부 케이스에서 off-by-one)
    # text_h * (pct - 100) / 100, half-up rounding
    spacing_value = (text_h * (para_style.line_spacing_pct - 100) + 50) // 100
    if spacing_value < 0: spacing_value = 0

    # baseline = text_h * baseline_ratio (half up)
    baseline = round(text_h * baseline_ratio)

    # 디스플레이 수식 단락 식별: 텍스트 0 + 인라인 1개만 있는 단락 (eq_block)
    is_display_eq = (len(text) == 0 and len(inline_objs) >= 1)

    linesegs = []
    vert_pos = base_vert_pos
    for li, line in enumerate(lines):
        # line height = max(line 안 chars 의 char_shape.height). 한컴 line metric.
        if per_char_heights:
            line_heights = []
            for ci in range(line.text_start, min(line.text_end, len(per_char_heights))):
                h = per_char_heights[ci]
                if h > 0:
                    line_heights.append(h)
            line_text_h = max(line_heights) if line_heights else text_h
        else:
            line_text_h = text_h
        line_baseline = round(line_text_h * baseline_ratio)

        # 인라인 객체가 텍스트 높이보다 클 때 줄 높이가 인라인에 맞춰 늘어남.
        # baseline 비율: 일반 본문(인라인 첨자 포함) 0.85, 디스플레이 수식 0.58.
        if line.contains_inline and line.max_inline_h > line_text_h:
            line_text_h = line.max_inline_h
            ratio = 0.58 if is_display_eq else 0.85
            line_baseline = round(line_text_h * ratio)

        line_spacing_value = (line_text_h * (para_style.line_spacing_pct - 100) + 50) // 100
        if line_spacing_value < 0: line_spacing_value = 0

        text_pos_utf16 = text_pos_to_utf16(text, line.text_start, inline_objs)
        is_first_line = (li == 0)
        flags = 0x60000
        if is_first_line:
            flags |= 0x100000  # 1441792 = 0x160000

        linesegs.append(LineSeg(
            text_pos=text_pos_utf16,
            vert_pos=vert_pos,
            vert_size=line_text_h,
            text_height=line_text_h,
            baseline=line_baseline,
            spacing=line_spacing_value,
            horz_pos=horz_pos_hwp,
            horz_size=available_width_hwp,
            flags=flags,
        ))
        vert_pos += line_text_h + line_spacing_value

    return linesegs


# ──────────────────────────────────────────────────────────
# header.xml metric 자동 추출 — paraPr / charPr / secPr
# ──────────────────────────────────────────────────────────

import re as _re


@dataclass
class ParaMetric:
    line_spacing_pct: int
    intent: int
    margin_left: int
    margin_right: int
    margin_prev: int
    margin_next: int
    condense: int
    align_horz: str   # JUSTIFY/CENTER/LEFT/RIGHT/DISTRIBUTE_SPACE/...
    align_vert: str


@dataclass
class CharMetric:
    height: int          # font_size_hwp
    ratio_hangul: int
    ratio_latin: int
    spacing_hangul: int
    fontref_hangul: int
    fontref_latin: int


@dataclass
class PagePrMetric:
    page_width: int
    page_height: int
    margin_left: int
    margin_right: int
    margin_top: int
    margin_bottom: int
    margin_header: int
    margin_footer: int
    col_count: int
    col_gap: int

    @property
    def column_width(self) -> int:
        text_w = self.page_width - self.margin_left - self.margin_right
        if self.col_count <= 1:
            return text_w
        # Hancom 실측: (text_w - gap*(n-1)) // n 후 나머지가 있으면 -1 (모든 컬럼 균일 floor).
        # 예: 78803-2*5244-2976=65339, 65339//2=32669 — saved=32668 → -1.
        total = text_w - self.col_gap * (self.col_count - 1)
        cw = total // self.col_count
        if total % self.col_count != 0:
            cw -= 1
        return cw

    @property
    def column_text_height(self) -> int:
        return self.page_height - self.margin_top - self.margin_bottom - self.margin_header - self.margin_footer


def _attr(xml: str, name: str, default: str = "") -> str:
    m = _re.search(rf'\b{name}="([^"]*)"', xml)
    return m.group(1) if m else default


def parse_para_metric(parapr_xml: str) -> ParaMetric:
    ls = _re.search(r'<hh:lineSpacing\b[^/]*/>', parapr_xml)
    ls_xml = ls.group(0) if ls else ""
    ls_type = _attr(ls_xml, "type", "PERCENT")
    ls_value = int(_attr(ls_xml, "value", "100"))
    # Treat non-PERCENT specially: BETWEENLINES adds fixed spacing; AT_LEAST/FIXED rare.
    line_spacing_pct = ls_value if ls_type == "PERCENT" else 100  # fallback

    mar = _re.search(r'<hh:margin>.*?</hh:margin>', parapr_xml, _re.DOTALL)
    intent = left = right = prev = nxt = 0
    if mar:
        m_intent = _re.search(r'<hc:intent\b[^/]*/>', mar.group(0))
        if m_intent: intent = int(_attr(m_intent.group(0), "value", "0"))
        for fld, var in (("left", "left"), ("right", "right"), ("prev", "prev"), ("next", "next")):
            tag = _re.search(rf'<hc:{fld}\b[^/]*/>', mar.group(0))
            if tag:
                v = int(_attr(tag.group(0), "value", "0"))
                if fld == "left": left = v
                elif fld == "right": right = v
                elif fld == "prev": prev = v
                elif fld == "next": nxt = v

    al = _re.search(r'<hh:align\b[^/]*/>', parapr_xml)
    al_h = _attr(al.group(0), "horizontal", "JUSTIFY") if al else "JUSTIFY"
    al_v = _attr(al.group(0), "vertical", "BASELINE") if al else "BASELINE"

    condense = int(_attr(parapr_xml, "condense", "0"))

    return ParaMetric(
        line_spacing_pct=line_spacing_pct,
        intent=intent,
        margin_left=left, margin_right=right,
        margin_prev=prev, margin_next=nxt,
        condense=condense,
        align_horz=al_h, align_vert=al_v,
    )


def parse_char_metric(charpr_xml: str) -> CharMetric:
    height = int(_attr(charpr_xml, "height", "1000"))
    rt = _re.search(r'<hh:ratio\b[^/]*/>', charpr_xml)
    ratio_h = int(_attr(rt.group(0), "hangul", "100")) if rt else 100
    ratio_l = int(_attr(rt.group(0), "latin", "100")) if rt else 100
    sp = _re.search(r'<hh:spacing\b[^/]*/>', charpr_xml)
    sp_h = int(_attr(sp.group(0), "hangul", "0")) if sp else 0
    fr = _re.search(r'<hh:fontRef\b[^/]*/>', charpr_xml)
    fr_h = int(_attr(fr.group(0), "hangul", "0")) if fr else 0
    fr_l = int(_attr(fr.group(0), "latin", "0")) if fr else 0
    return CharMetric(
        height=height,
        ratio_hangul=ratio_h, ratio_latin=ratio_l,
        spacing_hangul=sp_h,
        fontref_hangul=fr_h, fontref_latin=fr_l,
    )


def parse_pagepr(secpr_xml: str, colpr_xml: str = "") -> PagePrMetric:
    """secpr_xml: <hp:secPr>...</hp:secPr>. colpr_xml: separate <hp:colPr ...>...</hp:colPr> if any.
    HWPX 양식은 colPr를 별도 <hp:ctrl> (LayoutDef)에 두므로 secPr에 없을 수 있음."""
    pp = _re.search(r'<hp:pagePr\b[^>]*>.*?</hp:pagePr>', secpr_xml, _re.DOTALL)
    pp_xml = pp.group(0) if pp else "<hp:pagePr/>"
    width = int(_attr(pp_xml, "width", "0"))
    height = int(_attr(pp_xml, "height", "0"))
    mar = _re.search(r'<hp:margin\b[^/]*/>', pp_xml)
    margin_attrs = mar.group(0) if mar else ""
    ml = int(_attr(margin_attrs, "left", "0"))
    mr = int(_attr(margin_attrs, "right", "0"))
    mt = int(_attr(margin_attrs, "top", "0"))
    mb = int(_attr(margin_attrs, "bottom", "0"))
    mh = int(_attr(margin_attrs, "header", "0"))
    mf = int(_attr(margin_attrs, "footer", "0"))

    # Try colPr inside secPr first, then fallback to standalone colPr
    cp = _re.search(r'<hp:colPr\b[^>]*>', secpr_xml)
    if not cp and colpr_xml:
        cp = _re.search(r'<hp:colPr\b[^>]*>', colpr_xml)
    col_count = int(_attr(cp.group(0), "colCount", "1")) if cp else 1
    col_gap = int(_attr(cp.group(0), "sameGap", "0")) if cp else 0

    return PagePrMetric(
        page_width=width, page_height=height,
        margin_left=ml, margin_right=mr, margin_top=mt, margin_bottom=mb,
        margin_header=mh, margin_footer=mf,
        col_count=col_count, col_gap=col_gap,
    )


def build_metric_tables(styles) -> tuple[dict[int, ParaMetric], dict[int, CharMetric]]:
    """StyleTable의 para_shapes/char_shapes에서 id → metric 사전 빌드."""
    para_t: dict[int, ParaMetric] = {}
    char_t: dict[int, CharMetric] = {}
    for entry in styles.para_shapes:
        try: para_t[entry.id] = parse_para_metric(entry.xml)
        except Exception: pass
    for entry in styles.char_shapes:
        try: char_t[entry.id] = parse_char_metric(entry.xml)
        except Exception: pass
    return para_t, char_t


# ──────────────────────────────────────────────────────────
# Paragraph items → text + inline objects 추출
# ──────────────────────────────────────────────────────────

def _utf16_len(s: str) -> int:
    return sum(1 if ord(c) < 0x10000 else 2 for c in s)


def extract_text_and_inlines(items) -> tuple[str, list[InlineObj], int, dict]:
    """
    codec Paragraph.items → (text_str, inline_objs, primary_char_shape_id, info)
    text_str: 모든 CharItem.text 연결. inline 위치는 text 내 그 시점의 char index 기록.
    inline_objs: hp:equation / hp:pic 같은 OpaqueInlineItem (treat_as_char=1).
    info: dict(has_tab, has_linebreak, has_table, ..., tab_widths)
    tab_widths: list[int] — i번째 TabItem 의 인라인 width (HWPX hp:tab @width).
                메타가 없으면 0.
    """
    from .codec.schema import (
        CharItem, TabItem, LineBreakItem, EmptyRunItem,
        OpaqueInlineItem, TableItem, SectionMeta, ColumnDef,
        LayoutDef, ScopeDef, NoteDef,
    )

    text_parts: list[str] = []
    inlines: list[InlineObj] = []
    primary_cs = -1
    char_cs_map: list[int] = []  # per-char cs id (parallel to text)
    info = {'has_tab': False, 'has_linebreak': False, 'has_table': False,
            'has_secpr': False, 'has_layout': False, 'has_scope': False,
            'tab_widths': []}

    cur_pos = 0
    for it in items:
        if isinstance(it, CharItem):
            if primary_cs < 0: primary_cs = it.char_shape_id
            text_parts.append(it.text)
            char_cs_map.extend([it.char_shape_id] * len(it.text))
            cur_pos += len(it.text)
        elif isinstance(it, TabItem):
            if primary_cs < 0: primary_cs = it.char_shape_id
            info['has_tab'] = True
            # tab_attrs 에 'width' 가 있으면 width(HWPUNIT) 사용
            tw = 0
            attrs = getattr(it, 'tab_attrs', {}) or {}
            try:
                tw = int(attrs.get('width', '0'))
            except Exception:
                tw = 0
            info['tab_widths'].append(tw)
            text_parts.append('\t')
            char_cs_map.append(it.char_shape_id)
            cur_pos += 1
        elif isinstance(it, LineBreakItem):
            if primary_cs < 0: primary_cs = it.char_shape_id
            info['has_linebreak'] = True
            text_parts.append('\n')
            char_cs_map.append(it.char_shape_id)
            cur_pos += 1
        elif isinstance(it, EmptyRunItem):
            if primary_cs < 0: primary_cs = it.char_shape_id
        elif isinstance(it, OpaqueInlineItem):
            if primary_cs < 0: primary_cs = it.char_shape_id
            # extract sz from xml
            w_m = _re.search(r'<hp:sz\b[^>]*\bwidth="(\d+)"', it.xml)
            h_m = _re.search(r'<hp:sz\b[^>]*\bheight="(\d+)"', it.xml)
            tac_m = _re.search(r'\btreatAsChar="(\d+)"', it.xml)
            treat_as_char = (tac_m and tac_m.group(1) == "1") if tac_m else True
            if w_m and h_m and treat_as_char:
                inlines.append(InlineObj(
                    char_pos=cur_pos,
                    utf16_len=8,  # HWPX 표준: inline run-element 1 unit, but Hancom counts 8
                    width_hwp=int(w_m.group(1)),
                    height_hwp=int(h_m.group(1)),
                    treat_as_char=True,
                ))
        elif isinstance(it, TableItem):
            info['has_table'] = True
            if primary_cs < 0: primary_cs = it.char_shape_id
        elif isinstance(it, SectionMeta):
            info['has_secpr'] = True
        elif isinstance(it, (LayoutDef, ColumnDef)):
            info['has_layout'] = True
        elif isinstance(it, ScopeDef):
            info['has_scope'] = True

    if primary_cs < 0: primary_cs = 0
    info['char_cs_map'] = char_cs_map
    return ''.join(text_parts), inlines, primary_cs, info


# ──────────────────────────────────────────────────────────
# 한 단락 → linesegs (자동 metric 적용)
# ──────────────────────────────────────────────────────────

def generate_linesegs_for_paragraph(
    paragraph,
    para_metric: ParaMetric,
    char_metric: CharMetric,
    width_tbl: CharWidthTable,
    column_width: int,
    base_vert_pos: int,
    char_t: Optional[dict] = None,
) -> tuple[list[LineSeg], int]:
    """
    단일 paragraph + (paraPr/charPr metric + 컬럼 폭) → linesegs + 단락 끝의 vert_pos.

    horz_pos = paraPr.margin.left (검증된 공식)
    horz_size = column_width − horz_pos
    spacing = round(text_h × (line_spacing_pct − 100) / 100)
    text_h = vert_size = max(charPr.height, max(inline.height_hwp))
    flags: 단일행=393216, 다중행 첫줄=1441792 (가설 — 실측 확인 필요)
    """
    text, inlines, _, info = extract_text_and_inlines(paragraph.items)
    # doc id 직접 사용 — width_tbl.register_doc_metric 가 doc id 별 한컴 native width 등록.
    cs = paragraph.char_shape_id_first or 0
    raw_cs_map = info.get('char_cs_map') if isinstance(info, dict) else None
    char_cs_uids: Optional[list[int]] = list(raw_cs_map) if raw_cs_map is not None else None

    char_style = CharStyle(
        char_shape_id=cs,
        font_size_hwp=char_metric.height,
        ratio_pct=char_metric.ratio_hangul,
        spacing_pct=char_metric.spacing_hangul,
    )
    para_style = ParaStyle(
        line_spacing_pct=para_metric.line_spacing_pct,
        indent_hwp=para_metric.intent,
        margin_left_hwp=para_metric.margin_left,
        margin_right_hwp=para_metric.margin_right,
        condense=para_metric.condense,
    )

    horz_pos = para_metric.margin_left
    horz_size = column_width - horz_pos

    # 빈 paragraph 처리 — 한 줄 lineSeg를 생성
    # text_h = max(charPr.height, max(inline.height)) — condense는 vs/th에 미적용 (실측)
    text_h = char_metric.height
    inline_max_h = max((io.height_hwp for io in inlines), default=0)
    if inline_max_h > text_h:
        text_h = inline_max_h

    spacing_value = (text_h * (para_metric.line_spacing_pct - 100) + 50) // 100
    if spacing_value < 0: spacing_value = 0

    # 빈 단락 = 단일 lineSeg. Hanword stores no horizontal content width
    # for paragraphs without text or treat-as-char inline objects.
    if not text and not inlines:
        baseline = round(text_h * 0.85)
        seg = LineSeg(
            text_pos=0, vert_pos=base_vert_pos,
            vert_size=text_h, text_height=text_h,
            baseline=baseline, spacing=spacing_value,
            horz_pos=horz_pos, horz_size=0,
            flags=0x60000,  # 단일행 — 393216
        )
        end_vp = base_vert_pos + text_h + spacing_value
        return [seg], end_vp

    # generate_linesegs.available_width_hwp = lineSeg.horzsize = column 폭 − margin_left
    tab_widths = info.get('tab_widths', []) if isinstance(info, dict) else []

    per_char_heights: Optional[list[int]] = None
    if raw_cs_map is not None and char_t is not None:
        per_char_heights = []
        for cs_id in raw_cs_map:
            cm_local = char_t.get(cs_id)
            h = cm_local.height if cm_local else char_metric.height
            per_char_heights.append(h)

    linesegs = generate_linesegs(
        text=text, inline_objs=inlines,
        char_style=char_style, para_style=para_style,
        width_tbl=width_tbl,
        available_width_hwp=horz_size,
        horz_pos_hwp=horz_pos,
        base_vert_pos=base_vert_pos,
        tab_widths=tab_widths,
        char_cs_uids=char_cs_uids,
        per_char_heights=per_char_heights,
    )

    # 단일행이면 첫줄 flags를 0x60000(393216)으로
    if len(linesegs) == 1:
        linesegs[0] = LineSeg(
            **{**linesegs[0].__dict__, 'flags': 0x60000}
        )

    end_vp = linesegs[-1].vert_pos + linesegs[-1].vert_size + linesegs[-1].spacing
    return linesegs, end_vp


# ──────────────────────────────────────────────────────────
# enrich_linesegs(doc) — 문서 전체 자동 lineSeg 채우기
# ──────────────────────────────────────────────────────────

def enrich_linesegs(doc, calib_path: Optional[Path] = None):
    """
    HwpxDocument → 모든 paragraph에 linesegs_xml을 채운 새 doc.

    Verified rules:
      - horz_pos = paraPr.margin.left
      - horz_size = column_width − horz_pos
      - delta(prev→curr) = max(prev.margin.next, curr.margin.prev)
      - spacing = round(text_h × (pct−100) / 100)
      - text_h = max(charPr.height, max(inline.height))
      - vert_pos[k+1 in para] = vert_pos[k] + vert_size + spacing

    Pending verification (may be off in edge cases):
      - flags 1441792 vs 393216 multi/single rule
      - column-wrap reset position
      - table cell vert_pos contribution to next-paragraph base
    """
    from dataclasses import replace as _dc_replace
    from .codec.schema import SectionMeta, LayoutDef, ColumnDef

    if calib_path is None:
        # default to package resource
        calib_path = Path(__file__).parent / "_resources" / "calibration" / "charpr_widths.json"
    width_tbl = CharWidthTable(calib_path)

    para_t, char_t = build_metric_tables(doc.styles)

    # doc 의 모든 charPr 를 등록 — 한국어 fixed-em advance (HFT) × ratio_hangul.
    for cs_id, cm in char_t.items():
        width_tbl.register_doc_metric(cs_id, cm)

    # 2026-05-19: inline_correction 호출은 layout.enrich_doc 가 책임진다.
    # 이 함수는 paragraph.linesegs_xml 채움 + cell.height 결정만 담당.

    new_sections = []
    for sec in doc.sections:
        # secPr는 body[0]의 첫 run에 SectionMeta로 들어감.
        # colPr는 별도 LayoutDef(<hp:ctrl><hp:colPr/></hp:ctrl>)로 들어가는 경우가 많음.
        secpr_xml = ""
        colpr_xml = ""
        for p in sec.body:
            for it in p.items:
                if isinstance(it, SectionMeta) and not secpr_xml:
                    secpr_xml = it.raw_xml
                elif isinstance(it, (LayoutDef, ColumnDef)):
                    raw = getattr(it, 'raw_xml', '')
                    if '<hp:colPr' in raw and not colpr_xml:
                        colpr_xml = raw
            if secpr_xml and colpr_xml: break
        if secpr_xml:
            pagepr = parse_pagepr(secpr_xml, colpr_xml)
        else:
            pagepr = PagePrMetric(78803, 113386, 5244, 5244, 5669, 5669, 6236, 3203, 2, 2976)

        column_width = pagepr.column_width

        new_paras = []
        cum_vp = 0       # cumulative vertical position in column
        prev_next = 0    # previous paragraph's margin.next
        for p in sec.body:
            # stored lineseg 보존: 한컴-touched input 의 native lineseg 덮어쓰지 않음.
            if p.linesegs_xml and "<hp:lineseg" in p.linesegs_xml:
                new_paras.append(_normalize_empty_paragraph_linesegs(p))
                pm_pres = para_t.get(p.para_shape_id)
                prev_next = pm_pres.margin_next if pm_pres else 0
                continue
            pm = para_t.get(p.para_shape_id)
            cm = char_t.get(p.char_shape_id_first or 0)
            if pm is None or cm is None:
                # metric 누락 — lineseg 생성 스킵 (rhwp가 fallback)
                new_paras.append(_enrich_cell_linesegs_in_items(
                    p, para_t, char_t, width_tbl,
                ))
                continue
            # 표/섹션 메타 등 구조 복잡 paragraph 만 제외. tab/linebreak 는 자체 처리.
            _, _, _, info = extract_text_and_inlines(p.items)
            if (info['has_table'] or info['has_secpr'] or info['has_layout']
                    or info['has_scope']):
                # 표 wrapper paragraph 자체는 lineseg 안 채움 (한컴 동작 동일).
                # 단, 표 셀 안 paragraph 들에는 lineseg 를 채워줘야 rhwp 가
                # 셀 텍스트를 정상적으로 줄바꿈한다 (한컴 원본 검증: 모든
                # 셀 paragraph 가 linesegarray 보유).
                new_paras.append(_enrich_cell_linesegs_in_items(
                    p, para_t, char_t, width_tbl,
                ))
                prev_next = 0
                continue

            # 단락 간 delta 적용
            delta = max(prev_next, pm.margin_prev)
            base_vp = cum_vp + delta if new_paras else 0  # 첫 단락은 0에서 시작 (가설)

            try:
                linesegs, end_vp = generate_linesegs_for_paragraph(
                    paragraph=p, para_metric=pm, char_metric=cm,
                    width_tbl=width_tbl,
                    column_width=column_width,
                    base_vert_pos=base_vp,
                    char_t=char_t,
                )
                xml_str = "<hp:linesegarray>" + "".join(ls.to_xml() for ls in linesegs) + "</hp:linesegarray>"
                new_p = _dc_replace(p, linesegs_xml=xml_str)
                new_paras.append(new_p)
                cum_vp = end_vp
                prev_next = pm.margin_next
            except Exception as e:
                # 실패 단락은 lineseg 비움 — fallback에 위임
                new_paras.append(p)

        new_sec = _dc_replace(sec, body=tuple(new_paras))
        new_sections.append(new_sec)

    # inline_correction 호출은 layout.enrich_doc 가 책임진다.
    return _dc_replace(doc, sections=tuple(new_sections))


def _correct_inline_eq_paragraph(p):
    """paragraph 안에 affectLSpacing="0" inline equation 이 있으면 lineseg.vs/vp
    를 paragraph 의 base textheight 로 cap 한다.

    한컴 spec: paragraph 안 분수가 들어간 line 의 vs 를 분수 height 로 저장하고,
    paragraph 의 다른 line 도 같이 max vs 로 저장. 그러나 한컴 PDF 렌더는
    affectLSpacing="0" inline 을 line spacing 에 반영하지 않는다. 결과적으로
    paragraph height 가 textheight 기반 (분수 무시) 으로 그려진다.

    rhwp 가 한컴 vs 를 line_height 로 그대로 쓰면 paragraph/cell height 가
    부풀려진다. 이 함수가 우리 출력 hwpx 의 lineseg.vs 를 textheight 로 cap
    해서 rhwp 가 자연스럽게 한컴 PDF 동일 결과를 그리게 한다.

    분수 자체는 rhwp paragraph_layout 이 별도 RenderNode 로 baseline 정렬해서
    그리므로 line vs 가 작아도 분수는 paragraph 영역 안/위/아래로 그려진다.
    """
    import re as _re
    from dataclasses import replace as _dc_replace
    from .codec.schema import OpaqueInlineItem

    has_target = False
    for item in getattr(p, "items", ()):
        if isinstance(item, OpaqueInlineItem) and getattr(item, "tag", "") == "hp:equation":
            if 'affectLSpacing="0"' in item.xml:
                has_target = True
                break
    if not has_target:
        return p

    xml = p.linesegs_xml or ""
    if "<hp:lineseg" not in xml:
        return p

    seg_iter = list(_re.finditer(r'<hp:lineseg\b[^/]*/>', xml))
    if not seg_iter:
        return p

    def gi(s, attr):
        a = _re.search(rf'\b{attr}="(-?\d+)"', s)
        return int(a.group(1)) if a else None

    segs = []
    for m in seg_iter:
        s = m.group()
        segs.append({
            "start": m.start(), "end": m.end(), "seg": s,
            "vp": gi(s, "vertpos"),
            "vs": gi(s, "vertsize"),
            "sp": gi(s, "spacing"),
            "th": gi(s, "textheight"),
            "bl": gi(s, "baseline"),
        })

    # base textheight = paragraph 의 line 중 가장 작은 textheight (분수 영향 안 받은 line)
    th_values = [s["th"] for s in segs if s["th"] is not None and s["th"] > 0]
    if not th_values:
        return p
    base_th = min(th_values)

    # 모든 line 의 vs 가 이미 base_th 이하면 보정 불필요
    if all(s["vs"] is None or s["vs"] <= base_th for s in segs):
        return p

    parts = []
    last_end = 0
    cur_vp = segs[0]["vp"] if segs[0]["vp"] is not None else 0
    for i, s in enumerate(segs):
        parts.append(xml[last_end:s["start"]])
        orig_vs = s["vs"] if s["vs"] is not None else base_th
        corrected_vs = min(orig_vs, base_th)
        corrected_th = min(s["th"] if s["th"] is not None else base_th, base_th)
        corrected_bl = s["bl"]
        if s["bl"] is not None and orig_vs > 0 and corrected_vs != orig_vs:
            corrected_bl = int(s["bl"] * corrected_vs / orig_vs)
        new_seg = s["seg"]
        new_seg = _re.sub(r'\bvertpos="-?\d+"', f'vertpos="{cur_vp}"', new_seg, count=1)
        new_seg = _re.sub(r'\bvertsize="\d+"', f'vertsize="{corrected_vs}"', new_seg, count=1)
        if corrected_bl is not None:
            new_seg = _re.sub(r'\bbaseline="\d+"', f'baseline="{corrected_bl}"', new_seg, count=1)
        new_seg = _re.sub(r'\btextheight="\d+"', f'textheight="{corrected_th}"', new_seg, count=1)
        parts.append(new_seg)
        last_end = s["end"]
        cur_vp = cur_vp + corrected_vs + (s["sp"] or 0)
    parts.append(xml[last_end:])
    new_xml = "".join(parts)
    return _dc_replace(p, linesegs_xml=new_xml)


def _apply_inline_eq_lineseg_correction(doc):
    """doc.sections 의 모든 paragraph (cell 안 paragraph 포함, 재귀) 에 대해
    `_correct_inline_eq_paragraph` 적용.
    """
    from dataclasses import replace as _dc_replace
    from .codec.schema import TableItem, CellItem

    def visit_para(p):
        p_new = _correct_inline_eq_paragraph(p)
        new_items = []
        any_changed = p_new is not p
        for it in getattr(p_new, "items", ()):
            if isinstance(it, TableItem):
                new_cells = []
                cell_changed = False
                for c in it.cells:
                    new_cps = tuple(visit_para(cp) for cp in c.paragraphs)
                    if any(ncp is not ocp for ncp, ocp in zip(new_cps, c.paragraphs)):
                        cell_changed = True
                        new_cells.append(_dc_replace(c, paragraphs=new_cps))
                    else:
                        new_cells.append(c)
                if cell_changed:
                    any_changed = True
                    new_items.append(_dc_replace(it, cells=tuple(new_cells)))
                else:
                    new_items.append(it)
            else:
                new_items.append(it)
        if any_changed:
            return p_new.with_items(tuple(new_items))
        return p

    new_sections = []
    for sec in doc.sections:
        new_body = tuple(visit_para(p) for p in sec.body)
        if any(np is not op for np, op in zip(new_body, sec.body)):
            new_sections.append(_dc_replace(sec, body=new_body))
        else:
            new_sections.append(sec)
    if any(ns is not os for ns, os in zip(new_sections, doc.sections)):
        return _dc_replace(doc, sections=tuple(new_sections))
    return doc


def _enrich_cell_linesegs_in_items(p, para_t, char_t, width_tbl):
    """cell paragraph 의 lineseg 만 채움. cell.cellSz.height 결정은 책임 X.

    cell_height.resolve(doc) 가 별도로 cell.height + row max 통일 + table.height
    결정을 담당한다 (layout/cell_height.py).

    Returns a new Paragraph (with TableItems replaced) if any cell paragraph
    was modified, otherwise the input paragraph unchanged.
    """
    import re as _re
    from dataclasses import replace as _dc_replace
    from .codec.schema import TableItem, CellItem, CharItem

    if not any(isinstance(it, TableItem) for it in p.items):
        return p

    new_items = []
    changed = False
    for it in p.items:
        if not isinstance(it, TableItem):
            new_items.append(it)
            continue

        canonical_bogi = _is_canonical_bogi_table(it)
        bogi_content_idx = _canonical_bogi_content_index(it) if canonical_bogi else -1
        new_cells = []
        for ci, c in enumerate(it.cells):
            # 보기 shell(label/filler cells)은 unified.hwpx의 계약이다. 다만
            # content cell 안의 source 문단은 새 frame 기준 lineSeg가 필요하다.
            if canonical_bogi and ci != bogi_content_idx:
                new_cells.append(c)
                continue

            sz_m = _re.search(r'<hp:cellSz\b[^/>]*\bwidth="(\d+)"', c.cell_meta_xml)
            cell_w = int(sz_m.group(1)) if sz_m else 0
            mg_m = _re.search(
                r'<hp:cellMargin\b[^/>]*\bleft="(\d+)"\s+right="(\d+)"',
                c.cell_meta_xml,
            )
            left_m = int(mg_m.group(1)) if mg_m else 0
            right_m = int(mg_m.group(2)) if mg_m else 0
            content_w = max(0, cell_w - left_m - right_m)

            cell_paras = []
            cum_vp = 0
            prev_next = 0
            for raw_cp in c.paragraphs:
                for cp in _split_cell_picture_paragraph(raw_cp):
                    pm = para_t.get(cp.para_shape_id)
                    cm = char_t.get(cp.char_shape_id_first or 0)
                    if pm is None or cm is None or content_w <= 0:
                        cell_paras.append(cp)
                        continue
                    _, _, _, info = extract_text_and_inlines(cp.items)
                    if (info['has_table'] or info['has_secpr']
                            or info['has_layout'] or info['has_scope']):
                        # nested table — recurse
                        cell_paras.append(_enrich_cell_linesegs_in_items(
                            cp, para_t, char_t, width_tbl,
                        ))
                        prev_next = 0
                        continue
                    # 2026-05-19: 원본 paragraph 에 이미 lineseg 가 있으면 보존.
                    # split_fused_paragraph 는 body top-level paragraph 만 clear 하므로
                    # 셀 안 paragraph 는 원본 lineseg (한컴이 저장한 정확한 줄나눔) 가
                    # 그대로 남아 있다. 재계산은 lineseg_gen 자체 메트릭으로 추정한
                    # 잘못된 줄나눔을 만들어 셀 텍스트가 깨지는 원인.
                    if cp.linesegs_xml and "<hp:lineseg" in cp.linesegs_xml:
                        norm_cp = _normalize_empty_paragraph_linesegs(cp)
                        if norm_cp is not cp:
                            changed = True
                        cp = norm_cp
                        cell_paras.append(cp)
                        # 보존된 lineseg 에서 cum_vp 추정 (셀 height 계산용)
                        last_ls = None
                        for ls_m in __import__('re').finditer(
                            r'<hp:lineseg\b[^/]*/>', cp.linesegs_xml,
                        ):
                            last_ls = ls_m.group()
                        if last_ls is not None:
                            vp_m = __import__('re').search(r'vertpos="(\d+)"', last_ls)
                            vs_m = __import__('re').search(r'vertsize="(\d+)"', last_ls)
                            sp_m = __import__('re').search(r'spacing="(-?\d+)"', last_ls)
                            vp = int(vp_m.group(1)) if vp_m else 0
                            vs = int(vs_m.group(1)) if vs_m else 0
                            sp = int(sp_m.group(1)) if sp_m else 0
                            cum_vp = vp + vs + sp
                        prev_next = pm.margin_next
                        continue
                    delta = max(prev_next, pm.margin_prev)
                    base_vp = cum_vp + delta if cell_paras else 0
                    try:
                        linesegs, end_vp = generate_linesegs_for_paragraph(
                            paragraph=cp, para_metric=pm, char_metric=cm,
                            width_tbl=width_tbl,
                            column_width=content_w,
                            base_vert_pos=base_vp,
                            char_t=char_t,
                        )
                        xml_str = ("<hp:linesegarray>"
                                   + "".join(ls.to_xml() for ls in linesegs)
                                   + "</hp:linesegarray>")
                        new_cp = _dc_replace(cp, linesegs_xml=xml_str)
                        cell_paras.append(new_cp)
                        cum_vp = max(end_vp, base_vp + _paragraph_opaque_bottom(cp))
                        prev_next = pm.margin_next
                        changed = True
                    except Exception:
                        cell_paras.append(cp)

            # cell.height 결정은 책임 X — layout/cell_height.py 가 별도로 처리.
            # cell_meta_xml 그대로 보존.
            new_cells.append(CellItem(
                cell_attrs=dict(c.cell_attrs),
                sublist_attrs=dict(c.sublist_attrs),
                paragraphs=tuple(cell_paras),
                cell_meta_xml=c.cell_meta_xml,
            ))

        # table.sz.height 갱신도 cell_height.py 의 책임.
        new_items.append(TableItem(
            table_attrs=dict(it.table_attrs),
            pre_rows_xml=it.pre_rows_xml,
            cells=tuple(new_cells),
            char_shape_id=it.char_shape_id,
            starts_new_run=it.starts_new_run,
        ))

    if not changed:
        return p
    return p.with_items(tuple(new_items))


def _normalize_empty_paragraph_linesegs(p):
    """Hanword stores horzsize=0 for content-less paragraphs.

    Some splitter paths preserve source linesegs_xml for paragraphs that only
    contain empty run markers. Those cached lineSegs can still carry the old
    frame width, so normalize the horizontal content width without touching
    real text, floating-object, table-wrapper, or section-carrier paragraphs.
    """
    from dataclasses import replace as _dc_replace
    from .codec.schema import (
        ColumnDef,
        LayoutDef,
        NoteDef,
        OpaqueInlineItem,
        ScopeDef,
        SectionMeta,
        TableItem,
    )

    xml = p.linesegs_xml or ""
    if "<hp:lineseg" not in xml:
        return p
    text, inlines, _, info = extract_text_and_inlines(p.items)
    if text or inlines:
        return p
    if info.get("has_table") or info.get("has_secpr") or info.get("has_layout") or info.get("has_scope"):
        return p
    if any(isinstance(it, (OpaqueInlineItem, TableItem, SectionMeta, ColumnDef, LayoutDef, ScopeDef, NoteDef))
           for it in getattr(p, "items", ())):
        return p
    new_xml = _re.sub(r'\bhorzsize="-?\d+"', 'horzsize="0"', xml)
    if new_xml == xml:
        return p
    return _dc_replace(p, linesegs_xml=new_xml)


def _xml_int_attr(xml: str, attr: str) -> Optional[int]:
    import re as _re
    m = _re.search(rf'\b{attr}="(\d+)"', xml)
    return int(m.group(1)) if m else None


def _replace_first_attr(xml: str, tag: str, attr: str, value: int) -> str:
    import re as _re
    pat = rf'(<{tag}\b[^>]*\b{attr}=")(\d+)(")'
    return _re.sub(pat, rf'\g<1>{value}\g<3>', xml, count=1)


def _replace_cell_height(cell_meta_xml: str, height: int) -> str:
    return _replace_first_attr(cell_meta_xml, "hp:cellSz", "height", height)


def _replace_table_height(pre_rows_xml: str, height: int) -> str:
    return _replace_first_attr(pre_rows_xml, "hp:sz", "height", height)


def _paragraph_lineseg_bottom(p) -> int:
    import re as _re
    bottom = 0
    for m in _re.finditer(
        r'<hp:lineseg\b[^>]*\bvertpos="(\d+)"[^>]*\bvertsize="(\d+)"[^>]*\bspacing="(-?\d+)"',
        p.linesegs_xml or "",
    ):
        bottom = max(bottom, int(m.group(1)) + int(m.group(2)) + int(m.group(3)))
    return bottom


def _paragraph_visual_bottom(p) -> int:
    return max(_paragraph_lineseg_bottom(p), _paragraph_opaque_bottom(p))


def _paragraph_last_line_advance(p) -> int:
    import re as _re
    last = 0
    for m in _re.finditer(
        r'<hp:lineseg\b[^>]*\bvertsize="(\d+)"[^>]*\bspacing="(-?\d+)"',
        p.linesegs_xml or "",
    ):
        last = max(0, int(m.group(1)) + int(m.group(2)))
    return last


def _signed_hwp_u32(v: int) -> int:
    return v - 0x100000000 if v >= 0x80000000 else v


def _paragraph_opaque_bottom(p) -> int:
    import re as _re
    bottom = 0
    for item in getattr(p, "items", ()):
        xml = getattr(item, "xml", "")
        if not xml or "<hp:pos" not in xml:
            continue
        sz_m = _re.search(r'<hp:curSz\b[^>]*\bheight="(\d+)"', xml)
        if not sz_m:
            sz_m = _re.search(r'<hp:sz\b[^>]*\bheight="(\d+)"', xml)
        if not sz_m:
            continue
        height = int(sz_m.group(1))
        pos_m = _re.search(r'<hp:pos\b[^>]*\bvertOffset="(\d+)"', xml)
        offset = _signed_hwp_u32(int(pos_m.group(1))) if pos_m else 0
        margin_m = _re.search(r'<hp:outMargin\b[^>]*\bbottom="(\d+)"', xml)
        margin_bottom = int(margin_m.group(1)) if margin_m else 0
        bottom = max(bottom, max(0, offset) + height + margin_bottom)
    return bottom


def _split_cell_picture_paragraph(p):
    """Split HWP-origin floating cell pictures into their own paragraphs.

    Some HWP files store pictures inside a table cell as flow-with-text
    floating objects (treatAsChar=0). rhwp can reserve blank flow space for
    them but may not paint them after the paragraph is transplanted into a
    different template section. In cells, those pictures are content. We keep
    the surrounding text in its original paragraph and emit a picture-only
    paragraph with treatAsChar=1 so the lineSeg generator can account for the
    picture as an atomic visual block.
    """
    from dataclasses import replace as _dc_replace
    from .codec.schema import OpaqueInlineItem, CharItem

    before = []
    pictures = []
    after = []
    seen_picture = False
    for item in getattr(p, "items", ()):
        if (
            isinstance(item, OpaqueInlineItem)
            and item.tag == "hp:pic"
            and 'flowWithText="1"' in item.xml
            and 'treatAsChar="0"' in item.xml
        ):
            xml = item.xml.replace('treatAsChar="0"', 'treatAsChar="1"', 1)
            pictures.append(_dc_replace(item, xml=xml))
            seen_picture = True
        elif seen_picture:
            after.append(item)
        else:
            before.append(item)

    if not pictures:
        return (p,)

    paragraphs = []
    text_items = tuple(before + after)
    if any(not (isinstance(item, CharItem) and item.text == "") for item in text_items):
        paragraphs.append(p.with_items(text_items))
    for picture in pictures:
        paragraphs.append(p.with_items((picture,)))
    return tuple(paragraphs)


def _is_canonical_bogi_table(tbl) -> bool:
    from .codec.schema import CharItem
    try:
        if str(tbl.table_attrs.get("rowCnt")) != "3":
            return False
        if str(tbl.table_attrs.get("colCnt")) != "3":
            return False
        texts = []
        for cell in tbl.cells:
            for p in cell.paragraphs:
                for item in p.items:
                    if isinstance(item, CharItem):
                        texts.append(item.text)
        text = "".join(texts).replace(" ", "")
        return "보기" in text
    except Exception:
        return False


def _canonical_bogi_content_index(tbl) -> int:
    from .codec.schema import CharItem
    best_idx = 0
    best_area = -1
    for idx, cell in enumerate(getattr(tbl, "cells", ())):
        texts = []
        for p in cell.paragraphs:
            for item in p.items:
                if isinstance(item, CharItem):
                    texts.append(item.text)
        if "보기" in "".join(texts).replace(" ", ""):
            continue
        width = _xml_int_attr(cell.cell_meta_xml, "width") or 0
        height = _xml_int_attr(cell.cell_meta_xml, "height") or 0
        area = width * height
        if area > best_area:
            best_idx = idx
            best_area = area
    return best_idx

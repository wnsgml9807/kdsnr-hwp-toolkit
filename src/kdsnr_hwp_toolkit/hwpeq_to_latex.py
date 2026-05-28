"""HYhwpEQ (HWP equation script) → LaTeX.

Converts a Hancom equation script into LaTeX so it can be fed to a language
model. Used by ``extract_questions`` to turn the equation scripts inlined in a
question's text into ``$...$`` LaTeX.

    from kdsnr_hwp_toolkit.hwpeq_to_latex import hwpeq_to_latex

    hwpeq_to_latex("a _{n+1} - a _{n} + 3")   # -> "a_{n+1} - a_{n} + 3"
"""

from __future__ import annotations

import re


# ── HYhwpEQ keyword → LaTeX (longest first) ──
_KEYWORD_TO_LATEX: list[tuple[str, str]] = [
    ("Rightarrow", r"\Rightarrow"),
    ("->", r"\to"),
    ("<-", r"\leftarrow"),
    ("<=", r"\leq"),
    (">=", r"\geq"),
    ("!=", r"\neq"),
    ("+-", r"\pm"),
    ("-+", r"\mp"),
    ("cdot", r"\cdot"),
    ("times", r"\times"),
    ("emptyset", r"\emptyset"),
    ("notin", r"\notin"),
    ("subset", r"\subset"),
    ("supset", r"\supset"),
    ("cap", r"\cap"),
    ("cup", r"\cup"),
    ("oint", r"\oint"),
    ("overline", r"\overline"),
    ("underline", r"\underline"),
    ("ddot", r"\ddot"),
    ("dot", r"\dot"),
    ("hat", r"\hat"),
    ("bar", r"\bar"),
    ("tilde", r"\tilde"),
    ("vec", r"\vec"),
    ("therefore", r"\therefore"),
    ("because", r"\because"),
    ("forall", r"\forall"),
    ("exists", r"\exists"),
]

_GREEK = [
    "alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta",
    "theta", "iota", "kappa", "lambda", "mu", "nu", "xi",
    "pi", "rho", "sigma", "tau", "upsilon", "phi", "chi", "psi", "omega",
    "Gamma", "Delta", "Theta", "Lambda", "Xi", "Pi", "Sigma",
    "Upsilon", "Phi", "Psi", "Omega",
]

_FUNCS = [
    "arcsin", "arccos", "arctan",
    "sinh", "cosh", "tanh",
    "sin", "cos", "tan", "cot", "sec", "csc",
    "log", "ln", "exp",
    "lim", "det", "dim", "ker", "deg",
]


def _extract_brace_group(s: str, pos: int) -> tuple[str | None, int]:
    """Extract the ``{...}`` group at ``pos`` (nesting-aware)."""
    while pos < len(s) and s[pos] == " ":
        pos += 1
    if pos >= len(s) or s[pos] != "{":
        return None, pos
    depth = 0
    start = pos + 1
    for i in range(pos, len(s)):
        if s[i] == "{":
            depth += 1
        elif s[i] == "}":
            depth -= 1
            if depth == 0:
                return s[start:i], i + 1
    return None, pos


# Structural keywords that HWP writes case-insensitively (and that the rules
# below match in lower case); normalize them so "LEFT"/"SQRT" also convert.
_STRUCTURAL = [
    "left", "right", "sqrt", "root", "over", "pile", "matrix",
    "lbrace", "rbrace", "box", "rm", "int", "oint", "sum", "prod",
    "cdot", "times", "inf",
]


def _normalize_dialect(s: str) -> str:
    """Smooth over real HWP-script quirks the rules below don't expect: thin-space
    backticks, alignment ``&``, upper-case keywords, and a space before a brace
    (``sqrt {x}``)."""
    s = s.replace("`", " ").replace("&", " ")
    for kw in _STRUCTURAL:
        s = re.sub(r"(?i)\b" + kw + r"\b", kw, s)
    s = re.sub(r"\b(sqrt|root)\s+\{", r"\1{", s)
    return s


def hwpeq_to_latex(hwpeq: str) -> str:
    """Convert a single HYhwpEQ script to LaTeX."""
    s = _normalize_dialect(hwpeq.strip())
    if not s:
        return s

    # {A} over {B} → \frac{A}{B}
    for _ in range(10):
        prev = s
        s = _convert_over(s)
        if s == prev:
            break

    s = re.sub(r'\bsqrt\{', r'\\sqrt{', s)
    s = re.sub(r'\broot\{([^}]*)\}\{([^}]*)\}', r'\\sqrt[\1]{\2}', s)

    def _pile_repl(m):
        lines = m.group(1).split("#")
        return r"\begin{cases} " + r" \\ ".join(l.strip() for l in lines) + r" \end{cases}"

    s = re.sub(
        r'left\s+lbrace\s+pile\{([^}]*)\}\s*right\b',
        _pile_repl, s, flags=re.DOTALL)
    s = re.sub(r'\bpile\{([^}]*)\}', _pile_repl, s, flags=re.DOTALL)

    def _matrix_repl(m):
        lines = m.group(1).split("#")
        return r"\begin{pmatrix} " + r" \\ ".join(l.strip() for l in lines) + r" \end{pmatrix}"

    s = re.sub(r'\bmatrix\{([^}]*)\}', _matrix_repl, s, flags=re.DOTALL)
    s = re.sub(r'\bbox\{([^}]*)\}', r'\\boxed{\1}', s)
    s = re.sub(r'\brm([A-Za-z]+)', r'\\mathrm{\1}', s)
    s = re.sub(r'\blbrace\b', r'\\{', s)
    s = re.sub(r'\brbrace\b', r'\\}', s)
    s = re.sub(r'\bleft\b', r'\\left', s)
    s = re.sub(r'\bright\b', r'\\right', s)
    s = re.sub(r'\boint\b', r'\\oint', s)
    s = re.sub(r'\bint\b', r'\\int', s)
    s = re.sub(r'\bsum\b', r'\\sum', s)
    s = re.sub(r'\bprod\b', r'\\prod', s)
    s = re.sub(r'(?<!\\o)(?<!\\)\binf\b', r'\\infty', s)

    for kw, latex_cmd in _KEYWORD_TO_LATEX:
        if kw in ("->", "<-", "<=", ">=", "!=", "+-", "-+"):
            s = s.replace(kw, latex_cmd)
        else:
            s = re.sub(r'\b' + re.escape(kw) + r'\b', lambda m, r=latex_cmd: r, s)

    for g in sorted(_GREEK, key=len, reverse=True):
        repl = '\\' + g
        s = re.sub(r'\b' + g + r'\b', lambda m, r=repl: r, s)

    for fn in sorted(_FUNCS, key=len, reverse=True):
        repl = '\\' + fn
        s = re.sub(r'\b' + fn + r'\b', lambda m, r=repl: r, s)

    s = re.sub(r'(?<![a-zA-Z\\])in(?![a-zA-Z])', r'\\in', s)
    s = re.sub(r'\\\\([a-zA-Z])', lambda m: '\\' + m.group(1), s)
    s = re.sub(r'  +', ' ', s)
    return s.strip()


def _convert_over(s: str) -> str:
    """HYhwpEQ ``{A} over {B}`` → LaTeX ``\\frac{A}{B}``."""
    idx = s.find(" over ")
    if idx < 0:
        idx = s.find("}over{")
        if idx < 0:
            return s
        idx += 1

    before = s[:idx].rstrip()
    after_over = s[idx:].lstrip()
    if after_over.startswith("over"):
        after_over = after_over[4:].lstrip()
    else:
        return s

    if before.endswith("}"):
        depth = 0
        for i in range(len(before) - 1, -1, -1):
            if before[i] == "}":
                depth += 1
            elif before[i] == "{":
                depth -= 1
                if depth == 0:
                    numerator = before[i + 1:-1]
                    prefix = before[:i]
                    break
        else:
            return s
    else:
        parts = before.rsplit(None, 1)
        if len(parts) == 2:
            prefix, numerator = parts
        else:
            prefix, numerator = "", parts[0]

    if after_over.startswith("{"):
        denominator, end_pos = _extract_brace_group(after_over, 0)
        if denominator is None:
            return s
        suffix = after_over[end_pos:]
    else:
        parts = after_over.split(None, 1)
        denominator = parts[0]
        suffix = parts[1] if len(parts) > 1 else ""

    result = f"{prefix}\\frac{{{numerator}}}{{{denominator}}}{suffix}"
    return result.strip()

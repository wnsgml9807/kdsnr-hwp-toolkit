"""rhwp quirk 보정 — 한컴 spec 그대로 따르면 rhwp 가 다르게 그리는 경우 보정.

원칙:
  - 한컴 원본 hwpx 를 rhwp 로 export 했을 때 깨지는지 먼저 확인
  - rhwp 의 한계로 입증되면 우리 출력 hwpx 의 metadata 를 spec 에서 살짝 비틀어
    rhwp 가 한컴 PDF 동일 결과 그리게 한다 (정공법보다 우회. 신중하게)

현재 보정:
  - affectLSpacing="0" inline equation: 한컴은 paragraph 의 모든 lineseg.vs 를
    분수 height 로 저장하지만, 한컴 PDF 렌더는 vs 를 line spacing 에 반영하지
    않음. rhwp 는 vs 를 line_height 로 사용해 paragraph height 부풀림.
    → vs 를 paragraph base textheight 로 cap.

이 모듈은 다른 layout 모듈 (linesegs, cell_height) 이 완료된 후 마지막에 호출.
"""
import re as _re
from dataclasses import replace as _dc_replace
from ..codec.schema import OpaqueInlineItem, TableItem


def apply(doc):
    """doc 의 모든 paragraph (cell 안 paragraph 포함, 재귀) 에 보정 적용."""
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
    sec_changed = False
    for sec in doc.sections:
        new_body = tuple(visit_para(p) for p in sec.body)
        if any(np is not op for np, op in zip(new_body, sec.body)):
            new_sections.append(_dc_replace(sec, body=new_body))
            sec_changed = True
        else:
            new_sections.append(sec)
    if sec_changed:
        return _dc_replace(doc, sections=tuple(new_sections))
    return doc


def _correct_inline_eq_paragraph(p):
    """paragraph 안에 affectLSpacing="0" inline equation 이 있으면 lineseg.vs
    를 paragraph base textheight 로 cap 한다.

    수정되는 attribute: vertpos (재계산), vertsize, textheight, baseline.
    분수 자체는 rhwp paragraph_layout 이 별도 RenderNode 로 baseline 정렬해서
    그리므로 line vs 가 작아도 분수는 paragraph 영역으로 표시됨.
    """
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

    th_values = [s["th"] for s in segs if s["th"] is not None and s["th"] > 0]
    if not th_values:
        return p
    base_th = min(th_values)

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

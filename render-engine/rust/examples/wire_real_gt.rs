//! `wire_real_gt` — 진짜 한컴 GT pdf 와 우리 wire 1:1 비교 (페이지 전체).
//!
//! ## 배경
//!
//! work/GT/<subject>__<sample>/<stem>.pdf 는 한컴 Office HWP.app 의 "PDF로 인쇄"
//! 출력 — Producer "macOS Quartz PDFContext", 1 page each, 페이지 사이즈 771×1117 pts
//! (= 272×394 mm 시험지 페이지). 기존 work/e2e/<...>/<stem>.png 는 rhwp 기반 crop
//! GT 라 좌표계 정합 안 됨. 본 binary 가 진짜 한컴 GT 와 비교.
//!
//! ## 흐름
//!
//! ```text
//! work/GT/<.../stem.pdf>   ──► pdftoppm 200 DPI ──► tmp PNG (페이지 전체)
//!                                                       │
//! work/e2e/<.../stem.hwpx> ──► rhwp parse ──► render_tree
//!                                              │
//!                                              ▼
//!                            wire_probe::traverse → SvgSurface
//!                                              │
//!                                              ▼
//!                                       SVG string → resvg → PNG (GT 크기)
//!                                              │
//!                                              ▼
//!                                  pixel_diff vs GT PNG → score
//! ```
//!
//! ## 좌표계 정합
//!
//! - 한컴 GT pdf = 페이지 271.9 × 394.0 mm @ 200 DPI = **2142 × 3103 px**
//! - 우리 rhwp page bbox = 1028.0 × 1489.1 px @ 96 DPI = **272 × 394 mm**
//! - 같은 페이지! resvg 가 우리 SVG (1028×1489 viewBox) 를 2142×3103 으로 uniform
//!   2.084x scale → 글자 모양 유지, 좌표계 1:1 정합

use kdsnr_render::brush::{Brush, EmptyBrush, SolidBrush};
use kdsnr_render::color::Color;
use kdsnr_render::pen::Pen;
use kdsnr_render::pixel_diff_harness::{make_heatmap_rgba, score_pages, score_pages_ink_only, score_pages_ink_iou, DiffOptions};
use kdsnr_render::surface::{Font, Image, Path, PathCmd, PointImpl, RectImpl, StringFormat, Surface};
use kdsnr_render::svg_surface::SvgSurface;
use std::path::{Path as StdPath, PathBuf};
use std::process::Command;
use std::sync::Arc;

const RENDER_DPI: u32 = 200;

// ─── helper: rhwp ColorRef → kdsnr_render::Color ──────────────────────

fn color_ref_to_color(cref: u32) -> Color {
    let r = (cref & 0xff) as u8;
    let g = ((cref >> 8) & 0xff) as u8;
    let b = ((cref >> 16) & 0xff) as u8;
    Color::from_rgb(r, g, b, std::ptr::null_mut())
}

fn solid_brush(cref: u32) -> Brush {
    Brush::Solid(SolidBrush::new(color_ref_to_color(cref)))
}

fn pen_for(stroke_color: u32, width: f64) -> Pen {
    let mut pen = Pen::new_default();
    pen.brush = Box::new(solid_brush(stroke_color));
    pen.set_thickness(width as f32);
    pen
}

fn empty_pen() -> Pen {
    let mut pen = Pen::new_default();
    pen.brush = Box::new(Brush::Empty(EmptyBrush::new()));
    pen
}

// ─── node traverse ─────────────────────────────────────────────────────

#[derive(Debug, Default)]
struct Counts {
    text_run: u32,
    text_rendered: u32,
    text_skipped: u32,
    rectangle: u32,
    line: u32,
    image: u32,
    image_rendered: u32,
    path: u32,
    equation: u32,
    group: u32,
    other: u32,
}

fn traverse(
    node: &rhwp::renderer::render_tree::RenderNode,
    svg: &mut SvgSurface,
    cache: &kdsnr_hft::HftCache,
    counts: &mut Counts,
) {
    use rhwp::renderer::render_tree::RenderNodeType as T;
    if !node.visible { return; }
    let bbox = node.bbox;
    let rect_f32 = RectImpl {
        x: bbox.x as f32, y: bbox.y as f32,
        w: bbox.width as f32, h: bbox.height as f32,
    };

    match &node.node_type {
        T::TextRun(tr) => {
            counts.text_run += 1;
            let face = if !tr.style.raw_font_family.is_empty() {
                &tr.style.raw_font_family
            } else {
                &tr.style.font_family
            };
            // DEBUG: ASCII-bearing run face dump (env WIRE_DUMP_FACES=1 로 활성화)
            if std::env::var("WIRE_DUMP_FACES").is_ok() {
                let has_ascii = tr.text.chars().any(|c| (0x21..0x80).contains(&(c as u32)));
                if has_ascii {
                    static SEEN: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<(String,String)>>> = std::sync::OnceLock::new();
                    let mtx = SEEN.get_or_init(|| std::sync::Mutex::new(std::collections::HashSet::new()));
                    let key = (tr.style.font_family.clone(), tr.style.raw_font_family.clone());
                    let mut s = mtx.lock().unwrap();
                    if !s.contains(&key) {
                        s.insert(key);
                        let sample: String = tr.text.chars().take(35).collect();
                        eprintln!("  [face] font={:?}  raw={:?}  sample={:?}",
                                  tr.style.font_family, tr.style.raw_font_family, sample);
                    }
                }
            }
            let text16: Vec<u16> = tr.text.encode_utf16().collect();
            if text16.is_empty() { return; }

            // family resolution — face name 그대로 (HFT alias 자동), 또는 font_family fallback
            let family_for_hangul = {
                let mut f = face.clone();
                let mut hit = false;
                for cp in text16.iter().take(5) {
                    if cache.get(&f, *cp as u32).is_some() { hit = true; break; }
                }
                if !hit && !tr.style.font_family.is_empty() && tr.style.font_family != *face {
                    let f2 = tr.style.font_family.clone();
                    for cp in text16.iter().take(5) {
                        if cache.get(&f2, *cp as u32).is_some() {
                            f = f2.clone(); hit = true; break;
                        }
                    }
                }
                f
            };

            let baseline_y = bbox.y as f32 + tr.baseline as f32;
            let brush = solid_brush(tr.style.color);
            let font_size = tr.style.font_size as f32;
            let color_hex = format!("#{:06x}",
                ((tr.style.color & 0xff) << 16)  // BGR → RGB
                | (tr.style.color & 0xff00)
                | ((tr.style.color >> 16) & 0xff)
            );

            // ASCII fallback Latin HFT family (한컴 본문 face → HCEN* 매핑).
            // rhwp 의 TextRun.style 은 lang split 후에도 face=한국어 face 만 들고
            // 옴 — ASCII 글자가 와도 cache.get(한국어 face, ASCII) = miss. 정공법:
            // 한국어 face 모양 (명조/고딕) 으로 HCEN* family 추정 lookup.
            //
            // HCEN HFT advance 는 probe 측정 결과 0.022-0.061em 으로 한컴 native
            // advance 와 다른 값 (한컴은 별도 metric 사용). path 만 가져오고
            // advance 는 system-font 추정값 유지.
            let latin_family = pick_latin_family(&family_for_hangul);

            // 글자별 처리:
            //   1) family_for_hangul HFT hit (한글/심볼) → HFT path emit
            //   2) miss + latin_family HFT hit (ASCII) → HFT path emit (latin family 사용)
            //   3) 모두 miss → SVG <text> fallback (system font)
            //
            // x 좌표: rhwp 의 layout 단계가 계산한 per-char positions 를 직접 사용.
            // tab/inline_tabs/letter_spacing/ratio 모두 처리됨. 우리 자체 advance
            // 누적 (HFT advance + ASCII 추정) 은 fallback 으로만 사용 (positions 길이
            // 미스매치 시).
            let char_positions: Vec<f64> = rhwp::renderer::layout::text_measurement
                ::compute_char_positions(&tr.text, &tr.style);
            // char_positions 는 N+1 개 경계값. N = tr.text.chars().count(). text16 의 글자 수는
            // surrogate pair 등으로 다를 수 있어 둘 다 사용 — char 단위로 mapping.
            let text_chars: Vec<char> = tr.text.chars().collect();

            let mut chunk_start = 0;
            // RenderState: 0 = hangul HFT, 1 = latin HFT, 2 = system serif fallback
            let mut chunk_state: Option<u8> = None;
            let mut chunk_x = bbox.x as f32;

            // 각 char 의 (codepoint, state, advance, family)
            let chars: Vec<(u32, u8, f32, String)> = text_chars.iter().enumerate().map(|(i, ch)| {
                let cp32 = *ch as u32;
                let (state, family) = if cache.get(&family_for_hangul, cp32).is_some() {
                    (0u8, family_for_hangul.clone())
                } else if !latin_family.is_empty() && cache.get(&latin_family, cp32).is_some() {
                    (1u8, latin_family.to_string())
                } else {
                    (2u8, String::new())
                };
                // advance 결정:
                //   - rhwp compute_char_positions 의 layout 계산값 (tab jump 포함)
                //   - 우리 HFT/ASCII 추정값 (글자 실제 visible width)
                //   - max(둘) = 한컴 tab 위치 보존 + 글자 자체 너비 최소 유지
                //
                // 단순 computed 사용 시 한글 본문 글자가 좁게 붙음 (rhwp measure_char_width
                // 가 너무 작음). max 로 한글 글자의 1em 너비 보장.
                let hft_or_est_adv = if state == 0 {
                    if let Some(g) = cache.get(&family_for_hangul, cp32) {
                        let scale = font_size / (g.em as f32);
                        g.advance as f32 * scale
                    } else { font_size * 0.5 }
                } else if cp32 == 0x20 || cp32 == 0xa0 {
                    font_size * 0.27
                } else if cp32 == 0x09 {
                    0.0  // tab 은 자체 width 0 — computed 만 사용
                } else if (0x30..=0x39).contains(&cp32) {
                    font_size * 0.50
                } else if cp32 < 0x80 {
                    font_size * 0.40
                } else {
                    font_size * 0.50
                };
                let adv = if i + 1 < char_positions.len() {
                    let computed = (char_positions[i + 1] - char_positions[i]) as f32;
                    computed.max(hft_or_est_adv)
                } else {
                    hft_or_est_adv
                };
                (cp32, state, adv, family)
            }).collect();

            // text_chars 의 글자 수와 text16 (UTF-16) 의 길이가 다를 수 있음 (surrogate pair 등).
            // 본 wire 의 chunking 은 text_chars 단위로 진행.
            let mut x = bbox.x as f32;
            let _ = chunk_state;
            let mut chunk_state: Option<u8> = None;
            let _ = chunk_start;
            let mut chunk_start: usize = 0;

            let n = chars.len();
            for i in 0..n {
                let st = chars[i].1;
                if chunk_state.is_none() {
                    chunk_state = Some(st);
                    chunk_start = i;
                    chunk_x = x;
                } else if chunk_state != Some(st) {
                    // flush chunk [chunk_start..i)
                    let fam = chars[chunk_start].3.clone();
                    emit_text_chunk(
                        svg, &chars[chunk_start..i],
                        chunk_state.unwrap(),
                        chunk_x, baseline_y,
                        &fam, font_size, &brush, &color_hex,
                        &text16[chunk_start..i],
                    );
                    chunk_start = i;
                    chunk_x = x;
                    chunk_state = Some(st);
                }
                x += chars[i].2;
            }
            if chunk_state.is_some() {
                let fam = chars[chunk_start].3.clone();
                emit_text_chunk(
                    svg, &chars[chunk_start..n],
                    chunk_state.unwrap(),
                    chunk_x, baseline_y,
                    &fam, font_size, &brush, &color_hex,
                    &text16[chunk_start..n],
                );
            }

            let any_hft_count: usize = chars.iter().filter(|c| c.1 < 2).count();
            if any_hft_count > 0 { counts.text_rendered += 1; }
            else { counts.text_skipped += 1; }
        }
        T::Rectangle(rn) => {
            counts.rectangle += 1;
            if let Some(fc) = rn.style.fill_color {
                svg.fill_rect_float(rect_f32, &solid_brush(fc));
            }
            if let Some(sc) = rn.style.stroke_color {
                let pen = pen_for(sc, rn.style.stroke_width.max(0.25));
                svg.outline_rect_float(rect_f32, &pen);
            }
        }
        T::Line(ln) => {
            counts.line += 1;
            let mut p = Path::default();
            p.commands.push(PathCmd::MoveTo(ln.x1 as f32, ln.y1 as f32));
            p.commands.push(PathCmd::LineTo(ln.x2 as f32, ln.y2 as f32));
            let pen = pen_for(ln.style.color, ln.style.width.max(0.25));
            svg.outline_path(&p, &pen);
        }
        T::Image(img_node) => {
            counts.image += 1;
            let Some(data) = &img_node.data else { return; };
            if data.is_empty() { return; }
            counts.image_rendered += 1;
            let image = Image {
                data: data.clone(),
                width: bbox.width as u32, height: bbox.height as u32,
            };
            svg.draw_image_f(rect_f32, &image, 1.0);
        }
        T::Path(pn) => {
            counts.path += 1;
            // rhwp PathCommand → surface::Path::PathCmd
            let mut p = Path::default();
            use rhwp::renderer::PathCommand as PC;
            for cmd in &pn.commands {
                match cmd {
                    PC::MoveTo(x, y) => p.commands.push(PathCmd::MoveTo(*x as f32, *y as f32)),
                    PC::LineTo(x, y) => p.commands.push(PathCmd::LineTo(*x as f32, *y as f32)),
                    PC::CurveTo(x1, y1, x2, y2, x3, y3) => {
                        p.commands.push(PathCmd::CurveTo(
                            *x1 as f32, *y1 as f32,
                            *x2 as f32, *y2 as f32,
                            *x3 as f32, *y3 as f32,
                        ));
                    }
                    PC::ArcTo(rx, ry, phi, large, sweep, x, y) => {
                        // SVG arc → bezier (rhwp svg_arc_to_beziers 헬퍼 사용)
                        // 현재 pen 위치 알 수 없어 임의 (0,0) 시작 — 정확도 떨어지지만
                        // P0 input 에 ArcTo 빈도 낮음. 정밀 fix 필요 시 last MoveTo/LineTo 추적.
                        let last = p.commands.last().copied();
                        let (sx, sy) = match last {
                            Some(PathCmd::MoveTo(x, y)) | Some(PathCmd::LineTo(x, y)) => (x, y),
                            Some(PathCmd::CurveTo(_, _, _, _, x, y)) => (x, y),
                            _ => (0.0, 0.0),
                        };
                        let beziers = rhwp::renderer::svg_arc_to_beziers(
                            sx as f64, sy as f64,
                            *rx, *ry, *phi,
                            *large, *sweep,
                            *x, *y,
                        );
                        for b in beziers {
                            match b {
                                PC::CurveTo(x1, y1, x2, y2, x3, y3) => {
                                    p.commands.push(PathCmd::CurveTo(
                                        x1 as f32, y1 as f32,
                                        x2 as f32, y2 as f32,
                                        x3 as f32, y3 as f32,
                                    ));
                                }
                                PC::LineTo(x, y) => {
                                    p.commands.push(PathCmd::LineTo(x as f32, y as f32));
                                }
                                _ => {}
                            }
                        }
                    }
                    PC::ClosePath => p.commands.push(PathCmd::Close),
                }
            }
            if let Some(fc) = pn.style.fill_color {
                svg.fill_path(&p, &solid_brush(fc));
            }
            if let Some(sc) = pn.style.stroke_color {
                let pen = pen_for(sc, pn.style.stroke_width.max(0.25));
                svg.outline_path(&p, &pen);
            }
        }
        T::Equation(eq) => {
            counts.equation += 1;
            // rhwp 가 이미 SVG fragment 를 만들어 줌. bbox 위치에 translate 후 raw emit.
            let _ = empty_pen;
            writeln!(
                &mut svg.buffer,
                r#"<g transform="translate({:.3} {:.3})">"#,
                bbox.x, bbox.y
            ).unwrap();
            svg.buffer.push_str(&eq.svg_content);
            svg.buffer.push_str("\n</g>\n");
        }
        T::Group(_) => { counts.group += 1; /* children only */ }
        T::RawSvg(raw) => {
            svg.buffer.push_str(&raw.svg);
            svg.buffer.push('\n');
        }
        T::Page(_) | T::Body { .. } | T::Header | T::Footer | T::MasterPage
        | T::Column(_) | T::FootnoteArea | T::TextLine(_) | T::Table(_)
        | T::TableCell(_) | T::PageBackground(_) | T::TextBox => {
            // recurse only
        }
        _ => { counts.other += 1; }
    }
    for child in &node.children {
        traverse(child, svg, cache, counts);
    }
}

use std::fmt::Write as _;

/// 한국어 face 이름 → Latin HFT family 추정 (HCEN*).
///
/// 한컴 hftinfo.dat 의 alias 가 한국어 face → 한국어 HFT 만 매핑하고
/// Latin HFT 매핑은 별도. probe_hft_ascii 로 확인한 ASCII 포함 family:
///   HCENSMJ (Latin 신명조 — 52/52 letters, 10/10 digits)
///   HCENGGT (Latin 견고딕 — 52/52, 10/10)
///   HCENGMJ (Latin 견명조 — 52/52, 10/10)
///
/// 한컴 본문 face 모양 (명조/고딕/heavy) 으로 매핑:
///   * 고딕 계열 (고딕/태고딕/신그래픽/Gothic) → HCENGGT
///   * 명조 계열 (명조/바탕/신명/한양신/Batang) → HCENSMJ
///   * default → HCENSMJ
fn pick_latin_family(hangul_face: &str) -> String {
    let lower = hangul_face.to_lowercase();
    let is_gothic = hangul_face.contains("고딕")
        || hangul_face.contains("그래픽")
        || hangul_face.contains("태고딕")
        || hangul_face.contains("중고딕")
        || hangul_face.contains("견고딕")
        || lower.contains("gothic")
        || lower.contains("dotum");
    if is_gothic {
        "HCENGGT".to_string()
    } else {
        "HCENSMJ".to_string()
    }
}

/// Emit a text chunk to SVG.
///
/// `state` 값:
///   0 = HFT path (한글 family)
///   1 = HFT path (Latin family — HCEN*)
///   2 = SVG `<text>` fallback (system serif)
///
/// `chars` 각 entry: (codepoint, state, advance, family). state 가 같은 chunk 내에서
/// family 도 같다고 가정 (chunking 시 state 만 비교하지만 family 는 state 별로 동일).
#[allow(clippy::too_many_arguments)]
fn emit_text_chunk(
    svg: &mut SvgSurface,
    chars: &[(u32, u8, f32, String)],
    state: u8,
    x_start: f32,
    baseline_y: f32,
    family: &str,
    font_size: f32,
    brush: &Brush,
    color_hex: &str,
    text16: &[u16],
) {
    if chars.is_empty() { return; }
    if state < 2 {
        // HFT path emit (한글 또는 Latin family).
        // ⚠️ 주의: state==1 (Latin) 일 때 HFT advance 가 비정상 (probe 0.022-0.061em)
        // 이므로 우리는 char 별 추정 advance 누적으로 x 계산. draw_string_point 가
        // HFT advance 그대로 다음 글자 위치 정한다면 chunk 안 글자가 겹침. 그래서
        // **state==1 (Latin) 은 char 별 단일 draw_string_point 호출** 로 우리 추정
        // x 사용.
        let font = Font {
            family: family.to_string(),
            size: font_size,
            bold: false,
            italic: false,
        };
        if state == 0 {
            // 한글 HFT — chunk 단위 emit (HFT advance 정상)
            svg.draw_string_point(
                text16, &font,
                PointImpl { x: x_start, y: baseline_y },
                brush, &StringFormat::default(),
            );
        } else {
            // Latin HFT — char 별 emit (우리 추정 advance 로 위치 강제)
            let mut x = x_start;
            for (i, ch) in chars.iter().enumerate() {
                let single = [text16[i]];
                svg.draw_string_point(
                    &single, &font,
                    PointImpl { x, y: baseline_y },
                    brush, &StringFormat::default(),
                );
                x += ch.2;
            }
        }
    } else {
        // SVG <text> fallback (system serif).
        // text 를 escape 한 뒤 그대로 emit. resvg 가 system font 로 렌더.
        let s: String = char::decode_utf16(text16.iter().copied())
            .filter_map(|r| r.ok()).collect();
        let escaped = xml_escape(&s);
        writeln!(
            &mut svg.buffer,
            r#"<text x="{:.2}" y="{:.2}" font-family="serif" font-size="{:.2}" fill="{}" xml:space="preserve">{}</text>"#,
            x_start, baseline_y, font_size, color_hex, escaped,
        ).unwrap();
    }
}

fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

// ─── PNG IO ────────────────────────────────────────────────────────────

fn read_png_rgba(path: &StdPath) -> Option<(Vec<u8>, u32, u32)> {
    let f = std::fs::File::open(path).ok()?;
    let mut r = png::Decoder::new(f).read_info().ok()?;
    let mut buf = vec![0u8; r.output_buffer_size()];
    let info = r.next_frame(&mut buf).ok()?;
    let (w, h) = (info.width, info.height);
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf[..info.buffer_size()].to_vec(),
        png::ColorType::Rgb => {
            let n = (w * h) as usize;
            let mut o = Vec::with_capacity(n * 4);
            for i in 0..n {
                o.push(buf[i * 3]); o.push(buf[i * 3 + 1]); o.push(buf[i * 3 + 2]); o.push(0xff);
            }
            o
        }
        _ => return None,
    };
    Some((rgba, w, h))
}

fn write_png_rgba(path: &StdPath, rgba: &[u8], w: u32, h: u32) {
    if let Some(p) = path.parent() { std::fs::create_dir_all(p).ok(); }
    let Ok(f) = std::fs::File::create(path) else { return };
    let mut e = png::Encoder::new(std::io::BufWriter::new(f), w, h);
    e.set_color(png::ColorType::Rgba);
    e.set_depth(png::BitDepth::Eight);
    let Ok(mut wr) = e.write_header() else { return };
    let _ = wr.write_image_data(rgba);
}

fn rasterize_svg(svg: &str, w: u32, h: u32) -> Option<Vec<u8>> {
    let mut opt = usvg::Options::default();
    opt.fontdb_mut().load_system_fonts();
    // 한컴 equation font (HyhwpEQ) — PUA 영역 (U+E000~) 의 그리스/수식 기호.
    // system font 에 없으면 resvg 가 fallback chain → ?
    // 박스. 정공법: HYHWPEQ.TTF 명시 load.
    // (rhwp equation svg 의 <text font-family="'HyhwpEQ'...">)
    let hyhwpeq_path = "/Applications/Hancom Office HWP.app/Contents/Resources/Hnc/Shared/TTF/Install/HYHWPEQ.TTF";
    if std::path::Path::new(hyhwpeq_path).exists() {
        opt.fontdb_mut().load_font_file(hyhwpeq_path).ok();
    }
    // 한컴 시험지 본문 face (한양신명조/함초롬바탕 등) 도 추가 load — system font 에 없으면 fallback.
    let hwp_ttf_dir = "/Applications/Hancom Office HWP.app/Contents/Resources/Hnc/Shared/TTF/Install";
    if let Ok(rd) = std::fs::read_dir(hwp_ttf_dir) {
        for entry in rd.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case("ttf")) == Some(true) {
                let _ = opt.fontdb_mut().load_font_file(&path);
            }
        }
    }
    let tree = usvg::Tree::from_str(svg, &opt).ok()?;
    let sz = tree.size();
    let sx = w as f32 / sz.width();
    let sy = h as f32 / sz.height();
    let t = tiny_skia::Transform::from_scale(sx, sy);
    let mut pix = tiny_skia::Pixmap::new(w, h)?;
    pix.fill(tiny_skia::Color::WHITE);
    resvg::render(&tree, t, &mut pix.as_mut());
    Some(pix.take())
}

fn pdf_page_to_png(pdf: &StdPath, out_png: &StdPath, dpi: u32) -> Result<(), String> {
    // pdftoppm 호출 → -r {dpi} -png -f 1 -l 1 <pdf> <prefix>
    let tmp = tempfile::TempDir::new().map_err(|e| format!("tmpdir: {e}"))?;
    let prefix = tmp.path().join("p");
    let out = Command::new("pdftoppm")
        .args(["-r", &dpi.to_string(), "-png", "-f", "1", "-l", "1"])
        .arg(pdf)
        .arg(&prefix)
        .output()
        .map_err(|e| format!("pdftoppm spawn: {e}"))?;
    if !out.status.success() {
        return Err(format!("pdftoppm exit {}: {}",
            out.status, String::from_utf8_lossy(&out.stderr)));
    }
    // poppler is variable in suffix — `<prefix>-1.png` 또는 `<prefix>.png`
    let cands = [
        tmp.path().join("p-1.png"),
        tmp.path().join("p-01.png"),
        tmp.path().join("p.png"),
    ];
    let mut found = None;
    for c in &cands { if c.exists() { found = Some(c.clone()); break; } }
    let src = found.ok_or_else(|| "pdftoppm produced no png".to_string())?;
    if let Some(p) = out_png.parent() { std::fs::create_dir_all(p).ok(); }
    std::fs::copy(&src, out_png).map_err(|e| format!("copy png: {e}"))?;
    Ok(())
}

// ─── batch driver ─────────────────────────────────────────────────────

fn discover_pairs(gt_root: &StdPath, hwpx_root: &StdPath) -> Vec<(PathBuf, PathBuf, String, String)> {
    let mut pairs = Vec::new();
    let Ok(rd) = std::fs::read_dir(gt_root) else { return pairs };
    let mut subdirs: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir()).collect();
    subdirs.sort();
    for sub in subdirs {
        let subname = sub.file_name().and_then(|s| s.to_str()).unwrap_or("").to_string();
        let Ok(rd) = std::fs::read_dir(&sub) else { continue };
        let mut files: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
        files.sort();
        for f in &files {
            if f.extension().and_then(|s| s.to_str()) != Some("pdf") { continue; }
            let Some(stem) = f.file_stem().and_then(|s| s.to_str()) else { continue };
            let hwpx = hwpx_root.join(&subname).join(format!("{stem}.hwpx"));
            if !hwpx.exists() { continue; }
            pairs.push((f.clone(), hwpx, subname.clone(), stem.to_string()));
        }
    }
    pairs
}

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    let gt_root = PathBuf::from(argv.get(1).map(|s| s.as_str()).unwrap_or("work/GT"));
    let hwpx_root = PathBuf::from(argv.get(2).map(|s| s.as_str()).unwrap_or("work/e2e"));
    let out_dir = PathBuf::from(argv.get(3).map(|s| s.as_str()).unwrap_or("work/e2e/_real_gt_output"));
    std::fs::create_dir_all(&out_dir).ok();

    eprintln!("=== wire_real_gt ===");
    eprintln!("gt_root  : {}", gt_root.display());
    eprintln!("hwpx_root: {}", hwpx_root.display());
    eprintln!("out_dir  : {}", out_dir.display());

    // HFT embedded
    let mut cache = kdsnr_hft::HftCache::new();
    let n = kdsnr_hft::embedded::load_into(&mut cache).expect("embedded HFT load");
    eprintln!("HFT embedded: {n} glyphs ({} families, {} aliases)",
              cache.family_count(), cache.alias_count());
    let cache_arc = Arc::new(cache);

    let pairs = discover_pairs(&gt_root, &hwpx_root);
    eprintln!("found {} GT pdf/hwpx pairs", pairs.len());
    // 첫 일부 face 호출로 hook 자체 trigger 확인 (sanity)
    let _ = rhwp::renderer::kdsnr_hft_global::advance_em("함초롬바탕", '가');
    eprintln!("kdsnr_hft hook sanity: ('함초롬바탕','가') initial hit/miss = {:?}",
              rhwp::renderer::kdsnr_hft_global::stats());
    println!("subdir,stem,gt_w,gt_h,page_bbox_w,page_bbox_h,text_total,text_rendered,text_skipped,rect,line,image,path,equation,group,score_pct,mismatch,avg_delta,ink_score_pct,ink_total,ink_matched,ink_avg_delta,iou2_pct,iou5_pct,gt_ink,our_ink");

    let mut all_scores = Vec::new();
    for (i, (pdf, hwpx, subdir, stem)) in pairs.iter().enumerate() {
        eprintln!("[{}/{}] {} / {}", i + 1, pairs.len(), subdir, stem);
        // 1) pdf → GT PNG (페이지 전체)
        let gt_png = out_dir.join(format!("{}__{}_gt.png", subdir, stem));
        if let Err(e) = pdf_page_to_png(pdf, &gt_png, RENDER_DPI) {
            eprintln!("  pdftoppm FAIL: {e}");
            continue;
        }
        let Some((gt_rgba, gt_w, gt_h)) = read_png_rgba(&gt_png) else {
            eprintln!("  gt png decode FAIL");
            continue;
        };

        // 2) hwpx parse + render_tree
        let Ok(data) = std::fs::read(hwpx) else {
            eprintln!("  hwpx read FAIL");
            continue;
        };
        let doc = match rhwp::wasm_api::HwpDocument::from_bytes(&data) {
            Ok(d) => d,
            Err(e) => { eprintln!("  rhwp parse FAIL: {e}"); continue; }
        };
        let tree = match doc.build_page_render_tree(0) {
            Ok(t) => t,
            Err(e) => { eprintln!("  build_page_render_tree FAIL: {e}"); continue; }
        };
        let page_w = tree.root.bbox.width as f32;
        let page_h = tree.root.bbox.height as f32;

        // 3) SvgSurface
        let mut svg = SvgSurface::new(page_w, page_h).with_hft_cache(Arc::clone(&cache_arc));
        // 흰 배경
        svg.fill_rect_float(
            RectImpl { x: 0.0, y: 0.0, w: page_w, h: page_h },
            &Brush::Solid(SolidBrush::new(Color::from_rgb(255, 255, 255, std::ptr::null_mut()))),
        );

        let mut counts = Counts::default();
        traverse(&tree.root, &mut svg, &cache_arc, &mut counts);
        let svg_str = svg.finish();

        // SVG 저장 (디버깅)
        let _ = std::fs::write(
            out_dir.join(format!("{}__{}_ours.svg", subdir, stem)),
            &svg_str,
        );

        // 4) resvg → ours PNG (GT 크기)
        let Some(our_rgba) = rasterize_svg(&svg_str, gt_w, gt_h) else {
            eprintln!("  rasterize FAIL");
            continue;
        };
        write_png_rgba(
            &out_dir.join(format!("{}__{}_ours.png", subdir, stem)),
            &our_rgba, gt_w, gt_h,
        );

        // 5) pixel-diff (전체 + ink-only)
        let opts = DiffOptions::default();
        let score = match score_pages(&our_rgba, &gt_rgba, gt_w, gt_h, &opts) {
            Ok(s) => s,
            Err(e) => { eprintln!("  score FAIL: {e}"); continue; }
        };
        let ink_score = match score_pages_ink_only(&our_rgba, &gt_rgba, gt_w, gt_h, &opts, 200) {
            Ok(s) => s,
            Err(e) => { eprintln!("  ink score FAIL: {e}"); continue; }
        };
        // alignment-tolerant ink IoU (dilate 2px)
        let iou2 = score_pages_ink_iou(&our_rgba, &gt_rgba, gt_w, gt_h, 200, 2).unwrap_or((0.0,0,0,0,0));
        let iou5 = score_pages_ink_iou(&our_rgba, &gt_rgba, gt_w, gt_h, 200, 5).unwrap_or((0.0,0,0,0,0));
        // heatmap
        if let Ok(heat) = make_heatmap_rgba(&our_rgba, &gt_rgba, gt_w, gt_h, &opts) {
            write_png_rgba(
                &out_dir.join(format!("{}__{}_heatmap.png", subdir, stem)),
                &heat, gt_w, gt_h,
            );
        }

        // CSV row
        println!(
            "{},{},{},{},{:.1},{:.1},{},{},{},{},{},{},{},{},{},{:.2},{},{:.2},{:.2},{},{},{:.2},{:.2},{:.2},{},{}",
            csv_esc(subdir), csv_esc(stem), gt_w, gt_h, page_w, page_h,
            counts.text_run, counts.text_rendered, counts.text_skipped,
            counts.rectangle, counts.line, counts.image, counts.path,
            counts.equation, counts.group,
            score.score_pct, score.mismatched_pixels, score.avg_delta,
            ink_score.score_pct, ink_score.total_pixels, ink_score.matched_pixels, ink_score.avg_delta,
            iou2.0, iou5.0, iou2.1, iou2.2,
        );
        all_scores.push((subdir.clone(), stem.clone(), score.score_pct, ink_score.score_pct, iou2.0, iou5.0));
    }

    // 집계
    if !all_scores.is_empty() {
        let avg_full = all_scores.iter().map(|(_,_,s,_,_,_)| s).sum::<f32>() / all_scores.len() as f32;
        let avg_ink  = all_scores.iter().map(|(_,_,_,s,_,_)| s).sum::<f32>() / all_scores.len() as f32;
        let avg_iou2 = all_scores.iter().map(|(_,_,_,_,s,_)| s).sum::<f32>() / all_scores.len() as f32;
        let avg_iou5 = all_scores.iter().map(|(_,_,_,_,_,s)| s).sum::<f32>() / all_scores.len() as f32;
        let worst_iou2 = all_scores.iter()
            .min_by(|a, b| a.4.partial_cmp(&b.4).unwrap()).unwrap();
        let best_iou2 = all_scores.iter()
            .max_by(|a, b| a.4.partial_cmp(&b.4).unwrap()).unwrap();
        eprintln!();
        eprintln!("=== aggregate vs real Hancom GT ===");
        eprintln!("  pairs : {}", all_scores.len());
        eprintln!();
        eprintln!("  full score (with white-bg match):       avg {:.2}%", avg_full);
        eprintln!("  ink-only score (strict, no tolerance):  avg {:.2}%", avg_ink);
        eprintln!("  ink IoU (dilate 2px, alignment-tol):    avg {:.2}%", avg_iou2);
        eprintln!("  ink IoU (dilate 5px, very loose):       avg {:.2}%", avg_iou5);
        eprintln!();
        eprintln!("  IoU dilate-2px:");
        eprintln!("    worst: {}/{}  {:.2}%", worst_iou2.0, worst_iou2.1, worst_iou2.4);
        eprintln!("    best : {}/{}  {:.2}%", best_iou2.0, best_iou2.1, best_iou2.4);
        let (h, m) = rhwp::renderer::kdsnr_hft_global::stats();
        eprintln!();
        eprintln!("  kdsnr-hft hook: hit={} miss={} (hit_ratio={:.1}%)",
            h, m, if h + m > 0 { h as f64 * 100.0 / (h + m) as f64 } else { 0.0 });
    }
}

fn csv_esc(s: &str) -> String {
    if s.contains(',') || s.contains('"') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else { s.to_string() }
}

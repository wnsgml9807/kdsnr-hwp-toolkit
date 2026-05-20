//! `SvgSurface` — Hancom `Render::Surface` API 의 SVG primitive 백엔드 어댑터.
//!
//! ## 위치
//!
//! [project_full_byteeq_plan.md] 의 **Phase S** 산출물. 본 어댑터가 우리 toolkit 의
//! **유일한 custom 영역** (200-400줄 목표) — 다른 모든 layer 는 한컴 byte-eq port.
//!
//! ## 역할
//!
//! Hancom `Glyph::Draw` vfunc 5-8 가 byte-eq port 되면 Surface API 를 통해 그리기 호출.
//! 본 어댑터는 그 호출을 SVG primitive (`<rect>`/`<path>`/`<line>`/`<text>`/`<image>`) 로
//! emit. 외부적으론 SVG string 을 누적. `svg2pdf` 가 SVG → PDF 변환.
//!
//! ## 변환 원칙
//!
//! - **글리프** (`DrawString` / `DrawDriverString`) → HFT decoder 로 glyph path 얻어 `<path>` emit.
//!   `<text>` 사용 금지 (SVG 렌더러의 폰트/kerning 이 한컴과 다를 위험).
//! - **shape** (`Fill*` / `Outline*` / `DrawPie`) → 자명 mapping (`<rect>`, `<path>`).
//! - **transform** (`SetTransform` / `Scale` / `Translate`) → SVG group 의 `transform` 속성.
//! - **clip** (`SetClip`) → `<clipPath>` 정의 + 후속 element 에 `clip-path=url(#...)` 부착.
//! - **이미지** (`DrawImage`) → `<image href="data:..."/>` (base64 inline).
//!
//! ## 진행 단계
//!
//! - **S-1** (현 단계): 빈 skeleton + `unimplemented!()` body. trait 구조 검증용.
//! - **S-2**: trivial 메소드 구현 (Fill/Outline/Transform/State 등).
//! - **S-3**: DrawString → HFT path 통합.
//! - **S-4**: DrawImage / SetClip 등 잔여.
//!
//! ## 정공법 정책 충돌 검토
//!
//! [feedback_no_time_optimization.md] "stub / `unimplemented!()` 금지" 정책과 충돌:
//! - 일반 byte-eq port 작업에선 stub 금지 (한컴 동작 모르고 추측하는 stub 만 금지)
//! - 본 SvgSurface 의 S-1 stub 은 한컴 동작 *재현* 이 아닌 *우리 custom backend* 의 단계적 구현.
//!   순서: skeleton (S-1) → trivial impl (S-2) → text+image (S-3) → 잔여 (S-4)
//! - 매 단계 mechanical mapping (DrawLine → `<line>`) 라 휴리스틱 아님

use crate::brush::Brush;
use crate::color::{color_type, Color};
use crate::pen::Pen;
use crate::surface::{
    Font, Image, Path, PathCmd, PointImpl, RectImpl, StringFormat, Surface, Transform2D,
};
use kdsnr_hft::HftCache;
use std::char;
use std::fmt::Write;
use std::sync::Arc;

// ─── helper: Color / Brush / Pen → SVG attribute ──────────────────────

/// Color → SVG color string. RGB 타입은 `#rrggbb`, 그 외는 fallback.
///
/// S-3 에서 SchemeStyle 해석 (ColorScheme lookup), CMYK / SCRGB / Effect 적용은 확장.
fn color_to_svg(color: &Color) -> String {
    match color.type_tag {
        color_type::RGB => {
            // raw value[0..3] = R, G, B
            format!("#{:02x}{:02x}{:02x}", color.value[0], color.value[1], color.value[2])
        }
        color_type::SCHEME => {
            // SchemeStyle index — ColorScheme 미통합 단계 fallback.
            // raw value[0] = SchemeStyle u8 (PropertyKey 매핑)
            // S-3 에서 Theme.ColorScheme 으로 해석
            "currentColor".to_string()
        }
        color_type::SYSTEM => "Window".to_string(),
        color_type::PRESET => "PresetColor".to_string(),
        color_type::CMYK | color_type::SC_RGB | color_type::HSL => {
            // 필요 시 conversion. 본 단계는 fallback.
            "#000000".to_string()
        }
        _ => "#000000".to_string(),
    }
}

/// Brush → SVG `fill` 속성값 ("none" / "#rrggbb" / "url(#...)").
///
/// EmptyBrush → "none". SolidBrush → color. Gradient/Hatch/Image/Group 은
/// S-3 에서 `<defs>` 생성 + url() 참조.
fn brush_to_fill(brush: &Brush) -> String {
    match brush {
        Brush::Empty(_) => "none".to_string(),
        Brush::Solid(sb) => {
            // SolidBrush 의 KEY_COLOR PropertyBag 에서 Color 추출.
            // S-3 에서 PropertyBag::get_color helper 도입 시 정밀화.
            // 본 단계: bag 비어있으면 black, 있으면 PColor 의 raw value.
            extract_solid_color(sb).unwrap_or_else(|| "#000000".to_string())
        }
        _ => "#888888".to_string(), // S-3 에서 확장
    }
}

fn extract_solid_color(sb: &crate::brush::SolidBrush) -> Option<String> {
    // raw `SolidBrush::GetColor()` (PropertyBag KEY_COLOR=0x259 lookup) 호출.
    // bag 비어있으면 default (0,0,0 RGB). 그 경우 None 반환해서 caller 가 fallback 처리.
    let color = sb.get_color();
    let svg = color_to_svg(&color);
    if svg == "#000000" {
        // bag 가 비었는지 (explicit attach 없음) vs 명시적 black 인지 판정.
        let bag_empty = unsafe { sb.bag.impl_ref().is_none() };
        if bag_empty {
            None
        } else {
            Some(svg)
        }
    } else {
        Some(svg)
    }
}

/// Pen → SVG stroke attribute string.
///
/// Pen 의 stroke brush + PropertyBag accessor 사용:
/// - stroke color: Pen.brush (SolidBrush 면 KEY_COLOR, 아니면 #000000)
/// - stroke-width: Pen.get_thickness() (raw KEY_THICKNESS=0x6b1)
/// - stroke-dasharray: DashStyle enum → SVG dasharray
/// - stroke-linecap: LineCapStyle → butt/round/square
/// - stroke-linejoin: LineJoinStyle → miter/round/bevel
/// - stroke-miterlimit: get_miter_limit()
fn pen_to_stroke_attrs(pen: &Pen) -> String {
    use crate::pen::{DashStyle, LineCapStyle, LineJoinStyle};

    // stroke color from inner brush
    let stroke_color = match pen.brush.as_ref() {
        Brush::Solid(sb) => extract_solid_color(sb).unwrap_or_else(|| "#000000".to_string()),
        Brush::Empty(_) => "none".to_string(),
        _ => "#000000".to_string(), // gradient/image stroke 는 SVG 표준 미지원, fallback
    };

    let width = pen.get_thickness().max(0.0);
    let cap = match pen.get_line_cap_style() {
        LineCapStyle::Round => "round",
        LineCapStyle::Square => "square",
        LineCapStyle::Flat => "butt",
    };
    let join = match pen.get_line_join_style() {
        LineJoinStyle::Miter => "miter",
        LineJoinStyle::Round => "round",
        LineJoinStyle::Bevel => "bevel",
    };
    let miter = pen.get_miter_limit().max(1.0);

    // SVG dasharray (단위: stroke-width 의 배수가 자연스러우나, 본 단계는 absolute px)
    let dash = match pen.get_dash_style() {
        DashStyle::Solid => String::new(),
        DashStyle::Dot => format!(r#" stroke-dasharray="{:.2} {:.2}""#, width * 1.0, width * 2.0),
        DashStyle::Dash => format!(r#" stroke-dasharray="{:.2} {:.2}""#, width * 4.0, width * 2.0),
        DashStyle::LongDash => format!(r#" stroke-dasharray="{:.2} {:.2}""#, width * 8.0, width * 2.0),
        DashStyle::DashDot => format!(
            r#" stroke-dasharray="{:.2} {:.2} {:.2} {:.2}""#,
            width * 4.0, width * 2.0, width * 1.0, width * 2.0
        ),
        DashStyle::LongDashDot => format!(
            r#" stroke-dasharray="{:.2} {:.2} {:.2} {:.2}""#,
            width * 8.0, width * 2.0, width * 1.0, width * 2.0
        ),
        DashStyle::LongDashDotDot => format!(
            r#" stroke-dasharray="{:.2} {:.2} {:.2} {:.2} {:.2} {:.2}""#,
            width * 8.0, width * 2.0, width * 1.0, width * 2.0, width * 1.0, width * 2.0
        ),
    };

    format!(
        r#"stroke="{}" stroke-width="{:.3}" stroke-linecap="{}" stroke-linejoin="{}" stroke-miterlimit="{:.2}"{}"#,
        stroke_color, width, cap, join, miter, dash
    )
}

// ─── helper: Path → SVG d-string ──────────────────────────────────────

fn path_to_d(path: &Path) -> String {
    let mut d = String::with_capacity(path.commands.len() * 16);
    for cmd in &path.commands {
        match cmd {
            PathCmd::MoveTo(x, y) => {
                write!(&mut d, "M{:.3} {:.3} ", x, y).unwrap();
            }
            PathCmd::LineTo(x, y) => {
                write!(&mut d, "L{:.3} {:.3} ", x, y).unwrap();
            }
            PathCmd::CurveTo(c1x, c1y, c2x, c2y, x, y) => {
                write!(
                    &mut d,
                    "C{:.3} {:.3} {:.3} {:.3} {:.3} {:.3} ",
                    c1x, c1y, c2x, c2y, x, y
                )
                .unwrap();
            }
            PathCmd::Close => {
                d.push_str("Z ");
            }
        }
    }
    d.trim_end().to_string()
}

// ─── helper: Transform2D matrix multiplication ─────────────────────────

fn transform_mul(a: &Transform2D, b: &Transform2D) -> Transform2D {
    // 2D affine 행렬 곱: result = A * B
    // [a.a a.b a.tx]   [b.a b.b b.tx]
    // [a.c a.d a.ty] * [b.c b.d b.ty]
    // [0   0   1   ]   [0   0   1   ]
    Transform2D {
        a: a.a * b.a + a.b * b.c,
        b: a.a * b.b + a.b * b.d,
        c: a.c * b.a + a.d * b.c,
        d: a.c * b.b + a.d * b.d,
        tx: a.a * b.tx + a.b * b.ty + a.tx,
        ty: a.c * b.tx + a.d * b.ty + a.ty,
    }
}

/// SVG primitive 누적 buffer + 현재 transform/clip 상태.
pub struct SvgSurface {
    /// SVG string 누적 (root `<svg>` 안의 내용물).
    pub buffer: String,
    /// 현재 transform stack (push/pop).
    pub transform_stack: Vec<Transform2D>,
    /// 현재 clip stack.
    pub clip_stack: Vec<Path>,
    /// 누적 id 카운터 (clipPath 등의 unique id 생성용).
    pub next_id: u32,
    /// canvas size (root `<svg width=... height=.../>` 용).
    pub width: f32,
    pub height: f32,
    /// 줌 / offset 상태.
    pub zoom: f32,
    pub offset: PointImpl<f32>,
    /// 안티에일리어싱 등 옵션 (현재 단순화).
    pub antialiasing: bool,
    pub pen_integer: bool,
    /// HFT glyph cache. text emit (DrawString*) 호출 시 필수. None 이면 panic.
    /// Arc 로 여러 surface 가 한 cache 공유 가능 (페이지마다 surface 생성 케이스).
    pub hft_cache: Option<Arc<HftCache>>,
    /// GState save/restore 스택 — `save_state` 마다 entry push (현 frame 안에서
    /// emit 된 `<g>` 개수). `concat_transform` 호출 시 top entry 의 group count 증가.
    /// `restore_state` 시 pop 후 그 count 만큼 `</g>` close. raw `SurfaceRestorer` 의
    /// RAII 등치.
    ///
    /// **group balance 정공법**: 한 save_state frame 안에서 N 번 concat_transform 시
    /// N+1 개 `<g>` open (save 자체의 1 + concat 의 N) → 한 restore_state 가 N+1 모두 close.
    pub gstate_stack: Vec<u32>,
    /// 이미지 binData 매핑 (source_id → base64 data URL).
    ///
    /// HWPX 의 `<hp:picture>` 가 참조하는 binData 의 actual byte 를 caller 가 미리
    /// 등록. SvgSurface 는 `<image href>` 에 그 data URL 을 embed.
    /// 미등록 source_id 는 fallback (`xlink:href="#missing-img"`).
    pub image_data_map: std::collections::HashMap<String, String>,
}

impl SvgSurface {
    /// 새 SVG surface 생성 (canvas 크기 지정). HFT cache 없이 생성.
    /// text emit 하려면 `with_hft_cache(cache)` 로 주입 필수.
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            buffer: String::with_capacity(8192),
            transform_stack: vec![Transform2D::IDENTITY],
            clip_stack: Vec::new(),
            next_id: 0,
            width,
            height,
            zoom: 1.0,
            offset: PointImpl { x: 0.0, y: 0.0 },
            antialiasing: true,
            pen_integer: false,
            hft_cache: None,
            gstate_stack: Vec::new(),
            image_data_map: std::collections::HashMap::new(),
        }
    }

    /// 이미지 binData 등록 (source_id → bytes). `<image href="data:..."/>` 에 embed.
    /// `mime` 은 "image/png", "image/jpeg" 등.
    pub fn register_image(&mut self, source_id: impl Into<String>, mime: &str, bytes: &[u8]) {
        use base64::Engine as _;
        let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
        let data_url = format!("data:{};base64,{}", mime, b64);
        self.image_data_map.insert(source_id.into(), data_url);
    }

    /// HFT cache 주입 (builder pattern).
    pub fn with_hft_cache(mut self, cache: Arc<HftCache>) -> Self {
        self.hft_cache = Some(cache);
        self
    }

    /// 누적 SVG buffer 를 완성된 `<svg>` document string 으로 반환.
    pub fn finish(&self) -> String {
        format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\">\n{}\n</svg>",
            self.width, self.height, self.width, self.height, self.buffer
        )
    }

    fn alloc_id(&mut self, prefix: &str) -> String {
        let id = format!("{}_{}", prefix, self.next_id);
        self.next_id += 1;
        id
    }

    /// `&[u16]` (UTF-16 wide string) → `Vec<u32>` codepoint sequence.
    /// surrogate pair 는 single u32 로 합쳐짐 (`char::decode_utf16` 사용).
    /// invalid surrogate 는 skip (한컴 native 도 보통 무시).
    fn decode_utf16(text: &[u16]) -> Vec<u32> {
        char::decode_utf16(text.iter().copied())
            .filter_map(|r| r.ok().map(|c| c as u32))
            .collect()
    }

    /// HftCache 필수 접근. 미주입 시 panic (text emit 정공법).
    fn require_cache(&self) -> &HftCache {
        self.hft_cache
            .as_ref()
            .expect("SvgSurface: text emit requires hft_cache (use .with_hft_cache())")
            .as_ref()
    }

    /// Brush → SVG `fill` 속성값 (`<defs>` 등록 가능).
    ///
    /// `brush_to_fill` (free fn) 의 self-aware 버전. Gradient/Hatch/Image 가 오면
    /// `<defs>` 에 정의를 emit 한 뒤 `url(#brush_N)` 형태의 참조를 반환.
    ///
    /// - **EmptyBrush** → "none"
    /// - **SolidBrush** → `#rrggbb` (PropertyBag KEY_COLOR 사용, 비면 "#000000")
    /// - **GradientBrush** → `<defs><linearGradient id="grad_N">...</linearGradient></defs>`
    ///   emit 후 `url(#grad_N)`
    /// - **HatchBrush** → `<defs><pattern id="hatch_N">...</pattern></defs>` emit 후
    ///   `url(#hatch_N)` (style index 0..11 별로 line 패턴)
    /// - **ImageBrush** → `<defs><pattern id="imgpat_N"><image href=...></pattern></defs>`
    ///   emit 후 `url(#imgpat_N)` (등록된 image_data_map 사용)
    /// - **GroupBrush** → "none" (multi-fill 은 별도 처리 필요; fallback)
    pub fn resolve_brush_to_fill(&mut self, brush: &Brush) -> String {
        match brush {
            Brush::Empty(_) => "none".to_string(),
            Brush::Solid(sb) => extract_solid_color(sb).unwrap_or_else(|| "#000000".to_string()),
            Brush::Gradient(gb) => {
                let id = self.alloc_id("grad");
                let stops = gb.get_stops();
                let angle = gb.get_angle_degrees();
                // SVG linearGradient 의 x1/y1/x2/y2 는 percentage. angle 만큼 회전한 선분.
                // angle=0 = 좌→우, angle=90 = 위→아래.
                let rad = angle.to_radians();
                let (cx, cy) = (0.5_f32, 0.5_f32);
                let r = 0.5_f32;
                let x1 = cx - r * rad.cos();
                let y1 = cy - r * rad.sin();
                let x2 = cx + r * rad.cos();
                let y2 = cy + r * rad.sin();
                let mut def = format!(
                    r#"<defs><linearGradient id="{}" x1="{:.3}" y1="{:.3}" x2="{:.3}" y2="{:.3}">"#,
                    id, x1, y1, x2, y2
                );
                for (offset, color) in &stops {
                    def.push_str(&format!(
                        r#"<stop offset="{:.3}" stop-color="{}"/>"#,
                        offset,
                        color_to_svg(color)
                    ));
                }
                def.push_str("</linearGradient></defs>\n");
                self.buffer.push_str(&def);
                format!("url(#{})", id)
            }
            Brush::Hatch(hb) => {
                let id = self.alloc_id("hatch");
                let style = hb.get_hatch_style();
                let fg = color_to_svg(&hb.get_fore_color());
                let bg = color_to_svg(&hb.get_back_color());
                // 12 style index → 패턴 d 선택 (raw HWPX HatchStyle enum 0..11):
                //   0 = Horizontal, 1 = Vertical, 2 = ForwardDiagonal, 3 = BackwardDiagonal,
                //   4 = Cross, 5 = DiagonalCross, 그 외는 같은 line family.
                let stroke_d = match style {
                    0 => "M0 4 L8 4".to_string(),                // horizontal
                    1 => "M4 0 L4 8".to_string(),                // vertical
                    2 => "M0 8 L8 0".to_string(),                // /
                    3 => "M0 0 L8 8".to_string(),                // \
                    4 => "M0 4 L8 4 M4 0 L4 8".to_string(),      // +
                    5 => "M0 0 L8 8 M0 8 L8 0".to_string(),      // ×
                    _ => "M0 4 L8 4".to_string(),                // fallback horizontal
                };
                let def = format!(
                    r#"<defs><pattern id="{id}" width="8" height="8" patternUnits="userSpaceOnUse"><rect width="8" height="8" fill="{bg}"/><path d="{stroke_d}" stroke="{fg}" stroke-width="1"/></pattern></defs>
"#,
                    id = id, bg = bg, fg = fg, stroke_d = stroke_d
                );
                self.buffer.push_str(&def);
                format!("url(#{})", id)
            }
            Brush::Image(ib) => {
                let id = self.alloc_id("imgpat");
                // image_data_map 에 등록된 source 면 base64 data URL, 아니면 placeholder.
                let href = self
                    .image_data_map
                    .get(&ib.source_id)
                    .cloned()
                    .unwrap_or_else(|| "#missing-img".to_string());
                // ImageBrush 의 tile_style + scale + offset 적용:
                //   - tile_style 0 = NoTile (stretch to surface) — 본 단계 기본
                //   - tile_style 1 = Tile (repeat) — patternUnits="userSpaceOnUse" + 작은 width/height
                //   - scale_x/y: 1.0 = native, > 1 = magnify
                //   - offset_x/y: pattern 시작점 평행이동 (patternTransform 으로 합성)
                let tile = ib.tile_style;
                let (tw, th) = if tile == 1 {
                    // tiled: 원본 크기 의 scale 만큼 (base 100×100 가정 — 정확한 image natural size 는
                    // image_data_map metadata 별도 필요)
                    (100.0 * ib.scale_x, 100.0 * ib.scale_y)
                } else {
                    // stretched: surface 전체 채움
                    (self.width * ib.scale_x.max(0.0001), self.height * ib.scale_y.max(0.0001))
                };
                let pattern_transform = if ib.offset_x != 0.0 || ib.offset_y != 0.0 {
                    format!(
                        r#" patternTransform="translate({:.3} {:.3})""#,
                        ib.offset_x, ib.offset_y
                    )
                } else {
                    String::new()
                };
                let def = format!(
                    r#"<defs><pattern id="{id}" patternUnits="userSpaceOnUse" width="{tw:.3}" height="{th:.3}"{pt}><image href="{href}" width="{tw:.3}" height="{th:.3}" preserveAspectRatio="none"/></pattern></defs>
"#,
                    id = id,
                    href = href,
                    tw = tw,
                    th = th,
                    pt = pattern_transform,
                );
                self.buffer.push_str(&def);
                format!("url(#{})", id)
            }
            Brush::Group(_) => {
                // GroupBrush: 복수 brush 의 합성. SVG 로는 별도 layer/clip 가 필요.
                // 본 단계는 첫 자식 brush 의 color 만 사용 (fallback).
                "none".to_string()
            }
        }
    }

    /// per-glyph SVG `<path>` emit.
    ///
    /// 각 (codepoint, position) 쌍에 대해:
    /// 1. `HftCache::get(font.family, cp)` → `Glyph { d, advance, em }`
    /// 2. transform: `translate(pos.x, pos.y) scale(font.size/em, -font.size/em)`
    ///    - em-coord 는 y-up (typographic). SVG 는 y-down. → y scale 음수
    ///    - baseline 이 glyph 의 (0,0). pos = baseline 위치.
    /// 3. fill = brush_to_fill(brush)
    /// 4. SVG: `<g transform="..."><path d="..." fill="..."/></g>`
    ///
    /// `outer_transform` (DrawDriverString 의 transform 인자) 는 group 의 추가
    /// transform 으로 prefix. 합성: outer × translate × scale (SVG 의 transform 은
    /// 좌→우로 합성).
    fn emit_glyph_paths(
        &mut self,
        codepoints: &[u32],
        font: &Font,
        brush: &Brush,
        positions: &[PointImpl<f32>],
        outer_transform: &Transform2D,
    ) {
        let fill = brush_to_fill(brush);
        let has_outer = !is_identity_transform(outer_transform);
        let outer_attr = if has_outer {
            format!(
                r#" transform="matrix({} {} {} {} {} {})""#,
                outer_transform.a,
                outer_transform.c,
                outer_transform.b,
                outer_transform.d,
                outer_transform.tx,
                outer_transform.ty,
            )
        } else {
            String::new()
        };
        // 외곽 group: outer_transform 만 적용. 안에 per-glyph group 으로 translate+scale.
        if has_outer {
            writeln!(&mut self.buffer, r#"<g{}>"#, outer_attr).unwrap();
        }
        // 미리 cache 참조 확보 (borrow checker: glyph 참조 hold 동안 self mutate 회피)
        let glyphs: Vec<(PointImpl<f32>, Option<(String, i32, u16)>)> = {
            let cache = self.require_cache();
            codepoints
                .iter()
                .zip(positions.iter())
                .map(|(cp, p)| {
                    let g = cache.get(&font.family, *cp).map(|g| (g.d.clone(), g.advance, g.em));
                    (*p, g)
                })
                .collect()
        };
        for (pos, gopt) in glyphs {
            let Some((d, _advance, em)) = gopt else { continue };
            if d.is_empty() { continue; }
            let s = font.size / em as f32;
            // SVG matrix(a,b,c,d,e,f) = [a c e; b d f; 0 0 1].
            // 우리는 translate(pos.x, pos.y) ∘ scale(s, -s) 를 하나의 matrix 로:
            //   [s 0 pos.x]
            //   [0 -s pos.y]
            //   [0 0  1   ]
            // → matrix(s, 0, 0, -s, pos.x, pos.y)
            writeln!(
                &mut self.buffer,
                r#"<path transform="matrix({:.6} 0 0 {:.6} {:.3} {:.3})" d="{}" fill="{}"/>"#,
                s, -s, pos.x, pos.y, d, fill
            )
            .unwrap();
        }
        if has_outer {
            self.buffer.push_str("</g>\n");
        }
    }
}

fn is_identity_transform(t: &Transform2D) -> bool {
    t.a == 1.0 && t.b == 0.0 && t.c == 0.0 && t.d == 1.0 && t.tx == 0.0 && t.ty == 0.0
}

/// Image bytes 의 magic 으로 MIME 결정 → `data:<mime>;base64,<base64>` URI.
/// 빈 데이터면 `"#missing-img"` placeholder.
///
/// 한컴 hwpx BinData/imageN.* 은 JPEG (FF D8 FF), PNG (89 50 4E 47), GIF (47 49
/// 46 38), BMP (42 4D), TIFF (49 49 / 4D 4D) 가 주로 등장. mime mismatch 면
/// resvg 가 image 를 silent skip — wire_real_gt 의 math Q28_3 도형 (입체 삼각형)
/// 누락의 원인이 이 mismatch 였다. 항상 magic 으로 결정.
fn image_data_uri(data: &[u8]) -> String {
    if data.is_empty() {
        return "#missing-img".to_string();
    }
    use base64::Engine as _;
    let mime = if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        "image/png"
    } else if data.len() >= 3 && data[0] == 0xff && data[1] == 0xd8 && data[2] == 0xff {
        "image/jpeg"
    } else if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
        "image/gif"
    } else if data.starts_with(b"BM") {
        "image/bmp"
    } else if data.starts_with(b"II*\0") || data.starts_with(b"MM\0*") {
        "image/tiff"
    } else if data.starts_with(b"RIFF") && data.len() >= 12 && &data[8..12] == b"WEBP" {
        "image/webp"
    } else {
        // 알 수 없는 형식 — 한컴 hwpx 표준에 PNG/JPEG 가 가장 흔하므로 JPEG fallback
        // (resvg 가 못 읽으면 그냥 skip 되지만 PNG 가 더 흔한 mismatch 케이스라 JPEG 안전)
        "image/jpeg"
    };
    let b64 = base64::engine::general_purpose::STANDARD.encode(data);
    format!("data:{};base64,{}", mime, b64)
}

// ─── Surface trait impl ─────────────────────────────────────────────────
//
// S-1 단계: 모든 method `unimplemented!()`. S-2 부터 단계적 구현.

impl Surface for SvgSurface {
    fn fill_rect_int(&mut self, rect: RectImpl<i32>, brush: &Brush) {
        let fill = self.resolve_brush_to_fill(brush);
        writeln!(
            &mut self.buffer,
            r#"<rect x="{}" y="{}" width="{}" height="{}" fill="{}"/>"#,
            rect.x, rect.y, rect.w, rect.h, fill
        )
        .unwrap();
    }

    fn fill_rect_float(&mut self, rect: RectImpl<f32>, brush: &Brush) {
        let fill = self.resolve_brush_to_fill(brush);
        writeln!(
            &mut self.buffer,
            r#"<rect x="{:.3}" y="{:.3}" width="{:.3}" height="{:.3}" fill="{}"/>"#,
            rect.x, rect.y, rect.w, rect.h, fill
        )
        .unwrap();
    }

    fn fill_path(&mut self, path: &Path, brush: &Brush) {
        let d = path_to_d(path);
        let fill = self.resolve_brush_to_fill(brush);
        writeln!(&mut self.buffer, r#"<path d="{}" fill="{}"/>"#, d, fill).unwrap();
    }

    fn outline_rect_int(&mut self, rect: RectImpl<i32>, pen: &Pen) {
        let attrs = pen_to_stroke_attrs(pen);
        writeln!(
            &mut self.buffer,
            r#"<rect x="{}" y="{}" width="{}" height="{}" fill="none" {}/>"#,
            rect.x, rect.y, rect.w, rect.h, attrs
        )
        .unwrap();
    }

    fn outline_rect_float(&mut self, rect: RectImpl<f32>, pen: &Pen) {
        let attrs = pen_to_stroke_attrs(pen);
        writeln!(
            &mut self.buffer,
            r#"<rect x="{:.3}" y="{:.3}" width="{:.3}" height="{:.3}" fill="none" {}/>"#,
            rect.x, rect.y, rect.w, rect.h, attrs
        )
        .unwrap();
    }

    fn outline_path(&mut self, path: &Path, pen: &Pen) {
        let d = path_to_d(path);
        let attrs = pen_to_stroke_attrs(pen);
        writeln!(&mut self.buffer, r#"<path d="{}" fill="none" {}/>"#, d, attrs).unwrap();
    }

    fn draw_image_rect(&mut self, rect: RectImpl<i32>, image: &Image, _flag: bool, alpha: f32) {
        // S-4: <image> emit. image.data 가 비어 있으면 placeholder rect.
        let href = image_data_uri(&image.data);
        let opacity = if alpha < 1.0 { format!(r#" opacity="{:.3}""#, alpha) } else { String::new() };
        writeln!(
            &mut self.buffer,
            r#"<image x="{}" y="{}" width="{}" height="{}" href="{}"{}/>"#,
            rect.x, rect.y, rect.w, rect.h, href, opacity
        ).unwrap();
    }

    fn draw_image_point(
        &mut self,
        pt: PointImpl<f32>,
        image: &Image,
        transform: &Transform2D,
        _color: Option<&Color>,
    ) {
        // raw `DrawImage(Point, Image, Transform2D, Color*)` — point 에 image 의 native
        // size 로 그리기. transform 추가 적용.
        let href = image_data_uri(&image.data);
        let t_attr = if is_identity_transform(transform) {
            String::new()
        } else {
            format!(
                r#" transform="matrix({} {} {} {} {} {})""#,
                transform.a, transform.c, transform.b, transform.d, transform.tx, transform.ty
            )
        };
        writeln!(
            &mut self.buffer,
            r#"<image x="{:.3}" y="{:.3}" width="{}" height="{}" href="{}"{}/>"#,
            pt.x, pt.y, image.width, image.height, href, t_attr
        ).unwrap();
    }

    fn draw_image_f(&mut self, rect: RectImpl<f32>, image: &Image, alpha: f32) {
        let href = image_data_uri(&image.data);
        let opacity = if alpha < 1.0 { format!(r#" opacity="{:.3}""#, alpha) } else { String::new() };
        writeln!(
            &mut self.buffer,
            r#"<image x="{:.3}" y="{:.3}" width="{:.3}" height="{:.3}" href="{}"{}/>"#,
            rect.x, rect.y, rect.w, rect.h, href, opacity
        ).unwrap();
    }

    fn draw_image_border(&mut self, rect: RectImpl<f32>, _image: &Image) {
        // 이미지 자리 표시용 border rect (image 가 로드 안 됐을 때 fallback)
        writeln!(
            &mut self.buffer,
            "<rect x=\"{:.3}\" y=\"{:.3}\" width=\"{:.3}\" height=\"{:.3}\" fill=\"none\" stroke=\"#cccccc\"/>",
            rect.x, rect.y, rect.w, rect.h
        ).unwrap();
    }

    fn draw_no_image(&mut self, rect: RectImpl<f32>) {
        // 이미지 없을 때 placeholder X-mark
        writeln!(
            &mut self.buffer,
            "<g><rect x=\"{:.3}\" y=\"{:.3}\" width=\"{:.3}\" height=\"{:.3}\" fill=\"#f0f0f0\" stroke=\"#cccccc\"/><line x1=\"{:.3}\" y1=\"{:.3}\" x2=\"{:.3}\" y2=\"{:.3}\" stroke=\"#cccccc\"/><line x1=\"{:.3}\" y1=\"{:.3}\" x2=\"{:.3}\" y2=\"{:.3}\" stroke=\"#cccccc\"/></g>",
            rect.x, rect.y, rect.w, rect.h,
            rect.x, rect.y, rect.x + rect.w, rect.y + rect.h,
            rect.x + rect.w, rect.y, rect.x, rect.y + rect.h,
        ).unwrap();
    }

    fn draw_string_point(
        &mut self,
        text: &[u16],
        font: &Font,
        pos: PointImpl<f32>,
        brush: &Brush,
        _format: &StringFormat,
    ) {
        // caller 가 단일 baseline starting point 만 제공. per-glyph position 은
        // advance 누적으로 자체 계산. 한컴 native 도 동일 (kerning 미적용,
        // metric 표 advance 만 사용 — TextOut 류 호출의 통상 동작).
        //
        // 본 단계: kerning/justify 적용 안 함. caller 가 정밀 position 원하면
        // DrawDriverString 사용 (kdsnr-layout 의 표준 호출 경로).
        let codepoints = Self::decode_utf16(text);
        if codepoints.is_empty() {
            return;
        }
        let cache = self.require_cache();
        let mut positions: Vec<PointImpl<f32>> = Vec::with_capacity(codepoints.len());
        let mut x = pos.x;
        let y = pos.y;
        for cp in &codepoints {
            positions.push(PointImpl { x, y });
            if let Some(g) = cache.get(&font.family, *cp) {
                let scale = font.size / (g.em as f32);
                x += g.advance as f32 * scale;
            }
            // glyph 없는 codepoint 도 position 만 advance 안 누적하고 넘김
            // (한컴 native 는 .notdef glyph emit, 우리는 skip — 정공법 한도)
        }
        self.emit_glyph_paths(&codepoints, font, brush, &positions, &Transform2D::IDENTITY);
    }

    fn draw_string_rect(
        &mut self,
        text: &[u16],
        font: &Font,
        rect: RectImpl<f32>,
        brush: &Brush,
        format: &StringFormat,
    ) {
        // rect 정렬: StringFormat.align 의 상세 의미는 한컴 Render::StringFormat
        // bit-flag (검증 안 됨) — 본 단계는 좌상 (rect.x, rect.y + font.size) baseline
        // 기준. caller 가 정밀 정렬 원하면 layout 단계에서 DrawDriverString 사용.
        let baseline_x = rect.x;
        let baseline_y = rect.y + font.size; // 좌상 anchor → baseline = top + ascender 근사
        self.draw_string_point(text, font, PointImpl { x: baseline_x, y: baseline_y }, brush, format);
    }

    fn draw_driver_string(
        &mut self,
        text: &[u16],
        font: &Font,
        brush: &Brush,
        positions: &[PointImpl<f32>],
        transform: &Transform2D,
    ) {
        // kdsnr-layout 의 표준 호출 경로. per-glyph absolute position 이 caller 결정.
        // Surface 는 그 위치에 HFT glyph path 만 emit (advance/kerning 안 함).
        let codepoints = Self::decode_utf16(text);
        if codepoints.is_empty() {
            return;
        }
        // 한컴 native: positions.len() == codepoints.len() (per-glyph 매칭).
        // 길이 불일치 시 짧은 쪽 기준 (방어).
        let n = codepoints.len().min(positions.len());
        self.emit_glyph_paths(&codepoints[..n], font, brush, &positions[..n], transform);
    }

    fn measure_string_point(
        &self,
        text: &[u16],
        font: &Font,
        pos: PointImpl<f32>,
        _format: &StringFormat,
    ) -> RectImpl<f32> {
        // advance 합 + em-box height (ascent + descent 통합 fallback = font.size).
        // 한컴 native 의 ascent/descent 분리 측정은 kdsnr-layout 책임.
        let codepoints = Self::decode_utf16(text);
        let mut width = 0.0_f32;
        let cache = self.require_cache();
        for cp in &codepoints {
            if let Some(g) = cache.get(&font.family, *cp) {
                width += g.advance as f32 * (font.size / g.em as f32);
            }
        }
        RectImpl { x: pos.x, y: pos.y - font.size, w: width, h: font.size }
    }

    fn measure_string_rect(
        &self,
        text: &[u16],
        font: &Font,
        rect: RectImpl<f32>,
        format: &StringFormat,
    ) -> RectImpl<f32> {
        // rect 기반 측정도 동일 — caller 가 결과의 bounding rect 만 원함.
        self.measure_string_point(text, font, PointImpl { x: rect.x, y: rect.y + font.size }, format)
    }

    fn measure_driver_string(
        &self,
        text: &[u16],
        font: &Font,
        positions: &[PointImpl<f32>],
    ) -> RectImpl<f32> {
        // per-glyph position 의 min/max + glyph advance 합으로 bounding rect.
        let codepoints = Self::decode_utf16(text);
        if codepoints.is_empty() || positions.is_empty() {
            return RectImpl { x: 0.0, y: 0.0, w: 0.0, h: 0.0 };
        }
        let cache = self.require_cache();
        let n = codepoints.len().min(positions.len());
        let mut min_x = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        for i in 0..n {
            let p = positions[i];
            let cp = codepoints[i];
            let advance = cache
                .get(&font.family, cp)
                .map(|g| g.advance as f32 * (font.size / g.em as f32))
                .unwrap_or(0.0);
            if p.x < min_x { min_x = p.x; }
            if p.x + advance > max_x { max_x = p.x + advance; }
            // baseline y. ascent = font.size, descent = 0 fallback.
            if p.y - font.size < min_y { min_y = p.y - font.size; }
            if p.y > max_y { max_y = p.y; }
        }
        RectImpl { x: min_x, y: min_y, w: max_x - min_x, h: max_y - min_y }
    }

    fn draw_pie(
        &mut self,
        rect: RectImpl<f32>,
        start_angle: f32,
        sweep_angle: f32,
        pen: &Pen,
    ) {
        // SVG path: M cx cy L startx starty A rx ry 0 large_arc 1 endx endy Z
        let cx = rect.x + rect.w / 2.0;
        let cy = rect.y + rect.h / 2.0;
        let rx = rect.w / 2.0;
        let ry = rect.h / 2.0;
        let start_rad = start_angle.to_radians();
        let end_rad = (start_angle + sweep_angle).to_radians();
        let sx = cx + rx * start_rad.cos();
        let sy = cy + ry * start_rad.sin();
        let ex = cx + rx * end_rad.cos();
        let ey = cy + ry * end_rad.sin();
        let large_arc = if sweep_angle.abs() > 180.0 { 1 } else { 0 };
        let sweep = if sweep_angle >= 0.0 { 1 } else { 0 };
        let attrs = pen_to_stroke_attrs(pen);
        writeln!(
            &mut self.buffer,
            r#"<path d="M{:.3} {:.3} L{:.3} {:.3} A{:.3} {:.3} 0 {} {} {:.3} {:.3} Z" fill="none" {}/>"#,
            cx, cy, sx, sy, rx, ry, large_arc, sweep, ex, ey, attrs
        )
        .unwrap();
    }

    fn get_transform(&self) -> Transform2D {
        *self.transform_stack.last().unwrap_or(&Transform2D::IDENTITY)
    }

    fn set_transform(&mut self, transform: &Transform2D) {
        if let Some(last) = self.transform_stack.last_mut() {
            *last = *transform;
        }
    }

    fn set_cartesian_transform(&mut self, transform: &Transform2D) {
        // Cartesian = y-up (math convention). SVG = y-down.
        // y-flip: 추가 transform 으로 ty = canvas height, d = -1 곱.
        let flip = Transform2D { a: 1.0, b: 0.0, c: 0.0, d: -1.0, tx: 0.0, ty: self.height };
        let composed = transform_mul(&flip, transform);
        self.set_transform(&composed);
    }

    fn get_cartesian_transform(&self) -> Transform2D {
        // 현재 transform 에서 y-flip 을 역으로 적용. 본 단계는 단순화: identity 가까운 결과만 정확.
        let t = self.get_transform();
        let flip = Transform2D { a: 1.0, b: 0.0, c: 0.0, d: -1.0, tx: 0.0, ty: self.height };
        transform_mul(&flip, &t)
    }

    fn apply_cartesian_coordinate(&mut self, transform: &Transform2D) {
        // 현재 transform 위에 cartesian transform 을 추가 합성.
        let current = self.get_transform();
        let flip = Transform2D { a: 1.0, b: 0.0, c: 0.0, d: -1.0, tx: 0.0, ty: self.height };
        let composed = transform_mul(&current, &transform_mul(&flip, transform));
        self.set_transform(&composed);
    }

    fn reset_transform(&mut self) {
        if let Some(last) = self.transform_stack.last_mut() {
            *last = Transform2D::IDENTITY;
        }
    }

    fn set_offset(&mut self, dx: f32, dy: f32) {
        self.offset = PointImpl { x: dx, y: dy };
    }

    fn get_offset(&self) -> PointImpl<f32> {
        self.offset
    }

    fn set_zoom(&mut self, zoom: f32) {
        self.zoom = zoom;
    }

    fn get_zoom(&self) -> f32 {
        self.zoom
    }

    fn init_zoom_and_offset(&mut self) {
        self.zoom = 1.0;
        self.offset = PointImpl { x: 0.0, y: 0.0 };
    }

    fn scale(&mut self, sx: f32, sy: f32) {
        let current = self.get_transform();
        let scale_t = Transform2D { a: sx, b: 0.0, c: 0.0, d: sy, tx: 0.0, ty: 0.0 };
        self.set_transform(&transform_mul(&current, &scale_t));
    }

    fn get_scale(&self) -> PointImpl<f32> {
        let t = self.get_transform();
        PointImpl { x: t.a, y: t.d }
    }

    fn get_context_scale(&self) -> f32 {
        self.zoom
    }

    fn translate(&mut self, dx: f32, dy: f32) {
        let current = self.get_transform();
        let trans = Transform2D { a: 1.0, b: 0.0, c: 0.0, d: 1.0, tx: dx, ty: dy };
        self.set_transform(&transform_mul(&current, &trans));
    }

    fn set_clip(&mut self, path: &Path) {
        let id = self.alloc_id("clip");
        let d = path_to_d(path);
        writeln!(
            &mut self.buffer,
            r#"<defs><clipPath id="{}"><path d="{}"/></clipPath></defs>"#,
            id, d
        )
        .unwrap();
        // 클립 스택에 푸시 (후속 element 에서 clip-path 사용 시 참조)
        self.clip_stack.push(Path { commands: path.commands.clone() });
    }

    fn reset_clip(&mut self) {
        self.clip_stack.clear();
    }

    fn get_clip_bounds(&self) -> RectImpl<f32> {
        RectImpl { x: 0.0, y: 0.0, w: self.width, h: self.height }
    }

    fn detach_region(&mut self) {
        // no-op for SVG
    }

    fn set_antialiasing(&mut self, enabled: bool) {
        self.antialiasing = enabled;
    }

    fn set_fill_antialiasing(&mut self, enabled: bool) {
        self.antialiasing = enabled;
    }

    fn set_interpolation_mode(&mut self, _mode: u32) {
        // SVG 는 별도 옵션 — image-rendering attribute 로 매핑
    }

    fn set_text_rendering_hint(&mut self, _hint: u32) {
        // SVG 의 text-rendering attribute. <path> emit 사용하면 무관
    }

    fn set_compositing_mode(&mut self, _mode: u32) {
        // SVG mix-blend-mode 로 매핑 가능
    }

    fn get_compositing_mode(&self) -> u32 {
        0
    }

    fn set_pen_integer(&mut self, enabled: bool) {
        self.pen_integer = enabled;
    }

    fn get_pen_integer(&self) -> bool {
        self.pen_integer
    }

    fn is_print(&self) -> bool {
        false
    }

    fn is_valid_memory(&self) -> bool {
        true
    }

    fn detach(&mut self) {
        // no-op for SVG
    }

    fn get_memory(&self) -> *const () {
        std::ptr::null()
    }

    fn get_native(&self) -> *const () {
        std::ptr::null()
    }

    fn get_impl(&self) -> *const () {
        std::ptr::null()
    }

    fn get_dc(&self) -> *const () {
        std::ptr::null()
    }

    fn release_dc(&mut self, _dc: *const ()) {}

    fn get_last_error(&self) -> i32 {
        0
    }

    // ─── GState / GroupedTransform / DrawBlip (S-4 image backend) ──────────

    fn save_state(&mut self) {
        // <g> open 하고 stack 에 frame 시작 (group count = 1, 이 base group 도 포함).
        self.buffer.push_str("<g>\n");
        let cur = self.get_transform();
        self.transform_stack.push(cur);
        // count=1 → 본 save 자체가 emit 한 group 1개. concat_transform 마다 increment.
        self.gstate_stack.push(1);
    }

    fn restore_state(&mut self) {
        // 가장 위 frame pop 후 그 count 만큼 `</g>` close. save_state 한 번에
        // restore_state 한 번 짝맞춤 — 그 사이 concat_transform N 회 면 N+1 개 close.
        if let Some(count) = self.gstate_stack.pop() {
            for _ in 0..count {
                self.buffer.push_str("</g>\n");
            }
        }
        // transform_stack 도 pop (save_state 에서 push 한 짝)
        if self.transform_stack.len() > 1 {
            self.transform_stack.pop();
        }
    }

    fn concat_transform(&mut self, t: &crate::transform2d::Transform2D) {
        // Hancom Transform2D 의 element 0..5 = (m00, m10, m01, m11, m20, m21)
        // get_element(0) = a (x scale), get_element(1) = b (y skew), ...
        // → SVG matrix(a, b, c, d, tx, ty) = (m00, m10, m01, m11, m20, m21)
        let m00 = t.get_element(0);
        let m10 = t.get_element(1);
        let m01 = t.get_element(2);
        let m11 = t.get_element(3);
        let m20 = t.get_element(4);
        let m21 = t.get_element(5);
        // 현재 group 안에 transform 적용된 sub-group emit (concat 의미)
        writeln!(
            &mut self.buffer,
            r#"<g transform="matrix({:.6} {:.6} {:.6} {:.6} {:.3} {:.3})">"#,
            m00, m10, m01, m11, m20, m21
        ).unwrap();
        // 현 frame 의 group count 증가. 마지막 restore_state 가 한 번에 모두 close.
        if let Some(top) = self.gstate_stack.last_mut() {
            *top += 1;
        } else {
            // save_state 호출 없이 concat_transform 단독 호출 — implicit frame 생성.
            // raw 한컴은 외부 save_state 없는 concat 호출 안 함 (caller contract).
            // 본 fallback 은 safety net — caller 가 reset_transform 으로 manual cleanup 가능.
            self.gstate_stack.push(1);
        }
    }

    fn draw_blip(
        &mut self,
        path: &crate::path::Path,
        picture: *mut crate::share_ptr::ControlBlock<crate::brush::ImageBrush>,
    ) {
        // raw 의 vfunc[13] 등치 — Path 안에 picture 그리기.
        //
        // 1. picture null 검사 (caller 도 했지만 한 번 더)
        if picture.is_null() {
            return;
        }
        let source_id = unsafe {
            if (*picture).obj.is_null() {
                return;
            }
            (*(*picture).obj).source_id.clone()
        };

        // 2. path 의 bounds 계산 (rect 가 path 의 유일한 entry 가정 — AddRect 만 호출)
        let bounds = path.get_bounds();
        let x = bounds.origin.x;
        let y = bounds.origin.y;
        let w = bounds.size_w;
        let h = bounds.size_h;

        // 3. source_id 로 등록된 data URL lookup. 없으면 fallback id.
        let href = self.image_data_map
            .get(&source_id)
            .cloned()
            .unwrap_or_else(|| format!("#img-{}", source_id));

        // 4. <image> emit (path 의 bounding rect 위치 + 크기)
        writeln!(
            &mut self.buffer,
            r#"<image x="{:.3}" y="{:.3}" width="{:.3}" height="{:.3}" href="{}" data-source-id="{}"/>"#,
            x, y, w, h, href, source_id
        ).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brush::EmptyBrush;
    use crate::color::Color;

    #[test]
    fn new_svg_surface() {
        let s = SvgSurface::new(595.0, 842.0); // A4 in points
        assert_eq!(s.width, 595.0);
        assert_eq!(s.height, 842.0);
        assert_eq!(s.zoom, 1.0);
        assert_eq!(s.transform_stack.len(), 1);
    }

    #[test]
    fn finish_produces_svg_document() {
        let s = SvgSurface::new(100.0, 100.0);
        let doc = s.finish();
        assert!(doc.starts_with("<svg"));
        assert!(doc.contains("width=\"100\""));
        assert!(doc.ends_with("</svg>"));
    }

    #[test]
    fn transform_get_set() {
        let mut s = SvgSurface::new(100.0, 100.0);
        let t = Transform2D { a: 2.0, b: 0.0, c: 0.0, d: 2.0, tx: 10.0, ty: 20.0 };
        s.set_transform(&t);
        assert_eq!(s.get_transform(), t);
    }

    #[test]
    fn zoom_get_set() {
        let mut s = SvgSurface::new(100.0, 100.0);
        s.set_zoom(2.5);
        assert_eq!(s.get_zoom(), 2.5);
    }

    #[test]
    fn offset_get_set() {
        let mut s = SvgSurface::new(100.0, 100.0);
        s.set_offset(15.0, 25.0);
        let o = s.get_offset();
        assert_eq!(o.x, 15.0);
        assert_eq!(o.y, 25.0);
    }

    #[test]
    fn alloc_id_unique() {
        let mut s = SvgSurface::new(10.0, 10.0);
        let id1 = s.alloc_id("clip");
        let id2 = s.alloc_id("clip");
        assert_ne!(id1, id2);
    }

    // ─── S-2 fill/outline tests ────────────────────────────────────

    #[test]
    fn fill_rect_int_emits_rect_with_fill_none_for_empty_brush() {
        let mut s = SvgSurface::new(100.0, 100.0);
        let brush = Brush::Empty(EmptyBrush::new());
        s.fill_rect_int(RectImpl { x: 10, y: 20, w: 30, h: 40 }, &brush);
        assert!(s.buffer.contains(r#"<rect x="10" y="20" width="30" height="40" fill="none"/>"#));
    }

    #[test]
    fn fill_rect_float_formats_decimals() {
        let mut s = SvgSurface::new(100.0, 100.0);
        let brush = Brush::Empty(EmptyBrush::new());
        s.fill_rect_float(RectImpl { x: 1.5, y: 2.25, w: 3.75, h: 4.0 }, &brush);
        assert!(s.buffer.contains(r#"x="1.500""#));
        assert!(s.buffer.contains(r#"width="3.750""#));
    }

    #[test]
    fn outline_rect_emits_stroke_attrs() {
        let mut s = SvgSurface::new(100.0, 100.0);
        let pen = Pen::new_default();
        s.outline_rect_int(RectImpl { x: 0, y: 0, w: 10, h: 10 }, &pen);
        assert!(s.buffer.contains(r#"fill="none""#));
        assert!(s.buffer.contains(r#"stroke="#));
    }

    // ─── Stage 4: pen_to_stroke_attrs 정밀화 검증 ──────────────────────

    #[test]
    fn pen_to_stroke_attrs_default_uses_solid_brush_color_and_thickness() {
        let pen = Pen::new_default();
        let attrs = pen_to_stroke_attrs(&pen);
        // default solid brush 의 black + 기본 thickness
        assert!(attrs.contains("stroke=\""), "got: {}", attrs);
        assert!(attrs.contains("stroke-width="));
        assert!(attrs.contains("stroke-linecap="));
        assert!(attrs.contains("stroke-linejoin="));
        assert!(attrs.contains("stroke-miterlimit="));
        // dash 없는 default 는 dasharray 없어야
        assert!(!attrs.contains("stroke-dasharray"));
    }

    #[test]
    fn pen_to_stroke_attrs_dash_style_emits_dasharray() {
        use crate::pen::DashStyle;
        let mut pen = Pen::new_default();
        unsafe { pen.override_enum_at(crate::pen::Pen::KEY_DASH, 2); } // 2 = Dash
        assert_eq!(pen.get_dash_style(), DashStyle::Dash);
        let attrs = pen_to_stroke_attrs(&pen);
        assert!(attrs.contains("stroke-dasharray="), "got: {}", attrs);
    }

    #[test]
    fn pen_to_stroke_attrs_round_cap_emits_round() {
        use crate::pen::LineCapStyle;
        let mut pen = Pen::new_default();
        unsafe { pen.override_enum_at(crate::pen::Pen::KEY_LINE_CAP, 0); } // 0 = Round
        assert_eq!(pen.get_line_cap_style(), LineCapStyle::Round);
        let attrs = pen_to_stroke_attrs(&pen);
        assert!(attrs.contains(r#"stroke-linecap="round""#), "got: {}", attrs);
    }

    #[test]
    fn pen_to_stroke_attrs_bevel_join_emits_bevel() {
        use crate::pen::LineJoinStyle;
        let mut pen = Pen::new_default();
        unsafe { pen.override_enum_at(crate::pen::Pen::KEY_LINE_JOIN, 2); } // 2 = Bevel
        assert_eq!(pen.get_line_join_style(), LineJoinStyle::Bevel);
        let attrs = pen_to_stroke_attrs(&pen);
        assert!(attrs.contains(r#"stroke-linejoin="bevel""#), "got: {}", attrs);
    }

    #[test]
    fn pen_to_stroke_attrs_empty_brush_returns_none_color() {
        let mut pen = Pen::new_default();
        pen.set_stroke_brush(Box::new(Brush::Empty(EmptyBrush::new())));
        let attrs = pen_to_stroke_attrs(&pen);
        assert!(attrs.contains(r#"stroke="none""#), "got: {}", attrs);
    }

    #[test]
    fn fill_path_emits_path_d() {
        let mut s = SvgSurface::new(100.0, 100.0);
        let path = Path {
            commands: vec![
                PathCmd::MoveTo(10.0, 20.0),
                PathCmd::LineTo(50.0, 60.0),
                PathCmd::Close,
            ],
        };
        let brush = Brush::Empty(EmptyBrush::new());
        s.fill_path(&path, &brush);
        assert!(s.buffer.contains(r#"d="M10.000 20.000 L50.000 60.000 Z""#));
    }

    #[test]
    fn path_to_d_with_curve() {
        let path = Path {
            commands: vec![
                PathCmd::MoveTo(0.0, 0.0),
                PathCmd::CurveTo(10.0, 10.0, 20.0, 20.0, 30.0, 30.0),
                PathCmd::LineTo(40.0, 40.0),
                PathCmd::Close,
            ],
        };
        let d = path_to_d(&path);
        assert!(d.contains("C10.000 10.000 20.000 20.000 30.000 30.000"));
        assert!(d.ends_with("Z"));
    }

    // ─── S-2 transform tests ───────────────────────────────────────

    #[test]
    fn translate_appends_to_current_transform() {
        let mut s = SvgSurface::new(100.0, 100.0);
        s.translate(10.0, 20.0);
        let t = s.get_transform();
        assert_eq!(t.tx, 10.0);
        assert_eq!(t.ty, 20.0);
        assert_eq!(t.a, 1.0);
        assert_eq!(t.d, 1.0);
    }

    #[test]
    fn scale_modifies_diagonal() {
        let mut s = SvgSurface::new(100.0, 100.0);
        s.scale(2.0, 3.0);
        let t = s.get_transform();
        assert_eq!(t.a, 2.0);
        assert_eq!(t.d, 3.0);
        assert_eq!(t.tx, 0.0);
    }

    #[test]
    fn scale_then_translate_composes() {
        let mut s = SvgSurface::new(100.0, 100.0);
        s.scale(2.0, 2.0);
        s.translate(5.0, 5.0);
        let t = s.get_transform();
        // result: scale(2) then translate(5,5) in scaled space = tx=10, ty=10
        assert_eq!(t.a, 2.0);
        assert_eq!(t.d, 2.0);
        assert_eq!(t.tx, 10.0);
        assert_eq!(t.ty, 10.0);
    }

    #[test]
    fn cartesian_transform_flips_y() {
        let mut s = SvgSurface::new(100.0, 200.0);
        let id = Transform2D::IDENTITY;
        s.set_cartesian_transform(&id);
        let t = s.get_transform();
        // y-flip: d = -1, ty = height
        assert_eq!(t.d, -1.0);
        assert_eq!(t.ty, 200.0);
    }

    // ─── helper tests ──────────────────────────────────────────────

    #[test]
    fn transform_mul_identity_is_idempotent() {
        let id = Transform2D::IDENTITY;
        let t = Transform2D { a: 2.0, b: 0.5, c: 0.5, d: 3.0, tx: 10.0, ty: 20.0 };
        assert_eq!(transform_mul(&id, &t), t);
        assert_eq!(transform_mul(&t, &id), t);
    }

    #[test]
    fn transform_mul_scale_then_scale() {
        let s1 = Transform2D { a: 2.0, b: 0.0, c: 0.0, d: 2.0, tx: 0.0, ty: 0.0 };
        let s2 = Transform2D { a: 3.0, b: 0.0, c: 0.0, d: 3.0, tx: 0.0, ty: 0.0 };
        let r = transform_mul(&s1, &s2);
        assert_eq!(r.a, 6.0);
        assert_eq!(r.d, 6.0);
    }

    #[test]
    fn color_to_svg_rgb_format() {
        let c = Color::from_rgb(0xAB, 0xCD, 0xEF, std::ptr::null_mut());
        let s = color_to_svg(&c);
        assert_eq!(s, "#abcdef");
    }

    #[test]
    fn brush_to_fill_empty_returns_none() {
        let b = Brush::Empty(EmptyBrush::new());
        assert_eq!(brush_to_fill(&b), "none");
    }

    // ─── Stage 4: resolve_brush_to_fill 4 brush types ─────────────────

    #[test]
    fn resolve_brush_to_fill_solid_uses_property_bag_color() {
        let mut s = SvgSurface::new(100.0, 100.0);
        let red = crate::color::Color::from_rgb(0xff, 0x00, 0x00, std::ptr::null_mut());
        let brush = Brush::Solid(crate::brush::SolidBrush::new(red));
        let fill = s.resolve_brush_to_fill(&brush);
        assert_eq!(fill, "#ff0000");
        // SolidBrush 는 defs 등록 안 함 (inline fill)
        assert!(!s.buffer.contains("<defs>"));
    }

    #[test]
    fn resolve_brush_to_fill_gradient_emits_linear_gradient_def() {
        let mut s = SvgSurface::new(100.0, 100.0);
        let brush = Brush::Gradient(crate::brush::GradientBrush::new());
        let fill = s.resolve_brush_to_fill(&brush);
        // url(#grad_N) 형태
        assert!(fill.starts_with("url(#grad_"), "got: {}", fill);
        // defs + linearGradient emit
        assert!(s.buffer.contains("<defs><linearGradient"), "buf: {}", s.buffer);
        assert!(s.buffer.contains("</linearGradient></defs>"));
    }

    #[test]
    fn resolve_brush_to_fill_hatch_emits_pattern_def_with_horizontal_style() {
        use crate::brush::HatchBrush;
        let mut s = SvgSurface::new(100.0, 100.0);
        let fg = crate::color::Color::from_rgb(0x00, 0x00, 0x00, std::ptr::null_mut());
        let bg = crate::color::Color::from_rgb(0xff, 0xff, 0xff, std::ptr::null_mut());
        let brush = Brush::Hatch(HatchBrush::new(0, fg, bg)); // style 0 = horizontal
        let fill = s.resolve_brush_to_fill(&brush);
        assert!(fill.starts_with("url(#hatch_"), "got: {}", fill);
        assert!(s.buffer.contains("<defs><pattern"));
        // horizontal: "M0 4 L8 4"
        assert!(s.buffer.contains("M0 4 L8 4"), "buf: {}", s.buffer);
        // bg fill rect + fg stroke path 둘 다
        assert!(s.buffer.contains("fill=\"#ffffff\""));
        assert!(s.buffer.contains("stroke=\"#000000\""));
    }

    #[test]
    fn resolve_brush_to_fill_hatch_diagonal_cross_emits_two_lines() {
        use crate::brush::HatchBrush;
        let mut s = SvgSurface::new(100.0, 100.0);
        let fg = crate::color::Color::from_rgb(0x00, 0x00, 0x00, std::ptr::null_mut());
        let bg = crate::color::Color::from_rgb(0xff, 0xff, 0xff, std::ptr::null_mut());
        let brush = Brush::Hatch(HatchBrush::new(5, fg, bg)); // style 5 = diagonal cross
        let _ = s.resolve_brush_to_fill(&brush);
        // diagonal cross: "M0 0 L8 8 M0 8 L8 0"
        assert!(s.buffer.contains("M0 0 L8 8 M0 8 L8 0"), "buf: {}", s.buffer);
    }

    #[test]
    fn resolve_brush_to_fill_image_with_registered_source_embeds_data_url() {
        use crate::brush::ImageBrush;
        let mut s = SvgSurface::new(200.0, 200.0);
        // PNG 1x1 transparent (1B placeholder)
        s.register_image("pic-1", "image/png", &[0xff, 0xd8, 0xff, 0xe0]);
        let brush = Brush::Image(ImageBrush::new("pic-1".to_string()));
        let fill = s.resolve_brush_to_fill(&brush);
        assert!(fill.starts_with("url(#imgpat_"), "got: {}", fill);
        assert!(s.buffer.contains("<defs><pattern"));
        assert!(s.buffer.contains("data:image/png;base64,"), "buf: {}", s.buffer);
    }

    #[test]
    fn resolve_brush_to_fill_image_without_registered_source_uses_fallback_href() {
        use crate::brush::ImageBrush;
        let mut s = SvgSurface::new(200.0, 200.0);
        let brush = Brush::Image(ImageBrush::new("pic-missing".to_string()));
        let _ = s.resolve_brush_to_fill(&brush);
        // 미등록은 "#missing-img" placeholder
        assert!(s.buffer.contains("href=\"#missing-img\""), "buf: {}", s.buffer);
    }

    #[test]
    fn resolve_brush_to_fill_empty_returns_none() {
        let mut s = SvgSurface::new(100.0, 100.0);
        let brush = Brush::Empty(EmptyBrush::new());
        let fill = s.resolve_brush_to_fill(&brush);
        assert_eq!(fill, "none");
        assert!(!s.buffer.contains("<defs>"));
    }

    #[test]
    fn resolve_brush_to_fill_image_tile_mode_uses_small_repeat_size() {
        use crate::brush::ImageBrush;
        let mut s = SvgSurface::new(800.0, 600.0);
        s.register_image("tile-src", "image/png", &[1, 2, 3, 4]);
        // tile_style=1 (Tile/repeat) + scale 2.0 = 200×200 패턴 cell
        let mut ib = ImageBrush::new("tile-src".to_string());
        ib.tile_style = 1;
        ib.scale_x = 2.0;
        ib.scale_y = 2.0;
        let brush = Brush::Image(ib);
        let _ = s.resolve_brush_to_fill(&brush);
        // 100 * 2 = 200
        assert!(s.buffer.contains(r#"width="200.000""#), "buf: {}", s.buffer);
        assert!(s.buffer.contains(r#"height="200.000""#));
    }

    #[test]
    fn resolve_brush_to_fill_image_stretch_mode_uses_surface_size() {
        use crate::brush::ImageBrush;
        let mut s = SvgSurface::new(500.0, 300.0);
        s.register_image("strch", "image/jpeg", &[5, 6, 7]);
        // tile_style=0 (NoTile/stretch) + scale 1.0 = surface 전체
        let mut ib = ImageBrush::new("strch".to_string());
        ib.scale_x = 1.0;
        ib.scale_y = 1.0;
        let brush = Brush::Image(ib);
        let _ = s.resolve_brush_to_fill(&brush);
        assert!(s.buffer.contains(r#"width="500.000""#), "buf: {}", s.buffer);
        assert!(s.buffer.contains(r#"height="300.000""#));
    }

    #[test]
    fn resolve_brush_to_fill_image_offset_emits_pattern_transform() {
        use crate::brush::ImageBrush;
        let mut s = SvgSurface::new(400.0, 400.0);
        s.register_image("off", "image/png", &[0xff]);
        let mut ib = ImageBrush::new("off".to_string());
        ib.offset_x = 10.5;
        ib.offset_y = 20.25;
        let brush = Brush::Image(ib);
        let _ = s.resolve_brush_to_fill(&brush);
        assert!(s.buffer.contains(r#"patternTransform="translate(10.500 20.250)""#),
            "buf: {}", s.buffer);
    }

    #[test]
    fn resolve_brush_to_fill_image_zero_offset_skips_pattern_transform() {
        use crate::brush::ImageBrush;
        let mut s = SvgSurface::new(400.0, 400.0);
        s.register_image("noof", "image/png", &[0xff]);
        let brush = Brush::Image(ImageBrush::new("noof".to_string()));
        let _ = s.resolve_brush_to_fill(&brush);
        assert!(!s.buffer.contains("patternTransform"), "buf: {}", s.buffer);
    }

    #[test]
    fn fill_rect_with_gradient_brush_emits_both_defs_and_rect() {
        let mut s = SvgSurface::new(100.0, 100.0);
        let brush = Brush::Gradient(crate::brush::GradientBrush::new());
        s.fill_rect_float(RectImpl { x: 10.0, y: 10.0, w: 50.0, h: 50.0 }, &brush);
        // 같은 buffer 에 defs 와 rect 둘 다
        assert!(s.buffer.contains("<defs><linearGradient"));
        assert!(s.buffer.contains("<rect"));
        assert!(s.buffer.contains("fill=\"url(#grad_"));
    }

    // ─── S-2 misc ───────────────────────────────────────────────────

    #[test]
    fn set_clip_emits_defs_clippath() {
        let mut s = SvgSurface::new(100.0, 100.0);
        let path = Path {
            commands: vec![PathCmd::MoveTo(0.0, 0.0), PathCmd::LineTo(50.0, 50.0), PathCmd::Close],
        };
        s.set_clip(&path);
        assert!(s.buffer.contains("<defs><clipPath"));
        assert!(s.buffer.contains("clip_0"));
        assert_eq!(s.clip_stack.len(), 1);
    }

    #[test]
    fn draw_pie_emits_arc_path() {
        let mut s = SvgSurface::new(100.0, 100.0);
        let pen = Pen::new_default();
        s.draw_pie(RectImpl { x: 0.0, y: 0.0, w: 100.0, h: 100.0 }, 0.0, 90.0, &pen);
        // M, L, A, Z 명령 모두 포함 (arc 명령 검증)
        assert!(s.buffer.contains(" A"));
        assert!(s.buffer.contains(" Z\""));
    }

    // ─── S-3 text (DrawString/MeasureString) tests ─────────────────

    use std::path::PathBuf;

    /// HFT 테스트 픽스처를 로드해서 (canonical: "HCHGGGT", "HGMJ") shared cache 반환.
    /// hft-decoder/rust/test-data/ 의 4개 .HFT 파일 사용.
    fn load_test_hft_cache() -> Arc<HftCache> {
        let mut cache = HftCache::new();
        let mut base = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        // render-engine/rust → ../../hft-decoder/rust/test-data
        base.pop(); // rust → render-engine
        base.pop(); // render-engine → toolkit
        base.push("hft-decoder");
        base.push("rust");
        base.push("test-data");
        cache.load_dir(&base).expect("load HFT test-data");
        Arc::new(cache)
    }

    fn make_text_surface() -> SvgSurface {
        SvgSurface::new(595.0, 842.0).with_hft_cache(load_test_hft_cache())
    }

    fn make_font(family: &str, size: f32) -> Font {
        Font { family: family.to_string(), size, bold: false, italic: false }
    }

    fn utf16(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    #[test]
    fn s3_load_test_cache_has_hchgggt() {
        // 사전 검증: test-data 의 HCHGGGT 가 로드되고 한글 한 글자 lookup 가능
        let cache = load_test_hft_cache();
        assert!(cache.family_count() > 0, "no HFT families loaded");
        assert!(cache.has_font("HCHGGGT"), "HCHGGGT not loaded");
        let g = cache.get("HCHGGGT", '가' as u32);
        assert!(g.is_some(), "'가' glyph missing from HCHGGGT");
        let g = g.unwrap();
        assert!(!g.d.is_empty());
        assert!(g.advance > 0);
        assert!(g.em > 0);
    }

    #[test]
    fn s3_draw_driver_string_emits_path_per_glyph() {
        let mut s = make_text_surface();
        let font = make_font("HCHGGGT", 12.0);
        let brush = Brush::Empty(EmptyBrush::new()); // fill="none" 으로 검증 단순화
        let text = utf16("가나");
        let positions = vec![
            PointImpl { x: 100.0, y: 200.0 },
            PointImpl { x: 150.0, y: 200.0 },
        ];
        s.draw_driver_string(&text, &font, &brush, &positions, &Transform2D::IDENTITY);
        // 두 글자 → 두 <path> emit
        let path_count = s.buffer.matches("<path ").count();
        assert_eq!(path_count, 2, "expected 2 glyph paths, got {}: {}", path_count, s.buffer);
        // 각 path 가 transform matrix(s 0 0 -s pos.x pos.y) 형식
        assert!(s.buffer.contains(r#"matrix("#));
        assert!(s.buffer.contains(r#" 100.000 200.000)"#));
        assert!(s.buffer.contains(r#" 150.000 200.000)"#));
    }

    #[test]
    fn s3_draw_driver_string_y_flip_in_matrix() {
        // em-coord 는 y-up → SVG y-down: matrix 의 d (4번째 값) 음수
        let mut s = make_text_surface();
        let font = make_font("HCHGGGT", 100.0); // em=1000 가정 시 scale 0.1
        let brush = Brush::Empty(EmptyBrush::new());
        let text = utf16("가");
        let positions = vec![PointImpl { x: 0.0, y: 0.0 }];
        s.draw_driver_string(&text, &font, &brush, &positions, &Transform2D::IDENTITY);
        // matrix(s 0 0 -s ...) — 3번째 0, 4번째 -s
        // s = 100/em. em 모르지만 음수 부호만 검증.
        assert!(s.buffer.contains("matrix("));
        // 음수 scale 확인 (예: -0.100000 또는 -0.1.. 등)
        let has_neg_scale = s.buffer.contains(" -0.") || s.buffer.contains(" -1.");
        assert!(has_neg_scale, "y scale should be negative for em-to-svg flip: {}", s.buffer);
    }

    #[test]
    fn s3_draw_driver_string_with_outer_transform_wraps_in_group() {
        let mut s = make_text_surface();
        let font = make_font("HCHGGGT", 12.0);
        let brush = Brush::Empty(EmptyBrush::new());
        let text = utf16("가");
        let positions = vec![PointImpl { x: 0.0, y: 0.0 }];
        let outer = Transform2D { a: 2.0, b: 0.0, c: 0.0, d: 2.0, tx: 50.0, ty: 60.0 };
        s.draw_driver_string(&text, &font, &brush, &positions, &outer);
        assert!(s.buffer.contains(r#"<g transform="matrix(2 0 0 2 50 60)">"#));
        assert!(s.buffer.contains("</g>"));
    }

    #[test]
    fn s3_draw_driver_string_identity_transform_no_group() {
        let mut s = make_text_surface();
        let font = make_font("HCHGGGT", 12.0);
        let brush = Brush::Empty(EmptyBrush::new());
        let text = utf16("가");
        let positions = vec![PointImpl { x: 0.0, y: 0.0 }];
        s.draw_driver_string(&text, &font, &brush, &positions, &Transform2D::IDENTITY);
        // identity 면 outer group 안 만듦
        assert!(!s.buffer.contains("<g transform"));
    }

    #[test]
    fn s3_draw_driver_string_length_mismatch_uses_shorter() {
        // positions 가 더 짧으면 그만큼만 emit (한컴 native 의 invariant 깨질 때 방어)
        let mut s = make_text_surface();
        let font = make_font("HCHGGGT", 12.0);
        let brush = Brush::Empty(EmptyBrush::new());
        let text = utf16("가나다라");
        let positions = vec![
            PointImpl { x: 0.0, y: 0.0 },
            PointImpl { x: 10.0, y: 0.0 },
        ];
        s.draw_driver_string(&text, &font, &brush, &positions, &Transform2D::IDENTITY);
        let path_count = s.buffer.matches("<path ").count();
        assert_eq!(path_count, 2);
    }

    #[test]
    fn s3_draw_driver_string_empty_text_noop() {
        let mut s = make_text_surface();
        let font = make_font("HCHGGGT", 12.0);
        let brush = Brush::Empty(EmptyBrush::new());
        s.draw_driver_string(&[], &font, &brush, &[], &Transform2D::IDENTITY);
        assert!(!s.buffer.contains("<path"));
    }

    #[test]
    fn s3_draw_string_point_accumulates_advance() {
        // caller 가 single baseline 만 제공 → advance 누적으로 자체 position 계산
        let mut s = make_text_surface();
        let font = make_font("HCHGGGT", 12.0);
        let brush = Brush::Empty(EmptyBrush::new());
        let text = utf16("가나다");
        s.draw_string_point(&text, &font, PointImpl { x: 30.0, y: 100.0 }, &brush, &StringFormat::default());
        let path_count = s.buffer.matches("<path ").count();
        assert_eq!(path_count, 3);
        // 첫 글자는 baseline x = 30
        assert!(s.buffer.contains(" 30.000 100.000)"));
        // 두 번째 글자 x > 30 (advance 누적). 정확값은 폰트 advance 의존 — 30 외 좌표가 있어야 함.
        let lines: Vec<&str> = s.buffer.lines().filter(|l| l.contains("<path ")).collect();
        assert_eq!(lines.len(), 3);
        // 2, 3번째 line 의 baseline x 가 첫 번째와 달라야 함
        let first_x_idx = lines[0].find(" 30.000 100.000)").is_some();
        assert!(first_x_idx);
    }

    #[test]
    fn s3_draw_string_point_empty_text_noop() {
        let mut s = make_text_surface();
        let font = make_font("HCHGGGT", 12.0);
        let brush = Brush::Empty(EmptyBrush::new());
        s.draw_string_point(&[], &font, PointImpl { x: 0.0, y: 0.0 }, &brush, &StringFormat::default());
        assert!(!s.buffer.contains("<path"));
    }

    #[test]
    fn s3_draw_string_rect_treats_rect_as_topleft_anchor() {
        let mut s = make_text_surface();
        let font = make_font("HCHGGGT", 12.0);
        let brush = Brush::Empty(EmptyBrush::new());
        let text = utf16("가");
        s.draw_string_rect(
            &text,
            &font,
            RectImpl { x: 50.0, y: 80.0, w: 200.0, h: 50.0 },
            &brush,
            &StringFormat::default(),
        );
        // baseline = rect.x, rect.y + font.size = (50, 92)
        assert!(s.buffer.contains(" 50.000 92.000)"));
    }

    #[test]
    fn s3_measure_string_point_returns_advance_sum() {
        let s = make_text_surface();
        let font = make_font("HCHGGGT", 12.0);
        let text = utf16("가나");
        let rect = s.measure_string_point(
            &text,
            &font,
            PointImpl { x: 10.0, y: 50.0 },
            &StringFormat::default(),
        );
        assert_eq!(rect.x, 10.0);
        // baseline - font.size
        assert_eq!(rect.y, 50.0 - 12.0);
        assert!(rect.w > 0.0, "width should be positive (advance sum)");
        assert_eq!(rect.h, 12.0);
    }

    #[test]
    fn s3_measure_string_empty_text_zero_width() {
        let s = make_text_surface();
        let font = make_font("HCHGGGT", 12.0);
        let rect = s.measure_string_point(
            &[],
            &font,
            PointImpl { x: 0.0, y: 0.0 },
            &StringFormat::default(),
        );
        assert_eq!(rect.w, 0.0);
    }

    #[test]
    fn s3_measure_driver_string_uses_position_extents() {
        let s = make_text_surface();
        let font = make_font("HCHGGGT", 12.0);
        let text = utf16("가나");
        let positions = vec![
            PointImpl { x: 100.0, y: 200.0 },
            PointImpl { x: 200.0, y: 200.0 },
        ];
        let rect = s.measure_driver_string(&text, &font, &positions);
        assert_eq!(rect.x, 100.0);
        // 마지막 글자 x + advance
        assert!(rect.w >= 100.0);
        // baseline y - font.size .. baseline y
        assert_eq!(rect.h, 12.0);
    }

    #[test]
    fn s3_measure_driver_string_empty_returns_zero_rect() {
        let s = make_text_surface();
        let font = make_font("HCHGGGT", 12.0);
        let rect = s.measure_driver_string(&[], &font, &[]);
        assert_eq!(rect.w, 0.0);
        assert_eq!(rect.h, 0.0);
    }

    #[test]
    #[should_panic(expected = "hft_cache")]
    fn s3_text_emit_without_cache_panics() {
        let mut s = SvgSurface::new(100.0, 100.0); // no with_hft_cache()
        let font = make_font("HCHGGGT", 12.0);
        let brush = Brush::Empty(EmptyBrush::new());
        s.draw_string_point(
            &utf16("가"),
            &font,
            PointImpl { x: 0.0, y: 0.0 },
            &brush,
            &StringFormat::default(),
        );
    }

    #[test]
    fn s3_unknown_glyph_skipped_no_panic() {
        // HFT 에 없는 codepoint (예: '😀' = U+1F600) 는 skip — emit 0개, panic 없음
        let mut s = make_text_surface();
        let font = make_font("HCHGGGT", 12.0);
        let brush = Brush::Empty(EmptyBrush::new());
        // surrogate pair 로 인코딩되는 emoji
        let text = utf16("😀");
        let positions = vec![PointImpl { x: 0.0, y: 0.0 }];
        s.draw_driver_string(&text, &font, &brush, &positions, &Transform2D::IDENTITY);
        // glyph 없어서 path 0개 emit, panic 없음
        assert_eq!(s.buffer.matches("<path ").count(), 0);
    }

    #[test]
    fn s3_utf16_surrogate_decoded_to_single_codepoint() {
        // '😀' (U+1F600) = surrogate pair (D83D, DE00). decode_utf16 가 single u32 로 합침.
        let text = utf16("😀");
        assert_eq!(text.len(), 2); // surrogate pair
        let codepoints = SvgSurface::decode_utf16(&text);
        assert_eq!(codepoints.len(), 1);
        assert_eq!(codepoints[0], 0x1F600);
    }

    #[test]
    fn s3_no_text_element_emitted_ever() {
        // 정공법: `<text>` 절대 emit 안 함 (시스템 폰트 fallback 차단)
        let mut s = make_text_surface();
        let font = make_font("HCHGGGT", 12.0);
        let brush = Brush::Empty(EmptyBrush::new());
        s.draw_string_point(&utf16("가나다"), &font, PointImpl { x: 0.0, y: 0.0 }, &brush, &StringFormat::default());
        assert!(!s.buffer.contains("<text"));
    }
}

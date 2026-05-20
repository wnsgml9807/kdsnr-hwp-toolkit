//! `probe_g_paragraph` — kdsnr-layout 의 `ppt_compose_layout` 직접 호출.
//!
//! G phase (HYBRID_LAYOUT_PLAN Phase D) 의 working step 진행.
//!
//! 두 단계:
//!   1. default (RunProperty 없음, metric=0) — flow 만 확인
//!   2. RunProperty + CharItemViewConstructorMetrics 채움 — metric 흐름 확인
//!
//! 입력: "AB\r" (3 chars), font_size=13pt, 가짜 metric (width=10, ascent=8, descent=2)

use kdsnr_layout::{
    glyph::{CharItemView, CharItemViewConstructorMetrics, ComposeResult, Glyph},
    value_types::BreakType,
    compose_layout::Break,
    ppt_compose_layout,
    runtime::RunProperty,
    properties::{HashMapPropertyBag, PropertyValue, keys},
};

// ── Glyph wrapper for CharItemView ────────────────
#[derive(Debug, Clone)]
struct CivItem(CharItemView);

impl Glyph for CivItem {
    fn clone_glyph(&self) -> Box<dyn Glyph> { Box::new(self.clone()) }
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn compose(&self, _bt: BreakType) -> ComposeResult {
        ComposeResult {
            replacement: Some(Box::new(self.0.clone())),
            can_break: false,
        }
    }
}

// ── Composition mockup ────────────────
#[derive(Debug)]
struct MutComp {
    children: Vec<Box<dyn Glyph>>,
}

impl Glyph for MutComp {
    fn clone_glyph(&self) -> Box<dyn Glyph> { unimplemented!("MutComp clone") }
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn get_count(&self) -> usize { self.children.len() }
    fn get_component(&self, idx: usize) -> Option<&dyn Glyph> {
        self.children.get(idx).map(|b| b.as_ref())
    }
}

// ── AppendRecorder ────────────────
#[derive(Debug, Default)]
struct AppendRecorder {
    items: Vec<Option<Box<dyn Glyph>>>,
}

impl Glyph for AppendRecorder {
    fn clone_glyph(&self) -> Box<dyn Glyph> { unimplemented!("AppendRecorder clone") }
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn append(&mut self, child: Option<Box<dyn Glyph>>) {
        self.items.push(child);
    }
}

// ── helpers ────────────────

/// 단순 RunProperty: font_size 만 set (한컴 key 0x96a).
fn make_run_property(font_size: f32) -> RunProperty {
    let bag = HashMapPropertyBag::new()
        .with(keys::FONT_SIZE, PropertyValue::Float(font_size));
    RunProperty::from_bag(bag)
}

/// 한 글자의 CharItemView — RunProperty + 가짜 metric 채움.
///
/// 한컴 raw `CharItemView::CharItemView` 는 `GetRealFont(theme)` 호출 후
/// `+0x3c, +0x40, +0x44, +0x4c, +0x50` 의 5 float metric 을 채우고
/// `compute_metrics` 호출. 본 helper 는 그 단계를 `from_constructor_metrics` 로
/// 한 번에 처리.
fn build_char_item_view(
    char_code: u16,
    font_size: f32,
    width: f32,
    ascent: f32,
    descent: f32,
) -> CharItemView {
    let run_property = Some(make_run_property(font_size));
    let metrics = CharItemViewConstructorMetrics {
        metric_3c: ascent + descent,  // 일단 ascent+descent = full height
        ascent,
        descent,
        metric_4c: width,
        metric_50: 0.0,
    };
    CharItemView::from_constructor_metrics(
        char_code,
        run_property,
        /*para_property*/ None,
        /*body_property*/ None,
        /*reset_or_size*/ font_size,
        metrics,
    )
}

fn dump_output(label: &str, output: &AppendRecorder) {
    println!("\n=== {} ===  ({} items)", label, output.items.len());
    for (i, item) in output.items.iter().enumerate() {
        match item {
            Some(g) => {
                if let Some(civ) = g.as_any().downcast_ref::<CharItemView>() {
                    let ch = char::from_u32(civ.char_code as u32).unwrap_or('?');
                    println!(
                        "  [{}] CharItemView char=0x{:04x} ({:?})  width={:.2}  ascent={:.2}  descent={:.2}  total_height={:.2}  line_height={:.2}",
                        i, civ.char_code, ch, civ.width, civ.ascent, civ.descent,
                        civ.total_height, civ.line_height
                    );
                } else {
                    let t = std::any::type_name_of_val(g);
                    println!("  [{}] {} (non-CharItemView)", i, t.rsplit("::").next().unwrap_or(t));
                }
            }
            None => println!("  [{}] (null Append)", i),
        }
    }
}

fn main() {
    let text = "AB\r";
    let chars: Vec<u16> = text.encode_utf16().collect();
    println!("input: {:?}  ({} chars)", text, chars.len());

    // ── Run 1: default CharItemView (no metric) ────
    let comp_default = MutComp {
        children: chars.iter()
            .map(|&c| Box::new(CivItem(CharItemView::new(c))) as Box<dyn Glyph>)
            .collect(),
    };
    let mut output_default = AppendRecorder::default();
    ppt_compose_layout(
        &comp_default, 1, Break::new(0, (chars.len() as i32) - 1),
        -1, 0, &mut output_default,
    );
    dump_output("Run 1: default (no RunProperty, metric=0)", &output_default);

    // ── Run 2: RunProperty + 가짜 metric (font_size=13, width=10, asc=8, desc=2) ────
    let font_size = 13.0;
    let comp_metric = MutComp {
        children: chars.iter()
            .map(|&c| {
                let view = build_char_item_view(c, font_size, 10.0, 8.0, 2.0);
                Box::new(CivItem(view)) as Box<dyn Glyph>
            })
            .collect(),
    };
    let mut output_metric = AppendRecorder::default();
    ppt_compose_layout(
        &comp_metric, 1, Break::new(0, (chars.len() as i32) - 1),
        -1, 0, &mut output_metric,
    );
    dump_output("Run 2: RunProperty(font_size=13) + metric(w=10, asc=8, desc=2)", &output_metric);

    println!();
    println!("Done. width/ascent/descent 가 Run 1 에선 0, Run 2 에선 채워졌는지 비교.");
}

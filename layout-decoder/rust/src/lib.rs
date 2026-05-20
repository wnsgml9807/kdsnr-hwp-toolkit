//! `kdsnr-layout` — Hancom HWPX paragraph layout engine.
//!
//! macOS `libHncDrawingEngine_arm64.dylib::Hnc::Shape::Text::*` 의 layout 시스템을
//! Ghidra RE 해서 1:1 포팅한 Rust 라이브러리. 최종 목표는 macOS 한컴 PDF 와
//! byte-equivalent 출력.
//!
//! RE 산출물 위치: `kdsnr-hwp-toolkit/work/hft_re/layout_re/`
//! - `decompiles_v2/` — 806 함수, 37 layout-관련 class
//! - `callers/` — virtual dispatch caller 분석
//! - `data_dump/` — DAT 상수 + vtable RTTI dump
//! - `COVERAGE.md` — coverage 정리
//! - `CLASS_HIERARCHY.md` — RTTI 검증된 class 계층
//!
//! 통합 plan: `kdsnr-hwp-toolkit/docs/HYBRID_LAYOUT_PLAN.md`
//!
//! 핵심 architecture (RTTI 검증, raid 24 확장):
//!
//! ```text
//! PptCompositor::ComposeLayout (paragraph entry)
//!   ↓ vtable virtual dispatch
//! {Col,Simple,Array}Compositor::ComposeLayout (strategy)
//!   ↓
//! ColCompositor::ComposeBreak (Knuth-Plass DP)
//!   ↓
//! Composition::ComposeGlyph (per-slot)
//!   ↓
//! Glyph::Compose (virtual; subclass override 가능)
//!   ↓
//! { Tile | Box | Glue | Strut | ... }
//! ```
//!
//! Phase 진행상황은 `HYBRID_LAYOUT_PLAN.md` 의 단계표 참조.
//!
//! # 모듈
//! - `value_types`: POD types (Requirement / Requisition / Allotment / Allocation / BreakType)
//! - `placement`: Placement hierarchy (PlaceFix / PlaceMargin / PlaceNatural / PlaceCenter)
//! - `glyph`: Glyph hierarchy (Tile / Box / Align / Deck / Glue / Space / Strut / MonoGlyph 등)

pub mod value_types;
pub mod placement;
pub mod glyph;
pub mod layout;
pub mod layout_factory;
pub mod compose_break;
pub mod compose_layout;
pub mod runtime;
pub mod properties;
pub mod text_property;
pub mod ppt_compositor;
pub mod ppt_compose_bullet;
pub mod ppt_compose_layout;
pub mod ppt_compose_numbering;
pub mod ppt_compose_break;
pub mod compositor;
pub mod simple_compositor;
pub mod array_compositor;
pub mod bullet_render;
pub mod autonum;
pub mod font_metric;
/// CoreText FFI 구현 — macOS 에서만 컴파일 (CoreText/CoreGraphics/CoreFoundation 링크).
#[cfg(target_os = "macos")]
pub mod font_metric_coretext;
pub mod composition;

pub use value_types::{
    Allocation, Allotment, BoundsRect, BreakType, Dimension, Extension, Requirement, Requisition,
};
pub use placement::{Placement, PlaceCenter, PlaceFix, PlaceMargin, PlaceNatural};
pub use glyph::{
    BlipGlyph, Box_, CharItemView, CharItemViewConstructorMetrics, ComposeResult,
    DebugGlyph, Deck, Direction, FirstLineMetrics, Glue, Glyph, HStrut, MonoGlyph,
    ShapeOf, Space, Strut, VStrut, WidgetGlyph,
};
pub use layout::{Align, Layout, Superpose, Tile, TileReverse};
pub use layout_factory::LayoutFactory;
pub use compose_break::{compose_break, ComposeBreakInput, ComposeBreakOutput};
pub use compose_layout::{compose_layout, Break};
pub use runtime::{RunProperty, ShapeEngine};
pub use properties::{
    BTreeMapPropertyBag, HashMapPropertyBag, PropertyBag, PropertyKey, PropertyValue,
    keys as property_keys,
};
pub use text_property::{Bullet, BodyProperty, ParaProperty};
pub use ppt_compositor::{
    find_para_cr_view, get_first_char_item_view_on_para, get_first_char_item_view_on_para_mut,
    get_para_item_view, is_first_line_on_para,
};
pub use ppt_compose_bullet::{
    bullet_render_key_901, bullet_render_paragraph_class, bullet_render_uses_vbox,
    create_bullet_render_from_properties, create_bullet_render_layout,
    create_bullet_render_with_children, ppt_compose_bullet, KEY_BULLET_RENDER_901,
};
pub use ppt_compose_break::ppt_compose_break;
pub use ppt_compose_layout::ppt_compose_layout;
pub use ppt_compose_numbering::ppt_compose_numbering;
pub use compositor::{
    ArrayCompositor, ColCompositor, Compositor, NumberingEntry, PptCompositor, SimpleCompositor,
};
pub use simple_compositor::{simple_compose_break, simple_compose_layout};
pub use array_compositor::array_compose_break;
pub use bullet_render::BulletRenderGlyph;
pub use font_metric::{
    combine_char_metrics, measure_string_advance, mul_div, parse_os2_metrics, select_system_font,
    CharCtorMetrics, CoreTextFontProvider, GlobalFontMetrics, GlobalMetricProvider, Os2Metrics,
    SystemFont,
};
#[cfg(target_os = "macos")]
pub use font_metric_coretext::CoreTextProvider;
pub use composition::{
    Composition, CompositionDirection, CompositionState, LRComposition, RowSegment,
    TBComposition,
};

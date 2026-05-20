//! `kdsnr-render` — Hancom HWPX rendering engine primitives.
//!
//! macOS `libHncDrawingEngine_arm64.dylib` + `libHncFoundation_arm64.dylib` 의
//! 렌더링 primitive 들 (Flag, BWMode, Hit, Theme, Surface ...) 을 Ghidra/objdump
//! RE 후 1:1 byte-equivalent 포팅한 Rust 라이브러리.
//!
//! `kdsnr-layout` (layout phase) 의 **하류** — Glyph::Draw/Undraw/GetBounds/Pick (vfunc 5-8)
//! 가 사용하는 렌더링 컨텍스트를 제공.
//!
//! # 진행 단계 (rendering phase grand plan, project_rendering_phase_plan.md 참조)
//!
//! - **R-1**: Flag/BWMode/Hit/Theme 의 raw 1:1 포팅 (Surface 제외) — 현 단계
//! - **R-2**: Surface vtable + 8 ctor 1:1 (multi-session)
//! - **R-3**: Surface 의 method (DrawLine/DrawRect/DrawText/FillPath 등 100+)
//! - **R-4**: libhsp GDI shim 의 macOS CoreGraphics backend
//! - **R-5**: Glyph::Draw/Undraw/GetBounds/Pick dispatch 포팅
//! - **R-6**: PDF writer RE
//! - **R-7**: e2e 파이프라인 wire + 한컴 PDF byte diff 검증
//!
//! # RE 정공법 정책
//!
//! - **stub / `unimplemented!()` / `// TODO 후속 단계` 우회 금지**
//! - 추측 금지 — raw decompile / asm / vtable dump 만 신뢰
//! - 모든 method 는 raw asm 을 doc comment 로 인용 후 1:1 포팅
//! - byte-equivalent 보장: bit width, signed/unsigned, overflow 동작 모두 raw 와 동일
//!
//! # 모듈
//!
//! - `flag`: `Hnc::Type::Flag` (libHncFoundation, 8B u64) — bit-flag container
//! - `bw_mode`: `Hnc::Shape::BWMode` (libHncDrawingEngine, u32 enum) + RenderMode 변환

pub mod flag;
pub mod bw_mode;
pub mod guid;
pub mod share_ptr;
pub mod string_w;
pub mod hit;
pub mod theme;
pub mod color_effect;
pub mod rb_tree;
pub mod scheme_style;
pub mod drawing_type;
pub mod color;
pub mod color_scheme;
pub mod text_font;
pub mod font_set;
pub mod format_scheme;
pub mod object_defaults;
pub mod font_scheme;
pub mod brush;
pub mod pen;
pub mod effect_style;
pub mod property_key;
pub mod property;
pub mod property_bag;
pub mod gradient_stop;
pub mod effects_container;
pub mod outer_shadow;
pub mod reflection;
pub mod surface;
pub mod svg_surface;
pub mod degree;
pub mod matrix3;
pub mod transform2d;
pub mod shape_engine;
pub mod render_util;
pub mod body_property;
pub mod path;
pub mod math_util;
pub mod pixel_util;
pub mod shape_render_converter;
pub mod scene3d;
pub mod sp3d;
pub mod blip_glyph;
pub mod char_item_view;
pub mod paths;
pub mod path_util;
pub mod logical_position;
pub mod shape_segment;
pub mod shape_path;
pub mod text_real_font;
pub mod run_property;
pub mod transformation;
pub mod calc_draw_variables;
pub mod char_item_view_effects;
pub mod char_item_view_draw;
pub mod char_item_view_draw_direct;
pub mod char_item_view_get_cached_render_path;
pub mod char_item_view_draw_underline;
pub mod char_item_view_bounds;
pub mod char_item_view_undraw;
pub mod char_item_view_pick;
pub mod char_item_view_source_rect;
pub mod shape_render_brush;
pub mod ttf_path_resolver;
pub mod equation;
pub mod pixel_diff_harness;

pub use flag::Flag;
pub use bw_mode::{BWMode, RenderMode, to_fill_render_mode, to_outline_render_mode};
pub use guid::Guid;
pub use share_ptr::{ControlBlock, SharePtr};
pub use string_w::CHncStringW;
pub use hit::Hit;
pub use theme::{THEME_SIZE_BYTES, THEME_ALIGN_BYTES};
pub use color_effect::{ColorEffect, COLOR_EFFECT_SIZE_BYTES, COLOR_EFFECT_ALIGN_BYTES};
pub use rb_tree::{
    tree_next, tree_remove, TreeBase, TreeNodeBase, TREE_BASE_SIZE_BYTES,
    TREE_NODE_BASE_SIZE_BYTES,
};
pub use scheme_style::SchemeStyle;
pub use drawing_type::{Cmyk, Hsl, Rgb, ScRgb};
pub use color::{
    color_type, Color, PresetStyle, SystemStyle, COLOR_ALIGN_BYTES, COLOR_SIZE_BYTES,
};
pub use color_scheme::{
    ColorScheme, ColorSchemeNode, COLOR_SCHEME_NODE_SIZE_BYTES, COLOR_SCHEME_SIZE_BYTES,
};
pub use text_font::{TextFont, TEXT_FONT_ALIGN_BYTES, TEXT_FONT_SIZE_BYTES};
pub use font_set::{
    FontSet, SupplementalFont, FONT_SET_ALIGN_BYTES, FONT_SET_SIZE_BYTES,
    SUP_VEC_INITIAL_CAPACITY,
};
pub use format_scheme::{
    BrushControlBlock, FormatScheme, FormatSchemeStyle, FsBrushMapNode,
    BRUSH_CONTROL_BLOCK_ALIGN_BYTES, BRUSH_CONTROL_BLOCK_SIZE_BYTES, FORMAT_SCHEME_ALIGN_BYTES,
    FORMAT_SCHEME_SIZE_BYTES, FS_BRUSH_MAP_NODE_ALIGN_BYTES, FS_BRUSH_MAP_NODE_SIZE_BYTES,
};
pub use object_defaults::{
    DefaultProperty, ObjectDefaults, OBJECT_DEFAULTS_ALIGN_BYTES, OBJECT_DEFAULTS_SIZE_BYTES,
};
pub use font_scheme::{FontScheme, FONT_SCHEME_ALIGN_BYTES, FONT_SCHEME_SIZE_BYTES};
pub use brush::{
    brush_vtable, Brush, BrushType, BrushVtable, EmptyBrush, GradientBrush, GroupBrush,
    HatchBrush, ImageBrush, SolidBrush, HATCH_BRUSH_VTABLE, SOLID_BRUSH_VTABLE,
};
pub use pen::{
    ArrowSizeStyle, ArrowStyle, DashStyle, LineCapStyle, LineJoinStyle, Pen, PenAlignStyle,
    PenCompoundStyle,
};
pub use effect_style::{
    EffectStyle, Effects, Scene3D, Sp3D, EFFECT_STYLE_ALIGN_BYTES, EFFECT_STYLE_SIZE_BYTES,
};
pub use property_key::{PropertyKey, PROPERTY_KEY_ALIGN_BYTES, PROPERTY_KEY_SIZE_BYTES};
pub use property::{
    state, PBool, PColor, PEnum, PFloat, PStops, PVec4, Property, PBOOL_ALIGN_BYTES,
    PBOOL_SIZE_BYTES, PCOLOR_ALIGN_BYTES, PCOLOR_SIZE_BYTES, PENUM_ALIGN_BYTES, PENUM_SIZE_BYTES,
    PFLOAT_ALIGN_BYTES, PFLOAT_SIZE_BYTES, PROPERTY_ALIGN_BYTES, PROPERTY_SIZE_BYTES,
    PSTOPS_ALIGN_BYTES, PSTOPS_SIZE_BYTES, PVEC4_ALIGN_BYTES, PVEC4_SIZE_BYTES,
};
pub use property_bag::{
    PropertyBag, PropertyBagImpl, PropertyBagNode, PROPERTY_BAG_ALIGN_BYTES,
    PROPERTY_BAG_IMPL_ALIGN_BYTES, PROPERTY_BAG_IMPL_SIZE_BYTES,
    PROPERTY_BAG_NODE_ALIGN_BYTES, PROPERTY_BAG_NODE_SIZE_BYTES, PROPERTY_BAG_SIZE_BYTES,
};
pub use gradient_stop::{
    GradientStop, GradientStopCtrl, GradientStopsVec, GRADIENT_STOPS_INITIAL_CAPACITY_BYTES,
    GRADIENT_STOPS_INITIAL_CAPACITY_ELEMS, GRADIENT_STOPS_VEC_ALIGN_BYTES,
    GRADIENT_STOPS_VEC_SIZE_BYTES, GRADIENT_STOP_ALIGN_BYTES, GRADIENT_STOP_CTRL_ALIGN_BYTES,
    GRADIENT_STOP_CTRL_SIZE_BYTES, GRADIENT_STOP_SIZE_BYTES,
};

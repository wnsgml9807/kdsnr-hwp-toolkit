//! 한컴 수식 스크립트 파싱 및 렌더링
//!
//! 수식 스크립트(버전 6.0)를 토큰화하고 AST로 변환한 뒤 SVG로 렌더링한다.
//! 참조: openhwp/docs/hwpx/appendix-i-formula.md

pub mod ast;
#[cfg(target_arch = "wasm32")]
pub mod canvas_render;
pub mod layout;
pub mod parser;
pub mod svg_render;
pub mod symbols;
pub mod tokenizer;

use crate::model::control::Equation;
use crate::renderer::hwpunit_to_px;
use layout::{EqLayout, LayoutBox};

/// HWPX 수식의 base font size 결정 + AST → layout 변환.
///
/// HWPX `<hp:equation baseUnit="…">` 값을 픽셀로 변환해서 layout 의 fs 로 사용한다.
/// 실제 렌더 시점의 비례 스케일링은 svg.rs Equation 분기에서 처리.
pub fn build_equation_layout(eq: &Equation, dpi: f64) -> (f64, LayoutBox) {
    let tokens = tokenizer::tokenize(&eq.script);
    let ast = parser::EqParser::new(tokens).parse();
    let fs_px = hwpunit_to_px(eq.font_size as i32, dpi);
    let layout_box = EqLayout::new(fs_px).layout(&ast);
    (fs_px, layout_box)
}

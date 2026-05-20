//! 줄 나눔 엔진 (Line Breaking Engine)
//!
//! 문단 텍스트를 토큰화하고 줄 나눔을 수행한다.
//! 한글 어절/글자, 영어 단어/하이픈, CJK 개별 분할을 지원한다.

use super::{find_active_char_shape, is_lang_neutral};
use crate::model::paragraph::{CharShapeRef, LineSeg, Paragraph};
use crate::model::style::LineSpacingType;
use crate::renderer::layout::{
    estimate_text_width, estimate_text_width_unrounded, is_cjk_char, resolved_to_text_style,
};
use crate::renderer::px_to_hwpunit;
use crate::renderer::style_resolver::{detect_lang_category, ResolvedStyleSet};

/// 줄 나눔 토큰
#[derive(Debug, Clone)]
pub(crate) enum BreakToken {
    /// 분할 불가 텍스트 조각 (어절/단어/글자)
    /// char_widths: 글자별 px 폭 (char_level_break용, 단일 글자 토큰은 비어있음)
    Text {
        start_idx: usize,
        end_idx: usize,
        width: f64,
        max_font_size: f64,
        char_widths: Vec<f64>,
    },
    /// 공백 (줄 바꿈 가능 지점, 줄 끝에서 흡수)
    Space {
        idx: usize,
        width: f64,
        max_font_size: f64,
    },
    /// 탭 (줄 바꿈 가능 지점, 폭은 줄 위치에 따라 동적)
    Tab { idx: usize, max_font_size: f64 },
    /// 강제 줄 바꿈 (\n)
    LineBreak { idx: usize },
}

/// 줄 채움 결과
#[derive(Debug)]
struct LineBreakResult {
    start_idx: usize,
    end_idx: usize, // exclusive
    max_font_size: f64,
    has_line_break: bool, // 강제 줄 바꿈 여부
}

/// 줄 머리 금칙: 줄 시작에 올 수 없는 문자
pub(crate) fn is_line_start_forbidden(ch: char) -> bool {
    matches!(
        ch,
        ')' | ']'
            | '}'
            | ','
            | '.'
            | '!'
            | '?'
            | ';'
            | ':'
            | '\''
            | '"'
            | '\u{3001}'
            | '\u{3002}'
            | '\u{2026}'
            | '\u{00B7}'
            | '\u{2015}'
            | '\u{30FC}'
            | '\u{300B}'
            | '\u{300D}'
            | '\u{300F}'
            | '\u{3011}'
            | '\u{FF09}'
            | '\u{FF5D}'
            | '\u{3015}'
            | '\u{3009}'
            | '\u{FF1E}'
            | '\u{226B}'
            | '\u{FF3D}'
            | '\u{FE5E}'
            | '\u{301E}'
            | '\u{2019}'
            | '\u{201D}'
            | '\u{FF0C}'
            | '\u{FF0E}'
            | '\u{FF01}'
            | '\u{FF1F}'
            | '\u{FF1B}'
            | '\u{FF1A}'
            | '%'
            | '\u{2030}'
            | '\u{2103}'
            | '\u{00B0}'
            | '\u{FF05}'
    )
}

/// 줄 꼬리 금칙: 줄 끝에 올 수 없는 문자
pub(crate) fn is_line_end_forbidden(ch: char) -> bool {
    matches!(
        ch,
        '(' | '['
            | '{'
            | '\''
            | '"'
            | '\u{300A}'
            | '\u{300C}'
            | '\u{300E}'
            | '\u{3010}'
            | '\u{FF08}'
            | '\u{FF5B}'
            | '\u{3014}'
            | '\u{3008}'
            | '\u{FF1C}'
            | '\u{226A}'
            | '\u{FF3B}'
            | '\u{301D}'
            | '\u{2018}'
            | '\u{201C}'
            | '$'
            | '\u{20A9}'
            | '\u{00A3}'
            | '\u{20AC}'
            | '\u{00A5}'
            | '\u{FF04}'
            | '\u{FFE5}'
    )
}

/// 한글 음절/자모 여부 (옛한글 확장 자모 포함)
fn is_hangul(ch: char) -> bool {
    ('\u{AC00}'..='\u{D7A3}').contains(&ch)       // 한글 음절
        || ('\u{1100}'..='\u{11FF}').contains(&ch) // 한글 자모
        || ('\u{3130}'..='\u{318F}').contains(&ch) // 한글 호환 자모 (ㆍ U+318D 포함)
        || ('\u{A960}'..='\u{A97F}').contains(&ch) // 한글 자모 확장-A (옛한글 초성)
        || ('\u{D7B0}'..='\u{D7FF}').contains(&ch) // 한글 자모 확장-B (옛한글 중/종성)
}

/// 라틴 문자 여부 (영문+숫자)
fn is_latin(ch: char) -> bool {
    let lang = detect_lang_category(ch);
    lang == 1 // English/Latin
}

/// CJK 문자 여부 (한자/일본어 — 개별 분할 대상)
fn is_cjk_ideograph(ch: char) -> bool {
    let lang = detect_lang_category(ch);
    lang == 2 || lang == 3 // Chinese or Japanese
}

/// 문단 텍스트를 줄 나눔 토큰으로 분할한다.
pub(crate) fn tokenize_paragraph(
    text_chars: &[char],
    char_offsets: &[u32],
    char_shapes: &[CharShapeRef],
    styles: &ResolvedStyleSet,
    english_break_unit: u8,
    korean_break_unit: u8,
) -> Vec<BreakToken> {
    let text_len = text_chars.len();
    if text_len == 0 {
        return Vec::new();
    }

    let mut tokens = Vec::new();
    let mut i = 0;
    let mut current_lang: usize = 0;

    while i < text_len {
        let ch = text_chars[i];

        // 강제 줄 바꿈
        if ch == '\n' {
            tokens.push(BreakToken::LineBreak { idx: i });
            i += 1;
            continue;
        }

        // 탭
        if ch == '\t' {
            let utf16_pos = if i < char_offsets.len() {
                char_offsets[i]
            } else {
                i as u32
            };
            let style_id = find_active_char_shape(char_shapes, utf16_pos);
            let ts = resolved_to_text_style(styles, style_id, current_lang);
            let font_size = if ts.font_size > 0.0 {
                ts.font_size
            } else {
                12.0
            };
            tokens.push(BreakToken::Tab {
                idx: i,
                max_font_size: font_size,
            });
            i += 1;
            continue;
        }

        // 공백 (줄 바꿈 지점) — NonBreakingSpace(\u{00A0})는 제외
        if ch == ' ' {
            let utf16_pos = if i < char_offsets.len() {
                char_offsets[i]
            } else {
                i as u32
            };
            let style_id = find_active_char_shape(char_shapes, utf16_pos);
            let ts = resolved_to_text_style(styles, style_id, current_lang);
            let font_size = if ts.font_size > 0.0 {
                ts.font_size
            } else {
                12.0
            };
            let w = estimate_text_width_unrounded(" ", &ts);
            tokens.push(BreakToken::Space {
                idx: i,
                width: w,
                max_font_size: font_size,
            });
            i += 1;
            continue;
        }

        // 한글 어절 또는 글자
        if is_hangul(ch) {
            if korean_break_unit == 0 {
                // 어절 모드: 연속 한글 + 후행 금칙 문자를 하나의 토큰으로
                let start = i;
                let mut max_fs = 0.0f64;
                let mut token_text = String::new();
                let mut token_lang = current_lang;

                while i < text_len {
                    let c = text_chars[i];
                    if c == ' ' || c == '\n' || c == '\t' {
                        break;
                    }
                    // 한글이 아니고 라틴이면 다른 토큰으로 분리
                    if !is_hangul(c) && is_latin(c) {
                        break;
                    }
                    // CJK 한자/일본어는 개별 토큰
                    if is_cjk_ideograph(c) {
                        break;
                    }

                    let utf16_pos = if i < char_offsets.len() {
                        char_offsets[i]
                    } else {
                        i as u32
                    };
                    let style_id = find_active_char_shape(char_shapes, utf16_pos);
                    let lang = if is_lang_neutral(c) {
                        token_lang
                    } else {
                        let detected = detect_lang_category(c);
                        token_lang = detected;
                        current_lang = detected;
                        detected
                    };
                    let ts = resolved_to_text_style(styles, style_id, lang);
                    let fs = if ts.font_size > 0.0 {
                        ts.font_size
                    } else {
                        12.0
                    };
                    if fs > max_fs {
                        max_fs = fs;
                    }
                    token_text.push(c);
                    i += 1;
                }

                // 후행 금칙 문자 (줄 머리 금칙) 흡수
                while i < text_len
                    && is_line_start_forbidden(text_chars[i])
                    && text_chars[i] != '\n'
                    && text_chars[i] != '\t'
                {
                    let c = text_chars[i];
                    let utf16_pos = if i < char_offsets.len() {
                        char_offsets[i]
                    } else {
                        i as u32
                    };
                    let style_id = find_active_char_shape(char_shapes, utf16_pos);
                    let lang = if is_lang_neutral(c) {
                        current_lang
                    } else {
                        let detected = detect_lang_category(c);
                        current_lang = detected;
                        detected
                    };
                    let ts = resolved_to_text_style(styles, style_id, lang);
                    let fs = if ts.font_size > 0.0 {
                        ts.font_size
                    } else {
                        12.0
                    };
                    if fs > max_fs {
                        max_fs = fs;
                    }
                    token_text.push(c);
                    i += 1;
                }

                if !token_text.is_empty() {
                    let width = measure_token_width(
                        &token_text,
                        start,
                        char_offsets,
                        char_shapes,
                        styles,
                        current_lang,
                    );
                    tokens.push(BreakToken::Text {
                        start_idx: start,
                        end_idx: i,
                        width,
                        max_font_size: max_fs,
                        char_widths: vec![],
                    });
                }
                continue;
            } else {
                // 글자 모드: 한글 개별 분할
                let utf16_pos = if i < char_offsets.len() {
                    char_offsets[i]
                } else {
                    i as u32
                };
                let style_id = find_active_char_shape(char_shapes, utf16_pos);
                current_lang = detect_lang_category(ch);
                let ts = resolved_to_text_style(styles, style_id, current_lang);
                let fs = if ts.font_size > 0.0 {
                    ts.font_size
                } else {
                    12.0
                };
                let w = estimate_text_width_unrounded(&ch.to_string(), &ts);
                tokens.push(BreakToken::Text {
                    start_idx: i,
                    end_idx: i + 1,
                    width: w,
                    max_font_size: fs,
                    char_widths: vec![],
                });
                i += 1;
                continue;
            }
        }

        // 라틴 단어 또는 글자
        if is_latin(ch) {
            if english_break_unit == 0 || english_break_unit == 1 {
                // 단어/하이픈 모드: 연속 라틴 문자를 하나의 토큰으로
                let start = i;
                let mut max_fs = 0.0f64;
                let mut token_text = String::new();

                while i < text_len {
                    let c = text_chars[i];
                    if c == ' ' || c == '\n' || c == '\t' {
                        break;
                    }
                    if !is_latin(c) && !is_lang_neutral(c) {
                        break;
                    }
                    // 하이픈 모드: 하이픈에서 분할 (하이픈 포함 후 분리)
                    if english_break_unit == 1 && c == '-' && !token_text.is_empty() {
                        let utf16_pos = if i < char_offsets.len() {
                            char_offsets[i]
                        } else {
                            i as u32
                        };
                        let style_id = find_active_char_shape(char_shapes, utf16_pos);
                        let lang = 1usize; // English
                        let ts = resolved_to_text_style(styles, style_id, lang);
                        let fs = if ts.font_size > 0.0 {
                            ts.font_size
                        } else {
                            12.0
                        };
                        if fs > max_fs {
                            max_fs = fs;
                        }
                        token_text.push(c);
                        i += 1;
                        break; // 하이픈 뒤에서 분할
                    }

                    let utf16_pos = if i < char_offsets.len() {
                        char_offsets[i]
                    } else {
                        i as u32
                    };
                    let style_id = find_active_char_shape(char_shapes, utf16_pos);
                    let lang = if is_lang_neutral(c) {
                        current_lang
                    } else {
                        current_lang = 1; // English
                        1
                    };
                    let ts = resolved_to_text_style(styles, style_id, lang);
                    let fs = if ts.font_size > 0.0 {
                        ts.font_size
                    } else {
                        12.0
                    };
                    if fs > max_fs {
                        max_fs = fs;
                    }
                    token_text.push(c);
                    i += 1;
                }

                if !token_text.is_empty() {
                    let width = measure_token_width(
                        &token_text,
                        start,
                        char_offsets,
                        char_shapes,
                        styles,
                        current_lang,
                    );
                    // 개별 글자 폭 수집 (char_level_break용)
                    let cw: Vec<f64> = (start..i)
                        .map(|ci| {
                            let c = text_chars[ci];
                            let u16p = if ci < char_offsets.len() {
                                char_offsets[ci]
                            } else {
                                ci as u32
                            };
                            let sid = find_active_char_shape(char_shapes, u16p);
                            let lang = if is_lang_neutral(c) { current_lang } else { 1 };
                            let ts = resolved_to_text_style(styles, sid, lang);
                            estimate_text_width_unrounded(&c.to_string(), &ts)
                        })
                        .collect();
                    tokens.push(BreakToken::Text {
                        start_idx: start,
                        end_idx: i,
                        width,
                        max_font_size: max_fs,
                        char_widths: cw,
                    });
                }
                continue;
            } else {
                // 글자 모드
                let utf16_pos = if i < char_offsets.len() {
                    char_offsets[i]
                } else {
                    i as u32
                };
                let style_id = find_active_char_shape(char_shapes, utf16_pos);
                current_lang = 1;
                let ts = resolved_to_text_style(styles, style_id, current_lang);
                let fs = if ts.font_size > 0.0 {
                    ts.font_size
                } else {
                    12.0
                };
                let w = estimate_text_width_unrounded(&ch.to_string(), &ts);
                tokens.push(BreakToken::Text {
                    start_idx: i,
                    end_idx: i + 1,
                    width: w,
                    max_font_size: fs,
                    char_widths: vec![],
                });
                i += 1;
                continue;
            }
        }

        // CJK 한자/일본어: 항상 개별 토큰
        if is_cjk_ideograph(ch) {
            let utf16_pos = if i < char_offsets.len() {
                char_offsets[i]
            } else {
                i as u32
            };
            let style_id = find_active_char_shape(char_shapes, utf16_pos);
            current_lang = detect_lang_category(ch);
            let ts = resolved_to_text_style(styles, style_id, current_lang);
            let fs = if ts.font_size > 0.0 {
                ts.font_size
            } else {
                12.0
            };
            let w = estimate_text_width_unrounded(&ch.to_string(), &ts);
            tokens.push(BreakToken::Text {
                start_idx: i,
                end_idx: i + 1,
                width: w,
                max_font_size: fs,
                char_widths: vec![],
            });
            i += 1;
            continue;
        }

        // 기타 문자 (기호, NonBreakingSpace 등): 개별 Text 토큰
        {
            let utf16_pos = if i < char_offsets.len() {
                char_offsets[i]
            } else {
                i as u32
            };
            let style_id = find_active_char_shape(char_shapes, utf16_pos);
            let lang = if is_lang_neutral(ch) {
                current_lang
            } else {
                let detected = detect_lang_category(ch);
                current_lang = detected;
                detected
            };
            let ts = resolved_to_text_style(styles, style_id, lang);
            let fs = if ts.font_size > 0.0 {
                ts.font_size
            } else {
                12.0
            };
            let w = estimate_text_width_unrounded(&ch.to_string(), &ts);
            tokens.push(BreakToken::Text {
                start_idx: i,
                end_idx: i + 1,
                width: w,
                max_font_size: fs,
                char_widths: vec![],
            });
            i += 1;
        }
    }

    tokens
}

/// 토큰 텍스트의 폭을 글자별 언어 인식 측정으로 합산한다.
fn measure_token_width(
    text: &str,
    start_char_idx: usize,
    char_offsets: &[u32],
    char_shapes: &[CharShapeRef],
    styles: &ResolvedStyleSet,
    default_lang: usize,
) -> f64 {
    let mut total = 0.0;
    let mut current_lang = default_lang;
    for (offset, ch) in text.chars().enumerate() {
        let idx = start_char_idx + offset;
        let utf16_pos = if idx < char_offsets.len() {
            char_offsets[idx]
        } else {
            idx as u32
        };
        let style_id = find_active_char_shape(char_shapes, utf16_pos);
        let lang = if is_lang_neutral(ch) {
            current_lang
        } else {
            let detected = detect_lang_category(ch);
            current_lang = detected;
            detected
        };
        let ts = resolved_to_text_style(styles, style_id, lang);
        total += estimate_text_width_unrounded(&ch.to_string(), &ts);
    }
    total
}

/// px를 HWPUNIT(i32)로 변환 (내림, DPI=96 기준: px * 75)
#[inline]
fn to_hwp(px: f64) -> i32 {
    (px * 75.0) as i32
}

/// G-W-3 helper: ppt_compose_break (한컴 byte-eq line break) 결과를
/// 우리 LineBreakResult 형태로 변환. **per-char element model**: 한글 한 글자씩
/// 풀어서 ppt_compose_break 에 전달 → intra-Text break 가 가능해진다.
///
/// 호출처는 reflow_line_segs 한 곳 (env-gate `RHWP_USE_KDSNR_LAYOUT`).
#[cfg(not(target_arch = "wasm32"))]
fn compute_kdsnr_breaks(
    tokens: &[BreakToken],
    text_chars: &[char],
    char_offsets: &[u32],
    char_shapes: &[crate::model::paragraph::CharShapeRef],
    styles: &ResolvedStyleSet,
    available_width_px: f64,
    para_style: Option<&crate::renderer::style_resolver::ResolvedParaStyle>,
) -> Vec<LineBreakResult> {
    use kdsnr_layout::ppt_compose_break;

    // per-char element: (width, penalty, char_idx_in_text, max_fs, is_force_break)
    // penalty=2 (no break-after), 1 (break-after OK), 0 (force break / line break)
    #[derive(Clone, Copy)]
    struct Elem {
        width: f32,
        penalty: i32,
        start_char_idx: usize, // char idx (inclusive) at start of this elem
        end_char_idx: usize,   // char idx (exclusive) at end of this elem
        max_fs: f64,
        has_lb: bool,
    }

    // 한컴 char_class 단순화 — Korean / CJK / ASCII letter,digit 구분.
    // ⓘ 완벽한 byte-eq 는 아니지만, 한글 사이는 break-OK, ASCII 단어 안은 no-break.
    fn is_korean(ch: char) -> bool {
        let c = ch as u32;
        (0xAC00..=0xD7A3).contains(&c) // Hangul Syllables
            || (0x1100..=0x11FF).contains(&c) // Hangul Jamo
            || (0x3130..=0x318F).contains(&c) // Hangul Compatibility Jamo
            || (0x3000..=0x303F).contains(&c) // CJK punctuation
            || (0x4E00..=0x9FFF).contains(&c) // CJK Unified
    }
    fn is_line_start_forbidden_local(ch: char) -> bool {
        matches!(
            ch,
            ')' | ']'
                | '}'
                | ','
                | '.'
                | '!'
                | '?'
                | ';'
                | ':'
                | '\''
                | '"'
                | '\u{3001}'
                | '\u{3002}'
                | '\u{2026}'
                | '\u{FF09}'
                | '\u{FF3D}'
                | '\u{FF5D}'
        )
    }

    let mut elems: Vec<Elem> = Vec::new();

    for token in tokens {
        match token {
            BreakToken::Text {
                start_idx,
                end_idx,
                width,
                max_font_size,
                char_widths,
            } => {
                let n_chars = end_idx.saturating_sub(*start_idx);
                let first_ch = text_chars.get(*start_idx).copied().unwrap_or(' ');
                let token_is_korean = is_korean(first_ch);
                // per-char expand: Latin word (with char_widths) 만 단어 통째로 = token-level 안전.
                // Korean 어절 mode (char_widths 비어 있음) — 인라인 per-char width 계산은 TTF
                // 메트릭이 Hancom HFT 와 어긋나 Q11 회귀 → token-level 유지.
                if token_is_korean
                    && !char_widths.is_empty()
                    && char_widths.len() == n_chars
                    && n_chars > 1
                {
                    for i in 0..n_chars {
                        let ci = *start_idx + i;
                        let next_ch = text_chars.get(ci + 1).copied().unwrap_or(' ');
                        let pen = if i == n_chars - 1 {
                            2
                        } else if is_line_start_forbidden_local(next_ch) {
                            2
                        } else {
                            1
                        };
                        elems.push(Elem {
                            width: char_widths[i] as f32,
                            penalty: pen,
                            start_char_idx: ci,
                            end_char_idx: ci + 1,
                            max_fs: *max_font_size,
                            has_lb: false,
                        });
                    }
                } else {
                    // Latin word OR Korean 어절 token-level — 통째로 elem 1 개.
                    elems.push(Elem {
                        width: *width as f32,
                        penalty: 2,
                        start_char_idx: *start_idx,
                        end_char_idx: *end_idx,
                        max_fs: *max_font_size,
                        has_lb: false,
                    });
                }
            }
            BreakToken::Space {
                idx,
                width,
                max_font_size,
            } => {
                elems.push(Elem {
                    width: *width as f32,
                    penalty: 1,
                    start_char_idx: *idx,
                    end_char_idx: *idx + 1,
                    max_fs: *max_font_size,
                    has_lb: false,
                });
            }
            BreakToken::Tab { idx, max_font_size } => {
                elems.push(Elem {
                    width: 0.0,
                    penalty: 1,
                    start_char_idx: *idx,
                    end_char_idx: *idx + 1,
                    max_fs: *max_font_size,
                    has_lb: false,
                });
            }
            BreakToken::LineBreak { idx } => {
                elems.push(Elem {
                    width: 0.0,
                    penalty: 0,
                    start_char_idx: *idx,
                    end_char_idx: *idx + 1,
                    max_fs: 0.0,
                    has_lb: true,
                });
            }
        }
    }

    if elems.is_empty() {
        return vec![LineBreakResult {
            start_idx: 0,
            end_idx: text_chars.len(),
            max_font_size: 0.0,
            has_line_break: false,
        }];
    }

    let widths: Vec<f32> = elems.iter().map(|e| e.width).collect();
    let penalties: Vec<i32> = elems.iter().map(|e| e.penalty).collect();
    let heights = vec![available_width_px as f32; widths.len().max(1)];
    let composition = crate::renderer::kdsnr_bridge::build_para_composition(para_style);

    let breaks = ppt_compose_break(
        &widths,
        &[],
        &[],
        &penalties,
        &heights,
        &composition,
        0,
        (widths.len() as i32).saturating_sub(1),
    );

    // breaks → LineBreakResult. breaks[i] = 마지막 elem idx (inclusive).
    let mut results: Vec<LineBreakResult> = Vec::new();
    let mut start_elem: usize = 0;
    let total = elems.len();

    let push_line = |results: &mut Vec<LineBreakResult>, s: usize, e: usize, elems: &[Elem]| {
        if s >= total || s > e {
            return;
        }
        let line_elems = &elems[s..=e];
        let char_start = line_elems[0].start_char_idx;
        let char_end = line_elems
            .last()
            .map(|e| e.end_char_idx)
            .unwrap_or(char_start);
        let mut max_fs = 0.0_f64;
        let mut has_lb = false;
        for el in line_elems {
            if el.max_fs > max_fs {
                max_fs = el.max_fs;
            }
            if el.has_lb {
                has_lb = true;
            }
        }
        results.push(LineBreakResult {
            start_idx: char_start,
            end_idx: char_end,
            max_font_size: max_fs,
            has_line_break: has_lb,
        });
    };

    for &br in &breaks {
        let end_elem = br as usize;
        if end_elem >= total {
            break;
        }
        push_line(&mut results, start_elem, end_elem, &elems);
        start_elem = end_elem + 1;
    }
    if start_elem < total {
        push_line(&mut results, start_elem, total - 1, &elems);
    }
    if results.is_empty() {
        results.push(LineBreakResult {
            start_idx: 0,
            end_idx: text_chars.len(),
            max_font_size: 0.0,
            has_line_break: false,
        });
    }
    results
}

/// 토큰을 줄에 배치하는 Greedy 알고리즘
/// 한컴과 동일한 결과를 위해 HWPUNIT 정수로 폭을 누적한다.
fn fill_lines(
    tokens: &[BreakToken],
    text_chars: &[char],
    available_width_px: f64,
    indent_px: f64,
    default_tab_width: f64,
    korean_break_unit: u8,
) -> Vec<LineBreakResult> {
    if tokens.is_empty() {
        return vec![LineBreakResult {
            start_idx: 0,
            end_idx: 0,
            max_font_size: 0.0,
            has_line_break: false,
        }];
    }

    let tab_w_hwp = to_hwp(if default_tab_width > 0.0 {
        default_tab_width
    } else {
        48.0
    });
    let tab_w_px = if default_tab_width > 0.0 {
        default_tab_width
    } else {
        48.0
    };
    let mut results = Vec::new();
    let mut line_start_idx = 0usize;
    let mut lw = 0i32; // HWPUNIT 정수 누적
    let mut line_max_fs = 0.0f64;
    let mut is_first_line = true;

    let mut last_break_token_idx: Option<usize> = None;
    let mut last_break_char_idx: usize = 0;
    let mut width_at_last_break = 0i32;
    let mut fs_at_last_break = 0.0f64;

    let eff_w = |first: bool| -> i32 {
        if indent_px > 0.0 {
            if first {
                to_hwp((available_width_px - indent_px).max(1.0))
            } else {
                to_hwp(available_width_px)
            }
        } else if indent_px < 0.0 {
            if first {
                to_hwp(available_width_px)
            } else {
                to_hwp((available_width_px + indent_px).max(1.0))
            }
        } else {
            to_hwp(available_width_px)
        }
    };

    for (ti, token) in tokens.iter().enumerate() {
        match token {
            BreakToken::LineBreak { idx } => {
                results.push(LineBreakResult {
                    start_idx: line_start_idx,
                    end_idx: *idx + 1,
                    max_font_size: line_max_fs,
                    has_line_break: true,
                });
                line_start_idx = *idx + 1;
                lw = 0;
                line_max_fs = 0.0;
                is_first_line = false;
                last_break_token_idx = None;
            }
            BreakToken::Tab { idx, max_font_size } => {
                // 탭 계산은 px로 수행 후 HWPUNIT 변환 (정밀도 유지)
                let lw_px = lw as f64 / 75.0;
                let next_tab_px = ((lw_px / tab_w_px).floor() + 1.0) * tab_w_px;
                let next_tab_hwp = to_hwp(next_tab_px);
                if *max_font_size > line_max_fs {
                    line_max_fs = *max_font_size;
                }

                if next_tab_hwp > eff_w(is_first_line) && line_start_idx < *idx {
                    if let Some(_) = last_break_token_idx {
                        results.push(LineBreakResult {
                            start_idx: line_start_idx,
                            end_idx: last_break_char_idx,
                            max_font_size: fs_at_last_break,
                            has_line_break: false,
                        });
                        line_start_idx = last_break_char_idx;
                        lw = lw - width_at_last_break;
                    } else {
                        results.push(LineBreakResult {
                            start_idx: line_start_idx,
                            end_idx: *idx,
                            max_font_size: line_max_fs,
                            has_line_break: false,
                        });
                        line_start_idx = *idx;
                        lw = 0;
                        line_max_fs = *max_font_size;
                    }
                    is_first_line = false;
                    last_break_token_idx = None;
                    let lw_px2 = lw as f64 / 75.0;
                    let next_tab2 = ((lw_px2 / tab_w_px).floor() + 1.0) * tab_w_px;
                    lw = to_hwp(next_tab2);
                } else {
                    last_break_token_idx = Some(ti);
                    last_break_char_idx = *idx;
                    width_at_last_break = lw;
                    fs_at_last_break = line_max_fs;
                    lw = next_tab_hwp;
                }
            }
            BreakToken::Space {
                idx,
                width,
                max_font_size,
            } => {
                if *max_font_size > line_max_fs {
                    line_max_fs = *max_font_size;
                }
                last_break_token_idx = Some(ti);
                last_break_char_idx = *idx;
                width_at_last_break = lw;
                fs_at_last_break = line_max_fs;
                lw += to_hwp(*width);
            }
            BreakToken::Text {
                start_idx,
                end_idx,
                width,
                max_font_size,
                ref char_widths,
            } => {
                if *max_font_size > line_max_fs {
                    line_max_fs = *max_font_size;
                }

                let w_hwp = to_hwp(*width);

                // 단일 문자 CJK/한글 토큰의 줄바꿈 가능 지점 처리
                // 이 글자를 포함한 후 break point 갱신 (end_idx 사용)
                // → 초과 시 이 글자까지 L0에 포함하고 다음 토큰부터 다음 줄
                if *end_idx - *start_idx == 1 && *start_idx > line_start_idx {
                    let c = text_chars[*start_idx];
                    let allow_break = if is_hangul(c) {
                        korean_break_unit == 1
                    } else {
                        is_cjk_ideograph(c)
                    };
                    // 이 글자가 줄에 들어가는 경우에만 break point 갱신
                    if allow_break && lw + w_hwp <= eff_w(is_first_line) + LINE_BREAK_TOLERANCE {
                        last_break_token_idx = Some(ti);
                        last_break_char_idx = *end_idx; // 이 글자 다음 (이 글자 포함)
                        width_at_last_break = lw + w_hwp; // 이 글자 폭 포함
                        fs_at_last_break = line_max_fs;
                    }
                }
                // 한컴은 HWPUNIT 정수 양자화 시 미세한 반올림 차이를 허용
                // 12 HU(~0.17mm) 이내의 초과는 줄에 포함 (경험적 허용 오차)
                const LINE_BREAK_TOLERANCE: i32 = 15;
                if lw + w_hwp > eff_w(is_first_line) + LINE_BREAK_TOLERANCE {
                    if *start_idx > line_start_idx {
                        if let Some(_) = last_break_token_idx {
                            results.push(LineBreakResult {
                                start_idx: line_start_idx,
                                end_idx: last_break_char_idx,
                                max_font_size: fs_at_last_break,
                                has_line_break: false,
                            });
                            let mut next_start = last_break_char_idx;
                            while next_start < text_chars.len() && text_chars[next_start] == ' ' {
                                next_start += 1;
                            }
                            line_start_idx = next_start;
                            lw = recalc_width_hwp(tokens, ti, next_start);
                            lw += w_hwp;
                            line_max_fs = *max_font_size;
                            is_first_line = false;
                            last_break_token_idx = None;
                            continue;
                        }
                    }
                    // 토큰에 저장된 개별 글자 폭을 HWPUNIT로 변환
                    let cw_hwp: Vec<i32> = char_widths.iter().map(|w| to_hwp(*w)).collect();
                    let (results_part, remaining_w, remaining_fs) = char_level_break_hwp(
                        text_chars,
                        *start_idx,
                        *end_idx,
                        &mut line_start_idx,
                        lw,
                        line_max_fs,
                        eff_w(is_first_line),
                        eff_w(false),
                        is_first_line,
                        &cw_hwp,
                    );
                    for r in results_part {
                        results.push(r);
                        is_first_line = false;
                    }
                    lw = remaining_w;
                    line_max_fs = remaining_fs;
                    last_break_token_idx = None;
                    continue;
                } else {
                    lw += w_hwp;
                }
            }
        }
    }

    let last_end = tokens
        .last()
        .map(|t| match t {
            BreakToken::Text { end_idx, .. } => *end_idx,
            BreakToken::Space { idx, .. }
            | BreakToken::Tab { idx, .. }
            | BreakToken::LineBreak { idx } => *idx + 1,
        })
        .unwrap_or(text_chars.len());

    if line_start_idx <= last_end {
        results.push(LineBreakResult {
            start_idx: line_start_idx,
            end_idx: last_end,
            max_font_size: line_max_fs,
            has_line_break: false,
        });
    }

    if results.is_empty() {
        results.push(LineBreakResult {
            start_idx: 0,
            end_idx: text_chars.len(),
            max_font_size: 0.0,
            has_line_break: false,
        });
    }

    results
}

/// 줄 바꿈 지점 이후 토큰의 누적 폭 재계산 (HWPUNIT)
fn recalc_width_hwp(tokens: &[BreakToken], current_token_idx: usize, new_line_start: usize) -> i32 {
    let mut w = 0i32;
    for t in &tokens[..current_token_idx] {
        match t {
            BreakToken::Text {
                start_idx, width, ..
            } if *start_idx >= new_line_start => {
                w += to_hwp(*width);
            }
            BreakToken::Space { idx, width, .. } if *idx >= new_line_start => {
                w += to_hwp(*width);
            }
            _ => {}
        }
    }
    w
}

/// 긴 단어 폴백: 글자 단위 분할 (HWPUNIT)
/// char_widths_hwp: 토큰 내 각 글자의 HWPUNIT 폭 (None이면 휴리스틱)
fn char_level_break_hwp(
    text_chars: &[char],
    token_start: usize,
    token_end: usize,
    line_start_idx: &mut usize,
    mut lw: i32,
    mut line_max_fs: f64,
    first_line_w: i32,
    normal_w: i32,
    mut is_first_line: bool,
    char_widths_hwp: &[i32], // 토큰 내 글자별 HWPUNIT 폭
) -> (Vec<LineBreakResult>, i32, f64) {
    let mut results = Vec::new();
    let mut current_w = if is_first_line {
        first_line_w
    } else {
        normal_w
    };

    for ci in token_start..token_end {
        let rel_idx = ci - token_start;
        let char_w = if rel_idx < char_widths_hwp.len() {
            char_widths_hwp[rel_idx]
        } else {
            let ch = text_chars[ci];
            let char_w_px = if is_cjk_char(ch) {
                line_max_fs.max(12.0)
            } else {
                line_max_fs.max(12.0) * 0.5
            };
            to_hwp(char_w_px)
        };

        if lw + char_w > current_w && ci > *line_start_idx {
            results.push(LineBreakResult {
                start_idx: *line_start_idx,
                end_idx: ci,
                max_font_size: line_max_fs,
                has_line_break: false,
            });
            *line_start_idx = ci;
            lw = char_w;
            is_first_line = false;
            current_w = normal_w;
        } else {
            lw += char_w;
        }
    }

    (results, lw, line_max_fs)
}

/// 문단의 line_segs를 텍스트 내용과 컬럼 너비에 맞게 재계산한다.
///
/// 텍스트 편집(삽입/삭제) 후 호출하여 줄 바꿈을 재배치한다.
/// `available_width_px`는 문단 여백을 제외한 사용 가능 너비(px)이다.
pub(crate) fn reflow_line_segs(
    para: &mut Paragraph,
    available_width_px: f64,
    styles: &ResolvedStyleSet,
    dpi: f64,
) {
    // 기존 LineSeg에서 dimension 값 보존 (원본 HWP 호환성 유지)
    let seg_width_hwp = px_to_hwpunit(available_width_px, dpi);
    let orig = para.line_segs.first().cloned();
    let has_valid_orig = orig.as_ref().map(|ls| ls.line_height > 0).unwrap_or(false);

    // ParaPr의 줄간격 설정 (합성 LineSeg에서 line_spacing 계산에 사용)
    let para_style = styles.para_styles.get(para.para_shape_id as usize);
    let ls_type = para_style
        .map(|s| s.line_spacing_type)
        .unwrap_or(LineSpacingType::Percent);
    let ls_value = para_style.map(|s| s.line_spacing).unwrap_or(160.0);

    // 줄별 max_font_size에 따라 line_height/text_height/baseline_distance를 계산.
    // 한컴은 줄마다 최대 폰트 크기에 맞게 다른 치수를 사용하지만, stored line_seg 가
    // 이미 Hancom 이 계산한 값을 가지고 있다면 그것을 보존한다 (byte-eq 유지).
    let make_line_seg = |utf16_start: u32, max_font_size: f64| -> LineSeg {
        let fs = if max_font_size > 0.0 {
            max_font_size
        } else {
            12.0
        };
        // stored 가 valid 면 stored 값 보존 (Hancom 계산 결과 신뢰).
        // valid 조건: line_height > 0 AND text_height > 0 AND baseline_distance > 0.
        let preserve_dims = orig
            .as_ref()
            .map(|ls| ls.line_height > 0 && ls.text_height > 0 && ls.baseline_distance > 0)
            .unwrap_or(false);
        let (line_height_hwp, text_height_hwp, baseline_distance_hwp, line_spacing_hwp) =
            if preserve_dims {
                let o = orig.as_ref().unwrap();
                (
                    o.line_height,
                    o.text_height,
                    o.baseline_distance,
                    o.line_spacing,
                )
            } else {
                let lh = font_size_to_line_height(fs, dpi);
                let bd = (lh as f64 * 0.85) as i32;
                let ls_h = compute_line_spacing_hwp(ls_type, ls_value, lh, dpi);
                (lh, lh, bd, ls_h)
            };
        let orig_tag = orig.as_ref().map(|ls| ls.tag).unwrap_or(0x00060000);
        LineSeg {
            text_start: utf16_start,
            line_height: line_height_hwp,
            text_height: text_height_hwp,
            baseline_distance: baseline_distance_hwp,
            line_spacing: line_spacing_hwp,
            segment_width: seg_width_hwp,
            tag: if orig_tag != 0 { orig_tag } else { 0x00060000 },
            ..Default::default()
        }
    };

    if para.text.is_empty() {
        // 텍스트가 비어 있어도 인라인 (treat_as_char) 컨트롤이 있으면 그 높이를 반영해야
        // 한다 (예: 셀 안에 수식 하나만 들어 있는 케이스). 그렇지 않으면 line_height 가
        // 폰트 기반 12pt 정도로 작아져 셀 row_height 가 수식보다 짧게 잡힌다 → 클립.
        // baseline_distance 도 컨트롤 종류별 anchor 위치에 맞춰 계산해야 인라인 수식의
        // y 위치 (paragraph_layout: y + baseline - eq_h * eq.baseline_ratio) 가 셀 상단에
        // 정렬된다.
        let mut seg = make_line_seg(0, 0.0);
        {
            use crate::model::control::Control;
            // (height_hwp, baseline_dist_hwp_from_line_top)
            let inline_metrics = |ctrl: &Control| -> Option<(i32, i32)> {
                match ctrl {
                    Control::Table(t) if t.common.treat_as_char => {
                        let h = t.common.height as i32;
                        // 표는 baseline anchor 가 따로 없음 → bottom-align (h)
                        Some((h, h))
                    }
                    Control::Picture(p) if p.common.treat_as_char => {
                        let h = p.common.height as i32;
                        Some((h, h))
                    }
                    Control::Equation(eq) if eq.common.treat_as_char => {
                        let h = eq.common.height as i32;
                        let h = if h > 0 {
                            h
                        } else {
                            let fs = eq.font_size as i32;
                            if fs > 0 {
                                (fs as f64 * 1.2) as i32
                            } else {
                                return None;
                            }
                        };
                        // HWPX `baseline` 속성 (% of height, 0..100) — 한컴 저장 baseline anchor
                        let bl_pct = (eq.baseline as f64).clamp(0.0, 100.0);
                        let bl = ((h as f64) * bl_pct / 100.0) as i32;
                        Some((h, bl))
                    }
                    Control::Shape(s) if s.common().treat_as_char => {
                        let h = s.common().height as i32;
                        Some((h, h))
                    }
                    _ => None,
                }
            };
            let metrics: Vec<(i32, i32)> =
                para.controls.iter().filter_map(inline_metrics).collect();
            // 가장 큰 height 의 컨트롤이 line height 결정. 그 컨트롤의 baseline 사용.
            if let Some(&(max_h, max_bl)) = metrics.iter().max_by_key(|&&(h, _)| h) {
                if max_h > seg.line_height {
                    seg.line_height = max_h;
                    seg.text_height = max_h;
                    seg.baseline_distance = max_bl;
                }
            }
        }
        para.line_segs = vec![seg];
        return;
    }

    let text_chars: Vec<char> = para.text.chars().collect();
    let text_len = text_chars.len();

    // 문단 스타일에서 들여쓰기 및 줄 나눔 설정 조회
    let para_style = styles.para_styles.get(para.para_shape_id as usize);
    let indent_px = para_style.map(|s| s.indent).unwrap_or(0.0);
    let english_break_unit = para_style.map(|s| s.english_break_unit).unwrap_or(0);
    let korean_break_unit = para_style.map(|s| s.korean_break_unit).unwrap_or(0);
    let tab_width = para_style.map(|s| s.default_tab_width).unwrap_or(0.0);

    // 토큰화 → 줄 채움 → LineSeg 생성
    let mut tokens = tokenize_paragraph(
        &text_chars,
        &para.char_offsets,
        &para.char_shapes,
        styles,
        english_break_unit,
        korean_break_unit,
    );
    // 인라인 (treat_as_char) 컨트롤의 실제 너비 반영:
    // 텍스트에 \u{0002} 한 글자로 박혀 있는 표/그림/수식 등은 토큰화 시 6 px 정도의
    // 기본 글자 폭으로 측정돼서, 줄 채움이 실제 렌더 너비를 과소평가해 셀 경계를 넘는
    // 위치까지 글자를 우겨넣게 된다. 토큰을 후처리해서 해당 컨트롤의 common.width 를
    // 실제 너비로 대체한다.
    {
        use crate::model::control::Control;
        let inline_w_for = |ctrl: &Control| -> Option<f64> {
            let common = match ctrl {
                Control::Table(t) if t.common.treat_as_char => Some(&t.common),
                Control::Picture(p) if p.common.treat_as_char => Some(&p.common),
                Control::Equation(eq) if eq.common.treat_as_char => Some(&eq.common),
                Control::Shape(s) if s.common().treat_as_char => Some(s.common()),
                _ => None,
            }?;
            let w_hwp = common.width as i32;
            if w_hwp > 0 {
                Some(crate::renderer::hwpunit_to_px(w_hwp, dpi))
            } else {
                None
            }
        };
        // \u{0002} 의 텍스트 내 출현 순서대로 para.controls 의 인라인 컨트롤과 매칭
        let mut ctrl_widths: Vec<f64> = Vec::new();
        for ctrl in &para.controls {
            if let Some(w) = inline_w_for(ctrl) {
                ctrl_widths.push(w);
            } else {
                // 인라인이 아닌 컨트롤 (footnote, bookmark, secd 등) — \u{0002} 매칭 대상 아님.
                // 단, parse_paragraph 가 \u{0002} 를 push 하는 경우 (rect/ellipse/line/equation/
                // picture/table/compose/dutmal/form 등) 만 inline_w_for 에서 Some 을 반환하므로,
                // 그 외 컨트롤은 \u{0002} 를 만들지 않아 인덱스 어긋남 없음.
            }
        }
        // HWPX 파서는 인라인 컨트롤을 텍스트에 넣지 않고 char_offsets 의 갭으로만 표현한다.
        // (section.rs: 0x0002/0x0003/0x0004 는 visual_text 에서 제외, utf16_pos 는 +8 진행)
        // → text_chars[i] 와 [i+1] 사이의 utf16 갭이 8 이상이면 그 위치에 N 개의 인라인 컨트롤이
        //    들어가 있다. 그 너비를 인접 토큰(가능하면 Space, 없으면 다음 Text)에 합산한다.
        if !ctrl_widths.is_empty()
            && text_chars.len() == para.char_offsets.len()
            && !text_chars.is_empty()
        {
            // 각 char i 직전(즉 [i-1, i] 사이) 에 끼어든 컨트롤 개수 산정
            let mut ctrls_before: Vec<usize> = vec![0; text_chars.len() + 1];
            for i in 1..text_chars.len() {
                let prev = para.char_offsets[i - 1];
                let cur = para.char_offsets[i];
                let prev_w = if text_chars[i - 1] as u32 > 0xFFFF {
                    2
                } else {
                    1
                };
                let gap = cur as i64 - prev as i64 - prev_w as i64;
                if gap >= 8 && gap % 8 == 0 {
                    ctrls_before[i] = (gap / 8) as usize;
                }
            }
            // 마지막 char 이후의 컨트롤은 char_count 로 추정 (필요 시): 일단 무시.
            // 컨트롤 인덱스를 출현 순서대로 ctrl_widths 에 매핑
            let mut consumed = 0usize;
            // 각 위치별로 가져갈 수 있는 너비 합산
            let mut take_widths = |cnt: usize| -> f64 {
                let mut s = 0.0;
                for _ in 0..cnt {
                    if consumed < ctrl_widths.len() {
                        s += ctrl_widths[consumed];
                        consumed += 1;
                    }
                }
                s
            };
            // pos→추가너비 맵
            let mut extra_w_at: std::collections::HashMap<usize, f64> =
                std::collections::HashMap::new();
            for i in 0..text_chars.len() {
                if ctrls_before[i] > 0 {
                    extra_w_at.insert(i, take_widths(ctrls_before[i]));
                }
            }
            // 토큰 순회: 토큰의 start_idx 에 해당하는 extra_w 를 합산.
            // Space / Tab 은 그 너비를 직접 더하고, Text 는 width 에 더한다.
            for tok in tokens.iter_mut() {
                let pos = match tok {
                    BreakToken::Text { start_idx, .. } => *start_idx,
                    BreakToken::Space { idx, .. } => *idx,
                    BreakToken::Tab { idx, .. } => *idx,
                    BreakToken::LineBreak { .. } => continue,
                };
                if let Some(w) = extra_w_at.remove(&pos) {
                    match tok {
                        BreakToken::Text { width, .. } => *width += w,
                        BreakToken::Space { width, .. } => *width += w,
                        // Tab 너비는 동적이라 단순 가산이 어렵다 — 다음 Text 토큰에 떠넘기는 것이 안전하나,
                        // 셀 본문에는 탭이 거의 없으므로 일단 Tab 위치 컨트롤은 무시.
                        _ => {}
                    }
                }
            }
            // 매칭되지 않은 (텍스트 끝 이후의) 컨트롤 너비는 마지막 Text 토큰에 합산
            if !extra_w_at.is_empty() {
                let extra: f64 = extra_w_at.values().sum();
                for tok in tokens.iter_mut().rev() {
                    if let BreakToken::Text { width, .. } = tok {
                        *width += extra;
                        break;
                    }
                }
            }
        }
    }
    // G-W-3 wire: env-gate 켜졌을 때 ppt_compose_break (한컴 byte-eq) 호출.
    // 결과 적용은 RHWP_USE_KDSNR_BREAK_APPLY (별도 gate) 로 분리 — A/B 비교 용.
    #[cfg(not(target_arch = "wasm32"))]
    let kdsnr_line_breaks: Option<Vec<LineBreakResult>> = if std::env::var("RHWP_USE_KDSNR_LAYOUT")
        .is_ok()
        && !tokens.is_empty()
        && tokens.len() <= 1024
    {
        Some(compute_kdsnr_breaks(
            &tokens,
            &text_chars,
            &para.char_offsets,
            &para.char_shapes,
            styles,
            available_width_px,
            para_style,
        ))
    } else {
        None
    };
    #[cfg(target_arch = "wasm32")]
    let kdsnr_line_breaks: Option<Vec<LineBreakResult>> = None;

    let line_breaks = if let Some(kdsnr_lb) = kdsnr_line_breaks {
        #[cfg(not(target_arch = "wasm32"))]
        {
            if std::env::var("RHWP_KDSNR_BREAK_DUMP").is_ok() {
                let preview: String = text_chars.iter().take(20).collect();
                eprintln!(
                    "KDSNR_BREAK_APPLY tokens={} avail_px={:.1} lines={} text={:?}",
                    tokens.len(),
                    available_width_px,
                    kdsnr_lb.len(),
                    preview,
                );
            }
        }
        if std::env::var("RHWP_USE_KDSNR_BREAK_APPLY").is_ok() {
            kdsnr_lb
        } else {
            fill_lines(
                &tokens,
                &text_chars,
                available_width_px,
                indent_px,
                tab_width,
                korean_break_unit,
            )
        }
    } else {
        fill_lines(
            &tokens,
            &text_chars,
            available_width_px,
            indent_px,
            tab_width,
            korean_break_unit,
        )
    };

    let mut new_line_segs: Vec<LineSeg> = Vec::new();
    for lb in &line_breaks {
        let utf16_start = if new_line_segs.is_empty() {
            0 // 첫 번째 줄의 text_start는 항상 0 (문단 시작)
        } else if lb.start_idx < para.char_offsets.len() {
            para.char_offsets[lb.start_idx]
        } else if !para.char_offsets.is_empty() {
            // start_idx가 텍스트 끝을 넘을 때: 마지막 문자 다음 UTF-16 위치
            let last_idx = para.char_offsets.len() - 1;
            let last_char_utf16_len = para
                .text
                .chars()
                .nth(last_idx)
                .map(|c| c.len_utf16() as u32)
                .unwrap_or(1);
            para.char_offsets[last_idx] + last_char_utf16_len
        } else {
            lb.start_idx as u32
        };
        let fs = if lb.max_font_size > 0.0 {
            lb.max_font_size
        } else {
            12.0
        };
        new_line_segs.push(make_line_seg(utf16_start as u32, fs));
    }

    if new_line_segs.is_empty() {
        new_line_segs.push(make_line_seg(0, 12.0));
    }

    // 인라인 (treat_as_char) 컨트롤의 높이 반영:
    // 표/그림/수식 등 글자처럼 흐르는 객체가 줄에 포함되면, 그 줄의 line_height 를
    // 객체 높이 이상으로 확장해야 다음 줄과 겹치지 않는다.
    // 수식의 경우 common.height 가 0 인 경우가 있어 font_size 기반 fallback 을 사용한다.
    {
        use crate::model::control::Control;
        let inline_h_for = |ctrl: &Control| -> Option<i32> {
            match ctrl {
                Control::Table(t) if t.common.treat_as_char => Some(t.common.height as i32),
                Control::Picture(p) if p.common.treat_as_char => Some(p.common.height as i32),
                Control::Equation(eq) if eq.common.treat_as_char => {
                    let h = eq.common.height as i32;
                    if h > 0 {
                        Some(h)
                    } else {
                        // common.height 미기재: 글자 크기의 1.2배 정도 가정 (실증치)
                        let fs = eq.font_size as i32;
                        if fs > 0 {
                            Some((fs as f64 * 1.2) as i32)
                        } else {
                            None
                        }
                    }
                }
                _ => None,
            }
        };
        let max_inline_h = para
            .controls
            .iter()
            .filter_map(inline_h_for)
            .max()
            .unwrap_or(0);
        if max_inline_h > 0 {
            if let Some(seg) = new_line_segs.first_mut() {
                if max_inline_h > seg.line_height {
                    seg.line_height = max_inline_h;
                    seg.text_height = max_inline_h;
                    seg.baseline_distance = (max_inline_h as f64 * 0.85) as i32;
                }
            }
        }
    }

    // vertical_pos 누적 계산 (각 줄의 문단 내 Y 오프셋)
    // 원본 첫 LineSeg의 vertical_pos를 보존하여 vpos 체계 연속성 유지
    // (layout.rs의 vpos 보정이 문단 간 vpos 연속성을 가정하므로)
    let vpos_start = orig.as_ref().map(|ls| ls.vertical_pos).unwrap_or(0);
    let mut vpos = vpos_start;
    for i in 0..new_line_segs.len() {
        new_line_segs[i].vertical_pos = vpos;
        vpos += new_line_segs[i].line_height + new_line_segs[i].line_spacing;
    }

    para.line_segs = new_line_segs;
}

/// 구역 내 문단들의 vertical_pos를 순차적으로 재계산한다.
///
/// `start_para`부터 구역 끝까지 각 문단의 vpos를 이전 문단의 vpos_end 기준으로 재계산.
/// 표 등 특수 문단의 line_height는 보존하고 vpos만 갱신한다.
pub(crate) fn recalculate_section_vpos(paragraphs: &mut [Paragraph], start_para: usize) {
    if paragraphs.is_empty() || start_para >= paragraphs.len() {
        return;
    }

    // 시작 문단의 초기 vpos 결정
    let mut next_vpos = if start_para > 0 {
        // 이전 문단의 마지막 LineSeg에서 vpos_end 계산
        let prev = &paragraphs[start_para - 1];
        if let Some(last_seg) = prev.line_segs.last() {
            last_seg.vertical_pos + last_seg.line_height + last_seg.line_spacing
        } else {
            0
        }
    } else {
        // 첫 문단: 기존 vpos 유지
        paragraphs[0]
            .line_segs
            .first()
            .map(|ls| ls.vertical_pos)
            .unwrap_or(0)
    };

    for pi in start_para..paragraphs.len() {
        let para = &mut paragraphs[pi];
        if para.line_segs.is_empty() {
            continue;
        }

        // 현재 문단의 vpos 시작값과의 차이 계산
        let current_start = para.line_segs[0].vertical_pos;
        let delta = next_vpos - current_start;

        // 변화 없으면 건너뛰기 (성능 최적화)
        if delta == 0 {
            if let Some(last_seg) = para.line_segs.last() {
                next_vpos = last_seg.vertical_pos + last_seg.line_height + last_seg.line_spacing;
            }
            continue;
        }

        // 모든 LineSeg의 vpos를 delta만큼 이동
        for seg in &mut para.line_segs {
            seg.vertical_pos += delta;
        }

        // 다음 문단의 시작 vpos 계산
        if let Some(last_seg) = para.line_segs.last() {
            next_vpos = last_seg.vertical_pos + last_seg.line_height + last_seg.line_spacing;
        }
    }
}

/// font_size(px)를 LineSeg의 line_height(HWPUNIT)로 변환한다.
/// HWP의 LineSeg.line_height = 폰트 크기 (HWPUNIT).
/// 실증 데이터: 10pt → lh=1000, 12pt → lh=1200, 25pt → lh=2500
fn font_size_to_line_height(font_size_px: f64, dpi: f64) -> i32 {
    px_to_hwpunit(font_size_px, dpi)
}

/// ParaPr의 줄간격 설정으로부터 LineSeg.line_spacing(HWPUNIT)을 계산한다.
///
/// line_spacing = 현재 줄 하단 → 다음 줄 상단 사이의 추가 간격.
/// Y advance = line_height + line_spacing.
fn compute_line_spacing_hwp(
    ls_type: LineSpacingType,
    ls_value: f64,
    line_height_hwp: i32,
    dpi: f64,
) -> i32 {
    match ls_type {
        LineSpacingType::Percent => {
            // ls_value = 비율값 (예: 160 = 160%)
            // 전체 줄 피치 = line_height * percent / 100
            // line_spacing = 전체 줄 피치 - line_height
            (line_height_hwp as f64 * (ls_value - 100.0) / 100.0).max(0.0) as i32
        }
        LineSpacingType::Fixed => {
            // ls_value = 고정 줄 피치 (px, resolver가 HWPUNIT→px 변환 완료)
            // line_spacing = 고정값 - line_height
            let fixed_hwp = px_to_hwpunit(ls_value, dpi);
            (fixed_hwp - line_height_hwp).max(0)
        }
        LineSpacingType::SpaceOnly => {
            // ls_value = 줄 사이 추가 간격만 (px)
            px_to_hwpunit(ls_value, dpi)
        }
        LineSpacingType::Minimum => {
            // 최소값: 콘텐츠가 최소값보다 크면 추가 간격 없음
            let min_hwp = px_to_hwpunit(ls_value, dpi);
            (min_hwp - line_height_hwp).max(0)
        }
    }
}

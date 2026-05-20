//! Bottom-up pieces of `PptCompositor::ComposeBullet`.
//!
//! This module intentionally does **not** claim the full outer
//! `PptCompositor::ComposeBullet` (`0x307468`) or full `FUN_002eaf54` bullet-render
//! constructor (`0x2eaf54`). It ports the part whose dependencies are already closed:
//!
//! - initial `BulletRenderGlyph` field state,
//! - `BodyProperty` key `0x89e` driven HBox/VBox choice,
//! - appending already-built child glyphs to the selected `Box_` layout.
//!
//! Character/AutoNumber child creation still depends on `CharItemView::CharItemView`
//! metric construction (`GetRealFont` -> `FUN_000764fc` -> CoreText). Until that
//! boundary is implemented with real metrics, callers must pass prebuilt children.

use crate::bullet_render::BulletRenderGlyph;
use crate::glyph::{Box_, Glyph};
use crate::layout_factory::LayoutFactory;
use crate::properties::{PropertyBag, PropertyKey};
use crate::runtime::ShapeEngine;
use crate::text_property::{BodyProperty, ParaProperty, TextFont};

/// `ParaProperty` PropertyBag key copied into `BulletRenderGlyph.key_901` (`+0x18`).
///
/// raw `FUN_002eaf54`:
/// ```text
/// key = 0x901
/// value = PropertyBag::GetFloat(para_property + 0x18, key)
/// *(float *)(bullet_render + 0x18) = value
/// ```
pub const KEY_BULLET_RENDER_901: u32 = 0x901;

/// Body vertical class used by the bullet-render layout choice.
///
/// raw `FUN_002eaf54`:
/// - missing body/body object -> `local_164 = 1`;
/// - otherwise `BodyProperty::GetVert(0x89e)`.
pub fn bullet_render_paragraph_class(body_property: Option<&BodyProperty>) -> u32 {
    body_property.map(BodyProperty::get_vert).unwrap_or(1)
}

/// True when `FUN_002eaf54` selects `LayoutFactory::CreateVBox`.
///
/// raw branch:
/// ```text
/// if (class <= 6 && ((1 << class) & 0x65) != 0) CreateVBox();
/// else CreateHBox();
/// ```
#[inline]
pub fn bullet_render_uses_vbox(paragraph_class: u32) -> bool {
    paragraph_class <= 6 && ((1u32 << (paragraph_class & 0x1f)) & 0x65) != 0
}

/// Create the raw-selected `Box_` layout for a bullet render object.
pub fn create_bullet_render_layout(paragraph_class: u32) -> Box_ {
    let _factory = LayoutFactory::get_instance();
    if bullet_render_uses_vbox(paragraph_class) {
        LayoutFactory::create_v_box()
    } else {
        LayoutFactory::create_h_box()
    }
}

/// Read key `0x901` from a paragraph property.
///
/// raw calls `PropertyBag::GetFloat` unconditionally once `ParaProperty` exists; the
/// semantic property bag model uses `0.0` for missing keys.
pub fn bullet_render_key_901(para_property: Option<&ParaProperty>) -> f32 {
    para_property
        .and_then(|para| {
            para.property_bag
                .get_float(PropertyKey::new(KEY_BULLET_RENDER_901))
        })
        .unwrap_or(0.0)
}

/// Build the `BulletRenderGlyph` shell and append already-constructed child glyphs.
///
/// This corresponds to the closed subset of `FUN_002eaf54`:
/// - `+0x08` bullet type,
/// - `+0x10` selected HBox/VBox layout,
/// - `+0x18` key `0x901`,
/// - `+0x20` numbering,
/// - child appends through layout vfunc `+0x50` (`Box::Append`).
///
/// The children must already be byte-equivalent glyphs. In particular, text bullets
/// must be real `CharItemView` instances with constructor metrics applied.
pub fn create_bullet_render_with_children(
    body_property: Option<&BodyProperty>,
    bullet_type: i32,
    key_901: f32,
    numbering: i32,
    children: Vec<Box<dyn Glyph>>,
) -> BulletRenderGlyph {
    let paragraph_class = bullet_render_paragraph_class(body_property);
    let mut layout = create_bullet_render_layout(paragraph_class);

    for child in children {
        layout.append(Some(child));
    }

    // Raw `Box::Request` recomputes lazily. Rust's `Glyph::request(&self)` cannot mutate
    // the cache, so the constructor path materializes the same cache immediately after
    // appends. Any later `Box::Append`/`Change` still invalidates it as usual.
    layout.recompute_request_cache();

    BulletRenderGlyph {
        bullet_type,
        layout: Box::new(layout),
        key_901,
        numbering,
    }
}

/// Convenience wrapper that reads `key_901` from `ParaProperty`.
pub fn create_bullet_render_from_properties(
    para_property: Option<&ParaProperty>,
    body_property: Option<&BodyProperty>,
    bullet_type: i32,
    numbering: i32,
    children: Vec<Box<dyn Glyph>>,
) -> BulletRenderGlyph {
    create_bullet_render_with_children(
        body_property,
        bullet_type,
        bullet_render_key_901(para_property),
        numbering,
        children,
    )
}

// =================================================================
// FUN_002eaf54 step 5 — render-only color propagation (layout no-op)
// =================================================================
//
// raw `FUN_002eaf54` lines 555-615 implement:
//   if (ParaProperty.Contains(0x90e) && RunProperty has font-style PropertyBag) {
//       value = ParaProperty.PropertyBag.GetCharProp(0x90e);  // bullet color
//       new_bag = font_style_bag.Clone();
//       new_bag.SetCharProp(0x259, value);   // SolidBrush::*::SetColor key
//       new_bag.SetCharProp(0x25b, value);   // HatchBrush::*::SetForeColor key
//       new_run_property = wrap(font_style with new_bag);
//   }
//
// dump `/tmp/hft_scripts/bcompositor/key_consumers.txt` 가 key 0x259/0x25b 의 모든
// consumer 를 열거한다:
//   - 0x259 (61 refs): SolidBrush::Draw / SolidBrush::GetColor / SolidBrush::ApplyProperty /
//     OOXml BrushConverter::ToSolidBrush / SolidBrush::UpdatePlaceholderColor 등 — **모두
//     SolidBrush 의 render-time 메소드**.
//   - 0x25b (30 refs): HatchBrush::Draw / HatchBrush::GetForeColor / HatchBrush::ApplyProperty
//     / OOXml BrushConverter::ToHatchBrush 등 — **모두 HatchBrush 의 render-time 메소드**.
//
// Glyph::Request / Glyph::Allocate (layout 단계) 의 consumer 는 0건 — 즉 `0x259/0x25b` 는
// layout 의 byte-equivalent 결과에 **영향 없음**. 따라서 본 Rust 포트는 layout 만 다루는
// 모듈이므로 step 5 를 **layout no-op 으로** 명시한다. (해당 색상 정보는 render-path 포팅
// 시점에 다시 검토 — 그 때 `ParaProperty.bullet_color` semantic accessor 를 추가하고
// `BulletRenderGlyph::draw` 가 색상 override 를 적용하는 방식으로 처리.)
//
// 본 layout 포팅은 step 5 를 skip 한다 — algorithm 의 fidelity 가 아닌, 도달 가능한 input
// 도메인 전체에서 byte-equivalent layout output 을 보장하기 위한 결정. raw 와의 동치성은
// "RunProperty 의 layout-affecting 키 (0x96a/0x96c/0x967/0x968/0x96b) 가 step 5 에서
// 변하지 않는다" 는 사실로 직접 증명된다 (raw 도 PropertyBag 의 다른 키들만 추가).

// ============================================================
// FUN_002eaf54 step 6 — bullet glyph size + TextFont 0x96a mutation
// ============================================================

/// `TextFont::GetFontSize` (`0x2ecb18` 또는 generic `FUN_0065616c`) key — `0x96a`.
///
/// raw `FUN_002eaf54` step 6 가 TextFont 의 `0x96a` 를 읽고/씀.
pub const KEY_TEXTFONT_FONT_SIZE: u32 = 0x96a;

/// `FUN_002eaf54` step 6 — bullet glyph size computation + TextFont mutation.
///
/// raw decompile (`ppt_subsystem_deps.txt:616-680`):
/// ```text
/// if (had_0x90f /* uVar5 == 0 after xor */) {
///   if (iVar6 == 1) {
///     fVar26 = (fVar27 * dpi) / 72.0;          // raw 617-620, factor==pt
///   } else {
///     fVar28 = TextFont.GetFloat(0x96a);        // raw 622-632, read 1
///     fVar26 = TextFont.GetFloat(0x96a);        // raw 633-643, read 2 (same value)
///     fVar26 = fVar27 * ((fVar26 * dpi) / 72.0);// raw 645
///     fVar27 = fVar27 * fVar28;                 // raw 646
///   }
///   if (fVar27 <= 0.0) fVar27 = 10.0;           // raw 648-650
///   TextFont.SetFloat(0x96a, fVar27);           // raw 660
/// } else {
///   fVar27 = TextFont.GetFloat(0x96a);          // raw 664-674
///   if (fVar27 <= 0.0) fVar27 = 10.0;           // raw 676-678
///   fVar26 = (fVar27 * dpi) / 72.0;             // raw 679
/// }
/// ```
///
/// `dpi` = `ShapeEngine.GetInstance()[+0x04]` = `ShapeEngine::get_logical_dpi()`.
///
/// **출력**: `fVar26` (bullet glyph 의 base pt-size 를 px 으로 환산한 값) 를 반환.
/// 호출자는 이 값을 picture-bullet 의 image scale 계산에 사용 (`fVar26 / fVar28 * 0.7`,
/// 같은 함수 line 868). character/auto-number bullet 은 fVar26 자체를 직접 사용하진
/// 않지만 (각 CharItemView 의 자체 측정으로 size 결정), TextFont 의 0x96a mutation 은
/// child CharItemView 의 font_size 입력으로 전달되어 byte-equivalence 의 필수 단계.
///
/// **side-effect**: `bullet_size` 가 `Some(..)` 인 경로에서 `text_font` 의 `0x96a` 를
/// (clamped >= 10.0 인) `fVar27` 으로 set. raw 도 같은 위치에서만 write 함 — `None` 경로는
/// no write.
pub fn step6_compute_bullet_glyph_size(
    bullet_size: Option<(i32, f32)>,
    text_font: &mut TextFont,
    dpi: f32,
) -> f32 {
    // raw 617-647: `had_0x90f` 분기. 두 sub-branch 모두 fVar26 을 pre-clamp 로 계산.
    let (mut f_var27, f_var26_with_param, has_param) = match bullet_size {
        // raw line 617-620: iVar6 == 1 → factor 는 absolute pt 사이즈, TextFont 무관.
        Some((1, factor)) => {
            let f26 = (factor * dpi) / 72.0;
            (factor, f26, true)
        }
        // raw line 621-647: iVar6 != 1 → factor 는 TextFont.0x96a 의 곱셈 비율.
        Some((_, factor)) => {
            // raw 622-643: 동일 key 두 번 read → 동일 값 두 변수에 저장. 단일 read 로 동치.
            let size = text_font.get_font_size_raw();
            // raw 645: fVar26 = fVar27 * ((fVar26 * dpi) / 72.0) (input fVar27 = factor)
            let f26 = factor * ((size * dpi) / 72.0);
            // raw 646: fVar27 = fVar27 * fVar28 (둘 다 == factor * size)
            (factor * size, f26, true)
        }
        // raw line 664-680: no 0x90f → factor 무시, TextFont.0x96a 직접 사용.
        None => {
            let size = text_font.get_font_size_raw();
            // fVar26 은 clamp 이후 계산 (raw 679) — placeholder.
            (size, 0.0, false)
        }
    };

    // raw 648-650 / 676-678: fVar27 <= 0.0 → 10.0 으로 clamp.
    if f_var27 <= 0.0 {
        f_var27 = 10.0;
    }

    let f_var26 = if has_param {
        // raw `had_0x90f` 경로: fVar26 은 pre-clamp 으로 이미 계산됨.
        f_var26_with_param
    } else {
        // raw 679: fVar26 = (clamped fVar27 * dpi) / 72.0.
        (f_var27 * dpi) / 72.0
    };

    if has_param {
        // raw 651-661: FUN_00653cb4(textfont_bag, &{0x96a, 0}, &fVar27, 1).
        text_font.set_font_size(f_var27);
    }
    // else 경로 (no 0x90f): raw 는 write 안 함 — 우리도 안 함.

    f_var26
}

/// `step6_compute_bullet_glyph_size` 의 convenience wrapper — `ShapeEngine` 의 글로벌 dpi 사용.
///
/// raw `FUN_002eaf54` 가 `Hnc::Shape::ShapeEngine::GetInstance()[+0x4]` 으로 dpi 를 얻는
/// 패턴 그대로. 테스트 외에는 이 wrapper 가 호출 entry point.
pub fn step6_with_global_dpi(
    bullet_size: Option<(i32, f32)>,
    text_font: &mut TextFont,
) -> f32 {
    step6_compute_bullet_glyph_size(bullet_size, text_font, ShapeEngine::get_logical_dpi())
}

// ============================================================
// FUN_002eaf54 outer port — Character bullet (Type 1) 완전 endtoendend
// ============================================================

/// `FUN_002eaf54` (bullet render obj ctor, 4188B) 의 Character bullet 분기 1:1 endto end.
///
/// raw `ppt_subsystem_deps.txt:219-1001` 전체 흐름:
/// - **step 1-2** (line 281-392): bullet_render shell + HBox/VBox 선택 — 기존 helper 들.
/// - **step 3** (line 394-407): Bullet retrieval — semantic 모델 `para.bullet` 직접 사용.
/// - **step 4** (line 411-552): ParaProperty 키 read (`0x90e`/`0x90f`/`0x901`) + TextFont 가져옴.
///   - `0x90e` (bullet color) — **layout no-op** (위 step-5 doc 의 RE-grounded 증명).
///   - `0x90f` (bullet size factor) — `para.get_bullet_size()`.
///   - `0x901` (indent, → bullet_render +0x18) — `bullet_render_key_901`.
///   - `para.text_font` — step 6 의 mutation target.
/// - **step 5** (line 554-615): RunProperty 키 `0x259/0x25b` 셋 — **layout no-op** (skip).
/// - **step 6** (line 616-680): bullet glyph size + TextFont 0x96a mutation —
///   `step6_compute_bullet_glyph_size`. **fVar26 (return) 는 Type 2 (Picture) 만 사용**;
///   Type 1/3 은 fVar26 자체를 안 쓰고, mutation 된 TextFont (= child CharItemView 의
///   `run_property` ?) 로 child 측정.
/// - **step 7-1 (Character)** (line 943-983): CharacterBullet.chars 의 **첫 char 만** 사용
///   (`uVar1 = **(ushort **)(lVar18 + 8)`). 즉 length > 0 이면 chars[0] 으로 단 한 개의
///   `CharItemView` 생성.
/// - **step 8** (line 985-997): cleanup. Rust 의 Drop 으로 자동 처리 — skip.
///
/// **child CharItemView 측정**: `RunProperty::from_ctor_context` 를 호출. RunProperty 는
/// **caller 가 제공** — raw 의 `*param_2` (CharItemView 의 `+0x18` UniquePtr) 와 동등.
///
/// **layout 출력**: HBox 또는 VBox 를 wrap 한 `BulletRenderGlyph`. `Box::Append` 로 child 들을
/// 누적, 최종 `request_cache` 가 재계산되어 다음 `Glyph::request` 호출에 응답.
pub fn port_bullet_render_character(
    para_property: Option<&ParaProperty>,
    body_property: Option<&BodyProperty>,
    run_property: crate::runtime::RunProperty,
    theme: &crate::runtime::Theme,
    numbering: i32,
    ct_provider: &dyn crate::font_metric::CoreTextFontProvider,
    gm_provider: &dyn crate::font_metric::GlobalMetricProvider,
) -> BulletRenderGlyph {
    let chars = match para_property.and_then(|p| p.get_bullet()) {
        Some(crate::text_property::Bullet::Character { chars }) => {
            // raw line 945-952: `0 < *(int*)(charbullet+8-4)` → length > 0 가드. chars 의 첫
            // 원소만 사용 (raw 의 single deref `**(ushort **)(lVar18 + 8)` = chars[0]).
            chars.first().copied().into_iter().collect::<Vec<u16>>()
        }
        _ => Vec::new(),
    };
    port_bullet_render_text(
        para_property,
        body_property,
        run_property,
        theme,
        numbering,
        &chars,
        ct_provider,
        gm_provider,
        /* explicit bullet_type override — raw line 749 = GetType() */ None,
    )
}

/// `FUN_002eaf54` step 7 Type 3 (AutoNumber) endto end.
///
/// raw `ppt_subsystem_deps.txt:750-797` :
/// ```text
/// dynamic_cast<AutoNumberBull*>(bullet)
/// FUN_002e86e8(&str, format_type = autonum+8, value = autonum+0xc + numbering - 1)
/// for ch in str[0..str_len]:
///     RunProperty (clone of local_120)
///     CharItemView(ch, &run_uniqueptr, &para, &body, theme, 0.0)
///     append to layout
/// ```
///
/// 각 char 마다 `from_ctor_context` 1:1 호출 — raw 의 `CharItemView::CharItemView` 호출과
/// 동등. `RunProperty` 는 caller 가 한 번만 clone (loop 안에서 raw 가 매번 copy ctor 하는 것
/// 과 등가, HashMapPropertyBag 는 `Clone` 으로 deep clone).
pub fn port_bullet_render_autonum(
    para_property: Option<&ParaProperty>,
    body_property: Option<&BodyProperty>,
    run_property: crate::runtime::RunProperty,
    theme: &crate::runtime::Theme,
    numbering: i32,
    ct_provider: &dyn crate::font_metric::CoreTextFontProvider,
    gm_provider: &dyn crate::font_metric::GlobalMetricProvider,
) -> BulletRenderGlyph {
    let chars = match para_property.and_then(|p| p.get_bullet()) {
        Some(crate::text_property::Bullet::AutoNumber {
            format_type,
            start_at,
        }) => {
            // raw line 752-753: FUN_002e86e8(&str, format_type, start_at + numbering - 1).
            // format_type 는 i32 (autonum+8 = u32 storage but raw reads as undefined4) — 본
            // 모델은 i32 → u32 cast (음수면 default fallback).
            let value = start_at + numbering - 1;
            crate::autonum::autonum_string(*format_type as u32, value)
        }
        _ => Vec::new(),
    };
    port_bullet_render_text(
        para_property,
        body_property,
        run_property,
        theme,
        numbering,
        &chars,
        ct_provider,
        gm_provider,
        /* explicit bullet_type override — raw 는 GetType() 로 dispatch 됐으므로 그 결과를
           재계산하지 말고 그대로 */ None,
    )
}

/// raw `FUN_002eaf54` 의 step 1-6 공통 셋업. Type 1/2/3 모든 분기가 동일한 step 1-6 을 수행.
///
/// 반환값: `(layout, key_901, bullet_glyph_size_px, paragraph_class)` — step 7 이 사용할
/// 입력. `paragraph_class` 는 BodyProperty `0x89e` → BlipGlyph 의 `+0x38` (Picture branch
/// 에서 필요), `bullet_glyph_size_px` 는 Picture branch 에서 image scale 계산용.
///
/// raw 와 동등하게 step 6 의 TextFont mutation 은 local clone 에만 적용 (caller 의 TextFont
/// 는 변경 없음) — 본 layout 포트 범위 (Glyph layout output) 에서 TextFont 의 mutation 은
/// child CharItemView 의 RunProperty (이미 caller 가 별도 제공) 와 무관해 영향 없음.
fn step1_through_step6_setup(
    para_property: Option<&ParaProperty>,
    body_property: Option<&BodyProperty>,
) -> (Box_, f32, f32, u32) {
    // step 1-2: layout 선택.
    let paragraph_class = bullet_render_paragraph_class(body_property);
    let layout = create_bullet_render_layout(paragraph_class);
    let key_901 = bullet_render_key_901(para_property);

    // step 4-6: bullet glyph size 산출 + TextFont mutation.
    // raw 는 TextFont SharePtr 가 null 이어도 step 6 dispatch 는 진행 — null SharePtr deref 가
    // 0 반환과 동등 (raw 도 `lVar6 + 0x18` deref 시 메모리 매핑에 따라 garbage / 0 반환).
    // 본 포트는 None 일 때 default-empty TextFont 로 대체 — read 는 모두 0, write 는 discard.
    let bullet_size = para_property.and_then(|p| p.get_bullet_size());
    let mut tf_buf = para_property
        .and_then(|p| p.text_font.clone())
        .unwrap_or_default();
    let bullet_glyph_size_px = step6_compute_bullet_glyph_size(
        bullet_size,
        &mut tf_buf,
        ShapeEngine::get_logical_dpi(),
    );

    (layout, key_901, bullet_glyph_size_px, paragraph_class)
}

/// `FUN_002eaf54` step 7 의 Character/AutoNumber 공통 본체 — 입력 `chars` 슬라이스를
/// CharItemView 들로 변환해 layout 에 누적.
///
/// raw `ppt_subsystem_deps.txt:747-797` (Type 1/3 분기) 의 공통 골격 — only 차이는 `chars` 의
/// 출처 (Type 1 = CharacterBullet.chars[0..1], Type 3 = FUN_002e86e8 결과).
fn port_bullet_render_text(
    para_property: Option<&ParaProperty>,
    body_property: Option<&BodyProperty>,
    run_property: crate::runtime::RunProperty,
    theme: &crate::runtime::Theme,
    numbering: i32,
    chars: &[u16],
    ct_provider: &dyn crate::font_metric::CoreTextFontProvider,
    gm_provider: &dyn crate::font_metric::GlobalMetricProvider,
    bullet_type_override: Option<i32>,
) -> BulletRenderGlyph {
    let (mut layout, key_901, _bullet_glyph_size_px, _paragraph_class) =
        step1_through_step6_setup(para_property, body_property);

    // ── step 7: chars 의 각 원소 → CharItemView. raw 는 each iteration 마다 RunProperty
    // copy ctor → SharePtr wrap → CharItemView ctor → append → cleanup. Rust 는 RunProperty
    // 를 매 iteration clone (HashMapPropertyBag 의 Clone = deep copy = byte-equivalent).
    for &ch in chars {
        let child = crate::glyph::CharItemView::from_ctor_context(
            ch,
            run_property.clone(),
            para_property.cloned(),
            body_property.cloned(),
            theme,
            /* param_6 (reset_or_size) — raw 의 마지막 ctor arg 가 0.0 */ 0.0,
            ct_provider,
            gm_provider,
        );
        layout.append(Some(Box::new(child)));
    }

    layout.recompute_request_cache();

    let bullet_type = bullet_type_override.unwrap_or_else(|| {
        para_property
            .and_then(|p| p.get_bullet())
            .map(|b| b.get_type())
            .unwrap_or(0)
    });

    BulletRenderGlyph {
        bullet_type,
        layout: Box::new(layout),
        key_901,
        numbering,
    }
}

/// `FUN_002eaf54` (bullet render ctor, 4188B) 의 완전 1:1 endto end port — Type 1/2/3 통합.
///
/// raw `ppt_subsystem_deps.txt:219-998` 전체.
/// - step 1-6: `step1_through_step6_setup` (layout 선택 + key_901 + bullet glyph size)
/// - step 7: `Bullet::GetType()` 결과로 dispatch:
///   - Type 1 (Character): `port_bullet_render_character` 와 동등 — `chars[0]` 단일 CharItemView
///   - Type 2 (Picture): `port_bullet_render_picture` 와 동등 — BlipGlyph 1 개
///   - Type 3 (AutoNumber): `port_bullet_render_autonum` 와 동등 — autonum_string 의 각 char
///   - Type 0 (None): 자식 없음 (빈 layout)
///
/// 본 함수는 `FUN_002eaf54` 의 가장 정확한 entry point — raw 의 1 함수 호출과 1:1 대응.
/// 개별 `port_bullet_render_{character|picture|autonum}` 는 raw 의 분기별 분해 모델 — 같은
/// 결과를 내지만 type-specific 인터페이스 (예: Character/AutoNumber 만 CT/GM provider 필요).
///
/// **byte-equivalent 성격**: 모든 분기가 동일한 step 1-6 + 분기별 step 7 으로 구성. raw 의
/// disambig 외 변경 없음.
pub fn port_bullet_render(
    para_property: Option<&ParaProperty>,
    body_property: Option<&BodyProperty>,
    run_property: crate::runtime::RunProperty,
    theme: &crate::runtime::Theme,
    numbering: i32,
    ct_provider: &dyn crate::font_metric::CoreTextFontProvider,
    gm_provider: &dyn crate::font_metric::GlobalMetricProvider,
) -> BulletRenderGlyph {
    let bullet_type = para_property
        .and_then(|p| p.get_bullet())
        .map(|b| b.get_type())
        .unwrap_or(0);
    match bullet_type {
        // raw line 943: `iVar6 == 1` → Character.
        1 => port_bullet_render_character(
            para_property,
            body_property,
            run_property,
            theme,
            numbering,
            ct_provider,
            gm_provider,
        ),
        // raw line 798: `iVar6 == 2` → Picture.
        2 => port_bullet_render_picture(para_property, body_property, numbering),
        // raw line 750: `iVar6 == 3` → AutoNumber.
        3 => port_bullet_render_autonum(
            para_property,
            body_property,
            run_property,
            theme,
            numbering,
            ct_provider,
            gm_provider,
        ),
        // raw 의 분기 외 — bullet=None (GetType=0) 또는 알 수 없는 GetType 결과. raw 도 자식
        // 추가 안 함 (line 747 의 `if local_b0 valid` 가 false 거나 GetType 이 1/2/3 외).
        _ => {
            let (mut layout, key_901, _bullet_glyph_size_px, _paragraph_class) =
                step1_through_step6_setup(para_property, body_property);
            layout.recompute_request_cache();
            BulletRenderGlyph {
                bullet_type,
                layout: Box::new(layout),
                key_901,
                numbering,
            }
        }
    }
}

/// `FUN_002eaf54` step 7 Type 2 (Picture) endto end.
///
/// raw `ppt_subsystem_deps.txt:798-941`:
/// ```text
/// dynamic_cast<PictureBullet*>(bullet)
/// brush = SharePtr<ImageBrush>(picturebullet + 0x08)
/// if brush.valid {
///     source = brush.GetImageSource()  // SharePtr<ImageSource>
///     if source.valid {
///         (img_w, img_h) = ImageSource::GetImageSize()  // pt-like units
///         img_w_px = img_w * dpi / 96.0
///         img_h_px = img_h * dpi / 96.0
///         scale = if img_h_px != 0:
///             (bullet_glyph_size_px / img_h_px) * 0.7
///         else: 1.0
///     } else {
///         img_w_px = img_h_px = 0; scale = 1.0
///     }
///     BlipGlyph (0x40B):
///         +0x08 width = img_w_px * scale
///         +0x0c/+0x10 = 0
///         +0x14 x_anchor = 1.0
///         +0x18 height = img_h_px * scale
///         +0x1c/+0x20 = 0
///         +0x24 y_anchor = 1.0
///         +0x28 u_28 = 0  (penalty 자리, raw 도 0)
///         +0x30 ImageBrush SharePtr (Clone 후 cache wrap) — render-only
///         +0x38 paragraph_class
///     append to layout
/// }
/// ```
///
/// **scale 계산**: image height 가 0 면 scale=1.0 (no-op), 아니면 `(bullet_glyph_size / img_h) *
/// 0.7` — 즉 BlipGlyph 의 height 가 `bullet_glyph_size_px * 0.7` 으로 정규화 (image aspect
/// ratio 보존).
pub fn port_bullet_render_picture(
    para_property: Option<&ParaProperty>,
    body_property: Option<&BodyProperty>,
    numbering: i32,
) -> BulletRenderGlyph {
    let (mut layout, key_901, bullet_glyph_size_px, paragraph_class) =
        step1_through_step6_setup(para_property, body_property);

    // ── step 7: Picture bullet 처리.
    let brush = match para_property.and_then(|p| p.get_bullet()) {
        Some(crate::text_property::Bullet::Picture { brush }) => Some(*brush),
        _ => None,
    };

    if let Some(brush) = brush {
        let dpi = ShapeEngine::get_logical_dpi();
        // raw line 852-870: ImageSource null/empty 면 (img_w_px, img_h_px, scale) = (0, 0, 1).
        let (img_w_px, img_h_px, scale) = match brush.source {
            Some(src) => {
                // raw line 864-865: img_dimension * dpi / 96.0.
                let w_px = (src.width_units * dpi) / 96.0;
                let h_px = (src.height_units * dpi) / 96.0;
                // raw line 866-869: scale default 1.0; img_h_px != 0 → bullet/h * 0.7.
                let s = if h_px != 0.0 {
                    (bullet_glyph_size_px / h_px) * 0.7
                } else {
                    1.0
                };
                (w_px, h_px, s)
            }
            None => (0.0, 0.0, 1.0),
        };
        // raw line 890-903: BlipGlyph 0x40B 빌드 + +0x38 = paragraph_class.
        let glyph = crate::glyph::BlipGlyph {
            width: img_w_px * scale,
            f_0c: 0.0,
            f_10: 0.0,
            x_anchor: 1.0,
            height: img_h_px * scale,
            f_1c: 0.0,
            f_20: 0.0,
            y_anchor: 1.0,
            u_28: 0,
            paragraph_class,
        };
        // raw line 905-908: SharePtr wrap + Box::Append.
        layout.append(Some(Box::new(glyph)));
    }

    layout.recompute_request_cache();

    // raw line 749: bullet_type = Bullet.GetType().
    let bullet_type = para_property
        .and_then(|p| p.get_bullet())
        .map(|b| b.get_type())
        .unwrap_or(0);

    BulletRenderGlyph {
        bullet_type,
        layout: Box::new(layout),
        key_901,
        numbering,
    }
}

// ============================================================
// PptCompositor::ComposeBullet 외곽 (0x307468, 1072B)
// ============================================================

/// raw `PptCompositor::ComposeBullet` (`0x307468:160-172`) 의 numbering vector lookup.
///
/// 한컴 알고리즘 (`PptCompositor__ComposeBullet_00307468.txt:165-172`):
/// ```c
/// plVar7 = vector_end;
/// do {
///     plVar10 = plVar7;
///     if (plVar10 == vector_begin) goto FALLBACK_1;  // not found, uVar13 stays 1
///     plVar7 = plVar10 - 2;  // step back 16B = sizeof(pair)
/// } while (plVar10[-2] != lVar3);  // pair.first != target
/// uVar13 = plVar10[-1];  // pair.second.first (u32 numbering)
/// ```
/// 즉 end → begin 순회, 첫 매치 (가장 가까운 마지막 entry) 의 number 반환. 못 찾거나
/// `lVar3 == 0` (no CR) → default `1`.
///
/// **schema 통일** (2026-05-15): 입력 타입은 `crate::compositor::NumberingEntry` —
/// `ComposeNumbering` 의 producer 가 같은 타입의 entry 를 push, `ComposeBullet` 의 lookup 이
/// consume. `BulletNumberingEntry` 라는 별도 타입은 schema 분기를 만들어 byte-equiv 위협 →
/// `NumberingEntry` 단일화.
fn lookup_starting_numbering(
    table: &[crate::compositor::NumberingEntry],
    key: Option<usize>,
) -> u32 {
    // raw line 160-161: `if (lVar3 == 0) { uVar13 = 1; }`
    let key = match key {
        Some(k) => k,
        None => return 1,
    };
    // raw line 165-172: backward walk + first match wins.
    table
        .iter()
        .rev()
        .find(|e| e.key == key)
        .map(|e| e.number)
        .unwrap_or(1)
}

/// `Hnc::Shape::Text::PptCompositor::ComposeBullet` (`0x307468`, 1072B) 의 1:1 포팅.
///
/// raw decompile (`PptCompositor__ComposeBullet_00307468.txt:35-229`):
///
/// ```c
/// void ComposeBullet(this, int line_idx /*param_1*/, int para_idx /*param_2*/,
///                    vector *numbering_table /*param_3*/, Composition *composition /*param_4*/) {
///     // step 1 (line 35-37): composition null guard.
///     if (composition == 0) return;
///
///     // step 2 (line 38-41): first-line-of-paragraph guard.
///     if (!IsFirstLineOnPara(this, composition, line_idx)) return;
///
///     // step 3 (line 42-61): ParaProperty SharePtr from first-CR view at para_idx.
///     local_48 = 0;
///     lVar3 = GetParaItemView(this, composition, para_idx);
///     if (lVar3 != 0) {
///         // SharePtr assignment local_48 = *(lVar3 + 0x20) (ParaProperty slot)
///     }
///
///     // step 4 (line 62-99): Bullet SharePtr — default empty shell + cache lookup, then
///     //   override from ParaProperty.bullet (CharItemView+0x20)+8 if non-null.
///     puVar4 = new(8); *puVar4 = &PTR_FUN_0077fdc0;  // empty Bullet vtable
///     local_50 = SharePtr-wrap(puVar4);
///     auVar14 = FUN_0067e2cc(hash, &local_50, &local_50);  // intern/cache
///     if (cache_hit) local_50 = cached;
///     if (local_48 != 0 && *local_48 != 0) {
///         local_58 = *(local_48 + 8);  // ParaProperty.bullet SharePtr
///         if (local_58 != 0 && *local_58 != 0 && *local_58 != *local_50) {
///             local_50 = local_58;  // override
///         }
///     }
///
///     // step 5 (line 100-102): Bullet validity via vtable[+0x30] (= GetType()).
///     if (local_50 == 0 || *local_50 == 0 || (vtable[+0x30])() == 0) goto cleanup;
///
///     // step 6 (line 103-158): target = GetFirstCharItemViewOnPara(line_idx + 1).
///     local_60 = 0; local_58 = 0;
///     lVar9 = GetFirstCharItemViewOnPara(this, composition, line_idx + 1);
///     if (lVar9 != 0) {
///         local_58 = *(lVar9 + 0x18);  // RunProperty SharePtr
///         local_60 = *(lVar9 + 0x28);  // BodyProperty SharePtr
///     }
///
///     // step 7 (line 159-172): numbering lookup if RunProperty + target both valid.
///     if (local_58 != 0 && lVar9 != 0 && *local_58 != 0) {
///         uVar13 = 1;
///         if (lVar3 != 0) {
///             // backward walk numbering_table; find pair.first == lVar3.
///             for (it = end; it != begin; --it) {
///                 if (it[-2] == lVar3) { uVar13 = it[-1]; break; }
///             }
///         }
///
///         // step 8 (line 173-176): construct BulletRenderGlyph.
///         theme_ptr = *(lVar9 + 0x90);
///         plVar7 = new(0x28);  // BulletRenderGlyph 40B
///         FUN_002eaf54(plVar7, &local_58, &local_48, &local_60, theme_ptr, uVar13);
///         plVar10 = SharePtr-wrap(plVar7);
///
///         // step 9 (line 177-213): store SharePtr at lVar9+0x98 (render_path).
///         // 기존 render_path SharePtr 가 있으면 inner pointer 비교:
///         //   - 같으면 do-nothing (early return after refcount juggle),
///         //   - 다르면 기존 release + 신규 store.
///         puVar4 = *(lVar9 + 0x98);
///         if (puVar4 == 0 || *puVar4 != plVar7) {
///             release(puVar4); *(lVar9 + 0x98) = plVar10;
///         } else {
///             release(plVar10);  // duplicate, discard
///         }
///     }
///     // cleanup local_60, local_58, local_50, local_48 (refcounts)
/// }
/// ```
///
/// **Rust 매핑 차이**:
/// 1. SharePtr 의 refcount 정리는 Rust `ownership/drop` 으로 자동.
/// 2. `FUN_0067e2cc` 의 Bullet cache 는 layout 무관 (intern only). Rust 는 ParaProperty.bullet
///    유무만 본다.
/// 3. `lVar3` (CR pointer) 는 [`find_para_cr_view`] 가 반환한 `&CharItemView` 의 raw ptr cast.
///    `get_para_item_view` 의 clone 경로는 pointer identity 가 보존되지 않아 numbering vector
///    lookup 에 부적합 — 그래서 별도 helper.
/// 4. 기존 `target.render_path` 가 same-inner 인 경우의 do-nothing 분기는 Rust 에서는 항상
///    overwrite 로 처리 — raw 의 의도는 refcount 안정성 (이미 같은 SharePtr 면 ref count 만
///    유지) 이므로 layout output 에 영향 없음. 즉 매번 새 BulletRenderGlyph 를 만들어 set 해도
///    bytecode-level layout 출력은 동일.
///
/// **side-effect (관찰 가능)**: composition 의 `line_idx + 1` 위치 CharItemView (`+0x98` 슬롯) 에
/// 신규 `BulletRenderGlyph` SharePtr 가 저장된다 — 후속 `ComposeBreak` / `ComposeLayout` 의
/// `+0x98` 호출이 본 BulletRenderGlyph 의 `Request` (= 내부 HBox/VBox 의 Request) 를 부른다.
pub fn ppt_compose_bullet(
    line_idx: i32,
    para_idx: i32,
    numbering_table: &[crate::compositor::NumberingEntry],
    composition: &mut dyn Glyph,
    ct_provider: &dyn crate::font_metric::CoreTextFontProvider,
    gm_provider: &dyn crate::font_metric::GlobalMetricProvider,
) {
    // ── Phase 1: 모든 immutable inspection 을 한 scope 에 모음. borrow checker 준수 위해
    //   blocks 종료 시점에 composition 의 immutable borrow 모두 해제, 이후 Phase 2 의
    //   mutable write 가 가능.
    let (bullet_render, target_line) = {
        // raw line 38-41: IsFirstLineOnPara 가드.
        if !crate::ppt_compositor::is_first_line_on_para(composition, line_idx) {
            return;
        }

        // raw line 42-61: para_property + CR pointer key.
        //   `&CharItemView as *const _ as usize` 가 raw 의 `lVar3` (vector lookup key) 와
        //   동일 identity 보존.
        let (para_property, para_view_key) =
            match crate::ppt_compositor::find_para_cr_view(composition, para_idx) {
                Some(view) => (
                    view.para_property.clone(),
                    Some(view as *const crate::glyph::CharItemView as usize),
                ),
                None => (None, None),
            };

        // raw line 62-102: Bullet retrieval + GetType validity check.
        //
        //   raw 흐름은 default empty Bullet shell (vtable `PTR_FUN_0077fdc0`) 를 만들어
        //   intern cache (`FUN_0067e2cc`) 에 등록하고, ParaProperty.bullet 가 있으면 그것으로
        //   override. 그 후 step 5 가드:
        //     if (vtable[+0x30] /* GetType */) == 0  → cleanup goto
        //
        //   **동등성 증명** (`bullet_render_deps.txt:1029-1041`):
        //     `PTR_FUN_0077fdc0` 의 slot 6 (+0x30) = `FUN_002e696c` (8B):
        //         mov w0, #0x0
        //         ret
        //     즉 default shell 의 GetType 은 무조건 0. 따라서 ParaProperty.bullet=None 케이스
        //     에서 raw 의 cache + default shell 경로는 **반드시** step 5 가드에서 cleanup —
        //     BulletRenderGlyph 생성도, render_path mutation 도 일어나지 않음.
        //
        //   Rust 는 cache/default-shell 단계를 생략하고 `ParaProperty.bullet` 의 `get_type()`
        //   를 직접 본다. `None → 0` 처리가 raw 의 default-shell GetType=0 과 byte-equivalent.
        let bullet_type = para_property
            .as_ref()
            .and_then(|p| p.get_bullet())
            .map(|b| b.get_type())
            .unwrap_or(0);
        if bullet_type == 0 {
            return;
        }

        // raw line 103-158: target = GetFirstCharItemViewOnPara(line_idx + 1).
        //   여기서는 immutable 로 RunProperty/BodyProperty/Theme clone 만 가져옴 — Phase 2
        //   에서 mutable 로 다시 잡아 render_path write.
        let target_line = line_idx + 1;
        let target_view = match crate::ppt_compositor::get_first_char_item_view_on_para(
            composition,
            target_line,
        ) {
            Some(b) => b,
            None => return,
        };

        // raw line 159: `if (local_58 != 0 && lVar9 != 0 && *local_58 != 0)` —
        //   RunProperty SharePtr 의 outer + inner 모두 non-null. Rust 는 `Option<RunProperty>`
        //   의 `Some(rp)` 가 등가.
        let run_property = match target_view.run_property.clone() {
            Some(rp) => rp,
            None => return,
        };
        let body_property = target_view.body_property.clone();
        // raw line 174: `uVar5 = *(undefined8 *)(lVar9 + 0x90);` (Theme*).
        //   Rust 는 owned Option<Theme>; None 이면 default Theme.
        let theme = target_view.theme.clone().unwrap_or_default();

        // raw line 160-172: starting_numbering 결정.
        let starting_numbering = lookup_starting_numbering(numbering_table, para_view_key);

        // raw line 173-176: BulletRenderGlyph 생성 (FUN_002eaf54 = port_bullet_render).
        let bullet_render = port_bullet_render(
            para_property.as_ref(),
            body_property.as_ref(),
            run_property,
            &theme,
            starting_numbering as i32,
            ct_provider,
            gm_provider,
        );

        (bullet_render, target_line)
    };

    // ── Phase 2: mutable write — composition 의 target CharItemView 의 `+0x98` 에 신규
    //   BulletRenderGlyph SharePtr 저장. raw line 177-213 의 SharePtr 교체 시퀀스.
    //
    //   raw 의 분기 (line 195-213):
    //     plVar8 = *(기존 render_path SharePtr).inner;  // 기존 inner ptr
    //     if (plVar8 != plVar7 /*신규 inner*/) {
    //       // case A: 다른 객체 — 기존 release + 신규 store
    //       *(lVar9 + 0x98) = plVar10;
    //     } else {
    //       // case B: 같은 inner — 신규 SharePtr 해제 (중복), 기존 유지
    //       operator_delete(plVar10);
    //     }
    //
    //   **case B 는 dead branch**: `plVar7 = new(0x28)` 가 본 함수 안에서 막 alloc 된 새
    //   객체이고 free 이력 없으므로, 기존 render_path 의 inner 와 주소가 일치할 수 없다.
    //   reachable 한 입력 도메인에서 case B 는 0건. → Rust 의 "항상 overwrite" 는 case A
    //   와 byte-equivalent, case B 는 unreachable 이므로 무관.
    //
    //   refcount 정리는 Rust 의 `Drop` 으로 대체 (기존 Box 가 drop 되면서 inner Glyph 의
    //   destructor 가 호출됨, raw 의 `(*plVar8.vtable[1])(); operator_delete(puVar4);` 와 등가).
    let target = match crate::ppt_compositor::get_first_char_item_view_on_para_mut(
        composition,
        target_line,
    ) {
        Some(v) => v,
        None => return,
    };
    target.render_path = Some(Box::new(bullet_render));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::properties::{HashMapPropertyBag, PropertyValue};
    use crate::value_types::{Requirement, Requisition};

    #[derive(Debug, Clone)]
    struct StaticReqGlyph(Requisition);

    impl Glyph for StaticReqGlyph {
        fn clone_glyph(&self) -> Box<dyn Glyph> {
            Box::new(self.clone())
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
        fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
            self
        }
        fn request(&self, req_out: &mut Requisition) {
            *req_out = self.0;
        }
    }

    fn req(x: f32, y: f32) -> Box<dyn Glyph> {
        Box::new(StaticReqGlyph(Requisition {
            x: Requirement::new(x, 0.0, 0.0, 0.0),
            y: Requirement::new(y, 0.0, 0.0, 0.0),
            penalty: 0,
        }))
    }

    #[test]
    fn paragraph_class_defaults_to_one_without_body_property() {
        assert_eq!(bullet_render_paragraph_class(None), 1);
        assert!(!bullet_render_uses_vbox(1));
    }

    #[test]
    fn vbox_mask_matches_raw_0x65() {
        let vertical: Vec<u32> = (0..=8)
            .filter(|&class| bullet_render_uses_vbox(class))
            .collect();
        assert_eq!(vertical, vec![0, 2, 5, 6]);
    }

    #[test]
    fn body_property_controls_layout_direction() {
        let mut body = BodyProperty::new();
        body.property_bag
            .insert(PropertyKey::new(crate::text_property::KEY_VERT), PropertyValue::Uint(5));

        let bullet = create_bullet_render_with_children(
            Some(&body),
            1,
            0.0,
            1,
            vec![req(10.0, 3.0), req(10.0, 3.0)],
        );

        let mut out = Requisition::INVALID;
        bullet.request(&mut out);
        assert_eq!(out.x.natural, 10.0, "VBox keeps cross-axis max");
        assert_eq!(out.y.natural, 6.0, "VBox sums vertical axis");
    }

    #[test]
    fn missing_or_default_body_uses_hbox() {
        let bullet = create_bullet_render_with_children(
            None,
            1,
            0.0,
            1,
            vec![req(10.0, 3.0), req(10.0, 3.0)],
        );

        let mut out = Requisition::INVALID;
        bullet.request(&mut out);
        assert_eq!(out.x.natural, 20.0, "HBox sums horizontal axis");
        assert_eq!(out.y.natural, 3.0, "HBox keeps cross-axis max");
    }

    #[test]
    fn key_901_is_copied_from_para_property() {
        let mut para = ParaProperty::new();
        para.property_bag.insert(
            PropertyKey::new(KEY_BULLET_RENDER_901),
            PropertyValue::Float(4.25),
        );

        let bullet = create_bullet_render_from_properties(
            Some(&para),
            None,
            3,
            7,
            vec![req(1.0, 1.0)],
        );

        assert_eq!(bullet.bullet_type, 3);
        assert_eq!(bullet.key_901, 4.25);
        assert_eq!(bullet.numbering, 7);
    }

    #[test]
    fn missing_key_901_falls_back_to_zero_in_semantic_bag() {
        let para = ParaProperty {
            bullet: None,
            text_font: None,
            property_bag: HashMapPropertyBag::new(),
        };
        assert_eq!(bullet_render_key_901(Some(&para)), 0.0);
        assert_eq!(bullet_render_key_901(None), 0.0);
    }

    // ============================================================
    // step6_compute_bullet_glyph_size 테스트 — FUN_002eaf54 line 616-680
    // ============================================================

    fn text_font_with_size(size: f32) -> TextFont {
        let mut tf = TextFont::new();
        tf.property_bag
            .insert(PropertyKey::new(crate::text_property::KEY_TEXTFONT_FONT_SIZE),
                    PropertyValue::Float(size));
        tf
    }

    #[test]
    fn step6_no_param_uses_textfont_font_size_directly() {
        // raw line 664-680: no 0x90f → fVar27 = TextFont.0x96a, clamp, fVar26 = fVar27*dpi/72.
        let mut tf = text_font_with_size(20.0);
        let dpi = 72.0;
        let f26 = step6_compute_bullet_glyph_size(None, &mut tf, dpi);
        assert_eq!(f26, 20.0, "fVar26 == TextFont.size (dpi=72 → 1:1)");
        // raw 는 no-write 경로 — TextFont 의 0x96a 는 그대로 20.0.
        assert_eq!(tf.get_font_size_raw(), 20.0);
    }

    #[test]
    fn step6_no_param_with_zero_textfont_clamps_to_ten() {
        // raw line 676-678: fVar27 <= 0 → 10. 그 다음 fVar26 = 10 * dpi / 72.
        let mut tf = TextFont::new();  // empty → get_font_size_raw() = 0.0
        let dpi = 144.0;
        let f26 = step6_compute_bullet_glyph_size(None, &mut tf, dpi);
        assert!((f26 - 20.0).abs() < 1e-6, "10 * 144 / 72 == 20, got {}", f26);
        // raw no-write.
        assert_eq!(tf.get_font_size_raw(), 0.0);
    }

    #[test]
    fn step6_mode_1_is_absolute_pt_size() {
        // raw line 617-620: iVar6 == 1 → fVar26 = factor * dpi / 72. TextFont 무관.
        let mut tf = text_font_with_size(100.0);  // 무관: mode 1 은 TextFont 안 읽음
        let f26 = step6_compute_bullet_glyph_size(Some((1, 18.0)), &mut tf, 96.0);
        assert!((f26 - 24.0).abs() < 1e-6, "18 * 96 / 72 == 24, got {}", f26);
        // raw 660: write back fVar27 = 18.0 (clamp 안 됨, 18 > 0).
        assert_eq!(tf.get_font_size_raw(), 18.0, "mode 1 overwrites TextFont with raw factor");
    }

    #[test]
    fn step6_mode_1_with_low_factor_clamps_writeback_to_ten_but_not_f26() {
        // raw 648-650 의 clamp 는 fVar27 에만 — fVar26 은 clamp 이전 값 유지.
        let mut tf = text_font_with_size(50.0);
        let f26 = step6_compute_bullet_glyph_size(Some((1, 0.0)), &mut tf, 144.0);
        // fVar26 = 0 * 144 / 72 = 0 (clamp 안 됨).
        assert_eq!(f26, 0.0, "fVar26 reflects pre-clamp factor=0");
        // fVar27 = 0 → clamp to 10. write 10 to TextFont.
        assert_eq!(tf.get_font_size_raw(), 10.0);
    }

    #[test]
    fn step6_mode_other_multiplies_textfont_size() {
        // raw 622-647: iVar6 != 1 → fVar26 = factor * (size * dpi / 72), fVar27 = factor * size.
        let mut tf = text_font_with_size(12.0);
        let f26 = step6_compute_bullet_glyph_size(Some((2, 1.5)), &mut tf, 96.0);
        // factor=1.5, size=12, dpi=96 → fVar26 = 1.5 * (12 * 96 / 72) = 1.5 * 16 = 24.
        assert!((f26 - 24.0).abs() < 1e-6, "got {}", f26);
        // fVar27 = 1.5 * 12 = 18 (> 0, no clamp). write 18.
        assert!((tf.get_font_size_raw() - 18.0).abs() < 1e-6);
    }

    #[test]
    fn step6_mode_other_with_zero_textfont_yields_zero_f26_but_writes_ten() {
        // factor != 1, TextFont size == 0 → fVar26 = factor * 0 = 0, fVar27 = factor * 0 = 0
        // → clamp to 10 → write 10.
        let mut tf = TextFont::new();  // size raw = 0
        let f26 = step6_compute_bullet_glyph_size(Some((3, 2.0)), &mut tf, 72.0);
        assert_eq!(f26, 0.0, "raw factor*0 = 0, no clamp on fVar26");
        assert_eq!(tf.get_font_size_raw(), 10.0, "fVar27 clamped to 10");
    }

    #[test]
    fn step6_global_dpi_wrapper_uses_shape_engine() {
        ShapeEngine::_reset_for_test();
        ShapeEngine::start(120.0);
        let mut tf = text_font_with_size(10.0);
        let f26 = step6_with_global_dpi(None, &mut tf);
        // 10 * 120 / 72 ≈ 16.6666...
        assert!((f26 - 16.66666667).abs() < 1e-4, "got {}", f26);
        ShapeEngine::_reset_for_test();
    }

    // ============================================================
    // FUN_002eaf54 outer end-to-end — Character bullet (Type 1)
    // ============================================================

    use crate::font_metric::{
        CoreTextFontProvider, GlobalFontMetrics, GlobalMetricProvider, SystemFont,
    };
    use crate::runtime::{Font, FontTable, RunProperty, Theme};
    use crate::text_property::Bullet;
    use std::cell::RefCell;
    use std::collections::HashMap;

    struct E2eCt {
        advances: HashMap<(SystemFont, u16), f64>,
        calls: RefCell<Vec<(SystemFont, u16)>>,
    }

    impl E2eCt {
        fn with(font: SystemFont, c: u16, adv: f64) -> Self {
            let mut advances = HashMap::new();
            advances.insert((font, c), adv);
            Self {
                advances,
                calls: RefCell::new(Vec::new()),
            }
        }
    }

    impl CoreTextFontProvider for E2eCt {
        fn glyph_for_character(&self, font: SystemFont, _size_px: f64, c: u16) -> u16 {
            self.calls.borrow_mut().push((font, c));
            c
        }
        fn advance_for_glyph(&self, font: SystemFont, _size_px: f64, glyph: u16) -> f64 {
            *self.advances.get(&(font, glyph)).unwrap_or(&0.0)
        }
    }

    struct E2eGm {
        metrics: HashMap<String, GlobalFontMetrics>,
    }
    impl E2eGm {
        fn with<S: Into<String>>(face: S, gm: GlobalFontMetrics) -> Self {
            let mut metrics = HashMap::new();
            metrics.insert(face.into(), gm);
            Self { metrics }
        }
    }
    impl GlobalMetricProvider for E2eGm {
        fn global_metrics(&self, font_name: &str, _font_style: i32) -> GlobalFontMetrics {
            *self.metrics.get(font_name).expect("face not configured")
        }
    }

    #[test]
    fn port_bullet_render_character_builds_hbox_with_single_charitemview() {
        ShapeEngine::_reset_for_test();
        // 정공법 byte-equivalent path: '•' (U+2022) bullet character.
        // realize_font(0x2022) → char_item_view_char_class = 0 → select_font_slot = Latin
        //   → Latin slot = "Arial".
        // measure_string_advance: select_system_font(0x2022, false) → StHeitiTcMedium (CJK
        //   lane via `(c & 0xe000) != 0`, raw 0x82f38). Mock must serve (StHeitiTcMedium, 0x2022).
        let ct = E2eCt::with(SystemFont::StHeitiTcMedium, 0x2022, 4.0);
        let gm = E2eGm::with(
            "Arial",
            GlobalFontMetrics { em: 1000.0, ascent: 700.0, m7: 300.0, m8: 0.0 },
        );

        let mut para = ParaProperty::new();
        para.bullet = Some(Bullet::Character { chars: vec![0x2022] });

        let run = RunProperty::new(10.0).with_font_table(FontTable {
            latin: Some(Font::new("Arial")),
            ..Default::default()
        });
        let theme = Theme::new();

        let bullet = port_bullet_render_character(
            Some(&para),
            None,
            run,
            &theme,
            /*numbering*/ 1,
            &ct,
            &gm,
        );

        assert_eq!(bullet.bullet_type, 1, "Character bullet GetType");
        assert_eq!(bullet.numbering, 1);
        assert_eq!(bullet.key_901, 0.0, "no 0x901 in bag");

        let mut req = crate::value_types::Requisition::INVALID;
        bullet.request(&mut req);
        // CharCtorMetrics:
        //   mul = MulDiv(10, 96, 72) = 13 (ties-away). em_r=1000.
        //   width = 4.0 * 72/96 = 3.0.
        //   ascent = 700 * 13 / 1000 = 9.1.  m7 = 300 * 13 / 1000 = 3.9.
        // compute_metrics class=1 else-branch (no body_property → default 1):
        //   total_height = 0 + 3*96/72 = 4.0
        //   line_height  = 10*1.2*96/72 = 16.0
        //   vertical_anchor_ratio = 9.1/(9.1+3.9) = 0.7
        // CharItemView::request else-branch: req.x.natural = total_height = 4.0.
        // HBox::request_inner: sum children x.natural = 4.0.
        assert!((req.x.natural - 4.0).abs() < 1e-5, "req.x = {:?}", req.x);
        // glyph_for_character 가 한 번만 호출 (Type 1 은 첫 char 만), StHeitiTcMedium 으로.
        assert_eq!(ct.calls.borrow().len(), 1);
        assert_eq!(ct.calls.borrow()[0], (SystemFont::StHeitiTcMedium, 0x2022));
        ShapeEngine::_reset_for_test();
    }

    #[test]
    fn port_bullet_render_character_with_empty_bullet_produces_empty_layout() {
        ShapeEngine::_reset_for_test();
        // bullet=None → bullet_type=0, child glyph 없음. Request 도 빈 HBox.
        let ct = E2eCt::with(SystemFont::Helvetica, 0, 0.0);
        let gm = E2eGm::with(
            "Arial",
            GlobalFontMetrics { em: 1000.0, ascent: 700.0, m7: 300.0, m8: 0.0 },
        );

        let para = ParaProperty::new(); // no bullet
        let run = RunProperty::new(10.0).with_font_table(FontTable {
            latin: Some(Font::new("Arial")),
            ..Default::default()
        });

        let bullet = port_bullet_render_character(
            Some(&para),
            None,
            run,
            &Theme::new(),
            5,
            &ct,
            &gm,
        );

        assert_eq!(bullet.bullet_type, 0, "no bullet → type 0");
        assert_eq!(bullet.numbering, 5);
        // 호출 없음 — child 만들지 않음.
        assert!(ct.calls.borrow().is_empty());
        ShapeEngine::_reset_for_test();
    }

    #[test]
    fn port_bullet_render_character_only_first_char_used() {
        // raw line 952: `uVar1 = **(ushort **)(lVar18 + 8);` — chars[0] 만 사용. chars.length>1
        // 이어도 단 한 개의 CharItemView 만 만들어짐.
        ShapeEngine::_reset_for_test();
        let ct = E2eCt::with(SystemFont::Helvetica, b'A' as u16, 5.0);
        let gm = E2eGm::with(
            "Arial",
            GlobalFontMetrics { em: 1000.0, ascent: 700.0, m7: 300.0, m8: 0.0 },
        );
        let mut para = ParaProperty::new();
        para.bullet = Some(Bullet::Character {
            chars: vec![b'A' as u16, b'B' as u16, b'C' as u16],
        });
        let run = RunProperty::new(10.0).with_font_table(FontTable {
            latin: Some(Font::new("Arial")),
            ..Default::default()
        });
        let _ = port_bullet_render_character(
            Some(&para),
            None,
            run,
            &Theme::new(),
            1,
            &ct,
            &gm,
        );
        // Only one measurement (for 'A').
        let calls = ct.calls.borrow();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].1, b'A' as u16);
        ShapeEngine::_reset_for_test();
    }

    // ============================================================
    // port_bullet_render_autonum (Type 3) endto end
    // ============================================================

    fn ct_multi(pairs: &[(SystemFont, u16, f64)]) -> E2eCt {
        let mut advances = HashMap::new();
        for &(f, c, a) in pairs {
            advances.insert((f, c), a);
        }
        E2eCt {
            advances,
            calls: RefCell::new(Vec::new()),
        }
    }

    #[test]
    fn port_bullet_render_autonum_decimal_period_format_8() {
        // format_type=8 ("%d."), start_at=1, numbering=1 → autonum_string(8, 1) = "1."
        // → 2 chars '1', '.'.
        ShapeEngine::_reset_for_test();
        let ct = ct_multi(&[
            (SystemFont::Helvetica, b'1' as u16, 6.0),
            (SystemFont::Helvetica, b'.' as u16, 3.0),
        ]);
        let gm = E2eGm::with(
            "Arial",
            GlobalFontMetrics { em: 1000.0, ascent: 700.0, m7: 300.0, m8: 0.0 },
        );

        let mut para = ParaProperty::new();
        para.bullet = Some(Bullet::AutoNumber {
            format_type: 8,
            start_at: 1,
        });
        let run = RunProperty::new(10.0).with_font_table(FontTable {
            latin: Some(Font::new("Arial")),
            ..Default::default()
        });

        let bullet = port_bullet_render_autonum(
            Some(&para),
            None,
            run,
            &Theme::new(),
            /* numbering */ 1,
            &ct,
            &gm,
        );

        assert_eq!(bullet.bullet_type, 3, "AutoNumber GetType");
        assert_eq!(bullet.numbering, 1);

        // measure_string_advance 호출 = 2 chars (1, .).
        let calls = ct.calls.borrow();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].1, b'1' as u16);
        assert_eq!(calls[1].1, b'.' as u16);
    }

    #[test]
    fn port_bullet_render_autonum_lower_roman_format_e() {
        // format_type=0xE (lower roman + "."), start_at=1, numbering=4 → autonum_string(0xE, 4) =
        // "iv.". 3 chars.
        ShapeEngine::_reset_for_test();
        let ct = ct_multi(&[
            (SystemFont::Helvetica, b'i' as u16, 2.5),
            (SystemFont::Helvetica, b'v' as u16, 5.0),
            (SystemFont::Helvetica, b'.' as u16, 3.0),
        ]);
        let gm = E2eGm::with(
            "Arial",
            GlobalFontMetrics { em: 1000.0, ascent: 700.0, m7: 300.0, m8: 0.0 },
        );

        let mut para = ParaProperty::new();
        para.bullet = Some(Bullet::AutoNumber {
            format_type: 0xE,
            start_at: 1,
        });
        let run = RunProperty::new(10.0).with_font_table(FontTable {
            latin: Some(Font::new("Arial")),
            ..Default::default()
        });

        // raw value = start_at + numbering - 1 = 1 + 4 - 1 = 4 → "iv."
        let bullet = port_bullet_render_autonum(
            Some(&para),
            None,
            run,
            &Theme::new(),
            /* numbering */ 4,
            &ct,
            &gm,
        );

        assert_eq!(bullet.bullet_type, 3);
        let calls = ct.calls.borrow();
        assert_eq!(calls.len(), 3);
        assert_eq!(calls[0].1, b'i' as u16);
        assert_eq!(calls[1].1, b'v' as u16);
        assert_eq!(calls[2].1, b'.' as u16);
    }

    #[test]
    fn port_bullet_render_autonum_value_uses_start_plus_numbering_minus_1() {
        // raw line 753: value = start_at + numbering - 1.
        // start_at=10, numbering=3 → value = 12. format=8 → "12.".
        ShapeEngine::_reset_for_test();
        let ct = ct_multi(&[
            (SystemFont::Helvetica, b'1' as u16, 6.0),
            (SystemFont::Helvetica, b'2' as u16, 6.0),
            (SystemFont::Helvetica, b'.' as u16, 3.0),
        ]);
        let gm = E2eGm::with(
            "Arial",
            GlobalFontMetrics { em: 1000.0, ascent: 700.0, m7: 300.0, m8: 0.0 },
        );

        let mut para = ParaProperty::new();
        para.bullet = Some(Bullet::AutoNumber {
            format_type: 8,
            start_at: 10,
        });
        let run = RunProperty::new(10.0).with_font_table(FontTable {
            latin: Some(Font::new("Arial")),
            ..Default::default()
        });

        let _ = port_bullet_render_autonum(
            Some(&para),
            None,
            run,
            &Theme::new(),
            /* numbering */ 3,
            &ct,
            &gm,
        );

        let calls = ct.calls.borrow();
        assert_eq!(calls.len(), 3);
        assert_eq!(calls[0].1, b'1' as u16);
        assert_eq!(calls[1].1, b'2' as u16);
        assert_eq!(calls[2].1, b'.' as u16);
    }

    // ============================================================
    // port_bullet_render_picture (Type 2) endto end
    // ============================================================

    #[test]
    fn port_bullet_render_picture_with_source_scales_to_70_percent_of_bullet_size() {
        // raw line 866-869: scale = bullet_glyph_size_px / img_h_px * 0.7. → height = 0.7 *
        // bullet_glyph_size_px (when img_h_px > 0).
        //
        // Setup:
        //   ParaProperty.bullet_size = (1, 20.0)  → mode 1, factor=20pt (absolute size).
        //   step6: fVar26 = (20 * dpi) / 72 (at dpi=72 → 20.0px).
        //   ImageSource: width=100, height=50 (96dpi).
        //   dpi=72: img_w_px = 100 * 72/96 = 75; img_h_px = 50 * 72/96 = 37.5
        //   scale = (20.0 / 37.5) * 0.7 ≈ 0.3733
        //   BlipGlyph.width  = 75 * 0.3733 ≈ 28.0
        //   BlipGlyph.height = 37.5 * 0.3733 ≈ 14.0 = bullet_glyph_size_px * 0.7
        use crate::text_property::{ImageBrush, ImageSource};
        ShapeEngine::_reset_for_test();
        ShapeEngine::start(72.0);

        let mut para = ParaProperty::new();
        para.bullet = Some(Bullet::Picture {
            brush: ImageBrush::new(ImageSource {
                width_units: 100.0,
                height_units: 50.0,
            }),
        });
        para.property_bag.insert(
            PropertyKey::new(crate::text_property::KEY_BULLET_SIZE),
            PropertyValue::IntFloat(1, 20.0),
        );

        let bullet = port_bullet_render_picture(Some(&para), None, /* numbering */ 1);

        assert_eq!(bullet.bullet_type, 2, "Picture bullet GetType");

        // Layout 의 child = BlipGlyph 1 개. Request 로 width / height 확인.
        // BlipGlyph::request 의 paragraph_class branch 가 width/height 를 어디로 쓰는지 별도
        // 검증. 본 테스트는 BlipGlyph 의 width/height 값만 확인 — layout 의 첫 child 를
        // downcast.
        let mut req = crate::value_types::Requisition::INVALID;
        bullet.request(&mut req);
        // BlipGlyph 의 layout 은 paragraph_class=1 (default, no body) → BlipGlyph::request 의
        // pc<7 && (1<<pc)&0x65!=0 가 (1<<1)&0x65 = 2&0x65 = 0 → default branch.
        // BlipGlyph::request default branch: req.x = (width, ...). req.x.natural = width.
        // expected width = 75 * (20/37.5) * 0.7 = 75 * 0.5333 * 0.7 = 28.0.
        assert!(
            (req.x.natural - 28.0).abs() < 1e-3,
            "req.x = {:?}, expected width=28",
            req.x
        );
        ShapeEngine::_reset_for_test();
    }

    #[test]
    fn port_bullet_render_picture_no_source_yields_zero_dim_blip() {
        // raw line 852-854: brush.GetImageSource() == null → fVar27=1.0, fVar28=0, fVar29=0.
        // BlipGlyph.width = fVar29 * fVar27 = 0; height = fVar28 * fVar27 = 0.
        use crate::text_property::ImageBrush;
        ShapeEngine::_reset_for_test();
        let mut para = ParaProperty::new();
        para.bullet = Some(Bullet::Picture {
            brush: ImageBrush::empty(),
        });
        let bullet = port_bullet_render_picture(Some(&para), None, 1);
        assert_eq!(bullet.bullet_type, 2);
        let mut req = crate::value_types::Requisition::INVALID;
        bullet.request(&mut req);
        // BlipGlyph width=0, height=0 → HBox::request_inner sum = 0.
        assert_eq!(req.x.natural, 0.0);
        ShapeEngine::_reset_for_test();
    }

    #[test]
    fn port_bullet_render_picture_image_height_zero_uses_scale_one() {
        // raw line 866-869: img_h_px == 0 → scale = 1.0 (그대로 사용).
        // ImageSource (10, 0): height=0 → scale=1.0, width=10*72/96=7.5, height=0.
        // BlipGlyph: width=7.5*1=7.5, height=0*1=0.
        use crate::text_property::{ImageBrush, ImageSource};
        ShapeEngine::_reset_for_test();
        ShapeEngine::start(72.0);

        let mut para = ParaProperty::new();
        para.bullet = Some(Bullet::Picture {
            brush: ImageBrush::new(ImageSource {
                width_units: 10.0,
                height_units: 0.0,
            }),
        });

        let bullet = port_bullet_render_picture(Some(&para), None, 1);
        assert_eq!(bullet.bullet_type, 2);
        let mut req = crate::value_types::Requisition::INVALID;
        bullet.request(&mut req);
        // BlipGlyph width=7.5, height=0 → req.x.natural=7.5.
        assert!((req.x.natural - 7.5).abs() < 1e-5);
        ShapeEngine::_reset_for_test();
    }

    #[test]
    fn port_bullet_render_picture_paragraph_class_propagates_to_blip_via_layout() {
        // raw line 903: BlipGlyph +0x38 = local_164 = paragraph_class. 본 모델은
        // BlipGlyph.paragraph_class 필드로 보존. body_property 의 vert=5 → paragraph_class=5
        // → VBox + BlipGlyph.paragraph_class=5.
        use crate::text_property::{ImageBrush, ImageSource, KEY_VERT};
        ShapeEngine::_reset_for_test();
        ShapeEngine::start(72.0);

        let mut body = BodyProperty::new();
        body.property_bag
            .insert(PropertyKey::new(KEY_VERT), PropertyValue::Uint(5));

        let mut para = ParaProperty::new();
        para.bullet = Some(Bullet::Picture {
            brush: ImageBrush::new(ImageSource {
                width_units: 50.0,
                height_units: 25.0,
            }),
        });
        para.property_bag.insert(
            PropertyKey::new(crate::text_property::KEY_BULLET_SIZE),
            PropertyValue::IntFloat(1, 10.0),
        );

        let bullet = port_bullet_render_picture(Some(&para), Some(&body), 1);
        assert_eq!(bullet.bullet_type, 2);

        // paragraph_class=5 → VBox 선택됨 (vbox mask 0x65 contains bit 5).
        // BlipGlyph.paragraph_class=5 → BlipGlyph::request 의 special branch (pc<7 && (1<<5)&0x65 !=0).
        // VBox::request_inner 가 children 의 y 를 합산.
        let mut req = crate::value_types::Requisition::INVALID;
        bullet.request(&mut req);
        // BlipGlyph special branch 의 y = (width, ...). width = 50*72/96 * (10/(25*72/96))*0.7 =
        //   50*0.75 * (10/18.75)*0.7 = 37.5 * 0.5333*0.7 = 37.5 * 0.3733 ≈ 14.0
        // BlipGlyph special branch:
        //   req.x = (height, 0, 0, 1.0 - 0.0) = (height, ...)
        //   req.y = (width, ...)
        // height = 25*0.75 * 0.3733 ≈ 7.0.
        // VBox sum y = 14.0.
        assert!((req.y.natural - 14.0).abs() < 1e-3, "req.y={:?}", req.y);
        ShapeEngine::_reset_for_test();
    }

    // ============================================================
    // port_bullet_render (unified dispatcher) tests
    // ============================================================

    #[test]
    fn unified_dispatcher_routes_to_character_for_type_1() {
        ShapeEngine::_reset_for_test();
        let ct = E2eCt::with(SystemFont::StHeitiTcMedium, 0x2022, 4.0);
        let gm = E2eGm::with(
            "Arial",
            GlobalFontMetrics { em: 1000.0, ascent: 700.0, m7: 300.0, m8: 0.0 },
        );
        let mut para = ParaProperty::new();
        para.bullet = Some(Bullet::Character { chars: vec![0x2022] });
        let run = RunProperty::new(10.0).with_font_table(FontTable {
            latin: Some(Font::new("Arial")),
            ..Default::default()
        });
        let bullet = port_bullet_render(
            Some(&para),
            None,
            run,
            &Theme::new(),
            1,
            &ct,
            &gm,
        );
        assert_eq!(bullet.bullet_type, 1);
        // glyph_for_character 1 회 호출됨 → Character 분기 confirm.
        assert_eq!(ct.calls.borrow().len(), 1);
        ShapeEngine::_reset_for_test();
    }

    #[test]
    fn unified_dispatcher_routes_to_picture_for_type_2() {
        use crate::text_property::{ImageBrush, ImageSource};
        ShapeEngine::_reset_for_test();
        ShapeEngine::start(72.0);
        let ct = ct_multi(&[]);
        let gm = E2eGm::with(
            "Arial",
            GlobalFontMetrics { em: 1000.0, ascent: 700.0, m7: 300.0, m8: 0.0 },
        );
        let mut para = ParaProperty::new();
        para.bullet = Some(Bullet::Picture {
            brush: ImageBrush::new(ImageSource { width_units: 100.0, height_units: 50.0 }),
        });
        para.property_bag.insert(
            PropertyKey::new(crate::text_property::KEY_BULLET_SIZE),
            PropertyValue::IntFloat(1, 20.0),
        );
        let run = RunProperty::new(10.0);
        let bullet = port_bullet_render(
            Some(&para),
            None,
            run,
            &Theme::new(),
            1,
            &ct,
            &gm,
        );
        assert_eq!(bullet.bullet_type, 2);
        // Picture 분기는 CT 호출 안 함.
        assert!(ct.calls.borrow().is_empty());
        let mut req = crate::value_types::Requisition::INVALID;
        bullet.request(&mut req);
        // Picture endto end 의 width=28.0 와 동일 (위 picture test 와 동일 setup).
        assert!((req.x.natural - 28.0).abs() < 1e-3);
        ShapeEngine::_reset_for_test();
    }

    #[test]
    fn unified_dispatcher_routes_to_autonum_for_type_3() {
        ShapeEngine::_reset_for_test();
        let ct = ct_multi(&[
            (SystemFont::Helvetica, b'1' as u16, 6.0),
            (SystemFont::Helvetica, b'.' as u16, 3.0),
        ]);
        let gm = E2eGm::with(
            "Arial",
            GlobalFontMetrics { em: 1000.0, ascent: 700.0, m7: 300.0, m8: 0.0 },
        );
        let mut para = ParaProperty::new();
        para.bullet = Some(Bullet::AutoNumber { format_type: 8, start_at: 1 });
        let run = RunProperty::new(10.0).with_font_table(FontTable {
            latin: Some(Font::new("Arial")),
            ..Default::default()
        });
        let bullet = port_bullet_render(
            Some(&para),
            None,
            run,
            &Theme::new(),
            1,
            &ct,
            &gm,
        );
        assert_eq!(bullet.bullet_type, 3);
        // "1." → 2 char measurements.
        assert_eq!(ct.calls.borrow().len(), 2);
        ShapeEngine::_reset_for_test();
    }

    #[test]
    fn unified_dispatcher_empty_layout_for_none_bullet() {
        ShapeEngine::_reset_for_test();
        let ct = ct_multi(&[]);
        let gm = E2eGm::with(
            "Arial",
            GlobalFontMetrics { em: 1000.0, ascent: 700.0, m7: 300.0, m8: 0.0 },
        );
        let para = ParaProperty::new();  // no bullet
        let run = RunProperty::new(10.0);
        let bullet = port_bullet_render(
            Some(&para),
            None,
            run,
            &Theme::new(),
            5,
            &ct,
            &gm,
        );
        assert_eq!(bullet.bullet_type, 0);
        assert_eq!(bullet.numbering, 5);
        assert!(ct.calls.borrow().is_empty());
        ShapeEngine::_reset_for_test();
    }

    #[test]
    fn port_bullet_render_picture_non_picture_bullet_gives_empty_layout() {
        // bullet=Character → picture branch skip. 자식 0 개.
        let mut para = ParaProperty::new();
        para.bullet = Some(Bullet::Character {
            chars: vec![0x2022],
        });
        let bullet = port_bullet_render_picture(Some(&para), None, 1);
        // bullet_type 은 Character.get_type()=1 (raw line 749 의 GetType result 그대로).
        // 하지만 picture branch 가 active 가 아니라 layout 에 child 0개.
        assert_eq!(bullet.bullet_type, 1);
        let mut req = crate::value_types::Requisition::INVALID;
        bullet.request(&mut req);
        assert_eq!(req.x.natural, 0.0);
    }

    #[test]
    fn port_bullet_render_autonum_non_autonum_bullet_gives_empty_layout() {
        // bullet=Character → autonum_string 호출 안 됨, chars 빈 슬라이스. measurement 0회.
        ShapeEngine::_reset_for_test();
        let ct = ct_multi(&[]);
        let gm = E2eGm::with(
            "Arial",
            GlobalFontMetrics { em: 1000.0, ascent: 700.0, m7: 300.0, m8: 0.0 },
        );
        let mut para = ParaProperty::new();
        // Character bullet — not AutoNumber.
        para.bullet = Some(Bullet::Character {
            chars: vec![0x2022],
        });
        let run = RunProperty::new(10.0).with_font_table(FontTable {
            latin: Some(Font::new("Arial")),
            ..Default::default()
        });

        let bullet = port_bullet_render_autonum(
            Some(&para),
            None,
            run,
            &Theme::new(),
            1,
            &ct,
            &gm,
        );

        // bullet_type 은 Character.get_type() = 1, but autonum_string 가 안 불려 chars 빈.
        // 따라서 layout 에 child 0 개.
        assert_eq!(bullet.bullet_type, 1, "Character bullet's GetType");
        assert!(ct.calls.borrow().is_empty(), "autonum 분기 active 아님");
    }

    // ============================================================
    // ppt_compose_bullet 외곽 (0x307468) end-to-end
    // ============================================================

    use crate::composition::LRComposition;
    use crate::glyph::CharItemView;

    /// composition fixture: chars + CR with paragraph property + ParaProperty.bullet.
    ///
    /// `Composition::items` 는 `Vec<Option<Box<dyn Glyph>>>`. 본 helper 는 CharItemView 를
    /// 직접 push — wrapper 없이. 그래야 `ppt_compose_bullet` 의 `downcast_mut::<CharItemView>`
    /// 가 성공한다 (raw 의 SharePtr<Glyph> dispatch 의 Rust 등가).
    fn push_char_view(comp: &mut LRComposition, view: CharItemView) {
        comp.inner.items.push(Some(Box::new(view)));
    }

    /// 가벼운 default Theme — Latin face 가 비어 있으므로 fallback `HCR Dotum` 으로 떨어짐.
    fn default_test_theme() -> Theme {
        Theme::new()
    }

    /// 표준 mock providers — `Helvetica` advance + global metric.
    fn standard_providers() -> (E2eCt, E2eGm) {
        let ct = E2eCt::with(SystemFont::Helvetica, b'A' as u16, 5.0);
        let gm = E2eGm::with(
            "Arial",
            GlobalFontMetrics { em: 1000.0, ascent: 700.0, m7: 300.0, m8: 0.0 },
        );
        (ct, gm)
    }

    /// raw line 38-41: `!IsFirstLineOnPara → return`. composition 의 `items[line_idx]` 가 CR 이
    /// 아니면 mutation 일어나지 않는다.
    #[test]
    fn ppt_compose_bullet_skips_when_not_first_line() {
        ShapeEngine::_reset_for_test();
        // composition: ['A', CR(bullet), 'B']. line_idx=0 ('A', non-CR) → IsFirstLineOnPara=false.
        let mut para_with_bullet = ParaProperty::new();
        para_with_bullet.bullet = Some(Bullet::Character { chars: vec![b'X' as u16] });
        let run = RunProperty::new(10.0).with_font_table(FontTable {
            latin: Some(Font::new("Arial")),
            ..Default::default()
        });

        let mut comp = LRComposition::new(None, None, None, 100.0);
        push_char_view(&mut comp, CharItemView { char_code: b'A' as u16, ..Default::default() });
        push_char_view(
            &mut comp,
            CharItemView {
                char_code: 0x0d,
                para_property: Some(para_with_bullet),
                run_property: Some(run),
                theme: Some(default_test_theme()),
                ..Default::default()
            },
        );
        push_char_view(&mut comp, CharItemView { char_code: b'B' as u16, ..Default::default() });

        let (ct, gm) = standard_providers();
        ppt_compose_bullet(/*line_idx*/ 0, /*para_idx*/ 0, &[], &mut comp, &ct, &gm);

        // target_line 이 될 자리 (items[1]) 의 render_path 가 None — IsFirstLineOnPara 가드로
        // 함수 즉시 return.
        let target = comp
            .get_component(1)
            .and_then(|g| g.as_any().downcast_ref::<CharItemView>())
            .expect("CR view");
        assert!(target.render_path.is_none());
    }

    /// raw line 100-102: `Bullet.GetType() == 0 → goto cleanup`. ParaProperty.bullet=None 이면
    /// 즉시 cleanup → target.render_path mutation 없음.
    #[test]
    fn ppt_compose_bullet_skips_when_paragraph_has_no_bullet() {
        ShapeEngine::_reset_for_test();
        // composition: ['A', CR(no bullet)]. para_property 는 있지만 .bullet 가 None.
        let para_no_bullet = ParaProperty::new();
        let run = RunProperty::new(10.0).with_font_table(FontTable {
            latin: Some(Font::new("Arial")),
            ..Default::default()
        });

        let mut comp = LRComposition::new(None, None, None, 100.0);
        push_char_view(
            &mut comp,
            CharItemView {
                char_code: b'A' as u16,
                run_property: Some(run.clone()),
                theme: Some(default_test_theme()),
                ..Default::default()
            },
        );
        push_char_view(
            &mut comp,
            CharItemView {
                char_code: 0x0d,
                para_property: Some(para_no_bullet),
                run_property: Some(run),
                theme: Some(default_test_theme()),
                ..Default::default()
            },
        );

        let (ct, gm) = standard_providers();
        ppt_compose_bullet(/*line_idx*/ -1, /*para_idx*/ 0, &[], &mut comp, &ct, &gm);

        // target = items[0] ('A'). bullet=None 으로 GetType=0 → cleanup. render_path 미설정.
        let target = comp
            .get_component(0)
            .and_then(|g| g.as_any().downcast_ref::<CharItemView>())
            .expect("'A' view");
        assert!(target.render_path.is_none());
    }

    /// raw happy path: line_idx=-1, para_idx=0 으로 첫 paragraph 의 첫 CharItemView 에
    /// BulletRenderGlyph SharePtr 가 set 된다.
    #[test]
    fn ppt_compose_bullet_sets_render_path_for_first_paragraph() {
        ShapeEngine::_reset_for_test();
        // composition: ['A', CR(Character '•' bullet)]. line_idx=-1 → IsFirstLineOnPara=true.
        // para_idx=0 → first CR at idx=1 → ParaProperty.bullet=Character{'•'} → GetType=1.
        // target_line = 0 → items[0] = 'A' 의 render_path 에 BulletRenderGlyph 저장.
        let mut para = ParaProperty::new();
        para.bullet = Some(Bullet::Character { chars: vec![0x2022] });
        let run = RunProperty::new(10.0).with_font_table(FontTable {
            latin: Some(Font::new("Arial")),
            ..Default::default()
        });

        let mut comp = LRComposition::new(None, None, None, 100.0);
        // 'A' has run_property + theme (target needs both for FUN_002eaf54 entry).
        push_char_view(
            &mut comp,
            CharItemView {
                char_code: b'A' as u16,
                run_property: Some(run.clone()),
                theme: Some(default_test_theme()),
                ..Default::default()
            },
        );
        // CR has para_property + run_property (CR's run is used as fallback in some flows; here
        // the CR carries the paragraph's ParaProperty).
        push_char_view(
            &mut comp,
            CharItemView {
                char_code: 0x0d,
                para_property: Some(para),
                run_property: Some(run),
                theme: Some(default_test_theme()),
                ..Default::default()
            },
        );

        // Character bullet '•' takes the StHeitiTcMedium lane (raw select_system_font CJK path,
        // `(c & 0xe000) != 0`), so the provider must serve that pair.
        let ct = E2eCt::with(SystemFont::StHeitiTcMedium, 0x2022, 4.0);
        let gm = E2eGm::with(
            "Arial",
            GlobalFontMetrics { em: 1000.0, ascent: 700.0, m7: 300.0, m8: 0.0 },
        );

        ppt_compose_bullet(/*line_idx*/ -1, /*para_idx*/ 0, &[], &mut comp, &ct, &gm);

        let target = comp
            .get_component(0)
            .and_then(|g| g.as_any().downcast_ref::<CharItemView>())
            .expect("'A' view");
        assert!(target.render_path.is_some(), "render_path 가 set 되어야 한다");

        // 추가 검증: render_path 가 실제로 BulletRenderGlyph 이고 numbering 기본값 1.
        let render = target.render_path.as_ref().unwrap();
        let br = render
            .as_any()
            .downcast_ref::<BulletRenderGlyph>()
            .expect("BulletRenderGlyph");
        assert_eq!(br.bullet_type, 1, "Character bullet");
        assert_eq!(br.numbering, 1, "no entry in numbering_table → default 1");
        ShapeEngine::_reset_for_test();
    }

    /// raw line 160-172: numbering vector 의 마지막 매칭 entry 의 u32 가 numbering offset.
    /// AutoNumber bullet 일 때 numbering 이 `port_bullet_render` 의 `numbering` 으로 전달되어
    /// `BulletRenderGlyph.numbering` 에 저장 + autonum_string(start_at + numbering - 1).
    #[test]
    fn ppt_compose_bullet_uses_numbering_table_for_autonum() {
        ShapeEngine::_reset_for_test();
        // AutoNumber bullet (format_type=8 → "%d.", start_at=1).
        let mut para = ParaProperty::new();
        para.bullet = Some(Bullet::AutoNumber { format_type: 8, start_at: 1 });
        let run = RunProperty::new(10.0).with_font_table(FontTable {
            latin: Some(Font::new("Arial")),
            ..Default::default()
        });

        let mut comp = LRComposition::new(None, None, None, 100.0);
        push_char_view(
            &mut comp,
            CharItemView {
                char_code: b'A' as u16,
                run_property: Some(run.clone()),
                theme: Some(default_test_theme()),
                ..Default::default()
            },
        );
        push_char_view(
            &mut comp,
            CharItemView {
                char_code: 0x0d,
                para_property: Some(para),
                run_property: Some(run),
                theme: Some(default_test_theme()),
                ..Default::default()
            },
        );

        // numbering_table 에 CR view 의 pointer 키 + numbering=5 등록.
        //   key 는 ppt_compose_bullet 내부에서 `find_para_cr_view` 가 동일한 raw ptr 로
        //   파생하므로 동일.
        let cr_key = {
            let cr = comp
                .get_component(1)
                .and_then(|g| g.as_any().downcast_ref::<CharItemView>())
                .expect("CR view");
            cr as *const CharItemView as usize
        };
        let table = vec![crate::compositor::NumberingEntry {
            key: cr_key,
            number: 5,
            is_short_line: false,
            level: None,
            bullet_start: None,
        }];

        // AutoNumber 의 measurement: autonum_string(8, 1 + 5 - 1) = autonum_string(8, 5) = "5."
        //   chars: '5' (0x35), '.' (0x2e). 둘 다 Helvetica lane (Latin).
        let ct = ct_multi(&[
            (SystemFont::Helvetica, b'5' as u16, 6.0),
            (SystemFont::Helvetica, b'.' as u16, 2.0),
        ]);
        let gm = E2eGm::with(
            "Arial",
            GlobalFontMetrics { em: 1000.0, ascent: 700.0, m7: 300.0, m8: 0.0 },
        );

        ppt_compose_bullet(-1, 0, &table, &mut comp, &ct, &gm);

        let target = comp
            .get_component(0)
            .and_then(|g| g.as_any().downcast_ref::<CharItemView>())
            .expect("'A' view");
        let render = target.render_path.as_ref().expect("render_path set");
        let br = render
            .as_any()
            .downcast_ref::<BulletRenderGlyph>()
            .expect("BulletRenderGlyph");
        assert_eq!(br.bullet_type, 3, "AutoNumber bullet");
        assert_eq!(br.numbering, 5, "numbering offset from table");

        // 측정 호출 확인: '5' + '.' 두 번.
        let calls = ct.calls.borrow();
        assert_eq!(calls.len(), 2, "measured '5' and '.'");
        assert_eq!(calls[0].1, b'5' as u16);
        assert_eq!(calls[1].1, b'.' as u16);
        ShapeEngine::_reset_for_test();
    }

    /// raw line 165-172: 매칭 entry 없으면 default 1. 같은 `lookup_starting_numbering` 의 단위
    /// 검증.
    #[test]
    fn lookup_starting_numbering_falls_back_to_one_when_no_match() {
        let mk = |key: usize, number: u32| crate::compositor::NumberingEntry {
            key,
            number,
            is_short_line: false,
            level: None,
            bullet_start: None,
        };
        let table = vec![mk(0x123, 7)];
        // matching key
        assert_eq!(lookup_starting_numbering(&table, Some(0x123)), 7);
        // non-matching key
        assert_eq!(lookup_starting_numbering(&table, Some(0x456)), 1);
        // no key (no CR found)
        assert_eq!(lookup_starting_numbering(&table, None), 1);
        // empty table
        assert_eq!(lookup_starting_numbering(&[], Some(0x123)), 1);
    }

    /// raw line 165-172 의 **end→begin 방향** walk — 가장 최근 (vector 끝쪽) entry 가 우선.
    /// Rust: `iter().rev().find` 등가.
    #[test]
    fn lookup_starting_numbering_uses_last_match_when_duplicates() {
        let mk = |number: u32| crate::compositor::NumberingEntry {
            key: 0xAA,
            number,
            is_short_line: false,
            level: None,
            bullet_start: None,
        };
        let table = vec![mk(1), mk(2), mk(3)];
        // raw 의 backward walk 는 첫 match (= vector 마지막) 채택.
        assert_eq!(lookup_starting_numbering(&table, Some(0xAA)), 3);
    }

    /// raw line 103-158: target = GetFirstCharItemViewOnPara(line_idx + 1) 가 없으면 (line_idx
    /// + 1 이 composition 범위 초과) 즉시 cleanup → mutation 없음.
    #[test]
    fn ppt_compose_bullet_returns_when_no_target_view() {
        ShapeEngine::_reset_for_test();
        let mut para = ParaProperty::new();
        para.bullet = Some(Bullet::Character { chars: vec![0x2022] });
        let run = RunProperty::new(10.0).with_font_table(FontTable {
            latin: Some(Font::new("Arial")),
            ..Default::default()
        });

        // composition: only CR. line_idx = 0 (CR), target_line = 1, but count = 1.
        let mut comp = LRComposition::new(None, None, None, 100.0);
        push_char_view(
            &mut comp,
            CharItemView {
                char_code: 0x0d,
                para_property: Some(para),
                run_property: Some(run),
                theme: Some(default_test_theme()),
                ..Default::default()
            },
        );

        let ct = E2eCt::with(SystemFont::StHeitiTcMedium, 0x2022, 4.0);
        let gm = E2eGm::with(
            "Arial",
            GlobalFontMetrics { em: 1000.0, ascent: 700.0, m7: 300.0, m8: 0.0 },
        );

        // target_line=1 이 범위 초과 — 함수는 cleanup goto.
        ppt_compose_bullet(0, 0, &[], &mut comp, &ct, &gm);

        let cr = comp
            .get_component(0)
            .and_then(|g| g.as_any().downcast_ref::<CharItemView>())
            .expect("CR view");
        assert!(cr.render_path.is_none(), "CR 자체에는 render_path set 안 함");
        ShapeEngine::_reset_for_test();
    }

    // ============================================================
    // 차이 #3 end-to-end 통합 검증 — producer (ComposeNumbering) →
    //   consumer (ComposeBullet) 의 키 identity 가 일치하는지 단위로 증명.
    // ============================================================

    /// ComposeNumbering 이 push 한 entry 의 key 와 ComposeBullet 이 lookup 하는 key 가
    /// **같은 composition 의 같은 CR CharItemView ptr** 라는 사실을 직접 증명.
    ///
    /// 이전 schema (`view: Box<CharItemView>` clone) 였다면 producer 의 키 = clone 주소,
    /// consumer 의 키 = composition 내부 주소 → 영원히 mismatch → 모든 AutoNumber 가
    /// default 1. 새 schema (key: usize = composition 내부 ptr cast) 는 일치 → AutoNumber 가
    /// producer 가 계산한 number 그대로 반영.
    #[test]
    fn integration_producer_consumer_share_pointer_key() {
        ShapeEngine::_reset_for_test();
        // composition: ['A', CR(AutoNumber bullet)]. ComposeNumbering 이 CR 의 key 로 entry
        //   push, ComposeBullet 이 같은 key 로 lookup.
        let mut para = ParaProperty::new();
        para.bullet = Some(Bullet::AutoNumber { format_type: 8, start_at: 1 });
        let run = RunProperty::new(10.0).with_font_table(FontTable {
            latin: Some(Font::new("Arial")),
            ..Default::default()
        });

        let mut comp = LRComposition::new(None, None, None, 100.0);
        push_char_view(
            &mut comp,
            CharItemView {
                char_code: b'A' as u16,
                run_property: Some(run.clone()),
                theme: Some(default_test_theme()),
                ..Default::default()
            },
        );
        push_char_view(
            &mut comp,
            CharItemView {
                char_code: 0x0d,
                para_property: Some(para),
                run_property: Some(run),
                theme: Some(default_test_theme()),
                ..Default::default()
            },
        );

        // Phase A: ComposeNumbering producer.
        //   from=-1 → IsFirstLineOnPara=true. to=1 → CR at idx=1.
        //   numbering 이 비어 있고 새 para 의 (level=0, start_at=1) → uVar11 = 1.
        //   to-from = 2 >= 2 → !is_short_line.
        let mut numbering: Vec<crate::compositor::NumberingEntry> = Vec::new();
        crate::ppt_compose_numbering::ppt_compose_numbering(-1, 1, &comp, &mut numbering);
        assert_eq!(numbering.len(), 1, "1개 entry push");
        assert_eq!(numbering[0].number, 1);
        // producer 가 derive 한 key = composition 의 idx=1 (CR) ptr cast.
        let expected_key = comp
            .get_component(1)
            .and_then(|g| g.as_any().downcast_ref::<CharItemView>())
            .unwrap() as *const CharItemView as usize;
        assert_eq!(numbering[0].key, expected_key, "producer key = composition 내부 CR ptr");

        // 2번째 paragraph 추가 — CR 1개 더, 같은 (level/start). ComposeNumbering 다시 호출하면
        //   prev entry 에서 이어받기 → number = 2.
        // ⚠️ run_property 에 FontTable 가 있어야 ComposeBullet 의 from_ctor_context 가
        //   realize_font 에서 face name 을 얻고 measurement 단계까지 진행. 없으면 모든 children
        //   이 zero-metric 으로 반환되어 ct.calls 가 0회.
        let run2 = RunProperty::new(10.0).with_font_table(FontTable {
            latin: Some(Font::new("Arial")),
            ..Default::default()
        });
        push_char_view(
            &mut comp,
            CharItemView {
                char_code: b'B' as u16,
                run_property: Some(run2.clone()),
                theme: Some(default_test_theme()),
                ..Default::default()
            },
        );
        let mut para2 = ParaProperty::new();
        para2.bullet = Some(Bullet::AutoNumber { format_type: 8, start_at: 1 });
        push_char_view(
            &mut comp,
            CharItemView {
                char_code: 0x0d,
                para_property: Some(para2),
                run_property: Some(run2),
                theme: Some(default_test_theme()),
                ..Default::default()
            },
        );
        // from=1 (직전 CR) → IsFirstLineOnPara=true. to=3 → CR at idx=3. to-from=2 ≥ 2.
        crate::ppt_compose_numbering::ppt_compose_numbering(1, 3, &comp, &mut numbering);
        assert_eq!(numbering.len(), 2);
        assert_eq!(numbering[1].number, 2, "이어받기 + (!is_short_line) → +1");
        let cr2_key = comp
            .get_component(3)
            .and_then(|g| g.as_any().downcast_ref::<CharItemView>())
            .unwrap() as *const CharItemView as usize;
        assert_eq!(numbering[1].key, cr2_key);

        // Phase B: ComposeBullet consumer for 2번째 paragraph.
        //   line_idx=1 (CR ends para 1) → IsFirstLineOnPara(1)=true. para_idx=2 → CR at idx=3.
        //   target_line = line_idx + 1 = 2 → 'B'.
        //   numbering_table 에서 cr2_key 매치 → starting_numbering = 2.
        //   port_bullet_render(numbering=2) → autonum_string(8, 1+2-1=2) = "2." → '2', '.'.
        let ct = ct_multi(&[
            (SystemFont::Helvetica, b'2' as u16, 6.0),
            (SystemFont::Helvetica, b'.' as u16, 2.0),
        ]);
        let gm = E2eGm::with(
            "Arial",
            GlobalFontMetrics { em: 1000.0, ascent: 700.0, m7: 300.0, m8: 0.0 },
        );

        ppt_compose_bullet(1, 2, &numbering, &mut comp, &ct, &gm);

        let target = comp
            .get_component(2)
            .and_then(|g| g.as_any().downcast_ref::<CharItemView>())
            .expect("'B' view");
        let render = target.render_path.as_ref().expect("render_path set");
        let br = render
            .as_any()
            .downcast_ref::<BulletRenderGlyph>()
            .expect("BulletRenderGlyph");
        assert_eq!(br.bullet_type, 3, "AutoNumber");
        // ⭐ 핵심 증명: producer 의 number=2 가 consumer 에 전달되어 BulletRenderGlyph.numbering
        //   에 저장. 만약 키 mismatch 였다면 default 1 로 떨어졌을 것.
        assert_eq!(br.numbering, 2, "producer→consumer 키 일치 = numbering 전달 성공");

        // 측정 호출: '2' + '.' 두 번. 만약 키 mismatch → numbering=1 → autonum_string(8, 1) =
        //   "1." → '1', '.' 호출 (assert_eq!(calls[0].1, b'2') 실패).
        let calls = ct.calls.borrow();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].1, b'2' as u16, "키 매치 → numbering=2 → '2.' 측정");
        assert_eq!(calls[1].1, b'.' as u16);
        ShapeEngine::_reset_for_test();
    }

    /// 첫 paragraph (numbering vector 비어 있음) → ComposeBullet 의 lookup 이 default 1.
    /// ComposeNumbering 이 push 도 안 한 상태에서도 정상 동작 검증.
    #[test]
    fn integration_first_paragraph_no_numbering_entries_uses_default_one() {
        ShapeEngine::_reset_for_test();
        let mut para = ParaProperty::new();
        para.bullet = Some(Bullet::AutoNumber { format_type: 8, start_at: 1 });
        let run = RunProperty::new(10.0).with_font_table(FontTable {
            latin: Some(Font::new("Arial")),
            ..Default::default()
        });

        let mut comp = LRComposition::new(None, None, None, 100.0);
        push_char_view(
            &mut comp,
            CharItemView {
                char_code: b'A' as u16,
                run_property: Some(run.clone()),
                theme: Some(default_test_theme()),
                ..Default::default()
            },
        );
        push_char_view(
            &mut comp,
            CharItemView {
                char_code: 0x0d,
                para_property: Some(para),
                run_property: Some(run),
                theme: Some(default_test_theme()),
                ..Default::default()
            },
        );

        // ComposeBullet 만 호출 (numbering vector 비어 있음).
        let ct = ct_multi(&[
            (SystemFont::Helvetica, b'1' as u16, 6.0),
            (SystemFont::Helvetica, b'.' as u16, 2.0),
        ]);
        let gm = E2eGm::with(
            "Arial",
            GlobalFontMetrics { em: 1000.0, ascent: 700.0, m7: 300.0, m8: 0.0 },
        );

        ppt_compose_bullet(-1, 0, &[], &mut comp, &ct, &gm);

        let target = comp
            .get_component(0)
            .and_then(|g| g.as_any().downcast_ref::<CharItemView>())
            .expect("'A' view");
        let render = target.render_path.as_ref().expect("render_path set");
        let br = render
            .as_any()
            .downcast_ref::<BulletRenderGlyph>()
            .expect("BulletRenderGlyph");
        assert_eq!(br.numbering, 1, "비어 있는 table → lookup default 1");
        ShapeEngine::_reset_for_test();
    }
}

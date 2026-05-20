//! `Hnc::Shape::Text` 의 paragraph/body-level property 객체 — `PptCompositor` 의
//! `ComposeNumbering` / `ComposeBullet` / `ComposeBreak` 가 의존하는 subsystem.
//!
//! ## RE 출처
//!
//! - `ParaProperty::ParaProperty` ctor (`0x31abf8`, 740B) — field layout 확정.
//! - `ParaProperty::GetBullet` (`0x2ec8c8`) / `::Contains` (`0x2ec8f0`) / `::GetLevel` (`0x3073fc`).
//! - `Bullet` 계열 RTTI + `GetType()` (object-vptr `+0x30` vfunc):
//!   - base `Bullet` (vtable `PTR_FUN_0077fdc0`, 8B) — `GetType()` = `0` (`FUN_002e696c`).
//!   - `CharacterBullet` (vtable `DAT_0077fe40`, 0x10B) — `GetType()` = `1` (`FUN_002e6df4`).
//!   - `PictureBullet` (vtable `0x77fea0`) — `GetType()` = `2` (`0x2e7904`).
//!   - `AutoNumberBull` (vtable `PTR_FUN_0077ff20`, 0x10B) — `GetType()` = `3` (`FUN_002e8350`).
//!     `+0x08` = numbering format type, `+0x0c` = startAt (`TextConverterUtil::ToAutoNumberBullet`
//!     `0x34f52c`: `*(puVar3+1) = type; *(puVar3+0xc) = startAt`).
//! - `BodyProperty::GetVert` (`0x2d2c6c`) — `get_uint(key 0x89e)`.
//!
//! ## 모델링 정책
//!
//! 한컴 원본은 SharePtr<RunProperty>/<Bullet>/<TextFont> + embedded `Hnc::Property::PropertyBag`
//! (red-black tree, libHncFoundation). `properties.rs` 가 이미 `PropertyBag` 를 semantic
//! trait 로 추상화 — 본 모듈은 그 위에서 `ParaProperty`/`BodyProperty`/`Bullet` 를
//! **semantic** 하게 모델 (byte layout 이 아닌 의미 보존). RunProperty/TextFont 등
//! numbering/bullet/break 가 쓰지 않는 필드는 생략.

use crate::properties::{HashMapPropertyBag, PropertyBag, PropertyKey, PropertyValue};

// ============================================================
// Bullet — paragraph 의 글머리표/번호
// ============================================================

/// `Hnc::Shape::Render::ImageSource` — image data wrapper (`FUN_002eaf54` picture branch).
///
/// raw `Hnc::Shape::Render::ImageSource::GetImageSize()` 가 returns 8-byte `{ f32 width,
/// f32 height }` (raw line 856 의 `Hnc::Shape::Render::ImageSource::GetImageSize()` 가
/// `local_150` 의 두 f32 슬롯을 채움). 본 모델은 layout 영향분만 보존 — image data /
/// codec / pixel buffer 는 render-only 라 생략.
///
/// **units**: raw 의 GetImageSize 가 반환하는 f32 는 image 의 "natural size" — 실측 결과
/// 96 DPI 기준 pixel count (raw `image_size * dpi / 96.0` 환산이 96 base 인 점에서 추론).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImageSource {
    /// raw `local_150._0_4_` (`(float)local_150`) — image width @ 96dpi.
    pub width_units: f32,
    /// raw `local_150._4_4_` — image height @ 96dpi.
    pub height_units: f32,
}

/// `Hnc::Shape::ImageBrush` — image bullet 의 brush wrapper.
///
/// raw `ImageBrush` 는 ImageSource SharePtr + PropertyBag + (color override 등 render-only)
/// 필드를 가진 객체. `FUN_002eaf54` picture branch 는 오직 `ImageBrush::GetImageSource()` 만
/// 사용 — 그 외 필드 (key 0x269 = tile_style 등) 는 모두 render-only.
///
/// 본 모델은 layout 영향분만 모델: `Option<ImageSource>` 단일 필드. `None` 이면 raw line 852
/// (`local_a8 == null || *local_a8 == 0`) 의 "image source 없음" path 와 동등 → BlipGlyph 의
/// width/height 모두 0 으로 산출.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ImageBrush {
    /// raw `ImageBrush::GetImageSource()` 반환. SharePtr 가 비어 있거나 obj 가 null 이면
    /// `None`.
    pub source: Option<ImageSource>,
}

impl ImageBrush {
    pub fn new(source: ImageSource) -> Self {
        Self { source: Some(source) }
    }

    pub fn empty() -> Self {
        Self { source: None }
    }
}

/// `Hnc::Shape::Text::Bullet` 계열. `Bullet::GetType()` (object-vptr `+0x30` vfunc) 의
/// 반환값으로 4종 구분: base=0, Character=1, Picture=2, AutoNumber=3 (RE 검증).
///
/// `PptCompositor::ComposeNumbering` 은 `GetType() == 3` (AutoNumber) 일 때만
/// `dynamic_cast<AutoNumberBull*>` 후 `+0xc` (startAt) 를 읽는다.
/// `FUN_002eaf54` 의 CharacterBullet 경로는 `+0x08` 의 `CHncStringW` 길이와 첫
/// UTF-16 code unit 을 읽어 bullet `CharItemView` 하나를 만든다.
#[derive(Debug, Clone, PartialEq)]
pub enum Bullet {
    /// base `Bullet` (vtable `PTR_FUN_0077fdc0`) — `GetType()` = 0. 글머리표 없음.
    None,
    /// `CharacterBullet` (vtable `DAT_0077fe40`) — `GetType()` = 1. 문자 글머리표.
    ///
    /// raw layout: 0x10B object, `+0x08` = `CHncStringW`. `TextConverterUtil::
    /// ToCharacterBullet` (`0x34f5a0`) copies OOXML `CTextCharBullet::Getchar()` here.
    /// `FUN_002eaf54` checks `len > 0`, then reads only `chars[0]`.
    Character {
        /// UTF-16 code units stored in the raw `CHncStringW`.
        chars: Vec<u16>,
    },
    /// `PictureBullet` (vtable `0x77fea0`) — `GetType()` = 2. 그림 글머리표.
    ///
    /// raw layout: `+0x08` = `SharePtr<ImageBrush>` (`bullet_render_deps.txt` ASM
    /// `002ee8b0 ldr x20,[x19, #0x8]`). `FUN_002eaf54` 의 picture branch (raw
    /// `ppt_subsystem_deps.txt:798-941`) 가 `ImageBrush.GetImageSource()` 호출.
    Picture {
        /// `+0x08` SharePtr<ImageBrush>. raw line 801: `*(RunProperty **)(lVar18 + 8)`.
        brush: ImageBrush,
    },
    /// `AutoNumberBull` (vtable `PTR_FUN_0077ff20`) — `GetType()` = 3. 자동 번호.
    /// raw layout: `+0x08` = `format_type`, `+0x0c` = `start_at`.
    AutoNumber {
        /// `+0x08` — numbering format type (OOXML autonumber type → `DAT_007475a4` 매핑값).
        format_type: i32,
        /// `+0x0c` — `GetstartAt()` (시작 번호). `ComposeNumbering` 의 `local_88`/`local_84`
        /// continuity 비교 대상.
        start_at: i32,
    },
}

impl Bullet {
    /// `Bullet::GetType()` — object-vptr `+0x30` vfunc. base=0/Character=1/Picture=2/AutoNumber=3.
    pub fn get_type(&self) -> i32 {
        match self {
            Bullet::None => 0,
            Bullet::Character { .. } => 1,
            Bullet::Picture { .. } => 2,
            Bullet::AutoNumber { .. } => 3,
        }
    }

    /// `FUN_002eaf54` character bullet path:
    /// ```c
    /// if (0 < *(int *)(*(ushort **)(bullet + 8) + -4)) {
    ///   uVar1 = **(ushort **)(bullet + 8);
    ///   CharItemView::CharItemView(..., (uint)uVar1, ...);
    /// }
    /// ```
    pub fn first_character_code_unit(&self) -> Option<u16> {
        match self {
            Bullet::Character { chars } => chars.first().copied(),
            _ => None,
        }
    }
}

// ============================================================
// ParaProperty
// ============================================================

/// `Hnc::Shape::Text::ParaProperty` — paragraph-level 속성.
///
/// raw layout (ctor `0x31abf8`):
/// ```text
/// +0x00  SharePtr<RunProperty>   (default run property) — numbering/bullet 미사용, 생략
/// +0x08  SharePtr<Bullet>        → `bullet`
/// +0x10  SharePtr<TextFont>      → `text_font` (12번째 세션 추가 — bullet ctor 사용)
/// +0x18  Hnc::Property::PropertyBag (embedded) → `property_bag`
/// ```
/// ctor 는 `+0x18` PropertyBag 에 기본 키 (0x8fc/0x8fd/0x8ff/0x900/0x901/0x903..0x906) 를
/// 채운다. key `0x902` (level) 는 `ApplyProperty` 등에서 별도 설정.
#[derive(Debug, Clone, Default)]
pub struct ParaProperty {
    /// `+0x08` — `SharePtr<Bullet>`. `ParaProperty::GetBullet` (`0x2ec8c8`) = `*(this+8)`.
    pub bullet: Option<Bullet>,
    /// `+0x10` — `SharePtr<TextFont>` (paragraph 의 default 텍스트 폰트).
    ///
    /// raw `FUN_002eaf54` step 6 (`0x2eaf54+0x6cc..` decompile line 622-679) 이 이 슬롯의
    /// `+0x18` PropertyBag 의 key `0x96a` (font size) 를 **mutate** 한다 — bullet 의 size
    /// 변환 결과로 paragraph 의 default font size 를 갱신.
    pub text_font: Option<TextFont>,
    /// `+0x18` — embedded `Hnc::Property::PropertyBag`. `ParaProperty::Contains` (`0x2ec8f0`)
    /// = `PropertyBag::Contains(this+0x18, key)`.
    pub property_bag: HashMapPropertyBag,
}

impl ParaProperty {
    pub fn new() -> Self {
        Self::default()
    }

    /// `ParaProperty::Contains(PropertyKey const&)` (`0x2ec8f0`) — `+0x18` PropertyBag 위임.
    pub fn contains(&self, key: PropertyKey) -> bool {
        self.property_bag.contains(key)
    }

    /// `ParaProperty::GetLevel()` (`0x3073fc`) — `FUN_006671e0(*(this+0x18 deref), key 0x902)`
    /// 의 `*(int*)` 결과. key `0x902` 의 int 값. (raw 는 key 없으면 `out_of_range` throw —
    /// 본 모델은 호출 측이 `contains` 로 가드한다는 invariant 하에 `None` → `0`.)
    pub fn get_level(&self) -> i32 {
        self.property_bag
            .get_int(PropertyKey::new(KEY_LEVEL))
            .unwrap_or(0)
    }

    /// `ParaProperty::GetBullet()` (`0x2ec8c8`) — `*(this+0x08)`.
    pub fn get_bullet(&self) -> Option<&Bullet> {
        self.bullet.as_ref()
    }

    /// `ParaProperty::GetBulletSize()` (`0x2ec964`, sz=88) — key `0x90f`.
    ///
    /// raw decompile:
    /// ```text
    /// FUN_006805d0(propBag, &{key=0x90f, extra=0}) -> long*
    /// // 반환 8-byte: piVar12[0] = mode (i32), (float)piVar12[1] = factor (f32).
    /// ```
    /// `mode == 1` → factor 는 absolute pt size. else → factor 는 TextFont 의 0x96a 에
    /// 대한 곱셈 비율.
    pub fn get_bullet_size(&self) -> Option<(i32, f32)> {
        self.property_bag.get_int_float(PropertyKey::new(KEY_BULLET_SIZE))
    }
}

// ============================================================
// TextFont — paragraph 의 default 텍스트 폰트
// ============================================================

/// `Hnc::Shape::Text::TextFont` — paragraph 의 default 텍스트 폰트 (RunProperty 와 별개).
///
/// raw layout: `+0x18` 에 embedded `Hnc::Property::PropertyBag`. semantic 한 wrap 만 모델.
///
/// **사용처**: `FUN_002eaf54` (bullet ctor) 가 `*(ParaProperty+0x10)` 로 받아서:
/// - read key `0x96a` (font size, raw 값 — `RunProperty::get_font_size` 같은 클램프 없음)
/// - write key `0x96a` (bullet size 변환 결과로 mutate)
///
/// `RunProperty.bag` 와 같은 key (0x96a) 를 쓰지만 별도 객체.
#[derive(Debug, Clone, Default)]
pub struct TextFont {
    /// raw `+0x18` — embedded `Hnc::Property::PropertyBag`.
    pub property_bag: HashMapPropertyBag,
}

impl TextFont {
    pub fn new() -> Self {
        Self::default()
    }

    /// raw `FUN_0065616c(textfont_bag, &{key=0x96a, extra=0})` — float 값을 반환.
    ///
    /// 한컴 원본은 fallback 없음 (key 없으면 `*pfVar14` 가 어떤 값이든 그대로 반환).
    /// 본 포트는 키 없으면 `0.0` 반환 — `FUN_002eaf54` 가 그 다음에 `if (val <= 0.0) val = 10.0`
    /// 으로 clamp 하므로 byte-equivalent.
    pub fn get_font_size_raw(&self) -> f32 {
        self.property_bag
            .get_float(PropertyKey::new(KEY_TEXTFONT_FONT_SIZE))
            .unwrap_or(0.0)
    }

    /// raw `FUN_00653cb4(textfont_bag, &{key=0x96a, extra=0}, &value, 1)` — float `set`.
    pub fn set_font_size(&mut self, value: f32) {
        self.property_bag.insert(
            PropertyKey::new(KEY_TEXTFONT_FONT_SIZE),
            PropertyValue::Float(value),
        );
    }
}

// ============================================================
// BodyProperty
// ============================================================

/// `Hnc::Shape::Text::BodyProperty` — body-level 속성.
///
/// `PptCompositor::ComposeBreak` 가 `BodyProperty::GetVert` (`0x2d2c6c`) 만 사용:
/// `FUN_0067d0e4(*(this deref), key 0x89e)` 의 `*(uint*)` 결과. raw layout 의 `+0x00` 이
/// PropertyBag 인 구조 — semantic 하게 `property_bag` 한 필드로 모델.
#[derive(Debug, Clone, Default)]
pub struct BodyProperty {
    /// raw `+0x00` — embedded `Hnc::Property::PropertyBag`.
    pub property_bag: HashMapPropertyBag,
}

impl BodyProperty {
    pub fn new() -> Self {
        Self::default()
    }

    /// `BodyProperty::GetVert()` (`0x2d2c6c`) — `get_uint(key 0x89e)`. raw 는 key 없으면
    /// throw — 본 모델은 `None` → `0`.
    pub fn get_vert(&self) -> u32 {
        self.property_bag
            .get_uint(PropertyKey::new(KEY_VERT))
            .unwrap_or(0)
    }
}

// ============================================================
// PropertyKey 상수 (Ppt subsystem 전용)
// ============================================================

/// `0x902` — paragraph numbering level. `ParaProperty::GetLevel` / `ComposeNumbering` 의
/// `iVar3`/`iVar4`.
pub const KEY_LEVEL: u32 = 0x902;

/// `0x89e` — body vertical type. `BodyProperty::GetVert`.
pub const KEY_VERT: u32 = 0x89e;

/// `0x90f` — bullet size modifier (`ParaProperty::GetBulletSize`). 8-byte payload
/// `{ i32 mode, f32 factor }`. `FUN_002eaf54` step 6 가 사용.
pub const KEY_BULLET_SIZE: u32 = 0x90f;

/// `0x90e` — bullet color override (`ParaProperty::GetBulletColor`). raw `FUN_002eaf54`
/// step 5 가 RunProperty 의 key `0x259` (SolidBrush color) / `0x25b` (HatchBrush color) 에
/// 복사 — **render-only** (`SolidBrush::Draw`/`SolidBrush::GetColor` 등), layout 의 Request/
/// Allocate 경로엔 영향 없음. dump: `/tmp/hft_scripts/bcompositor/key_consumers.txt`.
pub const KEY_BULLET_COLOR: u32 = 0x90e;

/// `0x96a` — TextFont font size (paragraph default).
pub const KEY_TEXTFONT_FONT_SIZE: u32 = 0x96a;

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::properties::PropertyValue;

    #[test]
    fn bullet_get_type_discriminants() {
        // RE 검증: base=0, Character=1, Picture=2, AutoNumber=3.
        assert_eq!(Bullet::None.get_type(), 0);
        assert_eq!(Bullet::Character { chars: vec![0x2022] }.get_type(), 1);
        assert_eq!(Bullet::Picture { brush: ImageBrush::empty() }.get_type(), 2);
        assert_eq!(
            Bullet::AutoNumber { format_type: 0, start_at: 1 }.get_type(),
            3
        );
    }

    #[test]
    fn para_property_get_level_from_bag() {
        let mut pp = ParaProperty::new();
        // key 0x902 가 없으면 get_level → 0.
        assert_eq!(pp.get_level(), 0);
        assert!(!pp.contains(PropertyKey::new(KEY_LEVEL)));
        // key 0x902 = 2 설정.
        pp.property_bag
            .insert(PropertyKey::new(KEY_LEVEL), PropertyValue::Int(2));
        assert!(pp.contains(PropertyKey::new(KEY_LEVEL)));
        assert_eq!(pp.get_level(), 2);
    }

    #[test]
    fn para_property_get_bullet() {
        let mut pp = ParaProperty::new();
        assert!(pp.get_bullet().is_none());
        pp.bullet = Some(Bullet::AutoNumber { format_type: 5, start_at: 1 });
        assert_eq!(
            pp.get_bullet(),
            Some(&Bullet::AutoNumber { format_type: 5, start_at: 1 })
        );
    }

    #[test]
    fn character_bullet_first_code_unit() {
        let bullet = Bullet::Character { chars: vec![0x2022, 0x25e6] };
        assert_eq!(bullet.first_character_code_unit(), Some(0x2022));
        let empty = Bullet::Character { chars: vec![] };
        assert_eq!(empty.first_character_code_unit(), None);
    }

    #[test]
    fn body_property_get_vert() {
        let mut bp = BodyProperty::new();
        assert_eq!(bp.get_vert(), 0);
        bp.property_bag
            .insert(PropertyKey::new(KEY_VERT), PropertyValue::Uint(4));
        assert_eq!(bp.get_vert(), 4);
    }
}

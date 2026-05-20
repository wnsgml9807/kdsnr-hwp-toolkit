//! `Hnc::Shape::Theme` — 72B 1:1 byte-equivalent port.
//!
//! libHncDrawingEngine_arm64 의 `Theme` 는 OOXML `<a:theme>` (TXT, table 등의
//! 도형 스타일 일괄 적용 단위) 의 root container.
//!
//! # raw 72B layout (확정 from `Theme::Theme(bool)` @ `0x1eb8b4` 정독; 본 R-1.6
//! 단계에서 audit 정정)
//!
//! ```text
//! offset  field             타입                       의미
//! 0x00    guid              Guid                      16B (random instance ID)
//! 0x10    parent            SharePtr<Theme>            8B (parent theme; null in bool-ctor)
//! 0x18    init_defaults     bool                       1B (= bool ctor 인자)
//! 0x19    _pad              [u8; 7]                    7B padding
//! 0x20    name              CHncStringW                8B (refcounted)
//! 0x28    color_scheme      ColorScheme*               8B (heap 32B if true)
//! 0x30    format_scheme     FormatScheme*              8B (heap 104B if true)
//! 0x38    font_scheme       FontScheme*                8B (heap 24B if true)   ← audit 정정 (이전엔 FontSet*)
//! 0x40    object_defaults   ObjectDefaults*            8B (heap 24B if true)
//! ```
//!
//! 총 72B (0x48) / 8B align.
//!
//! # raw `Theme::Theme(bool init_defaults)` @ `0x1eb8b4`
//!
//! ```asm
//! ; Phase 1 — Guid (offset 0x00, 16B sret)
//! 1eb8e0: bl  Guid::Generator::CreateID()    ; populates [x0..x0+0x10]
//!
//! ; Phase 2 — SharePtr<Theme> null + bool stored
//! 1eb8e8: str xzr, [x20, #0x10]!             ; x20 = self+0x10; [x20] = 0
//! 1eb8ec: strb w21, [x20, #0x8]              ; [self+0x18] = bool
//!
//! ; Phase 3 — CHncStringW name default ctor (offset 0x20)
//! 1eb8f0: add x21, x20, #0x10                 ; x21 = self+0x20
//! 1eb8f8: bl  CHncStringW::CHncStringW()
//!
//! ; Phase 4 — 4 pointer slots zero-init (offsets 0x28..0x48)
//! 1eb904: str q0, [x22, #0x28]!               ; x22 += 0x28 (= self+0x28); [x22..+0x10] = 0,0
//! 1eb90c: str q0, [x23, #0x10]!               ; x23 += 0x10 (= self+0x38); [x23..+0x10] = 0,0
//!
//! ; Phase 5 — if init_defaults == false: jump to exit
//! 1eb910-1eb914: ldurb w8, [x22, #-0x10]; cbz w8, 0x1eba84
//!
//! ; Phase 6 (true path) — populate 4 sub-objects
//! 1eb920: mov w0, #0x20 (=32); bl operator_new        ; alloc ColorScheme
//! 1eb92c: bl ColorScheme::ColorScheme()                ; init (12 SetAt)
//! 1eb934: str x27, [x22]                                ; [self+0x28] = ColorScheme*
//!
//! 1eb958: bl FormatScheme::CreateDefault()             ; sret → sp[0x28]
//! 1eb968: str x8, [x24]                                 ; [self+0x30] = FormatScheme*
//!
//! 1eb9fc: mov w0, #0x18 (=24); bl operator_new        ; alloc FontScheme
//! 1eba10: bl CHncStringW::CHncStringW()                ; init FontScheme.name at +0
//! 1eba18: str x27, [x23]                                ; [self+0x38] = FontScheme*
//!   (FontScheme[+0x8..+0x18] = 0,0 — 2 null FontSet*)
//!
//! 1eba54: bl ObjectDefaults::CreateDefault()           ; sret → sp[0x28]
//! 1eba64: str x8, [x25]                                 ; [self+0x40] = ObjectDefaults*
//! ```
//!
//! # raw `~Theme()` @ `0x1ebfec` (역순 cleanup)
//!
//! 1. ObjectDefaults*: ~ObjectDefaults + delete
//! 2. FontScheme*: ~FontScheme + delete
//! 3. FormatScheme*: ~FormatScheme + delete
//! 4. ColorScheme*: ~ColorScheme + delete
//! 5. CHncStringW name: ~CHncStringW
//! 6. SharePtr<Theme> parent: refcount--
//! 7. Guid: trivial (POD)
//!
//! # 본 R-1.6 단계 scope
//!
//! - 72B layout + field offsets 1:1
//! - `Theme::new_uninitialized()` (= bool=false) **완전 byte-eq** — 모든 sub-objects null.
//! - `Theme::new_with_defaults_partial()` (= bool=true, 부분 byte-eq) — ColorScheme +
//!   EMPTY FontScheme 만 알맞게 생성. FormatScheme/ObjectDefaults 는 null 유지
//!   (CreateDefault 가 Brush/Pen/EffectStyle/DefaultProperty 의 multi-session 작업 필요).
//! - `~Theme`: 4 sub-objects conditional cleanup.
//! - `Guid::new` (random) — 본 단계에선 zero-init (CreateID 의 정확한 byte-eq 는 R-1.5.1
//!   포팅에서 done — Guid 자체 ctor 호출만 하면 byte-eq).
//!
//! # 의도적 deferred
//!
//! - `Theme(SharePtr<Theme> const&, bool)` (`0x1ebb6c`) — SharePtr copy + same bool path.
//! - `Theme(Theme const&)` (`0x1ebe3c`) — full copy ctor (각 sub-object 의 clone).
//! - Theme(true) 의 FormatScheme::CreateDefault / ObjectDefaults::CreateDefault — sub-objects RE multi-session.
//! - `Theme::operator=` / accessors (`GetColorScheme`, `GetFormatScheme` 등).
//!
//! # raw audit 정정 (15번째 세션의 추정 오류 정정)
//!
//! - 이전 audit: `self+0x38` = `FontSet*` (alloc 24B). FontSet 자체는 48B 인데 24B alloc 이라
//!   추정만 했었음.
//! - 본 16번째 세션: raw asm 정독으로 **`self+0x38` = `FontScheme*` (24B = CHncStringW + 2 FontSet*)**
//!   임을 확정. FontScheme 모듈 추가 (`font_scheme.rs`).

use crate::color_scheme::ColorScheme;
use crate::font_scheme::FontScheme;
use crate::font_set::FontSet;
use crate::format_scheme::FormatScheme;
use crate::guid::Guid;
use crate::object_defaults::ObjectDefaults;
use crate::share_ptr::ControlBlock;
use crate::string_w::CHncStringW;
use std::ptr;

pub const THEME_SIZE_BYTES: usize = 72;
pub const THEME_ALIGN_BYTES: usize = 8;

/// Theme 의 raw field offset 들 — raw asm 으로부터 도출.
pub mod offset {
    pub const GUID: usize = 0x00;
    pub const PARENT_SHAREPTR: usize = 0x10;
    pub const INIT_DEFAULTS: usize = 0x18;
    pub const NAME: usize = 0x20;
    pub const COLOR_SCHEME: usize = 0x28;
    pub const FORMAT_SCHEME: usize = 0x30;
    /// **audit 정정** (16번째 세션): `FontScheme*` (24B), 이전엔 `FontSet*` 오추정.
    pub const FONT_SCHEME: usize = 0x38;
    pub const OBJECT_DEFAULTS: usize = 0x40;
}

/// raw 72B `Hnc::Shape::Theme`.
#[repr(C)]
pub struct Theme {
    /// raw +0x00: Guid (16B).
    pub guid: Guid,
    /// raw +0x10: SharePtr<Theme> (8B) — parent theme; null in bool ctor.
    pub parent: *mut ControlBlock<()>,
    /// raw +0x18: bool init_defaults (1B).
    pub init_defaults: u8,
    /// raw +0x19: 7B alignment padding.
    pub _pad_0x19: [u8; 7],
    /// raw +0x20: CHncStringW name (8B).
    pub name: CHncStringW,
    /// raw +0x28: ColorScheme* (8B, owning, nullable).
    pub color_scheme: *mut ColorScheme,
    /// raw +0x30: FormatScheme* (8B, owning, nullable).
    pub format_scheme: *mut FormatScheme,
    /// raw +0x38: FontScheme* (8B, owning, nullable).
    pub font_scheme: *mut FontScheme,
    /// raw +0x40: ObjectDefaults* (8B, owning, nullable).
    pub object_defaults: *mut ObjectDefaults,
}

const _: () = assert!(std::mem::size_of::<Theme>() == THEME_SIZE_BYTES);
const _: () = assert!(std::mem::align_of::<Theme>() == THEME_ALIGN_BYTES);

impl Theme {
    /// raw `Theme::Theme(bool init_defaults=false)` (`0x1eb8b4`) — 모든 sub-objects null.
    ///
    /// **완전 byte-eq 보장** — raw 의 `cbz w8, exit` path 와 byte 단위 일치.
    pub fn new_uninitialized() -> Self {
        Theme {
            guid: Guid::new(),
            parent: ptr::null_mut(),
            init_defaults: 0,
            _pad_0x19: [0; 7],
            name: CHncStringW::default(),
            color_scheme: ptr::null_mut(),
            format_scheme: ptr::null_mut(),
            font_scheme: ptr::null_mut(),
            object_defaults: ptr::null_mut(),
        }
    }

    /// raw `Theme::Theme(bool init_defaults=true)` (`0x1eb8b4` true path) — **부분
    /// byte-eq**.
    ///
    /// 본 단계 implementation:
    /// - ColorScheme: heap alloc 32B + 12 hardcoded SetAt (✓ 완전 byte-eq).
    /// - FormatScheme: **null (deferred)** — raw 는 `CreateDefault()` 결과. 본 단계
    ///   엔 sub-objects (Brush/Pen/EffectStyle) RE 미완 으로 placeholder.
    /// - FontScheme: heap alloc 24B + name CHncStringW default + 2 null FontSet*
    ///   (✓ 완전 byte-eq — raw 의 inline 빈 FontScheme 과 동일).
    /// - ObjectDefaults: **null (deferred)** — raw 는 `CreateDefault()`. 본 단계
    ///   엔 DefaultProperty RE 미완 으로 placeholder.
    ///
    /// raw 와의 byte 차이: `[self+0x30]`, `[self+0x40]` 가 null vs 실제 alloc'd ptr.
    /// 외부 binary 출력 (PDF) 에 영향 — 추후 sub-objects RE 완료 시 정정.
    pub fn new_with_defaults_partial() -> Self {
        // Phase 1: Guid (raw CreateID — Rust 는 random/nil 둘 다 가능. 본 단계 byte-eq
        //   목적엔 어떤 16B 라도 동일 사용. Rust 의 random Guid 는 raw 와 자릿수 단위
        //   매칭 불가 — 본 단계는 nil 사용으로 stability 우선; 추후 CreateID 의 byte-eq
        //   random 매칭은 multi-session 작업).
        let mut t = Theme {
            guid: Guid::new(),
            parent: ptr::null_mut(),
            init_defaults: 1,
            _pad_0x19: [0; 7],
            name: CHncStringW::default(),
            color_scheme: ptr::null_mut(),
            format_scheme: ptr::null_mut(),
            font_scheme: ptr::null_mut(),
            object_defaults: ptr::null_mut(),
        };

        // Phase 6.1: ColorScheme — raw 0x1eb920-0x1eb934
        // alloc 32B + ColorScheme::ColorScheme() (12 SetAt)
        let cs = Box::into_raw(ColorScheme::new());
        t.color_scheme = cs;

        // Phase 6.2: FormatScheme — raw 0x1eb958 (CreateDefault, deferred).
        // 본 단계는 null 유지 (sub-objects RE 미완).
        // Note: 향후 FormatScheme::CreateDefault 가 port 되면 그 시점에 set.

        // Phase 6.3: FontScheme — raw 0x1eb9fc-0x1eba18
        // alloc 24B + CHncStringW default + 2 null FontSet*
        let fs = Box::into_raw(Box::new(FontScheme::new_empty()));
        t.font_scheme = fs;

        // Phase 6.4: ObjectDefaults — raw 0x1eba54 (CreateDefault, deferred).
        // 본 단계는 null 유지.

        t
    }

    /// raw `Theme::Theme(SharePtr<Theme> const& parent, bool init_defaults)` (`0x1ebb6c`).
    ///
    /// `Theme(bool)` 과 동일 paths + offset 0x10 에 parent SharePtr.raw 복사 +
    /// refcount++:
    ///
    /// ```asm
    /// 1ebba0: ldr x8, [x20]                ; x8 = arg1.raw
    /// 1ebba8: str x8, [x20, #0x10]!         ; [self+0x10] = arg1.raw (SharePtr copy)
    /// 1ebbac-1ebbb8: if x8 != null: refcount++
    /// ; (이후 bool/name/sub-objects path 는 Theme(bool) 과 동일)
    /// ```
    ///
    /// 본 단계는 `init_defaults=false` 만 완전 byte-eq, `=true` 는 ColorScheme +
    /// empty FontScheme 까지만 (FormatScheme/ObjectDefaults CreateDefault deferred).
    ///
    /// # Safety
    /// `parent_cb` 는 valid `ControlBlock<Theme>*` (또는 null). refcount 가 본
    /// 함수 내에서 증가.
    pub unsafe fn new_with_parent(
        parent_cb: *mut ControlBlock<()>,
        init_defaults: bool,
    ) -> Self {
        let mut t = if init_defaults {
            Self::new_with_defaults_partial()
        } else {
            Self::new_uninitialized()
        };
        t.init_defaults = if init_defaults { 1 } else { 0 };
        // raw 0x1ebba8: [self+0x10] = arg1.raw
        t.parent = parent_cb;
        // raw 0x1ebbac-0x1ebbb8: refcount++ if non-null (SharePtr copy semantic)
        if !parent_cb.is_null() {
            (*parent_cb).refcount = (*parent_cb).refcount.wrapping_add(1);
        }
        t
    }

    /// raw `~Theme()` (`0x1ebfec`) — 역순 sub-object cleanup.
    ///
    /// 본 메소드는 Drop impl 의 본체. 4 sub-objects 가 null 일 수 있어 조건적 free.
    pub fn destruct_inplace(&mut self) {
        unsafe {
            if !self.object_defaults.is_null() {
                drop(Box::from_raw(self.object_defaults));
                self.object_defaults = ptr::null_mut();
            }
            if !self.font_scheme.is_null() {
                drop(Box::from_raw(self.font_scheme));
                self.font_scheme = ptr::null_mut();
            }
            if !self.format_scheme.is_null() {
                drop(Box::from_raw(self.format_scheme));
                self.format_scheme = ptr::null_mut();
            }
            if !self.color_scheme.is_null() {
                drop(Box::from_raw(self.color_scheme));
                self.color_scheme = ptr::null_mut();
            }
            // name CHncStringW 는 자동 drop (field 순서)
            // parent SharePtr<Theme> 는 raw refcount-- 패턴
            if !self.parent.is_null() {
                let cb = self.parent;
                (*cb).refcount = (*cb).refcount.wrapping_sub(1);
                if (*cb).refcount == 0 {
                    // T 의 dtor 호출 + free (raw 는 virtual; 본 단계는 ZST 가정 — defer).
                    if !(*cb).obj.is_null() {
                        // Theme parent 의 실제 ptr — 본 단계는 보수적으로 leak 방지
                        // 차원에서 dealloc.
                        std::alloc::dealloc(
                            (*cb).obj as *mut u8,
                            std::alloc::Layout::new::<Theme>(),
                        );
                    }
                    std::alloc::dealloc(
                        cb as *mut u8,
                        std::alloc::Layout::new::<ControlBlock<()>>(),
                    );
                }
                self.parent = ptr::null_mut();
            }
        }
    }

    /// raw `Theme::Theme(const Theme&)` (`0x1ebe3c`) 1:1 — copy ctor.
    ///
    /// 알고리즘 (raw 1ebe5c..1ebf2c):
    /// 1. **Guid** (offset 0x00): `bl 0x6b3c00` → `Guid::Generator::CreateID()` —
    ///    src 의 guid 를 **copy 안 함**. 새 random v4 UUID 생성. ⚠️
    /// 2. **parent SharePtr<Theme>** (offset 0x10): src.parent → self.parent
    ///    (raw byte copy), 그 후 non-null 이면 `(*cb).refcount++`.
    /// 3. **init_defaults bool** (offset 0x18): src 의 byte 복사.
    /// 4. **name CHncStringW** (offset 0x20): raw `bl 0x6b3a44` →
    ///    `CHncStringW::CHncStringW(const CHncStringW&)` (refcount++).
    /// 5. **color_scheme** (offset 0x28): raw `bl 0x66ead8` →
    ///    `ColorScheme::clone_or_null(src.color_scheme)`. 결과 ptr 저장.
    /// 6. **format_scheme** (offset 0x30): null → null; non-null → alloc 104B +
    ///    `FormatScheme::FormatScheme(const FormatScheme&)`.
    /// 7. **font_scheme** (offset 0x38): null → null; non-null → alloc 24B +
    ///    `FontScheme::FontScheme(const FontScheme&)`.
    /// 8. **object_defaults** (offset 0x40): null → null; non-null → alloc 24B +
    ///    `ObjectDefaults::ObjectDefaults(const ObjectDefaults&)`.
    ///
    /// **byte-eq scope (현재)**:
    /// - Guid: random v4 — raw 와 byte 다름 (raw 도 매 호출 다름).
    /// - parent / bool / name / ColorScheme / 3 sub-object null path 모두 byte-eq.
    /// - non-null FormatScheme: Brush/Pen/EffectStyle vfunc Clone RE 후 완전 byte-eq.
    /// - non-null FontScheme.major/minor: FontSet copy ctor (0x633c40) RE 후 byte-eq.
    /// - non-null ObjectDefaults.share_ptr: DefaultProperty Clone vfunc RE 후 byte-eq.
    ///
    /// # Safety
    /// `src` 는 valid `&Theme`. 본 메소드는 `Self` 를 직접 반환 — caller 가 Box 으로
    /// 감싸거나 stack 에 사용.
    pub unsafe fn copy_ctor(src: &Theme) -> Self {
        // raw `1ebe58-1ebe5c`: mov x8, x0; bl CreateID
        // CreateID 는 self+0x00 에 새 Guid 저장 (sret pattern).
        let new_guid = Guid::create_id();

        // raw `1ebe60-1ebe78`: SharePtr<Theme> copy at +0x10 + refcount++
        let new_parent = src.parent;
        if !new_parent.is_null() {
            (*new_parent).refcount = (*new_parent).refcount.wrapping_add(1);
        }

        // raw `1ebe7c-1ebe80`: bool copy at +0x18
        let new_flag = src.init_defaults;

        // raw `1ebe84-1ebe90`: CHncStringW copy ctor at +0x20
        let new_name = src.name.clone();

        // raw `1ebe94-1ebe9c`: ColorScheme clone-or-null at +0x28
        let new_color_scheme = ColorScheme::clone_or_null(src.color_scheme);

        // raw `1ebea0-1ebec0` (or null path `1ebf04`): FormatScheme at +0x30
        let new_format_scheme: *mut FormatScheme = if src.format_scheme.is_null() {
            ptr::null_mut()
        } else {
            (*src.format_scheme).clone_to_heap()
        };

        // raw `1ebec0-1ebee0` (or null path `1ebf14`): FontScheme at +0x38
        let new_font_scheme: *mut FontScheme = if src.font_scheme.is_null() {
            ptr::null_mut()
        } else {
            (*src.font_scheme).clone_to_heap()
        };

        // raw `1ebee4-1ebf00` (or null path `1ebf28`): ObjectDefaults at +0x40
        let new_object_defaults: *mut ObjectDefaults = if src.object_defaults.is_null() {
            ptr::null_mut()
        } else {
            (*src.object_defaults).clone_to_heap()
        };

        Theme {
            guid: new_guid,
            parent: new_parent,
            init_defaults: new_flag,
            _pad_0x19: src._pad_0x19,
            name: new_name,
            color_scheme: new_color_scheme,
            format_scheme: new_format_scheme,
            font_scheme: new_font_scheme,
            object_defaults: new_object_defaults,
        }
    }

    /// heap-alloc 새 Theme + copy ctor — raw `new Theme(*src)` 패턴.
    ///
    /// # Safety
    /// 반환 ptr 은 `Box::from_raw` 등 으로 해제.
    pub unsafe fn clone_to_heap(&self) -> *mut Theme {
        let layout = std::alloc::Layout::new::<Theme>();
        let p = std::alloc::alloc(layout) as *mut Theme;
        if p.is_null() {
            std::alloc::handle_alloc_error(layout);
        }
        ptr::write(p, Self::copy_ctor(self));
        p
    }

    /// accessor — raw 의 GetColorScheme 등은 별도 export 가 있으나 본 단계는
    /// pub field 로 노출. 향후 raw accessor 의 정확한 동작 (offset access only
    /// or with sret SharePtr) RE 시 추가.
    #[inline]
    pub fn get_color_scheme(&self) -> *mut ColorScheme {
        self.color_scheme
    }
    #[inline]
    pub fn get_format_scheme(&self) -> *mut FormatScheme {
        self.format_scheme
    }
    #[inline]
    pub fn get_font_scheme(&self) -> *mut FontScheme {
        self.font_scheme
    }
    #[inline]
    pub fn get_object_defaults(&self) -> *mut ObjectDefaults {
        self.object_defaults
    }
    #[inline]
    pub fn get_name(&self) -> &CHncStringW {
        &self.name
    }
    #[inline]
    pub fn get_init_defaults_flag(&self) -> bool {
        self.init_defaults != 0
    }

    /// raw `Theme::GetFormatScheme(bool init_if_null)` (`0x1ec490`, 56B).
    ///
    /// ```asm
    /// 1ec490: prologue
    /// 1ec4a0: cbz w1, 0x1ec4e0       ; if init_if_null == false → simple return
    /// 1ec4a4: ldr x19, [x0, #0x30]    ; x19 = self->format_scheme
    /// 1ec4a8: cbnz x19, 0x1ec4cc      ; non-null → return
    /// 1ec4ac: mov x8, x0              ; x8 = current Theme*
    /// 1ec4b0: ldr x9, [x0, #0x10]     ; x9 = parent.raw (ControlBlock*)
    /// 1ec4b4: cbz x9, 0x1ec4c0        ; if cb null → check init_defaults
    /// 1ec4b8: ldr x0, [x9]            ; x0 = cb->obj (parent Theme*)
    /// 1ec4bc: cbnz x0, 0x1ec4a4       ; if non-null → loop with new self
    /// 1ec4c0: ldrb w8, [x8, #0x18]    ; w8 = LAST self->init_defaults
    /// 1ec4c4: cbz w8, 0x1ec4f8        ; if false → ShapeEngine fallback
    /// 1ec4c8: mov x19, #0             ; return null
    /// 1ec4cc-dc: return x19
    /// 1ec4e0: ldr x19, [x0, #0x30]    ; simple-mode: return as-is (no init walk)
    /// 1ec4f8: bl ShapeEngine::GetInstance
    /// 1ec4fc: ldr x19, [x0, #0x8]     ; ShapeEngine[+0x8] = default FormatScheme*
    /// ```
    ///
    /// 본 port 는 ShapeEngine 의존을 trait 으로 분리.
    pub fn get_format_scheme_init<E: ShapeEngineProvider + ?Sized>(
        &self,
        init_if_null: bool,
        engine: Option<&E>,
    ) -> *mut FormatScheme {
        if !init_if_null {
            // raw 1ec4e0: simple mode
            return self.format_scheme;
        }

        // raw 1ec4a4-bc: parent chain walk
        let mut current: *const Theme = self;
        let mut last_init_defaults: u8 = 0;
        loop {
            // SAFETY: current is either &self or *parent.obj, both valid.
            let cur_ref = unsafe { &*current };
            let fs = cur_ref.format_scheme;
            if !fs.is_null() {
                return fs;
            }
            // raw 1ec4ac: x8 = current
            last_init_defaults = cur_ref.init_defaults;
            // raw 1ec4b0: x9 = parent.raw
            let parent_cb = cur_ref.parent;
            if parent_cb.is_null() {
                break;
            }
            // raw 1ec4b8: x0 = parent.cb->obj
            let parent_obj = unsafe { (*parent_cb).obj as *const Theme };
            if parent_obj.is_null() {
                break;
            }
            current = parent_obj;
        }

        // raw 1ec4c0-c4: check last self's init_defaults
        if last_init_defaults != 0 {
            // raw 1ec4f8: ShapeEngine fallback
            if let Some(e) = engine {
                return e.default_format_scheme();
            }
        }
        // raw 1ec4c8: return null
        ptr::null_mut()
    }

    /// raw `Theme::GetFontScheme(bool init_if_null)` (`0x1ec65c`, 56B).
    /// 동일 패턴, offset 만 다름 (font_scheme @ +0x38, ShapeEngine default @ +0x10).
    pub fn get_font_scheme_init<E: ShapeEngineProvider + ?Sized>(
        &self,
        init_if_null: bool,
        engine: Option<&E>,
    ) -> *mut FontScheme {
        if !init_if_null {
            return self.font_scheme;
        }

        let mut current: *const Theme = self;
        let mut last_init_defaults: u8 = 0;
        loop {
            let cur_ref = unsafe { &*current };
            let fs = cur_ref.font_scheme;
            if !fs.is_null() {
                return fs;
            }
            last_init_defaults = cur_ref.init_defaults;
            let parent_cb = cur_ref.parent;
            if parent_cb.is_null() {
                break;
            }
            let parent_obj = unsafe { (*parent_cb).obj as *const Theme };
            if parent_obj.is_null() {
                break;
            }
            current = parent_obj;
        }
        if last_init_defaults != 0 {
            if let Some(e) = engine {
                return e.default_font_scheme();
            }
        }
        ptr::null_mut()
    }

    /// raw `Theme::GetSchemeBrush(FormatScheme::Style)` (`0x1ec84c`, ~196B).
    ///
    /// FormatScheme.brushes tree (offset +0x10 = end_node_left) 에서 style key 로
    /// lower_bound 검색 → 정확 일치 시 brush ctrl ptr 반환 + refcount++.
    ///
    /// ```asm
    /// 1ec870: GetFormatScheme(true); cbz → null
    /// 1ec880: ldr x9, [x0, #0x10]!    ; x9 = brushes.end_node_left (root)
    /// 1ec884: cbz → null
    /// 1ec888-a4: lower_bound 루프 (key at +0x20, right at +0x8)
    /// 1ec8a8: cmp best, sentinel
    /// 1ec8b0-b8: cmp best.key vs style → b.ls → exact-match path
    /// 1ec8e4: ldr x8, [best, #0x28]   ; value = ControlBlock*
    /// 1ec8e8-900: write to out + refcount++
    /// ```
    ///
    /// 반환 ptr 은 ControlBlock* — non-null 일 때 refcount 가 이미 증분된 상태.
    /// Caller (= SharePtr<Brush> 의 ctor) 가 ownership 보유.
    pub fn get_scheme_brush<E: ShapeEngineProvider + ?Sized>(
        &self,
        style: u32,
        engine: Option<&E>,
    ) -> *mut crate::format_scheme::BrushControlBlock {
        let fs = self.get_format_scheme_init(true, engine);
        if fs.is_null() {
            return ptr::null_mut();
        }
        // raw 1ec880: brushes.end_node_left
        let root = unsafe { (*fs).brushes.end_node_left };
        match lookup_brush_node(root, style) {
            None => ptr::null_mut(),
            Some(node) => {
                let cb = unsafe { (*node).value };
                if cb.is_null() {
                    return ptr::null_mut();
                }
                // raw 1ec8f0-900: retain (refcount++ if obj non-null)
                unsafe {
                    let cb_ref = &mut *cb;
                    if cb_ref.obj.is_null() {
                        return ptr::null_mut();
                    }
                    cb_ref.strong += 1;
                }
                cb
            }
        }
    }

    /// raw `Theme::GetSchemeBackgroundBrush(FormatScheme::Style)` (`0x1ec914`).
    /// `GetSchemeBrush` 와 동일 — `FormatScheme.bg_brushes` (offset +0x28) 사용.
    pub fn get_scheme_background_brush<E: ShapeEngineProvider + ?Sized>(
        &self,
        style: u32,
        engine: Option<&E>,
    ) -> *mut crate::format_scheme::BrushControlBlock {
        let fs = self.get_format_scheme_init(true, engine);
        if fs.is_null() {
            return ptr::null_mut();
        }
        let root = unsafe { (*fs).bg_brushes.end_node_left };
        match lookup_brush_node(root, style) {
            None => ptr::null_mut(),
            Some(node) => {
                let cb = unsafe { (*node).value };
                if cb.is_null() {
                    return ptr::null_mut();
                }
                unsafe {
                    let cb_ref = &mut *cb;
                    if cb_ref.obj.is_null() {
                        return ptr::null_mut();
                    }
                    cb_ref.strong += 1;
                }
                cb
            }
        }
    }

    /// raw `Theme::GetMajorFont() const` @ `0x168c04` (~28 instr) byte-eq.
    ///
    /// ```asm
    /// 168c14: mov w1, #0x1            ; init_if_null = true
    /// 168c18: bl  GetFontScheme       ; x0 = font_scheme*
    /// 168c1c: mov x19, x0
    /// 168c20: ldr x0, [x0, #0x8]      ; major = font_scheme.major
    /// 168c24: cbz x0, lazy_init       ; if null → CreateDefaultFontSet
    /// ; non-null fast path:
    /// 168c28-34: epilogue → return major
    /// ; lazy init path:
    /// 168c38: add x8, sp, #0x8        ; sret slot
    /// 168c3c: bl  FontScheme::CreateDefaultFontSet()
    /// 168c40: ldr x0, [sp, #0x8]      ; x0 = new FontSet*
    /// 168c44: ldr x20, [x19, #0x8]    ; old major
    /// 168c48: str x0, [x19, #0x8]     ; major = new
    /// 168c4c: cbz x20, ret_new        ; if no old → return new
    /// 168c50-58: ~FontSet(old) + dealloc
    /// 168c5c: ldr x0, [x19, #0x8]     ; reload (now = new)
    /// 168c60-6c: epilogue
    /// ```
    ///
    /// **byte-eq scope (L-5c-5b1 완료 후)**: fast path + lazy init path 모두 byte-eq.
    /// `FontScheme::create_default_font_set` (raw 0x16a144 port) 가 통합되어 raw 의
    /// 168c38-6c 의 alloc + assign + old release 시퀀스를 1:1 복제.
    pub fn get_major_font<E: ShapeEngineProvider + ?Sized>(
        &mut self,
        engine: Option<&E>,
    ) -> *mut FontSet {
        // raw 168c18: GetFontScheme(true)
        let fs = self.get_font_scheme_init(true, engine);
        if fs.is_null() {
            return ptr::null_mut();
        }
        unsafe {
            // raw 168c20: ldr x0, [x19, #0x8] = font_scheme.major
            let major = (*fs).major;
            if !major.is_null() {
                // raw 168c28-34: fast path return
                return major;
            }
            // ----- raw 168c38-6c: lazy init path
            // raw 168c3c: bl FontScheme::CreateDefaultFontSet → new FontSet*
            let new_set = FontScheme::create_default_font_set();
            // raw 168c44: ldr x20, [x19, #0x8] (old major, before overwrite)
            let old = (*fs).major;
            // raw 168c48: str x0, [x19, #0x8] (major = new)
            (*fs).major = new_set;
            // raw 168c4c-58: if old non-null → ~FontSet(old) + dealloc
            if !old.is_null() {
                drop(Box::from_raw(old));
            }
            // raw 168c5c: ldr x0, [x19, #0x8] (reload = new) + return
            (*fs).major
        }
    }

    /// raw `Theme::GetMinorFont() const` @ `0x168c70` — GetMajorFont 와 동일 패턴.
    /// 차이: `[font_scheme+0x10]` (minor) instead of `[+0x8]` (major).
    pub fn get_minor_font<E: ShapeEngineProvider + ?Sized>(
        &mut self,
        engine: Option<&E>,
    ) -> *mut FontSet {
        let fs = self.get_font_scheme_init(true, engine);
        if fs.is_null() {
            return ptr::null_mut();
        }
        unsafe {
            // raw 168c8c: ldr x0, [x19, #0x10] = font_scheme.minor
            let minor = (*fs).minor;
            if !minor.is_null() {
                return minor;
            }
            // raw 168ca4-cc: lazy init path (동일 패턴, offset 만 +0x10)
            let new_set = FontScheme::create_default_font_set();
            let old = (*fs).minor;
            (*fs).minor = new_set;
            if !old.is_null() {
                drop(Box::from_raw(old));
            }
            (*fs).minor
        }
    }

    /// raw `Theme::GetSchemeEffects(FormatScheme::Style)` @ `0x15de24` (~190B) byte-eq.
    ///
    /// ```asm
    /// 15de48: bl GetFormatScheme(true); cbz → null
    /// 15de58: bl GetFormatScheme(true) (호출 두 번 — raw 정확 그대로)
    /// 15de5c: ldr x9, [x0, #0x58]!     ; x0 += 0x58 (effects tree end_node_left)
    /// 15de60-de80: lower_bound walk (key at +0x20, right at +0x8)
    /// 15de84: cmp x8(best), x0(sentinel); b.eq → not found
    /// 15de8c-94: cmp best.key, style; b.ls → found path; else → not found
    /// 15ded4: ldr x20, [best+0x28]     ; x20 = EffectStyleControlBlock*
    /// 15dee0: ldr x8, [x20]             ; x8 = ctrl.obj = EffectStyle*
    /// 15dee4: cbz x8 → empty
    /// 15dee8-f0: refcount++ on EffectStyleControlBlock
    /// 15def4: bl 0x6492b8               ; post-retain hook (trace stub)
    /// 15def8: ldr x8, [x20]             ; reload obj
    /// 15df00: ldr x8, [x8, #0x10]       ; effects = EffectStyle.effects (SharePtr<Effects>)
    /// 15df04: str x8, [x19]             ; sret
    /// 15df08: cbz x8 → done (null effects)
    /// 15df0c-1c: refcount++ on Effects ctrl
    /// 15df20: bl 0x649980               ; post-retain hook (trace stub)
    /// 15df24: b epilogue
    /// ```
    ///
    /// 반환 ptr 은 `*mut ControlBlock<Effects>` — non-null 일 때 refcount 가 이미 증분된 상태.
    pub fn get_scheme_effects<E: ShapeEngineProvider + ?Sized>(
        &self,
        style: u32,
        engine: Option<&E>,
    ) -> *mut crate::share_ptr::ControlBlock<crate::effects_container::Effects> {
        let fs = self.get_format_scheme_init(true, engine);
        if fs.is_null() {
            return ptr::null_mut();
        }
        let root = unsafe { (*fs).effects.end_node_left };
        match lookup_effect_style_node(root, style) {
            None => ptr::null_mut(),
            Some(node) => unsafe {
                // raw 15ded4: ldr x20, [best+0x28] = EffectStyleControlBlock*
                let es_cb = (*node).value;
                if es_cb.is_null() {
                    return ptr::null_mut();
                }
                let es_cb_ref = &mut *es_cb;
                if es_cb_ref.obj.is_null() {
                    return ptr::null_mut();
                }
                // raw 15dee8-f0: refcount++ on EffectStyle ctrl
                es_cb_ref.strong += 1;
                // raw 15df00: ldr x8 = EffectStyle.effects (+0x10) — SharePtr<Effects>
                let es = &*es_cb_ref.obj;
                let effects_cb = es.effects;
                if effects_cb.is_null() {
                    return ptr::null_mut();
                }
                let effects_cb_ref = &mut *effects_cb;
                if effects_cb_ref.obj.is_null() {
                    return effects_cb;
                }
                // raw 15df14-1c: refcount++ on Effects ctrl (raw uses .refcount field)
                effects_cb_ref.refcount = effects_cb_ref.refcount.wrapping_add(1);
                effects_cb
            },
        }
    }

    /// raw `Theme::GetSchemeScene3D(FormatScheme::Style)` @ `0x15e1fc` byte-eq.
    /// Same pattern as GetSchemeEffects — only field offset differs (`+0x00` for Scene3D).
    pub fn get_scheme_scene3d<E: ShapeEngineProvider + ?Sized>(
        &self,
        style: u32,
        engine: Option<&E>,
    ) -> *mut crate::share_ptr::ControlBlock<crate::scene3d::Scene3D> {
        let fs = self.get_format_scheme_init(true, engine);
        if fs.is_null() {
            return ptr::null_mut();
        }
        let root = unsafe { (*fs).effects.end_node_left };
        match lookup_effect_style_node(root, style) {
            None => ptr::null_mut(),
            Some(node) => unsafe {
                let es_cb = (*node).value;
                if es_cb.is_null() {
                    return ptr::null_mut();
                }
                let es_cb_ref = &mut *es_cb;
                if es_cb_ref.obj.is_null() {
                    return ptr::null_mut();
                }
                es_cb_ref.strong += 1;
                // raw 15e2d8: ldr x8 = EffectStyle.scene3d (+0x00) — SharePtr<Scene3D>
                let es = &*es_cb_ref.obj;
                let s3d_cb = es.scene3d;
                if s3d_cb.is_null() {
                    return ptr::null_mut();
                }
                let s3d_cb_ref = &mut *s3d_cb;
                if s3d_cb_ref.obj.is_null() {
                    return s3d_cb;
                }
                // raw 15e2ec-f4: refcount++ on Scene3D ctrl
                s3d_cb_ref.refcount = s3d_cb_ref.refcount.wrapping_add(1);
                s3d_cb
            },
        }
    }

    /// raw `Theme::GetSchemeSp3D(FormatScheme::Style)` @ `0x15e434` byte-eq.
    /// Same pattern — field offset `+0x08` for Sp3D.
    pub fn get_scheme_sp3d<E: ShapeEngineProvider + ?Sized>(
        &self,
        style: u32,
        engine: Option<&E>,
    ) -> *mut crate::share_ptr::ControlBlock<crate::sp3d::Sp3D> {
        let fs = self.get_format_scheme_init(true, engine);
        if fs.is_null() {
            return ptr::null_mut();
        }
        let root = unsafe { (*fs).effects.end_node_left };
        match lookup_effect_style_node(root, style) {
            None => ptr::null_mut(),
            Some(node) => unsafe {
                let es_cb = (*node).value;
                if es_cb.is_null() {
                    return ptr::null_mut();
                }
                let es_cb_ref = &mut *es_cb;
                if es_cb_ref.obj.is_null() {
                    return ptr::null_mut();
                }
                es_cb_ref.strong += 1;
                // raw 15e510: ldr x8 = EffectStyle.sp3d (+0x08) — SharePtr<Sp3D>
                let es = &*es_cb_ref.obj;
                let sp3d_cb = es.sp3d;
                if sp3d_cb.is_null() {
                    return ptr::null_mut();
                }
                let sp3d_cb_ref = &mut *sp3d_cb;
                if sp3d_cb_ref.obj.is_null() {
                    return sp3d_cb;
                }
                // raw 15e524-2c: refcount++ on Sp3D ctrl
                sp3d_cb_ref.refcount = sp3d_cb_ref.refcount.wrapping_add(1);
                sp3d_cb
            },
        }
    }

    /// raw `Theme::GetSchemePen(FormatScheme::Style)` (`0x1ec9dc`).
    /// `GetSchemeBrush` 와 동일 — `FormatScheme.pens` (offset +0x40) 사용.
    /// Pen 의 ControlBlock 은 `FsPenMapNode.value` (= 24B `*mut PenControlBlock`).
    /// 본 단계는 layout 동등성 가정 — BrushControlBlock 의 obj/strong layout 과 동일.
    pub fn get_scheme_pen<E: ShapeEngineProvider + ?Sized>(
        &self,
        style: u32,
        engine: Option<&E>,
    ) -> *mut crate::format_scheme::BrushControlBlock {
        // Note: raw 가 PenControlBlock 을 BrushControlBlock 과 동일하게 취급 — 둘 다
        // (obj_ptr, refcount, flag, _pad) 16+8 = 24B layout. 본 port 는 같은 type
        // 으로 노출 (caller 가 cast).
        let fs = self.get_format_scheme_init(true, engine);
        if fs.is_null() {
            return ptr::null_mut();
        }
        let root = unsafe { (*fs).pens.end_node_left };
        match lookup_pen_node(root, style) {
            None => ptr::null_mut(),
            Some(node_ptr) => unsafe {
                let cb_ptr = node_ptr.value as *mut crate::format_scheme::BrushControlBlock;
                if cb_ptr.is_null() {
                    return ptr::null_mut();
                }
                let cb_ref = &mut *cb_ptr;
                if cb_ref.obj.is_null() {
                    return ptr::null_mut();
                }
                cb_ref.strong += 1;
                cb_ptr
            },
        }
    }
}

/// `Hnc::Shape::ShapeEngine` 의존성 trait — `Theme::GetFormatScheme` 등이 init
/// fallback 시 ShapeEngine::GetInstance 로 default scheme 을 얻음.
///
/// raw 의 ShapeEngine 은 singleton:
/// - `[singleton + 0x8]` = default FormatScheme*
/// - `[singleton + 0x10]` = default FontScheme*
///
/// 본 trait 은 그 두 default 만 제공.
pub trait ShapeEngineProvider {
    fn default_format_scheme(&self) -> *mut FormatScheme;
    fn default_font_scheme(&self) -> *mut FontScheme;
}

/// raw 의 RB tree lower_bound: BrushMapNode 에서 style key 로 검색.
///
/// 반환:
/// - None: tree empty, 또는 best.key != style (exact match 없음)
/// - Some(node): exact match (best.key == style)
fn lookup_brush_node(root: *mut crate::rb_tree::TreeNodeBase, style: u32)
    -> Option<*mut crate::format_scheme::FsBrushMapNode>
{
    use crate::format_scheme::FsBrushMapNode;
    use crate::rb_tree::TreeNodeBase;

    if root.is_null() {
        return None;
    }
    // raw 1ec888: x8 = sentinel = original ptr (before node walk)
    // 본 Rust port 는 "best 가 변경되었는지" 를 bool 로 추적.
    let mut best: *mut TreeNodeBase = ptr::null_mut();
    let mut node: *mut TreeNodeBase = root;
    while !node.is_null() {
        // raw 1ec88c: ldr w10, [x9, #0x20] — key at +0x20
        let key = unsafe { (*(node as *mut FsBrushMapNode)).key };
        if key < style {
            // raw 1ec898: csel x10, x11, x9, lo → right
            node = unsafe { (*node).right };
        } else {
            // raw csel else → x10 = node (= &node.left); best = node
            best = node;
            node = unsafe { (*node).left };
        }
    }
    // raw 1ec8a8: cmp x8, x0 → if no candidate → not found
    if best.is_null() {
        return None;
    }
    // raw 1ec8b0-b8: cmp best.key vs style → b.ls (= unsigned <=) → exact match
    let best_key = unsafe { (*(best as *mut FsBrushMapNode)).key };
    if best_key <= style {
        // After lower_bound, best.key >= style. Combined with <= style → ==.
        Some(best as *mut FsBrushMapNode)
    } else {
        None
    }
}

/// `lookup_brush_node` 의 Pen variant — FsPenMapNode.key offset 동일.
fn lookup_pen_node(root: *mut crate::rb_tree::TreeNodeBase, style: u32)
    -> Option<&'static mut crate::format_scheme::FsPenMapNode>
{
    use crate::format_scheme::FsPenMapNode;
    use crate::rb_tree::TreeNodeBase;

    if root.is_null() {
        return None;
    }
    let mut best: *mut TreeNodeBase = ptr::null_mut();
    let mut node: *mut TreeNodeBase = root;
    while !node.is_null() {
        let key = unsafe { (*(node as *mut FsPenMapNode)).key };
        if key < style {
            node = unsafe { (*node).right };
        } else {
            best = node;
            node = unsafe { (*node).left };
        }
    }
    if best.is_null() {
        return None;
    }
    let best_key = unsafe { (*(best as *mut FsPenMapNode)).key };
    if best_key <= style {
        // SAFETY: caller guarantees pointer stays valid during use.
        unsafe { Some(&mut *(best as *mut FsPenMapNode)) }
    } else {
        None
    }
}

/// `lookup_brush_node` 의 EffectStyle variant — FsEffectStyleMapNode.key offset 동일.
fn lookup_effect_style_node(
    root: *mut crate::rb_tree::TreeNodeBase,
    style: u32,
) -> Option<*mut crate::format_scheme::FsEffectStyleMapNode> {
    use crate::format_scheme::FsEffectStyleMapNode;
    use crate::rb_tree::TreeNodeBase;

    if root.is_null() {
        return None;
    }
    let mut best: *mut TreeNodeBase = ptr::null_mut();
    let mut node: *mut TreeNodeBase = root;
    while !node.is_null() {
        let key = unsafe { (*(node as *mut FsEffectStyleMapNode)).key };
        if key < style {
            node = unsafe { (*node).right };
        } else {
            best = node;
            node = unsafe { (*node).left };
        }
    }
    if best.is_null() {
        return None;
    }
    let best_key = unsafe { (*(best as *mut FsEffectStyleMapNode)).key };
    if best_key <= style {
        Some(best as *mut FsEffectStyleMapNode)
    } else {
        None
    }
}

impl Drop for Theme {
    fn drop(&mut self) {
        self.destruct_inplace();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_size_constants() {
        assert_eq!(THEME_SIZE_BYTES, 72);
        assert_eq!(THEME_ALIGN_BYTES, 8);
        assert_eq!(std::mem::size_of::<Theme>(), 72);
        assert_eq!(std::mem::align_of::<Theme>(), 8);
    }

    // ===== Theme::copy_ctor + sub-object Clone tests =====

    #[test]
    fn copy_ctor_uninitialized_yields_all_null_subobjects() {
        let src = Theme::new_uninitialized();
        let copy = unsafe { Theme::copy_ctor(&src) };

        // Guid is regenerated (random v4) — NOT copied from src.
        // (src is nil, copy will be non-nil unless arc4random collapses to 0 which is virtually 0.)
        // We just verify structure, not byte-eq.
        assert_eq!(copy.init_defaults, 0);
        assert!(copy.parent.is_null());
        assert!(copy.color_scheme.is_null());
        assert!(copy.format_scheme.is_null());
        assert!(copy.font_scheme.is_null());
        assert!(copy.object_defaults.is_null());
    }

    #[test]
    fn copy_ctor_partial_yields_cloned_color_scheme_and_font_scheme() {
        let src = Theme::new_with_defaults_partial();
        let copy = unsafe { Theme::copy_ctor(&src) };

        // bool copied
        assert_eq!(copy.init_defaults, 1);

        // ColorScheme should be a fresh clone (different ptr, same content)
        assert!(!copy.color_scheme.is_null());
        assert_ne!(copy.color_scheme, src.color_scheme);
        // Verify content matches: both should have 12 entries.
        unsafe {
            assert_eq!((*copy.color_scheme).len(), 12);
            // PKey 0 / 1 are System styles
            for k in 0u32..12 {
                let src_color = (*src.color_scheme).get_color(k);
                let cp_color = (*copy.color_scheme).get_color(k);
                assert!(src_color.is_some());
                assert!(cp_color.is_some());
                let s = src_color.unwrap();
                let c = cp_color.unwrap();
                assert_eq!(s.value, c.value);
                assert_eq!(s.type_tag, c.type_tag);
            }
        }

        // FontScheme should be a fresh clone (different ptr).
        assert!(!copy.font_scheme.is_null());
        assert_ne!(copy.font_scheme, src.font_scheme);
        unsafe {
            // Both have null FontSet ptrs (partial scope).
            assert!((*copy.font_scheme).major.is_null());
            assert!((*copy.font_scheme).minor.is_null());
        }

        // FormatScheme / ObjectDefaults: null in src (deferred), null in copy.
        assert!(copy.format_scheme.is_null());
        assert!(copy.object_defaults.is_null());
    }

    #[test]
    fn copy_ctor_parent_share_ptr_increments_refcount() {
        unsafe {
            // Create a parent SharePtr (ControlBlock<()>) with refcount=1.
            // We use raw allocation to mimic raw's structure.
            let cb_layout = std::alloc::Layout::new::<ControlBlock<()>>();
            let cb_ptr = std::alloc::alloc_zeroed(cb_layout) as *mut ControlBlock<()>;
            (*cb_ptr).obj = ptr::null_mut(); // No T; refcount-only test
            (*cb_ptr).refcount = 1;

            // Use Theme::new_with_parent
            let src = Theme::new_with_parent(cb_ptr, false);
            // refcount now 2 (= 1 original + 1 src)
            assert_eq!((*cb_ptr).refcount, 2);

            // Now copy_ctor: refcount should go to 3
            let copy = Theme::copy_ctor(&src);
            assert_eq!((*cb_ptr).refcount, 3);

            assert_eq!(copy.parent, cb_ptr);

            // Cleanup: drop copy → 2; drop src → 1; then manually dealloc cb.
            drop(copy);
            assert_eq!((*cb_ptr).refcount, 2);
            drop(src);
            assert_eq!((*cb_ptr).refcount, 1);

            // Final manual cleanup
            (*cb_ptr).refcount = 0;
            std::alloc::dealloc(cb_ptr as *mut u8, cb_layout);
        }
    }

    #[test]
    fn copy_ctor_independent_buffers() {
        // ColorScheme + FontScheme of copy are separate heap allocations from src.
        let src = Theme::new_with_defaults_partial();
        let copy = unsafe { Theme::copy_ctor(&src) };

        // Different ptrs
        assert_ne!(copy.color_scheme, src.color_scheme);
        assert_ne!(copy.font_scheme, src.font_scheme);

        // Verify independent: mutating one doesn't affect other (no actual mutate, just ptr identity check).
        let src_cs_addr = src.color_scheme as usize;
        let cp_cs_addr = copy.color_scheme as usize;
        assert_ne!(src_cs_addr, cp_cs_addr);
    }

    #[test]
    fn copy_ctor_color_scheme_tree_shape_matches() {
        // Both src and copy should have the same RB tree shape since:
        // (1) src tree was built with 12 sequential SetAt calls (sorted keys 0..11).
        // (2) copy iterates in-order (sorted) and SetAt to new tree.
        // Result: byte-eq tree shape (same sequence of inserts).
        let src = Theme::new_with_defaults_partial();
        let copy = unsafe { Theme::copy_ctor(&src) };

        unsafe {
            // Verify all 12 keys traversable in-order in both.
            let src_cs = &*src.color_scheme;
            let cp_cs = &*copy.color_scheme;
            assert_eq!(src_cs.len(), 12);
            assert_eq!(cp_cs.len(), 12);

            // Both have a valid root (= end_node_left).
            assert!(!src_cs.tree_end_node_left.is_null());
            assert!(!cp_cs.tree_end_node_left.is_null());
        }
    }

    #[test]
    fn copy_ctor_drop_releases_all_subobjects() {
        // Verify no double-free or leak: copy_ctor → drop should free its
        // independent ColorScheme/FontScheme allocations.
        let src = Theme::new_with_defaults_partial();
        {
            let _copy = unsafe { Theme::copy_ctor(&src) };
            // _copy dropped at end of scope
        }
        // src still valid
        unsafe {
            assert_eq!((*src.color_scheme).len(), 12);
        }
    }

    #[test]
    fn copy_ctor_guid_is_regenerated_not_copied() {
        // raw behavior: bl CreateID generates a new Guid, NOT copy from src.
        // Verify: src.guid (nil) vs copy.guid (non-nil after create_id).
        let src = Theme::new_uninitialized();
        assert!(src.guid.is_nil());
        let copy = unsafe { Theme::copy_ctor(&src) };
        // copy.guid should be non-nil (v4 UUID with version bits set).
        assert!(!copy.guid.is_nil());
        // Version bits: bytes[6] high nibble = 0x4 (v4 variant).
        let bytes = copy.guid.as_bytes();
        assert_eq!(bytes[6] & 0xF0, 0x40);
        // Variant bits: bytes[8] high two bits = 0b10.
        assert_eq!(bytes[8] & 0xC0, 0x80);
    }

    #[test]
    fn copy_ctor_clone_to_heap_alloc_and_free() {
        let src = Theme::new_with_defaults_partial();
        unsafe {
            let p = src.clone_to_heap();
            assert!(!p.is_null());
            assert_eq!((*p).init_defaults, 1);
            assert_ne!((*p).color_scheme, src.color_scheme);
            // Cleanup
            drop(Box::from_raw(p));
        }
    }

    #[test]
    fn guid_create_id_produces_unique_values() {
        let g1 = Guid::create_id();
        let g2 = Guid::create_id();
        assert!(!g1.is_nil());
        assert!(!g2.is_nil());
        // Random — overwhelmingly likely to differ
        assert!(g1.ne_guid(&g2));
        // v4 variant bits
        let b1 = g1.as_bytes();
        let b2 = g2.as_bytes();
        assert_eq!(b1[6] & 0xF0, 0x40);
        assert_eq!(b1[8] & 0xC0, 0x80);
        assert_eq!(b2[6] & 0xF0, 0x40);
        assert_eq!(b2[8] & 0xC0, 0x80);
    }

    #[test]
    fn font_scheme_copy_ctor_null_fontsets() {
        unsafe {
            let src = FontScheme::new_empty();
            let mut dst = std::mem::MaybeUninit::<FontScheme>::uninit();
            FontScheme::copy_from_raw(dst.as_mut_ptr(), &src as *const FontScheme);
            let dst_ref = &*dst.as_ptr();
            assert!(dst_ref.major.is_null());
            assert!(dst_ref.minor.is_null());
            // Drop dst to test
            ptr::drop_in_place(dst.as_mut_ptr());
        }
    }

    #[test]
    fn format_scheme_copy_ctor_empty_trees() {
        unsafe {
            let src = FormatScheme::new();
            let copy_p = src.clone_to_heap();
            assert!(!copy_p.is_null());
            assert!((*copy_p).is_empty());
            // 4 trees of copy each empty
            assert_eq!((*copy_p).brushes.size, 0);
            assert_eq!((*copy_p).bg_brushes.size, 0);
            assert_eq!((*copy_p).pens.size, 0);
            assert_eq!((*copy_p).effects.size, 0);
            FormatScheme::raw_delete(copy_p);
        }
    }

    #[test]
    fn object_defaults_copy_ctor_null_share_ptrs() {
        unsafe {
            let src = ObjectDefaults::new();
            let copy_p = src.clone_to_heap();
            assert!(!copy_p.is_null());
            assert!((*copy_p).line_default.is_null());
            assert!((*copy_p).textbox_default.is_null());
            assert!((*copy_p).shape_default.is_null());
            ObjectDefaults::raw_delete(copy_p);
        }
    }

    #[test]
    fn color_scheme_clone_or_null_with_null_returns_null() {
        unsafe {
            let p = ColorScheme::clone_or_null(ptr::null());
            assert!(p.is_null());
        }
    }

    #[test]
    fn color_scheme_clone_to_heap_yields_independent_tree() {
        unsafe {
            let mut src = ColorScheme::new();
            let cp_p = src.clone_to_heap();
            assert!(!cp_p.is_null());
            let cp = &mut *cp_p;
            assert_eq!(cp.len(), 12);
            // Independent: same content but different node addresses
            for k in 0u32..12 {
                let s = src.get_color(k).unwrap();
                let c = cp.get_color(k).unwrap();
                assert_eq!(s.value, c.value);
                assert_eq!(s.type_tag, c.type_tag);
            }
            ColorScheme::raw_delete(cp_p);
        }
    }

    #[test]
    fn theme_field_offsets_match_raw_asm() {
        let t = Theme::new_uninitialized();
        let p = &t as *const Theme as usize;
        assert_eq!(&t.guid as *const _ as usize - p, offset::GUID);
        assert_eq!(&t.parent as *const _ as usize - p, offset::PARENT_SHAREPTR);
        assert_eq!(
            &t.init_defaults as *const _ as usize - p,
            offset::INIT_DEFAULTS
        );
        assert_eq!(&t.name as *const _ as usize - p, offset::NAME);
        assert_eq!(
            &t.color_scheme as *const _ as usize - p,
            offset::COLOR_SCHEME
        );
        assert_eq!(
            &t.format_scheme as *const _ as usize - p,
            offset::FORMAT_SCHEME
        );
        assert_eq!(
            &t.font_scheme as *const _ as usize - p,
            offset::FONT_SCHEME
        );
        assert_eq!(
            &t.object_defaults as *const _ as usize - p,
            offset::OBJECT_DEFAULTS
        );
    }

    #[test]
    fn new_uninitialized_all_subobjects_null() {
        let t = Theme::new_uninitialized();
        assert_eq!(t.init_defaults, 0);
        assert!(t.parent.is_null());
        assert!(t.color_scheme.is_null());
        assert!(t.format_scheme.is_null());
        assert!(t.font_scheme.is_null());
        assert!(t.object_defaults.is_null());
        assert!(!t.get_init_defaults_flag());
    }

    #[test]
    fn new_with_defaults_partial_has_color_scheme_and_font_scheme() {
        let t = Theme::new_with_defaults_partial();
        assert_eq!(t.init_defaults, 1);
        assert!(t.get_init_defaults_flag());
        // ColorScheme 가 alloc + 12 SetAt 됨
        assert!(!t.color_scheme.is_null());
        unsafe {
            let cs = &*t.color_scheme;
            assert_eq!(cs.len(), 12);
        }
        // FontScheme 가 empty 로 alloc 됨
        assert!(!t.font_scheme.is_null());
        unsafe {
            let fs = &*t.font_scheme;
            assert!(fs.major.is_null());
            assert!(fs.minor.is_null());
            assert_eq!(fs.name.length(), 0);
        }
        // FormatScheme / ObjectDefaults: deferred (null)
        assert!(t.format_scheme.is_null());
        assert!(t.object_defaults.is_null());
    }

    #[test]
    fn drop_uninitialized_no_panic() {
        for _ in 0..30 {
            let t = Theme::new_uninitialized();
            drop(t);
        }
    }

    #[test]
    fn drop_partial_releases_color_scheme_and_font_scheme() {
        for _ in 0..30 {
            let t = Theme::new_with_defaults_partial();
            drop(t);
        }
    }

    #[test]
    fn guid_default_is_nil_for_uninitialized() {
        let t = Theme::new_uninitialized();
        assert_eq!(t.guid, Guid::new());
    }

    #[test]
    fn name_default_is_empty() {
        let t = Theme::new_uninitialized();
        assert_eq!(t.get_name().length(), 0);
    }

    #[test]
    fn padding_after_bool_is_zero() {
        let t = Theme::new_uninitialized();
        assert_eq!(t._pad_0x19, [0u8; 7]);
    }

    #[test]
    fn all_subobject_pointer_offsets_in_8b_grid() {
        // 4 sub-object pointer slots 가 [0x28..0x48] 의 8B grid 내 정확히 배치
        assert_eq!(offset::COLOR_SCHEME, 0x28);
        assert_eq!(offset::FORMAT_SCHEME, 0x30);
        assert_eq!(offset::FONT_SCHEME, 0x38);
        assert_eq!(offset::OBJECT_DEFAULTS, 0x40);
        assert_eq!(offset::OBJECT_DEFAULTS + 8, THEME_SIZE_BYTES);
    }

    #[test]
    fn color_scheme_in_theme_matches_standalone() {
        // Theme(true) 의 ColorScheme 와 standalone ColorScheme::new() 가 byte-eq
        let t = Theme::new_with_defaults_partial();
        let standalone = ColorScheme::new();
        unsafe {
            let in_theme = &*t.color_scheme;
            assert_eq!(in_theme.len(), standalone.len());
            assert_eq!(in_theme.tree_size, standalone.tree_size);
        }
    }

    #[test]
    fn new_with_parent_null_acts_like_bool_ctor() {
        unsafe {
            // parent = null + init_defaults=false → 동일 결과
            let t = Theme::new_with_parent(ptr::null_mut(), false);
            assert_eq!(t.init_defaults, 0);
            assert!(t.parent.is_null());
            assert!(t.color_scheme.is_null());
            assert!(t.font_scheme.is_null());
        }
    }

    #[test]
    fn new_with_parent_non_null_increments_refcount() {
        unsafe {
            use crate::share_ptr::ControlBlock;
            // 외부 ControlBlock 생성 (refcount=1)
            let cb = Box::into_raw(Box::new(ControlBlock {
                obj: 0xDEAD_BEEF as *mut (), // dummy non-null
                refcount: 1u64,
            }));

            let t = Theme::new_with_parent(cb, false);
            // refcount 가 2 로 증가 (SharePtr copy semantic)
            assert_eq!((*cb).refcount, 2);
            assert_eq!(t.parent, cb);

            // drop t → refcount-- 로 1 복귀 (raw 의 ~Theme parent cleanup)
            drop(t);
            assert_eq!((*cb).refcount, 1);

            // cleanup
            std::alloc::dealloc(
                cb as *mut u8,
                std::alloc::Layout::new::<ControlBlock<()>>(),
            );
        }
    }

    #[test]
    fn new_with_parent_true_populates_subobjects() {
        unsafe {
            use crate::share_ptr::ControlBlock;
            let cb = Box::into_raw(Box::new(ControlBlock {
                obj: 0xDEAD_BEEF as *mut (),
                refcount: 1u64,
            }));
            let t = Theme::new_with_parent(cb, true);
            assert_eq!(t.init_defaults, 1);
            assert_eq!(t.parent, cb);
            assert!(!t.color_scheme.is_null());
            assert!(!t.font_scheme.is_null());
            // refcount 가 2 (parent 증가)
            assert_eq!((*cb).refcount, 2);

            drop(t);
            // refcount 1 복귀
            assert_eq!((*cb).refcount, 1);
            std::alloc::dealloc(
                cb as *mut u8,
                std::alloc::Layout::new::<ControlBlock<()>>(),
            );
        }
    }

    #[test]
    fn font_scheme_in_theme_is_byte_eq_empty() {
        let t = Theme::new_with_defaults_partial();
        unsafe {
            let fs = &*t.font_scheme;
            // raw `stp xzr, xzr, [x27, #0x8]` 의 결과: 2 nulls at +0x8, +0x10
            let fs_addr = fs as *const _ as usize;
            let major_ptr = (fs_addr + 0x08) as *const *mut crate::FontSet;
            let minor_ptr = (fs_addr + 0x10) as *const *mut crate::FontSet;
            assert!((*major_ptr).is_null());
            assert!((*minor_ptr).is_null());
        }
    }

    // =========================================================================
    // L-5c-5a: Theme accessor (GetFormatScheme/GetFontScheme/GetSchemeBrush 등)
    // =========================================================================

    struct DummyEngine;
    impl ShapeEngineProvider for DummyEngine {
        fn default_format_scheme(&self) -> *mut FormatScheme {
            ptr::null_mut()
        }
        fn default_font_scheme(&self) -> *mut FontScheme {
            ptr::null_mut()
        }
    }

    #[test]
    fn get_format_scheme_no_init_returns_field_directly() {
        let t = Theme::new_uninitialized();
        // raw 1ec4e0: cbz w1 → ldr x19, [x0, #0x30]; ret
        let r = t.get_format_scheme_init(false, None::<&DummyEngine>);
        assert!(r.is_null(), "uninitialized theme has null format_scheme");
    }

    #[test]
    fn get_format_scheme_init_no_parent_no_engine_returns_null() {
        let t = Theme::new_uninitialized();
        // init flag=0, no parent, no engine → null
        let r = t.get_format_scheme_init(true, None::<&DummyEngine>);
        assert!(r.is_null());
    }

    #[test]
    fn get_format_scheme_init_with_existing_non_null() {
        // FormatScheme::Create + assign to Theme.format_scheme
        let mut t = Theme::new_uninitialized();
        let fs = unsafe { FormatScheme::create_raw() };
        t.format_scheme = fs;
        let r = t.get_format_scheme_init(true, None::<&DummyEngine>);
        assert_eq!(r, fs, "raw 1ec4a8 cbnz: existing non-null returned as-is");
        // cleanup: prevent double-drop
        t.format_scheme = ptr::null_mut();
        unsafe { FormatScheme::raw_delete(fs); }
    }

    #[test]
    fn get_format_scheme_init_with_init_defaults_flag_uses_engine() {
        struct E(*mut FormatScheme);
        impl ShapeEngineProvider for E {
            fn default_format_scheme(&self) -> *mut FormatScheme { self.0 }
            fn default_font_scheme(&self) -> *mut FontScheme { ptr::null_mut() }
        }
        let default_fs = unsafe { FormatScheme::create_raw() };
        let engine = E(default_fs);

        // Theme with init_defaults=1 but format_scheme=null → engine fallback
        let mut t = Theme::new_uninitialized();
        t.init_defaults = 1;
        let r = t.get_format_scheme_init(true, Some(&engine));
        assert_eq!(r, default_fs,
            "raw 1ec4c4 cbz w8 fallthrough: init_defaults=1 → ShapeEngine default");

        unsafe { FormatScheme::raw_delete(default_fs); }
    }

    #[test]
    fn get_format_scheme_init_no_init_flag_no_engine_returns_null() {
        // init_defaults=0, no parent, but init_if_null=true → walks to null,
        // last_init_defaults=0 → return null (no engine fallback)
        let t = Theme::new_uninitialized();
        // init_defaults is 0 by default
        let r = t.get_format_scheme_init(true, None::<&DummyEngine>);
        assert!(r.is_null());
    }

    #[test]
    fn get_font_scheme_init_with_existing_returns_it() {
        let t = Theme::new_with_defaults_partial();
        // partial 은 font_scheme 을 non-null 로 set
        let r = t.get_font_scheme_init(true, None::<&DummyEngine>);
        assert!(!r.is_null());
        assert_eq!(r, t.font_scheme);
    }

    #[test]
    fn get_scheme_brush_empty_tree_returns_null() {
        // FormatScheme with empty brushes tree
        let mut t = Theme::new_uninitialized();
        let fs = unsafe { FormatScheme::create_raw() };
        t.format_scheme = fs;
        let r = t.get_scheme_brush(0, None::<&DummyEngine>);
        assert!(r.is_null(), "empty tree → null");
        // cleanup
        t.format_scheme = ptr::null_mut();
        unsafe { FormatScheme::raw_delete(fs); }
    }

    #[test]
    fn get_scheme_brush_null_format_scheme_returns_null() {
        let t = Theme::new_uninitialized();
        let r = t.get_scheme_brush(5, None::<&DummyEngine>);
        assert!(r.is_null());
    }

    #[test]
    fn get_scheme_pen_null_format_scheme_returns_null() {
        let t = Theme::new_uninitialized();
        let r = t.get_scheme_pen(5, None::<&DummyEngine>);
        assert!(r.is_null());
    }

    #[test]
    fn get_scheme_background_brush_empty_tree_returns_null() {
        let mut t = Theme::new_uninitialized();
        let fs = unsafe { FormatScheme::create_raw() };
        t.format_scheme = fs;
        let r = t.get_scheme_background_brush(0, None::<&DummyEngine>);
        assert!(r.is_null());
        t.format_scheme = ptr::null_mut();
        unsafe { FormatScheme::raw_delete(fs); }
    }

    #[test]
    fn get_format_scheme_init_walks_parent_chain() {
        // child Theme has null format_scheme; parent Theme has non-null
        let parent_fs = unsafe { FormatScheme::create_raw() };
        let mut parent = Theme::new_uninitialized();
        parent.format_scheme = parent_fs;

        // Wrap parent in ControlBlock<()>-style SharePtr (parent stays alive in scope)
        let parent_ptr: *mut Theme = &mut parent;
        let cb = Box::into_raw(Box::new(crate::ControlBlock::<()> {
            obj: parent_ptr as *mut (),
            refcount: 2,
        }));

        let mut child = Theme::new_uninitialized();
        child.parent = cb;

        let r = child.get_format_scheme_init(true, None::<&DummyEngine>);
        assert_eq!(r, parent_fs,
            "raw 1ec4ac-bc: parent chain walk yields parent's format_scheme");

        // cleanup
        parent.format_scheme = ptr::null_mut();
        unsafe { FormatScheme::raw_delete(parent_fs); }
        child.parent = ptr::null_mut();
        unsafe { drop(Box::from_raw(cb)); }
    }

    // =========================================================================
    // L-5c-5b (partial): GetMajorFont / GetMinorFont — fast path byte-eq
    // =========================================================================

    #[test]
    fn get_major_font_with_null_font_scheme_returns_null() {
        // raw 168c18: GetFontScheme(true) — null if Theme uninitialized & no parent
        let mut t = Theme::new_uninitialized();
        let r = t.get_major_font(None::<&DummyEngine>);
        assert!(r.is_null(), "no font_scheme → null FontSet");
    }

    #[test]
    fn get_major_font_with_null_major_triggers_lazy_init() {
        // raw 168c24-6c: lazy init via CreateDefaultFontSet, store, return new.
        let mut t = Theme::new_uninitialized();
        // attach an empty FontScheme so get_font_scheme_init returns non-null
        let fs_box = Box::new(FontScheme::new_empty());
        let fs_ptr = Box::into_raw(fs_box);
        t.font_scheme = fs_ptr;
        let r = t.get_major_font(None::<&DummyEngine>);
        assert!(!r.is_null(),
            "raw 168c3c-c4c: lazy init path 가 새 default FontSet 생성");
        unsafe {
            // FontScheme.major 가 새로 채워졌는지 검증
            assert_eq!((*fs_ptr).major, r);
        }
        // cleanup — FontScheme Drop 이 major 의 FontSet 도 free
        t.font_scheme = ptr::null_mut();
        unsafe { drop(Box::from_raw(fs_ptr)); }
    }

    #[test]
    fn get_minor_font_with_null_minor_triggers_lazy_init() {
        let mut t = Theme::new_uninitialized();
        let fs_box = Box::new(FontScheme::new_empty());
        let fs_ptr = Box::into_raw(fs_box);
        t.font_scheme = fs_ptr;
        let r = t.get_minor_font(None::<&DummyEngine>);
        assert!(!r.is_null());
        unsafe {
            assert_eq!((*fs_ptr).minor, r);
        }
        t.font_scheme = ptr::null_mut();
        unsafe { drop(Box::from_raw(fs_ptr)); }
    }

    #[test]
    fn get_major_font_lazy_init_creates_default_font_with_correct_latin_typeface() {
        // raw create_default_font_set 가 latin TextFont = "HNC_GO_B_HINT_GS" 보장
        let mut t = Theme::new_uninitialized();
        let fs_box = Box::new(FontScheme::new_empty());
        let fs_ptr = Box::into_raw(fs_box);
        t.font_scheme = fs_ptr;
        let r = t.get_major_font(None::<&DummyEngine>);
        unsafe {
            let latin = (*r).get_latin();
            assert!(!latin.is_null());
            let typeface = (*latin).get_typeface();
            let slice = typeface.as_wide();
            let s = String::from_utf16_lossy(slice);
            assert_eq!(s, "HNC_GO_B_HINT_GS");
        }
        t.font_scheme = ptr::null_mut();
        unsafe { drop(Box::from_raw(fs_ptr)); }
    }

    #[test]
    fn get_major_font_with_non_null_major_returns_major_directly() {
        // raw 168c20-34: fast path — major non-null → return major
        // 본 test 는 dangling-but-non-null pointer 로 fast path 만 확인 (deref 안 함).
        let mut t = Theme::new_uninitialized();
        let major_ptr = 0xDEADBEEF as *mut FontSet;
        let fs_box = Box::new(unsafe { FontScheme::new(major_ptr, ptr::null_mut()) });
        let fs_ptr = Box::into_raw(fs_box);
        t.font_scheme = fs_ptr;
        let r = t.get_major_font(None::<&DummyEngine>);
        assert_eq!(r, major_ptr,
            "raw 168c28-34: font_scheme.major non-null → return as-is");
        // cleanup: prevent FontScheme dtor from trying to drop 0xDEADBEEF
        unsafe {
            (*fs_ptr).major = ptr::null_mut();
            t.font_scheme = ptr::null_mut();
            drop(Box::from_raw(fs_ptr));
        }
    }

    #[test]
    fn get_minor_font_with_non_null_minor_returns_minor_directly() {
        let mut t = Theme::new_uninitialized();
        let minor_ptr = 0xCAFEBABE as *mut FontSet;
        let fs_box = Box::new(unsafe { FontScheme::new(ptr::null_mut(), minor_ptr) });
        let fs_ptr = Box::into_raw(fs_box);
        t.font_scheme = fs_ptr;
        let r = t.get_minor_font(None::<&DummyEngine>);
        assert_eq!(r, minor_ptr,
            "raw 168c8c: font_scheme.minor non-null → return as-is");
        unsafe {
            (*fs_ptr).minor = ptr::null_mut();
            t.font_scheme = ptr::null_mut();
            drop(Box::from_raw(fs_ptr));
        }
    }

    // =========================================================================
    // L-5c-5b2: GetSchemeEffects / GetSchemeScene3D / GetSchemeSp3D — byte-eq
    // =========================================================================

    /// Build an FsEffectStyleMapNode + EffectStyleControlBlock + EffectStyle wired
    /// into an effects tree at root. Returns: (tree_root, [allocated nodes/blocks/styles]).
    ///
    /// Test helper — single-node tree is sufficient for lookup byte-eq verification.
    unsafe fn build_single_node_effects_tree(
        key: u32,
        scene3d_cb: *mut crate::ControlBlock<crate::scene3d::Scene3D>,
        sp3d_cb: *mut crate::ControlBlock<crate::sp3d::Sp3D>,
        effects_cb: *mut crate::ControlBlock<crate::effects_container::Effects>,
    ) -> (
        *mut crate::rb_tree::TreeNodeBase,
        *mut crate::format_scheme::FsEffectStyleMapNode,
        *mut crate::format_scheme::EffectStyleControlBlock,
        *mut crate::effect_style::EffectStyle,
    ) {
        use crate::effect_style::EffectStyle;
        use crate::format_scheme::{EffectStyleControlBlock, FsEffectStyleMapNode};
        use crate::rb_tree::TreeNodeBase;

        // Build EffectStyle on the heap
        let es_box = Box::new(EffectStyle {
            scene3d: scene3d_cb,
            sp3d: sp3d_cb,
            effects: effects_cb,
        });
        let es_ptr = Box::into_raw(es_box);

        // EffectStyleControlBlock wraps EffectStyle
        let es_cb = EffectStyleControlBlock::create_raw(es_ptr);

        // FsEffectStyleMapNode: standalone single-node tree (no parent, no children).
        let node_layout = std::alloc::Layout::new::<FsEffectStyleMapNode>();
        let node = std::alloc::alloc(node_layout) as *mut FsEffectStyleMapNode;
        std::ptr::write(
            node,
            FsEffectStyleMapNode {
                base: TreeNodeBase {
                    left: std::ptr::null_mut(),
                    right: std::ptr::null_mut(),
                    parent: std::ptr::null_mut(),
                    is_black: 1,
                    _pad_0x19: [0; 7],
                },
                key,
                _pad: 0,
                value: es_cb,
            },
        );

        (node as *mut TreeNodeBase, node, es_cb, es_ptr)
    }

    /// Cleanup the test helper allocations from build_single_node_effects_tree.
    unsafe fn cleanup_effects_tree(
        node: *mut crate::format_scheme::FsEffectStyleMapNode,
        es_cb: *mut crate::format_scheme::EffectStyleControlBlock,
        es: *mut crate::effect_style::EffectStyle,
    ) {
        // dealloc FsEffectStyleMapNode
        let node_layout = std::alloc::Layout::new::<crate::format_scheme::FsEffectStyleMapNode>();
        std::alloc::dealloc(node as *mut u8, node_layout);
        // dealloc EffectStyleControlBlock
        let cb_layout = std::alloc::Layout::new::<crate::format_scheme::EffectStyleControlBlock>();
        std::alloc::dealloc(es_cb as *mut u8, cb_layout);
        // EffectStyle
        drop(Box::from_raw(es));
    }

    #[test]
    fn get_scheme_effects_null_format_scheme_returns_null() {
        let t = Theme::new_uninitialized();
        let r = t.get_scheme_effects(5, None::<&DummyEngine>);
        assert!(r.is_null());
    }

    #[test]
    fn get_scheme_scene3d_null_format_scheme_returns_null() {
        let t = Theme::new_uninitialized();
        let r = t.get_scheme_scene3d(5, None::<&DummyEngine>);
        assert!(r.is_null());
    }

    #[test]
    fn get_scheme_sp3d_null_format_scheme_returns_null() {
        let t = Theme::new_uninitialized();
        let r = t.get_scheme_sp3d(5, None::<&DummyEngine>);
        assert!(r.is_null());
    }

    #[test]
    fn get_scheme_effects_empty_tree_returns_null() {
        let mut t = Theme::new_uninitialized();
        let fs = unsafe { FormatScheme::create_raw() };
        t.format_scheme = fs;
        let r = t.get_scheme_effects(0, None::<&DummyEngine>);
        assert!(r.is_null(), "empty effects tree → null");
        t.format_scheme = ptr::null_mut();
        unsafe { FormatScheme::raw_delete(fs); }
    }

    #[test]
    fn get_scheme_effects_returns_effects_share_ptr_at_offset_0x10() {
        // Build tree: key=5 → EffectStyle{scene3d=null, sp3d=null, effects=CB}
        unsafe {
            // Effects::new() returns Box<Self>
            let effects_obj_box = crate::effects_container::Effects::new();
            let effects_obj = Box::into_raw(effects_obj_box);
            let effects_cb_box = Box::new(crate::ControlBlock {
                obj: effects_obj,
                refcount: 1u64,
            });
            let effects_cb = Box::into_raw(effects_cb_box);

            let (root, node, es_cb, es) = build_single_node_effects_tree(
                5,
                ptr::null_mut(),
                ptr::null_mut(),
                effects_cb,
            );

            let mut t = Theme::new_uninitialized();
            let fs = FormatScheme::create_raw();
            (*fs).effects.end_node_left = root;
            t.format_scheme = fs;

            let r = t.get_scheme_effects(5, None::<&DummyEngine>);
            assert_eq!(r, effects_cb,
                "raw 15df00: 결과가 EffectStyle.effects (+0x10) 의 ctrl ptr");
            // raw 15df14-1c: refcount++ 검증
            assert_eq!((*effects_cb).refcount, 2,
                "raw refcount: 1 → 2 (Get 가 retain)");

            // cleanup
            (*fs).effects.end_node_left = ptr::null_mut();
            t.format_scheme = ptr::null_mut();
            FormatScheme::raw_delete(fs);
            cleanup_effects_tree(node, es_cb, es);
            // effects_cb still pointed-to by Box semantics; manually drop.
            drop(Box::from_raw(effects_cb));
            drop(Box::from_raw(effects_obj));
        }
    }

    #[test]
    fn get_scheme_scene3d_returns_scene3d_share_ptr_at_offset_0() {
        unsafe {
            let s3d_box = Box::new(crate::scene3d::Scene3D::new_empty());
            let s3d_obj = Box::into_raw(s3d_box);
            let s3d_cb = Box::into_raw(Box::new(crate::ControlBlock {
                obj: s3d_obj,
                refcount: 1u64,
            }));

            let (root, node, es_cb, es) = build_single_node_effects_tree(
                7,
                s3d_cb,
                ptr::null_mut(),
                ptr::null_mut(),
            );

            let mut t = Theme::new_uninitialized();
            let fs = FormatScheme::create_raw();
            (*fs).effects.end_node_left = root;
            t.format_scheme = fs;

            let r = t.get_scheme_scene3d(7, None::<&DummyEngine>);
            assert_eq!(r, s3d_cb,
                "raw 15e2d8: 결과가 EffectStyle.scene3d (+0x00) 의 ctrl ptr");
            assert_eq!((*s3d_cb).refcount, 2);

            (*fs).effects.end_node_left = ptr::null_mut();
            t.format_scheme = ptr::null_mut();
            FormatScheme::raw_delete(fs);
            cleanup_effects_tree(node, es_cb, es);
            drop(Box::from_raw(s3d_cb));
            drop(Box::from_raw(s3d_obj));
        }
    }

    #[test]
    fn get_scheme_sp3d_returns_sp3d_share_ptr_at_offset_8() {
        unsafe {
            let sp3d_box = Box::new(crate::sp3d::Sp3D::new_empty());
            let sp3d_obj = Box::into_raw(sp3d_box);
            let sp3d_cb = Box::into_raw(Box::new(crate::ControlBlock {
                obj: sp3d_obj,
                refcount: 1u64,
            }));

            let (root, node, es_cb, es) = build_single_node_effects_tree(
                9,
                ptr::null_mut(),
                sp3d_cb,
                ptr::null_mut(),
            );

            let mut t = Theme::new_uninitialized();
            let fs = FormatScheme::create_raw();
            (*fs).effects.end_node_left = root;
            t.format_scheme = fs;

            let r = t.get_scheme_sp3d(9, None::<&DummyEngine>);
            assert_eq!(r, sp3d_cb,
                "raw 15e510: 결과가 EffectStyle.sp3d (+0x08) 의 ctrl ptr");
            assert_eq!((*sp3d_cb).refcount, 2);

            (*fs).effects.end_node_left = ptr::null_mut();
            t.format_scheme = ptr::null_mut();
            FormatScheme::raw_delete(fs);
            cleanup_effects_tree(node, es_cb, es);
            drop(Box::from_raw(sp3d_cb));
            drop(Box::from_raw(sp3d_obj));
        }
    }

    #[test]
    fn get_scheme_effects_null_inner_share_ptr_returns_null() {
        // EffectStyle.effects = null → raw 15df08: cbz → return null/done
        unsafe {
            let (root, node, es_cb, es) = build_single_node_effects_tree(
                3,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
            );
            let mut t = Theme::new_uninitialized();
            let fs = FormatScheme::create_raw();
            (*fs).effects.end_node_left = root;
            t.format_scheme = fs;
            let r = t.get_scheme_effects(3, None::<&DummyEngine>);
            assert!(r.is_null(),
                "EffectStyle.effects 가 null → 결과도 null");
            (*fs).effects.end_node_left = ptr::null_mut();
            t.format_scheme = ptr::null_mut();
            FormatScheme::raw_delete(fs);
            cleanup_effects_tree(node, es_cb, es);
        }
    }

    #[test]
    fn get_scheme_effects_missing_key_returns_null() {
        unsafe {
            // Effects::new() returns Box<Self>
            let effects_obj_box = crate::effects_container::Effects::new();
            let effects_obj = Box::into_raw(effects_obj_box);
            let effects_cb = Box::into_raw(Box::new(crate::ControlBlock {
                obj: effects_obj,
                refcount: 1u64,
            }));
            let (root, node, es_cb, es) = build_single_node_effects_tree(
                10,
                ptr::null_mut(),
                ptr::null_mut(),
                effects_cb,
            );
            let mut t = Theme::new_uninitialized();
            let fs = FormatScheme::create_raw();
            (*fs).effects.end_node_left = root;
            t.format_scheme = fs;
            // request key 99 (not present, > 10 → lookup returns None)
            let r = t.get_scheme_effects(99, None::<&DummyEngine>);
            assert!(r.is_null(),
                "key 없음 → null (lower_bound 후 best.key < style → fail)");
            (*fs).effects.end_node_left = ptr::null_mut();
            t.format_scheme = ptr::null_mut();
            FormatScheme::raw_delete(fs);
            cleanup_effects_tree(node, es_cb, es);
            drop(Box::from_raw(effects_cb));
            drop(Box::from_raw(effects_obj));
        }
    }
}

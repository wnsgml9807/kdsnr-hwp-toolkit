//! `Hnc::Shape::Text::CharItemView` 의 TTF glyph path 빌드 helpers
//! (`FUN_0x07ae80` + `FUN_0x07b254` + CGPath release) outer port (L-5c-RE-5b2-ttf).
//!
//! ## raw 출처 (libHncDrawingEngine.dylib arm64)
//!
//! - `FUN_0x07ae80` — TTF/CoreText glyph path 빌더. ~800B.
//!   - 시그니처: `(font_size_px: i32, default_path_ctrl: *mut ControlBlock,
//!     character: u16, glue_const: i32, stringw_ptr: *u8, font_id: u32,
//!     anchor: *Point) → CGPathRef`
//!   - 호출 5단계 CTFont fallback chain:
//!     1. `_CTFontCreateWithName(postscript_name, size, NULL)` — primary
//!     2. NULL 시 `_CTFontCreateWithName(full_name, size, NULL)` — full name
//!     3. NULL 시 `_CTFontCreateWithName(family_name, size, NULL)` — family
//!     4. NULL 시 `_CTFontCreateUIFontForLanguage(uiType=2 = Application,
//!        size, NULL)` — system default
//!     5. NULL 시 `_CTFontCreateWithName("Helvetica", size, NULL)` — hard fallback
//!   - 각 단계 NULL 검사 후 `_CTFontCreatePathForGlyph(font, glyph_id,
//!     &transform_matrix)` → CGPathRef
//!   - CGPathRef 반환 (caller 책임 release)
//! - `FUN_0x07b254` — CGPath → Render::Path 합성. ~200B.
//!   - 시그니처: `(path_inner: *mut u8, cg_path: CGPathRef, anchor: *Point)`
//!   - CGPath 의 모든 element (MoveTo/LineTo/CurveTo/QuadTo/Close) 를 traverse 하면서
//!     `Render::Path::AddMove/AddLine/AddCurve/...` 호출
//!   - anchor 좌표 만큼 평행이동
//! - `_CGPathRelease(cg_path)` — CoreGraphics release.
//!
//! ## 함수 의미 + 분리
//!
//! 1. **font_resolve_5_fallback**: 5단계 CTFont fallback chain — name → font_id 매핑
//! 2. **glyph_path_extract**: CTFont + glyph_id → CGPath 추출
//! 3. **cgpath_to_render_path**: CGPath 의 element traversal + Render::Path 추가
//! 4. **release**: CGPath cleanup
//!
//! ## 본 port scope (L-5c-RE-5b2-ttf)
//!
//! - ✅ Outer control flow (5단계 fallback + path 합성 + release sequence) byte-eq
//! - ✅ Anchor 좌표 합산 + transform matrix 셋업
//! - ⏸️ macOS CoreText 실제 호출은 trait callback (`TtfPathBackend`) 으로 위임
//!       (cross-platform: `ab_glyph` / `rusttype` 또는 system CoreText 모두 같은 trait 구현)
//! - ⏸️ font name → font_id 매핑 자체는 caller (kdsnr-hft 의 sister module) 책임

use crate::share_ptr::ControlBlock;

/// CoreText CTFont opaque handle (raw `CTFontRef = ^CTFont`).
///
/// 본 port 는 platform-specific 한 opaque ptr 만 다루고, 실제 layout 은 backend 가 처리.
pub type CtFontRef = *mut u8;

/// CoreGraphics CGPath opaque handle (raw `CGPathRef = ^CGPath`).
pub type CgPathRef = *mut u8;

/// raw 5단계 CTFont fallback 시도 단계.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontResolveStage {
    /// Stage 1: PostScript name
    PostScriptName,
    /// Stage 2: full name
    FullName,
    /// Stage 3: family name
    FamilyName,
    /// Stage 4: system default UI font (uiType=2 Application)
    SystemDefault,
    /// Stage 5: "Helvetica" hard fallback
    HardFallback,
}

impl FontResolveStage {
    pub fn next(self) -> Option<Self> {
        match self {
            FontResolveStage::PostScriptName => Some(FontResolveStage::FullName),
            FontResolveStage::FullName => Some(FontResolveStage::FamilyName),
            FontResolveStage::FamilyName => Some(FontResolveStage::SystemDefault),
            FontResolveStage::SystemDefault => Some(FontResolveStage::HardFallback),
            FontResolveStage::HardFallback => None,
        }
    }
}

/// `FUN_0x07ae80` outer port 의 결과.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BuildGlyphPathOutcome {
    /// 어느 fallback stage 든 font 가 생성되었고 CGPath 도 성공.
    Ok {
        stage: FontResolveStage,
        cg_path: CgPathRef,
    },
    /// 5단계 모두 font 가 NULL → 결과 null CGPath
    AllFontResolveFailed,
    /// font 는 있지만 _CTFontCreatePathForGlyph 가 NULL 반환 (glyph 없음)
    GlyphNotFound { stage: FontResolveStage },
}

/// TTF backend dependency — actual CoreText calls (or stub for tests / cross-platform).
pub trait TtfPathBackend {
    /// raw `_CTFontCreateWithName(name, size, NULL)` 호출.
    /// `name` 은 byte slice (UTF-16LE wchar from raw stringw_ptr).
    /// font 없으면 NULL 반환.
    unsafe fn ct_font_create_with_name(
        &mut self,
        name: &[u16],
        size: f32,
    ) -> CtFontRef;

    /// raw `_CTFontCreateUIFontForLanguage(uiType=2 (Application), size, NULL)`.
    unsafe fn ct_font_create_ui_application(&mut self, size: f32) -> CtFontRef;

    /// raw `_CTFontCreatePathForGlyph(font, glyph_id, &transform_matrix)`.
    /// transform 은 anchor translate; backend 가 매개변수로 받아서 합성.
    /// glyph 없거나 path empty 면 NULL 반환.
    unsafe fn ct_font_create_path_for_glyph(
        &mut self,
        font: CtFontRef,
        glyph_id: u32,
        anchor_x: f32,
        anchor_y: f32,
    ) -> CgPathRef;

    /// raw `_CFRelease(font)` cleanup.
    unsafe fn ct_font_release(&mut self, font: CtFontRef);

    /// font name lookup callback — font_id → (postscript_name, full_name, family_name)
    /// 각 byte slice 반환. raw 의 stringw_ptr 가 이미 postscript 인지 etc 는 caller 가
    /// 분기하지만, 본 callback 으로 3종 모두 받음 (Stage 1-3).
    /// none 항목이 빈 slice 면 그 stage skip.
    unsafe fn font_name_lookup(&mut self, font_id: u32) -> FontNameTriple;

    /// `FUN_0x07b254` outer 의 backend — CGPath 의 element 를 traverse 하면서
    /// `Render::Path` 에 합성. caller 는 anchor offset 도 backend 에 전달.
    unsafe fn append_cgpath_to_render_path(
        &mut self,
        path_inner: *mut u8,
        cg_path: CgPathRef,
        anchor_x: f32,
        anchor_y: f32,
    );

    /// raw `_CGPathRelease(cg_path)` cleanup.
    unsafe fn cg_path_release(&mut self, cg_path: CgPathRef);
}

/// raw font_name_lookup 의 반환 — 3종 font name 후보.
#[derive(Debug, Default, Clone)]
pub struct FontNameTriple {
    pub postscript: Vec<u16>,
    pub full_name: Vec<u16>,
    pub family_name: Vec<u16>,
}

/// raw `FUN_0x07ae80(font_size_px, default_path_ctrl, character, glue=1, stringw_ptr,
/// font_id, anchor)` outer byte-eq port.
///
/// 흐름:
/// 1. font_name_lookup(font_id) → (PostScript, FullName, FamilyName)
/// 2. Stage 1-3: 각 name 으로 `_CTFontCreateWithName` 시도 — non-NULL 이면 break
/// 3. Stage 4: NULL 이면 `_CTFontCreateUIFontForLanguage(uiType=2 Application)`
/// 4. Stage 5: 그래도 NULL 이면 `_CTFontCreateWithName("Helvetica", ...)`
/// 5. font 잡혔으면 `_CTFontCreatePathForGlyph(font, character (glyph_id), transform)`
/// 6. font release
/// 7. CGPath 반환 (caller 가 별도 cgpath_to_render_path 호출 + release)
///
/// # Safety
/// `backend` 가 CoreText 호출에 대해 valid (또는 test/cross-platform stub).
pub unsafe fn build_glyph_path(
    font_size_px: f32,
    character: u16,
    font_id: u32,
    anchor_x: f32,
    anchor_y: f32,
    backend: &mut dyn TtfPathBackend,
) -> BuildGlyphPathOutcome {
    let names = backend.font_name_lookup(font_id);

    // 5단계 fallback sequence (raw 의 `bl _CTFontCreateWithName; cbz x0, next_stage`)
    let mut stage = FontResolveStage::PostScriptName;
    let font: CtFontRef;
    loop {
        let attempted: CtFontRef = match stage {
            FontResolveStage::PostScriptName => {
                if !names.postscript.is_empty() {
                    backend.ct_font_create_with_name(&names.postscript, font_size_px)
                } else {
                    std::ptr::null_mut()
                }
            }
            FontResolveStage::FullName => {
                if !names.full_name.is_empty() {
                    backend.ct_font_create_with_name(&names.full_name, font_size_px)
                } else {
                    std::ptr::null_mut()
                }
            }
            FontResolveStage::FamilyName => {
                if !names.family_name.is_empty() {
                    backend.ct_font_create_with_name(&names.family_name, font_size_px)
                } else {
                    std::ptr::null_mut()
                }
            }
            FontResolveStage::SystemDefault => {
                backend.ct_font_create_ui_application(font_size_px)
            }
            FontResolveStage::HardFallback => {
                // "Helvetica" UTF-16LE
                let helvetica: Vec<u16> = "Helvetica".encode_utf16().collect();
                backend.ct_font_create_with_name(&helvetica, font_size_px)
            }
        };
        if !attempted.is_null() {
            font = attempted;
            break;
        }
        match stage.next() {
            Some(s) => stage = s,
            None => {
                // 5단계 모두 실패 — raw 의 `b _exit_null_path` 동등
                return BuildGlyphPathOutcome::AllFontResolveFailed;
            }
        }
    }

    // font 잡혔음 — glyph path 추출
    // glyph_id = character (raw 는 ASCII/BMP code point 를 그대로 glyph_id 로 사용,
    // CTFont 가 내부 cmap 으로 매핑)
    let cg_path =
        backend.ct_font_create_path_for_glyph(font, character as u32, anchor_x, anchor_y);

    // font 는 더 이상 필요 없음 — release
    backend.ct_font_release(font);

    if cg_path.is_null() {
        return BuildGlyphPathOutcome::GlyphNotFound { stage };
    }

    BuildGlyphPathOutcome::Ok { stage, cg_path }
}

/// raw `FUN_0x07b254(path_inner, cg_path, anchor)` outer — wrapper.
///
/// CGPath traversal 은 backend (`TtfPathBackend::append_cgpath_to_render_path`) 가 처리.
/// 본 outer 는 anchor 적용 sequence 만 byte-eq.
///
/// # Safety
/// `path_inner` 는 valid `Render::Path` 내부 포인터. `cg_path` 는 valid CGPathRef.
pub unsafe fn append_cgpath_to_path(
    path_inner: *mut u8,
    cg_path: CgPathRef,
    anchor_x: f32,
    anchor_y: f32,
    backend: &mut dyn TtfPathBackend,
) {
    if path_inner.is_null() || cg_path.is_null() {
        return;
    }
    backend.append_cgpath_to_render_path(path_inner, cg_path, anchor_x, anchor_y);
}

/// raw `_CGPathRelease(cg_path)` wrapper.
pub unsafe fn release_cg_path(cg_path: CgPathRef, backend: &mut dyn TtfPathBackend) {
    if cg_path.is_null() {
        return;
    }
    backend.cg_path_release(cg_path);
}

/// 통합 helper: `FUN_0x07ae80` + `FUN_0x07b254` + release 를 한 번에 처리.
///
/// caller 가 default_path_ctrl 에 cgpath 의 결과 path 를 합성하고 정리.
/// `GetCachedRenderPath` 의 slow path 와 동일 sequence.
pub unsafe fn build_and_append_glyph(
    font_size_px: f32,
    character: u16,
    font_id: u32,
    anchor_x: f32,
    anchor_y: f32,
    default_path_inner: *mut u8,
    backend: &mut dyn TtfPathBackend,
) -> BuildGlyphPathOutcome {
    let outcome = build_glyph_path(
        font_size_px,
        character,
        font_id,
        anchor_x,
        anchor_y,
        backend,
    );
    match outcome {
        BuildGlyphPathOutcome::Ok { cg_path, .. } => {
            append_cgpath_to_path(default_path_inner, cg_path, anchor_x, anchor_y, backend);
            release_cg_path(cg_path, backend);
        }
        BuildGlyphPathOutcome::AllFontResolveFailed
        | BuildGlyphPathOutcome::GlyphNotFound { .. } => {}
    }
    outcome
}

/// `GetCachedRenderPath` 의 callback bridge — `TtfPathBackend` 을 그 자리 trait 으로
/// 노출하는 helper. caller 는 `GetCachedRenderPathDeps::build_glyph_path` 에서
/// `build_glyph_path` 를 직접 호출하면 됨.
pub struct CgPathHandle(pub CgPathRef);

impl CgPathHandle {
    pub fn is_null(&self) -> bool {
        self.0.is_null()
    }
    pub fn as_raw(&self) -> CgPathRef {
        self.0
    }
}

/// raw `ControlBlock<Path>` re-export (for caller convenience).
pub type PathControlBlock = ControlBlock<u8>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    struct StubBackend {
        // 각 stage 가 NULL 을 반환할지 (true = NULL, false = pseudo-handle)
        stages_return_null: [bool; 5],
        path_for_glyph_returns_null: bool,
        // 호출 trace
        calls: RefCell<Vec<String>>,
        // font name lookup 결과
        names: FontNameTriple,
        // anchor 가 backend 에 전달된 값
        last_anchor: RefCell<(f32, f32)>,
    }
    impl StubBackend {
        fn new() -> Self {
            Self {
                stages_return_null: [false; 5],
                path_for_glyph_returns_null: false,
                calls: RefCell::new(Vec::new()),
                names: FontNameTriple {
                    postscript: "Arial-Bold".encode_utf16().collect(),
                    full_name: "Arial Bold".encode_utf16().collect(),
                    family_name: "Arial".encode_utf16().collect(),
                },
                last_anchor: RefCell::new((0.0, 0.0)),
            }
        }
        fn make_null_until(&mut self, stages: &[FontResolveStage]) {
            for s in stages {
                let idx = match s {
                    FontResolveStage::PostScriptName => 0,
                    FontResolveStage::FullName => 1,
                    FontResolveStage::FamilyName => 2,
                    FontResolveStage::SystemDefault => 3,
                    FontResolveStage::HardFallback => 4,
                };
                self.stages_return_null[idx] = true;
            }
        }
        fn pseudo_font(&self) -> CtFontRef {
            0x1234_usize as *mut u8
        }
        fn pseudo_cgpath(&self) -> CgPathRef {
            0x5678_usize as *mut u8
        }
    }
    impl TtfPathBackend for StubBackend {
        unsafe fn ct_font_create_with_name(
            &mut self,
            name: &[u16],
            _size: f32,
        ) -> CtFontRef {
            let n: String = String::from_utf16_lossy(name);
            self.calls.borrow_mut().push(format!("create_with_name({})", n));
            // determine stage by checking name match
            let idx = if name == self.names.postscript.as_slice() {
                0
            } else if name == self.names.full_name.as_slice() {
                1
            } else if name == self.names.family_name.as_slice() {
                2
            } else {
                // hard fallback "Helvetica"
                4
            };
            if self.stages_return_null[idx] {
                std::ptr::null_mut()
            } else {
                self.pseudo_font()
            }
        }
        unsafe fn ct_font_create_ui_application(&mut self, _size: f32) -> CtFontRef {
            self.calls.borrow_mut().push("ui_application".to_string());
            if self.stages_return_null[3] {
                std::ptr::null_mut()
            } else {
                self.pseudo_font()
            }
        }
        unsafe fn ct_font_create_path_for_glyph(
            &mut self,
            _font: CtFontRef,
            glyph_id: u32,
            anchor_x: f32,
            anchor_y: f32,
        ) -> CgPathRef {
            self.calls.borrow_mut().push(format!("path_for_glyph({})", glyph_id));
            *self.last_anchor.borrow_mut() = (anchor_x, anchor_y);
            if self.path_for_glyph_returns_null {
                std::ptr::null_mut()
            } else {
                self.pseudo_cgpath()
            }
        }
        unsafe fn ct_font_release(&mut self, _font: CtFontRef) {
            self.calls.borrow_mut().push("font_release".to_string());
        }
        unsafe fn font_name_lookup(&mut self, _font_id: u32) -> FontNameTriple {
            self.names.clone()
        }
        unsafe fn append_cgpath_to_render_path(
            &mut self,
            _: *mut u8,
            _: CgPathRef,
            ax: f32,
            ay: f32,
        ) {
            self.calls.borrow_mut().push(format!("append({},{})", ax, ay));
        }
        unsafe fn cg_path_release(&mut self, _: CgPathRef) {
            self.calls.borrow_mut().push("cgpath_release".to_string());
        }
    }

    #[test]
    fn font_resolve_stage_progression() {
        assert_eq!(
            FontResolveStage::PostScriptName.next(),
            Some(FontResolveStage::FullName)
        );
        assert_eq!(
            FontResolveStage::FullName.next(),
            Some(FontResolveStage::FamilyName)
        );
        assert_eq!(
            FontResolveStage::FamilyName.next(),
            Some(FontResolveStage::SystemDefault)
        );
        assert_eq!(
            FontResolveStage::SystemDefault.next(),
            Some(FontResolveStage::HardFallback)
        );
        assert_eq!(FontResolveStage::HardFallback.next(), None);
    }

    #[test]
    fn stage_1_postscript_success_returns_ok() {
        unsafe {
            let mut backend = StubBackend::new();
            let r = build_glyph_path(12.0, 0x0041, 1, 0.0, 0.0, &mut backend);
            match r {
                BuildGlyphPathOutcome::Ok { stage, cg_path } => {
                    assert_eq!(stage, FontResolveStage::PostScriptName);
                    assert!(!cg_path.is_null());
                }
                _ => panic!("expected Ok stage 1, got {:?}", r),
            }
            // Verify only 1 create_with_name call (PostScript), no fallback
            let calls = backend.calls.borrow();
            assert_eq!(
                calls.iter().filter(|c| c.starts_with("create_with_name")).count(),
                1
            );
            assert!(calls.iter().any(|c| c == "font_release"));
        }
    }

    #[test]
    fn stage_2_fullname_fallback_after_postscript_null() {
        unsafe {
            let mut backend = StubBackend::new();
            backend.make_null_until(&[FontResolveStage::PostScriptName]);
            let r = build_glyph_path(12.0, 0x0041, 1, 0.0, 0.0, &mut backend);
            match r {
                BuildGlyphPathOutcome::Ok { stage, .. } => {
                    assert_eq!(stage, FontResolveStage::FullName);
                }
                _ => panic!("expected Ok stage 2"),
            }
        }
    }

    #[test]
    fn stage_4_system_default_when_first_3_null() {
        unsafe {
            let mut backend = StubBackend::new();
            backend.make_null_until(&[
                FontResolveStage::PostScriptName,
                FontResolveStage::FullName,
                FontResolveStage::FamilyName,
            ]);
            let r = build_glyph_path(12.0, 0x0041, 1, 0.0, 0.0, &mut backend);
            match r {
                BuildGlyphPathOutcome::Ok { stage, .. } => {
                    assert_eq!(stage, FontResolveStage::SystemDefault);
                }
                _ => panic!("expected Ok stage 4"),
            }
            // Verify ui_application was called
            assert!(backend.calls.borrow().iter().any(|c| c == "ui_application"));
        }
    }

    #[test]
    fn stage_5_hard_fallback_helvetica_when_first_4_null() {
        unsafe {
            let mut backend = StubBackend::new();
            backend.make_null_until(&[
                FontResolveStage::PostScriptName,
                FontResolveStage::FullName,
                FontResolveStage::FamilyName,
                FontResolveStage::SystemDefault,
            ]);
            let r = build_glyph_path(12.0, 0x0041, 1, 0.0, 0.0, &mut backend);
            match r {
                BuildGlyphPathOutcome::Ok { stage, .. } => {
                    assert_eq!(stage, FontResolveStage::HardFallback);
                }
                _ => panic!("expected Ok stage 5 (Helvetica fallback)"),
            }
            // Verify Helvetica was queried
            let calls = backend.calls.borrow();
            assert!(calls.iter().any(|c| c.contains("Helvetica")));
        }
    }

    #[test]
    fn all_5_stages_null_returns_all_font_resolve_failed() {
        unsafe {
            let mut backend = StubBackend::new();
            backend.make_null_until(&[
                FontResolveStage::PostScriptName,
                FontResolveStage::FullName,
                FontResolveStage::FamilyName,
                FontResolveStage::SystemDefault,
                FontResolveStage::HardFallback,
            ]);
            let r = build_glyph_path(12.0, 0x0041, 1, 0.0, 0.0, &mut backend);
            assert_eq!(r, BuildGlyphPathOutcome::AllFontResolveFailed);
            // No font_release because no font was created
            assert!(!backend.calls.borrow().iter().any(|c| c == "font_release"));
        }
    }

    #[test]
    fn font_ok_but_path_for_glyph_null_returns_glyph_not_found() {
        unsafe {
            let mut backend = StubBackend::new();
            backend.path_for_glyph_returns_null = true;
            let r = build_glyph_path(12.0, 0xFFFE, 1, 0.0, 0.0, &mut backend);
            match r {
                BuildGlyphPathOutcome::GlyphNotFound { stage } => {
                    assert_eq!(stage, FontResolveStage::PostScriptName);
                }
                _ => panic!("expected GlyphNotFound"),
            }
            // font_release MUST be called even when path is null (cleanup)
            assert!(backend.calls.borrow().iter().any(|c| c == "font_release"));
        }
    }

    #[test]
    fn anchor_propagates_to_path_for_glyph_call() {
        unsafe {
            let mut backend = StubBackend::new();
            let _ = build_glyph_path(12.0, 0x0041, 1, 100.5, 50.25, &mut backend);
            let (ax, ay) = *backend.last_anchor.borrow();
            assert_eq!(ax, 100.5);
            assert_eq!(ay, 50.25);
        }
    }

    #[test]
    fn empty_postscript_skips_to_fullname() {
        unsafe {
            let mut backend = StubBackend::new();
            backend.names.postscript.clear();
            let r = build_glyph_path(12.0, 0x0041, 1, 0.0, 0.0, &mut backend);
            match r {
                BuildGlyphPathOutcome::Ok { stage, .. } => {
                    // Skipped PostScript (empty) → FullName succeeded
                    assert_eq!(stage, FontResolveStage::FullName);
                }
                _ => panic!("expected Ok stage 2"),
            }
            // No create_with_name call for PostScript (empty slice = skip)
            let calls = backend.calls.borrow();
            // First create call should be for "Arial Bold" (full name)
            let first_create = calls.iter().find(|c| c.starts_with("create_with_name"));
            assert!(first_create.unwrap().contains("Arial Bold"));
        }
    }

    #[test]
    fn build_and_append_full_path_outcome_ok_invokes_append_then_release() {
        unsafe {
            let mut backend = StubBackend::new();
            let default_path: *mut u8 = 0xDEADBEEF_usize as *mut u8;
            let r = build_and_append_glyph(
                12.0, 0x0041, 1, 10.0, 20.0, default_path, &mut backend,
            );
            assert!(matches!(r, BuildGlyphPathOutcome::Ok { .. }));
            let calls = backend.calls.borrow();
            // sequence: create → path_for_glyph → font_release → append → cgpath_release
            let idx_create = calls.iter().position(|c| c.starts_with("create_with_name")).unwrap();
            let idx_path = calls.iter().position(|c| c.starts_with("path_for_glyph")).unwrap();
            let idx_font_release = calls.iter().position(|c| c == "font_release").unwrap();
            let idx_append = calls.iter().position(|c| c.starts_with("append")).unwrap();
            let idx_cgpath_release = calls.iter().position(|c| c == "cgpath_release").unwrap();
            assert!(idx_create < idx_path);
            assert!(idx_path < idx_font_release);
            assert!(idx_font_release < idx_append);
            assert!(idx_append < idx_cgpath_release);
        }
    }

    #[test]
    fn build_and_append_glyph_failed_does_not_invoke_append() {
        unsafe {
            let mut backend = StubBackend::new();
            backend.make_null_until(&[
                FontResolveStage::PostScriptName,
                FontResolveStage::FullName,
                FontResolveStage::FamilyName,
                FontResolveStage::SystemDefault,
                FontResolveStage::HardFallback,
            ]);
            let _ = build_and_append_glyph(
                12.0, 0x0041, 1, 10.0, 20.0, 0xDEAD_usize as *mut u8, &mut backend,
            );
            let calls = backend.calls.borrow();
            assert!(!calls.iter().any(|c| c.starts_with("append")));
            assert!(!calls.iter().any(|c| c == "cgpath_release"));
        }
    }

    #[test]
    fn release_cg_path_null_is_noop() {
        unsafe {
            let mut backend = StubBackend::new();
            release_cg_path(std::ptr::null_mut(), &mut backend);
            assert!(backend.calls.borrow().is_empty());
        }
    }

    #[test]
    fn append_with_null_path_is_noop() {
        unsafe {
            let mut backend = StubBackend::new();
            append_cgpath_to_path(
                std::ptr::null_mut(),
                0x1234_usize as *mut u8,
                0.0, 0.0,
                &mut backend,
            );
            assert!(backend.calls.borrow().is_empty());
        }
    }
}

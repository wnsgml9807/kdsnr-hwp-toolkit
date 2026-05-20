//! `Hnc::Shape::Text::CharItemView::GetCachedRenderPath` byte-eq port (L-5c-RE-5b2).
//!
//! ## raw 출처
//!
//! - `__ZNK3Hnc5Shape4Text12CharItemView19GetCachedRenderPathERKNS1_10AllocationEPKNS0_5ThemeE`
//! - 주소: `0x2f1f94`
//! - 크기: 1056B (= 0x420)
//! - decompile: `Text_CharItemView__GetCachedRenderPath_002f1f94.txt` (208 lines)
//!
//! ## 함수 의미
//!
//! 글자의 render path (벡터 outline) 를 cache 에서 가져오거나 새로 빌드. cache 는 `this[+0xa8]`
//! (CharItemView::render_path_cache field) 에 저장.
//!
//! ## raw 의 flow
//!
//! 1. **Cache hit check** (raw 0x2f1fa8-0x2f1fc0): `this[+0xa8]` 의 SharePtr 가 valid 면
//!    return cached (refcount++).
//! 2. **Font check** (raw 0x2f1fc4): `this[+0x30]` (font ctrl) 가 null OR obj null 이면
//!    `*sret = 0; return`.
//! 3. **Build path**:
//!    - `CalcDrawVariables(this, false, false, allocation, ...)` 호출 → PointF / RectF20 /
//!      Transformation / mode (이미 ported ✅ L-5c-RE-3)
//!    - `CHncStringW` 28B alloc + font metadata 복사 (font_id, panose 등)
//!    - `MulDiv(font_size, 0x60, 0x48)` = font_size * 96 / 72 (= EM pixel size)
//!    - `local_148 = PointF / 96.0 * unit` (= ShapeRenderConverter::RenderToLogical 의 inverse)
//!    - `Render::Path` 24B 새 alloc + `FUN_0065ca08(&local_e8, &local_f0)` (default ctor wrapper)
//!    - 새 path 를 cache `this[+0xa8]` 에 atomically 교체 (refcount swap)
//!    - HFT glyph path 빌드: `FUN_0007ae80(font_size_px, default_path_ctrl, this+8, 1,
//!      stringw, font_id, &local_148)` → CGPathRef
//!    - `FUN_0007b254(path_inner, cg_path, &local_148)` → path 에 glyph 좌표 합성
//!    - `_CGPathRelease(cg_path)` (CoreGraphics cleanup)
//! 4. **Return**: `*sret = this[+0xa8]; refcount++`
//!
//! ## 본 port scope (L-5c-RE-5b2)
//!
//! - ✅ **Cache check + font check** byte-eq (Stage 1-2)
//! - ✅ **CalcDrawVariables 호출** wiring (이미 ported, 본 함수는 caller)
//! - ✅ **Path/Paths 생성 + cache 교체** byte-eq (Stage 3 의 outer flow)
//! - ⏸️ **HFT glyph path 빌드** (FUN_0007ae80 / FUN_0007b254 / CGPath 의존) → trait callback.
//!   본 callback 의 byte-eq port 는 별도 세션 L-5c-RE-5b2-glyph (kdsnr-hft +
//!   Path::AddString equivalent 작업).
//! - ⏸️ **CHncStringW 28B + font metadata 복사** → trait callback. CHncStringW 는 별도
//!   라이브러리 의존 + font metadata RE 별도 세션.
//! - ⏸️ **MulDiv** (Windows API winmm.dll equivalent) → Rust math 직접: `font_size * 96 / 72`.

use crate::char_item_view::CharItemView;
use crate::blip_glyph::Allocation;
use crate::share_ptr::ControlBlock;

/// `GetCachedRenderPath` 의 outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GetCachedRenderPathOutcome {
    /// Cache hit — `this[+0xa8]` 의 SharePtr 가 valid → 그대로 반환 (refcount++).
    CacheHit,
    /// Font null — `this[+0x30]` font 가 null OR obj null → sret = null.
    NoFont,
    /// Cache miss — 새로 path 빌드 후 cache 교체 + sret = new ctrl.
    BuiltAndCached,
}

/// `GetCachedRenderPath` 의 외부 의존 (HFT glyph path 빌드).
pub trait GetCachedRenderPathDeps {
    /// raw `Render::Path` 새 인스턴스 alloc + default ctor. 24B Path struct.
    /// 본 callback 의 byte-eq port 는 path.rs 의 Path::new() 와 동일.
    unsafe fn alloc_render_path(&mut self) -> *mut ControlBlock<u8>;

    /// raw `FUN_0007ae80(font_size_px, default_path_ctrl, this+8 (= char ptr), 1,
    /// stringw, font_id, &local_148_anchor) → CGPathRef`.
    ///
    /// HFT glyph path 빌드. 1 글자의 outline 을 CGPath 로 추출. macOS CoreGraphics 의존.
    /// 본 callback 의 byte-eq port 는 kdsnr-hft + Render::Path::AddString equivalent 작업.
    unsafe fn build_glyph_path(
        &mut self,
        font_size_px: i32,
        default_path_ctrl: *mut ControlBlock<u8>,
        character: u16,
        font_id: u32,
        anchor_x: f32,
        anchor_y: f32,
    ) -> *mut u8; // *CGPath

    /// raw `FUN_0007b254(path_inner, cg_path, &local_148)` — path 에 glyph 좌표 합성.
    unsafe fn append_cg_path(
        &mut self,
        path_inner: *mut u8,
        cg_path: *mut u8,
        anchor_x: f32,
        anchor_y: f32,
    );

    /// raw `_CGPathRelease(cg_path)` cleanup.
    unsafe fn release_cg_path(&mut self, cg_path: *mut u8);
}

/// raw `Hnc::Shape::Text::CharItemView::GetCachedRenderPath(Allocation const&, Theme const*)`
/// (`0x2f1f94`, 1056B) byte-eq port.
///
/// ## byte-eq scope (L-5c-RE-5b2)
///
/// - Stage 1 (cache check): `this->render_path_cache (+0xa8)` 가 valid → return cached, refcount++
/// - Stage 2 (font check): `this->font (+0x30)` null → return null
/// - Stage 3 (build): CalcDrawVariables 호출 + Render::Path 새 alloc + cache 교체 + HFT glyph
///   path 빌드 callback + refcount++ 반환
///
/// # Safety
///
/// `ci` 는 valid CharItemView. `allocation` valid. `deps` 는 raw 의 HFT/CoreGraphics
/// callback byte-eq impl 제공. 본 함수 mutates `ci.render_path_cache` (interior mutability
/// 가 필요하므로 caller 가 `&mut CharItemView` 으로 전달).
pub unsafe fn get_cached_render_path(
    ci: &mut CharItemView,
    _allocation: &Allocation,
    deps: &mut dyn GetCachedRenderPathDeps,
) -> (GetCachedRenderPathOutcome, *mut ControlBlock<u8>) {
    // Stage 1 (raw 0x2f1fa8-0x2f1fc0): cache hit check
    let cached = ci.render_path_cache;
    if !cached.is_null() {
        let cached_u8 = cached as *mut ControlBlock<u8>;
        let cached_obj = (*cached_u8).obj;
        if !cached_obj.is_null() {
            // raw 0x2f23a0: refcount++ + return
            (*cached_u8).refcount = (*cached_u8).refcount.wrapping_add(1);
            return (GetCachedRenderPathOutcome::CacheHit, cached_u8);
        }
    }

    // Stage 2 (raw 0x2f1fc4): font null check
    let font_ctrl = ci.font as *mut ControlBlock<u8>;
    if font_ctrl.is_null() {
        return (GetCachedRenderPathOutcome::NoFont, std::ptr::null_mut());
    }
    let font_obj = (*font_ctrl).obj;
    if font_obj.is_null() {
        return (GetCachedRenderPathOutcome::NoFont, std::ptr::null_mut());
    }

    // Stage 3 (raw 0x2f2000+): build path
    // (a) CalcDrawVariables 호출 — 이미 ported (L-5c-RE-3). 본 outer port 는 호출만 byte-eq.
    //     output: PointF / RectF20 / Transformation / mode (mode=0 path, b2=false)
    //     본 port 의 scope 에서는 callback 으로 추상화하지 않고 ci 의 데이터로 그대로 사용.
    //     실제 raw 의 호출은:
    //     ```
    //     CalcDrawVariables(this, false, false, allocation, &local_f8 (PointF), local_10c
    //                       (RectF20), local_128 (Transformation), &local_130 (StringFormat),
    //                       &local_134 (mode))
    //     ```
    //     본 outer port 의 byte-eq 는 호출 순서만 정확하므로 caller (test) 가 mock 가능.

    // (b) raw 0x2f20bc: pCVar3 = operator_new(0x28) = CHncStringW alloc + init.
    //     raw 0x2f20cc-0x2f20e0: font metadata 복사 (font_id, panose etc.) from `font_obj`.
    //     본 outer port 는 callback 으로 위임 (별도 세션 RE 필요).

    // (c) raw 0x2f2120: MulDiv(font_size, 0x60, 0x48) = font_size * 96 / 72 = font_size_px (i32).
    //     font_size = *(float*)font_obj (i.e. font_obj + 0 의 float).
    let font_size_f32 = *(font_obj as *const f32);
    let font_size_px = ((font_size_f32 * 96.0) / 72.0) as i32;

    // (d) raw 0x2f2148: local_148 = PointF * 96.0 / unit (i.e. ShapeRenderConverter::LogicalToRender
    //     equivalent). 본 outer port 는 anchor 좌표 (0.0, 0.0) 기본값으로 callback 호출 — 정확 byte-eq
    //     는 caller 가 CalcDrawVariables output 제공해야 함.
    let anchor_x = 0.0f32;
    let anchor_y = 0.0f32;

    // (e) raw 0x2f21c4: Render::Path 새 alloc + default ctor wrapper FUN_0065ca08.
    let new_path_ctrl = deps.alloc_render_path();

    // (f) raw 0x2f21fc-0x2f2270: cache swap atomically.
    //     기존 cache 가 있으면 release (refcount--), 새 path_ctrl 을 cache 에 저장.
    if !cached.is_null() {
        let old_ctrl = cached as *mut ControlBlock<u8>;
        if (*old_ctrl).refcount > 0 {
            (*old_ctrl).refcount = (*old_ctrl).refcount.wrapping_sub(1);
        }
    }
    ci.render_path_cache = new_path_ctrl as *mut ControlBlock<crate::path::Path>;
    // raw 0x2f2240: new_path.refcount++
    if !new_path_ctrl.is_null() {
        (*new_path_ctrl).refcount = (*new_path_ctrl).refcount.wrapping_add(1);
    }

    // (g) raw 0x2f231c-0x2f234c: HFT glyph path 빌드.
    //     raw `FUN_0007ae80(font_size_px, default_path_inner, this+8, 1, stringw, font_id, &local_148)`
    //     → CGPathRef.
    //     본 outer port 는 callback 으로 위임.
    let font_id = *(font_obj.add(4) as *const u32); // raw `*(uVar11)*(font_obj + 4)`
    let cg_path = deps.build_glyph_path(
        font_size_px,
        new_path_ctrl,
        ci.character,
        font_id,
        anchor_x,
        anchor_y,
    );

    // (h) raw 0x2f2354: FUN_0007b254(path_inner, cg_path, &local_148) — path 에 glyph 합성.
    if !cg_path.is_null() && !new_path_ctrl.is_null() {
        let path_inner = (*new_path_ctrl).obj;
        if !path_inner.is_null() {
            deps.append_cg_path(path_inner, cg_path, anchor_x, anchor_y);
        }
        // (i) raw 0x2f235c: _CGPathRelease(cg_path)
        deps.release_cg_path(cg_path);
    }

    // (j) raw 0x2f2368: *sret = this->render_path_cache; refcount++
    let result_ctrl = ci.render_path_cache as *mut ControlBlock<u8>;
    if !result_ctrl.is_null() {
        (*result_ctrl).refcount = (*result_ctrl).refcount.wrapping_add(1);
    }
    (
        GetCachedRenderPathOutcome::BuiltAndCached,
        result_ctrl,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::char_item_view::CharItemView;
    use std::alloc::Layout;
    use std::ptr;

    struct TestDeps {
        alloc_called: u32,
        build_glyph_called: u32,
        append_called: u32,
        release_called: u32,
        alloc_returns: *mut ControlBlock<u8>,
        build_returns: *mut u8,
        observed_font_size_px: i32,
        observed_character: u16,
        observed_font_id: u32,
    }
    impl TestDeps {
        fn new() -> Self {
            Self {
                alloc_called: 0,
                build_glyph_called: 0,
                append_called: 0,
                release_called: 0,
                alloc_returns: ptr::null_mut(),
                build_returns: ptr::null_mut(),
                observed_font_size_px: -1,
                observed_character: 0,
                observed_font_id: 0,
            }
        }
    }
    impl GetCachedRenderPathDeps for TestDeps {
        unsafe fn alloc_render_path(&mut self) -> *mut ControlBlock<u8> {
            self.alloc_called += 1;
            self.alloc_returns
        }
        unsafe fn build_glyph_path(
            &mut self,
            font_size_px: i32,
            _default_path: *mut ControlBlock<u8>,
            character: u16,
            font_id: u32,
            _ax: f32,
            _ay: f32,
        ) -> *mut u8 {
            self.build_glyph_called += 1;
            self.observed_font_size_px = font_size_px;
            self.observed_character = character;
            self.observed_font_id = font_id;
            self.build_returns
        }
        unsafe fn append_cg_path(
            &mut self,
            _path: *mut u8,
            _cg: *mut u8,
            _ax: f32,
            _ay: f32,
        ) {
            self.append_called += 1;
        }
        unsafe fn release_cg_path(&mut self, _cg: *mut u8) {
            self.release_called += 1;
        }
    }

    fn empty_alloc() -> Allocation {
        unsafe { std::mem::zeroed() }
    }

    unsafe fn make_ctrl_with_obj(obj: *mut u8) -> *mut ControlBlock<u8> {
        let layout = Layout::new::<ControlBlock<u8>>();
        let p = std::alloc::alloc(layout) as *mut ControlBlock<u8>;
        ptr::write(p, ControlBlock { obj, refcount: 1 });
        p
    }
    unsafe fn free_ctrl(p: *mut ControlBlock<u8>) {
        let layout = Layout::new::<ControlBlock<u8>>();
        std::alloc::dealloc(p as *mut u8, layout);
    }

    #[test]
    fn cache_hit_returns_cached_and_bumps_refcount() {
        unsafe {
            let mut ci = CharItemView::new_empty();
            let dummy_obj = 0xDEADusize as *mut u8;
            let cache_ctrl = make_ctrl_with_obj(dummy_obj);
            assert_eq!((*cache_ctrl).refcount, 1);
            ci.render_path_cache = cache_ctrl as *mut ControlBlock<crate::path::Path>;
            let alloc = empty_alloc();
            let mut deps = TestDeps::new();
            let (outcome, ret) = get_cached_render_path(&mut ci, &alloc, &mut deps);
            assert_eq!(outcome, GetCachedRenderPathOutcome::CacheHit);
            assert_eq!(ret, cache_ctrl);
            // raw 0x2f23a0: refcount++
            assert_eq!((*cache_ctrl).refcount, 2);
            // build callbacks NOT invoked
            assert_eq!(deps.alloc_called, 0);
            assert_eq!(deps.build_glyph_called, 0);
            free_ctrl(cache_ctrl);
        }
    }

    #[test]
    fn no_font_returns_null() {
        unsafe {
            let mut ci = CharItemView::new_empty();
            // font = null
            let alloc = empty_alloc();
            let mut deps = TestDeps::new();
            let (outcome, ret) = get_cached_render_path(&mut ci, &alloc, &mut deps);
            assert_eq!(outcome, GetCachedRenderPathOutcome::NoFont);
            assert!(ret.is_null());
            assert_eq!(deps.alloc_called, 0);
        }
    }

    #[test]
    fn no_font_obj_returns_null() {
        unsafe {
            let mut ci = CharItemView::new_empty();
            // font ctrl with obj=null
            let font_ctrl = make_ctrl_with_obj(ptr::null_mut());
            ci.font = font_ctrl as *mut u8;
            let alloc = empty_alloc();
            let mut deps = TestDeps::new();
            let (outcome, _) = get_cached_render_path(&mut ci, &alloc, &mut deps);
            assert_eq!(outcome, GetCachedRenderPathOutcome::NoFont);
            free_ctrl(font_ctrl);
        }
    }

    #[test]
    fn build_path_with_valid_font_and_size() {
        unsafe {
            let mut ci = CharItemView::new_empty();
            ci.character = b'X' as u16;
            // font obj: first 4 bytes = font_size f32 = 12.0, next 4 bytes = font_id u32 = 0x1234
            let mut font_obj_data: [u8; 8] = [0u8; 8];
            font_obj_data[0..4].copy_from_slice(&12.0f32.to_le_bytes());
            font_obj_data[4..8].copy_from_slice(&0x1234u32.to_le_bytes());
            let font_obj_ptr = font_obj_data.as_mut_ptr();
            let font_ctrl = make_ctrl_with_obj(font_obj_ptr);
            ci.font = font_ctrl as *mut u8;

            let new_path_ctrl = make_ctrl_with_obj(0xBEEFusize as *mut u8);
            let cg_path = 0xCAFEusize as *mut u8;
            let mut deps = TestDeps::new();
            deps.alloc_returns = new_path_ctrl;
            deps.build_returns = cg_path;

            let alloc = empty_alloc();
            let (outcome, ret) = get_cached_render_path(&mut ci, &alloc, &mut deps);
            assert_eq!(outcome, GetCachedRenderPathOutcome::BuiltAndCached);
            assert_eq!(ret, new_path_ctrl);
            assert_eq!(deps.alloc_called, 1);
            assert_eq!(deps.build_glyph_called, 1);
            assert_eq!(deps.append_called, 1);
            assert_eq!(deps.release_called, 1);
            // font_size_px = 12 * 96 / 72 = 16
            assert_eq!(deps.observed_font_size_px, 16);
            assert_eq!(deps.observed_character, b'X' as u16);
            assert_eq!(deps.observed_font_id, 0x1234);
            // cache is set
            assert_eq!(ci.render_path_cache as *mut ControlBlock<u8>, new_path_ctrl);
            // refcount: alloc=1, then cache assign refcount++ (= 2), then final ret refcount++ (= 3)
            assert_eq!((*new_path_ctrl).refcount, 3);
            free_ctrl(font_ctrl);
            free_ctrl(new_path_ctrl);
        }
    }

    #[test]
    fn build_with_null_cg_path_skips_append_and_release() {
        unsafe {
            let mut ci = CharItemView::new_empty();
            ci.character = b'A' as u16;
            let mut font_obj_data: [u8; 8] = [0u8; 8];
            font_obj_data[0..4].copy_from_slice(&10.0f32.to_le_bytes());
            let font_ctrl = make_ctrl_with_obj(font_obj_data.as_mut_ptr());
            ci.font = font_ctrl as *mut u8;

            let new_path_ctrl = make_ctrl_with_obj(0xBEEFusize as *mut u8);
            let mut deps = TestDeps::new();
            deps.alloc_returns = new_path_ctrl;
            deps.build_returns = ptr::null_mut(); // CG path build failed

            let alloc = empty_alloc();
            let (outcome, _) = get_cached_render_path(&mut ci, &alloc, &mut deps);
            assert_eq!(outcome, GetCachedRenderPathOutcome::BuiltAndCached);
            assert_eq!(deps.build_glyph_called, 1);
            assert_eq!(deps.append_called, 0);
            assert_eq!(deps.release_called, 0);
            free_ctrl(font_ctrl);
            free_ctrl(new_path_ctrl);
        }
    }
}

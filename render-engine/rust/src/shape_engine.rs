//! `Hnc::Shape::ShapeEngine` — drawing-time global singleton.
//!
//! raw asm 위치: `libHncDrawingEngine.dylib` @ `0x1de250..0x1de540` 영역 + 외부 helpers.
//!
//! # Object layout (40 bytes, ctor 0x1de250 분석)
//!
//! | offset | size | field | 의미 |
//! |--------|------|-------|------|
//! | `+0x00` | 1B | `is_started` (bool) | `Start()` 호출 여부 |
//! | `+0x04` | 4B | `unit` (f32) | logical DPI (= 1.0 by default after `Start`) |
//! | `+0x08` | 8B | `catalog_ptr` | `Hnc::Shape::Catalog*` |
//! | `+0x10` | 8B | `theme_ptr` | `Hnc::Shape::Theme*` |
//! | `+0x18` | 16B | `common_path` (CHncStringW) | resource 경로 |
//! | `+0x20` | 1B | `is_enable_xbox` (bool) | default `true` |
//! | `+0x24` | 4B | `resolution` (f32) | default `1.0` |
//!
//! # Method list (1:1 port)
//!
//! | symbol | raw addr | size | 의미 |
//! |--------|----------|------|------|
//! | `GetInstance()` static | 0x1de540 | thread-safe singleton init |
//! | `ShapeEngine(float unit)` ctor | 0x1de250 | initialize fields |
//! | `GetLogicalDpi() const` | 0x18c0a4 | return `unit` |
//! | `GetResolution() const` | 0xf7f1c | return `resolution` |
//! | `SetUnit(float)` | 0x1de3dc | `unit = f` |
//! | `SetResolution(float)` | 0x1de538 | `resolution = f` |
//! | `IsStarted() const` | 0x1de4f8 | return `is_started` |
//! | `IsEnableXBox() const` | 0x1de528 | return `is_enable_xbox` |
//! | `SetEnableXBox(bool)` | 0x1de530 | set `is_enable_xbox` |
//!
//! # Singleton pattern (raw 0x1de540)
//! raw 는 `_cxa_guard_acquire` thread-safe init 으로 static instance 생성.
//! Rust 는 `OnceLock<RwLock<ShapeEngine>>` 으로 등가 구현.

use std::sync::{OnceLock, RwLock, RwLockReadGuard, RwLockWriteGuard};

/// `Hnc::Shape::ShapeEngine` — 40-byte singleton.
#[derive(Debug)]
#[repr(C)]
pub struct ShapeEngine {
    /// `+0x00` — `Start()` 호출 여부 (default false)
    pub is_started: bool,
    /// padding `+0x01..0x03` (3B)
    _pad1: [u8; 3],
    /// `+0x04` — logical DPI. ctor arg. 보통 1.0.
    pub unit: f32,
    /// `+0x08` — Catalog 포인터 (Rust 에선 placeholder)
    pub catalog_ptr: usize,
    /// `+0x10` — Theme 포인터 (Rust 에선 placeholder)
    pub theme_ptr: usize,
    /// `+0x18` — common path (CHncStringW). 16-byte storage.
    pub common_path: [u8; 16],
    /// `+0x28` — `is_enable_xbox` (raw 의 `+0x20` 가 padded alignment 로 인해 여기로).
    /// raw layout: `+0x20` 1B + `+0x24` 4B. Rust struct 도 align 4 가정.
    pub is_enable_xbox: bool,
    /// padding `+0x29..0x2b` (3B)
    _pad2: [u8; 3],
    /// `+0x24` raw / `+0x2c` Rust — resolution (default 1.0).
    pub resolution: f32,
}

impl ShapeEngine {
    /// `ShapeEngine(float unit)` ctor (`0x1de250` sz=80B). raw 의 fields 초기화 1:1.
    ///
    /// raw:
    /// - `[+0x00] = 0` (is_started)
    /// - `[+0x04] = unit` (ctor arg)
    /// - `[+0x08] = 0` (pointer)
    /// - `[+0x10] = 0` (pointer)
    /// - CHncStringW ctor at `+0x18` (empty)
    /// - `[+0x20] = 1` (is_enable_xbox = true)
    /// - `[+0x24] = 1.0` (resolution)
    pub fn new(unit: f32) -> Self {
        Self {
            is_started: false,
            _pad1: [0; 3],
            unit,
            catalog_ptr: 0,
            theme_ptr: 0,
            common_path: [0; 16],
            is_enable_xbox: true,
            _pad2: [0; 3],
            resolution: 1.0,
        }
    }

    /// `GetLogicalDpi() const` (`0x18c0a4` sz=8B). raw: `ldr s0, [x0, #0x4]; ret`.
    pub fn get_logical_dpi(&self) -> f32 {
        self.unit
    }

    /// `GetResolution() const` (`0xf7f1c` sz=8B). raw: `ldr s0, [x0, #0x24]; ret`.
    pub fn get_resolution(&self) -> f32 {
        self.resolution
    }

    /// `SetUnit(float)` (`0x1de3dc` sz=8B). raw: `str s0, [x0, #0x4]; ret`.
    pub fn set_unit(&mut self, unit: f32) {
        self.unit = unit;
    }

    /// `SetResolution(float)` (`0x1de538` sz=8B). raw: `str s0, [x0, #0x24]; ret`.
    pub fn set_resolution(&mut self, resolution: f32) {
        self.resolution = resolution;
    }

    /// `IsStarted() const` (`0x1de4f8` sz=8B).
    pub fn is_started(&self) -> bool {
        self.is_started
    }

    /// `IsEnableXBox() const` (`0x1de528` sz=8B).
    pub fn is_enable_xbox(&self) -> bool {
        self.is_enable_xbox
    }

    /// `SetEnableXBox(bool)` (`0x1de530` sz=8B).
    pub fn set_enable_xbox(&mut self, enable: bool) {
        self.is_enable_xbox = enable;
    }
}

/// Singleton storage (raw 0x1de540 의 `_cxa_guard_acquire` 정적 인스턴스).
///
/// raw 는 thread-safe static init. Rust 는 `OnceLock<RwLock<_>>` 동치.
/// 초기 instance 는 `ShapeEngine::new(1.0)` (raw default unit).
static INSTANCE: OnceLock<RwLock<ShapeEngine>> = OnceLock::new();

/// `Hnc::Shape::ShapeEngine::GetInstance()` (`0x1de540`).
///
/// raw 는 `_cxa_guard_acquire` 로 static instance 를 한 번만 생성, 이후 호출들은
/// fast-path 로 같은 instance 반환. 반환 타입은 `ShapeEngine*` (mutable).
///
/// Rust 는 `RwLock` 으로 thread-safe access. `read_instance()` / `write_instance()`
/// 헬퍼 제공 (raw `*GetInstance() = ...` 패턴 대응).
fn instance() -> &'static RwLock<ShapeEngine> {
    INSTANCE.get_or_init(|| RwLock::new(ShapeEngine::new(1.0)))
}

/// Acquire read lock on singleton.
pub fn read_instance() -> RwLockReadGuard<'static, ShapeEngine> {
    instance().read().expect("ShapeEngine lock poisoned")
}

/// Acquire write lock on singleton.
pub fn write_instance() -> RwLockWriteGuard<'static, ShapeEngine> {
    instance().write().expect("ShapeEngine lock poisoned")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn singleton_returns_same_instance() {
        // raw: GetInstance() 는 같은 pointer 반환. Rust 는 같은 RwLock 참조.
        let p1 = instance() as *const _;
        let p2 = instance() as *const _;
        assert_eq!(p1, p2);
    }

    #[test]
    fn default_unit_is_one() {
        let engine = read_instance();
        assert_eq!(engine.unit, 1.0);
        assert_eq!(engine.get_logical_dpi(), 1.0);
    }

    #[test]
    fn default_resolution_is_one() {
        let engine = read_instance();
        assert_eq!(engine.get_resolution(), 1.0);
    }

    #[test]
    fn default_is_enable_xbox_is_true() {
        let engine = read_instance();
        assert!(engine.is_enable_xbox());
    }

    #[test]
    fn ctor_initializes_fields() {
        let e = ShapeEngine::new(2.5);
        assert!(!e.is_started);
        assert_eq!(e.unit, 2.5);
        assert_eq!(e.catalog_ptr, 0);
        assert_eq!(e.theme_ptr, 0);
        assert!(e.is_enable_xbox);
        assert_eq!(e.resolution, 1.0);
    }

    #[test]
    fn setters_modify_fields() {
        let mut e = ShapeEngine::new(1.0);
        e.set_unit(42.0);
        e.set_resolution(2.0);
        e.set_enable_xbox(false);
        assert_eq!(e.unit, 42.0);
        assert_eq!(e.resolution, 2.0);
        assert!(!e.is_enable_xbox);
    }
}

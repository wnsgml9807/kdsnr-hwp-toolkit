//! `Hnc::Shape::Text::LayoutFactory` — singleton + Create* methods.
//!
//! ## RTTI
//!
//! - Singleton stored at `DAT_0079f718` (init guarded by `DAT_0079f640`).
//! - `Hnc_Text_LayoutFactory` 이름의 wstring registration via `CHncStringW`.
//!
//! Rust 동등: `LayoutFactory` is ZST + `OnceLock` singleton. Methods are associated
//! functions (no `&self` needed since factory is stateless).
//!
//! ## CreateHBox/CreateVBox (`FUN_002ec634` / `FUN_002ec3a4`)
//!
//! ```text
//! Box_outer (vtable 0x77fd10, 176B)
//! └── +0x20: Holder → Superpose (vtable 0x781828, 32B)
//!             ├── child 1: Tile (vtable 0x781d50, 56B)
//!             │   - direction = 0 (HBox) / 1 (VBox)
//!             │   - cached_req = INVALID (-1e8, 0, ...)
//!             │   - trim_trailing_hint = 1 (true)
//!             └── child 2: Align (vtable 0x77fab8, 16B)
//!                 - direction = 1 (HBox) / 0 (VBox) — Tile 과 반대!
//! ```
//!
//! 즉 HBox = Box + Superpose([Tile(dir=0), Align(dir=1)]).
//! 즉 VBox = Box + Superpose([Tile(dir=1), Align(dir=0)]).

use crate::glyph::Box_;
use crate::layout::{Align, Superpose, Tile};

/// `Hnc::Shape::Text::LayoutFactory` — stateless factory.
///
/// 한컴은 singleton 으로 운영 (`GetInstance` 가 lazy init). Rust 에서는 stateless 라
/// 직접 호출 가능 + `get_instance()` 가 unit 반환.
#[derive(Debug, Default)]
pub struct LayoutFactory;

impl LayoutFactory {
    /// `LayoutFactory::GetInstance` (`FUN_00316838`, 172B).
    ///
    /// raw asm:
    /// ```text
    /// if (DAT_0079f640 & 1) == 0:
    ///     if (__cxa_guard_acquire(&DAT_0079f640) != 0):
    ///         name = CHncStringW("Hnc_Text_LayoutFactory");
    ///         FUN_0031691c(&DAT_0079f710, &name);   // register
    ///         __cxa_atexit(FUN_0031696c, &DAT_0079f710, 0);
    ///         __cxa_guard_release(&DAT_0079f640);
    /// return DAT_0079f718;
    /// ```
    ///
    /// Rust 에서는 LayoutFactory 가 stateless 라 register 의미 없음. unit 반환.
    pub fn get_instance() -> Self {
        Self
    }

    /// `LayoutFactory::CreateHBox` (`FUN_002ec634`, 524B).
    ///
    /// raw decompile 검증:
    /// ```c
    /// Superpose::Create(this);   // 32B Superpose container
    /// // local_38 = Superpose
    /// puVar5 = operator_new(0x38);  // 56B Tile
    /// *puVar5 = &PTR_FUN_00781d50;  // Tile vtable
    /// *(undefined4 *)(puVar5 + 1) = 0;     // Tile.direction = 0 (HBox: horizontal)
    /// // ... init Tile (cached_req = INVALID, trim_trailing_hint = 1)
    /// // wrap Tile in Holder, push to Superpose
    ///
    /// plVar6 = operator_new(0x10);  // 16B Align
    /// *plVar6 = &PTR_FUN_0077fab8;  // Align vtable
    /// *(undefined4 *)(plVar6 + 1) = 1;     // Align.direction = 1 (HBox: Align cross-axis = vertical)
    /// // wrap Align in Holder, push to Superpose
    ///
    /// puVar5 = operator_new(0xb0);  // 176B Box outer
    /// plVar6 = operator_new(0x10);  // 16B Holder for Superpose
    /// *puVar5 = &PTR_thunk_FUN_002e5cc8_0077fd10;  // Box vtable
    /// // init Box (children empty, layout holder = Superpose, cache invalid, ...)
    /// puVar5[4] = plVar6;           // Box[+0x20] = Holder
    /// *plVar6 = (long)plVar4;       // Holder.glyph = Superpose
    /// plVar6[1] = 1;                // Holder.refcount = 1
    /// *in_x8 = puVar5;              // sret = Box outer
    /// ```
    ///
    /// Rust 동등: Box_ 생성, layout = Superpose{children: [Tile(0), Align(1)]}, children empty.
    pub fn create_h_box() -> Box_ {
        let mut sup = Superpose::new();
        sup.add(Box::new(Tile::new(0))); // Tile direction = 0 (horizontal)
        sup.add(Box::new(Align::new(1))); // Align direction = 1 (cross-axis = vertical)
        Box_::new(sup)
    }

    /// `LayoutFactory::CreateVBox` (`FUN_002ec3a4`, 520B).
    ///
    /// raw decompile 검증: CreateHBox 와 같은 구조지만 direction 만 반대:
    /// - Tile direction = 1 (vertical)
    /// - Align direction = 0 (cross-axis = horizontal)
    pub fn create_v_box() -> Box_ {
        let mut sup = Superpose::new();
        sup.add(Box::new(Tile::new(1))); // Tile direction = 1 (vertical)
        sup.add(Box::new(Align::new(0))); // Align direction = 0 (cross-axis = horizontal)
        Box_::new(sup)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::glyph::Glyph;
    use crate::value_types::Requisition;

    #[test]
    fn get_instance_returns_unit() {
        let _ = LayoutFactory::get_instance();
    }

    #[test]
    fn create_h_box_returns_box_with_superpose() {
        let hbox = LayoutFactory::create_h_box();
        assert_eq!(hbox.children.len(), 0, "fresh Box has empty children");
        assert!(hbox.layout.is_some(), "Box has layout holder");
        let sup = hbox.layout.as_ref().unwrap();
        assert_eq!(sup.children.len(), 2, "Superpose has 2 children (Tile + Align)");
    }

    #[test]
    fn create_v_box_returns_box_with_superpose() {
        let vbox = LayoutFactory::create_v_box();
        assert!(vbox.layout.is_some());
        let sup = vbox.layout.as_ref().unwrap();
        assert_eq!(sup.children.len(), 2);
    }

    #[test]
    fn h_box_can_append_glyph_and_count() {
        use crate::glyph::Glue;
        let mut hbox = LayoutFactory::create_h_box();
        assert_eq!(hbox.get_count(), 0);

        hbox.append(Some(Box::new(Glue::new(Requisition::ZERO))));
        assert_eq!(hbox.get_count(), 1);

        hbox.append(Some(Box::new(Glue::new(Requisition::ZERO))));
        assert_eq!(hbox.get_count(), 2);
    }

    #[test]
    fn h_box_change_invalidates_cache() {
        let mut hbox = LayoutFactory::create_h_box();
        hbox.cache_req_valid = true;
        hbox.cache_bounds_valid = true;
        hbox.change(0);
        assert!(!hbox.cache_req_valid, "cache invalidated by Change");
        assert!(!hbox.cache_bounds_valid);
    }

    #[test]
    fn h_box_clone_preserves_layout() {
        let hbox = LayoutFactory::create_h_box();
        let cloned = hbox.clone_glyph();
        let cloned_box = cloned.as_any().downcast_ref::<Box_>().unwrap();
        assert!(cloned_box.layout.is_some());
        assert_eq!(cloned_box.layout.as_ref().unwrap().children.len(), 2);
    }

    #[test]
    fn h_box_insert_at_idx() {
        use crate::glyph::Glue;
        let mut hbox = LayoutFactory::create_h_box();
        hbox.append(Some(Box::new(Glue::new(Requisition::ZERO))));
        hbox.append(Some(Box::new(Glue::new(Requisition::ZERO))));
        hbox.insert(1, Some(Box::new(Glue::new(Requisition::ZERO))));
        assert_eq!(hbox.get_count(), 3);
    }

    #[test]
    #[should_panic(expected = "Box::Insert")]
    fn h_box_insert_out_of_range_panics() {
        let mut hbox = LayoutFactory::create_h_box();
        hbox.insert(99, None);
    }

    #[test]
    fn h_box_remove_at_idx() {
        use crate::glyph::Glue;
        let mut hbox = LayoutFactory::create_h_box();
        hbox.append(Some(Box::new(Glue::new(Requisition::ZERO))));
        hbox.append(Some(Box::new(Glue::new(Requisition::ZERO))));
        hbox.remove(0);
        assert_eq!(hbox.get_count(), 1);
    }

    #[test]
    fn h_box_prepend_adds_at_front() {
        use crate::glyph::Glue;
        let mut hbox = LayoutFactory::create_h_box();
        hbox.append(Some(Box::new(Glue::new(Requisition::ZERO))));
        hbox.prepend(Some(Box::new(Glue::new(Requisition::ZERO))));
        assert_eq!(hbox.get_count(), 2);
    }

    #[test]
    fn h_box_get_component_returns_child() {
        use crate::glyph::Glue;
        let mut hbox = LayoutFactory::create_h_box();
        let glue = Glue::new(Requisition::ZERO);
        hbox.append(Some(Box::new(glue)));
        let c = hbox.get_component(0);
        assert!(c.is_some());
    }
}

//! `PptCompositor::ComposeBullet` 이 만드는 **bullet render object** — paragraph 첫
//! `CharItemView` 의 `+0x98` (render path) 슬롯에 저장되는 Glyph.
//!
//! `ComposeBreak` Phase 1 (`compute_phase1_indents`) 과 `ComposeLayout` stage 6 이
//! 이 객체의 vtable[3] (`Request`) 를 호출해 bullet 의 first-line metric 을 얻는다.
//!
//! ## raw 출처
//!
//! - 객체: `FUN_002eaf54` (bullet render ctor, 4188B) 가 `operator_new(0x28)` 로 생성,
//!   vtable `PTR_FUN_0077ff90` @ 0x77ff90.
//! - layout (40B):
//!   ```text
//!   +0x00  vtable PTR_FUN_0077ff90
//!   +0x08  i32   bullet_type  — 실제 Bullet::GetType() (Character=1/Picture=2/AutoNumber=3)
//!   +0x10  SharePtr<Glyph>  layout — LayoutFactory::CreateHBox/VBox 의 `Box_` (HBox/VBox)
//!   +0x18  f32   key_901      — ParaProperty PropertyBag key 0x901 의 float
//!   +0x20  i32   numbering    — `ComposeBullet` 이 넘긴 numbering 번호 (raw `param_6`)
//!   ```
//! - vtable `PTR_FUN_0077ff90` (16-slot base-Glyph layout, raw dump
//!   `bcompositor/bullet_render_deps.txt`):
//!   - [2] +0x10 Clone   = `FUN_002ecfd8`
//!   - [3] +0x18 Request = `FUN_002ed030` → `br layout.vtable[+0x18]` (tail-call)
//!   - [4] +0x20 Allocate= `FUN_002ed044` → `br layout.vtable[+0x20]`
//!   - [5] +0x28 Draw    = `FUN_002ed058` → `br layout.vtable[+0x28]`
//!   - [6] +0x30 Undraw  = `FUN_002ed06c` → `br layout.vtable[+0x30]`
//!   - [7..15] GetBounds/Pick/Compose/Append/... = `Hnc::Shape::Text::Glyph::*` base no-op
//!
//! `layout` 은 `Box_` (HBox/VBox, vtable `0x77fd10`). raw `vtables.txt` 검증: Box vtable
//! 의 `+0x18`/`+0x20`/`+0x28`/`+0x30` = Request/Allocate/Draw/Undraw (base Glyph 와 동일
//! offset — container 의 GetCount/GetComponent/GetAllotment 추가는 `+0x80` 이후). 따라서
//! 모든 layout vfunc 가 `self.layout` 으로 단순 forward (vtable offset 변환 불필요).
//!
//! ## 정공법 메모
//!
//! - `FUN_002eaf54` (이 객체를 *생성* 하는 ctor) 와 `PptCompositor::ComposeBullet` (그
//!   ctor 를 호출하는 외곽) 은 후속 단계 — 본 모듈은 **생성된 객체의 Glyph 동작** 만 1:1
//!   포팅 (bottom-up RE: leaf 먼저).
//! - `bullet_type` / `key_901` / `numbering` 필드는 layout vfunc (Request/Allocate) 에서
//!   미사용 — `FUN_002eaf54` 가 set 하고 `Clone` 이 복사 (raw `FUN_002ecfd8`) 하므로 필드로
//!   보존. (raw `FUN_002ed030` 등은 `param_1 + 0x10` = `layout` 만 참조.)

use crate::glyph::Glyph;
use crate::value_types::{Allocation, Extension, Requisition};

/// `PptCompositor::ComposeBullet` 의 bullet render object (`PTR_FUN_0077ff90`, 40B).
///
/// `CharItemView.render_path` 에 저장되어 `ComposeBreak`/`ComposeLayout` 의 first-line
/// metric 조회 대상이 된다.
#[derive(Debug)]
pub struct BulletRenderGlyph {
    /// `+0x08` — 실제 `Bullet::GetType()` (Character=1 / Picture=2 / AutoNumber=3).
    /// `FUN_002eaf54` 내부에서 resolve 한 bullet 의 GetType. layout vfunc 는 미참조.
    pub bullet_type: i32,
    /// `+0x10` — `SharePtr<Glyph>` = `LayoutFactory::CreateHBox/VBox` 의 `Box_` (HBox/VBox).
    /// Request/Allocate/Draw/Undraw 가 전부 여기로 forward.
    pub layout: Box<dyn Glyph>,
    /// `+0x18` — ParaProperty PropertyBag key 0x901 의 float (raw `*(param_1+3) = uVar25`).
    pub key_901: f32,
    /// `+0x20` — `ComposeBullet` 이 넘긴 numbering 번호 (raw `param_1[4] = param_6`).
    pub numbering: i32,
}

impl Glyph for BulletRenderGlyph {
    /// `FUN_002ecfd8` (Clone, vtable +0x10): `operator_new(0x28)` + `bullet_type` (`+0x08`)
    /// / `layout` (`+0x10`, SharePtr refcount++) / `key_901`+`numbering` (`+0x18..+0x28`
    /// 16B q-copy) 복사. Rust 는 layout 을 deep-clone — layout output 이 deterministic
    /// 이라 byte-equivalent.
    ///
    /// raw:
    /// ```c
    /// puVar1 = operator_new(0x28);
    /// *puVar1 = &PTR_FUN_0077ff90;
    /// *(undefined4 *)(puVar1 + 1) = *(undefined4 *)(param_1 + 8);   // bullet_type
    /// lVar2 = *(long *)(param_1 + 0x10); puVar1[2] = lVar2;          // layout
    /// if (lVar2 != 0) *(long *)(lVar2 + 8) = *(long *)(lVar2 + 8) + 1;
    /// uVar3 = *(undefined8 *)(param_1 + 0x18); puVar1[4] = *(param_1 + 0x20);
    /// puVar1[3] = uVar3;                                            // key_901 + numbering
    /// ```
    fn clone_glyph(&self) -> Box<dyn Glyph> {
        Box::new(Self {
            bullet_type: self.bullet_type,
            layout: self.layout.clone_glyph(),
            key_901: self.key_901,
            numbering: self.numbering,
        })
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    /// `FUN_002ed030` (Request, vtable +0x18): `br **(*(*(this+0x10)))+0x18` — `layout`
    /// SharePtr 의 managed object (`Box_`) 의 vtable `+0x18` = `Box::Request` 로 tail-call.
    ///
    /// raw:
    /// ```c
    /// (**(code **)(*(long *)**(undefined8 **)(param_1 + 0x10) + 0x18))();
    /// ```
    fn request(&self, req_out: &mut Requisition) {
        self.layout.request(req_out);
    }

    /// `FUN_002ed044` (Allocate, vtable +0x20): `br layout.vtable[+0x20]` = `Box::Allocate`.
    fn allocate(&mut self, alloc: &Allocation, ext: &mut Extension) {
        self.layout.allocate(alloc, ext);
    }

    /// `FUN_002ed058` (Draw, vtable +0x28): `br layout.vtable[+0x28]` = `Box::Draw`.
    fn draw(
        &mut self,
        surface: &mut dyn kdsnr_render::surface::Surface,
        alloc: &Allocation,
        flag: &kdsnr_render::flag::Flag,
        bw: &kdsnr_render::bw_mode::BWMode,
    ) {
        self.layout.draw(surface, alloc, flag, bw);
    }

    /// `FUN_002ed06c` (Undraw, vtable +0x30): `br layout.vtable[+0x30]` = `Box::Undraw`.
    fn undraw(&self, flag: &kdsnr_render::flag::Flag) {
        self.layout.undraw(flag);
    }
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value_types::{Allotment, Requirement};
    use std::cell::Cell;
    use std::rc::Rc;

    /// `request`/`allocate`/`draw`/`undraw` 호출을 기록하고 결정적 출력을 내는 mock layout.
    /// `BulletRenderGlyph` 의 forward 동작 (raw `FUN_002ed030` 등의 tail-call) 검증용.
    #[derive(Debug)]
    struct RecordingLayout {
        /// `request` 시 `req_out` 에 쓸 값.
        req_value: Requisition,
        /// `allocate` 시 `ext` 에 쓸 값.
        ext_value: Extension,
        request_calls: Rc<Cell<u32>>,
        allocate_calls: Rc<Cell<u32>>,
        draw_calls: Rc<Cell<u32>>,
        undraw_calls: Rc<Cell<u32>>,
    }

    impl Glyph for RecordingLayout {
        fn clone_glyph(&self) -> Box<dyn Glyph> {
            Box::new(RecordingLayout {
                req_value: self.req_value,
                ext_value: self.ext_value,
                request_calls: Rc::clone(&self.request_calls),
                allocate_calls: Rc::clone(&self.allocate_calls),
                draw_calls: Rc::clone(&self.draw_calls),
                undraw_calls: Rc::clone(&self.undraw_calls),
            })
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
        fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
            self
        }
        fn request(&self, req_out: &mut Requisition) {
            self.request_calls.set(self.request_calls.get() + 1);
            *req_out = self.req_value;
        }
        fn allocate(&mut self, _alloc: &Allocation, ext: &mut Extension) {
            self.allocate_calls.set(self.allocate_calls.get() + 1);
            *ext = self.ext_value;
        }
        fn draw(
            &mut self,
            _surface: &mut dyn kdsnr_render::surface::Surface,
            _alloc: &Allocation,
            _flag: &kdsnr_render::flag::Flag,
            _bw: &kdsnr_render::bw_mode::BWMode,
        ) {
            self.draw_calls.set(self.draw_calls.get() + 1);
        }
        fn undraw(&self, _flag: &kdsnr_render::flag::Flag) {
            self.undraw_calls.set(self.undraw_calls.get() + 1);
        }
    }

    fn sample_req() -> Requisition {
        Requisition {
            x: Requirement {
                natural: 12.0,
                stretch: 1.0,
                shrink: 2.0,
                alignment: 0.25,
            },
            y: Requirement {
                natural: 7.0,
                stretch: 0.0,
                shrink: 0.0,
                alignment: 0.5,
            },
            penalty: 3,
        }
    }

    fn make_bullet(layout: RecordingLayout) -> BulletRenderGlyph {
        BulletRenderGlyph {
            bullet_type: 3,
            layout: Box::new(layout),
            key_901: 4.5,
            numbering: 7,
        }
    }

    #[test]
    fn request_forwards_to_layout() {
        // raw FUN_002ed030: bullet render obj 의 Request 는 layout (Box_) 의 Request 로
        //   tail-call. → req_out 이 layout 의 출력으로 채워지고 layout.request 1회 호출.
        let calls = Rc::new(Cell::new(0));
        let layout = RecordingLayout {
            req_value: sample_req(),
            ext_value: Extension::default(),
            request_calls: Rc::clone(&calls),
            allocate_calls: Rc::new(Cell::new(0)),
            draw_calls: Rc::new(Cell::new(0)),
            undraw_calls: Rc::new(Cell::new(0)),
        };
        let bullet = make_bullet(layout);
        let mut out = Requisition::ZERO;
        bullet.request(&mut out);
        assert_eq!(calls.get(), 1, "layout.request 정확히 1회 forward");
        assert_eq!(out, sample_req(), "req_out = layout 의 Request 출력");
    }

    #[test]
    fn allocate_forwards_to_layout() {
        // raw FUN_002ed044: Allocate 는 layout 의 Allocate 로 forward.
        let calls = Rc::new(Cell::new(0));
        let ext_value = Extension {
            left: -1.0,
            top: -2.0,
            right: 3.0,
            bottom: 4.0,
        };
        let layout = RecordingLayout {
            req_value: Requisition::ZERO,
            ext_value,
            request_calls: Rc::new(Cell::new(0)),
            allocate_calls: Rc::clone(&calls),
            draw_calls: Rc::new(Cell::new(0)),
            undraw_calls: Rc::new(Cell::new(0)),
        };
        let mut bullet = make_bullet(layout);
        let alloc = Allocation {
            x: Allotment {
                origin: 10.0,
                span: 5.0,
                alignment: 0.0,
            },
            y: Allotment {
                origin: 20.0,
                span: 8.0,
                alignment: 0.0,
            },
        };
        let mut ext = Extension::default();
        bullet.allocate(&alloc, &mut ext);
        assert_eq!(calls.get(), 1);
        assert_eq!(ext, ext_value, "ext = layout 의 Allocate 출력");
    }

    #[test]
    fn draw_undraw_forward_to_layout() {
        // raw FUN_002ed058 / FUN_002ed06c: Draw/Undraw 도 layout 으로 forward.
        use kdsnr_render::flag::Flag;
        use kdsnr_render::bw_mode::BWMode;
        use kdsnr_render::svg_surface::SvgSurface;
        let draw_calls = Rc::new(Cell::new(0));
        let undraw_calls = Rc::new(Cell::new(0));
        let layout = RecordingLayout {
            req_value: Requisition::ZERO,
            ext_value: Extension::default(),
            request_calls: Rc::new(Cell::new(0)),
            allocate_calls: Rc::new(Cell::new(0)),
            draw_calls: Rc::clone(&draw_calls),
            undraw_calls: Rc::clone(&undraw_calls),
        };
        let mut bullet = make_bullet(layout);
        let mut surface = SvgSurface::new(100.0, 100.0);
        let alloc = Allocation::ZERO;
        let flag = Flag::new();
        let bw = BWMode::V0;
        bullet.draw(&mut surface, &alloc, &flag, &bw);
        bullet.undraw(&flag);
        assert_eq!(draw_calls.get(), 1);
        assert_eq!(undraw_calls.get(), 1);
    }

    #[test]
    fn clone_preserves_fields_and_layout() {
        // raw FUN_002ecfd8: Clone 은 bullet_type/key_901/numbering 복사 + layout SharePtr
        //   refcount++. Rust 는 layout deep-clone — clone 의 request 도 동일 출력.
        let layout = RecordingLayout {
            req_value: sample_req(),
            ext_value: Extension::default(),
            request_calls: Rc::new(Cell::new(0)),
            allocate_calls: Rc::new(Cell::new(0)),
            draw_calls: Rc::new(Cell::new(0)),
            undraw_calls: Rc::new(Cell::new(0)),
        };
        let bullet = make_bullet(layout);
        let cloned = bullet.clone_glyph();
        let cloned = cloned
            .as_any()
            .downcast_ref::<BulletRenderGlyph>()
            .expect("clone 은 BulletRenderGlyph");
        assert_eq!(cloned.bullet_type, 3);
        assert_eq!(cloned.key_901, 4.5);
        assert_eq!(cloned.numbering, 7);
        // clone 의 layout 도 동일 Request 출력.
        let mut out = Requisition::ZERO;
        cloned.request(&mut out);
        assert_eq!(out, sample_req());
    }

    #[test]
    fn forwards_to_real_hbox_layout() {
        // 실제 LayoutFactory HBox (`Box_`) 를 layout 으로 — forward chain 이 panic 없이
        //   동작하는지 (Box::Request) 확인.
        let hbox = crate::layout_factory::LayoutFactory::create_h_box();
        let bullet = BulletRenderGlyph {
            bullet_type: 1,
            layout: Box::new(hbox),
            key_901: 0.0,
            numbering: 1,
        };
        let mut out = Requisition::INVALID;
        bullet.request(&mut out);
        // Box::Request 가 child 없는 HBox 에 대해 결정적 출력을 냄 — 값 자체보다 forward
        //   체인이 동작하는 것이 핵심.
        let _ = out;
    }
}

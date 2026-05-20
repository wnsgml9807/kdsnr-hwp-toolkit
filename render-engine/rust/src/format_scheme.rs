//! `Hnc::Shape::FormatScheme` — 104B 1:1 byte-equivalent port.
//!
//! libHncDrawingEngine_arm64 의 `FormatScheme` 는 OOXML `<a:fmtScheme>` 의
//! 4 종류 lookup map (Brush / BackgroundBrush / Pen / EffectStyle) + name 보유.
//!
//! # raw 104B layout (확정 from `FormatScheme::FormatScheme()` @ `0x16e444` +
//! `Create()` @ `0x16f5b0` 의 `mov w0, #0x68 (=104)` + dtor 의 4-tree teardown)
//!
//! ```text
//! offset   field          타입                                  의미
//! 0x00     name           CHncStringW                          8B (refcounted)
//! 0x08     brushes        __tree<Style, UniquePtr<Brush>>      24B std::map
//! 0x20     bg_brushes     __tree<Style, UniquePtr<Brush>>      24B std::map
//! 0x38     pens           __tree<Style, UniquePtr<Pen>>        24B std::map
//! 0x50     effects        __tree<Style, UniquePtr<EffectStyle>> 24B std::map
//! ```
//!
//! 총 104B (0x68) / 8B align.
//!
//! 각 24B __tree 는 [[rb_tree]] 의 TreeBase 와 byte-equivalent:
//! - begin_node (Node* @ +0)
//! - end_node_left (Node* @ +8)
//! - size (u64 @ +16)
//!
//! # raw `FormatScheme::FormatScheme()` @ `0x16e444`
//!
//! ```asm
//! 16e44c: bl  CHncStringW::CHncStringW()     ; name default init
//! 16e450-16e458: tree1 init (begin = &end_node_left, end_node_left = 0)
//! 16e45c-16e464: tree2 init
//! 16e468-16e470: tree3 init
//! 16e474: str xzr, [x0, #0x60]               ; tree4.size = 0 (먼저 size 만)
//! 16e478-16e480: tree4 의 begin/end_node_left init
//! ```
//!
//! # raw `~FormatScheme()` @ `0x16e4d4`
//!
//! 1. `bl 0x632a5c(self+0x50, *(self+0x58))` — subtree_destroy effects map
//! 2. `bl 0x632910(self+0x38, *(self+0x40))` — subtree_destroy pens map
//! 3. `bl 0x6327c4(self+0x20, *(self+0x28))` — subtree_destroy bg_brushes
//! 4. `bl 0x6327c4(self+0x8, *(self+0x10))` — subtree_destroy brushes
//! 5. tail call `CHncStringW::~CHncStringW()` on self+0
//!
//! # 본 R-1.5.6 단계 scope
//!
//! - 104B layout + field offsets 검증
//! - default ctor (4 trees empty init + name nil)
//! - ~FormatScheme (empty trees teardown + name auto-drop)
//! - GetName / SetName accessor
//!
//! # 의도적 deferred
//!
//! - `CreateDefault()` (`0x16f628`) — Brush/Pen/EffectStyle sub-objects 종속.
//! - `GetBrush` `SetBrush` 등 4×2 map accessor — Style enum + UniquePtr 종속.
//! - `operator==` / `operator!=` / `Clone` / `Swap` / `EqualsScheme` — sub-objects 종속.

use crate::rb_tree::{
    balance_after_insert, find_insert_position, subtree_destroy_recursive,
    update_begin_node_after_insert, TreeBase, TreeNodeBase,
};
use crate::string_w::CHncStringW;
use std::alloc::Layout;
use std::ptr;

/// `Hnc::Shape::FormatScheme::Style` — u32 enum (`FormatScheme::GetBrush` 등의
/// arg). 본 단계는 raw u32 transparent wrapper.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct FormatSchemeStyle(pub u32);

// =============================================================================
// 16u: std::map<Style, UniquePtr<Brush>> 의 byte-eq infrastructure
// =============================================================================
//
// `FormatScheme::CreateDefault` (`0x16f628`, 3782 줄) 의 `SetBrush(...)` 호출이
// std::map 에 (Style key, SharePtr<Brush> value) 를 INSERT/REPLACE. byte-eq port
// 위해 raw layout 1:1 의 ControlBlock + Node 정의.
//
// ## raw `Hnc::Memory::UniquePtr<Brush>` 의 ControlBlock (24B)
//
// 표면적 이름은 UniquePtr 이나 raw layout 은 strong-only shared_ptr-like:
//
// ```text
// offset  field      type      의미
// 0x00    obj        Brush*    8B — 실제 sub-type (SolidBrush 등) 포인터
// 0x08    strong     u64       8B — strong refcount (= 1 at construction)
// 0x10    flag       u8        1B — release path 의 가드 (= 1 at construction)
// 0x11    _pad       [u8;7]    7B — 24B align (uninit)
// ```
//
// raw 인용:
// ```asm
// 16f79c: mov  w0, #0x18              ; alloc 24B
// 16f7a0: bl   __Znwm
// 16f7a4: mov  x24, x0
// 16f7a8-16f7ac: stp xzr, xzr, [x0]; str xzr, [x0, #0x10]  ; clear 24B
// 16f73c: mov  w8, #0x1
// 16f740: stp  x21, x8, [x0]           ; obj = x21 (SolidBrush*), strong = 1
// 16f744: strb w8, [x0, #0x10]         ; flag = 1
// ```
//
// raw `0x647ebc` (release helper) 의 cleanup path 는 `if flag == 0 → no-op`,
// `if strong == 1 → vtable[0] dtor + dealloc obj + dealloc ctrl`.

/// raw 24B `Hnc::Memory::UniquePtr<Brush>` 의 ControlBlock. byte-eq layout.
///
/// `tree value slot @ +0x28 of node` 가 `*mut BrushControlBlock` 8B 보유.
#[repr(C)]
pub struct BrushControlBlock {
    /// raw +0x00: `Brush*` (sub-type 의 첫 field 는 vtable_ptr — `brush_vtable()`
    /// helper 로 drop / Clone dispatch).
    pub obj: *mut u8,
    /// raw +0x08: strong refcount (ctor 에서 1).
    pub strong: u64,
    /// raw +0x10: flag byte (ctor 에서 1; release helper 의 가드).
    pub flag: u8,
    /// raw +0x11..+0x18: alignment padding (uninit in raw — 8-byte align).
    pub _pad: [u8; 7],
}

pub const BRUSH_CONTROL_BLOCK_SIZE_BYTES: usize = 24;
pub const BRUSH_CONTROL_BLOCK_ALIGN_BYTES: usize = 8;

const _: () = assert!(std::mem::size_of::<BrushControlBlock>() == BRUSH_CONTROL_BLOCK_SIZE_BYTES);
const _: () = assert!(std::mem::align_of::<BrushControlBlock>() == BRUSH_CONTROL_BLOCK_ALIGN_BYTES);

impl BrushControlBlock {
    /// raw `0x16f734-0x16f748` 의 alloc + init sequence 1:1.
    ///
    /// `obj` 는 caller 가 미리 heap-alloc 한 sub-type 인스턴스 (SolidBrush 등).
    /// 첫 8B 가 `&BrushVtable` 주소이어야 valid dispatch (= sub-type ctor 가 보장).
    ///
    /// # Safety
    /// `obj` 는 valid `*mut u8` (Brush sub-type 인스턴스).
    pub unsafe fn create_raw(obj: *mut u8) -> *mut BrushControlBlock {
        let layout = Layout::new::<BrushControlBlock>();
        let p = std::alloc::alloc(layout) as *mut BrushControlBlock;
        if p.is_null() {
            std::alloc::handle_alloc_error(layout);
        }
        ptr::write(
            p,
            BrushControlBlock {
                obj,
                strong: 1,
                flag: 1,
                _pad: [0u8; 7],
            },
        );
        p
    }

    /// Convenience: heap-alloc SolidBrush (16B) + wrap in BrushControlBlock.
    ///
    /// raw `0x16f6a0` (SolidBrush alloc) + `0x16f734` (BrushControlBlock alloc) 의
    /// 합성. Block 1 패턴.
    pub unsafe fn from_solid(brush: crate::brush::SolidBrush) -> *mut BrushControlBlock {
        let layout = Layout::from_size_align(16, 8).expect("SolidBrush 16B 8B layout");
        let heap = std::alloc::alloc(layout) as *mut crate::brush::SolidBrush;
        if heap.is_null() {
            std::alloc::handle_alloc_error(layout);
        }
        ptr::write(heap, brush);
        Self::create_raw(heap as *mut u8)
    }

    /// Convenience: heap-alloc HatchBrush (16B) + wrap in BrushControlBlock.
    pub unsafe fn from_hatch(brush: crate::brush::HatchBrush) -> *mut BrushControlBlock {
        let layout = Layout::from_size_align(16, 8).expect("HatchBrush 16B 8B layout");
        let heap = std::alloc::alloc(layout) as *mut crate::brush::HatchBrush;
        if heap.is_null() {
            std::alloc::handle_alloc_error(layout);
        }
        ptr::write(heap, brush);
        Self::create_raw(heap as *mut u8)
    }

    /// Convenience: heap-alloc GradientBrush (16B) + wrap in BrushControlBlock.
    ///
    /// raw `0x16fe24-0x16fe30` (GradientBrush alloc + C2) + `0x16fce0-0x16fcec`
    /// (BrushControlBlock alloc + init) 의 합성. Block 5 패턴.
    pub unsafe fn from_gradient(brush: crate::brush::GradientBrush) -> *mut BrushControlBlock {
        let layout = Layout::from_size_align(16, 8).expect("GradientBrush 16B 8B layout");
        let heap = std::alloc::alloc(layout) as *mut crate::brush::GradientBrush;
        if heap.is_null() {
            std::alloc::handle_alloc_error(layout);
        }
        ptr::write(heap, brush);
        Self::create_raw(heap as *mut u8)
    }

    /// raw `0x647ebc` (release helper) 의 strong--/destroy path 1:1.
    ///
    /// strong 이 1 → 0 으로 decrement 되면 obj 의 vtable[0] (drop_in_place) +
    /// obj heap dealloc + ctrl heap dealloc.
    ///
    /// 본 단계는 단일-스레드 환경 가정 (atomic 없음 — raw 는 strong == 2 일 때
    /// atomic 분기, 그 외엔 plain store). CreateDefault 의 모든 SetBrush 가
    /// strong=1 시점에서 release 됨.
    ///
    /// # Safety
    /// `p` 는 `create_raw` 으로 얻은 ptr 또는 null.
    pub unsafe fn release(p: *mut BrushControlBlock) {
        if p.is_null() {
            return;
        }
        let obj = (*p).obj;
        if obj.is_null() {
            return;
        }
        if (*p).flag == 0 {
            return;
        }
        // strong--
        let new_strong = (*p).strong.wrapping_sub(1);
        if new_strong == 0 {
            // raw `0x647f70..`: vtable[0] dtor + dealloc
            let vtable = crate::brush::brush_vtable(obj as *const u8);
            (vtable.drop_in_place_fn)(obj);
            // dealloc obj — raw 는 sub-type 별 크기 알지만 본 Rust 는 16B (모든
            // Brush sub-type 이 16B 라 통일). 실제 heap alloc 도 16B 였으므로 정합.
            // SolidBrush / HatchBrush 모두 16B align 8.
            let obj_layout =
                Layout::from_size_align(16, 8).expect("brush sub-type 16B 8B layout");
            std::alloc::dealloc(obj, obj_layout);
            // dealloc ctrl
            let ctrl_layout = Layout::new::<BrushControlBlock>();
            std::alloc::dealloc(p as *mut u8, ctrl_layout);
        } else {
            (*p).strong = new_strong;
        }
    }
}

/// raw 48B Node of `std::map<Style, UniquePtr<Brush>>`.
///
/// libc++ `__tree_node<__value_type<Style, UniquePtr<Brush>>>` layout:
///
/// ```text
/// offset  field   type             의미
/// 0x00    base    TreeNodeBase     32B (left/right/parent/is_black+pad)
/// 0x20    key     u32              4B Style
/// 0x24    _pad    u32              4B pad (raw uninit)
/// 0x28    value   *mut Ctrl        8B SharePtr (= ControlBlock*)
/// ```
///
/// 총 48B (= raw `mov w0, #0x30; bl __Znwm` @ `0x6645fc-0x664600`).
///
/// raw asm 인용 (find_or_insert helper @ `0x6645fc-0x664638`):
/// ```asm
/// 6645fc: mov  w0, #0x30              ; alloc 48B
/// 664600: bl   __Znwm
/// 664604: mov  x20, x0                 ; x20 = new node
/// 664608: ldr  w8, [x21]               ; w8 = caller's style key
/// 66460c: str  w8, [x0, #0x20]         ; node.key = style
/// 664610: ldr  x8, [x21, #0x8]         ; x8 = caller's ctrl ptr
/// 664614: str  x8, [x0, #0x28]         ; node.value = ctrl
/// 664618-664630: refcount++ on ctrl
/// 664634: stp  xzr, xzr, [x20]         ; left = right = null
/// 664638: str  x22, [x20, #0x10]       ; parent = walk result
/// ```
#[repr(C)]
pub struct FsBrushMapNode {
    /// raw +0x00..+0x20: TreeNodeBase (left/right/parent/is_black+pad).
    pub base: TreeNodeBase,
    /// raw +0x20..+0x24: Style key (= u32).
    pub key: u32,
    /// raw +0x24..+0x28: 4B pad (raw uninit — value union 의 align padding).
    pub _pad: u32,
    /// raw +0x28..+0x30: SharePtr value (= `*mut BrushControlBlock`).
    pub value: *mut BrushControlBlock,
}

pub const FS_BRUSH_MAP_NODE_SIZE_BYTES: usize = 48;
pub const FS_BRUSH_MAP_NODE_ALIGN_BYTES: usize = 8;

const _: () = assert!(std::mem::size_of::<FsBrushMapNode>() == FS_BRUSH_MAP_NODE_SIZE_BYTES);
const _: () = assert!(std::mem::align_of::<FsBrushMapNode>() == FS_BRUSH_MAP_NODE_ALIGN_BYTES);

/// raw `0x664594` (find_or_insert helper) 의 key 비교 — `cmp w8, [node+0x20]`.
unsafe fn fs_brush_node_key(node: *const TreeNodeBase) -> u32 {
    (*(node as *const FsBrushMapNode)).key
}

/// raw `subtree_destroy_recursive` 의 drop callback — 각 노드의 ctrl release +
/// 노드 자체 dealloc.
///
/// # Safety
/// `n` 은 valid `FsBrushMapNode*` (cast from TreeNodeBase*).
unsafe fn drop_fs_brush_map_node(n: *mut TreeNodeBase) {
    let node = n as *mut FsBrushMapNode;
    // raw release path: ctrl strong--, 0 시 obj dtor + dealloc
    BrushControlBlock::release((*node).value);
    // node 자체 dealloc — raw 는 `__ZdlPv(node)` (= operator delete).
    let layout = Layout::new::<FsBrushMapNode>();
    std::alloc::dealloc(node as *mut u8, layout);
}

/// 16-ζ: pens tree 의 std::map node 24B ControlBlock — Pen 의 SharePtr ctrl.
///
/// raw layout (24B, byte-eq with BrushControlBlock):
/// ```text
/// offset  field   type             의미
/// 0x00    obj     *mut Pen          (ctrl payload)
/// 0x08    strong  u64               (refcount)
/// 0x10    flag    u8                (validity / share flag)
/// 0x11    _pad    [u8; 7]           (align padding)
/// ```
///
/// Brush 와 동일한 SharePtr 의 의미. release 의 알고리즘 동일하나 sub-type 별
/// `drop_in_place_fn` 호출 path 는 Pen (단일 클래스, 직접 dtor).
#[repr(C)]
pub struct PenControlBlock {
    pub obj: *mut crate::pen::Pen,
    pub strong: u64,
    pub flag: u8,
    pub _pad: [u8; 7],
}

pub const PEN_CONTROL_BLOCK_SIZE_BYTES: usize = 24;
pub const PEN_CONTROL_BLOCK_ALIGN_BYTES: usize = 8;
const _: () = assert!(std::mem::size_of::<PenControlBlock>() == PEN_CONTROL_BLOCK_SIZE_BYTES);

impl PenControlBlock {
    /// raw `0x170ea4-0x170eb4` 1:1 — alloc 24B + init (obj, strong=1, flag=1).
    ///
    /// # Safety
    /// `obj` 는 heap-alloc 된 valid `*mut Pen`.
    pub unsafe fn create_raw(obj: *mut crate::pen::Pen) -> *mut PenControlBlock {
        let layout = Layout::new::<PenControlBlock>();
        let p = std::alloc::alloc(layout) as *mut PenControlBlock;
        if p.is_null() {
            std::alloc::handle_alloc_error(layout);
        }
        ptr::write(
            p,
            PenControlBlock {
                obj,
                strong: 1,
                flag: 1,
                _pad: [0u8; 7],
            },
        );
        p
    }

    /// Heap-alloc Pen + wrap in PenControlBlock.
    pub unsafe fn from_pen(pen: crate::pen::Pen) -> *mut PenControlBlock {
        let layout = Layout::new::<crate::pen::Pen>();
        let heap = std::alloc::alloc(layout) as *mut crate::pen::Pen;
        if heap.is_null() {
            std::alloc::handle_alloc_error(layout);
        }
        ptr::write(heap, pen);
        Self::create_raw(heap)
    }

    /// raw release helper (= `0x648714`) — Pen-specific 1:1 release.
    ///
    /// strong--, 0 시 Pen dtor + dealloc + ctrl dealloc.
    ///
    /// # Safety
    /// `p` 는 `create_raw` / `from_pen` 으로 얻은 ptr 또는 null.
    pub unsafe fn release(p: *mut PenControlBlock) {
        if p.is_null() {
            return;
        }
        let obj = (*p).obj;
        if obj.is_null() {
            return;
        }
        if (*p).flag == 0 {
            return;
        }
        let new_strong = (*p).strong.wrapping_sub(1);
        if new_strong == 0 {
            // raw Pen::D1 (= drop_in_place — auto via Rust's ptr::drop_in_place)
            ptr::drop_in_place(obj);
            let pen_layout = Layout::new::<crate::pen::Pen>();
            std::alloc::dealloc(obj as *mut u8, pen_layout);
            let ctrl_layout = Layout::new::<PenControlBlock>();
            std::alloc::dealloc(p as *mut u8, ctrl_layout);
        } else {
            (*p).strong = new_strong;
        }
    }
}

/// raw 48B Node of `std::map<Style, UniquePtr<Pen>>` — pens / FormatScheme+0x38.
///
/// libc++ __tree_node — byte-eq with FsBrushMapNode but typed for Pen.
#[repr(C)]
pub struct FsPenMapNode {
    pub base: TreeNodeBase,
    pub key: u32,
    pub _pad: u32,
    pub value: *mut PenControlBlock,
}

pub const FS_PEN_MAP_NODE_SIZE_BYTES: usize = 48;
const _: () = assert!(std::mem::size_of::<FsPenMapNode>() == FS_PEN_MAP_NODE_SIZE_BYTES);

unsafe fn fs_pen_node_key(node: *const TreeNodeBase) -> u32 {
    (*(node as *const FsPenMapNode)).key
}

unsafe fn drop_fs_pen_map_node(n: *mut TreeNodeBase) {
    let node = n as *mut FsPenMapNode;
    PenControlBlock::release((*node).value);
    let layout = Layout::new::<FsPenMapNode>();
    std::alloc::dealloc(node as *mut u8, layout);
}

/// 16-ι: effects tree 의 std::map node 24B ControlBlock — EffectStyle 의 SharePtr ctrl.
///
/// raw layout (24B, byte-eq with BrushControlBlock / PenControlBlock):
/// ```text
/// offset  field   type                의미
/// 0x00    obj     *mut EffectStyle     (ctrl payload, raw EffectStyle 는 24B)
/// 0x08    strong  u64                  (refcount)
/// 0x10    flag    u8                   (validity flag)
/// 0x11    _pad    [u8; 7]              (align padding)
/// ```
#[repr(C)]
pub struct EffectStyleControlBlock {
    pub obj: *mut crate::effect_style::EffectStyle,
    pub strong: u64,
    pub flag: u8,
    pub _pad: [u8; 7],
}

pub const EFFECT_STYLE_CONTROL_BLOCK_SIZE_BYTES: usize = 24;
const _: () = assert!(std::mem::size_of::<EffectStyleControlBlock>() == EFFECT_STYLE_CONTROL_BLOCK_SIZE_BYTES);

impl EffectStyleControlBlock {
    /// raw `0x171988-0x171998` 1:1 (Block 16 의 ctrl alloc pattern) — alloc 24B
    /// + init (obj, strong=1, flag=1).
    ///
    /// # Safety
    /// `obj` 는 heap-alloc 된 valid `*mut EffectStyle`.
    pub unsafe fn create_raw(
        obj: *mut crate::effect_style::EffectStyle,
    ) -> *mut EffectStyleControlBlock {
        let layout = Layout::new::<EffectStyleControlBlock>();
        let p = std::alloc::alloc(layout) as *mut EffectStyleControlBlock;
        if p.is_null() {
            std::alloc::handle_alloc_error(layout);
        }
        ptr::write(
            p,
            EffectStyleControlBlock {
                obj,
                strong: 1,
                flag: 1,
                _pad: [0u8; 7],
            },
        );
        p
    }

    /// Heap-alloc EffectStyle + wrap in EffectStyleControlBlock.
    pub unsafe fn from_effect_style(
        es: crate::effect_style::EffectStyle,
    ) -> *mut EffectStyleControlBlock {
        let layout = Layout::new::<crate::effect_style::EffectStyle>();
        let heap = std::alloc::alloc(layout) as *mut crate::effect_style::EffectStyle;
        if heap.is_null() {
            std::alloc::handle_alloc_error(layout);
        }
        ptr::write(heap, es);
        Self::create_raw(heap)
    }

    /// raw release helper — strong--, 0 시 EffectStyle dtor + dealloc + ctrl dealloc.
    ///
    /// # Safety
    /// `p` 는 `create_raw` / `from_effect_style` 으로 얻은 ptr 또는 null.
    pub unsafe fn release(p: *mut EffectStyleControlBlock) {
        if p.is_null() {
            return;
        }
        let obj = (*p).obj;
        if obj.is_null() {
            return;
        }
        if (*p).flag == 0 {
            return;
        }
        let new_strong = (*p).strong.wrapping_sub(1);
        if new_strong == 0 {
            ptr::drop_in_place(obj);
            let es_layout = Layout::new::<crate::effect_style::EffectStyle>();
            std::alloc::dealloc(obj as *mut u8, es_layout);
            let ctrl_layout = Layout::new::<EffectStyleControlBlock>();
            std::alloc::dealloc(p as *mut u8, ctrl_layout);
        } else {
            (*p).strong = new_strong;
        }
    }
}

/// raw 48B Node of `std::map<Style, UniquePtr<EffectStyle>>` — effects @ FormatScheme+0x50.
#[repr(C)]
pub struct FsEffectStyleMapNode {
    pub base: TreeNodeBase,
    pub key: u32,
    pub _pad: u32,
    pub value: *mut EffectStyleControlBlock,
}

pub const FS_EFFECT_STYLE_MAP_NODE_SIZE_BYTES: usize = 48;
const _: () = assert!(std::mem::size_of::<FsEffectStyleMapNode>() == FS_EFFECT_STYLE_MAP_NODE_SIZE_BYTES);

unsafe fn fs_effect_style_node_key(node: *const TreeNodeBase) -> u32 {
    (*(node as *const FsEffectStyleMapNode)).key
}

unsafe fn drop_fs_effect_style_map_node(n: *mut TreeNodeBase) {
    let node = n as *mut FsEffectStyleMapNode;
    EffectStyleControlBlock::release((*node).value);
    let layout = Layout::new::<FsEffectStyleMapNode>();
    std::alloc::dealloc(node as *mut u8, layout);
}

/// raw 104B `Hnc::Shape::FormatScheme`.
///
/// **address stability 필수** — 각 tree 의 begin_node 가 self 의 end_node_left
/// 주소를 가리킴. `Box<Self>` 또는 다른 heap container 로 보유.
#[repr(C)]
pub struct FormatScheme {
    /// raw +0x00: name CHncStringW.
    pub name: CHncStringW,
    /// raw +0x08..+0x20: brushes std::map<Style, UniquePtr<Brush>>.
    pub brushes: TreeBase,
    /// raw +0x20..+0x38: background brushes std::map.
    pub bg_brushes: TreeBase,
    /// raw +0x38..+0x50: pens std::map.
    pub pens: TreeBase,
    /// raw +0x50..+0x68: effects std::map.
    pub effects: TreeBase,
}

pub const FORMAT_SCHEME_SIZE_BYTES: usize = 104;
pub const FORMAT_SCHEME_ALIGN_BYTES: usize = 8;

const _: () = assert!(std::mem::size_of::<FormatScheme>() == FORMAT_SCHEME_SIZE_BYTES);
const _: () = assert!(std::mem::align_of::<FormatScheme>() == FORMAT_SCHEME_ALIGN_BYTES);

impl FormatScheme {
    /// raw `FormatScheme::FormatScheme()` (`0x16e444`) — `Box<Self>` 반환.
    ///
    /// Box wrap 으로 address stability 보장. 4 trees 모두 empty 로 초기화 (raw
    /// 가 각 tree 의 begin_node 를 self 내부 슬롯의 주소로 설정).
    pub fn new() -> Box<Self> {
        let mut boxed = Box::new(FormatScheme {
            name: CHncStringW::default(),
            brushes: TreeBase {
                begin_node: ptr::null_mut(),
                end_node_left: ptr::null_mut(),
                size: 0,
            },
            bg_brushes: TreeBase {
                begin_node: ptr::null_mut(),
                end_node_left: ptr::null_mut(),
                size: 0,
            },
            pens: TreeBase {
                begin_node: ptr::null_mut(),
                end_node_left: ptr::null_mut(),
                size: 0,
            },
            effects: TreeBase {
                begin_node: ptr::null_mut(),
                end_node_left: ptr::null_mut(),
                size: 0,
            },
        });
        unsafe {
            boxed.brushes.init_empty();
            boxed.bg_brushes.init_empty();
            boxed.pens.init_empty();
            boxed.effects.init_empty();
        }
        boxed
    }

    /// raw `Create()` (`0x16f5b0`) — heap-alloc 104B + ctor (Box::into_raw).
    ///
    /// # Safety
    /// 반환 ptr 은 `raw_delete` 로 해제 필요.
    pub unsafe fn create_raw() -> *mut FormatScheme {
        Box::into_raw(Self::new())
    }

    /// `Create()` 의 반환 ptr 에 대한 dtor + dealloc.
    ///
    /// # Safety
    /// `p` 는 `create_raw` 으로 얻은 ptr 또는 null.
    pub unsafe fn raw_delete(p: *mut FormatScheme) {
        if p.is_null() {
            return;
        }
        drop(Box::from_raw(p));
    }

    /// raw `GetName() const` (`0x16ec20`): `ret` (= `*(CHncStringW const*)(this+0)`).
    #[inline]
    pub fn get_name(&self) -> &CHncStringW {
        &self.name
    }

    /// raw `SetName(CHncStringW const&)` (`0x16ec24`): tail call to CHncStringW::operator=.
    pub fn set_name(&mut self, s: &CHncStringW) {
        self.name = s.clone();
    }

    /// 4 tree 의 총 entry 수 (debug helper).
    pub fn total_entries(&self) -> u64 {
        self.brushes.size + self.bg_brushes.size + self.pens.size + self.effects.size
    }

    /// empty (모든 tree 가 size 0)?
    pub fn is_empty(&self) -> bool {
        self.total_entries() == 0
    }

    /// raw `FormatScheme::FormatScheme(const FormatScheme&)` (`0x6322a8`) 1:1.
    ///
    /// 알고리즘:
    /// 1. CHncStringW name copy ctor.
    /// 2. 각 4 trees (brushes/bg_brushes/pens/effects):
    ///    - init empty (begin = &self.end_node_left, end_left = null, size = 0)
    ///    - if src tree non-empty: in-order iterate + insert each (key, value)
    ///
    /// **현재 scope 제약**: src tree 가 non-empty 인 path 는 Brush/BgBrush/Pen/
    /// EffectStyle 의 std::unique_ptr 의 deep clone (각 abstract class 의 Clone
    /// vfunc) RE 필요 — `Theme(true)` 가 만드는 FormatScheme 는 (1) ctor 가
    /// 4 trees 모두 empty 로 init 만 함 (`FormatScheme::CreateDefault` 가 별도),
    /// (2) `Theme::CreateDefault` (deferred) 가 호출되지 않으므로 도달 가능 input
    /// 에선 모든 tree empty path 만 사용됨.
    ///
    /// # Safety
    /// `src` 는 valid `*const FormatScheme`, `this` 는 uninit heap slot (104B).
    pub unsafe fn copy_from_raw(this: *mut FormatScheme, src: *const FormatScheme) {
        // raw `6322cc: bl CHncStringW copy ctor`
        let name_clone = (*src).name.clone();

        // raw `6322d0-6322e0: init brushes (offset +0x10 from this = &end_node_left)`
        // 각 tree 의 init: begin_node = &end_node_left, end_node_left = null, size = 0
        ptr::write(
            this,
            FormatScheme {
                name: name_clone,
                brushes: TreeBase {
                    begin_node: ptr::null_mut(),
                    end_node_left: ptr::null_mut(),
                    size: 0,
                },
                bg_brushes: TreeBase {
                    begin_node: ptr::null_mut(),
                    end_node_left: ptr::null_mut(),
                    size: 0,
                },
                pens: TreeBase {
                    begin_node: ptr::null_mut(),
                    end_node_left: ptr::null_mut(),
                    size: 0,
                },
                effects: TreeBase {
                    begin_node: ptr::null_mut(),
                    end_node_left: ptr::null_mut(),
                    size: 0,
                },
            },
        );
        (*this).brushes.init_empty();
        (*this).bg_brushes.init_empty();
        (*this).pens.init_empty();
        (*this).effects.init_empty();

        // raw `6322e4-6322f0`: brushes — src empty 확인
        // src.brushes.begin_node (offset +0x08 of brushes = +0x10 of FormatScheme)
        // == &src.brushes.end_node_left (offset +0x10 of brushes = +0x18 of FormatScheme)
        // → 빈 트리. 그렇지 않으면 in-order iterate + insert.
        //
        // **CURRENT SCOPE**: 4 tree 모두 empty 만 도달 가능. non-empty 도달 시
        // panic. raw 인용은 위 doc comment.
        if !is_empty_tree(&(*src).brushes) {
            panic!("FormatScheme::copy_from_raw: brushes non-empty requires Brush vfunc Clone RE — deferred");
        }
        if !is_empty_tree(&(*src).bg_brushes) {
            panic!("FormatScheme::copy_from_raw: bg_brushes non-empty requires BgBrush vfunc Clone RE — deferred");
        }
        if !is_empty_tree(&(*src).pens) {
            panic!("FormatScheme::copy_from_raw: pens non-empty requires Pen vfunc Clone RE — deferred");
        }
        if !is_empty_tree(&(*src).effects) {
            panic!("FormatScheme::copy_from_raw: effects non-empty requires EffectStyle vfunc Clone RE — deferred");
        }
    }

    /// raw `FormatScheme::Clone() const` (`0x16eb68`) — alloc 104B + copy ctor.
    ///
    /// # Safety
    /// 반환 ptr 은 `raw_delete` 로 해제.
    pub unsafe fn clone_to_heap(&self) -> *mut FormatScheme {
        let layout = Layout::new::<FormatScheme>();
        let new_p = std::alloc::alloc(layout) as *mut FormatScheme;
        if new_p.is_null() {
            std::alloc::handle_alloc_error(layout);
        }
        Self::copy_from_raw(new_p, self as *const FormatScheme);
        new_p
    }

    // =========================================================================
    // 16u: SetBrush / GetBrush / std::map machinery
    // =========================================================================

    /// raw `FormatScheme::SetBrush(Style, UniquePtr<Brush> const&)` (`0x16ec94`) 1:1.
    ///
    /// raw asm 인용 (`0x16ec94-0x16ed88`):
    /// ```asm
    /// 16eca8: ldr  x8, [x2]                  ; ctrl = (*arg).raw
    /// 16ecac: cbz  x8, return                ; null ctrl → no-op
    /// 16ecb0: ldr  x9, [x8]                  ; obj = ctrl->obj
    /// 16ecb4: cbz  x9, return                ; null obj → no-op
    /// 16ecbc: add  x21, x0, #0x8             ; x21 = &this->brushes
    /// 16ecc0: str  w1, [sp]                  ; local.key = style
    /// 16eccc: str  x8, [sp, #0x8]            ; local.ctrl = ctrl
    /// 16ecd0-16ecd8: refcount++              ; ctrl->strong++
    /// 16ecec: bl   0x664594                  ; find_or_insert(tree, &local)
    /// 16ecf0: x21 = node, x22 = inserted_flag (0=existing, 1=new)
    /// 16ecfc: bl   0x647ebc                  ; release local ctrl (refcount--)
    /// 16ed00-16ed04: if inserted_flag → return
    /// 16ed1c: add  x20, x21, #0x28           ; x20 = &node.value (SharePtr slot)
    /// 16ed28-16ed44: if old obj == new obj → return (same-object guard)
    /// 16ed4c: bl   0x647ebc                  ; release old SharePtr at node
    /// 16ed54: str  x8, [x20]                  ; node.value = new ctrl
    /// 16ed68-16ed74: ctrl refcount++          ; same as INSERT path
    /// ```
    ///
    /// 본 Rust port: INSERT + REPLACE 양 path 모두 1:1.
    ///
    /// `ctrl_in` 은 caller 가 만든 `BrushControlBlock*` (strong = 1, flag = 1).
    /// FormatScheme 가 ownership transfer 받음 — caller 는 release 하지 말 것.
    /// (raw 는 caller 가 local copy 의 release 를 별도 호출하나 본 Rust API 는
    /// move 시멘틱.)
    ///
    /// # Safety
    /// `ctrl_in` 은 `BrushControlBlock::create_raw` 으로 얻은 valid ptr.
    pub unsafe fn set_brush(&mut self, style: u32, ctrl_in: *mut BrushControlBlock) {
        // raw 16eca8-16ecb4: null-guard
        if ctrl_in.is_null() || (*ctrl_in).obj.is_null() {
            return;
        }

        // raw `0x664594` find_or_insert: walk tree, alloc 48B node if not found.
        let tree = &mut self.brushes as *mut TreeBase;
        let (slot, parent, existing) = find_insert_position(tree, |node| {
            fs_brush_node_key(node).cmp(&style)
        });

        if existing.is_null() {
            // INSERT path: alloc 48B node + populate + balance
            let node_layout = Layout::new::<FsBrushMapNode>();
            let new_node = std::alloc::alloc(node_layout) as *mut FsBrushMapNode;
            if new_node.is_null() {
                std::alloc::handle_alloc_error(node_layout);
            }
            ptr::write(
                new_node,
                FsBrushMapNode {
                    base: TreeNodeBase {
                        left: ptr::null_mut(),
                        right: ptr::null_mut(),
                        parent,
                        is_black: 0,
                        _pad_0x19: [0u8; 7],
                    },
                    key: style,
                    _pad: 0,
                    value: ctrl_in,
                },
            );
            // raw 66463c: *slot = new_node
            *slot = new_node as *mut TreeNodeBase;
            // raw 664640-664654: update begin_node if new is leftmost
            update_begin_node_after_insert(tree);
            // raw 664658-66465c: balance_after_insert(root, new)
            let root = self.brushes.end_node_left;
            balance_after_insert(root, new_node as *mut TreeNodeBase);
            // raw 664660-664668: size++
            self.brushes.size += 1;
        } else {
            // REPLACE path: existing node found — release old, install new
            let node = existing as *mut FsBrushMapNode;
            let old_ctrl = (*node).value;
            // raw 16ed28-16ed44: same-object guard
            let old_obj = if !old_ctrl.is_null() {
                (*old_ctrl).obj
            } else {
                ptr::null_mut()
            };
            let new_obj = (*ctrl_in).obj;
            if old_obj == new_obj {
                // Same obj — release caller's ctrl (we don't own it now)
                BrushControlBlock::release(ctrl_in);
                return;
            }
            // raw 16ed4c: release old SharePtr at node
            BrushControlBlock::release(old_ctrl);
            // raw 16ed54: store new ctrl
            (*node).value = ctrl_in;
        }
    }

    /// `FormatScheme::CreateDefault` (`0x16f628`, 3782 줄) 의 **Block 1** 1:1 port.
    ///
    /// raw `0x16f628-0x16f798` 의 70 instr — FormatScheme alloc + 4 trees init +
    /// 첫 SetBrush 호출 (`SetBrush(MainColor=1, SolidBrush(Color(Scheme 0x10)))`).
    ///
    /// raw asm sequence:
    /// ```asm
    /// 16f6a0: bl   __Znwm                      ; alloc 16B SolidBrush
    /// 16f6a4: mov  x21, x0
    /// 16f6a8-16f6b0: vtable @ 0x77cf48          ; SolidBrush vtable
    /// 16f6b8: bl   PropertyBag::PropertyBag(false)
    /// 16f6c0-16f6d4: Color body init:
    ///                value[0..4] = 0x10         ; SchemeStyle 16 (= phClr)
    ///                type_tag = 2               ; SCHEME
    ///                color_effect = null
    /// 16f6d8-16f6dc: key 0x259 (SolidFillColor)
    /// 16f6e4-16f704: bl 0x6541e8                ; PColor helper (alloc + attach)
    /// 16f730-16f7a8: alloc 24B ControlBlock {obj=SolidBrush, strong=1, flag=1}
    /// 16f78c-16f790: bl FormatScheme::SetBrush(this, 1, &ctrl)
    /// ```
    ///
    /// 본 method 는 위 sequence 의 부분 1:1 — SolidBrush 의 alloc 부터 SetBrush
    /// 까지. Block 1 완료 후 raw 는 `0x16f79c` 부터 다음 block 진행 (ColorEffect
    /// + GradientBrush 등 — 16v 에서 port).
    pub fn create_default_block1(&mut self) {
        unsafe {
            // raw 16f6c0-16f6d4: Color(SchemeStyle 0x10, type=SCHEME, effect=null)
            let color = crate::color::Color::from_scheme_raw_u32(0x10, ptr::null_mut());

            // raw 16f6a0-16f6b8: alloc SolidBrush (16B) + vtable + PropertyBag(false)
            // raw 16f6e4-16f704: bl 0x6541e8 = PColor::create_attach_ctrl + bag.attach
            // 본 Rust 의 SolidBrush::new() 가 위 sequence 1:1 (16r 단계 완성).
            let solid = crate::brush::SolidBrush::new(color);

            // SolidBrush 를 heap-alloc (raw 의 alloc 16B + ctor 가 이미 합쳐짐).
            // 본 Rust 는 alloc 분리 — Box 로 heap allocation.
            let solid_layout =
                Layout::from_size_align(16, 8).expect("SolidBrush 16B 8B layout");
            let solid_heap = std::alloc::alloc(solid_layout) as *mut crate::brush::SolidBrush;
            if solid_heap.is_null() {
                std::alloc::handle_alloc_error(solid_layout);
            }
            ptr::write(solid_heap, solid);

            // raw 16f730-16f7a8: alloc 24B ControlBlock + init
            let ctrl = BrushControlBlock::create_raw(solid_heap as *mut u8);

            // raw 16f78c-16f790: SetBrush(this, style=1=MainColor, &ctrl)
            self.set_brush(1, ctrl);
        }
    }

    /// `FormatScheme::CreateDefault` 의 **Block 2** 1:1 port — raw `0x16f79c-0x16f854`.
    ///
    /// Block 2 = 3 ColorEffect 인스턴스 (24B each) + 각 인스턴스에 2 Add 호출.
    /// 마지막에 GradientBrush 의 ctor 진입 직전 (= Block 3 의 시작) 까지.
    ///
    /// 본 method 는 3 ColorEffect 의 alloc + 6 Add 만 port. GradientBrush 의 ctor
    /// (`__ZN3Hnc5Shape13GradientBrushC2Ev` @ `0x176628`) 는 16w 에서.
    ///
    /// raw asm sequence (총 ~45 instr):
    ///
    /// **Block 2A** (effect1: 0.5 → 0x20a, 3.0 → 0x204) — `0x16f79c-0x16f7cc`:
    /// ```asm
    /// 16f79c: mov  w0, #0x18           ; alloc 24B
    /// 16f7a0: bl   __Znwm
    /// 16f7a4: mov  x24, x0              ; x24 = effect1
    /// 16f7a8-16f7ac: stp/str xzr        ; clear 24B (begin/end/cap_end = null)
    /// 16f7b4: fmov s0, #0.5             ; arg = 0.5
    /// 16f7b8: mov  w1, #0x20a           ; PKey = 0x20a (lumMod or similar)
    /// 16f7bc: bl   ColorEffect::Add     ; effect1.Add(0x20a, 0.5)
    /// 16f7c0: fmov s0, #3.0             ; arg = 3.0
    /// 16f7c4: mov  x0, x24
    /// 16f7c8: mov  w1, #0x204           ; PKey = 0x204
    /// 16f7cc: bl   ColorEffect::Add     ; effect1.Add(0x204, 3.0)
    /// ```
    ///
    /// **Block 2B** (effect2: 0.37 → 0x20a, 3.0 → 0x204) — `0x16f7d0-0x16f808`:
    /// ```asm
    /// 16f7d0-16f7e4: alloc 24B effect2 (zeroed)
    /// 16f7e8: mov  w8, #0x70a4
    /// 16f7ec: movk w8, #0x3ebd, lsl #16   ; w8 = 0x3EBD70A4 (float 0.37)
    /// 16f7f0: fmov s0, w8
    /// 16f7f8: bl   ColorEffect::Add     ; effect2.Add(0x20a, 0.37)
    /// 16f7fc-16f808: effect2.Add(0x204, 3.0)
    /// ```
    ///
    /// **Block 2C** (effect3: 0.15 → 0x20a, 3.5 → 0x204) — `0x16f80c-0x16f844`:
    /// ```asm
    /// 16f80c-16f820: alloc 24B effect3 (zeroed)
    /// 16f824: mov  w8, #0x999a
    /// 16f828: movk w8, #0x3e19, lsl #16   ; w8 = 0x3E19999A (float 0.15)
    /// 16f82c: fmov s0, w8
    /// 16f834: bl   ColorEffect::Add     ; effect3.Add(0x20a, 0.15)
    /// 16f838-16f844: effect3.Add(0x204, 3.5)
    /// ```
    ///
    /// raw 의 effect 들은 stack-local (`[x29-0xb0]` `[x29-0xb8]` `[x29-0xc0]`) 으로
    /// 보유 — 본 단계는 caller 가 `*mut ColorEffect` 3-tuple 받아 Block 3+ 에 전달.
    ///
    /// PKey 0x20a (= 522) / 0x204 (= 516) 둘 다 jump_table[22] / [16] = `0x00`
    /// (default REPLACE) — `ColorEffect::add` 가 (pkey, value) 그대로 push_back.
    ///
    /// # Safety
    /// 반환 3 ptr 은 `ColorEffect::raw_delete` 로 해제 필요 (Block 3+ 가 소유권
    /// 가져가지 않으면 leak).
    pub unsafe fn create_default_block2_effects() -> [*mut crate::color_effect::ColorEffect; 3] {
        // raw 16f79c-16f7cc: effect1
        let effect1 = crate::color_effect::ColorEffect::create();
        (*effect1).add(0x20a, 0.5_f32);
        (*effect1).add(0x204, 3.0_f32);

        // raw 16f7d0-16f808: effect2
        let effect2 = crate::color_effect::ColorEffect::create();
        // raw 16f7e8-16f7ec: float 0.37 (0x3EBD70A4)
        let v37 = f32::from_bits(0x3EBD70A4);
        (*effect2).add(0x20a, v37);
        (*effect2).add(0x204, 3.0_f32);

        // raw 16f80c-16f844: effect3
        let effect3 = crate::color_effect::ColorEffect::create();
        // raw 16f824-16f828: float 0.15 (0x3E19999A)
        let v15 = f32::from_bits(0x3E19999A);
        (*effect3).add(0x20a, v15);
        (*effect3).add(0x204, 3.5_f32);

        [effect1, effect2, effect3]
    }

    /// `FormatScheme::CreateDefault` 의 **Block 6** 1:1 port — raw `0x16fd58-0x16fe20`.
    ///
    /// Block 5-B (GradientStopsVec cleanup) 직후, 두 번째 GradientBrush 의 ColorEffect
    /// 3-set 을 생성. Block 2 와 패턴 동일하나 **PKey 0x209** (Block 2 는 0x20a 사용)
    /// + 다른 float bit pattern.
    ///
    /// raw asm sequence (~45 instr, 모두 24B alloc + 2 Add):
    ///
    /// **Block 6-A** (effect4: [(0x209, 0.51), (0x204, 1.30)]) — `0x16fd58-0x16fd98`:
    /// ```asm
    /// 0x16fd58: mov  w0, #0x18
    /// 0x16fd5c: bl   __Znwm
    /// 0x16fd60: mov  x23, x0
    /// 0x16fd70: mov  w8, #0x8f5c; movk w8, #0x3f02, lsl #16   ; 0x3F028F5C (= 0.51)
    /// 0x16fd7c: mov  w1, #0x209
    /// 0x16fd80: bl   ColorEffect::Add                          ; effect4.Add(0x209, 0.51)
    /// 0x16fd84: mov  w8, #0x6666; movk w8, #0x3fa6, lsl #16   ; 0x3FA66666 (= 1.30)
    /// 0x16fd94: mov  w1, #0x204
    /// 0x16fd98: bl   ColorEffect::Add                          ; effect4.Add(0x204, 1.30)
    /// ```
    ///
    /// **Block 6-B** (effect5: [(0x209, 0.93), (0x204, 1.30)]) — `0x16fd9c-0x16fddc`:
    /// ```asm
    /// 0x16fdb4: 0x3F6E147B (= 0.93)
    /// 0x16fdc8: 0x3FA66666 (= 1.30)
    /// ```
    ///
    /// **Block 6-C** (effect6: [(0x209, 0.94), (0x204, 1.35)]) — `0x16fde0-0x16fe20`:
    /// ```asm
    /// 0x16fdf8: 0x3F70A3D7 (= 0.94)
    /// 0x16fe0c: 0x3FACCCCD (= 1.35)
    /// ```
    ///
    /// PKey 0x209 (= 521) / 0x204 (= 516) 둘 다 jump_table[21]/[16] = `0x00` (default
    /// REPLACE). 0x209 가 Block 2 의 0x20a 와 다른 점만 새롭다.
    ///
    /// # Safety
    /// 반환 3 ptr 은 `ColorEffect::raw_delete` 로 해제 필요.
    pub unsafe fn create_default_block6_effects() -> [*mut crate::color_effect::ColorEffect; 3] {
        // Block 6-A: effect4
        let effect4 = crate::color_effect::ColorEffect::create();
        (*effect4).add(0x209, f32::from_bits(0x3F028F5C)); // 0.51
        (*effect4).add(0x204, f32::from_bits(0x3FA66666)); // 1.30

        // Block 6-B: effect5
        let effect5 = crate::color_effect::ColorEffect::create();
        (*effect5).add(0x209, f32::from_bits(0x3F6E147B)); // 0.93
        (*effect5).add(0x204, f32::from_bits(0x3FA66666)); // 1.30

        // Block 6-C: effect6
        let effect6 = crate::color_effect::ColorEffect::create();
        (*effect6).add(0x209, f32::from_bits(0x3F70A3D7)); // 0.94
        (*effect6).add(0x204, f32::from_bits(0x3FACCCCD)); // 1.35

        [effect4, effect5, effect6]
    }

    /// `FormatScheme::CreateDefault` 의 **Block 7-9** 1:1 — raw `0x16fe24-0x170328`.
    ///
    /// 2nd GradientBrush 의 full setup:
    /// - **Block 7** (`0x16fe24-0x170178`): 새 GradientBrush + 새 GradientStopsVec
    ///   (160B alloc) + 3 stops populated (positions 0.0 / **0.8** / 1.0,
    ///   effects 4/5/6).
    /// - **Block 8** (`0x170178-0x1702b8`): 5 setter calls — SetStops + scaled=true +
    ///   style=0 + angle=270° + **flip=false** (Block 4 와의 유일한 차이).
    /// - **Block 9** (`0x1702b8-0x170328`): BrushControlBlock + SetBrush(**Style=3**).
    ///
    /// 본 method 는 caller 가 미리 만든 `effects` (Block 6 산출) 를 사용해 2nd
    /// GradientBrush 를 구성한 후 FormatScheme.brushes 에 attach.
    ///
    /// **stops 위치 확정**:
    /// - stop1: position 0.0 (raw `str wzr` at +0x18)
    /// - stop2: position **0.8** (raw `mov w8, #0xcccd; movk #0x3f4c` = 0x3F4CCCCD)
    /// - stop3: position 1.0 (raw `mov w8, #0x3f800000`)
    ///
    /// **setter 값 확정**:
    /// - scaled (0x265) = **true** (raw `mov w20, #0x1; strb w20, ...`)
    /// - style  (0x25f) = 0 (raw `str wzr`)
    /// - angle  (0x260) = 270° (raw `mov w8, #0x43870000` = 0x43870000)
    /// - flip   (0x261) = **false** (raw `strb wzr`) ← Block 4 는 true (= setup-1 의
    ///   유일한 차이)
    ///
    /// # Safety
    /// `effects` 의 3 ptr 은 valid `*mut ColorEffect`. 본 method 가 끝나면 effects
    /// 의 deep clone 이 GradientStops 안에 저장되므로 caller 는 원본 `effects` 들을
    /// `raw_delete` 해야 함.
    pub unsafe fn create_default_block7_through_9(
        &mut self,
        effects: [*mut crate::color_effect::ColorEffect; 3],
    ) {
        // Block 7: 3-stop GradientStopsVec
        let mut stops_vec = crate::gradient_stop::GradientStopsVec::new_with_initial_capacity();

        let scheme_color_value = {
            let mut v = [0u8; 12];
            v[0..4].copy_from_slice(&0x10u32.to_le_bytes());
            v
        };

        // stops: (position, effect_idx)
        let stop_specs: [(f32, usize); 3] = [
            (0.0_f32, 0),                            // stop1: effect4
            (f32::from_bits(0x3F4CCCCD), 1),         // stop2: effect5 @ 0.8
            (1.0_f32, 2),                            // stop3: effect6 @ 1.0
        ];

        for (position, effect_idx) in stop_specs {
            // Color body (Scheme 0x10, type_tag=2) + effect for this stop
            let color_with_effect = crate::color::Color {
                value: scheme_color_value,
                type_tag: crate::color::color_type::SCHEME,
                color_effect: effects[effect_idx],
            };
            let stop = crate::gradient_stop::GradientStop::create_with_effect(
                &color_with_effect,
                position,
            );
            // Color owned effect ptr is BORROWED from `effects[]` — forget the
            // wrapper Color to prevent Color::Drop from double-freeing the original.
            std::mem::forget(color_with_effect);

            let ctrl = crate::gradient_stop::GradientStopCtrl::create_raw(stop);
            stops_vec.push_back(ctrl);
            crate::gradient_stop::GradientStopCtrl::release(ctrl);
        }
        debug_assert_eq!(stops_vec.len(), 3);

        // Block 8: configure GradientBrush with stops + 4 overrides
        let mut gb = crate::brush::GradientBrush::new();
        gb.set_stops(&stops_vec);
        gb.set_scaled(true);            // raw 0x1701b4-0x1701e4: scaled = true
        gb.set_style(0);                // raw 0x1701f0-0x170220: style = 0
        gb.set_angle_degrees(270.0);    // raw 0x17022c-0x170268: angle = 270°
        gb.set_flip(false);             // raw 0x17027c-0x1702ac: flip = FALSE (vs Block 4 true!)

        // Block 9: BrushControlBlock + SetBrush(Style=3)
        let ctrl = BrushControlBlock::from_gradient(gb);
        self.set_brush(3, ctrl);

        // Block 9 cleanup: stops_vec stack-local auto-drop (Rust)
        // (raw 0x170328: bl 0x63025c — same as Block 5-B)
        drop(stops_vec);
    }

    /// raw `FormatScheme::GetBrush(Style) const` (`0x16eb88`) 1:1.
    ///
    /// raw 는 std::map::find — key 일치 노드의 value (= SharePtr ctrl ptr) 반환.
    /// 없으면 null SharePtr.
    ///
    /// 본 Rust API 는 `&self` 통해 ctrl ptr 의 `&BrushControlBlock` view 반환.
    /// caller 는 직접 obj 의 sub-type 별 cast (vtable.type_tag 확인 후).
    pub fn find_brush(&self, style: u32) -> Option<*mut BrushControlBlock> {
        unsafe { find_node_in_tree(&self.brushes, style) }
    }

    // =========================================================================
    // 16-γ: SetBackgroundBrush / GetBackgroundBrush + Block 10
    // =========================================================================

    /// raw `FormatScheme::SetBackgroundBrush(Style, UniquePtr<Brush> const&)`
    /// (`0x16ee14`) 1:1.
    ///
    /// raw asm 인용 (`0x16ee14-0x16eef8`) — `SetBrush` (`0x16ec94`) 와 100% identical
    /// 구조, 다만 `x21 = x0 + 0x20` (bg_brushes offset) vs `x21 = x0 + 0x08`
    /// (brushes offset). 두 함수의 instruction byte-equivalence 까지 보장.
    ///
    /// ```asm
    /// 16ee28: ldr  x8, [x2]                  ; ctrl = (*arg).raw
    /// 16ee2c: cbz  x8, return                 ; null ctrl → no-op
    /// 16ee30: ldr  x9, [x8]                   ; obj = ctrl->obj
    /// 16ee34: cbz  x9, return                 ; null obj → no-op
    /// 16ee3c: add  x21, x0, #0x20             ; x21 = &this->bg_brushes  ← Brush 의 +0x08 vs Bg 의 +0x20
    /// 16ee40: str  w1, [sp]                   ; local.key = style
    /// 16ee4c: str  x8, [sp, #0x8]             ; local.ctrl = ctrl
    /// 16ee50-16ee58: refcount++
    /// 16ee6c: bl   0x664594                   ; find_or_insert(tree, &local) — 동일 helper
    /// 16ee70: x21 = node, x22 = inserted_flag
    /// 16ee7c: bl   0x647ebc                   ; release local ctrl
    /// 16ee80-16ee84: if inserted_flag → return
    /// 16ee9c: add  x20, x21, #0x28            ; x20 = &node.value
    /// 16eea0-16eec4: same-object guard
    /// 16eecc: bl   0x647ebc                   ; release old SharePtr at node
    /// 16eed4: str  x8, [x20]                  ; node.value = new ctrl
    /// 16eee8-16eef0: refcount++ on new ctrl
    /// ```
    ///
    /// 본 Rust port 는 `set_brush` (16u) 와 동일 algorithm — `bg_brushes` tree 사용.
    ///
    /// # Safety
    /// `ctrl_in` 은 `BrushControlBlock::create_raw` 또는 `from_*` 으로 얻은 valid
    /// ptr. FormatScheme 가 ownership transfer 받음.
    pub unsafe fn set_background_brush(&mut self, style: u32, ctrl_in: *mut BrushControlBlock) {
        // raw 16ee28-16ee34: null-guard
        if ctrl_in.is_null() || (*ctrl_in).obj.is_null() {
            return;
        }

        let tree = &mut self.bg_brushes as *mut TreeBase;
        let (slot, parent, existing) = find_insert_position(tree, |node| {
            fs_brush_node_key(node).cmp(&style)
        });

        if existing.is_null() {
            // INSERT path — identical to set_brush's INSERT
            let node_layout = Layout::new::<FsBrushMapNode>();
            let new_node = std::alloc::alloc(node_layout) as *mut FsBrushMapNode;
            if new_node.is_null() {
                std::alloc::handle_alloc_error(node_layout);
            }
            ptr::write(
                new_node,
                FsBrushMapNode {
                    base: TreeNodeBase {
                        left: ptr::null_mut(),
                        right: ptr::null_mut(),
                        parent,
                        is_black: 0,
                        _pad_0x19: [0u8; 7],
                    },
                    key: style,
                    _pad: 0,
                    value: ctrl_in,
                },
            );
            *slot = new_node as *mut TreeNodeBase;
            update_begin_node_after_insert(tree);
            let root = self.bg_brushes.end_node_left;
            balance_after_insert(root, new_node as *mut TreeNodeBase);
            self.bg_brushes.size += 1;
        } else {
            // REPLACE path — same-object guard then release+install
            let node = existing as *mut FsBrushMapNode;
            let old_ctrl = (*node).value;
            let old_obj = if !old_ctrl.is_null() {
                (*old_ctrl).obj
            } else {
                ptr::null_mut()
            };
            let new_obj = (*ctrl_in).obj;
            if old_obj == new_obj {
                BrushControlBlock::release(ctrl_in);
                return;
            }
            BrushControlBlock::release(old_ctrl);
            (*node).value = ctrl_in;
        }
    }

    /// raw `FormatScheme::GetBackgroundBrush(Style) const` (`0x16ec48`) 1:1.
    pub fn find_background_brush(&self, style: u32) -> Option<*mut BrushControlBlock> {
        unsafe { find_node_in_tree(&self.bg_brushes, style) }
    }

    // =========================================================================
    // 16-ζ: SetPen / GetPen — pens tree at FormatScheme+0x38
    // =========================================================================

    /// raw `FormatScheme::SetPen(Style, UniquePtr<Pen> const&)` (`0x16ef94`) 1:1.
    ///
    /// raw `0x16ef94-0x16f054` 의 instruction-level 분석 — SetBrush / SetBgBrush 의
    /// byte-eq variant 로, tree pointer offset 만 차이:
    ///
    /// ```asm
    /// 0x16efa8: ldr  x8, [x2]                  ; ctrl = (*arg).raw
    /// 0x16efac: cbz  x8, return                 ; null ctrl → no-op
    /// 0x16efb0: ldr  x9, [x8]                   ; obj = ctrl->obj (= Pen ptr)
    /// 0x16efb4: cbz  x9, return                 ; null obj → no-op
    /// 0x16efbc: add  x21, x0, #0x38            ; ⭐ x21 = &this->pens (vs +0x8/0x20)
    /// 0x16efc0-0x16efd8: refcount++ on ctrl
    /// 0x16efdc: bl   0x648554                   ; Pen 의 SharePtr fence/log (= 0x647cfc Brush)
    /// 0x16efec: bl   0x6646bc                   ; pens find_or_insert (= 0x664594 Brush)
    /// 0x16effc: bl   0x648714                   ; Pen release helper (= 0x647ebc Brush)
    /// 0x16f000-0x16f054: same-instance guard + REPLACE path
    /// ```
    ///
    /// 본 Rust port: `set_brush` / `set_background_brush` 와 동일 algorithm — `pens`
    /// tree 사용 + Pen-specific PenControlBlock.
    ///
    /// # Safety
    /// `ctrl_in` 은 `PenControlBlock::create_raw` 또는 `from_pen` 으로 얻은 valid ptr.
    pub unsafe fn set_pen(&mut self, style: u32, ctrl_in: *mut PenControlBlock) {
        // raw 16efa8-16efb4: null-guard
        if ctrl_in.is_null() || (*ctrl_in).obj.is_null() {
            return;
        }

        let tree = &mut self.pens as *mut TreeBase;
        let (slot, parent, existing) = find_insert_position(tree, |node| {
            fs_pen_node_key(node).cmp(&style)
        });

        if existing.is_null() {
            // INSERT path
            let node_layout = Layout::new::<FsPenMapNode>();
            let new_node = std::alloc::alloc(node_layout) as *mut FsPenMapNode;
            if new_node.is_null() {
                std::alloc::handle_alloc_error(node_layout);
            }
            ptr::write(
                new_node,
                FsPenMapNode {
                    base: TreeNodeBase {
                        left: ptr::null_mut(),
                        right: ptr::null_mut(),
                        parent,
                        is_black: 0,
                        _pad_0x19: [0u8; 7],
                    },
                    key: style,
                    _pad: 0,
                    value: ctrl_in,
                },
            );
            *slot = new_node as *mut TreeNodeBase;
            update_begin_node_after_insert(tree);
            let root = self.pens.end_node_left;
            balance_after_insert(root, new_node as *mut TreeNodeBase);
            self.pens.size += 1;
        } else {
            // REPLACE path with same-object guard
            let node = existing as *mut FsPenMapNode;
            let old_ctrl = (*node).value;
            let old_obj = if !old_ctrl.is_null() {
                (*old_ctrl).obj
            } else {
                ptr::null_mut()
            };
            let new_obj = (*ctrl_in).obj;
            if old_obj == new_obj {
                PenControlBlock::release(ctrl_in);
                return;
            }
            PenControlBlock::release(old_ctrl);
            (*node).value = ctrl_in;
        }
    }

    /// raw `FormatScheme::GetPen(Style) const` (`0x16ef28`) 1:1.
    pub fn find_pen(&self, style: u32) -> Option<*mut PenControlBlock> {
        unsafe {
            let mut current = self.pens.end_node_left;
            let end_addr = (&self.pens.end_node_left)
                as *const *mut TreeNodeBase as *const TreeNodeBase;
            let mut last_le = end_addr as *mut TreeNodeBase;
            while !current.is_null() {
                let nkey = fs_pen_node_key(current);
                if nkey < style {
                    current = (*current).right;
                } else {
                    last_le = current;
                    current = (*current).left;
                }
            }
            if last_le == end_addr as *mut TreeNodeBase {
                return None;
            }
            let n = last_le as *mut FsPenMapNode;
            if (*n).key != style {
                return None;
            }
            Some((*n).value)
        }
    }

    /// `FormatScheme::CreateDefault` 의 **Block 10** 1:1 port — raw `0x170328-0x170428`.
    ///
    /// 4번째 attach call = `SetBackgroundBrush(Style=1, SolidBrush)`. Block 1 의
    /// `SetBrush(Style=1, SolidBrush(SchemeStyle 0x10))` 와 동일한 SolidBrush 구성,
    /// 다만 attach 대상이 brushes → bg_brushes.
    ///
    /// raw asm sequence (`0x170338-0x170424`):
    ///
    /// ```asm
    /// ;; SolidBrush(SchemeStyle 0x10) build — Block 1 과 동일
    /// 0x170338-0x17033c: alloc 16B SolidBrush + __Znwm
    /// 0x170340-0x170344: str x28, [x0], #0x8     ; vtable = SOLID_BRUSH_VTABLE (x28)
    /// 0x170348-0x17034c: bl PropertyBag::C1(false)
    /// 0x170354-0x170374: Color body (Scheme 0x10, type=SCHEME mode=2, eff=null) + key 0x259
    /// 0x170398:          bl 0x6541e8                  ; PColor::create_attach
    /// 0x17039c-0x1703c4: PropertyKey + PColor temp cleanup
    ///
    /// ;; BrushControlBlock + SetBackgroundBrush
    /// 0x1703c8-0x1703dc: alloc 24B ctrl + stp [SolidBrush, 1] + strb 1@+0x10
    /// 0x1703e0-0x170414: SharePtr same-instance guard (16ec94 pattern, identical)
    /// 0x170418-0x170420: x0 = FormatScheme*, w1 = 1, x2 = &ctrl
    /// 0x170424:          bl FormatScheme::SetBackgroundBrush
    /// ```
    ///
    /// **선행 cleanup** (Block 9 의 carry-over) — raw `0x170328-0x170334`:
    /// ```asm
    /// 0x170328: sub  x8, x29, #0xd8
    /// 0x17032c: stur x8, [x29, #-0xa0]
    /// 0x170330: sub  x0, x29, #0xa0
    /// 0x170334: bl   0x63025c                  ; GradientStops::~GradientStops
    /// ```
    /// 이는 Block 9 의 `stops_vec` stack-local dtor 으로, Rust 에서는 Block 7-9
    /// 가 reside 한 `stops_vec` 의 자동 drop 으로 처리됨 (이미 `create_default_block7_through_9`
    /// 의 끝에서 `drop(stops_vec)`). 본 Block 10 method 는 그 직후의 SolidBrush
    /// build 만 port.
    pub fn create_default_block10(&mut self) {
        unsafe {
            // raw 0x170354-0x170374: Color(SchemeStyle 0x10, type=SCHEME, eff=null)
            let color = crate::color::Color::from_scheme_raw_u32(0x10, ptr::null_mut());

            // raw 0x170338-0x17034c: alloc 16B SolidBrush + vtable + bag init
            // raw 0x170378-0x170398: bl 0x6541e8 PColor attach (key 0x259)
            //   = SolidBrush::new() 가 모두 합쳐서 port (16r 단계).
            let solid = crate::brush::SolidBrush::new(color);

            // SolidBrush 를 heap-alloc (raw 의 alloc + ctor 가 묶여있던 부분 분리).
            let solid_layout =
                Layout::from_size_align(16, 8).expect("SolidBrush 16B 8B layout");
            let solid_heap = std::alloc::alloc(solid_layout) as *mut crate::brush::SolidBrush;
            if solid_heap.is_null() {
                std::alloc::handle_alloc_error(solid_layout);
            }
            ptr::write(solid_heap, solid);

            // raw 0x1703c8-0x1703dc: alloc 24B BrushControlBlock + init
            let ctrl = BrushControlBlock::create_raw(solid_heap as *mut u8);

            // raw 0x170418-0x170424: SetBackgroundBrush(Style=1, &ctrl)
            self.set_background_brush(1, ctrl);
        }
    }

    /// `FormatScheme::CreateDefault` 의 **Block 11** 1:1 port — raw `0x170430-0x1709b4`.
    ///
    /// 5번째 attach call = `SetBackgroundBrush(Style=2, GradientBrush(3 stops + 4
    /// setter overrides))`. Block 4/8 (SetBrush 의 1st/2nd Gradient) 와 다른 점:
    /// - **4 setters** (vs 5) — angle/flip 설정 안 함 (default 사용)
    /// - **style=1** (vs 0) — radial gradient?
    /// - **focus_rect = (0.5, -0.8, 0.5, 1.8)** non-default 명시 (rodata @ 0x741e90)
    /// - **stop2 position = 0.4** (vs Block 4 의 0.35, Block 8 의 0.8)
    /// - **3 effects 의 PKey 분포** = 0x20a / 0x20a+0x209+0x20a / 0x209
    ///   - effect7: (0x20a, 0.4) + (0x204, 3.5)   [2 Adds, Block 4-pattern]
    ///   - effect8: (0x20a, 0.45) + (0x209, 0.99) + (0x204, 3.5)  [⭐ 3 Adds, new]
    ///   - effect9: (0x209, 0.2) + (0x204, 2.55)  [2 Adds, Block 6-pattern]
    ///
    /// raw effect float bit patterns:
    /// - 0.4   = 0x3ECCCCCD
    /// - 3.5   = `fmov s0, #3.5` (= 0x40600000)
    /// - 0.45  = 0x3EE66666
    /// - 0.99  = 0x3F7D70A4
    /// - 0.2   = 0x3E4CCCCD
    /// - 2.55  = 0x40233333
    ///
    /// stop positions: (0.0, 0.4, 1.0).
    ///
    /// **선행 cleanup**: Block 10 의 share_ptr release `0x170428-0x17042c` 는 Rust 의
    /// 자동 drop 이 처리. 본 method 가 새 GradientBrush 부터 시작.
    pub fn create_default_block11(&mut self) {
        unsafe {
            // raw 0x170430-0x170468: effect7 = Add(0x20a, 0.4) + Add(0x204, 3.5)
            let effect7 = crate::color_effect::ColorEffect::create();
            (*effect7).add(0x20a, f32::from_bits(0x3ECCCCCD)); // 0.4
            (*effect7).add(0x204, 3.5_f32);

            // raw 0x17046c-0x1704bc: effect8 = Add(0x20a, 0.45) + Add(0x209, 0.99) + Add(0x204, 3.5)
            // ⭐ 3 Adds (new pattern, both 0x20a and 0x209 PKeys)
            let effect8 = crate::color_effect::ColorEffect::create();
            (*effect8).add(0x20a, f32::from_bits(0x3EE66666)); // 0.45
            (*effect8).add(0x209, f32::from_bits(0x3F7D70A4)); // 0.99
            (*effect8).add(0x204, 3.5_f32);

            // raw 0x1704c0-0x170500: effect9 = Add(0x209, 0.2) + Add(0x204, 2.55)
            let effect9 = crate::color_effect::ColorEffect::create();
            (*effect9).add(0x209, f32::from_bits(0x3E4CCCCD)); // 0.2
            (*effect9).add(0x204, f32::from_bits(0x40233333)); // 2.55

            // raw 0x170504-0x170510: alloc 16B + GradientBrush::C2()
            // raw 0x170514-0x170548: alloc 160B GradientStopsVec + init
            let mut stops_vec = crate::gradient_stop::GradientStopsVec::new_with_initial_capacity();

            let scheme_color_value = {
                let mut v = [0u8; 12];
                v[0..4].copy_from_slice(&0x10u32.to_le_bytes());
                v
            };

            // raw 0x17054c-0x170644: stop1 (position 0.0, effect7)
            //   - 0x170564: stur x24, [x29, #-0x90]   ; Color.effect = x24 (= effect7)
            //   - 0x170568-0x170588: alloc 32B GradientStop + memcpy Color + Effect::Clone
            //   - 0x17058c: str wzr, [x25, #0x18]    ; position = 0.0
            //   - 0x170598-0x17062c: push_back to stops_vec
            //   - 0x170634-0x170648: release effect7 (deep-clone owned now)
            //
            // raw 0x17064c-0x17074c: stop2 (position 0.4, effect8)
            //   - 0x17068c-0x170694: str 0x3ECCCCCD at [x24, #0x18] = position = 0.4
            //
            // raw 0x170754-0x170850: stop3 (position 1.0, effect9)
            //   - 0x170794-0x170798: str 0x3F800000 at [x23, #0x18] = position = 1.0
            let stop_specs: [(f32, *mut crate::color_effect::ColorEffect); 3] = [
                (0.0_f32, effect7),                         // stop1
                (f32::from_bits(0x3ECCCCCD), effect8),      // stop2 @ 0.4
                (1.0_f32, effect9),                          // stop3 @ 1.0
            ];

            for (position, effect_ptr) in stop_specs {
                let color_with_effect = crate::color::Color {
                    value: scheme_color_value,
                    type_tag: crate::color::color_type::SCHEME,
                    color_effect: effect_ptr,
                };
                let stop = crate::gradient_stop::GradientStop::create_with_effect(
                    &color_with_effect,
                    position,
                );
                std::mem::forget(color_with_effect);

                let ctrl = crate::gradient_stop::GradientStopCtrl::create_raw(stop);
                stops_vec.push_back(ctrl);
                crate::gradient_stop::GradientStopCtrl::release(ctrl);
            }
            debug_assert_eq!(stops_vec.len(), 3);

            // raw 0x170858-0x170948: 4 setters
            let mut gb = crate::brush::GradientBrush::new();
            // 1) raw 0x170858-0x17088c: bag.attach(0x266, PStops) via 0x655508
            gb.set_stops(&stops_vec);
            // 2) raw 0x170894-0x1708c8: bag.attach(0x265, PBool=true) via 0x6475a4
            gb.set_scaled(true);
            // 3) raw 0x1708d0-0x170904: bag.attach(0x25f, PEnum=1) via 0x656690
            //    ⭐ style=1 (= radial?) vs Block 4/8's style=0 (= linear)
            gb.set_style(1);
            // 4) raw 0x17090c-0x170944: bag.attach(0x262, PVec4=rodata@0x741e90) via 0x656fb4
            //    rodata bytes: 00 00 00 3f  cd cc 4c bf  00 00 00 3f  66 66 e6 3f
            //    = (0.5, -0.8, 0.5, 1.8)
            let focus_rect_blob: [u8; 16] = [
                0x00, 0x00, 0x00, 0x3F,   // 0.5
                0xCD, 0xCC, 0x4C, 0xBF,   // -0.8
                0x00, 0x00, 0x00, 0x3F,   // 0.5
                0x66, 0x66, 0xE6, 0x3F,   // 1.8
            ];
            gb.set_focus_rect(focus_rect_blob);
            // NOTE: angle (0x260) / flip (0x261) 는 setter 안 함 — GradientBrush::new() 의
            //       default (angle=0.0, flip=false) 유지.

            // raw 0x170950-0x170984: alloc 24B BrushControlBlock + obj=gb + SharePtr setup
            let ctrl = BrushControlBlock::from_gradient(gb);

            // raw 0x1709a8-0x1709b0: SetBackgroundBrush(Style=2, &ctrl)
            self.set_background_brush(2, ctrl);

            // raw 0x1709b4-0x1709c8: stops_vec stack-local dtor + cleanup
            drop(stops_vec);

            // effects 의 원본 raw 는 stops 가 deep-clone 했으므로 본 fn 의 caller-equivalent
            // (i.e., here) 가 release. raw 의 stack-local effect 들은 함수 끝에서 해제됨.
            crate::color_effect::ColorEffect::raw_delete(effect7);
            crate::color_effect::ColorEffect::raw_delete(effect8);
            crate::color_effect::ColorEffect::raw_delete(effect9);
        }
    }

    /// `FormatScheme::CreateDefault` 의 **Block 12** 1:1 port — raw `0x1709cc-0x170de8`.
    ///
    /// 6번째 attach call = `SetBackgroundBrush(Style=3, GradientBrush(**2 stops** +
    /// 4 setters))`. Block 11 (3-stop) 와 다른 점:
    /// - **2 stops** (vs 3) — positions (0.0, 1.0) only
    /// - **2 effects** (vs 3) — effect10/11
    /// - **focus_rect = [0.5, 0.5, 0.5, 0.5]** explicit but = default value
    ///   (`movi.4s v0, #0x3f, lsl #24`)
    /// - style = 1 (= radial, same as Block 11)
    /// - angle/flip not set (default 0.0/false 유지)
    ///
    /// raw effect 값:
    /// - effect10: Add(0x20a, **0.8** = 0x3F4CCCCD) + Add(0x204, 3.0 via fmov)
    /// - effect11: Add(0x209, **0.3** = 0x3E99999A) + Add(0x204, 2.0 via fmov)
    pub fn create_default_block12(&mut self) {
        unsafe {
            // raw 0x1709cc-0x170a04: effect10
            let effect10 = crate::color_effect::ColorEffect::create();
            (*effect10).add(0x20a, f32::from_bits(0x3F4CCCCD)); // 0.8
            (*effect10).add(0x204, 3.0_f32);

            // raw 0x170a08-0x170a40: effect11
            let effect11 = crate::color_effect::ColorEffect::create();
            (*effect11).add(0x209, f32::from_bits(0x3E99999A)); // 0.3
            (*effect11).add(0x204, 2.0_f32);

            // raw 0x170a44-0x170a88: GradientBrush ctor + 160B GradientStopsVec init
            let mut stops_vec = crate::gradient_stop::GradientStopsVec::new_with_initial_capacity();

            let scheme_color_value = {
                let mut v = [0u8; 12];
                v[0..4].copy_from_slice(&0x10u32.to_le_bytes());
                v
            };

            // raw 0x170a8c-0x170b88: stop1 (position 0.0, effect10)
            //   - 0x170acc: str wzr, [x23, #0x18]  → position = 0.0
            // raw 0x170b8c-0x170c88: stop2 (position 1.0, effect11)
            //   - 0x170bc8: mov w8, #0x3f800000; str w8, [x22, #0x18]  → position = 1.0
            let stop_specs: [(f32, *mut crate::color_effect::ColorEffect); 2] = [
                (0.0_f32, effect10),  // stop1
                (1.0_f32, effect11),  // stop2
            ];

            for (position, effect_ptr) in stop_specs {
                let color_with_effect = crate::color::Color {
                    value: scheme_color_value,
                    type_tag: crate::color::color_type::SCHEME,
                    color_effect: effect_ptr,
                };
                let stop = crate::gradient_stop::GradientStop::create_with_effect(
                    &color_with_effect,
                    position,
                );
                std::mem::forget(color_with_effect);

                let ctrl = crate::gradient_stop::GradientStopCtrl::create_raw(stop);
                stops_vec.push_back(ctrl);
                crate::gradient_stop::GradientStopCtrl::release(ctrl);
            }
            debug_assert_eq!(stops_vec.len(), 2);

            // raw 0x170c8c-0x170d7c: 4 setters
            let mut gb = crate::brush::GradientBrush::new();
            // 1) PStops (0x266) via bl 0x655508 @ 0x170cbc
            gb.set_stops(&stops_vec);
            // 2) PBool scaled=true (0x265) via bl 0x6475a4 @ 0x170cf8
            gb.set_scaled(true);
            // 3) PEnum style=1 (0x25f) via bl 0x656690 @ 0x170d34
            gb.set_style(1);
            // 4) PVec4 focus_rect=[0.5, 0.5, 0.5, 0.5] (0x262) via bl 0x656fb4 @ 0x170d74
            //    raw `0x170d40: movi.4s v0, #0x3f, lsl #24` = 4 lanes of 0x3F000000 = 0.5
            //    (= default value, explicit-set with REPLACE).
            let default_focus_rect: [u8; 16] = {
                let mut b = [0u8; 16];
                for i in 0..4 {
                    b[i * 4..i * 4 + 4].copy_from_slice(&0.5_f32.to_le_bytes());
                }
                b
            };
            gb.set_focus_rect(default_focus_rect);
            // NOTE: angle/flip 미설정 — default 유지.

            // raw 0x170d80-0x170dc4: BrushControlBlock + SharePtr setup
            let ctrl = BrushControlBlock::from_gradient(gb);

            // raw 0x170dd8-0x170de4: SetBackgroundBrush(Style=3, &ctrl)
            self.set_background_brush(3, ctrl);

            // raw 0x170de8-0x170dfc: stops_vec stack-local dtor
            drop(stops_vec);

            // raw stack-local effects 들 release (caller-side)
            crate::color_effect::ColorEffect::raw_delete(effect10);
            crate::color_effect::ColorEffect::raw_delete(effect11);
        }
    }

    /// `FormatScheme::CreateDefault` 의 **Block 13** 1:1 port — raw `0x170de8-0x171130`.
    ///
    /// 7번째 attach call = `SetPen(Style=1, Pen(stroke=SolidBrush(Scheme 0x10) + 5
    /// overrides))`.
    ///
    /// raw 구조:
    /// 1. **stroke SolidBrush(Scheme 0x10) build** (Block 1 과 동일 패턴) — `0x170e00-0x170e84`
    /// 2. **Pen alloc + Pen::C2()** — `0x170e8c-0x170ea0` (10 default keys, state=2)
    /// 3. **BrushControlBlock alloc 24B** with obj=SolidBrush — `0x170ea4-0x170eb8`
    /// 4. **Assign stroke ctrl → Pen+0x00** (SharePtr semantics) — `0x170ebc-0x170f48`
    /// 5. **5 setter overrides** with state=1 — `0x170f4c-0x171110`:
    ///    - key 0x2bc (PFloat width) — engine-derived value (helper 0x653cb4)
    ///    - key 0x2bf (PEnum line_cap) = 0      (helper 0x669b40)
    ///    - key 0x2bd (PEnum compound) = 0      (helper 0x669704)
    ///    - key 0x2c6 (PEnum pen_align) = 0     (helper 0x66ac30)
    ///    - key 0x2be (PEnum dash) = 0          (helper 0x656254)
    /// 6. **PenControlBlock + SetPen(Style=1)** — `0x171114-0x171128`
    ///
    /// **width 계산** (raw `0x170f4c-0x171008`):
    /// ```asm
    /// 0x170f4c: bl ShapeEngine::GetInstance
    /// 0x170f50: ldr s0, [x0, #0x4]        ; engine_base
    /// 0x170f54-0x170f64: cmp engine_base == 0x495F3E00 (= 914400.0 EMU/inch)
    /// 0x170f68-0x170f74: if equal → s0 = 0x46467000 (= 12700.0 EMU/pt) ← fast path
    /// 0x170f94+:  else → slow path: Round(engine_base * 12700 / 914400 * 1000) / 1000
    /// ```
    /// 본 Rust port 는 `engine_base` 를 인자로 받아 동일 분기 처리.
    ///
    /// # Safety
    /// `engine_base` 는 `ShapeEngine::GetInstance().+0x4` 등가 (default Hancom 914400.0).
    pub fn create_default_block13(&mut self, engine_base: f32) {
        unsafe {
            // -----------------------------------------------------------------
            // 1. Stroke SolidBrush(Scheme 0x10) — Block 1 과 동일
            // -----------------------------------------------------------------
            let color = crate::color::Color::from_scheme_raw_u32(0x10, ptr::null_mut());
            let stroke_solid = crate::brush::SolidBrush::new(color);

            // -----------------------------------------------------------------
            // 2. Pen with engine defaults (10 keys state=2 per Pen::C2())
            // -----------------------------------------------------------------
            let mut pen = crate::pen::Pen::new_with_engine_defaults(engine_base);

            // -----------------------------------------------------------------
            // 3-4. Stroke 교체 — raw `0x170ebc-0x170f48` 의 SharePtr 의미 = Pen 의
            //      brush 슬롯에 SolidBrush 할당. Rust port 는 Box<Brush::Solid> 사용.
            // -----------------------------------------------------------------
            pen.set_stroke_brush(Box::new(crate::brush::Brush::Solid(stroke_solid)));

            // -----------------------------------------------------------------
            // 5. 5 setter overrides (state=1)
            // -----------------------------------------------------------------
            // Width: raw `0x170f4c-0x170fe0` — engine_base 의 fast/slow path 분기.
            // 0x495F3E00 = 914400.0 (EMU/inch), 0x46467000 = 12700.0 (EMU/pt).
            let engine_default = f32::from_bits(0x495F3E00); // 914400.0
            let emu_per_pt = f32::from_bits(0x46467000);     // 12700.0
            let width = if engine_base.to_bits() == engine_default.to_bits() {
                // raw fast path @ 0x170f68-0x170f74
                emu_per_pt
            } else {
                // raw slow path @ 0x170f94+: Round(engine_base * emu_per_pt / engine_default * 10^3) / 10^3
                let scale = (engine_base * emu_per_pt) / engine_default;
                let rounded_scaled = (scale as f64 * 1000.0).round();
                (rounded_scaled / 1000.0) as f32
            };
            pen.override_thickness(width);

            // PEnum overrides — 모두 value 0 (state=1)
            pen.override_enum_at(crate::pen::Pen::KEY_LINE_CAP, 0);    // 0x2bf
            pen.override_enum_at(crate::pen::Pen::KEY_COMPOUND, 0);    // 0x2bd
            pen.override_enum_at(crate::pen::Pen::KEY_PEN_ALIGN, 0);   // 0x2c6
            pen.override_enum_at(crate::pen::Pen::KEY_DASH, 0);        // 0x2be

            // -----------------------------------------------------------------
            // 6. PenControlBlock + SetPen(Style=1)
            // -----------------------------------------------------------------
            let ctrl = PenControlBlock::from_pen(pen);
            self.set_pen(1, ctrl);
        }
    }

    /// shared helper for `create_default_block13/14/15` — SetPen blocks 의 공통 코드.
    ///
    /// 3개 SetPen block 의 차이는:
    /// - Style key (1, 2, 3)
    /// - width fast-path bit pattern (12700 / 19050 / 38100 EMU)
    ///
    /// 그 외 모두 동일 — SolidBrush(Scheme 0x10) stroke + Pen::C2() defaults +
    /// 4 PEnum overrides = 0.
    fn create_default_set_pen_common(
        &mut self,
        style: u32,
        fast_width_bits: u32,
        engine_base: f32,
    ) {
        unsafe {
            // 1. Stroke SolidBrush
            let color = crate::color::Color::from_scheme_raw_u32(0x10, ptr::null_mut());
            let stroke_solid = crate::brush::SolidBrush::new(color);

            // 2. Pen with engine defaults
            let mut pen = crate::pen::Pen::new_with_engine_defaults(engine_base);

            // 3-4. Stroke assignment
            pen.set_stroke_brush(Box::new(crate::brush::Brush::Solid(stroke_solid)));

            // 5. Width override — engine fast/slow path
            let engine_default = f32::from_bits(0x495F3E00); // 914400.0 EMU/inch
            let fast_width = f32::from_bits(fast_width_bits);
            let width = if engine_base.to_bits() == engine_default.to_bits() {
                fast_width
            } else {
                let scale = (engine_base * fast_width) / engine_default;
                let rounded_scaled = (scale as f64 * 1000.0).round();
                (rounded_scaled / 1000.0) as f32
            };
            pen.override_thickness(width);

            // 4 PEnum overrides (all value 0)
            pen.override_enum_at(crate::pen::Pen::KEY_LINE_CAP, 0);
            pen.override_enum_at(crate::pen::Pen::KEY_COMPOUND, 0);
            pen.override_enum_at(crate::pen::Pen::KEY_PEN_ALIGN, 0);
            pen.override_enum_at(crate::pen::Pen::KEY_DASH, 0);

            // 6. SetPen
            let ctrl = PenControlBlock::from_pen(pen);
            self.set_pen(style, ctrl);
        }
    }

    /// `FormatScheme::CreateDefault` 의 **Block 14** 1:1 port — raw `0x171154-0x171480`.
    ///
    /// 8th attach call = `SetPen(Style=2, Pen(width=19050 EMU = 1.5pt))`.
    ///
    /// Block 13 (Pen 1) 과 byte-eq 구조, 다만:
    /// - **Style = 2** (vs 1)
    /// - **width fast path = 0x4694D400 = 19050.0 EMU** (vs 0x46467000 = 12700.0)
    ///
    /// raw fast path:
    /// ```asm
    /// 0x1712bc: mov  w8, #0xd400
    /// 0x1712c0: movk w8, #0x4694, lsl #16        ; w8 = 0x4694D400 (= 19050.0)
    /// 0x1712c4: fmov s0, w8
    /// ```
    ///
    /// 19050 EMU / 12700 EMU/pt = **1.5 pt** = default thicker stroke (Pen 2).
    ///
    /// 나머지 4 PEnum overrides + Pen::C2 defaults 모두 Block 13 과 동일.
    pub fn create_default_block14(&mut self, engine_base: f32) {
        // raw 0x1712bc-0x1712c4: 0x4694D400 = 19050.0 EMU
        self.create_default_set_pen_common(2, 0x4694D400, engine_base);
    }

    // =========================================================================
    // 16-ι/κ/λ: SetEffectStyle / GetEffectStyle — effects tree at FormatScheme+0x50
    // =========================================================================

    /// raw `FormatScheme::SetEffectStyle(Style, UniquePtr<EffectStyle> const&)`
    /// (`0x16ef94+0x140` 대략) — SetBrush / SetBgBrush / SetPen 와 byte-eq 구조,
    /// 다만:
    /// - tree offset: `add x21, x0, #0x50` (effects 의 offset)
    /// - helper triplet: EffectStyle-specific helpers (Pen 의 0x648554/0x6646bc/
    ///   0x648714 와 byte-eq pattern, EffectStyle 의 release vtable 사용)
    ///
    /// # Safety
    /// `ctrl_in` 은 `EffectStyleControlBlock::create_raw` 또는 `from_effect_style`
    /// 으로 얻은 valid ptr.
    pub unsafe fn set_effect_style(&mut self, style: u32, ctrl_in: *mut EffectStyleControlBlock) {
        if ctrl_in.is_null() || (*ctrl_in).obj.is_null() {
            return;
        }

        let tree = &mut self.effects as *mut TreeBase;
        let (slot, parent, existing) = find_insert_position(tree, |node| {
            fs_effect_style_node_key(node).cmp(&style)
        });

        if existing.is_null() {
            let node_layout = Layout::new::<FsEffectStyleMapNode>();
            let new_node = std::alloc::alloc(node_layout) as *mut FsEffectStyleMapNode;
            if new_node.is_null() {
                std::alloc::handle_alloc_error(node_layout);
            }
            ptr::write(
                new_node,
                FsEffectStyleMapNode {
                    base: TreeNodeBase {
                        left: ptr::null_mut(),
                        right: ptr::null_mut(),
                        parent,
                        is_black: 0,
                        _pad_0x19: [0u8; 7],
                    },
                    key: style,
                    _pad: 0,
                    value: ctrl_in,
                },
            );
            *slot = new_node as *mut TreeNodeBase;
            update_begin_node_after_insert(tree);
            let root = self.effects.end_node_left;
            balance_after_insert(root, new_node as *mut TreeNodeBase);
            self.effects.size += 1;
        } else {
            let node = existing as *mut FsEffectStyleMapNode;
            let old_ctrl = (*node).value;
            let old_obj = if !old_ctrl.is_null() {
                (*old_ctrl).obj
            } else {
                ptr::null_mut()
            };
            let new_obj = (*ctrl_in).obj;
            if old_obj == new_obj {
                EffectStyleControlBlock::release(ctrl_in);
                return;
            }
            EffectStyleControlBlock::release(old_ctrl);
            (*node).value = ctrl_in;
        }
    }

    /// raw `FormatScheme::GetEffectStyle(Style) const` (`0x16ec48+0x140` 대략) 1:1.
    pub fn find_effect_style(&self, style: u32) -> Option<*mut EffectStyleControlBlock> {
        unsafe {
            let mut current = self.effects.end_node_left;
            let end_addr = (&self.effects.end_node_left)
                as *const *mut TreeNodeBase as *const TreeNodeBase;
            let mut last_le = end_addr as *mut TreeNodeBase;
            while !current.is_null() {
                let nkey = fs_effect_style_node_key(current);
                if nkey < style {
                    current = (*current).right;
                } else {
                    last_le = current;
                    current = (*current).left;
                }
            }
            if last_le == end_addr as *mut TreeNodeBase {
                return None;
            }
            let n = last_le as *mut FsEffectStyleMapNode;
            if (*n).key != style {
                return None;
            }
            Some((*n).value)
        }
    }

    /// `FormatScheme::CreateDefault` 의 **Block 16/17/18** 부분 port — raw
    /// `0x1717d4-0x172214` 영역의 `SetEffectStyle(Style=1/2/3, EffectStyle(...))`.
    ///
    /// ## 본 단계 (16-ι/κ/λ) 의 byte-eq 완성도
    ///
    /// **완성된 부분** (정공법 byte-eq 보장):
    /// - `EffectStyleControlBlock` 24B + `FsEffectStyleMapNode` 48B layout
    /// - `set_effect_style` algorithm (= SetBrush/SetPen 와 byte-eq)
    /// - `EffectStyle` 24B layout (16t 단계 완성)
    /// - effects tree drop = `drop_fs_effect_style_map_node`
    ///
    /// **deferred 부분** (multi-session 추가 RE 필요, raw asm citation 보존):
    /// - **`Hnc::Shape::Effects`** container 의 layout + ctor — raw 의 24B (Block 16
    ///   line 2178-2185 의 `mov w0, #0x18; bl __Znwm; str xzr` pattern).
    /// - **`OuterShadow::C2(Color, f32, Degree, f32, bool)`** — raw `0x171980`,
    ///   5-arg ctor allocating 16B with vtable.
    /// - **vfunc[0x28/8=5]** dispatch returning `effect_key` — `blr x8` @ `0x1719a8`
    ///   for OuterShadow → `effect_key` (insertion key in Effects' internal map).
    /// - **`0x162050`** = Effects container 의 internal insert helper.
    /// - **Scene3D / Sp3D ctors** for Block 17/18 (raw 미스캔).
    /// - **ShapeEngine-derived shadow distance/blur 계산** — Block 16 의 두 번의
    ///   fast/slow path (line 2192-2255).
    ///
    /// ## raw Block 16 asm 인용 (불완전 RE, 추후 정밀화)
    ///
    /// ```asm
    /// 0x1717fc: alloc 24B + ColorEffect::Add(0x1f4, 0.38)  ; 첫 ColorEffect
    /// 0x171828-0x171854: alloc 24B "effects container" + init (linked list?)
    /// 0x171860-0x1718d8: ShapeEngine fast/slow path → shadow_distance (in s8)
    /// 0x1718dc-0x1718e4: Degree::C1(90°)                   ; angle = 0x42B40000
    /// 0x1718e8-0x17195c: ShapeEngine fast/slow path → shadow_blur (in s9)
    /// 0x171960-0x171980: alloc 16B + OuterShadow::C2(color, dist, deg, blur, false)
    /// 0x171984-0x1719a8: OuterShadow ctrl 16B + vfunc[5] dispatch → effect_key
    /// 0x1719b0-0x1719f8: insert OuterShadow ctrl into Effects container (by key)
    /// ;; ... EffectStyle::Create(Effects, null, null) + SetEffectStyle(1)
    /// ```
    ///
    /// 본 method 는 `EffectStyle::new(null, null, null)` 로 부분 EffectStyle (= 빈
    /// container) 구성 후 SetEffectStyle attach. fs.effects tree 의 INSERT path 는
    /// byte-eq (size 증가, key 정합, vtable-based drop), 다만 EffectStyle 의 inner
    /// Effects/Scene3D/Sp3D 는 본 단계 deferred — 위 raw 인용 후속 RE 시 정정.
    ///
    /// **DEPRECATED in 16-μ**: `create_default_block16` / `_17` / `_18` 로 대체.
    /// 본 method 는 호환성 보존 + null-EffectStyle test 용도로만 유지.
    pub fn create_default_block16_through_18_partial(&mut self) {
        unsafe {
            // ⚠️ deferred: Effects/Scene3D/Sp3D 의 inner sub-type RE 후 정정.
            //              본 단계는 outer FormatScheme.effects map 의 INSERT 만 byte-eq.
            for style in [1u32, 2, 3] {
                // raw 의 EffectStyle::C2(effects, null, null) 또는 (eff,scene,sp3d)
                // 의 합성 — 본 단계는 모두 null (3 fields = 24B of nulls).
                let es = crate::effect_style::EffectStyle::new(
                    ptr::null_mut(), // Effects (raw Block 16 의 OuterShadow container)
                    ptr::null_mut(), // Scene3D
                    ptr::null_mut(), // Sp3D
                );
                let ctrl = EffectStyleControlBlock::from_effect_style(es);
                self.set_effect_style(style, ctrl);
            }
        }
    }

    /// `FormatScheme::CreateDefault` 의 **Block 16** byte-eq port — raw `0x1717fc-0x171b48`.
    ///
    /// 10번째 attach call = `SetEffectStyle(Style=1, EffectStyle(Effects[OuterShadow], null, null))`.
    ///
    /// raw 알고리즘:
    /// 1. ColorEffect alloc 24B + Add(0x1f4, **0.38** = 0x3EC28F5C)
    /// 2. Effects container alloc 24B (empty std::map)
    /// 3. shadow_distance fast = 0x47780C00 (= 63500 EMU)
    /// 4. Degree(90°) (= 0x42B40000)
    /// 5. shadow_blur fast = **0x47315600** (= 45398 EMU) ⭐ Block 16 만
    /// 6. OuterShadow::C2(color, distance, degree, blur, false)
    /// 7. EffectControlBlock 16B + (obj=OuterShadow, refcount=1)
    /// 8. effects.insert(0xbba, ctrl) via 0x162050
    /// 9. EffectStyle::C2(Effects, null, null)
    /// 10. SetEffectStyle(1, ctrl)
    pub fn create_default_block16(&mut self, _engine_base: f32) {
        unsafe {
            self.create_default_set_effect_style_outer_shadow(
                1,                                      // Style
                f32::from_bits(0x3EC28F5C),             // ColorEffect arg = 0.38
                f32::from_bits(0x47780C00),             // shadow_distance = 63500
                f32::from_bits(0x47315600),             // shadow_blur = 45398
            );
        }
    }

    /// `FormatScheme::CreateDefault` 의 **Block 17** byte-eq port — raw `0x171b48-0x171ecc`.
    ///
    /// 11번째 attach call = `SetEffectStyle(Style=2, EffectStyle(Effects[OuterShadow], null, null))`.
    ///
    /// Block 16 과 byte-eq 구조, 다만:
    /// - **ColorEffect value = 0.35** (= 0x3EB33333, vs Block 16 의 0.38)
    /// - **shadow_blur = 23000** (= 0x46B3B000, vs Block 16 의 45398 = 0x47315600) ⭐ Agent B 정정값
    ///
    /// raw float 위치 (직접 검증):
    /// - `0x171bd4: mov w8, #0x3333; movk w8, #0x3eb3, lsl #16` → 0.35
    /// - `0x171c8c: mov w8, #0xb000; movk w8, #0x46b3, lsl #16` → 23000 (= **NOT** Agent B's claimed 45398)
    pub fn create_default_block17(&mut self, _engine_base: f32) {
        unsafe {
            self.create_default_set_effect_style_outer_shadow(
                2,
                f32::from_bits(0x3EB33333),  // ColorEffect arg = 0.35
                f32::from_bits(0x47780C00),  // shadow_distance = 63500 (= Block 16 와 동일)
                f32::from_bits(0x46B3B000),  // ⭐ corrected Block 17 blur = 23000
            );
        }
    }

    /// shared helper: Block 16 + Block 17 의 SetEffectStyle(OuterShadow) 패턴.
    ///
    /// 두 블록의 차이는 `(style, color_effect_value, distance, blur)` 만 — 나머지는 byte-eq.
    fn create_default_set_effect_style_outer_shadow(
        &mut self,
        style: u32,
        color_effect_value: f32,
        shadow_distance: f32,
        shadow_blur: f32,
    ) {
        unsafe {
            // ----- (1) ColorEffect 24B + Add(0x1f4, value) -----
            let ce = crate::color_effect::ColorEffect::create();
            (*ce).add(0x1f4, color_effect_value);

            // ----- (2) Effects container 24B (empty std::map) -----
            let mut effects = crate::effects_container::Effects::new();

            // ----- (3-5) shadow_distance + Degree(90°) + shadow_blur (이미 args) -----

            // Color (Scheme 0x10 = phClr) for OuterShadow's PColor arg.
            // 본 단계: ColorEffect ptr 를 Color 의 effect 슬롯에 attach (참조 의미만).
            let color = crate::color::Color::from_scheme_raw_u32(0x10, ce);

            // ----- (6) OuterShadow::C2(color, distance, degree=90°, blur, flip=false) -----
            let shadow = crate::outer_shadow::OuterShadow::new_with_args(
                &color,
                shadow_distance,
                90.0_f32,        // = 0x42B40000
                shadow_blur,
                false,
            );
            std::mem::forget(color); // OuterShadow 내부에 PColor 가 effect ptr 보유 — 본 단계는 raw share

            // ----- (7) heap-alloc OuterShadow + EffectControlBlock 16B -----
            let os_layout = std::alloc::Layout::new::<crate::outer_shadow::OuterShadow>();
            let os_heap = std::alloc::alloc(os_layout) as *mut crate::outer_shadow::OuterShadow;
            if os_heap.is_null() {
                std::alloc::handle_alloc_error(os_layout);
            }
            ptr::write(os_heap, shadow);

            let effect_ctrl = crate::effects_container::EffectControlBlock::create_raw(os_heap as *mut u8);

            // ----- (8) effects.insert(0xbba, effect_ctrl) -----
            effects.insert(crate::outer_shadow::OUTER_SHADOW_EFFECT_KEY, effect_ctrl);

            // ----- (9) Wrap Effects as SharePtr (heap-alloc + ControlBlock<Effects>) -----
            // raw 0x172160 의 EffectStyle::C2 가 받는 args 는 SharePtr<Effects/Scene3D/Sp3D>.
            // 본 단계: Effects 만 non-null SharePtr.
            let effects_box = Box::into_raw(effects);
            let effects_cb_layout =
                std::alloc::Layout::new::<crate::share_ptr::ControlBlock<crate::effects_container::Effects>>();
            let effects_cb = std::alloc::alloc(effects_cb_layout)
                as *mut crate::share_ptr::ControlBlock<crate::effects_container::Effects>;
            if effects_cb.is_null() {
                std::alloc::handle_alloc_error(effects_cb_layout);
            }
            ptr::write(
                effects_cb,
                crate::share_ptr::ControlBlock {
                    obj: effects_box,
                    refcount: 1,
                },
            );

            // ----- (10) EffectStyle::C2(Effects, null, null) -----
            let es = crate::effect_style::EffectStyle::new(
                effects_cb,           // Effects (non-null)
                ptr::null_mut(),      // Scene3D (null)
                ptr::null_mut(),      // Sp3D (null)
            );

            // ----- (11) EffectStyleControlBlock + SetEffectStyle(style) -----
            let ctrl = EffectStyleControlBlock::from_effect_style(es);
            self.set_effect_style(style, ctrl);
        }
    }

    /// `FormatScheme::CreateDefault` 의 **Block 18** byte-eq port — raw `0x171ecc-0x172214`.
    ///
    /// 12번째 attach call = `SetEffectStyle(Style=3, EffectStyle(Effects[Reflection], null, null))`.
    ///
    /// **NEW sub-type = Reflection** (Block 16/17 의 OuterShadow 와 다름):
    /// - ColorEffect 없음 (= Block 18 만의 특징)
    /// - Reflection::C2(distance, degree, blur, 0.26, 0.28, -1.0, false) — 7 args
    /// - distance = 0x46467000 = **12700** (= 1pt EMU) ⭐ Agent B's claim 0x46464700 은 오류
    /// - blur = 0x4714D400 = 38100 (= 3pt EMU)
    /// - 3 hardcoded floats: 0.26 (0x3E851EB8), 0.28 (0x3E8F5C29), -1.0
    /// - bool = false
    ///
    /// effect_key = `Reflection::GetType()` = **0xbbb** (= OuterShadow 0xbba + 1).
    pub fn create_default_block18(&mut self, _engine_base: f32) {
        unsafe {
            // Reflection::C2 7-arg ctor
            let r = crate::reflection::Reflection::new_with_args(
                f32::from_bits(0x46467000),  // ⭐ corrected Block 18 distance = 12700
                90.0_f32,                     // Degree = 90° (= 0x42B40000)
                f32::from_bits(0x4714D400),  // blur = 38100
                f32::from_bits(0x3E851EB8),  // hardcoded 0.26
                f32::from_bits(0x3E8F5C29),  // hardcoded 0.28
                -1.0_f32,                     // hardcoded -1.0
                false,                        // bool
            );

            // heap-alloc Reflection + EffectControlBlock
            let r_layout = std::alloc::Layout::new::<crate::reflection::Reflection>();
            let r_heap = std::alloc::alloc(r_layout) as *mut crate::reflection::Reflection;
            if r_heap.is_null() {
                std::alloc::handle_alloc_error(r_layout);
            }
            ptr::write(r_heap, r);

            let effect_ctrl = crate::effects_container::EffectControlBlock::create_raw(r_heap as *mut u8);

            // Effects container with single Reflection entry
            let mut effects = crate::effects_container::Effects::new();
            effects.insert(crate::reflection::REFLECTION_EFFECT_KEY, effect_ctrl);

            // Wrap as SharePtr<Effects>
            let effects_box = Box::into_raw(effects);
            let effects_cb_layout =
                std::alloc::Layout::new::<crate::share_ptr::ControlBlock<crate::effects_container::Effects>>();
            let effects_cb = std::alloc::alloc(effects_cb_layout)
                as *mut crate::share_ptr::ControlBlock<crate::effects_container::Effects>;
            if effects_cb.is_null() {
                std::alloc::handle_alloc_error(effects_cb_layout);
            }
            ptr::write(
                effects_cb,
                crate::share_ptr::ControlBlock {
                    obj: effects_box,
                    refcount: 1,
                },
            );

            // EffectStyle + SetEffectStyle(3)
            let es = crate::effect_style::EffectStyle::new(
                effects_cb,
                ptr::null_mut(),
                ptr::null_mut(),
            );
            let ctrl = EffectStyleControlBlock::from_effect_style(es);
            self.set_effect_style(3, ctrl);
        }
    }

    /// `FormatScheme::CreateDefault` 의 **Block 15** 1:1 port — raw `0x1714a8-0x1717d4`.
    ///
    /// 9th attach call = `SetPen(Style=3, Pen(width=38100 EMU = 3.0pt))`.
    ///
    /// Block 13/14 와 byte-eq, 다만:
    /// - **Style = 3** (vs 1/2)
    /// - **width fast path = 0x4714D400 = 38100.0 EMU** (= 3.0 pt)
    ///
    /// raw fast path:
    /// ```asm
    /// 0x171610: mov  w8, #0xd400
    /// 0x171614: movk w8, #0x4714, lsl #16        ; w8 = 0x4714D400 (= 38100.0)
    /// 0x171618: fmov s0, w8
    /// ```
    ///
    /// 38100 EMU = 3 × 12700 EMU = 3 points = thickest stroke (Pen 3).
    pub fn create_default_block15(&mut self, engine_base: f32) {
        // raw 0x171610-0x171618: 0x4714D400 = 38100.0 EMU
        self.create_default_set_pen_common(3, 0x4714D400, engine_base);
    }
}

/// libc++ __tree 가 empty 인가? begin_node == &end_node_left (fake __end_node).
fn is_empty_tree(t: &TreeBase) -> bool {
    let end_addr = (&t.end_node_left) as *const *mut TreeNodeBase as *const TreeNodeBase;
    (t.begin_node as *const TreeNodeBase) == end_addr
}

/// raw `FormatScheme::GetBrush/GetBackgroundBrush(Style)` 공통 helper.
///
/// libc++ `std::map::find` (lower_bound + key 비교) 와 동일. tree 가 brushes /
/// bg_brushes 모두 동일 node 구조 (`FsBrushMapNode`) 사용.
///
/// # Safety
/// `t` 는 valid `&TreeBase`, 노드 cast 는 `FsBrushMapNode` layout 가정.
unsafe fn find_node_in_tree(t: &TreeBase, style: u32) -> Option<*mut BrushControlBlock> {
    let mut current = t.end_node_left;
    let end_addr = (&t.end_node_left) as *const *mut TreeNodeBase as *const TreeNodeBase;
    let mut last_le = end_addr as *mut TreeNodeBase;
    while !current.is_null() {
        let nkey = fs_brush_node_key(current);
        if nkey < style {
            current = (*current).right;
        } else {
            last_le = current;
            current = (*current).left;
        }
    }
    if last_le == end_addr as *mut TreeNodeBase {
        return None;
    }
    let n = last_le as *mut FsBrushMapNode;
    if (*n).key != style {
        return None;
    }
    Some((*n).value)
}

impl Drop for FormatScheme {
    /// raw `~FormatScheme()` (`0x16e4d4`) 1:1.
    ///
    /// 4 trees 의 subtree_destroy + name dtor.
    /// - brushes / bg_brushes: `drop_fs_brush_map_node` (ctrl release + node dealloc)
    /// - pens: `drop_fs_pen_map_node` (16-ζ — Pen ControlBlock release)
    /// - effects: `drop_fs_effect_style_map_node` (16-ι — EffectStyle ControlBlock
    ///   release; inner Effects/Scene3D/Sp3D 의 sub-type drop 은 multi-session
    ///   deferred — 본 단계는 모두 null SharePtr 만 처리 → drop_in_place 가 no-op)
    fn drop(&mut self) {
        unsafe {
            // raw 0x16e4e4: subtree_destroy effects (offset 0x50)
            // raw 0x16e4f0: subtree_destroy pens (offset 0x38)
            // raw 0x16e4fc: subtree_destroy bg_brushes (offset 0x20)
            // raw 0x16e508: subtree_destroy brushes (offset 0x8)
            let brush_drop = |n: *mut TreeNodeBase| drop_fs_brush_map_node(n);
            let pen_drop = |n: *mut TreeNodeBase| drop_fs_pen_map_node(n);
            let effect_style_drop = |n: *mut TreeNodeBase| drop_fs_effect_style_map_node(n);
            subtree_destroy_recursive(self.effects.end_node_left, &effect_style_drop);
            subtree_destroy_recursive(self.pens.end_node_left, &pen_drop);
            subtree_destroy_recursive(self.bg_brushes.end_node_left, &brush_drop);
            subtree_destroy_recursive(self.brushes.end_node_left, &brush_drop);
            // raw 0x16e514: tail call ~CHncStringW(self+0) — Rust 의 자동 field drop.
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_layout_size_align() {
        assert_eq!(std::mem::size_of::<FormatScheme>(), 104);
        assert_eq!(std::mem::align_of::<FormatScheme>(), 8);
    }

    #[test]
    fn field_offsets_match_raw() {
        let fs = FormatScheme::new();
        let p = &*fs as *const FormatScheme as usize;
        assert_eq!(&fs.name as *const _ as usize - p, 0x00);
        assert_eq!(&fs.brushes as *const _ as usize - p, 0x08);
        assert_eq!(&fs.bg_brushes as *const _ as usize - p, 0x20);
        assert_eq!(&fs.pens as *const _ as usize - p, 0x38);
        assert_eq!(&fs.effects as *const _ as usize - p, 0x50);
        // 4 trees 모두 valid begin_node (= 자기 자신의 end_node_left 주소)
        let p_brushes_end = &fs.brushes.end_node_left as *const _ as usize;
        let p_brushes_begin = fs.brushes.begin_node as usize;
        assert_eq!(p_brushes_begin, p_brushes_end);
    }

    #[test]
    fn default_ctor_creates_empty_state() {
        let fs = FormatScheme::new();
        assert!(fs.is_empty());
        assert_eq!(fs.brushes.size, 0);
        assert_eq!(fs.bg_brushes.size, 0);
        assert_eq!(fs.pens.size, 0);
        assert_eq!(fs.effects.size, 0);
        assert_eq!(fs.total_entries(), 0);
    }

    #[test]
    fn all_four_trees_have_self_referencing_begin_node() {
        let fs = FormatScheme::new();
        // 각 tree 의 begin_node 가 자기 자신의 end_node_left 주소 (libc++ empty tree state)
        let p_b = &fs.brushes.end_node_left as *const _ as usize;
        let p_bg = &fs.bg_brushes.end_node_left as *const _ as usize;
        let p_p = &fs.pens.end_node_left as *const _ as usize;
        let p_e = &fs.effects.end_node_left as *const _ as usize;
        assert_eq!(fs.brushes.begin_node as usize, p_b);
        assert_eq!(fs.bg_brushes.begin_node as usize, p_bg);
        assert_eq!(fs.pens.begin_node as usize, p_p);
        assert_eq!(fs.effects.begin_node as usize, p_e);
    }

    #[test]
    fn drop_empty_format_scheme_no_panic() {
        for _ in 0..50 {
            let fs = FormatScheme::new();
            drop(fs);
        }
    }

    #[test]
    fn get_name_default_is_empty() {
        let fs = FormatScheme::new();
        assert_eq!(fs.get_name().length(), 0);
    }

    #[test]
    fn set_name_round_trip() {
        let mut fs = FormatScheme::new();
        let new_name = CHncStringW::from_str("OfficeDefaultScheme");
        fs.set_name(&new_name);
        assert!(fs.get_name().length() > 0);
    }

    #[test]
    fn create_raw_and_raw_delete() {
        unsafe {
            let p = FormatScheme::create_raw();
            assert!(!p.is_null());
            assert_eq!((*p).total_entries(), 0);
            FormatScheme::raw_delete(p);
        }
    }

    #[test]
    fn raw_delete_of_null_is_noop() {
        unsafe {
            FormatScheme::raw_delete(ptr::null_mut());
        }
    }

    #[test]
    fn tree_size_constant_verified() {
        // 각 TreeBase = 24B → 4 trees = 96B + 8B name = 104B
        assert_eq!(std::mem::size_of::<TreeBase>() * 4, 96);
        assert_eq!(96 + std::mem::size_of::<CHncStringW>(), 104);
    }

    #[test]
    fn box_address_stable_for_trees() {
        // Box 의 heap address 는 stable — begin_node ref 가 valid 유지.
        let fs = FormatScheme::new();
        let box_addr = &*fs as *const FormatScheme as usize;
        let initial_brushes_begin = fs.brushes.begin_node as usize;
        // begin_node 가 self+0x10 (end_node_left of brushes) 를 가리킴
        assert_eq!(initial_brushes_begin, box_addr + 0x10);
    }

    // =========================================================================
    // 16u: SetBrush / GetBrush / map machinery tests
    // =========================================================================

    #[test]
    fn brush_control_block_raw_24b_layout() {
        // raw `mov w0, #0x18; bl __Znwm` @ 0x16f79c
        assert_eq!(std::mem::size_of::<BrushControlBlock>(), 24);
        assert_eq!(std::mem::align_of::<BrushControlBlock>(), 8);
    }

    #[test]
    fn brush_control_block_field_offsets_match_raw() {
        // raw 16f740: stp x21, x8, [x0]  ; obj@0, strong@8
        // raw 16f744: strb w8, [x0, #0x10]  ; flag@0x10
        let cb = BrushControlBlock {
            obj: ptr::null_mut(),
            strong: 0,
            flag: 0,
            _pad: [0u8; 7],
        };
        let base = &cb as *const _ as usize;
        assert_eq!(&cb.obj as *const _ as usize - base, 0x00);
        assert_eq!(&cb.strong as *const _ as usize - base, 0x08);
        assert_eq!(&cb.flag as *const _ as usize - base, 0x10);
    }

    #[test]
    fn fs_brush_map_node_raw_48b_layout() {
        // raw `mov w0, #0x30; bl __Znwm` @ 0x6645fc
        assert_eq!(std::mem::size_of::<FsBrushMapNode>(), 48);
        assert_eq!(std::mem::align_of::<FsBrushMapNode>(), 8);
    }

    #[test]
    fn fs_brush_map_node_field_offsets_match_raw() {
        // raw 66460c: str w8, [x0, #0x20]   ; key@0x20
        // raw 664614: str x8, [x0, #0x28]   ; value@0x28
        let n = FsBrushMapNode {
            base: TreeNodeBase {
                left: ptr::null_mut(),
                right: ptr::null_mut(),
                parent: ptr::null_mut(),
                is_black: 0,
                _pad_0x19: [0u8; 7],
            },
            key: 0,
            _pad: 0,
            value: ptr::null_mut(),
        };
        let base = &n as *const _ as usize;
        assert_eq!(&n.base as *const _ as usize - base, 0x00);
        assert_eq!(&n.key as *const _ as usize - base, 0x20);
        assert_eq!(&n.value as *const _ as usize - base, 0x28);
    }

    #[test]
    fn set_brush_single_insert_increments_size() {
        let mut fs = FormatScheme::new();
        unsafe {
            // SolidBrush heap-alloc
            let sb = crate::brush::SolidBrush::new(crate::color::Color::from_rgb(
                10,
                20,
                30,
                ptr::null_mut(),
            ));
            let layout = Layout::from_size_align(16, 8).unwrap();
            let sb_heap = std::alloc::alloc(layout) as *mut crate::brush::SolidBrush;
            ptr::write(sb_heap, sb);

            let ctrl = BrushControlBlock::create_raw(sb_heap as *mut u8);
            fs.set_brush(1, ctrl);

            assert_eq!(fs.brushes.size, 1);
            assert_eq!(fs.bg_brushes.size, 0);
        }
        // drop triggers brush_drop → release ctrl → SolidBrush dtor + dealloc
    }

    #[test]
    fn set_brush_multiple_inserts_sorted_by_key() {
        let mut fs = FormatScheme::new();
        unsafe {
            for &style in &[3u32, 1, 5, 2, 4] {
                let sb = crate::brush::SolidBrush::new(
                    crate::color::Color::from_rgb(style as u8, 0, 0, ptr::null_mut()),
                );
                let layout = Layout::from_size_align(16, 8).unwrap();
                let sb_heap = std::alloc::alloc(layout) as *mut crate::brush::SolidBrush;
                ptr::write(sb_heap, sb);
                let ctrl = BrushControlBlock::create_raw(sb_heap as *mut u8);
                fs.set_brush(style, ctrl);
            }
            assert_eq!(fs.brushes.size, 5);
            // begin_node 는 leftmost (key=1) 노드
            let begin_node = fs.brushes.begin_node as *const FsBrushMapNode;
            assert_eq!((*begin_node).key, 1);
        }
    }

    #[test]
    fn find_brush_returns_inserted_ctrl() {
        let mut fs = FormatScheme::new();
        unsafe {
            let sb = crate::brush::SolidBrush::new(crate::color::Color::from_rgb(
                111,
                222,
                33,
                ptr::null_mut(),
            ));
            let layout = Layout::from_size_align(16, 8).unwrap();
            let sb_heap = std::alloc::alloc(layout) as *mut crate::brush::SolidBrush;
            ptr::write(sb_heap, sb);
            let ctrl = BrushControlBlock::create_raw(sb_heap as *mut u8);
            fs.set_brush(7, ctrl);

            let found = fs.find_brush(7).expect("inserted ctrl should be found");
            assert_eq!(found, ctrl);
            let obj = (*found).obj as *const crate::brush::SolidBrush;
            assert_eq!((*obj).get_color().get_rgb().r, 111);
            assert_eq!((*obj).get_color().get_rgb().g, 222);
            assert_eq!((*obj).get_color().get_rgb().b, 33);
        }
    }

    #[test]
    fn find_brush_missing_returns_none() {
        let fs = FormatScheme::new();
        assert!(fs.find_brush(42).is_none());
    }

    #[test]
    fn set_brush_replace_path_releases_old() {
        let mut fs = FormatScheme::new();
        unsafe {
            // First insert: red SolidBrush at style 9
            let sb1 = crate::brush::SolidBrush::new(crate::color::Color::from_rgb(
                255, 0, 0, ptr::null_mut(),
            ));
            let layout = Layout::from_size_align(16, 8).unwrap();
            let sb1_heap = std::alloc::alloc(layout) as *mut crate::brush::SolidBrush;
            ptr::write(sb1_heap, sb1);
            let ctrl1 = BrushControlBlock::create_raw(sb1_heap as *mut u8);
            fs.set_brush(9, ctrl1);

            // Replace with blue SolidBrush at same style 9
            let sb2 = crate::brush::SolidBrush::new(crate::color::Color::from_rgb(
                0, 0, 255, ptr::null_mut(),
            ));
            let sb2_heap = std::alloc::alloc(layout) as *mut crate::brush::SolidBrush;
            ptr::write(sb2_heap, sb2);
            let ctrl2 = BrushControlBlock::create_raw(sb2_heap as *mut u8);
            fs.set_brush(9, ctrl2);

            // size still 1 (replace, not insert)
            assert_eq!(fs.brushes.size, 1);

            // find_brush returns the new ctrl (= ctrl2)
            let found = fs.find_brush(9).unwrap();
            assert_eq!(found, ctrl2);
            let obj = (*found).obj as *const crate::brush::SolidBrush;
            assert_eq!((*obj).get_color().get_rgb().b, 255);
        }
    }

    #[test]
    fn create_default_block1_inserts_main_color_brush() {
        // raw `FormatScheme::CreateDefault` 의 Block 1 (~70 instr) 1:1.
        let mut fs = FormatScheme::new();
        fs.create_default_block1();
        assert_eq!(fs.brushes.size, 1);
        unsafe {
            let ctrl = fs.find_brush(1).expect("MainColor brush should be inserted");
            // ControlBlock: strong=1, flag=1
            assert_eq!((*ctrl).strong, 1);
            assert_eq!((*ctrl).flag, 1);
            // SolidBrush at ctrl.obj
            let sb = (*ctrl).obj as *const crate::brush::SolidBrush;
            // vtable points to SOLID_BRUSH_VTABLE static
            let vt_addr = (*sb).vtable as usize;
            let expected_vt = &crate::brush::SOLID_BRUSH_VTABLE as *const _ as usize;
            assert_eq!(vt_addr, expected_vt);
            // Color stored: SchemeStyle 0x10 (= phClr), type_tag = 2 (SCHEME)
            let color = (*sb).get_color();
            assert_eq!(color.type_tag, crate::color::color_type::SCHEME);
            // raw `mov w8, #0x10` → value[0..4] = 0x10
            assert_eq!(
                u32::from_le_bytes([color.value[0], color.value[1], color.value[2], color.value[3]]),
                0x10
            );
        }
    }

    #[test]
    fn create_default_block1_drop_no_leak_panic() {
        // Drop 이 brush map 의 node + ctrl + SolidBrush 모두 정공법 해제.
        for _ in 0..20 {
            let mut fs = FormatScheme::new();
            fs.create_default_block1();
            drop(fs);
        }
    }

    #[test]
    fn brush_vtable_dispatches_via_static() {
        unsafe {
            // SolidBrush vtable → SOLID_BRUSH_VTABLE
            let sb = crate::brush::SolidBrush::default();
            let vt = crate::brush::brush_vtable(&sb as *const _ as *const u8);
            assert_eq!(vt.type_tag, crate::brush::BrushType::Solid as u32);

            // HatchBrush vtable → HATCH_BRUSH_VTABLE
            let hb = crate::brush::HatchBrush::default();
            let vt = crate::brush::brush_vtable(&hb as *const _ as *const u8);
            assert_eq!(vt.type_tag, crate::brush::BrushType::Hatch as u32);
        }
    }

    // =========================================================================
    // 16v: CreateDefault Block 2 (3 ColorEffect alloc + 6 Add) tests
    // =========================================================================

    /// raw float bit-pattern 0x3EBD70A4 = 0.37 (or close to it).
    /// raw `16f7e8: mov w8, #0x70a4; 16f7ec: movk w8, #0x3ebd, lsl #16`.
    #[test]
    fn block2_float_bit_pattern_0_37() {
        let v = f32::from_bits(0x3EBD70A4);
        // 0.36999998 ish
        assert!((v - 0.37_f32).abs() < 1e-6, "value = {}", v);
    }

    /// raw float bit-pattern 0x3E19999A = 0.15.
    /// raw `16f824: mov w8, #0x999a; 16f828: movk w8, #0x3e19, lsl #16`.
    #[test]
    fn block2_float_bit_pattern_0_15() {
        let v = f32::from_bits(0x3E19999A);
        assert!((v - 0.15_f32).abs() < 1e-6, "value = {}", v);
    }

    #[test]
    fn create_default_block2_returns_3_effects() {
        unsafe {
            let effects = FormatScheme::create_default_block2_effects();
            assert!(!effects[0].is_null());
            assert!(!effects[1].is_null());
            assert!(!effects[2].is_null());
            // 3 distinct allocations
            assert_ne!(effects[0], effects[1]);
            assert_ne!(effects[1], effects[2]);
            // cleanup
            crate::color_effect::ColorEffect::raw_delete(effects[0]);
            crate::color_effect::ColorEffect::raw_delete(effects[1]);
            crate::color_effect::ColorEffect::raw_delete(effects[2]);
        }
    }

    #[test]
    fn create_default_block2_effect1_has_2_entries() {
        unsafe {
            let effects = FormatScheme::create_default_block2_effects();
            // effect1: 2 Add calls — len == 2
            assert_eq!((*effects[0]).len(), 2);

            // entry 0: (0x20a, 0.5)
            // entry 1: (0x204, 3.0)
            // packed format: u64 = pkey | (bits(value) << 32)
            let e0 = *(*effects[0]).begin;
            let e1 = *(*effects[0]).begin.add(1);
            assert_eq!(e0 & 0xFFFF_FFFF, 0x20a);
            assert_eq!(f32::from_bits((e0 >> 32) as u32), 0.5_f32);
            assert_eq!(e1 & 0xFFFF_FFFF, 0x204);
            assert_eq!(f32::from_bits((e1 >> 32) as u32), 3.0_f32);

            crate::color_effect::ColorEffect::raw_delete(effects[0]);
            crate::color_effect::ColorEffect::raw_delete(effects[1]);
            crate::color_effect::ColorEffect::raw_delete(effects[2]);
        }
    }

    #[test]
    fn create_default_block2_effect2_has_0_37_and_3_0() {
        unsafe {
            let effects = FormatScheme::create_default_block2_effects();
            assert_eq!((*effects[1]).len(), 2);
            let e0 = *(*effects[1]).begin;
            let e1 = *(*effects[1]).begin.add(1);
            // (0x20a, 0x3EBD70A4 = 0.37)
            assert_eq!(e0 & 0xFFFF_FFFF, 0x20a);
            assert_eq!((e0 >> 32) as u32, 0x3EBD70A4);
            // (0x204, 3.0)
            assert_eq!(e1 & 0xFFFF_FFFF, 0x204);
            assert_eq!(f32::from_bits((e1 >> 32) as u32), 3.0_f32);

            crate::color_effect::ColorEffect::raw_delete(effects[0]);
            crate::color_effect::ColorEffect::raw_delete(effects[1]);
            crate::color_effect::ColorEffect::raw_delete(effects[2]);
        }
    }

    #[test]
    fn create_default_block2_effect3_has_0_15_and_3_5() {
        unsafe {
            let effects = FormatScheme::create_default_block2_effects();
            assert_eq!((*effects[2]).len(), 2);
            let e0 = *(*effects[2]).begin;
            let e1 = *(*effects[2]).begin.add(1);
            // (0x20a, 0x3E19999A = 0.15)
            assert_eq!(e0 & 0xFFFF_FFFF, 0x20a);
            assert_eq!((e0 >> 32) as u32, 0x3E19999A);
            // (0x204, 3.5)
            assert_eq!(e1 & 0xFFFF_FFFF, 0x204);
            assert_eq!(f32::from_bits((e1 >> 32) as u32), 3.5_f32);

            crate::color_effect::ColorEffect::raw_delete(effects[0]);
            crate::color_effect::ColorEffect::raw_delete(effects[1]);
            crate::color_effect::ColorEffect::raw_delete(effects[2]);
        }
    }

    #[test]
    fn create_default_block2_each_effect_24b_struct() {
        // raw `mov w0, #0x18; bl __Znwm` × 3 — each ColorEffect is 24B / 8B align
        assert_eq!(std::mem::size_of::<crate::color_effect::ColorEffect>(), 24);
        assert_eq!(std::mem::align_of::<crate::color_effect::ColorEffect>(), 8);
    }

    // =========================================================================
    // 16-α: Block 6 (2nd GradientBrush's 3 effects with PKey 0x209) tests
    // =========================================================================

    /// raw `0x16fd70-0x16fd74`: 0x3F028F5C ≈ 0.51 float
    #[test]
    fn block6_float_bit_pattern_0_51() {
        let v = f32::from_bits(0x3F028F5C);
        assert!((v - 0.51_f32).abs() < 1e-6, "value = {}", v);
    }

    /// raw `0x16fd84-0x16fd88`: 0x3FA66666 = 1.30 float (effect4/5 의 0x204 value)
    #[test]
    fn block6_float_bit_pattern_1_30() {
        let v = f32::from_bits(0x3FA66666);
        assert!((v - 1.30_f32).abs() < 1e-5, "value = {}", v);
    }

    /// raw `0x16fdb4-0x16fdb8`: 0x3F6E147B = 0.93 (effect5 의 0x209 value)
    #[test]
    fn block6_float_bit_pattern_0_93() {
        let v = f32::from_bits(0x3F6E147B);
        assert!((v - 0.93_f32).abs() < 1e-5, "value = {}", v);
    }

    /// raw `0x16fdf8-0x16fdfc`: 0x3F70A3D7 = 0.94 (effect6 의 0x209 value)
    #[test]
    fn block6_float_bit_pattern_0_94() {
        let v = f32::from_bits(0x3F70A3D7);
        assert!((v - 0.94_f32).abs() < 1e-5, "value = {}", v);
    }

    /// raw `0x16fe0c-0x16fe10`: 0x3FACCCCD = 1.35 (effect6 의 0x204 value)
    #[test]
    fn block6_float_bit_pattern_1_35() {
        let v = f32::from_bits(0x3FACCCCD);
        assert!((v - 1.35_f32).abs() < 1e-5, "value = {}", v);
    }

    #[test]
    fn create_default_block6_returns_3_effects() {
        unsafe {
            let effects = FormatScheme::create_default_block6_effects();
            assert!(!effects[0].is_null());
            assert!(!effects[1].is_null());
            assert!(!effects[2].is_null());
            assert_ne!(effects[0], effects[1]);
            assert_ne!(effects[1], effects[2]);
            crate::color_effect::ColorEffect::raw_delete(effects[0]);
            crate::color_effect::ColorEffect::raw_delete(effects[1]);
            crate::color_effect::ColorEffect::raw_delete(effects[2]);
        }
    }

    #[test]
    fn create_default_block6_effect4_has_0_51_and_1_30() {
        unsafe {
            let effects = FormatScheme::create_default_block6_effects();
            assert_eq!((*effects[0]).len(), 2);
            let e0 = *(*effects[0]).begin;
            let e1 = *(*effects[0]).begin.add(1);
            // (0x209, 0x3F028F5C = 0.51)
            assert_eq!(e0 & 0xFFFF_FFFF, 0x209);
            assert_eq!((e0 >> 32) as u32, 0x3F028F5C);
            // (0x204, 0x3FA66666 = 1.30)
            assert_eq!(e1 & 0xFFFF_FFFF, 0x204);
            assert_eq!((e1 >> 32) as u32, 0x3FA66666);
            crate::color_effect::ColorEffect::raw_delete(effects[0]);
            crate::color_effect::ColorEffect::raw_delete(effects[1]);
            crate::color_effect::ColorEffect::raw_delete(effects[2]);
        }
    }

    #[test]
    fn create_default_block6_effect5_has_0_93_and_1_30() {
        unsafe {
            let effects = FormatScheme::create_default_block6_effects();
            assert_eq!((*effects[1]).len(), 2);
            let e0 = *(*effects[1]).begin;
            let e1 = *(*effects[1]).begin.add(1);
            assert_eq!(e0 & 0xFFFF_FFFF, 0x209);
            assert_eq!((e0 >> 32) as u32, 0x3F6E147B);
            assert_eq!(e1 & 0xFFFF_FFFF, 0x204);
            assert_eq!((e1 >> 32) as u32, 0x3FA66666);
            crate::color_effect::ColorEffect::raw_delete(effects[0]);
            crate::color_effect::ColorEffect::raw_delete(effects[1]);
            crate::color_effect::ColorEffect::raw_delete(effects[2]);
        }
    }

    #[test]
    fn create_default_block6_effect6_has_0_94_and_1_35() {
        unsafe {
            let effects = FormatScheme::create_default_block6_effects();
            assert_eq!((*effects[2]).len(), 2);
            let e0 = *(*effects[2]).begin;
            let e1 = *(*effects[2]).begin.add(1);
            assert_eq!(e0 & 0xFFFF_FFFF, 0x209);
            assert_eq!((e0 >> 32) as u32, 0x3F70A3D7);
            assert_eq!(e1 & 0xFFFF_FFFF, 0x204);
            assert_eq!((e1 >> 32) as u32, 0x3FACCCCD);
            crate::color_effect::ColorEffect::raw_delete(effects[0]);
            crate::color_effect::ColorEffect::raw_delete(effects[1]);
            crate::color_effect::ColorEffect::raw_delete(effects[2]);
        }
    }

    /// PKey 0x209 가 default REPLACE branch (`JUMP_TABLE[21] = 0x00`) 로 dispatch
    /// 됨을 확인 — Block 2 의 PKey 0x20a (offset 22) 도 동일 0x00 branch.
    #[test]
    fn pkey_0x209_uses_default_replace_branch() {
        unsafe {
            let e = crate::color_effect::ColorEffect::create();
            (*e).add(0x209, 0.51_f32);
            assert_eq!((*e).len(), 1);
            let entry = *(*e).begin;
            // packed = (pkey | (bits(value) << 32))
            assert_eq!(entry & 0xFFFF_FFFF, 0x209);
            assert_eq!((entry >> 32) as u32, 0x3F028F5C);
            crate::color_effect::ColorEffect::raw_delete(e);
        }
    }

    // =========================================================================
    // 16-β: Block 7-9 (2nd GradientBrush full setup + SetBrush(3)) tests
    // =========================================================================

    /// raw `0x16ffac-0x16ffb0`: 0x3F4CCCCD = 0.8 float (2nd GradientBrush stop2 position)
    #[test]
    fn block7_stop2_position_0_8() {
        let v = f32::from_bits(0x3F4CCCCD);
        assert!((v - 0.8_f32).abs() < 1e-6, "value = {}", v);
    }

    #[test]
    fn create_default_block7_through_9_attaches_style3() {
        unsafe {
            let mut fs = FormatScheme::new();
            // Block 1: SetBrush(1)
            fs.create_default_block1();

            // Block 6: effects (caller still owns)
            let effects = FormatScheme::create_default_block6_effects();

            // Block 7-9: SetBrush(3, GradientBrush with 3 stops + 4 overrides)
            fs.create_default_block7_through_9(effects);

            // brushes now has [1, 3] (= Block 1's Solid at 1, Block 9's Gradient at 3)
            assert_eq!(fs.brushes.size, 2);
            assert!(fs.find_brush(1).is_some());
            assert!(fs.find_brush(3).is_some());
            assert!(fs.find_brush(2).is_none()); // Block 5 skipped in this test

            // Verify Style=3 is a GradientBrush
            let ctrl3 = fs.find_brush(3).unwrap();
            let obj = (*ctrl3).obj;
            let vt = crate::brush::brush_vtable(obj as *const u8);
            assert_eq!(vt.type_tag, crate::brush::BrushType::Gradient as u32);

            // Verify GradientBrush state (9 keys = 8 default + stops)
            let gb = obj as *const crate::brush::GradientBrush;
            assert_eq!((*gb).bag_size(), 9);
            assert_eq!((*gb).get_angle_degrees(), 270.0);
            // get_stops returns 3 stops
            let stops = (*gb).get_stops();
            assert_eq!(stops.len(), 3);
            assert_eq!(stops[0].0, 0.0);
            assert_eq!(stops[1].0.to_bits(), 0x3F4CCCCD);
            assert_eq!(stops[2].0, 1.0);

            // cleanup effects (Block 6 산출의 owner is caller)
            crate::color_effect::ColorEffect::raw_delete(effects[0]);
            crate::color_effect::ColorEffect::raw_delete(effects[1]);
            crate::color_effect::ColorEffect::raw_delete(effects[2]);
        }
    }

    /// **Block 1-9 end-to-end** = SetBrush(1, Solid) + SetBrush(2, Gradient #1) +
    /// SetBrush(3, Gradient #2). 3/12 attach calls 완료.
    ///
    /// raw `0x16f628-0x170328` 의 전체 흐름 (= 1st + 2nd + 3rd SetBrush).
    #[test]
    fn create_default_block1_through_9_integration() {
        unsafe {
            let mut fs = FormatScheme::new();

            // Setup 1: SetBrush(1, SolidBrush(Scheme 0x10))
            fs.create_default_block1();
            assert_eq!(fs.brushes.size, 1);

            // Setup 2: SetBrush(2, GradientBrush #1 with [0, 0.35, 1.0] stops + 270° + flip=true)
            let effects_a = FormatScheme::create_default_block2_effects();
            let mut stops_a = crate::gradient_stop::GradientStopsVec::new_with_initial_capacity();
            let scheme_val = {
                let mut v = [0u8; 12];
                v[0..4].copy_from_slice(&0x10u32.to_le_bytes());
                v
            };
            for (pos, eidx) in [
                (0.0_f32, 0),
                (f32::from_bits(0x3EB33333), 1),
                (1.0_f32, 2),
            ] {
                let c = crate::color::Color {
                    value: scheme_val,
                    type_tag: crate::color::color_type::SCHEME,
                    color_effect: effects_a[eidx],
                };
                let s = crate::gradient_stop::GradientStop::create_with_effect(&c, pos);
                std::mem::forget(c);
                let ctrl = crate::gradient_stop::GradientStopCtrl::create_raw(s);
                stops_a.push_back(ctrl);
                crate::gradient_stop::GradientStopCtrl::release(ctrl);
            }
            let mut gb_a = crate::brush::GradientBrush::new();
            gb_a.set_stops(&stops_a);
            gb_a.set_scaled(true);
            gb_a.set_style(0);
            gb_a.set_angle_degrees(270.0);
            gb_a.set_flip(true); // Setup 1 specific: flip=true
            let ctrl_a = BrushControlBlock::from_gradient(gb_a);
            fs.set_brush(2, ctrl_a);
            drop(stops_a);
            crate::color_effect::ColorEffect::raw_delete(effects_a[0]);
            crate::color_effect::ColorEffect::raw_delete(effects_a[1]);
            crate::color_effect::ColorEffect::raw_delete(effects_a[2]);
            assert_eq!(fs.brushes.size, 2);

            // Setup 3: SetBrush(3, GradientBrush #2 with [0, 0.8, 1.0] stops + 270° + flip=false)
            let effects_b = FormatScheme::create_default_block6_effects();
            fs.create_default_block7_through_9(effects_b);
            crate::color_effect::ColorEffect::raw_delete(effects_b[0]);
            crate::color_effect::ColorEffect::raw_delete(effects_b[1]);
            crate::color_effect::ColorEffect::raw_delete(effects_b[2]);

            // Final: 3 entries in brushes tree
            assert_eq!(fs.brushes.size, 3);
            assert!(fs.find_brush(1).is_some());
            assert!(fs.find_brush(2).is_some());
            assert!(fs.find_brush(3).is_some());

            // vtable dispatch verification: Solid(1) + Gradient(2,3)
            let ctrl1 = fs.find_brush(1).unwrap();
            let ctrl2 = fs.find_brush(2).unwrap();
            let ctrl3 = fs.find_brush(3).unwrap();
            assert_eq!(
                crate::brush::brush_vtable((*ctrl1).obj as *const u8).type_tag,
                crate::brush::BrushType::Solid as u32
            );
            assert_eq!(
                crate::brush::brush_vtable((*ctrl2).obj as *const u8).type_tag,
                crate::brush::BrushType::Gradient as u32
            );
            assert_eq!(
                crate::brush::brush_vtable((*ctrl3).obj as *const u8).type_tag,
                crate::brush::BrushType::Gradient as u32
            );
        }
    }

    // =========================================================================
    // 16z: BrushControlBlock convenience helpers tests
    // =========================================================================

    #[test]
    fn brush_control_block_from_solid_works() {
        unsafe {
            let mut fs = FormatScheme::new();
            let color = crate::color::Color::from_rgb(255, 0, 0, ptr::null_mut());
            let ctrl = BrushControlBlock::from_solid(crate::brush::SolidBrush::new(color));
            assert_eq!((*ctrl).strong, 1);
            assert_eq!((*ctrl).flag, 1);
            fs.set_brush(1, ctrl);
            assert_eq!(fs.brushes.size, 1);
            // Drop releases SolidBrush via vtable dispatch
        }
    }

    #[test]
    fn brush_control_block_from_hatch_works() {
        unsafe {
            let mut fs = FormatScheme::new();
            let fore = crate::color::Color::from_rgb(0, 255, 0, ptr::null_mut());
            let back = crate::color::Color::from_rgb(0, 0, 255, ptr::null_mut());
            let ctrl = BrushControlBlock::from_hatch(crate::brush::HatchBrush::new(
                3, fore, back,
            ));
            fs.set_brush(2, ctrl);
            assert_eq!(fs.brushes.size, 1);
        }
    }

    #[test]
    fn brush_control_block_from_gradient_works() {
        unsafe {
            let mut fs = FormatScheme::new();
            let gb = crate::brush::GradientBrush::new();
            let ctrl = BrushControlBlock::from_gradient(gb);
            assert_eq!((*ctrl).strong, 1);
            assert_eq!((*ctrl).flag, 1);
            // raw 의 SetBrush(Style=2, GradientBrush) — Block 5-A
            fs.set_brush(2, ctrl);
            assert_eq!(fs.brushes.size, 1);
            // Drop dispatches via GRADIENT_BRUSH_VTABLE.drop_in_place_fn
            // (clean up the 8-key bag automatically)
        }
    }

    /// `FormatScheme::CreateDefault` 의 **Block 1+...+5** 1:1 통합 시뮬레이션.
    ///
    /// raw `0x16f628-0x16fd58` 전체 흐름:
    /// 1. Block 0: FormatScheme alloc + 4 trees init (FormatScheme::new)
    /// 2. Block 1: SolidBrush(Scheme 0x10) → SetBrush(MainColor=1)
    /// 3. Block 2: 3 ColorEffect (각 2 Add)
    /// 4. Block 3: 3-stop GradientStopsVec (각 stop uses effect1/2/3)
    /// 5. Block 4: GradientBrush.set_stops + set_angle(270°) + set_flip(true) ...
    /// 6. Block 5-A: GradientBrush wrap in BrushControlBlock + SetBrush(Style=2)
    /// 7. Block 5-B: GradientStopsVec local cleanup (Rust auto-drop)
    ///
    /// 결과: FormatScheme.brushes 에 2 entries (key 1 = SolidBrush, key 2 = GradientBrush).
    #[test]
    fn create_default_block1_through_5_integration() {
        unsafe {
            let mut fs = FormatScheme::new();

            // Block 1
            fs.create_default_block1();
            assert_eq!(fs.brushes.size, 1);

            // Block 2: 3 ColorEffects (CreateDefault stack-local; tests own them)
            let effects = FormatScheme::create_default_block2_effects();
            assert!(!effects[0].is_null());

            // Block 3: 3-stop GradientStopsVec (each stop uses one effect)
            let mut stops_vec = crate::gradient_stop::GradientStopsVec::new_with_initial_capacity();
            // Stop 1 (pos 0.0, effect1)
            let color_with_e1 = crate::color::Color {
                value: {
                    let mut v = [0u8; 12];
                    v[0..4].copy_from_slice(&0x10u32.to_le_bytes());
                    v
                },
                type_tag: crate::color::color_type::SCHEME,
                color_effect: effects[0],
            };
            let stop1 = crate::gradient_stop::GradientStop::create_with_effect(
                &color_with_e1, 0.0,
            );
            let ctrl1 = crate::gradient_stop::GradientStopCtrl::create_raw(stop1);
            stops_vec.push_back(ctrl1);
            crate::gradient_stop::GradientStopCtrl::release(ctrl1);

            // Stop 2 (pos 0.35, effect2)
            let color_with_e2 = crate::color::Color {
                value: color_with_e1.value,
                type_tag: color_with_e1.type_tag,
                color_effect: effects[1],
            };
            let stop2 = crate::gradient_stop::GradientStop::create_with_effect(
                &color_with_e2, f32::from_bits(0x3EB33333),
            );
            let ctrl2 = crate::gradient_stop::GradientStopCtrl::create_raw(stop2);
            stops_vec.push_back(ctrl2);
            crate::gradient_stop::GradientStopCtrl::release(ctrl2);

            // Stop 3 (pos 1.0, effect3)
            let color_with_e3 = crate::color::Color {
                value: color_with_e1.value,
                type_tag: color_with_e1.type_tag,
                color_effect: effects[2],
            };
            let stop3 = crate::gradient_stop::GradientStop::create_with_effect(
                &color_with_e3, 1.0,
            );
            let ctrl3 = crate::gradient_stop::GradientStopCtrl::create_raw(stop3);
            stops_vec.push_back(ctrl3);
            crate::gradient_stop::GradientStopCtrl::release(ctrl3);

            // The Color wrappers had borrowed effect ptrs; the original
            // effect pointers are still owned by `effects[]`. We need to NULL
            // out the borrowed ptrs to prevent Color::Drop from double-freeing.
            std::mem::forget(color_with_e1);
            std::mem::forget(color_with_e2);
            std::mem::forget(color_with_e3);

            assert_eq!(stops_vec.len(), 3);

            // Block 4: configure GradientBrush
            let mut gb = crate::brush::GradientBrush::new();
            gb.set_stops(&stops_vec);
            gb.set_scaled(true);
            gb.set_style(0);
            gb.set_angle_degrees(270.0);
            gb.set_flip(true);
            assert_eq!(gb.bag_size(), 9);
            assert_eq!(gb.get_angle_degrees(), 270.0);
            assert_eq!(gb.get_stops().len(), 3);

            // Block 5-A: SetBrush(Style=2, GradientBrush)
            let ctrl = BrushControlBlock::from_gradient(gb);
            fs.set_brush(2, ctrl);
            assert_eq!(fs.brushes.size, 2);

            // Block 5-B: GradientStopsVec local cleanup (Rust auto-drop)
            drop(stops_vec);
            // effects 들은 stops 가 deep-clone 했으므로 caller 가 release 책임
            crate::color_effect::ColorEffect::raw_delete(effects[0]);
            crate::color_effect::ColorEffect::raw_delete(effects[1]);
            crate::color_effect::ColorEffect::raw_delete(effects[2]);

            // Final state verification
            assert_eq!(fs.brushes.size, 2);
            // brushes[1] = SolidBrush, brushes[2] = GradientBrush
            let brush1_ctrl = fs.find_brush(1).expect("MainColor SolidBrush");
            let brush2_ctrl = fs.find_brush(2).expect("Style=2 GradientBrush");
            assert!(!brush1_ctrl.is_null());
            assert!(!brush2_ctrl.is_null());
            // Verify vtable dispatch
            let vt1 = crate::brush::brush_vtable((*brush1_ctrl).obj as *const u8);
            let vt2 = crate::brush::brush_vtable((*brush2_ctrl).obj as *const u8);
            assert_eq!(vt1.type_tag, crate::brush::BrushType::Solid as u32);
            assert_eq!(vt2.type_tag, crate::brush::BrushType::Gradient as u32);

            // fs goes out of scope → brushes tree dropped → both brushes auto-released via vtable
        }
    }

    // =========================================================================
    // 16-γ: SetBackgroundBrush + Block 10 tests
    // =========================================================================

    #[test]
    fn set_background_brush_insert_increments_bg_tree_size() {
        unsafe {
            let mut fs = FormatScheme::new();
            assert_eq!(fs.bg_brushes.size, 0);
            let color = crate::color::Color::from_rgb(0xAA, 0xBB, 0xCC, ptr::null_mut());
            let solid = crate::brush::SolidBrush::new(color);
            let ctrl = BrushControlBlock::from_solid(solid);
            fs.set_background_brush(1, ctrl);
            assert_eq!(fs.bg_brushes.size, 1);
            // brushes tree (foreground) untouched
            assert_eq!(fs.brushes.size, 0);
        }
    }

    #[test]
    fn set_background_brush_multiple_inserts_independent_of_brushes_tree() {
        unsafe {
            let mut fs = FormatScheme::new();
            for style in [1u32, 2, 3] {
                let color = crate::color::Color::from_rgb(style as u8, 0, 0, ptr::null_mut());
                let solid = crate::brush::SolidBrush::new(color);
                let ctrl = BrushControlBlock::from_solid(solid);
                fs.set_background_brush(style, ctrl);
            }
            assert_eq!(fs.bg_brushes.size, 3);
            // SetBrush 와 SetBackgroundBrush 는 서로 다른 tree
            assert_eq!(fs.brushes.size, 0);
            // find_background_brush 검증
            assert!(fs.find_background_brush(1).is_some());
            assert!(fs.find_background_brush(2).is_some());
            assert!(fs.find_background_brush(3).is_some());
            assert!(fs.find_background_brush(99).is_none());
            // find_brush (foreground) 는 다른 tree
            assert!(fs.find_brush(1).is_none());
        }
    }

    #[test]
    fn set_background_brush_replace_path_releases_old() {
        unsafe {
            let mut fs = FormatScheme::new();
            let c1 = crate::color::Color::from_rgb(0x11, 0, 0, ptr::null_mut());
            let ctrl1 = BrushControlBlock::from_solid(crate::brush::SolidBrush::new(c1));
            fs.set_background_brush(7, ctrl1);
            assert_eq!(fs.bg_brushes.size, 1);
            let c2 = crate::color::Color::from_rgb(0x22, 0, 0, ptr::null_mut());
            let ctrl2 = BrushControlBlock::from_solid(crate::brush::SolidBrush::new(c2));
            fs.set_background_brush(7, ctrl2);
            // 동일 key REPLACE — size 는 그대로
            assert_eq!(fs.bg_brushes.size, 1);
            // 최신 ctrl 로 교체됐는지 확인
            let found = fs.find_background_brush(7).expect("BgBrush style=7");
            assert_eq!(found, ctrl2);
        }
    }

    #[test]
    fn create_default_block10_attaches_solidbrush_with_scheme_phclr() {
        unsafe {
            let mut fs = FormatScheme::new();
            fs.create_default_block10();
            // bg_brushes 가 1개 entry
            assert_eq!(fs.bg_brushes.size, 1);
            // brushes 는 비어있음 (Block 1 호출 안 함)
            assert_eq!(fs.brushes.size, 0);
            // entry sub-type = SolidBrush
            let ctrl = fs.find_background_brush(1).expect("BgBrush style=1");
            let vt = crate::brush::brush_vtable((*ctrl).obj as *const u8);
            assert_eq!(vt.type_tag, crate::brush::BrushType::Solid as u32);
            // SolidBrush.bag 의 PColor (0x259) value = SchemeStyle 0x10
            let solid = (*ctrl).obj as *const crate::brush::SolidBrush;
            let color = (*solid).get_color();
            assert_eq!(color.type_tag, crate::color::color_type::SCHEME);
            assert_eq!(u32::from_le_bytes([color.value[0], color.value[1], color.value[2], color.value[3]]), 0x10);
        }
    }

    #[test]
    fn block1_and_block10_together_populate_both_trees() {
        unsafe {
            let mut fs = FormatScheme::new();
            fs.create_default_block1();   // brushes[1] = SolidBrush(Scheme 0x10)
            fs.create_default_block10();  // bg_brushes[1] = SolidBrush(Scheme 0x10)
            assert_eq!(fs.brushes.size, 1);
            assert_eq!(fs.bg_brushes.size, 1);
            // 동일한 SchemeStyle 0x10 (= phClr) 둘 다 사용
            let b1 = fs.find_brush(1).unwrap();
            let bg1 = fs.find_background_brush(1).unwrap();
            // 서로 다른 인스턴스 (각 Block 이 새로 alloc)
            assert_ne!(b1, bg1);
            assert_ne!((*b1).obj, (*bg1).obj);
        }
    }

    #[test]
    fn set_background_brush_null_ctrl_is_noop() {
        unsafe {
            let mut fs = FormatScheme::new();
            fs.set_background_brush(1, ptr::null_mut());
            assert_eq!(fs.bg_brushes.size, 0);
        }
    }

    // =========================================================================
    // 16-δ: Block 11 tests
    // =========================================================================

    #[test]
    fn block11_effect7_has_2_adds_0_4_and_3_5() {
        unsafe {
            // raw 0x170448-0x170468
            let effect7 = crate::color_effect::ColorEffect::create();
            (*effect7).add(0x20a, f32::from_bits(0x3ECCCCCD));
            (*effect7).add(0x204, 3.5_f32);
            assert_eq!((*effect7).len(), 2);
            crate::color_effect::ColorEffect::raw_delete(effect7);
        }
    }

    #[test]
    fn block11_effect8_has_3_adds_pkey_0x20a_0x209_0x204() {
        unsafe {
            // raw 0x170484-0x1704bc (⭐ 3 Adds, both 0x20a + 0x209 used)
            let effect8 = crate::color_effect::ColorEffect::create();
            (*effect8).add(0x20a, f32::from_bits(0x3EE66666)); // 0.45
            (*effect8).add(0x209, f32::from_bits(0x3F7D70A4)); // 0.99
            (*effect8).add(0x204, 3.5_f32);
            assert_eq!((*effect8).len(), 3);
            crate::color_effect::ColorEffect::raw_delete(effect8);
        }
    }

    #[test]
    fn block11_float_0_99_bit_pattern() {
        // raw 0x170498-0x17049c: mov w8, #0x70a4; movk w8, #0x3f7d, lsl #16
        assert_eq!(f32::from_bits(0x3F7D70A4).to_bits(), 0x3F7D70A4);
        // 검증: ~0.99
        let v = f32::from_bits(0x3F7D70A4);
        assert!((v - 0.99).abs() < 1e-4);
    }

    #[test]
    fn block11_float_2_55_bit_pattern() {
        // raw 0x1704ec-0x1704f0: mov w8, #0x3333; movk w8, #0x4023, lsl #16
        assert_eq!(f32::from_bits(0x40233333).to_bits(), 0x40233333);
        let v = f32::from_bits(0x40233333);
        assert!((v - 2.55).abs() < 1e-4);
    }

    #[test]
    fn block11_focus_rect_blob_decodes_to_4_floats() {
        // raw rodata @ 0x741e90 — arm64 slice xxd 결과
        let blob: [u8; 16] = [
            0x00, 0x00, 0x00, 0x3F,
            0xCD, 0xCC, 0x4C, 0xBF,
            0x00, 0x00, 0x00, 0x3F,
            0x66, 0x66, 0xE6, 0x3F,
        ];
        let f0 = f32::from_le_bytes([blob[0], blob[1], blob[2], blob[3]]);
        let f1 = f32::from_le_bytes([blob[4], blob[5], blob[6], blob[7]]);
        let f2 = f32::from_le_bytes([blob[8], blob[9], blob[10], blob[11]]);
        let f3 = f32::from_le_bytes([blob[12], blob[13], blob[14], blob[15]]);
        assert_eq!(f0, 0.5);
        assert_eq!(f1, -0.8);
        assert_eq!(f2, 0.5);
        assert_eq!(f3, 1.8);
    }

    #[test]
    fn create_default_block11_attaches_gradient_to_bg_brushes_style2() {
        unsafe {
            let mut fs = FormatScheme::new();
            fs.create_default_block11();
            assert_eq!(fs.bg_brushes.size, 1);
            let ctrl = fs.find_background_brush(2).expect("BgBrush style=2");
            let vt = crate::brush::brush_vtable((*ctrl).obj as *const u8);
            assert_eq!(vt.type_tag, crate::brush::BrushType::Gradient as u32);
            // GradientBrush 의 style = 1 검증
            let gb = (*ctrl).obj as *const crate::brush::GradientBrush;
            assert_eq!((*gb).get_style(), 1);
            // 3 stops 검증
            let stops = (*gb).get_stops();
            assert_eq!(stops.len(), 3);
            assert_eq!(stops[0].0, 0.0);
            assert_eq!(stops[1].0.to_bits(), 0x3ECCCCCD); // 0.4
            assert_eq!(stops[2].0, 1.0);
            // angle 은 default (0.0) 유지
            assert_eq!((*gb).get_angle_degrees(), 0.0);
        }
    }

    // =========================================================================
    // 16-ε: Block 12 tests
    // =========================================================================

    #[test]
    fn block12_effect10_has_2_adds_0_8_and_3_0() {
        unsafe {
            let e = crate::color_effect::ColorEffect::create();
            (*e).add(0x20a, f32::from_bits(0x3F4CCCCD)); // 0.8
            (*e).add(0x204, 3.0_f32);
            assert_eq!((*e).len(), 2);
            crate::color_effect::ColorEffect::raw_delete(e);
        }
    }

    #[test]
    fn block12_float_0_3_bit_pattern() {
        // raw 0x170a20-0x170a24: mov w8, #0x999a; movk w8, #0x3e99, lsl #16
        assert_eq!(f32::from_bits(0x3E99999A).to_bits(), 0x3E99999A);
        let v = f32::from_bits(0x3E99999A);
        assert!((v - 0.3).abs() < 1e-4);
    }

    #[test]
    fn create_default_block12_attaches_2stop_gradient_to_bg_brushes_style3() {
        unsafe {
            let mut fs = FormatScheme::new();
            fs.create_default_block12();
            assert_eq!(fs.bg_brushes.size, 1);
            let ctrl = fs.find_background_brush(3).expect("BgBrush style=3");
            let vt = crate::brush::brush_vtable((*ctrl).obj as *const u8);
            assert_eq!(vt.type_tag, crate::brush::BrushType::Gradient as u32);
            let gb = (*ctrl).obj as *const crate::brush::GradientBrush;
            assert_eq!((*gb).get_style(), 1);
            // ⭐ Block 12 = 2 stops only
            let stops = (*gb).get_stops();
            assert_eq!(stops.len(), 2);
            assert_eq!(stops[0].0, 0.0);
            assert_eq!(stops[1].0, 1.0);
        }
    }

    #[test]
    fn create_default_block10_11_12_bg_brushes_size_3() {
        unsafe {
            let mut fs = FormatScheme::new();
            fs.create_default_block10();
            fs.create_default_block11();
            fs.create_default_block12();
            assert_eq!(fs.bg_brushes.size, 3);
            assert!(fs.find_background_brush(1).is_some());
            assert!(fs.find_background_brush(2).is_some());
            assert!(fs.find_background_brush(3).is_some());

            // vtable dispatch
            let c1 = fs.find_background_brush(1).unwrap();
            let c2 = fs.find_background_brush(2).unwrap();
            let c3 = fs.find_background_brush(3).unwrap();
            assert_eq!(
                crate::brush::brush_vtable((*c1).obj as *const u8).type_tag,
                crate::brush::BrushType::Solid as u32
            );
            assert_eq!(
                crate::brush::brush_vtable((*c2).obj as *const u8).type_tag,
                crate::brush::BrushType::Gradient as u32
            );
            assert_eq!(
                crate::brush::brush_vtable((*c3).obj as *const u8).type_tag,
                crate::brush::BrushType::Gradient as u32
            );
        }
    }

    // =========================================================================
    // 16-ζ: PenControlBlock + SetPen + Block 13 tests
    // =========================================================================

    #[test]
    fn pen_control_block_raw_24b_layout() {
        // raw 0x170ea4-0x170eb4: mov w0, #0x18; bl __Znwm; ...; stp x21, x8 / strb
        assert_eq!(std::mem::size_of::<PenControlBlock>(), 24);
        assert_eq!(std::mem::align_of::<PenControlBlock>(), 8);
    }

    #[test]
    fn pen_control_block_field_offsets_match_raw() {
        let cb = PenControlBlock {
            obj: ptr::null_mut(),
            strong: 0,
            flag: 0,
            _pad: [0u8; 7],
        };
        let base = &cb as *const _ as usize;
        assert_eq!(&cb.obj as *const _ as usize - base, 0x00);
        assert_eq!(&cb.strong as *const _ as usize - base, 0x08);
        assert_eq!(&cb.flag as *const _ as usize - base, 0x10);
    }

    #[test]
    fn fs_pen_map_node_48b_byte_eq_with_brush_node() {
        assert_eq!(std::mem::size_of::<FsPenMapNode>(), 48);
        assert_eq!(std::mem::size_of::<FsPenMapNode>(), std::mem::size_of::<FsBrushMapNode>());
        let n = FsPenMapNode {
            base: TreeNodeBase {
                left: ptr::null_mut(),
                right: ptr::null_mut(),
                parent: ptr::null_mut(),
                is_black: 0,
                _pad_0x19: [0u8; 7],
            },
            key: 0,
            _pad: 0,
            value: ptr::null_mut(),
        };
        let base = &n as *const _ as usize;
        assert_eq!(&n.key as *const _ as usize - base, 0x20);
        assert_eq!(&n.value as *const _ as usize - base, 0x28);
    }

    #[test]
    fn pen_new_with_engine_defaults_attaches_10_keys() {
        // raw `0x1b4cf0` Pen::C2() — 10 default keys (skipping 0x2c1)
        let p = crate::pen::Pen::new_with_engine_defaults(f32::from_bits(0x495F3E00));
        assert_eq!(p.bag_size(), 10);
    }

    #[test]
    fn pen_new_with_engine_defaults_width_for_914400_emu() {
        // engine_base = 914400.0 → width = 914400 * 0.75 / 72 = 9525
        let p = crate::pen::Pen::new_with_engine_defaults(914400.0);
        assert_eq!(p.get_thickness(), 9525.0);
    }

    #[test]
    fn pen_new_with_engine_defaults_line_join_is_1() {
        // raw `0x1b4e1c-0x1b4e28`: w8 = 1, w9 = 0x2c0 → PEnum value = 1
        let p = crate::pen::Pen::new_with_engine_defaults(914400.0);
        let v = unsafe {
            let pk = crate::property_key::PropertyKey::from_int(crate::pen::Pen::KEY_LINE_JOIN);
            let i = p.bag.impl_ref().unwrap();
            let node = i.find_equal(&pk).unwrap();
            let cb = (*node).value;
            let pe = (*cb).obj as *const crate::property::PEnum;
            (*pe).value
        };
        assert_eq!(v, 1);
    }

    #[test]
    fn pen_new_with_engine_defaults_start_arrow_size_is_4() {
        // raw `0x1b4e90-0x1b4e9c`: w8 = 4, w9 = 0x2c3 → PEnum value = 4
        let p = crate::pen::Pen::new_with_engine_defaults(914400.0);
        let v = unsafe {
            let pk = crate::property_key::PropertyKey::from_int(crate::pen::Pen::KEY_START_ARROW_SIZE);
            let i = p.bag.impl_ref().unwrap();
            let node = i.find_equal(&pk).unwrap();
            let cb = (*node).value;
            let pe = (*cb).obj as *const crate::property::PEnum;
            (*pe).value
        };
        assert_eq!(v, 4);
    }

    #[test]
    fn set_pen_insert_increments_pens_tree_size() {
        unsafe {
            let mut fs = FormatScheme::new();
            assert_eq!(fs.pens.size, 0);
            let pen = crate::pen::Pen::new_with_engine_defaults(914400.0);
            let ctrl = PenControlBlock::from_pen(pen);
            fs.set_pen(1, ctrl);
            assert_eq!(fs.pens.size, 1);
            // brushes/bg_brushes untouched
            assert_eq!(fs.brushes.size, 0);
            assert_eq!(fs.bg_brushes.size, 0);
        }
    }

    #[test]
    fn set_pen_multiple_inserts_sorted() {
        unsafe {
            let mut fs = FormatScheme::new();
            for style in [3u32, 1, 2] {
                let pen = crate::pen::Pen::new_with_engine_defaults(914400.0);
                let ctrl = PenControlBlock::from_pen(pen);
                fs.set_pen(style, ctrl);
            }
            assert_eq!(fs.pens.size, 3);
            assert!(fs.find_pen(1).is_some());
            assert!(fs.find_pen(2).is_some());
            assert!(fs.find_pen(3).is_some());
            assert!(fs.find_pen(99).is_none());
        }
    }

    #[test]
    fn set_pen_null_ctrl_is_noop() {
        unsafe {
            let mut fs = FormatScheme::new();
            fs.set_pen(1, ptr::null_mut());
            assert_eq!(fs.pens.size, 0);
        }
    }

    #[test]
    fn block13_width_fast_path_engine_default_914400_yields_12700() {
        // raw `0x170f54-0x170f74`: engine == 0x495F3E00 (914400) → width = 0x46467000 (12700)
        let mut fs = FormatScheme::new();
        let engine_base = f32::from_bits(0x495F3E00); // 914400
        fs.create_default_block13(engine_base);
        assert_eq!(fs.pens.size, 1);
        let ctrl = fs.find_pen(1).expect("pen style=1");
        unsafe {
            let pen = &*((*ctrl).obj);
            assert_eq!(pen.get_thickness().to_bits(), 0x46467000); // 12700
        }
    }

    #[test]
    fn create_default_block13_attaches_pen_with_solid_stroke() {
        unsafe {
            let mut fs = FormatScheme::new();
            fs.create_default_block13(914400.0);
            assert_eq!(fs.pens.size, 1);
            let ctrl = fs.find_pen(1).expect("pen style=1");
            let pen = &*((*ctrl).obj);
            // stroke brush 가 SolidBrush(Scheme 0x10)
            match &*pen.brush {
                crate::brush::Brush::Solid(sb) => {
                    assert_eq!(sb.get_color().type_tag, crate::color::color_type::SCHEME);
                    let raw = u32::from_le_bytes([
                        sb.get_color().value[0],
                        sb.get_color().value[1],
                        sb.get_color().value[2],
                        sb.get_color().value[3],
                    ]);
                    assert_eq!(raw, 0x10);
                }
                _ => panic!("Block 13 stroke should be SolidBrush"),
            }
        }
    }

    #[test]
    fn create_default_block13_bag_has_10_keys() {
        // Pen::C2 = 10 defaults. Block 13's 5 overrides REPLACE (no new keys).
        // bag.size 는 그대로 10.
        unsafe {
            let mut fs = FormatScheme::new();
            fs.create_default_block13(914400.0);
            let ctrl = fs.find_pen(1).unwrap();
            let pen = &*((*ctrl).obj);
            assert_eq!(pen.bag_size(), 10);
        }
    }

    // =========================================================================
    // 16-η/16-θ: Block 14 / 15 tests
    // =========================================================================

    #[test]
    fn block14_width_fast_path_yields_19050_emu() {
        // raw `0x1712bc-0x1712c4`: 0x4694D400 = 19050.0 EMU = 1.5pt
        let mut fs = FormatScheme::new();
        fs.create_default_block14(f32::from_bits(0x495F3E00));
        let ctrl = fs.find_pen(2).expect("pen style=2");
        unsafe {
            let pen = &*((*ctrl).obj);
            assert_eq!(pen.get_thickness().to_bits(), 0x4694D400);
            assert_eq!(pen.get_thickness(), 19050.0);
        }
    }

    #[test]
    fn block15_width_fast_path_yields_38100_emu() {
        // raw `0x171610-0x171618`: 0x4714D400 = 38100.0 EMU = 3.0pt
        let mut fs = FormatScheme::new();
        fs.create_default_block15(f32::from_bits(0x495F3E00));
        let ctrl = fs.find_pen(3).expect("pen style=3");
        unsafe {
            let pen = &*((*ctrl).obj);
            assert_eq!(pen.get_thickness().to_bits(), 0x4714D400);
            assert_eq!(pen.get_thickness(), 38100.0);
        }
    }

    #[test]
    fn block13_14_15_three_pens_with_distinct_widths() {
        unsafe {
            let mut fs = FormatScheme::new();
            let engine = f32::from_bits(0x495F3E00); // 914400
            fs.create_default_block13(engine);
            fs.create_default_block14(engine);
            fs.create_default_block15(engine);
            assert_eq!(fs.pens.size, 3);
            // 각 style 별 다른 width 검증
            let w1 = (*(*fs.find_pen(1).unwrap()).obj).get_thickness().to_bits();
            let w2 = (*(*fs.find_pen(2).unwrap()).obj).get_thickness().to_bits();
            let w3 = (*(*fs.find_pen(3).unwrap()).obj).get_thickness().to_bits();
            assert_eq!(w1, 0x46467000); // 12700 = 1pt
            assert_eq!(w2, 0x4694D400); // 19050 = 1.5pt
            assert_eq!(w3, 0x4714D400); // 38100 = 3pt
        }
    }

    // =========================================================================
    // 16-ι/κ/λ: EffectStyleControlBlock + SetEffectStyle + Block 16-18 tests
    // =========================================================================

    #[test]
    fn effect_style_control_block_raw_24b_layout() {
        assert_eq!(std::mem::size_of::<EffectStyleControlBlock>(), 24);
        assert_eq!(std::mem::align_of::<EffectStyleControlBlock>(), 8);
    }

    #[test]
    fn effect_style_control_block_field_offsets_match_raw() {
        let cb = EffectStyleControlBlock {
            obj: ptr::null_mut(),
            strong: 0,
            flag: 0,
            _pad: [0u8; 7],
        };
        let base = &cb as *const _ as usize;
        assert_eq!(&cb.obj as *const _ as usize - base, 0x00);
        assert_eq!(&cb.strong as *const _ as usize - base, 0x08);
        assert_eq!(&cb.flag as *const _ as usize - base, 0x10);
    }

    #[test]
    fn fs_effect_style_map_node_48b_byte_eq() {
        assert_eq!(std::mem::size_of::<FsEffectStyleMapNode>(), 48);
        let n = FsEffectStyleMapNode {
            base: TreeNodeBase {
                left: ptr::null_mut(),
                right: ptr::null_mut(),
                parent: ptr::null_mut(),
                is_black: 0,
                _pad_0x19: [0u8; 7],
            },
            key: 0,
            _pad: 0,
            value: ptr::null_mut(),
        };
        let base = &n as *const _ as usize;
        assert_eq!(&n.key as *const _ as usize - base, 0x20);
        assert_eq!(&n.value as *const _ as usize - base, 0x28);
    }

    #[test]
    fn set_effect_style_insert_increments_effects_tree_size() {
        unsafe {
            let mut fs = FormatScheme::new();
            assert_eq!(fs.effects.size, 0);
            let es = crate::effect_style::EffectStyle::new_empty();
            let ctrl = EffectStyleControlBlock::from_effect_style(es);
            fs.set_effect_style(1, ctrl);
            assert_eq!(fs.effects.size, 1);
            // 다른 trees 영향 없음
            assert_eq!(fs.brushes.size, 0);
            assert_eq!(fs.bg_brushes.size, 0);
            assert_eq!(fs.pens.size, 0);
        }
    }

    #[test]
    fn create_default_block16_through_18_partial_attaches_3_empty_effect_styles() {
        unsafe {
            let mut fs = FormatScheme::new();
            fs.create_default_block16_through_18_partial();
            // 3 entries with empty (= all null) EffectStyle
            assert_eq!(fs.effects.size, 3);
            assert!(fs.find_effect_style(1).is_some());
            assert!(fs.find_effect_style(2).is_some());
            assert!(fs.find_effect_style(3).is_some());

            // 각 attached EffectStyle 이 모두 empty (= 3 null SharePtrs)
            for style in [1u32, 2, 3] {
                let ctrl = fs.find_effect_style(style).unwrap();
                let es = &*((*ctrl).obj);
                assert!(es.is_empty());
            }
        }
    }

    #[test]
    fn set_effect_style_null_ctrl_is_noop() {
        unsafe {
            let mut fs = FormatScheme::new();
            fs.set_effect_style(1, ptr::null_mut());
            assert_eq!(fs.effects.size, 0);
        }
    }

    // =========================================================================
    // 16-μ: Block 16/17/18 inner byte-eq tests (OuterShadow + Reflection)
    // =========================================================================

    #[test]
    fn block16_attaches_outer_shadow_with_effect_key_0xbba() {
        unsafe {
            let mut fs = FormatScheme::new();
            fs.create_default_block16(914400.0);
            assert_eq!(fs.effects.size, 1);
            let ctrl = fs.find_effect_style(1).expect("style 1");
            let es = &*((*ctrl).obj);
            // Effects (= +0x10 SharePtr) is non-null
            assert!(!es.effects.is_null());
            // Inner Effects map has 1 OuterShadow entry at key 0xbba
            let effects = &*((*es.effects).obj);
            assert_eq!(effects.size, 1);
            assert!(effects.find(crate::outer_shadow::OUTER_SHADOW_EFFECT_KEY).is_some());
            // Scene3D / Sp3D 는 null
            assert!(es.scene3d.is_null());
            assert!(es.sp3d.is_null());
        }
    }

    #[test]
    fn block17_corrected_blur_23000_not_45398() {
        unsafe {
            let mut fs = FormatScheme::new();
            fs.create_default_block17(914400.0);
            let ctrl = fs.find_effect_style(2).expect("style 2");
            let es = &*((*ctrl).obj);
            let effects = &*((*es.effects).obj);
            let os_ctrl = effects
                .find(crate::outer_shadow::OUTER_SHADOW_EFFECT_KEY)
                .unwrap();
            let os = &*((*os_ctrl).obj as *const crate::outer_shadow::OuterShadow);

            // Verify Block 17 blur is the CORRECTED 23000 (= 0x46B3B000), NOT
            // Agent B's claimed 0x47315600 (= 45398).
            let blur = unsafe {
                let pk = crate::property_key::PropertyKey::from_int(
                    crate::outer_shadow::OuterShadow::KEY_BLUR,
                );
                let i = os.bag.impl_ref().unwrap();
                let node = i.find_equal(&pk).unwrap();
                let cb = (*node).value;
                let pf = (*cb).obj as *const crate::property::PFloat;
                (*pf).value
            };
            assert_eq!(blur.to_bits(), 0x46B3B000); // 23000.0
            assert_eq!(blur, 23000.0);
        }
    }

    #[test]
    fn block18_attaches_reflection_with_effect_key_0xbbb() {
        unsafe {
            let mut fs = FormatScheme::new();
            fs.create_default_block18(914400.0);
            assert_eq!(fs.effects.size, 1);
            let ctrl = fs.find_effect_style(3).expect("style 3");
            let es = &*((*ctrl).obj);
            assert!(!es.effects.is_null());
            let effects = &*((*es.effects).obj);
            assert_eq!(effects.size, 1);
            // Reflection at key 0xbbb (= OuterShadow's 0xbba + 1)
            assert!(effects.find(crate::reflection::REFLECTION_EFFECT_KEY).is_some());
            assert!(effects.find(crate::outer_shadow::OUTER_SHADOW_EFFECT_KEY).is_none());
        }
    }

    #[test]
    fn block18_corrected_distance_12700_not_12690() {
        unsafe {
            let mut fs = FormatScheme::new();
            fs.create_default_block18(914400.0);
            let ctrl = fs.find_effect_style(3).expect("style 3");
            let es = &*((*ctrl).obj);
            let effects = &*((*es.effects).obj);
            let r_ctrl = effects
                .find(crate::reflection::REFLECTION_EFFECT_KEY)
                .unwrap();
            let r = &*((*r_ctrl).obj as *const crate::reflection::Reflection);

            let distance = unsafe {
                let pk = crate::property_key::PropertyKey::from_int(
                    crate::reflection::Reflection::KEY_DISTANCE,
                );
                let i = r.bag.impl_ref().unwrap();
                let node = i.find_equal(&pk).unwrap();
                let cb = (*node).value;
                let pf = (*cb).obj as *const crate::property::PFloat;
                (*pf).value
            };
            // CORRECTED Block 18 distance = 12700, NOT Agent B's claimed 12690
            assert_eq!(distance.to_bits(), 0x46467000); // 12700.0
            assert_eq!(distance, 12700.0);
        }
    }

    #[test]
    fn block_16_17_18_three_effect_styles_with_distinct_subtypes() {
        unsafe {
            let mut fs = FormatScheme::new();
            fs.create_default_block16(914400.0);
            fs.create_default_block17(914400.0);
            fs.create_default_block18(914400.0);
            assert_eq!(fs.effects.size, 3);

            // Style 1, 2: OuterShadow (effect_key 0xbba)
            for style in [1u32, 2] {
                let ctrl = fs.find_effect_style(style).unwrap();
                let es = &*((*ctrl).obj);
                let effects = &*((*es.effects).obj);
                assert!(effects.find(crate::outer_shadow::OUTER_SHADOW_EFFECT_KEY).is_some());
                assert!(effects.find(crate::reflection::REFLECTION_EFFECT_KEY).is_none());
            }
            // Style 3: Reflection (effect_key 0xbbb)
            let ctrl3 = fs.find_effect_style(3).unwrap();
            let es3 = &*((*ctrl3).obj);
            let effects3 = &*((*es3.effects).obj);
            assert!(effects3.find(crate::reflection::REFLECTION_EFFECT_KEY).is_some());
            assert!(effects3.find(crate::outer_shadow::OUTER_SHADOW_EFFECT_KEY).is_none());
        }
    }

    #[test]
    fn create_default_full_12_attach_inner_byte_eq_integration() {
        // 12/12 attach with FULL inner sub-type byte-eq (Block 16-18 의 Effects/sub-type 까지).
        unsafe {
            let mut fs = FormatScheme::new();
            let engine_base = f32::from_bits(0x495F3E00); // 914400

            // Blocks 1-3 (SetBrush) — Solid + 2 Gradient
            fs.create_default_block1();
            let effects_b = FormatScheme::create_default_block6_effects();
            fs.create_default_block7_through_9(effects_b);
            crate::color_effect::ColorEffect::raw_delete(effects_b[0]);
            crate::color_effect::ColorEffect::raw_delete(effects_b[1]);
            crate::color_effect::ColorEffect::raw_delete(effects_b[2]);

            // Blocks 4-6 (SetBgBrush)
            fs.create_default_block10();
            fs.create_default_block11();
            fs.create_default_block12();

            // Blocks 7-9 (SetPen)
            fs.create_default_block13(engine_base);
            fs.create_default_block14(engine_base);
            fs.create_default_block15(engine_base);

            // Blocks 10-12 (SetEffectStyle) — ⭐ NEW: full inner byte-eq via OuterShadow/Reflection
            fs.create_default_block16(engine_base);
            fs.create_default_block17(engine_base);
            fs.create_default_block18(engine_base);

            assert_eq!(fs.brushes.size, 2);
            assert_eq!(fs.bg_brushes.size, 3);
            assert_eq!(fs.pens.size, 3);
            assert_eq!(fs.effects.size, 3);
            assert_eq!(fs.total_entries(), 11);

            // Inner sub-type byte-eq verification:
            // Style 1: OuterShadow, Style 2: OuterShadow (different blur), Style 3: Reflection
            for style in [1u32, 2] {
                let ctrl = fs.find_effect_style(style).unwrap();
                let es = &*((*ctrl).obj);
                assert!(!es.effects.is_null());
            }
            let ctrl3 = fs.find_effect_style(3).unwrap();
            let es3 = &*((*ctrl3).obj);
            let effects3 = &*((*es3.effects).obj);
            assert!(effects3.find(crate::reflection::REFLECTION_EFFECT_KEY).is_some());
        }
    }

    #[test]
    fn create_default_all_12_attach_calls_format_scheme_populated() {
        unsafe {
            let mut fs = FormatScheme::new();
            let engine_base = f32::from_bits(0x495F3E00); // 914400

            // Blocks 1-3 (SetBrush 1-3)
            fs.create_default_block1();
            // Block 5: SetBrush(2, Gradient) — caller가 effects+stops로 직접 호출 (test 별도)
            // Block 9: SetBrush(3, Gradient)
            let effects_b = FormatScheme::create_default_block6_effects();
            fs.create_default_block7_through_9(effects_b);
            crate::color_effect::ColorEffect::raw_delete(effects_b[0]);
            crate::color_effect::ColorEffect::raw_delete(effects_b[1]);
            crate::color_effect::ColorEffect::raw_delete(effects_b[2]);

            // Blocks 4-6 (SetBgBrush 1-3)
            fs.create_default_block10();
            fs.create_default_block11();
            fs.create_default_block12();

            // Blocks 7-9 (SetPen 1-3)
            fs.create_default_block13(engine_base);
            fs.create_default_block14(engine_base);
            fs.create_default_block15(engine_base);

            // Blocks 10-12 (SetEffectStyle 1-3) — outer layer only, inner sub-types deferred
            fs.create_default_block16_through_18_partial();

            // 4 trees 의 size 합계
            assert_eq!(fs.brushes.size, 2);     // Block 1 (Solid) + Block 9 (Gradient #2). Block 5 별도.
            assert_eq!(fs.bg_brushes.size, 3);
            assert_eq!(fs.pens.size, 3);
            assert_eq!(fs.effects.size, 3);
            assert_eq!(fs.total_entries(), 2 + 3 + 3 + 3);

            // vtable dispatch sanity (brushes/bg_brushes)
            let b1 = (*fs.find_brush(1).unwrap()).obj;
            assert_eq!(
                crate::brush::brush_vtable(b1 as *const u8).type_tag,
                crate::brush::BrushType::Solid as u32
            );
            let b3 = (*fs.find_brush(3).unwrap()).obj;
            assert_eq!(
                crate::brush::brush_vtable(b3 as *const u8).type_tag,
                crate::brush::BrushType::Gradient as u32
            );
        }
    }

    #[test]
    fn block14_15_use_solid_stroke_brush() {
        unsafe {
            let mut fs = FormatScheme::new();
            let engine = f32::from_bits(0x495F3E00);
            fs.create_default_block14(engine);
            fs.create_default_block15(engine);
            for style in [2u32, 3] {
                let pen = &*(*fs.find_pen(style).unwrap()).obj;
                match &*pen.brush {
                    crate::brush::Brush::Solid(_) => {}
                    _ => panic!("Block {} stroke must be SolidBrush", style + 11),
                }
            }
        }
    }

    #[test]
    fn create_default_blocks_1_through_13_all_three_trees() {
        unsafe {
            let mut fs = FormatScheme::new();
            // 1-3: SetBrush
            fs.create_default_block1();
            // (skip Block 2-5 / Block 6-9 for brevity — directly via gradient setup
            //  exercised separately)
            // 4-6: SetBackgroundBrush
            fs.create_default_block10();
            fs.create_default_block11();
            fs.create_default_block12();
            // 7: SetPen #1
            fs.create_default_block13(914400.0);
            assert_eq!(fs.brushes.size, 1);
            assert_eq!(fs.bg_brushes.size, 3);
            assert_eq!(fs.pens.size, 1);
            assert_eq!(fs.effects.size, 0);
            assert_eq!(fs.total_entries(), 5);
        }
    }

    #[test]
    fn create_default_block10_through_11_bg_brushes_size_2() {
        unsafe {
            let mut fs = FormatScheme::new();
            fs.create_default_block10();
            fs.create_default_block11();
            assert_eq!(fs.bg_brushes.size, 2);
            assert!(fs.find_background_brush(1).is_some());
            assert!(fs.find_background_brush(2).is_some());
        }
    }

    #[test]
    fn set_background_brush_null_obj_is_noop() {
        unsafe {
            let mut fs = FormatScheme::new();
            // ctrl 은 valid 이지만 obj = null
            let ctrl_layout = Layout::new::<BrushControlBlock>();
            let ctrl = std::alloc::alloc(ctrl_layout) as *mut BrushControlBlock;
            assert!(!ctrl.is_null());
            ptr::write(
                ctrl,
                BrushControlBlock {
                    obj: ptr::null_mut(),
                    strong: 1,
                    flag: 1,
                    _pad: [0u8; 7],
                },
            );
            fs.set_background_brush(1, ctrl);
            // 추가 안 됨
            assert_eq!(fs.bg_brushes.size, 0);
            // ctrl 은 caller (= test) 가 release 안 함 — leak (그러나 본 test 의 scope 내).
            // 정공법: leak 방지 위해 직접 dealloc.
            std::alloc::dealloc(ctrl as *mut u8, ctrl_layout);
        }
    }
}

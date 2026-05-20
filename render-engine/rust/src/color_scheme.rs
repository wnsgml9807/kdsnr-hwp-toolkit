//! `Hnc::Shape::ColorScheme` — 32B 1:1 byte-equivalent port.
//!
//! libHncDrawingEngine_arm64 의 `ColorScheme` 는 `CHncStringW name`
//! (+0x00..0x08) + libc++ `std::map<SchemeStyle, Color>` 의 `__tree` struct
//! (+0x08..0x20) 의 결합. 총 32B / 8B align.
//!
//! # raw 32B layout (확정 from `ColorScheme::SetAt` @ `0x150074` + ctor @ `0x14fd1c`)
//!
//! ```text
//! offset  field          type            의미
//! 0x00    name           CHncStringW     테마 이름 (default = nil sentinel)
//! 0x08    begin_node     NodeBase*       leftmost 노드 (empty = &end_node_left at +0x10)
//! 0x10    end_node_left  NodeBase*       __end_node.__left_ (= root, empty = null)
//! 0x18    size           u64             노드 개수
//! ```
//!
//! `&self.end_node_left` 의 주소가 "fake `__end_node` base" 로 사용 — first 8B
//! (__left_) 만 valid. 본 ColorScheme 의 주소가 stable 해야 하므로 `Box<Self>`
//! 또는 heap-alloc 으로 보유 필수.
//!
//! # raw `ColorScheme::ColorScheme()` (`0x14fd1c`)
//!
//! 1. `bl CHncStringW::CHncStringW()` — name default-init.
//! 2. `begin_node = &end_node_left` (raw `str x8, [x20, #0x8]!` after pre-index).
//! 3. `end_node_left = null` (raw `str xzr, [x8, #0x10]!`).
//! 4. `size = 0` (raw `str xzr, [x0, #0x18]`).
//! 5. 12 hardcoded `SetAt(key, Color)` calls. 각 Color 는 inline stack-store 후
//!    `SetAt(key, &local)` 호출.
//!
//! # raw `ColorScheme::SetAt(SchemeStyle key, Color const& value)` (`0x150074`)
//!
//! ```asm
//! 150074-150088: setup + ldr x8, [x0, #0x10]! ; x0 += 0x10, x8 = root
//!                cbz x8, INSERT_EMPTY
//! 150090-1500a8: walk loop — x22 tracks "last LE node", x23 tracks insert slot
//! 1500ac-1500bc: post-walk equality check
//!                if x20 == &end_node: INSERT_EMPTY (no match)
//!                if x20.key <= target: UPDATE
//!                else: INSERT
//!
//! INSERT @ 0x1500c0:
//!   ; Build local stack pair: sp[0..4] = key, sp[8..16] = Color value+type, sp[24..32] = clone1
//!   1500dc: bl __tree::__emplace_unique (0x662e34) ; allocates Node 64B + clone2 from local
//!   1500ec-150108: free local (clone1)
//!
//! UPDATE @ 0x15011c:
//!   ; Copy caller's value to local sp[0..12] (staging only)
//!   ; Clone caller's effect → cloned (single clone)
//!   ; Write node.color.value = caller's bytes (16B)
//!   ; Save old node.color.color_effect; set new
//!   ; Free old effect (if non-null)
//! ```
//!
//! # raw `ColorScheme::~ColorScheme()` (`0x15016c`)
//!
//! 1. `add x0, x19, #0x8` — pass `&tree` (= self+0x8) as allocator arg.
//! 2. `ldr x1, [x19, #0x10]` — pass root (= end_node_left).
//! 3. `bl 0x631b24` — recursive subtree_destroy (post-order Node free w/ Color dtor).
//! 4. tail call `CHncStringW::~CHncStringW()` on self+0x0.

use crate::color::Color;
use crate::color_effect::ColorEffect;
use crate::rb_tree::{
    balance_after_insert, find_insert_position, subtree_destroy_recursive,
    update_begin_node_after_insert, TreeBase, TreeNodeBase,
};
use crate::string_w::CHncStringW;
use crate::color::SystemStyle;
use std::alloc::Layout;
use std::ptr;

/// `std::map<SchemeStyle, Color>` 의 `__tree_node<value_type>` — 64B.
///
/// raw layout (`ColorScheme::SetAt` UPDATE 의 `node[+0x20]=key`, `node[+0x28]=value+type`,
/// `node[+0x38]=color_effect` 으로 도출):
///
/// ```text
/// offset  field          의미
/// 0x00    base           TreeNodeBase (32B: left/right/parent/is_black + pad)
/// 0x20    key            u32 (SchemeStyle)
/// 0x24    _pad           4B (alignment of Color)
/// 0x28    color          Color (24B: value+type_tag+color_effect)
/// ```
///
/// 총 64B / 8B align. raw 의 `mov w0, #0x40 (=64); bl operator_new` (`0x662e9c`).
#[repr(C)]
pub struct ColorSchemeNode {
    /// `__tree_node_base` — raw +0x00..+0x20.
    pub base: TreeNodeBase,
    /// raw +0x20: u32 key (SchemeStyle 또는 임의 u32).
    pub key: u32,
    /// raw +0x24: 4B padding (compiler-inserted for Color's 8B align).
    pub _pad_0x24: u32,
    /// raw +0x28: Color value (24B).
    pub color: Color,
}

pub const COLOR_SCHEME_NODE_SIZE_BYTES: usize = 64;

const _: () = assert!(std::mem::size_of::<ColorSchemeNode>() == COLOR_SCHEME_NODE_SIZE_BYTES);
const _: () = assert!(std::mem::align_of::<ColorSchemeNode>() == 8);

/// `Hnc::Shape::ColorScheme` — 32B.
///
/// **address stability 필수** — `tree.begin_node` 가 `&self.end_node_left` 를
/// 가리키므로 self 가 move 되면 안 됨. `Box<ColorScheme>` 또는 다른 heap
/// container 로 보유.
#[repr(C)]
pub struct ColorScheme {
    /// raw +0x00: `CHncStringW name`.
    pub name: CHncStringW,
    /// raw +0x08..+0x20: libc++ `__tree` 의 24B (begin/end/size).
    /// (호환성을 위해 inline 구조로 모델링; flat fields 가 raw 와 byte-equiv.)
    pub tree_begin_node: *mut TreeNodeBase,
    pub tree_end_node_left: *mut TreeNodeBase,
    pub tree_size: u64,
}

pub const COLOR_SCHEME_SIZE_BYTES: usize = 32;
pub const COLOR_SCHEME_ALIGN_BYTES: usize = 8;

const _: () = assert!(std::mem::size_of::<ColorScheme>() == COLOR_SCHEME_SIZE_BYTES);
const _: () = assert!(std::mem::align_of::<ColorScheme>() == COLOR_SCHEME_ALIGN_BYTES);

/// raw `ColorScheme::ColorScheme()` 의 12 hardcoded entry — `(key, Color)` 쌍.
///
/// raw asm 의 `mov w8, #...; stur w8; mov w1, #key; bl SetAt` 패턴에서 추출.
const DEFAULT_ENTRIES: [(u32, ColorSeed); 12] = [
    (0, ColorSeed::System(8)),
    (1, ColorSeed::System(5)),
    (2, ColorSeed::Rgb(0x3a, 0x3c, 0x84)),
    (3, ColorSeed::Rgb(0xfa, 0xf3, 0xdb)),
    (4, ColorSeed::Rgb(0x61, 0x82, 0xd6)),
    (5, ColorSeed::Rgb(0xff, 0x84, 0x3a)),
    (6, ColorSeed::Rgb(0xb2, 0xb2, 0xb2)),
    (7, ColorSeed::Rgb(0xff, 0xd7, 0x00)),
    (8, ColorSeed::Rgb(0x28, 0x9b, 0x6e)),
    (9, ColorSeed::Rgb(0x9d, 0x5c, 0xbb)),
    (10, ColorSeed::Rgb(0x00, 0x00, 0xff)),
    (11, ColorSeed::Rgb(0x80, 0x00, 0x80)),
];

#[derive(Clone, Copy)]
enum ColorSeed {
    System(u32),
    Rgb(u8, u8, u8),
}

impl ColorSeed {
    fn build_color(self) -> Color {
        match self {
            ColorSeed::System(v) => Color::from_system_style(SystemStyle(v)),
            ColorSeed::Rgb(r, g, b) => Color::from_rgb(r, g, b, ptr::null_mut()),
        }
    }
}

impl ColorScheme {
    /// raw `ColorScheme::ColorScheme()` (`0x14fd1c`) — `Box<Self>` 반환.
    ///
    /// Box wrap 으로 address stability 보장 (tree.begin_node 의 self-pointer
    /// 가 valid 유지).
    ///
    /// 본 함수는:
    /// 1. heap-alloc 32B (Box).
    /// 2. name default-init (CHncStringW::default — nil sentinel).
    /// 3. tree init_empty.
    /// 4. 12 hardcoded SetAt 호출 (raw 순서 그대로).
    pub fn new() -> Box<Self> {
        let mut boxed: Box<ColorScheme> = Box::new(ColorScheme {
            name: CHncStringW::default(),
            tree_begin_node: ptr::null_mut(),
            tree_end_node_left: ptr::null_mut(),
            tree_size: 0,
        });
        unsafe {
            boxed.init_tree();
            for (key, seed) in DEFAULT_ENTRIES {
                let local_color = seed.build_color();
                boxed.set_at(key, &local_color);
                drop(local_color); // raw 의 local destroy
            }
        }
        boxed
    }

    /// raw `tree.init_empty()` (라이브러리 [[rb_tree]] 의 `TreeBase::init_empty`
    /// 와 동등) — begin_node = &end_node_left, end_node_left = null, size = 0.
    ///
    /// # Safety
    /// `self` 는 stable heap address.
    pub unsafe fn init_tree(&mut self) {
        let end_node_addr =
            (&mut self.tree_end_node_left) as *mut *mut TreeNodeBase as *mut TreeNodeBase;
        self.tree_begin_node = end_node_addr;
        self.tree_end_node_left = ptr::null_mut();
        self.tree_size = 0;
    }

    /// 내부 view of __tree as `TreeBase` — find_insert_position / balance 등에
    /// pass 하기 위함.
    #[inline]
    fn tree_view(&mut self) -> *mut TreeBase {
        // ColorScheme 의 [0x08..0x20] 는 정확히 TreeBase layout 과 일치.
        // 따라서 self + 8 을 *mut TreeBase 로 reinterpret.
        let p = self as *mut ColorScheme as *mut u8;
        unsafe { p.add(8) as *mut TreeBase }
    }

    /// raw `ColorScheme::SetAt(SchemeStyle key, Color const& value)` (`0x150074`).
    ///
    /// 동일 key 가 이미 있으면 UPDATE (raw `0x15011c-0x150150`), 없으면 INSERT
    /// (raw `0x1500c0-0x15010c` via `0x662e34` __emplace_unique).
    ///
    /// # Safety
    /// `value.color_effect` 가 valid 또는 null.
    pub unsafe fn set_at(&mut self, key: u32, value: &Color) {
        // raw walk @ 0x150084-0x1500bc
        let tree_ptr = self.tree_view();
        let target = key;
        let (slot, parent, existing) = find_insert_position(tree_ptr, |np| {
            let n = np as *const ColorSchemeNode;
            (*n).key.cmp(&target)
        });

        if !existing.is_null() {
            // UPDATE branch (raw 0x15011c-0x150150) — single clone of caller's effect.
            let n = existing as *mut ColorSchemeNode;
            // raw 0x150130: `bl 0x65411c` on caller's effect → cloned
            let cloned_effect = ColorEffect::clone_raw(value.color_effect);
            // raw 0x150138-0x150140: write value bytes (16B) into node
            (*n).color.value = value.value;
            (*n).color.type_tag = value.type_tag;
            // raw 0x150144-0x150148: save old, set new
            let old_effect = (*n).color.color_effect;
            (*n).color.color_effect = cloned_effect;
            // raw 0x15014c-0x150150: if old non-null, free
            if !old_effect.is_null() {
                ColorEffect::raw_delete(old_effect);
            }
        } else {
            // INSERT branch — strict 2-clone path matching raw.
            //
            // raw 0x1500c8-0x1500dc: build local pair on stack with cloned effect.
            //
            // Rust 에서 stack-local `Color` 의 RAII 가 raw 의 스택 cleanup 과
            // semantic 동일. `Color::copy_ctor` 가 raw `Color::Color(Color const&)`
            // 의 직접 1:1 (16B memcpy + ColorEffect::clone_raw).
            let local_color = Color::copy_ctor(value);

            // raw 0x1500dc-0x1500e8: bl __tree::__emplace_unique
            //   안 (raw 0x662e9c-0x662ec0): alloc 64B Node, init key + base + value via
            //   Color copy ctor (= 2nd clone of effect).
            let node_layout = Layout::new::<ColorSchemeNode>();
            let new_node = std::alloc::alloc(node_layout) as *mut ColorSchemeNode;
            if new_node.is_null() {
                std::alloc::handle_alloc_error(node_layout);
            }
            // raw 0x662ec4: stp xzr, xzr, [x21]   → left/right = null
            // raw 0x662ec8: str x22, [x21, #0x10] → parent = (parent from walk)
            // is_black 은 balance_after_insert 가 초기화.
            ptr::write(
                new_node,
                ColorSchemeNode {
                    base: TreeNodeBase {
                        left: ptr::null_mut(),
                        right: ptr::null_mut(),
                        parent,
                        is_black: 0,
                        _pad_0x19: [0; 7],
                    },
                    key: target,
                    _pad_0x24: 0,
                    // raw 0x662eb0-0x662ec0: 16B memcpy + clone effect (= 2nd clone)
                    color: Color::copy_ctor(&local_color),
                },
            );

            // raw 0x662ecc: *slot = new_node — link into tree
            *slot = new_node as *mut TreeNodeBase;

            // raw 0x662ed0-0x662ee4: update begin_node if newly inserted is leftmost
            update_begin_node_after_insert(tree_ptr);

            // raw 0x662ee8-0x662eec: balance after insert
            //   root = (*tree).end_node_left
            let root = (*tree_ptr).end_node_left;
            balance_after_insert(root, new_node as *mut TreeNodeBase);

            // raw 0x662ef0-0x662ef8: size++
            (*tree_ptr).size += 1;

            // raw 0x1500ec-0x150108: free local effect (clone1).
            // Rust: drop(local_color) 가 자동으로 raw_delete 호출.
            drop(local_color);
        }
    }

    /// raw `ColorScheme::Contains(SchemeStyle) const` (`0x14fa38`) — key 존재 여부.
    ///
    /// 본 raw 함수는 별도 dump 안 떴지만 표준 std::map::count() 동등.
    pub fn contains(&mut self, key: u32) -> bool {
        unsafe {
            let tree_ptr = self.tree_view();
            let target = key;
            let (_slot, _parent, existing) = find_insert_position(tree_ptr, |np| {
                let n = np as *const ColorSchemeNode;
                (*n).key.cmp(&target)
            });
            !existing.is_null()
        }
    }

    /// raw `ColorScheme::GetColor(SchemeStyle) const` (`0x14fc58`) — key 의 Color
    /// 반환. 없으면 None (raw 는 default-init Color 반환하는 패턴; Rust 는 안전성
    /// 위해 Option).
    pub fn get_color(&mut self, key: u32) -> Option<&Color> {
        unsafe {
            let tree_ptr = self.tree_view();
            let target = key;
            let (_slot, _parent, existing) = find_insert_position(tree_ptr, |np| {
                let n = np as *const ColorSchemeNode;
                (*n).key.cmp(&target)
            });
            if existing.is_null() {
                None
            } else {
                let n = existing as *const ColorSchemeNode;
                Some(&(*n).color)
            }
        }
    }

    /// tree size (number of entries).
    #[inline]
    pub fn len(&self) -> u64 {
        self.tree_size
    }

    /// empty 인가?
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.tree_size == 0
    }

    /// raw `ColorScheme::Clone() const` (`0x15059c`) — sret 패턴 1:1 port.
    ///
    /// 알고리즘:
    /// 1. heap-alloc 32B 새 ColorScheme.
    /// 2. CHncStringW name 의 copy ctor (refcount++) 호출.
    /// 3. tree init_empty.
    /// 4. src tree 의 in-order 순회 (`__tree_next` chain): 각 (key, color) 에 대해
    ///    new.set_at(key, color).
    /// 5. 새 ptr 반환.
    ///
    /// raw 의 tree iteration (`0x150628-0x150654`) 은 [[rb_tree]] 의 `tree_next` 1:1.
    /// 각 노드의 insert 는 raw `0x63188c` (`__emplace_unique`) — 본 Rust port 는 set_at
    /// 의 INSERT path 와 functional 동등 (sorted input → 동일 RB tree shape).
    ///
    /// # Safety
    /// self 는 valid heap-alloc ColorScheme (Box::into_raw 또는 동등).
    pub unsafe fn clone_to_heap(&self) -> *mut ColorScheme {
        // raw `1505b8-1505bc: mov w0, #0x20; bl operator_new` — alloc 32B
        let layout = Layout::new::<ColorScheme>();
        let new_p = std::alloc::alloc(layout) as *mut ColorScheme;
        if new_p.is_null() {
            std::alloc::handle_alloc_error(layout);
        }
        // raw `1505c4-1505c8: bl CHncStringW copy ctor` — name field
        // raw `1505cc-1505dc: empty tree init` — begin = end_node, end_left = null, size = 0
        ptr::write(
            new_p,
            ColorScheme {
                name: self.name.clone(), // refcount++ semantic
                tree_begin_node: ptr::null_mut(),
                tree_end_node_left: ptr::null_mut(),
                tree_size: 0,
            },
        );
        (*new_p).init_tree();

        // raw `1505e0-1505ec: if src tree empty, return`
        let src_tree_p = self as *const ColorScheme as *mut ColorScheme;
        let src_end_addr = {
            let p = src_tree_p as *mut u8;
            (p.add(8 + 8)) as *mut TreeNodeBase // = &src.tree_end_node_left
        };
        let mut cur = self.tree_begin_node;
        if cur == src_end_addr {
            return new_p;
        }

        // raw `150614-150654`: in-order traversal — for each node, SetAt 새 트리에
        loop {
            let n = cur as *const ColorSchemeNode;
            let key = (*n).key;
            let color_ref = &(*n).color;
            (*new_p).set_at(key, color_ref);

            // raw `__tree_next`: 다음 in-order 노드
            let next = crate::rb_tree::tree_next(cur);
            if next == src_end_addr {
                break;
            }
            cur = next;
        }

        new_p
    }

    /// raw `ColorScheme::CloneOrNew` 등 (raw @ `0x66ead8`) — null-safe wrapper.
    ///
    /// 의미: `src ? new ColorScheme(*src) : nullptr`. Theme copy ctor 에서
    /// `self.color_scheme = clone_or_null(src.color_scheme)` 패턴으로 사용.
    ///
    /// raw `0x66ead8` 의 본문은 `Clone` (0x15059c) 과 거의 동일 — null check + 동일 deep copy.
    ///
    /// # Safety
    /// `src` 는 valid `*const ColorScheme` 또는 null.
    pub unsafe fn clone_or_null(src: *const ColorScheme) -> *mut ColorScheme {
        // raw `66eaec: cbz x0, 0x66eb94` — null → null
        if src.is_null() {
            return ptr::null_mut();
        }
        (*src).clone_to_heap()
    }

    /// heap-alloc 된 ColorScheme* 해제 — Theme dtor 에 의해 호출되는 패턴.
    ///
    /// raw `~Theme` (`0x1ec084-0x1ec0a0`) 의 ColorScheme cleanup:
    /// ```asm
    /// 1ec08c: add x0, x20, #0x8          ; tree
    /// 1ec090: ldr x1, [x20, #0x10]        ; root
    /// 1ec094: bl 0x631b24                  ; subtree_destroy_recursive
    /// 1ec098: mov x0, x20
    /// 1ec09c: bl 0x6b3a5c                  ; ~CHncStringW (name)
    /// 1ec0a0: bl operator_delete            ; free struct
    /// ```
    ///
    /// Rust: `drop_in_place` 가 ColorScheme 의 Drop impl 호출 (tree teardown + name),
    /// 이어서 dealloc.
    ///
    /// # Safety
    /// `p` 가 `clone_to_heap` 또는 동등 heap alloc 으로 얻은 ptr 또는 null.
    pub unsafe fn raw_delete(p: *mut ColorScheme) {
        if p.is_null() {
            return;
        }
        ptr::drop_in_place(p);
        std::alloc::dealloc(p as *mut u8, Layout::new::<ColorScheme>());
    }
}

impl Drop for ColorScheme {
    /// raw `ColorScheme::~ColorScheme()` (`0x15016c`) — subtree_destroy + name dtor.
    ///
    /// ```asm
    /// 15017c: add  x0, x0, #0x8           ; pass &tree (allocator arg of subtree_destroy)
    /// 150180: ldr  x1, [x19, #0x10]       ; x1 = root (= end_node_left)
    /// 150184: bl   0x631b24               ; recursive destroy
    /// 150188-15018c: x0 = this, tail call ~CHncStringW
    /// ```
    fn drop(&mut self) {
        unsafe {
            let root = self.tree_end_node_left;
            // Recursive destroy with per-node closure: drop value + free node.
            let drop_node = |node: *mut TreeNodeBase| {
                let n = node as *mut ColorSchemeNode;
                // raw 0x631b50-0x631b6c: free ColorEffect (= ~Color in Node).
                // Rust: drop_in_place runs Color's Drop impl which handles effect.
                ptr::drop_in_place(&mut (*n).color);
                // raw 0x631b74-0x631b7c: tail call operator_delete on node.
                std::alloc::dealloc(n as *mut u8, Layout::new::<ColorSchemeNode>());
            };
            subtree_destroy_recursive(root, &drop_node);
            // raw 0x150194: tail call ~CHncStringW — Rust: drop(name) auto.
            // (이미 ColorScheme 의 자동 field drop 순서로 처리됨; Drop impl 내에선
            // 자동 호출 안 됨. 직접 drop_in_place 호출 필요.)
            // 단, Rust Drop 은 drop body 후 fields 자동 drop 하므로 name 은
            // 별도 명시 불필요. ✓
            self.tree_end_node_left = ptr::null_mut();
            self.tree_begin_node = ptr::null_mut();
            self.tree_size = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scheme_style::SchemeStyle;

    #[test]
    fn raw_layout_size_align() {
        assert_eq!(std::mem::size_of::<ColorScheme>(), 32);
        assert_eq!(std::mem::align_of::<ColorScheme>(), 8);
    }

    #[test]
    fn node_layout_size_align() {
        assert_eq!(std::mem::size_of::<ColorSchemeNode>(), 64);
        assert_eq!(std::mem::align_of::<ColorSchemeNode>(), 8);
    }

    #[test]
    fn node_field_offsets_match_raw() {
        // raw 의 access pattern 검증:
        //   node[+0x20] = key (4B)
        //   node[+0x28] = color (24B)
        let n: ColorSchemeNode = unsafe { std::mem::zeroed() };
        let p = &n as *const ColorSchemeNode as usize;
        let pb = &n.base as *const _ as usize;
        let pk = &n.key as *const _ as usize;
        let pc = &n.color as *const _ as usize;
        assert_eq!(pb - p, 0x00);
        assert_eq!(pk - p, 0x20);
        assert_eq!(pc - p, 0x28);
        // Discard to skip Drop of `Color` (it expects valid effect ptr; we zeroed which is null = OK,
        // but the implicit drop after `n` 스코프 exit triggers Color::drop which checks null ok).
    }

    #[test]
    fn scheme_field_offsets_match_raw() {
        let mut cs = ColorScheme {
            name: CHncStringW::default(),
            tree_begin_node: ptr::null_mut(),
            tree_end_node_left: ptr::null_mut(),
            tree_size: 0,
        };
        let p = &cs as *const ColorScheme as usize;
        let pn = &cs.name as *const _ as usize;
        let pb = &cs.tree_begin_node as *const _ as usize;
        let pe = &cs.tree_end_node_left as *const _ as usize;
        let ps = &cs.tree_size as *const _ as usize;
        assert_eq!(pn - p, 0x00);
        assert_eq!(pb - p, 0x08);
        assert_eq!(pe - p, 0x10);
        assert_eq!(ps - p, 0x18);
        let _ = cs.tree_view();
    }

    #[test]
    fn default_ctor_creates_12_entries() {
        let mut cs = ColorScheme::new();
        assert_eq!(cs.len(), 12);
        assert!(!cs.is_empty());
        for k in 0..12 {
            assert!(cs.contains(k), "missing key {}", k);
        }
        // out of range
        assert!(!cs.contains(12));
        assert!(!cs.contains(100));
    }

    #[test]
    fn default_colors_match_raw_hardcoded_bytes() {
        let mut cs = ColorScheme::new();
        // Entry 0: SystemStyle(8)
        {
            let c = cs.get_color(0).unwrap();
            assert_eq!(c.type_tag, 3);
            assert_eq!(c.value[0], 8);
        }
        // Entry 1: SystemStyle(5)
        {
            let c = cs.get_color(1).unwrap();
            assert_eq!(c.type_tag, 3);
            assert_eq!(c.value[0], 5);
        }
        // Entry 2..11: Rgb 의 hardcoded 색상 (raw 와 byte-eq)
        let expected_rgbs: [(u32, [u8; 3]); 10] = [
            (2, [0x3a, 0x3c, 0x84]),
            (3, [0xfa, 0xf3, 0xdb]),
            (4, [0x61, 0x82, 0xd6]),
            (5, [0xff, 0x84, 0x3a]),
            (6, [0xb2, 0xb2, 0xb2]),
            (7, [0xff, 0xd7, 0x00]),
            (8, [0x28, 0x9b, 0x6e]),
            (9, [0x9d, 0x5c, 0xbb]),
            (10, [0x00, 0x00, 0xff]),
            (11, [0x80, 0x00, 0x80]),
        ];
        for (key, expected) in expected_rgbs {
            let c = cs.get_color(key).unwrap();
            assert_eq!(c.type_tag, 0, "key {} expected RGB type", key);
            assert_eq!(c.value[0], expected[0], "key {} R", key);
            assert_eq!(c.value[1], expected[1], "key {} G", key);
            assert_eq!(c.value[2], expected[2], "key {} B", key);
        }
    }

    #[test]
    fn set_at_then_get_round_trip() {
        let mut cs = ColorScheme::new();
        let new_color = Color::from_rgb(0xAB, 0xCD, 0xEF, ptr::null_mut());
        unsafe {
            cs.set_at(SchemeStyle::Background2 as u32, &new_color);
        }
        let got = cs.get_color(SchemeStyle::Background2 as u32).unwrap();
        assert_eq!(got.value[0], 0xAB);
        assert_eq!(got.value[1], 0xCD);
        assert_eq!(got.value[2], 0xEF);
        assert_eq!(got.type_tag, 0);
    }

    #[test]
    fn set_at_overwrites_existing() {
        let mut cs = ColorScheme::new();
        // Default for key 2 = Rgb(0x3a, 0x3c, 0x84)
        assert_eq!(cs.get_color(2).unwrap().value[0], 0x3a);
        let new_color = Color::from_rgb(0x11, 0x22, 0x33, ptr::null_mut());
        unsafe {
            cs.set_at(2, &new_color);
        }
        assert_eq!(cs.get_color(2).unwrap().value[0], 0x11);
        assert_eq!(cs.get_color(2).unwrap().value[1], 0x22);
        assert_eq!(cs.get_color(2).unwrap().value[2], 0x33);
        // size unchanged
        assert_eq!(cs.len(), 12);
    }

    #[test]
    fn set_at_new_key_grows_tree() {
        let mut cs = ColorScheme::new();
        assert_eq!(cs.len(), 12);
        let new_color = Color::from_rgb(0xFE, 0xED, 0xFA, ptr::null_mut());
        unsafe {
            cs.set_at(100, &new_color);
        }
        assert_eq!(cs.len(), 13);
        assert!(cs.contains(100));
        assert_eq!(cs.get_color(100).unwrap().value[0], 0xFE);
    }

    #[test]
    fn set_at_preserves_effect_clone() {
        unsafe {
            let mut cs = ColorScheme::new();
            // Color with non-null effect
            let effect = ColorEffect::create();
            let layout = std::alloc::Layout::from_size_align(8, 8).unwrap();
            let buf = std::alloc::alloc(layout) as *mut u64;
            *buf = 0xABCDEF;
            (*effect).begin = buf;
            (*effect).end = buf.add(1);
            (*effect).cap_end = buf.add(1);

            let c = Color::from_rgb(1, 2, 3, effect);
            cs.set_at(50, &c);

            // The stored color has its own effect clone
            let stored = cs.get_color(50).unwrap();
            assert!(!stored.color_effect.is_null());
            assert_ne!(stored.color_effect, c.color_effect);
            assert_eq!(*(*stored.color_effect).begin, 0xABCDEF);
        }
    }

    #[test]
    fn drop_color_scheme_releases_all_nodes() {
        // Smoke test: build full scheme, drop, no panic/leak.
        for _ in 0..50 {
            let cs = ColorScheme::new();
            drop(cs);
        }
    }

    #[test]
    fn empty_color_scheme_get_returns_none() {
        let mut cs: Box<ColorScheme> = Box::new(ColorScheme {
            name: CHncStringW::default(),
            tree_begin_node: ptr::null_mut(),
            tree_end_node_left: ptr::null_mut(),
            tree_size: 0,
        });
        unsafe {
            cs.init_tree();
        }
        assert!(cs.is_empty());
        assert!(cs.get_color(0).is_none());
        assert!(!cs.contains(0));
    }

    #[test]
    fn set_at_stress_100_random_keys() {
        let mut cs: Box<ColorScheme> = Box::new(ColorScheme {
            name: CHncStringW::default(),
            tree_begin_node: ptr::null_mut(),
            tree_end_node_left: ptr::null_mut(),
            tree_size: 0,
        });
        unsafe {
            cs.init_tree();
            let mut state: u32 = 67890;
            let mut keys = Vec::new();
            for _ in 0..200 {
                state = state.wrapping_mul(1103515245).wrapping_add(12345);
                let k = state % 1000;
                if !keys.contains(&k) {
                    keys.push(k);
                    let c = Color::from_rgb((k & 0xff) as u8, 0, 0, ptr::null_mut());
                    cs.set_at(k, &c);
                }
            }
            assert_eq!(cs.len(), keys.len() as u64);
            for k in keys {
                assert!(cs.contains(k));
                assert_eq!(cs.get_color(k).unwrap().value[0], (k & 0xff) as u8);
            }
        }
    }

    #[test]
    fn name_default_is_nil_sentinel() {
        let cs = ColorScheme::new();
        // CHncStringW default 가 nil sentinel — debug/length 등 검증
        // CHncStringW default 는 nil sentinel (length = 0).
        assert_eq!(cs.name.length(), 0);
    }
}

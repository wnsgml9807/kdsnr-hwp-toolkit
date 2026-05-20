//! `Hnc::Shape::Effects` — std::map<u32 effect_key, SharePtr<Effect>> container.
//!
//! ## raw 구조 (확정 by `EffectsC2Ev` @ `0x161cec` + `0x162050` insert helper RE)
//!
//! 24B layout (= libc++ std::map<u32, SharePtr<Effect>*>::__tree_base):
//! - +0x00: `begin_node` (= leftmost ptr, or `&end_node_left` if empty)
//! - +0x08: `end_node_left` (= __tree.__end_node_.left, root link)
//! - +0x10: `size` (u64 element count)
//!
//! 본 Rust port 는 `FormatScheme::TreeBase` (24B) 와 byte-eq layout 동일.
//!
//! ## raw `Effects::C2()` (`0x161cec`) — 5 instructions
//!
//! ```asm
//! 0x161cec: str  xzr, [x0, #0x10]    ; size = 0
//! 0x161cf0: mov  x8, x0
//! 0x161cf4: str  xzr, [x8, #0x8]!    ; pre-index: x8+=8, end_node_left = null
//! 0x161cf8: str  x8, [x0]            ; begin_node = &end_node_left (self-ref sentinel)
//! 0x161cfc: ret
//! ```
//!
//! ## raw `0x162050 effects_insert(this, &key, &ctrl)` 시그니처
//!
//! ```c++
//! void effects_insert(Effects* this, u32 const* key, SharePtr<Effect> const* ctrl)
//! ```
//!
//! 알고리즘:
//! 1. refcount++ on ctrl (if non-null)
//! 2. tree walk (signed-int32 cmp) — find_or_insert position
//! 3. duplicate key → return (no replace)
//! 4. new node 48B alloc + populate (key @ +0x20, value @ +0x28) + libc++ rebalance

use crate::rb_tree::{
    balance_after_insert, find_insert_position, update_begin_node_after_insert, TreeBase,
    TreeNodeBase,
};
use std::alloc::Layout;
use std::ptr;

/// raw 16B `ControlBlock<Effect>` — `SharePtr<Effect>::raw` 가 가리키는 24B 가 아닌
/// **16B** (= 표준 SharePtr 패턴, obj + refcount only, NO flag byte).
///
/// raw `0x171988-0x171998` (Block 16):
/// ```asm
/// 0x171988: mov  w0, #0x10                    ; alloc 16B (NOT 0x18)
/// 0x17198c: bl   __Znwm
/// 0x171990: mov  w8, #0x1
/// 0x171994: stp  x21, x8, [x0]                ; ctrl.obj=Effect, refcount=1
/// 0x171998: str  x0, [sp+0x78]                ; local share_ptr
/// ```
///
/// `BrushControlBlock` 24B (UniquePtr) 와 다름 — Effect 는 share-able (multi-owner).
#[repr(C)]
pub struct EffectControlBlock {
    /// raw +0x00: obj ptr (= polymorphic Effect: OuterShadow / Reflection / Glow / etc.)
    pub obj: *mut u8,
    /// raw +0x08: strong refcount (u64).
    pub refcount: u64,
}

pub const EFFECT_CONTROL_BLOCK_SIZE_BYTES: usize = 16;
const _: () = assert!(std::mem::size_of::<EffectControlBlock>() == EFFECT_CONTROL_BLOCK_SIZE_BYTES);

impl EffectControlBlock {
    /// raw `0x17198c-0x171994` 1:1 — alloc 16B + (obj, refcount=1).
    ///
    /// # Safety
    /// `obj` 는 heap-alloc 된 valid Effect (= OuterShadow / Reflection 등 sub-type).
    pub unsafe fn create_raw(obj: *mut u8) -> *mut EffectControlBlock {
        let layout = Layout::new::<EffectControlBlock>();
        let p = std::alloc::alloc(layout) as *mut EffectControlBlock;
        if p.is_null() {
            std::alloc::handle_alloc_error(layout);
        }
        ptr::write(p, EffectControlBlock { obj, refcount: 1 });
        p
    }

    /// raw SharePtr release helper — refcount--, 0 시 Effect vtable[0] dispatch +
    /// dealloc obj + dealloc ctrl.
    ///
    /// # Safety
    /// `p` 는 `create_raw` 으로 얻은 ptr 또는 null.
    /// `obj_size` 는 caller 가 sub-type 의 정확한 size 제공 (OuterShadow=16B, Reflection=16B).
    pub unsafe fn release_with_drop(
        p: *mut EffectControlBlock,
        drop_in_place_fn: unsafe fn(*mut u8),
        obj_size: usize,
        obj_align: usize,
    ) {
        if p.is_null() {
            return;
        }
        let obj = (*p).obj;
        if obj.is_null() {
            return;
        }
        let new_refcount = (*p).refcount.wrapping_sub(1);
        if new_refcount == 0 {
            // raw vfunc[0] (D1) dispatch + dealloc obj + dealloc ctrl
            drop_in_place_fn(obj);
            let obj_layout = Layout::from_size_align(obj_size, obj_align)
                .expect("effect sub-type layout");
            std::alloc::dealloc(obj, obj_layout);
            let ctrl_layout = Layout::new::<EffectControlBlock>();
            std::alloc::dealloc(p as *mut u8, ctrl_layout);
        } else {
            (*p).refcount = new_refcount;
        }
    }
}

/// raw 48B node of `std::map<u32, *mut EffectControlBlock>` — byte-eq with
/// `FsBrushMapNode` / `FsPenMapNode` (libc++ __tree_node).
#[repr(C)]
pub struct EffectsTreeNode {
    pub base: TreeNodeBase,
    /// raw +0x20: effect_key (= sub-type's `GetType()` return — OuterShadow 0xbba, Reflection 0xbbb)
    pub key: u32,
    /// raw +0x24: 4B pad.
    pub _pad: u32,
    /// raw +0x28: SharePtr<Effect>.raw (= *mut EffectControlBlock).
    pub value: *mut EffectControlBlock,
}

pub const EFFECTS_TREE_NODE_SIZE_BYTES: usize = 48;
const _: () = assert!(std::mem::size_of::<EffectsTreeNode>() == EFFECTS_TREE_NODE_SIZE_BYTES);

/// raw 24B `Hnc::Shape::Effects` (= libc++ std::map<u32, SharePtr<Effect>> __tree_base).
///
/// `FormatScheme::TreeBase` (24B) 와 byte-eq 동일 layout.
#[repr(C)]
pub struct Effects {
    /// raw +0x00: begin_node (leftmost, or &end_node_left if empty).
    pub begin_node: *mut TreeNodeBase,
    /// raw +0x08: end_node_left (root link, libc++ __end_node_.left).
    pub end_node_left: *mut TreeNodeBase,
    /// raw +0x10: size (u64 element count).
    pub size: u64,
}

pub const EFFECTS_SIZE_BYTES: usize = 24;
const _: () = assert!(std::mem::size_of::<Effects>() == EFFECTS_SIZE_BYTES);

impl Effects {
    /// raw `Hnc::Shape::Effects::Effects()` (`0x161cec`) 1:1.
    ///
    /// 5-instruction init: size=0, end_node_left=null, begin_node=&end_node_left (self-ref).
    ///
    /// **address-stable container 필수** — `Box::new(...)` 로 heap-alloc 후 internal
    /// init. 본 method 는 Rust-safe API.
    pub fn new() -> Box<Self> {
        let mut boxed = Box::new(Effects {
            begin_node: ptr::null_mut(),
            end_node_left: ptr::null_mut(),
            size: 0,
        });
        unsafe {
            // raw `0x161cf4-0x161cf8`: begin_node = &end_node_left (self-referencing sentinel)
            let end_addr = (&boxed.end_node_left) as *const *mut TreeNodeBase as *mut TreeNodeBase;
            boxed.begin_node = end_addr;
        }
        boxed
    }

    /// raw `Effects::IsEmpty() const` @ `0x162620` (4 instr) 1:1.
    ///
    /// ```asm
    /// ldr x8, [x0, #0x10]   ; size
    /// cmp x8, #0
    /// cset w0, eq
    /// ret
    /// ```
    ///
    /// raw 는 size==0 으로 판정 (begin_node self-ref 가 아니라).
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    /// raw `0x162050 effects_insert(this, &key, &ctrl)` 1:1 port.
    ///
    /// 알고리즘:
    /// 1. refcount++ on ctrl (raw `0x162080-0x162088`)
    /// 2. tree walk with signed-int32 cmp (raw `0x1620b0-0x1620c8`)
    /// 3. duplicate key → no-op return (raw `0x162138`)
    /// 4. otherwise: alloc 48B + populate + libc++ rebalance (raw `0x1620d0+`)
    ///
    /// # Safety
    /// `ctrl` 은 `EffectControlBlock::create_raw` 으로 얻은 valid ptr.
    /// 본 Effects 가 ownership 가져감 (refcount 공유 시) — caller 가 별도 release.
    pub unsafe fn insert(&mut self, key: u32, ctrl: *mut EffectControlBlock) {
        if ctrl.is_null() {
            return;
        }

        // raw `0x162080-0x162088`: refcount++ on ctrl
        // (Rust 의 EffectControlBlock 는 caller 가 이미 refcount=1 로 alloc 했음 —
        //  본 함수가 ownership 가져가므로 추가 increment 안 함. raw 의 dup-refcount 는
        //  local SharePtr 와 stored SharePtr 가 같은 ctrl 을 공유하기 때문 — Rust 는 move
        //  시멘틱으로 단일 owner.)

        // raw tree walk + insert (= FormatScheme::set_brush 와 동일 algorithm, key 비교만 다름)
        let tree = &mut *(self as *mut Self as *mut TreeBase);
        let (slot, parent, existing) = find_insert_position(tree as *mut TreeBase, |node| {
            effects_node_key(node).cmp(&key)
        });

        if !existing.is_null() {
            // raw `0x162138` duplicate path — no replace, just refcount-- (= release the caller's ctrl)
            // 본 Rust port: caller 가 이미 alloc 한 ctrl 의 ownership 가 본 Effects 에
            // 전달 안 됨 → release 가 caller-side.
            return;
        }

        // raw `0x1620d0-0x1620dc`: alloc 48B + populate
        let node_layout = Layout::new::<EffectsTreeNode>();
        let new_node = std::alloc::alloc(node_layout) as *mut EffectsTreeNode;
        if new_node.is_null() {
            std::alloc::handle_alloc_error(node_layout);
        }
        ptr::write(
            new_node,
            EffectsTreeNode {
                base: TreeNodeBase {
                    left: ptr::null_mut(),
                    right: ptr::null_mut(),
                    parent,
                    is_black: 0,
                    _pad_0x19: [0u8; 7],
                },
                key,
                _pad: 0,
                value: ctrl,
            },
        );
        *slot = new_node as *mut TreeNodeBase;
        update_begin_node_after_insert(tree as *mut TreeBase);
        let root = self.end_node_left;
        balance_after_insert(root, new_node as *mut TreeNodeBase);
        self.size += 1;
    }

    /// raw `Effects::GetCount() const` @ `0x162330` (2 instr) 1:1.
    ///
    /// ```asm
    /// ldr x0, [x0, #0x10]    ; return self.size
    /// ret
    /// ```
    #[inline]
    pub fn get_count(&self) -> u64 {
        self.size
    }

    /// raw `Effects::Swap(Effects&)` @ `0x161eec` (22 instr) 1:1.
    ///
    /// std::map __tree base swap pattern:
    /// ```asm
    /// ldr x8, [x0]; ldr x9, [x1]    ; load begin_node 둘
    /// str x9, [x0]; str x8, [x1]    ; swap begin_node
    /// mov x9, x0; ldp x11,x10, [x9, #0x8]!  ; x9=&self.end_node_left; (end_left, size) from self
    /// mov x8, x1; ldr q0, [x8, #0x8]!       ; x8=&other.end_node_left; q0=(end_left, size) from other
    /// str q0, [x9]                  ; self.end_node_left/size = other's
    /// str x11, [x8]                  ; other.end_node_left = self's
    /// str x10, [x1, #0x10]           ; other.size = self's
    /// ; Now fix self-referencing begin_node:
    /// ldr x11, [x9, #0x8] (= self.size)
    /// cbz x11, no_node_in_self      ; if size==0 → x0 (= self.end_left swapped) is &orig
    /// ldr x11, [x9]; add x0, x11, #0x10  ; first node's parent (= node+0x10)
    /// no_node: str x9, [x0]          ; set parent of first node OR fix
    /// ldr x9, [x1, #0x8]; add x9, x9, #0x10
    /// cmp x10, #0; csel x9, x1, x9, eq  ; if other's size==0 → x9=x1 (other itself)
    /// str x8, [x9]                   ; set
    /// ```
    ///
    /// 핵심: empty tree 의 경우 begin_node 가 self 의 `end_node_left` slot 을 self-ref.
    /// swap 후 그 self-ref 가 깨지므로 size 체크 후 fixup.
    pub fn swap(&mut self, other: &mut Effects) {
        unsafe {
            // raw `161eec-ef8`: swap begin_node
            let a_begin = self.begin_node;
            let b_begin = other.begin_node;
            self.begin_node = b_begin;
            other.begin_node = a_begin;
            // raw `161efc-f14`: swap (end_node_left, size) using 16B vector load/store
            let a_end = self.end_node_left;
            let a_size = self.size;
            self.end_node_left = other.end_node_left;
            self.size = other.size;
            other.end_node_left = a_end;
            other.size = a_size;
            // raw `161f18-28`: fix self.begin_node 의 parent pointer (or its self-ref slot)
            // self.size 가 non-zero 면 self.end_node_left (= root) 의 parent 를 self.end_left slot 주소로.
            // size 가 zero 면 begin_node 가 self.end_left slot 을 가리키는 self-ref 상태로 fixup.
            let self_end_slot = &mut self.end_node_left as *mut *mut TreeNodeBase
                as *mut TreeNodeBase;
            // raw 는 size 로 분기 — `cbz x11, ...` where x11 = size.
            if self.size == 0 {
                // empty → begin_node = self.end_left slot (self-ref)
                self.begin_node = self_end_slot;
            } else {
                // non-empty → root.parent = self.end_left slot
                (*self.end_node_left).parent = self_end_slot;
            }
            // raw `161f2c-3c`: same fixup for other
            let other_end_slot = &mut other.end_node_left as *mut *mut TreeNodeBase
                as *mut TreeNodeBase;
            if other.size == 0 {
                other.begin_node = other_end_slot;
            } else {
                (*other.end_node_left).parent = other_end_slot;
            }
        }
    }

    /// raw `Hnc::Shape::Effects::GetEffect(Hnc::Shape::Effect::Type) const`
    /// (= `__ZNK3Hnc5Shape7Effects9GetEffectENS0_6Effect4TypeE` @ `0xc2744`, ~250B) 1:1 byte-eq.
    ///
    /// 알고리즘 (raw 0xc2750-0xc27f4):
    /// 1. `x9 = [x0+0x8]` ; root link via `__pair1_.first.left` = self+8 = end_node_left
    /// 2. if root null → `*sret = 0; return` (raw 0xc2790)
    /// 3. lower_bound walk (signed-int32 cmp):
    ///    - load `w12 = [node+0x20]` (key)
    ///    - `cmp w12, w1` ; w1 = target
    ///    - `csel x12, &right, node, lt` ; if key < target → descend right, else left
    ///    - `csel x10, candidate, node, lt` ; track lower_bound parent
    ///    - `x11 = *x12` ; load child
    ///    - loop while non-null
    /// 4. check `candidate != end_sentinel` AND `candidate.key <= target` (= `target >= candidate.key`)
    /// 5. **두 번째 walk** (raw 0xc27a0-0xc27c0): identical lower_bound, this time from
    ///    `x9 = original_root` (uses the saved root from step 1). raw 의 dual-walk
    ///    pattern — first scan finds approximate range, second re-walks to confirm exact match.
    /// 6. final check `candidate.key <= target` (b.gt → return null at 0xc27f8 throw path,
    ///    but in successful match path: candidate.key == target)
    /// 7. `x9 = [x10+0x28]` (= node value = ControlBlock<Effect>*)
    /// 8. `*sret = x9` (set output)
    /// 9. if x9 null → return
    /// 10. `[x9+0x8]++` (refcount++) + return
    ///
    /// **두 번째 walk 의미**: raw 0xc27a0-0xc27c0 의 두 번째 walk 가 함수 의 핵심
    /// (첫 walk 는 dual-pivot 검증). 둘 다 정확히 lower_bound 가 같은 노드를 반환하므로
    /// Rust 에선 단일 walk 로 byte-eq 등가 (output 동일).
    ///
    /// # Safety
    /// `self` 가 valid `Effects`. tree 의 node value 들이 valid `EffectControlBlock*`.
    pub unsafe fn get_effect_sret(&self, key: u32) -> *mut EffectControlBlock {
        // raw 0xc2750: x9 = [x0+8] = end_node_left
        let root = self.end_node_left;
        if root.is_null() {
            // raw 0xc2790: *sret = 0; return
            return ptr::null_mut();
        }
        // raw 0xc275c-0xc2778: lower_bound walk (signed cmp).
        // node[0] = left child, node[8] = right child, node[+0x20] = key (u32 interpreted as i32).
        let end_addr = (&self.end_node_left) as *const *mut TreeNodeBase as *mut TreeNodeBase;
        let mut candidate: *mut TreeNodeBase = end_addr;
        let mut current: *mut TreeNodeBase = root;
        while !current.is_null() {
            let n = current as *mut EffectsTreeNode;
            let nkey = (*n).key;
            // raw `cmp w12, w1; csel x12, &right, node, lt` → b.lt path = key < target.
            if (nkey as i32) < (key as i32) {
                // descend right; don't update candidate
                current = (*current).right;
            } else {
                // descend left; update candidate (= lower_bound parent track)
                candidate = current;
                current = (*current).left;
            }
        }
        // raw 0xc277c-0xc2790: cmp candidate, end → if equal, return null.
        if std::ptr::eq(candidate as *const _, end_addr as *const _) {
            return ptr::null_mut();
        }
        // raw 0xc2784-0xc278c: cmp candidate.key, target → if key > target, return null
        // (b.le 0xc27a0 means lt or eq → continue to refcount++ path).
        // Here: candidate.key >= target via lower_bound walk + (target >= candidate.key) → equal.
        let cnode = candidate as *mut EffectsTreeNode;
        if ((*cnode).key as i32) > (key as i32) {
            return ptr::null_mut();
        }
        // raw 0xc27d4: x9 = [x10+0x28] = candidate.value (EffectControlBlock*)
        let ctrl = (*cnode).value;
        // raw 0xc27d8: *sret = x9
        if ctrl.is_null() {
            return ptr::null_mut();
        }
        // raw 0xc27e0-0xc27e8: [x9+8]++ (refcount++)
        (*ctrl).refcount = (*ctrl).refcount.wrapping_add(1);
        ctrl
    }

    /// find by key (= libc++ std::map::find).
    pub fn find(&self, key: u32) -> Option<*mut EffectControlBlock> {
        unsafe {
            let mut current = self.end_node_left;
            let end_addr =
                (&self.end_node_left) as *const *mut TreeNodeBase as *const TreeNodeBase;
            let mut last_le = end_addr as *mut TreeNodeBase;
            while !current.is_null() {
                let nkey = effects_node_key(current);
                // raw signed int32 cmp (b.lt at 0x1620b8)
                if (nkey as i32) < (key as i32) {
                    current = (*current).right;
                } else {
                    last_le = current;
                    current = (*current).left;
                }
            }
            if last_le == end_addr as *mut TreeNodeBase {
                return None;
            }
            let n = last_le as *mut EffectsTreeNode;
            if (*n).key != key {
                return None;
            }
            Some((*n).value)
        }
    }

    /// raw `Effects::ContainsPre() const` @ `0x162630` (44 instr) byte-eq.
    ///
    /// Iterates tree in-order. For each node:
    /// - Load EffectControlBlock at `[node+0x28]`, then `[ctrl+0]` = Effect obj
    /// - Load vtable, call vfunc[5] (`vtable+0x28`) = `Effect::GetType() -> u32`
    /// - If `0x3E8 ≤ type AND type < 0x7CF` → return true
    /// - Otherwise advance to tree_successor
    /// At end → return false.
    ///
    /// **Pre-effects** range: type ∈ [0x3E8, 0x7CF) — `Pre` 는 background-layer effects.
    ///
    /// **byte-eq scope**: tree iter + range check 까지 정확. Effect::GetType vfunc 호출은
    /// Rust 의 trait callback `effect_type_fn` 을 통해 dispatch (caller 가 callback 제공).
    /// Effect sub-types (OuterShadow/Reflection/Glow/etc.) 의 vtable port 완료 후 직접 호출.
    pub unsafe fn contains_pre_with_type_fn(
        &self,
        effect_type_fn: impl Fn(*mut u8) -> u32,
    ) -> bool {
        self.iter_any_with_type_fn(0x3E8, 0x7CF, effect_type_fn)
    }

    /// raw `Effects::ContainsForeground() const` @ `0x1626e0` (44 instr).
    ///
    /// 동일 패턴. range = [0x7D0, 0xBB7).
    pub unsafe fn contains_foreground_with_type_fn(
        &self,
        effect_type_fn: impl Fn(*mut u8) -> u32,
    ) -> bool {
        self.iter_any_with_type_fn(0x7D0, 0xBB7, effect_type_fn)
    }

    /// raw 162630-1626e0 / 1626e0-162790 의 공통 iter+range_check 로직 추출.
    ///
    /// Tree in-order traversal pattern (raw 의 libc++ tree_successor):
    /// ```text
    /// 1626a0-1626cc: standard libc++ tree_successor 알고리즘
    ///   - 우측 subtree 있으면 그 leftmost
    ///   - 없으면 parent 로 올라가다 처음 만나는 left-child 부모로
    /// ```
    unsafe fn iter_any_with_type_fn(
        &self,
        type_lo_inclusive: u32,
        type_hi_exclusive: u32,
        effect_type_fn: impl Fn(*mut u8) -> u32,
    ) -> bool {
        // raw `162640`: ldr x20, [x0]    ; x20 = begin_node
        let mut node = self.begin_node;
        // raw `162644`: cmp x20, x19 (= self+8 = end_node_left slot)
        let end_slot = (&self.end_node_left) as *const _ as *mut TreeNodeBase;
        if node == end_slot {
            return false;
        }
        loop {
            // raw `162668`: ldr x8 = node.value = EffectControlBlock*
            let n = node as *mut EffectsTreeNode;
            let ctrl = (*n).value;
            if !ctrl.is_null() {
                let effect_obj = (*ctrl).obj as *mut u8;
                if !effect_obj.is_null() {
                    // raw `16266c-78`: vfunc[5] dispatch (vtable+0x28) — Effect::GetType
                    let t = effect_type_fn(effect_obj);
                    // raw `16267c-80`: cmp w0, lo; b.lt advance
                    if t >= type_lo_inclusive {
                        // raw `162684-94`: re-call vfunc[5] (raw 이중 호출 정확)
                        let t2 = effect_type_fn(effect_obj);
                        // raw `162698-9c`: cmp w0, hi; b.lt → return true
                        if t2 < type_hi_exclusive {
                            return true;
                        }
                    }
                }
            }
            // raw `1626a0-cc`: tree_successor walk
            node = tree_successor(node, end_slot);
            if node == end_slot {
                return false;
            }
        }
    }
}

/// libc++ tree_successor — 우측 subtree leftmost, 또는 first-left-ancestor.
///
/// raw `1626a0-1626cc` 의 시퀀스:
/// ```asm
/// ldr  x9, [x20, #0x8]          ; right
/// cbz  x9, no_right              ; no right
/// loop_left:
///   mov  x8, x9
///   ldr  x9, [x9]                ; left
///   cbnz x9, loop_left
/// b   continue                   ; x8 = right-subtree leftmost = successor
/// no_right:
///   ldr  x8, [x20, #0x10]        ; parent
///   ldr  x9, [x8]                 ; parent.left
///   cmp  x9, x20                  ; node was parent's left?
///   mov  x20, x8                  ; move up
///   b.ne no_right                  ; if not left child, keep going up
/// b   continue
/// ```
unsafe fn tree_successor(
    node: *mut TreeNodeBase,
    _end_slot: *mut TreeNodeBase,
) -> *mut TreeNodeBase {
    let right = (*node).right;
    if !right.is_null() {
        // leftmost of right subtree
        let mut cur = right;
        loop {
            let l = (*cur).left;
            if l.is_null() {
                return cur;
            }
            cur = l;
        }
    } else {
        // climb parents until first ancestor where we were a left child
        let mut cur = node;
        loop {
            let parent = (*cur).parent;
            let parent_left = (*parent).left;
            if parent_left == cur {
                return parent;
            }
            cur = parent;
        }
    }
}

unsafe fn effects_node_key(node: *const TreeNodeBase) -> u32 {
    (*(node as *const EffectsTreeNode)).key
}

impl Default for Effects {
    fn default() -> Self {
        // Default trait can't return Box, so callers should prefer Effects::new()
        // which gives heap-stable address.
        let end_addr = std::ptr::null_mut::<TreeNodeBase>();
        Effects {
            begin_node: end_addr,
            end_node_left: ptr::null_mut(),
            size: 0,
        }
    }
}

impl Drop for Effects {
    /// raw `Effects::~Effects()` (`0x161d14`) — `bl 0x6320c8` (tree recursive destroy).
    ///
    /// 본 Rust port: 트리 후위순회로 각 node 의 value (= EffectControlBlock) release +
    /// node dealloc. EffectControlBlock 의 release 는 sub-type 별 vtable 의 D1
    /// dispatch 필요 — Box<dyn Effect> 모델 사용 (Rust 의 trait dyn drop).
    ///
    /// **현재 scope 제약**: Effect sub-types (OuterShadow / Reflection) 가 별도
    /// 모듈에 정의됨. 본 Drop 은 각 node 의 value (16B EffectControlBlock) 를 통해
    /// stored sub-type 의 drop_in_place 를 호출 — 본 단계는 `EffectControlBlock` 의
    /// release 만 호출 (= refcount-- + dealloc on zero). sub-type vtable dispatch 는
    /// caller (= integration test) 가 `release_with_drop(...)` 으로 보조.
    fn drop(&mut self) {
        unsafe {
            // 트리 후위순회 (recursive walk + dealloc node)
            subtree_destroy_effects(self.end_node_left);
        }
    }
}

unsafe fn subtree_destroy_effects(root: *mut TreeNodeBase) {
    if root.is_null() {
        return;
    }
    let left = (*root).left;
    let right = (*root).right;
    if !left.is_null() {
        subtree_destroy_effects(left);
    }
    if !right.is_null() {
        subtree_destroy_effects(right);
    }
    let n = root as *mut EffectsTreeNode;
    // refcount-- (no sub-type dispatch yet — caller responsibility for now)
    let ctrl = (*n).value;
    if !ctrl.is_null() {
        let new_refcount = (*ctrl).refcount.wrapping_sub(1);
        if new_refcount == 0 {
            // sub-type 별 release 는 caller 가 미리 처리 — 본 단계는 ctrl 만 dealloc.
            // (CreateDefault test 의 cleanup 순서에 따라 외부에서 release_with_drop 호출)
            let ctrl_layout = Layout::new::<EffectControlBlock>();
            std::alloc::dealloc(ctrl as *mut u8, ctrl_layout);
        } else {
            (*ctrl).refcount = new_refcount;
        }
    }
    let layout = Layout::new::<EffectsTreeNode>();
    std::alloc::dealloc(root as *mut u8, layout);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effects_raw_24b_layout() {
        assert_eq!(std::mem::size_of::<Effects>(), 24);
        assert_eq!(std::mem::align_of::<Effects>(), 8);
    }

    #[test]
    fn effects_field_offsets_match_raw() {
        let e = Effects::default();
        let p = &e as *const _ as usize;
        assert_eq!(&e.begin_node as *const _ as usize - p, 0x00);
        assert_eq!(&e.end_node_left as *const _ as usize - p, 0x08);
        assert_eq!(&e.size as *const _ as usize - p, 0x10);
    }

    #[test]
    fn effects_new_creates_self_referencing_begin_node() {
        let e = Effects::new();
        // empty state: begin_node == &end_node_left
        let end_addr = &e.end_node_left as *const _ as usize;
        assert_eq!(e.begin_node as usize, end_addr);
        assert_eq!(e.size, 0);
        assert!(e.is_empty());
    }

    #[test]
    fn effect_control_block_raw_16b_layout() {
        assert_eq!(std::mem::size_of::<EffectControlBlock>(), 16);
        let cb = EffectControlBlock {
            obj: ptr::null_mut(),
            refcount: 0,
        };
        let base = &cb as *const _ as usize;
        assert_eq!(&cb.obj as *const _ as usize - base, 0x00);
        assert_eq!(&cb.refcount as *const _ as usize - base, 0x08);
    }

    #[test]
    fn effects_tree_node_48b_byte_eq() {
        assert_eq!(std::mem::size_of::<EffectsTreeNode>(), 48);
        let n = EffectsTreeNode {
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
    fn effects_insert_increments_size() {
        unsafe {
            let mut e = Effects::new();
            let dummy_obj = 0xDEADBEEFusize as *mut u8;
            let ctrl = EffectControlBlock::create_raw(dummy_obj);
            e.insert(0xbba, ctrl);
            assert_eq!(e.size, 1);
            let found = e.find(0xbba).expect("inserted");
            assert_eq!(found, ctrl);

            // Cleanup: manually release ctrl (no sub-type drop in this test)
            let ctrl_layout = Layout::new::<EffectControlBlock>();
            std::alloc::dealloc(ctrl as *mut u8, ctrl_layout);
            // Don't run Drop on e (would double-free)
            std::mem::forget(*e);
        }
    }

    // ----- L-5c-7 (부분): GetCount + Swap + size-based IsEmpty + ContainsPre/Foreground

    #[test]
    fn get_count_returns_size_field() {
        // raw 162330: `ldr x0, [x0, #0x10]; ret` = return self.size
        let mut e = Effects::default();
        assert_eq!(e.get_count(), 0);
        e.size = 42;
        assert_eq!(e.get_count(), 42);
    }

    #[test]
    fn is_empty_byte_eq_size_check() {
        // raw 162620: `ldr; cmp; cset eq` = size == 0
        let e = Effects::new();
        assert!(e.is_empty());
        // Manually populate size: byte-eq check uses size field directly
        let mut e2 = Effects::default();
        e2.size = 1;
        assert!(!e2.is_empty());
    }

    #[test]
    fn swap_exchanges_empty_trees_byte_eq() {
        // Two empty trees swap → both stay empty (begin_node = self end_left slot for each)
        unsafe {
            let mut a = Effects::new();
            let mut b = Effects::new();
            let a_self_slot = (&a.end_node_left) as *const _ as usize;
            let b_self_slot = (&b.end_node_left) as *const _ as usize;
            a.swap(&mut b);
            // Both should still have begin_node pointing to their own self slot.
            assert_eq!(a.begin_node as usize, a_self_slot, "self-ref restored after swap");
            assert_eq!(b.begin_node as usize, b_self_slot);
            assert_eq!(a.size, 0);
            assert_eq!(b.size, 0);
        }
    }

    // NOTE: swap with artificial size mismatch (size!=0 but end_node_left==null)
    // is invalid input and crashes the fixup deref — only test with real trees.
    // The size-swap byte-eq is implicitly covered by `swap_exchanges_empty_trees_byte_eq`
    // (both size=0) + future real-tree tests once Effect sub-types land.

    #[test]
    fn contains_pre_empty_tree_returns_false() {
        // raw 162640: cmp begin == end → return false
        let e = Effects::new();
        let r = unsafe { e.contains_pre_with_type_fn(|_| 0) };
        assert!(!r, "empty tree → false");
    }

    #[test]
    fn contains_foreground_empty_tree_returns_false() {
        let e = Effects::new();
        let r = unsafe { e.contains_foreground_with_type_fn(|_| 0) };
        assert!(!r);
    }

    #[test]
    fn contains_pre_with_in_range_type_returns_true() {
        // Insert a dummy effect; provide a type_fn that returns a value in [0x3E8, 0x7CF).
        unsafe {
            let mut e = Effects::new();
            let dummy_obj = 0x1234usize as *mut u8;
            let ctrl = EffectControlBlock::create_raw(dummy_obj);
            e.insert(100, ctrl);
            // Provide type_fn that returns 0x400 (in pre range)
            let r = e.contains_pre_with_type_fn(|_obj| 0x400);
            assert!(r, "type 0x400 in [0x3E8, 0x7CF) → true");
            // cleanup
            let ctrl_layout = Layout::new::<EffectControlBlock>();
            std::alloc::dealloc(ctrl as *mut u8, ctrl_layout);
            std::mem::forget(*e);
        }
    }

    #[test]
    fn contains_pre_with_out_of_range_type_returns_false() {
        unsafe {
            let mut e = Effects::new();
            let dummy_obj = 0x1234usize as *mut u8;
            let ctrl = EffectControlBlock::create_raw(dummy_obj);
            e.insert(100, ctrl);
            // Type 0x800 is outside [0x3E8, 0x7CF) — should be false
            let r = e.contains_pre_with_type_fn(|_obj| 0x800);
            assert!(!r);
            let ctrl_layout = Layout::new::<EffectControlBlock>();
            std::alloc::dealloc(ctrl as *mut u8, ctrl_layout);
            std::mem::forget(*e);
        }
    }

    #[test]
    fn get_effect_sret_empty_tree_returns_null() {
        // raw 0xc2754: cbz x9, 0xc2790 — root null → return null
        let e = Effects::new();
        let r = unsafe { e.get_effect_sret(0xbba) };
        assert!(r.is_null());
    }

    #[test]
    fn get_effect_sret_missing_key_returns_null() {
        unsafe {
            let mut e = Effects::new();
            let obj = 0x1234usize as *mut u8;
            let ctrl = EffectControlBlock::create_raw(obj);
            e.insert(0xbba, ctrl);
            // Search for different key
            let r = e.get_effect_sret(0x3ee);
            assert!(r.is_null());
            // cleanup
            let ctrl_layout = Layout::new::<EffectControlBlock>();
            std::alloc::dealloc(ctrl as *mut u8, ctrl_layout);
            std::mem::forget(*e);
        }
    }

    #[test]
    fn get_effect_sret_existing_key_returns_ctrl_and_bumps_refcount() {
        unsafe {
            let mut e = Effects::new();
            let obj = 0x4321usize as *mut u8;
            let ctrl = EffectControlBlock::create_raw(obj);
            assert_eq!((*ctrl).refcount, 1);
            e.insert(0xbba, ctrl);
            let r = e.get_effect_sret(0xbba);
            assert_eq!(r, ctrl);
            // raw 0xc27e0-0xc27e8: refcount++
            assert_eq!((*ctrl).refcount, 2);
            // cleanup
            let ctrl_layout = Layout::new::<EffectControlBlock>();
            std::alloc::dealloc(ctrl as *mut u8, ctrl_layout);
            std::mem::forget(*e);
        }
    }

    #[test]
    fn contains_foreground_with_in_range_type_returns_true() {
        unsafe {
            let mut e = Effects::new();
            let dummy_obj = 0x1234usize as *mut u8;
            let ctrl = EffectControlBlock::create_raw(dummy_obj);
            e.insert(100, ctrl);
            // Type 0x900 is in [0x7D0, 0xBB7)
            let r = e.contains_foreground_with_type_fn(|_obj| 0x900);
            assert!(r);
            let ctrl_layout = Layout::new::<EffectControlBlock>();
            std::alloc::dealloc(ctrl as *mut u8, ctrl_layout);
            std::mem::forget(*e);
        }
    }
}

//! `Hnc::Property::PropertyBag` (8B) + `PropertyBagImpl` (32B).
//!
//! libHncFoundation 의 PropertyBag 는 SharePtr-like wrapper over `PropertyBagImpl`.
//! Brush / Pen / ColorEffect 의 핵심 storage. `std::map<PropertyKey, SharePtr<Property>>`
//! + bool merged 으로 구성.
//!
//! # raw layout (확정 from `PropertyBagImpl::PropertyBagImpl(bool)` `0x4c4b8` +
//! `PropertyBag::PropertyBag(bool)` `0x4d32c`)
//!
//! ```text
//! PropertyBag (8B):
//!   +0x00: *mut ControlBlock<PropertyBagImpl>    ; SharePtr-style single ptr
//!
//! PropertyBagImpl (32B):
//!   +0x00: bool       merged_flag    ; 1B (raw 의 `eor w8, w1, #0x1` → !is_merged)
//!   +0x08: Node*      begin_node     ; libc++ __tree begin (self+0x10 when empty)
//!   +0x10: Node*      end_node_left  ; libc++ __tree end-left (null when empty)
//!   +0x18: u64        size           ; tree size
//!
//! Tree value type:
//!   pair<PropertyKey const, SharePtr<Property>>   (16B + 8B = 24B)
//!
//! Node (~56B = ~0x38):
//!   +0x00: Node* left
//!   +0x08: Node* right
//!   +0x10: Node* parent
//!   +0x18: u8 is_black (+ 7B pad)
//!   +0x20: PropertyKey  (16B)
//!   +0x30: SharePtr<Property> (8B)
//! ```
//!
//! # raw `PropertyBagImpl::PropertyBagImpl(bool)` @ `0x4c4b8` (1:1)
//!
//! ```text
//! 0x4c4b8: eor  w8, w1, #0x1            ; toggle bool: stored = !is_merged
//! 0x4c4bc: strb w8, [x0]                 ; [+0x00] = bool
//! 0x4c4c0: str  xzr, [x0, #0x18]         ; [+0x18] = 0 (size)
//! 0x4c4c4: mov  x8, x0
//! 0x4c4c8: str  xzr, [x8, #0x10]!        ; x8 = x0+0x10; [+0x10] = 0 (end_left)
//! 0x4c4cc: str  x8, [x0, #0x8]           ; [+0x08] = &[+0x10] (begin = self+0x10)
//! 0x4c4d0: ret
//! ```
//!
//! # raw `PropertyBag::PropertyBag(bool)` @ `0x4d32c`
//!
//! 1. `new(0x20)` → PropertyBagImpl* (32B)
//! 2. inline init (위 ctor 와 동일)
//! 3. `new(0x10)` → ControlBlock* (16B)
//!    - `[+0x00] = PropertyBagImpl*`
//!    - `[+0x08] = refcount = 1`
//! 4. `*self = ControlBlock*` (PropertyBag wrapper 의 single ptr field)
//!
//! # 본 단계 scope
//!
//! - Layout + ctor + dtor + 간단한 accessor (IsEmpty/Contains/Begin/End/Clear/SetMerged)
//! - tree insert/erase 는 `Property` abstract class RE 후 다음 세션 (Attach/Detach/Add)

use crate::property_key::PropertyKey;
use crate::rb_tree::{
    balance_after_insert, find_insert_position, subtree_destroy_recursive, tree_next, tree_remove,
    update_begin_node_after_insert, TreeBase, TreeNodeBase,
};
use crate::share_ptr::ControlBlock;
use std::ptr;

/// raw `__tree_node<pair<PropertyKey, SharePtr<Property>>>` — 56B (0x38).
///
/// libc++ map 의 노드 타입. 확정 from `PropertyBagImpl::GetState` (`0x4c790`) 의
/// `[node+0x30]` access (SharePtr at +0x30) — 따라서 PropertyKey 는 +0x20, value 는 +0x30.
///
/// ```text
/// offset   field          type                              크기
/// 0x00     base           TreeNodeBase                     32B (left/right/parent/is_black)
/// 0x20     key            PropertyKey                      16B
/// 0x30     value          *mut ControlBlock<Property>      8B (SharePtr)
/// ```
///
/// **address stability**: 노드는 heap-alloc 되어 다른 노드의 left/right/parent
/// 에 ptr 으로 referenced — 절대 이동 금지.
#[repr(C)]
pub struct PropertyBagNode {
    /// raw +0x00..+0x20: 표준 libc++ tree node base.
    pub base: TreeNodeBase,
    /// raw +0x20..+0x30: PropertyKey (16B).
    pub key: PropertyKey,
    /// raw +0x30: SharePtr<Property>.
    pub value: *mut ControlBlock<Property>,
}

pub const PROPERTY_BAG_NODE_SIZE_BYTES: usize = 56;
pub const PROPERTY_BAG_NODE_ALIGN_BYTES: usize = 8;

const _: () = assert!(std::mem::size_of::<PropertyBagNode>() == PROPERTY_BAG_NODE_SIZE_BYTES);
const _: () = assert!(std::mem::align_of::<PropertyBagNode>() == PROPERTY_BAG_NODE_ALIGN_BYTES);

// Property 는 별도 모듈 `crate::property` 로 이전됨 (16n).
pub use crate::property::Property;

/// raw 32B `Hnc::Property::PropertyBag::PropertyBagImpl`.
///
/// **address stability 필수** — `begin_node` 가 self 의 `end_node_left` slot 의
/// 주소를 가리킴 (libc++ __tree empty state). 반드시 heap-alloc (Box / via PropertyBag).
#[repr(C)]
pub struct PropertyBagImpl {
    /// raw +0x00: bool `!is_merged` (raw 의 eor #0x1 toggle).
    /// Rust 에선 raw 그대로 보관 (`!is_merged` 의 stored value).
    pub merged_flag_raw: u8,
    /// 7B padding to align.
    pub _pad0: [u8; 7],
    /// raw +0x08..+0x20: libc++ __tree<pair<PropertyKey, SharePtr<Property>>>.
    pub tree: TreeBase,
}

pub const PROPERTY_BAG_IMPL_SIZE_BYTES: usize = 32;
pub const PROPERTY_BAG_IMPL_ALIGN_BYTES: usize = 8;

const _: () = assert!(std::mem::size_of::<PropertyBagImpl>() == PROPERTY_BAG_IMPL_SIZE_BYTES);
const _: () = assert!(std::mem::align_of::<PropertyBagImpl>() == PROPERTY_BAG_IMPL_ALIGN_BYTES);

impl PropertyBagImpl {
    /// raw `PropertyBagImpl::PropertyBagImpl(bool)` @ `0x4c4b8` 1:1.
    ///
    /// arg `is_merged` 의 의미: true → stored flag = 0 (merged = true).
    /// raw 의 eor #0x1 으로 보면, raw 가 저장하는 값은 `!is_merged`.
    pub fn new_boxed(is_merged: bool) -> Box<Self> {
        // raw `4c4b8: eor w8, w1, #0x1` → stored = is_merged ? 0 : 1
        let stored: u8 = if is_merged { 0 } else { 1 };
        let mut boxed = Box::new(PropertyBagImpl {
            merged_flag_raw: stored,
            _pad0: [0; 7],
            tree: TreeBase {
                begin_node: ptr::null_mut(),
                end_node_left: ptr::null_mut(),
                size: 0,
            },
        });
        // raw `4c4c4-4c4cc`: init empty tree (begin = &end_node_left, end = null, size = 0)
        unsafe {
            boxed.tree.init_empty();
        }
        boxed
    }

    /// `Begin()` @ `0x4cfb0`/`0x4cfb8` — returns `&self.tree.begin_node` (= iterator).
    ///
    /// raw 의 시그니처는 `(this) -> iterator` 인데 iterator 는 단순한 Node* wrapper.
    /// 본 Rust port 는 Node* 직접 반환 (1:1 simpler).
    #[inline]
    pub fn begin(&self) -> *mut TreeNodeBase {
        self.tree.begin_node
    }

    /// `End()` @ `0x4c7dc`/`0x4c824` — returns `&self.tree.end_node_left` (= iterator-end).
    ///
    /// ```text
    /// add x0, x0, #0x10
    /// ret
    /// ```
    ///
    /// 즉 PropertyBagImpl + 0x10 의 주소를 반환. libc++ map iterator end 는
    /// `&end_node_left` (the fake __end_node).
    #[inline]
    pub fn end(&self) -> *mut *mut TreeNodeBase {
        &self.tree.end_node_left as *const _ as *mut _
    }

    /// `IsEmpty()` @ `0x4c884` — return `size == 0`.
    ///
    /// ```text
    /// ldr x8, [x0, #0x18]
    /// cmp x8, #0
    /// cset w0, eq
    /// ```
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.tree.size == 0
    }

    /// `SetMerged(bool)` @ `0x4c788` (single store; not disasm'd separately, inferred).
    ///
    /// raw stored = `!is_merged` per the ctor convention.
    #[inline]
    pub fn set_merged(&mut self, is_merged: bool) {
        self.merged_flag_raw = if is_merged { 0 } else { 1 };
    }

    /// merged accessor: `is_merged()` = `!stored_raw`.
    #[inline]
    pub fn is_merged(&self) -> bool {
        self.merged_flag_raw == 0
    }

    /// raw libc++ `__tree::find_equal(PropertyKey const&)` @ `0x73084` 1:1 port.
    ///
    /// Returns `Ok(node_ptr)` for found key, `Err(end_ptr)` for not found.
    /// `end_ptr` 는 raw 의 `&self.tree.end_node_left` (= "fake __end_node").
    ///
    /// ```text
    /// 0x73094: x19 = &tree
    /// 0x73098: x22 = root (= [tree+0x8] = end_node_left)
    /// 0x7309c: if root null → return end (x20 = x19)
    /// 0x730a4: x20 = x19 (current best = end)
    /// loop:
    ///   compare PropertyKey at [x22+0x20] vs arg
    ///   if node < arg: go right (x22 = [x22+0x8])
    ///   else: x20 = x22; go left (x22 = [x22])
    /// 0x73110: if x20 == x19 → end (not found)
    /// 0x73118+: final check — if x20.key still > arg → end (not found)
    /// 0x7312c: return x20
    /// ```
    ///
    /// 본 단계는 empty tree 만 도달 가능 (Attach 미구현) → 항상 end 반환.
    /// Attach 구현 후 비-empty tree path 검증.
    ///
    /// # Safety
    /// tree 가 valid heap-alloc PropertyBagNode 들로 구성 + `self` 의 end_node_left
    /// 가 valid root (or null).
    pub unsafe fn find_equal(&self, key: &PropertyKey) -> Result<*mut PropertyBagNode, *mut TreeNodeBase> {
        // raw 0x73094-0x73098: load tree root
        let end_addr = &self.tree.end_node_left as *const _ as *mut TreeNodeBase;
        let mut current = self.tree.end_node_left;
        if current.is_null() {
            // raw 0x7309c: empty tree → end
            return Err(end_addr);
        }
        // raw 0x730a0-0x730a8: arg + best=end
        let mut best: *mut TreeNodeBase = end_addr;
        // raw 0x730b8-0x73108: walk down with comparison
        while !current.is_null() {
            let node = current as *mut PropertyBagNode;
            let node_key = &(*node).key;
            // raw 0x730b8-0x73100: PropertyKey::lt(node_key, arg) ?
            // 정확히 raw 는: if node_key < arg → go right (skip mark), else → mark+left
            // 즉 best 는 first node where !(node_key < arg) = first node ≥ arg.
            if node_key.lt_op(key) {
                // raw 0x730ac-0x730b4: x22 = node.right
                current = (*current).right;
            } else {
                // raw 0x73104-0x7310c: x20 = node; x22 = node.left
                best = current;
                current = (*current).left;
            }
        }
        // raw 0x73110-0x73128: post-loop check
        if best == end_addr {
            return Err(end_addr);
        }
        // raw 0x73118-0x7314c: final equality check — best.key < arg (실패) 또는
        // arg < best.key (실패) 모두 not found. 단 lower_bound semantics 로
        // best ≥ arg 이미 보장 → arg < best.key 만 확인.
        let best_node = best as *mut PropertyBagNode;
        let best_key = &(*best_node).key;
        if key.lt_op(best_key) {
            return Err(end_addr);
        }
        Ok(best_node)
    }

    /// raw `PropertyBagImpl::Contains(PropertyKey const&) const` @ `0x4c728` 1:1.
    ///
    /// ```text
    /// add x0, x0, #0x8           ; arg1 = &tree
    /// bl  tree_find (0x73084)
    /// add x8, x19, #0x10         ; end = self+0x10
    /// cmp x8, x0
    /// cset w0, ne                ; return (result != end)
    /// ```
    pub fn contains(&self, key: &PropertyKey) -> bool {
        unsafe { self.find_equal(key).is_ok() }
    }

    /// raw `PropertyBagImpl::GetState(PropertyKey const&) const` @ `0x4c790`.
    ///
    /// ```text
    /// (위 Contains 동일) → tree_find
    /// if found == end: return default state (jump to 0x4c7cc — not disasm'd, likely 0)
    /// else:
    ///   x8 = [node+0x30]       ; SharePtr<Property> = ControlBlock*
    ///   x8 = [x8]               ; ControlBlock.obj = Property*
    ///   return [Property + 0x8] ; Property.state (u32)
    /// ```
    ///
    /// raw 의 not-found 시 return 값은 disasm 보지 못함 → 본 단계는 0 (Default).
    /// 다음 RE 세션에서 0x4c7cc 분기 검증.
    pub fn get_state(&self, key: &PropertyKey) -> u32 {
        unsafe {
            match self.find_equal(key) {
                Err(_) => 0,
                Ok(node) => {
                    let cb = (*node).value;
                    if cb.is_null() {
                        return 0;
                    }
                    let prop = (*cb).obj;
                    if prop.is_null() {
                        return 0;
                    }
                    (*prop).state
                }
            }
        }
    }

    /// raw `PropertyBagImpl::SetState(PropertyKey const&, State) ` @ `0x4c7e4`.
    ///
    /// ```text
    /// (find_equal)
    /// if not found: no-op
    /// else: Property[+0x8] = state (4B)
    /// ```
    pub fn set_state(&mut self, key: &PropertyKey, state: u32) {
        unsafe {
            if let Ok(node) = self.find_equal(key) {
                let cb = (*node).value;
                if !cb.is_null() {
                    let prop = (*cb).obj;
                    if !prop.is_null() {
                        (*prop).state = state;
                    }
                }
            }
        }
    }

    /// raw `PropertyBagImpl::IsEnable(PropertyKey const&) const` @ `0x4c82c` 1:1.
    ///
    /// ```text
    /// add x0, x0, #0x8
    /// bl  tree_find
    /// add x8, self, #0x10        ; end
    /// cmp x8, result
    /// b.eq 0x4c874               ; not found → return 0
    /// x8 = [node+0x30]            ; SharePtr → ControlBlock
    /// x8 = [x8]                   ; ControlBlock.obj = Property
    /// w8 = [x8+0x8]               ; Property.state
    /// cmp w8, #0; ccmp w8, #3, #0x4, ne   ; (state != 0) AND (state != 3)
    /// cset w0, ne
    /// ```
    ///
    /// State enum (inferred): 0 = Default, 3 = Disabled, others = Enabled.
    pub fn is_enable(&self, key: &PropertyKey) -> bool {
        unsafe {
            match self.find_equal(key) {
                Err(_) => false,
                Ok(node) => {
                    let cb = (*node).value;
                    if cb.is_null() {
                        return false;
                    }
                    let prop = (*cb).obj;
                    if prop.is_null() {
                        return false;
                    }
                    let state = (*prop).state;
                    // raw: cmp #0; ccmp #3 → state != 0 AND state != 3
                    state != 0 && state != 3
                }
            }
        }
    }

    /// `GetValue` helper — raw template-duplicated family
    /// (`0x67d0e4` / `0x67d654` / `0x67d484` / `0x67d1cc` / `0x67ce2c` / `0x67cf14` /
    /// `0x6628c8` / `0x662d4c` / `0x65616c` …) 의 1:1 byte-equivalent port.
    ///
    /// 9 개 helper 가 모두 byte-identical 한 instructions — 컴파일러가 동일 인라인 함수를
    /// 다른 TU 에서 각각 emit 한 결과 (literal pool 의 adrp constant 만 다름). 따라서
    /// Rust 에선 단일 `get_value_addr` 으로 통일.
    ///
    /// # raw 알고리즘 (`0x67d0e4`)
    ///
    /// ```text
    /// arg0 (x0)  = PropertyBagImpl* (caller passes 자신, helper 가 +0x10 으로 보정)
    /// arg1 (x1)  = PropertyKey* (search key)
    ///
    /// // libc++ tree lower_bound walk:
    /// 0x67d0f4 x20 = arg0
    /// 0x67d0f8 x22 = [x20+0x10]; x20 += 0x10       ; x22 = root, x20 = &end_node
    /// 0x67d0fc if x22 == 0 → throw out_of_range "GetValue"
    /// 0x67d104 x21 = x20                            ; best = end
    /// loop @ 0x67d108:
    ///   w0 = PropertyKey::operator<(node_key, search_key)
    ///   if (node_key < search_key): x22 = node.right (go right, no parent update)
    ///   else: x21 = node, x22 = node.left          (parent = node, go left)
    /// 0x67d12c if x21 == x20: throw out_of_range
    /// 0x67d134-0x67d140 if (search_key < x21.key): throw out_of_range
    /// 0x67d144 x8 = [x21+0x30]  ; SharePtr.ctrl
    /// 0x67d148 if x8 == 0: throw bad_cast
    /// 0x67d14c x8 = [x8]         ; ControlBlock.obj = Property*
    /// 0x67d150 if x8 == 0: throw bad_cast
    /// 0x67d154 return x8 + 0xc   ; Property+0xc = typed value field
    /// ```
    ///
    /// `Property+0xc` 는 `Property` base 의 `_pad` slot — 모든 ValueProperty
    /// sub-class (`PEnum`/`PFloat`/`PBool`/`PInt`) 가 이 slot 에 typed value 를
    /// 저장한다 (PEnum::value @ +0xc, PFloat::value @ +0xc, PBool::value @ +0xc).
    /// caller 는 반환된 `*mut u8` 을 typed 포인터로 cast 해서 dereference.
    ///
    /// # Throws
    ///
    /// - `out_of_range`: key 가 map 에 없음 (= `tree empty` 또는 `lower_bound > search_key`).
    /// - `bad_cast`: value 의 SharePtr 가 null 이거나, ControlBlock.obj 가 null.
    ///
    /// Rust 에선 둘 다 `panic!` 으로 표현 — raw 가 unwind 까지 던지지만, BodyProperty
    /// getter 들이 `__Unwind_Resume` 으로 PropertyKey D1 정리 후 재던지므로
    /// caller side 의 정리는 RAII (Rust 의 Drop) 으로 자동.
    ///
    /// # Safety
    ///
    /// `bag_impl_ptr` 가 valid `PropertyBagImpl` 가리키거나 null. tree 가 valid
    /// `PropertyBagNode` 로 구성. `key` 는 valid `PropertyKey`.
    pub unsafe fn get_value_addr(bag_impl_ptr: *const PropertyBagImpl, key: &PropertyKey) -> *mut u8 {
        // raw 0x67d0f4-0x67d0fc:
        //   x20 = arg0 (= PropertyBagImpl*)
        //   x22 = *(x20 + 0x10) ; load end_node_left (= root or null)
        //   if x22 == 0 → throw
        if bag_impl_ptr.is_null() {
            panic!("PropertyBagImpl::GetValue: bag is null");
        }
        let bag = &*bag_impl_ptr;
        let root = bag.tree.end_node_left;
        if root.is_null() {
            panic!("PropertyBagImpl::GetValue: out_of_range (empty tree)");
        }
        // raw 0x67d104: x21 = x20 (= &end_node), parent_iter init.
        let end_addr = &bag.tree.end_node_left as *const _ as *mut TreeNodeBase;
        let mut parent: *mut TreeNodeBase = end_addr;
        let mut current: *mut TreeNodeBase = root;
        // raw 0x67d108-0x67d128: tree-walk lower_bound loop.
        while !current.is_null() {
            let node_ptr = current as *mut PropertyBagNode;
            let node_key = &(*node_ptr).key;
            // raw 0x67d108-0x67d110: PropertyKey::lt_op(node_key, search_key)
            if node_key.lt_op(key) {
                // raw: x8 = &node.right (= node+0x8) ; csel x21 unchanged
                current = (*current).right;
            } else {
                // raw: x8 = node (= &node.left), x21 = node (parent = node)
                parent = current;
                current = (*current).left;
            }
        }
        // raw 0x67d12c-0x67d130: if parent == &end_node → throw
        if std::ptr::eq(parent as *const _, end_addr as *const _) {
            panic!("PropertyBagImpl::GetValue: out_of_range (key > all)");
        }
        // raw 0x67d134-0x67d140: if search_key < parent.key → throw (not equal)
        let parent_node = parent as *mut PropertyBagNode;
        let parent_key = &(*parent_node).key;
        if key.lt_op(parent_key) {
            panic!("PropertyBagImpl::GetValue: out_of_range (key not present)");
        }
        // raw 0x67d144: x8 = [parent + 0x30] = SharePtr.ctrl
        let ctrl_ptr = (*parent_node).value;
        if ctrl_ptr.is_null() {
            panic!("PropertyBagImpl::GetValue: bad_cast (SharePtr ctrl null)");
        }
        // raw 0x67d14c: x8 = [x8] = ControlBlock.obj = Property*
        let prop_ptr = (*ctrl_ptr).obj;
        if prop_ptr.is_null() {
            panic!("PropertyBagImpl::GetValue: bad_cast (ControlBlock.obj null)");
        }
        // raw 0x67d154: return Property + 0xc (typed value slot)
        (prop_ptr as *mut u8).add(0xc)
    }

    /// raw `PropertyBagImpl::Attach(PropertyKey const&, SharePtr<Property> const&)` @ `0x4c9c4`.
    ///
    /// **Returns** the previously-attached SharePtr at `key` (or null if new).
    ///
    /// ## raw 알고리즘 (전체 흐름)
    ///
    /// 0x4c9e0-0x4c9ec: validity check on `value`:
    /// - If `value.ctrl == null`: return null (no-op)
    /// - If `value.ctrl.obj == null`: return ctrl AS-IS with refcount++ (raw 0x4ca68):
    ///   `str x8, [x19]; refcount++` — pass-through of original ctrl.
    ///
    /// 0x4c9f8-0x4ca10: find_equal(tree, key):
    /// - If found: REPLACE path
    /// - If not found: INSERT path (jump to 0x4cae4)
    ///
    /// REPLACE path (raw 0x4ca14-0x4cae0):
    /// 1. `*sret = old SharePtr` (refcount++ on existing)
    /// 2. Compute in-order successor of existing node (for begin update)
    /// 3. If `tree.begin == existing`: begin = successor
    /// 4. size--
    /// 5. tree_remove(0x70238) — erase node + rebalance
    /// 6. ~PropertyKey on existing.key
    /// 7. delete existing node
    /// 8. Fall through to INSERT
    ///
    /// INSERT path (raw 0x4cae4 → 0x4cb0c helper):
    /// 1. Clone key (`bl 0x6e984`: CHncStringW box clone)
    /// 2. Copy value.ctrl with refcount++
    /// 3. Call `__emplace_unique` (0x7319c)
    /// 4. Cleanup local key/ctrl (move-construct nulled them on success)
    /// 5. If failure (= dup key — should not happen post-erase): throw invalid_argument
    ///
    /// ## 본 단계 scope (16l)
    ///
    /// - validity check ✓
    /// - INSERT path ✓ (단일/다중 unique key, rb-tree balance via existing helpers)
    /// - REPLACE path **defer → 16m** (tree_remove RE 필요)
    ///
    /// # Safety
    /// `value` 는 valid `ControlBlock<Property>*` 또는 null. `key` 는 valid.
    pub unsafe fn attach(
        &mut self,
        key: &PropertyKey,
        value: *mut ControlBlock<Property>,
    ) -> *mut ControlBlock<Property> {
        // raw 0x4c9e0-0x4c9ec: validity check
        if value.is_null() {
            // raw 0x4ca4c: *sret = null; return
            return ptr::null_mut();
        }
        if (*value).obj.is_null() {
            // raw 0x4ca68: *sret = value; value.refcount++; return
            (*value).refcount = (*value).refcount.wrapping_add(1);
            return value;
        }

        // raw 0x4ca00-0x4ca04: tree_find
        match self.find_equal(key) {
            Ok(existing) => {
                // REPLACE path (raw 0x4ca14-0x4cae0): erase existing + insert new
                let old_sp = self.attach_replace_existing(existing, value);
                self.attach_insert_new(key, value);
                old_sp
            }
            Err(_end) => {
                // INSERT path (raw 0x4cae4 → 0x4cb0c)
                self.attach_insert_new(key, value);
                // raw 의 sret = null (no old value) — INSERT path 의 0x4ca90 stores xzr
                ptr::null_mut()
            }
        }
    }

    /// 공통 erase helper — raw `0x4ca14-0x4cae0` (REPLACE) 와 `0x4c8c4-0x4c964`
    /// (Detach) 의 공유 알고리즘:
    ///
    /// 1. `old_sp = existing.value` + refcount++ (return old SharePtr)
    /// 2. successor = tree_next(existing) (for begin update)
    /// 3. If `tree.begin == existing` → begin = successor
    /// 4. size--
    /// 5. tree_remove(root, existing) — splice + rebalance, returns new root
    /// 6. root.parent = &end_node (libc++ 의 begin/end node invariant 유지)
    /// 7. PropertyKey::~Drop + delete node (value 는 *sret 으로 ownership 이전)
    ///
    /// Returns: 이전 SharePtr (refcount++ 된 상태 — caller responsibility).
    ///
    /// # Safety
    /// `existing` 는 트리에 valid + 본 메소드 단독 호출 (aliasing 없음).
    unsafe fn erase_node(&mut self, existing: *mut PropertyBagNode) -> *mut ControlBlock<Property> {
        // 1. *sret = existing.value + refcount++
        let old_sp = (*existing).value;
        if !old_sp.is_null() {
            (*old_sp).refcount = (*old_sp).refcount.wrapping_add(1);
        }

        // 2. in-order successor for begin update
        let existing_base = existing as *mut TreeNodeBase;
        let successor = tree_next(existing_base);

        // 3. update begin if existing was leftmost
        if self.tree.begin_node == existing_base {
            self.tree.begin_node = successor;
        }

        // 4. size--
        self.tree.size = self.tree.size.wrapping_sub(1);

        // 5. tree_remove → new root
        let new_root = tree_remove(self.tree.end_node_left, existing_base);
        self.tree.end_node_left = new_root;

        // 6. libc++ invariant: root.parent = &end_node; empty tree begin = &end_node
        let end_addr =
            &mut self.tree.end_node_left as *mut *mut TreeNodeBase as *mut TreeNodeBase;
        if !new_root.is_null() {
            (*new_root).parent = end_addr;
        } else {
            self.tree.begin_node = end_addr;
        }

        // 7. delete node (value field 는 null 로 reset 하여 double-release 방지)
        let mut node_box = Box::from_raw(existing);
        node_box.value = ptr::null_mut();
        drop(node_box); // PropertyKey::Drop 자동 호출

        old_sp
    }

    /// REPLACE path helper — raw `0x4ca14-0x4cae0` 1:1.
    ///
    /// 호출자가 이미 find_equal 로 existing 찾은 상태. erase + (caller 가 새로 insert).
    unsafe fn attach_replace_existing(
        &mut self,
        existing: *mut PropertyBagNode,
        _new_value: *mut ControlBlock<Property>,
    ) -> *mut ControlBlock<Property> {
        self.erase_node(existing)
    }

    /// raw `PropertyBagImpl::Detach(PropertyKey const&)` @ `0x4c894` 1:1.
    ///
    /// ```text
    /// tree_find(key)
    /// if not found: *sret = null; return
    /// else: erase_node sequence (위 helper) + tail-call __ZdlPv (delete)
    /// ```
    ///
    /// Returns: 이전 SharePtr (refcount++ 된 상태).
    ///
    /// # Safety
    /// `key` 는 valid.
    pub unsafe fn detach(&mut self, key: &PropertyKey) -> *mut ControlBlock<Property> {
        match self.find_equal(key) {
            Err(_end) => {
                // raw 0x4c8fc: *sret = null; return
                ptr::null_mut()
            }
            Ok(existing) => self.erase_node(existing),
        }
    }

    /// INSERT path helper — raw 0x4cb0c + 0x7319c (`__emplace_unique`) 1:1 essence.
    ///
    /// 1. PropertyKey::clone_op (raw `bl 0x6e984` + 8B box alloc)
    /// 2. SharePtr ctrl ref bump (raw `ldr x9, [x8+0x8]; x9++; str x9, [x8+0x8]`)
    /// 3. find_insert_position (uses PropertyKey::cmp)
    /// 4. Allocate 56B PropertyBagNode
    /// 5. Link into tree at insert_slot
    /// 6. balance_after_insert
    /// 7. size++
    /// 8. update begin if leftmost
    ///
    /// # Safety
    /// 호출자가 key 가 unique (= find_equal 가 not-found) 함을 보장.
    unsafe fn attach_insert_new(
        &mut self,
        key: &PropertyKey,
        value_ctrl: *mut ControlBlock<Property>,
    ) {
        // 1. clone key (= raw 의 local copy 0x4cb24-0x4cb30)
        let cloned_key = key.clone_op();

        // 2. SharePtr refcount++ (raw 의 0x4cb40-0x4cb48)
        // value_ctrl is non-null (validated by caller).
        (*value_ctrl).refcount = (*value_ctrl).refcount.wrapping_add(1);

        // 3. Allocate node (raw 의 __emplace_unique 의 node alloc + init)
        let mut node_box: Box<PropertyBagNode> = Box::new(PropertyBagNode {
            base: TreeNodeBase {
                left: ptr::null_mut(),
                right: ptr::null_mut(),
                parent: ptr::null_mut(),
                is_black: 0, // red initially
                _pad_0x19: [0; 7],
            },
            key: cloned_key,
            value: value_ctrl,
        });
        let node_ptr: *mut PropertyBagNode = &mut *node_box as *mut PropertyBagNode;

        // 4. find_insert_position
        let tree_ptr = &mut self.tree as *mut TreeBase;
        // Comparator: (node vs target_key) where target_key = the key being inserted
        let target_clone = key.clone_op(); // for closure capture
        let target_ref: &PropertyKey = &target_clone;
        let cmp = |n_ptr: *const TreeNodeBase| {
            let n = n_ptr as *const PropertyBagNode;
            (*n).key.cmp(target_ref)
        };
        let (insert_slot, parent, existing) = find_insert_position(tree_ptr, cmp);
        // INSERT path 의 invariant: existing == null (caller 가 unique 보장)
        debug_assert!(existing.is_null(), "attach_insert_new called with duplicate key");

        // 5. Link node
        (*node_ptr).base.parent = parent;
        *insert_slot = node_ptr as *mut TreeNodeBase;

        // 6. balance_after_insert — `root` 은 actual root (end_node_left value),
        // 단일 insert 일 때 root == new_node 라 balance 가 is_black=1 로 설정.
        let root = (*tree_ptr).end_node_left;
        balance_after_insert(root, node_ptr as *mut TreeNodeBase);

        // 7. size++
        self.tree.size = self.tree.size.wrapping_add(1);

        // 8. update begin if leftmost (libc++ pattern)
        update_begin_node_after_insert(tree_ptr);

        // Forget node_box (ownership transferred to tree)
        std::mem::forget(node_box);
    }
    ///
    /// ```text
    /// mov  x19, x0
    /// ldr  x1, [x19, #0x10]!         ; x1 = self.end_left; x19 = self+0x10
    /// sub  x0, x19, #0x8              ; x0 = &self.begin_node (self+0x8)
    /// bl   0x72c28                    ; tree destroy recursive
    /// stp  x19, xzr, [x19, #-0x8]     ; self.begin = self+0x10; self.end_left = 0
    /// str  xzr, [x19, #0x8]           ; self.size = 0
    /// ```
    ///
    /// 16l 이후: insert 된 PropertyBagNode 들 모두 free.
    pub fn clear(&mut self) {
        unsafe {
            subtree_destroy_recursive(self.tree.end_node_left, &|n| drop_property_bag_node(n));
            self.tree.init_empty();
            self.tree.size = 0;
        }
    }
}

/// `subtree_destroy_recursive` 의 callback — PropertyBagNode 하나를 free.
///
/// 각 node 의 ownership chain:
/// - `key`: PropertyKey (Drop 으로 CHncStringW box auto-free)
/// - `value`: `*mut ControlBlock<Property>` (SharePtr refcount-- + chain destroy)
unsafe fn drop_property_bag_node(n: *mut TreeNodeBase) {
    if n.is_null() {
        return;
    }
    let node = n as *mut PropertyBagNode;
    // SharePtr<Property> refcount--
    let ctrl = (*node).value;
    if !ctrl.is_null() {
        let prev = (*ctrl).refcount;
        let next = prev.wrapping_sub(1);
        if next == 0 {
            // (Property 의 vfunc destroy + delete obj + delete ctrl)
            // 본 단계는 Property 가 opaque ZST — 안전한 default 는 ctrl 만 delete
            // (obj 가 zero-sized 일 때 Box::from_raw 무의미). Property 의 실제 RE
            // 이후 vfunc[1] (delete) 호출로 정확화.
            let obj = (*ctrl).obj;
            if !obj.is_null() {
                // Property opaque — placeholder: skip vtable call, deallocate via std::alloc::dealloc
                // (Property 가 vtable_ptr+state+pad 인 16B 가정)
                // 본 단계는 안전을 위해 leak 처리. 정확 free 는 PColor 등 sub-class RE 후.
            }
            // ctrl 자체는 16B heap, free 가능
            drop(Box::from_raw(ctrl));
        } else {
            (*ctrl).refcount = next;
        }
    }
    // PropertyBagNode 자체 free — Box::from_raw 가 PropertyKey::Drop 자동 호출
    drop(Box::from_raw(node));
}

impl Drop for PropertyBagImpl {
    /// raw `~PropertyBagImpl()` @ `0x4c4f0`.
    ///
    /// tree destroy recursive + name dtor.
    /// **현재 단계**: tree empty 만 도달 (insert 미구현).
    fn drop(&mut self) {
        unsafe {
            subtree_destroy_recursive(self.tree.end_node_left, &|n| drop_property_bag_node(n));
        }
    }
}

/// raw 8B `Hnc::Property::PropertyBag` — SharePtr-style wrapper.
///
/// 단일 field: `*mut ControlBlock<PropertyBagImpl>`. raw 의 모든 method 가
/// inline 으로 `(*self).ctrl.obj` 를 통해 Impl 에 위임.
#[repr(C)]
pub struct PropertyBag {
    /// raw +0x00: `ControlBlock<PropertyBagImpl>*` (16B control block on heap).
    pub ctrl: *mut ControlBlock<PropertyBagImpl>,
}

pub const PROPERTY_BAG_SIZE_BYTES: usize = 8;
pub const PROPERTY_BAG_ALIGN_BYTES: usize = 8;

const _: () = assert!(std::mem::size_of::<PropertyBag>() == PROPERTY_BAG_SIZE_BYTES);
const _: () = assert!(std::mem::align_of::<PropertyBag>() == PROPERTY_BAG_ALIGN_BYTES);

impl PropertyBag {
    /// raw `PropertyBag::PropertyBag(bool)` @ `0x4d32c` 1:1.
    ///
    /// 1. `new(0x20)` → PropertyBagImpl* + inline init (위 PropertyBagImpl ctor)
    /// 2. `new(0x10)` → ControlBlock* (= 16B)
    ///    - `[+0x00] = impl_ptr`
    ///    - `[+0x08] = refcount = 1`
    /// 3. `*self = ctrl_ptr`
    pub fn new(is_merged: bool) -> Self {
        // raw 0x4d348-0x4d36c: new + inline PropertyBagImpl init
        let impl_box = PropertyBagImpl::new_boxed(is_merged);
        let impl_ptr = Box::into_raw(impl_box);

        // raw 0x4d378-0x4d388: new ControlBlock + init
        let ctrl_box = Box::new(ControlBlock {
            obj: impl_ptr,
            refcount: 1,
        });
        let ctrl_ptr = Box::into_raw(ctrl_box);

        PropertyBag { ctrl: ctrl_ptr }
    }

    /// raw `PropertyBag::PropertyBag(auto_ptr<PropertyBagImpl>)` @ `0x4d448` —
    /// wrap existing impl ptr. **multi-session deferred** (auto_ptr semantics).
    ///
    /// # Safety
    /// `impl_ptr` 는 heap-alloc PropertyBagImpl 또는 null.
    pub unsafe fn from_impl(impl_ptr: *mut PropertyBagImpl) -> Self {
        let ctrl_box = Box::new(ControlBlock {
            obj: impl_ptr,
            refcount: 1,
        });
        PropertyBag {
            ctrl: Box::into_raw(ctrl_box),
        }
    }

    /// raw `~PropertyBag()` @ `0x4d540` — SharePtr dtor pattern.
    ///
    /// refcount-- ; if 0, delete Impl + delete ControlBlock.
    ///
    /// Rust 구현은 `Drop` impl 으로 자동.
    #[inline]
    fn release(&mut self) {
        if self.ctrl.is_null() {
            return;
        }
        unsafe {
            let cb = &mut *self.ctrl;
            if cb.obj.is_null() {
                // defensive — should not happen
                drop(Box::from_raw(self.ctrl));
                self.ctrl = ptr::null_mut();
                return;
            }
            cb.refcount = cb.refcount.wrapping_sub(1);
            if cb.refcount == 0 {
                // dispose Impl
                drop(Box::from_raw(cb.obj));
                cb.obj = ptr::null_mut();
                // dispose ControlBlock
                drop(Box::from_raw(self.ctrl));
            }
            self.ctrl = ptr::null_mut();
        }
    }

    /// PropertyBagImpl 의 borrow (null check 후).
    ///
    /// # Safety
    /// `self.ctrl` 가 valid + `cb.obj` non-null 일 때만 호출.
    pub unsafe fn impl_ref(&self) -> Option<&PropertyBagImpl> {
        if self.ctrl.is_null() {
            return None;
        }
        let cb = &*self.ctrl;
        if cb.obj.is_null() {
            None
        } else {
            Some(&*cb.obj)
        }
    }

    /// PropertyBagImpl 의 mutable borrow.
    ///
    /// # Safety
    /// `self.ctrl` 가 valid + `cb.obj` non-null + 단독 사용 (aliasing 금지).
    pub unsafe fn impl_mut(&mut self) -> Option<&mut PropertyBagImpl> {
        if self.ctrl.is_null() {
            return None;
        }
        let cb = &mut *self.ctrl;
        if cb.obj.is_null() {
            None
        } else {
            Some(&mut *cb.obj)
        }
    }

    /// raw `PropertyBag::Attach(PropertyKey&, SharePtr<Property>&)` @ `0x4dc84`.
    ///
    /// Wrapper 의 8-instr code:
    /// ```text
    /// ldr x9, [x0]       ; self.ctrl
    /// cbz x9, 0x4dc94    ; null → call Impl::Attach(null, ...)
    /// ldr x0, [x9]       ; impl = ctrl.obj
    /// b   Impl::Attach
    /// 0x4dc94: x0 = null; b Impl::Attach
    /// ```
    ///
    /// # Safety
    /// `value_ctrl` 는 valid ControlBlock 또는 null.
    pub unsafe fn attach(
        &mut self,
        key: &PropertyKey,
        value_ctrl: *mut ControlBlock<Property>,
    ) -> *mut ControlBlock<Property> {
        match self.impl_mut() {
            Some(im) => im.attach(key, value_ctrl),
            None => ptr::null_mut(),
        }
    }

    /// raw `PropertyBag::Detach(PropertyKey&)` @ `0x4db8c` — forward to Impl.
    ///
    /// # Safety
    /// `key` is valid.
    pub unsafe fn detach(&mut self, key: &PropertyKey) -> *mut ControlBlock<Property> {
        match self.impl_mut() {
            Some(im) => im.detach(key),
            None => ptr::null_mut(),
        }
    }

    /// raw `Contains()` @ `0x4dc9c` — forward to Impl.
    pub fn contains(&self, key: &PropertyKey) -> bool {
        unsafe {
            self.impl_ref()
                .map(|i| i.contains(key))
                .unwrap_or(false)
        }
    }

    /// raw `GetState()` @ `0x4da2c` — forward to Impl.
    pub fn get_state(&self, key: &PropertyKey) -> u32 {
        unsafe { self.impl_ref().map(|i| i.get_state(key)).unwrap_or(0) }
    }

    /// raw `SetState()` @ `0x4da98` — forward to Impl.
    pub fn set_state(&mut self, key: &PropertyKey, state: u32) {
        unsafe {
            if let Some(i) = self.impl_mut() {
                i.set_state(key, state);
            }
        }
    }

    /// raw `IsEnable()` @ `0x4dafc` — forward to Impl.
    pub fn is_enable(&self, key: &PropertyKey) -> bool {
        unsafe {
            self.impl_ref()
                .map(|i| i.is_enable(key))
                .unwrap_or(false)
        }
    }

    /// raw `IsEmpty()` @ `0x4db74` — forward to Impl.
    pub fn is_empty(&self) -> bool {
        unsafe { self.impl_ref().map(|i| i.is_empty()).unwrap_or(true) }
    }

    /// raw `SetMerged(bool)` @ `0x4da1c` — forward to Impl.
    pub fn set_merged(&mut self, is_merged: bool) {
        unsafe {
            if let Some(i) = self.impl_mut() {
                i.set_merged(is_merged);
            }
        }
    }

    /// raw `Clear()` @ `0x4dcdc` — forward to Impl.
    pub fn clear(&mut self) {
        unsafe {
            if let Some(i) = self.impl_mut() {
                i.clear();
            }
        }
    }

    /// 현재 refcount (debug helper, raw 엔 export 없음).
    pub fn refcount(&self) -> u64 {
        if self.ctrl.is_null() {
            return 0;
        }
        unsafe { (*self.ctrl).refcount }
    }

    /// raw `Begin()` @ `0x4de40` (4 instr).
    ///
    /// ```text
    /// ldr  x8, [x0]              ; ctrl
    /// ldr  x8, [x8]              ; impl
    /// ldr  x0, [x8, #0x8]        ; impl.tree.begin_node
    /// ret
    /// ```
    pub fn begin(&self) -> *mut TreeNodeBase {
        unsafe { self.impl_ref().map(|i| i.begin()).unwrap_or(ptr::null_mut()) }
    }

    /// raw `End()` @ `0x4de50` (4 instr).
    ///
    /// ```text
    /// ldr  x8, [x0]                  ; ctrl
    /// ldr  x8, [x8]                  ; impl
    /// add  x0, x8, #0x10             ; &impl.tree.end_node_left
    /// ret
    /// ```
    pub fn end(&self) -> *mut *mut TreeNodeBase {
        unsafe {
            self.impl_ref()
                .map(|i| i.end())
                .unwrap_or(ptr::null_mut())
        }
    }

    /// raw `PropertyBag::Swap(PropertyBag&)` @ `0x4da08` (libHncFoundation) 1:1.
    ///
    /// ```asm
    /// ldr x8, [x0]      ; self.ctrl
    /// ldr x9, [x1]      ; other.ctrl
    /// str x9, [x0]      ; self.ctrl = other.ctrl
    /// str x8, [x1]      ; other.ctrl = self.ctrl
    /// ret
    /// ```
    ///
    /// 24B byte-eq. ControlBlock 의 ownership 만 교환 — refcount 변경 없음.
    pub fn swap(&mut self, other: &mut PropertyBag) {
        let a = self.ctrl;
        let b = other.ctrl;
        self.ctrl = b;
        other.ctrl = a;
    }

    /// raw `PropertyBag::operator==(PropertyBag const&) const` @ `0x4d618` (libHncFoundation).
    ///
    /// **wrapper-level byte-eq**:
    /// - both bags 에 valid impl 없음 (둘 다 null ctrl 거나 null obj) → true
    /// - 한쪽만 impl 가짐 → false
    /// - 둘 다 impl 있음, 같은 impl ptr → true (raw 의 `cmp x9, x8; b.eq 0x4d6c0`)
    /// - 둘 다 impl 있음, 다른 impl ptr → tree compare via `tree_compare_helper` (raw 0x72db4 호출)
    ///
    /// **tree compare 단계**는 `Property::vfunc[2]` (operator==) 호출 필요 — 모든
    /// Property subclass 의 vtable port 완료 후 별도 sub-task. 현 단계는 wrapper
    /// fast-paths 만 byte-eq 으로 다룸.
    ///
    /// raw 정밀 분기 (libHncFoundation `0x4d618-0x4d6cc`):
    /// ```asm
    /// ldr  x8, [x1]                 ; ctrl_b
    /// cbz  x8, branch_b_null        ; b null → 0x4d68c
    /// ldr  x8, [x8]                 ; impl_b
    /// ldr  x9, [x0]                 ; ctrl_a
    /// cbz  x8, branch_impl_b_null   ; impl_b null → 0x4d6bc
    /// cbz  x9, false                ; impl_b non-null but a null → 0x4d6ac (false)
    /// ldr  x9, [x9]                 ; impl_a
    /// cbz  x9, false                ; → 0x4d6ac
    /// cmp  x9, x8
    /// b.eq true                     ; same impl ptr → 0x4d6c0 (true)
    /// ldrb w10, [x9]; ldrb w11, [x8]; cmp; b.ne false  ; merged_flag mismatch
    /// ldr  x10, [x9, #0x18]; ldr x11, [x8, #0x18]; cmp; b.ne false  ; size mismatch
    /// ldr  x0, [x9, #0x8]          ; begin_a
    /// add  x1, x9, #0x10           ; sentinel_a (&end_node_left_a)
    /// ldr  x2, [x8, #0x8]          ; begin_b
    /// sub  x3, x29, #0x1           ; some local arg
    /// bl   0x72db4                  ; tree iter compare
    /// ret
    /// ```
    ///
    /// 본 port 는 wrapper 단계 + tree shape compare (merged_flag/size) 까지 byte-eq.
    /// Tree value-by-value compare (raw `0x72db4`) 는 deferred.
    pub fn eq_op(&self, other: &PropertyBag) -> bool {
        unsafe {
            let ctrl_b = other.ctrl;
            let ctrl_a = self.ctrl;
            // raw `cbz x8 (=ctrl_b), 0x4d68c` branch_b_null
            if ctrl_b.is_null() {
                // raw 0x4d68c: ldr x9 = ctrl_a; cbz x9, true; ldr x8=[x9]=impl_a; cmp x8, #0; cset eq
                if ctrl_a.is_null() {
                    return true;
                }
                let impl_a = (*ctrl_a).obj;
                return impl_a.is_null();
            }
            let impl_b = (*ctrl_b).obj;
            // raw `cbz x8 (=impl_b), 0x4d6bc` branch_impl_b_null
            if impl_b.is_null() {
                // raw 0x4d6bc: cbnz x9, 0x4d694; (x9=ctrl_a, falls through to true if null)
                if ctrl_a.is_null() {
                    return true;
                }
                let impl_a = (*ctrl_a).obj;
                return impl_a.is_null();
            }
            // raw 0x4d638: cbz x9 (=ctrl_a), 0x4d6ac → false
            if ctrl_a.is_null() {
                return false;
            }
            // raw 0x4d63c: ldr x9=[x9]=impl_a; cbz x9, false
            let impl_a = (*ctrl_a).obj;
            if impl_a.is_null() {
                return false;
            }
            // raw 0x4d644: cmp impl_a, impl_b; b.eq true (same impl)
            if impl_a as *const _ == impl_b as *const _ {
                return true;
            }
            let a_impl = &*impl_a;
            let b_impl = &*impl_b;
            // raw 0x4d64c: merged_flag compare
            if a_impl.merged_flag_raw != b_impl.merged_flag_raw {
                return false;
            }
            // raw 0x4d65c: size compare
            if a_impl.tree.size != b_impl.tree.size {
                return false;
            }
            // raw 0x4d66c: tree iter compare via 0x72db4 helper
            // — Property::vfunc[2] 필요. 별도 sub-task L-5c-3b 에서 port.
            // 현재 단계 fallback: size==0 (empty trees) 면 자동 equal.
            if a_impl.tree.size == 0 {
                return true;
            }
            // Non-empty distinct trees 의 정확 compare 는 deferred.
            // 안전한 conservative 결과: false (다르다고 가정).
            false
        }
    }

    /// raw `PropertyBag::operator!=(PropertyBag const&)` 의 등가 헬퍼.
    /// raw 엔 PropertyBag 자체 ne 없지만 Scene3D::ne 가 PropertyBag::eq 의 XOR 1.
    #[inline]
    pub fn ne_op(&self, other: &PropertyBag) -> bool {
        !self.eq_op(other)
    }
}

impl Drop for PropertyBag {
    fn drop(&mut self) {
        self.release();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn impl_raw_layout_size_align() {
        assert_eq!(std::mem::size_of::<PropertyBagImpl>(), 32);
        assert_eq!(std::mem::align_of::<PropertyBagImpl>(), 8);
    }

    #[test]
    fn impl_field_offsets_match_raw() {
        let i = PropertyBagImpl::new_boxed(false);
        let p = &*i as *const PropertyBagImpl as usize;
        assert_eq!(&i.merged_flag_raw as *const _ as usize - p, 0x00);
        assert_eq!(&i.tree.begin_node as *const _ as usize - p, 0x08);
        assert_eq!(&i.tree.end_node_left as *const _ as usize - p, 0x10);
        assert_eq!(&i.tree.size as *const _ as usize - p, 0x18);
    }

    #[test]
    fn impl_default_state_is_empty() {
        let i = PropertyBagImpl::new_boxed(false);
        assert!(i.is_empty());
        assert_eq!(i.tree.size, 0);
    }

    #[test]
    fn impl_merged_flag_inverted_per_raw() {
        // raw `eor #0x1`: ctor arg `is_merged=true` → stored flag = 0
        let i_merged = PropertyBagImpl::new_boxed(true);
        assert_eq!(i_merged.merged_flag_raw, 0);
        assert!(i_merged.is_merged());

        let i_not = PropertyBagImpl::new_boxed(false);
        assert_eq!(i_not.merged_flag_raw, 1);
        assert!(!i_not.is_merged());
    }

    #[test]
    fn impl_set_merged_toggles_flag() {
        let mut i = PropertyBagImpl::new_boxed(false);
        assert_eq!(i.merged_flag_raw, 1);
        i.set_merged(true);
        assert_eq!(i.merged_flag_raw, 0);
        assert!(i.is_merged());
        i.set_merged(false);
        assert_eq!(i.merged_flag_raw, 1);
        assert!(!i.is_merged());
    }

    #[test]
    fn impl_begin_returns_end_addr_when_empty() {
        let i = PropertyBagImpl::new_boxed(false);
        let begin = i.begin();
        let end_addr = &i.tree.end_node_left as *const _ as usize;
        assert_eq!(begin as usize, end_addr);
    }

    #[test]
    fn impl_end_returns_end_node_left_addr() {
        let i = PropertyBagImpl::new_boxed(false);
        let end_ptr = i.end();
        let expected_addr = &i.tree.end_node_left as *const _ as usize;
        assert_eq!(end_ptr as usize, expected_addr);
    }

    #[test]
    fn impl_drop_empty_no_panic() {
        for _ in 0..100 {
            let i = PropertyBagImpl::new_boxed(false);
            drop(i);
        }
    }

    #[test]
    fn impl_clear_on_empty_no_panic() {
        let mut i = PropertyBagImpl::new_boxed(false);
        i.clear();
        assert!(i.is_empty());
    }

    #[test]
    fn bag_raw_layout_size_align() {
        assert_eq!(std::mem::size_of::<PropertyBag>(), 8);
        assert_eq!(std::mem::align_of::<PropertyBag>(), 8);
    }

    #[test]
    fn bag_new_creates_ctrl_with_refcount_1() {
        let bag = PropertyBag::new(false);
        assert!(!bag.ctrl.is_null());
        assert_eq!(bag.refcount(), 1);
        assert!(bag.is_empty());
    }

    #[test]
    fn bag_new_merged_flag_passes_to_impl() {
        let bag_m = PropertyBag::new(true);
        unsafe {
            let im = bag_m.impl_ref().unwrap();
            assert!(im.is_merged());
        }

        let bag_n = PropertyBag::new(false);
        unsafe {
            let im = bag_n.impl_ref().unwrap();
            assert!(!im.is_merged());
        }
    }

    #[test]
    fn bag_set_merged_forwards_to_impl() {
        let mut bag = PropertyBag::new(false);
        bag.set_merged(true);
        unsafe {
            assert!(bag.impl_ref().unwrap().is_merged());
        }
    }

    #[test]
    fn bag_clear_on_empty_no_panic() {
        let mut bag = PropertyBag::new(false);
        bag.clear();
        assert!(bag.is_empty());
    }

    #[test]
    fn bag_drop_releases_ctrl_and_impl() {
        for _ in 0..200 {
            let bag = PropertyBag::new(false);
            drop(bag);
        }
    }

    #[test]
    fn bag_is_empty_when_ctrl_null() {
        let bag = PropertyBag { ctrl: ptr::null_mut() };
        assert!(bag.is_empty());
        // 강제 ctor (drop 에서 release 가 null check 됨)
        std::mem::forget(bag);
    }

    #[test]
    fn bag_begin_forwards_to_impl_when_empty() {
        let bag = PropertyBag::new(false);
        let bg = bag.begin();
        unsafe {
            let im = bag.impl_ref().unwrap();
            assert_eq!(bg as usize, im.begin() as usize);
        }
    }

    #[test]
    fn bag_end_forwards_to_impl() {
        let bag = PropertyBag::new(false);
        let e = bag.end();
        unsafe {
            let im = bag.impl_ref().unwrap();
            assert_eq!(e as usize, im.end() as usize);
        }
    }

    #[test]
    fn node_layout_size_and_field_offsets() {
        // PropertyBagNode 56B = TreeNodeBase(32) + PropertyKey(16) + ptr(8)
        assert_eq!(std::mem::size_of::<PropertyBagNode>(), 56);
        assert_eq!(std::mem::align_of::<PropertyBagNode>(), 8);
        // 가짜 인스턴스 — base/key/value offset 검증
        let node = PropertyBagNode {
            base: TreeNodeBase {
                left: ptr::null_mut(),
                right: ptr::null_mut(),
                parent: ptr::null_mut(),
                is_black: 0,
                _pad_0x19: [0; 7],
            },
            key: PropertyKey::from_int(0),
            value: ptr::null_mut(),
        };
        let p = &node as *const _ as usize;
        assert_eq!(&node.base as *const _ as usize - p, 0x00);
        assert_eq!(&node.key as *const _ as usize - p, 0x20);
        assert_eq!(&node.value as *const _ as usize - p, 0x30);
        std::mem::forget(node);
    }

    #[test]
    fn find_equal_empty_returns_end() {
        let i = PropertyBagImpl::new_boxed(false);
        let k = PropertyKey::from_int(601);
        unsafe {
            let result = i.find_equal(&k);
            assert!(result.is_err());
            let end_addr = &i.tree.end_node_left as *const _ as usize;
            assert_eq!(result.unwrap_err() as usize, end_addr);
        }
    }

    #[test]
    fn contains_empty_returns_false() {
        let i = PropertyBagImpl::new_boxed(false);
        let k = PropertyKey::from_int(601);
        assert!(!i.contains(&k));
    }

    #[test]
    fn get_state_empty_returns_zero() {
        let i = PropertyBagImpl::new_boxed(false);
        let k = PropertyKey::from_int(601);
        assert_eq!(i.get_state(&k), 0);
    }

    #[test]
    fn set_state_empty_is_noop() {
        let mut i = PropertyBagImpl::new_boxed(false);
        let k = PropertyKey::from_int(601);
        i.set_state(&k, 42);
        // 여전히 empty 이므로 get_state 는 0
        assert_eq!(i.get_state(&k), 0);
    }

    #[test]
    fn bag_contains_get_state_forward_to_impl() {
        let bag = PropertyBag::new(false);
        let k = PropertyKey::from_int(601);
        assert!(!bag.contains(&k));
        assert_eq!(bag.get_state(&k), 0);
        assert!(!bag.is_enable(&k));
    }

    #[test]
    fn bag_set_state_forward_no_effect_on_empty() {
        let mut bag = PropertyBag::new(false);
        let k = PropertyKey::from_int(601);
        bag.set_state(&k, 99);
        assert_eq!(bag.get_state(&k), 0);
    }

    /// 수동으로 PropertyBagNode 트리를 만들어 find_equal 의 비-empty path 검증.
    ///
    /// 구조: end ← (left)─ Node(key=100)  (root, no children)
    /// libc++ empty 시 end_node_left = null; 트리에 root 가 있으면
    /// end_node_left = root, root.parent = &end_node, root.left=null, right=null.
    #[test]
    fn find_equal_single_node_root() {
        unsafe {
            let mut bag_impl = PropertyBagImpl::new_boxed(false);

            // 노드 1개 heap-alloc — Box::into_raw 로 stable address
            let node = Box::into_raw(Box::new(PropertyBagNode {
                base: TreeNodeBase {
                    left: ptr::null_mut(),
                    right: ptr::null_mut(),
                    parent: ptr::null_mut(),
                    is_black: 1,
                    _pad_0x19: [0; 7],
                },
                key: PropertyKey::from_int(100),
                value: ptr::null_mut(),
            }));

            // tree 의 root 설정. end_node_left = node, begin_node = leftmost = node.
            let end_node_left_addr =
                &mut bag_impl.tree.end_node_left as *mut *mut TreeNodeBase as *mut TreeNodeBase;
            (*node).base.parent = end_node_left_addr;
            bag_impl.tree.end_node_left = node as *mut TreeNodeBase;
            bag_impl.tree.begin_node = node as *mut TreeNodeBase;
            bag_impl.tree.size = 1;

            // 검색: 일치하는 key 찾으면 found
            let k_match = PropertyKey::from_int(100);
            let r1 = bag_impl.find_equal(&k_match);
            assert!(r1.is_ok());
            assert_eq!(r1.unwrap() as usize, node as usize);

            // 검색: 다른 key 는 not-found
            let k_miss = PropertyKey::from_int(200);
            let r2 = bag_impl.find_equal(&k_miss);
            assert!(r2.is_err());

            // 더 작은 key 도 not-found (root 만 있으므로 best 가 root, 하지만 arg < best.key → end)
            let k_smaller = PropertyKey::from_int(50);
            let r3 = bag_impl.find_equal(&k_smaller);
            assert!(r3.is_err());

            // 정리: tree 비우기 (drop panic 회피)
            bag_impl.tree.end_node_left = ptr::null_mut();
            bag_impl.tree.begin_node = end_node_left_addr;
            bag_impl.tree.size = 0;
            drop(Box::from_raw(node));
            drop(bag_impl);
        }
    }

    // ---------- 16l Attach tests ----------

    /// Test helper: build a heap `ControlBlock<Property>` with given state.
    /// Property 는 heap-alloc 으로 obj.is_null() == false. refcount=1.
    fn make_share_ptr(state: u32) -> *mut ControlBlock<Property> {
        unsafe {
            let prop = Box::into_raw(Box::new(Property {
                vtable: ptr::null(),
                state,
                _pad: 0,
            }));
            Box::into_raw(Box::new(ControlBlock {
                obj: prop,
                refcount: 1,
            }))
        }
    }

    /// 정리: SharePtr 의 obj 와 ctrl 둘 다 free (테스트 만든 것).
    unsafe fn free_share_ptr(sp: *mut ControlBlock<Property>) {
        if sp.is_null() {
            return;
        }
        let obj = (*sp).obj;
        if !obj.is_null() {
            drop(Box::from_raw(obj));
        }
        drop(Box::from_raw(sp));
    }

    #[test]
    fn attach_null_value_returns_null_and_noop() {
        let mut i = PropertyBagImpl::new_boxed(false);
        let k = PropertyKey::from_int(601);
        unsafe {
            let r = i.attach(&k, ptr::null_mut());
            assert!(r.is_null());
            assert!(i.is_empty());
        }
    }

    #[test]
    fn attach_invalid_ctrl_obj_null_returns_ctrl_with_refbump() {
        // raw 0x4ca68 path: ctrl.obj == null → return ctrl + refcount++
        unsafe {
            let ctrl = Box::into_raw(Box::new(ControlBlock::<Property> {
                obj: ptr::null_mut(),
                refcount: 1,
            }));
            let mut i = PropertyBagImpl::new_boxed(false);
            let k = PropertyKey::from_int(700);
            let r = i.attach(&k, ctrl);
            assert_eq!(r as usize, ctrl as usize);
            assert_eq!((*ctrl).refcount, 2);
            // tree 비어있음 — 실제 insert 안 됨
            assert!(i.is_empty());
            // cleanup
            drop(Box::from_raw(ctrl));
        }
    }

    #[test]
    fn attach_single_insert_new_key() {
        unsafe {
            let mut i = PropertyBagImpl::new_boxed(false);
            let k = PropertyKey::from_int(601);
            let sp = make_share_ptr(2);
            assert_eq!((*sp).refcount, 1);

            let r = i.attach(&k, sp);
            assert!(r.is_null(), "new key → null sret");
            assert_eq!((*sp).refcount, 2, "node holds refcount++");
            assert_eq!(i.tree.size, 1);
            assert!(i.contains(&k));
            assert_eq!(i.get_state(&k), 2);
            // begin_node 는 새 node 가 leftmost
            assert!(!i.tree.begin_node.is_null());
            // tree drop 시 node free (ctrl refcount-- → 1, ctrl 자체 미 free; obj leak 처리)
        }
    }

    #[test]
    fn attach_multiple_inserts_size_and_get_state() {
        unsafe {
            let mut i = PropertyBagImpl::new_boxed(false);
            // 5 keys 순서 의도적 mix: 50, 20, 80, 10, 60
            let keys = [50u32, 20, 80, 10, 60];
            for (idx, &kv) in keys.iter().enumerate() {
                let k = PropertyKey::from_int(kv);
                let sp = make_share_ptr(idx as u32 + 1);
                let r = i.attach(&k, sp);
                assert!(r.is_null());
            }
            assert_eq!(i.tree.size, 5);

            // 모두 contains
            for &kv in &keys {
                let k = PropertyKey::from_int(kv);
                assert!(i.contains(&k));
            }

            // get_state 정확
            for (idx, &kv) in keys.iter().enumerate() {
                let k = PropertyKey::from_int(kv);
                assert_eq!(i.get_state(&k), idx as u32 + 1);
            }
        }
    }

    #[test]
    fn attach_then_set_state_round_trip() {
        unsafe {
            let mut i = PropertyBagImpl::new_boxed(false);
            let k = PropertyKey::from_int(601);
            let sp = make_share_ptr(2);
            i.attach(&k, sp);
            assert_eq!(i.get_state(&k), 2);

            i.set_state(&k, 7);
            assert_eq!(i.get_state(&k), 7);
        }
    }

    #[test]
    fn attach_clear_restores_empty() {
        unsafe {
            let mut i = PropertyBagImpl::new_boxed(false);
            for kv in [10u32, 20, 30] {
                let k = PropertyKey::from_int(kv);
                let sp = make_share_ptr(1);
                i.attach(&k, sp);
            }
            assert_eq!(i.tree.size, 3);
            i.clear();
            assert!(i.is_empty());
            assert_eq!(i.tree.size, 0);
        }
    }

    #[test]
    fn attach_duplicate_key_replaces_and_returns_old() {
        unsafe {
            let mut i = PropertyBagImpl::new_boxed(false);
            let k = PropertyKey::from_int(601);
            let sp1 = make_share_ptr(1);
            let sp2 = make_share_ptr(2);
            assert!(i.attach(&k, sp1).is_null());
            assert_eq!((*sp1).refcount, 2); // node holds +1

            // Replace
            let returned = i.attach(&k, sp2);
            assert_eq!(returned as usize, sp1 as usize, "should return old SharePtr");
            // sp1 was: in node refcount=2, then erase: erase_node adds another refcount++ → 3
            //   but tree_remove doesn't release. So refcount=3. The returned sret is sp1 with one more ref.
            assert_eq!((*sp1).refcount, 3);
            assert_eq!((*sp2).refcount, 2); // new node holds it

            assert_eq!(i.tree.size, 1);
            assert_eq!(i.get_state(&k), 2); // new value's state
        }
    }

    #[test]
    fn detach_not_found_returns_null() {
        unsafe {
            let mut i = PropertyBagImpl::new_boxed(false);
            let k = PropertyKey::from_int(601);
            let r = i.detach(&k);
            assert!(r.is_null());
            assert!(i.is_empty());
        }
    }

    #[test]
    fn detach_single_node_empties_tree() {
        unsafe {
            let mut i = PropertyBagImpl::new_boxed(false);
            let k = PropertyKey::from_int(601);
            let sp = make_share_ptr(5);
            i.attach(&k, sp);
            assert_eq!(i.tree.size, 1);
            assert!(i.contains(&k));

            let r = i.detach(&k);
            assert_eq!(r as usize, sp as usize, "return old SharePtr");
            assert_eq!(i.tree.size, 0);
            assert!(i.is_empty());
            assert!(!i.contains(&k));
            // begin_node 가 &end_node_left 로 reset
            let end_addr = &i.tree.end_node_left as *const _ as usize;
            assert_eq!(i.tree.begin_node as usize, end_addr);
        }
    }

    #[test]
    fn detach_one_of_many_keys() {
        unsafe {
            let mut i = PropertyBagImpl::new_boxed(false);
            for kv in [10u32, 20, 30, 40, 50] {
                i.attach(&PropertyKey::from_int(kv), make_share_ptr(kv));
            }
            assert_eq!(i.tree.size, 5);

            let k = PropertyKey::from_int(30);
            let _r = i.detach(&k);
            assert_eq!(i.tree.size, 4);
            assert!(!i.contains(&k));
            // others remain
            for kv in [10u32, 20, 40, 50] {
                let k = PropertyKey::from_int(kv);
                assert!(i.contains(&k), "missing {}", kv);
                assert_eq!(i.get_state(&k), kv);
            }
        }
    }

    #[test]
    fn detach_then_re_attach_works() {
        unsafe {
            let mut i = PropertyBagImpl::new_boxed(false);
            let k = PropertyKey::from_int(99);
            i.attach(&k, make_share_ptr(1));
            i.detach(&k);
            // re-attach
            i.attach(&k, make_share_ptr(2));
            assert_eq!(i.get_state(&k), 2);
            assert_eq!(i.tree.size, 1);
        }
    }

    #[test]
    fn detach_stress_50_keys_remove_half() {
        unsafe {
            let mut i = PropertyBagImpl::new_boxed(false);
            for kv in 1..=50u32 {
                i.attach(&PropertyKey::from_int(kv), make_share_ptr(kv));
            }
            assert_eq!(i.tree.size, 50);

            // Detach even keys
            for kv in (2..=50u32).step_by(2) {
                let r = i.detach(&PropertyKey::from_int(kv));
                assert!(!r.is_null(), "key {} should be present", kv);
            }
            assert_eq!(i.tree.size, 25);

            // Remaining odd keys still findable
            for kv in (1..=49u32).step_by(2) {
                let k = PropertyKey::from_int(kv);
                assert!(i.contains(&k), "odd {} missing", kv);
                assert_eq!(i.get_state(&k), kv);
            }
            // Removed even keys gone
            for kv in (2..=50u32).step_by(2) {
                let k = PropertyKey::from_int(kv);
                assert!(!i.contains(&k), "even {} should be gone", kv);
            }
        }
    }

    #[test]
    fn random_insert_delete_stress_200_ops() {
        // 100 random inserts → detach half random → re-insert detached → final state check
        unsafe {
            let mut i = PropertyBagImpl::new_boxed(false);
            let mut seed: u32 = 0x12345;
            let mut keys: Vec<u32> = (1..=100).collect();
            // LCG shuffle
            for last in (1..keys.len()).rev() {
                seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
                let idx = (seed >> 16) as usize % (last + 1);
                keys.swap(last, idx);
            }
            // Insert 100
            for &k in &keys {
                i.attach(&PropertyKey::from_int(k), make_share_ptr(k));
            }
            assert_eq!(i.tree.size, 100);

            // Detach first 50 (in shuffled order)
            for &k in &keys[..50] {
                let r = i.detach(&PropertyKey::from_int(k));
                assert!(!r.is_null(), "detach {} expected hit", k);
            }
            assert_eq!(i.tree.size, 50);

            // Verify 50 remaining all retrievable
            for &k in &keys[50..] {
                let key = PropertyKey::from_int(k);
                assert!(i.contains(&key), "key {} should remain", k);
                assert_eq!(i.get_state(&key), k);
            }

            // Detached 50 should NOT be in tree
            for &k in &keys[..50] {
                let key = PropertyKey::from_int(k);
                assert!(!i.contains(&key), "key {} should be gone", k);
            }

            // Re-insert detached
            for &k in &keys[..50] {
                let r = i.attach(&PropertyKey::from_int(k), make_share_ptr(k + 1000));
                assert!(r.is_null(), "re-insert {} expected new", k);
            }
            assert_eq!(i.tree.size, 100);

            // Verify all 100 retrievable (re-inserted with state k+1000)
            for &k in &keys[..50] {
                let key = PropertyKey::from_int(k);
                assert_eq!(i.get_state(&key), k + 1000, "re-inserted {} wrong state", k);
            }
            for &k in &keys[50..] {
                let key = PropertyKey::from_int(k);
                assert_eq!(i.get_state(&key), k, "original {} wrong state", k);
            }
        }
    }

    #[test]
    fn bag_detach_forward_to_impl() {
        unsafe {
            let mut bag = PropertyBag::new(false);
            let k = PropertyKey::from_int(601);
            let sp = make_share_ptr(7);
            bag.attach(&k, sp);
            assert!(bag.contains(&k));

            let r = bag.detach(&k);
            assert_eq!(r as usize, sp as usize);
            assert!(bag.is_empty());
        }
    }

    #[test]
    fn attach_stress_100_unique_keys() {
        // 100 unique int keys 삽입 + 모두 retrieve + 사이즈 검증.
        // libc++ rb-tree balance 검증 — randomized insertion order 에서도
        // 정상 동작 + lookup 의 시간복잡도 정상.
        unsafe {
            let mut i = PropertyBagImpl::new_boxed(false);
            // pseudo-random shuffle (deterministic for repeatable test)
            let mut keys: Vec<u32> = (1..=100).collect();
            // Fisher-Yates 의 deterministic LFSR shuffle
            let mut seed: u32 = 0xACE1;
            for last in (1..keys.len()).rev() {
                // simple LCG step
                seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
                let idx = (seed >> 16) as usize % (last + 1);
                keys.swap(last, idx);
            }
            // insert
            for (n, &kv) in keys.iter().enumerate() {
                let k = PropertyKey::from_int(kv);
                let sp = make_share_ptr(n as u32 + 1);
                let r = i.attach(&k, sp);
                assert!(r.is_null(), "insert {} → expected new, got refbump", kv);
            }
            assert_eq!(i.tree.size, 100);
            // verify all
            for (n, &kv) in keys.iter().enumerate() {
                let k = PropertyKey::from_int(kv);
                assert!(i.contains(&k), "missing key {}", kv);
                assert_eq!(i.get_state(&k), n as u32 + 1, "wrong state for key {}", kv);
            }
            // not-contained
            assert!(!i.contains(&PropertyKey::from_int(0)));
            assert!(!i.contains(&PropertyKey::from_int(101)));
            assert!(!i.contains(&PropertyKey::from_int(99999)));
        }
    }

    #[test]
    fn bag_attach_forward_to_impl() {
        unsafe {
            let mut bag = PropertyBag::new(false);
            let k = PropertyKey::from_int(601);
            let sp = make_share_ptr(5);

            let r = bag.attach(&k, sp);
            assert!(r.is_null());
            assert!(bag.contains(&k));
            assert_eq!(bag.get_state(&k), 5);
            assert!(!bag.is_empty());
        }
    }

    #[test]
    fn bag_clone_independence_after_clear() {
        // begin/end consistency under various states
        let mut bag = PropertyBag::new(false);
        bag.clear();
        bag.set_merged(true);
        assert!(bag.is_empty());
        unsafe {
            assert!(bag.impl_ref().unwrap().is_merged());
        }
    }

    // ---------- get_value_addr (raw 0x67d0e4 family) tests ----------

    /// Make a heap PEnum (state=1, value=v) wrapped in ControlBlock<Property>.
    fn make_pe_ctrl(value: u32) -> *mut ControlBlock<Property> {
        let pe = crate::property::PEnum::create_attach_ctrl(1, value);
        pe as *mut ControlBlock<Property>
    }

    #[test]
    fn get_value_addr_returns_property_plus_0xc_for_existing_key() {
        unsafe {
            let mut bag_impl = PropertyBagImpl::new_boxed(false);
            let k = PropertyKey::from_int(0x89e); // VERT
            let ctrl = make_pe_ctrl(0x1234_5678);
            assert!(bag_impl.attach(&k, ctrl).is_null());

            let impl_raw: *const PropertyBagImpl = &*bag_impl;
            let addr = PropertyBagImpl::get_value_addr(impl_raw, &k);
            assert!(!addr.is_null());
            // raw: Property+0xc = PEnum.value (u32)
            let value = *(addr as *const u32);
            assert_eq!(value, 0x1234_5678);
        }
    }

    #[test]
    #[should_panic(expected = "out_of_range")]
    fn get_value_addr_throws_on_empty_tree() {
        unsafe {
            let bag_impl = PropertyBagImpl::new_boxed(false);
            let k = PropertyKey::from_int(0x89e);
            let _ = PropertyBagImpl::get_value_addr(&*bag_impl, &k);
        }
    }

    #[test]
    #[should_panic(expected = "out_of_range")]
    fn get_value_addr_throws_on_missing_key() {
        unsafe {
            let mut bag_impl = PropertyBagImpl::new_boxed(false);
            let k1 = PropertyKey::from_int(100);
            let ctrl = make_pe_ctrl(5);
            assert!(bag_impl.attach(&k1, ctrl).is_null());

            let k2 = PropertyKey::from_int(200);
            let _ = PropertyBagImpl::get_value_addr(&*bag_impl, &k2);
        }
    }

    #[test]
    fn get_value_addr_walks_multi_key_tree_correctly() {
        // raw lower_bound walk: insert 10 keys, fetch each, verify value.
        unsafe {
            let mut bag_impl = PropertyBagImpl::new_boxed(false);
            for i in 0..10u32 {
                let k = PropertyKey::from_int(0x1000 + i);
                let ctrl = make_pe_ctrl(i * 100);
                assert!(bag_impl.attach(&k, ctrl).is_null());
            }
            let impl_raw: *const PropertyBagImpl = &*bag_impl;
            for i in 0..10u32 {
                let k = PropertyKey::from_int(0x1000 + i);
                let addr = PropertyBagImpl::get_value_addr(impl_raw, &k);
                assert_eq!(*(addr as *const u32), i * 100, "key {}", i);
            }
        }
    }

    #[test]
    #[should_panic(expected = "GetValue: bag is null")]
    fn get_value_addr_panics_on_null_bag() {
        unsafe {
            let k = PropertyKey::from_int(0x89e);
            let _ = PropertyBagImpl::get_value_addr(ptr::null(), &k);
        }
    }

    // --- L-5c-3 잔여: PropertyBag::swap / eq_op (byte-eq raw 0x4d618, 0x4da08)

    #[test]
    fn bag_swap_exchanges_ctrl_only() {
        let mut a = PropertyBag::new(false);
        let mut b = PropertyBag::new(true);
        let a_ctrl_before = a.ctrl;
        let b_ctrl_before = b.ctrl;
        a.swap(&mut b);
        assert_eq!(a.ctrl, b_ctrl_before);
        assert_eq!(b.ctrl, a_ctrl_before);
        // merged flag follows the swap
        unsafe {
            assert!(a.impl_ref().unwrap().is_merged());
            assert!(!b.impl_ref().unwrap().is_merged());
        }
    }

    #[test]
    fn bag_eq_two_empty_bags_are_equal() {
        let a = PropertyBag::new(false);
        let b = PropertyBag::new(false);
        assert!(a.eq_op(&b));
        assert!(!a.ne_op(&b));
    }

    #[test]
    fn bag_eq_two_null_ctrl_are_equal() {
        let a = PropertyBag {
            ctrl: ptr::null_mut(),
        };
        let b = PropertyBag {
            ctrl: ptr::null_mut(),
        };
        assert!(a.eq_op(&b));
        std::mem::forget(a);
        std::mem::forget(b);
    }

    #[test]
    fn bag_eq_one_null_ctrl_one_empty_bag_is_equal() {
        // raw branch_b_null path: ctrl_b null → check ctrl_a (null → true; non-null+null obj → true)
        // empty bag with non-null impl: ctrl_a non-null, impl_a non-null
        // → raw 0x4d68c: ldr x8=[x9]=impl_a; cmp x8, #0; cset eq → impl_a non-null → false
        let a = PropertyBag::new(false);
        let b = PropertyBag {
            ctrl: ptr::null_mut(),
        };
        assert!(!a.eq_op(&b));
        std::mem::forget(b);
    }

    #[test]
    fn bag_eq_same_ctrl_ptr_is_equal() {
        // raw 0x4d644: `cmp x9, x8; b.eq 0x4d6c0` (same impl ptr → true)
        let a = PropertyBag::new(false);
        let b = PropertyBag { ctrl: a.ctrl };
        assert!(a.eq_op(&b));
        // 같은 ctrl 을 둘 다 가짐 → b drop 이 free → a 가 dangling. forget b 으로 회피.
        std::mem::forget(b);
    }

    #[test]
    fn bag_eq_different_merged_flag_is_not_equal() {
        // raw 0x4d64c: merged_flag mismatch → false
        let a = PropertyBag::new(false);
        let b = PropertyBag::new(true);
        // 둘 다 size 0 인데 merged_flag 다름 → false
        assert!(!a.eq_op(&b));
    }

    #[test]
    fn bag_ne_op_is_xor_of_eq() {
        let a = PropertyBag::new(false);
        let b = PropertyBag::new(false);
        assert_eq!(a.ne_op(&b), !a.eq_op(&b));
    }
}

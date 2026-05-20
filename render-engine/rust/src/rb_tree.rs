//! libc++ `std::__1::__tree<T>` (Red-Black Tree) primitives — 1:1 byte-equivalent port.
//!
//! `Hnc::Shape::ColorScheme` 내부의 `std::map<SchemeStyle, Color>` 가
//! libc++ map (= 24B `__tree` + 64B Node) 으로 구현됨. 본 모듈은 그 RB-tree
//! 의 **untyped base layer** 1:1 포팅:
//!
//! - `TreeNodeBase` (32B: left/right/parent/is_black+pad) — 모든 generic 노드의
//!   공통 prefix.
//! - `TreeBase` (24B: begin_node/end_node_left/size) — `__tree` 의 외부 struct.
//! - `balance_after_insert` (raw @ `0x26550`) — RB-tree fixup w/ rotations.
//! - `left_rotate` / `right_rotate` — pointer rewires (inline in `0x26550`
//!   루프이지만 본 모듈에선 명시적 함수로 분리, raw 와 byte-equivalent).
//! - `is_left_child` helper (raw `ldr x10, [parent]; cmp x10, x; b.eq`).
//! - `subtree_destroy_recursive` (raw @ `0x631b24`) — post-order recursive
//!   destruction with caller-provided `drop_node` callback.
//! - `find_insert_position` (raw inline in `ColorScheme::SetAt` @ `0x150084..0x1500bc`)
//!   — key 비교 walk; 반환 = (insert_slot_ptr, parent_ptr, existing_node_or_null).
//!
//! 본 모듈은 **untyped** — Node 의 value layout (K, V) 는 사용자(ColorScheme,
//! FontSet 등) 가 자체 `#[repr(C)]` 구조체로 정의하며, 첫 32B 가
//! `TreeNodeBase` 와 byte-compatible 해야 함.
//!
//! # raw asm RE 출처
//!
//! - `kdsnr-hwp-toolkit/work/hft_re/render_re/COLORSCHEME_RE.txt` — 본 RE.
//! - 추가 검증: libc++ headers (`__tree`) 의 알고리즘 canonical form.

use std::ptr;

/// libc++ `__tree_node_base` — 모든 RB-tree 노드의 공통 32B prefix.
///
/// raw layout (raw asm `ColorScheme::SetAt` 의 `ldr w9, [x8, #0x20]`(key) +
/// `0x26550 balance` 의 access pattern 으로 도출):
///
/// ```text
/// offset  field        type         의미
/// 0x00    left         Node*        왼쪽 자식
/// 0x08    right        Node*        오른쪽 자식
/// 0x10    parent       NodeBase*    부모 (root 의 parent 는 __end_node)
/// 0x18    is_black     u8           1 = black, 0 = red
/// 0x19    _pad         [u8; 7]      8-byte alignment 패딩
/// ```
///
/// 총 32B / 8B align. `__end_node` 도 first 8B (= __left_) 만 사용한다 —
/// __tree::__end_node_() = `&__tree + 0x8` 위치의 8B 만 valid 한 base 로
/// 간주. 나머지 24B 는 dereference 금지 (libc++ 알고리즘이 보장).
#[repr(C)]
#[derive(Debug)]
pub struct TreeNodeBase {
    /// `__left_` — raw +0x00.
    pub left: *mut TreeNodeBase,
    /// `__right_` — raw +0x08.
    pub right: *mut TreeNodeBase,
    /// `__parent_` — raw +0x10. (root 의 parent 는 __end_node 의 base view)
    pub parent: *mut TreeNodeBase,
    /// `__is_black_` — raw +0x18 (1B). 1 = black, 0 = red.
    pub is_black: u8,
    /// 8B alignment 패딩 — raw 의 `strb` 가 7B uninit 남김.
    pub _pad_0x19: [u8; 7],
}

pub const TREE_NODE_BASE_SIZE_BYTES: usize = 32;
pub const TREE_NODE_BASE_ALIGN_BYTES: usize = 8;

const _: () = assert!(std::mem::size_of::<TreeNodeBase>() == TREE_NODE_BASE_SIZE_BYTES);
const _: () = assert!(std::mem::align_of::<TreeNodeBase>() == TREE_NODE_BASE_ALIGN_BYTES);

/// libc++ `__tree<T>` 의 외부 struct — 24B.
///
/// raw layout (ColorScheme[0x08..0x20] 영역; 본 모듈의 별도 struct):
///
/// ```text
/// offset  field            type         의미
/// 0x00    begin_node       NodeBase*    leftmost 노드 (empty 시 = &end_node)
/// 0x08    end_node_left    NodeBase*    __end_node.__left_ (= 트리 root, empty=null)
/// 0x10    size             u64          노드 개수
/// ```
///
/// `&self.end_node_left as *mut NodeBase` 의 의미: TreeNodeBase 의 first 8B 만
/// valid 한 "fake __end_node" — libc++ 알고리즘이 절대 right/parent/is_black
/// 을 안 읽음을 보장.
#[repr(C)]
#[derive(Debug)]
pub struct TreeBase {
    /// `__begin_node_` — raw +0x00. Empty 시 `&end_node_left as *mut NodeBase`.
    pub begin_node: *mut TreeNodeBase,
    /// `__end_node_.__left_` — raw +0x08. 트리 root (empty 시 null).
    pub end_node_left: *mut TreeNodeBase,
    /// `__size_` — raw +0x10.
    pub size: u64,
}

pub const TREE_BASE_SIZE_BYTES: usize = 24;
pub const TREE_BASE_ALIGN_BYTES: usize = 8;

const _: () = assert!(std::mem::size_of::<TreeBase>() == TREE_BASE_SIZE_BYTES);
const _: () = assert!(std::mem::align_of::<TreeBase>() == TREE_BASE_ALIGN_BYTES);

impl TreeBase {
    /// raw `ColorScheme::ColorScheme()` 의 tree init 부분 1:1 — empty tree.
    ///
    /// ```asm
    /// 14fd3c: str xzr, [x8, #0x10]!   ; (this+0x08) += 0x10 (= &end_node), *(this+0x10) = 0
    /// 14fd40: mov x20, x0
    /// 14fd44: str x8, [x20, #0x8]!    ; (this+0x08) = x8 (= &end_node) → begin_node = &end_node
    /// 14fd48: str xzr, [x0, #0x18]    ; size = 0
    /// ```
    ///
    /// 본 메소드는 `tree_base` 의 in-place init — 호출자가 `Pin` 또는 stable
    /// address 보장 필요 (begin_node 가 self 의 end_node_left 주소를 가리키므로).
    ///
    /// # Safety
    /// `self` 의 메모리 주소가 향후 stable 해야 함 (moving 금지). 호출 후
    /// `self.begin_node = &self.end_node_left as *mut TreeNodeBase`.
    pub unsafe fn init_empty(&mut self) {
        // end_node 의 base address = &self.end_node_left as *mut NodeBase
        // (TreeNodeBase 의 first 8B 가 __left_ 이므로 호환)
        let end_node_addr = (&mut self.end_node_left) as *mut *mut TreeNodeBase as *mut TreeNodeBase;
        self.begin_node = end_node_addr;
        self.end_node_left = ptr::null_mut();
        self.size = 0;
    }

    /// __end_node 의 base address (= &self.end_node_left as *mut TreeNodeBase).
    ///
    /// 이 주소는 TreeNodeBase 의 first 8B (= __left_) 만 valid. 절대로
    /// right/parent/is_black dereference 금지.
    #[inline]
    pub fn end_node_addr(&mut self) -> *mut TreeNodeBase {
        (&mut self.end_node_left) as *mut *mut TreeNodeBase as *mut TreeNodeBase
    }
}

/// `__tree_is_left_child(node)` — node 가 부모의 LEFT 자식인가?
///
/// raw 패턴 (예: `0x265cc` `ldr x10, [x9]; cmp x10, x1` 등):
/// ```text
/// is_left_child(x) = (x.parent.left == x)
/// ```
///
/// # Safety
/// `node` 는 valid 한 NodeBase. `node.parent` 는 valid 한 NodeBase 또는
/// `__end_node` (first 8B 만 valid). __end_node 의 경우 `.left` (= root) 를
/// 읽는데 그게 정확히 node 일 수도 있음 (root 의 경우 parent.left == node 가
/// true 임 — root 는 __end_node.__left_ 가 자기 자신을 가리킴).
#[inline]
pub unsafe fn is_left_child(node: *mut TreeNodeBase) -> bool {
    (*((*node).parent)).left == node
}

/// libc++ `__tree_next(node)` — in-order successor.
///
/// raw 의 `0x150628..0x150654` (ColorScheme::Clone 내부) 알고리즘 1:1:
///
/// ```text
/// 1. if node.right != null:
///       x = node.right
///       while x.left != null:
///           x = x.left
///       return x                  // leftmost of right subtree
/// 2. else:
///       while !is_left_child(node):
///           node = node.parent
///       return node.parent
/// ```
///
/// 결과: in-order traversal 의 다음 노드. 종착점은 `__end_node` (= tree.end_node_addr).
///
/// # Safety
/// `node` 는 fake __end_node 가 아닌 valid real node. parent chain 따라 올라가다가
/// `__end_node` 까지 도달 가능 (= 종료 sentinel).
pub unsafe fn tree_next(node: *mut TreeNodeBase) -> *mut TreeNodeBase {
    let n = node;
    if !(*n).right.is_null() {
        // leftmost of right subtree
        let mut x = (*n).right;
        while !(*x).left.is_null() {
            x = (*x).left;
        }
        x
    } else {
        // walk up until n is a left child
        let mut cur = n;
        while !is_left_child(cur) {
            cur = (*cur).parent;
        }
        (*cur).parent
    }
}

/// `__tree_left_rotate(x)` — x 를 중심으로 left rotation.
///
/// 알고리즘 (libc++ canonical, raw 의 `0x26618-0x26648` 인라인 패턴과 1:1):
/// ```text
/// y = x.right
/// x.right = y.left
/// if x.right != null: x.right.parent = x
/// y.parent = x.parent
/// if is_left_child(x): x.parent.left = y
/// else:                 x.parent.right = y
/// y.left = x
/// x.parent = y
/// ```
///
/// raw asm 의 `csel`/`ccmp` 패턴이 한 줄 if 로 풀림 (예: `0x2663c-0x26644`).
///
/// # Safety
/// `x` 는 valid NodeBase, `x.right` 는 non-null, `x.parent` 는 valid.
pub unsafe fn left_rotate(x: *mut TreeNodeBase) {
    let y = (*x).right;
    (*x).right = (*y).left;
    if !(*x).right.is_null() {
        (*(*x).right).parent = x;
    }
    (*y).parent = (*x).parent;
    if is_left_child(x) {
        (*((*x).parent)).left = y;
    } else {
        (*((*x).parent)).right = y;
    }
    (*y).left = x;
    (*x).parent = y;
}

/// `__tree_right_rotate(x)` — left_rotate 의 mirror.
///
/// # Safety
/// `x` 는 valid, `x.left` 는 non-null, `x.parent` 는 valid.
pub unsafe fn right_rotate(x: *mut TreeNodeBase) {
    let y = (*x).left;
    (*x).left = (*y).right;
    if !(*x).left.is_null() {
        (*(*x).left).parent = x;
    }
    (*y).parent = (*x).parent;
    if is_left_child(x) {
        (*((*x).parent)).left = y;
    } else {
        (*((*x).parent)).right = y;
    }
    (*y).right = x;
    (*x).parent = y;
}

/// `__tree_balance_after_insert(root, x)` — raw @ `0x26550`. RB fixup.
///
/// raw 알고리즘 (libc++ canonical):
/// ```text
/// x.is_black = (x == root)
/// while x != root && !x.parent.is_black:
///     if is_left_child(x.parent):
///         y = x.parent.parent.right    // uncle
///         if y != null && !y.is_black:
///             // CASE 1: red uncle — recolor
///             x = x.parent;             x.is_black = true
///             x = x.parent;             x.is_black = (x == root)
///             y.is_black = true
///         else:
///             if !is_left_child(x):
///                 // CASE 2: x is right child
///                 x = x.parent
///                 left_rotate(x)
///             // CASE 3: right rotate grandparent
///             x = x.parent;             x.is_black = true
///             x = x.parent;             x.is_black = false
///             right_rotate(x)
///             break
///     else:
///         // mirror image
///         y = x.parent.parent.left
///         if y != null && !y.is_black:
///             x = x.parent;             x.is_black = true
///             x = x.parent;             x.is_black = (x == root)
///             y.is_black = true
///         else:
///             if is_left_child(x):
///                 x = x.parent
///                 right_rotate(x)
///             x = x.parent;             x.is_black = true
///             x = x.parent;             x.is_black = false
///             left_rotate(x)
///             break
/// ```
///
/// # Safety
/// `root` 는 트리 root (non-null at entry — empty 시 caller 가 직접 root 설정
/// 후 호출). `x` 는 막 삽입된 노드. `x.parent` 부터 root 까지의 chain 이
/// valid.
pub unsafe fn balance_after_insert(root: *mut TreeNodeBase, x_init: *mut TreeNodeBase) {
    let mut x = x_init;
    // raw `26550: cmp x1, x0; cset w8, eq; strb w8, [x1, #0x18]`
    (*x).is_black = if x == root { 1 } else { 0 };
    // raw `2655c: b.eq exit` — 새 노드가 root 이면 끝
    if x == root {
        return;
    }

    // raw loop @ 0x26584
    loop {
        // x != root && !x.parent.is_black 일 때 진행
        if x == root || (*((*x).parent)).is_black != 0 {
            break;
        }
        let parent = (*x).parent;
        // parent 의 left-child 여부
        if is_left_child(parent) {
            // raw `265b0: ldr x11, [x8, #0x8]` — uncle = grandparent.right
            let grandparent = (*parent).parent;
            let uncle = (*grandparent).right;
            if !uncle.is_null() && (*uncle).is_black == 0 {
                // CASE 1: red uncle — recolor (raw `0x26568..0x26580`)
                (*parent).is_black = 1;
                x = grandparent;
                (*x).is_black = if x == root { 1 } else { 0 };
                (*uncle).is_black = 1;
            } else {
                // CASE 2/3
                if !is_left_child(x) {
                    // CASE 2: left-rotate parent
                    x = parent;
                    left_rotate(x);
                }
                // CASE 3: right-rotate grandparent
                x = (*x).parent;
                (*x).is_black = 1;
                x = (*x).parent;
                (*x).is_black = 0;
                right_rotate(x);
                break;
            }
        } else {
            // mirror image
            let grandparent = (*parent).parent;
            let uncle = (*grandparent).left;
            if !uncle.is_null() && (*uncle).is_black == 0 {
                // CASE 1 mirror
                (*parent).is_black = 1;
                x = grandparent;
                (*x).is_black = if x == root { 1 } else { 0 };
                (*uncle).is_black = 1;
            } else {
                if is_left_child(x) {
                    x = parent;
                    right_rotate(x);
                }
                x = (*x).parent;
                (*x).is_black = 1;
                x = (*x).parent;
                (*x).is_black = 0;
                left_rotate(x);
                break;
            }
        }
    }
}

/// libc++ `__tree_remove(__root, __z)` — splice node `z` out of tree + rebalance.
///
/// raw `0x70238` (libHncFoundation), ~200 instr. Standard libc++ rb-tree
/// erase algorithm with delete fix-up. 본 port 는 libc++ canonical version 의
/// algorithmic 1:1 (instruction-by-instruction 은 컴파일러 의존이라 algorithmic
/// equivalence 가 byte-eq 의 sufficient — output tree shape 가 동일).
///
/// ## 알고리즘 (4 단계)
///
/// 1. **Find replacement `y`**:
///    - If `z` has at most one child: `y = z`
///    - Else: `y = __tree_next(z)` (leftmost of z's right subtree)
///
/// 2. **Compute `x` (y's child)**: `y.left` if non-null else `y.right`. May be null.
///
/// 3. **Splice `y` out of its position**:
///    - Connect `x` to `y.parent`
///    - Update `y.parent.left` or `.right` to `x`
///    - Track `w` = sibling of `y` (for later fix-up)
///    - If `y != z`: also move `y` into `z`'s position (copy parent/children + is_black)
///
/// 4. **Rebalance** (if removed node was black + tree non-empty):
///    - 4 standard cases (Case 1-4) with mirror image (left/right child).
///
/// **caller 책임**: `z` 자체의 dealloc + value 의 dtor — 본 함수는 link 제거만.
///
/// # Safety
/// - `root` 는 valid tree 의 root (= `tree.end_node_left`), non-null
/// - `z` 는 트리 내 valid node
/// - `z.parent` (== root 의 경우 `__end_node`) valid
///
/// Returns: 새 root (may differ from input if z was root).
pub unsafe fn tree_remove(root: *mut TreeNodeBase, z: *mut TreeNodeBase) -> *mut TreeNodeBase {
    // Step 1: find replacement node y
    let y: *mut TreeNodeBase = if (*z).left.is_null() || (*z).right.is_null() {
        z
    } else {
        // tree_next of z: leftmost of z's right subtree
        let mut t = (*z).right;
        while !(*t).left.is_null() {
            t = (*t).left;
        }
        t
    };

    // Step 2: x = y's non-null child (if any)
    let x: *mut TreeNodeBase = if !(*y).left.is_null() {
        (*y).left
    } else {
        (*y).right
    };

    // Step 3: splice y out
    let mut w: *mut TreeNodeBase = ptr::null_mut();
    if !x.is_null() {
        (*x).parent = (*y).parent;
    }
    let y_is_left = is_left_child(y);
    if y_is_left {
        (*(*y).parent).left = x;
    } else {
        (*(*y).parent).right = x;
    }

    let mut new_root = root;
    if y == root {
        new_root = x;
    } else {
        // sibling of y (now sibling of x — for fix-up)
        w = if y_is_left {
            (*(*y).parent).right
        } else {
            (*(*y).parent).left
        };
    }

    let removed_black = (*y).is_black != 0;

    // Step 4: if y != z, move y into z's position
    if y != z {
        (*y).parent = (*z).parent;
        if is_left_child(z) {
            (*(*y).parent).left = y;
        } else {
            (*(*y).parent).right = y;
        }
        (*y).left = (*z).left;
        (*(*y).left).parent = y;
        (*y).right = (*z).right;
        if !(*y).right.is_null() {
            (*(*y).right).parent = y;
        }
        (*y).is_black = (*z).is_black;
        if z == root {
            new_root = y;
        }
    }

    // Step 5: rebalance if removed node was black
    if removed_black && !new_root.is_null() {
        // libc++ canonical fix-up: track (parent, x) explicitly since x may be null.
        // After splice, w is sibling at parent.
        //
        // 시작 위치: x 는 (null or non-null). w 는 sibling (== y's prior sibling).
        // parent_of_x = (x non-null) ? x.parent : (y's prior parent — same as w.parent)
        //
        // Note: 위 step 3 에서 x 가 non-null 인 경우 x.parent = y.parent 로 이미 설정.
        //       w 의 parent 는 항상 y.parent (= x.parent) 와 동일.
        let mut x_cur = x;
        let mut w_cur = w;
        // Determine the parent for the case when x_cur is null.
        // 초기 진입 시: w_cur 는 sibling at x's position's parent.
        // x_cur 의 parent (= w_cur 의 parent) 를 추적.
        loop {
            // 종료: x_cur 가 root 이거나 (x_cur 가 red 면 색칠 후 종료, 아래에서)
            if x_cur == new_root {
                break;
            }
            // x_cur 가 non-null 이면서 red → 종료 (loop 밖에서 black 으로 색칠)
            if !x_cur.is_null() && (*x_cur).is_black == 0 {
                break;
            }
            // parent — w_cur.parent 가 invariant 한 부모
            let parent = (*w_cur).parent;
            // 어느 side 인가? (x_cur == parent.left 인지)
            let x_is_left = (*parent).left == x_cur;

            if x_is_left {
                // w_cur 는 parent.right
                // Case 1: w red
                if (*w_cur).is_black == 0 {
                    (*w_cur).is_black = 1;
                    (*parent).is_black = 0;
                    left_rotate(parent);
                    if parent == new_root {
                        new_root = w_cur;
                    }
                    // new w = parent.right (after rotation)
                    w_cur = (*parent).right;
                }
                // Now w_cur is black
                let wl_black = (*w_cur).left.is_null() || (*(*w_cur).left).is_black != 0;
                let wr_black = (*w_cur).right.is_null() || (*(*w_cur).right).is_black != 0;
                if wl_black && wr_black {
                    // Case 2: both children black — recolor w red, propagate up
                    (*w_cur).is_black = 0;
                    x_cur = parent;
                    if x_cur == new_root {
                        break;
                    }
                    // w_cur = sibling at new x_cur's position
                    w_cur = if is_left_child(x_cur) {
                        (*(*x_cur).parent).right
                    } else {
                        (*(*x_cur).parent).left
                    };
                    continue;
                }
                // Case 3: w.right black (w.left red) → right-rotate w
                if wr_black {
                    (*(*w_cur).left).is_black = 1;
                    (*w_cur).is_black = 0;
                    right_rotate(w_cur);
                    w_cur = (*parent).right;
                }
                // Case 4: terminal — swap colors + left-rotate parent
                (*w_cur).is_black = (*parent).is_black;
                (*parent).is_black = 1;
                (*(*w_cur).right).is_black = 1;
                left_rotate(parent);
                if parent == new_root {
                    new_root = w_cur;
                }
                break;
            } else {
                // mirror (x_cur is right child, w_cur is parent.left)
                if (*w_cur).is_black == 0 {
                    (*w_cur).is_black = 1;
                    (*parent).is_black = 0;
                    right_rotate(parent);
                    if parent == new_root {
                        new_root = w_cur;
                    }
                    w_cur = (*parent).left;
                }
                let wl_black = (*w_cur).left.is_null() || (*(*w_cur).left).is_black != 0;
                let wr_black = (*w_cur).right.is_null() || (*(*w_cur).right).is_black != 0;
                if wl_black && wr_black {
                    (*w_cur).is_black = 0;
                    x_cur = parent;
                    if x_cur == new_root {
                        break;
                    }
                    w_cur = if is_left_child(x_cur) {
                        (*(*x_cur).parent).right
                    } else {
                        (*(*x_cur).parent).left
                    };
                    continue;
                }
                if wl_black {
                    (*(*w_cur).right).is_black = 1;
                    (*w_cur).is_black = 0;
                    left_rotate(w_cur);
                    w_cur = (*parent).left;
                }
                (*w_cur).is_black = (*parent).is_black;
                (*parent).is_black = 1;
                (*(*w_cur).left).is_black = 1;
                right_rotate(parent);
                if parent == new_root {
                    new_root = w_cur;
                }
                break;
            }
        }
        // x_cur (now possibly red or null) — paint black if non-null
        if !x_cur.is_null() {
            (*x_cur).is_black = 1;
        }
    }
    new_root
}

/// raw `0x631b24` — post-order recursive subtree destroy.
///
/// 각 노드에서:
/// 1. recurse into `node.left`
/// 2. recurse into `node.right`
/// 3. `drop_node(node)` — 호출자 정의 callback (value dtor + `operator_delete(node)`).
///
/// raw asm:
/// ```asm
/// 631b24: cbz x1, exit            ; if (node == null) return
/// 631b28-34: stack setup; x19 = node
/// 631b3c: ldr x1, [x1]            ; x1 = node.left
/// 631b40: bl 0x631b24              ; recurse(left)
/// 631b44: ldr x1, [x19, #0x8]     ; x1 = node.right
/// 631b48: mov x0, x20              ; allocator (passed-through)
/// 631b4c: bl 0x631b24              ; recurse(right)
/// 631b50-...: drop value + free node (ColorScheme 별 specific)
/// ```
///
/// 본 generic 함수는 raw 의 outer recursion 만 다루고, leaf-step (drop value +
/// free node) 는 `drop_node` callback 으로 위임 — 호출자가 Node 의 size +
/// value type 을 알기 때문.
///
/// # Safety
/// `node` 는 null 또는 valid NodeBase (full Node 의 prefix). `drop_node` 는
/// node 의 value dtor + `operator_delete(node)` 를 수행해야 함.
pub unsafe fn subtree_destroy_recursive<F>(node: *mut TreeNodeBase, drop_node: &F)
where
    F: Fn(*mut TreeNodeBase),
{
    if node.is_null() {
        return;
    }
    let left = (*node).left;
    subtree_destroy_recursive(left, drop_node);
    let right = (*node).right;
    subtree_destroy_recursive(right, drop_node);
    drop_node(node);
}

/// raw `ColorScheme::SetAt` 의 walk @ `0x150084..0x1500bc` — key-ordered
/// 트리 walk 로 insert 위치 + 기존 노드 (있다면) 결정.
///
/// 반환: (`insert_slot`, `parent`, `existing_node`)
/// - `insert_slot`: `*mut *mut TreeNodeBase` — 새 노드를 link 할 자리 (= &x.left
///   or &x.right or &tree.end_node_left). `existing_node` 가 non-null 이면
///   의미 없음.
/// - `parent`: 새 노드의 parent (= 새 노드가 link 될 위치의 owner).
/// - `existing_node`: 동일 key 의 기존 노드 (있다면) — 있으면 caller 가
///   update path 로 진행.
///
/// raw asm:
/// ```asm
/// 150084: ldr x8, [x0, #0x10]!     ; x0 += 0x10 → &end_node; x8 = root
/// 150088: cbz x8, no_root          ; empty tree
/// 15008c: mov x20, x0              ; x20 = &end_node (initial "last LE node" tracker)
/// 150090: ldr w9, [x8, #0x20]      ; w9 = node.key
/// 150094: add x10, x8, #0x8        ; x10 = &node.right
/// 150098: cmp w9, w1               ; node.key vs target
/// 15009c: csel x9, x10, x8, lt     ; if node.key < target: go right (&node.right), else stay at &node = &node.left
/// 1500a0: csel x20, x20, x8, lt    ; if node.key < target: keep x20 prev; else x20 = node (track of "last LE node")
/// 1500a4: ldr x8, [x9]             ; x8 = *(insert_slot) = next node
/// 1500a8: cbnz x8, loop             ; if non-null: continue
///
/// 1500ac: cmp x20, x0              ; if x20 == &end_node, no equality found
/// 1500b0: b.eq insert_branch
/// 1500b4: ldr w8, [x20, #0x20]     ; check x20.key vs target
/// 1500b8: cmp w8, w1
/// 1500bc: b.le update_branch        ; key 같으면 update; else insert
/// ```
///
/// **insert_slot 의 정확한 정의**: 새 노드가 들어갈 자리의 pointer slot.
/// 새 노드의 parent 는 그 슬롯의 owner (= x23 / x22 in raw).
///
/// raw 의 `x23` (insert_slot) 과 `x22` (parent) 추적:
/// - empty tree: insert_slot = &tree.end_node_left, parent = &end_node
/// - walking down: x22 = current node (= 항상 parent of next visited),
///   x23 = &x22.left or &x22.right
///
/// 본 함수는 untyped — caller 가 `T = TreeNodeBase` 의 generic Node 의 key
/// 비교 함수를 제공.
///
/// # Safety
/// `tree` 는 valid TreeBase. `cmp_key` 는 node 와 target_key 의 비교 함수
/// (Rust enum 으로 less/equal/greater).
pub unsafe fn find_insert_position<F>(
    tree: *mut TreeBase,
    cmp: F,
) -> (
    *mut *mut TreeNodeBase, // insert_slot
    *mut TreeNodeBase,      // parent
    *mut TreeNodeBase,      // existing_node (or null)
)
where
    F: Fn(*const TreeNodeBase) -> std::cmp::Ordering,
{
    let end_node = (*tree).end_node_addr();
    let current = (*tree).end_node_left; // root
    if current.is_null() {
        // 빈 트리: insert at end_node.left
        return (
            (&mut (*tree).end_node_left) as *mut *mut TreeNodeBase,
            end_node,
            ptr::null_mut(),
        );
    }
    // raw 의 walk loop @ 0x150090
    // x20 (track of "last LE node") 초기 = end_node
    let mut last_le_node = end_node;
    let mut next_node = current;
    loop {
        let node = next_node;
        let ord = cmp(node as *const TreeNodeBase);
        // raw `csel x9, x10, x8, lt` — node.key < target: go right; else go left (stay)
        // 우리 cmp 는 (node vs target). Less = node < target → go RIGHT
        let (next_slot, new_last_le) = match ord {
            std::cmp::Ordering::Less => {
                // node < target → 오른쪽 자식으로 — last_le_node unchanged
                ((&mut (*node).right) as *mut *mut TreeNodeBase, last_le_node)
            }
            std::cmp::Ordering::Equal | std::cmp::Ordering::Greater => {
                // node >= target → 왼쪽 자식으로 — last_le_node = this node
                ((&mut (*node).left) as *mut *mut TreeNodeBase, node)
            }
        };
        last_le_node = new_last_le;
        let nv = *next_slot;
        if nv.is_null() {
            // 더 못 내려감 — insert_slot 확정
            // raw `1500ac-1500bc`: equality check
            if last_le_node == end_node {
                // 모든 분기에서 LE 없음 (모든 walk 가 Less = go right)
                return (next_slot, node, ptr::null_mut());
            }
            // last_le_node.key >= target. 같으면 update path
            let cmp_last = cmp(last_le_node as *const TreeNodeBase);
            match cmp_last {
                std::cmp::Ordering::Less => {
                    // 불가능 — last_le_node 는 >= target 여야 함
                    return (next_slot, node, ptr::null_mut());
                }
                std::cmp::Ordering::Equal => {
                    // 동일 key — existing 으로 반환
                    return (next_slot, node, last_le_node);
                }
                std::cmp::Ordering::Greater => {
                    // last_le_node > target → insert
                    return (next_slot, node, ptr::null_mut());
                }
            }
        }
        next_node = nv;
    }
}

/// raw `0x662ed0-0x662ee4` — insert 직후 begin_node 갱신.
///
/// libc++ 패턴:
/// ```text
/// if begin_node->left != null:
///     begin_node = begin_node->left
/// ```
///
/// 새로 삽입된 노드가 leftmost 보다 더 왼쪽이면 begin_node 갱신.
///
/// # Safety
/// `tree` 는 valid. begin_node 가 valid NodeBase (= TreeBase 의 end_node 또는
/// 실제 node).
pub unsafe fn update_begin_node_after_insert(tree: *mut TreeBase) {
    let begin = (*tree).begin_node;
    let begin_left = (*begin).left;
    if !begin_left.is_null() {
        (*tree).begin_node = begin_left;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::alloc::Layout;

    // Tests 용: TestNode = TreeNodeBase + u32 key + u32 dummy value.
    #[repr(C)]
    struct TestNode {
        base: TreeNodeBase,
        key: u32,
        _pad: u32,
        value: u32,
    }

    unsafe fn alloc_test_node(key: u32, value: u32) -> *mut TestNode {
        let layout = Layout::new::<TestNode>();
        let p = std::alloc::alloc(layout) as *mut TestNode;
        ptr::write(
            p,
            TestNode {
                base: TreeNodeBase {
                    left: ptr::null_mut(),
                    right: ptr::null_mut(),
                    parent: ptr::null_mut(),
                    is_black: 0,
                    _pad_0x19: [0; 7],
                },
                key,
                _pad: 0,
                value,
            },
        );
        p
    }

    unsafe fn free_test_node(p: *mut TestNode) {
        std::alloc::dealloc(p as *mut u8, Layout::new::<TestNode>());
    }

    fn cmp_node_to_key(target: u32) -> impl Fn(*const TreeNodeBase) -> std::cmp::Ordering {
        move |node_base| {
            let node = node_base as *const TestNode;
            let nk = unsafe { (*node).key };
            nk.cmp(&target)
        }
    }

    #[test]
    fn raw_layout_size_align() {
        assert_eq!(std::mem::size_of::<TreeNodeBase>(), 32);
        assert_eq!(std::mem::align_of::<TreeNodeBase>(), 8);
        assert_eq!(std::mem::size_of::<TreeBase>(), 24);
        assert_eq!(std::mem::align_of::<TreeBase>(), 8);
    }

    #[test]
    fn tree_node_base_field_offsets() {
        // Pointer arithmetic 검증
        let base = TreeNodeBase {
            left: ptr::null_mut(),
            right: ptr::null_mut(),
            parent: ptr::null_mut(),
            is_black: 0,
            _pad_0x19: [0; 7],
        };
        let p = &base as *const TreeNodeBase as usize;
        let pl = &base.left as *const _ as usize;
        let pr = &base.right as *const _ as usize;
        let pp = &base.parent as *const _ as usize;
        let pi = &base.is_black as *const _ as usize;
        assert_eq!(pl - p, 0x00);
        assert_eq!(pr - p, 0x08);
        assert_eq!(pp - p, 0x10);
        assert_eq!(pi - p, 0x18);
    }

    #[test]
    fn tree_base_field_offsets() {
        let mut tb = TreeBase {
            begin_node: ptr::null_mut(),
            end_node_left: ptr::null_mut(),
            size: 0,
        };
        let p = &tb as *const TreeBase as usize;
        let pb = &tb.begin_node as *const _ as usize;
        let pe = &tb.end_node_left as *const _ as usize;
        let ps = &tb.size as *const _ as usize;
        assert_eq!(pb - p, 0x00);
        assert_eq!(pe - p, 0x08);
        assert_eq!(ps - p, 0x10);
        let _ = tb.end_node_addr(); // sanity call
    }

    #[test]
    fn tree_init_empty_state() {
        let mut tb = TreeBase {
            begin_node: ptr::null_mut(),
            end_node_left: ptr::null_mut(),
            size: 999,
        };
        unsafe {
            tb.init_empty();
        }
        // begin_node 는 end_node_left 의 address 를 가리킴
        let expected_begin = (&mut tb.end_node_left) as *mut *mut TreeNodeBase as *mut TreeNodeBase;
        assert_eq!(tb.begin_node, expected_begin);
        assert!(tb.end_node_left.is_null());
        assert_eq!(tb.size, 0);
    }

    #[test]
    fn insert_single_node_into_empty_tree() {
        unsafe {
            let mut tb = TreeBase {
                begin_node: ptr::null_mut(),
                end_node_left: ptr::null_mut(),
                size: 0,
            };
            tb.init_empty();

            let (slot, parent, existing) =
                find_insert_position(&mut tb as *mut TreeBase, cmp_node_to_key(5));
            assert!(existing.is_null());

            // 새 노드 alloc + link
            let new_node = alloc_test_node(5, 100);
            (*new_node).base.parent = parent;
            *slot = new_node as *mut TreeNodeBase;
            // balance: empty tree 의 root insert 는 root.is_black = 1
            let root = tb.end_node_left;
            assert_eq!(root, new_node as *mut TreeNodeBase);
            balance_after_insert(root, new_node as *mut TreeNodeBase);
            assert_eq!((*new_node).base.is_black, 1);

            update_begin_node_after_insert(&mut tb as *mut TreeBase);
            // begin_node 갱신 안 됨 (root.left == null)
            assert_eq!(tb.begin_node, new_node as *mut TreeNodeBase);

            // cleanup
            free_test_node(new_node);
        }
    }

    /// Helper: 트리에 노드 N 개 삽입 (key 순서대로).
    unsafe fn build_tree_from_keys(tb: *mut TreeBase, keys: &[u32]) -> Vec<*mut TestNode> {
        let mut nodes = Vec::new();
        for (i, &k) in keys.iter().enumerate() {
            let (slot, parent, existing) = find_insert_position(tb, cmp_node_to_key(k));
            assert!(existing.is_null(), "duplicate key {} at idx {}", k, i);
            let new_node = alloc_test_node(k, k.wrapping_mul(10));
            (*new_node).base.parent = parent;
            *slot = new_node as *mut TreeNodeBase;
            let root = (*tb).end_node_left;
            balance_after_insert(root, new_node as *mut TreeNodeBase);
            // root may have changed via rotation — re-read
            (*tb).end_node_left = {
                let mut cur = new_node as *mut TreeNodeBase;
                while (*cur).parent != (*tb).end_node_addr() {
                    cur = (*cur).parent;
                }
                cur
            };
            update_begin_node_after_insert(tb);
            (*tb).size += 1;
            nodes.push(new_node);
        }
        nodes
    }

    /// In-order traversal 로 key 들 수집.
    unsafe fn collect_in_order(node: *mut TreeNodeBase, out: &mut Vec<u32>) {
        if node.is_null() {
            return;
        }
        collect_in_order((*node).left, out);
        let n = node as *mut TestNode;
        out.push((*n).key);
        collect_in_order((*node).right, out);
    }

    #[test]
    fn insert_ascending_produces_sorted_tree() {
        unsafe {
            let mut tb = TreeBase {
                begin_node: ptr::null_mut(),
                end_node_left: ptr::null_mut(),
                size: 0,
            };
            tb.init_empty();
            let nodes = build_tree_from_keys(&mut tb, &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
            let mut out = Vec::new();
            collect_in_order(tb.end_node_left, &mut out);
            assert_eq!(out, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
            assert_eq!(tb.size, 10);
            for n in nodes {
                free_test_node(n);
            }
        }
    }

    #[test]
    fn insert_descending_balances_correctly() {
        unsafe {
            let mut tb = TreeBase {
                begin_node: ptr::null_mut(),
                end_node_left: ptr::null_mut(),
                size: 0,
            };
            tb.init_empty();
            let nodes = build_tree_from_keys(&mut tb, &[10, 9, 8, 7, 6, 5, 4, 3, 2, 1]);
            let mut out = Vec::new();
            collect_in_order(tb.end_node_left, &mut out);
            assert_eq!(out, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
            assert_eq!(tb.size, 10);
            for n in nodes {
                free_test_node(n);
            }
        }
    }

    #[test]
    fn insert_random_balances_correctly() {
        unsafe {
            let mut tb = TreeBase {
                begin_node: ptr::null_mut(),
                end_node_left: ptr::null_mut(),
                size: 0,
            };
            tb.init_empty();
            let keys: Vec<u32> = vec![50, 30, 70, 20, 40, 60, 80, 10, 25, 35, 45, 55, 65, 75, 85];
            let nodes = build_tree_from_keys(&mut tb, &keys);
            let mut out = Vec::new();
            collect_in_order(tb.end_node_left, &mut out);
            let mut sorted = keys.clone();
            sorted.sort();
            assert_eq!(out, sorted);
            assert_eq!(tb.size, keys.len() as u64);

            // Verify RB invariants
            verify_rb_invariants(&tb);

            for n in nodes {
                free_test_node(n);
            }
        }
    }

    unsafe fn verify_rb_invariants(tb: &TreeBase) {
        // Root is black
        let root = tb.end_node_left;
        if root.is_null() {
            return;
        }
        assert_eq!((*root).is_black, 1, "Root must be black");
        // No two red nodes adjacent + black-height consistency
        let _ = check_node(root);
    }

    unsafe fn check_node(node: *mut TreeNodeBase) -> u32 {
        if node.is_null() {
            return 1; // null = black-height 1
        }
        let left = (*node).left;
        let right = (*node).right;
        // Red node 의 자식은 모두 black
        if (*node).is_black == 0 {
            if !left.is_null() {
                assert_eq!((*left).is_black, 1, "Red node has red left child");
            }
            if !right.is_null() {
                assert_eq!((*right).is_black, 1, "Red node has red right child");
            }
        }
        let lbh = check_node(left);
        let rbh = check_node(right);
        assert_eq!(lbh, rbh, "Black-height mismatch");
        lbh + if (*node).is_black == 1 { 1 } else { 0 }
    }

    #[test]
    fn find_existing_node_returns_it() {
        unsafe {
            let mut tb = TreeBase {
                begin_node: ptr::null_mut(),
                end_node_left: ptr::null_mut(),
                size: 0,
            };
            tb.init_empty();
            let nodes = build_tree_from_keys(&mut tb, &[1, 2, 3, 4, 5]);

            let (_slot, _parent, existing) =
                find_insert_position(&mut tb as *mut TreeBase, cmp_node_to_key(3));
            assert!(!existing.is_null());
            assert_eq!((*(existing as *mut TestNode)).key, 3);

            for n in nodes {
                free_test_node(n);
            }
        }
    }

    #[test]
    fn subtree_destroy_recursive_visits_all_nodes() {
        unsafe {
            let mut tb = TreeBase {
                begin_node: ptr::null_mut(),
                end_node_left: ptr::null_mut(),
                size: 0,
            };
            tb.init_empty();
            let _nodes = build_tree_from_keys(&mut tb, &[1, 2, 3, 4, 5, 6, 7]);

            use std::cell::RefCell;
            let visited: RefCell<Vec<u32>> = RefCell::new(Vec::new());

            let drop_node = |node: *mut TreeNodeBase| {
                let n = node as *mut TestNode;
                visited.borrow_mut().push((*n).key);
                free_test_node(n);
            };

            subtree_destroy_recursive(tb.end_node_left, &drop_node);

            // post-order: left subtree, right subtree, then self
            let v = visited.borrow();
            assert_eq!(v.len(), 7);
            let mut sorted: Vec<u32> = v.clone();
            sorted.sort();
            assert_eq!(sorted, vec![1, 2, 3, 4, 5, 6, 7]);
        }
    }

    #[test]
    fn subtree_destroy_on_null_is_noop() {
        unsafe {
            let drop_node = |_n: *mut TreeNodeBase| panic!("should not be called");
            subtree_destroy_recursive(ptr::null_mut(), &drop_node);
        }
    }

    #[test]
    fn is_left_child_correct_for_root() {
        unsafe {
            let mut tb = TreeBase {
                begin_node: ptr::null_mut(),
                end_node_left: ptr::null_mut(),
                size: 0,
            };
            tb.init_empty();
            let root = alloc_test_node(5, 0);
            tb.end_node_left = root as *mut TreeNodeBase;
            (*root).base.parent = tb.end_node_addr();
            (*root).base.is_black = 1;
            // root.parent = end_node, end_node.left = root. So root is "left child" of end_node.
            assert!(is_left_child(root as *mut TreeNodeBase));
            free_test_node(root);
        }
    }

    #[test]
    fn left_rotate_swaps_pivot_with_right_child() {
        unsafe {
            // 트리:   x
            //          \
            //           y
            // 회전 후: y
            //          /
            //         x
            let mut tb = TreeBase {
                begin_node: ptr::null_mut(),
                end_node_left: ptr::null_mut(),
                size: 0,
            };
            tb.init_empty();
            let x_node = alloc_test_node(1, 10);
            let y_node = alloc_test_node(2, 20);
            tb.end_node_left = x_node as *mut TreeNodeBase;
            (*x_node).base.parent = tb.end_node_addr();
            (*x_node).base.right = y_node as *mut TreeNodeBase;
            (*y_node).base.parent = x_node as *mut TreeNodeBase;

            left_rotate(x_node as *mut TreeNodeBase);

            // 새 root = y
            assert_eq!(tb.end_node_left, y_node as *mut TreeNodeBase);
            assert!((*y_node).base.right.is_null());
            assert_eq!(
                (*y_node).base.left,
                x_node as *mut TreeNodeBase,
                "y.left = x"
            );
            assert_eq!((*x_node).base.parent, y_node as *mut TreeNodeBase);

            free_test_node(x_node);
            free_test_node(y_node);
        }
    }

    #[test]
    fn right_rotate_swaps_pivot_with_left_child() {
        unsafe {
            // 트리:   x
            //         /
            //        y
            // 회전 후: y
            //          \
            //           x
            let mut tb = TreeBase {
                begin_node: ptr::null_mut(),
                end_node_left: ptr::null_mut(),
                size: 0,
            };
            tb.init_empty();
            let x_node = alloc_test_node(2, 10);
            let y_node = alloc_test_node(1, 20);
            tb.end_node_left = x_node as *mut TreeNodeBase;
            (*x_node).base.parent = tb.end_node_addr();
            (*x_node).base.left = y_node as *mut TreeNodeBase;
            (*y_node).base.parent = x_node as *mut TreeNodeBase;

            right_rotate(x_node as *mut TreeNodeBase);

            assert_eq!(tb.end_node_left, y_node as *mut TreeNodeBase);
            assert!((*y_node).base.left.is_null());
            assert_eq!((*y_node).base.right, x_node as *mut TreeNodeBase);
            assert_eq!((*x_node).base.parent, y_node as *mut TreeNodeBase);

            free_test_node(x_node);
            free_test_node(y_node);
        }
    }

    #[test]
    fn balance_after_root_insert_makes_it_black() {
        unsafe {
            let mut tb = TreeBase {
                begin_node: ptr::null_mut(),
                end_node_left: ptr::null_mut(),
                size: 0,
            };
            tb.init_empty();
            let root = alloc_test_node(5, 0);
            tb.end_node_left = root as *mut TreeNodeBase;
            (*root).base.parent = tb.end_node_addr();
            balance_after_insert(root as *mut TreeNodeBase, root as *mut TreeNodeBase);
            assert_eq!((*root).base.is_black, 1);
            free_test_node(root);
        }
    }

    #[test]
    fn begin_node_tracks_leftmost_after_inserts() {
        unsafe {
            let mut tb = TreeBase {
                begin_node: ptr::null_mut(),
                end_node_left: ptr::null_mut(),
                size: 0,
            };
            tb.init_empty();
            // 빈 트리: begin_node = &end_node
            let initial = tb.end_node_addr();
            assert_eq!(tb.begin_node, initial);

            let nodes = build_tree_from_keys(&mut tb, &[50, 30, 70, 20, 10, 5]);
            // 가장 작은 키 = 5 → leftmost.
            let leftmost = nodes.iter().find(|&&n| (*n).key == 5).copied().unwrap();
            assert_eq!(tb.begin_node, leftmost as *mut TreeNodeBase);

            for n in nodes {
                free_test_node(n);
            }
        }
    }

    #[test]
    fn balance_stress_50_random_keys() {
        unsafe {
            let mut tb = TreeBase {
                begin_node: ptr::null_mut(),
                end_node_left: ptr::null_mut(),
                size: 0,
            };
            tb.init_empty();
            // Pseudo-random sequence (Lehmer-style)
            let mut state: u32 = 12345;
            let mut keys = Vec::new();
            for _ in 0..50 {
                state = state.wrapping_mul(1103515245).wrapping_add(12345);
                let k = state % 1000;
                if !keys.contains(&k) {
                    keys.push(k);
                }
            }
            let nodes = build_tree_from_keys(&mut tb, &keys);
            let mut out = Vec::new();
            collect_in_order(tb.end_node_left, &mut out);
            let mut sorted = keys.clone();
            sorted.sort();
            assert_eq!(out, sorted);
            verify_rb_invariants(&tb);

            for n in nodes {
                free_test_node(n);
            }
        }
    }
}

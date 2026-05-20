//! `Hnc::Shape::Text::ArrayCompositor` 의 vfunc body 1:1 포팅.
//!
//! RTTI: `ArrayCompositor` vtable `@ 0x7804e8`, object vptr `@ 0x7804f8`.
//! ctor `0x304b94` / `0x304ba4` (`ArrayCompositor(unsigned long param_1)`) — raw:
//! ```c
//! *(undefined ***)this = &PTR__ArrayCompositor_007804f8;   // vptr
//! *(ulong *)(this + 8) = param_1;                          // +0x08 = divisor (ulong)
//! ```
//! → `ArrayCompositor` 는 `+0x08` 에 단일 `u64` field (`divisor`) 를 가진다.
//!
//! vfunc (object-vptr-relative):
//! - `+0x18 ComposeNumbering` (`0x304bf4`) / `+0x20 ComposeBullet` (`0x304bf8`) — raw `ret`
//!   no-op (`return param_1`). → `Compositor` trait default 사용.
//! - `+0x28 ComposeBreak` (`0x304bfc`, 352B) — `array_compose_break`, 본 파일.
//! - `+0x30 ComposeLayout` (`0x304d5c`, 2440B) — `SimpleCompositor::ComposeLayout`
//!   (`0x303f80`) 와 decompile 完全 동일 (LAB_ 주소만 차이) →
//!   `simple_compositor::simple_compose_layout` 를 그대로 공유 (별도 함수 불필요).

// ============================================================
// ArrayCompositor::ComposeBreak  (FUN_00304bfc, size 352)
// ============================================================

/// `Hnc::Shape::Text::ArrayCompositor::ComposeBreak` 1:1 포팅.
///
/// raw C++ signature (decompile, SimpleCompositor::ComposeBreak 와 동일 prototype):
/// ```c
/// ulong ArrayCompositor::ComposeBreak(
///     vector<float> const& param_1,   // x1 — widths
///     vector<float> const& param_2,   // x2 — stretches  (body 미사용)
///     vector<float> const& param_3,   // x3 — shrinks    (body 미사용)
///     vector<int>   const& param_4,   // x4 — penalties  (body 미사용)
///     vector<float> const& param_5,   // x5 — heights    (body 미사용!)
///     Composition const*   param_6,   // x6 — Composition* (body 미사용)
///     int param_7,                    // x7 — from        (body 미사용)
///     int param_8,                    // stack0 — to      (body 미사용)
///     vector<int>&  param_9)          // stack1 — &output (in/out, pre-sized)
/// ```
///
/// raw asm: `mov x8,x0` 로 this 보존 후 `ldr w8,[x8,#0x8]` → **`this->divisor` (`+0x08`) 만
/// 사용**. `widths`(x1) + `output`(param_9) 외 인자 (`heights` 포함) 는 전부 미사용.
///
/// ## output cap = `widths.len()`
///
/// `SimpleCompositor::ComposeBreak` 과 동일 — `Composition::Repair` 에서 widths 와 output 이
/// 항상 같은 크기로 동시 resize (`repair.txt` line 153-206). 따라서 `n_out == n_w == widths.len()`.
///
/// ## 알고리즘 (fixed-array, raw 0x304bfc-0x304d58)
///
/// 각 라인이 정확히 `divisor` 개 item 을 담는 고정 배열 레이아웃:
/// ```text
/// iVar3  = widths.len() - 1                       (마지막 char index)
/// iVar2  = this.divisor as i32                    (라인당 item 수)
/// iVar13 = (iVar2 != 0) ? iVar3 / iVar2 : 0       (full line 수)
/// uVar5  = min(n_out, iVar13 + 1)                 (총 line 수)
/// output[p] = min((p+1) * iVar2 - 1, iVar3)       for p in 0..uVar5
/// ```
/// raw 는 16개씩 NEON unroll 하지만 (`smin v.4S,...`), lane base = `uzp1(_DAT_007415c0,
/// _DAT_007415b0)` = `[0,1,2,3]` (+ 16/iter) 임이 확인됨 → SIMD/scalar 모두 위치-선형
/// `(p+1)*iVar2-1` 을 산출. scalar tail (`0x304cf8-d1c`): `madd w13,w8,w13,w8` =
/// `iVar2*p + iVar2` → `- 1` → `iVar2*(p+1)-1`, `csel ...lt` = `min(iVar3, ·)`.
///
/// 32-bit modular 산술 (`madd`/`add`/`sub w`) 1:1 보존 위해 `wrapping_*` 사용.
pub fn array_compose_break(widths: &[f32], divisor: u64) -> Vec<u32> {
    // raw 0x304c10-28: iVar3 = widths.size() - 1.
    let i_var3: i32 = (widths.len() as i32).wrapping_sub(1);
    // raw 0x304c1c-24: n_out = output.size()  (== widths.len(), 위 invariant).
    let n_out: i32 = widths.len() as i32;
    // raw 0x304c2c: iVar2 = *(int*)(this + 0x08)  — divisor 의 low 32 bits.
    let i_var2: i32 = divisor as i32;

    // raw 0x304c30-34: iVar13 = (iVar2 != 0) ? iVar3 / iVar2 : 0.
    //   raw 는 iVar2==0 일 때 sdiv 를 건너뛰고 iVar13=0 (decompile: `if (iVar2 != 0)`).
    let i_var13: i32 = if i_var2 != 0 {
        i_var3.wrapping_div(i_var2)
    } else {
        0
    };

    // raw 0x304c34-3c: uVar5 = (iVar13 + 1 < n_out) ? iVar13 + 1 : n_out  = min(n_out, iVar13+1).
    let i_var13_plus_1 = i_var13.wrapping_add(1);
    let u_var5: i32 = if i_var13_plus_1 < n_out {
        i_var13_plus_1
    } else {
        n_out
    };

    // raw 0x304c40-44: uVar5 < 1 → 채우기 skip.
    let mut out: Vec<u32> = Vec::new();
    if u_var5 > 0 {
        // raw 0x304c58-d1c (SIMD) + 0x304cf8-d1c (scalar tail): 둘 다 위치-선형.
        //   output[p] = min((p+1)*iVar2 - 1, iVar3).
        for p in 0..u_var5 {
            // raw scalar: madd w13,w8,w13,w8 = iVar2*p + iVar2;  sub w13,w13,#1.
            let val = i_var2
                .wrapping_mul(p)
                .wrapping_add(i_var2)
                .wrapping_sub(1);
            // raw: csel w16,w11,w13,lt → min(iVar3, val).
            let clamped = if i_var3 < val { i_var3 } else { val };
            out.push(clamped as u32);
        }
    }

    // raw 0x304d20-58: uVar7 = uVar5. output 을 uVar5 로 resize (truncate/grow). uVar5 =
    //   min(n_out, ...) <= n_out 이므로 grow (`FUN_0067ee78`) 는 dead, truncate 만 발생.
    //   우리 `out` 은 정확히 uVar5 개 push → out.len() == uVar5 == 반환값. 그대로 반환.
    out
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn divisor_zero_single_line() {
        // iVar2=0 → iVar13=0 → uVar5=min(4,1)=1. output[0]=min(0*1-1, 3)=min(-1,3)=-1.
        //   (raw: iVar2=0 이면 p=0 에서 0*0+0-1 = -1; csel min(iVar3=3, -1) = -1.)
        let r = array_compose_break(&[1.0, 1.0, 1.0, 1.0], 0);
        assert_eq!(r, vec![(-1_i32) as u32]);
    }

    #[test]
    fn divisor_two_six_chars() {
        // 6 chars, divisor=2. iVar3=5, iVar2=2, iVar13=5/2=2, uVar5=min(6,3)=3.
        // output[0]=min(1*2-1,5)=1, output[1]=min(2*2-1,5)=3, output[2]=min(3*2-1,5)=5.
        assert_eq!(array_compose_break(&[0.0; 6], 2), vec![1, 3, 5]);
    }

    #[test]
    fn divisor_three_clamps_to_last() {
        // 7 chars, divisor=3. iVar3=6, iVar2=3, iVar13=6/3=2, uVar5=min(7,3)=3.
        // output[0]=min(3-1,6)=2, output[1]=min(6-1,6)=5, output[2]=min(9-1,6)=6 (clamped).
        assert_eq!(array_compose_break(&[0.0; 7], 3), vec![2, 5, 6]);
    }

    #[test]
    fn divisor_larger_than_chars() {
        // 3 chars, divisor=10. iVar3=2, iVar2=10, iVar13=2/10=0, uVar5=min(3,1)=1.
        // output[0]=min(10-1,2)=2.
        assert_eq!(array_compose_break(&[0.0; 3], 10), vec![2]);
    }

    #[test]
    fn empty_widths() {
        // widths empty → iVar3=-1, n_out=0. iVar2=5 → iVar13=-1/5=0, uVar5=min(0,1)=0.
        //   uVar5<1 → 빈 출력.
        assert_eq!(array_compose_break(&[], 5), Vec::<u32>::new());
    }

    #[test]
    fn divisor_one_each_char_own_line() {
        // 4 chars, divisor=1. iVar3=3, iVar2=1, iVar13=3, uVar5=min(4,4)=4.
        // output[p] = min((p+1)*1-1, 3) = min(p, 3) = p.
        assert_eq!(array_compose_break(&[0.0; 4], 1), vec![0, 1, 2, 3]);
    }
}

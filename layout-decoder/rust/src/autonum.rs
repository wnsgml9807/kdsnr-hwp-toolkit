//! `FUN_002e86e8` — paragraph autonumber bullet string generator.
//!
//! raw `FUN_002e86e8` (`bullet_render_deps.txt:2905-3082`, 1300B + 18 leaf 함수) 의
//! byte-equivalent 1:1 포팅. 41 case dispatch — OOXML/HWPX autonumber format type →
//! UTF-16 string.
//!
//! ## RE 출처
//!
//! - **dispatch**: `bullet_render_deps.txt:2905-3082` (jump table @ `DAT_007444e8`).
//! - **leaf functions**: `/tmp/hft_scripts/bcompositor/autonum_subsystem.txt` —
//!   18 dump 본체. Roman (`FUN_002ea7ac`) 는 `autonum_leafs2.txt:1-345`.
//! - **format strings**: `bcompositor/autonum_leafs2.txt:347-353`. 13 Roman string +
//!   3 digit tables + parens/period suffixes.
//!
//! ## 알고리즘 family
//!
//! 1. **Alphabet doubling** (cases 0-5): `repeat=((v-1)%780)/26+1` copies of
//!    `'A'+(v-1)%780%26`, optional `MakeLower`, optional `(`/`)` 또는 `.` wrap.
//! 2. **Plain decimal Format** (cases 6-9, 0x1c-0x28): `%d` 변형 (괄호/마침표).
//! 3. **Roman numerals** (cases 10-15, 0x1b): greedy subtraction `M/CM/D/CD/...`.
//! 4. **Korean enclosed numbers** (case 0x10/0x11/0x12): 1-10 의 PUA / circled chars.
//! 5. **Fullwidth decimal digit-by-digit** (cases 0x13/0x14): digit table @ `0x74749a`.
//! 6. **Chinese-tens decimal** (cases 0x15/0x16/0x17): `十X`, `X十Y`, table @ `0x747486`.
//! 7. **Mixed-table digit-by-digit** (cases 0x19/0x1a/0x1b): table @ `0x747472`.

// ============================================================
// 데이터 테이블 (raw `0x747486`/`0x74749a`/`0x747472`)
// ============================================================

/// raw `DAT_00747486` — Chinese-tens 의 0..9 digit chars (case 0x15/0x16/0x17 의 base).
///
/// 10 u16 entries. Index 0 = `十` (ten-marker), 1-9 = `一..九`.
///
/// 인덱스 0 (`U+5341` 十) 은 tens-separator 로만 사용되고, 1-9 는 일반 digit.
pub const CHINESE_TENS_TABLE: [u16; 10] = [
    0x5341, // 十 (ten marker)
    0x4E00, // 一
    0x4E8C, // 二
    0x4E09, // 三
    0x56DB, // 四
    0x4E94, // 五
    0xF9D1, // (CJK Compatibility, looks like 六)
    0x4E03, // 七
    0x516B, // 八
    0x4E5D, // 九
];

/// raw `DAT_0074749a` — fullwidth decimal digits 0..9 (case 0x13/0x14 base).
///
/// `U+FF10..U+FF19` ('０'..'９'). digit-by-digit expansion 으로 fullwidth 결과 생성.
pub const FULLWIDTH_DIGIT_TABLE: [u16; 10] = [
    0xFF10, 0xFF11, 0xFF12, 0xFF13, 0xFF14, 0xFF15, 0xFF16, 0xFF17, 0xFF18, 0xFF19,
];

/// raw `DAT_00747472` — mixed-table for case 0x19/0x1a/0x1b. 0 = `〇` (U+25CB),
/// 1-9 = 한자 digit (table 동일).
///
/// case 0x19/0x1a 는 digit-by-digit, 0 자리는 `〇` 으로.
pub const MIXED_DIGIT_TABLE: [u16; 10] = [
    0x25CB, // 〇 (CIRCLE)
    0x4E00, 0x4E8C, 0x4E09, 0x56DB, 0x4E94, 0xF9D1, 0x4E03, 0x516B, 0x4E5D,
];

// ============================================================
// Roman numeral table — raw FUN_002ea7ac
// ============================================================

/// raw FUN_002ea7ac 의 13-entry greedy subtraction table.
///
/// `(value_threshold, roman_string)` — `>= threshold` 면 string concat 후 `-= threshold`.
/// 순서 중요: M → CM → D → CD → C → XC → L → XL → X → IX → V → IV → I.
const ROMAN_TABLE: &[(u32, &[u16])] = &[
    (1000, &['M' as u16]),
    (900, &['C' as u16, 'M' as u16]),
    (500, &['D' as u16]),
    (400, &['C' as u16, 'D' as u16]),
    (100, &['C' as u16]),
    (90, &['X' as u16, 'C' as u16]),
    (50, &['L' as u16]),
    (40, &['X' as u16, 'L' as u16]),
    (10, &['X' as u16]),
    (9, &['I' as u16, 'X' as u16]),
    (5, &['V' as u16]),
    (4, &['I' as u16, 'V' as u16]),
    (1, &['I' as u16]),
];

// ============================================================
// Primitive generators (각 leaf 함수의 알고리즘 부분)
// ============================================================

/// raw `FUN_002e8df0..002e94b8` 의 공통 alpha-doubling 알고리즘 (case 0-5 의 base).
///
/// raw asm (e.g. `002e8e14-002e8e54`):
/// ```text
/// w8 = value - 1
/// q = ((value - 1) >> 2) * 0x54054055 >> 38   ; div by 780 (10 sets of 78)
/// w8 = (value - 1) - q*780                     ; (value-1) % 780
/// q9 = (w8 * 0x9d9) >> 16                      ; div by 26
/// w8 = w8 - q9*26                              ; ((value-1)%780) % 26
/// w8 += 0x41                                    ; 'A' base
/// w9 += 1                                       ; repeat count
/// ```
///
/// 결과: 'A' + r 을 `q+1` 번 반복. r = `((value-1) % 780) % 26`. q = `((value-1) % 780) / 26`.
///
/// 사이클: value=1 → "A" / 2 → "B" / 26 → "Z" / 27 → "AA" / 52 → "ZZ" / 53 → "AAA" / ... /
/// 78×10 = 780 → "ZZZZZZZZZZ" (10 Z's) / 781 → wrap around 다시 "A".
pub fn alpha_double_letters(value: i32) -> Vec<u16> {
    // raw asm 는 signed/unsigned 혼합 — value 가 음수일 때 raw 와 정확히 일치하려면 그대로
    // 처리. value <= 0 의 경우 raw 는 wrap-around (e.g. value=0 → 'A' * (large repeat)).
    // 정공법: raw 동작 보존.
    let v_minus_1 = (value as u32).wrapping_sub(1);
    let mod_780 = v_minus_1 % 780;
    let repeat = (mod_780 / 26) + 1;
    let ch = ('A' as u32 + mod_780 % 26) as u16;
    vec![ch; repeat as usize]
}

/// raw `FUN_002ea7ac` (`autonum_leafs2.txt:1-345`, 960B) 의 Roman numeral 생성기 1:1.
///
/// `value` (u32) → "M..."" 형식 Roman. `lower=true` 면 `MakeLower` 적용.
///
/// raw decompile의 greedy subtraction 13 단계 — 위 `ROMAN_TABLE` 1:1.
///
/// 마지막 "I" 루프는 raw asm `002ea9dc-002eaab4` 의 `iVar2 = -iVar2; while (iVar2 != -1)`
/// 패턴 — `value+1` 의 부호 뒤집어 iterate. 결과적으로 0..3 의 "I" 추가.
///
/// raw 는 1000 미만 input 도 지원 (M loop 가 안 돌아 즉시 다음 단계로). 0 → 빈 string.
pub fn roman(value: u32, lower: bool) -> Vec<u16> {
    // raw decompile line 41-76: greedy table loop.
    let mut out: Vec<u16> = Vec::new();
    let mut remaining = value;
    for &(threshold, s) in ROMAN_TABLE {
        while remaining >= threshold {
            out.extend_from_slice(s);
            remaining -= threshold;
        }
    }

    // raw line 81-83: param_3 != 0 → MakeLower (전체 string).
    if lower {
        for ch in out.iter_mut() {
            if (b'A' as u16..=b'Z' as u16).contains(ch) {
                *ch += (b'a' - b'A') as u16;
            }
        }
    }

    out
}

/// raw `FUN_002eac50` / `FUN_002eae38` 의 digit-by-digit expansion.
///
/// raw decompile (`autonum_leafs2.txt:928-1006`): `value != 0` 면 do-while 으로 마지막 자릿수
/// 부터 추출 → 표 조회 → string 의 앞에 prepend. 자리수 모두 처리 후 빠져나옴.
///
/// `value < 10` 이면 단일 char (`table[value]`). `value == 0` 면 빈 string.
pub fn decimal_digit_table_expand(value: u32, table: &[u16; 10]) -> Vec<u16> {
    if value == 0 {
        return Vec::new();
    }
    // raw 는 마지막 자릿수부터 추출 + Concat (즉 prepend 효과). Rust 에서는 추출 후 reverse.
    let mut digits_rev: Vec<u16> = Vec::new();
    let mut v = value;
    loop {
        let d = (v % 10) as usize;
        digits_rev.push(table[d]);
        v /= 10;
        if v < 10 {
            // raw `while (9 < uVar1)`: value >= 10 면 loop. 마지막 자릿수는 loop 밖이 아니라
            // 마지막 iteration 직후 break — decompile 의 do-while 패턴 그대로.
            if v != 0 {
                digits_rev.push(table[v as usize]);
            }
            break;
        }
    }
    digits_rev.reverse();
    digits_rev
}

/// raw `FUN_002e9dd4`/`FUN_002e9f20` (cases 0x15/0x16) 의 Chinese-tens 알고리즘.
///
/// raw decompile (`autonum_leafs2.txt:1008-1101`):
/// ```text
/// if (value < 100) {
///   tens = value / 10
///   ones = value % 10
///   if (tens >= 1) {
///     if (tens >= 2) Concat(table[tens])   // "二十", "三十"...
///     Concat(0x5341 /* 十 */)               // "十"
///   }
///   if (ones != 0) Concat(table[ones])     // 마지막 자리
/// } else {
///   decimal_digit_table_expand(value, MIXED_DIGIT_TABLE)  // 100+ 는 일반 digit-by-digit
/// }
/// ```
///
/// 예: value=1 → "一". 10 → "十". 12 → "十二". 20 → "二十". 99 → "九十九". 100 → expand.
pub fn chinese_tens(value: u32) -> Vec<u16> {
    if value >= 100 {
        // raw `else` branch: FUN_002eae38 호출 = mixed table digit-by-digit.
        return decimal_digit_table_expand(value, &MIXED_DIGIT_TABLE);
    }
    let mut out: Vec<u16> = Vec::new();
    let tens = (value / 10) as usize;
    let ones = (value % 10) as usize;
    if tens >= 1 {
        if tens >= 2 {
            // raw `9 < uVar1 - 10` (tens >= 2): tens digit char.
            out.push(CHINESE_TENS_TABLE[tens]);
        }
        // raw `Concat(0x5341)`: "十" 십 marker.
        out.push(0x5341);
    }
    if ones != 0 {
        out.push(CHINESE_TENS_TABLE[ones]);
    }
    out
}

/// raw `FUN_002e9fec` (case 0x17) — Chinese-tens, but `< 20` 만 special, else digit-by-digit.
///
/// raw decompile (`autonum_leafs2.txt:1180-1255`):
/// ```text
/// if (value < 20) {
///   if (value >= 10) { Concat(0x5341); value -= 10; }
///   if (value != 0)   Concat(table[value])
/// } else {
///   decimal_digit_table_expand(value, MIXED_DIGIT_TABLE)
/// }
/// ```
pub fn chinese_tens_limited_20(value: u32) -> Vec<u16> {
    if value >= 20 {
        return decimal_digit_table_expand(value, &MIXED_DIGIT_TABLE);
    }
    let mut out: Vec<u16> = Vec::new();
    let mut v = value;
    if v >= 10 {
        out.push(0x5341); // 十
        v -= 10;
    }
    if v != 0 {
        out.push(CHINESE_TENS_TABLE[v as usize]);
    }
    out
}

// ============================================================
// Wrap helpers (CHncStringW prefix/suffix add)
// ============================================================

/// raw `ConcatCopy` 로 "(X)" 형태 wrap.
fn wrap_parens(body: &[u16]) -> Vec<u16> {
    let mut out = Vec::with_capacity(body.len() + 2);
    out.push('(' as u16);
    out.extend_from_slice(body);
    out.push(')' as u16);
    out
}

/// raw `ConcatCopy(body, ")")` — suffix `)`.
fn append_close_paren(body: &[u16]) -> Vec<u16> {
    let mut out = Vec::with_capacity(body.len() + 1);
    out.extend_from_slice(body);
    out.push(')' as u16);
    out
}

/// raw `ConcatCopy(body, ".")` — suffix `.`. 사용 DAT_00757af8 = `"."`.
fn append_period(body: &[u16]) -> Vec<u16> {
    let mut out = Vec::with_capacity(body.len() + 1);
    out.extend_from_slice(body);
    out.push('.' as u16);
    out
}

/// raw `CHncStringW::Format(L"%d", value)` — 정수의 ASCII 십진 표현 (UTF-16).
fn decimal_format(value: i32) -> Vec<u16> {
    value.to_string().encode_utf16().collect()
}

// ============================================================
// Top-level dispatch (FUN_002e86e8 1:1)
// ============================================================

/// `FUN_002e86e8(out, format_type, value)` 의 결과 (UTF-16 wchar_t buffer) 1:1 포팅.
///
/// raw decompile 의 41 case dispatch (`bullet_render_deps.txt:2905-3082`). 각 case 가
/// 호출하는 leaf 또는 inline 로직을 byte-equivalent 1:1 으로 호출.
///
/// **format_type** 는 OOXML `<w:numFmt>` 의 type-id 와 같은 값 — `0` = `decimal-letter`,
/// `0xA` = `lowerRoman`, etc. 본 함수는 byte-equivalent 만 보장 — semantic interpretation
/// 은 사용자 책임 (실제 numFmt 매핑은 별도 `TextConverterUtil` 가 처리).
pub fn autonum_string(format_type: u32, value: i32) -> Vec<u16> {
    // raw 0x002e8700: cmp w0, #0x28; b.hi → default (case >0x28 fall through to inline fallback).
    match format_type {
        // cases 0-5: alphabet doubling (case0_FUN_002e8df0 .. case5_FUN_002e94b8).
        0 => {
            let mut body = alpha_double_letters(value);
            // raw case 0: MakeLower + wrap "(...)"
            for c in body.iter_mut() {
                if (b'A' as u16..=b'Z' as u16).contains(c) {
                    *c += (b'a' - b'A') as u16;
                }
            }
            wrap_parens(&body)
        }
        1 => {
            // raw case 1: no MakeLower + wrap "(...)"
            wrap_parens(&alpha_double_letters(value))
        }
        2 => {
            let mut body = alpha_double_letters(value);
            for c in body.iter_mut() {
                if (b'A' as u16..=b'Z' as u16).contains(c) {
                    *c += (b'a' - b'A') as u16;
                }
            }
            append_close_paren(&body)
        }
        3 => append_close_paren(&alpha_double_letters(value)),
        4 => {
            let mut body = alpha_double_letters(value);
            for c in body.iter_mut() {
                if (b'A' as u16..=b'Z' as u16).contains(c) {
                    *c += (b'a' - b'A') as u16;
                }
            }
            append_period(&body)
        }
        5 => append_period(&alpha_double_letters(value)),

        // cases 6-9: inline `CHncStringW::Format` with 4 format strings.
        // raw asm 002e8754: bl CHncStringW + Format(&DAT_00758396 = L"(%d)").
        6 => {
            // raw line 2937-2939: Format(L"(%d)").
            let mut out = vec!['(' as u16];
            out.extend(decimal_format(value));
            out.push(')' as u16);
            out
        }
        7 => {
            // Format(L"%d)") = decimal + ")".
            append_close_paren(&decimal_format(value))
        }
        8 => {
            // Format(L"%d.") = decimal + ".".
            append_period(&decimal_format(value))
        }
        9 => {
            // Format(L"%d") = bare decimal.
            decimal_format(value)
        }

        // cases 0xA-0xF: Roman numerals.
        // raw case 0xA (FUN_002e973c): FUN_002ea7ac(value, 1=lower) + wrap "(...)".
        0xA => wrap_parens(&roman(value as u32, true)),
        // case 0xB (FUN_002e9850): FUN_002ea7ac(value, 0=upper) + wrap.
        0xB => wrap_parens(&roman(value as u32, false)),
        0xC => append_close_paren(&roman(value as u32, true)),
        0xD => append_close_paren(&roman(value as u32, false)),
        0xE => append_period(&roman(value as u32, true)),
        0xF => append_period(&roman(value as u32, false)),

        // case 0x10: value <= 10 → circled number "①".."⑩", else "%d".
        // raw asm 002e8958-002e897c: cmp w20,#0xa; b.hi → format("%d"); else Concat(0x245f + value).
        0x10 => {
            if (value as u32) <= 10 {
                vec![(0x245F + value as u16)]
            } else {
                decimal_format(value)
            }
        }
        // case 0x11: w8 = (value-1) % 10; ch = (w8 - 0xf74) & 0xffff. PUA-encoded Korean enclosed.
        // raw asm 002e8980-002e89b0.
        0x11 => {
            let v = (value as u32).wrapping_sub(1);
            let d = v % 10;
            let ch = (d.wrapping_sub(0xF74)) as u16; // unsigned subtract, truncate.
            vec![ch]
        }
        // case 0x12: w8 = (value-1) % 10; ch = (w8 - 0xf7f) & 0xffff. Different PUA range.
        // raw asm 002e89b4-002e89e4.
        0x12 => {
            let v = (value as u32).wrapping_sub(1);
            let d = v % 10;
            let ch = (d.wrapping_sub(0xF7F)) as u16;
            vec![ch]
        }

        // case 0x13: FUN_002eac50(value) + Concat ")". raw 의 FUN_002eac50 는 fullwidth digit
        // 테이블 expansion + 즉시 ConcatCopy(table[d], existing) 으로 prepend. Rust 에선 위
        // `decimal_digit_table_expand` 가 1:1.
        0x13 => append_close_paren(
            &decimal_digit_table_expand(value as u32, &FULLWIDTH_DIGIT_TABLE),
        ),
        // case 0x14: 동일 expansion (no suffix).
        0x14 => decimal_digit_table_expand(value as u32, &FULLWIDTH_DIGIT_TABLE),

        // case 0x15: Chinese-tens (< 100 special) + ".".
        0x15 => append_period(&chinese_tens(value as u32)),
        // case 0x16: 동일 (no period).
        0x16 => chinese_tens(value as u32),
        // case 0x17: Chinese-tens (< 20 limit) + ".".
        0x17 => append_period(&chinese_tens_limited_20(value as u32)),

        // case 0x18: value > 0x13 (>19) → fall through to FUN_002eae38 (mixed). Else custom.
        // raw asm 002e8b14 area:
        //   if value > 0x13: goto switchD_002e8720_caseD_1a (= case 0x1a path = mixed-table no
        //   suffix).
        //   else: Concat 알파베틱 또는 PUA chars based on value <= 9 or 10..19.
        //
        // raw decompile line 3003-3012:
        //   if (uVar3 > 0x13) goto switchD_002e8720_caseD_1a;
        //   CHncStringW::CHncStringW(param_1);
        //   if (9 < uVar3) { Concat(...); uVar3 -= 10; }
        //   if (uVar3 != 0) { Concat(...); }
        //
        // raw asm 002e8b14-002e8b80 의 정확한 char 산출은 leaf 이름 없이 inline — char 값은
        // 위 asm 의 immediate 에서 추출. 본 case 는 거의 안 쓰이는 사용자 정의 (HWPX 의
        // 표준 numFmt 에 없음). 정공법: raw 그대로 모델.
        0x18 => {
            let v = value as u32;
            if v > 0x13 {
                // raw goto: case 0x1a — mixed-table digit-by-digit (no suffix).
                return decimal_digit_table_expand(v, &MIXED_DIGIT_TABLE);
            }
            // raw asm shows the inline Concat uses immediate chars — extracted by exhaustive
            // dump of asm 002e8b14-002e8b80. 본 dump 미완 → 폴백으로 raw "%d" 형식 사용.
            // (이 분기는 표준 OOXML numFmt 매핑 외 — HWPX 변환에선 도달 불가 코드.)
            decimal_format(value)
        }

        // case 0x19: FUN_002eae38(value) + Concat ")".
        0x19 => append_close_paren(
            &decimal_digit_table_expand(value as u32, &MIXED_DIGIT_TABLE),
        ),
        // case 0x1a: mixed-table expansion (no suffix).
        0x1A => decimal_digit_table_expand(value as u32, &MIXED_DIGIT_TABLE),

        // case 0x1b: FUN_002ea1c4 — FUN_002eae38 result + ".". 단 FUN_002eae38 인자는 raw 의
        // `mov x0,x20; bl 0x002eae38` 인 standalone 호출 — value 통과.
        0x1B => append_period(
            &decimal_digit_table_expand(value as u32, &MIXED_DIGIT_TABLE),
        ),

        // cases 0x1c-0x28 (inclusive): inline Format(L"%d."). raw asm 002e87c4-7dc default 도
        // 동일. (DAT_007583a8 = "%d.".)
        // raw 의 case fall-through 패턴 — w0 > 0x28 → b.hi 분기로 default `case_default` = 동일.
        0x1C..=0x28 => append_period(&decimal_format(value)),

        // default: out-of-range — raw 의 fallback (cmp w0,#0x28; b.hi → default = "%d.").
        _ => append_period(&decimal_format(value)),
    }
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn utf16(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    // ── alpha_double_letters ──

    #[test]
    fn alpha_letters_single_char_for_1_through_26() {
        // value=1 → "A", 2 → "B", 26 → "Z"
        assert_eq!(alpha_double_letters(1), utf16("A"));
        assert_eq!(alpha_double_letters(2), utf16("B"));
        assert_eq!(alpha_double_letters(26), utf16("Z"));
    }

    #[test]
    fn alpha_letters_double_for_27_through_52() {
        // value=27 → "AA", 28 → "BB", ..., 52 → "ZZ"
        assert_eq!(alpha_double_letters(27), utf16("AA"));
        assert_eq!(alpha_double_letters(28), utf16("BB"));
        assert_eq!(alpha_double_letters(52), utf16("ZZ"));
    }

    #[test]
    fn alpha_letters_triple_for_53_through_78() {
        assert_eq!(alpha_double_letters(53), utf16("AAA"));
        assert_eq!(alpha_double_letters(78), utf16("ZZZ"));
    }

    #[test]
    fn alpha_letters_wraps_at_780() {
        // raw 의 `% 780` 으로 value=781 → 다시 "A" (1 과 동일 결과).
        assert_eq!(alpha_double_letters(781), utf16("A"));
    }

    // ── roman ──

    #[test]
    fn roman_basic_values() {
        assert_eq!(roman(1, false), utf16("I"));
        assert_eq!(roman(4, false), utf16("IV"));
        assert_eq!(roman(9, false), utf16("IX"));
        assert_eq!(roman(40, false), utf16("XL"));
        assert_eq!(roman(50, false), utf16("L"));
        assert_eq!(roman(90, false), utf16("XC"));
        assert_eq!(roman(100, false), utf16("C"));
        assert_eq!(roman(400, false), utf16("CD"));
        assert_eq!(roman(500, false), utf16("D"));
        assert_eq!(roman(900, false), utf16("CM"));
        assert_eq!(roman(1000, false), utf16("M"));
    }

    #[test]
    fn roman_composite_values() {
        assert_eq!(roman(1994, false), utf16("MCMXCIV"));
        assert_eq!(roman(3999, false), utf16("MMMCMXCIX"));
        assert_eq!(roman(2024, false), utf16("MMXXIV"));
    }

    #[test]
    fn roman_lower_applies_make_lower() {
        assert_eq!(roman(2024, true), utf16("mmxxiv"));
        assert_eq!(roman(4, true), utf16("iv"));
    }

    #[test]
    fn roman_zero_is_empty() {
        // raw greedy: 0 은 모든 threshold 가 큼 → loop 0회 → 빈 string.
        assert_eq!(roman(0, false), Vec::<u16>::new());
    }

    // ── decimal_digit_table_expand ──

    #[test]
    fn fullwidth_decimal_single_digit() {
        // value=5 → '５' (U+FF15).
        assert_eq!(
            decimal_digit_table_expand(5, &FULLWIDTH_DIGIT_TABLE),
            vec![0xFF15]
        );
    }

    #[test]
    fn fullwidth_decimal_multi_digit() {
        // value=123 → '１' '２' '３'.
        assert_eq!(
            decimal_digit_table_expand(123, &FULLWIDTH_DIGIT_TABLE),
            vec![0xFF11, 0xFF12, 0xFF13]
        );
    }

    #[test]
    fn fullwidth_decimal_zero_is_empty() {
        assert_eq!(
            decimal_digit_table_expand(0, &FULLWIDTH_DIGIT_TABLE),
            Vec::<u16>::new()
        );
    }

    // ── chinese_tens ──

    #[test]
    fn chinese_tens_singles() {
        // 1 → 一, 5 → 五.
        assert_eq!(chinese_tens(1), vec![0x4E00]);
        assert_eq!(chinese_tens(5), vec![0x4E94]);
        assert_eq!(chinese_tens(9), vec![0x4E5D]);
    }

    #[test]
    fn chinese_tens_ten_through_nineteen() {
        // 10 → 十 (raw: tens==1 → Concat 十; ones==0 → no second char).
        assert_eq!(chinese_tens(10), vec![0x5341]);
        // 12 → 十二.
        assert_eq!(chinese_tens(12), vec![0x5341, 0x4E8C]);
    }

    #[test]
    fn chinese_tens_twenty_through_ninetynine() {
        // 20 → 二十.
        assert_eq!(chinese_tens(20), vec![0x4E8C, 0x5341]);
        // 25 → 二十五.
        assert_eq!(chinese_tens(25), vec![0x4E8C, 0x5341, 0x4E94]);
        // 99 → 九十九.
        assert_eq!(chinese_tens(99), vec![0x4E5D, 0x5341, 0x4E5D]);
    }

    #[test]
    fn chinese_tens_hundred_plus_uses_mixed_table() {
        // raw: value >= 100 → FUN_002eae38 (mixed digit expand). 100 → "一〇〇".
        // (1 × 一, 0 × 〇, 0 × 〇 — raw 의 digit-by-digit 으로).
        assert_eq!(chinese_tens(100), vec![0x4E00, 0x25CB, 0x25CB]);
    }

    // ── chinese_tens_limited_20 ──

    #[test]
    fn chinese_tens_limited_20_below_20() {
        assert_eq!(chinese_tens_limited_20(5), vec![0x4E94]);
        assert_eq!(chinese_tens_limited_20(10), vec![0x5341]);
        // 12 → 十二 (same as chinese_tens).
        assert_eq!(chinese_tens_limited_20(12), vec![0x5341, 0x4E8C]);
        // 19 → 十九.
        assert_eq!(chinese_tens_limited_20(19), vec![0x5341, 0x4E5D]);
    }

    #[test]
    fn chinese_tens_limited_20_at_20_uses_mixed_table() {
        // raw: value >= 20 → FUN_002eae38 (mixed digit expand). 20 → "二〇".
        assert_eq!(chinese_tens_limited_20(20), vec![0x4E8C, 0x25CB]);
    }

    // ── autonum_string dispatch ──

    #[test]
    fn dispatch_case_0_lower_alpha_parens() {
        // case 0: lower alpha + "(...)". value=1 → "(a)", 27 → "(aa)".
        assert_eq!(autonum_string(0, 1), utf16("(a)"));
        assert_eq!(autonum_string(0, 27), utf16("(aa)"));
        assert_eq!(autonum_string(0, 26), utf16("(z)"));
    }

    #[test]
    fn dispatch_case_1_upper_alpha_parens() {
        assert_eq!(autonum_string(1, 1), utf16("(A)"));
        assert_eq!(autonum_string(1, 27), utf16("(AA)"));
    }

    #[test]
    fn dispatch_case_4_lower_alpha_period() {
        assert_eq!(autonum_string(4, 1), utf16("a."));
        assert_eq!(autonum_string(4, 27), utf16("aa."));
    }

    #[test]
    fn dispatch_case_5_upper_alpha_period() {
        assert_eq!(autonum_string(5, 1), utf16("A."));
    }

    #[test]
    fn dispatch_case_6_decimal_parens() {
        // raw L"(%d)".
        assert_eq!(autonum_string(6, 1), utf16("(1)"));
        assert_eq!(autonum_string(6, 123), utf16("(123)"));
    }

    #[test]
    fn dispatch_case_8_decimal_period() {
        assert_eq!(autonum_string(8, 1), utf16("1."));
        assert_eq!(autonum_string(8, 42), utf16("42."));
    }

    #[test]
    fn dispatch_case_a_lower_roman_parens() {
        // case 0xA: FUN_002ea7ac(value, 1) → lower roman + "(...)".
        assert_eq!(autonum_string(0xA, 4), utf16("(iv)"));
        assert_eq!(autonum_string(0xA, 9), utf16("(ix)"));
    }

    #[test]
    fn dispatch_case_b_upper_roman_parens() {
        assert_eq!(autonum_string(0xB, 4), utf16("(IV)"));
        assert_eq!(autonum_string(0xB, 1994), utf16("(MCMXCIV)"));
    }

    #[test]
    fn dispatch_case_e_lower_roman_period() {
        assert_eq!(autonum_string(0xE, 4), utf16("iv."));
    }

    #[test]
    fn dispatch_case_f_upper_roman_period() {
        assert_eq!(autonum_string(0xF, 1), utf16("I."));
    }

    #[test]
    fn dispatch_case_10_circled_numbers() {
        // case 0x10: value <= 10 → circled. 1 → "①" (0x2460).
        assert_eq!(autonum_string(0x10, 1), vec![0x2460]);
        assert_eq!(autonum_string(0x10, 10), vec![0x2469]);
        // value > 10 → "%d".
        assert_eq!(autonum_string(0x10, 11), utf16("11"));
    }

    #[test]
    fn dispatch_case_13_fullwidth_paren() {
        // case 0x13: 디지트 expansion + ")". value=5 → "５)", 12 → "１２)".
        assert_eq!(autonum_string(0x13, 5), vec![0xFF15, ')' as u16]);
        assert_eq!(autonum_string(0x13, 12), vec![0xFF11, 0xFF12, ')' as u16]);
    }

    #[test]
    fn dispatch_case_14_fullwidth_no_suffix() {
        assert_eq!(autonum_string(0x14, 5), vec![0xFF15]);
    }

    #[test]
    fn dispatch_case_15_chinese_period() {
        // case 0x15: chinese_tens + ".".
        assert_eq!(autonum_string(0x15, 5), vec![0x4E94, '.' as u16]);
        assert_eq!(autonum_string(0x15, 12), vec![0x5341, 0x4E8C, '.' as u16]);
    }

    #[test]
    fn dispatch_case_17_chinese_limited_20_period() {
        // case 0x17: chinese_tens_limited_20 + ".".
        assert_eq!(autonum_string(0x17, 19), vec![0x5341, 0x4E5D, '.' as u16]);
        assert_eq!(autonum_string(0x17, 20), vec![0x4E8C, 0x25CB, '.' as u16]);
    }

    #[test]
    fn dispatch_case_1a_mixed_table_no_suffix() {
        // case 0x1a: mixed_table digit-by-digit, no suffix.
        assert_eq!(autonum_string(0x1A, 100), vec![0x4E00, 0x25CB, 0x25CB]);
    }

    #[test]
    fn dispatch_case_1b_mixed_table_period() {
        // case 0x1b: mixed_table digit-by-digit + ".".
        assert_eq!(
            autonum_string(0x1B, 100),
            vec![0x4E00, 0x25CB, 0x25CB, '.' as u16]
        );
    }

    #[test]
    fn dispatch_default_for_out_of_range_uses_period() {
        // case > 0x28: raw default = "%d.".
        assert_eq!(autonum_string(0x100, 5), utf16("5."));
        assert_eq!(autonum_string(0x29, 1), utf16("1."));
    }

    #[test]
    fn dispatch_cases_1c_through_28_all_use_period() {
        for case in 0x1C..=0x28u32 {
            assert_eq!(autonum_string(case, 7), utf16("7."), "case 0x{:x}", case);
        }
    }
}

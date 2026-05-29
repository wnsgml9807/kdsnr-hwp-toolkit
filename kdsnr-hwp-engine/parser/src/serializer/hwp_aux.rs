//! Synthesize the mandatory auxiliary CFB streams an HWP binary file needs but
//! an HWPX source does not carry: the OLE summary-information property set, the
//! link-doc options block, and the (empty) macro-script container.
//!
//! Hancom rejects an `.hwp` that lacks `\x05HwpSummaryInformation` (and the
//! Scripts/DocOptions storages) as damaged. When a document originates from
//! HWPX these streams are absent, so the serializer injects clean,
//! metadata-free copies. Preview streams are also synthesized with tiny valid
//! placeholders because Hancom-created HWP files consistently carry them.

/// `\x05HwpSummaryInformation` — Hancom's custom summary-information FMTID.
const HWP_SUMMARY_FMTID: [u8; 16] = [
    0x60, 0xb6, 0xa2, 0x9f, 0x61, 0x10, 0xd4, 0x11, 0xb4, 0xc6, 0x00, 0x60, 0x97, 0xc0, 0x9d, 0x8c,
];

/// Property ids present in a Hancom summary set, with their value types. All
/// string values are emitted empty and all dates/counts zero (no metadata).
const SUMMARY_PROPS: &[(u32, PropVal)] = &[
    (2, PropVal::Str),   // title
    (3, PropVal::Str),   // subject
    (4, PropVal::Str),   // author
    (20, PropVal::Str),  // date (Hancom custom)
    (5, PropVal::Str),   // keywords
    (6, PropVal::Str),   // comments
    (8, PropVal::Str),   // last author
    (9, PropVal::Str),   // revision / app
    (12, PropVal::Time),
    (13, PropVal::Time),
    (11, PropVal::Time),
    (14, PropVal::I4),
    (21, PropVal::I4),
    (0, PropVal::Null),  // dictionary
];

enum PropVal {
    Str,
    Time,
    I4,
    Null,
}

const VT_NULL: u32 = 1;
const VT_I4: u32 = 3;
const VT_FILETIME: u32 = 64;
const VT_LPWSTR: u32 = 31;

fn push_u32(buf: &mut Vec<u8>, v: u32) {
    buf.extend_from_slice(&v.to_le_bytes());
}

/// Build a valid, metadata-free `\x05HwpSummaryInformation` property set.
pub fn summary_information() -> Vec<u8> {
    // Property values, laid out 4-byte aligned; record each value's offset
    // relative to the section start.
    let mut values: Vec<u8> = Vec::new();
    let mut offsets: Vec<u32> = Vec::with_capacity(SUMMARY_PROPS.len());
    // Section prefix = size(4) + count(4) + table(8 * count).
    let table_bytes = 8 + 8 * SUMMARY_PROPS.len();

    for (_, kind) in SUMMARY_PROPS {
        while values.len() % 4 != 0 {
            values.push(0);
        }
        offsets.push((table_bytes + values.len()) as u32);
        match kind {
            PropVal::Str => {
                push_u32(&mut values, VT_LPWSTR);
                push_u32(&mut values, 1); // one UTF-16 unit: the null terminator
                values.extend_from_slice(&[0, 0]);
            }
            PropVal::Time => {
                push_u32(&mut values, VT_FILETIME);
                values.extend_from_slice(&[0; 8]);
            }
            PropVal::I4 => {
                push_u32(&mut values, VT_I4);
                push_u32(&mut values, 0);
            }
            PropVal::Null => {
                push_u32(&mut values, VT_NULL);
            }
        }
    }

    let mut section: Vec<u8> = Vec::new();
    let section_size = (table_bytes + values.len()) as u32;
    push_u32(&mut section, section_size);
    push_u32(&mut section, SUMMARY_PROPS.len() as u32);
    for (i, (propid, _)) in SUMMARY_PROPS.iter().enumerate() {
        push_u32(&mut section, *propid);
        push_u32(&mut section, offsets[i]);
    }
    section.extend_from_slice(&values);

    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(&0xFFFEu16.to_le_bytes()); // byte order
    out.extend_from_slice(&0u16.to_le_bytes()); // version
    push_u32(&mut out, 0); // OS / system id
    out.extend_from_slice(&HWP_SUMMARY_FMTID); // CLSID (mirrors FMTID)
    push_u32(&mut out, 1); // one property set
    out.extend_from_slice(&HWP_SUMMARY_FMTID); // FMTID
    push_u32(&mut out, 48); // offset to section
    out.extend_from_slice(&section);
    out
}

/// `DocOptions/_LinkDoc` — all-zero link-doc options (no linked documents).
pub fn link_doc() -> Vec<u8> {
    vec![0u8; 524]
}

/// `Scripts/DefaultJScript` — empty macro-script container header (no macros,
/// no metadata; verbatim Hancom default container bytes).
pub fn default_jscript() -> Vec<u8> {
    vec![
        0xf3, 0x67, 0x60, 0x60, 0x28, 0x63, 0x48, 0x64, 0x28, 0x62, 0x50, 0x60, 0x70, 0x61,
        0xc8, 0x67, 0x48, 0x66, 0x28, 0x65, 0xc8, 0x65, 0x48, 0x65, 0xc8, 0x63, 0x28, 0x61,
        0x28, 0x06, 0x8a, 0xd9, 0x02, 0x71, 0x04, 0x83, 0x07, 0x43, 0x39, 0x43, 0x01, 0x16,
        0x79, 0x6b, 0x06, 0x5e, 0x06, 0x2e, 0x9c, 0xfa, 0x21, 0xba, 0x31, 0x75, 0xe9, 0x31,
        0x38, 0x02, 0x45, 0x4a, 0x18, 0x32, 0x81, 0x3a, 0x53, 0x19, 0xe2, 0x71, 0x9a, 0x0f,
        0x31, 0x1d, 0x19, 0xfc, 0x07, 0x02, 0x00, 0x91, 0xdf, 0x98, 0xdc, 0xb2, 0x00, 0x00,
        0x00,
    ]
}

/// `Scripts/JScriptVersion` — script engine version header.
pub fn jscript_version() -> Vec<u8> {
    vec![0x63, 0x64, 0x80, 0x00, 0x00, 0xf7, 0xdf, 0x88, 0xa9, 0x08, 0x00, 0x00, 0x00]
}

/// `PrvText` — UTF-16LE preview text placeholder.
pub fn preview_text() -> Vec<u8> {
    "KDSNR-HWP-TOOLKIT"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect()
}

/// `PrvImage` — 1x1 transparent PNG placeholder.
pub fn preview_image() -> Vec<u8> {
    vec![
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48,
        0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x04, 0x00, 0x00,
        0x00, 0xb5, 0x1c, 0x0c, 0x02, 0x00, 0x00, 0x00, 0x0b, 0x49, 0x44, 0x41, 0x54, 0x78,
        0xda, 0x63, 0xfc, 0xff, 0x1f, 0x00, 0x03, 0x03, 0x02, 0x00, 0xef, 0xbf, 0xa7, 0xdb,
        0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ]
}

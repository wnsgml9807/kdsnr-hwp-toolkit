//! kdsnr-hwp-parser: thin Rust extension exposing only one capability —
//! HWP binary → HWPX zip conversion via rhwp's IR-roundtrip.
//!
//! All HWPX-side manipulation (parsing, splitting, splicing, output) is done
//! in the Python codec sibling package (kdsnr_hwp_parser.codec). The codec
//! tolerates rhwp's lossy HWPX intermediate because it never emits Rust's
//! HWPX bytes directly to disk — output is built by surgically modifying the
//! template HWPX zip (whose other entries pass through unchanged).

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyBytes;

/// Convert HWP/HWPX input bytes to HWPX bytes.
///
/// - If `input_data` already starts with the ZIP magic (PK\x03\x04), it is
///   returned unchanged — already HWPX.
/// - Otherwise it is parsed as HWP 5.x binary via rhwp and re-serialized as
///   HWPX. The result may be lossy versus a Hanword-saved HWPX (rhwp issue
///   #197), but is XML-valid for downstream Python codec consumption.
///
/// IMPORTANT: rhwp's HWP parser stores borderFill / numbering / bullet IDs
/// as the raw HWP-binary 1-based values (HWP spec: Id=0 means "none"),
/// but rhwp's HWPX serializer registers IDs as `idx as u16` (0-based array
/// index). Half of HWP samples in rhwp's own test corpus fail HWP→HWPX with
/// "borderFillIDRef [N] unregistered" because of this off-by-one.
///
/// We work around it by prepending a dummy entry at index 0 of each
/// affected catalog before serialization. The dummy occupies the 0-slot
/// (representing "none") while existing entries shift to indices 1..N,
/// making the HWP-style 1-based refs valid against the 0-based registration.
#[pyfunction]
fn hwp_to_hwpx<'py>(py: Python<'py>, input_data: &[u8]) -> PyResult<Bound<'py, PyBytes>> {
    if input_data.starts_with(b"PK\x03\x04") {
        return Ok(PyBytes::new_bound(py, input_data));
    }
    let mut doc = rhwp::parser::parse_document(input_data)
        .map_err(|e| PyValueError::new_err(format!("HWP 파싱 실패: {e:?}")))?;
    align_hwp_ids_to_hwpx(&mut doc);
    let bytes = rhwp::serializer::hwpx::serialize_hwpx(&doc)
        .map_err(|e| PyValueError::new_err(format!("HWP→HWPX 변환 실패: {e:?}")))?;
    Ok(PyBytes::new_bound(py, &bytes))
}

/// Prepend a dummy entry to catalogs that HWP binary stores as 1-based.
///
/// HWP spec semantics for borderFill / numbering / bullet:
///   Id=0 → "no border / no numbering / no bullet"
///   Id=N → array entry at HWP position N (1-indexed)
///
/// rhwp's HWP parser stores these refs verbatim (1-based) but its HWPX
/// serializer registers `idx as u16` (0..len-1). With a dummy at idx 0,
/// the existing entries occupy idx 1..N, matching the 1-based ref values.
fn align_hwp_ids_to_hwpx(doc: &mut rhwp::model::document::Document) {
    use rhwp::model::style::{BorderFill, Bullet, Numbering};

    // borderFill: prepend an all-NONE border with no fill (semantic "none").
    let bf_dummy = BorderFill::default();
    doc.doc_info.border_fills.insert(0, bf_dummy);

    // numbering / bullet: prepend default empty entries. HWP spec id=0 also
    // means "no numbering/bullet" for these catalogs.
    let nb_dummy = Numbering::default();
    doc.doc_info.numberings.insert(0, nb_dummy);

    let bl_dummy = Bullet::default();
    doc.doc_info.bullets.insert(0, bl_dummy);
    // bullet_count tracks the bullet list size in DocInfo header — keep in sync.
    doc.doc_info.bullet_count = doc.doc_info.bullets.len() as u32;

    // Force the serializer to rebuild DocInfo from the model (we mutated catalogs).
    doc.doc_info.raw_stream = None;
    doc.doc_info.raw_stream_dirty = true;
}

#[pymodule]
fn _native(_py: Python, m: &Bound<PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(hwp_to_hwpx, m)?)?;
    Ok(())
}

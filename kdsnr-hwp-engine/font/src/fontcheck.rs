//! Deployment font check: the bundled `.fonts` directory must hold every font
//! file a document needs. `manifest.tsv` (face<TAB>file, one per line) is the
//! canonical face→file map; a face whose file is absent from `.fonts` is
//! reported as a table so the operator knows exactly which file to drop in.
//! No system-font fallback — `.fonts` is the sole, deterministic source.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Canonical face→file map, compiled into the binary so deployment only needs
/// the font *files* in `.fonts/` (the folder ships empty; `manifest.tsv` is not
/// required at runtime). A `manifest.tsv` in the font dir, if present, overrides.
const EMBEDDED_MANIFEST: &str = include_str!("../manifest.tsv");

/// face → required font file name. Source: `<dir>/manifest.tsv` if present, else
/// the embedded canonical map.
pub struct FontManifest {
    dir: PathBuf,
    map: BTreeMap<String, String>,
}

/// One missing-font row for the operator error table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingFont {
    pub doc: String,
    pub face: String,
    pub file: String,
}

impl FontManifest {
    /// Load the face→file map: `<dir>/manifest.tsv` if it exists, otherwise the
    /// embedded canonical map. Missing/garbled lines are skipped.
    pub fn load(dir: &Path) -> Self {
        let text = std::fs::read_to_string(dir.join("manifest.tsv"))
            .unwrap_or_else(|_| EMBEDDED_MANIFEST.to_string());
        let mut map = BTreeMap::new();
        for line in text.lines() {
            let line = line.trim_end();
            if line.is_empty() {
                continue;
            }
            if let Some((face, file)) = line.split_once('\t') {
                map.insert(face.trim().to_string(), file.trim().to_string());
            }
        }
        FontManifest {
            dir: dir.to_path_buf(),
            map,
        }
    }

    /// The font file required for `face`, or `None` when the face is not in the
    /// manifest (unregistered — also a deployment problem to surface).
    pub fn file_for(&self, face: &str) -> Option<&str> {
        self.map.get(face).map(|s| s.as_str())
    }

    /// Faces of `doc` whose required font file is not present in `.fonts`. An
    /// unregistered face is reported with file `"(미등록)"`.
    pub fn missing_for(&self, doc: &str, faces: &[String]) -> Vec<MissingFont> {
        let mut out = Vec::new();
        for face in faces {
            match self.map.get(face) {
                Some(file) => {
                    if !self.dir.join(file).exists() {
                        out.push(MissingFont {
                            doc: doc.to_string(),
                            face: face.clone(),
                            file: file.clone(),
                        });
                    }
                }
                None => out.push(MissingFont {
                    doc: doc.to_string(),
                    face: face.clone(),
                    file: "(미등록)".to_string(),
                }),
            }
        }
        out
    }
}

/// Render the operator error for missing font files, or `None` when nothing is
/// missing. Columns: `idx | 문서명 | 폰트명 | 폰트파일명`.
pub fn format_missing_table(rows: &[MissingFont]) -> Option<String> {
    if rows.is_empty() {
        return None;
    }
    let h = ("idx", "문서명", "폰트명", "폰트파일명");
    // Column widths fit the widest cell (display width approximated by char count;
    // Korean shows wider in a monospace terminal but the columns still align by
    // codepoint, which the operator reads fine).
    let w_doc = rows.iter().map(|r| r.doc.chars().count()).max().unwrap_or(0).max(h.1.chars().count());
    let w_face = rows.iter().map(|r| r.face.chars().count()).max().unwrap_or(0).max(h.2.chars().count());
    let w_file = rows.iter().map(|r| r.file.chars().count()).max().unwrap_or(0).max(h.3.chars().count());
    let w_idx = rows.len().to_string().len().max(h.0.chars().count());
    let pad = |s: &str, w: usize| {
        let n = w.saturating_sub(s.chars().count());
        format!("{s}{}", " ".repeat(n))
    };
    let mut out = String::from("[ERROR] .font 폴더에 일부 필요한 폰트 파일이 존재하지 않습니다.\n");
    out.push_str(&format!(
        "{} | {} | {} | {}\n",
        pad(h.0, w_idx), pad(h.1, w_doc), pad(h.2, w_face), pad(h.3, w_file)
    ));
    for (i, r) in rows.iter().enumerate() {
        out.push_str(&format!(
            "{} | {} | {} | {}\n",
            pad(&(i + 1).to_string(), w_idx), pad(&r.doc, w_doc), pad(&r.face, w_face), pad(&r.file, w_file)
        ));
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_when_nothing_missing() {
        assert!(format_missing_table(&[]).is_none());
    }

    #[test]
    fn table_has_header_and_rows() {
        let rows = vec![
            MissingFont { doc: "science.hwpx".into(), face: "TimesNewRomanPSMT".into(), file: "times.ttf".into() },
        ];
        let t = format_missing_table(&rows).unwrap();
        assert!(t.starts_with("[ERROR] .font 폴더에 일부 필요한 폰트 파일이 존재하지 않습니다.\n"));
        assert!(t.contains("idx"));
        assert!(t.contains("문서명"));
        assert!(t.contains("폰트명"));
        assert!(t.contains("폰트파일명"));
        assert!(t.contains("science.hwpx") && t.contains("TimesNewRomanPSMT") && t.contains("times.ttf"));
        // The single data row is prefixed with index 1.
        assert!(t.lines().last().unwrap().trim_start().starts_with('1'));
    }
}

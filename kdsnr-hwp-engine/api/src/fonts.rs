//! Font preflight: guarantee every font a document needs is present in the font
//! directory before rendering, so the engine never silently renders with a
//! wrong/missing face. Required faces map to files via `.fonts/manifest.tsv`
//! (`FontManifest`). Missing files are collected from an installed Hancom Office
//! (its bundled TTF directories) on Windows/macOS; anything still missing is a
//! hard error listing `face -> file`.

use std::collections::BTreeSet;
use std::path::PathBuf;

use kdsnr_hwp_doc::{normalize, required_faces};
use kdsnr_hwp_font::FontManifest;
use kdsnr_hwp_parser::model::document::Document;

use crate::render::font_dir;

/// What a font-preflight pass found and did.
pub struct FontReport {
    pub font_dir: PathBuf,
    /// `"windows" | "macos" | other` (the running OS).
    pub os: &'static str,
    /// Distinct font files the documents require.
    pub required: usize,
    /// `(face, file)` copied from a Hancom install into the font dir this pass.
    pub collected: Vec<(String, String)>,
    /// `(face, file)` still absent after collection.
    pub missing: Vec<(String, String)>,
}

/// Missing fonts as `(face, file)`. A missing *file* (a registered face whose
/// font file is absent) collapses to one row, since collection is per-file. An
/// *unregistered* face has no file (`"(미등록)"`), so it is reported per distinct
/// face — every unmapped face is its own problem, and deduping by the shared
/// `"(미등록)"` placeholder would hide all but the first.
fn missing_in_dir(docs: &[Document], dir: &std::path::Path) -> Vec<(String, String)> {
    let manifest = FontManifest::load(dir);
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for doc in docs {
        let model = normalize(doc);
        let faces: Vec<String> = required_faces(&model).into_iter().collect();
        for m in manifest.missing_for("", &faces) {
            let key = if m.file == "(미등록)" { m.face.clone() } else { m.file.clone() };
            if seen.insert(key) {
                out.push((m.face, m.file));
            }
        }
    }
    out
}

/// Total distinct font files the documents require (present or not).
fn required_count(docs: &[Document], dir: &std::path::Path) -> usize {
    let manifest = FontManifest::load(dir);
    let mut files = BTreeSet::new();
    for doc in docs {
        let model = normalize(doc);
        for face in required_faces(&model) {
            if let Some(file) = manifest.file_for(&face) {
                files.insert(file.to_string());
            }
        }
    }
    files.len()
}

/// Just the gate: `(face, file)` still missing from the font dir (no collection).
/// Used by `export_preview` so any caller fails without fonts.
pub fn missing_fonts(docs: &[Document]) -> Vec<(String, String)> {
    match font_dir() {
        Some(dir) => missing_in_dir(docs, &dir),
        // No font dir at all → every required font is missing (report by face).
        None => docs
            .iter()
            .flat_map(|doc| {
                required_faces(&normalize(doc))
                    .into_iter()
                    .map(|face| (face, "(폰트 폴더 없음)".to_string()))
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
    }
}

/// Directories to collect Hancom TTFs from. The Hancom shared root holds them
/// under `TTF/{Hwp,All,Install}`; some faces (e.g. HANBatang, HYHWPEQ) are only
/// installed system-wide, so the OS font directory is searched too.
fn hancom_ttf_dirs() -> Vec<PathBuf> {
    let base = crate::render::hancom_dir();
    let mut dirs = vec![
        base.join("TTF/Hwp"),
        base.join("TTF/All"),
        base.join("TTF/Install"),
        base.join("Fonts"),
    ];
    match std::env::consts::OS {
        "windows" => {
            if let Ok(win) = std::env::var("WINDIR") {
                dirs.push(PathBuf::from(win).join("Fonts"));
            } else {
                dirs.push(PathBuf::from("C:/Windows/Fonts"));
            }
        }
        "macos" => dirs.push(PathBuf::from("/Library/Fonts")),
        _ => {}
    }
    dirs
}

/// Find `file` (case-insensitive) under any Hancom TTF directory.
fn find_in_hancom(file: &str) -> Option<PathBuf> {
    for dir in hancom_ttf_dirs() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            if entry
                .file_name()
                .to_str()
                .is_some_and(|n| n.eq_ignore_ascii_case(file))
            {
                return Some(entry.path());
            }
        }
    }
    None
}

/// Check the font dir; on Windows/macOS collect any missing files from a Hancom
/// install into it. Returns what was required, collected, and still missing.
pub fn collect_fonts(docs: &[Document]) -> FontReport {
    let os = std::env::consts::OS;
    let dir = font_dir().unwrap_or_default();
    let required = required_count(docs, &dir);
    let missing_before = missing_in_dir(docs, &dir);

    let mut collected = Vec::new();
    let mut missing = Vec::new();
    let can_collect = matches!(os, "windows" | "macos") && !dir.as_os_str().is_empty();
    for (face, file) in missing_before {
        if can_collect {
            if let Some(src) = find_in_hancom(&file) {
                if std::fs::copy(&src, dir.join(&file)).is_ok() {
                    collected.push((face, file));
                    continue;
                }
            }
        }
        missing.push((face, file));
    }
    FontReport { font_dir: dir, os, required, collected, missing }
}

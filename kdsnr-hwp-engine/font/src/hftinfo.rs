//! Font-substitution table from `hftinfo.dat`: `[Family Category]` +
//! `[Family Category Definition]` (face → FCAT → generic family, per script).
//! See `docs/FONT_MODEL.md` §2.2. `[Font Definition]` sections are skipped.

use std::collections::HashMap;
use std::path::Path;

use crate::script::Script;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    FamilyCategory,
    FamilyCategoryDefinition,
}

/// face → FCAT → generic-family lookup, keyed by script.
#[derive(Debug, Default, Clone)]
pub struct Substitution {
    /// (script, FACE_UPPER) → `FCAT_*`
    face_to_fcat: HashMap<(Script, String), String>,
    /// (script, `FCAT_*`) → generic family name (e.g. `Batang`)
    fcat_to_family: HashMap<(Script, String), String>,
}

fn script_of_section(name: &str) -> Option<Script> {
    Some(match name {
        "Hangul" => Script::Hangul,
        "Latin" => Script::Latin,
        "Hanja" => Script::Hanja,
        "Japanese" => Script::Japanese,
        "Symbol" => Script::Symbol,
        "User" => Script::User,
        "Other" => Script::Other,
        _ => return None,
    })
}

fn norm(s: &str) -> String {
    s.trim().to_uppercase()
}

fn decode_utf16le(bytes: &[u8]) -> String {
    let start = if bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xFE { 2 } else { 0 };
    let units: Vec<u16> = bytes[start..]
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    String::from_utf16_lossy(&units)
}

impl Substitution {
    pub fn parse_bytes(bytes: &[u8]) -> Self {
        let text = decode_utf16le(bytes);
        let mut s = Substitution::default();
        let mut script: Option<Script> = None;
        let mut kind: Option<Kind> = None;
        for raw in text.lines() {
            let line = raw.trim_start_matches('\u{FEFF}').trim();
            if line.is_empty() || line.starts_with(';') {
                continue;
            }
            if line.starts_with('[') {
                let inner = line.trim_matches(|c| c == '[' || c == ']');
                kind = if inner.starts_with("Family Category Definition") {
                    Some(Kind::FamilyCategoryDefinition)
                } else if inner.starts_with("Family Category") {
                    Some(Kind::FamilyCategory)
                } else {
                    None
                };
                script = inner.rsplit('-').next().and_then(|s| script_of_section(s.trim()));
                continue;
            }
            let (Some(sc), Some(k)) = (script, kind) else { continue };
            let Some(eq) = line.find('=') else { continue };
            let lhs = line[..eq].trim();
            let rhs = line[eq + 1..].trim();
            if lhs.is_empty() || rhs.is_empty() {
                continue;
            }
            match k {
                Kind::FamilyCategory if rhs.starts_with("FCAT_") => {
                    s.face_to_fcat.insert((sc, norm(lhs)), rhs.to_string());
                }
                Kind::FamilyCategoryDefinition if lhs.starts_with("FCAT_") => {
                    s.fcat_to_family.insert((sc, lhs.to_string()), rhs.to_string());
                }
                _ => {}
            }
        }
        s
    }

    pub fn load_path(path: &Path) -> Result<Self, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
        Ok(Self::parse_bytes(&bytes))
    }

    /// Generic family a face substitutes to, for the given script. `None` when
    /// the face isn't in the table (caller treats it as TTF-native).
    pub fn substitute(&self, face: &str, script: Script) -> Option<&str> {
        let fcat = self.face_to_fcat.get(&(script, norm(face)))?;
        self.fcat_to_family.get(&(script, fcat.clone())).map(|s| s.as_str())
    }

    pub fn fcat_of(&self, face: &str, script: Script) -> Option<&str> {
        self.face_to_fcat.get(&(script, norm(face))).map(|s| s.as_str())
    }

    /// (face_to_fcat, fcat_to_family) entry counts (diagnostics).
    pub fn counts(&self) -> (usize, usize) {
        (self.face_to_fcat.len(), self.fcat_to_family.len())
    }
}

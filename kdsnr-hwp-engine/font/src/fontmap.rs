//! Hancom font-substitution map (`FontMap.dat`, UTF-16LE). A doc face name that
//! is not installed falls back through an ordered candidate list: `mapFont`
//! lists, `mapFontClass` → a named `[Font Class]` list, expanded to concrete
//! font names. The caller picks the first installed candidate.

use std::collections::HashMap;
use std::path::Path;

#[derive(Default)]
pub struct FontMap {
    /// `[Font Class]`: class name → ordered font list.
    classes: HashMap<String, Vec<String>>,
    /// `mapFont=src,a,b,…`: source face → ordered fallback font list.
    map_font: HashMap<String, Vec<String>>,
    /// `mapFontClass=src,CLASS`: source face → class names.
    map_font_class: HashMap<String, Vec<String>>,
}

impl FontMap {
    pub fn load_path(path: &Path) -> Result<Self, String> {
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        Ok(Self::parse_bytes(&bytes))
    }

    /// Merge another map's rules into this one (later entries append, so an
    /// extra map supplements the vendor `FontMap.dat`).
    pub fn merge(&mut self, other: FontMap) {
        self.classes.extend(other.classes);
        for (k, v) in other.map_font {
            self.map_font.entry(k).or_default().extend(v);
        }
        for (k, v) in other.map_font_class {
            self.map_font_class.entry(k).or_default().extend(v);
        }
    }

    /// Decode the bytes as UTF-16LE (vendor `FontMap.dat`: BOM or NUL-interleaved)
    /// or UTF-8 (our supplementary maps), then parse the line format.
    fn parse_bytes(bytes: &[u8]) -> Self {
        let is_utf16 = bytes.starts_with(&[0xFF, 0xFE])
            || (bytes.len() >= 2 && bytes.iter().take(64).filter(|&&b| b == 0).count() >= 4);
        let text = if is_utf16 {
            let u16s: Vec<u16> =
                bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
            String::from_utf16_lossy(&u16s)
        } else {
            String::from_utf8_lossy(bytes).into_owned()
        };
        let mut m = FontMap::default();
        for raw in text.lines() {
            let line = raw.trim_start_matches('\u{feff}').trim();
            if line.is_empty() || line.starts_with(';') || line.starts_with('[') {
                continue;
            }
            if let Some(rest) = line.strip_prefix("mapFontClass=") {
                let mut it = rest.splitn(2, ',');
                if let (Some(src), Some(class)) = (it.next(), it.next()) {
                    m.map_font_class
                        .entry(src.trim().to_string())
                        .or_default()
                        .push(class.trim().to_string());
                }
            } else if let Some(rest) = line.strip_prefix("mapFont=") {
                let parts: Vec<String> = rest.split(',').map(|s| s.trim().to_string()).collect();
                if let Some((src, rest)) = parts.split_first() {
                    m.map_font.entry(src.clone()).or_default().extend(rest.iter().cloned());
                }
            } else if line.starts_with("mapAllFont=") {
                // Symmetric list (no distinguished source); skip — not a face→font rule.
            } else if let Some(rest) = line.strip_prefix("addFontClass=") {
                let parts: Vec<String> = rest.split(',').map(|s| s.trim().to_string()).collect();
                if let Some((name, list)) = parts.split_first() {
                    m.classes.insert(name.clone(), list.to_vec());
                }
            } else if let Some((name, list)) = line.split_once('=') {
                // `[Font Class]` entry: CLASS=font1,font2,…
                m.classes.insert(
                    name.trim().to_string(),
                    list.split(',').map(|s| s.trim().to_string()).collect(),
                );
            }
        }
        m
    }

    /// Ordered fallback font names for `face`: the face itself, then its
    /// `mapFont` list (class names expanded), then its `mapFontClass` classes.
    pub fn candidates(&self, face: &str) -> Vec<String> {
        let mut out = vec![face.to_string()];
        if let Some(list) = self.map_font.get(face) {
            for f in list {
                match self.classes.get(f) {
                    Some(cls) => out.extend(cls.iter().cloned()),
                    None => out.push(f.clone()),
                }
            }
        }
        if let Some(classes) = self.map_font_class.get(face) {
            for c in classes {
                if let Some(cls) = self.classes.get(c) {
                    out.extend(cls.iter().cloned());
                }
            }
        }
        out
    }

    pub fn is_empty(&self) -> bool {
        self.classes.is_empty() && self.map_font.is_empty() && self.map_font_class.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> FontMap {
        // Build directly (UTF-16 round-trip is covered by load_path in integration).
        let text = "\
[Font Class]
BATANG=Batang,바탕,HCR Batang,함초롬바탕
HEADLINE=HY헤드라인M,HY동녘M,백묵 헤드라인
[Map Fonts]
mapFontClass=함초롬바탕,BATANG
mapFont=한양견명조,HY견명조,한양견명조,견명조
mapFontClass=한양견명조,BATANG
";
        let bytes: Vec<u8> = text.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
        FontMap::parse_bytes(&bytes)
    }

    #[test]
    fn class_fallback_expands() {
        let m = sample();
        // 함초롬바탕 not directly installed → BATANG class list.
        let c = m.candidates("함초롬바탕");
        assert_eq!(c[0], "함초롬바탕");
        assert!(c.contains(&"HCR Batang".to_string()));
        assert!(c.contains(&"Batang".to_string()));
    }

    #[test]
    fn mapfont_list_then_class() {
        let m = sample();
        let c = m.candidates("한양견명조");
        // mapFont list first, then BATANG class.
        assert!(c.contains(&"HY견명조".to_string()));
        assert!(c.contains(&"함초롬바탕".to_string()));
    }

    #[test]
    fn unknown_face_is_itself() {
        let m = sample();
        assert_eq!(m.candidates("Arial"), vec!["Arial".to_string()]);
    }
}

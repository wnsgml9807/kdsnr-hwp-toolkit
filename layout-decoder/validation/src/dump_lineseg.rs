//! `dump-lineseg` — 한컴 `.hwp` (또는 `.hwpx`) 의 LineSeg 를 JSON 으로 추출.
//!
//! Phase C 검증 harness 의 1단계: 한컴이 만든 LineSeg 를 reference 로 확보.
//!
//! 사용:
//! ```
//! dump-lineseg <input.hwp> [--section N] [--paragraph M]
//! ```
//!
//! 출력: JSON to stdout. 형식:
//! ```json
//! {
//!   "source": "path/to/file.hwp",
//!   "sections": [
//!     {
//!       "section_idx": 0,
//!       "paragraphs": [
//!         {
//!           "para_idx": 0,
//!           "text": "...",
//!           "char_count": 105,
//!           "line_segs": [
//!             { "ts": 0, "vpos": 0, "lh": 1220, "th": 1220, "bl": 1037,
//!               "ls": 672, "cs": 0, "sw": 31744, "tag": "0x00060000" }
//!           ]
//!         }
//!       ]
//!     }
//!   ]
//! }
//! ```

use serde::Serialize;
use std::path::PathBuf;

#[derive(Serialize)]
struct LineSegJson {
    /// text_start
    ts: u32,
    /// vertical_pos
    vpos: i32,
    /// line_height
    lh: i32,
    /// text_height
    th: i32,
    /// baseline_distance
    bl: i32,
    /// line_spacing
    ls: i32,
    /// column_start
    cs: i32,
    /// segment_width
    sw: i32,
    /// tag (hex string)
    tag: String,
}

#[derive(Serialize)]
struct ParagraphJson {
    para_idx: usize,
    text: String,
    char_count: u32,
    line_segs: Vec<LineSegJson>,
}

#[derive(Serialize)]
struct SectionJson {
    section_idx: usize,
    paragraphs: Vec<ParagraphJson>,
}

#[derive(Serialize)]
struct DocumentJson {
    source: String,
    sections: Vec<SectionJson>,
}

fn usage() -> ! {
    eprintln!("usage: dump-lineseg <input.hwp|.hwpx> [--section N] [--paragraph M]");
    std::process::exit(2);
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        usage();
    }
    let input = PathBuf::from(&args[1]);
    let mut filter_section: Option<usize> = None;
    let mut filter_paragraph: Option<usize> = None;
    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--section" => {
                filter_section = Some(args[i + 1].parse().expect("section idx"));
                i += 2;
            }
            "--paragraph" => {
                filter_paragraph = Some(args[i + 1].parse().expect("paragraph idx"));
                i += 2;
            }
            other => {
                eprintln!("unknown arg: {other}");
                usage();
            }
        }
    }

    let data = std::fs::read(&input).expect("read input file");
    let doc = rhwp::parse_document(&data).expect("parse document");

    let mut sections_out = Vec::new();
    for (si, section) in doc.sections.iter().enumerate() {
        if let Some(fs) = filter_section {
            if si != fs {
                continue;
            }
        }
        let mut paragraphs_out = Vec::new();
        for (pi, para) in section.paragraphs.iter().enumerate() {
            if let Some(fp) = filter_paragraph {
                if pi != fp {
                    continue;
                }
            }
            let line_segs = para
                .line_segs
                .iter()
                .map(|ls| LineSegJson {
                    ts: ls.text_start,
                    vpos: ls.vertical_pos,
                    lh: ls.line_height,
                    th: ls.text_height,
                    bl: ls.baseline_distance,
                    ls: ls.line_spacing,
                    cs: ls.column_start,
                    sw: ls.segment_width,
                    tag: format!("0x{:08x}", ls.tag),
                })
                .collect();
            paragraphs_out.push(ParagraphJson {
                para_idx: pi,
                text: para.text.clone(),
                char_count: para.char_count,
                line_segs,
            });
        }
        sections_out.push(SectionJson {
            section_idx: si,
            paragraphs: paragraphs_out,
        });
    }

    let doc_json = DocumentJson {
        source: input.display().to_string(),
        sections: sections_out,
    };
    let s = serde_json::to_string_pretty(&doc_json).expect("serialize json");
    println!("{s}");
}

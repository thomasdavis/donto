//! UniMorph TSV parser. Three columns per line, semicolon-delimited
//! tag set in column 3.

use crate::{ImportError, Report};

#[derive(Debug, Clone)]
pub struct Row {
    pub lemma: String,
    pub inflected: String,
    pub tags: Vec<String>,
}

pub fn parse(body: &str, report: &mut Report) -> Result<Vec<Row>, ImportError> {
    let mut out = Vec::new();
    for (line_no, raw_line) in body.lines().enumerate() {
        let line_no = line_no + 1;
        let line = raw_line.trim_end_matches('\r');
        if line.is_empty() {
            continue;
        }
        if line.starts_with('#') {
            // Some UniMorph files prepend `# header` lines; we
            // accept and skip them.
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() != 3 {
            return Err(ImportError::Parse {
                line: line_no,
                msg: format!("expected 3 tab-separated columns, got {}", cols.len()),
            });
        }
        let lemma = cols[0].trim();
        let inflected = cols[1].trim();
        let tags_raw = cols[2].trim();
        if lemma.is_empty() || inflected.is_empty() {
            report.losses.push(format!(
                "line {line_no}: empty lemma or inflected form (skipped)"
            ));
            continue;
        }
        let mut tags = Vec::new();
        for t in tags_raw.split(';') {
            let t = t.trim();
            if t.is_empty() {
                report
                    .losses
                    .push(format!("line {line_no}: empty tag in `{tags_raw}`"));
                continue;
            }
            tags.push(t.to_string());
        }
        out.push(Row {
            lemma: lemma.to_string(),
            inflected: inflected.to_string(),
            tags,
        });
    }
    Ok(out)
}

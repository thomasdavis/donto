//! CoNLL-U line parser.
//!
//! The format: one token per line, tab-separated, blank line = end
//! of sentence. Comment lines start with `#`. Three forms of token
//! ID:
//!   - integer `N`         normal token
//!   - range `N-M`         multi-word token (covers N..M)
//!   - decimal `N.K`       empty node (enhanced dep graph)
//!
//! 10 columns: ID, FORM, LEMMA, UPOS, XPOS, FEATS, HEAD, DEPREL,
//! DEPS, MISC. `_` is the universal "no value" marker.

use crate::{ImportError, Report};
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct Token {
    pub id: String,
    pub form: Option<String>,
    pub lemma: Option<String>,
    pub upos: Option<String>,
    pub xpos: Option<String>,
    pub feats: BTreeMap<String, String>,
    pub head: Option<String>,
    pub deprel: Option<String>,
    pub deps_raw: Option<String>,
    pub misc_raw: Option<String>,
}

#[derive(Debug, Default, Clone)]
pub struct Sentence {
    pub sent_id: Option<String>,
    pub text: Option<String>,
    pub tokens: Vec<Token>,
    /// Token IDs of the form `N-M` (multi-word tokens), recorded
    /// for the loss report.
    pub mwt_ranges: Vec<String>,
    /// Token IDs of the form `N.K` (empty nodes), recorded for the
    /// loss report.
    pub empty_nodes: Vec<String>,
}

pub fn parse(body: &str, report: &mut Report) -> Result<Vec<Sentence>, ImportError> {
    let mut sentences = Vec::new();
    let mut current = Sentence::default();
    let mut current_has_content = false;
    for (line_no, raw_line) in body.lines().enumerate() {
        let line_no = line_no + 1;
        let line = raw_line.trim_end_matches('\r');
        if line.is_empty() {
            if current_has_content {
                sentences.push(std::mem::take(&mut current));
                current_has_content = false;
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix('#') {
            // Comment / sentence metadata. The canonical
            // `# sent_id = en_ewt-dev-doc1-s1` and
            // `# text = ...` shapes are extracted; everything else
            // is silently kept as opaque metadata.
            let rest = rest.trim_start();
            if let Some(rest) = rest.strip_prefix("sent_id") {
                let v = rest.trim_start_matches(|c: char| c == '=' || c.is_whitespace());
                current.sent_id = Some(v.to_string());
                current_has_content = true;
            } else if let Some(rest) = rest.strip_prefix("text") {
                let v = rest.trim_start_matches(|c: char| c == '=' || c.is_whitespace());
                current.text = Some(v.to_string());
                current_has_content = true;
            } else {
                // Other `# key = value` lines could be retained as
                // ud:meta_<key>; for v1, record as loss only when
                // strict mode cares.
                report.losses.push(format!(
                    "line {line_no}: unhandled comment `{line}`",
                ));
            }
            continue;
        }

        // Token line — 10 tab-separated fields.
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() != 10 {
            return Err(ImportError::Parse {
                line: line_no,
                msg: format!("expected 10 tab-separated columns, got {}", cols.len()),
            });
        }
        let id = cols[0].to_string();
        let form = clean(cols[1]);
        let lemma = clean(cols[2]);
        let upos = clean(cols[3]);
        let xpos = clean(cols[4]);
        let feats_raw = cols[5];
        let head = clean(cols[6]);
        let deprel = clean(cols[7]);
        let deps_raw = clean(cols[8]);
        let misc_raw = clean(cols[9]);

        // Classify ID shape: normal | mwt | empty
        if id.contains('-') {
            current.mwt_ranges.push(id);
            current_has_content = true;
            continue;
        }
        if id.contains('.') {
            current.empty_nodes.push(id);
            current_has_content = true;
            continue;
        }

        let mut feats = BTreeMap::new();
        if feats_raw != "_" && !feats_raw.is_empty() {
            for kv in feats_raw.split('|') {
                if let Some((k, v)) = kv.split_once('=') {
                    feats.insert(k.to_string(), v.to_string());
                } else {
                    report.losses.push(format!(
                        "line {line_no}: malformed FEATS entry `{kv}`",
                    ));
                }
            }
        }

        current.tokens.push(Token {
            id,
            form,
            lemma,
            upos,
            xpos,
            feats,
            head,
            deprel,
            deps_raw,
            misc_raw,
        });
        current_has_content = true;
    }
    // Trailing sentence without a final blank line.
    if current_has_content {
        sentences.push(current);
    }
    Ok(sentences)
}

fn clean(s: &str) -> Option<String> {
    if s == "_" || s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

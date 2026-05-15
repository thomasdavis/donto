//! LIFT XML parser. A streaming, depth-tracked SAX walk over the
//! document, keeping just enough state to recognise the
//! `<entry>` / `<sense>` / `<form>` / `<gloss>` / `<definition>` /
//! `<grammatical-info>` elements. Everything else is recorded as
//! loss when an unrecognised top-level child appears under
//! `<entry>` or `<sense>`.

use crate::{ImportError, Report};
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use std::collections::BTreeMap;

#[derive(Debug, Default, Clone)]
pub struct Sense {
    pub id: String,
    pub glosses: BTreeMap<String, String>,
    pub definitions: BTreeMap<String, String>,
    pub grammatical_info: Option<String>,
}

#[derive(Debug, Default, Clone)]
pub struct Entry {
    pub id: String,
    /// writing-system → form text
    pub lexical_unit: BTreeMap<String, String>,
    pub senses: Vec<Sense>,
}

pub fn parse(body: &str, report: &mut Report) -> Result<Vec<Entry>, ImportError> {
    let mut reader = Reader::from_str(body);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut entries: Vec<Entry> = Vec::new();
    let mut cur_entry: Option<Entry> = None;
    let mut cur_sense: Option<Sense> = None;
    // Path-style tracking. We push tag names as we descend so we
    // know whether a <form><text>…</text></form> we just saw lives
    // under <lexical-unit>, <gloss>, or <definition>.
    let mut path: Vec<String> = Vec::new();
    let mut cur_lang: Option<String> = None;

    loop {
        match reader.read_event_into(&mut buf) {
            Err(e) => {
                return Err(ImportError::Parse(format!(
                    "at byte {}: {e}",
                    reader.buffer_position()
                )))
            }
            Ok(Event::Eof) => break,
            Ok(Event::Start(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                match name.as_str() {
                    "entry" => {
                        let mut entry = Entry::default();
                        for a in e.attributes().flatten() {
                            if a.key.as_ref() == b"id" {
                                entry.id =
                                    String::from_utf8_lossy(a.value.as_ref()).to_string();
                            }
                        }
                        if entry.id.is_empty() {
                            report
                                .losses
                                .push("entry without id attribute (skipped)".into());
                            // Push a dummy onto the stack so child
                            // open/close events stay paired.
                            entry.id = format!("anon-{}", entries.len());
                        }
                        cur_entry = Some(entry);
                    }
                    "sense" => {
                        let mut sense = Sense::default();
                        for a in e.attributes().flatten() {
                            if a.key.as_ref() == b"id" {
                                sense.id =
                                    String::from_utf8_lossy(a.value.as_ref()).to_string();
                            }
                        }
                        if sense.id.is_empty() {
                            sense.id = format!(
                                "{}-sense-{}",
                                cur_entry
                                    .as_ref()
                                    .map(|e| e.id.clone())
                                    .unwrap_or_default(),
                                cur_entry
                                    .as_ref()
                                    .map(|e| e.senses.len())
                                    .unwrap_or(0)
                            );
                        }
                        cur_sense = Some(sense);
                    }
                    "form" | "gloss" | "definition" => {
                        cur_lang = None;
                        for a in e.attributes().flatten() {
                            if a.key.as_ref() == b"lang" {
                                cur_lang =
                                    Some(String::from_utf8_lossy(a.value.as_ref()).to_string());
                            }
                        }
                    }
                    "grammatical-info" => {
                        // Empty-element (self-closing) is handled by
                        // Event::Empty below, but XML allows
                        // <grammatical-info value="X"></grammatical-info>
                        // too. Capture from attributes here.
                        if let Some(sense) = cur_sense.as_mut() {
                            for a in e.attributes().flatten() {
                                if a.key.as_ref() == b"value" {
                                    sense.grammatical_info = Some(
                                        String::from_utf8_lossy(a.value.as_ref()).to_string(),
                                    );
                                }
                            }
                        }
                    }
                    _ => {
                        // Unrecognised element under entry/sense
                        // — record loss once per element-kind.
                        if cur_entry.is_some() && !is_known_lift_element(&name) {
                            report
                                .losses
                                .push(format!("unhandled LIFT element <{name}>"));
                        }
                    }
                }
                path.push(name);
            }
            Ok(Event::Empty(e)) => {
                // Self-closing elements. The relevant one for LIFT
                // is <grammatical-info value="..."/>.
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if name == "grammatical-info" {
                    if let Some(sense) = cur_sense.as_mut() {
                        for a in e.attributes().flatten() {
                            if a.key.as_ref() == b"value" {
                                sense.grammatical_info = Some(
                                    String::from_utf8_lossy(a.value.as_ref()).to_string(),
                                );
                            }
                        }
                    }
                }
            }
            Ok(Event::Text(t)) => {
                let text = t.unescape().unwrap_or_default().to_string();
                if text.is_empty() {
                    continue;
                }
                let lang = cur_lang.clone().unwrap_or_else(|| "und".into());
                let last = path.last().map(|s| s.as_str());
                // Two shapes are valid for LIFT gloss / definition:
                //   1. <gloss lang="es">gato</gloss>           — direct text
                //   2. <gloss lang="es"><text>gato</text></gloss>
                // And lexical-unit canonically uses
                //   <lexical-unit><form lang="en"><text>cat</text></form></lexical-unit>
                match last {
                    Some("text") => {
                        // Inside a <text> wrapper. Look at the parent
                        // (form / gloss / definition) and resolve.
                        let parent = path.iter().rev().nth(1).map(|s| s.as_str());
                        match parent {
                            Some("form") => {
                                let grand = path.iter().rev().nth(2).map(|s| s.as_str());
                                match grand {
                                    Some("lexical-unit") => {
                                        if let Some(e) = cur_entry.as_mut() {
                                            e.lexical_unit.insert(lang, text);
                                        }
                                    }
                                    Some("definition") => {
                                        if let Some(s) = cur_sense.as_mut() {
                                            s.definitions.insert(lang, text);
                                        }
                                    }
                                    _ => {}
                                }
                            }
                            Some("gloss") => {
                                if let Some(s) = cur_sense.as_mut() {
                                    s.glosses.insert(lang, text);
                                }
                            }
                            Some("definition") => {
                                if let Some(s) = cur_sense.as_mut() {
                                    s.definitions.insert(lang, text);
                                }
                            }
                            _ => {}
                        }
                    }
                    Some("gloss") => {
                        // Direct-text gloss shape.
                        if let Some(s) = cur_sense.as_mut() {
                            s.glosses.insert(lang, text);
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::End(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if path.last() == Some(&name) {
                    path.pop();
                }
                match name.as_str() {
                    "entry" => {
                        if let Some(entry) = cur_entry.take() {
                            entries.push(entry);
                        }
                    }
                    "sense" => {
                        if let (Some(sense), Some(entry)) =
                            (cur_sense.take(), cur_entry.as_mut())
                        {
                            entry.senses.push(sense);
                        }
                    }
                    "form" | "gloss" | "definition" => {
                        cur_lang = None;
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        buf.clear();
    }
    Ok(entries)
}

fn is_known_lift_element(name: &str) -> bool {
    matches!(
        name,
        "lift"
            | "header"
            | "entry"
            | "sense"
            | "lexical-unit"
            | "form"
            | "text"
            | "gloss"
            | "definition"
            | "grammatical-info"
    )
}

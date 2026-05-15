//! EAF (ELAN) parser. Reads the TIME_ORDER table once, then walks
//! tiers and resolves ALIGNABLE_ANNOTATION time-slot refs into
//! actual millisecond values.

use crate::{ImportError, Report};
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy)]
pub enum AnnotationKind {
    Alignable,
    Ref,
}

#[derive(Debug, Clone)]
pub struct Annotation {
    pub id: String,
    pub kind: AnnotationKind,
    pub value: Option<String>,
    pub start_ms: Option<i64>,
    pub end_ms: Option<i64>,
    pub ref_to: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Tier {
    pub id: String,
    pub participant: Option<String>,
    pub annotations: Vec<Annotation>,
}

#[derive(Debug, Default)]
pub struct Parsed {
    pub media_urls: Vec<String>,
    pub tiers: Vec<Tier>,
}

pub fn parse(body: &str, report: &mut Report) -> Result<Parsed, ImportError> {
    let mut reader = Reader::from_str(body);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    // Pass 1: collect time slots so annotation refs resolve cheaply.
    let mut time_slots: BTreeMap<String, i64> = BTreeMap::new();
    // Pass 1 also collects everything else; one walk is enough since
    // TIME_ORDER lives in the HEADER before TIERS.

    let mut out = Parsed::default();
    let mut cur_tier: Option<Tier> = None;
    let mut cur_ann: Option<Annotation> = None;
    let mut cur_ann_in_text: bool = false;
    let mut cur_value: String = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Err(e) => {
                return Err(ImportError::Parse(format!(
                    "at byte {}: {e}",
                    reader.buffer_position()
                )))
            }
            Ok(Event::Eof) => break,
            Ok(Event::Empty(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                match name.as_str() {
                    "TIME_SLOT" => {
                        let mut id = String::new();
                        let mut val: Option<i64> = None;
                        for a in e.attributes().flatten() {
                            match a.key.as_ref() {
                                b"TIME_SLOT_ID" => {
                                    id = String::from_utf8_lossy(a.value.as_ref()).to_string()
                                }
                                b"TIME_VALUE" => {
                                    val = String::from_utf8_lossy(a.value.as_ref())
                                        .parse::<i64>()
                                        .ok()
                                }
                                _ => {}
                            }
                        }
                        if let Some(v) = val {
                            time_slots.insert(id, v);
                        }
                    }
                    "MEDIA_DESCRIPTOR" => {
                        for a in e.attributes().flatten() {
                            if a.key.as_ref() == b"MEDIA_URL"
                                || a.key.as_ref() == b"RELATIVE_MEDIA_URL"
                            {
                                let v =
                                    String::from_utf8_lossy(a.value.as_ref()).to_string();
                                if !v.is_empty() {
                                    out.media_urls.push(v);
                                }
                            }
                        }
                    }
                    "LINGUISTIC_TYPE"
                    | "LOCALE"
                    | "LANGUAGE"
                    | "CONSTRAINT"
                    | "CONTROLLED_VOCABULARY"
                    | "LICENSE" => {
                        report
                            .losses
                            .push(format!("EAF element <{name}> not represented"));
                    }
                    _ => {}
                }
            }
            Ok(Event::Start(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                match name.as_str() {
                    "TIER" => {
                        let mut tier = Tier::default();
                        for a in e.attributes().flatten() {
                            match a.key.as_ref() {
                                b"TIER_ID" => {
                                    tier.id = String::from_utf8_lossy(a.value.as_ref())
                                        .to_string()
                                }
                                b"PARTICIPANT" => {
                                    tier.participant = Some(
                                        String::from_utf8_lossy(a.value.as_ref()).to_string(),
                                    )
                                }
                                _ => {}
                            }
                        }
                        if tier.id.is_empty() {
                            report
                                .losses
                                .push("tier without TIER_ID attribute (skipped)".into());
                            continue;
                        }
                        cur_tier = Some(tier);
                    }
                    "ALIGNABLE_ANNOTATION" => {
                        let mut ann = Annotation {
                            id: String::new(),
                            kind: AnnotationKind::Alignable,
                            value: None,
                            start_ms: None,
                            end_ms: None,
                            ref_to: None,
                        };
                        let mut ts1: Option<String> = None;
                        let mut ts2: Option<String> = None;
                        for a in e.attributes().flatten() {
                            match a.key.as_ref() {
                                b"ANNOTATION_ID" => {
                                    ann.id = String::from_utf8_lossy(a.value.as_ref())
                                        .to_string()
                                }
                                b"TIME_SLOT_REF1" => {
                                    ts1 = Some(
                                        String::from_utf8_lossy(a.value.as_ref()).to_string(),
                                    )
                                }
                                b"TIME_SLOT_REF2" => {
                                    ts2 = Some(
                                        String::from_utf8_lossy(a.value.as_ref()).to_string(),
                                    )
                                }
                                _ => {}
                            }
                        }
                        ann.start_ms = ts1.and_then(|t| time_slots.get(&t).copied());
                        ann.end_ms = ts2.and_then(|t| time_slots.get(&t).copied());
                        cur_ann = Some(ann);
                    }
                    "REF_ANNOTATION" => {
                        let mut ann = Annotation {
                            id: String::new(),
                            kind: AnnotationKind::Ref,
                            value: None,
                            start_ms: None,
                            end_ms: None,
                            ref_to: None,
                        };
                        for a in e.attributes().flatten() {
                            match a.key.as_ref() {
                                b"ANNOTATION_ID" => {
                                    ann.id = String::from_utf8_lossy(a.value.as_ref())
                                        .to_string()
                                }
                                b"ANNOTATION_REF" => {
                                    ann.ref_to = Some(
                                        String::from_utf8_lossy(a.value.as_ref()).to_string(),
                                    )
                                }
                                _ => {}
                            }
                        }
                        cur_ann = Some(ann);
                    }
                    "ANNOTATION_VALUE" => {
                        cur_ann_in_text = true;
                        cur_value.clear();
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(t)) => {
                if cur_ann_in_text {
                    if let Ok(text) = t.unescape() {
                        cur_value.push_str(&text);
                    }
                }
            }
            Ok(Event::End(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                match name.as_str() {
                    "ANNOTATION_VALUE" => {
                        cur_ann_in_text = false;
                        if let Some(a) = cur_ann.as_mut() {
                            if !cur_value.is_empty() {
                                a.value = Some(std::mem::take(&mut cur_value));
                            }
                        }
                    }
                    "ALIGNABLE_ANNOTATION" | "REF_ANNOTATION" => {
                        if let (Some(ann), Some(tier)) =
                            (cur_ann.take(), cur_tier.as_mut())
                        {
                            tier.annotations.push(ann);
                        }
                    }
                    "TIER" => {
                        if let Some(tier) = cur_tier.take() {
                            out.tiers.push(tier);
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        buf.clear();
    }
    Ok(out)
}

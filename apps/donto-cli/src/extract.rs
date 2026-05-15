use anyhow::{Context, Result};
use donto_blob::BlobStore;
use donto_client::{DontoClient, Literal, Object, Polarity, StatementInput};
use serde::{Deserialize, Serialize};
use std::path::Path;
use uuid::Uuid;

const OPENROUTER_URL: &str = "https://openrouter.ai/api/v1/chat/completions";

const DEFAULT_MODEL: &str = "x-ai/grok-4.1-fast";
const FALLBACK_MODEL: &str = "mistralai/mistral-large-2512";

#[derive(Debug, Serialize)]
pub struct ExtractReport {
    pub source: String,
    pub model: String,
    pub context: String,
    pub facts_extracted: u64,
    pub statements_ingested: u64,
    pub tiers: TierBreakdown,
    pub cost_estimate: Option<f64>,
    pub elapsed_ms: u64,
    /// SHA-256 of the source bytes (hex).
    pub source_sha256: Option<String>,
    /// Storage URI where the source bytes live.
    pub source_uri: Option<String>,
    /// donto_document.iri for the source.
    pub document_iri: Option<String>,
    /// donto_document_revision.revision_id linked from each statement.
    pub revision_id: Option<Uuid>,
    /// Number of donto_evidence_link rows created (one per
    /// asserted statement when blob wiring is on).
    pub evidence_links: u64,
}

#[derive(Debug, Default, Serialize)]
pub struct TierBreakdown {
    pub t1: u64,
    pub t2: u64,
    pub t3: u64,
    pub t4: u64,
    pub t5: u64,
    pub t6: u64,
    pub t7: u64,
    pub t8: u64,
}

#[derive(Debug, Deserialize)]
struct LlmResponse {
    choices: Vec<Choice>,
    usage: Option<Usage>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: Message,
}

#[derive(Debug, Deserialize)]
struct Message {
    content: String,
}

#[derive(Debug, Deserialize)]
struct Usage {
    prompt_tokens: Option<u64>,
    completion_tokens: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct ExtractionOutput {
    facts: Vec<ExtractedFact>,
}

#[derive(Debug, Deserialize)]
struct ExtractedFact {
    subject: String,
    predicate: String,
    object: FactObject,
    #[serde(default = "default_tier", deserialize_with = "deser_flexible_u8")]
    tier: u8,
    #[serde(
        default = "default_confidence",
        deserialize_with = "deser_flexible_f64"
    )]
    confidence: f64,
    #[serde(default)]
    notes: Option<String>,
}

fn deser_flexible_u8<'de, D: serde::Deserializer<'de>>(d: D) -> std::result::Result<u8, D::Error> {
    let v = serde_json::Value::deserialize(d)?;
    match v {
        serde_json::Value::Number(n) => Ok(n.as_u64().unwrap_or(1) as u8),
        serde_json::Value::String(s) => Ok(s.parse().unwrap_or(1)),
        _ => Ok(1),
    }
}

fn deser_flexible_f64<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> std::result::Result<f64, D::Error> {
    let v = serde_json::Value::deserialize(d)?;
    match v {
        serde_json::Value::Number(n) => Ok(n.as_f64().unwrap_or(0.7)),
        serde_json::Value::String(s) => Ok(s.parse().unwrap_or(0.7)),
        _ => Ok(0.7),
    }
}

fn default_tier() -> u8 {
    1
}
fn default_confidence() -> f64 {
    0.7
}

/// LLMs return objects in various shapes. We handle them all.
#[derive(Debug)]
enum FactObject {
    Iri(String),
    Lit(LiteralValue),
}

impl<'de> serde::Deserialize<'de> for FactObject {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let v = serde_json::Value::deserialize(d)?;
        match &v {
            serde_json::Value::Object(map) => {
                if let Some(iri) = map.get("iri").and_then(|v| v.as_str()) {
                    Ok(FactObject::Iri(iri.to_string()))
                } else if let Some(lit) = map.get("literal") {
                    let lv: LiteralValue =
                        serde_json::from_value(lit.clone()).map_err(serde::de::Error::custom)?;
                    Ok(FactObject::Lit(lv))
                } else if map.contains_key("v") {
                    let lv: LiteralValue =
                        serde_json::from_value(v.clone()).map_err(serde::de::Error::custom)?;
                    Ok(FactObject::Lit(lv))
                } else {
                    Ok(FactObject::Lit(LiteralValue {
                        v: v.clone(),
                        dt: "xsd:string".into(),
                        lang: None,
                    }))
                }
            }
            serde_json::Value::String(s) => {
                if s.starts_with("ex:") || s.starts_with("http") || s.starts_with("ctx:") {
                    Ok(FactObject::Iri(s.clone()))
                } else {
                    Ok(FactObject::Lit(LiteralValue {
                        v: v.clone(),
                        dt: "xsd:string".into(),
                        lang: None,
                    }))
                }
            }
            _ => Ok(FactObject::Lit(LiteralValue {
                v: v.clone(),
                dt: "xsd:string".into(),
                lang: None,
            })),
        }
    }
}

#[derive(Debug, Deserialize)]
struct LiteralValue {
    v: serde_json::Value,
    #[serde(default = "default_dt")]
    dt: String,
    #[serde(default)]
    lang: Option<String>,
}

fn default_dt() -> String {
    "xsd:string".into()
}

fn confidence_to_maturity(c: f64) -> u8 {
    match c {
        c if c >= 0.95 => 4,
        c if c >= 0.8 => 3,
        c if c >= 0.6 => 2,
        c if c >= 0.4 => 1,
        _ => 0,
    }
}

fn fact_to_statement(fact: &ExtractedFact, context: &str) -> StatementInput {
    let object = match &fact.object {
        FactObject::Iri(iri) => Object::iri(iri.clone()),
        FactObject::Lit(literal) => Object::lit(Literal {
            v: literal.v.clone(),
            dt: literal.dt.clone(),
            lang: literal.lang.clone(),
        }),
    };
    StatementInput::new(&fact.subject, &fact.predicate, object)
        .with_context(context)
        .with_polarity(Polarity::Asserted)
        .with_maturity(confidence_to_maturity(fact.confidence))
}

pub async fn run(
    client: &DontoClient,
    source_path: &Path,
    context: &str,
    model: &str,
    _batch_size: usize,
    api_key: &str,
    dry_run: bool,
    blob_store: Option<&dyn BlobStore>,
) -> Result<ExtractReport> {
    let start = std::time::Instant::now();
    let source_name = source_path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".into());

    let text = std::fs::read_to_string(source_path)
        .with_context(|| format!("reading {}", source_path.display()))?;

    eprintln!(
        "extracting from {} ({} chars) with {model}...",
        source_name,
        text.len()
    );

    // 1. Source blob: hash + register + (optionally) upload.
    //    Skipped entirely on dry-run so we don't touch the DB.
    let mut source_sha256: Option<String> = None;
    let mut source_uri: Option<String> = None;
    let mut document_iri: Option<String> = None;
    let mut revision_id: Option<Uuid> = None;
    if !dry_run {
        if let Some(store) = blob_store {
            let mime = donto_blob::sniff_mime(source_path)
                .or_else(|| Some("text/markdown".into()));
            let summary = store
                .put_bytes(text.as_bytes(), mime.as_deref())
                .await
                .with_context(|| "blob put failed")?;
            donto_blob::register_with_db(client, &summary).await?;
            let sha_hex = donto_blob::sha_hex(&summary.sha256);
            source_sha256 = Some(sha_hex.clone());
            source_uri = Some(summary.uri.clone());

            // 2. Source document: IRI is content-addressed so the
            //    same file always maps to the same document.
            let doc_iri = format!("donto:blob/sha256/{}", &sha_hex);
            document_iri = Some(doc_iri.clone());
            // ensure_document → idempotent on iri. Default-policy
            // handling is done by migration 0123 (NOT NULL +
            // fallback to policy:default/restricted_pending_review).
            let document_id = client
                .ensure_document(
                    &doc_iri,
                    mime.as_deref().unwrap_or("text/plain"),
                    Some(&source_name),
                    Some(&format!("file://{}", source_path.display())),
                    None,
                )
                .await?;

            // 3. Revision (one per extraction run — the body is
            //    immutable per sha256, but a fresh revision_id makes
            //    the evidence-link chain trivial to reason about).
            let rev_id = client
                .add_revision(
                    document_id,
                    Some(&text),
                    None,
                    Some(&format!("donto-extract/{model}")),
                )
                .await?;
            // Stamp the revision with blob_hash + uri + body_storage.
            // Inline + bucket = "both"; FTS works on body_inline,
            // canonical bytes are addressable via bucket_uri.
            let conn = client.pool().get().await?;
            conn.execute(
                "update donto_document_revision \
                 set blob_hash = $1, body_uri = $2, body_inline = $3, \
                     byte_size = $4, body_storage = 'both' \
                 where revision_id = $5",
                &[
                    &summary.sha256.as_slice(),
                    &summary.uri,
                    &text,
                    &(summary.byte_size as i64),
                    &rev_id,
                ],
            )
            .await?;
            revision_id = Some(rev_id);
        }
    }

    let facts = call_llm(api_key, model, &text).await?;
    let num_facts = facts.len() as u64;

    let mut tiers = TierBreakdown::default();
    for f in &facts {
        match f.tier {
            1 => tiers.t1 += 1,
            2 => tiers.t2 += 1,
            3 => tiers.t3 += 1,
            4 => tiers.t4 += 1,
            5 => tiers.t5 += 1,
            6 => tiers.t6 += 1,
            7 => tiers.t7 += 1,
            8 => tiers.t8 += 1,
            _ => tiers.t1 += 1,
        }
    }

    eprintln!(
        "  extracted {num_facts} facts across {} tiers",
        count_active_tiers(&tiers)
    );

    let mut ingested: u64 = 0;
    let mut evidence_links: u64 = 0;

    if dry_run {
        eprintln!("  dry-run: skipping ingest");
        for (i, fact) in facts.iter().enumerate() {
            println!(
                "{}",
                serde_json::json!({
                    "n": i + 1,
                    "subject": fact.subject,
                    "predicate": fact.predicate,
                    "object": match &fact.object {
                        FactObject::Iri(iri) => serde_json::json!({"iri": iri}),
                        FactObject::Lit(lit) => serde_json::json!({"v": lit.v, "dt": lit.dt}),
                    },
                    "tier": fact.tier,
                    "confidence": fact.confidence,
                    "notes": fact.notes,
                })
            );
        }
    } else {
        client
            .ensure_context(context, "custom", "permissive", None)
            .await?;
        // 4. Insert each statement individually so we get the
        //    statement_id back, then link it to the revision.
        //    Slower than assert_batch but the only way to record
        //    per-statement evidence without a returning extension
        //    to the batch path.
        for (fact, stmt) in facts.iter().zip(
            facts
                .iter()
                .map(|f| fact_to_statement(f, context))
                .collect::<Vec<_>>()
                .iter(),
        ) {
            let stmt_id = client.assert(stmt).await?;
            ingested += 1;
            if let Some(rev_id) = revision_id {
                let conn = client.pool().get().await?;
                conn.execute(
                    "insert into donto_evidence_link \
                        (statement_id, link_type, target_revision_id, confidence) \
                     values ($1, 'extracted_from', $2, $3)",
                    &[&stmt_id, &rev_id, &fact.confidence],
                )
                .await?;
                evidence_links += 1;
            }
        }
    }

    Ok(ExtractReport {
        source: source_name,
        model: model.into(),
        context: context.into(),
        facts_extracted: num_facts,
        statements_ingested: ingested,
        tiers,
        cost_estimate: None,
        elapsed_ms: start.elapsed().as_millis() as u64,
        source_sha256,
        source_uri,
        document_iri,
        revision_id,
        evidence_links,
    })
}

async fn call_llm(api_key: &str, model: &str, text: &str) -> Result<Vec<ExtractedFact>> {
    let http = reqwest::Client::new();

    let body = serde_json::json!({
        "model": model,
        "temperature": 0.1,
        "max_tokens": 32768,
        "messages": [
            {
                "role": "system",
                "content": EXTRACTION_PROMPT
            },
            {
                "role": "user",
                "content": format!("Extract all predicates from the following text:\n\n---\n{text}\n---")
            }
        ]
    });

    let resp = http
        .post(OPENROUTER_URL)
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .context("calling OpenRouter")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("OpenRouter returned {status}: {body}");
    }

    let llm_resp: LlmResponse = resp.json().await.context("parsing OpenRouter response")?;
    let content = llm_resp
        .choices
        .first()
        .map(|c| c.message.content.clone())
        .unwrap_or_default();

    // Strip markdown code fences if present
    let json_str = content
        .trim()
        .strip_prefix("```json")
        .or_else(|| content.trim().strip_prefix("```"))
        .unwrap_or(content.trim())
        .strip_suffix("```")
        .unwrap_or(content.trim())
        .trim();

    let output: ExtractionOutput = serde_json::from_str(json_str).with_context(|| {
        format!(
            "parsing extraction JSON (first 200 chars): {}",
            &json_str[..json_str.len().min(200)]
        )
    })?;

    Ok(output.facts)
}

fn count_active_tiers(t: &TierBreakdown) -> u8 {
    [t.t1, t.t2, t.t3, t.t4, t.t5, t.t6, t.t7, t.t8]
        .iter()
        .filter(|&&n| n > 0)
        .count() as u8
}

pub fn resolve_model(name: &str) -> &str {
    match name {
        "grok" | "fast" | "default" => DEFAULT_MODEL,
        "mistral" | "fallback" => FALLBACK_MODEL,
        other => other,
    }
}

const EXTRACTION_PROMPT: &str = r#"You are a predicate extraction engine. Given a source text (article, transcript,
essay, interview, etc.), extract the MAXIMUM CONCEIVABLE number of atomic
predicates — (subject, predicate, object) triples.

Your goal is TOTAL EXTRACTION. Not a summary. Not the "main points." Every
single relationship, claim, implication, presupposition, rhetorical move,
and philosophical commitment expressed or implied by the text becomes a triple.

You must INVENT predicate names yourself. Use camelCase. Be specific — prefer
"graduatedFrom" over "relatedTo". Mint as many novel predicates as the text
demands.

## EXTRACTION TIERS

Work through ALL of these tiers. Do not stop at Tier 1.

### Tier 1 — Surface facts (what the text explicitly states)
Identity, classification, biography, affiliation, education, location, temporal,
authorship, quantitative, attribution predicates.

### Tier 2 — Relational and structural (how things connect)
Causal, temporal ordering, mereological, spatial, comparison, dependency,
contrast, succession predicates.

### Tier 3 — Opinions, stances, and evaluative claims
Evaluation, preference, advocacy, criticism, agreement, emotional stance.

### Tier 4 — Epistemic and modal (known, possible, necessary)
Certainty, uncertainty, evidence, knowledge source, possibility, necessity, belief.

### Tier 5 — Pragmatic and rhetorical (what the text DOES)
Speech acts, rhetorical moves, hedging, emphasis, framing, audience.

### Tier 6 — Presuppositions and implicature (assumed without stating)
Presuppositions, implicature, existential commitments, absence.

### Tier 7 — Philosophical and ontological (deep structure)
Ontological, teleological, axiological, deontic, counterfactual, essentialism.

### Tier 8 — Intertextual and contextual (beyond the text itself)
References, cultural context, genre, historical.

## OUTPUT FORMAT

Return a JSON object with a single "facts" array. Each fact:

{
  "subject": "ex:<kebab-case-subject>",
  "predicate": "<camelCase predicate you invented>",
  "object": { "iri": "ex:<kebab-case>" } OR { "literal": { "v": <value>, "dt": "<xsd type>" } },
  "tier": <1-8>,
  "confidence": <0.0-1.0>,
  "notes": "<brief justification>"
}

## CRITICAL RULES

1. ALL IRIs must be kebab-lower-case: "ex:mrs-watson", NOT "ex:MrsWatson".
2. NEVER use boolean objects. Use predicates instead.
3. Prefer IRIs over string literals for entities.
4. String literals must be SHORT (name, date, quote, number — not sentences).
5. Confidence: 1.0 = directly stated, 0.9 = minor inference, 0.7 = significant, 0.5 = speculative.
6. Tier labels must be honest — article metadata is Tier 1, not Tier 8.
7. 15-30+ distinct subjects per 500-word article.
8. Decompose aggressively. Mint predicates freely.
9. Bias toward MORE triples. Target 100-500+ depending on article length.
10. EVERY predicate must be grounded in the text.

Return ONLY the JSON. No commentary before or after."#;

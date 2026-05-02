"""Donto Knowledge Graph API — the foundational interface to donto.

All clients (CLI, web, agents) should talk to this API.
This API talks to dontosrv (Rust HTTP server on localhost:7879) for graph
operations, and calls OpenRouter directly for LLM extraction.

Docs: https://genes.apexpots.com/docs
"""

import json
import os
import re
import time
from typing import Optional

import httpx

# No subprocess, no CLI — this API talks directly to dontosrv and OpenRouter.
from fastapi import FastAPI, HTTPException, Query
from fastapi.responses import HTMLResponse
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

DONTOSRV = os.environ.get("DONTOSRV_URL", "http://127.0.0.1:7879")
OPENROUTER_URL = "https://openrouter.ai/api/v1/chat/completions"
OPENROUTER_KEY = os.environ.get("OPENROUTER_API_KEY", "")

DEFAULT_MODEL = "x-ai/grok-4.1-fast"
FALLBACK_MODEL = "mistralai/mistral-large-2512"

srv = httpx.AsyncClient(base_url=DONTOSRV, timeout=600.0)
openrouter = httpx.AsyncClient(timeout=600.0)

app = FastAPI(
    title="Donto Knowledge Graph API",
    description="""# Donto — Bitemporal Paraconsistent Knowledge Graph

A public API for building, querying, and aligning knowledge graphs from unstructured text.
Currently powering genealogical research with **35.8 million statements**.

---

## For AI Agents — Recommended Workflow

### 1. Find source material
- **Obituaries** — names, dates, family relationships, locations
- **Newspaper archives** — Trove (trove.nla.gov.au), newspapers.com
- **Government records** — BDM registries, census, immigration manifests
- **Church records** — baptism, marriage, burial registers
- **Historical publications** — local history journals, bulletins
- **Oral histories** — interview transcripts, testimonies
- **Legal documents** — wills, probate, land titles, native title claims
- **Wikipedia** — biographical articles, event summaries
- **Academic papers** — genealogical journals, anthropological studies

### 2. Extract and ingest: `POST /extract-and-ingest`
```json
{"text": "Mary Watson was born in Cornwall...", "context": "ctx:genes/mary-watson"}
```
~$0.005/article, yields 60-150 facts across 8 analytical tiers.

### 3. Query: `GET /match?subject=ex:mary-watson`

### 4. Align predicates: `POST /align/auto?threshold=0.6`
Different sources mint different predicate names. Auto-align converges them.

### 5. Unified query: `GET /shadow?subject=ex:mary-watson`
Expands through predicate equivalences.

---

## Extraction Tiers

| Tier | Category | Examples |
|------|----------|---------|
| T1 | Surface facts | bornIn, marriedTo, childOf, locatedIn |
| T2 | Relational | causedBy, precedes, partOf, succeededBy |
| T3 | Opinions | holdsOpinion, criticizes, advocatesFor |
| T4 | Epistemic | assertsAsFact, speculatesAbout |
| T5 | Rhetorical | framesAs, emphasizes, hedgesClaim |
| T6 | Presuppositions | presupposesThat, impliesThat |
| T7 | Philosophical | reifiesAs, treatsAsEssentialProperty |
| T8 | Intertextual | drawsOnTradition, situatesInDiscourse |

## Alignment Relations

| Relation | Meaning |
|----------|---------|
| `exact_equivalent` | Same meaning, same direction |
| `inverse_equivalent` | Same meaning, swap subject/object |
| `sub_property_of` | Specific implies general |
| `close_match` | Similar but not identical |
| `not_equivalent` | Explicitly NOT the same |

## Response Times

| Endpoint | Typical Time | Notes |
|----------|-------------|-------|
| GET /health, /version | <100ms | Instant |
| GET /predicates, /subjects, /contexts | 1-3s | Database query |
| GET /search, /history | 1-10s | Depends on result count |
| POST /assert, /assert/batch | <500ms | Direct insert |
| POST /align/register, /align/rebuild | 1-5s | Database operations |
| **POST /extract-and-ingest** | **30-120s** | **LLM call to OpenRouter — set your HTTP timeout to at least 120s** |
| **POST /extract** | **30-120s** | **Same — LLM extraction takes time** |

**Important for agents**: The extract endpoints call an external LLM (Grok 4.1 Fast via OpenRouter).
This typically takes 30-60 seconds but can take up to several minutes for long texts. **Set your HTTP
client timeout to at least 10 minutes (600 seconds)** when calling any endpoint. The server-side
timeout is 10 minutes.

## IRIs: subjects `ex:kebab-case`, predicates `camelCase`, contexts `ctx:namespace/topic`

## Full Documentation

**[Simple guide for agents at /simple-docs](/simple-docs)** — step-by-step, copy-paste, no theory.

**[Full documentation at /full-docs](/full-docs)** — includes research strategy guide,
worked examples, source recommendations, IRI conventions, bitemporal model explanation,
and complete endpoint reference with curl examples.
""",
    version="0.3.0",
)

app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_methods=["*"], allow_headers=["*"],
)


# ── Helpers ─────────────────────────────────────────────────────────────


async def srv_get(path: str, params: dict = None):
    p = {k: v for k, v in (params or {}).items() if v is not None}
    r = await srv.get(path, params=p)
    if r.status_code >= 400:
        raise HTTPException(r.status_code, r.json() if r.headers.get("content-type", "").startswith("application/json") else r.text)
    try:
        return r.json()
    except Exception:
        return {"raw": r.text}


async def srv_post(path: str, body=None):
    r = await srv.post(path, json=body)
    if r.status_code >= 400:
        raise HTTPException(r.status_code, r.json() if r.headers.get("content-type", "").startswith("application/json") else r.text)
    text = r.text.strip()
    if not text:
        return {"ok": True}
    try:
        return r.json()
    except Exception:
        return {"raw": text}


def resolve_model(name: str) -> str:
    return {"grok": DEFAULT_MODEL, "fast": DEFAULT_MODEL, "default": DEFAULT_MODEL,
            "mistral": FALLBACK_MODEL, "fallback": FALLBACK_MODEL}.get(name, name)


def confidence_to_maturity(c: float) -> int:
    if c >= 0.95: return 4
    if c >= 0.8: return 3
    if c >= 0.6: return 2
    if c >= 0.4: return 1
    return 0


def parse_fact_object(obj):
    """Parse LLM extraction output object into dontosrv assert format."""
    if isinstance(obj, dict):
        if "iri" in obj:
            return obj["iri"], None
        if "literal" in obj:
            lit = obj["literal"]
            return None, {"v": lit.get("v"), "dt": lit.get("dt", "xsd:string"), "lang": lit.get("lang")}
        if "v" in obj:
            return None, {"v": obj["v"], "dt": obj.get("dt", "xsd:string"), "lang": obj.get("lang")}
    if isinstance(obj, str):
        if obj.startswith("ex:") or obj.startswith("http") or obj.startswith("ctx:"):
            return obj, None
        return None, {"v": obj, "dt": "xsd:string"}
    return None, {"v": obj, "dt": "xsd:string"}


EXTRACTION_PROMPT = """You are a predicate extraction engine. Given a source text (article, transcript,
essay, interview, etc.), extract the MAXIMUM CONCEIVABLE number of atomic
predicates — (subject, predicate, object) triples.

Your goal is TOTAL EXTRACTION. Not a summary. Not the "main points." Every
single relationship, claim, implication, presupposition, rhetorical move,
and philosophical commitment expressed or implied by the text becomes a triple.

You must INVENT predicate names yourself. Use camelCase. Be specific — prefer
"graduatedFrom" over "relatedTo". Mint as many novel predicates as the text demands.

## EXTRACTION TIERS — work through ALL of these. Do not stop at Tier 1.

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

Return ONLY the JSON. No commentary before or after."""


async def call_openrouter(text: str, model: str) -> list[dict]:
    """Call OpenRouter and return parsed facts."""
    resp = await openrouter.post(
        OPENROUTER_URL,
        headers={"Authorization": f"Bearer {OPENROUTER_KEY}", "Content-Type": "application/json"},
        json={
            "model": model,
            "temperature": 0.1,
            "max_tokens": 32768,
            "messages": [
                {"role": "system", "content": EXTRACTION_PROMPT},
                {"role": "user", "content": f"Extract all predicates from the following text:\n\n---\n{text}\n---"},
            ],
        },
    )
    if resp.status_code != 200:
        raise HTTPException(502, f"OpenRouter returned {resp.status_code}: {resp.text[:500]}")

    content = resp.json()["choices"][0]["message"]["content"]

    # Strip markdown fences
    cleaned = content.strip()
    if cleaned.startswith("```"):
        cleaned = re.sub(r"^```(?:json)?\s*", "", cleaned)
        cleaned = re.sub(r"\s*```$", "", cleaned)

    try:
        return json.loads(cleaned)["facts"]
    except (json.JSONDecodeError, KeyError) as e:
        raise HTTPException(502, f"Failed to parse extraction output: {e}. First 300 chars: {cleaned[:300]}")


async def ingest_facts(facts: list[dict], context: str) -> int:
    """Convert extracted facts to dontosrv assert format and batch-insert."""
    # Ensure context exists
    await srv_post("/contexts/ensure", {"iri": context, "kind": "custom", "mode": "permissive"})

    # Convert facts to assert format
    statements = []
    for f in facts:
        obj_iri, obj_lit = parse_fact_object(f.get("object"))
        confidence = f.get("confidence", 0.7)
        if isinstance(confidence, str):
            try: confidence = float(confidence)
            except: confidence = 0.7
        tier = f.get("tier", 1)
        if isinstance(tier, str):
            try: tier = int(tier)
            except: tier = 1

        statements.append({
            "subject": f["subject"],
            "predicate": f["predicate"],
            "object_iri": obj_iri,
            "object_lit": obj_lit,
            "context": context,
            "polarity": "asserted",
            "maturity": confidence_to_maturity(confidence),
        })

    # Batch insert via dontosrv
    if statements:
        result = await srv_post("/assert/batch", {"statements": statements})
        return result if isinstance(result, int) else len(statements)
    return 0


# ── System ──────────────────────────────────────────────────────────────


@app.get("/health", tags=["System"])
async def health():
    """Health check — verifies both the API and dontosrv are running."""
    try:
        r = await srv.get("/health")
        return {"status": "ok", "dontosrv": r.text.strip()}
    except Exception as e:
        return {"status": "degraded", "dontosrv": str(e)}


@app.get("/version", tags=["System"])
async def version():
    """Version info."""
    return await srv_get("/version")


# ── Extract and Ingest (native — no CLI) ────────────────────────────────


class ExtractIngestRequest(BaseModel):
    text: str = Field(..., description="Source text to extract knowledge from")
    context: str = Field(..., description="Context IRI (e.g. ctx:genes/mary-watson)")
    model: str = Field("grok", description="grok (default), mistral, or OpenRouter model ID")


@app.post("/extract-and-ingest", tags=["Extract"],
    summary="Extract knowledge from text and ingest (preferred)")
async def extract_and_ingest(req: ExtractIngestRequest):
    """**Preferred endpoint for agents.** Extracts facts from text via LLM,
    ingests directly into the graph. No intermediate steps.

    1. Calls OpenRouter with the extraction prompt
    2. Parses the returned facts (subject/predicate/object/tier/confidence)
    3. Batch-inserts into donto via dontosrv /assert/batch
    4. Returns a report with counts and tier breakdown
    """
    start = time.time()
    model = resolve_model(req.model)

    facts = await call_openrouter(req.text, model)

    tiers = {f"t{i}": 0 for i in range(1, 9)}
    for f in facts:
        t = f.get("tier", 1)
        if isinstance(t, str):
            try: t = int(t)
            except: t = 1
        key = f"t{min(max(t, 1), 8)}"
        tiers[key] = tiers.get(key, 0) + 1

    ingested = await ingest_facts(facts, req.context)

    return {
        "model": model,
        "context": req.context,
        "facts_extracted": len(facts),
        "statements_ingested": ingested,
        "tiers": tiers,
        "elapsed_ms": int((time.time() - start) * 1000),
    }


class ExtractRequest(BaseModel):
    text: str = Field(..., description="Source text")
    context: Optional[str] = Field(None, description="Context IRI")
    model: str = Field("grok", description="Model")
    dry_run: bool = Field(False, description="Preview without ingesting")


@app.post("/extract", tags=["Extract"],
    summary="Extract with dry-run option")
async def extract(req: ExtractRequest):
    """Like /extract-and-ingest but with dry_run to preview facts before ingesting."""
    start = time.time()
    model = resolve_model(req.model)
    facts = await call_openrouter(req.text, model)

    tiers = {f"t{i}": 0 for i in range(1, 9)}
    for f in facts:
        t = f.get("tier", 1)
        if isinstance(t, str):
            try: t = int(t)
            except: t = 1
        tiers[f"t{min(max(t, 1), 8)}"] = tiers.get(f"t{min(max(t, 1), 8)}", 0) + 1

    if req.dry_run:
        return {
            "model": model,
            "facts_extracted": len(facts),
            "tiers": tiers,
            "dry_run": True,
            "facts": facts,
            "elapsed_ms": int((time.time() - start) * 1000),
        }

    context = req.context or f"ctx:extract/{model.split('/')[-1]}"
    ingested = await ingest_facts(facts, context)

    return {
        "model": model,
        "context": context,
        "facts_extracted": len(facts),
        "statements_ingested": ingested,
        "tiers": tiers,
        "elapsed_ms": int((time.time() - start) * 1000),
    }


# ── Ingest (direct to dontosrv) ────────────────────────────────────────


class AssertRequest(BaseModel):
    subject: str = Field(..., description="Subject IRI")
    predicate: str = Field(..., description="Predicate")
    object_iri: Optional[str] = Field(None, description="Object IRI")
    object_lit: Optional[dict] = Field(None, description='{"v": value, "dt": "xsd:type"}')
    context: str = Field("donto:anonymous", description="Context IRI")
    polarity: str = Field("asserted")
    maturity: int = Field(0)


@app.post("/assert", tags=["Ingest"], summary="Assert a single statement")
async def assert_stmt(req: AssertRequest):
    """Insert a single statement."""
    return await srv_post("/assert", req.model_dump())


class BatchAssertRequest(BaseModel):
    statements: list[dict] = Field(..., description="Array of statement objects")


@app.post("/assert/batch", tags=["Ingest"], summary="Assert a batch of statements")
async def assert_batch(req: BatchAssertRequest):
    """Batch-insert statements."""
    return await srv_post("/assert/batch", {"statements": req.statements})


# ── Query (direct to dontosrv) ──────────────────────────────────────────


@app.get("/subjects", tags=["Query"], summary="List subjects")
async def subjects():
    """List subjects with statement counts."""
    return await srv_get("/subjects")


@app.get("/search", tags=["Query"], summary="Full-text search (~5ms)")
async def search(q: str = Query(..., description="Search query"), limit: int = Query(25, description="Max results")):
    """Search subjects by name/label. Uses a trigram-indexed label cache for instant results.

    Returns subjects with matching labels, ordered by statement count (most connected first).
    """
    import asyncpg
    dsn = os.environ.get("DONTO_DSN", "postgres://donto:donto@127.0.0.1:5432/donto")
    conn = await asyncpg.connect(dsn)
    try:
        rows = await conn.fetch(
            "SELECT subject, label, stmt_count FROM donto_label_cache WHERE label ILIKE $1 ORDER BY stmt_count DESC LIMIT $2",
            f"%{q}%", limit,
        )
        return {"matches": [{"subject": r["subject"], "label": r["label"], "count": r["stmt_count"]} for r in rows], "q": q}
    finally:
        await conn.close()


@app.get("/history/{subject:path}", tags=["Query"], summary="Statement history")
async def history(subject: str):
    """All statements for a subject (including retracted)."""
    return await srv_get(f"/history/{subject}")


@app.get("/statement/{id}", tags=["Query"], summary="Statement detail")
async def statement_detail(id: str):
    """Full detail for a statement by UUID."""
    return await srv_get(f"/statement/{id}")


@app.get("/contexts", tags=["Query"], summary="List contexts")
async def contexts():
    """List all contexts."""
    return await srv_get("/contexts")


@app.get("/predicates", tags=["Query"], summary="List predicates with counts")
async def predicates():
    """List predicates ordered by frequency."""
    return await srv_get("/predicates")


class QueryRequest(BaseModel):
    query: str = Field(..., description="DontoQL or SPARQL query text")


@app.post("/query", tags=["Query"], summary="DontoQL / SPARQL query")
async def query(req: QueryRequest):
    """Run DontoQL (MATCH ...) or SPARQL subset (SELECT ...)."""
    q = req.query.strip()
    if q.upper().startswith("SELECT") or q.upper().startswith("PREFIX"):
        return await srv_post("/sparql", {"query": q})
    return await srv_post("/dontoql", {"query": q})


# ── Retract ─────────────────────────────────────────────────────────────


@app.post("/retract/{statement_id}", tags=["Mutate"], summary="Retract a statement")
async def retract(statement_id: str):
    """Bitemporal delete — closes tx_time, row remains for history."""
    return await srv_post("/retract", {"statement_id": statement_id})


# ── Alignment (direct to dontosrv) ──────────────────────────────────────


class AlignRegisterRequest(BaseModel):
    source: str = Field(...)
    target: str = Field(...)
    relation: str = Field(...)
    confidence: float = Field(1.0)


@app.post("/align/register", tags=["Alignment"], summary="Register alignment")
async def align_register(req: AlignRegisterRequest):
    """Register a predicate alignment. Call /align/rebuild after."""
    return await srv_post("/alignment/register", req.model_dump())


@app.post("/align/rebuild", tags=["Alignment"], summary="Rebuild closure")
async def align_rebuild():
    """Rebuild predicate closure index. Call after registering alignments."""
    return await srv_post("/alignment/rebuild-closure", {})


@app.post("/align/retract/{alignment_id}", tags=["Alignment"], summary="Retract alignment")
async def align_retract(alignment_id: str):
    """Retract an alignment edge."""
    return await srv_post("/alignment/retract", {"alignment_id": alignment_id})


@app.get("/align/suggest/{predicate}", tags=["Alignment"], summary="Suggest similar predicates")
async def align_suggest(predicate: str, threshold: float = Query(0.3), limit: int = Query(20)):
    """Find predicates with similar names using trigram similarity."""
    return await srv_post("/sparql", {"query":
        f"SELECT target_iri, similarity, target_label FROM donto_suggest_alignments('{predicate}', {threshold}, {limit})"
    })


# ── Evidence ────────────────────────────────────────────────────────────


@app.get("/evidence/{statement_id}", tags=["Evidence"], summary="Evidence for a statement")
async def evidence_for(statement_id: str):
    """Get evidence spans linked to a statement."""
    return await srv_get(f"/evidence/{statement_id}")


@app.get("/claim/{statement_id}", tags=["Evidence"], summary="Claim card")
async def claim_card(statement_id: str):
    """Full claim card — evidence, arguments, obligations."""
    return await srv_get(f"/claim/{statement_id}")


# ── Full Documentation ──────────────────────────────────────────────────

FULL_DOCS_HTML = """<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Donto Knowledge Graph API — Full Documentation</title>
<style>
  body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; max-width: 900px; margin: 0 auto; padding: 2rem; line-height: 1.7; color: #1a1a2e; background: #fafbfc; }
  h1 { color: #0f3460; border-bottom: 3px solid #0f3460; padding-bottom: 0.5rem; }
  h2 { color: #16213e; margin-top: 3rem; border-bottom: 1px solid #ddd; padding-bottom: 0.3rem; }
  h3 { color: #1a1a2e; margin-top: 2rem; }
  code { background: #e8eaf6; padding: 2px 6px; border-radius: 3px; font-size: 0.9em; }
  pre { background: #1a1a2e; color: #e0e0e0; padding: 1rem; border-radius: 8px; overflow-x: auto; line-height: 1.5; }
  pre code { background: none; color: inherit; padding: 0; }
  table { border-collapse: collapse; width: 100%; margin: 1rem 0; }
  th, td { border: 1px solid #ddd; padding: 8px 12px; text-align: left; }
  th { background: #e8eaf6; }
  tr:nth-child(even) { background: #f5f5f5; }
  .endpoint { background: #e3f2fd; padding: 0.5rem 1rem; border-radius: 6px; margin: 1rem 0; font-family: monospace; font-size: 1.1em; }
  .get { border-left: 4px solid #4caf50; }
  .post { border-left: 4px solid #2196f3; }
  .tip { background: #fff3e0; border-left: 4px solid #ff9800; padding: 1rem; margin: 1rem 0; border-radius: 4px; }
  .warning { background: #fce4ec; border-left: 4px solid #f44336; padding: 1rem; margin: 1rem 0; border-radius: 4px; }
  a { color: #1565c0; }
</style>
</head>
<body>

<h1>Donto Knowledge Graph API</h1>
<p><strong>Base URL:</strong> <code>https://genes.apexpots.com</code></p>
<p><strong>Interactive Swagger UI:</strong> <a href="/docs">/docs</a> &nbsp;|&nbsp; <strong>OpenAPI Spec:</strong> <a href="/openapi.json">/openapi.json</a></p>
<p>A public API for building, querying, and aligning knowledge graphs from unstructured text. Currently serving <strong>35.8 million statements</strong> in a bitemporal paraconsistent quad store.</p>

<div class="tip">
<strong>For AI agents:</strong> Read this entire page before making API calls. It contains critical information about timeouts, IRI conventions, research strategies, and the recommended workflow that will save you from common mistakes.
</div>

<h2>Table of Contents</h2>
<ol>
<li><a href="#concepts">Core Concepts</a></li>
<li><a href="#quickstart">Quick Start for Agents</a></li>
<li><a href="#research">Research Strategy Guide</a></li>
<li><a href="#extract">Extract &amp; Ingest Endpoints</a></li>
<li><a href="#query">Query Endpoints</a></li>
<li><a href="#alignment">Predicate Alignment</a></li>
<li><a href="#mutate">Mutate Endpoints</a></li>
<li><a href="#evidence">Evidence &amp; Claims</a></li>
<li><a href="#tiers">Extraction Tiers (Deep Dive)</a></li>
<li><a href="#iri">IRI &amp; Naming Conventions</a></li>
<li><a href="#bitemporal">Bitemporal Model</a></li>
<li><a href="#timeouts">Timeouts &amp; Performance</a></li>
<li><a href="#examples">Complete Worked Examples</a></li>
<li><a href="#errors">Error Handling</a></li>
</ol>

<h2 id="concepts">1. Core Concepts</h2>

<h3>What is donto?</h3>
<p>Donto is a <strong>bitemporal paraconsistent quad store</strong>. Every fact is stored as:</p>
<pre><code>(subject, predicate, object, context)</code></pre>
<p>Plus metadata: polarity, maturity, valid-time range, transaction-time range.</p>

<h3>Key properties</h3>
<table>
<tr><th>Property</th><th>What it means</th><th>Why it matters</th></tr>
<tr><td><strong>Bitemporal</strong></td><td>Every fact has two time axes: when it was true (valid-time) and when it was recorded (transaction-time)</td><td>You can ask "what did we know on April 1?" even after corrections</td></tr>
<tr><td><strong>Paraconsistent</strong></td><td>Contradictions are stored, not rejected</td><td>Two sources say different birth years? Both are kept. Query with <code>polarity=any</code> to see conflicts.</td></tr>
<tr><td><strong>Quad store</strong></td><td>Every fact belongs to a context</td><td>Contexts scope facts by source, research question, or extraction run. You can retract an entire context.</td></tr>
<tr><td><strong>Alignment layer</strong></td><td>Predicates can be declared equivalent</td><td>Different LLM runs mint <code>bornIn</code> vs <code>birthplaceOf</code> — alignment lets queries find both.</td></tr>
</table>

<h3>What is a statement?</h3>
<pre><code>{
  "statement_id": "uuid",
  "subject": "ex:mary-watson",           // WHO or WHAT
  "predicate": "bornIn",                  // THE RELATIONSHIP
  "object": "ex:cornwall" or {"v": 1860, "dt": "xsd:integer"},  // THE VALUE
  "context": "ctx:genes/mary-watson",     // WHERE THIS FACT CAME FROM
  "polarity": "asserted",                 // asserted | negated | absent | unknown
  "maturity": 3,                          // 0-4, higher = more confident
  "valid_lo": "1860-01-01",              // when the fact was true (optional)
  "valid_hi": null,                       // null = still true
  "tx_lo": "2026-05-02T...",             // when recorded in DB
  "tx_hi": null                           // null = current; set when retracted
}</code></pre>

<h2 id="quickstart">2. Quick Start for Agents</h2>

<p>The typical workflow in 5 API calls:</p>

<pre><code># 1. Extract knowledge from text
POST /extract-and-ingest
{"text": "...", "context": "ctx:genes/my-topic"}

# 2. See what was extracted
GET /search?q=person-name

# 3. Get full details
GET /history/ex:person-name

# 4. Auto-align predicates across sources
POST /align/auto?threshold=0.6

# 5. Query unified view
GET /search?q=person-name</code></pre>

<div class="warning">
<strong>Timeout:</strong> Set your HTTP client timeout to <strong>at least 600 seconds (10 minutes)</strong> for all calls. The extract endpoints call an external LLM which can take 30-120 seconds. Other endpoints typically respond in under 1 second.
</div>

<h2 id="research">3. Research Strategy Guide</h2>

<p>This section is specifically for AI agents conducting research. Follow these strategies to build comprehensive knowledge graphs.</p>

<h3>3.1 Finding Source Material</h3>

<h4>For genealogy / people research:</h4>
<table>
<tr><th>Source Type</th><th>What You Get</th><th>Where to Find It</th><th>Quality</th></tr>
<tr><td><strong>Obituaries</strong></td><td>Names, dates, family relationships, locations, occupations, achievements</td><td>newspapers.com, legacy.com, local newspaper archives</td><td>High — written by family, fact-checked by editors</td></tr>
<tr><td><strong>Birth/Death/Marriage records</strong></td><td>Exact dates, parents' names, witnesses, locations</td><td>BDM registries (state-specific), FamilySearch.org</td><td>Very high — official government records</td></tr>
<tr><td><strong>Census records</strong></td><td>Household composition, ages, occupations, birthplaces</td><td>ancestry.com, FamilySearch.org, national archives</td><td>High — but self-reported, may contain errors</td></tr>
<tr><td><strong>Newspaper articles</strong></td><td>Events, quotes, context, public activities</td><td>Trove (trove.nla.gov.au), newspapers.com, chroniclingamerica.loc.gov</td><td>Medium — journalistic, may contain errors</td></tr>
<tr><td><strong>Church records</strong></td><td>Baptisms, marriages, burials, godparents</td><td>FamilySearch.org, local parish archives</td><td>High — contemporaneous records</td></tr>
<tr><td><strong>Immigration records</strong></td><td>Ship manifests, arrival dates, ports, ages</td><td>ancestry.com, national archives, Ellis Island</td><td>High — official records</td></tr>
<tr><td><strong>Wills &amp; probate</strong></td><td>Family relationships, property, beneficiaries</td><td>State archives, probate courts</td><td>Very high — legal documents</td></tr>
<tr><td><strong>Military records</strong></td><td>Service dates, ranks, units, medals, next of kin</td><td>National Archives, military databases</td><td>Very high — official records</td></tr>
<tr><td><strong>Wikipedia</strong></td><td>Biographical summaries, event context</td><td>wikipedia.org</td><td>Medium — good overview but may have errors</td></tr>
<tr><td><strong>Oral histories</strong></td><td>Personal narratives, family stories, cultural context</td><td>Libraries, universities, cultural institutions</td><td>Medium — subjective but culturally rich</td></tr>
<tr><td><strong>DNA results</strong></td><td>Ethnicity estimates, relative matches, shared segments</td><td>AncestryDNA, 23andMe, FTDNA</td><td>High for matches, variable for ethnicity estimates</td></tr>
</table>

<h4>For general research:</h4>
<table>
<tr><th>Source Type</th><th>Best For</th></tr>
<tr><td>Academic papers</td><td>Scientific claims, methodology, citations</td></tr>
<tr><td>Legal documents</td><td>Contracts, regulations, case law, precedent</td></tr>
<tr><td>Technical docs</td><td>Product specs, API references, standards</td></tr>
<tr><td>News articles</td><td>Events, quotes, public statements</td></tr>
<tr><td>Interview transcripts</td><td>Expert opinions, personal accounts</td></tr>
<tr><td>Government reports</td><td>Statistics, policy, official findings</td></tr>
</table>

<h3>3.2 Research Workflow (Detailed)</h3>

<h4>Step 1: Initial extraction</h4>
<p>Start by extracting from the most authoritative source you can find. Use a descriptive context:</p>
<pre><code>POST /extract-and-ingest
{
  "text": "&lt;full text of obituary, article, or document&gt;",
  "context": "ctx:genes/lisa-watts/obituary-2023"
}</code></pre>
<p>The context should encode: namespace / topic / source-type. This makes it easy to query, compare, and retract later.</p>

<h4>Step 2: Extract from multiple sources</h4>
<p>Always get 2+ sources. Different sources fill different gaps:</p>
<pre><code># Source 1: Obituary
POST /extract-and-ingest
{"text": "...", "context": "ctx:genes/lisa-watts/obituary"}

# Source 2: Birth record
POST /extract-and-ingest
{"text": "...", "context": "ctx:genes/lisa-watts/bdm-birth"}

# Source 3: Census record
POST /extract-and-ingest
{"text": "...", "context": "ctx:genes/lisa-watts/census-1911"}

# Source 4: Newspaper mention
POST /extract-and-ingest
{"text": "...", "context": "ctx:genes/lisa-watts/newspaper-1920"}</code></pre>

<h4>Step 3: Query and cross-reference</h4>
<pre><code># Search for the person
GET /search?q=lisa+watts

# Get all facts about them
GET /history/ex:lisa-watts

# Check specific predicates
GET /match?subject=ex:lisa-watts&amp;predicate=bornIn
GET /match?subject=ex:lisa-watts&amp;predicate=marriedTo

# See all facts from a specific source
GET /match?context=ctx:genes/lisa-watts/obituary</code></pre>

<h4>Step 4: Align predicates</h4>
<pre><code># Auto-align everything (do this after multiple extractions)
POST /align/auto?threshold=0.6

# Or manually align specific predicates
GET /align/suggest/bornIn?threshold=0.3
POST /align/register
{"source": "wasBornIn", "target": "bornIn", "relation": "exact_equivalent", "confidence": 0.95}
POST /align/rebuild</code></pre>

<h4>Step 5: Look for contradictions</h4>
<p>Different sources may disagree. This is valuable — don't discard conflicts.</p>
<pre><code># Query with polarity=any to see all assertions including contradictions
GET /match?subject=ex:lisa-watts&amp;polarity=any

# If a source is wrong, retract individual statements
POST /retract/{statement_id}</code></pre>

<h4>Step 6: Fill gaps</h4>
<p>After reviewing what you have, identify gaps and find more sources:</p>
<pre><code># What do we know?
GET /history/ex:lisa-watts
# Missing: death date, parents' names, children...
# → Find more sources and extract from them</code></pre>

<h3>3.3 Context Naming Conventions</h3>
<table>
<tr><th>Pattern</th><th>Use For</th><th>Example</th></tr>
<tr><td><code>ctx:genes/&lt;person-slug&gt;/&lt;source&gt;</code></td><td>Genealogy per-source</td><td><code>ctx:genes/lisa-watts/obituary</code></td></tr>
<tr><td><code>ctx:genes/&lt;topic-slug&gt;</code></td><td>Genealogy general topic</td><td><code>ctx:genes/native-title-research</code></td></tr>
<tr><td><code>ctx:research/&lt;topic&gt;</code></td><td>General research</td><td><code>ctx:research/climate-models</code></td></tr>
<tr><td><code>ctx:extract/&lt;filename&gt;/&lt;model&gt;</code></td><td>Auto-generated by extract</td><td><code>ctx:extract/article/grok-4.1-fast</code></td></tr>
<tr><td><code>ctx:test/&lt;name&gt;</code></td><td>Testing</td><td><code>ctx:test/api-validation</code></td></tr>
</table>

<h2 id="extract">4. Extract &amp; Ingest Endpoints</h2>

<div class="endpoint post">POST /extract-and-ingest &nbsp;&nbsp; <strong>(preferred)</strong></div>
<p>The primary endpoint for agents. Extracts knowledge from text and ingests in one call.</p>
<pre><code>POST /extract-and-ingest
Content-Type: application/json

{
  "text": "Mary Watson was born in Cornwall, England in 1860...",
  "context": "ctx:genes/mary-watson-research",
  "model": "grok"  // optional: "grok" (default), "mistral", or any OpenRouter model ID
}

Response:
{
  "model": "x-ai/grok-4.1-fast",
  "context": "ctx:genes/mary-watson-research",
  "facts_extracted": 72,
  "statements_ingested": 72,
  "tiers": {"t1": 27, "t2": 12, "t3": 7, "t4": 6, "t5": 6, "t6": 7, "t7": 4, "t8": 3},
  "elapsed_ms": 35000
}</code></pre>

<div class="tip">
<strong>Cost:</strong> ~$0.005 per article via Grok 4.1 Fast. A 500-word article yields 60-150 facts across all 8 tiers. Longer articles yield more facts proportionally.
</div>

<div class="endpoint post">POST /extract</div>
<p>Like /extract-and-ingest but with <code>dry_run</code> option to preview facts before ingesting.</p>
<pre><code>{
  "text": "...",
  "context": "ctx:genes/topic",
  "model": "grok",
  "dry_run": true  // returns facts array without ingesting
}</code></pre>

<div class="endpoint post">POST /assert</div>
<p>Insert a single fact directly (no LLM extraction).</p>
<pre><code>{
  "subject": "ex:lisa-watts",
  "predicate": "bornIn",
  "object_iri": "ex:sydney",      // for entity objects
  // OR: "object_lit": {"v": 1985, "dt": "xsd:integer"},  // for literal values
  "context": "ctx:genes/lisa-watts/manual",
  "polarity": "asserted",
  "maturity": 3
}</code></pre>

<div class="endpoint post">POST /assert/batch</div>
<p>Insert multiple facts in one call.</p>
<pre><code>{
  "statements": [
    {"subject": "ex:lisa-watts", "predicate": "bornIn", "object_iri": "ex:sydney", "context": "ctx:genes/lisa-watts"},
    {"subject": "ex:lisa-watts", "predicate": "hasBirthYear", "object_lit": {"v": 1985, "dt": "xsd:integer"}, "context": "ctx:genes/lisa-watts"}
  ]
}</code></pre>

<div class="endpoint post">POST /ingest</div>
<p>Ingest pre-structured JSONL data.</p>
<pre><code>{
  "statements": [
    {"s": "ex:lisa-watts", "p": "bornIn", "o": {"iri": "ex:sydney"}},
    {"s": "ex:lisa-watts", "p": "hasBirthYear", "o": {"v": 1985, "dt": "xsd:integer"}}
  ],
  "context": "ctx:genes/lisa-watts"
}</code></pre>

<h2 id="query">5. Query Endpoints</h2>

<div class="endpoint get">GET /search?q=&lt;query&gt;&amp;limit=25</div>
<p>Fast full-text search across all entity labels. Trigram-indexed, responds in ~5ms.</p>
<pre><code>GET /search?q=lisa+watts&amp;limit=10

Response:
{
  "matches": [
    {"subject": "ctx:genealogy/research-db/iri/31448699f0e5", "label": "Lisa Watts", "count": 41},
    {"subject": "ctx:genealogy/research-db/iri/8d0ee1e126de", "label": "Lisa Watts", "count": 29}
  ],
  "q": "lisa watts"
}</code></pre>

<div class="endpoint get">GET /history/{subject}</div>
<p>Get ALL statements about a subject, including retracted ones. The most comprehensive view.</p>
<pre><code>GET /history/ex:mary-watson

Response:
{
  "count": 19,
  "rows": [
    {"statement_id": "uuid", "predicate": "bornInPlace", "object_iri": "ex:cornwall-england", ...},
    {"statement_id": "uuid", "predicate": "hasBirthYear", "object_lit": {"v": 1860, "dt": "xsd:integer"}, ...}
  ]
}</code></pre>

<div class="endpoint get">GET /match?subject=&amp;predicate=&amp;object_iri=&amp;context=&amp;polarity=&amp;min_maturity=</div>
<p>Pattern-match. All filters optional. Returns current (non-retracted) statements.</p>
<pre><code># Everything about a person
GET /match?subject=ex:mary-watson

# All marriages in a context
GET /match?predicate=marriedTo&amp;context=ctx:genes/my-research

# Only high-confidence facts
GET /match?subject=ex:mary-watson&amp;min_maturity=3

# Including contradictions
GET /match?subject=ex:mary-watson&amp;polarity=any</code></pre>

<div class="endpoint get">GET /subjects</div>
<p>List subjects with statement counts (top 50).</p>

<div class="endpoint get">GET /contexts</div>
<p>List all contexts in the graph.</p>

<div class="endpoint get">GET /predicates</div>
<p>List all predicates ordered by frequency. Useful for understanding the graph vocabulary.</p>

<div class="endpoint post">POST /query</div>
<p>Run DontoQL or SPARQL subset queries for complex graph traversals.</p>
<pre><code>// DontoQL
{"query": "MATCH ?s ?p ?o LIMIT 20"}

// SPARQL subset
{"query": "SELECT ?s ?p ?o WHERE { ?s ?p ?o } LIMIT 20"}</code></pre>

<div class="endpoint get">GET /shadow?subject=&amp;predicate=&amp;...</div>
<p>Like /match but expands through predicate alignments. Use after running /align/auto.</p>

<h2 id="alignment">6. Predicate Alignment</h2>

<p>Different LLM extraction runs mint different predicate names for the same relationship. The alignment layer maps them together so queries work across sources.</p>

<h3>The problem</h3>
<pre><code>Source 1 says: (ex:mary, bornIn, ex:cornwall)
Source 2 says: (ex:cornwall, birthplaceOf, ex:mary)
Source 3 says: (ex:mary, bornInPlace, ex:cornwall)

→ A query for "bornIn" misses 2 out of 3 facts!</code></pre>

<h3>The solution</h3>
<pre><code># Auto-align all predicates by name similarity
POST /align/auto?threshold=0.6

# Now a query expands through equivalences
GET /shadow?subject=ex:mary → returns ALL three facts</code></pre>

<h3>Alignment endpoints</h3>

<div class="endpoint post">POST /align/register</div>
<pre><code>{
  "source": "bornIn",
  "target": "birthplaceOf",
  "relation": "inverse_equivalent",
  "confidence": 0.95
}</code></pre>

<div class="endpoint post">POST /align/auto?threshold=0.6</div>
<p>Batch auto-align all predicates using trigram name similarity. This is the easiest way — run it once after extracting from multiple sources.</p>

<div class="endpoint post">POST /align/rebuild</div>
<p>Rebuild the closure index. Call after any manual registration.</p>

<div class="endpoint get">GET /align/suggest/{predicate}?threshold=0.3</div>
<p>Find predicates with similar names. Useful before manual registration.</p>

<h3>Relation types</h3>
<table>
<tr><th>Relation</th><th>Meaning</th><th>Query Effect</th><th>Example</th></tr>
<tr><td><code>exact_equivalent</code></td><td>Same meaning, same direction</td><td>Direct substitution</td><td>bornIn = bornInPlace</td></tr>
<tr><td><code>inverse_equivalent</code></td><td>Same meaning, swap S/O</td><td>Subject↔Object swap</td><td>bornIn ↔ birthplaceOf</td></tr>
<tr><td><code>sub_property_of</code></td><td>Specific → general</td><td>Upward expansion</td><td>assassinatedBy → killedBy</td></tr>
<tr><td><code>close_match</code></td><td>Similar not identical</td><td>Lower confidence</td><td>authored ≈ wroteFor</td></tr>
<tr><td><code>decomposition</code></td><td>One → n-ary frame</td><td>Component expansion</td><td>married → (date, location, spouse)</td></tr>
<tr><td><code>not_equivalent</code></td><td>Explicitly NOT same</td><td>Prevents bad auto-alignment</td><td>killed ≠ died</td></tr>
</table>

<h2 id="mutate">7. Mutate Endpoints</h2>

<div class="endpoint post">POST /retract/{statement_id}</div>
<p>Retract a statement. This is a <strong>bitemporal delete</strong> — the row stays in the database with its <code>tx_hi</code> closed. Historical queries still see it. Use when you discover a fact is wrong.</p>
<pre><code>POST /retract/8312e7fc-9312-4f25-8050-453f200f3096

Response: {"retracted": true, "statement_id": "8312e7fc..."}</code></pre>

<h2 id="evidence">8. Evidence &amp; Claims</h2>

<div class="endpoint get">GET /evidence/{statement_id}</div>
<p>Get evidence spans linked to a statement — text spans, source documents, extraction runs.</p>

<div class="endpoint get">GET /claim/{statement_id}</div>
<p>Full claim card: the statement plus its evidence, arguments, obligations, and blockers.</p>

<h2 id="tiers">9. Extraction Tiers (Deep Dive)</h2>

<p>The LLM extraction engine produces facts across 8 tiers. Most systems only extract Tier 1. Donto extracts all 8, capturing the full depth of meaning in a text.</p>

<table>
<tr><th>Tier</th><th>Category</th><th>What It Captures</th><th>Example Predicates</th><th>Typical % of Output</th></tr>
<tr><td>T1</td><td>Surface facts</td><td>What the text explicitly states</td><td>bornIn, marriedTo, childOf, diedOn, locatedIn, founderOf, memberOf, employedBy</td><td>30-40%</td></tr>
<tr><td>T2</td><td>Relational</td><td>How things connect to each other</td><td>causedBy, precedes, partOf, succeededBy, derivedFrom, dependsOn, contradicts</td><td>15-20%</td></tr>
<tr><td>T3</td><td>Opinions</td><td>Evaluative claims and stances</td><td>holdsOpinion, criticizes, advocatesFor, prefers, endorses, evaluatesAs</td><td>5-10%</td></tr>
<tr><td>T4</td><td>Epistemic</td><td>Knowledge status and certainty</td><td>assertsAsFact, speculatesAbout, believesThat, lacksEvidenceFor, consideredPossible</td><td>5-8%</td></tr>
<tr><td>T5</td><td>Rhetorical</td><td>What the text does (not says)</td><td>framesAs, emphasizes, hedgesClaim, appealsToAuthority, addressesAudience</td><td>5-8%</td></tr>
<tr><td>T6</td><td>Presuppositions</td><td>What the text assumes without stating</td><td>presupposesThat, impliesThat, notablyOmits, takesAsGiven, existsPriorTo</td><td>10-15%</td></tr>
<tr><td>T7</td><td>Philosophical</td><td>Deep ontological structure</td><td>reifiesAs, treatsAsEssentialProperty, counterfactuallyAssumes, hasEssenceOf</td><td>3-5%</td></tr>
<tr><td>T8</td><td>Intertextual</td><td>How text relates to broader context</td><td>drawsOnTradition, situatesInDiscourse, employsGenreConvention, historicallyContextualizes</td><td>3-5%</td></tr>
</table>

<h3>Confidence → Maturity Mapping</h3>
<table>
<tr><th>LLM Confidence</th><th>Maturity</th><th>Meaning</th></tr>
<tr><td>0.95 – 1.0</td><td>L4 (verified)</td><td>Directly and explicitly stated, zero inference</td></tr>
<tr><td>0.80 – 0.94</td><td>L3 (strong)</td><td>Minor inference from explicit text</td></tr>
<tr><td>0.60 – 0.79</td><td>L2 (moderate)</td><td>Significant inference, plausible interpretation</td></tr>
<tr><td>0.40 – 0.59</td><td>L1 (speculative)</td><td>Reading between the lines</td></tr>
<tr><td>0.00 – 0.39</td><td>L0 (raw)</td><td>Unverified, low confidence</td></tr>
</table>

<h2 id="iri">10. IRI &amp; Naming Conventions</h2>
<table>
<tr><th>Element</th><th>Convention</th><th>Examples</th></tr>
<tr><td>Subjects</td><td>kebab-lower-case with <code>ex:</code> prefix</td><td><code>ex:mary-watson</code>, <code>ex:cooktown-municipality</code>, <code>ex:lizard-island-attack</code></td></tr>
<tr><td>Predicates</td><td>camelCase (LLM-minted)</td><td><code>bornIn</code>, <code>marriedTo</code>, <code>oversawConstructionOf</code>, <code>presupposesThat</code></td></tr>
<tr><td>Contexts</td><td><code>ctx:namespace/topic/source</code></td><td><code>ctx:genes/mary-watson/obituary</code></td></tr>
<tr><td>Object IRIs</td><td>kebab-lower-case with <code>ex:</code> prefix</td><td><code>ex:cornwall-england</code>, <code>ex:queensland</code></td></tr>
<tr><td>Object literals</td><td>Short values only — names, dates, numbers</td><td><code>{"v": 1860, "dt": "xsd:integer"}</code></td></tr>
</table>

<div class="warning">
<strong>Never use booleans as objects.</strong> <code>{"v": true}</code> destroys information. Use a predicate instead: <code>(ex:city, wasA, ex:municipality)</code> not <code>(ex:city, wasMunicipality, true)</code>.
</div>

<h2 id="bitemporal">11. Bitemporal Model</h2>

<pre><code>           valid_lo ──────────── valid_hi
           (when fact was true)

  tx_lo ─────────────────────── tx_hi
  (when recorded)                (when retracted, or null if current)

  Example:
  - "Mary was married" valid 1879-1881, recorded 2026-05-02, not retracted
    valid_lo=1879, valid_hi=1881, tx_lo=2026-05-02, tx_hi=null

  - Wrong birth date retracted:
    valid_lo=1862, tx_lo=2026-05-01, tx_hi=2026-05-02  ← closed
    valid_lo=1860, tx_lo=2026-05-02, tx_hi=null         ← current</code></pre>

<h2 id="timeouts">12. Timeouts &amp; Performance</h2>

<table>
<tr><th>Endpoint</th><th>Typical Time</th><th>Notes</th></tr>
<tr><td>GET /health</td><td>&lt;100ms</td><td></td></tr>
<tr><td>GET /search</td><td>~400ms</td><td>Trigram-indexed label cache</td></tr>
<tr><td>GET /predicates, /subjects, /contexts</td><td>1-3s</td><td></td></tr>
<tr><td>GET /history/{subject}</td><td>1-5s</td><td>Depends on fact count</td></tr>
<tr><td>GET /match</td><td>1-10s</td><td>Depends on filters and result count</td></tr>
<tr><td>POST /assert, /assert/batch</td><td>&lt;500ms</td><td>Direct insert</td></tr>
<tr><td>POST /align/register, /rebuild</td><td>1-5s</td><td></td></tr>
<tr><td>POST /align/auto</td><td>5-30s</td><td>Scans all predicates</td></tr>
<tr><td><strong>POST /extract-and-ingest</strong></td><td><strong>30-120s</strong></td><td><strong>LLM call — set timeout to 600s</strong></td></tr>
</table>

<h2 id="examples">13. Complete Worked Examples</h2>

<h3>Example 1: Research a historical person</h3>
<pre><code># 1. Extract from an obituary
curl -X POST https://genes.apexpots.com/extract-and-ingest \\
  -H "Content-Type: application/json" \\
  -d '{
    "text": "Mary Watson was born in Cornwall, England in 1860. She married Robert Watson in Cooktown, Queensland in 1879. Robert was a beche-de-mer fisherman who operated from Lizard Island...",
    "context": "ctx:genes/mary-watson/obituary"
  }'

# 2. Search for her
curl https://genes.apexpots.com/search?q=mary+watson

# 3. Get her full profile
curl https://genes.apexpots.com/history/ex:mary-watson

# 4. Extract from a second source
curl -X POST https://genes.apexpots.com/extract-and-ingest \\
  -H "Content-Type: application/json" \\
  -d '{
    "text": "The monument in Cooktown commemorates Mrs Watson...",
    "context": "ctx:genes/mary-watson/monument-article"
  }'

# 5. Auto-align predicates across both sources
curl -X POST https://genes.apexpots.com/align/auto?threshold=0.6

# 6. Get unified view
curl https://genes.apexpots.com/history/ex:mary-watson</code></pre>

<h3>Example 2: Record a manual correction</h3>
<pre><code># Find the wrong fact
curl "https://genes.apexpots.com/match?subject=ex:mary-watson&predicate=hasBirthYear"
# → statement_id: "abc-123", object: 1862 (WRONG)

# Retract the wrong fact
curl -X POST https://genes.apexpots.com/retract/abc-123

# Assert the correct fact
curl -X POST https://genes.apexpots.com/assert \\
  -H "Content-Type: application/json" \\
  -d '{
    "subject": "ex:mary-watson",
    "predicate": "hasBirthYear",
    "object_lit": {"v": 1860, "dt": "xsd:integer"},
    "context": "ctx:genes/mary-watson/correction",
    "maturity": 4
  }'</code></pre>

<h3>Example 3: Batch import structured data</h3>
<pre><code>curl -X POST https://genes.apexpots.com/assert/batch \\
  -H "Content-Type: application/json" \\
  -d '{
    "statements": [
      {"subject": "ex:lisa-watts", "predicate": "rdf:type", "object_iri": "ex:Person", "context": "ctx:genes/lisa-watts"},
      {"subject": "ex:lisa-watts", "predicate": "rdfs:label", "object_lit": {"v": "Lisa Watts", "dt": "xsd:string"}, "context": "ctx:genes/lisa-watts"},
      {"subject": "ex:lisa-watts", "predicate": "childOf", "object_iri": "ex:thomas-davis", "context": "ctx:genes/lisa-watts", "maturity": 3}
    ]
  }'</code></pre>

<h2 id="errors">14. Error Handling</h2>
<table>
<tr><th>HTTP Code</th><th>Meaning</th><th>What To Do</th></tr>
<tr><td>200</td><td>Success</td><td>Parse the JSON response</td></tr>
<tr><td>400</td><td>Bad request</td><td>Check your request body format</td></tr>
<tr><td>500</td><td>Server error</td><td>Check the <code>error</code> field in response. Usually a database issue.</td></tr>
<tr><td>502</td><td>OpenRouter error</td><td>LLM extraction failed. Check API key or try again.</td></tr>
<tr><td>504</td><td>Timeout</td><td>Request took too long. For extract endpoints, increase your client timeout to 600s.</td></tr>
</table>

<p style="margin-top: 3rem; color: #888; text-align: center;">
Donto Knowledge Graph API v0.3.0 — 35.8 million statements<br>
<a href="/docs">Swagger UI</a> · <a href="/openapi.json">OpenAPI Spec</a> · <a href="/health">Health Check</a>
</p>

</body>
</html>"""


@app.get("/full-docs", response_class=HTMLResponse, tags=["System"], summary="Full documentation page")
async def full_docs():
    """Comprehensive documentation for agents and developers. HTML page with research strategies, worked examples, and complete endpoint reference."""
    return FULL_DOCS_HTML


SIMPLE_DOCS_HTML = """<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Donto API — Simple Guide</title>
<style>
  body { font-family: monospace; max-width: 800px; margin: 0 auto; padding: 2rem; line-height: 1.8; background: #111; color: #eee; }
  h1 { color: #4fc3f7; }
  h2 { color: #81c784; margin-top: 3rem; }
  pre { background: #1e1e1e; padding: 1rem; border-radius: 4px; overflow-x: auto; border: 1px solid #333; }
  code { color: #ce93d8; }
  .important { background: #ff9800; color: #000; padding: 0.5rem 1rem; font-weight: bold; }
  .step { background: #1e1e1e; border-left: 4px solid #4fc3f7; padding: 1rem; margin: 1rem 0; }
  a { color: #4fc3f7; }
  table { border-collapse: collapse; width: 100%; }
  th, td { border: 1px solid #444; padding: 8px; text-align: left; }
  th { background: #222; }
</style>
</head>
<body>

<h1>Donto API — Simple Guide for AI Agents</h1>

<p class="important">
BASE URL: https://genes.apexpots.com<br>
SET YOUR HTTP TIMEOUT TO 600 SECONDS (10 MINUTES).<br>
ALL REQUESTS USE JSON. SET Content-Type: application/json FOR POST REQUESTS.
</p>

<p>This is a knowledge graph with 35 million facts. You can add facts, search facts, and ask questions.</p>
<p><a href="/full-docs">Advanced documentation</a> · <a href="/docs">Swagger UI</a> · <a href="/openapi.json">OpenAPI spec</a></p>

<h2>WHAT YOU CAN DO</h2>

<p>There are only 5 things you need to do:</p>
<ol>
<li><strong>EXTRACT</strong> — Give it text, it finds all the facts in the text and saves them</li>
<li><strong>SEARCH</strong> — Find a person or thing by name</li>
<li><strong>HISTORY</strong> — Get all facts about a person or thing</li>
<li><strong>ADD</strong> — Add a fact manually</li>
<li><strong>ALIGN</strong> — Make similar predicates match each other (run once after adding lots of facts)</li>
</ol>

<h2>1. EXTRACT FACTS FROM TEXT</h2>

<p>This is the most important endpoint. Give it any text. It reads the text, finds every fact, and saves them all.</p>

<div class="step">
<p><strong>Send this:</strong></p>
<pre><code>POST https://genes.apexpots.com/extract-and-ingest
Content-Type: application/json

{
  "text": "PASTE THE FULL TEXT HERE",
  "context": "ctx:genes/TOPIC-NAME-HERE"
}</code></pre>

<p><strong>You get back:</strong></p>
<pre><code>{
  "facts_extracted": 72,
  "statements_ingested": 72,
  "elapsed_ms": 35000
}</code></pre>
</div>

<p><strong>Rules:</strong></p>
<ul>
<li>The text can be anything: an obituary, a newspaper article, a Wikipedia page, a transcript</li>
<li>The context is a label you choose. Use <code>ctx:genes/TOPIC</code> format. Examples: <code>ctx:genes/lisa-watts</code>, <code>ctx:genes/cook-expedition</code></li>
<li>This call takes 30-120 seconds. DO NOT set a short timeout.</li>
<li>Cost: about $0.005 per article</li>
</ul>

<h2>2. SEARCH FOR A PERSON OR THING</h2>

<div class="step">
<pre><code>GET https://genes.apexpots.com/search?q=lisa+watts</code></pre>

<p><strong>You get back:</strong></p>
<pre><code>{
  "matches": [
    {"subject": "ctx:genealogy/research-db/iri/31448699f0e5", "label": "Lisa Watts", "count": 41},
    {"subject": "ctx:genealogy/research-db/iri/8d0ee1e126de", "label": "Lisa Watts", "count": 29}
  ]
}</code></pre>
</div>

<p><strong>The "subject" is the ID you use in other calls. The "count" is how many facts exist about this person.</strong></p>

<h2>3. GET ALL FACTS ABOUT SOMEONE</h2>

<p>Use the subject from the search results:</p>

<div class="step">
<pre><code>GET https://genes.apexpots.com/history/ex:mary-watson</code></pre>

<p><strong>You get back:</strong></p>
<pre><code>{
  "count": 19,
  "rows": [
    {"predicate": "bornInPlace", "object_iri": "ex:cornwall-england"},
    {"predicate": "hasBirthYear", "object_lit": {"v": 1860}},
    {"predicate": "marriedTo", "object_iri": "ex:robert-watson"},
    ...
  ]
}</code></pre>
</div>

<p>You can also use the match endpoint with filters:</p>
<pre><code>GET https://genes.apexpots.com/match?subject=ex:mary-watson
GET https://genes.apexpots.com/match?predicate=marriedTo
GET https://genes.apexpots.com/match?context=ctx:genes/my-research</code></pre>

<h2>4. ADD A FACT MANUALLY</h2>

<div class="step">
<pre><code>POST https://genes.apexpots.com/assert
Content-Type: application/json

{
  "subject": "ex:lisa-watts",
  "predicate": "bornIn",
  "object_iri": "ex:sydney",
  "context": "ctx:genes/lisa-watts"
}</code></pre>
</div>

<p>For a number or text value instead of an entity:</p>
<pre><code>{
  "subject": "ex:lisa-watts",
  "predicate": "hasBirthYear",
  "object_lit": {"v": 1985, "dt": "xsd:integer"},
  "context": "ctx:genes/lisa-watts"
}</code></pre>

<p>For multiple facts at once:</p>
<pre><code>POST https://genes.apexpots.com/assert/batch
Content-Type: application/json

{
  "statements": [
    {"subject": "ex:lisa-watts", "predicate": "bornIn", "object_iri": "ex:sydney", "context": "ctx:genes/lisa-watts"},
    {"subject": "ex:lisa-watts", "predicate": "hasBirthYear", "object_lit": {"v": 1985, "dt": "xsd:integer"}, "context": "ctx:genes/lisa-watts"}
  ]
}</code></pre>

<h2>5. ALIGN PREDICATES (run once after adding many facts)</h2>

<p>Different text sources use different words for the same thing. "bornIn" and "birthplaceOf" mean the same thing. This command finds all the similar predicates and links them:</p>

<div class="step">
<pre><code>POST https://genes.apexpots.com/align/auto?threshold=0.6</code></pre>
</div>

<p>That's it. Run this once after you've extracted from multiple sources.</p>

<h2>COMPLETE EXAMPLE: Research a Person</h2>

<p>Here is exactly what to do, step by step:</p>

<div class="step">
<p><strong>Step 1:</strong> Find information about the person on the internet. Copy the text.</p>

<p><strong>Step 2:</strong> Extract facts from the text.</p>
<pre><code>POST https://genes.apexpots.com/extract-and-ingest
{
  "text": "PASTE OBITUARY TEXT HERE",
  "context": "ctx:genes/person-name/obituary"
}</code></pre>
<p>Wait for the response. It will take 30-120 seconds.</p>

<p><strong>Step 3:</strong> Find more sources. Extract from each one with a different context.</p>
<pre><code>POST https://genes.apexpots.com/extract-and-ingest
{"text": "PASTE NEWSPAPER ARTICLE HERE", "context": "ctx:genes/person-name/newspaper"}

POST https://genes.apexpots.com/extract-and-ingest
{"text": "PASTE WIKIPEDIA TEXT HERE", "context": "ctx:genes/person-name/wikipedia"}</code></pre>

<p><strong>Step 4:</strong> Align predicates.</p>
<pre><code>POST https://genes.apexpots.com/align/auto?threshold=0.6</code></pre>

<p><strong>Step 5:</strong> Search for the person.</p>
<pre><code>GET https://genes.apexpots.com/search?q=person+name</code></pre>

<p><strong>Step 6:</strong> Get all their facts.</p>
<pre><code>GET https://genes.apexpots.com/history/ex:person-name</code></pre>
</div>

<h2>ALL ENDPOINTS — QUICK REFERENCE</h2>

<table>
<tr><th>What</th><th>Method</th><th>URL</th><th>Speed</th></tr>
<tr><td>Health check</td><td>GET</td><td>/health</td><td>instant</td></tr>
<tr><td>Search by name</td><td>GET</td><td>/search?q=NAME</td><td>fast</td></tr>
<tr><td>All facts about X</td><td>GET</td><td>/history/SUBJECT</td><td>1-5s</td></tr>
<tr><td>Pattern match</td><td>GET</td><td>/match?subject=X&amp;predicate=Y</td><td>1-10s</td></tr>
<tr><td>List predicates</td><td>GET</td><td>/predicates</td><td>1-3s</td></tr>
<tr><td>List contexts</td><td>GET</td><td>/contexts</td><td>1-3s</td></tr>
<tr><td>List subjects</td><td>GET</td><td>/subjects</td><td>1-3s</td></tr>
<tr><td>Extract from text</td><td>POST</td><td>/extract-and-ingest</td><td><strong>30-120s</strong></td></tr>
<tr><td>Extract (preview)</td><td>POST</td><td>/extract</td><td><strong>30-120s</strong></td></tr>
<tr><td>Add one fact</td><td>POST</td><td>/assert</td><td>fast</td></tr>
<tr><td>Add many facts</td><td>POST</td><td>/assert/batch</td><td>fast</td></tr>
<tr><td>Delete a fact</td><td>POST</td><td>/retract/UUID</td><td>fast</td></tr>
<tr><td>Auto-align</td><td>POST</td><td>/align/auto?threshold=0.6</td><td>5-30s</td></tr>
<tr><td>Rebuild alignment</td><td>POST</td><td>/align/rebuild</td><td>1-5s</td></tr>
<tr><td>Register alignment</td><td>POST</td><td>/align/register</td><td>fast</td></tr>
<tr><td>Suggest alignments</td><td>GET</td><td>/align/suggest/PREDICATE</td><td>1-3s</td></tr>
<tr><td>Claim card</td><td>GET</td><td>/claim/UUID</td><td>1-3s</td></tr>
<tr><td>Evidence</td><td>GET</td><td>/evidence/UUID</td><td>1-3s</td></tr>
<tr><td>Run query</td><td>POST</td><td>/query</td><td>varies</td></tr>
</table>

<h2>OBJECT TYPES</h2>

<p>When adding facts, the object (the value) can be two things:</p>

<table>
<tr><th>Type</th><th>When to use</th><th>JSON</th></tr>
<tr><td>Entity (IRI)</td><td>The object is a person, place, or thing</td><td><code>"object_iri": "ex:sydney"</code></td></tr>
<tr><td>Value (literal)</td><td>The object is a number, date, or text</td><td><code>"object_lit": {"v": 1985, "dt": "xsd:integer"}</code></td></tr>
</table>

<p>Common data types for literals:</p>
<table>
<tr><th>Type</th><th>Example</th></tr>
<tr><td><code>xsd:string</code></td><td><code>{"v": "John Smith", "dt": "xsd:string"}</code></td></tr>
<tr><td><code>xsd:integer</code></td><td><code>{"v": 1985, "dt": "xsd:integer"}</code></td></tr>
<tr><td><code>xsd:date</code></td><td><code>{"v": "1985-03-15", "dt": "xsd:date"}</code></td></tr>
<tr><td><code>xsd:boolean</code></td><td>DO NOT USE. Never use booleans.</td></tr>
</table>

<h2>COMMON MISTAKES</h2>

<ol>
<li><strong>Timeout too short.</strong> Set your HTTP timeout to 600 seconds. The extract endpoint takes 30-120 seconds.</li>
<li><strong>Forgot Content-Type header.</strong> All POST requests need <code>Content-Type: application/json</code>.</li>
<li><strong>Using the wrong subject format.</strong> Subjects look like <code>ex:mary-watson</code> (kebab-case). NOT <code>MaryWatson</code> or <code>mary_watson</code>.</li>
<li><strong>Searching before extracting.</strong> The database only knows what you've told it. Extract from sources first.</li>
<li><strong>Not running align/auto.</strong> After extracting from multiple sources, run <code>POST /align/auto?threshold=0.6</code> once. Otherwise different predicate names won't match.</li>
<li><strong>Using object_literal instead of object_lit.</strong> The field name is <code>object_lit</code>, not <code>object_literal</code>.</li>
</ol>

<h2>WHERE TO FIND INFORMATION FOR RESEARCH</h2>

<p>When researching a person, search these sources in this order:</p>

<ol>
<li><strong>Obituaries</strong> — Best single source. Has birth date, death date, family members, locations, occupation. Try newspapers.com or local newspaper websites.</li>
<li><strong>Wikipedia</strong> — Good overview for notable people. Copy the full article text.</li>
<li><strong>Newspaper articles</strong> — Try trove.nla.gov.au (Australian), newspapers.com, or chroniclingamerica.loc.gov (US).</li>
<li><strong>Government records</strong> — Birth, death, marriage certificates. Try your state's BDM registry.</li>
<li><strong>Census records</strong> — Household details. Try FamilySearch.org.</li>
<li><strong>Church records</strong> — Baptisms, marriages, burials. Try FamilySearch.org.</li>
</ol>

<p>For each source you find, copy the full text and call <code>POST /extract-and-ingest</code> with it.</p>

<p style="margin-top: 3rem; color: #666; text-align: center;">
<a href="/full-docs">Advanced docs</a> · <a href="/docs">Swagger UI</a> · <a href="/openapi.json">OpenAPI spec</a>
</p>

</body>
</html>"""


@app.get("/simple-docs", response_class=HTMLResponse, tags=["System"], summary="Simple documentation for basic agents")
async def simple_docs():
    """Simple step-by-step guide. Copy-paste instructions. No theory. For agents with limited reasoning."""
    return SIMPLE_DOCS_HTML

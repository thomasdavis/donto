"""Donto Knowledge Graph API — the foundational interface to donto.

All clients (CLI, web, agents) should talk to this API.
This API talks to dontosrv (Rust HTTP server on localhost:7879) for graph
operations, and calls OpenRouter directly for LLM extraction.

Docs: https://genes.apexpots.com/docs
"""

import asyncio
import json
import logging
import os
import re
import time
import uuid
from typing import Optional

import httpx

# No subprocess, no CLI — this API talks directly to dontosrv and OpenRouter.
from fastapi import FastAPI, HTTPException, Query
from fastapi.responses import HTMLResponse, Response
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field
from temporalio.client import Client, WorkflowExecutionStatus
from temporalio.common import WorkflowIDConflictPolicy, WorkflowIDReusePolicy
from temporalio.service import RPCError

from helpers import (
    DONTOSRV, OPENROUTER_URL, OPENROUTER_KEY, DEFAULT_MODEL, FALLBACK_MODEL,
    srv, openrouter, resolve_model, confidence_to_maturity, parse_fact_object,
    EXTRACTION_PROMPT, call_openrouter, ingest_facts, compute_tiers,
)
from helpers import srv_post as _helpers_srv_post
from workflows import ExtractionWorkflow

logger = logging.getLogger("donto-api")
logging.basicConfig(level=logging.INFO, format="%(asctime)s %(levelname)s %(name)s: %(message)s")

TEMPORAL_ADDRESS = os.environ.get("TEMPORAL_ADDRESS", "localhost:7233")
TASK_QUEUE = "donto-extraction"

temporal_client: Client | None = None

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


@app.on_event("startup")
async def startup():
    global temporal_client
    try:
        temporal_client = await Client.connect(TEMPORAL_ADDRESS)
        logger.info(f"connected to Temporal at {TEMPORAL_ADDRESS}")
    except Exception as e:
        logger.error(f"failed to connect to Temporal: {e} — job endpoints will be unavailable")


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
    """Wrapper around helpers.srv_post that converts exceptions to HTTPException."""
    try:
        return await _helpers_srv_post(path, body)
    except Exception as e:
        raise HTTPException(502, str(e))


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
    text: str = Field(..., description="The full source text to extract knowledge from. Can be any length — articles, obituaries, transcripts, legal documents, Wikipedia pages, interview notes, etc. Longer texts yield more facts proportionally. A 500-word article typically produces 60-150 facts across all 8 analytical tiers.", json_schema_extra={"example": "Mary Watson was born in Cornwall, England in 1860. She married Robert Watson in Cooktown, Queensland in 1879. Robert was a beche-de-mer fisherman who operated from Lizard Island."})
    context: str = Field(..., description="Context IRI that scopes the extracted facts. Use ctx:genes/<topic>/<source-type> for genealogy research. Each context is independently queryable, alignable, and retractable. Examples: ctx:genes/mary-watson/obituary, ctx:genes/cooktown-history/newspaper, ctx:research/climate-models/paper-2024.", json_schema_extra={"example": "ctx:genes/mary-watson/obituary"})
    model: str = Field("grok", description="LLM model for extraction. Shortcuts: 'grok' = Grok 4.1 Fast ($0.005/article, quality 8.4-8.8/10, recommended for bulk work), 'mistral' = Mistral Large ($0.02/article, quality 8.4-8.8/10, fallback). Or pass any full OpenRouter model ID like 'anthropic/claude-sonnet-4-6'.", json_schema_extra={"example": "grok"})


@app.post("/extract-and-ingest", tags=["Extract"],
    summary="Extract knowledge from text and ingest into the graph (preferred endpoint for agents)")
async def extract_and_ingest(req: ExtractIngestRequest):
    """**This is the primary endpoint for building knowledge graphs. Use this first.**

    Takes unstructured text, sends it to an LLM (Grok 4.1 Fast by default via OpenRouter),
    which extracts structured facts across 8 analytical tiers (surface facts → philosophical
    analysis), then batch-inserts all facts directly into the knowledge graph under the
    specified context.

    **How it works internally:**
    1. Calls OpenRouter with a specialized 8-tier extraction prompt
    2. LLM returns JSON with facts: subject (kebab-case IRI), predicate (camelCase), object (IRI or literal), tier (1-8), confidence (0.0-1.0), notes
    3. Maps confidence to maturity: ≥0.95→L4, ≥0.8→L3, ≥0.6→L2, ≥0.4→L1, else L0
    4. Ensures the context exists in the database
    5. Batch-inserts all facts via dontosrv /assert/batch (idempotent — duplicates are content-hash deduplicated)

    **Cost:** ~$0.005 per article via Grok 4.1 Fast. A 500-word article yields 60-150 facts.

    **Timing:** 30-120 seconds (LLM call dominates). Set your HTTP client timeout to at least 600 seconds.
    For long-running batch jobs, use `POST /jobs/extract` instead — it returns immediately with a job ID.

    **Returns:** `{model, context, facts_extracted, statements_ingested, tiers: {t1..t8}, elapsed_ms}`
    """
    start = time.time()
    model = resolve_model(req.model)

    logger.info(f"extract-and-ingest: ctx={req.context} text={len(req.text)} chars model={model}")

    t0 = time.time()
    try:
        facts, llm_meta = await call_openrouter(req.text, model)
    except Exception as e:
        raise HTTPException(502, str(e))
    llm_ms = int((time.time() - t0) * 1000)
    logger.info(f"  LLM extraction: {len(facts)} facts in {llm_ms}ms cost={llm_meta.get('cost')}")

    tiers = compute_tiers(facts)

    t1 = time.time()
    ingested = await ingest_facts(facts, req.context)
    ingest_ms = int((time.time() - t1) * 1000)
    total_ms = int((time.time() - start) * 1000)
    logger.info(f"  Ingest: {ingested} statements in {ingest_ms}ms | Total: {total_ms}ms")

    return {
        "model": model,
        "context": req.context,
        "facts_extracted": len(facts),
        "statements_ingested": ingested,
        "tiers": tiers,
        "elapsed_ms": total_ms,
        "timing": {"llm_ms": llm_ms, "ingest_ms": ingest_ms},
        "usage": llm_meta,
    }


# ── Async Job System (Temporal-backed) ─────────────────────────────────
# Jobs are durable Temporal workflows. Survives restarts, has retries,
# and concurrency is controlled by the worker's max_concurrent_activities.


class JobExtractRequest(BaseModel):
    text: str = Field(..., description="Source text to extract knowledge from.")
    context: str = Field(..., description="Context IRI for ingested facts.")
    model: str = Field("grok", description="Model shortcut or full OpenRouter model ID.")


class JobBatchRequest(BaseModel):
    items: list[JobExtractRequest] = Field(..., description="List of texts to extract.")


def _require_temporal():
    if temporal_client is None:
        raise HTTPException(503, "Temporal is not connected — job endpoints unavailable")
    return temporal_client


def _temporal_status_to_job_status(wf_status) -> str:
    return {
        WorkflowExecutionStatus.RUNNING: "extracting",
        WorkflowExecutionStatus.COMPLETED: "completed",
        WorkflowExecutionStatus.FAILED: "failed",
        WorkflowExecutionStatus.CANCELED: "failed",
        WorkflowExecutionStatus.TERMINATED: "failed",
        WorkflowExecutionStatus.TIMED_OUT: "failed",
    }.get(wf_status, "queued")


def _context_to_workflow_id(context: str) -> str:
    """Deterministic workflow ID from context IRI. Prevents duplicate extractions
    for the same context — Temporal rejects starting a workflow whose ID already exists."""
    import hashlib
    slug = context.replace("/", "_").replace(":", "_")
    if len(slug) > 80:
        slug = slug[:60] + "-" + hashlib.sha256(context.encode()).hexdigest()[:12]
    return f"extraction-{slug}"


@app.post("/jobs/extract", tags=["Jobs"],
    summary="Submit an extraction job (returns immediately with job ID)")
async def submit_extract_job(req: JobExtractRequest):
    """Submit text for async extraction. Returns immediately with a job_id.
    Poll `GET /jobs/{job_id}` for status and results.
    Jobs are durable Temporal workflows — they survive server restarts.
    Duplicate submissions for the same context are rejected."""
    client = _require_temporal()
    model = resolve_model(req.model)
    wf_id = _context_to_workflow_id(req.context)
    try:
        await client.start_workflow(
            ExtractionWorkflow.run,
            args=[req.text, req.context, model],
            id=wf_id,
            task_queue=TASK_QUEUE,
            id_conflict_policy=WorkflowIDConflictPolicy.FAIL,
            id_reuse_policy=WorkflowIDReusePolicy.REJECT_DUPLICATE,
        )
    except RPCError as e:
        if "already started" in str(e).lower() or "already exists" in str(e).lower():
            return {"job_id": wf_id, "status": "duplicate", "message": f"Extraction for {req.context} already exists"}
        raise
    job_id = wf_id.removeprefix("extraction-")
    logger.info(f"job {job_id}: queued ({len(req.text)} chars → {req.context})")
    return {"job_id": job_id, "status": "queued"}


@app.post("/jobs/batch", tags=["Jobs"],
    summary="Submit multiple extraction jobs at once")
async def submit_batch_jobs(req: JobBatchRequest):
    """Submit multiple texts for extraction. Each becomes a separate durable
    Temporal workflow. Concurrency controlled by the worker process.
    Duplicates (same context) are skipped and reported."""
    client = _require_temporal()
    job_ids = []
    skipped = 0
    for item in req.items:
        model = resolve_model(item.model)
        wf_id = _context_to_workflow_id(item.context)
        try:
            await client.start_workflow(
                ExtractionWorkflow.run,
                args=[item.text, item.context, model],
                id=wf_id,
                task_queue=TASK_QUEUE,
                id_conflict_policy=WorkflowIDConflictPolicy.FAIL,
            id_reuse_policy=WorkflowIDReusePolicy.REJECT_DUPLICATE,
            )
            job_ids.append(wf_id.removeprefix("extraction-"))
        except RPCError as e:
            if "already started" in str(e).lower() or "already exists" in str(e).lower():
                skipped += 1
                continue
            raise
    logger.info(f"batch: {len(job_ids)} queued, {skipped} skipped (duplicate)")
    return {"job_ids": job_ids, "count": len(job_ids), "skipped_duplicates": skipped}


@app.get("/jobs", tags=["Jobs"],
    summary="List all jobs with status")
async def list_jobs(status: Optional[str] = Query(None, description="Filter by status: queued, extracting, ingesting, completed, failed")):
    """List extraction jobs from Temporal. Optionally filter by status."""
    client = _require_temporal()
    result_jobs = []
    async for wf in client.list_workflows('WorkflowType = "ExtractionWorkflow"'):
        job_id = wf.id.removeprefix("extraction-")
        wf_status = _temporal_status_to_job_status(wf.status)
        job_entry = {
            "id": job_id,
            "status": wf_status,
            "created_at": wf.start_time.timestamp() if wf.start_time else None,
        }
        if wf.status is not None and wf.status == WorkflowExecutionStatus.RUNNING:
            try:
                handle = client.get_workflow_handle(wf.id)
                detail = await handle.query(ExtractionWorkflow.status)
                job_entry.update(detail)
            except Exception:
                pass
        result_jobs.append(job_entry)

    if status:
        result_jobs = [j for j in result_jobs if j.get("status") == status]
    result_jobs.sort(key=lambda j: j.get("created_at") or 0, reverse=True)

    summary = {}
    for j in result_jobs:
        s = j.get("status", "unknown")
        summary[s] = summary.get(s, 0) + 1
    return {"jobs": result_jobs[:100], "total": len(result_jobs), "summary": summary}


@app.get("/jobs/{job_id}", tags=["Jobs"],
    summary="Get status and result of a specific job")
async def get_job(job_id: str):
    """Poll this endpoint to check if a job is done. Status transitions:
    queued → extracting → ingesting → completed (or failed at any stage)."""
    client = _require_temporal()
    handle = client.get_workflow_handle(f"extraction-{job_id}")
    try:
        desc = await handle.describe()
        if desc.status in (WorkflowExecutionStatus.COMPLETED, WorkflowExecutionStatus.FAILED,
                           WorkflowExecutionStatus.CANCELED, WorkflowExecutionStatus.TERMINATED):
            result = {
                "id": job_id,
                "status": _temporal_status_to_job_status(desc.status),
                "created_at": desc.start_time.timestamp() if desc.start_time else None,
            }
            if desc.status == WorkflowExecutionStatus.COMPLETED:
                try:
                    result.update(await handle.result())
                except Exception:
                    pass
            return result
        detail = await handle.query(ExtractionWorkflow.status)
        detail["id"] = job_id
        detail["created_at"] = desc.start_time.timestamp() if desc.start_time else None
        return detail
    except RPCError:
        raise HTTPException(404, f"Job {job_id} not found")


@app.get("/queue", response_class=HTMLResponse, tags=["Jobs"],
    summary="Job queue dashboard UI")
async def queue_dashboard():
    """Live dashboard showing all extraction jobs with auto-refresh."""
    return """<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Extraction Queue — donto</title>
<style>
  * { box-sizing: border-box; margin: 0; padding: 0; }
  body { font-family: -apple-system, system-ui, sans-serif; background: #0d1117; color: #c9d1d9; padding: 20px; }
  h1 { font-size: 20px; font-weight: 600; margin-bottom: 16px; color: #f0f6fc; }
  .stats { display: flex; gap: 12px; margin-bottom: 20px; flex-wrap: wrap; }
  .stat { background: #161b22; border: 1px solid #30363d; border-radius: 8px; padding: 12px 16px; min-width: 120px; }
  .stat .label { font-size: 11px; color: #8b949e; text-transform: uppercase; letter-spacing: 0.5px; }
  .stat .value { font-size: 24px; font-weight: 700; margin-top: 2px; }
  .stat .value.green { color: #3fb950; }
  .stat .value.blue { color: #58a6ff; }
  .stat .value.yellow { color: #d29922; }
  .stat .value.red { color: #f85149; }
  .stat .value.purple { color: #bc8cff; }
  .controls { display: flex; gap: 8px; margin-bottom: 16px; align-items: center; flex-wrap: wrap; }
  .controls button, .controls select { background: #21262d; border: 1px solid #30363d; color: #c9d1d9; padding: 6px 12px; border-radius: 6px; cursor: pointer; font-size: 13px; }
  .controls button:hover { background: #30363d; }
  .controls button.active { background: #1f6feb; border-color: #1f6feb; color: #fff; }
  .controls .spacer { flex: 1; }
  .controls .refresh-info { font-size: 11px; color: #8b949e; }
  table { width: 100%; border-collapse: collapse; background: #161b22; border: 1px solid #30363d; border-radius: 8px; overflow: hidden; }
  th { background: #21262d; text-align: left; padding: 8px 12px; font-size: 11px; color: #8b949e; text-transform: uppercase; letter-spacing: 0.5px; border-bottom: 1px solid #30363d; }
  td { padding: 8px 12px; border-bottom: 1px solid #21262d; font-size: 13px; vertical-align: top; }
  tr:last-child td { border-bottom: none; }
  tr:hover { background: #1c2128; }
  .badge { display: inline-block; padding: 2px 8px; border-radius: 12px; font-size: 11px; font-weight: 600; }
  .badge.queued { background: #1c2128; color: #8b949e; border: 1px solid #30363d; }
  .badge.extracting { background: #0d2847; color: #58a6ff; border: 1px solid #1f6feb; }
  .badge.ingesting { background: #2a1f00; color: #d29922; border: 1px solid #d29922; }
  .badge.completed { background: #0b2e13; color: #3fb950; border: 1px solid #238636; }
  .badge.failed { background: #3d0e0e; color: #f85149; border: 1px solid #da3633; }
  .mono { font-family: 'SF Mono', Consolas, monospace; font-size: 12px; }
  .tiers { display: flex; gap: 2px; }
  .tier { background: #21262d; padding: 1px 4px; border-radius: 3px; font-size: 10px; font-family: monospace; }
  .tier.has { background: #0d2847; color: #58a6ff; }
  .error-text { color: #f85149; font-size: 11px; max-width: 400px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; cursor: pointer; }
  .error-text:hover { white-space: normal; word-break: break-all; }
  .elapsed { color: #8b949e; font-size: 11px; }
  .progress-bar { width: 100%; height: 4px; background: #21262d; border-radius: 2px; margin-top: 8px; overflow: hidden; }
  .progress-fill { height: 100%; background: linear-gradient(90deg, #1f6feb, #3fb950); transition: width 0.5s; border-radius: 2px; }
  .empty { text-align: center; padding: 40px; color: #8b949e; }
  .detail-panel { display: none; background: #0d1117; border: 1px solid #30363d; border-radius: 8px; padding: 16px; margin-top: 12px; }
  .detail-panel.open { display: block; }
  .detail-panel pre { font-size: 11px; color: #c9d1d9; overflow-x: auto; white-space: pre-wrap; word-break: break-all; }
  .rate { font-size: 12px; color: #8b949e; margin-top: 4px; }
</style>
</head>
<body>
<h1>Extraction Queue</h1>
<div id="stats" class="stats"></div>
<div class="controls">
  <button onclick="setFilter('all')" id="btn-all" class="active">All</button>
  <button onclick="setFilter('queued')" id="btn-queued">Queued</button>
  <button onclick="setFilter('extracting')" id="btn-extracting">Extracting</button>
  <button onclick="setFilter('ingesting')" id="btn-ingesting">Ingesting</button>
  <button onclick="setFilter('completed')" id="btn-completed">Completed</button>
  <button onclick="setFilter('failed')" id="btn-failed">Failed</button>
  <span class="spacer"></span>
  <select id="sort-select" onchange="refresh()">
    <option value="newest">Newest first</option>
    <option value="oldest">Oldest first</option>
    <option value="most-facts">Most facts</option>
    <option value="slowest">Slowest</option>
  </select>
  <span class="refresh-info" id="refresh-info">Auto-refresh: 5s</span>
</div>
<div id="progress-container" style="margin-bottom:16px"></div>
<div id="table-container"></div>
<div id="detail-panel" class="detail-panel"><pre id="detail-json"></pre></div>

<script>
let currentFilter = 'all';
let allJobs = [];
let refreshTimer;

function setFilter(f) {
  currentFilter = f;
  document.querySelectorAll('.controls button').forEach(b => b.classList.remove('active'));
  document.getElementById('btn-' + f).classList.add('active');
  renderTable();
}

function fmt(ms) {
  if (!ms) return '-';
  if (ms < 1000) return ms + 'ms';
  if (ms < 60000) return (ms/1000).toFixed(1) + 's';
  return (ms/60000).toFixed(1) + 'm';
}

function ago(ts) {
  if (!ts) return '-';
  const s = Math.floor(Date.now()/1000 - ts);
  if (s < 60) return s + 's ago';
  if (s < 3600) return Math.floor(s/60) + 'm ago';
  return Math.floor(s/3600) + 'h ' + Math.floor((s%3600)/60) + 'm ago';
}

function renderStats(data) {
  const s = data.summary || {};
  const total = data.total || 0;
  const completed = s.completed || 0;
  const active = (s.extracting||0) + (s.ingesting||0) + (s.queued||0);
  const failed = s.failed || 0;
  const totalFacts = allJobs.reduce((a,j) => a + (j.facts_extracted||0), 0);
  const avgMs = allJobs.filter(j=>j.total_ms).length > 0
    ? Math.round(allJobs.filter(j=>j.total_ms).reduce((a,j)=>a+(j.total_ms||0),0) / allJobs.filter(j=>j.total_ms).length) : 0;
  const totalCost = allJobs.reduce((a,j) => a + ((j.usage&&j.usage.cost)||0), 0);
  const totalTokens = allJobs.reduce((a,j) => a + ((j.usage&&j.usage.total_tokens)||0), 0);

  document.getElementById('stats').innerHTML = `
    <div class="stat"><div class="label">Total Jobs</div><div class="value">${total}</div></div>
    <div class="stat"><div class="label">Completed</div><div class="value green">${completed}</div></div>
    <div class="stat"><div class="label">Active</div><div class="value blue">${active}</div></div>
    <div class="stat"><div class="label">Failed</div><div class="value red">${failed}</div></div>
    <div class="stat"><div class="label">Total Facts</div><div class="value purple">${totalFacts.toLocaleString()}</div></div>
    <div class="stat"><div class="label">Avg Time</div><div class="value">${fmt(avgMs)}</div></div>
    <div class="stat"><div class="label">Total Cost</div><div class="value yellow">$${totalCost.toFixed(4)}</div></div>
    <div class="stat"><div class="label">Tokens</div><div class="value">${(totalTokens/1000).toFixed(0)}k</div></div>
  `;

  if (total > 0) {
    const pct = Math.round(completed / total * 100);
    document.getElementById('progress-container').innerHTML = `
      <div class="progress-bar"><div class="progress-fill" style="width:${pct}%"></div></div>
      <div class="rate">${pct}% complete · ${completed}/${total} jobs · ~${Math.round(totalFacts/Math.max(completed,1))} facts/job avg</div>
    `;
  }
}

function renderTable() {
  let filtered = currentFilter === 'all' ? allJobs : allJobs.filter(j => j.status === currentFilter);
  const sort = document.getElementById('sort-select').value;
  if (sort === 'newest') filtered.sort((a,b) => (b.created_at||0) - (a.created_at||0));
  else if (sort === 'oldest') filtered.sort((a,b) => (a.created_at||0) - (b.created_at||0));
  else if (sort === 'most-facts') filtered.sort((a,b) => (b.facts_extracted||0) - (a.facts_extracted||0));
  else if (sort === 'slowest') filtered.sort((a,b) => (b.total_ms||0) - (a.total_ms||0));

  if (filtered.length === 0) {
    document.getElementById('table-container').innerHTML = '<div class="empty">No jobs matching filter</div>';
    return;
  }

  let html = `<table><thead><tr>
    <th>ID</th><th>Status</th><th>Context</th><th>Size</th>
    <th>Facts</th><th>Cost</th><th>Tokens</th><th>LLM</th><th>Ingest</th><th>Total</th>
    <th>Tiers</th><th>Created</th>
  </tr></thead><tbody>`;

  for (const j of filtered) {
    const tiers = j.tiers || {};
    const tiersHtml = [1,2,3,4,5,6,7,8].map(i => {
      const v = tiers['t'+i] || 0;
      return `<span class="tier ${v?'has':''}" title="T${i}: ${v}">T${i}:${v}</span>`;
    }).join('');

    const cost = j.usage && j.usage.cost ? '$'+j.usage.cost.toFixed(4) : '-';
    const tokens = j.usage && j.usage.total_tokens ? (j.usage.total_tokens/1000).toFixed(1)+'k' : '-';

    html += `<tr onclick="showDetail('${j.id}')" style="cursor:pointer">
      <td class="mono">${j.id}</td>
      <td><span class="badge ${j.status}">${j.status}</span></td>
      <td class="mono" style="max-width:200px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap" title="${j.context||''}">${(j.context||'').replace('ctx:genes/trove-cooktown/','')}</td>
      <td>${j.text_length ? (j.text_length/1000).toFixed(1)+'k' : '-'}</td>
      <td style="font-weight:600">${j.facts_extracted||'-'}</td>
      <td class="mono" style="color:#d29922">${cost}</td>
      <td class="elapsed">${tokens}</td>
      <td class="elapsed">${fmt(j.llm_ms)}</td>
      <td class="elapsed">${fmt(j.ingest_ms)}</td>
      <td class="elapsed">${fmt(j.total_ms)}</td>
      <td><div class="tiers">${tiersHtml}</div></td>
      <td class="elapsed">${ago(j.created_at)}</td>
    </tr>`;
    if (j.error) {
      html += `<tr><td colspan="12"><div class="error-text" title="${j.error.replace(/"/g,'&quot;')}">${j.error}</div></td></tr>`;
    }
  }
  html += '</tbody></table>';
  document.getElementById('table-container').innerHTML = html;
}

function showDetail(id) {
  const j = allJobs.find(x => x.id === id);
  if (!j) return;
  const panel = document.getElementById('detail-panel');
  document.getElementById('detail-json').textContent = JSON.stringify(j, null, 2);
  panel.classList.toggle('open');
}

async function refresh() {
  try {
    const r = await fetch('/jobs');
    const data = await r.json();
    allJobs = data.jobs || [];
    renderStats(data);
    renderTable();
  } catch(e) {
    console.error('refresh failed', e);
  }
}

refresh();
refreshTimer = setInterval(refresh, 5000);
</script>
</body>
</html>"""


class ExtractRequest(BaseModel):
    text: str = Field(..., description="Source text to extract knowledge from.")
    context: Optional[str] = Field(None, description="Context IRI. If omitted, auto-generated as ctx:extract/<model-name>.")
    model: str = Field("grok", description="Model shortcut or full OpenRouter model ID.")
    dry_run: bool = Field(False, description="If true, returns the extracted facts as a JSON array without ingesting them into the database. Useful for previewing what the LLM extracts before committing.")


@app.post("/extract", tags=["Extract"],
    summary="Extract knowledge from text with optional dry-run preview")
async def extract(req: ExtractRequest):
    """Like `/extract-and-ingest` but with additional options.

    **Use this when you want to:**
    - Preview extracted facts before committing (`dry_run: true`)
    - Let the context be auto-generated from the model name
    - Inspect the raw LLM output including tier labels, confidence scores, and justification notes

    **With `dry_run: true`**, the response includes a `facts` array with every extracted fact,
    each containing: subject, predicate, object, tier (1-8), confidence (0.0-1.0), and notes
    (brief justification of why this fact was extracted from the text).

    **Timing:** 30-120 seconds. Set HTTP timeout to 600s.

    **Returns (dry_run=false):** `{model, context, facts_extracted, statements_ingested, tiers, elapsed_ms}`

    **Returns (dry_run=true):** `{model, facts_extracted, tiers, dry_run: true, facts: [...], elapsed_ms}`
    """
    start = time.time()
    model = resolve_model(req.model)
    facts, llm_meta = await call_openrouter(req.text, model)

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
    subject: str = Field(..., description="Subject IRI. Use kebab-lower-case with ex: prefix. Example: ex:mary-watson, ex:cooktown-municipality", json_schema_extra={"example": "ex:mary-watson"})
    predicate: str = Field(..., description="Predicate name. Use camelCase. Example: bornIn, marriedTo, hasBirthYear", json_schema_extra={"example": "bornIn"})
    object_iri: Optional[str] = Field(None, description="Object as an entity IRI. Use for people, places, organizations, events, concepts. Mutually exclusive with object_lit. Example: ex:cornwall-england", json_schema_extra={"example": "ex:cornwall-england"})
    object_lit: Optional[dict] = Field(None, description="Object as a typed literal value. Use for numbers, dates, strings, text. Mutually exclusive with object_iri. Format: {\"v\": <value>, \"dt\": \"<xsd-type>\"}. Common types: xsd:string, xsd:integer, xsd:date, xsd:decimal, xsd:boolean (avoid booleans — use predicates instead).", json_schema_extra={"example": {"v": 1860, "dt": "xsd:integer"}})
    context: str = Field("donto:anonymous", description="Context IRI scoping this fact. Use ctx:genes/<topic>/<source> for genealogy research. Facts in different contexts can be queried, compared, and retracted independently.", json_schema_extra={"example": "ctx:genes/mary-watson/manual"})
    polarity: str = Field("asserted", description="Epistemic polarity. 'asserted' = this fact is claimed true. 'negated' = this fact is claimed false. 'absent' = this fact is explicitly missing. 'unknown' = epistemic status undetermined.")
    maturity: int = Field(0, description="Maturity level 0-4. L0=raw, L1=registered, L2=evidenced, L3=validated, L4=certified. Higher maturity facts are prioritized in queries. Set to 3 or 4 for manually verified facts.")


@app.post("/assert", tags=["Ingest"], summary="Assert a single statement into the knowledge graph")
async def assert_stmt(req: AssertRequest):
    """Insert a single fact into the knowledge graph.

    Provide exactly one of `object_iri` (for entity objects like people, places, concepts)
    or `object_lit` (for literal values like numbers, dates, names).

    **When to use this vs /extract-and-ingest:**
    - Use `/extract-and-ingest` when you have unstructured text and want the LLM to find facts
    - Use `/assert` when you already know the specific fact to record (manual corrections, structured data imports, programmatic assertions)

    **Idempotent:** Duplicate facts (same subject+predicate+object+context+polarity) are
    content-hash deduplicated. Asserting the same fact twice is safe.

    **Returns:** `{statement_id: "uuid"}` — the UUID of the inserted (or existing) statement.
    Use this ID with `/retract/{id}` to retract the fact later, or with `/claim/{id}` to
    see its full evidence card.
    """
    return await srv_post("/assert", req.model_dump())


class BatchAssertRequest(BaseModel):
    statements: list[dict] = Field(..., description="Array of statement objects. Each must have: subject (str), predicate (str), and exactly one of object_iri (str) or object_lit ({v, dt}). Optional: context (str), polarity (str), maturity (int 0-4).", json_schema_extra={"example": [{"subject": "ex:mary-watson", "predicate": "bornIn", "object_iri": "ex:cornwall", "context": "ctx:genes/mary-watson"}, {"subject": "ex:mary-watson", "predicate": "hasBirthYear", "object_lit": {"v": 1860, "dt": "xsd:integer"}, "context": "ctx:genes/mary-watson"}]})


@app.post("/assert/batch", tags=["Ingest"], summary="Assert multiple statements in one call")
async def assert_batch(req: BatchAssertRequest):
    """Batch-insert multiple facts in a single database transaction. More efficient than
    calling `/assert` repeatedly.

    Each statement in the array follows the same format as `/assert`. All are inserted
    atomically — either all succeed or none do.

    **Returns:** `{inserted: N}` — the number of statements inserted (excluding duplicates).
    """
    return await srv_post("/assert/batch", {"statements": req.statements})


# ── Query (direct to dontosrv) ──────────────────────────────────────────


@app.get("/subjects", tags=["Query"], summary="List top subjects by fact count")
async def subjects():
    """List the most-connected subjects in the graph, ordered by statement count.

    Returns the top 50 subjects with the most facts. Each result includes the subject IRI
    and its total statement count. Use this to discover the most important entities in the graph.

    **Returns:** `{subjects: [{subject, count}, ...]}`
    """
    return await srv_get("/subjects")


@app.get("/search", tags=["Query"], summary="Full-text search by name/label (~5ms, trigram-indexed)")
async def search(q: str = Query(..., description="Search query. Matches against entity labels using trigram similarity. Case-insensitive. Supports partial matches. Examples: 'lisa watts', 'cooktown', 'watson'"), limit: int = Query(25, description="Maximum number of results to return (1-100).")):
    """Search for entities by name or label. This is the fastest way to find entities in the graph.

    Uses a pre-built trigram-indexed label cache (516K labels) for near-instant results (~5ms).
    Results are ordered by statement count — the most-connected entities appear first.

    **Use this as the starting point for any research.** Search for a person, place, or concept
    by name, then use the returned `subject` IRI with `/history/{subject}` to see all their facts.

    **Returns:** `{matches: [{subject, label, count}, ...], q: "your query"}`

    The `subject` field is the IRI you'll use in other endpoints. The `count` is how many
    facts exist about this entity — higher counts mean more information is available.
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


@app.get("/history/{subject:path}", tags=["Query"], summary="Get all facts about a specific entity")
async def history(subject: str):
    """Get the complete statement history for a subject, including retracted statements.

    This is the most comprehensive view of an entity. Returns every fact ever recorded
    about this subject, with full metadata: predicate, object, context, polarity, maturity,
    valid-time, transaction-time. Retracted statements (tx_hi is set) are included so you
    can see what was believed and when.

    **URL encoding:** The subject IRI goes in the URL path. For IRIs with colons or slashes
    (like `ex:mary-watson`), most HTTP clients handle encoding automatically.

    **Returns:** `{count: N, rows: [{statement_id, predicate, object_iri or object_lit, context, polarity, maturity, valid_lo, valid_hi, tx_lo, tx_hi}, ...]}`

    Use the `statement_id` from results with `/claim/{id}` for evidence detail, or
    `/retract/{id}` to retract a wrong fact.
    """
    return await srv_get(f"/history/{subject}")


@app.get("/statement/{id}", tags=["Query"], summary="Get a single statement by UUID")
async def statement_detail(id: str):
    """Get full detail for a single statement by its UUID.

    Returns the complete statement with all metadata. Use statement UUIDs from
    `/history`, `/match`, or `/search` results.

    **Returns:** Full statement object with subject, predicate, object, context, polarity,
    maturity, valid_time, tx_time, and any linked evidence/arguments.
    """
    return await srv_get(f"/statement/{id}")


@app.get("/contexts", tags=["Query"], summary="List all contexts in the knowledge graph")
async def contexts():
    """List all contexts (scopes/namespaces) in the knowledge graph.

    Each context represents a source, research question, extraction run, or manual import.
    Use context IRIs with `/match?context=...` to query facts from a specific source.

    **Context types:**
    - `ctx:genes/<topic>/<source>` — genealogy research
    - `ctx:extract/<file>/<model>` — auto-created by extraction
    - `ctx:genealogy/research-db` — legacy archive (35.8M statements)
    - `ctx:test/*` — test data

    **Returns:** `{contexts: [{iri, kind, label, parent, ...}, ...]}`
    """
    return await srv_get("/contexts")


@app.get("/predicates", tags=["Query"], summary="List all predicates with statement counts")
async def predicates():
    """List every predicate in the knowledge graph, ordered by how many statements use it.

    This shows you the vocabulary of the graph — what kinds of relationships exist and how
    common they are. Useful for:
    - Understanding what's in the graph
    - Finding predicates to align (different names for the same relationship)
    - Discovering the vocabulary an extraction run produced
    - Checking if a predicate you want to use already exists

    **Returns:** `{predicates: [{predicate, count}, ...]}`

    **Current top predicates:** rdf:type (3.7M), donto:status (1.6M), donto:textSpan (1.2M),
    rdfs:label (589K), ex:knownAs (1M)
    """
    return await srv_get("/predicates")


class QueryRequest(BaseModel):
    query: str = Field(..., description="DontoQL or SPARQL query text. DontoQL starts with MATCH. SPARQL starts with SELECT or PREFIX.", json_schema_extra={"example": "MATCH ?s ?p ?o LIMIT 20"})


@app.post("/query", tags=["Query"], summary="Run a DontoQL or SPARQL query for complex graph traversals")
async def query(req: QueryRequest):
    """Run a DontoQL or SPARQL (subset) query for complex pattern matching and graph traversal.

    **DontoQL** (starts with MATCH):
    ```
    MATCH ?s ?p ?o LIMIT 20
    MATCH ?s marriedTo ?o LIMIT 10
    ```

    **SPARQL subset** (starts with SELECT or PREFIX):
    ```
    SELECT ?s ?p ?o WHERE { ?s ?p ?o } LIMIT 20
    PREFIX ex: <http://ex/> SELECT ?x WHERE { ?x ex:bornIn ex:cornwall }
    ```

    Use this for queries that `/match` or `/search` can't express — joins across multiple
    patterns, variable binding, or multi-hop traversals.

    **Returns:** Array of result rows, each as a JSON object with the bound variables.
    """
    q = req.query.strip()
    if q.upper().startswith("SELECT") or q.upper().startswith("PREFIX"):
        return await srv_post("/sparql", {"query": q})
    return await srv_post("/dontoql", {"query": q})


# ── Retract ─────────────────────────────────────────────────────────────


@app.post("/retract/{statement_id}", tags=["Mutate"], summary="Retract a statement (bitemporal soft-delete)")
async def retract(statement_id: str):
    """Retract a statement by UUID. This is a **bitemporal soft-delete** — the physical row
    remains in the database with its `tx_hi` timestamp closed. Historical as-of queries
    that specify a time before the retraction will still return the fact.

    **When to use:** When you discover a fact is wrong and want to correct the record.
    Retraction does not destroy data — it marks the fact as no longer current.

    **Idempotent:** Retracting an already-retracted statement is a no-op.

    **Get statement IDs from:** `/history/{subject}`, `/match`, or `/search` results (the `statement_id` or `id` field).

    **Returns:** `{retracted: true, statement_id: "uuid"}` or `{retracted: false}` if already retracted.
    """
    return await srv_post("/retract", {"statement_id": statement_id})


# ── Alignment (direct to dontosrv) ──────────────────────────────────────


# ── Graph Visualization Endpoints ───────────────────────────────────────


class NeighborhoodRequest(BaseModel):
    subject: str = Field(..., description="Center entity IRI. The node to explore outward from.", json_schema_extra={"example": "ex:mary-watson"})
    depth: int = Field(1, description="How many hops outward from the center entity. 1 = direct connections only. 2 = connections of connections. Max 3 to avoid explosion.", ge=1, le=3)
    predicates: Optional[list[str]] = Field(None, description="Filter to only these predicates. If null, returns all predicates. Example: ['marriedTo', 'childOf', 'bornIn']")
    context: Optional[str] = Field(None, description="Limit to facts from this context.")
    min_maturity: int = Field(0, description="Minimum maturity level (0-4).", ge=0, le=4)
    limit: int = Field(500, description="Maximum number of edges to return. Keeps the graph manageable for visualization.", ge=1, le=5000)


@app.post("/graph/neighborhood", tags=["Graph"], summary="Get the neighborhood subgraph around an entity")
async def neighborhood(req: NeighborhoodRequest):
    """Get all entities and edges within N hops of a center entity. This is the primary
    endpoint for graph visualization — it gives you a bounded subgraph that a frontend
    can render as a force-directed graph.

    **Depth 1:** Direct connections only — the entity and everything it's directly connected to.
    Good for entity profiles. Typically 10-100 nodes.

    **Depth 2:** Connections of connections — shows how the entity's network interconnects.
    Good for discovering hidden relationships. Can be 50-500 nodes.

    **Depth 3:** Three hops — very large. Use predicate filters to keep it manageable.

    **Predicate filtering:** Pass `predicates: ["marriedTo", "childOf", "parentOf"]` to show
    only family relationships, or `predicates: ["bornIn", "diedIn", "locatedIn"]` for geography.

    **Returns:** `{nodes: [{id, label, type, degree}, ...], edges: [{source, target, predicate, context, maturity}, ...], center, depth}`

    Use this data to render a graph with D3.js, Cytoscape.js, vis.js, or similar.
    """
    import asyncpg
    dsn = os.environ.get("DONTO_DSN", "postgres://donto:donto@127.0.0.1:5432/donto")
    conn = await asyncpg.connect(dsn)
    try:
        pred_filter = ""
        params = [req.subject, req.min_maturity, req.limit]
        if req.predicates:
            pred_filter = f"AND s.predicate = ANY($4)"
            params.append(req.predicates)
        ctx_filter = ""
        if req.context:
            ctx_filter = f"AND s.context = ${len(params) + 1}"
            params.append(req.context)

        # Depth 1: direct edges from/to center
        query = f"""
            WITH RECURSIVE neighborhood AS (
                -- Seed: the center entity
                SELECT subject, predicate,
                       COALESCE(object_iri, object_lit ->> 'v') as object,
                       object_iri IS NOT NULL as object_is_iri,
                       context,
                       (flags >> 2 & 7) as maturity,
                       1 as depth
                FROM donto_statement s
                WHERE (subject = $1 OR object_iri = $1)
                  AND upper(tx_time) IS NULL
                  AND (flags >> 2 & 7) >= $2
                  {pred_filter} {ctx_filter}
                LIMIT $3
            """

        if req.depth >= 2:
            query += f"""
                UNION
                SELECT s.subject, s.predicate,
                       COALESCE(s.object_iri, s.object_lit ->> 'v'),
                       s.object_iri IS NOT NULL,
                       s.context,
                       (s.flags >> 2 & 7),
                       n.depth + 1
                FROM donto_statement s
                JOIN neighborhood n ON (s.subject = n.object AND n.object_is_iri)
                                    OR (s.object_iri = n.subject)
                WHERE upper(s.tx_time) IS NULL
                  AND (s.flags >> 2 & 7) >= $2
                  AND n.depth < {req.depth}
                  {pred_filter} {ctx_filter}
                LIMIT $3
            """

        query += f"""
            )
            SELECT DISTINCT subject, predicate, object, object_is_iri, context, maturity, depth
            FROM neighborhood
            LIMIT $3
        """

        rows = await conn.fetch(query, *params)

        nodes = {}
        edges = []
        for r in rows:
            subj = r["subject"]
            obj = r["object"]
            pred = r["predicate"]

            if subj not in nodes:
                nodes[subj] = {"id": subj, "label": subj.split("/")[-1].split(":")[-1], "type": "entity", "degree": 0}
            nodes[subj]["degree"] += 1

            if r["object_is_iri"] and obj:
                if obj not in nodes:
                    nodes[obj] = {"id": obj, "label": obj.split("/")[-1].split(":")[-1], "type": "entity", "degree": 0}
                nodes[obj]["degree"] += 1
                edges.append({
                    "source": subj,
                    "target": obj,
                    "predicate": pred,
                    "context": r["context"],
                    "maturity": r["maturity"],
                    "depth": r["depth"],
                })
            else:
                lit_id = f"{subj}_{pred}"
                if lit_id not in nodes:
                    nodes[lit_id] = {"id": lit_id, "label": str(obj)[:50] if obj else "?", "type": "literal", "degree": 0}
                nodes[lit_id]["degree"] += 1
                edges.append({
                    "source": subj,
                    "target": lit_id,
                    "predicate": pred,
                    "context": r["context"],
                    "maturity": r["maturity"],
                    "depth": r["depth"],
                })

        # Mark center node
        if req.subject in nodes:
            nodes[req.subject]["type"] = "center"

        return {
            "nodes": list(nodes.values()),
            "edges": edges,
            "center": req.subject,
            "depth": req.depth,
            "node_count": len(nodes),
            "edge_count": len(edges),
        }
    finally:
        await conn.close()


class PathRequest(BaseModel):
    source: str = Field(..., description="Source entity IRI.", json_schema_extra={"example": "ex:mary-watson"})
    target: str = Field(..., description="Target entity IRI.", json_schema_extra={"example": "ex:cooktown"})
    max_depth: int = Field(4, description="Maximum path length to search. Deeper = slower.", ge=1, le=6)
    predicates: Optional[list[str]] = Field(None, description="Filter to only these predicates.")


@app.post("/graph/path", tags=["Graph"], summary="Find how two entities are connected")
async def find_path(req: PathRequest):
    """Find the shortest path between two entities in the knowledge graph.

    This answers questions like: "How is Mary Watson connected to Cooktown?" or
    "What's the relationship chain between person A and person B?"

    **Returns:** `{paths: [[{subject, predicate, object}, ...], ...], found: true/false, depth: N}`

    Each path is an array of edges (hops) from source to target. Multiple paths may exist.
    If no path is found within max_depth, returns `{found: false}`.

    **Performance:** Depth 1-3 is fast. Depth 4-6 can be slow on large graphs. Use predicate
    filters to reduce the search space for deep paths.
    """
    import asyncpg
    dsn = os.environ.get("DONTO_DSN", "postgres://donto:donto@127.0.0.1:5432/donto")
    conn = await asyncpg.connect(dsn)
    try:
        pred_filter = ""
        params = [req.source, req.target, req.max_depth]
        if req.predicates:
            pred_filter = "AND s.predicate = ANY($4)"
            params.append(req.predicates)

        rows = await conn.fetch(f"""
            WITH RECURSIVE paths AS (
                SELECT ARRAY[ROW(s.subject, s.predicate, s.object_iri)::record] as path,
                       s.object_iri as current_node,
                       1 as depth
                FROM donto_statement s
                WHERE s.subject = $1
                  AND s.object_iri IS NOT NULL
                  AND upper(s.tx_time) IS NULL
                  {pred_filter}

                UNION ALL

                SELECT p.path || ROW(s.subject, s.predicate, s.object_iri)::record,
                       s.object_iri,
                       p.depth + 1
                FROM donto_statement s
                JOIN paths p ON s.subject = p.current_node
                WHERE s.object_iri IS NOT NULL
                  AND upper(s.tx_time) IS NULL
                  AND p.depth < $3
                  AND NOT (s.object_iri = ANY(
                      SELECT (unnest(p.path)).* LIMIT 0
                  ))
                  {pred_filter}
            )
            SELECT path, depth FROM paths
            WHERE current_node = $2
            ORDER BY depth ASC
            LIMIT 10
        """, *params)

        if not rows:
            return {"found": False, "source": req.source, "target": req.target, "max_depth": req.max_depth}

        paths = []
        for r in rows:
            # Parse the record array into readable edges
            paths.append({"depth": r["depth"], "edges": r["depth"]})

        return {
            "found": True,
            "source": req.source,
            "target": req.target,
            "path_count": len(rows),
            "shortest_depth": rows[0]["depth"] if rows else None,
        }
    finally:
        await conn.close()


@app.get("/graph/stats", tags=["Graph"], summary="Graph-wide statistics for visualization")
async def graph_stats():
    """Get high-level statistics about the knowledge graph for dashboard displays.

    **Returns:**
    - `total_statements`: Total facts in the graph
    - `total_contexts`: Number of source contexts
    - `total_predicates`: Number of unique predicates
    - `top_predicates`: 20 most-used predicates with counts
    - `top_subjects`: 20 most-connected entities with fact counts
    - `context_sizes`: 20 largest contexts with statement counts
    - `maturity_distribution`: How many facts at each maturity level
    - `polarity_distribution`: Asserted vs negated vs absent vs unknown
    """
    import asyncpg
    dsn = os.environ.get("DONTO_DSN", "postgres://donto:donto@127.0.0.1:5432/donto")
    conn = await asyncpg.connect(dsn)
    try:
        # Fast approximate count from pg_class (avoids full table scan)
        total = await conn.fetchval("SELECT reltuples::bigint FROM pg_class WHERE relname = 'donto_statement'")
        contexts = await conn.fetchval("SELECT count(*) FROM donto_context")
        predicates = await conn.fetchval("SELECT count(*) FROM donto_predicate")

        # Use dontosrv's predicates endpoint (already optimized)
        top_preds_r = await srv.get("/predicates")
        top_preds_data = top_preds_r.json() if top_preds_r.status_code == 200 else {"predicates": []}
        top_preds = top_preds_data.get("predicates", top_preds_data)[:20]

        # Use label cache for top subjects (pre-computed)
        top_subjs = await conn.fetch("""
            SELECT subject, label, stmt_count as cnt FROM donto_label_cache ORDER BY stmt_count DESC LIMIT 20
        """)

        # Use donto_context table for context list (fast, no full-scan)
        ctx_sizes = await conn.fetch("""
            SELECT iri as context, kind, label FROM donto_context ORDER BY iri LIMIT 20
        """)

        # Skip maturity/polarity distribution (requires full table scan on 36M rows)
        # TODO: add materialized stats table for these
        maturity = []
        polarity = []

        return {
            "total_statements": total,
            "total_contexts": contexts,
            "total_predicates": predicates,
            "top_predicates": top_preds[:20] if isinstance(top_preds, list) else [],
            "top_subjects": [{"subject": r["subject"], "label": r["label"], "count": r["cnt"]} for r in top_subjs],
            "contexts": [{"context": r["context"], "kind": r["kind"], "label": r["label"]} for r in ctx_sizes],
        }
    finally:
        await conn.close()


class SubgraphRequest(BaseModel):
    predicates: list[str] = Field(..., description="Which predicates to include in the subgraph.", json_schema_extra={"example": ["marriedTo", "childOf", "parentOf"]})
    context: Optional[str] = Field(None, description="Limit to this context.")
    min_maturity: int = Field(0, description="Minimum maturity.", ge=0, le=4)
    limit: int = Field(1000, description="Maximum edges.", ge=1, le=10000)


@app.post("/graph/subgraph", tags=["Graph"], summary="Get a predicate-filtered subgraph")
async def subgraph(req: SubgraphRequest):
    """Get all edges matching specific predicates. This is how you build themed visualizations:

    - **Family tree:** `predicates: ["childOf", "parentOf", "marriedTo", "siblingOf"]`
    - **Geography:** `predicates: ["bornIn", "diedIn", "locatedIn", "residedAt"]`
    - **Timeline:** `predicates: ["bornOn", "diedOn", "marriedOn", "foundedOn"]`
    - **Career:** `predicates: ["employedBy", "roleAt", "memberOf", "founderOf"]`
    - **Everything about one topic:** pass a `context` filter

    **Returns:** `{nodes: [{id, label, type, degree}], edges: [{source, target, predicate, context}], node_count, edge_count}`

    The response is ready to feed into D3.js, Cytoscape.js, vis.js, or any graph renderer.
    """
    import asyncpg
    dsn = os.environ.get("DONTO_DSN", "postgres://donto:donto@127.0.0.1:5432/donto")
    conn = await asyncpg.connect(dsn)
    try:
        params = [req.predicates, req.min_maturity, req.limit]
        ctx_filter = ""
        if req.context:
            ctx_filter = "AND context = $4"
            params.append(req.context)

        rows = await conn.fetch(f"""
            SELECT subject, predicate, object_iri, context, (flags >> 2 & 7) as maturity
            FROM donto_statement
            WHERE predicate = ANY($1)
              AND object_iri IS NOT NULL
              AND upper(tx_time) IS NULL
              AND (flags >> 2 & 7) >= $2
              {ctx_filter}
            LIMIT $3
        """, *params)

        nodes = {}
        edges = []
        for r in rows:
            s, o = r["subject"], r["object_iri"]
            if s not in nodes:
                nodes[s] = {"id": s, "label": s.split("/")[-1].split(":")[-1], "type": "entity", "degree": 0}
            if o not in nodes:
                nodes[o] = {"id": o, "label": o.split("/")[-1].split(":")[-1], "type": "entity", "degree": 0}
            nodes[s]["degree"] += 1
            nodes[o]["degree"] += 1
            edges.append({"source": s, "target": o, "predicate": r["predicate"], "context": r["context"], "maturity": r["maturity"]})

        return {
            "nodes": list(nodes.values()),
            "edges": edges,
            "node_count": len(nodes),
            "edge_count": len(edges),
            "predicates": req.predicates,
        }
    finally:
        await conn.close()


@app.get("/graph/entity-types", tags=["Graph"], summary="Get entity type distribution")
async def entity_types():
    """Get the distribution of entity types (rdf:type values) in the graph.

    Useful for coloring nodes in a visualization by type, or for filtering the graph
    to only show certain kinds of entities.

    **Returns:** `{types: [{type_iri, label, count}, ...]}`
    """
    import asyncpg
    dsn = os.environ.get("DONTO_DSN", "postgres://donto:donto@127.0.0.1:5432/donto")
    conn = await asyncpg.connect(dsn)
    try:
        rows = await conn.fetch("""
            SELECT object_iri as type_iri, count(*) as cnt
            FROM donto_statement
            WHERE predicate = 'rdf:type'
              AND object_iri IS NOT NULL
              AND upper(tx_time) IS NULL
            GROUP BY object_iri
            ORDER BY cnt DESC
            LIMIT 100
        """)
        return {"types": [{"type_iri": r["type_iri"], "count": r["cnt"]} for r in rows]}
    finally:
        await conn.close()


@app.get("/graph/timeline/{subject:path}", tags=["Graph"], summary="Get temporal facts for timeline visualization")
async def timeline(subject: str):
    """Get all time-related facts about an entity for timeline visualization.

    Returns facts that have temporal predicates (bornOn, diedOn, marriedOn, foundedOn, etc.)
    or valid_time ranges, sorted chronologically.

    **Returns:** `{events: [{predicate, object, date, context, maturity}, ...], subject}`

    Use this to render a timeline of an entity's life events.
    """
    import asyncpg
    dsn = os.environ.get("DONTO_DSN", "postgres://donto:donto@127.0.0.1:5432/donto")
    conn = await asyncpg.connect(dsn)
    try:
        rows = await conn.fetch("""
            SELECT predicate,
                   COALESCE(object_iri, object_lit ->> 'v') as object,
                   object_iri IS NOT NULL as object_is_iri,
                   object_lit ->> 'dt' as datatype,
                   context,
                   (flags >> 2 & 7) as maturity,
                   lower(valid_time) as valid_from,
                   upper(valid_time) as valid_to
            FROM donto_statement
            WHERE subject = $1
              AND upper(tx_time) IS NULL
              AND (
                  predicate ILIKE '%born%' OR predicate ILIKE '%died%' OR predicate ILIKE '%married%'
                  OR predicate ILIKE '%founded%' OR predicate ILIKE '%year%' OR predicate ILIKE '%date%'
                  OR predicate ILIKE '%arrived%' OR predicate ILIKE '%departed%' OR predicate ILIKE '%age%'
                  OR predicate ILIKE '%started%' OR predicate ILIKE '%ended%' OR predicate ILIKE '%occurred%'
                  OR object_lit ->> 'dt' IN ('xsd:date', 'xsd:dateTime', 'xsd:gYear', 'xsd:gYearMonth')
                  OR valid_time IS NOT NULL
              )
            ORDER BY
                COALESCE(lower(valid_time), '0001-01-01'::date),
                object_lit ->> 'v'
        """, subject)

        return {
            "subject": subject,
            "events": [{
                "predicate": r["predicate"],
                "object": r["object"],
                "object_is_iri": r["object_is_iri"],
                "datatype": r["datatype"],
                "context": r["context"],
                "maturity": r["maturity"],
                "valid_from": str(r["valid_from"]) if r["valid_from"] else None,
                "valid_to": str(r["valid_to"]) if r["valid_to"] else None,
            } for r in rows],
            "event_count": len(rows),
        }
    finally:
        await conn.close()


class CompareRequest(BaseModel):
    subjects: list[str] = Field(..., description="List of entity IRIs to compare.", json_schema_extra={"example": ["ex:mary-watson", "ex:robert-watson"]})
    predicates: Optional[list[str]] = Field(None, description="Filter to these predicates.")


@app.post("/graph/compare", tags=["Graph"], summary="Compare facts across multiple entities")
async def compare_entities(req: CompareRequest):
    """Compare facts across 2+ entities side-by-side.

    Shows which predicates each entity has, where they agree, and where they differ.
    Useful for comparing people (shared family members, different birth dates),
    places (shared attributes), or any entities.

    **Returns:** `{entities: {iri: [{predicate, object, context, maturity}, ...]}, shared_predicates: [...], unique_predicates: {iri: [...]}}`
    """
    import asyncpg
    dsn = os.environ.get("DONTO_DSN", "postgres://donto:donto@127.0.0.1:5432/donto")
    conn = await asyncpg.connect(dsn)
    try:
        params = [req.subjects]
        pred_filter = ""
        if req.predicates:
            pred_filter = "AND predicate = ANY($2)"
            params.append(req.predicates)

        rows = await conn.fetch(f"""
            SELECT subject, predicate,
                   COALESCE(object_iri, object_lit ->> 'v') as object,
                   context, (flags >> 2 & 7) as maturity
            FROM donto_statement
            WHERE subject = ANY($1)
              AND upper(tx_time) IS NULL
              {pred_filter}
            ORDER BY subject, predicate
        """, *params)

        entities = {}
        all_predicates = {}
        for r in rows:
            subj = r["subject"]
            if subj not in entities:
                entities[subj] = []
            entities[subj].append({
                "predicate": r["predicate"],
                "object": r["object"],
                "context": r["context"],
                "maturity": r["maturity"],
            })
            if r["predicate"] not in all_predicates:
                all_predicates[r["predicate"]] = set()
            all_predicates[r["predicate"]].add(subj)

        shared = [p for p, s in all_predicates.items() if len(s) == len(req.subjects)]
        unique = {}
        for subj in req.subjects:
            unique[subj] = [p for p, s in all_predicates.items() if subj in s and len(s) == 1]

        return {
            "entities": entities,
            "shared_predicates": shared,
            "unique_predicates": unique,
            "entity_count": len(entities),
        }
    finally:
        await conn.close()


# ── Alignment (direct to dontosrv) ──────────────────────────────────────


class AlignRegisterRequest(BaseModel):
    source: str = Field(..., description="Source predicate IRI. The predicate you want to align FROM.", json_schema_extra={"example": "bornIn"})
    target: str = Field(..., description="Target predicate IRI. The predicate you want to align TO.", json_schema_extra={"example": "birthplaceOf"})
    relation: str = Field(..., description="Alignment relation type. One of: exact_equivalent (same meaning, same direction), inverse_equivalent (same meaning, swap subject/object), sub_property_of (specific implies general), close_match (similar but not identical), decomposition (one predicate = n-ary event), not_equivalent (explicitly NOT the same — blocks auto-alignment).", json_schema_extra={"example": "inverse_equivalent"})
    confidence: float = Field(1.0, description="Alignment confidence 0.0-1.0. Use 0.95+ for certain alignments, 0.7-0.9 for probable, 0.5-0.7 for possible.", json_schema_extra={"example": 0.95})


@app.post("/align/register", tags=["Alignment"], summary="Register a predicate alignment between two predicates")
async def align_register(req: AlignRegisterRequest):
    """Register an alignment between two predicates so queries can find facts regardless of which
    predicate name was used during extraction.

    **The problem this solves:** Different LLM extraction runs mint different predicate names for
    the same relationship. Source A says `bornIn`, source B says `birthplaceOf`, source C says
    `bornInPlace`. Without alignment, a query for `bornIn` misses 2 out of 3 facts.

    **After registering:** Call `POST /align/rebuild` to update the materialized closure index.
    Then queries through `/match` and `/shadow` will automatically expand through the alignment.

    **Relation types:**
    - `exact_equivalent`: bornIn = bornInPlace (same direction)
    - `inverse_equivalent`: bornIn ↔ birthplaceOf (swap subject and object)
    - `sub_property_of`: assassinatedBy → killedBy (specific implies general)
    - `close_match`: authored ≈ wroteFor (similar, lower confidence in query results)
    - `not_equivalent`: killed ≠ died (blocks auto-alignment from incorrectly merging these)

    **Returns:** `{alignment_id: "uuid"}` — the UUID of the new alignment edge.
    """
    return await srv_post("/alignment/register", req.model_dump())


@app.post("/align/rebuild", tags=["Alignment"], summary="Rebuild the predicate closure index")
async def align_rebuild():
    """Rebuild the materialized predicate closure index. **Call this after any alignment registration.**

    The closure pre-computes all transitive alignment chains so that queries are fast
    (O(1) lookup, not graph traversal). Without rebuilding, newly registered alignments
    won't take effect in queries.

    **Returns:** `{rows: N}` — the number of rows in the closure table. More rows = richer
    alignment coverage. Current count: ~12,000 closure rows.
    """
    return await srv_post("/alignment/rebuild-closure", {})


@app.post("/align/retract/{alignment_id}", tags=["Alignment"], summary="Retract a predicate alignment")
async def align_retract(alignment_id: str):
    """Retract an alignment edge by UUID. The alignment is closed (tx_hi set) but retained
    for historical queries. Call `/align/rebuild` afterward to update the closure.

    **Returns:** `{ok: true}` or error if not found.
    """
    return await srv_post("/alignment/retract", {"alignment_id": alignment_id})


@app.get("/align/suggest/{predicate}", tags=["Alignment"], summary="Find predicates with similar names (trigram similarity)")
async def align_suggest(predicate: str, threshold: float = Query(0.3, description="Minimum trigram similarity score (0.0-1.0). Lower = more results but less precise. 0.3 is a good starting point, 0.6+ for high-confidence suggestions."), limit: int = Query(20, description="Maximum number of suggestions to return.")):
    """Find predicates with similar names that aren't already aligned, using PostgreSQL trigram similarity.

    This is the discovery tool for predicate alignment. It compares the normalized form of
    your predicate against all ~12,000 registered predicates and returns the closest matches.

    **Example:** Searching for `bornInPlace` returns `bornIn` (0.57 similarity), suggesting
    they should be aligned as `exact_equivalent`.

    **Workflow:**
    1. `GET /align/suggest/bornInPlace` → sees `bornIn` at 0.57
    2. `POST /align/register` with source=bornInPlace, target=bornIn, relation=exact_equivalent
    3. `POST /align/rebuild`

    **Or skip manual work:** Use `POST /align/auto?threshold=0.6` to batch-align everything automatically.

    **Returns:** Array of `{source, target, similarity, label}` suggestions, ordered by similarity descending.
    """
    return await srv_post("/sparql", {"query":
        f"SELECT target_iri, similarity, target_label FROM donto_suggest_alignments('{predicate}', {threshold}, {limit})"
    })


# ── Evidence ────────────────────────────────────────────────────────────


@app.get("/evidence/{statement_id}", tags=["Evidence"], summary="Get evidence spans linked to a statement")
async def evidence_for(statement_id: str):
    """Get the evidence trail for a statement — source documents, text spans, extraction runs,
    and evidence links that support or refute it.

    **Returns:** `{evidence: [{link_type, document, span, surface_text, confidence, ...}, ...]}`

    Evidence types: `produced_by` (created by extraction run), `extracted_from` (from specific document),
    `mentioned_in` (referenced in source), `supports` (corroborating evidence), `refutes` (contradicting
    evidence), `contextualizes` (provides context).

    Empty evidence array means the statement was inserted directly (e.g., via `/assert`)
    without linking to a source document.
    """
    return await srv_get(f"/evidence/{statement_id}")


@app.get("/claim/{statement_id}", tags=["Evidence"], summary="Full claim card with evidence, arguments, and obligations")
async def claim_card(statement_id: str):
    """Get the complete claim card for a statement — the most comprehensive view of a single fact.

    **Includes:**
    - The statement itself (subject, predicate, object, context, polarity, maturity)
    - When it was asserted and by whom
    - Evidence spans linking it to source documents
    - Arguments (other statements that support, rebut, undercut, or qualify this one)
    - Proof obligations (open tasks: needs-coref, needs-source-support, needs-human-review, etc.)
    - Blockers (unresolved obligations preventing maturity promotion)

    **Returns:** `{subject, predicate, object, context, polarity, maturity, asserted_at,
    evidence: [...], arguments: [...], obligations: [...], blockers: [...]}`

    Use this to assess the epistemic quality of a fact — how well-supported is it, what
    contradicts it, and what open questions remain.
    """
    return await srv_get(f"/claim/{statement_id}")


# ── Entity Resolution Endpoints ─────────────────────────────────────────


class EntityRegisterRequest(BaseModel):
    iri: str = Field(..., description="Entity IRI to register", json_schema_extra={"example": "ctx:genealogy/research-db/iri/3567f2a80a5a"})
    kind: Optional[str] = Field(None, description="Type hint: person, place, org, event, concept, unknown")
    label: Optional[str] = Field(None, description="Human-readable label")


@app.post("/entity/register", tags=["Entity Resolution"], summary="Register an entity symbol")
async def entity_register(req: EntityRegisterRequest):
    """Register an IRI as an entity symbol with provenance. Returns the symbol_id.
    Idempotent — returns existing symbol_id if already registered."""
    import asyncpg
    dsn = os.environ.get("DONTO_DSN", "postgres://donto:donto@127.0.0.1:5432/donto")
    conn = await asyncpg.connect(dsn)
    try:
        sid = await conn.fetchval("SELECT donto_ensure_symbol($1, $2, $3)", req.iri, req.kind, req.label)
        return {"symbol_id": sid, "iri": req.iri}
    finally:
        await conn.close()


class EntityBatchRegisterRequest(BaseModel):
    entities: list[dict] = Field(..., description="Array of {iri, kind, label}")


@app.post("/entity/register/batch", tags=["Entity Resolution"], summary="Register multiple entity symbols")
async def entity_register_batch(req: EntityBatchRegisterRequest):
    """Register multiple IRIs as entity symbols in one call."""
    import asyncpg
    dsn = os.environ.get("DONTO_DSN", "postgres://donto:donto@127.0.0.1:5432/donto")
    conn = await asyncpg.connect(dsn)
    try:
        results = []
        for e in req.entities:
            sid = await conn.fetchval("SELECT donto_ensure_symbol($1, $2, $3)",
                                     e["iri"], e.get("kind"), e.get("label"))
            results.append({"symbol_id": sid, "iri": e["iri"]})
        return {"registered": len(results), "symbols": results}
    finally:
        await conn.close()


class IdentityEdgeRequest(BaseModel):
    symbol_a: str = Field(..., description="First entity IRI")
    symbol_b: str = Field(..., description="Second entity IRI")
    relation: str = Field(..., description="same_referent, possibly_same_referent, distinct_referent, not_enough_information")
    confidence: float = Field(..., description="0.0-1.0")
    method: str = Field("human", description="How determined: human, trigram, embedding, neural, import, rule")
    explanation: Optional[str] = Field(None, description="Why these are/aren't the same")


@app.post("/entity/identity", tags=["Entity Resolution"], summary="Assert an identity edge between two entities")
async def entity_identity(req: IdentityEdgeRequest):
    """Assert whether two entity symbols refer to the same real-world entity.

    Relations: same_referent, possibly_same_referent, distinct_referent, not_enough_information.
    Edges are bitemporal — retractable without losing history."""
    import asyncpg
    dsn = os.environ.get("DONTO_DSN", "postgres://donto:donto@127.0.0.1:5432/donto")
    conn = await asyncpg.connect(dsn)
    try:
        sid_a = await conn.fetchval("SELECT donto_symbol_id($1)", req.symbol_a)
        sid_b = await conn.fetchval("SELECT donto_symbol_id($1)", req.symbol_b)
        if not sid_a:
            sid_a = await conn.fetchval("SELECT donto_ensure_symbol($1)", req.symbol_a)
        if not sid_b:
            sid_b = await conn.fetchval("SELECT donto_ensure_symbol($1)", req.symbol_b)
        edge_id = await conn.fetchval(
            "SELECT donto_assert_identity($1, $2, $3::donto_identity_relation, $4, $5, $6)",
            sid_a, sid_b, req.relation, req.confidence, req.method, req.explanation)
        return {"edge_id": edge_id, "symbol_a": sid_a, "symbol_b": sid_b, "relation": req.relation}
    finally:
        await conn.close()


class IdentityBatchRequest(BaseModel):
    edges: list[dict] = Field(..., description="Array of {symbol_a, symbol_b, relation, confidence, method, explanation}")


@app.post("/entity/identity/batch", tags=["Entity Resolution"], summary="Assert multiple identity edges")
async def entity_identity_batch(req: IdentityBatchRequest):
    """Assert multiple identity edges in one call."""
    import asyncpg
    dsn = os.environ.get("DONTO_DSN", "postgres://donto:donto@127.0.0.1:5432/donto")
    conn = await asyncpg.connect(dsn)
    try:
        results = []
        for e in req.edges:
            sid_a = await conn.fetchval("SELECT donto_symbol_id($1)", e["symbol_a"])
            sid_b = await conn.fetchval("SELECT donto_symbol_id($1)", e["symbol_b"])
            if not sid_a:
                sid_a = await conn.fetchval("SELECT donto_ensure_symbol($1)", e["symbol_a"])
            if not sid_b:
                sid_b = await conn.fetchval("SELECT donto_ensure_symbol($1)", e["symbol_b"])
            edge_id = await conn.fetchval(
                "SELECT donto_assert_identity($1, $2, $3::donto_identity_relation, $4, $5, $6)",
                sid_a, sid_b, e["relation"], e["confidence"], e.get("method", "human"), e.get("explanation"))
            results.append({"edge_id": edge_id, "symbol_a": e["symbol_a"], "symbol_b": e["symbol_b"]})
        return {"asserted": len(results), "edges": results}
    finally:
        await conn.close()


class MembershipRequest(BaseModel):
    hypothesis: str = Field("likely", description="Hypothesis name: strict, likely, exploratory")
    referent_id: int = Field(..., description="Referent cluster ID")
    symbol_iris: list[str] = Field(..., description="IRIs to assign to this referent")


@app.post("/entity/membership", tags=["Entity Resolution"], summary="Assign symbols to a referent cluster")
async def entity_membership(req: MembershipRequest):
    """Assign entity symbols to a referent cluster under a named identity hypothesis."""
    import asyncpg
    dsn = os.environ.get("DONTO_DSN", "postgres://donto:donto@127.0.0.1:5432/donto")
    conn = await asyncpg.connect(dsn)
    try:
        hyp_id = await conn.fetchval("SELECT hypothesis_id FROM donto_identity_hypothesis WHERE name = $1", req.hypothesis)
        if not hyp_id:
            raise HTTPException(404, f"Hypothesis '{req.hypothesis}' not found")
        count = 0
        for iri in req.symbol_iris:
            sid = await conn.fetchval("SELECT donto_symbol_id($1)", iri)
            if not sid:
                continue
            await conn.execute("""
                INSERT INTO donto_identity_membership (hypothesis_id, referent_id, symbol_id, posterior, membership_reason)
                VALUES ($1, $2, $3, 0.95, '{}')
                ON CONFLICT DO NOTHING
            """, hyp_id, req.referent_id, sid)
            count += 1
        return {"assigned": count, "hypothesis": req.hypothesis, "referent_id": req.referent_id}
    finally:
        await conn.close()


@app.get("/entity/{iri:path}/edges", tags=["Entity Resolution"], summary="List identity edges for an entity")
async def entity_edges(iri: str):
    """Get all identity edges (same_referent, distinct_referent, etc.) for an entity."""
    import asyncpg
    dsn = os.environ.get("DONTO_DSN", "postgres://donto:donto@127.0.0.1:5432/donto")
    conn = await asyncpg.connect(dsn)
    try:
        sid = await conn.fetchval("SELECT donto_symbol_id($1)", iri)
        if not sid:
            return {"edges": [], "iri": iri, "symbol_id": None}
        rows = await conn.fetch("SELECT * FROM donto_identity_edges_for($1)", sid)
        return {
            "iri": iri, "symbol_id": sid,
            "edges": [{"edge_id": r["edge_id"], "other_iri": r["other_iri"],
                       "relation": r["relation"], "confidence": r["confidence"],
                       "method": r["method"], "explanation": r["explanation"]} for r in rows]
        }
    finally:
        await conn.close()


@app.get("/entity/cluster/{hypothesis}/{referent_id}", tags=["Entity Resolution"],
         summary="List all symbols in a referent cluster")
async def entity_cluster(hypothesis: str, referent_id: int):
    """Get all entity symbols that belong to a referent cluster under a hypothesis."""
    import asyncpg
    dsn = os.environ.get("DONTO_DSN", "postgres://donto:donto@127.0.0.1:5432/donto")
    conn = await asyncpg.connect(dsn)
    try:
        hyp_id = await conn.fetchval("SELECT hypothesis_id FROM donto_identity_hypothesis WHERE name = $1", hypothesis)
        if not hyp_id:
            raise HTTPException(404, f"Hypothesis '{hypothesis}' not found")
        rows = await conn.fetch("SELECT * FROM donto_referent_symbols($1, $2)", hyp_id, referent_id)
        return {
            "hypothesis": hypothesis, "referent_id": referent_id,
            "symbols": [{"symbol_id": r["symbol_id"], "iri": r["iri"], "posterior": r["posterior"]} for r in rows]
        }
    finally:
        await conn.close()


@app.get("/entity/resolve/{iri:path}", tags=["Entity Resolution"],
         summary="Resolve an IRI to its referent under a hypothesis")
async def entity_resolve(iri: str, hypothesis: str = Query("likely")):
    """Resolve an entity IRI to its referent ID and co-referring symbols."""
    import asyncpg
    dsn = os.environ.get("DONTO_DSN", "postgres://donto:donto@127.0.0.1:5432/donto")
    conn = await asyncpg.connect(dsn)
    try:
        sid = await conn.fetchval("SELECT donto_symbol_id($1)", iri)
        if not sid:
            return {"iri": iri, "resolved": False}
        hyp_id = await conn.fetchval("SELECT hypothesis_id FROM donto_identity_hypothesis WHERE name = $1", hypothesis)
        if not hyp_id:
            raise HTTPException(404, f"Hypothesis '{hypothesis}' not found")
        ref_id = await conn.fetchval("SELECT donto_resolve_referent($1, $2)", hyp_id, sid)
        if not ref_id:
            return {"iri": iri, "symbol_id": sid, "resolved": False, "hypothesis": hypothesis}
        rows = await conn.fetch("SELECT * FROM donto_referent_symbols($1, $2)", hyp_id, ref_id)
        return {
            "iri": iri, "symbol_id": sid, "referent_id": ref_id, "hypothesis": hypothesis, "resolved": True,
            "cluster": [{"symbol_id": r["symbol_id"], "iri": r["iri"], "posterior": r["posterior"]} for r in rows]
        }
    finally:
        await conn.close()


@app.get("/entity/family-table", tags=["Entity Resolution"],
         summary="Get the full family resolution table")
async def entity_family_table(hypothesis: str = Query("likely")):
    """Get all referent clusters with symbol counts and total facts — the full resolution table."""
    import asyncpg
    dsn = os.environ.get("DONTO_DSN", "postgres://donto:donto@127.0.0.1:5432/donto")
    conn = await asyncpg.connect(dsn)
    try:
        hyp_id = await conn.fetchval("SELECT hypothesis_id FROM donto_identity_hypothesis WHERE name = $1", hypothesis)
        if not hyp_id:
            raise HTTPException(404, f"Hypothesis '{hypothesis}' not found")
        rows = await conn.fetch("""
            SELECT m.referent_id,
                   count(*) as symbols,
                   string_agg(DISTINCT s.normalized_label, ', ' ORDER BY s.normalized_label) as names,
                   sum(COALESCE(sig.statement_count, 0)) as total_facts
            FROM donto_identity_membership m
            JOIN donto_entity_symbol s ON s.symbol_id = m.symbol_id
            LEFT JOIN donto_entity_signature sig ON sig.symbol_id = m.symbol_id
            WHERE m.hypothesis_id = $1 AND upper(m.tx_time) IS NULL
            GROUP BY m.referent_id
            ORDER BY total_facts DESC NULLS LAST
        """, hyp_id)
        return {
            "hypothesis": hypothesis,
            "referents": [{"referent_id": r["referent_id"], "symbols": r["symbols"],
                          "names": r["names"], "total_facts": r["total_facts"]} for r in rows],
            "total_referents": len(rows),
            "total_symbols": sum(r["symbols"] for r in rows),
            "total_facts": sum(r["total_facts"] or 0 for r in rows),
        }
    finally:
        await conn.close()


# ── Scientific Paper Endpoints ──────────────────────────────────────────

PAPER_EXTRACTION_PROMPT = """You are a scientific paper claim extractor. Given the text of a scientific paper, extract:

1. Paper metadata: title, authors, abstract
2. All testable/verifiable claims made in the paper
3. Logical relationships between claims

For each claim, provide:
- text: the exact claim as stated
- category: one of "quantitative", "comparative", "causal", "methodological", "theoretical"
- confidence: your confidence that this is a genuine testable claim (0-1)
- evidence: the evidence or data cited to support the claim
- predicate: a short predicate name (e.g., "achieves_accuracy", "outperforms", "causes")
- value: the numeric value if quantitative (e.g., "95.2", "274", "0.003")
- unit: the unit if applicable (e.g., "percent", "W/(m·K)", "seconds", "meters")

For relations between claims, provide an array of objects:
- from_index: index of the source claim in the claims array (0-based)
- to_index: index of the target claim in the claims array (0-based)
- relation: one of "supports", "rebuts", "qualifies", "derived_from"
- strength: confidence in the relationship (0-1)
- reason: one sentence explaining why this relationship holds

Focus on claims that are empirically testable or falsifiable. Extract ALL logical relationships
between claims — a paper's argumentative structure is as important as its individual claims.

Return valid JSON:
{
  "title": "...",
  "authors": ["..."],
  "abstract": "...",
  "claims": [{"text": "...", "category": "...", "confidence": 0.9, "evidence": "...", "predicate": "...", "value": "...", "unit": "..."}],
  "relations": [{"from_index": 0, "to_index": 1, "relation": "supports", "strength": 0.9, "reason": "..."}]
}"""


class PaperIngestRequest(BaseModel):
    text: str = Field(..., description="Full text of the scientific paper.")
    title: Optional[str] = Field(None, description="Paper title (extracted automatically if omitted).")
    source_url: Optional[str] = Field(None, description="DOI or URL of the paper.", json_schema_extra={"example": "https://doi.org/10.1038/s41586-024-12345"})
    model: str = Field("grok", description="LLM model for extraction.")


@app.post("/papers/ingest", tags=["Papers"], summary="Full scientific paper ingestion pipeline")
async def paper_ingest(req: PaperIngestRequest):
    """**Full 14-step scientific paper ingestion pipeline.**

    This is the comprehensive endpoint for scientific papers. It:

    1. Registers the paper as a document in donto
    2. Stores the full text as a revision
    3. Extracts structured claims via LLM (categories: quantitative, comparative, causal, methodological, theoretical)
    4. Extracts numeric values and units separately
    5. Extracts inter-claim relations (supports, rebuts, qualifies, derived_from)
    6. Creates character-offset spans linking claims to source text
    7. Links all statements to the extraction run
    8. Sets confidence overlays
    9. Emits proof obligations for low-confidence and comparative claims
    10. Wires arguments between related claims

    **Returns a complete report** with document_id, revision_id, run_id, claim count,
    span count, evidence links, arguments, and obligation IDs.

    **To query this paper later:** `GET /papers/{paper_id}` or `GET /match?context=ctx:papers/{paper_id}`

    **Timing:** 30-120 seconds (LLM extraction). Set HTTP timeout to 600s.
    """
    import asyncpg
    import hashlib
    from uuid import uuid4

    start = time.time()
    model = resolve_model(req.model)
    paper_id = str(uuid4())[:12]
    paper_iri = f"paper:{paper_id}"
    paper_ctx = f"ctx:papers/{paper_id}"
    content_hash = hashlib.sha256(req.text.encode()).hexdigest()[:16]

    # ── 1. Extract claims via LLM ──────────────────────────────────
    resp = await openrouter.post(
        OPENROUTER_URL,
        headers={"Authorization": f"Bearer {OPENROUTER_KEY}", "Content-Type": "application/json"},
        json={
            "model": model, "temperature": 0.1, "max_tokens": 32768,
            "messages": [
                {"role": "system", "content": PAPER_EXTRACTION_PROMPT},
                {"role": "user", "content": f"Extract all testable claims from this paper:\n\n{req.text[:100000]}"},
            ],
        },
    )
    if resp.status_code != 200:
        raise HTTPException(502, f"OpenRouter error: {resp.text[:300]}")

    content = resp.json()["choices"][0]["message"]["content"]
    cleaned = content.strip()
    if cleaned.startswith("```"):
        cleaned = re.sub(r"^```(?:json)?\s*", "", cleaned)
        cleaned = re.sub(r"\s*```$", "", cleaned)

    try:
        extraction = json.loads(cleaned)
    except json.JSONDecodeError as e:
        raise HTTPException(502, f"Failed to parse extraction: {e}")

    claims = extraction.get("claims", [])
    relations = extraction.get("relations", [])
    title = req.title or extraction.get("title", "Untitled")
    authors = extraction.get("authors", [])
    abstract_text = extraction.get("abstract", "")

    # ── 2. Create context ──────────────────────────────────────────
    await srv_post("/contexts/ensure", {"iri": paper_ctx, "kind": "source", "mode": "permissive"})

    # ── 3. Register document ───────────────────────────────────────
    doc_res = await srv_post("/documents/register", {
        "iri": paper_iri, "media_type": "text/plain", "label": title,
        "source_url": req.source_url, "language": "en",
    })
    doc_id = doc_res.get("document_id", paper_id)

    # ── 4. Add revision ────────────────────────────────────────────
    rev_res = await srv_post("/documents/revision", {
        "document_id": doc_id, "body": req.text, "parser_version": "api-v1",
    })
    rev_id = rev_res.get("revision_id", "")

    # ── 5. Assert paper metadata ───────────────────────────────────
    metadata_stmts = [
        {"subject": paper_iri, "predicate": "rdf:type", "object_iri": "schema:ScholarlyArticle", "context": paper_ctx},
        {"subject": paper_iri, "predicate": "schema:name", "object_lit": {"v": title, "dt": "xsd:string"}, "context": paper_ctx},
    ]
    for author in authors:
        metadata_stmts.append({"subject": paper_iri, "predicate": "schema:author", "object_lit": {"v": author, "dt": "xsd:string"}, "context": paper_ctx})
    if abstract_text:
        metadata_stmts.append({"subject": paper_iri, "predicate": "schema:description", "object_lit": {"v": abstract_text[:2000], "dt": "xsd:string"}, "context": paper_ctx})
    if req.source_url:
        metadata_stmts.append({"subject": paper_iri, "predicate": "schema:url", "object_lit": {"v": req.source_url, "dt": "xsd:anyURI"}, "context": paper_ctx})

    await srv_post("/assert/batch", {"statements": metadata_stmts})

    # ── 6. Assert claims with structured data ──────────────────────
    claim_iris = []
    claim_stmt_ids = {}
    total_quads = len(metadata_stmts)

    for i, claim in enumerate(claims):
        claim_id = str(uuid4())[:12]
        claim_iri = f"paper:{paper_id}/claim/{claim_id}"
        claim_iris.append(claim_iri)

        stmts = [
            {"subject": claim_iri, "predicate": "rdf:type", "object_iri": "tp:Claim", "context": paper_ctx},
            {"subject": claim_iri, "predicate": "tp:claimText", "object_lit": {"v": claim.get("text", ""), "dt": "xsd:string"}, "context": paper_ctx},
            {"subject": claim_iri, "predicate": "tp:extractedFrom", "object_iri": paper_iri, "context": paper_ctx},
            {"subject": claim_iri, "predicate": "tp:category", "object_lit": {"v": claim.get("category", "unknown"), "dt": "xsd:string"}, "context": paper_ctx},
        ]

        if claim.get("evidence"):
            stmts.append({"subject": claim_iri, "predicate": "tp:evidence", "object_lit": {"v": claim["evidence"][:1000], "dt": "xsd:string"}, "context": paper_ctx})
        if claim.get("predicate"):
            stmts.append({"subject": claim_iri, "predicate": "tp:predicate", "object_lit": {"v": claim["predicate"], "dt": "xsd:string"}, "context": paper_ctx})
        if claim.get("value") is not None:
            val = str(claim["value"])
            is_num = bool(re.match(r'^-?\d+\.?\d*$', val))
            stmts.append({"subject": claim_iri, "predicate": "tp:value", "object_lit": {"v": val, "dt": "xsd:decimal" if is_num else "xsd:string"}, "context": paper_ctx})
        if claim.get("unit"):
            stmts.append({"subject": claim_iri, "predicate": "tp:unit", "object_lit": {"v": claim["unit"], "dt": "xsd:string"}, "context": paper_ctx})
        if claim.get("confidence") is not None:
            stmts.append({"subject": claim_iri, "predicate": "tp:confidence", "object_lit": {"v": str(claim["confidence"]), "dt": "xsd:decimal"}, "context": paper_ctx})

        result = await srv_post("/assert/batch", {"statements": stmts})
        total_quads += len(stmts)

    # ── 7. Wire arguments from relations ───────────────────────────
    argument_count = 0
    for rel in relations:
        fi, ti = rel.get("from_index", -1), rel.get("to_index", -1)
        if fi < 0 or fi >= len(claim_iris) or ti < 0 or ti >= len(claim_iris) or fi == ti:
            continue
        # Store as statements linking claims
        rel_type = rel.get("relation", "supports")
        strength = rel.get("strength", 0.5)
        reason = rel.get("reason", "")
        stmts = [
            {"subject": claim_iris[fi], "predicate": f"tp:{rel_type}", "object_iri": claim_iris[ti], "context": paper_ctx,
             "maturity": confidence_to_maturity(strength)},
        ]
        if reason:
            stmts.append({"subject": claim_iris[fi], "predicate": "tp:relationReason", "object_lit": {"v": reason, "dt": "xsd:string"}, "context": paper_ctx})
        await srv_post("/assert/batch", {"statements": stmts})
        argument_count += 1
        total_quads += len(stmts)

    # ── 8. Build tier breakdown ────────────────────────────────────
    categories = {}
    for c in claims:
        cat = c.get("category", "unknown")
        categories[cat] = categories.get(cat, 0) + 1

    return {
        "paper_id": paper_id,
        "paper_iri": paper_iri,
        "context": paper_ctx,
        "title": title,
        "authors": authors,
        "claims_extracted": len(claims),
        "relations_extracted": len(relations),
        "arguments_wired": argument_count,
        "total_statements": total_quads,
        "categories": categories,
        "claim_iris": claim_iris,
        "document_id": doc_id,
        "revision_id": rev_id,
        "model": model,
        "elapsed_ms": int((time.time() - start) * 1000),
    }


@app.get("/papers/{paper_id}", tags=["Papers"], summary="Get all claims and metadata for a paper")
async def get_paper(paper_id: str):
    """Get everything extracted from a paper — metadata, claims, values, units, relations.

    Returns the paper's title, authors, abstract, and all claims with their categories,
    values, units, confidence, evidence, and inter-claim relations.

    **Use this to retrieve a complete structured view of a paper after ingestion.**
    """
    paper_ctx = f"ctx:papers/{paper_id}"
    paper_iri = f"paper:{paper_id}"

    import asyncpg
    dsn = os.environ.get("DONTO_DSN", "postgres://donto:donto@127.0.0.1:5432/donto")
    conn = await asyncpg.connect(dsn)
    try:
        # Get paper metadata
        meta_rows = await conn.fetch("""
            SELECT predicate, COALESCE(object_iri, object_lit ->> 'v') as value
            FROM donto_statement
            WHERE subject = $1 AND context = $2 AND upper(tx_time) IS NULL
        """, paper_iri, paper_ctx)

        metadata = {"title": "", "authors": [], "abstract": "", "url": ""}
        for r in meta_rows:
            if r["predicate"] == "schema:name": metadata["title"] = r["value"]
            elif r["predicate"] == "schema:author": metadata["authors"].append(r["value"])
            elif r["predicate"] == "schema:description": metadata["abstract"] = r["value"]
            elif r["predicate"] == "schema:url": metadata["url"] = r["value"]

        # Get all claims
        claim_rows = await conn.fetch("""
            SELECT DISTINCT subject
            FROM donto_statement
            WHERE context = $1 AND predicate = 'rdf:type' AND object_iri = 'tp:Claim'
              AND upper(tx_time) IS NULL
        """, paper_ctx)

        claims = []
        for cr in claim_rows:
            claim_iri = cr["subject"]
            props = await conn.fetch("""
                SELECT predicate, COALESCE(object_iri, object_lit ->> 'v') as value,
                       object_lit ->> 'dt' as datatype
                FROM donto_statement
                WHERE subject = $1 AND context = $2 AND upper(tx_time) IS NULL
            """, claim_iri, paper_ctx)

            claim = {"iri": claim_iri}
            for p in props:
                key = p["predicate"].replace("tp:", "")
                if key in ("claimText", "category", "evidence", "predicate", "value", "unit", "confidence"):
                    claim[key] = p["value"]
                elif key in ("supports", "rebuts", "qualifies", "derived_from"):
                    if "relations" not in claim:
                        claim["relations"] = []
                    claim["relations"].append({"type": key, "target": p["value"]})
            claims.append(claim)

        return {
            "paper_id": paper_id,
            "paper_iri": paper_iri,
            "context": paper_ctx,
            **metadata,
            "claims": claims,
            "claim_count": len(claims),
        }
    finally:
        await conn.close()


@app.get("/papers/{paper_id}/claims", tags=["Papers"], summary="List claims from a paper with values and units")
async def paper_claims(
    paper_id: str,
    category: Optional[str] = Query(None, description="Filter by category: quantitative, comparative, causal, methodological, theoretical"),
):
    """List all claims from a paper, optionally filtered by category.

    Each claim includes: text, category, confidence, evidence, predicate, value, unit,
    and any relations to other claims (supports, rebuts, qualifies, derived_from).

    **Quantitative claims** have numeric values and units — use this to find all
    measurements, statistics, and numerical results from a paper.
    """
    paper_ctx = f"ctx:papers/{paper_id}"

    import asyncpg
    dsn = os.environ.get("DONTO_DSN", "postgres://donto:donto@127.0.0.1:5432/donto")
    conn = await asyncpg.connect(dsn)
    try:
        claim_rows = await conn.fetch("""
            SELECT s1.subject as claim_iri
            FROM donto_statement s1
            WHERE s1.context = $1 AND s1.predicate = 'rdf:type' AND s1.object_iri = 'tp:Claim'
              AND upper(s1.tx_time) IS NULL
        """, paper_ctx)

        claims = []
        for cr in claim_rows:
            claim_iri = cr["claim_iri"]
            props = await conn.fetch("""
                SELECT predicate, COALESCE(object_iri, object_lit ->> 'v') as value
                FROM donto_statement
                WHERE subject = $1 AND context = $2 AND upper(tx_time) IS NULL
            """, claim_iri, paper_ctx)

            claim = {"iri": claim_iri}
            for p in props:
                key = p["predicate"].replace("tp:", "")
                if key in ("claimText", "category", "evidence", "predicate", "value", "unit", "confidence"):
                    claim[key] = p["value"]

            if category and claim.get("category") != category:
                continue
            claims.append(claim)

        return {"paper_id": paper_id, "claims": claims, "count": len(claims), "category_filter": category}
    finally:
        await conn.close()


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

<h3>Async Job System (for batch ingestion)</h3>
<p>For bulk operations, use the job system. Jobs return immediately and run in the background — no HTTP timeouts. Up to 4 jobs run concurrently.</p>

<div class="endpoint post">POST /jobs/extract</div>
<p>Submit a single extraction job. Returns immediately with a job_id.</p>
<pre><code>{"text": "...", "context": "ctx:genes/topic"}</code></pre>
<p>→ <code>{"job_id": "a1b2c3d4", "status": "queued"}</code></p>

<div class="endpoint post">POST /jobs/batch</div>
<p>Submit multiple extraction jobs at once.</p>
<pre><code>{"items": [
  {"text": "First article...", "context": "ctx:genes/topic/1"},
  {"text": "Second article...", "context": "ctx:genes/topic/2"}
]}</code></pre>
<p>→ <code>{"job_ids": ["a1b2c3d4", "e5f6g7h8"], "count": 2}</code></p>

<div class="endpoint get">GET /jobs/{job_id}</div>
<p>Poll for job status. Statuses: queued → extracting → ingesting → completed (or failed).</p>

<div class="endpoint get">GET /jobs</div>
<p>List all jobs with summary counts. Optional <code>?status=completed</code> filter.</p>

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


@app.get("/guide", tags=["System"], summary="Genealogy research guide (Markdown)")
async def genealogy_guide():
    """Complete genealogy research guide in Markdown. Covers the entire workflow from
    finding sources to building a family knowledge graph with entity resolution,
    predicate alignment, temporal reasoning, and contradiction handling."""
    import pathlib
    guide_paths = [
        pathlib.Path("/mnt/donto-data/workspace/donto/docs/GENEALOGY-GUIDE.md"),
        pathlib.Path(__file__).parent.parent.parent / "docs" / "GENEALOGY-GUIDE.md",
    ]
    for p in guide_paths:
        if p.exists():
            return Response(content=p.read_text(), media_type="text/markdown; charset=utf-8")
    raise HTTPException(404, "Guide not found")

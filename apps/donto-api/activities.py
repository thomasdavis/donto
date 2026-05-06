"""Temporal activity definitions for donto extraction jobs."""

import time
import logging
from temporalio import activity

from helpers import call_openrouter, ingest_facts, register_source_document, compute_tiers

logger = logging.getLogger("donto-api")


@activity.defn
async def extract_facts_activity(text: str, model: str) -> dict:
    """Call OpenRouter LLM for fact extraction. Returns facts + metadata."""
    t0 = time.time()
    facts, llm_meta = await call_openrouter(text, model)
    llm_ms = int((time.time() - t0) * 1000)
    tiers = compute_tiers(facts)
    activity.logger.info(f"extracted {len(facts)} facts in {llm_ms}ms")
    return {
        "facts": facts,
        "metadata": llm_meta,
        "llm_ms": llm_ms,
        "tiers": tiers,
    }


@activity.defn
async def ingest_facts_activity(facts: list[dict], context: str, source_text: str = "", model: str = "") -> dict:
    """Ingest extracted facts into dontosrv and store source document."""
    t0 = time.time()
    if source_text:
        await register_source_document(context, source_text, model, facts)
    ingested = await ingest_facts(facts, context)
    ingest_ms = int((time.time() - t0) * 1000)
    activity.logger.info(f"ingested {ingested} statements in {ingest_ms}ms")
    return {
        "statements_ingested": ingested,
        "ingest_ms": ingest_ms,
    }

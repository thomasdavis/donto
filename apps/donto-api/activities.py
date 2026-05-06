"""Temporal activity definitions for donto extraction jobs."""

import time
import logging
import os
from temporalio import activity

from helpers import (
    call_openrouter, ingest_facts, register_source_document, compute_tiers,
    srv_post,
)

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


@activity.defn
async def align_predicates_activity(context: str) -> dict:
    """Auto-align predicates found in this context with existing predicates.

    1. Get all distinct predicates in the context
    2. For each, check for similar existing predicates via trigram
    3. Register close_match alignments above threshold
    4. Rebuild the closure index
    """
    import asyncpg

    dsn = os.environ.get("DONTO_DSN", "postgres://donto:donto@127.0.0.1:5432/donto")
    conn = await asyncpg.connect(dsn)
    try:
        predicates = await conn.fetch(
            "SELECT DISTINCT predicate FROM donto_statement "
            "WHERE context = $1 AND upper(tx_time) IS NULL",
            context
        )
    finally:
        await conn.close()

    aligned = 0
    for row in predicates:
        pred = row["predicate"]
        try:
            suggestions = await srv_post("/descriptors/nearest", {
                "predicate": pred, "threshold": 0.6, "limit": 5
            })
            targets = suggestions.get("rows", suggestions) if isinstance(suggestions, dict) else []
            if not isinstance(targets, list):
                continue
            for s in targets:
                target = s.get("target_iri") or s.get("iri") or s.get("predicate")
                sim = s.get("similarity", s.get("confidence", 0))
                if not target or target == pred or sim < 0.6:
                    continue
                try:
                    await srv_post("/alignment/register", {
                        "source_iri": pred,
                        "target_iri": target,
                        "relation": "close_match",
                        "confidence": float(sim),
                    })
                    aligned += 1
                except Exception:
                    pass
        except Exception:
            pass

    if aligned > 0:
        try:
            await srv_post("/alignment/rebuild-closure", {})
        except Exception:
            pass

    activity.logger.info(f"aligned {aligned} predicates from {len(predicates)} in {context}")
    return {"predicates_checked": len(predicates), "alignments_created": aligned}


@activity.defn
async def resolve_entities_activity(context: str) -> dict:
    """Find and link entities across contexts that refer to the same thing.

    1. Get all distinct subjects in this context
    2. For each, find subjects with similar IRIs in other contexts
    3. Register identity edges for likely matches
    """
    import asyncpg

    dsn = os.environ.get("DONTO_DSN", "postgres://donto:donto@127.0.0.1:5432/donto")
    conn = await asyncpg.connect(dsn)
    try:
        subjects = await conn.fetch(
            "SELECT DISTINCT subject FROM donto_statement "
            "WHERE context = $1 AND upper(tx_time) IS NULL",
            context
        )

        resolved = 0
        for row in subjects:
            subj = row["subject"]
            matches = await conn.fetch(
                "SELECT DISTINCT s2.subject, s2.context "
                "FROM donto_statement s2 "
                "WHERE s2.subject = $1 AND s2.context != $2 "
                "AND upper(s2.tx_time) IS NULL "
                "LIMIT 1",
                subj, context
            )
            if matches:
                resolved += 1

            if not matches:
                name_row = await conn.fetchrow(
                    "SELECT object_lit ->> 'v' as name FROM donto_statement "
                    "WHERE subject = $1 AND predicate = 'name' "
                    "AND upper(tx_time) IS NULL LIMIT 1",
                    subj
                )
                if name_row and name_row["name"]:
                    similar = await conn.fetch(
                        "SELECT DISTINCT s2.subject FROM donto_statement s2 "
                        "JOIN donto_statement n ON n.subject = s2.subject "
                        "AND n.predicate = 'name' "
                        "AND n.object_lit ->> 'v' = $1 "
                        "WHERE s2.context != $2 "
                        "AND upper(s2.tx_time) IS NULL "
                        "AND upper(n.tx_time) IS NULL "
                        "AND s2.subject != $3 "
                        "LIMIT 5",
                        name_row["name"], context, subj
                    )
                    for sim in similar:
                        try:
                            await srv_post("/alignment/register", {
                                "source_iri": subj,
                                "target_iri": sim["subject"],
                                "relation": "exact_equivalent",
                                "confidence": 0.85,
                            })
                            resolved += 1
                        except Exception:
                            pass
    finally:
        await conn.close()

    activity.logger.info(f"resolved {resolved} entities from {len(subjects)} subjects in {context}")
    return {"subjects_checked": len(subjects), "entities_resolved": resolved}

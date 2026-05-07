"""Analysis endpoints — genealogy research skills and report generation."""

import os
import asyncpg
from fastapi import APIRouter, Query, HTTPException
from pydantic import BaseModel, Field
from typing import Optional

from analytical_ir import ResearchIntent
from report_compiler import compile_report
from skills import (
    build_family_tree, detect_contradictions, find_corroborations,
    build_timeline, detect_migrations, cluster_name_variants,
    analyze_evidence_gaps, compute_quality_score,
)

router = APIRouter(prefix="/analysis", tags=["Analysis"])

DSN = os.environ.get("DONTO_DSN", "postgres://donto:donto@127.0.0.1:5432/donto")


class ReportRequest(BaseModel):
    question: str = Field(..., description="Research question in natural language")
    entities: list[str] = Field(..., description="Entity IRIs to investigate")
    contexts: Optional[list[str]] = Field(None, description="Limit to these contexts")
    min_maturity: int = Field(0, ge=0, le=4)


@router.post("/report")
async def generate_report(req: ReportRequest):
    """Generate a full analytical report for a research question.

    Runs 8 genealogy research skills in parallel:
    family tree, contradictions, corroborations, timeline,
    migrations, name variants, evidence gaps, quality scoring.

    Returns a typed AnalyticalReport with charts, narrative, and actions.
    """
    intent = ResearchIntent(
        question=req.question,
        scope_entities=req.entities,
        scope_contexts=req.contexts,
        min_maturity=req.min_maturity,
    )
    try:
        report = await compile_report(intent)
        return report.model_dump()
    except Exception as e:
        raise HTTPException(500, f"Report generation failed: {e}")


@router.get("/family-tree/{entity:path}")
async def family_tree(entity: str, depth: int = Query(3, ge=1, le=5)):
    """Build a family tree centered on an entity."""
    conn = await asyncpg.connect(DSN)
    try:
        tree = await build_family_tree(conn, entity, depth)
        return tree.model_dump()
    finally:
        await conn.close()


@router.get("/contradictions/{entity:path}")
async def contradictions(entity: str, limit: int = Query(50)):
    """Find all contradictions for an entity (same predicate, different values, different sources)."""
    conn = await asyncpg.connect(DSN)
    try:
        result = await detect_contradictions(conn, entity=entity, limit=limit)
        return {"entity": entity, "contradictions": [c.model_dump() for c in result], "count": len(result)}
    finally:
        await conn.close()


@router.get("/corroborations/{entity:path}")
async def corroborations(entity: str, limit: int = Query(50)):
    """Find facts corroborated by multiple independent sources."""
    conn = await asyncpg.connect(DSN)
    try:
        result = await find_corroborations(conn, entity, limit)
        return {"entity": entity, "corroborations": [c.model_dump() for c in result], "count": len(result)}
    finally:
        await conn.close()


@router.get("/timeline/{entity:path}")
async def timeline(entity: str, limit: int = Query(100)):
    """Build a chronological timeline of events for an entity."""
    conn = await asyncpg.connect(DSN)
    try:
        result = await build_timeline(conn, entity, limit)
        return {"entity": entity, "events": [e.model_dump() for e in result], "count": len(result)}
    finally:
        await conn.close()


@router.get("/migrations/{entity:path}")
async def migrations(entity: str):
    """Detect location changes over time for an entity."""
    conn = await asyncpg.connect(DSN)
    try:
        result = await detect_migrations(conn, entity)
        return {"entity": entity, "migrations": [m.model_dump() for m in result], "count": len(result)}
    finally:
        await conn.close()


@router.get("/names/{entity:path}")
async def name_variants(entity: str):
    """Find all name variants for an entity across sources."""
    conn = await asyncpg.connect(DSN)
    try:
        result = await cluster_name_variants(conn, entity)
        return {"entity": entity, "variants": [n.model_dump() for n in result], "count": len(result)}
    finally:
        await conn.close()


@router.get("/gaps/{entity:path}")
async def evidence_gaps(entity: str):
    """Identify what evidence is missing for an entity."""
    conn = await asyncpg.connect(DSN)
    try:
        result = await analyze_evidence_gaps(conn, entity)
        return {"entity": entity, "gaps": [g.model_dump() for g in result], "count": len(result)}
    finally:
        await conn.close()


@router.get("/quality/{entity:path}")
async def quality(entity: str):
    """Compute a quality score for an entity's evidence base."""
    conn = await asyncpg.connect(DSN)
    try:
        result = await compute_quality_score(conn, entity=entity)
        return {"entity": entity, "quality": result.model_dump()}
    finally:
        await conn.close()

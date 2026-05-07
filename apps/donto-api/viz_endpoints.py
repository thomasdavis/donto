"""Visualization data endpoints for the ECharts dashboard.

Each endpoint returns pre-shaped data optimized for specific chart types.
These are read-only and query Postgres directly for performance.
"""

import os
import json
import asyncpg
from fastapi import APIRouter, Query, HTTPException
from typing import Optional

router = APIRouter(prefix="/viz", tags=["Visualization"])

DSN = os.environ.get("DONTO_DSN", "postgres://donto:donto@127.0.0.1:5432/donto")


async def _conn():
    return await asyncpg.connect(DSN)


@router.get("/entity/{entity:path}")
async def entity_deep_dive(entity: str, limit: int = Query(200)):
    """Full entity profile: connections grouped by predicate category, maturity distribution,
    temporal evidence, and source provenance. Shaped for radial/sunburst charts."""
    conn = await _conn()
    try:
        # Outgoing edges
        outgoing = await conn.fetch("""
            SELECT predicate,
                   COALESCE(object_iri, object_lit ->> 'v') as object,
                   object_iri IS NOT NULL as is_iri,
                   context, (flags >> 2 & 7) as maturity,
                   lower(tx_time) as discovered_at
            FROM donto_statement
            WHERE subject = $1 AND upper(tx_time) IS NULL
            ORDER BY (flags >> 2 & 7) DESC
            LIMIT $2
        """, entity, limit)

        # Incoming edges
        incoming = await conn.fetch("""
            SELECT subject, predicate, context,
                   (flags >> 2 & 7) as maturity,
                   lower(tx_time) as discovered_at
            FROM donto_statement
            WHERE object_iri = $1 AND upper(tx_time) IS NULL
            ORDER BY (flags >> 2 & 7) DESC
            LIMIT $2
        """, entity, limit)

        # Group predicates by semantic category
        categories = _categorize_predicates(
            [r["predicate"] for r in outgoing] + [r["predicate"] for r in incoming]
        )

        # Maturity distribution
        maturity_dist = {}
        for r in list(outgoing) + list(incoming):
            m = f"L{r['maturity']}"
            maturity_dist[m] = maturity_dist.get(m, 0) + 1

        # Source contexts
        contexts = {}
        for r in list(outgoing) + list(incoming):
            ctx = r["context"]
            contexts[ctx] = contexts.get(ctx, 0) + 1

        # Build radial data: categories → predicates → objects
        radial_data = []
        pred_groups = {}
        for r in outgoing:
            cat = categories.get(r["predicate"], "other")
            if cat not in pred_groups:
                pred_groups[cat] = {}
            pred = r["predicate"]
            if pred not in pred_groups[cat]:
                pred_groups[cat][pred] = []
            pred_groups[cat][pred].append({
                "value": r["object"],
                "is_iri": r["is_iri"],
                "maturity": r["maturity"],
            })

        for cat, preds in pred_groups.items():
            children = []
            for pred, objects in preds.items():
                children.append({
                    "name": pred,
                    "value": len(objects),
                    "children": [{"name": o["value"] or "—", "value": 1, "maturity": o["maturity"]} for o in objects[:20]],
                })
            radial_data.append({"name": cat, "children": children})

        # Timeline data (discovery over time)
        timeline = {}
        for r in list(outgoing) + list(incoming):
            if r["discovered_at"]:
                day = r["discovered_at"].strftime("%Y-%m-%d")
                timeline[day] = timeline.get(day, 0) + 1

        return {
            "entity": entity,
            "total_outgoing": len(outgoing),
            "total_incoming": len(incoming),
            "radial": {"name": entity, "children": radial_data},
            "maturity": maturity_dist,
            "contexts": [{"context": k, "facts": v} for k, v in sorted(contexts.items(), key=lambda x: -x[1])[:15]],
            "timeline": [{"date": k, "facts": v} for k, v in sorted(timeline.items())],
            "incoming_subjects": [
                {"subject": r["subject"], "predicate": r["predicate"], "maturity": r["maturity"]}
                for r in incoming[:50]
            ],
        }
    finally:
        await conn.close()


@router.get("/pulse")
async def research_pulse(days: int = Query(30)):
    """Research activity pulse: facts/day, active contexts, predicate growth.
    Shaped for timeline + heatmap charts."""
    conn = await _conn()
    try:
        # Daily fact count
        daily = await conn.fetch("""
            SELECT date_trunc('day', lower(tx_time))::date as day, count(*) as facts
            FROM donto_statement
            WHERE upper(tx_time) IS NULL AND lower(tx_time) > now() - ($1 || ' days')::interval
            GROUP BY day ORDER BY day
        """, str(days))

        # Hourly for last 24h (heartbeat resolution)
        hourly = await conn.fetch("""
            SELECT date_trunc('hour', lower(tx_time)) as hour, count(*) as facts
            FROM donto_statement
            WHERE upper(tx_time) IS NULL AND lower(tx_time) > now() - interval '24 hours'
            GROUP BY hour ORDER BY hour
        """)

        # Active contexts this period
        active_contexts = await conn.fetch("""
            SELECT context, count(*) as facts,
                   min(lower(tx_time)) as first_fact,
                   max(lower(tx_time)) as last_fact
            FROM donto_statement
            WHERE upper(tx_time) IS NULL AND lower(tx_time) > now() - ($1 || ' days')::interval
            GROUP BY context ORDER BY facts DESC LIMIT 30
        """, str(days))

        # Predicate growth (new predicates per day)
        pred_growth = await conn.fetch("""
            SELECT date_trunc('day', min(lower(tx_time)))::date as first_seen, count(*) as new_predicates
            FROM (
                SELECT predicate, min(lower(tx_time)) as first_tx
                FROM donto_statement WHERE upper(tx_time) IS NULL
                GROUP BY predicate
            ) sub
            WHERE first_tx > now() - ($1 || ' days')::interval
            GROUP BY first_seen ORDER BY first_seen
        """, str(days))

        # Maturity distribution over time
        maturity_trend = await conn.fetch("""
            SELECT date_trunc('day', lower(tx_time))::date as day,
                   (flags >> 2 & 7) as maturity, count(*) as cnt
            FROM donto_statement
            WHERE upper(tx_time) IS NULL AND lower(tx_time) > now() - ($1 || ' days')::interval
            GROUP BY day, maturity ORDER BY day, maturity
        """, str(days))

        return {
            "daily": [{"date": str(r["day"]), "facts": r["facts"]} for r in daily],
            "hourly": [{"hour": r["hour"].isoformat(), "facts": r["facts"]} for r in hourly],
            "active_contexts": [
                {"context": r["context"], "facts": r["facts"],
                 "first": r["first_fact"].isoformat() if r["first_fact"] else None,
                 "last": r["last_fact"].isoformat() if r["last_fact"] else None}
                for r in active_contexts
            ],
            "predicate_growth": [{"date": str(r["first_seen"]), "new": r["new_predicates"]} for r in pred_growth],
            "maturity_trend": [
                {"date": str(r["day"]), "maturity": f"L{r['maturity']}", "count": r["cnt"]}
                for r in maturity_trend
            ],
        }
    finally:
        await conn.close()


@router.get("/evidence/{entity:path}")
async def evidence_web(entity: str, limit: int = Query(100)):
    """Evidence flow for an entity: source contexts → facts, with contradiction detection.
    Shaped for Sankey diagrams."""
    conn = await _conn()
    try:
        # All facts about this entity from all contexts
        facts = await conn.fetch("""
            SELECT subject, predicate,
                   COALESCE(object_iri, object_lit ->> 'v') as object,
                   context, (flags >> 2 & 7) as maturity,
                   (flags & 3) as polarity_code
            FROM donto_statement
            WHERE (subject = $1 OR object_iri = $1) AND upper(tx_time) IS NULL
            ORDER BY predicate, context
            LIMIT $2
        """, entity, limit)

        # Detect contradictions: same subject+predicate, different objects, from different contexts
        pred_objects: dict[str, list] = {}
        for r in facts:
            if r["subject"] == entity:
                key = r["predicate"]
                if key not in pred_objects:
                    pred_objects[key] = []
                pred_objects[key].append({
                    "object": r["object"],
                    "context": r["context"],
                    "maturity": r["maturity"],
                    "polarity": r["polarity_code"],
                })

        contradictions = []
        for pred, entries in pred_objects.items():
            objects = set(e["object"] for e in entries if e["object"])
            contexts = set(e["context"] for e in entries)
            if len(objects) > 1 and len(contexts) > 1:
                contradictions.append({
                    "predicate": pred,
                    "variants": [{"object": e["object"], "context": e["context"], "maturity": e["maturity"]} for e in entries],
                })

        # Sankey links: context → predicate → object
        sankey_nodes = set()
        sankey_links = []
        for r in facts:
            if r["subject"] == entity:
                src = r["context"]
                mid = r["predicate"]
                tgt = r["object"] or "—"
                sankey_nodes.add(src)
                sankey_nodes.add(mid)
                sankey_nodes.add(tgt)
                sankey_links.append({"source": src, "target": mid, "value": 1})
                sankey_links.append({"source": mid, "target": tgt, "value": 1})

        # Deduplicate sankey links
        link_map = {}
        for l in sankey_links:
            key = (l["source"], l["target"])
            link_map[key] = link_map.get(key, 0) + l["value"]

        return {
            "entity": entity,
            "total_facts": len(facts),
            "contradictions": contradictions,
            "sankey": {
                "nodes": [{"name": n} for n in sankey_nodes],
                "links": [{"source": k[0], "target": k[1], "value": v} for k, v in link_map.items()],
            },
            "contexts": list(set(r["context"] for r in facts)),
        }
    finally:
        await conn.close()


@router.get("/predicates")
async def predicate_universe(limit: int = Query(500)):
    """Predicate landscape: usage distribution, semantic clusters, alignment status.
    Shaped for treemap charts."""
    conn = await _conn()
    try:
        # Top predicates with counts
        preds = await conn.fetch("""
            SELECT predicate, count(*) as cnt
            FROM donto_statement
            WHERE upper(tx_time) IS NULL
              AND predicate NOT LIKE 'donto:%'
              AND predicate NOT LIKE 'rdf:%'
              AND predicate NOT LIKE 'rdfs:%'
              AND predicate NOT LIKE 'ex:normalized_claims/%'
              AND predicate NOT LIKE 'ex:column/%'
              AND predicate NOT LIKE 'ex:meta/%'
            GROUP BY predicate
            ORDER BY cnt DESC
            LIMIT $1
        """, limit)

        # Categorize
        categorized = {}
        for r in preds:
            cat = _categorize_predicate(r["predicate"])
            if cat not in categorized:
                categorized[cat] = []
            categorized[cat].append({"name": r["predicate"], "value": r["cnt"]})

        treemap = [
            {"name": cat, "children": items}
            for cat, items in sorted(categorized.items(), key=lambda x: -sum(i["value"] for i in x[1]))
        ]

        # Alignment stats
        try:
            alignment_count = await conn.fetchval(
                "SELECT count(*) FROM donto_predicate_alignment WHERE upper(tx_time) IS NULL"
            )
        except Exception:
            alignment_count = 0

        return {
            "total_content_predicates": sum(r["cnt"] for r in preds),
            "distinct_predicates": len(preds),
            "treemap": treemap,
            "alignment_count": alignment_count,
        }
    finally:
        await conn.close()


@router.get("/overview")
async def graph_overview():
    """High-level graph stats for the dashboard header."""
    conn = await _conn()
    try:
        stats = await conn.fetchrow("""
            SELECT
                (SELECT count(*) FROM donto_statement WHERE upper(tx_time) IS NULL) as total_statements,
                (SELECT count(DISTINCT subject) FROM donto_statement WHERE upper(tx_time) IS NULL) as subjects,
                (SELECT count(DISTINCT predicate) FROM donto_statement WHERE upper(tx_time) IS NULL) as predicates,
                (SELECT count(DISTINCT context) FROM donto_statement WHERE upper(tx_time) IS NULL) as contexts,
                (SELECT count(*) FROM donto_statement WHERE upper(tx_time) IS NULL AND lower(tx_time) > now() - interval '24 hours') as facts_24h,
                (SELECT count(*) FROM donto_statement WHERE upper(tx_time) IS NULL AND lower(tx_time) > now() - interval '7 days') as facts_7d
        """)
        return {
            "total_statements": stats["total_statements"],
            "subjects": stats["subjects"],
            "predicates": stats["predicates"],
            "contexts": stats["contexts"],
            "facts_24h": stats["facts_24h"],
            "facts_7d": stats["facts_7d"],
        }
    finally:
        await conn.close()


def _categorize_predicate(pred: str) -> str:
    """Assign a predicate to a semantic category based on name patterns."""
    p = pred.lower().replace("ex:", "")
    family = ["father", "mother", "parent", "child", "sibling", "spouse", "married", "wife", "husband", "son", "daughter", "born", "died", "birth", "death", "family"]
    location = ["location", "located", "place", "city", "country", "region", "lives", "resides", "address", "latitude", "longitude", "coordinates", "area", "boundary"]
    temporal = ["date", "year", "time", "when", "period", "era", "century", "decade", "month", "day", "age", "duration"]
    identity = ["name", "known", "alias", "label", "title", "called", "identified", "type", "kind", "isa", "istype", "classification"]
    source = ["source", "reference", "cite", "archive", "document", "url", "evidence", "provenance"]
    opinion = ["opinion", "believe", "think", "claim", "assert", "argue", "advocate", "criticize", "agree", "disagree", "view", "stance"]
    relation = ["associated", "related", "connected", "member", "part", "belongs", "contains", "includes", "involves"]

    for word in family:
        if word in p: return "family"
    for word in location:
        if word in p: return "location"
    for word in temporal:
        if word in p: return "temporal"
    for word in identity:
        if word in p: return "identity"
    for word in source:
        if word in p: return "source"
    for word in opinion:
        if word in p: return "opinion"
    for word in relation:
        if word in p: return "relation"
    return "other"


def _categorize_predicates(predicates: list[str]) -> dict[str, str]:
    """Categorize a list of predicates."""
    return {p: _categorize_predicate(p) for p in predicates}

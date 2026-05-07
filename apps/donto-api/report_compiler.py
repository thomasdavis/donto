"""Report compiler — generates full analytical reports from research intents.

Orchestrates skills, generates narratives (template-based, no LLM),
and produces ECharts-ready chart specs.
"""

import asyncio
import os
from datetime import datetime, timezone

import asyncpg

from analytical_ir import (
    ResearchIntent, AnalyticalReport, SemanticScope, EntityRef,
    ChartSpec, ActionItem,
)
from skills import (
    build_family_tree, detect_contradictions, find_corroborations,
    build_timeline, detect_migrations, cluster_name_variants,
    analyze_evidence_gaps, compute_quality_score,
)

DSN = os.environ.get("DONTO_DSN", "postgres://donto:donto@127.0.0.1:5432/donto")


async def compile_report(intent: ResearchIntent) -> AnalyticalReport:
    conn = await asyncpg.connect(DSN)
    try:
        primary = intent.scope_entities[0] if intent.scope_entities else None
        if not primary:
            return AnalyticalReport(intent=intent, narrative="No entities specified.")

        # Resolve entity info
        entity_info = await conn.fetchrow("""
            SELECT subject, count(*) as cnt FROM donto_statement
            WHERE subject = $1 AND upper(tx_time) IS NULL
            GROUP BY subject
        """, primary)
        fact_count = entity_info["cnt"] if entity_info else 0

        scope = SemanticScope(
            entities=[EntityRef(iri=primary, fact_count=fact_count)],
        )

        # Run independent skills in parallel
        family_task = build_family_tree(conn, primary, depth=3)
        contra_task = detect_contradictions(conn, entity=primary)
        corrob_task = find_corroborations(conn, primary)
        timeline_task = build_timeline(conn, primary)
        migration_task = detect_migrations(conn, primary)
        names_task = cluster_name_variants(conn, primary)
        gaps_task = analyze_evidence_gaps(conn, primary)
        quality_task = compute_quality_score(conn, entity=primary)

        (family, contradictions, corroborations, timeline, migrations,
         names, gaps, quality) = await asyncio.gather(
            family_task, contra_task, corrob_task, timeline_task,
            migration_task, names_task, gaps_task, quality_task,
        )

        # Generate actions from gaps and contradictions
        actions = _generate_actions(primary, contradictions, gaps, family, corroborations)

        # Generate charts
        charts = _generate_charts(primary, family, timeline, contradictions, quality, migrations)

        # Generate narrative
        narrative = _generate_narrative(
            intent, primary, fact_count, family, contradictions,
            corroborations, timeline, gaps, quality, actions, names, migrations,
        )

        return AnalyticalReport(
            intent=intent,
            scope=scope,
            family_tree=family,
            timeline=timeline,
            contradictions=contradictions,
            corroborations=corroborations,
            evidence_gaps=gaps,
            migrations=migrations,
            name_variants=names,
            quality=quality,
            actions=actions,
            charts=charts,
            narrative=narrative,
            generated_at=datetime.now(timezone.utc).isoformat(),
        )
    finally:
        await conn.close()


def _generate_actions(entity, contradictions, gaps, family, corroborations):
    actions = []

    for gap in gaps:
        actions.append(ActionItem(
            action_type="investigate",
            description=f"No {gap.missing_category.replace('_', ' ')} found. {gap.suggestion}",
            priority=gap.priority,
            target_entities=[entity],
            suggested_sources=["BDM records", "Trove archives", "Census records"],
        ))

    for contra in contradictions[:5]:
        vals = ", ".join(f"'{v.value}' ({v.context.split('/')[-1]})" for v in contra.variants[:3])
        actions.append(ActionItem(
            action_type="resolve_contradiction",
            description=f"Conflicting values for {contra.predicate}: {vals}",
            priority="high" if contra.severity == "hard" else "medium",
            target_entities=[contra.entity],
        ))

    # Check for family members with no facts
    if family:
        for member in family.members:
            if member.fact_count == 0 and member.iri != entity:
                actions.append(ActionItem(
                    action_type="investigate",
                    description=f"{member.iri} referenced in family tree but has 0 facts",
                    priority="medium",
                    target_entities=[member.iri],
                    suggested_sources=["Extract from any source mentioning this person"],
                ))

    actions.sort(key=lambda a: {"high": 0, "medium": 1, "low": 2}.get(a.priority, 3))
    return actions[:20]


def _generate_narrative(intent, entity, fact_count, family, contradictions,
                        corroborations, timeline, gaps, quality, actions, names, migrations):
    lines = [f"## Research Report: {intent.question}", ""]

    lines.append(f"**Entity:** `{entity}` ({fact_count} total facts)")
    lines.append(f"**Quality Score:** {quality.overall:.0%}")
    lines.append("")

    # Family
    if family and family.links:
        lines.append(f"### Family Structure")
        lines.append(f"Found {len(family.links)} family connections across {len(family.members)} family members.")
        for link in family.links[:10]:
            lines.append(f"- {link.subject} → *{link.predicate}* → {link.object} (L{link.maturity})")
        lines.append("")

    # Names
    if names:
        lines.append("### Known Names")
        for n in names[:5]:
            lines.append(f"- **{n.value}** ({n.occurrences}x across {len(n.contexts)} sources)")
        lines.append("")

    # Timeline
    if timeline:
        dated = [e for e in timeline if e.date_sort_key]
        lines.append(f"### Timeline ({len(dated)} dated events)")
        for e in dated[:15]:
            lines.append(f"- {e.date_sort_key}: {e.predicate} = {e.value} (L{e.maturity})")
        lines.append("")

    # Migrations
    if migrations:
        lines.append(f"### Location History ({len(migrations)} records)")
        for m in migrations[:10]:
            date_str = f" ({m.date_hint})" if m.date_hint else ""
            lines.append(f"- {m.predicate}: **{m.location}**{date_str}")
        lines.append("")

    # Contradictions
    if contradictions:
        lines.append(f"### Contradictions ({len(contradictions)} found)")
        for c in contradictions[:5]:
            vals = " vs ".join(f"'{v.value}' [{v.context.split('/')[-1]}]" for v in c.variants[:3])
            lines.append(f"- **{c.predicate}**: {vals} [{c.severity}]")
        lines.append("")

    # Corroborations
    if corroborations:
        lines.append(f"### Corroborated Facts ({len(corroborations)} multi-source)")
        for c in corroborations[:5]:
            lines.append(f"- {c.predicate} = {c.value} (confirmed by {len(c.source_contexts)} sources)")
        lines.append("")

    # Quality
    lines.append("### Evidence Quality")
    lines.append(f"- Source reliability: {quality.source_reliability:.0%}")
    lines.append(f"- Predicate coverage: {quality.predicate_coverage:.0%}")
    lines.append(f"- Corroboration rate: {quality.corroboration_rate:.0%}")
    lines.append(f"- Contradiction density: {quality.contradiction_density:.1f}%")
    lines.append("")

    # Gaps
    if gaps:
        lines.append(f"### Evidence Gaps ({len(gaps)} categories missing)")
        for g in gaps:
            lines.append(f"- **{g.missing_category}**: {g.suggestion}")
        lines.append("")

    # Actions
    if actions:
        lines.append(f"### Recommended Next Steps")
        for i, a in enumerate(actions[:10], 1):
            lines.append(f"{i}. [{a.priority.upper()}] {a.description}")

    return "\n".join(lines)


def _generate_charts(entity, family, timeline, contradictions, quality, migrations):
    charts = []

    # Family tree graph
    if family and family.links:
        nodes = [{"name": m.iri, "symbolSize": max(10, min(m.fact_count / 5, 50)),
                  "category": family.generations.get(m.iri, 0)} for m in family.members]
        edges = [{"source": l.subject, "target": l.object, "label": {"show": True, "formatter": l.predicate, "fontSize": 9}}
                 for l in family.links[:100]]
        categories = [{"name": f"Gen {g}"} for g in sorted(set(family.generations.values()))]

        charts.append(ChartSpec(
            chart_type="graph",
            title="Family Tree",
            description=f"{len(family.members)} members, {len(family.links)} connections",
            data={
                "type": "graph",
                "layout": "force",
                "data": nodes,
                "links": edges,
                "categories": categories,
                "roam": True,
                "force": {"repulsion": 200, "gravity": 0.1, "edgeLength": [80, 200]},
                "label": {"show": True, "fontSize": 10, "color": "#c9d1d9"},
                "lineStyle": {"color": "#30363d", "curveness": 0.1},
                "emphasis": {"focus": "adjacency", "lineStyle": {"width": 3}},
            },
        ))

    # Timeline
    if timeline:
        dated = [e for e in timeline if e.date_sort_key]
        if dated:
            charts.append(ChartSpec(
                chart_type="timeline",
                title="Event Timeline",
                description=f"{len(dated)} dated events",
                data={
                    "xAxis": {"type": "category", "data": [e.date_sort_key for e in dated]},
                    "yAxis": {"type": "value"},
                    "series": [{"type": "scatter", "symbolSize": 12,
                               "data": list(range(len(dated))),
                               "label": {"show": True, "formatter": lambda i: dated[i].predicate if i < len(dated) else ""}}],
                },
            ))

    # Quality radar
    charts.append(ChartSpec(
        chart_type="radar",
        title="Evidence Quality",
        description=f"Overall: {quality.overall:.0%}",
        data={
            "radar": {"indicator": [
                {"name": "Reliability", "max": 1},
                {"name": "Completeness", "max": 1},
                {"name": "Corroboration", "max": 1},
                {"name": "Low Contradiction", "max": 1},
                {"name": "Coverage", "max": 1},
            ]},
            "series": [{"type": "radar", "data": [{"value": [
                quality.source_reliability,
                quality.extraction_completeness,
                quality.corroboration_rate,
                1.0 - min(quality.contradiction_density / 10, 1.0),
                quality.predicate_coverage,
            ], "name": entity}]}],
        },
    ))

    return charts

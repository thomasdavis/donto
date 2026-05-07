"""Genealogy research skills — pure SQL analytical functions.

Each skill takes an asyncpg connection and returns typed IR objects.
No LLM calls. Sub-second execution against indexed columns.
"""

import re
from analytical_ir import (
    FamilyTree, FamilyLink, EntityRef, Contradiction, ContradictionVariant,
    Corroboration, TimelineEvent, EvidenceGap, QualityScore,
    MigrationStep, NameVariant,
)

FAMILY_PATTERNS = [
    '%child%', '%parent%', '%married%', '%sibling%', '%spouse%',
    '%father%', '%mother%', '%son%', '%daughter%', '%brother%', '%sister%',
    '%wife%', '%husband%', '%adopted%', '%stepchild%',
]

LOCATION_PATTERNS = [
    '%location%', '%located%', '%place%', '%city%', '%resid%',
    '%lived%', '%born%In%', '%died%In%', '%migrat%', '%moved%',
    '%arrived%', '%departed%', '%address%',
]

TEMPORAL_PATTERNS = [
    '%date%', '%year%', '%born%', '%died%', '%married%',
    '%arrived%', '%departed%', '%founded%', '%started%', '%ended%',
    '%occurred%', '%when%', '%age%',
]

NAME_PATTERNS = [
    '%name%', '%label%', '%known%', '%alias%', '%called%', '%titled%',
]

PERSON_EXPECTED = {
    "birth_details": ["born%", "birth%"],
    "death_details": ["died%", "death%"],
    "family": ["%child%", "%parent%", "%married%", "%spouse%", "%father%", "%mother%"],
    "location": ["%location%", "%resid%", "%lived%", "%born%In%"],
    "identity": ["%name%", "%known%", "%label%"],
}


async def build_family_tree(conn, entity: str, depth: int = 3) -> FamilyTree:
    rows = await conn.fetch("""
        WITH RECURSIVE family AS (
            SELECT subject, predicate, object_iri, context,
                   (flags >> 2 & 7) as maturity, 0 as hop
            FROM donto_statement
            WHERE (subject = $1 OR object_iri = $1)
              AND upper(tx_time) IS NULL
              AND object_iri IS NOT NULL
              AND predicate ILIKE ANY($2::text[])
            UNION
            SELECT s.subject, s.predicate, s.object_iri, s.context,
                   (s.flags >> 2 & 7), f.hop + 1
            FROM donto_statement s
            JOIN family f ON (s.subject = f.object_iri OR s.object_iri = f.subject)
            WHERE upper(s.tx_time) IS NULL
              AND s.object_iri IS NOT NULL
              AND s.predicate ILIKE ANY($2::text[])
              AND f.hop < $3
        )
        SELECT DISTINCT subject, predicate, object_iri, context, maturity, hop
        FROM family
        LIMIT 500
    """, entity, FAMILY_PATTERNS, depth)

    members_set = set()
    links = []
    for r in rows:
        members_set.add(r["subject"])
        if r["object_iri"]:
            members_set.add(r["object_iri"])
        links.append(FamilyLink(
            subject=r["subject"],
            predicate=r["predicate"],
            object=r["object_iri"] or "",
            context=r["context"],
            maturity=r["maturity"],
        ))

    member_facts = {}
    if members_set:
        counts = await conn.fetch("""
            SELECT subject, count(*) as cnt
            FROM donto_statement
            WHERE subject = ANY($1::text[]) AND upper(tx_time) IS NULL
            GROUP BY subject
        """, list(members_set))
        member_facts = {r["subject"]: r["cnt"] for r in counts}

    members = [EntityRef(iri=m, fact_count=member_facts.get(m, 0)) for m in members_set]

    # Compute generations via BFS
    generations = {entity: 0}
    parent_preds = {"childof", "sonof", "daughterof"}
    child_preds = {"parentof", "fatherof", "motherof"}
    for link in links:
        pred_lower = link.predicate.lower().replace("_", "").replace("-", "")
        if link.subject == entity and any(p in pred_lower for p in parent_preds):
            generations.setdefault(link.object, -1)
        elif link.object == entity and any(p in pred_lower for p in child_preds):
            generations.setdefault(link.subject, -1)
        elif link.subject == entity and any(p in pred_lower for p in child_preds):
            generations.setdefault(link.object, 1)

    return FamilyTree(
        root_entity=entity,
        members=members,
        links=links,
        generations=generations,
    )


async def detect_contradictions(conn, entity: str = None, context: str = None, limit: int = 50) -> list[Contradiction]:
    where = "upper(s1.tx_time) IS NULL AND upper(s2.tx_time) IS NULL"
    params = []
    idx = 1
    if entity:
        where += f" AND s1.subject = ${idx}"
        params.append(entity)
        idx += 1
    if context:
        where += f" AND (s1.context = ${idx} OR s2.context = ${idx})"
        params.append(context)
        idx += 1

    rows = await conn.fetch(f"""
        SELECT s1.subject, s1.predicate,
               COALESCE(s1.object_iri, s1.object_lit ->> 'v') as val1,
               COALESCE(s2.object_iri, s2.object_lit ->> 'v') as val2,
               s1.context as ctx1, s2.context as ctx2,
               (s1.flags >> 2 & 7) as mat1, (s2.flags >> 2 & 7) as mat2
        FROM donto_statement s1
        JOIN donto_statement s2
          ON s1.subject = s2.subject AND s1.predicate = s2.predicate
          AND COALESCE(s1.object_iri, s1.object_lit ->> 'v') != COALESCE(s2.object_iri, s2.object_lit ->> 'v')
          AND s1.statement_id < s2.statement_id
        WHERE {where}
          AND s1.predicate NOT LIKE 'donto:%'
          AND s1.predicate NOT LIKE 'rdf:%'
        LIMIT {limit}
    """, *params)

    grouped: dict[tuple, list] = {}
    for r in rows:
        key = (r["subject"], r["predicate"])
        if key not in grouped:
            grouped[key] = {}
        grouped[key][(r["val1"], r["ctx1"])] = r["mat1"]
        grouped[key][(r["val2"], r["ctx2"])] = r["mat2"]

    result = []
    for (subj, pred), variants_map in grouped.items():
        variants = [
            ContradictionVariant(value=val, context=ctx, maturity=mat)
            for (val, ctx), mat in variants_map.items()
        ]
        severity = "hard" if all(v.maturity >= 3 for v in variants) else "soft"
        result.append(Contradiction(entity=subj, predicate=pred, variants=variants, severity=severity))
    return result


async def find_corroborations(conn, entity: str, limit: int = 50) -> list[Corroboration]:
    rows = await conn.fetch("""
        SELECT subject, predicate,
               COALESCE(object_iri, object_lit ->> 'v') as value,
               array_agg(DISTINCT context) as contexts,
               count(DISTINCT context) as source_count,
               max((flags >> 2 & 7)) as max_maturity
        FROM donto_statement
        WHERE subject = $1 AND upper(tx_time) IS NULL
          AND predicate NOT LIKE 'donto:%'
        GROUP BY subject, predicate, COALESCE(object_iri, object_lit ->> 'v')
        HAVING count(DISTINCT context) >= 2
        ORDER BY source_count DESC
        LIMIT $2
    """, entity, limit)

    return [Corroboration(
        entity=r["subject"], predicate=r["predicate"], value=r["value"],
        source_contexts=list(r["contexts"]), combined_maturity=r["max_maturity"],
    ) for r in rows]


async def build_timeline(conn, entity: str, limit: int = 100) -> list[TimelineEvent]:
    rows = await conn.fetch("""
        SELECT subject, predicate,
               COALESCE(object_iri, object_lit ->> 'v') as value,
               object_lit ->> 'dt' as datatype,
               context, (flags >> 2 & 7) as maturity,
               lower(valid_time)::text as valid_from,
               upper(valid_time)::text as valid_to
        FROM donto_statement
        WHERE subject = $1 AND upper(tx_time) IS NULL
          AND (predicate ILIKE ANY($2::text[])
               OR object_lit ->> 'dt' IN ('xsd:date', 'xsd:gYear', 'xsd:dateTime'))
        ORDER BY COALESCE(lower(valid_time), 'infinity'::date)
        LIMIT $3
    """, entity, TEMPORAL_PATTERNS, limit)

    events = []
    for r in rows:
        date_key = None
        if r["valid_from"] and r["valid_from"] != "infinity":
            date_key = r["valid_from"]
        elif r["datatype"] in ("xsd:date", "xsd:gYear", "xsd:dateTime") and r["value"]:
            date_key = r["value"]
        elif r["value"] and re.match(r'^\d{4}', str(r["value"])):
            date_key = str(r["value"])[:10]

        events.append(TimelineEvent(
            entity=r["subject"], predicate=r["predicate"], value=r["value"],
            date_sort_key=date_key,
            valid_from=r["valid_from"] if r["valid_from"] != "infinity" else None,
            valid_to=r["valid_to"] if r["valid_to"] and r["valid_to"] != "infinity" else None,
            context=r["context"], maturity=r["maturity"],
        ))

    events.sort(key=lambda e: e.date_sort_key or "9999")
    return events


async def detect_migrations(conn, entity: str) -> list[MigrationStep]:
    rows = await conn.fetch("""
        SELECT predicate,
               COALESCE(object_iri, object_lit ->> 'v') as location,
               context, (flags >> 2 & 7) as maturity,
               lower(valid_time)::text as vfrom
        FROM donto_statement
        WHERE subject = $1 AND upper(tx_time) IS NULL
          AND predicate ILIKE ANY($2::text[])
        ORDER BY COALESCE(lower(valid_time), 'infinity'::date)
    """, entity, LOCATION_PATTERNS)

    return [MigrationStep(
        location=r["location"] or "", predicate=r["predicate"],
        date_hint=r["vfrom"] if r["vfrom"] != "infinity" else None,
        context=r["context"], maturity=r["maturity"],
    ) for r in rows]


async def cluster_name_variants(conn, entity: str) -> list[NameVariant]:
    rows = await conn.fetch("""
        SELECT COALESCE(object_iri, object_lit ->> 'v') as name_value,
               array_agg(DISTINCT context) as contexts,
               count(*) as occurrences
        FROM donto_statement
        WHERE subject = $1 AND upper(tx_time) IS NULL
          AND predicate ILIKE ANY($2::text[])
        GROUP BY name_value
        ORDER BY occurrences DESC
    """, entity, NAME_PATTERNS)

    return [NameVariant(
        value=r["name_value"] or "", contexts=list(r["contexts"]),
        occurrences=r["occurrences"],
    ) for r in rows]


async def analyze_evidence_gaps(conn, entity: str) -> list[EvidenceGap]:
    existing = await conn.fetch("""
        SELECT DISTINCT predicate FROM donto_statement
        WHERE subject = $1 AND upper(tx_time) IS NULL
    """, entity)
    existing_preds = {r["predicate"].lower() for r in existing}

    gaps = []
    for category, patterns in PERSON_EXPECTED.items():
        found = any(
            any(re.match(pat.replace('%', '.*'), pred, re.IGNORECASE) for pat in patterns)
            for pred in existing_preds
        )
        if not found:
            suggestions = {
                "birth_details": "Search BDM birth records, baptism registers, or census records",
                "death_details": "Search BDM death records, cemetery databases, or obituaries",
                "family": "Search marriage records, census households, or family bibles",
                "location": "Search electoral rolls, post office directories, or land records",
                "identity": "Extract from any source mentioning this entity to get a name fact",
            }
            gaps.append(EvidenceGap(
                entity=entity, missing_category=category,
                missing_predicates=patterns,
                priority="high" if category in ("birth_details", "identity") else "medium",
                suggestion=suggestions.get(category, "Search archival records"),
            ))
    return gaps


async def compute_quality_score(conn, entity: str = None, context: str = None) -> QualityScore:
    where = "upper(tx_time) IS NULL"
    params = []
    if entity:
        where += " AND subject = $1"
        params.append(entity)
    elif context:
        where += " AND context = $1"
        params.append(context)
    else:
        return QualityScore()

    stats = await conn.fetchrow(f"""
        SELECT count(*) as total,
               avg((flags >> 2 & 7)) as avg_maturity,
               count(DISTINCT predicate) as pred_count,
               count(DISTINCT context) as ctx_count
        FROM donto_statement WHERE {where}
    """, *params)

    total = stats["total"] or 1
    avg_mat = float(stats["avg_maturity"] or 0)

    # Corroboration rate
    corr = await conn.fetchval(f"""
        SELECT count(*) FROM (
            SELECT subject, predicate, COALESCE(object_iri, object_lit ->> 'v')
            FROM donto_statement WHERE {where}
            GROUP BY subject, predicate, COALESCE(object_iri, object_lit ->> 'v')
            HAVING count(DISTINCT context) >= 2
        ) sub
    """, *params)

    unique_claims = await conn.fetchval(f"""
        SELECT count(*) FROM (
            SELECT DISTINCT subject, predicate, COALESCE(object_iri, object_lit ->> 'v')
            FROM donto_statement WHERE {where}
        ) sub
    """, *params)

    corr_rate = (corr or 0) / max(unique_claims or 1, 1)

    # Contradiction density
    if entity:
        contras = await detect_contradictions(conn, entity=entity, limit=100)
        contra_density = len(contras) / max(total, 1) * 100
    else:
        contra_density = 0.0

    # Coverage
    if entity:
        gaps = await analyze_evidence_gaps(conn, entity)
        coverage = 1.0 - len(gaps) / len(PERSON_EXPECTED)
    else:
        coverage = 0.5

    reliability = min(avg_mat / 4.0, 1.0)
    completeness = min((stats["pred_count"] or 0) / 20.0, 1.0)

    overall = (reliability * 0.3 + completeness * 0.2 + corr_rate * 0.2
               + (1.0 - min(contra_density / 10.0, 1.0)) * 0.15 + coverage * 0.15)

    return QualityScore(
        source_reliability=round(reliability, 3),
        extraction_completeness=round(completeness, 3),
        corroboration_rate=round(corr_rate, 3),
        contradiction_density=round(contra_density, 3),
        predicate_coverage=round(coverage, 3),
        overall=round(overall, 3),
    )

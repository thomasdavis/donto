"""Agent instructions middleware — adds context-aware guidance to every API response.

Every JSON response gets an `agent_instructions` field that tells AI agents
what to do next, what related endpoints to call, and how to interpret the data.
"""

import json
import re


def generate_instructions(path: str, method: str, status_code: int, body: dict) -> dict:
    """Generate agent instructions based on the endpoint, method, and response data."""

    instructions = {
        "next_steps": [],
        "tips": [],
        "related_endpoints": [],
    }

    # ── Health ──
    if path == "/health":
        instructions["next_steps"] = [
            "System is healthy. You can now extract knowledge, query the graph, or check the job queue.",
            "Start with GET /subjects to see top entities, or GET /search?q=<term> to find specific entities.",
        ]
        instructions["related_endpoints"] = [
            {"method": "GET", "path": "/subjects", "description": "List top entities by fact count"},
            {"method": "GET", "path": "/search?q=<term>", "description": "Full-text search for entities"},
            {"method": "POST", "path": "/extract-and-ingest", "description": "Extract knowledge from text (synchronous)"},
            {"method": "POST", "path": "/jobs/extract", "description": "Extract knowledge from text (async, returns job ID)"},
        ]
        return instructions

    # ── Extract and Ingest (sync) ──
    if path == "/extract-and-ingest" and method == "POST":
        ctx = body.get("context", "")
        facts = body.get("facts_extracted", 0)
        instructions["next_steps"] = [
            f"Extraction complete: {facts} facts ingested into context '{ctx}'.",
            f"Query the extracted facts: GET /connections/{_first_subject(body)} to see how entities connect.",
            f"Check extraction quality: GET /context/analytics/{ctx}",
            "To extract more documents into the same context, call POST /extract-and-ingest again with the same context.",
            "To extract into a NEW context, use a different context IRI. Convention: ctx:namespace/topic/source-type",
        ]
        instructions["related_endpoints"] = [
            {"method": "GET", "path": f"/context/analytics/{ctx}", "description": "See what was extracted — subjects, predicates, new entities"},
            {"method": "GET", "path": "/connections/<entity-iri>", "description": "Explore an extracted entity's connections (both incoming and outgoing)"},
            {"method": "POST", "path": "/align/rebuild", "description": "Rebuild predicate alignment index after extraction"},
        ]
        instructions["tips"] = [
            "Each context is independently queryable. Use specific contexts like ctx:genes/person-name/source-type.",
            "The extraction uses 8 analytical tiers — from surface facts to philosophical implications.",
            f"Cost: ${body.get('usage', {}).get('cost', 0):.4f} for this extraction.",
        ]
        return instructions

    # ── Jobs: Submit ──
    if path == "/jobs/extract" and method == "POST":
        job_id = body.get("job_id", "")
        status = body.get("status", "")
        if status == "duplicate":
            instructions["next_steps"] = [
                f"This context was already submitted. The existing job ID is '{job_id}'.",
                f"Check its status: GET /jobs/{job_id}",
                "If the previous extraction failed, call POST /jobs/retry-failed to retry all failed jobs.",
            ]
        else:
            instructions["next_steps"] = [
                f"Job '{job_id}' is queued. It will go through 4 phases: extracting → ingesting → aligning → resolving → completed.",
                f"Poll for status: GET /jobs/{job_id} — check every 30-60 seconds.",
                f"The job typically takes 30-180 seconds depending on text length.",
                "Do NOT submit the same context again — duplicates are rejected.",
            ]
        instructions["related_endpoints"] = [
            {"method": "GET", "path": f"/jobs/{job_id}", "description": "Poll this job's status and results"},
            {"method": "GET", "path": "/jobs", "description": "See all jobs and queue summary"},
        ]
        return instructions

    # ── Jobs: Batch Submit ──
    if path == "/jobs/batch" and method == "POST":
        count = body.get("count", 0)
        skipped = body.get("skipped_duplicates", 0)
        job_ids = body.get("job_ids", [])
        instructions["next_steps"] = [
            f"Submitted {count} extraction jobs. {skipped} duplicates were skipped.",
            "Each job goes through: extracting → ingesting → aligning → resolving → completed.",
            "Poll GET /jobs for an overview of all jobs and their statuses.",
            f"Or poll individual jobs: GET /jobs/<job_id> for each of: {', '.join(job_ids[:5])}{'...' if len(job_ids) > 5 else ''}",
            "Wait for all jobs to reach 'completed' status before querying the graph for results.",
        ]
        instructions["related_endpoints"] = [
            {"method": "GET", "path": "/jobs", "description": "Queue overview — see how many are extracting, completed, failed"},
            {"method": "GET", "path": "/jobs/<job_id>", "description": "Individual job status"},
            {"method": "POST", "path": "/jobs/retry-failed", "description": "Retry any jobs that failed"},
        ]
        return instructions

    # ── Jobs: List ──
    if path == "/jobs" and method == "GET":
        summary = body.get("summary", {})
        total = body.get("total", 0)
        extracting = summary.get("extracting", 0)
        completed = summary.get("completed", 0)
        failed = summary.get("failed", 0)
        queued = summary.get("queued", 0)

        steps = []
        if extracting > 0 or queued > 0:
            steps.append(f"{extracting + queued} jobs still processing. Wait and poll again in 30-60 seconds.")
        if completed > 0 and extracting == 0 and queued == 0:
            steps.append("All jobs complete! Query the graph for results.")
            steps.append("Try: GET /subjects to see top entities, or GET /connections/<entity> to explore.")
        if failed > 0:
            steps.append(f"{failed} jobs failed. Call POST /jobs/retry-failed to retry them.")
        if total == 0:
            steps.append("No jobs yet. Submit text for extraction with POST /jobs/extract or POST /jobs/batch.")

        instructions["next_steps"] = steps
        instructions["related_endpoints"] = [
            {"method": "GET", "path": "/jobs?status=completed", "description": "See only completed jobs"},
            {"method": "GET", "path": "/jobs?status=failed", "description": "See failed jobs"},
            {"method": "POST", "path": "/jobs/retry-failed", "description": "Retry all failed jobs"},
            {"method": "GET", "path": "/jobs/<job_id>/facts", "description": "See extracted facts for a completed job"},
        ]
        return instructions

    # ── Jobs: Detail ──
    if re.match(r"/jobs/[^/]+$", path) and method == "GET" and "status" in body:
        job_id = body.get("id", "")
        status = body.get("status", "")
        context = body.get("context", "")

        if status == "completed":
            facts = body.get("facts_extracted", 0)
            aligned = body.get("alignments_created", 0)
            resolved = body.get("entities_resolved", 0)
            instructions["next_steps"] = [
                f"Job complete: {facts} facts extracted, {aligned} predicates aligned, {resolved} entities resolved.",
                f"View the extracted facts: GET /jobs/{job_id}/facts?limit=1000",
                f"View the source text: GET /jobs/{job_id}/source",
                f"Explore the context: GET /context/analytics/{context}",
                f"Explore specific entities: GET /connections/<entity-iri> for any entity in the facts.",
            ]
            instructions["related_endpoints"] = [
                {"method": "GET", "path": f"/jobs/{job_id}/facts?limit=1000", "description": "All extracted facts (subject, predicate, object, tier, maturity)"},
                {"method": "GET", "path": f"/jobs/{job_id}/source", "description": "The original source text"},
                {"method": "GET", "path": f"/context/analytics/{context}", "description": "Context stats — new subjects, predicate distribution, cross-context overlap"},
                {"method": "GET", "path": "/connections/<entity-iri>", "description": "Bidirectional connections for any entity"},
            ]
        elif status == "failed":
            instructions["next_steps"] = [
                f"Job '{job_id}' failed: {body.get('error', 'unknown error')}",
                "Retry this and all other failed jobs: POST /jobs/retry-failed",
                "Or submit the text again with a different model: POST /jobs/extract with model='mistral'",
            ]
            instructions["related_endpoints"] = [
                {"method": "POST", "path": "/jobs/retry-failed", "description": "Retry all failed jobs"},
            ]
        elif status in ("extracting", "ingesting", "aligning", "resolving"):
            phase_info = {
                "extracting": "LLM is analyzing the text and extracting facts (30-180s)",
                "ingesting": "Writing facts to the knowledge graph (<5s)",
                "aligning": "Finding similar predicates and creating alignments (<10s)",
                "resolving": "Linking entities across contexts (<10s)",
            }
            instructions["next_steps"] = [
                f"Job is in '{status}' phase: {phase_info.get(status, '')}",
                f"Poll again in 30 seconds: GET /jobs/{job_id}",
                "Do NOT submit another job for the same context while this one is running.",
            ]
        else:
            instructions["next_steps"] = [
                f"Job '{job_id}' is queued. Waiting for a worker slot (25 concurrent max).",
                f"Poll again in 30 seconds: GET /jobs/{job_id}",
            ]
        return instructions

    # ── Jobs: Facts ──
    if re.match(r"/jobs/[^/]+/facts", path):
        count = body.get("count", 0)
        context = body.get("context", "")
        facts = body.get("facts", [])
        subjects = list(set(f.get("subject", "") for f in facts[:100] if f.get("subject")))
        instructions["next_steps"] = [
            f"{count} facts found in context '{context}'.",
            "Each fact has: subject, predicate, object, tier (T1-T8), maturity (L0-L4).",
            "To explore an entity's full connections: GET /connections/<subject-iri>",
            f"Top subjects to explore: {', '.join(subjects[:5])}",
        ]
        instructions["related_endpoints"] = [
            {"method": "GET", "path": f"/connections/{subjects[0]}", "description": f"Explore {subjects[0]} connections"} if subjects else {},
            {"method": "GET", "path": f"/context/analytics/{context}", "description": "Context-level analytics"},
            {"method": "GET", "path": "/search?q=<term>", "description": "Search across all contexts"},
        ]
        instructions["related_endpoints"] = [e for e in instructions["related_endpoints"] if e]
        instructions["tips"] = [
            "Tier 1-2 = factual, Tier 3-4 = interpretive, Tier 5-6 = pragmatic, Tier 7-8 = philosophical/intertextual",
            "Maturity L4 = very high confidence, L3 = high, L2 = moderate, L1 = low, L0 = speculative",
        ]
        return instructions

    # ── Connections ──
    if path.startswith("/connections/"):
        entity = body.get("entity", "")
        out_count = body.get("total_outgoing", 0)
        in_count = body.get("total_incoming", 0)
        connected = body.get("connected_entities", [])
        instructions["next_steps"] = [
            f"Entity '{entity}' has {out_count} outgoing and {in_count} incoming connections.",
            f"Connected to {len(connected)} other entities.",
            "Explore connected entities by calling GET /connections/<entity> for each.",
            "For a visual subgraph: POST /graph/neighborhood with this entity as center.",
        ]
        instructions["related_endpoints"] = [
            {"method": "GET", "path": f"/connections/{connected[0]}", "description": f"Explore connected entity"} if connected else {},
            {"method": "POST", "path": "/graph/neighborhood", "description": "Get visual subgraph (depth 1-3)"},
            {"method": "GET", "path": f"/search?q={entity.replace('ex:', '')}", "description": "Search for related entities"},
        ]
        instructions["related_endpoints"] = [e for e in instructions["related_endpoints"] if e]
        instructions["tips"] = [
            "Outgoing = facts where this entity is the subject. Incoming = facts where this entity is the object.",
            "Use min_maturity=2 to filter out low-confidence connections.",
            "Use context= parameter to scope to a specific research context.",
        ]
        return instructions

    # ── Context Analytics ──
    if path.startswith("/context/analytics/"):
        stats = body.get("stats", {})
        new_subjects = body.get("new_subjects", [])
        shared = body.get("shared_subjects", [])
        instructions["next_steps"] = [
            f"Context has {stats.get('total_statements', 0)} statements, {stats.get('distinct_subjects', 0)} subjects, {stats.get('distinct_predicates', 0)} predicates.",
            f"{len(new_subjects)} subjects are NEW (only appear in this context).",
            f"{len(shared)} subjects appear in OTHER contexts too (cross-context links).",
            "Explore new subjects to find previously unknown entities.",
            "Explore shared subjects to find corroborating evidence across sources.",
        ]
        instructions["related_endpoints"] = [
            {"method": "GET", "path": f"/connections/{new_subjects[0]['subject']}", "description": f"Explore new entity: {new_subjects[0]['subject']}"} if new_subjects else {},
            {"method": "GET", "path": f"/connections/{shared[0]['subject']}", "description": f"Explore cross-context entity: {shared[0]['subject']}"} if shared else {},
            {"method": "POST", "path": "/align/rebuild", "description": "Rebuild predicate alignments for better query coverage"},
        ]
        instructions["related_endpoints"] = [e for e in instructions["related_endpoints"] if e]
        return instructions

    # ── Search ──
    if path == "/search":
        instructions["next_steps"] = [
            "Search results show entities matching your query with fact counts.",
            "Explore any entity: GET /connections/<subject-iri>",
            "Get full history: GET /history/<subject-iri>",
        ]
        instructions["related_endpoints"] = [
            {"method": "GET", "path": "/connections/<subject>", "description": "Full bidirectional connections"},
            {"method": "GET", "path": "/history/<subject>", "description": "Complete statement history"},
            {"method": "POST", "path": "/graph/neighborhood", "description": "Visual subgraph"},
        ]
        return instructions

    # ── Subjects ──
    if path == "/subjects":
        instructions["next_steps"] = [
            "These are the top entities by fact count in the knowledge graph.",
            "Explore any entity: GET /connections/<subject-iri> for bidirectional edges.",
            "Search for specific entities: GET /search?q=<term>",
        ]
        return instructions

    # ── Retry Failed ──
    if path == "/jobs/retry-failed" and method == "POST":
        retried = body.get("retried", 0)
        errors = body.get("errors", [])
        instructions["next_steps"] = [
            f"Retried {retried} failed jobs. {len(errors)} could not be retried.",
            "Poll GET /jobs to monitor progress.",
            "Wait for retried jobs to complete, then check results.",
        ]
        return instructions

    # ── Alignment ──
    if path.startswith("/align/"):
        instructions["next_steps"] = [
            "After registering alignments, ALWAYS call POST /align/rebuild to update the closure index.",
            "Without rebuilding, new alignments won't take effect in queries.",
            "Use GET /align/suggest/<predicate> to find similar predicates for alignment.",
        ]
        instructions["related_endpoints"] = [
            {"method": "POST", "path": "/align/rebuild", "description": "REQUIRED after registration — rebuilds closure index"},
            {"method": "GET", "path": "/align/suggest/<predicate>", "description": "Find similar predicates by trigram similarity"},
            {"method": "POST", "path": "/align/register", "description": "Register a new alignment"},
        ]
        return instructions

    # ── Firehose ──
    if path.startswith("/firehose/"):
        instructions["next_steps"] = [
            "The firehose shows real-time database activity.",
            "Use GET /firehose/stream (SSE) for live events.",
            "Use GET /firehose/recent?limit=100 for recent events without streaming.",
            "Use GET /firehose/stats for activity sparkline and active queries.",
        ]
        return instructions

    # ── Default ──
    instructions["next_steps"] = [
        "Explore the knowledge graph: GET /subjects, GET /search?q=<term>, GET /connections/<entity>",
        "Extract knowledge: POST /extract-and-ingest (sync) or POST /jobs/extract (async)",
        "Check system: GET /health, GET /version",
    ]
    instructions["related_endpoints"] = [
        {"method": "GET", "path": "/health", "description": "System health check"},
        {"method": "GET", "path": "/subjects", "description": "Top entities"},
        {"method": "GET", "path": "/search?q=<term>", "description": "Full-text entity search"},
        {"method": "POST", "path": "/jobs/extract", "description": "Async extraction with job tracking"},
    ]
    return instructions


def _first_subject(body: dict) -> str:
    """Extract a sample subject IRI from extraction results."""
    # This is a best-effort helper
    return "<entity-iri>"

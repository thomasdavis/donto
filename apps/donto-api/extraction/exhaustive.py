"""Multi-aperture exhaustive extraction.

`run_exhaustive` runs each requested aperture through the same
`ModelCaller` interface as the M5 single-pass orchestrator, then
unions the candidates with content-hash deduplication.

This replaces the 8-tier prompt's coverage story:

  before: one prompt → N facts, breakdown is "tier 1: 45, tier 2: 13…"
   after: K passes → ∪ deduped facts, breakdown is "surface: 45,
          linguistic: 312, presupposition: 87, inferential: 41,
          conceivable: 218, recursive: 56", with anchor coverage
          and hypothesis density as first-class metrics.

The point is not arbitrary categorisation; the point is that more
passes with different lenses mine more of the candidate space.
"""

from __future__ import annotations

import hashlib
import json
from dataclasses import dataclass, field
from typing import Awaitable, Callable, Mapping

from .apertures import Aperture, prompt_for_aperture
from .decomposers import decompose
from .domain import Domain, pick_domain
from .policy_gate import Authoriser, source_permits_external_model
from .quarantine import QuarantineSink, quarantine_candidate
from .validation import validate_candidate


ModelCaller = Callable[[str, str, str], Awaitable[tuple[list[dict], dict]]]
"""Same shape as M5: (system_prompt, user_text, model_id) -> (facts, metadata)."""


# All non-recursive apertures. RECURSIVE is special-cased: it consumes
# seeds from the union of prior passes.
DEFAULT_APERTURES: tuple[Aperture, ...] = (
    Aperture.SURFACE,
    Aperture.LINGUISTIC,
    Aperture.PRESUPPOSITION,
    Aperture.INFERENTIAL,
    Aperture.CONCEIVABLE,
)


@dataclass
class ApertureResult:
    aperture: Aperture
    raw_count: int = 0
    accepted_count: int = 0
    quarantined_count: int = 0
    metadata: dict = field(default_factory=dict)


@dataclass
class ExhaustiveOutcome:
    domain: Domain
    facts: list[dict] = field(default_factory=list)
    quarantined: list[tuple[dict, str]] = field(default_factory=list)
    blocked_by_policy: bool = False
    block_reason: str | None = None
    by_aperture: list[ApertureResult] = field(default_factory=list)
    dedup_collisions: int = 0
    recursion_depth: int = 0
    yield_metrics: dict = field(default_factory=dict)


def _content_key(c: dict) -> str:
    """Deterministic hash for deduplication.

    Two candidates collide iff they make the same claim about the same
    subject with the same modality. Anchors and confidences differ
    across apertures and are *not* part of the key — we want
    cross-aperture facts to merge.
    """
    obj = c.get("object") or {}
    if "iri" in obj and obj["iri"]:
        obj_repr = f"iri:{obj['iri']}"
    elif "literal" in obj and isinstance(obj["literal"], dict):
        lit = obj["literal"]
        obj_repr = f"lit:{lit.get('v')}^^{lit.get('dt', '')}@{lit.get('lang') or ''}"
    else:
        obj_repr = json.dumps(obj, sort_keys=True)
    payload = (
        f"{c.get('subject', '')}\x1f"
        f"{c.get('predicate', '')}\x1f"
        f"{obj_repr}\x1f"
        f"{bool(c.get('hypothesis_only'))}"
    )
    return hashlib.sha256(payload.encode()).hexdigest()


def _seeds_from(facts: list[dict], limit: int = 12) -> list[str]:
    """Pick the most frequent unique non-trivial subjects + objects as
    recursion seeds, capped to `limit`."""
    counts: dict[str, int] = {}
    for f in facts:
        for v in (f.get("subject"), (f.get("object") or {}).get("iri")):
            if isinstance(v, str) and v.startswith("ex:") and len(v) > 5:
                counts[v] = counts.get(v, 0) + 1
    return [k for k, _ in sorted(counts.items(), key=lambda kv: -kv[1])[:limit]]


def _compute_yield_metrics(
    facts: list[dict],
    by_aperture: list[ApertureResult],
    dedup_collisions: int,
    recursion_depth: int,
) -> dict:
    if not facts:
        return {
            "total_facts": 0, "distinct_predicates": 0, "distinct_subjects": 0,
            "anchor_coverage": 0.0, "hypothesis_density": 0.0,
            "dedup_collisions": dedup_collisions, "recursion_depth": recursion_depth,
            "facts_per_aperture": {ar.aperture.value: ar.accepted_count for ar in by_aperture},
        }
    distinct_p = len({f.get("predicate") for f in facts if f.get("predicate")})
    distinct_s = len({f.get("subject") for f in facts if f.get("subject")})
    anchored = sum(1 for f in facts if f.get("anchor"))
    hypothetical = sum(1 for f in facts if f.get("hypothesis_only"))
    return {
        "total_facts": len(facts),
        "distinct_predicates": distinct_p,
        "distinct_subjects": distinct_s,
        "anchor_coverage": round(anchored / len(facts), 3),
        "hypothesis_density": round(hypothetical / len(facts), 3),
        "dedup_collisions": dedup_collisions,
        "recursion_depth": recursion_depth,
        "facts_per_aperture": {ar.aperture.value: ar.accepted_count for ar in by_aperture},
    }


async def run_exhaustive(
    *,
    text: str,
    source_iri: str,
    model: str,
    model_caller: ModelCaller,
    authoriser: Authoriser,
    quarantine_sink: QuarantineSink,
    apertures: tuple[Aperture, ...] = DEFAULT_APERTURES,
    recurse: bool = True,
    max_recursion: int = 1,
    hints: Mapping[str, str] | None = None,
    policy_action: str = "derive",
) -> ExhaustiveOutcome:
    domain = pick_domain(text, hints)
    outcome = ExhaustiveOutcome(domain=domain)

    if not source_permits_external_model(authoriser, source_iri, policy_action):
        outcome.blocked_by_policy = True
        outcome.block_reason = (
            f"policy denied {policy_action} on source {source_iri}"
        )
        return outcome

    seen: dict[str, dict] = {}
    dedup_collisions = 0

    async def run_one(ap: Aperture, seeds: list[str] | None = None) -> ApertureResult:
        nonlocal dedup_collisions
        prompt = prompt_for_aperture(ap, domain=domain, seeds=seeds)
        raw_facts, metadata = await model_caller(prompt, text, model)
        result = ApertureResult(aperture=ap, raw_count=len(raw_facts), metadata=metadata)
        candidates = decompose(raw_facts, domain)
        for c in candidates:
            c["aperture"] = ap.value
            v = validate_candidate(c)
            if not v.ok:
                quarantine_candidate(quarantine_sink, c, v.reason or "invalid")
                outcome.quarantined.append((c, v.reason or "invalid"))
                result.quarantined_count += 1
                continue
            key = _content_key(c)
            if key in seen:
                dedup_collisions += 1
                # Merge: keep the highest-confidence anchored copy.
                prior = seen[key]
                if (c.get("anchor") and not prior.get("anchor")) or \
                   float(c.get("confidence", 0)) > float(prior.get("confidence", 0)):
                    seen[key] = c
                continue
            seen[key] = c
            result.accepted_count += 1
        return result

    for ap in apertures:
        outcome.by_aperture.append(await run_one(ap))

    if recurse and max_recursion > 0:
        seeds = _seeds_from(list(seen.values()))
        if seeds:
            outcome.recursion_depth = 1
            outcome.by_aperture.append(await run_one(Aperture.RECURSIVE, seeds=seeds))

    outcome.facts = list(seen.values())
    outcome.dedup_collisions = dedup_collisions
    outcome.yield_metrics = _compute_yield_metrics(
        outcome.facts, outcome.by_aperture,
        dedup_collisions, outcome.recursion_depth,
    )
    return outcome

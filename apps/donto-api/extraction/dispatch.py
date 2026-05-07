"""Extraction orchestrator.

`run_extraction(...)` ties the pieces together:

  1. Pick the domain (explicit hint wins; otherwise heuristics).
  2. Run the policy gate. Refusal short-circuits with no model call.
  3. Build the per-domain prompt and call the model adapter.
  4. Decompose the model's output into candidate dicts.
  5. Validate each candidate. Failures go to the quarantine sink;
     the rest are returned in `accepted`.

The `model_caller` is injected so unit tests don't need real LLMs.
The orchestrator never hides errors: model adapter failures
propagate. Validation failures are routed to quarantine.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Awaitable, Callable, Mapping

from .decomposers import decompose
from .domain import Domain, pick_domain
from .policy_gate import Authoriser, source_permits_external_model
from .prompts import prompt_for
from .quarantine import QuarantineSink, quarantine_candidate
from .validation import validate_candidate


ModelCaller = Callable[[str, str, str], Awaitable[tuple[list[dict], dict]]]
"""(prompt, text, model_id) -> (raw_facts, metadata)."""


@dataclass
class ExtractionOutcome:
    domain: Domain
    accepted: list[dict] = field(default_factory=list)
    quarantined: list[tuple[dict, str]] = field(default_factory=list)
    blocked_by_policy: bool = False
    block_reason: str | None = None
    metadata: dict = field(default_factory=dict)


async def run_extraction(
    *,
    text: str,
    source_iri: str,
    model: str,
    model_caller: ModelCaller,
    authoriser: Authoriser,
    quarantine_sink: QuarantineSink,
    hints: Mapping[str, str] | None = None,
    policy_action: str = "derive",
) -> ExtractionOutcome:
    domain = pick_domain(text, hints)
    outcome = ExtractionOutcome(domain=domain)

    if not source_permits_external_model(authoriser, source_iri, policy_action):
        outcome.blocked_by_policy = True
        outcome.block_reason = (
            f"policy denied {policy_action} on source {source_iri} "
            f"for anonymous caller"
        )
        return outcome

    prompt = prompt_for(domain)
    raw_facts, metadata = await model_caller(prompt, text, model)
    outcome.metadata = metadata

    candidates = decompose(raw_facts, domain)
    for c in candidates:
        result = validate_candidate(c)
        if result.ok:
            outcome.accepted.append(c)
        else:
            quarantine_candidate(quarantine_sink, c, result.reason or "invalid")
            outcome.quarantined.append((c, result.reason or "invalid"))
    return outcome

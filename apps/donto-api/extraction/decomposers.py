"""Per-domain decomposers.

A decomposer takes raw model output (a list of fact dicts) and
returns a normalised list of candidate dicts the validator can
inspect. Domain-specific normalisation lives here:

  * defaulting `hypothesis_only` when no anchor is present,
  * stamping a `domain` tag onto each candidate for downstream review,
  * trimming whitespace,
  * filling in `confidence=1.0` if missing.

The general decomposer is enough for most domains; specialist
decomposers can be plugged in via `_BY_DOMAIN`.
"""

from __future__ import annotations

from typing import Any

from .domain import Domain


def _normalise(fact: dict, domain: Domain) -> dict:
    out = dict(fact)
    if isinstance(out.get("subject"), str):
        out["subject"] = out["subject"].strip()
    if isinstance(out.get("predicate"), str):
        out["predicate"] = out["predicate"].strip()
    out.setdefault("anchor", None)
    if "hypothesis_only" not in out:
        out["hypothesis_only"] = out.get("anchor") is None
    out.setdefault("confidence", 1.0)
    out["domain"] = domain.value
    return out


def _general_decompose(facts: list[dict], domain: Domain) -> list[dict]:
    return [_normalise(f, domain) for f in facts if isinstance(f, dict)]


def _genealogy_decompose(facts: list[dict], domain: Domain) -> list[dict]:
    out = []
    for f in facts:
        if not isinstance(f, dict):
            continue
        c = _normalise(f, domain)
        # Genealogy: dates without an explicit anchor are inferred —
        # downgrade confidence to ≤ 0.7 to match the prompt contract.
        if c.get("anchor") is None and isinstance(c.get("predicate"), str) \
                and c["predicate"].lower().endswith("on"):
            c["confidence"] = min(float(c.get("confidence", 1.0)), 0.7)
        out.append(c)
    return out


_BY_DOMAIN = {
    Domain.GENEALOGY: _genealogy_decompose,
    Domain.LINGUISTIC: _general_decompose,
    Domain.PAPERS: _general_decompose,
    Domain.MEDICAL_STUB: _general_decompose,
    Domain.LEGAL_STUB: _general_decompose,
    Domain.GENERAL: _general_decompose,
}


def decompose(facts: Any, domain: Domain) -> list[dict]:
    if not isinstance(facts, list):
        return []
    fn = _BY_DOMAIN.get(domain, _general_decompose)
    return fn(facts, domain)

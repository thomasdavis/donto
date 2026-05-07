"""Domain-dispatched extraction kernel (M5).

Replaces the single 8-tier prompt with a domain selector + per-domain
prompt + per-domain decomposer + candidate validator + quarantine
sink + policy gate. The orchestrator in `dispatch.py` is the entry
point; everything else is replaceable in isolation.

Domains are intentionally a small fixed set:
    genealogy | linguistic | papers | medical_stub | legal_stub | general

The stubbed domains (`medical_stub`, `legal_stub`) reuse the general
prompt for now and exist so the dispatch table is exhaustive — domain
specialists fill them in later without touching the dispatcher.
"""

from .domain import Domain, pick_domain
from .prompts import prompt_for
from .decomposers import decompose
from .validation import validate_candidate, ValidationResult
from .quarantine import quarantine_candidate
from .policy_gate import source_permits_external_model
from .dispatch import run_extraction, ExtractionOutcome

__all__ = [
    "Domain",
    "pick_domain",
    "prompt_for",
    "decompose",
    "validate_candidate",
    "ValidationResult",
    "quarantine_candidate",
    "source_permits_external_model",
    "run_extraction",
    "ExtractionOutcome",
]

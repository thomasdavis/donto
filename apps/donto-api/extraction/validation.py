"""Candidate schema validation.

`validate_candidate` is the hard gate before a candidate enters the
ingest path. The rules are intentionally narrow — it rejects
malformed shapes only, not unlikely-but-syntactically-valid claims.
Semantic review lives downstream.

Hard-rejected:
  * subject missing or empty
  * predicate missing or empty
  * object missing both `iri` and `literal`
  * anchor present but not the {doc, start, end} shape with start <= end
  * confidence not numeric in [0, 1]
  * hypothesis_only must be bool
  * `hypothesis_only=false` with `anchor=null` (the PRD invariant:
    every candidate has anchor or hypothesis-only flag)
"""

from __future__ import annotations

from dataclasses import dataclass
from numbers import Real
from typing import Any


@dataclass(frozen=True)
class ValidationResult:
    ok: bool
    reason: str | None = None

    @classmethod
    def good(cls) -> "ValidationResult":
        return cls(True, None)

    @classmethod
    def bad(cls, reason: str) -> "ValidationResult":
        return cls(False, reason)


def _is_nonempty_str(v: Any) -> bool:
    return isinstance(v, str) and bool(v.strip())


def validate_candidate(c: Any) -> ValidationResult:
    if not isinstance(c, dict):
        return ValidationResult.bad("candidate must be a JSON object")

    if not _is_nonempty_str(c.get("subject")):
        return ValidationResult.bad("subject missing or empty")
    if not _is_nonempty_str(c.get("predicate")):
        return ValidationResult.bad("predicate missing or empty")

    obj = c.get("object")
    if not isinstance(obj, dict):
        return ValidationResult.bad("object must be {iri:..} or {literal:..}")
    has_iri = _is_nonempty_str(obj.get("iri"))
    lit = obj.get("literal")
    has_lit = isinstance(lit, dict) and "v" in lit and _is_nonempty_str(lit.get("dt"))
    if not (has_iri or has_lit):
        return ValidationResult.bad("object missing both iri and literal")

    anchor = c.get("anchor")
    if anchor is not None:
        if not isinstance(anchor, dict):
            return ValidationResult.bad("anchor must be {doc, start, end} or null")
        if not _is_nonempty_str(anchor.get("doc")):
            return ValidationResult.bad("anchor.doc missing")
        s, e = anchor.get("start"), anchor.get("end")
        if not (isinstance(s, int) and isinstance(e, int) and s >= 0 and e >= s):
            return ValidationResult.bad("anchor.start/end must be non-negative ints with start <= end")

    hyp = c.get("hypothesis_only", False)
    if not isinstance(hyp, bool):
        return ValidationResult.bad("hypothesis_only must be bool")

    if anchor is None and not hyp:
        return ValidationResult.bad(
            "candidate without anchor must set hypothesis_only=true"
        )

    conf = c.get("confidence", 1.0)
    if not isinstance(conf, Real) or isinstance(conf, bool):
        return ValidationResult.bad("confidence must be numeric")
    if not (0.0 <= float(conf) <= 1.0):
        return ValidationResult.bad("confidence must be in [0, 1]")

    return ValidationResult.good()

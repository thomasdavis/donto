"""Domain selection.

`pick_domain(text, hints)` returns the best-fit domain for a text:

* If the caller supplied an explicit hint via `hints["domain"]`, that
  wins (case-insensitive). Useful for callers that already know.
* Otherwise a small bag of heuristics inspects the text. They are
  intentionally cheap and conservative — false positives fall back to
  `General`, which is correct (just less specific).

The heuristics are not ML; they are keyword sniffing tuned for the
PRD's six domains. They exist so the system has something to dispatch
on before a learned classifier lands.
"""

from __future__ import annotations

from enum import Enum
import re
from typing import Mapping


class Domain(str, Enum):
    GENEALOGY = "genealogy"
    LINGUISTIC = "linguistic"
    PAPERS = "papers"
    MEDICAL_STUB = "medical_stub"
    LEGAL_STUB = "legal_stub"
    GENERAL = "general"

    @classmethod
    def parse(cls, raw: str | None) -> "Domain | None":
        if not raw:
            return None
        norm = raw.strip().lower().replace("-", "_")
        for d in cls:
            if d.value == norm:
                return d
        return None


_GENEALOGY_HINTS = re.compile(
    r"\b(born|b\.|d\.|died|baptised|baptized|m\.|married|christened|"
    r"father|mother|son of|daughter of|spouse)\b",
    re.IGNORECASE,
)

_LINGUISTIC_HINTS = re.compile(
    r"\b(phoneme|allophone|morpheme|gloss|interlinear|dialect|"
    r"glottal|fricative|consonant|vowel|tone|"
    r"ergative|absolutive|noun class|case marking)\b|"
    r"[æəʒŋθðʃʤʣ]",  # IPA
    re.IGNORECASE,
)

_PAPERS_HINTS = re.compile(
    r"\b(abstract|introduction|methods?|results?|discussion|conclusion|"
    r"references|figure\s*\d|table\s*\d|p\s*<\s*0\.\d|"
    r"doi:|arxiv|\bet\s*al\.?)\b",
    re.IGNORECASE,
)

_MEDICAL_HINTS = re.compile(
    r"\b(patient|diagnosis|symptom|prescribed|dose|mg/kg|"
    r"icd-?10|snomed|comorbidity|treatment outcome)\b",
    re.IGNORECASE,
)

_LEGAL_HINTS = re.compile(
    r"\b(plaintiff|defendant|appellant|appellee|"
    r"the court (held|finds|ruled)|jurisdiction|"
    r"\bv\.\s+[A-Z][\w-]+|"  # "Smith v. Jones"
    r"§\s*\d+|U\.S\.C\.?|statute)\b",
    re.IGNORECASE,
)


def pick_domain(text: str, hints: Mapping[str, str] | None = None) -> Domain:
    if hints and "domain" in hints:
        explicit = Domain.parse(hints["domain"])
        if explicit is not None:
            return explicit

    sample = text[:8000]  # cheap heuristics; cap at 8KB

    scores = {
        Domain.LINGUISTIC: len(_LINGUISTIC_HINTS.findall(sample)),
        Domain.GENEALOGY: len(_GENEALOGY_HINTS.findall(sample)),
        Domain.PAPERS: len(_PAPERS_HINTS.findall(sample)),
        Domain.MEDICAL_STUB: len(_MEDICAL_HINTS.findall(sample)),
        Domain.LEGAL_STUB: len(_LEGAL_HINTS.findall(sample)),
    }
    best_domain, best_score = max(scores.items(), key=lambda kv: kv[1])
    if best_score >= 2:
        return best_domain
    return Domain.GENERAL

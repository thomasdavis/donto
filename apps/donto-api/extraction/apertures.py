"""Apertures — different lenses through which the same text can be mined.

The 8-tier prompt was a single pass that asked the LLM to label each
fact with a tier. That capped the yield at "what one prompt can elicit
in one pass." It also conflated *what is in the text* with *what could
be claimed* — a fact about Ajax's birth year and a fact about Ajax
having hair are wildly different epistemically but both wore the same
"Tier 1" label.

Apertures replace tiers. Each aperture is a distinct extraction pass
with its own system prompt and its own truth contract:

* SURFACE       — what the text explicitly states. Anchored, asserted.
* LINGUISTIC    — clause-by-clause syntactic decomposition; every NP
                  becomes an entity claim, every VP an event claim,
                  every modifier a property claim. Anchored, asserted.
* PRESUPPOSITION — what the text takes for granted. Anchored,
                  hypothesis_only=true (presupposed, not asserted).
* INFERENTIAL   — claims that follow from stated facts via common
                  knowledge. Lower confidence; asserted.
* CONCEIVABLE   — claims that *could* be made about a mentioned entity
                  given its type. The "hairs on the head" lens.
                  Hypothesis_only=true; floods the candidate space.
* RECURSIVE     — re-runs SURFACE with each newly discovered entity as
                  a fresh seed, exhausting transitive context.

Yield comes from running multiple apertures and unioning the results
with content-hash deduplication. The metric isn't "how many tiers did
we hit" but "how many distinct claims did we mine, at what anchor
coverage, at what hypothesis density."
"""

from __future__ import annotations

from enum import Enum

from .domain import Domain


class Aperture(str, Enum):
    SURFACE = "surface"
    LINGUISTIC = "linguistic"
    PRESUPPOSITION = "presupposition"
    INFERENTIAL = "inferential"
    CONCEIVABLE = "conceivable"
    RECURSIVE = "recursive"


_OUTPUT_SCHEMA = """
## OUTPUT FORMAT

Return a JSON object with a single "facts" array. Each fact:

{
  "subject":   "ex:<kebab-case-iri>",
  "predicate": "<camelCase>",
  "object":    {"iri": "ex:<kebab-case>"} | {"literal": {"v": <value>, "dt": "<xsd type>"}},
  "anchor":    {"doc": "<doc-iri>", "start": <int>, "end": <int>} | null,
  "hypothesis_only": <bool>,
  "confidence": <0.0-1.0>,
  "aperture":   "<see system prompt>",
  "notes":     "<short>"
}

IRIs must be kebab-lower-case. Mint predicates freely; prefer specific
over generic. Return ONLY the JSON, no commentary.
"""


_SURFACE = """You are running the SURFACE aperture.

Extract every claim the text *explicitly states*: identities,
classifications, biographical facts, affiliations, education,
locations, dates, quantities, authorship, attributions, named
relationships. Anchor every fact to its text span. Set
hypothesis_only=false. Confidence 0.95-1.0 for stated facts.

Bias hard toward MORE triples — target 50-200+ for a 500-word
text. Do not stop at the obvious. If a sentence says "Alice studied
at Harvard, graduating in 2010", you owe at least four facts:
attendedUniversity, studiedAt, graduatedFromUniversity,
graduatedInYear.
""" + _OUTPUT_SCHEMA


_LINGUISTIC = """You are running the LINGUISTIC aperture.

Decompose the text clause by clause. For every clause:
  * Every noun phrase becomes an entity claim (`refersToEntity`,
    `mentionedAs`, `appositiveOf`).
  * Every verb phrase becomes an event claim (`eventType`, `eventAgent`,
    `eventPatient`, `eventInstrument`).
  * Every adjective/adverb becomes a property claim
    (`hasModifier`, `attributesQuality`).
  * Every pronoun becomes a coreference claim (`corefersWith`).
  * Every conjunction becomes a discourse claim (`discourseRelation`).

This pass is supposed to be EXHAUSTIVE at the syntactic level — it
should produce far more facts than SURFACE. Aim for one fact per
syntactic constituent.

Anchor everything. hypothesis_only=false. confidence 0.85-1.0.
""" + _OUTPUT_SCHEMA


_PRESUPPOSITION = """You are running the PRESUPPOSITION aperture.

Mine claims the text *takes for granted but does not assert*.
Examples:
  * "Ajax married Jane in 2018" presupposes: Ajax exists, Jane exists,
    they were both unmarried before, marriage is a relation that
    holds across time.
  * "the donto project" presupposes: donto is a project, projects can
    be referred to with definite articles, the speaker assumes the
    reader can identify it.

Set hypothesis_only=true on EVERY fact (it's presupposed, not
asserted). Anchor to the trigger phrase. confidence 0.7-0.95
depending on how robust the presupposition is.

Aim for 1-3 presuppositions per sentence — these proliferate quickly.
""" + _OUTPUT_SCHEMA


_INFERENTIAL = """You are running the INFERENTIAL aperture.

For every entity and event the text introduces, derive claims that
*follow from common knowledge*. Examples:
  * "Born 1990 in Sydney" → "is Australian", "is alive in 2026",
    "is < 100 years old", "has a Sydney birthplace".
  * "Founded the donto project" → "is a software person",
    "engages in open-source", "has at least one project".

These are NOT in the text but are obvious to any informed reader.
Anchor to the trigger fact's span. hypothesis_only=false (the
inference is real). confidence 0.4-0.7 (lower than stated facts).
""" + _OUTPUT_SCHEMA


_CONCEIVABLE = """You are running the CONCEIVABLE aperture.

For every entity mentioned, list claims that *could plausibly hold*
given the entity's type — even if neither stated nor inferable. This
is the "hairs on their head" lens.

If a person is mentioned: hasHair, hasBloodType, hasFavoriteColor,
hasShoeSize, hasHandedness, hasFirstLanguage, hasParents, hasGenome,
hasFingerprints, hasChildhoodMemories, hasHeartRate, hasDigestiveSystem.

If a project is mentioned: hasContributors, hasLicense, hasRepository,
hasDependencyGraph, hasIssueTracker, hasReleaseHistory, hasContributorAgreement.

Set hypothesis_only=true on EVERY fact. confidence 1.0 (it's
conceivable that humans have hair). anchor=null. Aim for 10-30
conceivables per major entity — the candidate space is huge.

These are flagged as hypothesis-only so downstream tooling can keep
them out of curated releases while still letting reviewers see the
question space.
""" + _OUTPUT_SCHEMA


_RECURSIVE_HEADER = """You are running the RECURSIVE aperture.

You have already extracted facts from the text. Now treat each newly
discovered entity (subjects from prior passes that you haven't
exhausted) as a fresh seed. For each seed entity, mine SURFACE-level
claims about it from the text — properties, relations, events, dates,
attributions — that you may have skipped on the first pass.

Seeds for this pass:
__SEEDS__

Anchor everything to the text. hypothesis_only=false. confidence 0.85-1.0.
""" + _OUTPUT_SCHEMA


_BY_APERTURE: dict[Aperture, str] = {
    Aperture.SURFACE: _SURFACE,
    Aperture.LINGUISTIC: _LINGUISTIC,
    Aperture.PRESUPPOSITION: _PRESUPPOSITION,
    Aperture.INFERENTIAL: _INFERENTIAL,
    Aperture.CONCEIVABLE: _CONCEIVABLE,
}


def prompt_for_aperture(aperture: Aperture, domain: Domain | None = None,
                         seeds: list[str] | None = None) -> str:
    """Return the system prompt for a given aperture.

    `domain` is reserved — when domain-specialised aperture prompts
    land, this is where they'll branch. The skeleton uses domain-agnostic
    aperture prompts; the M5 domain dispatcher still picks which
    apertures to run, but each aperture currently uses the general text.
    `seeds` is required for RECURSIVE; ignored for the others.
    """
    if aperture == Aperture.RECURSIVE:
        seed_list = "\n  ".join(f"- {s}" for s in (seeds or [])) or "(none)"
        return _RECURSIVE_HEADER.replace("__SEEDS__", seed_list)
    return _BY_APERTURE[aperture]

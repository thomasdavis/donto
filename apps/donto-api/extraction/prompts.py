"""Per-domain extraction prompts.

Each prompt includes the same candidate schema so the decomposer can
parse output uniformly. Domain-specific guidance steers the model
toward the predicates a domain reviewer expects to see.

The stubbed domains (medical, legal) reuse the general prompt; their
slot exists so the dispatch table is exhaustive and a domain owner
can drop in a real prompt without touching the dispatcher.
"""

from .domain import Domain


_CANDIDATE_SCHEMA_BLOCK = """
## OUTPUT FORMAT

Return a JSON object with a single "facts" array. Each fact:

{
  "subject": "ex:<kebab-case-subject>",
  "predicate": "<camelCase or domain-specific predicate>",
  "object": { "iri": "ex:<kebab-case>" } | { "literal": { "v": <value>, "dt": "<xsd type>" } },
  "anchor": { "doc": "<doc-iri>", "start": <int>, "end": <int> } | null,
  "hypothesis_only": <bool>,
  "confidence": <0.0-1.0>,
  "notes": "<brief justification>"
}

If the claim is not supported by an explicit text span, set
`anchor` to null AND `hypothesis_only` to true. Never invent an
anchor.
"""


_GENERAL = """You are a predicate extraction engine. Given a source text,
extract atomic (subject, predicate, object) triples for every
relationship, claim, and presupposition expressed by the text.

Use camelCase for predicates. Prefer specific predicates over generic ones.
""" + _CANDIDATE_SCHEMA_BLOCK


_GENEALOGY = """You are a genealogical claim extractor. Pull every
person, vital event, and family relationship asserted by the text.

Prefer these predicates when applicable:
  bornOn, diedOn, baptisedOn, marriedOn, fatherOf, motherOf,
  spouseOf, childOf, residedAt, occupation, citizenshipOf,
  buriedAt, immigratedTo, emigratedFrom.

Always anchor to the source span. If a claim is inferred from
context (e.g. "John (1820-1880)" implies bornOn 1820), keep the
inferred fact but lower the confidence to ≤ 0.7.
""" + _CANDIDATE_SCHEMA_BLOCK


_LINGUISTIC = """You are a linguistic claim extractor. Extract claims
about languages, phonology, morphology, syntax, lexicon, sociolinguistics,
and language relationships.

Prefer these predicates:
  hasPhoneme, lacksPhoneme, hasFeature, lacksFeature, alignsWith,
  borrowsFrom, descendsFrom, classifiedAs, spokenIn, glossedAs,
  hasMorpheme, hasOrthography.

Always anchor IPA characters and example sentences to their span.
Glottocodes belong as ex:glottolog/<code> IRIs. ISO 639-3 codes
belong as ex:iso639-3/<code>.
""" + _CANDIDATE_SCHEMA_BLOCK


_PAPERS = """You are a scientific-paper claim extractor. Extract:

  - Author claims (authorOf, affiliation, fundedBy)
  - Methodological claims (methodUsed, datasetUsed, sampleSize)
  - Findings (reports, observes, finds, contradicts, supports)
  - Citations (cites, supersedes, replicates, retractsClaim)

Findings should be tagged hypothesis_only=false only when the paper
explicitly states the result. Speculations and discussion-section
"may suggest" claims should be hypothesis_only=true.
""" + _CANDIDATE_SCHEMA_BLOCK


_MEDICAL_STUB = _GENERAL  # owned by the medical domain team — stub.
_LEGAL_STUB = _GENERAL  # owned by the legal domain team — stub.


_BY_DOMAIN: dict[Domain, str] = {
    Domain.GENEALOGY: _GENEALOGY,
    Domain.LINGUISTIC: _LINGUISTIC,
    Domain.PAPERS: _PAPERS,
    Domain.MEDICAL_STUB: _MEDICAL_STUB,
    Domain.LEGAL_STUB: _LEGAL_STUB,
    Domain.GENERAL: _GENERAL,
}


def prompt_for(domain: Domain) -> str:
    return _BY_DOMAIN[domain]

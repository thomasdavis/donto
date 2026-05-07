# donto — The Frontier (historical)

> **STATUS: SUPERSEDED.** Canonical PRD is now
> [`DONTO-V1000-PRD.md`](DONTO-V1000-PRD.md). This document is preserved
> as a historical artefact. Apply "donto" wherever the body text below
> uses an earlier project name.


> **Document type:** Research brief, vision, open-problem inventory.
> **Audience:** the principal engineer; ChatGPT Pro Research and other
> deep-research instruments; future contributors deciding whether to
> commit; reviewers deciding whether the project earns its scope.
> **Date:** 2026-05-07
> **Status:** Companion to [`V1000-REFACTOR-PLAN.md`](V1000-REFACTOR-PLAN.md).
> Where v1000 is "the next twelve months", this document is "the next
> ten years". It deliberately abstracts, generalises, and asks
> questions that the codebase cannot answer from the inside.
>
> **What this document does.** It states the thesis of Atlas Zero in
> the strongest available form, identifies what is genuinely novel
> about it, articulates the deep research questions that need outside
> input before the project commits to its long horizon, sketches a
> five-version roadmap (v2000 → v5000), enumerates the systems and
> literature the project should be in dialogue with, names the risks
> that will not be solved by engineering alone, and provides
> deep-research-ready prompts the user can hand to a literature-search
> instrument.
>
> **What this document does not do.** Specify code. The previous two
> documents do that.

---

## Table of contents

0. [The thesis, in its strongest form](#0-the-thesis-in-its-strongest-form)
1. [What is genuinely new about this](#1-what-is-genuinely-new-about-this)
2. [Why now](#2-why-now)
3. [The seventeen deep questions](#3-the-seventeen-deep-questions)
4. [The five-year horizon — v2000 through v5000](#4-the-five-year-horizon--v2000-through-v5000)
5. [Systems we should be in dialogue with](#5-systems-we-should-be-in-dialogue-with)
6. [The literature](#6-the-literature)
7. [Risks the engineering does not solve](#7-risks-the-engineering-does-not-solve)
8. [Deep-research prompts (paste-ready)](#8-deep-research-prompts-paste-ready)
9. [What success looks like, in concrete terms](#9-what-success-looks-like-in-concrete-terms)
10. [The closing thought](#10-the-closing-thought)
11. [Appendix: a glossary for outsiders](#11-appendix-a-glossary-for-outsiders)

---

## 0. The thesis, in its strongest form

> Atlas Zero is an **evidence operating system for contested
> knowledge**. It is what comes after the database, the wiki, the
> archive, and the knowledge graph — all four of which presuppose a
> kind of consensus that does not exist in any real corpus.

A database stores values. A wiki stores edits. An archive stores
artefacts. A knowledge graph stores triples. None of them, as a class,
preserves disagreement as data, anchors every assertion to its
evidence, separates machine confidence from scholarly truth, models
governance as part of the schema, supports time-travel over both
"when was this claimed" and "when was this true", or treats schema
itself as something to be aligned across rather than imposed.

The substrate that does all of those things is the substrate that
linguistics, medicine, law, intelligence analysis, science, history,
ethnography, and cultural heritage already need but do not have. Each
of those domains has built one-off systems that solve a subset.
Atlas Zero, built on donto, is the first attempt to specify and ship
the whole substrate as a single coherent thing.

The choice of linguistic evidence as the first proving ground is
deliberate. Linguistics has the richest combination of stress tests
in any single domain: thousands of incompatible analytical schemas
(WALS, Grambank, AUTOTYP, UD, UniMorph, GOLD, OLiA, OntoLex-Lemon),
sources that disagree more often than they agree, fundamental
identity ambiguity (which language is this even?), media types that
break flat schemas (interlinear glossed text, signed-language video,
field recordings with overlapping speakers), and a population of
data that frequently must remain restricted under cultural protocols
that no general-purpose system models. If donto can serve linguistic
evidence well, it can serve every other contested-knowledge domain.

---

## 1. What is genuinely new about this

Be honest with yourself: not many systems are actually novel. Most
"new" systems are recombinations. Atlas Zero combines existing
elements; what is novel is the **combination**, the **invariants
that hold across all of them**, and a few specific design moves
that have not been made together before.

### 1.1 The combination claim

Atlas Zero is the first system this author is aware of that combines
all of the following as **first-class, non-removable** primitives:

1. **Bitemporal storage** — both `valid_time` and `tx_time`.
2. **Paraconsistency** — contradictions are preserved as evidence,
   never silently flattened.
3. **Context-scoped statements** — every claim lives in at least one
   named scope (source / hypothesis / dialect / corpus / project).
4. **Open-world identity hypotheses** — language identity, lexeme
   identity, and morpheme identity all carry split / merge candidate
   semantics, not foreign keys.
5. **Predicate alignment with rich relation types** — exact / close /
   broad / narrow / decomposes-to / has-value-mapping / inverse /
   incompatible / derived-from / local-specialization, with closure
   and per-relation export semantics.
6. **Evidence anchors at every granularity** — char span, page
   bounding box, image bbox, media time, ELAN tier annotation, table
   cell, CSV row, corpus token, gloss line, archive record field.
7. **Modality dimension** separate from polarity and from confidence:
   descriptive / prescriptive / reconstructed / inferred / elicited /
   corpus-observed / typological-summary.
8. **Extraction levels** as epistemic acts: quoted / table-read /
   example-observed / source-generalization / cross-source-inference
   / model-hypothesis / human-hypothesis.
9. **N-ary analysis frames** — paradigm cells, allomorphy rules,
   construction templates, IGT examples, valency frames, identity
   hypotheses — all preserved n-ary, not flattened.
10. **Maturity ladder** that can only be climbed by earning evidence,
    review, and proof — never assigned.
11. **Argumentation graph** — supports / rebuts / undercuts /
    qualifies / alternative-analysis-of / same-evidence-different-
    analysis / same-claim-different-schema.
12. **Proof obligations** — open epistemic work as a queryable backlog.
13. **Validation as overlay, not gate** — shape failures produce
    annotations and obligations, not rejections.
14. **Lean-checkable certificates** for the strongest claims.
15. **Access governance with attestation, inheritance, audit** — not
    a permission table; a protocol.
16. **Releases as reproducible views** with checksum manifests and
    policy reports — not snapshots, not exports.
17. **Domain dispatch** for extraction prompts and decomposers — same
    engine, different epistemic shapes per domain.

Pick any other knowledge-graph or evidence system in production. It
will lack between four and twelve of these primitives. The combination
is the system.

### 1.2 The specific novel moves

A handful of specific design moves are, to the author's reading,
new (or at least extremely uncommon):

**M1. Predicate alignment as a first-class layer with closure
expansion at query time.** Most systems either pick a canonical
ontology (and lose source schemas) or refuse to align (and lose
cross-source query). The PAL approach — register every alignment with
a typed relation, materialise the closure, expand at query time
according to caller's safety preference (`STRICT` / `EXPAND` /
`EXPAND_ABOVE n`) — preserves both source schemas and cross-schema
queryability. It also explicitly distinguishes between alignments
that are safe for query expansion, safe for export, and safe for
logical inference, so a typological close-match can drive retrieval
without licensing logical conclusions.

**M2. Extraction levels as a separate dimension from confidence.**
A claim that the LLM extracted by reading a quoted sentence in the
source ("the language has five vowels") is epistemically different
from a claim the LLM proposed by analogy ("plausibly, this language
also has vowel harmony"). Both might have confidence 0.85. They are
not interchangeable. Atlas Zero stores the extraction level and
binds auto-promotion to it: model hypotheses cannot reach M3 without
human review, no matter how high their machine confidence.

**M3. Modality as a stored property of every claim.** "The language
has accusative alignment" is a *typological summary*. "The man saw
the dog" is a *corpus observation*. "Proto-X had ergative alignment"
is a *reconstructed* claim. Most systems collapse these into one
truth. Atlas Zero treats modality as orthogonal — you can ask
"reconstructed-only", "corpus-only", "elicited-only" as routine
queries.

**M4. Identity hypotheses as multiple coexisting worlds.** Strict /
likely / exploratory clusters (from `donto_identity_hypothesis`) let
a researcher run analyses under each independently. Most systems
have one entity-resolution result; Atlas Zero has as many as the
researcher needs.

**M5. Access governance with `train_model` as a separate action.**
Read permission ≠ training permission. Quote permission ≠ export
permission. Most platforms have a single "access" axis. Atlas Zero's
policy explicitly enumerates `read_metadata`, `read_content`,
`quote`, `export_claims`, `train_model`, `publish_release` — and
inherits the maximum restriction across derived data.

**M6. Attestation, not a session.** A caller does not log in and
gain access. The caller presents an *attestation* granted by a
specific authority for a specific policy with a specific rationale,
expirable, revocable, audit-logged. This models the way Indigenous
data governance, ELAR-style archive access, and ethics-board-bound
medical research actually work, rather than the way most platforms
pretend they work.

**M7. Releases as reproducible views, not exports.** A release is
a query plus a policy report plus a checksum manifest. Re-running
the query against unchanged source state produces the same release
(content-stable). The release artefact is citable; the database
remains mutable; the view is the contract.

**M8. Lean overlay for the strongest claims.** Most knowledge graphs
have no concept of "machine-checkable proof". Atlas Zero promotes
to certified maturity only when a Lean shape passes — paradigm
completeness, IGT alignment, allomorph environment consistency,
phonotactic regularity. Most claims will never reach this tier;
some will, and they are categorically more trustworthy.

**M9. Anchor-kind taxonomy.** Most systems have one kind of evidence
link (text span). Atlas Zero has ten or more, each with its own
locator schema and its own validator. Page bounding box has page +
bbox; ELAN tier annotation has tier ID + annotation ID + time span;
gloss line has IGT block + line number + morpheme index. The
locator schema is checked at write time.

**M10. Predicate minting as a refused-by-default operation.** A new
predicate cannot be minted without a descriptor (label, gloss,
domain, range, examples), an embedding, and a nearest-neighbour
search confirming the closest existing predicate is below threshold.
This is the single biggest mitigation for vocabulary explosion in
multi-extractor environments and is, to the author's knowledge,
unique to this design.

### 1.3 The negative claim

What Atlas Zero is *not*:

- It is not a foundation model and does not train one. It uses
  off-the-shelf LLMs (Grok 4.1 Fast via OpenRouter today; offline
  models for restricted material in v1010) and treats their outputs
  as candidate claims requiring evidence and review, not as truth.
- It is not a replacement for Glottolog, ISO 639-3, WALS, Grambank,
  UD, UniMorph, PHOIBLE, ELAN, or any existing archive. It is the
  substrate that integrates them.
- It is not a single ontology. There is no canonical predicate
  vocabulary; predicates are aligned across schemas, not unified.
- It is not a publication system. It is the substrate publication
  systems can sit on top of.
- It is not a theorem-prover. Lean is an overlay for shape validation;
  most claims never reach the formal layer.
- It is not a federated network in v1000. Federation is v5000.

### 1.4 Domain generality

Linguistics is the proving ground. The substrate generalises. A
sketch of how it serves four other domains:

- **Medicine.** Sources disagree (different studies on same
  population). Schemas multiply (UMLS, SNOMED CT, ICD, OMOP).
  Evidence is granular (specific chart entry, specific time stamp).
  Identity is contested (cohort definition, patient identity under
  pseudonymisation). Governance is mandatory (HIPAA, GDPR, IRB
  protocols). Reproducibility is essential.
- **Law.** Sources disagree (precedents from different
  jurisdictions). Schemas multiply (Eurovoc, USC, UNCITRAL). Evidence
  is hierarchical (case → opinion → paragraph → sentence). Identity
  is contested (corporate succession). Governance is mandatory
  (privilege, sealed records). Reproducibility is essential
  (citations).
- **Intelligence analysis.** Sources disagree by definition. Schemas
  are bespoke per agency. Evidence is restricted by clearance.
  Identity is the entire problem. Governance is paramount. Multiple
  competing hypotheses must coexist.
- **Cultural heritage.** Sources disagree across communities. Schemas
  multiply (Dublin Core, CIDOC CRM, Europeana Data Model). Evidence
  is multimedia. Identity is a political question. Governance is
  CARE-bound. Reproducibility supports the public record.

Atlas Zero is described in linguistic terms because that is what
v1000 ships. The substrate is domain-agnostic. The architecture is
domain-extensible.

---

## 2. Why now

The combination is feasible only now, for five reasons:

1. **LLM extraction at near-zero marginal cost.** Grok 4.1 Fast,
   Claude Haiku 4.5, GPT-4 Turbo class models cost $0.001–$0.01 per
   article extraction. This was $1+ in 2023. The economics of
   "extract everything from every source" only work below a few
   cents per chunk.

2. **Postgres has caught up.** pgvector, JSONB GIN indexes, generated
   columns, tstzrange and daterange types, partial unique indexes,
   recursive CTEs, FTS with trigram. A single Postgres node can hold
   the substrate at the scale we care about for years before
   partitioning.

3. **CLDF and Glottolog have matured.** Five years ago there was no
   shared linguistic data format; now there is. Importing Grambank,
   WALS, PHOIBLE, AUTOTYP, ValPaL, APiCS, SAILS as a single
   conventional pipeline is finally possible.

4. **Indigenous data governance has crystallised.** CARE Principles
   (2020), Local Contexts TK / BC Labels, AIATSIS Code of Ethics
   updates, OCAP. The frameworks are stable enough to design
   against.

5. **Lean and dependent types are tractable.** mathlib has hundreds
   of contributors. Lean 4 is fast enough for production validation.
   Five years ago, theorem-prover-backed shapes for a real database
   would have been a research project; now it is an overlay.

The window for shipping this substrate is open. Whoever ships first
defines the category.

---

## 3. The seventeen deep questions

These are the questions the codebase cannot answer from the inside.
ChatGPT Pro Research, expert consultations, and a literature review
should attempt each. Each is followed by why it matters and how the
answer would change the design.

### Q1. What does the production track record of paraconsistent reasoning over real corpora look like?

Paraconsistent logic has a literature. Production deployments of
paraconsistent reasoning over multi-source data are rare. We need to
know: who has actually done this, what scale, what failure modes,
what query patterns are useful, what query patterns explode.

*Why it matters:* If the literature shows paraconsistency-as-data
working well in production, our v2000 query planner should embrace
it. If the literature shows it producing degenerate query semantics
under common operations, we need to pre-design escape hatches.

*Likely sources:* da Costa school papers, Belnap's four-valued logic
deployments, Jaśkowski's discussive logic in clinical decision
support, Annotated Predicate Logic in databases.

### Q2. How is Indigenous data governance actually implemented technically, beyond Mukurtu CMS?

Mukurtu is the most-cited example. We need a wider survey: TK label
implementations in archives, ELAR's actual access enforcement
architecture, AIATSIS' digital protocols, OCAP-compliant systems in
Canadian First Nations health research, Kanaka Maoli archive systems,
Aboriginal-language teaching platforms with cultural protocols.

*Why it matters:* The design in §3.15 of the v1000 plan is informed
by Mukurtu and Local Contexts. There may be patterns from other
implementations we should learn from before committing to the
schema.

*Likely sources:* CARE Principles paper (Carroll et al, 2020), TK
Labels documentation, Mukurtu architecture papers, ELAR governance
documentation, OCAP technical guides.

### Q3. What is the state of cross-CLDF-database querying?

CLDF standardises a per-dataset format. CLLD (the host) aggregates
public datasets. But querying across datasets — "give me every
language for which Grambank GB148 = 1 AND PHOIBLE inventory contains
/p/" — is not a routine operation. Why not? Has anyone built it?
What would it require?

*Why it matters:* This is the use case Atlas Zero's M5 (structured
importers) plus PAL plus query language enables. If the linguistic
community has already built and abandoned this, we should know why.

*Likely sources:* CLDF spec papers, Lexibank papers, Glottolog
architecture papers, recent linguistic databases workshops at LREC /
ACL / EACL, Forkel et al's Lexibank/CLDF tooling.

### Q4. What did Universal Dependencies sacrifice to achieve cross-linguistic consistency, and how do those sacrifices interact with multi-source preservation?

UD's design decisions are well-documented but sometimes controversial.
Some communities reject UD's choices for their language. We need to
understand: which decisions are universally accepted, which are
contested, where treebanks for the same language disagree on UD
analysis, what kind of cross-source preservation UD enables vs.
forecloses.

*Why it matters:* If we ingest UD treebanks and treat them as ground
truth corpus annotation, we inherit UD's editorial decisions. Atlas
Zero should support disagreement with UD (alternative-analysis frames),
which means understanding where disagreement is most common.

*Likely sources:* UD design papers, contested-treebank case studies,
Hajič et al on UD's evolution, UD V2 versus V1 changes.

### Q5. What is the realistic state of OntoLex-Lemon adoption?

OntoLex-Lemon is the standard. Standards are not always adopted.
We need to know: which lexicons are actually published in OntoLex?
What are common deviations? What are the criticisms? Are there
better alternatives in active use?

*Why it matters:* §A.2 of the v1000 vocabulary uses OntoLex as
default. If most field linguists use LIFT (and they do), our LIFT
adapter is the load-bearing piece, not the OntoLex layer.

*Likely sources:* OntoLex-Lemon W3C community group documentation,
Lemon Cookbook, recent LREC papers on lexical linked data,
DELA / DBnary / lemon-uby projects.

### Q6. How do existing endangered-language documentation systems (ELAN, FieldWorks/FLEx, Toolbox, SayMore, Mukurtu) actually fit together in working linguists' workflows?

The systems exist. The integration is allegedly painful. Atlas Zero
could be: a replacement (very ambitious), a complement (likely),
the substrate underneath them (most useful). We need to know what
actually happens in working linguistic labs.

*Why it matters:* Adoption strategy. If linguists already have
working pipelines with ELAN + FLEx, we add value by being the
analysis substrate downstream, not by replacing those tools.

*Likely sources:* recent endangered-language documentation
methodology papers, Austin / Sallabank handbook chapters, language
documentation conference talks.

### Q7. Is there a credible technical path from per-instance Atlas Zero to a federated network?

SPARQL federation barely works. Solid pods barely work. ActivityPub
has limits. The federation question is hard. But sometimes a
specific-domain federation works (cf. ORCID, DataCite). We need to
know: what would federation look like for evidence systems
specifically? Has anyone tried? What were the failure modes?

*Why it matters:* v5000 in §4 below is "community-owned federated
network". If the federation question is unsolvable, that vision is
an aspiration, not a roadmap.

*Likely sources:* Linked Data Platform spec, Solid spec, ActivityPub
federation post-mortems, Verifiable Credentials work, decentralised
identifier specs (DIDs).

### Q8. What are the production-grade entity-resolution systems actually doing, and which ideas apply to language-variety / lexeme / morpheme resolution?

Splink, Magellan, Dedoop, Senzing, Zingg. Production ER. Real
research. We have a basic ER layer (donto migrations 0057–0061) and
a sketch (`docs/ARCHITECTURE-REPORT.md`). The deeper question is:
what ideas from production ER apply to *linguistic* resolution
(where the entities are language varieties, lexemes, morphemes,
phonemes)?

*Why it matters:* Linguistic entity resolution is harder than
person-name resolution because the granularity is contested. We
need ideas, not just code.

*Likely sources:* Splink papers (Robin Linacre), Magellan
(Konda et al), Senzing technical papers, recent SIGMOD ER
tutorials.

### Q9. What is FrameNet's production track record, and is frame semantics the right model for our analysis frames?

FrameNet has been around for 30 years. Adoption outside English NLP
is limited. PropBank and AMR are alternatives. Our analysis frames
(allomorphy_rule, paradigm_cell, IGT, valency_frame) feel
FrameNet-shaped. Are we right? What are the failure modes of
frame-semantic representations at scale?

*Why it matters:* Choice of n-ary representation affects every
downstream query and shape. If frame semantics has known issues
that bite at scale, we should know.

*Likely sources:* Berkeley FrameNet papers, MultiWordNet, Cross-
Lingual FrameNet projects, AMR papers (Banarescu et al), critiques
of frame semantics from cognitive linguistics.

### Q10. How do health-care vocabularies (UMLS, SNOMED CT, ICD, OMOP) actually do schema alignment at production scale?

These are the biggest deployed cross-vocabulary alignment systems
in the world. They predate our PAL by decades. What do they actually
do? What are the documented failure modes? What patterns transfer?

*Why it matters:* PAL inherits from W3C SKOS plus a few additions.
Healthcare vocabularies have evidence over far more diverse and
contested mappings. We can learn.

*Likely sources:* UMLS Metathesaurus papers, SNOMED CT's mapping
to ICD, OHDSI's OMOP common data model papers, BioPortal
mapping infrastructure documentation.

### Q11. What is the state of the art in temporal natural-language reasoning beyond Allen relations?

Allen relations are 1983. EDTF, ISO 8601, TimeML, UTime, fuzzy
interval calculi all came later. What does production-grade
temporal NL reasoning look like? Where do the standards live?
What handles "circa 1850 or possibly earlier" gracefully?

*Why it matters:* `donto_temporal_relation` (0064) and
`donto_time_expression` (0063) plant flags but do not implement
modern temporal NL reasoning. v2000 should.

*Likely sources:* James Pustejovsky's TimeML/TimeBank work, EDTF
spec, ISO 8601 extensions, fuzzy interval logic literature, recent
temporal QA benchmarks (TimeQA, TempLAMA).

### Q12. Has anyone built a system that combines paraconsistent logic, bitemporal reasoning, and human-in-the-loop review?

Each of the three has its own literature. The triple combination is
the Atlas Zero claim to novelty. Independent verification: who has
combined them? What can we learn?

*Why it matters:* If the answer is "nobody", we need to prove the
combination works. If the answer is "yes, in domain X", we should
study domain X first.

*Likely sources:* Datomic / XTDB documentation on bitemporal
reasoning, paraconsistent KG papers, INCEpTION / Argilla / Prodigy
on HITL review at scale.

### Q13. What are the realistic adoption paths for linguistic-evidence platforms in actual research workflows?

Atlas Zero will be useful only if linguists adopt it. Adoption
patterns in linguistics: what's worked (ELAN, FLEx), what hasn't
(SHEBANQ, BRAT for many groups), and why. Our adoption strategy
should be informed by this history.

*Why it matters:* Build-it-and-they-will-come is wrong. We need
to understand the actual constraints field linguists, descriptive
linguists, and computational linguists face.

*Likely sources:* "Why isn't tool X used more?" panels at
documentation linguistics conferences, archived adoption studies,
LSA / ALT / SLE community discussions.

### Q14. Differential privacy and trusted execution environments — is there a credible path for restricted-data extraction?

For data that cannot leave a community's control but where the
community wants to enable extraction by external parties:
differential privacy on queries? Trusted execution environments
(SGX, AMD SEV, Apple Secure Enclave)? Federated learning over
KGs? What's actually feasible?

*Why it matters:* §3.15 access governance enforces "you can't see
this". A more powerful capability would be "you can run extraction
over this without the data leaving its enclave". v3000 territory.

*Likely sources:* DP-SQL / SmartNoise / OpenDP papers, federated
analytics on graph data, TEE-based data sharing systems, the DARPA
Brandeis program.

### Q15. What actually breaks in CLDF / UD / UniMorph round-trip in practice?

Atlas Zero promises round-trip via M9 release builder + M5
importers. Round-trips are notoriously fragile. The CLDF and UD
communities have run round-trip tests; what are the known breakages?

*Why it matters:* If we promise round-trip and round-trip is
intrinsically lossy in the source format, we need to document the
exact information that survives and the exact information that does
not, before we ship.

*Likely sources:* CLDF round-trip discussions on GitHub, UD V2
migration documentation, UniMorph schema evolution notes.

### Q16. How do other domains with maturity ladders (medicine's evidence pyramid, law's evidence rules, intelligence's confidence levels) compare, and what do they get right?

L0–L4 (now M0–M5) is the donto / Atlas Zero ladder. Medical
evidence has a six-tier pyramid (case report → cohort → RCT →
systematic review → meta-analysis → guideline). Legal evidence
distinguishes hearsay / circumstantial / direct / forensic.
Intelligence uses words-of-estimative-probability. What does each
domain get right that we're missing?

*Why it matters:* Cross-pollination. Other domains have spent
decades formalising claim maturity. We should not reinvent.

*Likely sources:* Cochrane Collaboration evidence rating, Federal
Rules of Evidence, Sherman Kent's words of estimative probability,
GRADE working group on evidence assessment.

### Q17. What is the right unit of "release" — a CLDF dataset, an RO-Crate, a Software Heritage ID, a DOI-stamped PURL, a content-addressed IPFS hash?

Release as reproducible view (§3.17 of v1000) is content-stable.
The artefact format is undecided. What do existing scientific data
publication frameworks recommend? What's actually citable a decade
later?

*Why it matters:* M9 release builder ships native JSONL, CLDF,
CoNLL-U, UniMorph TSV, RO-Crate. Each has trade-offs. We should
know which actually survives long-term.

*Likely sources:* Software Heritage architecture, RO-Crate
adoption papers, FAIR Principles paper (Wilkinson et al, 2016),
DataCite metadata schema documentation, IPFS in research data.

---

## 4. The five-year horizon — v2000 through v5000

Versions cluster around capability tiers, not calendar dates.
Names rather than years.

### 4.1 v2000 — Planetary scale

Trigger: 100M+ statements in a single instance, query latency starts
to bite.

Capabilities:

- **Partitioned storage** of `donto_statement` by `tx_lo` month and
  by `predicate` family, per the ARCHITECTURE-REPORT recommendation.
- **Query planner** with EXPLAIN. PRESET resolution gets cost-based.
  Closure expansion has join-order optimisation.
- **Bloom filters per context** for fast access-policy precheck.
- **Materialised release shadows** for the most common scope queries
  (e.g., "all curated, asserted, L3+ claims about language X").
- **Vector ANN at scale** — pgvector with HNSW or IVFFlat at the
  predicate-descriptor and span-embedding level.
- **Streaming ingestion** for sources that drip (newspaper feeds,
  audio archive ingest, social-media research corpora).

Open question: is partitioning by `predicate` family compatible
with closure expansion? PAL crosses predicate families. Possibly
materialise predicate-family-cross indexes.

### 4.2 v3000 — Self-improving extraction

Trigger: 10M+ reviewer decisions are in the system; reviewer
feedback signal becomes valuable training data.

Capabilities:

- **Reviewer-decision distillation** into per-domain extractor
  preferences (not training the LLM; training a re-ranker over
  candidate facts).
- **Active learning hooks** — extractor preferentially queries
  for review the candidates whose acceptance / rejection is most
  informative.
- **Local / offline extraction** using small distilled models for
  restricted-data jobs (where `train_model = false` precludes
  external LLM calls).
- **Self-explaining LLM prompts** — prompts evolved by reviewer
  decisions to ask for the kind of evidence reviewers actually
  approve.
- **Confidence calibration** — confidence-by-domain calibrated
  against reviewer acceptance rates so 0.85 means "80% accepted"
  rather than "model thinks 85%".

Open question: does reviewer-decision distillation reproduce
reviewer biases at scale? Yes. So calibration must include
demographic / institutional / methodological metadata about
reviewers, not just acceptance rates.

### 4.3 v4000 — Cross-domain substrate

Trigger: linguistic v1000 → v3000 has matured; medical, legal,
scientific, and historical adopters approach.

Capabilities:

- **Domain plugins** — clean, documented protocol for new domains:
  predicate vocabulary registry, frame-type registry, prompt
  catalogue, decomposer, ingest adapters, validation shapes,
  release format.
- **Multi-domain release builder** — releases can span domains
  (e.g., "everything about a 19th-century immigrant family from a
  given community" combining genealogy, linguistic-of-letters,
  medical-of-vital-records, historical-of-newspapers).
- **Cross-domain entity resolution** — a person named in a
  genealogical source and a linguistic field-recording is the same
  person under specific identity-hypothesis contexts.
- **Domain-specific Lean shapes**: medical (drug-drug interaction
  consistency), legal (citation-validity), scientific (statistical
  test applicability).
- **Federated review** — reviewers from different domains and
  institutions can collaborate on overlapping content with role
  separation.

Open question: does domain mixing dilute the substrate's identity?
Risk: Atlas Zero becomes "the database for everything" and loses
clarity. Mitigation: domain plugins are first-class, but the
substrate's invariants (paraconsistency, bitemporal, evidence
required, governance enforced, releases reproducible) are non-
negotiable across domains.

### 4.4 v5000 — Federated network

Trigger: multiple Atlas Zero instances exist; demand for cross-
instance query.

Capabilities:

- **Federated identity** for varieties / lexemes / morphemes /
  predicates / agents — a global ID space with local namespaces.
- **Signed attestations** for cross-instance access. A Yolŋu-language
  community's instance can grant a researcher's instance access
  under specific protocols, recorded in signed attestation chains.
- **Federated query** — a DontoQL query against multiple instances
  with respect for each instance's policy.
- **Consensus releases** — releases that span multiple instances
  with manifest stability across the federation.
- **Disagreement at the federation level** — instance X says feature
  F = 1; instance Y says feature F = 0. Federated query returns
  both with provenance.
- **Federated Lean** — proof obligations and certificates can cross
  instances under shared shape vocabularies.

Open question: federation is hard. Most attempts at federated
SPARQL or Solid-style federated linked data have not reached
critical mass. Federation might be the right v5000 vision and
might be achievable as v9000 reality. We should pursue it but not
bet on it.

### 4.5 What sits beyond v5000

- **Atlas Zero in epistemically novel domains.** Cosmology
  (paraconsistency over disagreeing observations), economics
  (paraconsistency over conflicting indicators), philosophy of
  science (paraconsistency over disagreeing theories). The
  substrate is general; the application space is large.

- **Atlas Zero as research infrastructure.** Funded as
  infrastructure rather than as a project. The CLLD / Glottolog /
  Concepticon / WALS hosts are research infrastructure; Atlas Zero
  could be a successor / extension to them.

- **Public, reviewable, machine-checkable knowledge releases as
  citable artefacts.** "Per Atlas Zero release 2031.4.7" becomes a
  legitimate citation form, in the way that "per Glottolog 5.3"
  has become.

---

## 5. Systems we should be in dialogue with

Not "competitors". Not "ancestors". *Dialogue partners* — systems
we should compare against, learn from, possibly partner with.

### 5.1 Knowledge graph platforms

| System | Why we care |
|---|---|
| **Wikidata** | Largest open KG. Provenance via `references`. No paraconsistency; canonical statements. Models we use the alignment layer to integrate, but never to imitate. |
| **DBpedia** | Wikipedia-derived. Less curated. Schema mapping at scale. Worth studying for its limitations. |
| **BabelNet** | Multilingual lexical-conceptual graph. Aligns WordNets. Worth studying for its alignment heuristics. |
| **ConceptNet** | Crowd-sourced common sense. Different epistemic posture. Worth studying for its handling of low-confidence claims. |
| **Wolfram Knowledge Engine** | Curated, theorem-prover-adjacent. Different posture. Worth studying for its curation discipline. |
| **Datomic** | Bitemporal, immutable. Closest substrate-relative. Worth deep study. |
| **XTDB** | Bitemporal, open-source. Closer relative still. Direct prior art for bitemporal reasoning at scale. |

### 5.2 Linguistic platforms and aggregators

| System | Why we care |
|---|---|
| **CLLD** (Cross-Linguistic Linked Data) | Hosts the comparative databases. Designed before our generation of tooling. Atlas Zero ingests CLLD-hosted content; the relationship is friendly. |
| **Glottolog** | Language registry. We adopt its identifiers. We do not replace it. |
| **Lexibank** | Lexical comparative-data tooling. Ingest target. |
| **Concepticon** | Concept registry. Ingest target. |
| **CLICS** | Colexification. Ingest target. |
| **Universal Dependencies** | Token-level standard. Ingest target. We will preserve UD analyses while supporting alternatives. |
| **UniMorph** | Morphological standard. Ingest target. |
| **PHOIBLE** | Phonological standard. Ingest target. |
| **WALS / Grambank / AUTOTYP / ValPaL / APiCS / SAILS** | Typological databases. Ingest targets. |
| **Mukurtu** | Indigenous archive CMS. Closest relative on the governance axis. Worth deep study. |
| **ELAR** | Endangered Languages Archive. Reference for governance protocols. |
| **PARADISEC** | Pacific archive. Reference for archive workflow. |
| **AIATSIS Austlang** | Australian Indigenous-language registry. Reference for registry governance. |

### 5.3 Documentation tools

| System | Why we care |
|---|---|
| **ELAN** | Time-aligned annotation. Ingest target. |
| **FieldWorks (FLEx)** | Comprehensive linguistic database. Ingest target via LIFT. |
| **Toolbox / Shoebox** | Older linguistic database. Legacy ingest. |
| **Praat** | Phonetic analysis. Ingest target via TextGrid. |
| **SayMore** | Workflow tool. Ingest target. |

### 5.4 Annotation and review platforms

| System | Why we care |
|---|---|
| **INCEpTION** | Web-based annotation. Reference for review UI. |
| **Argilla** | LLM-data review. Reference for human-in-the-loop. |
| **Prodigy** | Active learning. Reference for active-learning hooks. |
| **Snorkel** | Weak supervision. Reference for noisy-label aggregation. |
| **BRAT** | Older annotation tool. Reference for what doesn't scale. |

### 5.5 Provenance and reproducibility

| System | Why we care |
|---|---|
| **PROV-O** | W3C provenance ontology. Export target. |
| **RO-Crate** | Research object packaging. Export target. |
| **Software Heritage** | Long-term code preservation. Reference for long-term release stability. |
| **DataCite** | Dataset citation. Reference for release metadata. |
| **FAIR data** | Principles. Adopted. |
| **CARE Principles** | Indigenous data governance. Adopted. |

### 5.6 Theorem-prover-adjacent systems

| System | Why we care |
|---|---|
| **Lean / mathlib** | The substrate of our certificate layer. |
| **Coq / Rocq** | Comparable. Some prior KG work. |
| **Isabelle/HOL** | Comparable. Less directly applicable. |
| **Agda** | Comparable. Less directly applicable. |

---

## 6. The literature

A reading list for the project's research arm. Skim is fine for
many; depth is essential for some.

### 6.1 Foundational books

- **"Knowledge Graphs"** — Hogan, Blomqvist, Cochez, d'Amato,
  de Melo, Gutierrez, Kirrane, Labra Gayo, Navigli, Neumaier,
  Ngomo, Polleres, Rashid, Rula, Schmelzeisen, Sequeda, Staab,
  Zimmermann (2021). The current standard reference.
- **"Linguistic Fieldwork: A Practical Guide"** — Bowern (2008).
  How linguistic data actually gets collected.
- **"Endangered Language Documentation"** — Austin, Sallabank
  (eds., 2011). Documentation methodology.
- **"Documenting Lexical Knowledge"** — Bowern, Evans, eds. The
  state of dictionary-making.
- **"The Probability of Inductive Inference"** — Carnap (1950).
  Foundational on confidence semantics.
- **"Evidence and Inquiry"** — Haack. Philosophy of evidence.
- **"Probabilistic Reasoning in Intelligent Systems"** — Pearl
  (1988). Core for confidence propagation.
- **"Provenance: An Introduction"** — Cheney, Chiticariu, Tan
  (2009). Provenance fundamentals.

### 6.2 Foundational papers

- **CARE Principles** — Carroll et al, Data Science Journal 2020.
  The Indigenous data governance reference.
- **FAIR Principles** — Wilkinson et al, Scientific Data 2016. The
  data reuse reference.
- **CLDF** — Forkel et al, Scientific Data 2018. The cross-
  linguistic data format reference.
- **Universal Dependencies** — Nivre et al, LREC 2016 onwards.
  The treebank standard.
- **UniMorph** — Sylak-Glassman et al, ACL 2015 onwards. The
  morphological standard.
- **PHOIBLE** — Moran, McCloy. The phonological inventory
  reference.
- **OntoLex-Lemon** — McCrae et al, W3C community group. The
  lexical linked data standard.
- **PROV-O** — W3C 2013. The provenance standard.
- **Allen interval algebra** — Allen 1983. The temporal reasoning
  classic.
- **Belnap four-valued logic** — Belnap 1977. Foundational
  paraconsistency.
- **da Costa / Krause / Béziau** — paraconsistent logic for
  practical systems.
- **Datomic / XTDB papers** — bitemporal reasoning at scale.

### 6.3 Recent technical papers worth tracking

- LREC / EACL / ACL papers on linguistic linked data.
- ISWC / ESWC papers on knowledge graph alignment.
- SIGMOD / VLDB papers on entity resolution at scale.
- ICALP / LICS papers on paraconsistent logic in databases.
- SIGCSE / SIGSE papers on large-scale annotation.
- Documentation linguistics conference proceedings (Hawaii,
  3L Summer School).
- Endangered language archive workshop proceedings.

### 6.4 Documentation we should produce

- A LANGUAGE-EXTRACTION-PRACTICE-GUIDE.md (parallel to GENEALOGY-
  GUIDE.md).
- An ETHNOGRAPHY-EXTRACTION-PRACTICE-GUIDE.md (cultural
  heritage).
- A GOVERNANCE-PROTOCOLS.md (CARE / AIATSIS / OCAP / ELDP detailed
  alignment).
- A FOR-COMMUNITY-AUTHORITIES.md (one-page explanation in plain
  language for community partners).
- A CITATION-GUIDE.md (how to cite an Atlas Zero release).

---

## 7. Risks the engineering does not solve

Engineering discipline solves engineering problems. The risks below
are not engineering problems and will not be solved by writing more
code.

### 7.1 The reviewer-bottleneck risk

If extraction outpaces review by 100×, the database fills with
unreviewed L1 claims and the maturity ladder becomes meaningless.

Mitigation lives in incentive design (what makes review rewarding),
not engineering. Possible interventions: reviewer-attestation
chains that make reviewers' calls citable; reviewer-rotation
policies that prevent burnout; community-curation models for
domains with active communities; reviewer-pay schemes for
institutional adopters.

### 7.2 The adoption-curve risk

Linguists already have ELAN + FLEx + their personal spreadsheet.
Atlas Zero's value is not obvious until the ecosystem reaches
critical mass.

Mitigation: integrate with rather than replace. Atlas Zero as the
analysis substrate downstream of FLEx, with bidirectional sync, is
adopt-able. Atlas Zero as a replacement for FLEx is not.

### 7.3 The cultural-protocol misuse risk

Access governance encodes cultural protocols. If the encoding is
wrong, the system's authority around restricted material becomes
liability. A platform can be technically correct and ethically
indefensible.

Mitigation: every governance design decision must be reviewed by
people from the relevant communities, not just by engineers who
read CARE Principles. Concrete: each adopter community has a
governance liaison; v1000 ships with a published governance
review process; the project does not market Atlas Zero to
communities until a governance steering group is in place.

### 7.4 The extraction-bias risk

Off-the-shelf LLMs have known biases. Their extractions embed those
biases. Reviewers reduce but do not eliminate this.

Mitigation: bias audits per release. The release builder includes
a reviewer-demographic and source-language summary. Releases
flagged as having insufficient reviewer diversity are non-public.

### 7.5 The vendor-LLM risk

Atlas Zero v1000 depends on OpenRouter / Grok / Claude. If those
APIs change pricing or terms, the extraction pipeline breaks.

Mitigation: support multiple LLM providers from day one (already
designed in). Local / offline models for restricted-data jobs.
Roadmap to an Atlas Zero open-extraction-model project, perhaps
v3000, that does not require any external API.

### 7.6 The "knowledge graph" market positioning risk

Knowledge graph platforms are crowded. Atlas Zero risks being
read as "another KG" and dismissed. The positioning is *evidence
operating system for contested knowledge* — a different category.
The risk is that the category is not legible to potential adopters.

Mitigation: position via concrete use cases (linguistic-evidence
platform first, governance-aware research platform second), not
via category labels.

### 7.7 The dependency-on-Postgres risk

Atlas Zero is Postgres-shaped. Postgres is excellent. But a few
decisions (paraconsistency, bitemporal indexing, JSONB
constraints, content-hash dedup) bind us. Migration to a different
RDBMS would be a multi-year project.

Mitigation: keep abstractions clean (the SQL functions are the
abstraction layer). Avoid extension-specific features where
standard SQL works. Document the Postgres dependencies so a future
migration is at least a known-cost project.

### 7.8 The "predicate explosion" risk despite mitigations

`donto predicates mint` refuses without descriptors. Reality: under
extraction load, descriptors get filled in by the LLM with poor
quality. Embeddings cluster shallow lexical similarity. The
mitigation works for human predicate coiners; LLM predicate coiners
will exploit the path of least resistance.

Mitigation: a periodic vocabulary audit (monthly or per release)
that proposes predicate merges based on usage patterns and
co-occurrence. Reviewers approve / reject. The audit is itself
review work.

### 7.9 The "audit log exceeds the data" risk

At a billion statements with full audit, the audit log dominates
storage. Compression helps; eventually, audit log retention policies
become necessary. The retention policy is itself a governance
decision.

Mitigation: audit-tier governance (some events kept forever, some
rolled into summaries). Per-context retention policies. v2000
problem; track now.

### 7.10 The substrate-adoption-political risk

Indigenous-data governance is politically contested. Communities
who adopt Atlas Zero will be evaluated by other communities,
researchers, and institutions on whether the system actually
implements the protocols claimed. If it doesn't, the project
becomes liability rather than tool.

Mitigation: independent technical audits by community-aligned
auditors. Transparent governance roadmap. Non-marketing: the
project does not claim CARE compliance until it has been audited.

---

## 8. Deep-research prompts (paste-ready)

These are the prompts to hand to ChatGPT Pro Research, Perplexity
Pro, Elicit, or a domain-expert literature reviewer. Each is
designed to elicit a 2000–5000 word synthesis.

### 8.1 Paraconsistent reasoning at production scale

> Survey production deployments of paraconsistent reasoning over
> multi-source structured data, 2000–2026. Identify systems by name,
> domain, paraconsistency variant (Belnap four-valued, da Costa
> C-systems, Annotated Predicate Logic, Jaśkowski discussive logic,
> other), scale (statements / queries-per-second / users), and
> documented failure modes. Specifically address: query semantics
> under common operations (joins, aggregations, transitive closure)
> in paraconsistent settings; comparison with consensus-based
> alternatives; whether any deployment combines paraconsistency with
> bitemporal reasoning. Conclude with: which deployments are most
> instructive for a new paraconsistent-bitemporal evidence platform
> (codename Atlas Zero) and why.

### 8.2 Indigenous data governance technical implementations

> Survey technical implementations of Indigenous data governance
> beyond Mukurtu CMS, including ELAR access protocols, AIATSIS
> digital systems, OCAP-compliant Canadian First Nations health
> research platforms, Kanaka Maoli archive systems, and recent
> Aboriginal-language teaching platforms with cultural protocols.
> For each, document: the policy framework adopted (CARE / TK / BC /
> OCAP / community-specific), the technical enforcement mechanism
> (row-level access / signed attestations / mediator-based / other),
> the audit and notification model, the handling of derived data,
> the scale of deployment, and known failure modes. Conclude with:
> which patterns are most transferable to a multi-domain evidence
> platform and what design decisions need community involvement
> rather than engineering judgment.

### 8.3 Cross-CLDF-database querying — what's been built and why not more

> The Cross-Linguistic Data Formats (CLDF) standard enables per-
> dataset publication, and CLLD aggregates CLDF datasets, but
> querying across multiple CLDF datasets simultaneously is rare.
> Survey: what cross-CLDF query systems exist (Glottolog Mapping,
> Lexibank tooling, the WALS-Grambank reconciliation, ad hoc
> projects), what they do well, where they fail, and what the
> blocking factors have been. Address: cross-database identifier
> alignment; cross-database value alignment (the same feature
> coded with incompatible value vocabularies); cross-database
> source attribution at row granularity; performance. Conclude
> with: design recommendations for a system intending to query
> across all major CLDF databases (WALS, Grambank, AUTOTYP,
> PHOIBLE, ValPaL, APiCS, SAILS, Concepticon, CLICS, WOLD)
> simultaneously while preserving per-source provenance.

### 8.4 Universal Dependencies — sacrifices and contested decisions

> Universal Dependencies has achieved cross-linguistic morpho-
> syntactic consistency through specific design decisions. Survey:
> the documented design decisions of UD V1 → V2; the contested
> decisions (DET vs PRON for demonstratives; aux dependency
> direction; nominal copular constructions; ergative-language
> handling); cases where treebanks for the same language disagree
> on UD analysis; criticisms of UD from typological / cognitive /
> functional linguistic perspectives. Conclude with: an inventory
> of UD analytical commitments that a multi-source evidence
> platform must be prepared to disagree with, and patterns for
> preserving alternative analyses alongside UD-default analyses.

### 8.5 OntoLex-Lemon adoption — the truth behind the standard

> OntoLex-Lemon is the W3C-blessed lexical linked data standard.
> Survey: which lexicons are actually published in OntoLex
> (DBnary, lemon-uby, individual published lexicons), what the
> common deviations from the standard are, what criticisms have
> been raised by working lexicographers and computational
> lexicologists, what alternative models are in active use (LIFT,
> TEI Dictionary, custom XML), and the comparative trade-offs.
> Conclude with: design recommendations for a system that needs
> to ingest from LIFT-format dictionaries (the dominant working
> format) but wants to expose data as OntoLex-conformant linked
> data, with attention to round-trip preservation.

### 8.6 Linguistic-documentation tool ecosystems and adoption

> Survey the actual workflows of working field, descriptive, and
> documentation linguists circa 2020–2026, focusing on the
> integration of ELAN, FLEx (FieldWorks Language Explorer),
> Toolbox / Shoebox, SayMore, Mukurtu, and (where relevant)
> Praat, ELAN-CorpA, ARBIL, Audacity, Camtasia. Address: typical
> data flow from field recording to published artefact; pain
> points in current workflows; what tools are adopted vs.
> abandoned and why; what kinds of intervention have succeeded vs.
> failed in changing linguist behaviour. Conclude with: adoption
> strategy recommendations for a new linguistic evidence
> platform that wants to fit into existing workflows rather than
> replace them.

### 8.7 Federated knowledge graphs — what worked and what didn't

> Survey attempts at federated knowledge graphs and federated
> linked data, 2010–2026: SPARQL federation (FedX, ANAPSID,
> Splendid), Solid pods, Verifiable Credentials work, ActivityPub
> as a federation substrate for structured data, ORCID and
> DataCite as domain-specific federations, and recent
> decentralised-identifier (DID) work. For each, document:
> architecture, scale of deployment, what works, what doesn't,
> common failure modes, and the specific reasons federation
> proves harder than centralisation. Conclude with: a credible
> roadmap (or honest skepticism) for federation of evidence-
> platform instances under cultural-protocol governance, where
> instances belong to different communities with different access
> policies.

### 8.8 Production entity resolution and its applicability to linguistic entities

> Survey production-grade entity resolution systems, 2018–2026:
> Splink, Magellan, Dedoop, Senzing, Zingg, Datablend, plus
> bespoke pipelines at major data integrators. For each, document:
> architecture, candidate-generation approach, pairwise-scoring
> approach, clustering and consensus, scaling characteristics,
> known failure modes. Specifically address: applicability to
> linguistic entity resolution, where entities are language
> varieties, lexemes, morphemes, phonemes, with much sparser
> features than person/business records. Conclude with: design
> recommendations for an entity-resolution layer over a
> bitemporal paraconsistent KG that needs to handle language
> variety / lexeme / morpheme / phoneme resolution under multiple
> competing identity hypotheses.

### 8.9 Frame semantics adoption and alternatives

> Frame semantics (Berkeley FrameNet, FrameNet Brasil,
> Multilingual FrameNets) has been around for 30 years.
> PropBank, NomBank, AMR, UCCA, and others are alternatives.
> Survey: actual adoption of frame-semantic representations in
> production KGs and in NL applications; trade-offs among
> FrameNet, PropBank, and AMR; criticisms from cognitive
> linguistics and from computational pragmatics; recent work on
> frame-semantic KG construction. Conclude with: design
> recommendations for an evidence platform that uses n-ary
> analysis frames for paradigm cells, allomorphy rules, IGT
> examples, valency frames, and identity hypotheses, with
> attention to the trade-offs between FrameNet-style and AMR-style
> representations.

### 8.10 Healthcare vocabulary alignment as production prior art

> The healthcare-data community has spent decades doing what we
> call predicate alignment: cross-vocabulary mapping at production
> scale. Survey UMLS Metathesaurus, SNOMED CT-to-ICD mappings,
> OHDSI's OMOP common data model, BioPortal mapping
> infrastructure, and OBO Foundry coordination. For each,
> document: the alignment relations supported, the curation
> workflow, the validation discipline, scale, known failure modes,
> and how disagreement among source vocabularies is preserved (or
> not). Conclude with: the patterns and pitfalls most relevant to
> a domain-agnostic predicate alignment layer with closure
> expansion at query time.

### 8.11 Modern temporal natural-language reasoning

> Survey the state of temporal natural-language reasoning beyond
> Allen interval relations, 2010–2026: TimeML, EDTF (Extended Date
> Time Format), ISO 8601 extensions, fuzzy interval logic, recent
> temporal QA benchmarks (TimeQA, TempLAMA, TempReason), and
> production temporal reasoning in healthcare (FHIR), genealogy
> (GEDCOM-X), and legal documents. Address: handling of imprecise
> dates ("circa 1850"), conditional dates, range dates, partial
> dates, ordering under uncertainty. Conclude with: design
> recommendations for the temporal layer of an evidence platform
> that needs to support all of: documentary evidence with
> imprecise dates, corpus annotations with token timestamps, audio
> recordings with timecode, and bitemporal valid_time / tx_time
> for the platform itself.

### 8.12 Has anyone combined paraconsistency, bitemporal, and HITL?

> Identify systems, research projects, or production deployments
> that combine all three of: paraconsistent reasoning over
> contradictory data; bitemporal storage; and human-in-the-loop
> review and approval. The combination is uncommon. Where the
> combination has been attempted, document the architecture, the
> domain, the scale, what worked, what didn't, and what the
> system did about query semantics under the triple combination.
> Conclude with: lessons for a new evidence platform aspiring to
> exactly this combination.

### 8.13 Linguistic-tool adoption history and lessons

> Survey the adoption history of major linguistic tools 1995–2026:
> Toolbox, Shoebox, FLEx (FieldWorks), Linguist's Assistant,
> ELAN, BRAT, GATE, INCEpTION, Argilla, Prodigy, plus archive
> tools (DELAMAN, ELAR, PARADISEC). For each, document: adoption
> trajectory, why adopted (or not), what worked, what didn't,
> migration patterns when tools change, the role of funding (DELP,
> NSF DEL, ELDP, ERC) in adoption. Conclude with: the principal
> success factors and failure factors for adoption of a new
> linguistic-evidence platform.

### 8.14 Differential privacy and TEEs for restricted-data extraction

> Survey practical paths to running structured extraction over data
> that cannot leave a community's control: differential-privacy
> approaches to KG queries (DP-SQL, SmartNoise, OpenDP), trusted
> execution environments (Intel SGX, AMD SEV, Apple Secure
> Enclave) for data sharing, federated learning over graph data,
> and recent secure multi-party computation work. For each,
> document: what's actually deployable in 2026, what's research-
> only, the threat models, and the failure modes. Conclude with:
> design recommendations for an evidence platform that wants to
> enable LLM extraction over data subject to cultural-protocol
> restrictions, with extraction running in an enclave or via
> privacy-preserving techniques.

### 8.15 Round-trip integrity in CLDF / UD / UniMorph

> CLDF, Universal Dependencies, and UniMorph all aspire to round-
> trip preservation. Survey: actual round-trip experiences and
> documented failure modes in all three, including community
> discussions on GitHub and at LREC / ACL. What information is
> intrinsically lost when round-tripping CLDF → JSONL → CLDF? When
> round-tripping CoNLL-U → custom format → CoNLL-U? When
> round-tripping UniMorph TSV → custom paradigm → UniMorph TSV?
> Conclude with: the precise information-preservation contract a
> new system can promise vs. the information-preservation
> aspirations it must caveat.

### 8.16 Maturity ladders across domains

> Compare claim-maturity / evidence-maturity ladders across
> domains: medicine's evidence pyramid (case report → cohort → RCT
> → systematic review → meta-analysis → guideline); GRADE
> evidence assessment; legal evidence rules (hearsay /
> circumstantial / direct / forensic); intelligence community
> words of estimative probability; scientific replication tiers;
> peer-review tiers (working paper / preprint / peer-reviewed /
> meta-analysed). Conclude with: what these ladders get right,
> what they have in common, and what a domain-agnostic evidence
> platform's maturity ladder should look like to be usable across
> them.

### 8.17 Citable knowledge release formats — what survives long-term

> Survey citable, long-term-stable knowledge release formats:
> Software Heritage IDs, RO-Crate, DOI-stamped PURLs, IPFS
> content-addressed releases, DataCite registrations, the
> Internet Archive Scholar program. For each, document: long-
> term stability evidence, citation conventions, ecosystem
> support, integration with research workflows, and the cost
> structure. Conclude with: recommended primary release format
> and secondary fallback formats for an evidence platform whose
> releases are intended to remain citable for decades.

---

## 9. What success looks like, in concrete terms

A vision is empty without testable success criteria. Atlas Zero
succeeds when:

### 9.1 At v1000

- A linguistic researcher can ingest a reference grammar PDF and,
  within minutes, see structured claims anchored to specific page
  ranges, automatically aligned to comparative-database features
  where applicable, with a reviewer queue ready.
- A community partner can register an access policy and trust that
  derived claims inherit it, that public exports never leak
  restricted material, and that the audit log is meaningful.
- A descriptive linguist can query "all claims about case marking
  in language X across all sources" and get cross-source results
  with disagreement preserved, evidence chains intact, and
  per-source provenance.
- A cross-linguistic typologist can issue a query in WALS terms
  and get results from Grambank-coded rows by virtue of registered
  alignment.
- A research project can publish a citable, reproducible release.

### 9.2 At v2000

- The system holds 1B+ statements on a single Postgres node with
  acceptable query latency.
- Predicate-alignment closure rebuilds in minutes, not hours.
- The release builder produces multi-format releases at competitive
  speed.
- The reviewer queue is the bottleneck, not the engine.

### 9.3 At v3000

- Reviewer decisions train an extraction re-ranker that improves
  reviewer-acceptance rate measurably from extraction batch to
  extraction batch.
- Restricted-data jobs run on local models with quality competitive
  with cloud LLMs.

### 9.4 At v4000

- Atlas Zero serves linguistic, medical, legal, and scientific
  domains simultaneously on a single instance.
- Cross-domain queries return calibrated cross-domain results.

### 9.5 At v5000

- Multiple community-owned Atlas Zero instances exist.
- Federated query works for at least two independent instances
  under signed-attestation cross-access.
- "Per Atlas Zero release X.Y.Z" is a recognised citation form.

### 9.6 The general success test

The general test, applicable at any version: **can a researcher
using Atlas Zero answer a question they could not have answered
before, with evidence, with disagreement preserved, with respect
for governance, and with a reproducible artefact?**

If yes, the system has earned its scope. If no, version forward
until yes.

---

## 10. The closing thought

Most knowledge systems were designed in eras with one of:

- A consensus assumption (encyclopedias, databases).
- A flat-permission assumption (wikis).
- A one-source-of-truth assumption (relational).
- A schema-imposition assumption (ontologies).
- An informal-trust assumption (knowledge graphs).

Atlas Zero is designed for the era we actually live in, where:

- Sources disagree by default.
- Schemas multiply faster than they unify.
- Communities have governance preferences that engineering must
  honor.
- LLMs can extract at near-zero marginal cost but produce candidate
  facts, not truth.
- Reproducibility is a research-integrity requirement, not a nice-
  to-have.
- Time matters in two dimensions.
- Identity is contested at every level.
- Evidence is granular and multimodal.

The thesis is that **a substrate built for these conditions is more
useful than a substrate built for the easier ones**, even though
the easier conditions are what the existing infrastructure was
built for. The bet is that the next generation of research
infrastructure will look more like Atlas Zero than like the
previous generation.

That bet might be wrong. The frontier is what we explore to find
out. The plan in `V1000-REFACTOR-PLAN.md` is the next twelve months.
The questions in §3 of this document are the next twelve years.

We earn the long horizon by shipping the short one. Start with
v1000. Stop at every milestone. Bring back what we learn. Adjust
the frontier as the ground reveals itself.

---

## 11. Appendix: a glossary for outsiders

Terms used in this document that an external reader (or a
literature-search instrument) needs to understand precisely.

| Term | Definition |
|------|------------|
| **Atlas Zero** | The product / project name for the linguistic application built on donto v1000+. By extension, the substrate generalised to any contested-knowledge domain. |
| **donto** | The bitemporal, paraconsistent quad-store engine that Atlas Zero is built on. The codebase. |
| **v1000** | The major version of donto in which it explicitly becomes the Atlas Zero substrate. Roughly the next 12 months of work. |
| **Bitemporal** | Storing two time axes per row: `valid_time` (when true in the world) and `tx_time` (when learned by the system). Allows time-travel over both. |
| **Paraconsistent** | Tolerating contradictory rows without rejecting either. Disagreement is data. |
| **Quad-store** | Storage of (subject, predicate, object, context) tuples — a triple plus a named scope. |
| **Context** | A named scope under which a statement is asserted. Used for sources, hypotheses, dialects, snapshots, projects. |
| **Maturity** | Donto's L0–L4 (now M0–M5) ladder for claim reliability. |
| **PAL** | Predicate Alignment Layer. The cross-schema mapping with closure expansion. |
| **Event frame** | n-ary relation modeled as a frame node with role predicates. |
| **Shape** | A validation rule attached to statements (Lean or Rust). |
| **Certificate** | A Lean-checkable proof attached to a statement, gating maturity to L4 (M4). |
| **Obligation** | Open epistemic work tracked as a row in `donto_proof_obligation`. |
| **Argument** | A relation between two statements: supports / rebuts / undercuts / qualifies / alternative-analysis-of / same-evidence-different-analysis. |
| **Identity hypothesis** | A scoped context for entity-resolution decisions that may compete with other hypotheses (strict / likely / exploratory). |
| **Anchor** | A typed evidence locator (text span, page bbox, ELAN tier annotation, etc.). |
| **Modality** | The epistemic-stance dimension: descriptive / prescriptive / reconstructed / inferred / elicited / corpus-observed / typological-summary. |
| **Extraction level** | The kind of epistemic act behind a claim (quoted / table-read / example-observed / source-generalization / cross-source-inference / model-hypothesis / human-hypothesis). |
| **Review state** | The reviewer-decided state of a claim (unreviewed / triaged / various needs-X / approved-X / rejected / superseded). |
| **Validation state** | The shape-validator-decided state (not_run / passed / warning / failed / blocked-by-X). |
| **Access policy** | A governance object controlling read / quote / export / train_model / publish actions. |
| **Attestation** | A caller's authorization to satisfy a policy, granted by a specific authority for a specific rationale, expirable, revocable. |
| **Release** | A versioned, manifest-stable view over content + policy + review state + schema. Citable. |
| **Manifest-stable** | Same content → same content hash. (Distinct from byte-stable.) |
| **CARE Principles** | Indigenous data governance principles (Collective benefit, Authority to control, Responsibility, Ethics). |
| **AIATSIS Code** | Australian Institute of Aboriginal and Torres Strait Islander Studies Code of Ethics for Aboriginal and Torres Strait Islander Research. |
| **OCAP** | First Nations data principles (Ownership, Control, Access, Possession). |
| **CLDF** | Cross-Linguistic Data Formats. JSON-LD-conformant standard for linguistic comparative datasets. |
| **Glottolog** | The reference catalogue of language varieties and their identifiers. |
| **WALS / Grambank / AUTOTYP / PHOIBLE / ValPaL / APiCS / SAILS** | Major comparative linguistic databases, all CLDF-publishable. |
| **UD** | Universal Dependencies. The token-level treebank standard. |
| **UniMorph** | Universal Morphology. The inflectional schema. |
| **OntoLex-Lemon** | The W3C lexical-linked-data standard. |
| **LIFT** | The Lexicon Interchange Format used by FLEx and many fieldwork tools. |
| **EAF / ELAN** | Time-aligned annotation format / tool for audio-video linguistic data. |
| **PROV-O** | The W3C provenance ontology. |
| **RO-Crate** | A research-object packaging standard for FAIR data. |
| **Software Heritage** | Long-term code preservation infrastructure. |
| **DataCite** | Dataset citation infrastructure. |

---

*End of frontier document. Pasting research output below this line is welcome; the document is designed to receive whatever the literature returns.*

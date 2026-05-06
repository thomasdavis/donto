# Language Extraction on donto — A Comprehensive Plan

> **STATUS: SUPERSEDED.** Canonical PRD is now
> [`DONTO-V1000-PRD.md`](DONTO-V1000-PRD.md). This document is preserved
> as a historical artefact.


> A detailed plan for using **donto** (this repository) as the substrate for
> extracting, storing, aligning, and querying every kind of linguistic
> evidence — for any language — across grammars, dictionaries, corpora,
> field recordings, archival manuscripts, and the full stack of comparative
> typological databases (WALS, Grambank, AUTOTYP, UniMorph, UD, PHOIBLE,
> ValPaL, APiCS, SAILS) and linguistic ontologies (GOLD, OLiA,
> OntoLex-Lemon, lexinfo, CLDF).
>
> This document is a planning artifact. Nothing in here changes existing
> donto code; everything either describes the current state (with file/line
> citations) or proposes named additions in clearly-scoped sister projects.

---

## Table of contents

1. [Executive summary](#1-executive-summary)
2. [The problem, stated precisely](#2-the-problem-stated-precisely)
3. [What donto already provides — technical state of the repo](#3-what-donto-already-provides--technical-state-of-the-repo)
4. [Why donto is the right substrate for linguistic data](#4-why-donto-is-the-right-substrate-for-linguistic-data)
5. [Identifier and language registry](#5-identifier-and-language-registry)
6. [Source typology and per-source ingest plans](#6-source-typology-and-per-source-ingest-plans)
7. [Comparative-database integrations (CLDF stack)](#7-comparative-database-integrations-cldf-stack)
8. [Linguistic ontology and predicate vocabulary layer](#8-linguistic-ontology-and-predicate-vocabulary-layer)
9. [Data-model walkthroughs with worked examples](#9-data-model-walkthroughs-with-worked-examples)
10. [Predicate alignment across linguistic schemas](#10-predicate-alignment-across-linguistic-schemas)
11. [Entity resolution for languages, lexemes, morphemes](#11-entity-resolution-for-languages-lexemes-morphemes)
12. [The maturity ladder applied to linguistic claims](#12-the-maturity-ladder-applied-to-linguistic-claims)
13. [Lean shape catalogue for linguistics](#13-lean-shape-catalogue-for-linguistics)
14. [Access governance — CARE, AIATSIS, ELAR, generic ACLs](#14-access-governance--care-aiatsis-elar-generic-acls)
15. [CLDF export and FAIR interop contract](#15-cldf-export-and-fair-interop-contract)
16. [Query patterns (DontoQL and SPARQL)](#16-query-patterns-dontoql-and-sparql)
17. [Batch extraction app — changes for linguistics](#17-batch-extraction-app--changes-for-linguistics)
18. [TUI and operational tooling](#18-tui-and-operational-tooling)
19. [Sister project specifications](#19-sister-project-specifications)
20. [End-to-end pipeline and milestones](#20-end-to-end-pipeline-and-milestones)
21. [Risks and open questions](#21-risks-and-open-questions)
22. [Appendices](#22-appendices)

---

## 1. Executive summary

donto is a bitemporal, paraconsistent quad-store with a Postgres core, a
Rust HTTP sidecar, a Lean 4 verifier, a Go TUI, and a Python batch
extraction app. It already supports the operations a comprehensive
linguistic database needs: claims under uncertainty, full source
provenance, hypothesis-scoped contexts, predicate alignment across
schemas, entity resolution under competing hypotheses, n-ary relations
via event frames, temporal validity, and a maturity ladder from "raw
ingest" to "Lean-certified".

Linguistic data is an unusually good fit because:

- **Sources disagree.** Two grammars routinely give incompatible analyses
  of the same morpheme. Paraconsistency keeps both as evidence rather
  than forcing a winner.
- **Schemas multiply.** WALS, Grambank, AUTOTYP, UD, UniMorph, GOLD, OLiA
  all describe overlapping but non-identical features. The Predicate
  Alignment Layer (PAL) lets every schema coexist without picking a
  canonical one.
- **Evidence is granular.** Every claim ought to be anchored to a page,
  timestamp, or token offset. donto's evidence substrate
  (`donto_document` → `donto_document_revision` → `donto_extraction_chunk`
  → `donto_span` → `donto_evidence_link`) does this row by row.
- **Hypotheses are routine.** "Under the hypothesis that X and Y are
  conditioned allomorphs of one morpheme…" is a hypothesis-kind
  context. You can assert under it without committing globally.
- **Indigenous/endangered language data needs governance.** That layer
  doesn't exist yet in donto — it's the most important piece to design
  before any restricted material is ingested. Section 14 specifies it.

The end state is a CLDF-interoperable, FAIR, auditable language
database where every claim is anchored to a source, every analytical
schema is preserved, contradictions are first-class, and access policies
travel with restricted material.

---

## 2. The problem, stated precisely

Extracting "every grammatical feature about a language" from real-world
sources is *not* one task. It is the union of at least eleven distinct
extraction problems, each with different evidence types, different
schemas, and different reliability profiles:

| # | Sub-problem                                                                           | Typical sources                                  |
|---|---------------------------------------------------------------------------------------|--------------------------------------------------|
| 1 | Phoneme inventory + distinctive features                                              | reference grammar phonology chapter, PHOIBLE     |
| 2 | Phonotactics, syllable structure, prosody                                             | grammar, instrumental phonetic studies           |
| 3 | Morphological inventory (morphemes, allomorphs, conditioning environments)            | grammar morphology chapter, dictionary           |
| 4 | Inflectional paradigms                                                                | grammar paradigm tables, UniMorph                |
| 5 | Word classes, derivational morphology                                                 | dictionary entries, grammar word-class chapter   |
| 6 | Case systems, alignment, agreement                                                    | grammar syntax chapter, WALS, Grambank, AUTOTYP  |
| 7 | Constituent order, phrase structure                                                   | grammar syntax chapter, UD treebank              |
| 8 | Clause types, complex clauses, subordination                                          | grammar, clause-type studies                     |
| 9 | Negation, interrogatives, particles, clitics                                          | grammar, corpus tagged texts                     |
| 10 | Valency, voice, argument structure                                                   | grammar, ValPaL, dictionary verb entries         |
| 11 | Information structure, discourse grammar, dialect/register variation                 | corpus, fieldwork notes, recordings              |

Each of these problems wants the same primitives: structured claims,
sources, schemas, alignment between schemas, evidence chains, validation,
and dialect-scoped variants. A general substrate should support all
eleven without privileging any.

There are also two cross-cutting requirements:

- **Comparative integration.** Already-coded data exists for thousands of
  languages in WALS / Grambank / AUTOTYP / PHOIBLE / UniMorph / UD /
  ValPaL / APiCS / SAILS. A serious linguistic database should ingest
  these as L1 (parsed) facts under per-source contexts, not re-extract
  them from secondary sources.

- **Ontology mapping.** The same conceptual feature is named differently
  across schemas. A real cross-source query needs the alignment layer to
  bridge `wals:Feature98 ↔ grambank:GBxxx ↔ ud:Case=Erg ↔
  lexinfo:ergativeCase ↔ gold:ErgativeCase`.

The remainder of this document specifies, in detail, how donto handles
each of these requirements with the architecture it already has, and
what sister projects fill the remaining gaps.

---

## 3. What donto already provides — technical state of the repo

This section is a current-state snapshot grounded in file paths. All
references are to paths under `/Users/ajaxdavis/repos/donto/`.

### 3.1 Schema (Postgres)

`packages/sql/migrations/` contains 67 sequential migrations
(`0001_core.sql` through `0067_rule_engine.sql`). Highlights for
linguistic use:

| Migration                            | Purpose                                                                                        |
|--------------------------------------|-----------------------------------------------------------------------------------------------|
| `0001_core.sql`                      | `donto_context`, `donto_statement`, `donto_stmt_lineage`, `donto_audit`                        |
| `0002_flags.sql`                     | Polarity (asserted/negated/absent/unknown) + maturity (L0–L4) packed in `flags smallint`       |
| `0003_functions.sql`                 | `donto_assert`, `donto_retract`, `donto_correct`, `donto_match`, `donto_ensure_context`        |
| `0005_presets.sql`                   | Named scopes: `latest`, `raw`, `curated`, `under_hypothesis`, `as_of`, `anywhere`              |
| `0008_shape.sql`, `0009_rule.sql`    | Validation shapes and derivation rules                                                         |
| `0010_certificate.sql`               | Lean-checkable proof attachments                                                               |
| `0013_search_trgm.sql`               | Trigram FTS for predicate/subject lookup                                                       |
| `0023_documents.sql` … `0029_evidence_links.sql` | Document → revision → span → mention → evidence chain                            |
| `0030_agents.sql`                    | Human / AI / service agents bound to contexts                                                  |
| `0031_arguments.sql`                 | `supports`, `rebuts`, `undercuts`, `qualifies` — claim-level disagreement                      |
| `0032_proof_obligations.sql`         | Open epistemic work (needs-disambiguation, needs-source-support, needs-human-review, …)        |
| `0034_claim_card.sql`                | `donto_claim_card(stmt_id)` reconstructs the full evidence chain                               |
| `0036_mentions.sql`, `0037_extraction_chunks.sql` | Per-chunk extraction provenance                                                   |
| `0038_confidence.sql`                | Confidence score overlay separate from maturity                                                |
| `0040_temporal_expressions.sql`, `0063_time_expression.sql`, `0064_temporal_relation.sql` | Circa, range, before/after/overlaps                  |
| `0048_predicate_alignment.sql` … `0055_match_alignment_integration.sql` | The Predicate Alignment Layer (PAL)                          |
| `0049_predicate_descriptor.sql`      | Predicate metadata: label, gloss, domain, range, examples, embedding                           |
| `0053_canonical_shadow.sql`          | Pre-materialized canonical predicate per statement                                             |
| `0054_event_frames.sql`              | n-ary decomposition for complex relations                                                      |
| `0057_entity_symbol.sql` … `0061_identity_hypothesis.sql` | Entity resolution with competing identity hypotheses                       |
| `0062_literal_canonical.sql`         | Cross-paper literal normalization                                                              |
| `0065_property_constraint.sql`       | Cardinality and value constraints                                                              |
| `0066_class_hierarchy.sql`           | OWL-style class / subclass relations                                                           |
| `0067_rule_engine.sql`               | Derivation rules (transitive, inverse, symmetric)                                              |

### 3.2 Sidecar HTTP API — `apps/dontosrv/src/lib.rs`

Routes are registered in `apps/dontosrv/src/lib.rs:41` (`pub fn router`).
Highlights:

| Path                                         | Method | Purpose                                                  |
|----------------------------------------------|--------|----------------------------------------------------------|
| `/sparql`, `/dontoql`                        | POST   | Query languages (see §16)                                |
| `/assert`, `/assert/batch`, `/retract`       | POST   | Write path (see §3.3)                                    |
| `/contexts/ensure`                           | POST   | Upsert a context                                         |
| `/documents/register`, `/documents/revision` | POST   | Source provenance                                        |
| `/evidence/link/span`, `/evidence/:stmt`     | POST/GET | Anchor a claim to a text span                          |
| `/agents/register`, `/agents/bind`           | POST   | Human/AI/service agents                                  |
| `/arguments/assert`, `/arguments/:stmt`      | POST/GET | Supports/rebuts/undercuts/qualifies                    |
| `/arguments/frontier`                        | GET    | Statements under contradiction pressure                  |
| `/obligations/emit`, `/obligations/resolve`  | POST   | Open epistemic work                                      |
| `/shapes/validate`, `/rules/derive`          | POST   | Run shape validation / derivation                        |
| `/certificates/attach`, `/certificates/verify/:stmt` | POST | Lean proof attachments                              |
| `/alignment/register`, `/alignment/retract`, `/alignment/rebuild-closure` | POST | Predicate alignment            |
| `/descriptors/upsert`, `/descriptors/nearest` | POST  | Predicate metadata + semantic search                     |
| `/shadow/materialize`, `/shadow/rebuild`     | POST   | Canonical-predicate materialization                      |
| `/dir`                                       | POST   | List registered shapes/rules/certificates                |
| `/claim/:id`                                 | GET    | Full claim card                                          |

### 3.3 Batch extraction app — `apps/donto-api/`

Python FastAPI + Temporal worker. Endpoints registered in
`apps/donto-api/main.py`:

| Line | Endpoint                                     |
|------|----------------------------------------------|
| 178  | `GET  /firehose/stream` (SSE)                |
| 228  | `GET  /firehose/recent`                      |
| 263  | `GET  /firehose/stats`                       |
| 309  | `GET  /health`                               |
| 319  | `GET  /version`                              |
| 334  | `POST /extract-and-ingest` (sync)            |
| 433  | `POST /jobs/extract` (async, single)         |
| 459  | `POST /jobs/batch` (async, many)             |
| 488  | `GET  /jobs`                                 |
| 551  | `GET  /jobs/{job_id}`                        |
| 581  | `POST /jobs/retry-failed`                    |
| 624  | `GET  /jobs/{job_id}/facts`                  |
| 703  | `GET  /jobs/{job_id}/source`                 |
| 740  | `GET  /queue` (HTML)                         |
| 1001 | `POST /extract` (no ingest)                  |
| 1069 | `POST /assert`                               |
| 1094 | `POST /assert/batch`                         |
| 1110 | `GET  /subjects`                             |
| 1122 | `GET  /search`                               |
| 1150 | `GET  /history/{subject:path}`               |
| 1170 | `GET  /statement/{id}`                       |
| 1183 | `GET  /contexts`                             |
| 1201 | `GET  /predicates`                           |
| 1224 | `POST /query` (DontoQL or SPARQL)            |
| 1254 | `POST /retract/{statement_id}`               |
| 1278 | `GET  /connections/{entity:path}`            |
| 1354 | `GET  /context/analytics/{context:path}`     |
| 1444 | `POST /graph/neighborhood`                   |
| 1585 | `POST /graph/path`                           |
| 1661 | `GET  /graph/stats`                          |
| 1723 | `POST /graph/subgraph`                       |
| 1781 | `GET  /graph/entity-types`                   |
| 1809 | `GET  /graph/timeline/{subject:path}`        |
| 1872 | `POST /graph/compare`                        |
| 1944 | `POST /align/register`                       |
| 1968 | `POST /align/rebuild`                        |
| 1982 | `POST /align/retract/{alignment_id}`         |
| 1992 | `GET  /align/suggest/{predicate}`            |
| 2019 | `GET  /evidence/{statement_id}`              |
| 2036 | `GET  /claim/{statement_id}`                 |
| 2066 | `POST /entity/register`                      |
| 2084 | `POST /entity/register/batch`                |
| 2110 | `POST /entity/identity`                      |
| 2138 | `POST /entity/identity/batch`                |
| 2168 | `POST /entity/membership`                    |
| 2194 | `GET  /entity/{iri:path}/edges`              |
| 2215 | `GET  /entity/cluster/{hypothesis}/{referent_id}` |
| 2235 | `GET  /entity/resolve/{iri:path}`            |
| 2261 | `GET  /entity/family-table`                  |
| 2340 | `POST /papers/ingest` (domain-specific)      |

The extraction prompt is at `apps/donto-api/helpers.py:64` (currently
the 8-tier sociology/genealogy prompt) and the confidence → maturity
mapping is at `helpers.py:39`. **For linguistic work, the prompt is
replaced (not edited) by a domain-specific one selected per job; see §17.**

### 3.4 CLI — `apps/donto-cli/src/main.rs`

Subcommands declared at line 56 (`#[derive(Subcommand, Debug)]`):
`migrate`, `ingest`, `match`, `query`, `retract`, `man`, `completions`.

### 3.5 Ingestion formats — `packages/donto-ingest/src/`

| File                  | Format                                                   |
|-----------------------|----------------------------------------------------------|
| `nquads.rs`           | N-Quads (named graph → context)                          |
| `turtle.rs`           | Turtle, TriG (named graph → context)                     |
| `rdfxml.rs`           | RDF/XML                                                  |
| `jsonld.rs`           | JSON-LD subset (`@context`, `@graph`, `@id`, `@type`)    |
| `jsonl.rs`            | One-statement-per-line JSONL (LLM-friendly)              |
| `property_graph.rs`   | Neo4j / AGE JSON dumps                                   |
| `csv.rs`              | CSV with mapping file                                    |
| `quarantine.rs`       | Failed-ingest quarantine                                 |
| `pipeline.rs`         | Shared pipeline                                          |

CLDF datasets are JSON-LD-native, so `jsonld.rs` covers them with a thin
metadata wrapper (specified in §15).

### 3.6 Query languages — `packages/donto-query/src/`

| File             | Purpose                                              |
|------------------|------------------------------------------------------|
| `dontoql.rs`     | DontoQL parser (PRESET, MATCH, FILTER, POLARITY, MATURITY, PREDICATES, PROJECT) |
| `sparql.rs`      | SPARQL 1.1 subset (PREFIX, SELECT, WHERE, LIMIT)     |
| `algebra.rs`     | Internal algebra                                     |
| `evaluator.rs`   | Nested-loop evaluator                                |

### 3.7 Migrators — `packages/donto-migrate/src/`

| File             | Targets                                    |
|------------------|--------------------------------------------|
| `genealogy.rs`   | SQLite genealogy DB → donto                |
| `relink.rs`      | Second-pass provenance enrichment          |
| `main.rs`        | CLI dispatcher                             |

### 3.8 TUI — `apps/donto-tui/`

Go/Charm Bubbletea. Tabs: Dashboard, Firehose, Explorer, Contexts, Claim
Card, Charts. Real-time updates via Postgres `LISTEN`/`NOTIFY` on
`donto_audit` and `donto_statement`.

---

## 4. Why donto is the right substrate for linguistic data

Mapping a generic linguistic database to donto primitives:

| Linguistic concept                                   | donto primitive                                                                                |
|------------------------------------------------------|------------------------------------------------------------------------------------------------|
| Language / dialect / variety / idiolect              | `donto_context` (kind = `language`, `dialect`, `register`)                                     |
| Source (grammar, paper, dictionary, archive item)    | `donto_context` (kind = `source`) + `donto_document` row                                       |
| Source manuscript revision / OCR / TEI               | `donto_document_revision`                                                                      |
| Page anchor / line anchor / timecode                 | `donto_span` (offsets + region anchors)                                                        |
| Mention of a form in text                            | `donto_mention`                                                                                |
| Linguistic feature definition                        | `donto_predicate` + `donto_predicate_descriptor`                                               |
| Feature value (categorical or numeric)               | Object IRI (categorical) or literal `{v, dt, lang}` (numeric)                                  |
| Feature observation under a hypothesis               | Statement under hypothesis-kind context                                                        |
| Lexeme entry                                         | Subject IRI; reified with `ontolex:LexicalEntry` predicate                                     |
| Form (orthographic, phonemic, phonetic)              | Object literal with `lang` and datatype hints                                                  |
| Sense                                                | Subject IRI under `ontolex:LexicalSense`                                                       |
| Morpheme, allomorph                                  | Subject IRI; allomorphy via event frame (lexeme + environment + form)                          |
| Inflectional paradigm                                | Event frame (lexeme + features + form) — preserves n-ary structure                             |
| Construction / clause type                           | Event frame (template + roles + constraints)                                                   |
| Token, sentence (corpus)                             | Subject IRI; head/deprel/upos/feats as predicates; sentence-scoped context                     |
| Disagreement between sources                         | `donto_argument` row with kind `rebuts`/`undercuts`                                            |
| Open analytical question                             | `donto_proof_obligation`                                                                       |
| Cross-schema mapping (WALS ↔ Grambank ↔ UD)          | `donto_predicate_alignment` + closure                                                          |
| Time-bounded validity (e.g., "this only applied pre-1950") | Statement `valid_time daterange`                                                          |
| When a claim was learned / revised                   | Statement `tx_time tstzrange` (closed by `donto_retract` / `donto_correct`)                    |
| Validation rule (e.g., paradigm completeness)        | `donto_shape` + `donto_shape_annotation`                                                       |
| Verifiable proof (e.g., phonotactic regularity)      | `donto_certificate` (Lean-checked)                                                             |
| Access policy on restricted material                 | **Sister project (§14)** — does not exist yet                                                  |

Things this gives you that a flat CLDF-style relational database does
not:

- **Disagreement is first-class.** Two grammars give incompatible
  descriptions of a clitic. Both rows survive. `arguments/frontier`
  surfaces the disagreement automatically.
- **Time travel.** "What did we believe about the agreement system as of
  2024-12-01?" is one query (`PRESET as_of`).
- **Hypothesis scopes.** Test an analysis without committing globally.
- **Schema-flexible queries.** Answer the same question in WALS-speak,
  Grambank-speak, or UniMorph-speak. Alignment closure does the work.
- **Evidence chains.** Drill from claim to the page or timestamp it
  came from in one query.
- **Maturity progression.** Promote claims from L0 (raw OCR) to L4
  (Lean-certified) as evidence accumulates.

---

## 5. Identifier and language registry

Every language and dialect in the database needs a stable IRI and a
registered set of equivalent identifiers. Use the canonical registries
where they exist; mint a local IRI under `lang:` for the donto-side
canonical handle.

### 5.1 IRI conventions

```
lang:<glottocode>                    canonical language IRI
lang:<glottocode>/dialect/<slug>     dialect IRI
lang:<glottocode>/register/<slug>    register/style IRI
lang:<glottocode>/idiolect/<speaker> speaker idiolect IRI
ctx:lang/<glottocode>                language-scope context
ctx:lang/<glottocode>/dialect/<slug> dialect-scope context
```

### 5.2 Identifier predicates

For each language register at minimum:

| Predicate              | Purpose                              | Source                                      |
|------------------------|--------------------------------------|---------------------------------------------|
| `glottolog:glottocode` | Glottolog glottocode                 | https://glottolog.org/                      |
| `iso639:code3`         | ISO 639-3 three-letter code          | https://iso639-3.sil.org/                   |
| `iso639:code1`         | ISO 639-1 two-letter (where exists)  |                                             |
| `wals:code`            | WALS lect code (where exists)        | https://wals.info/                          |
| `austlang:code`        | AIATSIS Austlang code (Australian)   | https://collection.aiatsis.gov.au/austlang/ |
| `elcat:id`             | Endangered Languages Catalogue id    | http://endangeredlanguages.com/             |
| `ethnologue:code`      | Ethnologue code                      |                                             |
| `lang:familyOf`        | Language-family parent               | Glottolog tree                              |
| `lang:hasDialect`      | Dialect membership                   | (project-defined)                           |
| `lang:speakerCount`    | Speaker count (literal, integer)     | (sourced; multiple sources OK)              |
| `lang:vitality`        | Vitality classification              | EGIDS / ELCat / project-defined             |
| `lang:officialIn`      | Country/region of official status    |                                             |
| `lang:writingSystem`   | Script(s) used                       | ISO 15924                                   |
| `lang:areaCoordinates` | WGS84 lat/long literal               |                                             |

### 5.3 Bootstrap from Glottolog

Glottolog is the recommended starting point because its entry for any
language already includes glottocode, ISO 639-3, family tree, alternate
names, bibliography, and links to WALS / Grambank / PHOIBLE / AIATSIS /
OLAC. Ingest Glottolog for the full set of languages of interest (or
all of it) under context `ctx:source/glottolog/<release-version>`,
emitting `lang:*` and `glottolog:*` statements at L1. Section 7 covers
the ingest mechanics.

---

## 6. Source typology and per-source ingest plans

Every source category has its own ingest pattern. The table below is
the master plan; subsections drill into each.

| # | Category                          | Format on disk                  | Ingest path                       | Initial maturity |
|---|-----------------------------------|----------------------------------|-----------------------------------|------------------|
| A | Reference grammar (PDF/print)     | PDF, scanned PDF, EPUB           | OCR → chunk → batch extraction    | L0 → L1          |
| B | Dictionary (electronic)           | XML (LIFT, TEI), CSV, custom     | Per-entry parse → JSONL ingest    | L1               |
| C | Dictionary (PDF/print)            | PDF                              | OCR → entry detection → batch     | L0 → L1          |
| D | Treebank                          | CoNLL-U, PML                     | Token-level mapping → JSONL       | L1               |
| E | Paradigm tables (UniMorph)        | TSV                              | Direct mapping → event frames     | L1               |
| F | Comparative DB (WALS, Grambank …) | CLDF (JSON-LD + CSV)             | CLDF ingester (sister project)    | L1               |
| G | Phonological inventory (PHOIBLE)  | CLDF                             | CLDF ingester                     | L1               |
| H | Field recording (audio/video)     | WAV/MP3/MP4 + ELAN/Praat/EAF     | Annotation parse → time-anchored  | L0 → L1          |
| I | Field notes (manuscript)          | scanned image / TXT              | OCR / transcription → batch       | L0               |
| J | Archival catalogue record         | METS, Dublin Core, custom        | Metadata parse → context bootstrap| L1               |
| K | Comparative wordlist              | CSV / Swadesh / IELex            | CSV ingester                      | L1               |

### 6.1 Reference grammars (A, C)

Workflow:

1. **Register the document.** `POST /documents/register` with
   bibliographic metadata (author, year, title, ISBN, archive id,
   license, access conditions). Returns `document_id`.
2. **Add a revision.** `POST /documents/revision` with the OCR'd or
   typed text, source format, OCR confidence per page, and processing
   metadata.
3. **Detect interlinear glossed text (IGT).** Heuristic: three
   consecutive lines where line 2 has a high proportion of glossing
   abbreviations (SG, PL, ERG, ABS, NOM, ACC, …). Annotate detected IGT
   as `donto_content_region` (migration `0041_content_regions.sql`)
   with kind `igt`.
4. **Chunk by section.** Reference grammars have a stable internal
   structure: phonology, morphology, syntax, complex clauses, etc. Use
   the table of contents (or heading detection) to chunk into
   semantically coherent pieces. Each chunk creates a
   `donto_extraction_chunk`.
5. **Batch extract.** `POST /jobs/batch` with `domain="linguistics"` and
   one job per chunk. Returns batch ID; monitor via `/jobs?status=*`.
6. **Post-extract alignment.** Workflow auto-runs `align_predicates`
   and `resolve_entities` activities (`apps/donto-api/workflows.py`).
7. **Anchor.** Each extracted claim already has a `donto_evidence_link`
   pointing back to the chunk; if it cites a specific page, the chunk
   range gives a span anchor at chunk granularity. Tighter anchoring
   (sentence-level) requires re-extraction with span return — see §17.

### 6.2 Electronic dictionaries (B)

LIFT (Lexicon Interchange Format) is the most common format for
fieldwork-grade dictionaries. Ingest pattern:

1. Parse LIFT XML into one record per entry.
2. For each entry, emit:
   - `<lex_iri> rdf:type ontolex:LexicalEntry`
   - `<lex_iri> ontolex:canonicalForm <form_iri>`
   - `<form_iri> ontolex:writtenRep "<form>"@<lang>`
   - `<lex_iri> lexinfo:partOfSpeech <pos_iri>`
   - one `ontolex:LexicalSense` per gloss with `skos:definition`
   - `<lex_iri> dct:source <document_id>` for provenance
3. Examples in entries become `examples:igt/<id>` event frames with
   roles `vernacular`, `gloss`, `translation`, `source-page`.
4. Cross-references (synonyms, antonyms, derivations) become
   relational predicates.

### 6.3 PDF dictionaries (C)

Same as 6.1 but with **per-entry detection**, not per-section. A
dictionary entry has a typographic shape (bolded headword, POS in
italics, gloss in roman, examples indented). Detection heuristics
plus an LLM cleanup pass usually achieve >95% recall on well-typeset
dictionaries; lower for handwritten ones.

### 6.4 Treebanks (D)

CoNLL-U is line-oriented; each line is one token with ten tab-separated
fields. Mapping:

| CoNLL-U field | donto encoding                                                          |
|---------------|-------------------------------------------------------------------------|
| ID            | `<sent_iri>/tok/<i>` IRI                                                |
| FORM          | `ontolex:writtenRep` literal                                            |
| LEMMA         | `ontolex:lemma` predicate to a `LexicalEntry` IRI                       |
| UPOS          | `ud:upos` to a UD POS IRI                                               |
| XPOS          | `ud:xpos` literal                                                       |
| FEATS         | One predicate per UD feature (`ud:Case`, `ud:Number`, `ud:Tense`, …)    |
| HEAD          | `ud:head` to another token IRI                                          |
| DEPREL        | `ud:deprel` to a UD relation IRI                                        |
| DEPS          | additional `ud:enhancedDep` rows                                        |
| MISC          | structured parse → predicates                                           |

Sentence metadata becomes `donto_document` (one per text) →
`donto_document_revision` per CoNLL-U file → context per sentence.

### 6.5 UniMorph (E)

UniMorph TSV: `lemma TAB inflection TAB feature_bundle`. Each row
becomes an event-frame statement of kind
`unimorph:InflectionAttestation` with roles `lemma`, `form`, and one
role per UniMorph feature (`unimorph:NUM=PL`, etc.). Aggregate over
lemmas to reconstruct paradigms.

### 6.6 Field recordings (H)

ELAN/EAF files are XML containing tier-aligned annotations referencing
audio/video timecodes. Mapping:

1. Register the recording as a `donto_document` with media type and
   archive identifier.
2. Each ELAN tier becomes a layer of `donto_span` rows with `region`
   set to a timecode range.
3. Each annotation row becomes a `donto_mention` plus the appropriate
   linguistic statements (transcription, gloss, free translation,
   speaker tier, etc.).
4. Speaker information goes into `donto_agent` rows; speakers are
   bound to the recording context via `donto_agent_binding`.

### 6.7 Field notes (I)

Same as 6.1, lower confidence floor. Initial maturity often L0 because
field notes frequently lack final analytical commitments.

### 6.8 Catalogue records (J)

Pure metadata ingest. Use the catalogue as a discovery layer that
points to the underlying material. Each catalogue record becomes a
`donto_document` plus a stub context.

### 6.9 Comparative wordlists (K)

CSV ingester with mapping file. Common formats: Swadesh lists, IELex
JSON, NorthEuralex, ASJP. Each row is a (language, concept, form)
triple; emit `<lex_iri> sense:cognateOf <concept_iri>` and form data.

---

## 7. Comparative-database integrations (CLDF stack)

The Cross-Linguistic Data Formats (CLDF) project standardizes how
typological / lexical / grammatical data is published. Most modern
comparative databases are CLDF-native or have CLDF mirrors:

| Database     | What it codes                                            | CLDF available? |
|--------------|----------------------------------------------------------|-----------------|
| WALS         | 192+ structural features for ~2,600 languages            | Yes             |
| Grambank     | 195 grammatical features for 2,467 languages             | Yes             |
| AUTOTYP      | 200+ fine-grained morphosyntactic variables, ~1,225 langs| Yes             |
| PHOIBLE      | 3,020 phonological inventories, 3,183 segments           | Yes             |
| ValPaL       | Valency patterns for 70 verb meanings                    | Yes             |
| APiCS        | 130 features for 76 pidgin/creole languages              | Yes             |
| SAILS        | Grammatical properties of South American Indigenous langs| Yes             |
| Glottolog    | Language inventory, classification, bibliography         | Yes             |
| Concepticon  | Standardised concept inventories                         | Yes             |
| CLICS        | Cross-linguistic colexifications                         | Yes             |
| WOLD         | World loanword database                                  | Yes             |

CLDF datasets are JSON-LD-conformant. Each has a `metadata.json`
declaring tables and column types; tables are CSV. Tables typically
include `LanguageTable`, `ParameterTable`, `CodeTable`, `ValueTable`,
`ExampleTable`, `FormTable`, etc.

### 7.1 CLDF importer — sister project `packages/donto-cldf`

A new Rust crate that wraps the existing JSON-LD ingester
(`packages/donto-ingest/src/jsonld.rs`) with CLDF-specific conventions.
Responsibilities:

1. **Parse `metadata.json`** to discover tables, foreign keys, value
   types, and the dataset's IRI namespace.
2. **Create one context per CLDF dataset**, e.g.
   `ctx:source/grambank/2024-09`. Bibliographic metadata becomes
   statements about the context.
3. **For each LanguageTable row**, ensure the language exists as a
   subject IRI under `lang:<glottocode>` and emit identifier statements
   (Glottocode, ISO 639-3, name, family, location, …).
4. **For each ParameterTable row**, register a predicate via
   `POST /descriptors/upsert` with the parameter's name, description,
   and (where applicable) examples. The predicate IRI is
   `<dataset>:Param/<id>` (e.g., `grambank:GB148`).
5. **For each CodeTable row**, register a value IRI under
   `<dataset>:Code/<param>/<id>`.
6. **For each ValueTable row**, emit
   `<lang_iri> <param_iri> <value_iri>` under the dataset context with
   maturity L1, plus a `donto_evidence_link` to the row's source
   (typically a citation already in the CLDF).
7. **For each ExampleTable row**, create an event frame with roles
   for vernacular, segmentation, gloss, translation, and metadata.
8. **For each FormTable row** (e.g., wordlists), emit
   `<lex_iri> ontolex:writtenRep "<form>"@<lang>` plus concept linkage.

Because CLDF is JSON-LD, much of step 6 is mechanical. The wrapper's
job is mostly steps 1–5 (registering languages, predicates, codes) so
that step 6 has the right IRIs in place.

### 7.2 Invariants the importer must preserve

- **No row collapse across datasets.** Two datasets coding the same
  feature should produce two statements (one per source context). PAL
  (§10) handles the equivalence; the importer never picks a winner.
- **Idempotent re-run.** Re-importing the same release version is a
  no-op (donto's content-hash uniqueness handles this). Re-importing a
  newer release adds new statements without touching old ones; old
  statements are still queryable via `PRESET as_of`.
- **Per-row source attribution.** When a CLDF value cites Hammarström
  2018 or a specific grammar, the citation becomes a
  `donto_evidence_link` in addition to the dataset-wide context.

### 7.3 Per-database notes

**WALS.** Many WALS feature values are coarse-grained ("dominant order
SVO"). Treat as L1 for typological queries; do not promote without
re-checking against a primary source.

**Grambank.** Mostly binary (yes/no/?) features. Easy to ingest, easy
to align cross-schema with Grambank as a common spine.

**AUTOTYP.** High-resolution, sometimes overlapping variables. Use as
the most granular schema; align coarser schemas into AUTOTYP via PAL
where applicable.

**PHOIBLE.** Each inventory is one source. Keep multiple inventories
per language (PHOIBLE itself does), modeled as separate contexts —
inventories from different field notes legitimately disagree.

**ValPaL.** Valency patterns are inherently event-frame data. The
70-verb questionnaire produces, per language, a set of attestations
with role marking patterns. Map directly to event frames.

**APiCS / SAILS.** Same pattern as WALS / Grambank, scoped to
contact and South American languages respectively.

**Glottolog.** Special role: it's the language registry (§5). Ingest
it first; it provides the IRI scaffolding everything else hangs on.

**Concepticon.** Special role: it's the concept registry. Use its
concept IRIs as objects of `sense:concept` predicates to align
wordlists across languages.

---

## 8. Linguistic ontology and predicate vocabulary layer

donto's predicate alignment is structural — it doesn't care about
linguistic conventions. The vocabulary layer makes those conventions
explicit so that newly-extracted claims land on the same predicates
existing tools (Linguist's Toolbox, FieldWorks, LingPy, lgpy, lexibank)
already understand.

### 8.1 Vocabularies to register

| Vocabulary       | Coverage                                        | URI prefix                                      |
|------------------|-------------------------------------------------|-------------------------------------------------|
| OntoLex-Lemon    | Lexicon (entries, forms, senses)                | `http://www.w3.org/ns/lemon/ontolex#`           |
| lexinfo          | Linguistic categories (POS, case, number, …)    | `http://www.lexinfo.net/ontology/3.0/lexinfo#`  |
| GOLD             | General Ontology for Linguistic Description     | `http://purl.org/linguistics/gold/`             |
| OLiA             | Ontologies of Linguistic Annotation             | `http://purl.org/olia/olia.owl#`                |
| Universal Dependencies | UPOS + UD features                        | `https://universaldependencies.org/u/`          |
| UniMorph         | Inflectional features                           | `https://unimorph.github.io/schema/`            |
| Glottolog        | Languages                                       | `https://glottolog.org/resource/languoid/id/`   |
| Concepticon      | Concepts                                        | `https://concepticon.clld.org/parameters/`      |
| CLDF             | Dataset metadata terms                          | `https://cldf.clld.org/v1.0/terms.rdf#`         |
| SKOS             | Cross-vocabulary mappings                       | `http://www.w3.org/2004/02/skos/core#`          |
| Dublin Core      | Bibliographic                                   | `http://purl.org/dc/terms/`                     |
| BIBO             | Bibliography                                    | `http://purl.org/ontology/bibo/`                |
| TEI              | Text-encoding metadata                          | `http://www.tei-c.org/ns/1.0/`                  |

### 8.2 Predicate registration pattern

For each predicate, call `POST /descriptors/upsert` with:

```jsonc
{
  "predicate": "lexinfo:case",
  "label": "case",
  "gloss": "Grammatical case feature on nominal forms",
  "subject_type": "ontolex:Form",
  "object_type": "lexinfo:CaseFeature",
  "domain": "linguistics/morphosyntax",
  "example_subject": "lex:domus/form/sg-acc",
  "example_object": "lexinfo:accusative",
  "source_sentence": "Latin 'domum' bears accusative case.",
  "cardinality": "many_to_one",
  "embedding": [/* float32[768] from any embedding model */]
}
```

After registration, semantic search via `/descriptors/nearest` lets a
later ingest find the right pre-existing predicate instead of minting
a new one.

### 8.3 Linguistic feature domains and minimum predicate set

The starting predicate inventory (§Appendix A) should cover, at
minimum, these domains. Each item maps to one or more registered
predicates.

| Domain                   | Examples                                                                                  |
|--------------------------|-------------------------------------------------------------------------------------------|
| Phonology                | inventory, segment, distinctive feature, allophone, phonotactic constraint                 |
| Prosody                  | stress, tone, length, foot, intonational contour                                          |
| Morphology               | morpheme, allomorph, conditioning environment, morpheme type, inflectional vs derivational |
| Word classes             | UD POS, lexinfo POS, language-specific POS                                                |
| Inflection               | paradigm, cell, exponent, syncretism, defective paradigm                                  |
| Derivation               | derivational rule, base, output, productivity                                             |
| Case                     | absolutive, ergative, nominative, accusative, locative, allative, ablative, instrumental, …|
| Alignment                | accusative, ergative, split, fluid-S, active-stative, differential                        |
| Pronouns                 | person, number, clusivity, gender, free vs bound                                          |
| Demonstratives           | proximal, medial, distal, visible/invisible, spatial vs temporal                          |
| TAM                      | tense, aspect, mood, evidentiality, polarity                                              |
| Negation                 | clausal, constituent, prohibitive, negative existential                                   |
| Interrogative            | polar, content, particle, intonational                                                    |
| Valency / voice          | transitive, intransitive, passive, antipassive, causative, applicative, reflexive, reciprocal |
| Phrase syntax            | NP order, modifier position, possession                                                   |
| Clause syntax            | constituent order, argument structure, copular, existential, non-verbal                   |
| Complex clauses          | coordination, subordination, complementation, relativisation, switch reference            |
| Particles & clitics      | discourse, focus, topic, second-position, modal                                           |
| Information structure    | topic, focus, contrast, givenness                                                         |
| Discourse                | sequencing, foreground/background, clause chaining, tail-head                             |
| Sociolinguistics         | dialect, register, genre, speaker variation                                               |
| Documentation metadata   | speaker, recording date, location, equipment                                              |

Appendix A provides a starter list of ~120 predicate IRIs covering
this space.

---

## 9. Data-model walkthroughs with worked examples

This section makes the model concrete by walking three different kinds
of linguistic claim end to end.

### 9.1 Example: a phoneme inventory entry

Claim: "Language L has the consonant /p/ in its inventory, attested in
PHOIBLE inventory P-1234."

**Triples (simplified):**

```
ctx = ctx:source/phoible/2.0/inv/P-1234
s   = lang:abcd1234/phoneme/p
p   = phoible:hasSegment
o   = phoible:Segment/0070            (IPA codepoint as IRI)
flags = polarity=asserted, maturity=1
valid_time = (-infinity, infinity)
tx_time = [now, ∞)
content_hash = sha256(s||p||o||valid||ctx)
```

Plus descriptive overlays:

```
lang:abcd1234/phoneme/p phoible:place "labial"
lang:abcd1234/phoneme/p phoible:manner "stop"
lang:abcd1234/phoneme/p phoible:voicing "voiceless"
```

If a second source disagrees (no /p/ in this language's inventory), it
asserts a separate row with `polarity=negated` under
`ctx:source/<other>`. Both rows persist; `arguments/frontier` surfaces
the disagreement.

### 9.2 Example: a morpheme with conditioned allomorphy

Claim: "Morpheme `LOC` in language L has two allomorphs `-ngka` (after
consonant-final stem) and `-ka` (after vowel-final stem)."

This is n-ary: morpheme + environment + form. Use event frames
(migration `0054_event_frames.sql`).

**Frame: `morph:loc/L/exposure/1`**

```
morph:loc/L                    rdf:type            gold:Morpheme
morph:loc/L                    gold:hasGloss       "LOC"
morph:loc/L                    morph:realizedAs    morph:loc/L/exp/1
morph:loc/L                    morph:realizedAs    morph:loc/L/exp/2

morph:loc/L/exp/1              rdf:type            morph:Exponent
morph:loc/L/exp/1              ontolex:writtenRep  "-ngka"@L
morph:loc/L/exp/1              morph:envCondition  cond:after-cons-final

morph:loc/L/exp/2              rdf:type            morph:Exponent
morph:loc/L/exp/2              ontolex:writtenRep  "-ka"@L
morph:loc/L/exp/2              morph:envCondition  cond:after-vowel-final
```

Now any instance attestation is just:

```
attestation:1 morph:realizes morph:loc/L/exp/1
attestation:1 morph:onStem  lex:wungar/form/cf
attestation:1 morph:source  src:patz1982/p47
```

A Lean shape (§13) checks: every attestation's stem satisfies the
allomorph's environment condition.

### 9.3 Example: a typological feature

Claim: Grambank GB148 ("Is there a morphological antipassive marked on
the lexical verb?") = "yes" for language L.

```
ctx = ctx:source/grambank/2024-09
s   = lang:abcd1234
p   = grambank:GB148
o   = grambank:Code/GB148/1                  (= "yes")
flags = polarity=asserted, maturity=1
content_hash = sha256(...)
```

Plus an evidence link to the Grambank citation row, plus alignment
statements:

```
grambank:GB148 align:close_match wals:108
grambank:GB148 align:sub_property_of typology:antipassiveMarking
```

Now a query in SPARQL using WALS terms:

```sparql
PREFIX wals: <http://wals.info/feature/>
SELECT ?lang
WHERE { ?lang wals:108 wals:108-1 . }
```

…with PAL closure expansion will return the language even though the
underlying row was coded under `grambank:GB148`.

### 9.4 Example: an inflectional paradigm cell

UniMorph row: `dormir TAB durmió TAB V;PST;3;SG`

**Event frame: `paradigm:dormir/cell/v.pst.3.sg`**

```
paradigm:dormir                          rdf:type            unimorph:Lexeme
paradigm:dormir                          ontolex:lemma       "dormir"@es
paradigm:dormir/cell/v.pst.3.sg          rdf:type            unimorph:Cell
paradigm:dormir/cell/v.pst.3.sg          unimorph:V           "V"
paradigm:dormir/cell/v.pst.3.sg          unimorph:Tense       unimorph:PST
paradigm:dormir/cell/v.pst.3.sg          unimorph:Person      unimorph:3
paradigm:dormir/cell/v.pst.3.sg          unimorph:Number      unimorph:SG
paradigm:dormir/cell/v.pst.3.sg          ontolex:writtenRep   "durmió"@es
paradigm:dormir/cell/v.pst.3.sg          unimorph:ofLexeme    paradigm:dormir
```

Queries can then ask "show all cells for this lexeme" or "show all
languages where this person/number cell shows stem-vowel alternation".

### 9.5 Example: a syntactic example with IGT

A grammar example block:

```
gun.duy-ngka       wungar      bama-nga
sun-LOC            walk        man-ABL
'The man walks in the sun.'
```

Stored as an event frame:

```
ex:1            rdf:type            ling:IGTExample
ex:1            ling:vernacular     "gun.duy-ngka wungar bama-nga"@L
ex:1            ling:segmented      "gun.duy-ngka wungar bama-nga"@L
ex:1            ling:gloss          "sun-LOC walk man-ABL"
ex:1            ling:translation    "The man walks in the sun."@en
ex:1            dct:source          src:hypothetical-grammar
ex:1            ling:atSpan         span:patz1982/47/12-14
```

Plus per-token statements connecting tokens to the morphemes they
realize:

```
ex:1/tok/1      morph:realizes      morph:gunduy/L/exp/cf
ex:1/tok/1      morph:realizes      morph:loc/L/exp/1
ex:1/tok/2      morph:realizes      morph:wungar/L/exp/cf
…
```

A Lean shape checks gloss/segmentation alignment.

---

## 10. Predicate alignment across linguistic schemas

The same conceptual feature appears under many names. PAL exists for
this exact problem.

### 10.1 Worked alignment graph

Take "ergative case marking on full noun phrases":

| Schema               | Identifier                    | Note                         |
|----------------------|-------------------------------|------------------------------|
| WALS                 | `wals:Feature98`              | Alignment of case marking    |
| Grambank             | `grambank:GBxxx`              | Binary marker                |
| AUTOTYP              | `autotyp:CASE.ERGATIVE.NP`    | More granular                |
| UD                   | `ud:Case=Erg`                 | Token-level                  |
| UniMorph             | `unimorph:ERG`                | Token-level                  |
| GOLD                 | `gold:ergativeCase`           | Ontological                  |
| lexinfo              | `lexinfo:ergativeCase`        | OntoLex-Lemon ecosystem      |
| Project-local        | `kuya:ergativeCase`           | Patz's ERG label             |

Alignment registrations:

```
ud:Case=Erg            align:exact_equivalent  unimorph:ERG
ud:Case=Erg            align:exact_equivalent  lexinfo:ergativeCase
lexinfo:ergativeCase   align:exact_equivalent  gold:ergativeCase
autotyp:CASE.ERGATIVE.NP align:close_match     grambank:GBxxx
wals:Feature98         align:close_match       grambank:GBxxx
kuya:ergativeCase      align:exact_equivalent  ud:Case=Erg
```

After `POST /align/rebuild`, the closure index covers the full
component. A query against any of these predicates returns rows coded
under any of the others (subject to alignment kind: `exact_equivalent`
expands freely; `close_match` expands only with `PREDICATES EXPAND` and
not in `STRICT` mode).

### 10.2 Where alignment must NOT be asserted

- Between schemas at different levels of granularity. AUTOTYP often
  splits a WALS feature into several. Use `sub_property_of`, not
  `exact_equivalent`.
- When the value spaces don't align cleanly. WALS values 1–4 don't
  always map 1-to-1 to Grambank's binary. Sometimes you need
  `decomposition` (one schema's value decomposes into multiple in
  another).
- When sources disagree on what the feature *means*. Then
  `not_equivalent` is the right registration; this is explicit
  negative knowledge.

### 10.3 Bootstrap alignment registry

Sister project `packages/donto-ling-align` ships a curated alignment
seed file (TSV) covering the most common cross-schema mappings. The
file is literature-grounded — every row cites the typological work
(e.g., the WALS chapter introduction) that justifies the mapping.

---

## 11. Entity resolution for languages, lexemes, morphemes

Three resolution problems show up in practice:

### 11.1 Language identity

Different sources call the same language different things. Glottocode
is the canonical ID, but historical sources predate it. Pattern:

1. Mint the canonical IRI `lang:<glottocode>`.
2. Register every alternate name and identifier as
   `donto_entity_symbol` rows pointing at the canonical IRI.
3. For uncertain identities (e.g., a 19th-century manuscript
   referencing a "language" that may be one of two modern lects),
   create an `donto_identity_hypothesis` and assert competing
   identity edges under each.

### 11.2 Lexeme identity

Two dictionary entries with identical headwords may or may not be the
same lexeme — could be homonyms or polysemy. Resolve by matching on
(form, POS, sense) tuples, with low confidence by default. Manual
review promotes to higher maturity.

### 11.3 Morpheme identity

The hardest case. `-ngka` on page 47 and `-ngka` on page 113 might be
the same morpheme, two morphemes with identical exponents, or one
morpheme with two functions. Resolve via:

1. Initial automatic clustering by (form, gloss).
2. Explicit identity edges asserted by analysts, scoped to a
   hypothesis context.
3. Shapes that detect potential conflicts (same form, different
   environment conditions, no identity edge).

---

## 12. The maturity ladder applied to linguistic claims

donto's L0–L4 maturity ladder corresponds naturally to the analytical
lifecycle of a linguistic claim.

| Level | Donto label   | Linguistic interpretation                                                                 |
|-------|---------------|-------------------------------------------------------------------------------------------|
| L0    | raw           | OCR'd text, untranscribed audio, unsegmented field note, raw ELAN annotation              |
| L1    | parsed        | Predicate registered; structure valid; LLM-extracted feature without span anchor          |
| L2    | linked        | Anchored to specific page, line, or timestamp; evidence chain complete                    |
| L3    | reviewed      | Cross-checked against another source, no unresolved shape violations or open obligations  |
| L4    | certified     | Lean-verifiable proof attached (paradigm completeness, IGT alignment, phonotactic rule)   |

`donto_why_not_higher(stmt_id)` returns the obligations blocking
promotion. Typical reasons for linguistic claims:

| Obligation kind            | Example                                                                |
|----------------------------|------------------------------------------------------------------------|
| `needs_source_support`     | Extracted from grammar but not yet anchored to a specific page         |
| `needs_disambiguation`     | "ngka" mentioned multiple times; coreference unresolved                |
| `needs_human_review`       | Confidence below threshold; analyst sign-off required                  |
| `needs_temporal_grounding` | Source attests "this was so in 1870" but `valid_time` left infinite    |
| `needs_dialect_scoping`    | Claim made under `lang:X` but really applies to `lang:X/dialect/Y`     |
| `needs_paradigm_completion`| Cell missing in paradigm; either fill in or assert `gold:DefectiveParadigm` |

A reviewer queue is just `POST /obligations/list-open` filtered by
priority and type.

---

## 13. Lean shape catalogue for linguistics

Shapes are validators that run over statements. donto's shape
infrastructure (migrations `0008_shape.sql`, `0015_shape_annotations.sql`,
`0045_auto_shape_validation.sql`) supports both Rust built-ins and
Lean-authored validators dispatched to `donto_engine`.

### 13.1 Recommended starter shapes

| Shape                                         | Domain               | Validation                                                                                     |
|-----------------------------------------------|----------------------|------------------------------------------------------------------------------------------------|
| `ling:IGTAlignment`                           | examples             | Token count of vernacular line equals token count of gloss line                                |
| `ling:GlossAbbreviationsRegistered`           | examples             | Every gloss abbreviation (ERG, ABS, PST…) is registered in the project gloss vocabulary        |
| `ling:ParadigmCompleteness`                   | inflection           | For lexeme + word class, every required cell is filled or marked `gold:DefectiveParadigm`      |
| `ling:AllomorphCoverage`                      | morphology           | For morpheme M with allomorphs {a₁,…,aₙ} and conditions {c₁,…,cₙ}, the conditions partition the relevant environments |
| `ling:AllomorphConditionConsistency`          | morphology           | Every attestation of an allomorph satisfies its environment condition                          |
| `ling:PhonotacticConstraint`                  | phonology            | Every form satisfies the language's phonotactic constraints (defined per language)             |
| `ling:LexemePOSConsistency`                   | lexicon              | All forms of a lexeme have the same POS                                                        |
| `ling:UDFeatsValid`                           | corpus               | UD `feats` only uses values from the language's feature inventory                              |
| `ling:DependencyTreeConnected`                | corpus               | Every sentence's UD parse forms a single connected tree with one root                          |
| `ling:CaseInventoryClosed`                    | morphosyntax         | Every case-marked form's case feature is in the language's declared case inventory             |
| `ling:DialectScopingPresent`                  | governance           | Every claim is scoped to a `lang:` or `lang:.../dialect/...` context                          |
| `ling:SourceAnchoringPresent`                 | governance           | Every L2+ claim has at least one `donto_evidence_link`                                         |
| `ling:TimeBoundsExplicit`                     | governance           | Claims tagged historical have an explicit `valid_time` upper bound                             |

### 13.2 Lean encoding pattern

Each shape lives in `packages/lean/Donto/Shapes/Ling/<Name>.lean` and
exposes a single function the Rust dispatcher calls. The full Lean
trip is reserved for L4 promotion; L3 review uses Rust built-ins
where possible for speed.

### 13.3 Failure handling

Shape failure produces a `donto_shape_annotation` with one of three
severities:

- **violate** — claim cannot reach L3
- **warn** — informational; does not block
- **pass** — explicit confirmation

Severity policy is per-shape and per-context; some projects accept
warnings, others don't.

---

## 14. Access governance — CARE, AIATSIS, ELAR, generic ACLs

This is the largest gap in current donto. It is also the most
important piece to design before any restricted material is ingested.

### 14.1 Why this matters

Linguistic data — especially endangered, Indigenous, or
ceremonial-knowledge data — frequently carries access conditions that
are stricter than open licensing. Examples:

- Speakers consented to research use but not redistribution.
- Material is restricted by gender, kinship, initiation status, or
  ceremonial role.
- A community holds collective authority over the corpus.
- An archive (e.g., AIATSIS, ELAR, PARADISEC) stipulates per-collection
  access conditions inherited by anything derived from the collection.

Frameworks:

- **CARE Principles**: Collective Benefit, Authority to Control,
  Responsibility, Ethics.
- **AIATSIS Code of Ethics**: Indigenous self-determination, leadership,
  impact and value, sustainability and accountability.
- **ELDP / ELAR access**: per-deposit conditions ranging from "open"
  to "depositor permission required".
- **PARADISEC access tiers**: similar.
- **OCAP (Canada)**: Ownership, Control, Access, Possession.

A donto-based linguistic database that does not honour these is not
ethically deployable for endangered or Indigenous-language work.

### 14.2 Proposed schema — sister migration `XXXX_access_policy.sql`

Three tables plus a query-time enforcement layer:

```sql
-- 1. Policy definitions
CREATE TABLE donto_access_policy (
    policy_id          uuid PRIMARY KEY,
    name               text NOT NULL UNIQUE,
    framework          text NOT NULL,        -- 'CARE', 'AIATSIS', 'ELDP', 'OCAP', 'project-local'
    authority          text NOT NULL,         -- who decides (community, family, depositor, …)
    description        text,
    reuse_conditions   text,                  -- machine-readable + human-readable
    restricted_flag    bool NOT NULL DEFAULT false,
    review_required    bool NOT NULL DEFAULT false,
    expires_at         timestamptz,
    created_at         timestamptz NOT NULL DEFAULT now()
);

-- 2. Assignments to targets
CREATE TABLE donto_access_assignment (
    assignment_id      uuid PRIMARY KEY,
    target_kind        text NOT NULL,         -- 'document', 'context', 'statement', 'span'
    target_id          uuid NOT NULL,
    policy_id          uuid NOT NULL REFERENCES donto_access_policy(policy_id),
    assigned_by        text NOT NULL,
    assigned_at        timestamptz NOT NULL DEFAULT now(),
    valid_time         daterange NOT NULL DEFAULT daterange(NULL, NULL, '[)'),
    notes              text,
    UNIQUE (target_kind, target_id, policy_id)
);

-- 3. Caller attestations
CREATE TABLE donto_access_attestation (
    attestation_id     uuid PRIMARY KEY,
    caller             text NOT NULL,         -- agent IRI
    policy_id          uuid NOT NULL REFERENCES donto_access_policy(policy_id),
    granted_by         text NOT NULL,         -- authority that granted
    granted_at         timestamptz NOT NULL DEFAULT now(),
    expires_at         timestamptz,
    rationale          text NOT NULL,         -- why granted; auditable
    revoked_at         timestamptz
);
```

### 14.3 Enforcement model

Two enforcement points:

1. **Sidecar middleware** — `apps/dontosrv` adds a
   `require_access_policies(caller, policy_ids)` check to every read
   path. The check (a) finds all policies covering the requested
   target via `donto_access_assignment`, (b) confirms the caller has
   non-revoked, non-expired attestations for each, (c) returns a 403
   if any are missing.
2. **Query-time row filtering** — `dontoql` and `sparql` evaluators
   are extended to filter out statements whose context (or any parent
   in the context tree) has an unsatisfied policy. Filter happens after
   binding but before result construction; users see "this query has
   N restricted rows hidden" rather than partial data without notice.

### 14.4 Policy-scoped contexts

A simpler alternative for many cases: contexts of kind `restricted`
inherit a default policy. Statements asserted into them require
attestation. Examples:

- `ctx:source/aiatsis/MS-12345/restricted` — policy = AIATSIS access
  level for that manuscript.
- `ctx:lang/<glot>/dialect/<x>/cultural-restricted` — policy = community
  authority for cultural knowledge in that dialect.

### 14.5 Audit and reciprocity

Every restricted-row read produces a `donto_audit` row recording the
caller, the policy, and the attestation used. This audit log can be
exposed back to communities so they see how their material is being
used — a CARE / AIATSIS expectation, not an optional nicety.

### 14.6 Defaults and safety

- **Fail-closed.** Any target with at least one assigned policy is
  hidden unless the caller has explicit attestation.
- **No silent leakage.** Aggregations that include restricted rows must
  either redact or refuse; never return a count that lets a caller
  reconstruct restricted content.
- **Explicit downgrade.** Promoting a row from restricted to public
  requires an `agent_action` trail; never automatic.

### 14.7 Why this is sister-project shaped

It crosses migration, sidecar, query evaluator, and TUI. It needs
careful design review with people who understand the relevant
frameworks (community advisors, archivists). It is not a small change.
But the schema above is enough to get a useful first pass live.

---

## 15. CLDF export and FAIR interop contract

For the dataset to be FAIR, it must be exportable as CLDF. The export
half of `packages/donto-cldf` (§7.1) handles this.

### 15.1 Mapping back to CLDF tables

| CLDF table       | Source query                                                                          |
|------------------|---------------------------------------------------------------------------------------|
| `LanguageTable`  | All `lang:<glottocode>` subjects + their identifier predicates                        |
| `ParameterTable` | All `donto_predicate` rows in the linguistics domain                                  |
| `CodeTable`      | Distinct values of categorical predicates                                              |
| `ValueTable`     | All `donto_statement` rows under typological-domain predicates                        |
| `FormTable`      | All `ontolex:writtenRep` rows                                                         |
| `ExampleTable`   | All `ling:IGTExample` event frames                                                    |
| `CognateTable`   | All `sense:cognateOf` rows                                                            |

### 15.2 Export modes

- **Full export** — entire database matching a domain filter; produces
  a release-versioned CLDF dataset.
- **Language scope** — all rows about one language; useful for sharing
  a language profile.
- **Source scope** — one source's contribution as CLDF; useful for
  giving a contributor a citable artifact.
- **Hypothesis scope** — a hypothesis context's view; useful for
  reproducing a paper's analysis.

### 15.3 Provenance preservation

Every exported CLDF row carries:

- A `Source_Bibtex_Key` pointing to the donto context's bibliography.
- A `Comment` field with the donto statement IRI for round-trip
  traceability.
- A `Confidence` and `Maturity` column (CLDF allows additional
  columns; declared in `metadata.json`).

### 15.4 Round-trip invariant

Re-importing an exported CLDF dataset produces no new statements (only
duplicate-collapse via content hash). Any new rows would indicate a
lossy export and is a bug.

---

## 16. Query patterns (DontoQL and SPARQL)

A few canonical queries that every linguistic project will run, with
the donto idiom.

### 16.1 "Show me every claim about case marking in language L from any source"

```dontoql
PRESET anywhere
MATCH ?s ?p ?o
WHERE ?s startsWith "lang:abcd1234"
FILTER ?p IN linguistics:case-marking-cluster
PREDICATES EXPAND
PROJECT ?s, ?p, ?o, source(?s, ?p, ?o)
```

The `linguistics:case-marking-cluster` is a predicate group — a saved
set of aligned predicates that PAL closure expands.

### 16.2 "What disagreements exist about feature F across my sources?"

```sql
SELECT * FROM donto_arguments_frontier
WHERE feature_predicate = 'grambank:GBxxx'
  AND under_pressure = true;
```

Or as a HTTP call: `GET /arguments/frontier?predicate=grambank:GBxxx`.

### 16.3 "Show all paradigm cells for lexeme L"

```sparql
PREFIX um: <https://unimorph.github.io/schema/>
SELECT ?cell ?form ?features WHERE {
  ?cell um:ofLexeme <paradigm:dormir> ;
        ontolex:writtenRep ?form ;
        um:Tense ?tense ;
        um:Person ?person ;
        um:Number ?number .
  BIND(CONCAT(?tense, ".", ?person, ".", ?number) AS ?features)
}
```

### 16.4 "Show every claim about ergative marking in any language, in WALS terms"

```sparql
PREFIX wals: <http://wals.info/feature/>
SELECT ?lang ?value WHERE { ?lang wals:98 ?value . }
```

PAL expansion returns Grambank/AUTOTYP/UD-coded rows alongside
WALS-coded rows because of registered alignment.

### 16.5 "What does our database know about language L as of 2024-12-01?"

```dontoql
PRESET as_of "2024-12-01T00:00:00Z"
MATCH ?p ?o
WHERE ?s = lang:abcd1234
PROJECT ?p, ?o
```

### 16.6 "Under the hypothesis that X and Y are conditioned allomorphs of one morpheme, what does the paradigm look like?"

```dontoql
PRESET under_hypothesis ctx:hyp/L/morpheme-merge-XY
MATCH paradigm:?lex ?p ?o
PROJECT ?lex, ?p, ?o
```

### 16.7 "List all open obligations for language L"

```http
POST /obligations/list-open
Content-Type: application/json

{ "context_prefix": "ctx:lang/abcd1234", "limit": 100 }
```

### 16.8 "Give me a claim card for this single statement"

```http
GET /claim/{statement_uuid}
```

Returns: statement fields, all evidence links, all arguments, all
shape annotations, all open obligations, source documents, alignment
expansions.

---

## 17. Batch extraction app — changes for linguistics

The current `apps/donto-api` extracts using an 8-tier sociology /
genealogy prompt at `helpers.py:64`. For linguistics, we need a parallel
domain-specific prompt and a domain selector.

### 17.1 Required changes (small)

1. **Add `domain` parameter** to `/jobs/extract`, `/jobs/batch`, and
   `/extract-and-ingest`. Default `general`.
2. **Add `prompts/` directory** under `apps/donto-api/` with one
   prompt module per domain. Existing prompt moves to
   `prompts/general.py`. New `prompts/linguistics.py` holds the
   linguistic prompt.
3. **Route in `helpers.py`** based on `domain`. Single dispatch table.
4. **Per-domain output schema.** Linguistics extraction returns
   structured records (morpheme, allomorph, paradigm cell, IGT
   example, feature value) rather than tier-tagged S-P-O triples. The
   `ingest_facts_activity` in `workflows.py` decomposes them into
   donto statements (event frames where appropriate).
5. **Span-aware extraction.** The prompt asks the LLM to return, with
   each claim, a `span` field indicating the character offsets in the
   chunk where the claim was made. The ingest step uses these to create
   `donto_span` rows directly, achieving L2 anchoring at extraction
   time.

### 17.2 Linguistic extraction prompt (sketch)

```
You are a linguistic feature extractor. Given a chunk of text from a
reference grammar, dictionary, or fieldwork source, identify and output
structured claims of the following kinds:

- phoneme: an entry in a phoneme inventory
- morpheme: a morpheme with gloss and category
- allomorph: a morpheme realisation with an environment condition
- paradigm_cell: a (lexeme, features, form) triple
- feature: a typological / morphosyntactic feature value
- igt_example: an interlinear glossed example
- construction: a clause-type or phrase-type description
- lexeme: a dictionary entry
- gloss_definition: an abbreviation defined locally

For every claim, return:
- type: one of the kinds above
- payload: kind-specific fields
- span: [start_char, end_char] in the input text
- confidence: 0.0–1.0
- notes: free-text justification, including any uncertainty

Do not invent claims. Where the text is descriptive prose without a
specific claim, return nothing.
```

The full prompt (Appendix B sketch) is several hundred lines once
schemas, examples, and fallback rules are filled in.

### 17.3 Confidence → maturity mapping (unchanged)

`helpers.py:39 confidence_to_maturity` already maps:
- 0.95+ → L4
- 0.80–0.94 → L3
- 0.60–0.79 → L2
- 0.40–0.59 → L1
- <0.40 → L0

For linguistics, automatic L4 should be **gated** — never auto-promote
above L3 from extraction alone. L4 requires Lean shape attachment.
Add a per-domain ceiling parameter.

### 17.4 Cost estimate

At ~$0.005/article for the existing genealogy prompt (Grok 4.1 Fast
via OpenRouter), a 400-page reference grammar chunked into ~80
sections costs roughly $0.40. A 30,000-entry dictionary processed
entry-by-entry costs roughly $5–15 depending on entry length. Cheap
enough that the bottleneck is review, not LLM cost.

---

## 18. TUI and operational tooling

`apps/donto-tui` already covers most of what's needed. Two linguistic
extensions are worthwhile:

### 18.1 Paradigm view (new tab)

A tab that, given a lexeme, displays the paradigm as a 2-D table
(rows = features, columns = features), highlighting empty cells
(needs-paradigm-completion obligation) and irregular forms.

### 18.2 IGT view

Given an example IRI, render the interlinear gloss with morpheme
alignment, click-through to source span, and shape annotation
indicators (e.g., red cell when alignment count mismatches).

### 18.3 Firehose filters

The existing firehose tab supports action filtering. Adding context
filtering by language IRI lets a community observer watch extraction
of just their language in real time.

### 18.4 Operational scripts

Justfile recipes worth adding:

- `just ling-bootstrap <glottocode>` — register language, ingest
  Glottolog entry, ingest WALS / Grambank / PHOIBLE rows, report
  what was found.
- `just ling-grammar <pdf> <glottocode>` — run the full grammar
  ingest pipeline for one PDF.
- `just ling-export-cldf <glottocode> <out-dir>` — emit CLDF dataset
  for one language scope.

---

## 19. Sister project specifications

Each below is a self-contained workstream. Listed in the order they
should be built.

### S1. `packages/donto-vocab-ling`

**Purpose.** Register the linguistic predicate vocabulary with
descriptors and embeddings.

**Deliverables.**
- A static TSV of predicate definitions covering the domains in §8.3.
- A binary that reads the TSV and idempotently calls
  `POST /descriptors/upsert` for each row.
- Embedding model selection and embedding generation step.
- Documentation of the vocabulary with examples.

**Estimated size.** Small — one Rust crate, ~500 lines, plus the TSV.

### S2. `apps/donto-api/prompts/linguistics.py`

**Purpose.** Domain-specific extraction prompt and output decomposer.

**Deliverables.**
- The prompt itself (§17.2).
- An output schema (Pydantic).
- A decomposer that turns LLM output into donto statements / event
  frames / spans.
- Tests using fixture grammar pages.

**Estimated size.** Small — one Python module + decomposer.

### S3. `packages/donto-cldf`

**Purpose.** CLDF importer and exporter.

**Deliverables.**
- Importer (parses `metadata.json`, walks tables, emits statements).
- Exporter (queries donto, writes CLDF tables).
- CLI subcommand: `donto-cldf import <dir>` and
  `donto-cldf export <out-dir> --scope <iri>`.
- Round-trip test (import then re-export then re-import is a no-op).

**Estimated size.** Medium — one Rust crate, ~2000 lines.

### S4. `packages/donto-migrate/src/grammar_pdf.rs`

**Purpose.** Grammar-PDF migrator.

**Deliverables.**
- PDF text extraction wrapper.
- Section / IGT detection.
- Document registration + chunk batch submission.
- Tests against a public-domain grammar PDF.

**Estimated size.** Medium — added to the existing `donto-migrate`
crate.

### S5. `packages/donto-migrate/src/treebank.rs`, `unimorph.rs`, `phoible.rs`, `wordlist.rs`

**Purpose.** Per-format migrators.

**Deliverables.**
- One migrator per format with idempotent re-run and content-hash
  uniqueness.
- Tests against small fixtures.

**Estimated size.** Small to medium per file.

### S6. `packages/lean/Donto/Shapes/Ling/`

**Purpose.** Lean shape catalogue (§13).

**Deliverables.**
- One `.lean` file per shape with proof obligation and Rust dispatcher
  glue.
- Shape registration migration (one new SQL migration).
- Test corpus that the shapes catch and don't catch what they
  shouldn't.

**Estimated size.** Medium, growing — start with the highest-value
shapes (paradigm completeness, IGT alignment).

### S7. Access governance — `packages/sql/migrations/XXXX_access_policy.sql` + sidecar middleware

**Purpose.** §14.

**Deliverables.**
- Schema migration.
- `dontosrv` middleware enforcing read-side checks.
- Query-evaluator extension for row-level filtering.
- TUI display of policy state on the claim card.
- CLI: `donto access list`, `donto access grant`, `donto access revoke`.
- Documentation aligned with CARE / AIATSIS / OCAP / ELDP frameworks.

**Estimated size.** Large. Build before any restricted material is
ingested.

### S8. `packages/donto-ling-align`

**Purpose.** Curated cross-schema alignment seed (§10.3).

**Deliverables.**
- TSV of alignment rows with citations.
- Idempotent loader.

**Estimated size.** Small loader; the TSV grows with project.

### S9. TUI extensions

**Purpose.** §18.

**Deliverables.**
- Paradigm tab.
- IGT view.
- Language-filter on firehose.

**Estimated size.** Small to medium per feature.

### S10. `packages/donto-ling-export`

**Purpose.** Convenience exports beyond CLDF.

**Deliverables.**
- TEI export (for textual data).
- LIFT export (for lexical data).
- ELAN-EAF export (for time-aligned data, where applicable).

**Estimated size.** Medium per format.

---

## 20. End-to-end pipeline and milestones

Numbered so each milestone is a useful artifact even if the project
stops there.

### M0. Governance bootstrap

S7 migration applied. Default policies registered. Authorisation
middleware enabled. **No restricted material ingested before this
exists.**

### M1. Language registry

Glottolog ingested. ISO 639-3 ingested. Top-level identifier predicates
registered. Useful artifact: a complete language registry queryable by
any of the standard codes.

### M2. Predicate vocabulary

S1 deployed. ~120 predicates registered with descriptors and
embeddings. Useful artifact: a queryable, semantically-searchable
predicate index.

### M3. Comparative-database integration

S3 importer built. Glottolog, WALS, Grambank, PHOIBLE, AUTOTYP,
UniMorph, ValPaL, APiCS, SAILS, Concepticon ingested under per-source
contexts. Useful artifact: a unified comparative database queryable in
each source's native schema.

### M4. Cross-schema alignment

S8 alignment seed loaded. Closure rebuilt. Useful artifact: queries
in any schema return cross-schema results.

### M5. First grammar ingestion

S4 + S2 deployed. One reference grammar processed end-to-end. Useful
artifact: the first grammar's claims integrated with the comparative
data.

### M6. Corpus integration

S5 (treebank, UniMorph, etc.) deployed. Corpus data flowing in.
Useful artifact: token-level evidence joins with feature-level claims.

### M7. Shape validation

S6 deployed. Shapes running. Obligations being raised. Useful
artifact: a continuous-quality view of the database.

### M8. CLDF export

S3 exporter complete. Useful artifact: a citable CLDF release of
project state, refreshable on demand.

### M9. Production review loop

Reviewer queue (`/obligations/list-open`) drives work. Promotions
through L1 → L4 happen routinely. TUI is the primary review interface.

### M10. Steady state

Continuous extraction, continuous validation, continuous CLDF release.
Community partners using the access governance layer to share material
under their terms.

---

## 21. Risks and open questions

### 21.1 Risks

- **Predicate proliferation.** The vocabulary will grow faster than
  alignment. Mitigation: required descriptor + embedding for new
  predicates; semantic-nearest check before mint.
- **Schema drift in upstream CLDF datasets.** Grambank releases v2 with
  feature renumbering. Mitigation: re-import under a new release
  context; old context remains queryable via `PRESET as_of`.
- **OCR quality.** Scanned grammars often produce noisy text. Mitigation:
  per-page OCR confidence on the document revision; obligations raised
  for low-confidence pages.
- **Disagreement between sources without a frontier flag.** A row
  rebuts another row without `arguments/frontier` lighting up.
  Mitigation: scheduled rebuilds of the frontier; alerts in firehose.
- **Access policy bypass.** A statement assigned no policy is treated
  as public; if a policy assignment is forgotten the row leaks.
  Mitigation: shape that fails any context tagged `restricted` without
  policy assignment; default-restricted toggle for entire context
  subtrees.
- **Performance on long-tail queries.** PAL closure can be expensive
  for highly-connected predicate clusters. Mitigation: canonical
  shadows (migration `0053_canonical_shadow.sql`) materialize the
  expansion.

### 21.2 Open questions

- **Token-level vs. feature-level granularity.** Does a single donto
  database hold both UD treebank tokens and typological feature rows,
  or do they live in separate schemas? Current default: one database,
  one schema, separate context subtrees (`ctx:corpus/...` vs.
  `ctx:source/...`). Revisit if performance demands.
- **How to model paradigm gaps.** Explicit `gold:DefectiveParadigm`
  annotations vs. absent rows. Default: explicit annotations because
  they survive cross-schema export.
- **Multilingual / parallel corpora.** Cross-language alignment of
  tokens is its own problem; OntoLex-Lemon's `decomp` module is the
  starting point.
- **Audio storage.** donto stores metadata and timecodes; the audio
  itself lives in object storage (S3, archive). Proposed: `donto_blob`
  table holding URI + checksum + access policy; out of current scope.
- **Embeddings model.** Predicate-descriptor embeddings depend on a
  chosen model. Decision should be made early because re-embedding is
  expensive.
- **Lean shape velocity.** Authoring Lean shapes is non-trivial.
  Project-internal Rust shapes may suffice for years before L4 promotion
  becomes a routine goal.

---

## 22. Appendices

### Appendix A — Starter predicate inventory

This is a starting set, not exhaustive. Each row would receive a full
descriptor (label, gloss, domain, range, examples, embedding) when
registered via `POST /descriptors/upsert`.

#### A.1 Languages and identifiers

```
glottolog:glottocode           lang:* → literal
iso639:code3                   lang:* → literal
wals:code                      lang:* → literal
austlang:code                  lang:* → literal
elcat:id                       lang:* → literal
ethnologue:code                lang:* → literal
lang:familyOf                  lang:* → lang:*
lang:hasDialect                lang:* → lang:*/dialect/*
lang:speakerCount              lang:* → integer
lang:vitality                  lang:* → vitality:*
lang:officialIn                lang:* → country:*
lang:writingSystem             lang:* → script:*
lang:areaCoordinates           lang:* → wkt-literal
```

#### A.2 Lexicon (OntoLex-Lemon)

```
ontolex:LexicalEntry            (class)
ontolex:Form                    (class)
ontolex:LexicalSense            (class)
ontolex:canonicalForm           lex:* → form:*
ontolex:otherForm               lex:* → form:*
ontolex:writtenRep              form:* → text-literal
ontolex:phoneticRep             form:* → IPA-literal
ontolex:lemma                   * → lex:*
ontolex:sense                   lex:* → sense:*
skos:definition                 sense:* → text-literal
ontolex:reference               sense:* → concepticon:*
```

#### A.3 Word classes (lexinfo, UD)

```
lexinfo:partOfSpeech            lex:* → lexinfo:POS-class
ud:upos                         token:* → ud:POS-class
ud:xpos                         token:* → text-literal
```

#### A.4 Inflection (lexinfo, UniMorph, UD)

```
lexinfo:case                    form:* → lexinfo:Case-class
lexinfo:number                  form:* → lexinfo:Number-class
lexinfo:gender                  form:* → lexinfo:Gender-class
lexinfo:person                  form:* → lexinfo:Person-class
lexinfo:tense                   form:* → lexinfo:Tense-class
lexinfo:aspect                  form:* → lexinfo:Aspect-class
lexinfo:mood                    form:* → lexinfo:Mood-class
lexinfo:voice                   form:* → lexinfo:Voice-class
lexinfo:polarity                form:* → lexinfo:Polarity-class
lexinfo:evidentiality           form:* → lexinfo:Evidentiality-class
lexinfo:definiteness            form:* → lexinfo:Definiteness-class
unimorph:Tense                  cell:* → unimorph:Tense-class
unimorph:Person                 cell:* → unimorph:Person-class
unimorph:Number                 cell:* → unimorph:Number-class
unimorph:Gender                 cell:* → unimorph:Gender-class
unimorph:ofLexeme               cell:* → lex:*
ud:Case ud:Number ud:Person ud:Tense ud:Aspect ud:Mood ud:Voice ud:Polarity
```

#### A.5 Morphology (GOLD-aligned, project-extended)

```
morph:Morpheme                  (class)
morph:Exponent                  (class)
morph:realizedAs                morph:Morpheme → morph:Exponent
morph:realizes                  attestation:* → morph:Exponent
morph:envCondition              morph:Exponent → cond:*
morph:onStem                    attestation:* → form:*
morph:morphemeType              morph:* → morph:Type-class
gold:Allomorph                  (class)
gold:DefectiveParadigm          (class)
```

#### A.6 Phonology (PHOIBLE-aligned)

```
phoible:hasSegment              lang:* → phoible:Segment/*
phoible:place                   phoible:Segment/* → text-literal
phoible:manner                  phoible:Segment/* → text-literal
phoible:voicing                 phoible:Segment/* → text-literal
phon:phonotactic                lang:* → constraint:*
phon:syllableStructure          lang:* → text-literal
prosody:stressPattern           lang:* → text-literal
prosody:tonePattern             lang:* → text-literal
```

#### A.7 Syntax / typology (WALS, Grambank, AUTOTYP)

```
wals:Feature1 … wals:FeatureN   lang:* → wals:Feature*-Code/*
grambank:GBxxx                  lang:* → grambank:Code/*
autotyp:VarN                    lang:* → autotyp:Code/*
typology:constituentOrder       lang:* → typology:Order-class
typology:alignment              lang:* → typology:Alignment-class
typology:antipassiveMarking     lang:* → boolean
typology:passiveMarking         lang:* → boolean
typology:causativeMarking       lang:* → boolean
```

#### A.8 Examples and corpus

```
ling:IGTExample                 (class)
ling:vernacular                 ex:* → text-literal
ling:segmented                  ex:* → text-literal
ling:gloss                      ex:* → text-literal
ling:translation                ex:* → text-literal
ling:atSpan                     ex:* → span:*
ud:head                         token:* → token:*
ud:deprel                       token:* → ud:deprel-class
ud:enhancedDep                  token:* → token:*
```

#### A.9 Constructions

```
construction:Construction       (class)
construction:hasRole            construction:* → role:*
construction:roleFiller         attestation:* × role:* → entity:*
construction:template           construction:* → text-literal
construction:meaning            construction:* → text-literal
```

#### A.10 Provenance and bibliography

```
dct:source                      * → src:*
dct:creator                     * → agent:*
dct:date                        * → date-literal
bibo:isbn bibo:doi bibo:issn bibo:locator
tei:respStmt                    src:* → agent:*
```

#### A.11 Disagreement

```
arg:supports arg:rebuts arg:undercuts arg:qualifies
```

#### A.12 Access governance (proposed)

```
access:policy                   * → policy:*
access:authority                policy:* → agent:*
access:reuseConditions          policy:* → text-literal
access:restrictedFlag           policy:* → boolean
```

### Appendix B — Linguistic extraction prompt skeleton

```
SYSTEM: You are a linguistic feature extractor for a knowledge graph
about <language-name> [<glottocode>]. The graph already contains:

- A predicate vocabulary registered in the project (you will receive
  the relevant slice as PREDICATES below).
- A registry of contexts identifying sources and dialects (you will
  receive the relevant slice as CONTEXTS below).

Given the chunk of source text in TEXT, output a JSON list of claim
records. Each claim record has the following fields:

  type       one of: phoneme, morpheme, allomorph, paradigm_cell,
             feature, igt_example, construction, lexeme,
             gloss_definition, sociolinguistic
  payload    type-specific fields, see SCHEMA below
  span       [start_char, end_char] in TEXT where the claim was made
  confidence 0.0–1.0
  notes      free-text justification, including any uncertainty

SCHEMAs:

phoneme {
  segment_ipa: string,
  attributes: { place?, manner?, voicing?, height?, backness?, … }
}

morpheme {
  form: string,
  gloss: string,
  category: string,                // case suffix, tense suffix, …
  word_class_constraint?: string
}

allomorph {
  morpheme_id_or_form: string,
  exponent: string,
  environment: string              // free-text condition, e.g. "after consonant-final stem"
}

paradigm_cell {
  lemma: string,
  features: { person?, number?, case?, tense?, aspect?, … },
  form: string
}

feature {
  predicate: string,               // an IRI or descriptive label
  value: string | number | boolean,
  scope?: string                   // dialect / register if narrower than chunk
}

igt_example {
  vernacular: string,
  segmented: string,
  gloss: string,
  translation: string
}

construction {
  name: string,
  template: string,
  roles: [string],
  examples: [igt_example]
}

lexeme {
  headword: string,
  pos: string,
  senses: [{ gloss: string, examples?: [igt_example] }]
}

gloss_definition {
  abbreviation: string,
  expansion: string
}

sociolinguistic {
  predicate: string,
  value: string,
  scope: string
}

Rules:
- Do not invent claims. If the text describes prose that does not
  contain a specific claim of the above types, return nothing for it.
- If the source text expresses uncertainty ("apparently", "perhaps",
  "the data is unclear"), reflect it in confidence and notes.
- Where a single sentence supports multiple claim records, emit each
  one separately with its own span.
- If the chunk includes IGT examples, capture each as `igt_example`
  records and additionally emit any morpheme/feature claims they
  illustrate.
- Use exact predicate IRIs from PREDICATES when applicable. If the
  appropriate predicate is not in PREDICATES, output the descriptive
  label and let the alignment layer match it post-hoc.

PREDICATES: <project-relevant predicate slice with glosses>
CONTEXTS:    <project-relevant context slice with descriptions>
TEXT:        <chunk text>
```

### Appendix C — Document registry template

When registering a new source via `POST /documents/register`, populate
at minimum:

```jsonc
{
  "iri": "src:patz1982",
  "title": "<full title>",
  "creators": [ "<author IRIs>" ],
  "publisher": "<IRI or text>",
  "year": 1982,
  "isbn": "<...>",
  "doi": "<...>",
  "archive_id": "<...>",
  "media_type": "application/pdf",
  "language_of_metadata": "en",
  "language_documented": "lang:<glottocode>",
  "license": "<spdx or text>",
  "access_policy_id": "<uuid or null>",
  "checksum_sha256": "<hex>",
  "byte_size": 12345678,
  "page_count": 412,
  "ocr_engine": "<...>",
  "ocr_confidence_per_page": [ 0.97, 0.98, … ]
}
```

### Appendix D — Context naming conventions

| Pattern                                            | Use                                            |
|----------------------------------------------------|------------------------------------------------|
| `ctx:lang/<glot>`                                  | Language scope                                 |
| `ctx:lang/<glot>/dialect/<slug>`                   | Dialect scope                                  |
| `ctx:lang/<glot>/register/<slug>`                  | Register / style scope                         |
| `ctx:source/<corpus>`                              | Comparative database (Glottolog, WALS, …)      |
| `ctx:source/<corpus>/<release>`                    | Versioned comparative database                 |
| `ctx:source/<author>/<year>`                       | Cited work (grammar, dictionary, paper)        |
| `ctx:corpus/<name>/sentence/<id>`                  | Per-sentence annotation context                |
| `ctx:hyp/<topic>/<slug>`                           | Hypothesis                                     |
| `ctx:project/<name>`                               | Project-canonical curated view                 |
| `ctx:restricted/<topic>`                           | Default-restricted material                    |

### Appendix E — Migration index (current state, for reference)

```
0001 core                              0035 document_sections
0002 flags                              0036 mentions
0003 functions                          0037 extraction_chunks
0004 migrations                         0038 confidence
0005 presets                            0039 units
0006 predicate                          0040 temporal_expressions
0007 snapshot                           0041 content_regions
0008 shape                              0042 entity_aliases
0009 rule                               0043 candidate_contexts
0010 certificate                        0044 ontology_seeds
0011 observability                      0045 auto_shape_validation
0012 match_scope_fix                    0046 references
0013 search_trgm                        0047 claim_lifecycle
0014 retrofit                           0048 predicate_alignment
0015 shape_annotations                  0049 predicate_descriptor
0016 valid_time_buckets                 0050 alignment_run
0017 reactions                          0051 predicate_closure
0018 aggregates                         0052 match_aligned
0019 fts                                0053 canonical_shadow
0020 bitemporal_canonicals              0054 event_frames
0021 same_meaning                       0055 match_alignment_integration
0022 context_env                        0056 lexical_normalizer
0023 documents                          0057 entity_symbol
0024 document_revisions                 0058 entity_mention
0025 spans                              0059 entity_signature
0026 annotations                        0060 identity_edge
0027 annotation_edges                   0061 identity_hypothesis
0028 extraction_runs                    0062 literal_canonical
0029 evidence_links                     0063 time_expression
0030 agents                             0064 temporal_relation
0031 arguments                          0065 property_constraint
0032 proof_obligations                  0066 class_hierarchy
0033 vectors                            0067 rule_engine
0034 claim_card                         (XXXX access_policy — proposed §14)
```

### Appendix F — Endpoint index (current state, for reference)

| Source                              | Endpoints |
|-------------------------------------|-----------|
| `apps/dontosrv/src/lib.rs:41` →     | `/health`, `/version`, `/sparql`, `/dontoql`, `/dir`, `/shapes/validate`, `/rules/derive`, `/certificates/{attach,verify/:stmt}`, `/subjects`, `/search`, `/history/:subject`, `/statement/:id`, `/contexts`, `/predicates`, `/contexts/ensure`, `/assert`, `/assert/batch`, `/retract`, `/react`, `/reactions/:id`, `/documents/register`, `/documents/revision`, `/evidence/link/span`, `/evidence/:stmt`, `/agents/{register,bind}`, `/arguments/{assert,:stmt,frontier}`, `/obligations/{emit,resolve,open,summary}`, `/claim/:id`, `/alignment/{register,retract,rebuild-closure,runs/start,runs/complete}`, `/descriptors/{upsert,nearest}`, `/shadow/{materialize,rebuild}` |
| `apps/donto-api/main.py` →          | `/firehose/{stream,recent,stats}`, `/health`, `/version`, `/extract-and-ingest`, `/jobs/{extract,batch}`, `/jobs`, `/jobs/{id}`, `/jobs/retry-failed`, `/jobs/{id}/{facts,source}`, `/queue`, `/extract`, `/assert`, `/assert/batch`, `/subjects`, `/search`, `/history/{subject}`, `/statement/{id}`, `/contexts`, `/predicates`, `/query`, `/retract/{id}`, `/connections/{entity}`, `/context/analytics/{ctx}`, `/graph/{neighborhood,path,stats,subgraph,entity-types,timeline/{subject},compare}`, `/align/{register,rebuild,retract/{id},suggest/{p}}`, `/evidence/{id}`, `/claim/{id}`, `/entity/{register,register/batch,identity,identity/batch,membership,{iri}/edges,cluster/{h}/{r},resolve/{iri},family-table}`, `/papers/* (domain-specific)`, `/full-docs`, `/simple-docs`, `/guide` |

### Appendix G — Confidence → maturity mapping (current)

`apps/donto-api/helpers.py:39`:

```
0.95+      → L4 (certified)        — auto-promote disabled for linguistics; gate on shape attachment
0.80–0.94  → L3 (reviewed)
0.60–0.79  → L2 (linked)
0.40–0.59  → L1 (parsed)
<0.40      → L0 (raw)
```

### Appendix H — Glossary

| Term              | Meaning                                                                                       |
|-------------------|-----------------------------------------------------------------------------------------------|
| Bitemporal        | Two time axes: world-time (`valid_time`) and system-time (`tx_time`)                          |
| Paraconsistent    | Tolerates contradictory rows without rejecting either                                         |
| Quad-store        | (subject, predicate, object, context) — context is the fourth element                         |
| PAL               | Predicate Alignment Layer — cross-schema predicate equivalence with closure                   |
| Event frame       | n-ary relation modeled as a frame node with role predicates                                   |
| Maturity          | L0–L4 reliability tier on each statement                                                      |
| Shape             | Validation rule attached to statements                                                        |
| Certificate       | Lean-checkable proof attached to a statement                                                  |
| Obligation        | Open epistemic work (needs-X)                                                                 |
| CLDF              | Cross-Linguistic Data Formats — JSON-LD-conformant standard for linguistic datasets           |
| OntoLex-Lemon     | W3C-blessed lexicon ontology                                                                  |
| GOLD              | General Ontology for Linguistic Description                                                   |
| OLiA              | Ontologies of Linguistic Annotation                                                           |
| IGT               | Interlinear Glossed Text                                                                      |
| UD                | Universal Dependencies — token-level dependency annotation                                    |
| UniMorph          | Universal Morphology — schema for inflectional features                                       |
| WALS              | World Atlas of Language Structures                                                            |
| Grambank          | Grammatical-feature comparative database                                                      |
| AUTOTYP           | Fine-grained typological variable database                                                    |
| PHOIBLE           | Phoneme-inventory database                                                                    |
| ValPaL            | Valency Patterns Leipzig                                                                      |
| CARE              | Indigenous data governance principles (Collective benefit, Authority to control, Responsibility, Ethics) |
| AIATSIS           | Australian Institute of Aboriginal and Torres Strait Islander Studies — Code of Ethics        |
| OCAP              | Indigenous data principles (Ownership, Control, Access, Possession)                           |
| ELDP / ELAR       | Endangered Languages Documentation Programme / Archive                                        |
| PARADISEC         | Pacific And Regional Archive for Digital Sources in Endangered Cultures                       |

---

*End of plan.*

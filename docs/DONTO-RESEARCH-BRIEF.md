# Donto: A Bitemporal Paraconsistent Knowledge Graph — Research Brief

**For**: Expert consultation on novel knowledge representation challenges  
**Author**: Thomas Davis  
**Date**: May 2026  
**System**: https://genes.apexpots.com (35.8M statements, live production)  
**Source**: https://github.com/thomasdavis/donto

---

## 1. Executive Summary

Donto is a production knowledge graph with 35.8 million statements, built from scratch in Rust + PostgreSQL. It stores facts as bitemporal quads with polarity, maturity, and context. It was designed for AI-driven genealogical research where contradictions are valuable, sources are unreliable, and the same real-world entity appears under dozens of names across hundreds of documents.

The system is now at an inflection point. We have a strong **assertion layer** (extracting and storing facts), a strong **predicate alignment layer** (converging freely-minted predicates), and a strong **evidence substrate** (linking statements to source documents). What we don't have is a coherent **entity layer** — subjects and objects are open-world text IRIs with no canonical forms, no type hierarchy, no cross-reference resolution, and no inference.

This document describes what exists, what's missing, and the novel research challenges we face. We are seeking an extremely detailed technical report on how to build the missing pieces.

---

## 2. What Exists Today

### 2.1 Core Statement Model

Every fact in donto is a **quad** with metadata:

```
(subject, predicate, object, context)
  + polarity    ∈ {asserted, negated, absent, unknown}
  + maturity    ∈ {L0=raw, L1=registered, L2=evidenced, L3=validated, L4=certified}
  + valid_time  = [lo, hi)  — when the fact was true in the real world
  + tx_time     = [lo, hi)  — when the fact was recorded/retracted in the database
  + content_hash = SHA256   — for idempotent deduplication
```

Objects can be either an **IRI** (pointing to another entity) or a **literal** (`{v, dt, lang}` — typed JSON value with datatype IRI and optional language tag).

Nothing is ever physically deleted. Retraction closes the `tx_hi` bound — the row remains for historical as-of queries.

### 2.2 Context System

Contexts form a **forest** (tree with multiple roots). Each context has:
- **Kind**: source, snapshot, hypothesis, user, pipeline, trust, derivation, quarantine, custom, system
- **Mode**: permissive (accept any predicate) or curated (only registered predicates)
- **Parent**: optional parent context for hierarchy
- **Environment variables**: sparse (key, value) pairs for advisory metadata

Queries are scoped via **scope descriptors** that include/exclude context subtrees, filter by kind, and set maturity floors. Named presets exist: `anywhere`, `raw`, `curated`, `latest`.

### 2.3 Predicate Alignment Layer (the best-developed part)

LLMs mint predicates freely during extraction. One run says `bornIn`, another says `birthplaceOf`, another says `bornInPlace`. The alignment layer converges them:

**Six relation types:**
| Relation | Meaning | Query behavior |
|----------|---------|---------------|
| `exact_equivalent` | Same meaning, same direction | Direct substitution |
| `inverse_equivalent` | Same meaning, swap subject/object | S↔O swap in query |
| `sub_property_of` | Specific implies general | Upward expansion |
| `close_match` | Similar but not identical | Lower confidence |
| `decomposition` | One predicate = n-ary event frame | Component expansion |
| `not_equivalent` | Explicitly NOT the same | Blocks auto-alignment |

**How it works:**
1. Alignments are registered (manually or via trigram auto-alignment)
2. A **materialized closure table** pre-computes all transitive chains
3. `donto_match_aligned()` expands queries through the closure at query time
4. **Canonical shadows** materialize fully-resolved statements for fast reads

**Supporting infrastructure:**
- **Predicate descriptors**: label, gloss, domain, range, subject/object type hints, embedding vectors, example sentences, cardinality
- **Lexical normalizer**: CamelCase splitting, trigram similarity, auto-suggest
- **Alignment runs**: provenance tracking for batch operations
- **Predicate registry**: status (active/deprecated/merged/implicit), functional/symmetric/transitive flags, cardinality constraints

### 2.4 Evidence Substrate

Every statement is (optionally) traceable to its source:

```
Statement ← EvidenceLink ← Span ← Revision ← Document
                                ← Annotation ← AnnotationSpace
                                              ← ExtractionRun
```

- **Documents**: registered source texts with immutable revisions
- **Spans**: character offsets, tokens, sentences, paragraphs, pages, XPath, CSS selectors
- **Annotations**: machine-generated observations on spans (NER, POS, sentiment, etc.)
- **Extraction runs**: PROV-O compatible provenance (model, prompt, temperature, status, counts)
- **Evidence links**: typed connections (produced_by, extracted_from, supports, refutes, contextualizes)

### 2.5 Arguments and Proof Obligations

**Arguments** connect statements to each other:
- supports, rebuts, undercuts, endorses, supersedes, qualifies
- potentially_same, same_referent, same_event
- Each has strength [0,1], context, agent attribution

**Proof obligations** are open tasks:
- needs-coref, needs-temporal-grounding, needs-source-support
- needs-unit-normalization, needs-entity-disambiguation, needs-relation-validation
- needs-human-review, needs-confidence-boost, needs-context-resolution
- Each has status (open → in_progress → resolved/rejected/deferred), priority, assigned agent

### 2.6 Shape Validation and Epistemic Sweep

**Shapes** declare constraints on predicates (functional, datatype). The **epistemic sweep** is a batch process that:
1. Validates shapes (sampled, not full-scan — the table has 35.8M rows)
2. Fires derivation rules (inverse, symmetric closures)
3. Detects contradictions on functional predicates
4. Emits proof obligations for unsupported claims
5. Promotes maturity (L0→L1→L2→L3) based on evidence presence and shape compliance

### 2.7 Entity Aliases and Coreference

- **Entity aliases**: `(alias_iri, canonical_iri, system, confidence)` — one-hop resolution
- **Coreference clusters**: `(cluster_id, cluster_type, members[])` — transitive equivalence groups
- Used during shadow materialization to resolve subject/object IRIs to canonical forms

### 2.8 Event Frames

N-ary relations decompose into event frames:
```
(marie-curie, worksAt, sorbonne) + {startDate: 1906, role: "professor"}
→ frame:uuid rdf:type ex:EmploymentEvent
  frame:uuid ex:subject marie-curie
  frame:uuid ex:object sorbonne
  frame:uuid ex:startDate 1906
  frame:uuid ex:role "professor"
```

Templates define how predicates decompose into role predicates.

### 2.9 LLM Extraction Pipeline

Production extraction uses Grok 4.1 Fast via OpenRouter ($0.005/article) with a v2 prompt that extracts across 8 analytical tiers:

| Tier | Category | % of output |
|------|----------|------------|
| T1 | Surface facts | 30-40% |
| T2 | Relational/structural | 15-20% |
| T3 | Opinions/stances | 5-10% |
| T4 | Epistemic/modal | 5-8% |
| T5 | Rhetorical | 5-8% |
| T6 | Presuppositions | 10-15% |
| T7 | Philosophical | 3-5% |
| T8 | Intertextual | 3-5% |

Benchmarked across 30+ models. Grok at $0.005/article with quality 8.4-8.8/10 wins on cost-adjusted quality. Sonnet 4.6 at $0.35/article with quality 9.4/10 for premium use.

---

## 3. What's Missing

### 3.1 Subject/Entity Layer (critical gap)

Subjects are bare text IRIs with no structure. We have:
- Entity aliases (one-hop canonical resolution)
- Coreference clusters (transitive equivalence)

We don't have:
- **Entity type hierarchy** — no way to say "ex:mary-watson is a Person, which is a subclass of Agent"
- **Entity properties vs relationships** — no distinction between intrinsic properties (birthDate) and relational properties (marriedTo)
- **Cross-document entity resolution** — different extractions create `ex:mary-watson`, `ex:mrs-watson`, `ex:watson-mary` for the same person. The alias system handles known mappings, but there's no automatic resolution.
- **Entity lifecycle** — no creation/destruction time modeling beyond valid_time on individual statements
- **Entity provenance** — which extraction run first introduced this entity?

### 3.2 Object Layer (partial gap)

Object IRIs are bare text like subjects. Object literals have types but:
- **No unit normalization** — "5 feet 10 inches" vs "178 cm" vs "1.78m" are unrelated
- **No value canonicalization** — dates appear as "1860", "1860-01-01", "circa 1860", "abt. 1860"
- **No range enforcement** — predicates declare `range_iri` and `range_datatype` but these aren't enforced during assertion
- **No object registry** — common objects (places, institutions, concepts) get minted fresh by each extraction with no dedup

### 3.3 RDFS/OWL Ontology Layer

No machine-readable class hierarchy:
- No `rdfs:domain` / `rdfs:range` declarations (only advisory type hints in predicate descriptors)
- No class subsumption (`rdfs:subClassOf`)
- No disjointness constraints (`owl:disjointWith`)
- No property chains (`owl:propertyChainAxiom`)
- No inverse/transitive rule automation beyond hardcoded epistemic sweep examples

### 3.4 Inference Engine

The epistemic sweep is a batch SQL script, not a reasoning engine:
- No forward-chaining rule application
- No backward-chaining query expansion
- No confidence-weighted inference
- No temporal reasoning (Allen's interval algebra on valid_time)
- No spatial reasoning
- No default reasoning / non-monotonic logic

### 3.5 Query Capabilities

- DontoQL is a simple pattern language (MATCH, FILTER, LIMIT)
- SPARQL subset is very limited (SELECT only, no CONSTRUCT, no OPTIONAL, no UNION, no property paths)
- No recursive graph traversal
- No aggregation in queries
- No EXPLAIN/query planning

### 3.6 Cross-System Integration

- No linked data protocol (HTTP content negotiation, turtle/jsonld serialization)
- No SPARQL endpoint (standard compliance)
- No federation / distributed querying
- No import from Wikidata, DBpedia, or other knowledge bases

---

## 4. The Novel Research Challenges

### 4.1 Paraconsistent Multi-Source Knowledge Fusion

Traditional knowledge graphs reject contradictions. Donto embraces them. The challenge:

**Given N extractions from M sources about the same entities, where sources contradict each other on dates, relationships, names, and even the existence of events:**
- How do we model the *degree* of contradiction (not just binary conflict)?
- How do we propagate confidence through inference chains when premises disagree?
- How do we present a "best current understanding" while preserving minority positions?
- How do we handle contradictions that are temporal (true at different times) vs factual (one is wrong)?

**What makes this novel**: Most paraconsistent logics are theoretical. We have 35.8M statements from real-world genealogical research where contradictions are the norm, not the exception. Colonial-era records are full of misspellings, date errors, racial misclassification, and deliberate falsification. The system must work with this data, not reject it.

### 4.2 Open-World Entity Resolution at Scale

LLMs mint entity IRIs freely during extraction. The same person appears as:
- `ex:mary-watson` (from one article)
- `ex:mrs-watson` (from another)
- `ex:mary-watson-nee-oxley` (from a marriage record)
- `ex:watson-mary` (from a census)
- `ctx:genealogy/research-db/iri/31448699f0e5` (from legacy data)

**The challenge**: How do we resolve these without a closed-world entity registry? We can't pre-define all entities because LLMs create new ones with every extraction. We need:
- Automatic candidate generation (trigram similarity on names? embedding similarity on descriptions?)
- Confidence-weighted merging (not just binary same/different)
- Provenance-aware merging (which source is more authoritative?)
- Reversible merging (undo a merge if it was wrong)
- Incremental resolution (don't re-process everything when a new entity appears)

### 4.3 Predicate-Subject-Object Alignment Triangle

We've solved predicate alignment. But the same challenge exists for subjects and objects:

```
Predicate alignment:  bornIn ↔ birthplaceOf ↔ bornInPlace
Subject alignment:    ex:mary-watson ↔ ex:mrs-watson ↔ ex:watson-mary
Object alignment:     ex:cornwall ↔ ex:cornwall-england ↔ ex:cornwall-uk
```

These three alignment problems interact:
- If we know `bornIn` and `birthplaceOf` are inverses, and `ex:mary-watson` is the same as `ex:mrs-watson`, then `(ex:mary-watson, bornIn, ex:cornwall)` should match `(ex:cornwall, birthplaceOf, ex:mrs-watson)` — that's **three** alignment hops in one query.

**The challenge**: How do we compose predicate, subject, and object alignment efficiently? The closure table approach works for predicates, but a three-dimensional closure is O(P × S × O) which is infeasible at 35M statements.

### 4.4 Temporal Knowledge Representation

Genealogical facts are inherently temporal:
- "Mary was married to Robert from 1879 to 1881"
- "The municipality was proclaimed in 1879 and dissolved in 1888"
- "He arrived during the gold rush" (imprecise temporal reference)

We store valid_time as a daterange but:
- How do we reason about temporal overlap? ("Was Mary alive when the municipality was dissolved?")
- How do we handle imprecise dates? ("circa 1860", "before 1880", "early 1900s")
- How do we model temporal relationships between events? (Allen's 13 interval relations)
- How do we query temporal patterns? ("Find all people who lived in Cooktown between 1870 and 1890")

### 4.5 Maturity Promotion and Epistemic Status

The current maturity ladder (L0→L4) is simplistic:
- L0: raw (just extracted)
- L1: predicate is registered
- L2: has evidence link
- L3: passes shape validation
- L4: certified (not implemented)

**The challenge**: Can we build a more nuanced epistemic model?
- Confidence should propagate through inference chains (if A supports B and B supports C, what's the confidence in C?)
- Contradictions should affect maturity (a contradicted fact should lose maturity, not keep L3)
- Source authority should matter (a government record is more authoritative than a newspaper)
- Temporal decay? (a claim from 1950 about events in 1860 is less reliable than a contemporaneous record)

### 4.6 The Extraction Feedback Loop

Currently, extraction is one-way: text → LLM → facts → donto. But the graph should inform future extractions:
- **Predicate suggestion**: When extracting from a new source about a known topic, suggest existing predicates to the LLM (partially implemented via `donto_extraction_predicate_candidates`)
- **Entity priming**: Tell the LLM about known entities so it reuses IRIs instead of minting new ones
- **Gap-directed extraction**: Identify what's missing (proof obligations) and find sources that might fill those gaps
- **Contradiction-aware extraction**: When extracting from a source that contradicts existing facts, should the LLM note the contradiction explicitly?

### 4.7 Scale and Performance

At 35.8M statements:
- Full-table scans are forbidden (they take 25+ seconds)
- The trigram index on literals gives sub-second search on labels
- But pattern-match queries with weak filters can still be slow
- The predicate closure has 11,000+ rows and growing
- Shape validation uses TABLESAMPLE to avoid full scans

**The challenge**: How do we maintain sub-second query times as we scale to 100M, 500M, 1B statements? Materialized views? Partitioning? Hybrid in-memory indexes?

---

## 5. Current Architecture

```
                 Agents / Web / CLI
                        │
                        ▼
              ┌─────────────────────┐
              │  Donto API (Python) │  ← FastAPI, native OpenRouter
              │  genes.apexpots.com │     extraction, proxies to dontosrv
              └──────────┬──────────┘
                         │
              ┌──────────▼──────────┐
              │  dontosrv (Rust)    │  ← Axum, 50+ HTTP endpoints
              │  localhost:7879     │     direct Postgres pool
              └──────────┬──────────┘
                         │
              ┌──────────▼──────────┐
              │  PostgreSQL 16      │  ← 35.8M statements, 27GB
              │  56 migrations      │     bitemporal, GiST+GIN indexes
              │  localhost:5432     │     pg_trgm, custom functions
              └─────────────────────┘
```

**Tech stack**: Rust (dontosrv, donto-cli, donto-client, donto-ingest, donto-query), Python (API), PostgreSQL (storage), OpenRouter/Grok (extraction)

**No external dependencies**: No Jena, no Neo4j, no Stardog, no pre-built RDF store. Everything is written from scratch on top of PostgreSQL.

---

## 6. What We Want From You

We are seeking an extremely detailed technical report addressing:

1. **Entity resolution architecture**: How should we build the subject/object alignment layer? What algorithms work for open-world entity resolution with uncertain, contradictory data? How do we compose entity alignment with predicate alignment efficiently?

2. **Ontology layer design**: Should we adopt RDFS/OWL subsets or design something novel? How do we add class hierarchies and property constraints without losing the open-world flexibility that makes LLM extraction work?

3. **Inference engine design**: What reasoning capabilities should we add first? Forward-chaining? Backward-chaining? Temporal reasoning? How do we handle confidence propagation through paraconsistent inference?

4. **Temporal reasoning**: How should we model imprecise dates, temporal relationships between events, and temporal queries? What's the right representation for "circa 1860" or "during the gold rush"?

5. **Scale strategy**: Given 35.8M statements today and a target of 1B, what architectural changes are needed? Partitioning? Materialized views? Hybrid in-memory indexes? Query planning?

6. **Extraction feedback loop**: How should the existing graph inform future LLM extractions? What's the right interface between the knowledge graph and the extraction prompt?

7. **Evaluation methodology**: How do we measure the quality of entity resolution, predicate alignment, and inference in a domain (genealogy) where ground truth is uncertain?

8. **Prior art**: What existing systems or papers address similar challenges? We're aware of Wikidata, DBpedia, YAGO, but our open-world paraconsistent model is significantly different from their closed-world curation model. What academic work on paraconsistent knowledge graphs, open-world entity resolution, or LLM-driven knowledge graph construction is relevant?

Please be as specific and technical as possible. We are experienced systems engineers building everything from scratch — we don't need introductions to RDF or knowledge graphs. We need novel solutions to novel problems.

---

## 7. Appendix: Database Schema Summary

### Core Tables
| Table | Rows | Purpose |
|-------|------|---------|
| `donto_statement` | 35.8M | Core quads with bitemporal + polarity + maturity |
| `donto_context` | ~5K | Context forest with kind, mode, parent |
| `donto_predicate` | ~12K | Predicate registry with metadata |
| `donto_predicate_alignment` | ~6K | Alignment edges with bitemporal provenance |
| `donto_predicate_closure` | ~12K | Materialized expansion index |
| `donto_predicate_descriptor` | ~500 | Rich metadata + embeddings |
| `donto_entity_alias` | ~2K | Entity canonical resolution |
| `donto_coref_cluster` | ~200 | Coreference groups |
| `donto_document` | ~500 | Source documents |
| `donto_span` | ~50K | Character-offset spans in documents |
| `donto_evidence_link` | ~100K | Statement ↔ source links |
| `donto_argument` | ~5K | Inter-statement arguments |
| `donto_proof_obligation` | ~10K | Open epistemic tasks |

### Key Functions
| Function | Purpose |
|----------|---------|
| `donto_assert()` | Insert a statement (bitemporal, idempotent) |
| `donto_assert_batch()` | Batch insert (JSONB array) |
| `donto_retract()` | Close tx_time on a statement |
| `donto_match()` | Pattern-match with scope + polarity + maturity |
| `donto_match_aligned()` | Match with predicate expansion through closure |
| `donto_rebuild_predicate_closure()` | Rebuild materialized closure |
| `donto_suggest_alignments()` | Trigram-based predicate suggestions |
| `donto_auto_align_batch()` | Lexical batch alignment |
| `donto_materialize_shadow()` | Canonicalize a statement |
| `donto_decompose_to_frame()` | N-ary event decomposition |
| `donto_resolve_scope()` | Scope descriptor → context list |
| `donto_resolve_entity()` | Entity alias resolution |
| `donto_claim_lifecycle()` | Compute claim maturity stage |

---

*This document describes donto as of May 2026. The system is in active development. All code is open source at https://github.com/thomasdavis/donto.*

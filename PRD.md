# PRD: donto

**donto** is a general-purpose ontological graph database, implemented as a PostgreSQL extension with Lean 4 as a formal verification and derivation layer. The name stands for "database ontology." This document is the source-of-truth design reference.

Working names:
- Project: **donto** (also: the donto database)
- Extension: `pg_donto`
- SQL schema prefix: `donto_`
- System IRI prefix: `donto:`
- Native query language: **DontoQL**
- Intermediate representation: **DIR** (Donto Intermediate Representation)
- Lean sidecar: **dontosrv**

Status: design document. Implementation lives in this repository. This PRD supersedes the earlier `PRD-ontograph.md`.

Author: Thomas Davis. Date: 2026-04-17.

---

## 0. One-paragraph summary

donto is a bitemporal, paraconsistent quad store in PostgreSQL with named graphs (contexts) as the universal overlay for provenance, snapshots, hypotheses, and trust. Statements carry polarity, confidence, and modality metadata. A predicate registry maps aliases to canonicals under an open-world taxonomy. A semantic maturity ladder (raw → registry-curated → shape-checked → rule-derived → certified) describes how data climbs from permissive ingestion to formally verified meaning. Lean 4 runs as a sidecar, providing SHACL-style shape validation, derivation rules, and machine-checkable certificates over context-scoped views of the graph. The narrow boundary between Postgres and Lean is a versioned IR (DIR). donto ingests RDF, JSON-LD, property-graph dumps, streaming LLM output, and CSV, with idempotent re-ingestion and a quarantine path for shape-violating content. Two modes, permissive and curated, are selectable per context, so a single database can host raw inputs, canonical knowledge, counterfactual hypotheses, and derived facts without mode-switching the whole store. The design target is a system that holds contradictions gracefully, scales to billions of statements on a single Postgres node, and is adoptable by applications that already speak Postgres.

---

## 1. Motivation

Graph databases split into two families:

1. **Property graphs** (Neo4j, Apache AGE, Memgraph). Strong ergonomics, weak formal semantics. Provenance, contradictions, bitemporality, and uncertainty become application concerns.
2. **Triple and quad stores** (Blazegraph, Jena, Virtuoso, Stardog). Strong semantics (RDF, OWL, SHACL) but operationally awkward, often JVM-centric, and poorly integrated with relational workloads.

Both miss the same thing: formal semantics as a first-class layer authored in a real theorem prover, composable cheaply over the operational store. Both also treat paraconsistency as a failure mode rather than a primitive, and none treats context as a unifying overlay for provenance, versioning, counterfactuals, and trust.

donto closes the gap. Target use cases:

- Knowledge graphs over heterogeneous sources.
- Bitemporal audit stores (regulatory, clinical, legal).
- Historical and genealogical research under uncertainty.
- Evolving scientific ontologies.
- Claim graphs from LLM extraction.
- Organizational knowledge graphs (roles, capabilities, responsibilities).
- Policy and compliance graphs with scoped enforcement.

The genealogy research project at this repository (230k entities, 530k claims, 541k open contradictions, 728k aliases, 156k events, active LLM extraction, Lean evidence engine) is both the anchoring case and the hardest stress test. donto must absorb that project's data model without feature loss while remaining domain-agnostic.

---

## 2. The semantic maturity ladder

The central product framing. Every statement in donto sits at one of five maturity levels. Higher levels unlock more donto features; lower levels are fully usable. Data promotes up the ladder as research and curation proceed. Nothing in the architecture requires uniform maturity.

**Level 0. Raw ingested.** A statement exists in the store. Any predicate, any subject, any object, any context. No validation beyond structural well-formedness. This is what LLM extractors, bulk RDF dumps, and streaming pipelines produce.

**Level 1. Registry-curated.** The statement's predicate is registered in the predicate registry with a canonical IRI, optional label, domain and range hints, and alias links. Queries resolve alias variants to the canonical form automatically.

**Level 2. Shape-checked.** The statement has been evaluated against one or more shapes. A shape report is attached. Shapes can pass, warn, or violate; violations do not remove the statement, they annotate it.

**Level 3. Rule-derived.** The statement was produced by a recorded derivation rule from input statements in a declared context scope. Its `source_stmt` pointers trace lineage back to inputs. Re-running the rule over the same inputs reproduces the statement.

**Level 4. Certified.** The statement (or its derivation) carries a machine-checkable certificate, verifiable independently of the rule that produced it. The certificate can be signed. Certificates are the strongest form of provenance.

This ladder is simultaneously a user mental model, a query filter (`maturity >= 2`), and a curation workflow. Levels are recorded per statement per context, not globally. A statement can be Level 4 in one snapshot context and Level 0 in a raw ingestion context.

---

## 3. Design principles

Non-negotiable. Features that conflict will be rejected.

1. **Paraconsistent by default.** The store holds mutually inconsistent statements. Consistency is a query, not a constraint.
2. **Every statement has a context.** Default context `donto:anonymous` exists, but the slot is never empty.
3. **Bitemporal from the atom.** Every statement has valid-time and transaction-time intervals. Retraction closes transaction-time; it never deletes.
4. **Open-world predicates.** The predicate space grows at runtime. Aliases and canonicals are first-class.
5. **Shapes are overlays, not schemas.** Shape failure is a report, not an ingestion error.
6. **Lean certifies; it does not gate.** Shapes and rules live in Lean. Ingestion does not wait on Lean. Lean's output is attached as annotations and certificates.
7. **Postgres owns execution. Lean owns meaning.** The extension is the runtime substrate. Lean is a sidecar. DIR is the narrow boundary.
8. **The atom is the statement.** Everything else (contexts, shapes, rules, certificates, snapshots, clusters, hypotheses, campaigns) is either a statement or a collection of statements.
9. **Provenance is mandatory, certificates are optional.** Every derived statement records its inputs; a certificate on top is a stronger claim.
10. **No hidden ordering.** Statement order is never meaningful except in aggregations with explicit `ORDER BY`.

---

(Sections 4-34 of the PRD are the design source of truth as drafted on 2026-04-17. They are reproduced in `docs/PRD-full.md` for archival; this file holds the principles, ladder, and atom that the implementation must conform to. When in doubt, consult the full PRD.)

See also:
- [`docs/PHASE-0.md`](docs/PHASE-0.md) — current phase plan and exit criteria.
- [`sql/migrations/`](sql/migrations/) — schema source of truth.
- [`crates/donto-client/`](crates/donto-client/) — Rust client implementing assert/retract/match against the Phase 0 schema.

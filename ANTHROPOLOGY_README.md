# donto — an anthropology of the project

## What donto is

donto is a database for claims that may be wrong.

Most databases assume you are storing facts — clean, consistent,
authoritative. donto assumes the opposite. It stores what was said, by
whom, based on what evidence, when it was believed, what contradicts it,
and what remains unresolved. It is a system for reasoning under
uncertainty, disagreement, and incomplete information.

The name is a portmanteau: *don't* + *onto* (ontology). A gentle
reminder that knowledge is provisional.

---

## The problem donto addresses

Every research domain — genealogy, medicine, law, intelligence analysis,
scientific literature — shares a structural problem: the sources
disagree.

A census record says Alice was born in 1899. A hospital admission says
1925. A family bible says 1898 with a question mark in the margin.
Traditional databases force you to pick one. The alternatives — a
spreadsheet with color-coded conflicts, a folder of PDFs with sticky
notes, a wiki page that someone overwrites — all lose information at the
exact moment it becomes interesting.

donto keeps all three claims. It records where each came from, when the
system learned about each, what confidence the extractor assigned, and
whether any human or machine has since endorsed, rebutted, or qualified
any of them. The contradiction is not a bug to fix. It is evidence to
reason about.

---

## Core concepts

### The statement as atom

Everything in donto is a statement: a subject-predicate-object triple
annotated with context (who said it), time (when it was true, when we
learned it), polarity (asserted, negated, absent, unknown), and maturity
(how much epistemic work has been done on it).

Statements are never deleted. Retraction closes a time window but
preserves the physical record. Correction retracts the old and asserts
a new, linking them. The full history of belief is always recoverable.

### Paraconsistency

Two sources can assert contradictory values for the same predicate on
the same subject. Both coexist. The system never silently picks a
winner, never raises an error on contradiction, never overwrites.

This is the foundational design decision. In the domains donto serves,
contradictions are the most informative data points. The moment two
records disagree about a birth year is the moment the real research
begins.

### Bitemporality

Every statement carries two time dimensions:

- **Valid time** — when was this true in the world?
- **Transaction time** — when did the system learn about it?

This means you can ask: "What did we believe about Alice last Tuesday?"
or "What changed in our understanding between March and April?" The
database is a complete audit trail of belief over time.

### The maturity ladder

Claims are not born equal. A raw LLM extraction from a web page is not
the same as a hand-verified record anchored to a primary source with a
machine-checkable certificate. donto makes this distinction explicit
with five maturity levels:

| Level | Name | What it means |
|-------|------|---------------|
| 0 | raw | Ingested, nobody has looked at it |
| 1 | parsed | The predicate is recognized, structure is valid |
| 2 | linked | Anchored to source material with evidence chains |
| 3 | reviewed | Shape-validated, argued, no unresolved violations |
| 4 | certified | A machine-checkable proof has been attached |

The system can tell you exactly why a given claim hasn't reached the
next level: missing evidence link, unresolved shape violation, open proof
obligation, active rebuttal.

### Contexts as belief spaces

Every statement lives in a context — a named subgraph that represents a
source, a hypothesis, a snapshot, a derivation, or a quarantine zone.
Contexts form a tree with inheritance. Queries are scoped: you can ask
"what does the census say?" or "what does the hospital say?" or "what
does everything say?" or "assuming this hypothesis is true, what
follows?"

Hypothesis contexts let you explore counterfactuals without contaminating
the main knowledge base. Snapshot contexts freeze a moment in time for
reproducible analysis. Quarantine contexts isolate suspicious data.

---

## The evidence substrate

donto doesn't just store claims. It stores the full provenance chain:

A **document** (a web page, a PDF, a scanned record) produces
**revisions** (text extractions at different parser versions). Revisions
contain **spans** (character-level anchors to specific passages). Spans
anchor **mentions** (references to entities, which may be ambiguous).
Mentions cluster into **coreference groups** (resolving "the deceased",
"Mrs. Smith", and "Alice" to the same person).

An **extraction run** records which model, prompt, temperature, and
chunking strategy produced a set of claims. Each **chunk** is tracked
individually with its prompt hash, response hash, and latency.

**Evidence links** connect statements to their source material:
extracted-from, supported-by, contradicted-by, derived-from. **Confidence
scores** record how certain the extractor was. **Shape annotations**
record whether the claim passes structural validity checks.

The **argumentation framework** lets agents and humans record
relationships between claims: supports, rebuts, undercuts, endorses,
supersedes, qualifies. The system computes a "contradiction frontier" —
the set of claims under the most argumentative pressure.

**Proof obligations** track what epistemic work remains: needs entity
disambiguation, needs source support, needs temporal grounding, needs
human review. They can be assigned to agents, prioritized, and resolved.

---

## The verification layer

donto includes a Lean 4 verification engine — a proof assistant that
provides mathematical guarantees about the data model. 62 theorems prove
structural invariants:

- Contradictions are always preserved (paraconsistency is not accidental)
- Retraction never destroys identity
- Snapshots are monotone (once a claim is in a snapshot, it stays)
- Scope exclusion always wins over inclusion
- Identical inputs produce identical outputs (idempotency)

These are not test cases. They are proofs that hold for every possible
input, verified by the Lean kernel at compile time. If a code change
violates an invariant, the build fails before it can be deployed.

The engine also runs user-authored validators — for example, a shape
that flags parent-child pairs where the parent is implausibly young, or
a rule that derives grandparent relationships from parent chains.

---

## Current applications

### Genealogical research

The primary production workload. 35+ million statements tracking family
relationships, vital records, DNA matches, immigration records, and oral
histories for families in North Queensland, Australia. Multiple LLM
extraction agents ingest web sources, census records, and archival
documents. The epistemic sweep detects contradictions between sources
(two records disagree about a birth year), derives missing relationships
(if A is parent of B, B is child of A), and creates proof obligations
for ungrounded claims.

This is the application that motivated donto's design. Genealogical
research is fundamentally about reasoning under contradiction — every
primary source may be wrong, transcribed incorrectly, or referring to a
different person with the same name.

### Scientific paper extraction

Structured claims from ML/AI papers. Benchmark results, model
comparisons, and measurements are ingested from PDFs with per-chunk
extraction provenance. The unit registry handles cross-paper
normalization — "60.1%" and "0.601" are recognized as the same value.

### Salon business research

Melbourne salon industry data. Business entities, staff counts, ratings,
service offerings, and franchise relationships.

### Interactive game state

A Clue-style murder mystery where players hold contradictory beliefs
about who committed the crime — demonstrating that the paraconsistent
model naturally handles adversarial reasoning.

---

## Tooling

### Terminal dashboard (donto-tui)

A terminal interface for watching a live donto database. Six views:
system health dashboard, real-time firehose of all database activity,
statement explorer with search, context browser, claim card deep-dive
on individual statements, and charts showing growth, context
distribution, and predicate usage over time.

The firehose captures all database activity — not just the official API
calls, but also bulk importers and raw SQL — by combining Postgres
LISTEN/NOTIFY triggers with pg_stat_activity polling.

### HTTP sidecar (dontosrv)

A stateless HTTP API with 35 endpoints spanning query, write, evidence,
argumentation, shapes, rules, certificates, obligations, and system
health. Horizontally scalable. The database is fully functional without
it.

### CLI (donto-cli)

Command-line interface for migration, ingestion (8 formats), querying
(DontoQL, SPARQL subset), matching, and retraction. Designed for
pipeline scripting.

### Documentation site

Astro Starlight site covering user guide, operator guide, CLI reference,
migration reference, Lean overlay documentation, and schema design
rationale. Auto-deployed to GitHub Pages.

---

## Architecture in brief

Everything is Postgres. All 55 tables live in one database with standard
ACID transactions. No separate search index, no graph database, no
document store. One backup target, one transaction boundary, one query
planner. The cost is that some access patterns are slower than
purpose-built engines. The benefit is that every piece of evidence
participates in the same consistency guarantees.

The codebase is a polyglot monorepo: Rust (core engine, CLI, HTTP
sidecar), Go (terminal dashboard), TypeScript (client library,
documentation site), Lean 4 (verification engine), and SQL (schema
source of truth). Each language is used where it is strongest.

---

## Design philosophy

**Source-first.** Every factual claim requires a source. The system
enforces this by default and makes opting out explicit and auditable.

**Contradictions are evidence.** The database never resolves
contradictions for you. It gives you tools to see them, argue about
them, and track their resolution — but the resolution is always a
human or agent decision, never an automatic one.

**Degradation over failure.** The Lean engine is optional. The HTTP
sidecar is optional. The TUI is optional. Each layer degrades gracefully
when the layers above it are absent. The database alone is always
sufficient.

**Idempotency everywhere.** Every ingestion path deduplicates via
content hash. Every migration is idempotent. Every epistemic sweep can
be re-run safely. The system is designed for pipelines that crash and
restart.

**Maturity is earned, not assigned.** A claim cannot be promoted to the
next level without meeting specific structural criteria. The system
tells you what's missing. This is deliberate friction — it forces
epistemic discipline on both human and machine contributors.

---

## License

Dual licensed under Apache 2.0 and MIT.

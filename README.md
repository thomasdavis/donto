# donto

A bitemporal, paraconsistent quad store. Postgres extension. Optional Lean 4
sidecar for shape validation, derivations, and machine-checkable certificates.

```text
                  fact = (subject, predicate, object, context,
                          valid_time, transaction_time, polarity, maturity)
```

donto is for systems where the truth is messy:

- Two sources disagree, and **both belong on the record**.
- Facts have a **history** — what we know now, what we knew last quarter,
  what was true in the world during 1899.
- Some statements are speculation, some are observation, some are
  derivations of derivations. donto treats those differently without
  needing five tables to do it.
- The schema, when it shows up, is **a report**, not a barrier to writing.

If your domain is genealogy, biomedical claims, regulatory audit,
intelligence analysis, knowledge graphs from LLM extractors, or any
research database where contradictions are evidence, donto is for you.

---

## A walked-through example

You're piecing together a family tree from old records.

A 1900 census names someone *Alice Brackenridge*, born around 1899. A
1925 hospital ledger names *Alice Julian*, born 1925. Could be the same
person — name change after marriage, common in that era — but you're not
sure yet. A colleague is convinced they're different people.

Most databases force you to pick. donto doesn't.

### 1. Each source becomes a context

```sql
SELECT donto_ensure_context('ctx:src/census1900', 'source', 'permissive', NULL);
SELECT donto_ensure_context('ctx:src/hospital1925', 'source', 'permissive', NULL);
```

A *context* is the universal overlay in donto: provenance, snapshots,
hypotheses, multi-tenancy — they're all contexts. Every fact lives in one.

### 2. Both birth years are recorded — paraconsistently

```sql
-- Census 1900 says Alice was born 1899.
SELECT donto_assert(
    'ex:alice_young', 'ex:birthYear',
     NULL, '{"v":1899,"dt":"xsd:integer"}'::jsonb,
    'ctx:src/census1900', 'asserted', 0, NULL, NULL, NULL);

-- Hospital 1925 says Alice was born 1925.
SELECT donto_assert(
    'ex:alice_old', 'ex:birthYear',
     NULL, '{"v":1925,"dt":"xsd:integer"}'::jsonb,
    'ctx:src/hospital1925', 'asserted', 0, NULL, NULL, NULL);
```

A query with both sources in scope returns both rows. donto never picks
a winner — that's an application decision, made downstream:

```bash
donto query 'SCOPE include <ctx:src/census1900>, <ctx:src/hospital1925>
             MATCH ?p ex:birthYear ?y
             PROJECT ?p, ?y'
# {"p":"ex:alice_young","y":{"v":1899,...}}
# {"p":"ex:alice_old", "y":{"v":1925,...}}
```

### 3. Explore "are they the same person?" in a hypothesis branch

```sql
-- Hypothesis contexts are themselves contexts. Anything you assert here
-- is invisible to the curated view but available under-hypothesis.
SELECT donto_ensure_context(
    'ctx:hypo/alice_merge', 'hypothesis', 'permissive', NULL);

SELECT donto_assert(
    'ex:alice_young', 'donto:sameAs', 'ex:alice_old', NULL,
    'ctx:hypo/alice_merge', 'asserted', 0, NULL, NULL, NULL);
```

```bash
# Curated view: nothing has changed. The hypothesis doesn't leak.
donto query 'PRESET curated MATCH ?x donto:sameAs ?y'
# (no rows)

# Under the hypothesis: Alice and Alice are the same person.
donto query 'SCOPE include <ctx:hypo/alice_merge>
             MATCH ?x donto:sameAs ?y'
# {"x":"ex:alice_young","y":"ex:alice_old"}
```

This is *non-monotonic identity*: drop the hypothesis from your scope and
the merge silently disappears. Different research branches can hold
incompatible identity views simultaneously. Most graph databases treat
identity as a global rewrite rule and force you to pick.

### 4. Bitemporal: change your mind without losing the trail

You initially recorded Alice's spouse as Bob, based on the census:

```sql
SELECT donto_assert('ex:alice_young', 'ex:spouse', 'ex:bob', NULL,
                    'ctx:src/census1900', 'asserted', 0, NULL, NULL, NULL);
```

Six months later, a parish record reveals you misread the census. Don't
overwrite — *correct*:

```sql
SELECT donto_correct(
    (SELECT statement_id
     FROM donto_match('ex:alice_young', 'ex:spouse', NULL, NULL, NULL,
                      'asserted', 0, NULL, NULL) LIMIT 1),
    NULL, NULL, 'ex:not_bob', NULL, NULL, NULL);
```

Today's view shows the corrected spouse:

```bash
donto query 'MATCH ex:alice_young ex:spouse ?s'
# {"s":"ex:not_bob"}
```

But the original belief is **never deleted**. Time-travel queries see
what you knew at the time:

```bash
donto match --subject ex:alice_young --predicate ex:spouse \
            --as-of-tx 2026-04-01T00:00:00Z
# {"s":"ex:bob"}
```

That's `tx_time` (when the system believed it). donto also tracks
`valid_time` (when the fact was true in the world), so you can ask
"what did we believe in March about who Alice was married to in 1923?"
without confusion. A fact about Alice's youth recorded today and a
fact about Alice's youth recorded in 1925 are both queryable; they
just sit in different `tx_time` intervals.

### 5. Spot impossible-by-definition data with shapes

Marriage is functional — at most one spouse at a time. donto ships a
built-in shape that flags violations as a *report*, not a write error:

```bash
curl -X POST localhost:7878/shapes/validate -H 'content-type: application/json' \
  -d '{"shape_iri":"builtin:functional/ex:spouse",
       "scope":{"include":["ctx:src/census1900","ctx:src/hospital1925"]}}'
# {"shape_iri":"builtin:functional/ex:spouse",
#  "violations":[{"focus":"ex:alice_young",
#                 "reason":"predicate ex:spouse is functional but has 2 objects",
#                 "evidence":["<uuid>","<uuid>"]}],
#  "source":"builtin"}
```

The violations stay visible. The data isn't deleted or rejected. Shapes
are **annotations**, not constraints — donto's job is to record what
the world said, not to police it.

### 6. Derive new facts that carry lineage and a certificate

You have parent edges. You want ancestors. Run the bundled transitive
closure rule:

```bash
curl -X POST localhost:7878/rules/derive -H 'content-type: application/json' \
  -d '{"rule_iri":"builtin:transitive/ex:parent",
       "scope":{"include":["ctx:src/census1900"]},
       "into":"ctx:derived/ancestors"}'
# {"emitted":42, "into":"ctx:derived/ancestors", "source":"builtin"}
```

Each emitted statement:
- Lives in the `ctx:derived/ancestors` derivation context.
- Carries `maturity = 3` (rule-derived; level 3 of the maturity ladder).
- Has lineage pointers back to every input it consumed.
- Re-running the rule with the same inputs is a cache hit — same
  fingerprint, same answer, no re-execution.

Optionally attach a certificate that an independent verifier can replay
without trusting the rule's code:

```bash
curl -X POST localhost:7878/certificates/attach -H 'content-type: application/json' \
  -d '{"statement_id":"<uuid>","kind":"transitive_closure",
       "body":{"predicate":"ex:parent","scope":{"include":["ctx:src/census1900"]}}}'

curl -X POST localhost:7878/certificates/verify/<uuid>
# {"ok":true}
```

That's seven of donto's distinctive features in one short story:
**contexts**, **paraconsistency**, **hypothesis scoping**,
**non-monotonic identity**, **bitemporal correction**, **shape
validation as overlay**, **derivation with lineage and certificates**.
The full set is in [PRD.md](PRD.md).

---

## Install

donto needs Postgres 16 and Rust stable. A `just` command runner is
optional but recommended.

```bash
git clone https://github.com/thomasdavis/donto
cd donto

# Bring up Postgres 16 in a container and apply migrations.
./scripts/pg-up.sh
cargo run -p donto-cli --quiet -- migrate
```

Want the Postgres extension proper (`CREATE EXTENSION pg_donto;`)?

```bash
./scripts/pgrx-build.sh    # builds + tests pg_donto inside a container
```

The Lean overlay (optional — donto runs without it):

```bash
cd lean && lake build      # produces .lake/build/bin/donto_engine
```

## Three query surfaces

donto exposes the same algebra through three front ends. Pick whichever
your team already speaks.

**Plain SQL functions** — for applications that already have a Postgres
connection pool:

```rust
use donto_client::{ContextScope, DontoClient, Object, StatementInput};

let c = DontoClient::from_dsn(env!("DONTO_DSN"))?;
c.assert(&StatementInput::new("ex:alice", "ex:knows", Object::iri("ex:bob"))
    .with_context("ctx:src/wikipedia")).await?;
let rows = c.match_pattern(Some("ex:alice"), None, None,
    Some(&ContextScope::just("ctx:src/wikipedia")),
    None, 0, None, None).await?;
```

**SPARQL 1.1 subset** — for RDF-native tooling:

```sparql
PREFIX ex: <http://example.org/>
SELECT ?x ?y WHERE { ?x ex:knows ?y . } LIMIT 10
```

**DontoQL** — donto's native language, the only one that exposes the
full surface (scope presets, polarity, maturity, identity expansion):

```dontoql
PRESET latest
MATCH ?x ex:knows ?y, ?y ex:name ?n
FILTER ?n != "Mallory"
POLARITY asserted
MATURITY >= 1
PROJECT ?x, ?n
LIMIT 10
```

## Migrating from existing stores

```bash
donto ingest dump.nq                        # any RDF quad store
donto ingest export.json --format property-graph    # Neo4j / AGE
donto-migrate genealogy /path/to/research.db        # SQLite genealogy schema
```

donto also ingests Turtle, TriG, RDF/XML, JSON-LD, JSONL streams (for
LLM extractor pipelines), and CSV with a column-mapping file.

## Documentation

- [`PRD.md`](PRD.md) — full design specification. Read §3 (principles)
  and §2 (the maturity ladder) before contributing.
- [`docs/USER-GUIDE.md`](docs/USER-GUIDE.md) — how to ingest, query,
  scope, snapshot, and reason under hypothesis.
- [`docs/OPERATOR-GUIDE.md`](docs/OPERATOR-GUIDE.md) — sizing, backup,
  observability, sidecar topology, capacity tuning.
- [`docs/PHASE-0.md`](docs/PHASE-0.md) — phase plans.

## Project layout

```
PRD.md                  Source of truth for the design.
sql/migrations/         Schema and SQL functions. SQL is canonical.
crates/donto-client/    Typed Rust wrapper over the SQL surface.
crates/donto-cli/       `donto` command — ingest, match, query, retract.
crates/donto-query/     DontoQL parser, SPARQL subset, evaluator.
crates/donto-ingest/    All ingestion formats.
crates/donto-migrate/   Migrators from external stores.
crates/dontosrv/        HTTP sidecar (SPARQL, DontoQL, shapes, rules, certs).
crates/pg_donto/        pgrx Postgres extension.
lean/                   Lean overlay (Core, IR, Shapes, Rules, Certs).
docs/                   User and operator guides.
```

## Status and roadmap

donto is **early**. The full design in [PRD.md](PRD.md) ships in eleven
phases (PRD §26); first implementations of all eleven exist on `main`.
What that means in practice:

- **Solid:** the data model, ingestion (8 formats), DontoQL/SPARQL
  query, contexts and scopes, bitemporal queries, retraction and
  correction, snapshots, predicate registry with aliases.
- **Working but minimal:** shape and rule built-ins (the Lean overlay
  exists as type definitions and combinators; the Rust sidecar mirrors
  the standard library so the system runs without Lean).
- **Not yet:** distributed deployment, OWL-lite reasoning, a Cypher
  front end, federated queries, the full certificate signing PKI.
  These are explicit follow-ons in PRD §26.

**Performance is not yet a goal.** PRD §25 lays out hypotheses
(10⁹ statements, 100k inserts/sec, sub-ms point queries) to validate
later. Current focus is correctness and PRD coverage. No speculative
indexes; no premature partitioning.

Versioning will follow SemVer once the first tagged release lands. For
now, treat `main` as a moving target.

## Contributing

See [`CONTRIBUTING.md`](CONTRIBUTING.md) for the contributor guide and
[`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md). [`CLAUDE.md`](CLAUDE.md) is
the shared working contract for AI agents and humans — a concise list
of donto's non-negotiables and the SQL idioms that took us a few tries
to get right.

## Security

See [`SECURITY.md`](SECURITY.md). Please report vulnerabilities
privately via GitHub Security Advisories.

## License

Dual licensed under either of:

- Apache License 2.0 ([`LICENSE-APACHE`](LICENSE-APACHE))
- MIT License ([`LICENSE-MIT`](LICENSE-MIT))

at your option. Unless you explicitly state otherwise, any contribution
intentionally submitted for inclusion in this work shall be dual-licensed
as above, without any additional terms or conditions.

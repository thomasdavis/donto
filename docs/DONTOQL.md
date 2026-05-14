# DontoQL — donto's native query language

**Status:** v2 spec landed in `packages/donto-query/`. Some clauses parse
but evaluate to `Unsupported` until the kernel piece behind them
(Trust Kernel HTTP middleware, schema-lens registry, etc.) lands. See
[§ Clause coverage](#clause-coverage) for the live verdict per clause.

DontoQL is a small, line-oriented graph query language with strong
defaults for donto's invariants: contradictions are first-class,
bitemporal time-travel is one clause, every statement has a context,
and there is no implicit ordering. The grammar is hand-rolled in
[`dontoql.rs`](../packages/donto-query/src/dontoql.rs) and compiles to
the algebra in [`algebra.rs`](../packages/donto-query/src/algebra.rs).

The companion SPARQL 1.1 subset (PREFIX / SELECT / WHERE / GRAPH /
FILTER / LIMIT / OFFSET) lives in
[`sparql.rs`](../packages/donto-query/src/sparql.rs) and lowers to the
same algebra — anything written in DontoQL has a SPARQL equivalent for
the basic graph pattern, but the dimensions unique to donto (polarity,
maturity, modality, extraction level, identity lens, etc.) are
DontoQL-only.

If you're new: jump to [§ A complete worked example](#a-complete-worked-example).

---

## Grammar at a glance

```text
query        := keyword_clause+
keyword_clause :=
    'SCOPE'    scope_descriptor
  | 'PRESET'   IDENT
  | 'MATCH'    triple (',' triple)*
  | 'FILTER'   filter_expr (',' filter_expr)*
  | 'POLARITY' ident_in_set
  | 'MATURITY' '>='? INT
  | 'IDENTITY' ident
  | 'IDENTITY_LENS' ident                       # alias of IDENTITY
  | 'PREDICATES' ('EXPAND' | 'STRICT' | 'EXPAND_ABOVE' INT)
  | 'MODALITY'        ident (',' ident)*
  | 'EXTRACTION_LEVEL' ident (',' ident)*
  | 'TRANSACTION_TIME' 'AS_OF' STRING_or_PREFIXED
  | 'AS_OF' STRING_or_PREFIXED                  # shorthand
  | 'POLICY' 'ALLOWS' ident
  | 'SCHEMA_LENS' (iri | ident)
  | 'EXPANDS_FROM' 'concept' iri 'USING' 'schema_lens' iri
  | 'ORDER_BY' ident ('DESC'|'ASC')?
  | 'ORDER' 'BY' ident ('DESC'|'ASC')?          # two-word form
  | 'WITH'     'evidence' '=' ident
  | 'PROJECT'  var (',' var)*
  | 'LIMIT'    INT
  | 'OFFSET'   INT

triple   := term term term ('IN' term)?
term     := var | iri | string-lit | int-lit
var      := '?' IDENT
iri      := '<' chars '>'  |  PREFIXED          # ex:foo, ctx:genes/topic, test:dql2:abc
filter_expr := term op term                     # op ∈ { = != < <= > >= }
```

Whitespace is insignificant; clauses can appear in any order. `#` to
end-of-line is a comment. Keyword matching is case-insensitive.

---

## Default behaviour

Without any clauses, a `MATCH ?s ?p ?o` query returns:

| Dimension              | Default                                           |
|------------------------|---------------------------------------------------|
| Scope                  | anywhere (every context)                          |
| Polarity               | `asserted` (use `POLARITY` to broaden)            |
| Maturity floor         | 0 (E0 / raw)                                      |
| Identity lens          | `default` (no expansion across `sameAs` clusters) |
| Predicate expansion    | `EXPAND` (rides the alignment closure)            |
| Transaction-time pin   | none (current state — open `tx_time`)             |
| Order                  | none (PRD I-No-hidden-ordering)                   |
| Limit / Offset         | none                                              |

Everything else is opt-in.

---

## Clause coverage

| Clause                     | Parser | Evaluator | Notes                                                                         |
|----------------------------|:------:|:---------:|-------------------------------------------------------------------------------|
| `SCOPE`                    | ✅      | ✅         | Include/exclude lists; `ancestors` / `no_descendants` modifiers.              |
| `PRESET`                   | ✅      | ✅         | `latest`, `raw`, `curated`, `under_hypothesis`, `as_of:<ts>`, `anywhere`.     |
| `MATCH`                    | ✅      | ✅         | Multi-triple, comma-separated. `IN ?g` per-pattern graph binding accepted.   |
| `FILTER`                   | ✅      | ✅         | `= != < <= > >=`. Numeric ops compare literal numbers; non-numeric → false.   |
| `POLARITY`                 | ✅      | ✅         | `asserted` (default), `negated`, `absent`, `unknown`.                         |
| `MATURITY`                 | ✅      | ✅         | `MATURITY 2` ≡ `MATURITY >= 2`. Range: 0–5 (E0..E5).                          |
| `IDENTITY` / `IDENTITY_LENS` | ✅    | ✅         | `default`, `expand_clusters`, `expand_sameas_transitive`, `strict`.           |
| `PREDICATES`               | ✅      | ✅         | `EXPAND` (default), `STRICT`, `EXPAND_ABOVE <pct>`.                           |
| `MODALITY`                 | ✅      | ✅         | Filter via `donto_stmt_modality` overlay.                                     |
| `EXTRACTION_LEVEL`         | ✅      | ✅         | Filter via `donto_stmt_extraction_level` overlay.                             |
| `TRANSACTION_TIME AS_OF`   | ✅      | ✅         | Bitemporal time-travel; routed to `donto_match`'s `p_as_of_tx`.               |
| `AS_OF`                    | ✅      | ✅         | Shorthand for the above.                                                      |
| `POLICY ALLOWS`            | ✅      | ❌ deferred| Needs statement→source→policy join (PRD M0 HTTP middleware).                  |
| `SCHEMA_LENS`              | ✅      | ❌ deferred| Needs schema-lens registry.                                                   |
| `EXPANDS_FROM … USING …`   | ✅      | ❌ deferred| Needs schema-lens + concept resolver.                                         |
| `ORDER BY contradiction_pressure` | ✅ | ❌ deferred| Needs evaluator to retain `statement_id` per binding for the join.            |
| `WITH evidence = …`        | ✅      | ☑ recorded | Parses to `EvidenceShape`; today the result-row shape is `Bindings` only.    |
| `PROJECT`                  | ✅      | ✅         | Filter the output columns. Empty `PROJECT` ≡ all bound vars.                  |
| `LIMIT` / `OFFSET`         | ✅      | ✅         | Applied after FILTER and PROJECT.                                             |

The deferred clauses still **parse cleanly** — programs written
against the full v2 surface won't surface mysterious "unknown
clause" errors as kernels land. Each deferred clause returns
`EvalError::Unsupported` with the exact reason and the PRD milestone
where it's tracked.

---

## Clause reference

### `SCOPE`

Constrain the contexts the query reads.

```text
SCOPE include ex:src, ex:other
SCOPE exclude ex:secret
SCOPE include ex:project ancestors
SCOPE include ex:project no_descendants
```

- `include` and `exclude` accept comma-separated IRIs.
- `ancestors`: also pull statements from parent contexts.
- `no_descendants`: don't descend into child contexts (default is to descend).

Maps to `donto_client::ContextScope`.

### `PRESET`

A single keyword that translates to a bundle of scope + maturity +
time-pin adjustments. Useful presets for genealogy / language work:

| Preset              | Effect                                                                                 |
|---------------------|----------------------------------------------------------------------------------------|
| `latest`            | Current state (default — no-op).                                                       |
| `raw`               | Sets `MATURITY 0` — include E0 raw extractions.                                         |
| `curated`           | Raises maturity floor to `>= 2` (E2: evidence-supported).                              |
| `under_hypothesis`  | Restricts scope to contexts of kind `hypothesis`.                                       |
| `as_of:<RFC3339>`   | Sets `as_of_tx` (bitemporal time-travel).                                              |
| `anywhere`          | Drops any caller-set scope.                                                            |

```text
PRESET curated
PRESET as_of:2026-01-01T00:00:00Z
```

### `MATCH`

The basic graph pattern. Each triple is three terms (`?var`, IRI, or
literal), optionally followed by an `IN <graph>` clause that overrides
the query scope for that pattern.

```text
MATCH ?x ex:knows ?y, ?y ex:name ?n
MATCH ?stmt ex:reviewed_by ?reviewer IN ex:review-graph
```

Variables are bound across triples by name — same `?y` in two triples
joins them.

### `FILTER`

Boolean expressions over bound variables. Operators: `=`, `!=`, `<`,
`<=`, `>`, `>=`. Numeric operators compare literal numbers (xsd:integer
or xsd:double); compares against non-numeric values return false.

```text
FILTER ?n != "Mallory"
FILTER ?age > 25
FILTER ?score >= 0.8, ?lang = "en"
```

Multiple expressions in one clause are comma-separated and AND-ed.

### `POLARITY`

```text
POLARITY asserted    # default
POLARITY negated
POLARITY absent
POLARITY unknown
```

donto stores polarity as one of four values per statement; a query
defaults to `asserted` so contradictions don't sneak into ordinary
queries without the caller asking for them.

### `MATURITY`

```text
MATURITY 2           # ≡ MATURITY >= 2
MATURITY >= 3
```

Inclusive lower bound on the [E0..E5 maturity
ladder](DONTO-PRD.md#71-maturity-ladder--e0-through-e5). E0 = raw, E5 =
certified. Hardly anything is E5; E2 ("evidence-supported") is the
practical "useful" floor.

### `IDENTITY` / `IDENTITY_LENS`

Both keywords parse to the same `IdentityMode`. `IDENTITY_LENS` is the
PRD §11 v2 name.

```text
IDENTITY default
IDENTITY expand_clusters         # follow IdentityHypothesis clusters
IDENTITY expand_sameas_transitive
IDENTITY strict                  # no expansion at all
```

### `PREDICATES`

How aggressively predicate alignment closures expand the matched
predicate set.

```text
PREDICATES EXPAND                # default — full closure
PREDICATES STRICT                # exact IRI only
PREDICATES EXPAND_ABOVE 80       # follow alignments with confidence ≥ 0.80
```

### `MODALITY`

Filter through the `donto_stmt_modality` overlay (sparse — statements
without an overlay row are dropped).

```text
MODALITY descriptive
MODALITY descriptive, inferred, reconstructed
```

Allowed values: `descriptive`, `prescriptive`, `reconstructed`,
`inferred`, `elicited`, `corpus_observed`, `typological_summary`,
`experimental_result`, `clinical_observation`, `legal_holding`,
`archival_metadata`, `oral_history`, `community_protocol`,
`model_output`, `other`.

### `EXTRACTION_LEVEL`

Filter through the `donto_stmt_extraction_level` overlay. Same
semantics as `MODALITY`.

```text
EXTRACTION_LEVEL quoted
EXTRACTION_LEVEL quoted, table_read, manual_entry
```

Allowed values: `quoted`, `table_read`, `example_observed`,
`source_generalization`, `cross_source_inference`,
`model_hypothesis`, `human_hypothesis`, `manual_entry`,
`registry_import`, `adapter_import`.

### `TRANSACTION_TIME AS_OF` / `AS_OF`

Bitemporal time-travel. The query reads the state of the store as of
the given RFC3339 timestamp (tx_time):

```text
AS_OF "2026-01-01T00:00:00Z"
TRANSACTION_TIME AS_OF "2025-12-31T23:59:59Z"
```

Both forms set `Query.as_of_tx`, which is plumbed to the
`p_as_of_tx` parameter of `donto_match`, `donto_match_strict`, and
`donto_match_aligned`.

**Quoting:** RFC3339 timestamps starting with digits must be
quoted (`"2026-01-01T00:00:00Z"`). The lexer eagerly reads `2026` as
an integer otherwise. Use `PRESET as_of:<ts>` if you need an
unquoted single-token form.

### `POLICY ALLOWS`

```text
POLICY ALLOWS read_metadata
POLICY ALLOWS publish_release
```

**Deferred.** Parses cleanly and stores in `Query.policy_allows`, but
the evaluator returns `Unsupported`. Needs the
statement→source→policy join from the Trust Kernel HTTP middleware
(PRD M0; the SQL substrate in migrations `0111`/`0112` exists).

### `SCHEMA_LENS`

```text
SCHEMA_LENS ex:linguistics-core
SCHEMA_LENS bare_name
```

**Deferred.** Records the lens to apply. Evaluator pending the
schema-lens registry.

### `EXPANDS_FROM … USING …`

```text
EXPANDS_FROM concept ex:case_marking
USING schema_lens ex:linguistics-core
```

**Deferred.** PRD §11.2 example 1. Parses to `Query.expands_from`
(`{ concept, schema_lens }`); evaluator pending.

### `ORDER BY` (one named order only)

```text
ORDER BY contradiction_pressure DESC
ORDER_BY contradiction_pressure         # default DESC
```

The only named ordering is `contradiction_pressure`, computed from
`donto_contradiction_frontier`. **Deferred** at the evaluator until
the binding pipeline retains per-row `statement_id`.

There is no implicit ordering. donto deliberately exposes no default
order — leak-prone callers should always `ORDER BY` explicitly when
order matters.

### `WITH evidence = …`

```text
WITH evidence = redacted_if_required
WITH evidence = full
WITH evidence = none           # default
```

Recorded as `Query.evidence_shape`. The current row shape is
`Bindings` only — evidence is not attached to result rows yet. This is
a future-shape directive (intent), not a filter.

### `PROJECT`, `LIMIT`, `OFFSET`

```text
PROJECT ?x, ?y, ?n
LIMIT 100
OFFSET 0
```

Applied after FILTER. With no `PROJECT`, all bound variables are
returned. Without `LIMIT`, the evaluator returns every row.

---

## A complete worked example

> *Find every claim about Annie Davis's birth that disagrees with
> another claim, in research contexts where the maturity is at least
> E2 (evidence-supported), in the state of the store as of
> 2026-04-01.*

```text
SCOPE include ctx:genes/annie-davis ancestors
PRESET curated
MATCH ?stmt ex:about ex:annie-davis,
      ?stmt ex:predicate ex:born_in,
      ?stmt ex:object    ?place
FILTER ?place != "unknown"
POLARITY asserted
TRANSACTION_TIME AS_OF "2026-04-01T00:00:00Z"
PREDICATES EXPAND_ABOVE 75
PROJECT ?stmt, ?place
LIMIT 50
```

Running this from the CLI:

```bash
donto query "$(cat query.dql)" --dsn "$DONTO_DSN"
```

Or programmatically:

```rust
use donto_query::{parse_dontoql, evaluate};

let q = parse_dontoql(include_str!("../query.dql"))?;
let rows = evaluate(&client, &q).await?;
```

---

## Practical patterns

### "Show me only curated facts about X"

```text
PRESET curated
MATCH ?p ex:about ex:somebody, ?p ?pred ?val
PROJECT ?pred, ?val
LIMIT 100
```

### "What did we know about X last week?"

```text
MATCH ?p ex:about ex:somebody, ?p ?pred ?val
AS_OF "2026-05-07T00:00:00Z"
LIMIT 100
```

### "Where do two sources disagree?"

```text
MATCH ?stmt ex:about ?subject, ?stmt ex:predicate ?pred
POLARITY negated
PROJECT ?stmt, ?subject, ?pred
LIMIT 50
```

(Add `ORDER BY contradiction_pressure DESC` once that clause's
evaluator lands.)

### "Only quoted-source facts, never inferred"

```text
MATCH ?s ?p ?o
EXTRACTION_LEVEL quoted, table_read
LIMIT 100
```

### "Same query, expand across alignment clusters"

```text
MATCH ?s ex:bornInPlace ?city
IDENTITY_LENS expand_clusters
PREDICATES EXPAND_ABOVE 70
LIMIT 100
```

---

## SPARQL subset compatibility

The same algebra is targetable from a SPARQL 1.1 subset; the parser
lives in [`sparql.rs`](../packages/donto-query/src/sparql.rs).
Supported: `PREFIX`, `SELECT` (including `SELECT *`), `WHERE`,
`GRAPH`, basic `FILTER` (numeric and string comparisons), `LIMIT`,
`OFFSET`. Donto-specific dimensions (polarity, maturity, modality,
extraction level, identity lens, AS_OF, etc.) are not in the SPARQL
surface — use DontoQL when you need them.

```sparql
PREFIX ex: <http://example.org/>
SELECT ?x ?y WHERE {
  ?x ex:knows ?y .
  FILTER (?y != ex:mallory)
}
LIMIT 10
```

---

## Tests

The query engine has 69 tests (36 unit + 11 SPARQL/e2e + 11 PRESET +
11 v2-clause integration) — run them with:

```bash
DONTO_TEST_DSN=postgres://donto:donto@127.0.0.1:55432/donto \
  cargo test -p donto-query
```

The integration tests under `packages/donto-query/tests/` skip
cleanly when Postgres is unreachable (`pg_or_skip!` pattern).

---

## What's *not* in DontoQL

These are PRD-listed but out of scope for the v2 surface and tracked
in [`DONTO-PRD.md`](DONTO-PRD.md):

- **Property paths** (`?x ex:knows+ ?y`) — Phase 10 (query planner).
- **`OPTIONAL`** — Phase 10.
- **Aggregations** (`COUNT`, `SUM`, `GROUP BY`) — out of scope; the PRD
  is explicit that aggregations name their order, but the basic
  aggregate functions are not in v2.
- **`UNION`** — out of scope.
- **Updates / mutations** (`INSERT DATA`, `DELETE DATA`) — donto uses
  `donto_assert` / `donto_retract` SQL calls for writes; the query
  language is read-only.

---

## See also

- [`DONTO-PRD.md` §11](DONTO-PRD.md#11-native-query-language-requirements) — product spec.
- [`packages/donto-query/src/dontoql.rs`](../packages/donto-query/src/dontoql.rs) — the parser.
- [`packages/donto-query/src/algebra.rs`](../packages/donto-query/src/algebra.rs) — the `Query` AST.
- [`packages/donto-query/src/evaluator.rs`](../packages/donto-query/src/evaluator.rs) — how queries execute against Postgres.
- [`packages/donto-query/tests/dontoql_v2.rs`](../packages/donto-query/tests/dontoql_v2.rs) — every v2 clause exercised against a live DB.

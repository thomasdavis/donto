# `donto` CLI reference

The `donto` binary is the end-user entry point for the donto quad store.
It's a thin wrapper over the `donto-client`, `donto-ingest`, and
`donto-query` crates. This document is the reference companion to the
built-in `donto --help` and `donto man` output.

> **For other Claude Code agents:** every subcommand's help is
> self-contained — run `donto <cmd> --help` for the authoritative
> signature. This file gives you the big picture and the gotchas.

## Install / invoke

The binary lands on `$PATH` as `donto`:

```bash
# From a checkout:
cargo install --path crates/donto-cli --locked --force
# Lands in ~/.cargo/bin/donto (assumed on $PATH).

# Verify:
which donto && donto --version
```

Optional artefacts:

```bash
# Man page.
donto man > ~/.local/share/man/man1/donto.1
man donto                                   # MANPATH must include ~/.local/share/man

# Shell completions. Generate for bash, zsh, fish, powershell, elvish.
donto completions bash > ~/.local/share/bash-completion/completions/donto
donto completions zsh  > ~/.local/share/zsh/site-functions/_donto
```

## Connection

A running Postgres 16 is required. Connect via:

- `--dsn <URI>` (global flag), OR
- `DONTO_DSN` environment variable, OR
- default `postgres://donto:donto@127.0.0.1:55432/donto` (the repo's
  `scripts/pg-up.sh` matches this).

## Global flags

| Flag            | Env          | Default                                                   | Notes                                 |
| --------------- | ------------ | --------------------------------------------------------- | ------------------------------------- |
| `--dsn <URI>`   | `DONTO_DSN`  | `postgres://donto:donto@127.0.0.1:55432/donto`            | libpq-style DSN.                      |

Logging is filtered by `RUST_LOG` (defaults to `info`).

## Subcommands

### `donto migrate`

Apply the embedded SQL migrations. Idempotent: safe to run before every
operation. `donto` refuses to create a statement before migrations run
because the `donto_statement` table won't exist yet.

```bash
donto migrate
# → "migrations applied"
```

### `donto ingest <PATH> [--format ...] [--default-context IRI] [--batch N]`

Load a file into donto. Auto-batches and returns an `IngestReport` JSON
describing inserted count, elapsed time, quarantined rows.

**Formats** (`--format`):

| Value             | Notes                                                         |
| ----------------- | ------------------------------------------------------------- |
| `n-quads`         | Default. Named graph becomes the context.                     |
| `turtle`          | No graph block — every statement gets `--default-context`.    |
| `trig`            | Named graphs → contexts.                                      |
| `rdf-xml`         | Standard RDF/XML.                                             |
| `json-ld`         | Subset: top-level `@context` prefix map, `@graph`, `@id`,     |
|                   | `@type`, scalar property values.                              |
| `jsonl`           | One JSON statement per line; schema documented below.         |
| `property-graph`  | Neo4j / AGE export. Edges reified as `ex:edge/<id>` IRIs.     |
| `csv`             | Reserved — requires a mapping (not yet on this CLI).          |

**JSONL schema** (one object per line):

```json
{"s":"ex:alice",
 "p":"ex:knows",
 "o":{"iri":"ex:bob"},               // or {"v":"...", "dt":"xsd:string", "lang":null}
 "c":"ctx:src",                      // optional; defaults to --default-context
 "pol":"asserted",                   // asserted | negated | absent | unknown
 "maturity":0,                       // 0..=4
 "valid_lo":"1970-01-01",            // optional
 "valid_hi":"2030-01-01"}            // optional
```

Idempotent on content: replaying the same file does not double-count.

```bash
donto ingest data.nq
donto ingest data.jsonl --format jsonl --default-context ctx:llm/run-42
donto ingest graph.json --format property-graph --batch 5000
```

### `donto match --subject IRI --predicate IRI --object-iri IRI --context IRI --polarity ... --min-maturity N`

Pattern-match against the live store. All filters are optional; an
unset filter leaves its axis unbound. Context sets a single-context
scope with descendants (`ContextScope::just`). Output is
newline-delimited JSON — one statement per line — pipeable through
`jq`.

```bash
donto match --subject ex:alice --polarity any
donto match --predicate ex:knows --min-maturity 3 | jq '.object'
```

**Polarity values**: `asserted` (default), `negated`, `absent`,
`unknown`, or `any` (no filter).

### `donto query '<QUERY>' [--preset NAME]`

Parse and evaluate a DontoQL or SPARQL-subset query. The dispatcher
uses the first non-whitespace keyword:

- starts with `SELECT` or `PREFIX` → SPARQL subset,
- otherwise → DontoQL.

Output is newline-delimited JSON, one row per line. `--preset` sets a
named scope preset (see PRD §7; registered via `donto_preset_scope`).

**DontoQL phase-4 grammar** (high level):

```text
[SCOPE ...] [PRESET <name>]
MATCH <triple> (, <triple>)*
[FILTER <var> (= | !=) <term> (, <filter>)*]
[POLARITY asserted|negated|absent|unknown]
[MATURITY [>=] <int>]
[IDENTITY <iri>]
[PROJECT ?v1 (, ?v2)*]
[LIMIT <int>] [OFFSET <int>]
```

`<` / `<=` / `>` / `>=` are intentionally rejected in FILTER (phase 4
ships with equality only).

**SPARQL subset**: `PREFIX`, `SELECT`, basic graph patterns, `GRAPH`
blocks, `FILTER`, `LIMIT` outside `WHERE`.

```bash
donto query 'MATCH ?x ex:knows ?y PROJECT ?x, ?y LIMIT 10'
donto query 'PREFIX ex: <http://example.org/>
             SELECT ?name WHERE { ?p ex:name ?name . FILTER (?p != ex:mallory) }'
```

### `donto retract <UUID>`

Close an open statement's `tx_time`. Idempotent: closing an
already-closed statement prints `no open statement` and exits 0.
Bitemporal note: the physical row is preserved — an as-of query before
the retraction still returns the statement.

```bash
donto retract 01234567-89ab-cdef-0123-456789abcdef
```

### `donto bench --insert-count N`

Run the builtin performance smoke subset (PRD §25 H1–H10). Writes N
synthetic rows under a throwaway context, times a point query and a
batch query. JSON report on stdout.

```bash
donto bench --insert-count 50000
```

### `donto man`

Emit the full roff-formatted man page on stdout. Redirect into
`~/.local/share/man/man1/donto.1` and then `man donto`.

### `donto completions <SHELL>`

Emit shell completions for `bash`, `zsh`, `fish`, `powershell`, or
`elvish` on stdout.

## Machine-readable output

Every data-producing subcommand writes either:

- a single pretty-printed JSON object (`ingest` reports, `bench`), or
- newline-delimited JSON, one row per line (`match`, `query`).

Newline-delimited JSON is the right shape for streaming into `jq`,
`xargs`, or a downstream agent.

## Exit codes

- `0` — success (including `retract` of an already-closed statement).
- `1` — connection failure, parse error, or unhandled runtime error.
- Subcommand-specific error text on stderr.

## Common invocation patterns

```bash
# Boot a local Postgres, migrate, ingest, query.
./scripts/pg-up.sh
donto migrate
donto ingest sample.nq
donto query 'MATCH ?s ?p ?o LIMIT 5' | jq

# Point a different DSN.
DONTO_DSN=postgres://u:p@host:5432/db donto match --subject ex:alice

# Inspect a single statement, then retract it.
donto match --subject ex:stale --polarity any | jq -r '.id' | head -n1 | xargs donto retract
```

## Where to read next

- `CLAUDE.md` at the repo root — contract for AI agents contributing code.
- `docs/USER-GUIDE.md` — human walkthrough.
- `docs/OPERATOR-GUIDE.md` — ops / deployment notes.
- `PRD.md` — full product spec; authoritative for any semantic question.

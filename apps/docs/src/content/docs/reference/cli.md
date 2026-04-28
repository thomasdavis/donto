---
title: CLI Reference
description: Complete reference for the donto command-line interface
---

The `donto` binary is the end-user entry point for the donto quad store.
It's a thin wrapper over the `donto-client`, `donto-ingest`, and
`donto-query` crates.

## Install / invoke

```bash
# From a checkout:
cargo install --path apps/donto-cli --locked --force

# Verify:
which donto && donto --version
```

Optional artefacts:

```bash
# Man page.
donto man > ~/.local/share/man/man1/donto.1
man donto

# Shell completions (bash, zsh, fish, powershell, elvish).
donto completions bash > ~/.local/share/bash-completion/completions/donto
donto completions zsh  > ~/.local/share/zsh/site-functions/_donto
```

## Connection

A running Postgres 16 is required. Connect via:

- `--dsn <URI>` (global flag), OR
- `DONTO_DSN` environment variable, OR
- default `postgres://donto:donto@127.0.0.1:55432/donto`

| Flag            | Env          | Default                                        |
| --------------- | ------------ | ---------------------------------------------- |
| `--dsn <URI>`   | `DONTO_DSN`  | `postgres://donto:donto@127.0.0.1:55432/donto` |

Logging is filtered by `RUST_LOG` (defaults to `info`).

## Subcommands

### `donto migrate`

Apply the embedded SQL migrations. Idempotent: safe to run before every operation.

```bash
donto migrate
```

### `donto ingest <PATH>`

Load a file into donto. Auto-batches and returns an `IngestReport` JSON.

**Flags:** `--format`, `--default-context`, `--batch`

| Format           | Notes                                                  |
| ---------------- | ------------------------------------------------------ |
| `n-quads`        | Default. Named graph becomes the context.              |
| `turtle`         | No graph block — every statement gets `--default-context`. |
| `trig`           | Named graphs become contexts.                          |
| `rdf-xml`        | Standard RDF/XML.                                      |
| `json-ld`        | Subset: `@context`, `@graph`, `@id`, `@type`, scalars. |
| `jsonl`          | One JSON statement per line (schema below).            |
| `property-graph` | Neo4j / AGE export. Edges reified as IRIs.             |
| `csv`            | Reserved — requires a mapping (not yet on CLI).        |

**JSONL schema:**

```json
{"s":"ex:alice",
 "p":"ex:knows",
 "o":{"iri":"ex:bob"},
 "c":"ctx:src",
 "pol":"asserted",
 "maturity":0,
 "valid_lo":"1970-01-01",
 "valid_hi":"2030-01-01"}
```

Idempotent on content: replaying the same file does not double-count.

```bash
donto ingest data.nq
donto ingest data.jsonl --format jsonl --default-context ctx:llm/run-42
donto ingest graph.json --format property-graph --batch 5000
```

### `donto match`

Pattern-match against the live store. All filters optional.

**Flags:** `--subject`, `--predicate`, `--object-iri`, `--context`, `--polarity`, `--min-maturity`

Output: newline-delimited JSON (one statement per line).

**Polarity values:** `asserted` (default), `negated`, `absent`, `unknown`, `any`.

```bash
donto match --subject ex:alice --polarity any
donto match --predicate ex:knows --min-maturity 3 | jq '.object'
```

### `donto query '<QUERY>'`

Parse and evaluate a DontoQL or SPARQL-subset query.

- Starts with `SELECT` or `PREFIX` -> SPARQL subset
- Otherwise -> DontoQL

**DontoQL grammar:**

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

```bash
donto query 'MATCH ?x ex:knows ?y PROJECT ?x, ?y LIMIT 10'
donto query 'PREFIX ex: <http://example.org/>
             SELECT ?name WHERE { ?p ex:name ?name . FILTER (?p != ex:mallory) }'
```

### `donto retract <UUID>`

Close an open statement's `tx_time`. Idempotent.

```bash
donto retract 01234567-89ab-cdef-0123-456789abcdef
```

### `donto bench --insert-count N`

Run the builtin performance smoke subset. JSON report on stdout.

```bash
donto bench --insert-count 50000
```

### `donto man`

Emit the full roff-formatted man page on stdout.

### `donto completions <SHELL>`

Emit shell completions for `bash`, `zsh`, `fish`, `powershell`, or `elvish`.

## Machine-readable output

Every data-producing subcommand writes either:

- a single pretty-printed JSON object (`ingest` reports, `bench`), or
- newline-delimited JSON, one row per line (`match`, `query`).

## Exit codes

- `0` — success (including `retract` of an already-closed statement).
- `1` — connection failure, parse error, or unhandled runtime error.

## Common invocation patterns

```bash
# Boot, migrate, ingest, query.
./scripts/pg-up.sh
donto migrate
donto ingest sample.nq
donto query 'MATCH ?s ?p ?o LIMIT 5' | jq

# Different DSN.
DONTO_DSN=postgres://u:p@host:5432/db donto match --subject ex:alice

# Inspect then retract.
donto match --subject ex:stale --polarity any | jq -r '.id' | head -n1 | xargs donto retract
```

# donto User Guide

## Mental model

Read [PRD §2 (the maturity ladder)](../PRD.md#2-the-semantic-maturity-ladder)
and [PRD §3 (design principles)](../PRD.md#3-design-principles) first.
The two ideas to internalize:

1. **The atom is the statement.** Everything is a statement: contexts hold them,
   shapes inspect them, rules emit them, certificates justify them.
2. **Contradictions coexist.** Two sources can disagree about Alice's birth
   year. donto stores both; you query under a scope that resolves the
   tension if you want one.

## Getting data in

### N-Quads, Turtle, JSON-LD, RDF/XML

```bash
donto ingest data.nq                      # N-Quads
donto ingest data.ttl --format turtle
donto ingest data.trig --format trig
donto ingest data.jsonld --format json-ld
donto ingest data.rdf --format rdf-xml
```

The graph IRI on each quad becomes the donto context. Triple-only formats
(Turtle, RDF/XML, basic JSON-LD) need `--default-context`:

```bash
donto ingest census-1900.ttl \
    --format turtle \
    --default-context ctx:src/census-1900
```

### LLM extractor JSONL

One statement per line:

```jsonl
{"s":"ex:alice","p":"ex:birthYear","o":{"v":1899,"dt":"xsd:integer"},"c":"ctx:src/wikipedia","pol":"asserted","maturity":0}
```

```bash
donto ingest claims.jsonl --format jsonl --default-context ctx:src/extractor-v2
```

### Property-graph dumps (Neo4j-style)

```bash
donto ingest neo4j-export.json --format property-graph
```

## Querying

Three surfaces. All compile to the same algebra.

### SQL (workhorse)

```sql
select subject, predicate, object_iri
  from donto_match(
      p_subject := 'ex:alice',
      p_scope := '{"include":["ctx:src/wikipedia"]}'::jsonb,
      p_polarity := 'asserted');
```

### SPARQL 1.1 subset

```bash
echo 'PREFIX ex: <http://example.org/>
SELECT ?x ?y WHERE { ?x ex:knows ?y . } LIMIT 10' | \
    donto query "$(cat)"
```

### DontoQL (native; full feature set)

```bash
donto query 'SCOPE include <ctx:src/wikipedia>
             MATCH ?x ex:knows ?y, ?y ex:name ?n
             FILTER ?n != "Mallory"
             POLARITY asserted
             MATURITY >= 1
             PROJECT ?x, ?n
             LIMIT 10'
```

### Scope presets

| Preset | Meaning |
|---|---|
| `latest` | Default. Excludes `hypothesis` and `quarantine`. |
| `raw` | Permissive contexts only (`source`, `pipeline`). |
| `curated` | Snapshot/derivation/trust/custom; maturity ≥ 1. |
| `anywhere` | Forensic. All contexts. |

```bash
donto query 'PRESET curated MATCH ?s ex:p ?o' --preset curated
```

## Contexts

Every statement is in a context. Default is `donto:anonymous`. Create one:

```sql
select donto_ensure_context('ctx:src/wikipedia', 'source', 'permissive', null);
select donto_ensure_context('ctx:hypo/alice_merge', 'hypothesis', 'permissive', null);
```

Hypothesis contexts let you reason "assuming this is true" without polluting
the curated view:

```bash
donto query 'SCOPE include <ctx:hypo/alice_merge> ancestors
             MATCH ?x donto:sameAs ?y'
```

## Bitemporal queries

```sql
-- "as of 2026-01-15, what did we believe Alice's birth year was?"
select * from donto_match(
    p_subject := 'ex:alice',
    p_predicate := 'ex:birthYear',
    p_as_of_tx := '2026-01-15'::timestamptz);

-- "what statements claim something true during 1899?"
select * from donto_match(p_as_of_valid := '1899-06-01'::date);
```

## Shape validation

Built-ins ship in dontosrv:

```bash
curl -X POST http://localhost:7878/shapes/validate -H 'content-type: application/json' \
     -d '{"shape_iri":"builtin:functional/ex:spouse",
          "scope":{"include":["ctx:src/wikipedia"]}}'
```

User-authored shapes live in Lean (see `lean/Donto/Shapes.lean` and
`docs/OPERATOR-GUIDE.md`).

## Derivations

```bash
curl -X POST http://localhost:7878/rules/derive -H 'content-type: application/json' \
     -d '{"rule_iri":"builtin:transitive/ex:parent",
          "scope":{"include":["ctx:src/wikipedia"]},
          "into":"ctx:derivation/ancestors"}'
```

The derivation context is queryable like any other context. Each derived
statement carries lineage pointers to its inputs.

## Snapshots

Freeze the visible state under a scope into a named context for reproducible
historical queries:

```sql
select donto_snapshot_create(
    'ctx:snapshot/2026-04-17',
    '{"include":["ctx:src/wikipedia","ctx:src/census-1900"], "exclude":["ctx:hypo/alice_merge"]}'::jsonb,
    'pre-LLM-rerun checkpoint');
```

Then:

```bash
donto query 'SCOPE include <ctx:snapshot/2026-04-17>
             MATCH ?s ex:birthYear ?y'
```

is deterministic regardless of subsequent retractions.

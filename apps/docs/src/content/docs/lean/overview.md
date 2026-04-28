---
title: Lean Overlay
description: How donto pairs Postgres with an optional Lean 4 verification sidecar
---

donto pairs a paraconsistent Postgres store with an optional Lean 4
sidecar that does two distinct jobs:

1. **Prove things about the data model.** Theorems in
   `lean/Donto/Theorems.lean` are kernel-checked propositions that hold
   for *every* possible input — not just the cases we happened to test.
2. **Run user-authored shapes and rules.** A `donto_engine` binary speaks
   line-delimited JSON DIR over stdio. `dontosrv` spawns it on demand
   and forwards `lean:` shape IRIs to it.

donto runs without Lean. The standard-library shapes and rules are
mirrored in Rust (`builtin:functional/...`, `builtin:transitive/...`),
so the database stays useful when the engine is offline.

## What the theorems prove

`lean/Donto/Theorems.lean` is a small file with a big claim. Each `theorem`
formalizes a PRD invariant; `lake build` succeeds only if every proof
type-checks.

| Theorem                                            | Why it matters |
| -------------------------------------------------- | -------------- |
| `polarity_total`                                   | Every statement has one of exactly four polarities. |
| `assert_negate_distinct`                           | An asserted statement and its negation are *different rows*. |
| `default_visibility_asserted_only`                 | Default queries return only asserted statements. |
| `confidence_strong_dominates`, `_floor`, `_reflexive` | The four-tier confidence ordering is total. |
| `maturity_bounded`                                 | The maturity ladder has exactly five levels. |
| `retract_preserves_identity`                       | Retraction never changes the statement_id, subject, predicate, object, or context. |
| `retract_does_not_negate`                          | Polarity and modality are independent. |
| `snapshot_membership_is_monotone`                  | Once a statement_id is in a snapshot, it stays in. |
| `exclude_wins_over_include`                        | Scope `exclude` always wins over scope `include`. |
| `identical_inputs_are_equal`                       | Two structurally identical statements are equal. |

Build and check:

```bash
cd lean
lake build
```

If it succeeds, every theorem above is true for every possible input, forever.

## What the engine runs

The engine is a child process. It reads JSON envelopes from stdin, one
per line, and writes one response per line on stdout.

```bash
cd lean
lake build              # produces .lake/build/bin/donto_engine

# Drive it directly:
echo '{"version":"0.1.0-json","kind":"ping"}' | ./.lake/build/bin/donto_engine
# {"version":"0.1.0-json","kind":"ready","engine":"lean"}
# {"version":"0.1.0-json","kind":"pong"}
```

Currently supported envelope kinds:

| Kind               | Direction            | Body                                          |
| ------------------ | -------------------- | --------------------------------------------- |
| `ready`            | engine -> parent     | one-shot banner on launch                     |
| `ping`             | parent -> engine     | engine replies `pong`                         |
| `validate_request` | parent -> engine     | `shape_iri`, `scope`, `statements[]`          |
| `validate_response`| engine -> parent     | `shape_iri`, `focus_count`, `violations[]`    |
| `error`            | engine -> parent     | `error` string                                |

## Running with dontosrv

```bash
# 1. Build the engine.
cd lean && lake build && cd ..

# 2. Start dontosrv with --lean-engine.
cargo run -p dontosrv -- \
  --lean-engine "$(pwd)/lean/.lake/build/bin/donto_engine"
```

Now the `lean:` shape IRI scheme is live:

```bash
curl -X POST localhost:7878/shapes/validate -H 'content-type: application/json' \
  -d '{"shape_iri":"lean:builtin/parent-child-age-gap",
       "scope":{"include":["ctx:src/census"]}}'
```

Without `--lean-engine`, `lean:` shapes return `sidecar_unavailable` but
`builtin:` shapes still work.

## Authoring a custom shape

Add a new combinator to `lean/Donto/Shapes.lean`:

```lean
namespace Donto.Shapes.StdLib

def implausibleBirthYear : Shape :=
  { iri := "lean:custom/implausible-birth-year"
    severity := .warning
    evaluate := fun stmts =>
      let bad := stmts.filterMap fun s =>
        if s.predicate == "ex:birthYear" && s.polarity == .asserted then
          match s.object with
          | .lit v _ _ =>
              match v.toInt? with
              | some n => if n < 1500 then
                  some { focus := s.subject
                         reason := s!"birth year {n} predates 1500"
                         evidence := [s.id.getD ""] }
                else none
              | none => none
          | _ => none
        else none
      { shapeIri := "lean:custom/implausible-birth-year"
        focusCount := stmts.length
        violations := bad } }

end Donto.Shapes.StdLib
```

Register it in `Donto.Engine.lookupShape` and rebuild.

## Roadmap

1. **More built-ins.** `RangeShape`, `MinCardinality`, `AcyclicClosure`, `PathShape`, `Disjoint`, etc.
2. **Lean-authored derivation rules.** The protocol is symmetric — `derive_request`/`derive_response` is already in DIR.
3. **Certificate verifiers in Lean.** Produce actual proof objects that an external tool could check independently.

None of these change the operational contract; the database stays usable when Lean is offline.

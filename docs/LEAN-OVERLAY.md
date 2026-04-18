# Lean overlay

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
so the database stays useful when the engine is offline. This is the
PRD §15 sidecar contract.

## What the theorems prove

`lean/Donto/Theorems.lean` is a small file with a big claim. Each `theorem`
formalizes a PRD invariant; `lake build` succeeds only if every proof
type-checks.

| Theorem                                            | PRD reference   | Why it matters |
| -------------------------------------------------- | --------------- | -------------- |
| `polarity_total`                                   | §3, §6          | Every statement has one of exactly four polarities. There is no "null" or "indeterminate" fifth state to forget about. |
| `assert_negate_distinct`                           | §3 principle 1  | An asserted statement and its negation are *different rows*. donto cannot silently coerce them into one. |
| `default_visibility_asserted_only`                 | §6 truth table  | Default queries return only asserted statements; negated/absent/unknown require explicit opt-in. |
| `confidence_strong_dominates`, `_floor`, `_reflexive` | §6           | The four-tier confidence ordering is total. |
| `maturity_bounded`                                 | §2              | The maturity ladder has exactly five levels. The structure invariant carries the bound; you cannot construct level 5. |
| `retract_preserves_identity`                       | §3 principle 3  | Retraction never changes the statement_id, subject, predicate, object, or context. The Postgres mirror is `update donto_statement set tx_time = ... where statement_id = $1`. |
| `retract_does_not_negate`                          | §6              | Polarity and modality are independent. A retracted-asserted statement is *not* equivalent to a negated one. |
| `snapshot_membership_is_monotone`                  | §8              | Once a statement_id is in a snapshot, it stays in. Subsequent retractions of the source row don't remove it. |
| `exclude_wins_over_include`                        | §7              | Scope `exclude` always wins over scope `include` — the rule that confused us during Phase 2. |
| `identical_inputs_are_equal`                       | §19             | Two structurally identical statements are equal. Underpins the idempotency guarantee of `donto_assert`. |

Build and check:

```bash
cd lean
lake build
```

If it succeeds, every theorem above is true for every possible input,
forever. If a future PR breaks one of them, the build will fail and we
will have to either fix the code or change the theorem (and document why).

## What the engine runs

The engine is a child process. It reads JSON envelopes from stdin, one
per line, and writes one response per line on stdout. The first line it
emits is a banner so the parent can synchronise.

```bash
cd lean
lake build              # produces .lake/build/bin/donto_engine

# Drive it directly:
echo '{"version":"0.1.0-json","kind":"ping"}' | ./.lake/build/bin/donto_engine
# {"version":"0.1.0-json","kind":"ready","engine":"lean"}
# {"version":"0.1.0-json","kind":"pong"}
```

Currently supported envelope kinds:

| Kind               | Direction            | Body                                                   |
| ------------------ | -------------------- | ------------------------------------------------------ |
| `ready`            | engine → parent      | one-shot banner on launch                              |
| `ping`             | parent → engine      | (none) — engine replies `pong`                          |
| `validate_request` | parent → engine      | `shape_iri`, `scope`, `statements[]`                   |
| `validate_response`| engine → parent      | `shape_iri`, `focus_count`, `violations[]`             |
| `error`            | engine → parent      | `error` string — parse failure or unknown shape iri    |

Statements are full objects (`subject`, `predicate`, `object_iri` or
`object_lit`, `context`, `polarity`, optional `id`, `valid_lo`,
`valid_hi`). The engine never queries Postgres directly — it operates
on the snapshot of statements that `dontosrv` ships in the request.
That keeps the boundary narrow.

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
# Insert some genealogy data.
psql donto <<'SQL'
SELECT donto_ensure_context('ctx:src/census', 'source', 'permissive', NULL);
SELECT donto_assert('ex:alice', 'ex:parentOf', 'ex:bob', NULL,
                    'ctx:src/census', 'asserted', 0, NULL, NULL, NULL);
SELECT donto_assert('ex:alice', 'ex:birthYear', NULL,
                    '{"v":1850,"dt":"xsd:integer"}'::jsonb,
                    'ctx:src/census', 'asserted', 0, NULL, NULL, NULL);
SELECT donto_assert('ex:bob', 'ex:birthYear', NULL,
                    '{"v":1855,"dt":"xsd:integer"}'::jsonb,
                    'ctx:src/census', 'asserted', 0, NULL, NULL, NULL);
SQL

# Validate via the Lean engine.
curl -X POST localhost:7878/shapes/validate -H 'content-type: application/json' \
  -d '{"shape_iri":"lean:builtin/parent-child-age-gap",
       "scope":{"include":["ctx:src/census"]}}'
# {"shape_iri":"lean:builtin/parent-child-age-gap",
#  "source":"lean",
#  "report":{"focus_count":1,
#            "violations":[{
#              "focus":"ex:alice",
#              "reason":"parent ex:alice (1850) is only 5y older than child ex:bob (1855); minimum 12",
#              "evidence":["<uuid>"]}]}}
```

Note `"source":"lean"` — the report came from a real Lean process, not
the Rust mirror.

If you start dontosrv without `--lean-engine`:

```bash
cargo run -p dontosrv
# (no --lean-engine flag)

curl -X POST localhost:7878/shapes/validate -H 'content-type: application/json' \
  -d '{"shape_iri":"lean:builtin/parent-child-age-gap","scope":{...}}'
# {"error":"sidecar_unavailable",
#  "shape_iri":"lean:builtin/parent-child-age-gap",
#  "detail":"Lean engine not configured (start dontosrv with --lean-engine /path/to/donto_engine)"}
```

The `builtin:` shapes still work in this mode. The PRD §15 sidecar
contract: donto stays useful when Lean is gone.

## Authoring a custom shape

Add a new combinator to `lean/Donto/Shapes.lean`:

```lean
namespace Donto.Shapes.StdLib

/-- Reject any statement asserting a person was born before 1500
    (likely a transcription error). -/
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

Register it in `Donto.Engine.lookupShape` and rebuild. The shape is
immediately reachable as `lean:custom/implausible-birth-year` over the
HTTP API.

## Roadmap

The Lean overlay has three phases beyond what's wired today:

1. **More built-ins.** PRD §16 lists `RangeShape`, `MinCardinality`,
   `AcyclicClosure`, `PathShape`, `Disjoint`, etc.
2. **Lean-authored derivation rules.** Currently rules are Rust-only.
   The protocol is symmetric — `derive_request`/`derive_response` is
   already in DIR; it needs an engine-side dispatcher and a
   `LeanClient::derive` method on the Rust side.
3. **Certificate verifiers in Lean.** PRD §18 names seven kinds; the
   Rust verifiers are correctness-only. A Lean verifier would produce
   an actual proof object that an external tool could check independently
   of dontosrv.

None of these change the operational contract; the database stays
usable when Lean is offline.

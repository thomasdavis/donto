---
title: Formalization Plan
description: Taking donto's Lean overlay from working-but-minimal to a full proof-carrying evidence substrate
---

Research document for taking donto's Lean overlay from "working but
minimal" to a full proof-carrying evidence substrate.

## Current State

### What exists (12 files, ~900 lines)

| File | Lines | What it does |
|------|-------|---|
| `Core.lean` | 64 | Mirror of Postgres types: `Statement`, `Context`, `Polarity`, `Modality`, `Confidence`, `Maturity`, `Object`, `ContextScope` |
| `IR.lean` | 32 | DIR envelope and 12 directive variants |
| `Truth.lean` | 24 | `visibleByDefault` and `atLeast` (confidence ordering) |
| `Temporal.lean` | 22 | `ValidTime` and `Precision` stub |
| `Predicates.lean` | 23 | `Predicate` structure mirroring `donto_predicate` |
| `Shapes.lean` | 202 | `Shape` structure + 4 stdlib shapes |
| `Rules.lean` | 55 | `Rule` structure + `transitiveClosure` combinator |
| `Certificate.lean` | 42 | `Certificate` + `Kind` enum (7 kinds) + two minimal verifiers |
| `Theorems.lean` | 269 | 20+ kernel-checked theorems |
| `Engine.lean` | 153 | JSON dispatch loop |
| `Main.lean` | 43 | Stdin/stdout line protocol with ready banner |
| `Donto.lean` | 12 | Root re-export |

### What's wired end-to-end

- **Shapes via Lean engine:** `dontosrv` spawns `donto_engine`, sends validate requests, receives violation reports
- **Shapes via Rust builtins:** `builtin:functional/` and `builtin:datatype/` mirrored in Rust
- **Rules via Rust builtins:** `builtin:transitive/`, `builtin:inverse/`, `builtin:symmetric/`
- **Certificates via Rust:** 5 of 7 verifiers implemented
- **Theorems:** All 20+ theorems compile and are kernel-checked by `lake build`

### What's missing

- Lean IR doesn't include the 11 new evidence-substrate directives
- No `derive_request` handler in Engine — rules are Rust-only
- Certificate verifiers in Lean are trivial stubs
- No formalization of: scope resolution, correction semantics, SameMeaning transitivity, bitemporal canonical drift, hypothesis scoping, evidence link chains, proof obligation lifecycle, argumentation framework semantics
- No proof-carrying shapes or derivation

## Implementation Plan

### Phase L1: Semantics Foundation

Formalize the core donto semantics that everything else depends on.

**L1.1 Scope Resolution** — Prove properties about `donto_resolve_scope`: exclude wins over include, descendant/ancestor monotonicity, determinism, no hypothesis leakage.

**L1.2 Correction Semantics** — Correction = retract old + assert new. Prove context inheritance, identity preservation, fresh ID for new statement.

**L1.3 SameMeaning Semantics** — Prove SameMeaning edges form an equivalence relation (reflexive, symmetric, transitive). Cluster membership stable under edge addition.

**L1.4 Hypothesis Scoping** — Prove non-leakage (hypothesis statements not visible from base scope), sibling isolation, base visibility with ancestors.

**L1.5 Bitemporal Canonical Drift** — Prove temporal alias resolution is deterministic, open-world (unregistered IRI -> self), timeless fallback, no alias chains.

### Phase L2: Proof-Carrying Shapes

Make shapes produce evidence witnesses, not just violation reports.

- `ShapeWitness` type for each stdlib shape
- Soundness theorems: if no violations reported, the property holds; if violation reported, it's genuine
- 10 new shape combinators (RangeShape, MinCardinality, AcyclicClosure, PathShape, Disjoint, TemporalShape, ContradictionShape, SupportThresholdShape, ExtractorQualityShape, MaxCardinality)

### Phase L3: Proof-Carrying Derivation

Make rules produce proof trees that Lean can verify.

- `ProofNode` / `ProofTree` types
- Soundness theorem for `transitiveClosure`
- Wire `derive_request` handler in `Engine.lean`

### Phase L4: Certificate Verifiers in Lean

Replace the Rust stub verifiers with real Lean verifiers that produce checkable proof objects.

| Kind | Rust Status | Lean Target |
|------|-------------|---|
| `direct_assertion` | Working | Verify source IRI resolves to a real document |
| `substitution` | Working | Verify substitution is semantically valid |
| `transitive_closure` | Working | Verify proof tree against the transitive axiom |
| `confidence_justification` | Stub | Verify evidence supports the claimed tier |
| `shape_entailment` | Partial | Verify the shape witness is sound |
| `hypothesis_scoped` | Working | Verify the hypothesis context actually scopes the claim |
| `replay` | Stub | Re-run the derivation and compare |

### Phase L5: Evidence Substrate Formalization

Formalize the evidence substrate tables (0023-0033) so Lean can reason about evidence chains, extraction provenance, and proof obligations.

### Phase L6: Update IR and Engine

Add the 11 new directive variants to `IR.lean` and wire handlers in `Engine.lean`.

## Dependencies

```
L1 Semantics Foundation ──→ L2 Proof-Carrying Shapes ──→ L4 Certificate Verifiers
                            L3 Proof-Carrying Derivation ──┘
L5 Evidence Substrate (parallel with L2/L3) ──→ L6 IR + Engine Update
```

**Critical path:** L1 -> L2 -> L4.

## Theorem Inventory

Total new theorems across all phases: ~36.

## Risk Assessment

- **Lean 4 version stability.** Pin the toolchain, don't chase nightly.
- **Proof complexity.** Some theorems require non-trivial induction. Budget for iteration.
- **Performance.** Lean shape evaluation is single-threaded. Rust builtins are the escape hatch for large scopes.
- **Scope creep.** Keep L5 to types and invariant theorems; don't formalize the full SQL surface.
- **Date arithmetic.** Use ISO 8601 lexicographic comparison rather than building a full date library.

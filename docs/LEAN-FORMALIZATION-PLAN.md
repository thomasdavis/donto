# Lean Formalization Plan

Research document for taking donto's Lean overlay from "working but
minimal" to a full proof-carrying evidence substrate.

## 1. Current State

### 1.1 What exists (12 files, ~900 lines)

| File | Lines | What it does |
|------|-------|---|
| `Core.lean` | 64 | Mirror of Postgres types: `Statement`, `Context`, `Polarity`, `Modality`, `Confidence`, `Maturity`, `Object`, `ContextScope` |
| `IR.lean` | 32 | DIR envelope and 12 directive variants (matches Rust `dir.rs` pre-evidence-substrate) |
| `Truth.lean` | 24 | `visibleByDefault` and `atLeast` (confidence ordering) |
| `Temporal.lean` | 22 | `ValidTime` and `Precision` stub — no actual date arithmetic |
| `Predicates.lean` | 23 | `Predicate` structure mirroring `donto_predicate` |
| `Shapes.lean` | 202 | `Shape` structure + 4 stdlib shapes: `functional`, `datatype`, `parentChildAgeGap`, `roleFit` |
| `Rules.lean` | 55 | `Rule` structure + `transitiveClosure` combinator (pure, not wired to engine) |
| `Certificate.lean` | 42 | `Certificate` + `Kind` enum (7 kinds) + two minimal verifiers (`verifyDirect`, `verifyTransitive`) |
| `Theorems.lean` | 269 | 20+ kernel-checked theorems about polarity, confidence, maturity, bitemporality, snapshots, scopes, idempotency |
| `Engine.lean` | 153 | JSON dispatch loop: parses statements, routes `validate_request` to shapes, returns reports |
| `Main.lean` | 43 | Stdin/stdout line protocol with ready banner |
| `Donto.lean` | 12 | Root re-export |

### 1.2 What's wired end-to-end

- **Shapes via Lean engine:** `dontosrv` spawns `donto_engine`, sends
  `validate_request` envelopes with scoped statements, receives
  `validate_response` with violations. Works for `lean:functional/`,
  `lean:datatype/`, `lean:builtin/parent-child-age-gap`,
  `lean:role/fit/`. Reports cached in `donto_shape_report`.

- **Shapes via Rust builtins:** `builtin:functional/` and
  `builtin:datatype/` are mirrored in Rust. Work without Lean.

- **Rules via Rust builtins:** `builtin:transitive/`,
  `builtin:inverse/`, `builtin:symmetric/` are fully implemented in
  `rules.rs` with SQL-based closure, lineage tracking, and caching.

- **Certificates via Rust:** 5 of 7 verifiers implemented
  (`direct_assertion`, `substitution`, `transitive_closure`,
  `hypothesis_scoped`, `shape_entailment`). Two stubbed
  (`confidence_justification`, `replay`).

- **Theorems:** All 20+ theorems compile and are kernel-checked by
  `lake build`. They prove model invariants but are not connected to
  runtime — they're compile-time guarantees only.

### 1.3 What's missing

**Missing from the Lean side:**

1. Lean IR doesn't include the 11 new evidence-substrate directives
   (IngestDocument, CreateSpan, etc.)
2. No `derive_request` handler in Engine — rules are Rust-only
3. Certificate verifiers in Lean are trivial stubs
4. No formalization of:
   - Scope resolution semantics (descendant walk, exclude-over-include)
   - Correction semantics (retract + re-assert)
   - SameMeaning transitivity
   - Bitemporal canonical drift
   - Hypothesis scoping and leakage
   - Context environment overlays
   - Evidence link chains
   - Proof obligation lifecycle
   - Argumentation framework semantics
5. No proof-carrying shapes (shapes produce reports, not witnesses)
6. No proof-carrying derivation (rules produce statements, not proof trees)
7. Temporal.lean has no actual date arithmetic

**Missing from the boundary:**

1. Lean `derive_request` → `derive_response` is defined in IR but
   Engine.lean only handles `validate_request` and `ping`
2. Certificate verification in Lean is not reachable from dontosrv
   (Rust verifiers are used instead)
3. No way to ask Lean to verify an evidence chain or proof obligation

## 2. Implementation Plan

### Phase L1: Semantics Foundation

Formalize the core donto semantics that everything else depends on.
This is the "Lean owns meaning" layer.

#### L1.1 Scope Resolution

The SQL function `donto_resolve_scope` is 50 lines of recursive CTE.
The Lean formalization needs to prove properties about it without
reimplementing Postgres.

```lean
-- Key types already exist in Core.lean. Add:

structure ContextTree where
  contexts : List Context
  -- Parent links form a forest (no cycles, multiple roots)
  hForest : ∀ c ∈ contexts, c.parent.map (·  ∈ contexts.map (·.iri)) ≠ some false

def resolve (tree : ContextTree) (scope : ContextScope) : List IRI :=
  -- Include + descendants + ancestors, minus excludes
  sorry -- actual implementation

-- Theorems to prove:

-- 1. Exclude always wins over include (already proved for flat lists;
--    extend to tree-resolved sets)
-- 2. An empty include list resolves to all contexts minus excludes
-- 3. Descendant resolution is monotone: adding a child never removes
--    a previously-included context
-- 4. Ancestor resolution is monotone: adding a parent never removes
--    a previously-included context
-- 5. Scope resolution is deterministic: same tree + same scope = same result
-- 6. No leakage: a hypothesis context is only visible if the scope
--    explicitly includes it or one of its ancestors
```

#### L1.2 Correction Semantics

Correction = retract old + assert new. Key property: the old row's
identity is preserved (same `statement_id`), it just gets closed
tx_time.

```lean
def correct (old : Statement) (patch : Statement) : (Statement × Statement) :=
  let retracted := { old with modality := .retracted }
  let new := { patch with
    context := old.context   -- context is inherited, not overridable
    maturity := old.maturity -- maturity inherited
  }
  (retracted, new)

-- Theorems:
-- 1. Corrected statement inherits context from original
-- 2. Retracted original preserves identity (subject, predicate, object, context)
-- 3. New statement has fresh identity (different id)
-- 4. If patch is structurally identical to old, result is idempotent
```

#### L1.3 SameMeaning Semantics

SameMeaning is symmetric and transitively closable. The SQL
implementation stores both directions explicitly. The Lean
formalization should prove closure properties.

```lean
-- SameMeaning edges form an equivalence relation
-- (reflexive, symmetric, transitive) over statement IDs.

def sameMeaningClosure (edges : List (String × String)) : List (String × String) :=
  -- transitive closure of the symmetric edge set
  sorry

-- Theorems:
-- 1. Symmetry: if (a,b) is in closure, (b,a) is too
-- 2. Transitivity: if (a,b) and (b,c) are in closure, (a,c) is too
-- 3. Reflexivity: every node in the edge set is in its own cluster
-- 4. Cluster membership is stable under edge addition
--    (adding edges can only merge clusters, never split them)
-- 5. Self-alignment is rejected (a ≠ b for all edges)
```

#### L1.4 Hypothesis Scoping

A hypothesis context sees its own statements plus all ancestor
contexts. Key invariant: statements in a hypothesis context are NOT
visible from the base scope unless the query explicitly opts in.

```lean
-- Theorems:
-- 1. Non-leakage: a statement asserted in a hypothesis context is NOT
--    visible under any scope that does not include the hypothesis or
--    one of its descendants
-- 2. Isolation: two sibling hypothesis contexts do not see each other's
--    statements (unless a shared ancestor is included)
-- 3. Base visibility: a hypothesis scope with include_ancestors=true
--    sees the full ancestor chain plus its own statements
```

#### L1.5 Bitemporal Canonical Drift

Predicate aliases can resolve to different canonicals at different
valid_time points. The Lean formalization should prove that resolution
is deterministic at any given point.

```lean
structure TemporalAlias where
  alias_iri : IRI
  canonical_iri : IRI
  valid_from : Option String  -- ISO date
  valid_to : Option String

def resolveAt (aliases : List TemporalAlias) (iri : IRI) (asOf : String) : IRI :=
  -- 1. Find narrowest containing interval
  -- 2. Fall back to timeless canonical
  -- 3. Fall back to self (open-world)
  sorry

-- Theorems:
-- 1. Resolution is deterministic: same aliases + same IRI + same date = same result
-- 2. Open-world: an unregistered IRI resolves to itself
-- 3. Timeless fallback: if no temporal alias matches, the timeless canonical_of is used
-- 4. No alias chains: canonical_iri is never itself an alias
```

### Phase L2: Proof-Carrying Shapes

Currently shapes produce `ShapeReport` (a list of violations). Make
them produce evidence witnesses — structured proof objects that a
verifier can check without re-running the shape.

#### L2.1 Shape Evidence Types

```lean
-- A shape evaluation produces a report + a witness.
-- The witness is a structured proof that the report is correct
-- relative to the input statements and a declarative predicate.

inductive ShapeWitness where
  -- For each subject that passed: the single object that satisfied functionality
  | functionalPass (subject : IRI) (theObject : Object) (stmtId : String)
  -- For each violation: the multiple objects found
  | functionalViolation (subject : IRI) (objects : List (Object × String))
  -- Datatype pass: the literal has the right type
  | datatypePass (subject : IRI) (lit : Object) (expectedDt : IRI)
  -- Datatype violation: wrong type
  | datatypeViolation (subject : IRI) (lit : Object) (expectedDt : IRI) (actualDt : IRI)
  -- Generic: custom shape with a JSON evidence blob
  | custom (shapeIri : IRI) (evidence : Lean.Json)

structure WitnessedReport where
  report : ShapeReport
  witnesses : List ShapeWitness
  -- Soundness claim: every violation in the report has a corresponding witness
  hComplete : report.violations.length ≤ witnesses.length
```

#### L2.2 Soundness Theorems

For each stdlib shape, prove that:
1. If the shape reports no violations, every subject has ≤ 1 object
   (for functional) or the correct datatype (for datatype)
2. If the shape reports a violation, the violation is genuine (exists
   in the input)
3. The witness is sufficient to reconstruct the judgment

```lean
-- Example for functional shape:
theorem functional_sound (predicate : IRI) (stmts : List Statement)
    (report : ShapeReport)
    (hEval : report = (StdLib.functional predicate).evaluate stmts)
    (hNoViolations : report.violations = []) :
    ∀ s₁ s₂, s₁ ∈ stmts → s₂ ∈ stmts →
      s₁.predicate = predicate → s₂.predicate = predicate →
      s₁.polarity = .asserted → s₂.polarity = .asserted →
      s₁.subject = s₂.subject → s₁ = s₂ := by
  sorry -- full proof requires structural induction over the evaluator
```

#### L2.3 New Shape Combinators

Shapes listed in the PRD roadmap but not yet implemented:

| Shape | Description | Difficulty |
|-------|-------------|---|
| `RangeShape` | Object value in [min, max] | Medium (needs date/number parsing) |
| `MinCardinality` | At least N objects per subject | Easy |
| `MaxCardinality` | At most N objects per subject | Easy |
| `AcyclicClosure` | No cycles in a transitive predicate | Medium |
| `PathShape` | A path pattern exists between two nodes | Medium |
| `Disjoint` | Two predicates never share a subject | Easy |
| `TemporalShape` | Valid_time constraints (before/after/overlap) | Hard (date arithmetic) |
| `ContradictionShape` | Flag subjects with both asserted and negated | Easy |
| `SupportThresholdShape` | Minimum evidence link count | Easy (needs evidence model) |
| `ExtractorQualityShape` | Extraction run confidence threshold | Easy (needs extraction model) |

### Phase L3: Proof-Carrying Derivation

Currently rules produce `List Statement`. Make them produce proof
trees that Lean can verify.

#### L3.1 Proof Tree Types

```lean
-- A derivation proof tree. Each node records the rule application
-- that produced the derived statement, plus pointers to its inputs.

inductive ProofNode where
  | axiom (stmt : Statement)  -- base fact, not derived
  | step (rule : IRI) (inputs : List ProofNode) (output : Statement)

structure ProofTree where
  root : ProofNode
  -- Every leaf is an axiom (base fact)
  hLeaves : ∀ n, n ∈ leaves root → isAxiom n

-- A derivation result: the derived statements + their proof trees
structure DerivationResult where
  statements : List Statement
  proofs : List ProofTree
  -- One proof per statement
  hMatch : statements.length = proofs.length
```

#### L3.2 Rule Soundness

For each stdlib rule, prove that the proof tree is valid:

```lean
-- Transitive closure: if (a,b) and (b,c) are in inputs,
-- then (a,c) is a valid derivation with a 2-step proof.
theorem transitive_closure_sound (predicate : IRI) (inputs : List Statement)
    (derived : Statement) (proof : ProofTree)
    (hDerived : derived ∈ (transitiveClosure predicate).apply inputs)
    (hProof : proof ∈ (transitiveClosure predicate).prove inputs) :
    validProof inputs proof := by
  sorry
```

#### L3.3 Wire Derivation Through Engine

Currently `Engine.lean` only handles `validate_request`. Add:

```lean
-- In Engine.dispatch:
| "derive_request" =>
    let ruleIri := ...
    let stmtsJson := ...
    match lookupRule ruleIri with
    | none => errorEnvelope s!"unknown rule: {ruleIri}"
    | some rule =>
        let result := rule.derive stmts
        deriveResponseToJson result
```

### Phase L4: Certificate Verifiers in Lean

Replace the Rust stub verifiers with real Lean verifiers that produce
checkable proof objects.

#### L4.1 Current Rust Verifier Status

| Kind | Rust Status | Lean Target |
|------|-------------|---|
| `direct_assertion` | ✅ Checks body.source non-empty | Verify source IRI resolves to a real document |
| `substitution` | ✅ Checks inputs contain substitutes | Verify substitution is semantically valid |
| `transitive_closure` | ✅ Re-walks closure via SQL | Verify proof tree against the transitive axiom |
| `confidence_justification` | ❌ Accepts anything | Verify evidence supports the claimed tier |
| `shape_entailment` | ⚠️ Checks report cache | Verify the shape witness is sound |
| `hypothesis_scoped` | ✅ Checks body.hypothesis non-empty | Verify the hypothesis context actually scopes the claim |
| `replay` | ❌ Only checks rule_iri exists | Re-run the derivation and compare |

#### L4.2 Certificate Proof Objects

```lean
-- A verified certificate carries a proof object that an external
-- tool can check independently of dontosrv.

inductive CertificateProof where
  | directAssertion (source : IRI) (documentExists : Bool)
  | transitiveClosure (proofTree : ProofTree)
  | shapeEntailment (witness : WitnessedReport)
  | hypothesisScoped (hypoCtx : IRI) (scopeContains : Bool)
  | substitution (original : Statement) (substituted : Statement) (rule : IRI)
  | replay (derivation : DerivationResult)
  | confidenceJustification (evidenceLinks : List IRI) (threshold : Confidence)

structure VerifiedCertificate where
  cert : Certificate
  proof : CertificateProof
  verdict : Verdict
  -- The proof is consistent with the verdict
  hConsistent : verdict = .ok ↔ proofValid proof
```

#### L4.3 Signature Binding

Certificates should bind not just the statement but the exact scope,
input hashes, and rule/shape versions used.

```lean
structure CertificateBinding where
  statementId : String
  scopeHash : String        -- SHA256 of the scope JSON
  inputHashes : List String -- SHA256 of each input statement
  ruleVersion : Option String
  shapeVersion : Option String
  timestamp : String        -- ISO datetime
```

### Phase L5: Evidence Substrate Formalization

Formalize the new evidence substrate tables (0023-0033) so Lean can
reason about evidence chains, extraction provenance, and proof
obligations.

#### L5.1 Evidence Chain Types

```lean
-- Mirror the SQL evidence_link types
inductive EvidenceLinkType where
  | extractedFrom | supportedBy | contradictedBy
  | derivedFrom | citedIn | anchoredAt | producedBy

-- An evidence chain from a statement back to its source material
inductive EvidenceNode where
  | statement (id : String)
  | span (id : String) (revision : String) (start : Nat) (stop : Nat)
  | document (id : String) (iri : IRI)
  | extractionRun (id : String) (model : String)
  | annotation (id : String) (feature : String) (value : String)

structure EvidenceChain where
  target : String  -- statement_id
  links : List (EvidenceLinkType × EvidenceNode)

-- Theorems:
-- 1. A fully-evidenced statement has at least one chain ending in a
--    document or span (grounded in source material)
-- 2. Evidence links are additive: adding a link never removes existing ones
-- 3. Retracted evidence links are excluded from current chains but
--    preserved in history
```

#### L5.2 Argumentation Semantics

The argumentation layer (0031_arguments.sql) implements a simplified
abstract argumentation framework. Formalize it.

```lean
inductive ArgumentRelation where
  | supports | rebuts | undercuts
  | endorses | supersedes | qualifies
  | potentiallySame | sameReferent | sameEvent

structure ArgumentGraph where
  nodes : List String  -- statement_ids
  edges : List (String × String × ArgumentRelation × Float)

-- Grounded semantics: a statement is acceptable if it is not
-- defeated by any acceptable attacker.
def acceptable (g : ArgumentGraph) (s : String) : Bool :=
  sorry

-- Theorems:
-- 1. A statement with no attackers is acceptable
-- 2. A statement whose only attacker is itself-attacked is acceptable
--    (reinstatement)
-- 3. The contradiction frontier is exactly the set of statements
--    with at least one undefeated attacker
-- 4. Net pressure = supports - attacks (soundness of the SQL query)
```

#### L5.3 Proof Obligation Lifecycle

```lean
inductive ObligationStatus where
  | open | inProgress | resolved | rejected | deferred

structure ProofObligation where
  id : String
  statementId : Option String
  obligationType : String
  status : ObligationStatus
  assignedAgent : Option String

-- Theorems:
-- 1. A resolved obligation cannot transition back to open
-- 2. An obligation can only be assigned from the open state
-- 3. Resolution requires a resolving agent
-- 4. The set of open obligations is monotonically non-increasing
--    under resolution (obligations can be emitted, but resolved ones
--    don't re-open)
```

#### L5.4 Extraction Provenance

```lean
structure ExtractionRun where
  id : String
  modelId : Option String
  modelVersion : Option String
  sourceRevision : Option String
  status : String  -- running | completed | failed | partial

-- Theorems:
-- 1. A completed run's statement count is non-negative
-- 2. Every annotation with a run_id references a valid run
-- 3. A failed run's outputs are still queryable (they exist as
--    low-maturity statements with proof obligations)
```

### Phase L6: Update IR and Engine

#### L6.1 IR Expansion

Add the 11 new directive variants to `IR.lean` to match the Rust
`dir.rs` expansion:

```lean
  | ingestDocument (iri : String) (mediaType : String) (label : Option String)
  | ingestRevision (documentIri : String) (body : Option String)
  | createSpan (revisionId : String) (spanType : String) (start : Option Nat) (stop : Option Nat)
  | createAnnotation (spanId : String) (spaceIri : String) (feature : String) (value : Option String)
  | startExtraction (modelId : Option String) (sourceRevisionId : Option String)
  | completeExtraction (runId : String) (status : String)
  | linkEvidence (statementId : String) (linkType : String) (target : Lean.Json)
  | registerAgent (iri : String) (agentType : String) (label : Option String)
  | assertArgument (source : String) (target : String) (relation : String) (context : String)
  | emitObligation (statementId : String) (obligationType : String) (context : String)
  | resolveObligation (obligationId : String) (status : String)
```

#### L6.2 Engine Handlers

Wire `derive_request` and `certificate` verification through the
engine dispatch loop.

## 3. Dependencies and Build Order

```
L1.1 Scope Resolution ──────────┐
L1.2 Correction Semantics ──────┤
L1.3 SameMeaning Semantics ─────┤
L1.4 Hypothesis Scoping ────────┼──→ L2 Proof-Carrying Shapes
L1.5 Bitemporal Canonicals ─────┘         │
                                          ├──→ L4 Certificate Verifiers
L3 Proof-Carrying Derivation ─────────────┘         │
                                                    │
L5.1 Evidence Chain Types ──────┐                   │
L5.2 Argumentation Semantics ───┼──→ L6 IR + Engine Update
L5.3 Proof Obligation Lifecycle ┤
L5.4 Extraction Provenance ─────┘
```

**Critical path:** L1 → L2 → L4. The evidence substrate formalization
(L5) can proceed in parallel with L2/L3.

## 4. Concrete Deliverables Per Phase

### L1 (Semantics Foundation)
- [ ] `Donto/Scope.lean` — scope resolution formalization + 6 theorems
- [ ] `Donto/Correction.lean` — correction semantics + 4 theorems
- [ ] `Donto/SameMeaning.lean` — equivalence closure + 5 theorems
- [ ] `Donto/Hypothesis.lean` — scoping + leakage proofs + 3 theorems
- [ ] `Donto/Canonicals.lean` — temporal alias resolution + 4 theorems
- [ ] Update `Theorems.lean` to import and re-export all new theorems

### L2 (Proof-Carrying Shapes)
- [ ] `Donto/ShapeWitness.lean` — witness types
- [ ] Update `Shapes.lean` — each shape produces `WitnessedReport`
- [ ] Soundness theorems for `functional` and `datatype`
- [ ] 10 new shape combinators (RangeShape through ExtractorQualityShape)
- [ ] Update `Engine.lean` — include witnesses in response JSON

### L3 (Proof-Carrying Derivation)
- [ ] `Donto/ProofTree.lean` — proof tree types
- [ ] Update `Rules.lean` — each rule produces `DerivationResult`
- [ ] Soundness theorem for `transitiveClosure`
- [ ] Wire `derive_request` handler in `Engine.lean`

### L4 (Certificate Verifiers)
- [ ] `Donto/CertificateProof.lean` — proof object types
- [ ] `Donto/Verifiers.lean` — all 7 verifiers as proof-producing functions
- [ ] Update `Engine.lean` — certificate verification handler
- [ ] Signature binding types

### L5 (Evidence Substrate)
- [ ] `Donto/Evidence.lean` — evidence chain types + 3 theorems
- [ ] `Donto/Argumentation.lean` — abstract argumentation framework + 4 theorems
- [ ] `Donto/Obligations.lean` — lifecycle formalization + 4 theorems
- [ ] `Donto/Extraction.lean` — provenance types + 3 theorems

### L6 (IR + Engine)
- [ ] Update `IR.lean` — 11 new directives
- [ ] Update `Engine.lean` — derive + certificate + evidence handlers

## 5. Theorem Inventory

Total new theorems across all phases: ~36

| Phase | Count | Key theorems |
|-------|-------|---|
| L1.1 | 6 | exclude-wins-tree, empty-include-all, descendant-monotone, ancestor-monotone, deterministic, no-hypo-leakage |
| L1.2 | 4 | context-inherited, identity-preserved, fresh-id, idempotent-noop |
| L1.3 | 5 | symmetry, transitivity, reflexivity, cluster-stable, no-self-align |
| L1.4 | 3 | non-leakage, sibling-isolation, base-visibility |
| L1.5 | 4 | deterministic, open-world, timeless-fallback, no-chain |
| L2 | 2+ | functional-sound, datatype-sound (per new shape) |
| L3 | 1+ | transitive-closure-sound |
| L5.1 | 3 | grounded, additive, retract-preserves |
| L5.2 | 4 | no-attacker-acceptable, reinstatement, frontier-correct, net-pressure-sound |
| L5.3 | 4 | no-reopen, assign-from-open, resolution-requires-agent, monotone-decrease |
| L5.4 | 3 | non-negative-count, run-fk-valid, failed-still-queryable |

## 6. Risk Assessment

**Lean 4 version stability.** We're on v4.12.0. Lean 4 is still
evolving. Pin the toolchain and don't chase nightly.

**Proof complexity.** Some theorems (transitive closure soundness,
argumentation grounded semantics) require non-trivial induction.
Budget for iteration. The `sorry` placeholder lets us ship the types
and theorem statements first, fill in proofs incrementally.

**Performance.** Lean shape evaluation is single-threaded and
processes the full statement snapshot shipped by dontosrv. For large
scopes (100k+ statements), this may be slow. The Rust builtins are
the escape hatch — they work without Lean and use SQL indexes.

**Scope creep.** The evidence substrate formalization (L5) could
expand indefinitely. Keep it to the types and invariant theorems; don't
try to formalize the full SQL surface in Lean.

**Date arithmetic.** `Temporal.lean` is a stub. Real date comparison
needs a Lean library or a custom implementation. Consider using the
JSON string comparison for ISO dates (lexicographic order works for
ISO 8601) rather than building a full date library.

## 7. Implementation Notes

### Lean idioms to follow

- Use `sorry` for proof placeholders during development. `lake build`
  will warn but succeed. Fill proofs in after the types are stable.
- Keep structures `deriving Repr, BEq, DecidableEq` where possible —
  these are needed for `decide` tactic in proofs.
- Use `abbrev` for type aliases (`abbrev IRI := String`) — this is
  already the pattern in `Core.lean`.
- Avoid `partial` except for genuinely recursive functions (only
  `transitiveClosure` needs it currently).
- Use `namespace` / `end` blocks to organize, matching the Rust
  module structure.

### Testing strategy

- `lake build` is the test. If it succeeds, all theorems are true.
- For engine behavior, the existing dontosrv Rust integration tests
  (`crates/dontosrv/tests/lean_engine.rs`) exercise the stdio protocol.
- Add Lean `#eval` examples for new shapes and rules as smoke tests
  (these run at elaboration time).

### Wire protocol additions

When adding new envelope kinds to the engine, follow the existing
pattern:
1. Add the kind to `Engine.dispatch` match
2. Parse the request JSON
3. Call the appropriate handler
4. Encode the response as JSON
5. Test via the dontosrv integration test that spawns the engine

No changes to `Main.lean` are needed — the line-delimited JSON
protocol is generic.

# donto Lean overlay

This is the Lean side of donto. It compiles to `donto_engine`, a stdio
binary that `dontosrv` (Rust) spawns to handle shape validation, rule
derivation, and certificate verification.

## Status

- `Donto.Core`, `Donto.Predicates`, `Donto.Truth`, `Donto.Temporal`,
  `Donto.IR`, `Donto.Shapes`, `Donto.Rules`, `Donto.Certificate`,
  `Donto.Engine`: type and combinator definitions. Compile-clean against
  `leanprover/lean4:v4.12.0`.
- `Main.lean`: minimal entry point that reads DIR envelopes and acks.
- Standard-library shapes (`Shapes.StdLib.functional`, `.datatype`) and
  rules (`Rules.StdLib.transitiveClosure`) are in place; `dontosrv` ships
  Rust ports of the same logic so the system is end-to-end functional
  without invoking Lean.

## Build (when Lean 4 is installed)

```bash
cd lean
lake build donto_engine
```

The build produces `lean/.lake/build/bin/donto_engine`.

## Phase plan

Phase 5 (this PR) lays down the project skeleton. Phase 6 adds the actual
DIR ↔ Lean handlers and wires `dontosrv` to spawn the engine. Phase 7 adds
the certificate verifiers. None of those phases require Lean to be present
for the rest of donto to function — `dontosrv` falls back to its built-in
shape/rule library and reports `sidecar_unavailable` for `lean://` IRIs.

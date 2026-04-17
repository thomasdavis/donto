## Summary

What this PR changes and why. Cite PRD.md section(s).

## Checklist

- [ ] Tests added / updated. At least one assertion of a PRD invariant.
- [ ] `cargo fmt --check` passes.
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` passes.
- [ ] `cargo test --workspace` passes against a live Postgres 16.
- [ ] If SQL changed: new migration file, no edits to prior migrations,
      `MIGRATIONS` list in `donto-client/src/migrations.rs` updated.
- [ ] If a generated column changed: confirmed every function in the
      expression is `IMMUTABLE`.
- [ ] No `delete from donto_statement` introduced.
- [ ] Sidecar operational contract preserved (PRD §15) — donto remains
      usable when `dontosrv` and the Lean engine are down.

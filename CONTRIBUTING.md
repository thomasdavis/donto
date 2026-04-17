# Contributing to donto

Thanks for your interest. donto is a small, opinionated project; the
[PRD.md](PRD.md) is the source of truth and reviews check against it.

## Before you open an issue

1. Read PRD §3 (design principles) and §2 (the maturity ladder). Most
   "wouldn't it be better if…" questions have an answer there.
2. Search existing issues. We mark won't-fix items clearly.

## Before you open a PR

- One conceptual change per PR.
- A new SQL function goes in a new migration file
  (`sql/migrations/NNNN_*.sql`), never by editing prior ones. Add an
  entry to the embedded `MIGRATIONS` list in
  `crates/donto-client/src/migrations.rs`.
- Tests are required for new behavior. Cover both the happy path and at
  least one PRD-invariant edge case (paraconsistency, bitemporality,
  scope inheritance, idempotency).
- `cargo fmt --check` and `cargo clippy --workspace --all-targets -D warnings`
  must pass.
- All workspace tests must pass against a live Postgres 16:
  `DONTO_TEST_DSN=postgres://donto:donto@127.0.0.1:55432/donto cargo test --workspace`.
- For SQL changes that touch generated columns, prove the expression is
  IMMUTABLE (see [`CLAUDE.md`](CLAUDE.md) → SQL idioms).

## What we won't accept

- Code that silently rejects contradictions. donto is paraconsistent.
- Code that calls `delete from donto_statement`. Use `donto_retract` /
  `donto_correct`.
- Performance optimizations without a measured query that's slow.
- Features outside the PRD. PRD amendment first, code second.
- Breaking the sidecar operational contract (PRD §15): the database
  must stay usable when `dontosrv` and the Lean engine are down.

## Commit messages

Conventional-style is fine but not required. The body matters more than
the subject — explain *why*, not *what*. The diff already shows what.

## Reviews

Maintainers will check:
- PRD alignment (cite section numbers in your description if helpful).
- Whether new SQL is idempotent and safe to re-apply.
- Whether tests assert behavior, not implementation.
- Whether you've added any "shadow features" (config knobs, alternative
  code paths) that aren't in the PRD.

## License

By contributing, you agree to dual-license your work under Apache-2.0
and MIT (the project license).

## Code of conduct

See [`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md). Be kind. Argue ideas, not
people.

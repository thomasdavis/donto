# Security Policy

## Reporting a vulnerability

Please email `security@donto.dev` (or open a GitHub Security Advisory at
https://github.com/thomasdavis/donto/security/advisories/new) rather
than filing a public issue.

We aim to:
- Acknowledge within 72 hours.
- Provide a remediation plan within 14 days.
- Release a fix within 30 days for high-severity issues.

## Scope

In scope:
- The `pg_donto` Postgres extension.
- The `dontosrv` HTTP sidecar.
- The `donto-client`, `donto-query`, `donto-ingest`, `donto-migrate`
  Rust crates.
- The Lean overlay (`lean/`).

Out of scope:
- Vulnerabilities in dependencies that have not yet been patched
  upstream (we'll track and pin once a fix exists).
- Misconfigurations in user deployments (e.g. exposing dontosrv to the
  public internet without authentication; that's a deployment concern
  but please send us a hardening note anyway).

## What we won't reward

donto does not currently have a bug bounty program.

## Disclosure

Once a fix is released and adopters have had reasonable time to upgrade,
we credit reporters by name (with permission) in the changelog and the
release notes.

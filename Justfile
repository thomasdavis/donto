# donto Phase 0 task runner. Use with `just` (https://github.com/casey/just),
# or just read the recipes and copy commands.

dsn := env_var_or_default("DONTO_DSN", "postgres://donto:donto@127.0.0.1:55432/donto")

# Bring up the dev Postgres container.
pg-up:
    ./scripts/pg-up.sh

# Stop the dev Postgres container.
pg-down:
    ./scripts/pg-down.sh

# Apply migrations against $DONTO_DSN.
migrate:
    cargo run -p donto-cli -- --dsn '{{dsn}}' migrate

# Build everything.
build:
    cargo build --workspace --all-targets

# Run all tests (requires running postgres at $DONTO_TEST_DSN).
test:
    DONTO_TEST_DSN='{{dsn}}' cargo test --workspace -- --nocapture

# Lint.
clippy:
    cargo clippy --workspace --all-targets -- -D warnings

# Ingest the bundled fixture.
ingest-fixture:
    cargo run -p donto-cli -- --dsn '{{dsn}}' ingest sql/fixtures/lubm-tiny.nq

# Smoke test: bring up pg, migrate, ingest fixture, query.
smoke: pg-up
    sleep 2
    just migrate
    just ingest-fixture
    cargo run -p donto-cli -- --dsn '{{dsn}}' match --predicate http://example.org/name

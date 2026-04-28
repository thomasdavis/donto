# donto Phase 0 task runner. Use with `just` (https://github.com/casey/just),
# or just read the recipes and copy commands.

dsn := env_var_or_default("DONTO_DSN", "postgres://donto:donto@127.0.0.1:55432/donto")
goroot := env_var_or_default("GOROOT", env_var("HOME") / "go")

export PATH := goroot / "bin" + ":" + env_var("PATH")

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
    cargo run -p donto-cli -- --dsn '{{dsn}}' ingest packages/sql/fixtures/lubm-tiny.nq

# Smoke test: bring up pg, migrate, ingest fixture, query.
smoke: pg-up
    sleep 2
    just migrate
    just ingest-fixture
    cargo run -p donto-cli -- --dsn '{{dsn}}' match --predicate http://example.org/name

# Build and test the pg_donto pgrx extension end-to-end via Docker
# (no host sudo, no host pg-dev headers).
pgrx pg="16":
    ./scripts/pgrx-build.sh {{pg}}

# Build the Lean overlay (requires elan / Lean 4.12).
lean:
    cd packages/lean && lake build

# Migrate the bundled tiny genealogy SQLite into a throwaway donto root.
migrate-genealogy: pg-up
    sqlite3 /tmp/donto_genealogy_demo.sqlite < packages/sql/fixtures/genealogy_seed.sql
    cargo run -p donto-migrate -- --dsn '{{dsn}}' \
        genealogy /tmp/donto_genealogy_demo.sqlite \
        --root ctx:demo/genealogy
    rm -f /tmp/donto_genealogy_demo.sqlite

# Ingest the Brooks exoneration fixture (the donto-faces default demo).
ingest-brooks: pg-up
    docker exec -i donto-pg psql -U donto -d donto -v ON_ERROR_STOP=1 -At \
        < packages/sql/fixtures/exoneration_brooks.sql >/dev/null
    @echo "ingested ex:darnell-brooks"

# Build the docs site.
docs:
    pnpm --filter '@donto/docs' build

# Dev-serve the docs site.
docs-dev:
    pnpm --filter '@donto/docs' dev

# Build the donto TUI.
tui-build:
    cd apps/donto-tui && go build -o ../../target/donto-tui .

# Run the donto TUI.
tui *ARGS:
    cd apps/donto-tui && go run . --dsn '{{dsn}}' {{ARGS}}

# Install LISTEN/NOTIFY trigger for real-time TUI firehose.
tui-triggers:
    docker exec -i donto-pg psql -U donto -d donto -v ON_ERROR_STOP=1 \
        < apps/donto-tui/sql/notify_trigger.sql

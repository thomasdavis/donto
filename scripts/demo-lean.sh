#!/usr/bin/env bash
# Side-by-side demo: same database, two shape engines (Rust built-in vs.
# Lean), live-edited Lean shape that flips its verdict after recompile.
#
# Prereqs:
#   ./scripts/pg-up.sh
#   cd lean && lake build && cd ..
#   cargo build -p dontosrv
#
# Run:
#   ./scripts/demo-lean.sh
set -euo pipefail

DSN="${DONTO_DSN:-postgres://donto:donto@127.0.0.1:55432/donto}"
ENGINE="$(pwd)/lean/.lake/build/bin/donto_engine"
PORT=7878
CTX="ctx:demo/lean/$(date +%s)"
SHAPE_FILE="lean/Donto/Shapes.lean"
LOG=/tmp/donto-demo-srv.log

step() { printf "\n\033[1;36m── %s ──\033[0m\n" "$*"; }
run()  { printf "\033[2m\$ %s\033[0m\n" "$*"; eval "$@"; }

# 1. Migrate.
step "0. Apply migrations"
run "cargo run -p donto-cli --quiet -- --dsn '$DSN' migrate >/dev/null"

# 2. Start dontosrv with --lean-engine on a free port.
step "0. Start dontosrv with --lean-engine"
pkill -f 'target/debug/dontosrv' 2>/dev/null || true
sleep 0.3
run "cargo run -p dontosrv --quiet -- --dsn '$DSN' --lean-engine '$ENGINE' --bind 127.0.0.1:$PORT >$LOG 2>&1 &"
SRV_PID=$!
trap 'kill $SRV_PID 2>/dev/null || true' EXIT
# Wait for /health.
for _ in {1..30}; do
    if curl -sf http://127.0.0.1:$PORT/health >/dev/null; then break; fi
    sleep 0.2
done

# 3. Insert genealogy data: a *5-year* parent/child gap (clearly impossible).
step "1. Insert genealogy data into $CTX"
# Route SQL through the docker postgres container so the demo doesn't
# require a host psql install.
docker exec -i donto-pg psql -U donto -d donto -v ON_ERROR_STOP=1 -At <<SQL
SELECT donto_ensure_context('$CTX', 'source', 'permissive', NULL);

-- A 5-year parent/child gap (impossible) AND a duplicate spouse claim.
SELECT donto_assert('ex:alice', 'ex:parentOf', 'ex:bob',  NULL, '$CTX', 'asserted', 0, NULL, NULL, NULL);
SELECT donto_assert('ex:alice', 'ex:birthYear', NULL, '{"v":1850,"dt":"xsd:integer"}'::jsonb, '$CTX', 'asserted', 0, NULL, NULL, NULL);
SELECT donto_assert('ex:bob',   'ex:birthYear', NULL, '{"v":1855,"dt":"xsd:integer"}'::jsonb, '$CTX', 'asserted', 0, NULL, NULL, NULL);

-- Two spouses for the same person: violates the functional-predicate shape.
SELECT donto_assert('ex:alice', 'ex:spouse', 'ex:bob',   NULL, '$CTX', 'asserted', 0, NULL, NULL, NULL);
SELECT donto_assert('ex:alice', 'ex:spouse', 'ex:carol', NULL, '$CTX', 'asserted', 0, NULL, NULL, NULL);
SQL

# 4. Run the Rust built-in shape over ex:spouse.
step "2. Validate via the RUST built-in (functional predicate)"
run "curl -s -X POST http://127.0.0.1:$PORT/shapes/validate \
        -H 'content-type: application/json' \
        -d '{\"shape_iri\":\"builtin:functional/ex:spouse\",
             \"scope\":{\"include\":[\"$CTX\"]}}' | jq ."

# 5. Run the Lean shape over the same data.
step "3. Validate via the LEAN engine (parentChildAgeGap)"
run "curl -s -X POST http://127.0.0.1:$PORT/shapes/validate \
        -H 'content-type: application/json' \
        -d '{\"shape_iri\":\"lean:builtin/parent-child-age-gap\",
             \"scope\":{\"include\":[\"$CTX\"]}}' | jq ."

# 6. Live-edit the Lean shape: change minimum gap from 12 to 4. Rebuild.
step "4. Edit Lean shape: minimum gap 12 → 4"
sed -i.bak 's/if gap < 12 then/if gap < 4 then/' "$SHAPE_FILE"
sed -i 's/than child {child} ({cYear}); minimum 12/than child {child} ({cYear}); minimum 4/' "$SHAPE_FILE"
grep -n 'if gap <' "$SHAPE_FILE"

step "5. lake build (re-typecheck the proof and recompile the engine)"
run "(cd lean && PATH=\$HOME/.elan/bin:\$PATH lake build) 2>&1 | tail -5"

# 7. Restart dontosrv so it picks up the new engine binary.
step "6. Restart dontosrv with the new engine binary"
kill $SRV_PID 2>/dev/null || true
sleep 0.5
run "cargo run -p dontosrv --quiet -- --dsn '$DSN' --lean-engine '$ENGINE' --bind 127.0.0.1:$PORT >$LOG 2>&1 &"
SRV_PID=$!
for _ in {1..30}; do
    if curl -sf http://127.0.0.1:$PORT/health >/dev/null; then break; fi
    sleep 0.2
done

# 8. Re-run the same Lean validation. Same data; new verdict.
step "7. Re-validate via Lean — verdict has flipped"
run "curl -s -X POST http://127.0.0.1:$PORT/shapes/validate \
        -H 'content-type: application/json' \
        -d '{\"shape_iri\":\"lean:builtin/parent-child-age-gap\",
             \"scope\":{\"include\":[\"$CTX\"]}}' | jq ."

# 9. Restore.
step "8. Restore Lean shape from backup"
mv "$SHAPE_FILE.bak" "$SHAPE_FILE"
run "(cd lean && PATH=\$HOME/.elan/bin:\$PATH lake build) 2>&1 | tail -3"

step "Done"
echo "Demo context: $CTX"
echo "(dontosrv log: $LOG)"

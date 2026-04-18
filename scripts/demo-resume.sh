#!/usr/bin/env bash
# Find Thomas a job, the donto+Lean way.
#
# Ingests his resume + a handful of hypothetical job postings, then asks the
# Lean engine to run the `roleFit` shape against each job, scoped to a union
# of (resume, job). Prints which jobs he fits and which he doesn't, with the
# Lean kernel computing the missing-skills set.
#
# Prereqs:
#   ./scripts/pg-up.sh
#   cd lean && lake build && cd ..
set -euo pipefail

DSN="${DONTO_DSN:-postgres://donto:donto@127.0.0.1:55432/donto}"
ENGINE="$(pwd)/lean/.lake/build/bin/donto_engine"
PORT=7878
LOG=/tmp/donto-resume-srv.log

step() { printf "\n\033[1;36m── %s ──\033[0m\n" "$*"; }

step "0. Apply migrations + ingest resume"
cargo run -p donto-cli --quiet -- --dsn "$DSN" migrate >/dev/null
docker exec -i donto-pg psql -U donto -d donto -v ON_ERROR_STOP=1 -At < sql/fixtures/resume_thomas.sql >/dev/null

step "0. Start dontosrv with --lean-engine"
pkill -9 -f 'target/debug/dontosrv' 2>/dev/null || true
sleep 0.3
cargo run -p dontosrv --quiet -- \
    --dsn "$DSN" --lean-engine "$ENGINE" --bind 127.0.0.1:$PORT \
    >$LOG 2>&1 &
SRV_PID=$!
trap 'kill $SRV_PID 2>/dev/null || true' EXIT
for _ in {1..30}; do
    if curl -sf http://127.0.0.1:$PORT/health >/dev/null; then break; fi
    sleep 0.2
done

# Each job is a separate context. The roleFit shape needs both the resume
# AND the job in scope, so we include both per call.
JOBS=(
    "anthropic-pe   ex:job/anthropic-pe       Anthropic-style Product Engineer (AI)"
    "vercel-de      ex:job/vercel-de          Vercel-style Developer Experience"
    "supabase-pe    ex:job/supabase-pe        Supabase-style Product Engineer"
    "genealogy-startup  ex:job/genealogy-startup  Genealogy / Knowledge Graph Startup"
    "citadel-quant  ex:job/citadel-quant      Citadel-style Quant Developer"
)

for entry in "${JOBS[@]}"; do
    slug=$(echo "$entry" | awk '{print $1}')
    iri=$(echo  "$entry" | awk '{print $2}')
    label=$(echo "$entry" | cut -d' ' -f3-)
    label="${label#"${label%%[![:space:]]*}"}"

    step "Fit: $label  ($iri)"
    body=$(jq -n --arg shape "lean:role/fit/$iri" --arg job_ctx "ctx:job/$slug" '{
        shape_iri: $shape,
        scope: { include: ["ctx:resume/thomas", $job_ctx] }
    }')
    resp=$(curl -s -X POST http://127.0.0.1:$PORT/shapes/validate \
            -H 'content-type: application/json' -d "$body")
    n=$(echo "$resp" | jq -r '.report.violations | length // (.violations | length // 0)')
    if [[ "$n" == "0" ]]; then
        printf "  \033[1;32m✓ FITS\033[0m  zero violations from the Lean kernel.\n"
    else
        printf "  \033[1;33m✗ %d gaps\033[0m\n" "$n"
        echo "$resp" | jq -r '
            (.report.violations // .violations // [])[]
            | "    – " + .reason'
    fi
done

step "Done"
echo "(dontosrv log: $LOG)"

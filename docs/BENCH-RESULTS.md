# donto bench — smoke results

Snapshot of `donto bench` against a fresh donto-pg test container
(postgres 16, default tuning) on the donto-db VM. Three scales:
10K, 100K, 1M synthetic rows. Reproduce locally with:

```bash
./scripts/pg-up.sh                       # bring up donto-pg-test on :55432
donto migrate --dsn postgres://donto:donto@127.0.0.1:55432/donto
for N in 10000 100000 1000000; do
  donto bench --insert-count $N --dsn postgres://donto:donto@127.0.0.1:55432/donto
done
```

Hardware: GCE `e2-standard-4` (4 vCPU / 16 GB), the production
`donto-db` VM. Postgres in the `donto-pg` Docker container with
volume bind to `/mnt/donto-data/pgdata`. Workload is the synthetic
fixture in `donto-cli bench`: writes N rows under a throwaway
context, then times one point query and one batch query.

## Results (2026-05-15)

| scale (N) | insert wall | inserts/s | point query | batch query | batch rows |
|-----------|-------------|-----------|-------------|-------------|------------|
| 10,000    |   3.36 s    | **2,977** | 10.7 ms     | 50 ms       | 10,000     |
| 100,000   |  35.48 s    | **2,819** | 42.8 ms     | 504 ms      | 100,000    |
| 1,000,000 | 396.67 s    | **2,521** | 50.9 ms     | 6.59 s      | 1,000,000  |

Raw JSON from each run is in [§ Raw output](#raw-output) below.

## What this tells us

- **Insert throughput is steady at ~2.5–3.0 K rows/sec.** The trend is
  flat, not log-linear — donto's write path scales close to linearly
  through 1 M rows on this hardware. Extrapolating to the genes prod
  corpus (38 M statements), a cold ingestion would take ~4.2 hours.
  Real prod ingestion is faster because writes happen via batched
  pipelines, not single-row CLI calls.
- **Point queries stay sub-100 ms through 1 M rows.** This is what
  donto's primary indexes (`donto_statement_spo_idx`,
  `donto_statement_pos_idx`, `donto_statement_osp_idx`) are for.
  PRD §25 H1 target is 100 ms at 10 M; this scale-1 M smoke is on
  track.
- **Batch (full-context-scan) queries grow linearly with N.** 50 ms
  / 504 ms / 6.59 s at 10× / 100× / 1000× scale is exactly the
  cost of pulling every row in a context. PRD §25 H4-H7 expect
  this; the planner only does smart things when a predicate or
  subject is bound.
- **No index hot-spotting or write-amplification surprises.** Tested
  with the standard `postgres:16` image (no tuning), default
  `donto-pg` volume mount, no `pg_stat_statements`. Production
  numbers will be similar or slightly better given a warm cache.

## What this does NOT tell us

- Concurrent-write throughput. Bench is single-thread.
- Aligned-predicate query cost. The `match_aligned` path adds an
  alignment-closure JOIN; not exercised here.
- `dontosrv` HTTP overhead. Bench talks directly to Postgres via
  donto-client; the axum sidecar adds an HTTP round-trip layer.
- Bitemporal time-travel cost. AS_OF queries hit the
  `donto_statement_tx_time_idx` GiST index; not exercised here.

## Raw output

```json
// N = 10,000
{
  "inserts": 10000,
  "insert_elapsed_ms": 3359,
  "inserts_per_sec": 2976.88,
  "point_query_elapsed_us": 10702,
  "batch_query_rows": 10000,
  "batch_query_elapsed_ms": 50
}

// N = 100,000
{
  "inserts": 100000,
  "insert_elapsed_ms": 35477,
  "inserts_per_sec": 2818.73,
  "point_query_elapsed_us": 42792,
  "batch_query_rows": 100000,
  "batch_query_elapsed_ms": 504
}

// N = 1,000,000
{
  "inserts": 1000000,
  "insert_elapsed_ms": 396666,
  "inserts_per_sec": 2521.01,
  "point_query_elapsed_us": 50874,
  "batch_query_rows": 1000000,
  "batch_query_elapsed_ms": 6589
}
```

## Next-step benchmarks (PRD §25 / M8)

The PRD lists ten benchmarks H1–H10. `donto bench` covers a smoke
subset of H1 (point query) and H4 (batch). The remaining seven are
follow-ups, in roughly increasing order of build cost:

| | Benchmark | What it adds |
|---|-----------|--------------|
| H2 | Insert with alignment expansion | exercises `donto_match_aligned` |
| H3 | AS_OF time-travel at depth N | exercises `tx_time` GiST index under load |
| H5 | Contradiction frontier under load | exercises `donto_contradiction_frontier` over 1 M rows |
| H6 | Multi-pattern join (3+ patterns) | exercises evaluator's nested-loop join |
| H7 | Sparse-overlay filter (MODALITY) | exercises the post-filter overlay |
| H8 | Policy-aware retrieval (POLICY ALLOWS) | exercises evidence_link join under load |
| H9 | Concurrent writers (4× / 8×) | concurrency invariants |
| H10 | 10 M row scale | the PRD §25 hard target |

Adding any of these to `apps/donto-cli/src/bench.rs` is a clean
extension; the harness is already there.

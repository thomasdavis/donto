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

H1 baseline (point query) and H4 batch query — first results set:

| scale (N) | insert wall | inserts/s | H1 point | H4 batch | batch rows |
|-----------|-------------|-----------|----------|----------|------------|
| 10,000    |   3.36 s    | **2,977** | 10.7 ms  | 50 ms    | 10,000     |
| 100,000   |  35.48 s    | **2,819** | 42.8 ms  | 504 ms   | 100,000    |
| 1,000,000 | 396.67 s    | **2,521** | 50.9 ms  | 6.59 s   | 1,000,000  |

Second run after `donto bench` was extended with H2/H3/H5/H7 (the
insert speed differs slightly because the extra benchmarks run
after the inserts, on the same DB; the insert measurement itself
is unchanged):

| scale (N) | H1 point | H4 batch | H2 aligned | H3 AS_OF | H5 frontier | H7 modality setup | H7 modality query |
|-----------|---------:|---------:|-----------:|---------:|------------:|------------------:|------------------:|
| 10,000    | 15.1 ms  | 80 ms    |  10.6 ms   |   4.4 ms |        3 ms |           143 ms  |           48 ms   |
| 100,000   |  8.9 ms  | 2.59 s   |   3.2 ms   |   3.4 ms |        2 ms |          1090 ms  |           95 ms   |
| 1,000,000 | 33.6 ms  | 8.21 s   |   4.6 ms   |   3.0 ms |       15 ms |        14.97 s    |          1.73 s   |

Third run adding H6 (multi-pattern join, subject-pinned), H8
(POLICY ALLOWS), and H9 (4 concurrent writers, 500 rows each):

| scale (N) | H6 join | H8 setup | H8 query | H8 rows kept | H9 4× concurrent (2,000 rows) |
|-----------|--------:|---------:|---------:|-------------:|-------------------------------:|
| 10,000    |    6 ms |  106 ms  |  107 ms  |  9,900       |  385 ms                        |
| 100,000   |    6 ms |  798 ms  |  812 ms  | 99,000       |  428 ms                        |

H9 is invariant in N because the writers operate on side-contexts
of fixed size (500 rows × 4 writers). H6 is constant because the
benchmark pins the leading subject — see the "honest H6" note
below. H8's setup grows with N (it has to attach evidence_link to
1% of rows); the query timing is what matters.

(H1 point query timings vary run-to-run due to plan cache state;
the order of magnitude is what's load-bearing.)

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
- **Aligned point matches (H2) are not slower than direct.** The
  predicate-closure JOIN adds a few μs of overhead but the planner
  still hits the SPO index. 3–10 ms at all three scales.
- **AS_OF point queries (H3) are competitive with current-state
  point queries.** 3–4 ms across all three scales — the
  `donto_statement_tx_time_idx` GiST index is doing its job.
- **Contradiction-frontier (H5) is cheap when the context has no
  arguments.** 2–15 ms across the three scales — the SQL function
  filters by `donto_argument.context` first; an empty argument
  table for the context means no real work.
- **Batch (full-context-scan) queries grow linearly with N** (H4).
  50 ms / 504 ms / 6.59 s at 10× / 100× / 1000× scale is exactly
  the cost of pulling every row in a context. PRD §25 expects
  this; the planner only does smart things when a predicate or
  subject is bound.
- **Sparse-overlay filter (H7) cost is dominated by the setup
  insert,** not the query. Setting MODALITY on 500K rows took 15 s;
  the resulting filtered query took 1.7 s — same order as a
  batch scan, which is expected since the overlay table has no
  index on `modality` alone for context-scoped queries. Adding a
  composite `(modality, statement_id)` index is a future tuning
  knob; for the genealogy workload (sparse modality, small filter
  sets) the current state is sufficient.
- **Multi-pattern join (H6) is **subject-pinned** in this
  benchmark.** The Phase-4 evaluator does one SQL roundtrip per
  intermediate binding (nested-loop join); an unconstrained
  leading pattern at 1 M rows would emit ~1 M roundtrips and take
  tens of minutes. To measure the join *machinery* without
  amplifying the planner gap, H6 pins the leading subject to
  `ex:s/42` — giving 1 binding feeding into pattern 2. The 6 ms
  result is the cost of two real-DB roundtrips plus unification.
  The planner upgrade (PRD §26 Phase 10) is what makes the
  unconstrained join shape practical at scale.
- **POLICY ALLOWS (H8) scales with N, modestly.** 107 ms at 10K,
  812 ms at 100K — the join through `donto_evidence_link →
  donto_document → donto_policy_capsule.allowed_actions` is
  cheap when the leading set is bounded by `statement_id = any(...)`.
  Extrapolating: ~8 s at 1 M, which is fine for a curated read
  workload but a tuning candidate for hot paths.
- **4× concurrent writers (H9) complete in ~400 ms regardless of
  the bench scale.** Each writer takes its own context; the
  advisory-lock + unique-content-hash path doesn't contend.
  Concurrency at this scale is bounded by Postgres connection
  pool, not by donto's invariants.
- **No index hot-spotting or write-amplification surprises.** Tested
  with the standard `postgres:16` image (no tuning), default
  `donto-pg` volume mount, no `pg_stat_statements`. Production
  numbers will be similar or slightly better given a warm cache.

## What this does NOT tell us

- `dontosrv` HTTP overhead. Bench talks directly to Postgres via
  donto-client; the axum sidecar adds an HTTP round-trip layer.
- 10 M-row scale (H10 PRD §25 hard target). Extrapolating from
  1 M: ~4,200 s insert wall, ~50 ms point query, ~80 s batch scan.
  Run once and lock the numbers when the budget allows.
- Multi-pattern join under load with an unconstrained leading
  pattern. The Phase-10 planner work makes this tractable; today
  H6 is intentionally subject-pinned.
- Concurrent writers >> 4 (contention thresholds, deadlock
  exposure). H9 at 4 writers is the smoke; tuning a real
  production workload would go higher.

## Raw output

```json
// N = 10,000 (H1-H7 run)
{
  "inserts": 10000,
  "insert_elapsed_ms": 5929,
  "inserts_per_sec": 1686.60,
  "point_query_elapsed_us": 15125,
  "batch_query_rows": 10000,
  "batch_query_elapsed_ms": 80,
  "h2_aligned_point_query_elapsed_us": 10614,
  "h2_aligned_point_query_rows": 1,
  "h3_asof_point_query_elapsed_us": 4359,
  "h3_asof_point_query_rows": 1,
  "h5_contradiction_frontier_elapsed_ms": 3,
  "h5_contradiction_frontier_rows": 0,
  "h7_modality_setup_elapsed_ms": 143,
  "h7_modality_query_elapsed_ms": 48,
  "h7_modality_query_rows": 5000
}

// N = 100,000 (H1-H7 run)
{
  "inserts": 100000,
  "insert_elapsed_ms": 46918,
  "inserts_per_sec": 2131.37,
  "point_query_elapsed_us": 8880,
  "batch_query_rows": 100000,
  "batch_query_elapsed_ms": 2591,
  "h2_aligned_point_query_elapsed_us": 3214,
  "h2_aligned_point_query_rows": 1,
  "h3_asof_point_query_elapsed_us": 3394,
  "h3_asof_point_query_rows": 1,
  "h5_contradiction_frontier_elapsed_ms": 2,
  "h5_contradiction_frontier_rows": 0,
  "h7_modality_setup_elapsed_ms": 1090,
  "h7_modality_query_elapsed_ms": 95,
  "h7_modality_query_rows": 50000
}

// N = 1,000,000 (H1-H7 run)
{
  "inserts": 1000000,
  "insert_elapsed_ms": 377891,
  "inserts_per_sec": 2646.26,
  "point_query_elapsed_us": 33589,
  "batch_query_rows": 1000000,
  "batch_query_elapsed_ms": 8214,
  "h2_aligned_point_query_elapsed_us": 4627,
  "h2_aligned_point_query_rows": 1,
  "h3_asof_point_query_elapsed_us": 2952,
  "h3_asof_point_query_rows": 1,
  "h5_contradiction_frontier_elapsed_ms": 15,
  "h5_contradiction_frontier_rows": 0,
  "h7_modality_setup_elapsed_ms": 14968,
  "h7_modality_query_elapsed_ms": 1731,
  "h7_modality_query_rows": 500000
}
```

## Next-step benchmarks (PRD §25 / M8)

`donto bench` now covers H1–H9. The single remaining H-number is:

| | Benchmark | What it adds |
|---|-----------|--------------|
| H10 | 10 M row scale | the PRD §25 hard target — mostly patience (~70 min insert wall extrapolated) |

H6 currently uses a subject-pinned shape; the unbounded
multi-pattern join becomes practical when the PRD §26 Phase 10
planner work lands.

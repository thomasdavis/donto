# Roadmap — what's next on donto

A snapshot of what's *not* done, organised by PRD milestone, with
the smallest credible next step for each. Written 2026-05-15 after
the DontoQL v2 + F-1 push.

For the canonical milestone list, see
[`DONTO-PRD.md` §18](DONTO-PRD.md#18-product-milestones--m0-through-m9).
For the working contract on what to do and not do, see
[`../CLAUDE.md`](../CLAUDE.md).

## What is done (substrate-complete)

These milestones have their SQL substrate landed; gaps remaining
are application-layer or polish.

- **M0 Trust Kernel** — migrations `0111`–`0114` ship policies,
  attestations, audit, capsule semantics. F-1 (`donto_document.policy_id`
  enforcement) closed by `0123_document_policy_id_required.sql`.
  Defence-in-depth at the HTTP layer is still a follow-up.
- **M2 Claim Kernel** — claim records, frames, contexts, modality
  (`0099`) and extraction-level (`0100`) overlays all shipped and
  queryable via DontoQL.
- **M3 Schema and Identity Kernel** — predicate alignment substrate
  (`0050`-ish onwards), `donto_predicate_alignment`, identity
  hypothesis (`0109`), lens-scoped expansion accessible via
  DontoQL `EXPANDS_FROM concept(..) USING schema_lens(..)`.

## What's executable today (DontoQL v2)

Every PRD §11 clause is now end-to-end against existing storage —
see [`DONTOQL.md`](DONTOQL.md) for the per-clause spec. The one
soft spot is **`WITH evidence`** — evidence rows attach to result
output but the result-shape is still a tuple `(Bindings,
Vec<EvidenceRow>)`. A nicer JSON shape (`{ bindings, evidence }`)
would be the next refinement; it's a cosmetic change.

## Big rocks still on the floor

### M5 Extraction Kernel — policy gate landed

`donto extract --policy-check` ships (commit `ef1e7b2`):
pre-flight call to `donto_action_allowed('document', <source>,
'derive_claims')` blocks the OpenRouter call when the source's
policy denies derivation. M5 acceptance bullet 4 satisfied.

Smallest next step: **reviewer acceptance/rejection metrics in
`donto-analytics`**. Schema is in place (`donto_detector_finding`
from migration 0119); needs a metric extractor that computes
reviewer agreement rates per extractor model and emits findings.
~150 LOC + tests.

### M6 Language Pilot — 5/5 importers shipped

All five importers from PRD §M6 are in:

| Crate                       | Format            | Tests |
|-----------------------------|-------------------|-------|
| `donto-ling-cldf`           | CLDF (TSV+JSON-LD)| 5     |
| `donto-ling-ud`             | CoNLL-U           | 6     |
| `donto-ling-unimorph`       | UniMorph TSV      | 3     |
| `donto-ling-lift`           | LIFT XML          | 3     |
| `donto-ling-eaf`            | EAF / ELAN XML    | 3     |

Each crate exposes the same surface: `Importer::new(client, ctx)`,
`Importer::import(path, opts) -> Report`. `ImportOptions` carries
`batch_size`, `strict`, format-specific options. `Report` returns
per-format counts + a `losses: Vec<String>` per PRD I9.

Remaining M6 work:

- **Run importers against real datasets** (not synthetic
  fixtures). Glottolog-CLDF, the English EWT UD treebank,
  UniMorph English paradigm, a small SIL LIFT dictionary, and
  one ELAN annotation file. Goal: verify the loss reports stay
  compact under real-world inputs.
- **18 language-specific frame types** (PRD §13). They're a
  single migration `0124_ling_frames.sql` registering rows in
  `donto_frame_type`. Best authored after the importers reveal
  which frames they actually emit.
- **CLI wiring.** `donto extract` and `donto ingest` already
  exist for generic formats; the linguistic importers would
  benefit from `donto ling cldf <path>` style subcommands.
  ~50 LOC per importer dispatch.

### M7 Release Builder — past skeleton

`d896cfc` lands a skeleton; `donto-release` has a `build_release`
example binary. Missing:

- **Loss report.** Adapter writes (e.g. lossy text → claims)
  should emit a loss-report rowset. Today nothing populates it.
- **RO-Crate export.** PRD §17 lists this; not started.
- **CLDF release export.** Native-format export for a curated
  release; blocked on the CLDF importer landing first (round-trip).

Smallest next step: a `donto release --dry-run` command that
prints the rows it would include and the policies that would
block them, without touching `donto_release_manifest`. ~80 LOC.

### M8 Scale and Calibration — H1-H9 done, H10 remains

`donto bench` now covers **H1, H2, H3, H4, H5, H6, H7, H8, H9**
([BENCH-RESULTS.md](BENCH-RESULTS.md)). The single remaining
H-number:

- **H10 10M row scale.** ~70 min insert wall on this hardware
  (extrapolated). Run once and lock the numbers as the PRD §25
  hard target. No code changes needed — just patience:

  ```bash
  donto bench --insert-count 10000000 \
    --dsn postgres://donto:donto@127.0.0.1:55432/donto \
    > docs/H10-baseline.json
  ```

  Then add the numbers to BENCH-RESULTS.md.

H6 is currently subject-pinned to dodge the Phase-4 evaluator's
nested-loop cost. The Phase-10 planner work is the proper fix;
revisit H6 then.

### M9 Federation Research Spike — decision recorded

Memo landed at [`M9-FEDERATION-MEMO.md`](M9-FEDERATION-MEMO.md).
Decision:
- **Proceed** — signed RO-Crate releases + DataCite-style
  citation metadata.
- **Reject** — live cross-instance SPARQL/DontoQL federation
  (count-channel leak risk; revisit when mitigation funded).
- **Defer** — Solid Pods (deployment-shape, not protocol).

Smallest next step: the 200-LOC `donto release sign` /
`donto release verify` spike outlined at the bottom of the memo.
That delivers the M9 acceptance bullet ("two toy instances
exchange policy-filtered release metadata") without committing
to a full federation stack.

## Other documented gaps

- **F-2 through F-18** in [`REVIEW-FINDINGS.md`](REVIEW-FINDINGS.md):
  all DOC severity. Each is "this is intentional but non-obvious";
  no work required, but worth re-reading before changing the
  related substrate.
- **Stale binaries.** `donto-tui` was refreshed in this session;
  `donto-migrate` is still May-2 and probably obsolete (the
  donto-cli `migrate` subcommand has subsumed its purpose).
  Decide whether to keep or delete on next pass.

## Cross-cutting things to consider

- **`WITH evidence` result-shape clean-up.** The tuple
  `EvalRow(Bindings, Vec<EvidenceRow>)` is wire-compatible but
  reads awkwardly; promoting to a struct with named fields makes
  the JSON output friendlier. Breaking change for any caller that
  destructures `EvalRow(b)`.
- **dontosrv `/dontoql` route.** It serialises EvalRow as JSON;
  the new tuple form ships as `[{...}, []]`. Worth a quick check
  that any debug-UI consumer doesn't break on the empty-Vec
  trailer.
- **Lean overlay parity.** `packages/lean/` has Rust built-ins
  mirroring a Lean standard library; the
  `autoresearch-genealogy/lean/` corpus has a more developed
  `Genealogy/` library. Converging them is a research milestone.
- **CLI manpage / completions.** `donto man` and `donto
  completions` exist but aren't installed by `dep`. Worth a
  one-line patch.

## How to use this file

- When picking up next session, scan `## What is done` for the
  current baseline.
- Pick **one** "smallest next step" from a milestone and
  finish-to-test-and-deploy in a single session. Don't fan out.
- Update this file when you ship something — the section's
  "smallest next step" should advance to the *next* smallest
  step, not be deleted.

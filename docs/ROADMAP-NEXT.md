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

### M5 Extraction Kernel — polish

Recent commits land the multi-aperture exhaustive extraction
(`14da3c6`, vision in `bfc3966`) and the domain-dispatched kernel
(`76ca770`). Per-domain decomposers exist; missing pieces are:

- **Reviewer acceptance/rejection metrics** in
  `donto-analytics`. The schema (`donto_detector_finding`) is there
  from `0119`; the metric extractor isn't.
- **Policy check before external model call.** Today `donto extract`
  unconditionally calls OpenRouter. Wire `donto_action_allowed(...,
  'derive_claims')` for the source before the LLM call.

Smallest next step: a `donto extract --dry-run --policy-check`
flag that prints whether the source is allowed and exits without
hitting OpenRouter. ~50 LOC.

### M6 Language Pilot — start the importers

The hardest milestone left. Needs five importers and 18
language-specific frame-types. Smallest next step:

- **Implement CLDF importer first.** CLDF is the most native
  fit — it's already a table-of-tables design that maps cleanly to
  `(s, p, o, context)` quads. Crate skeleton:
  `packages/donto-ling-cldf` with one workflow: read CLDF
  parameters table → ingest as `donto_predicate_alignment` rows;
  read values table → ingest as `donto_statement` rows.
- Acceptance: ingest the Glottolog small-CLDF dataset (a few
  thousand languages, registry-only) end-to-end, no value rows.
  ~300 LOC + 5 tests.

The other four importers (CoNLL-U, UniMorph, LIFT, EAF) follow
the same shape. Bundle is ~2 days of focused work each.

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

### M8 Scale and Calibration — extend the benchmark

`donto bench` now covers H1, H2, H3, H4, H5, H7
([BENCH-RESULTS.md](BENCH-RESULTS.md)). The four follow-ups:

- **H6 multi-pattern join.** Time a 3-pattern join (`?a knows ?b,
  ?b name ?n, ?b age ?g`) at 10K / 100K / 1M. The evaluator does a
  nested-loop join; this exercises that path.
- **H8 policy-aware retrieval.** Insert N statements with
  evidence_link rows, register a policy, time POLICY ALLOWS
  filtering. Set up cost is real (one document/policy per N
  rows); the query timing is the interesting half.
- **H9 concurrent writers.** Two/four/eight parallel batch
  asserters under the same context. Validates the advisory-lock /
  unique-content-hash path under contention.
- **H10 10 M row scale.** ~70 min insert wall on this hardware
  (extrapolated). Run once and lock the numbers as the PRD §25
  hard target.

Each is a clean extension of `apps/donto-cli/src/main.rs::bench`.
H6 is the cheapest to start (~50 LOC, no new infrastructure).

### M9 Federation Research Spike

PRD §18 marks this as a research/decision phase, not an
implementation. Smallest next step: a `docs/M9-FEDERATION-MEMO.md`
that compares Verifiable Credentials, DID, Solid Pods, SPARQL
Federation, and DataCite-style metadata exchange against donto's
attestation + release-manifest model. ~1,000 words; no code.

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

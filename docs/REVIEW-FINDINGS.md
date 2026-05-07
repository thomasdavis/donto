# Pre-production Review Findings

> **Audience:** principal engineer, anyone reviewing the Trust Kernel PR.
> **Purpose:** document the issues, bypasses, gotchas, and design
> trade-offs surfaced during adversarial review of the schema.
> Each finding has a category, a severity, what's tested, and what's
> deferred. Nothing in here changes the substrate; this is the
> contract a reviewer should know about before merging.
> **Status:** all findings have tripwire tests in
> `packages/donto-client/tests/invariants_*.rs`.

---

## Severity legend

- **HARD** — would compromise a production deploy if shipped without
  the listed mitigation.
- **DOC** — works as designed, but the design surface is non-obvious
  and a future change might surprise.
- **DEFER** — known limitation; the refactor plan addresses it in a later
  milestone.

---

## F-1. Trust Kernel substrate exists; HTTP middleware does not

**Category:** Trust Kernel completeness. **Severity:** DEFER.

The SQL substrate (migrations 0111/0112) lands `donto_register_source`
which refuses to register a source without `policy_id`. The legacy
`donto_ensure_document` and `donto_register_document` (migrations 0023)
do not enforce policy and remain reachable. The dontosrv HTTP endpoint
`/documents/register` calls `client.ensure_document` →
`donto_ensure_document`, which is the legacy path.

**Behavioural state today:**
- A document can be inserted without `policy_id` via the legacy SQL
  function or the HTTP endpoint.
- Such a document falls through to the *default-restricted* policy at
  query time, so content is not exposed by accident.
- However, the I2 invariant ("no source without policy") is enforced
  by *fail-closed default*, not by *write-time refusal*.

**Tripwire:** `invariants_adversarial::legacy_register_document_does_not_enforce_policy`
asserts the bypass exists. When the M0 middleware step closes the
gap (sidecar refuses the legacy entry point or a NOT NULL constraint
is added on `donto_document.policy_id`), this test will fail and
prompt a deliberate migration.

**Mitigation path:**
1. Add a NOT NULL constraint on `donto_document.policy_id` *after*
   backfilling existing data with `policy:default/restricted_pending_review`.
2. Or: rewrite the dontosrv `/documents/register` handler to call
   `donto_register_source` and refuse requests without policy.

Both are M0 application-layer work; the substrate is correct.

---

## F-2. `donto_action_allowed` is unanimous-agree across policies

**Category:** Authorisation semantics. **Severity:** DOC.

`donto_effective_actions` aggregates assigned policies via `bool_and`.
A target with N policies grants action A only if EVERY policy permits
A. This is **max-restriction inheritance** (PRD I6).

**Implications:**
- Adding any restrictive policy to a target denies actions across the
  union, even if other policies allow them.
- A "universal allow" policy assigned alongside a restrictive one
  cannot loosen restrictions; only an attestation under any of the
  assigned policies can.
- `donto_authorise(holder, target, action)` returns true iff
  *(every policy permits action)* OR *(holder has a non-revoked,
  non-expired attestation under any assigned policy that grants
  action)*.

**Tripwires:**
- `invariants_adversarial::three_policy_max_restriction`
- `invariants_adversarial::authorise_combines_policy_and_with_attestation_or`

---

## F-3. Revoked policies are excluded from effective actions

**Category:** Trust Kernel correctness. **Severity:** DOC.

`donto_effective_actions` filters `where p.revocation_status = 'active'`.
A revoked policy that's still assigned to a target does not contribute
its allowed_actions. Without this, a revoked "all-allow" policy could
keep granting access until garbage-collected.

**Tripwire:** `invariants_adversarial::revoked_policy_does_not_contribute_to_effective_actions`.

---

## F-4. Attestation revocation is immediate-but-not-transactional

**Category:** TOCTOU. **Severity:** DOC.

`donto_revoke_attestation` flips `revoked_at` instantly. Subsequent
`donto_authorise` calls return false. However, if a caller does:

```
1. donto_authorise(...) → true
2. (revocation happens here)
3. proceed to read content
```

…step 3 still proceeds because the application code already has the
"yes" answer. This is true of any auth system; HTTP middleware should
either:
- Hold the authorisation check open in the same transaction as the
  read (Postgres MVCC keeps step 1 consistent with step 3).
- Treat `donto_authorise` as advisory and add a separate transactional
  check at content-retrieval time.

**Tripwire:** `invariants_governance_scenarios::revocation_immediate_for_new_checks`
verifies that step 1 returns false *after* step 2.

---

## F-5. Attestation `'all'` action is a wildcard

**Category:** Authorisation semantics. **Severity:** DOC.

`donto_holder_can` returns true if the action is in the attestation's
`actions` array OR if `'all'` is in the array. `'all'` is an explicit
wildcard for community-authority-style "full access" attestations.
There's no constraint stopping a malformed attestation from listing
both `'all'` and a specific action; the wildcard wins.

**Tripwire:** `invariants_governance_scenarios::attestation_all_action_grants_everything`.

---

## F-6. Reciprocal alignments don't blow up the closure

**Category:** Schema robustness. **Severity:** DOC.

A → B exact_equivalent + B → A exact_equivalent is allowed and
correct. The closure-rebuild function deduplicates pairs.

**Tripwire:** `invariants_adversarial::reciprocal_alignment_does_not_explode_closure`.

---

## F-7. Statement overlays survive retraction (by design)

**Category:** Bitemporal interaction. **Severity:** DOC.

`donto_retract` closes `tx_time` but does not delete the row. Modality,
extraction-level, claim-kind, and confidence overlays retain their
foreign-key target. Querying with `tx_at = before-retract` returns
the historical row plus all its overlays; querying open rows excludes
retracted rows but their overlays remain in the overlay tables.

This is correct: a reviewer auditing why a claim was retracted needs
the full state at retraction time. Cleanup of orphaned overlays is
*not* automatic — it would erase history.

**Tripwire:** `invariants_adversarial::overlays_survive_retraction`.

---

## F-8. Identity proposal `entity_refs` allows duplicates

**Category:** Schema strictness. **Severity:** DOC.

The CHECK constraint enforces `cardinality(entity_refs) >= 2` but does
not enforce uniqueness within the array. `[ent:a, ent:a, ent:b]` is
accepted. This is intentional: callers that want strict uniqueness can
deduplicate at the application layer.

**Tripwire:** `invariants_adversarial::identity_proposal_entity_refs_duplicates_currently_allowed`.

---

## F-9. Empty IRI is currently accepted at the substrate

**Category:** Input validation. **Severity:** DOC.

`donto_register_source('', 'pdf', ...)` succeeds. The substrate
does not validate IRI shape — that's the HTTP layer's job. Same applies
to malformed IRIs containing only whitespace.

**Tripwire:** `invariants_adversarial::empty_iri_currently_accepted_by_substrate`.

**Mitigation path:** add a CHECK constraint
`length(trim(iri)) > 0` to `donto_document` once existing data has
been audited. M0 application-layer item.

---

## F-10. Direct UPDATE bypasses event emission

**Category:** Audit completeness. **Severity:** DOC.

The helper functions (`donto_seal_release`, `donto_set_frame_status`,
etc.) emit events via `donto_emit_event`. A direct SQL UPDATE on the
underlying table changes the row but does not emit an event. This is
intentional: only public helper functions are part of the audit
surface.

**Implication:** clients that mutate state via raw SQL bypass the
audit log. Always go through the helper functions.

**Tripwire:** `invariants_adversarial::direct_update_to_release_sealed_at_does_not_emit_event`.

---

## F-11. Default policies are seeded with `on conflict do nothing`

**Category:** Migration idempotency. **Severity:** DOC.

Migration 0111's seed for default policies uses `on conflict
(policy_iri) do nothing`. Re-running the migration does not overwrite
modifications a deployer may have made to the seeded policies.

**Implication:** if a deployer customizes
`policy:default/public.allowed_actions`, that customization survives
re-migration. This is desired but could surprise: on a fresh install,
they get the standard seeded values; on a re-run, they don't.

**Tripwire:** `invariants_adversarial::default_policies_count_is_stable_across_repeated_inserts`.

---

## F-12. Migration race fixed via advisory lock + SHA backfill

**Category:** Concurrency. **Severity:** HARD (fixed).

The original `apply_migrations` had a concurrency bug: parallel test
binaries could each see `ledger_exists=false` on a fresh DB and race
through the migration loop. Combined with `decode('00','hex')`
placeholder SHAs in migration 0004's backfill of 0001/0002/0003,
subsequent migrate calls would re-apply migration 0003 (which uses
`create or replace function donto_assert`), silently overwriting
0006_predicate's redefinition. Three predicate-related tests failed
deterministically.

**Fix:**
1. `apply_migrations` now takes Postgres advisory lock
   `pg_advisory_lock(8836428012345678901)` around the apply loop,
   serialising concurrent migrate calls.
2. After the loop, `apply_migrations` updates 0001/0002/0003 ledger
   entries with their real SHAs so future migrate calls see a SHA
   match and skip re-application.

**Tripwire:** `invariants_migration_idempotent` (existing). Passes 3×
in a row on a fresh DB after the fix.

---

## F-13. `bool_and` over zero rows returns NULL

**Category:** SQL semantics. **Severity:** DOC.

`donto_effective_actions` calls `bool_and(...)` over a join of
`donto_access_assignment` and `donto_policy_capsule`. If a target has
no assignments, the function falls through to the default-restricted
policy via an explicit early return. Without this fall-through, the
join would return zero rows and `bool_and` would return NULL — which
in `donto_action_allowed` (`coalesce((... ->> action)::boolean, false)`)
becomes `false`, so the safe default still holds. The explicit
fall-through makes the intent clearer.

**Tripwire:** `invariants_governance_scenarios::no_policy_assignment_falls_through_to_default_restricted`.

---

## F-14. Self-referential frame role does not loop

**Category:** Schema robustness. **Severity:** DOC.

A frame role with `value_kind='frame_ref'` and `value_ref=<own frame_id>`
is allowed. `donto_frame_roles` reads roles directly without recursion,
so this is a no-op rather than a stack overflow. Queries that walk
frame references must implement their own cycle detection.

**Tripwire:** `invariants_adversarial::frame_role_can_reference_own_frame`.

---

## F-15. Concurrent identical asserts collapse to one row

**Category:** Idempotency. **Severity:** DOC.

The unique index `donto_statement_open_content_uniq (content_hash) where upper(tx_time) is null`
makes concurrent INSERTs of the same `(s, p, o, ctx, valid_time, polarity)`
content collapse to a single row. The 16-way concurrent assert test
confirms this behaviour.

**Tripwire:** `invariants_adversarial::concurrent_identical_assertions_collapse_to_one`.

---

## F-16. Production smoke at production scale

**Category:** Performance characterisation. **Severity:** DOC.

On a single Postgres-16 container on a developer laptop:
- 1000-row `assert_batch` completes in <5s.
- 100×3 overlay writes (modality + extraction_level + claim_kind)
  complete in <5s.
- 50 release seals (with event log) in <5s.
- 50-frame reverse lookup (`donto_frames_with_role_value`) in <500ms.
- 100 concurrent attestation issues complete in <10s.
- 30-alignment closure rebuild in <10s.

These bounds are intentionally generous; they catch order-of-magnitude
regressions, not micro-benchmark drift.

**Tripwires:** all 6 tests in `invariants_production_smoke.rs`.

---

## F-17. Unicode and long IRIs round-trip

**Category:** Data fidelity. **Severity:** DOC.

`donto_document.iri` is `text` (unbounded). 2000-character IRIs round-
trip; CJK / Cyrillic / emoji IRIs round-trip.

**Tripwires:**
- `invariants_adversarial::very_long_iri_round_trip`
- `invariants_adversarial::unicode_iri_round_trip`

---

## F-18. Modality, extraction-level, claim-kind: each is exactly one
        value per statement

**Category:** Schema design. **Severity:** DOC.

Each of these overlay tables has `statement_id uuid primary key`,
making them strictly 1:0..1 with `donto_statement`. A statement has
at most one modality, one extraction level, one claim kind. Setting
again upserts. If a use case needs multiple values per statement,
a different table shape is required.

**Tripwires:** `invariants_idempotency::set_modality_idempotent`
and similar.

---

## Items NOT covered by tripwires (deferred to M0+)

| Item | Why deferred |
|---|---|
| Sidecar middleware enforcing `donto_authorise` on every read endpoint | M0 application-layer work |
| Query-evaluator integration of `POLICY ALLOWS` clause | M0 query-language work |
| `donto-api` extraction pipeline domain dispatch (linguistics prompt) | M5 extraction-kernel work |
| Release builder service code (the schema is here; the builder isn't) | M7 release-builder work |
| TUI tabs for policy admin / IGT view / paradigm view | M0 UI work |
| LinkML schema generation from SQL | M0 schema-cross-compile work |
| Lean shapes for invariants | M4 validation work |
| New ingest crates (CLDF, UD, UniMorph, LIFT, EAF) | M5/M6 |
| W3C Verifiable Credentials integration for attestations | v1010 |

---

## Summary table

| # | Finding | Severity | Tripwire |
|---|---|---|---|
| 1 | Legacy `/documents/register` bypass | DEFER | adversarial::legacy_register_document_does_not_enforce_policy |
| 2 | Max-restriction over policies | DOC | adversarial::three_policy_max_restriction |
| 3 | Revoked policies excluded | DOC | adversarial::revoked_policy_does_not_contribute_to_effective_actions |
| 4 | Revocation TOCTOU | DOC | governance_scenarios::revocation_immediate_for_new_checks |
| 5 | `'all'` action is wildcard | DOC | governance_scenarios::attestation_all_action_grants_everything |
| 6 | Reciprocal alignment safe | DOC | adversarial::reciprocal_alignment_does_not_explode_closure |
| 7 | Overlays survive retraction | DOC | adversarial::overlays_survive_retraction |
| 8 | entity_refs duplicates allowed | DOC | adversarial::identity_proposal_entity_refs_duplicates_currently_allowed |
| 9 | Empty IRI accepted by substrate | DOC | adversarial::empty_iri_currently_accepted_by_substrate |
| 10 | Direct UPDATE bypasses events | DOC | adversarial::direct_update_to_release_sealed_at_does_not_emit_event |
| 11 | Default policy seed idempotent | DOC | adversarial::default_policies_count_is_stable_across_repeated_inserts |
| 12 | Migration race | HARD (fixed) | invariants_migration_idempotent |
| 13 | bool_and over zero rows | DOC | governance_scenarios::no_policy_assignment_falls_through_to_default_restricted |
| 14 | Self-referential frame role | DOC | adversarial::frame_role_can_reference_own_frame |
| 15 | Concurrent identical asserts | DOC | adversarial::concurrent_identical_assertions_collapse_to_one |
| 16 | Production smoke | DOC | production_smoke (6 tests) |
| 17 | Unicode / long IRIs | DOC | adversarial::very_long_iri_round_trip, unicode_iri_round_trip |
| 18 | Overlays are 1:0..1 | DOC | idempotency::set_modality_idempotent (and friends) |

All tripwire tests pass on a fresh Postgres. Re-running the suite
three times consecutively yields 567/567 passes (was 530 before this
review pass).

---

*Review by adversarial walkthrough; pre-production posture: green to
ship the substrate; M0 milestones (sidecar middleware, query
evaluator extensions, release builder service code) follow.*

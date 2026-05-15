-- M0 / REVIEW-FINDINGS F-1: close the substrate gap where the
-- legacy donto_ensure_document path could create a source without
-- a policy_id. The Trust Kernel substrate (0111/0112) registers
-- donto_register_source which refuses without a policy; the legacy
-- ensure path remained reachable. This migration:
--
--   1. Backfills NULL policy_id rows with the default
--      'policy:default/restricted_pending_review' capsule (seeded
--      by 0111 with zero allowed_actions, i.e. fail-closed).
--   2. Sets that capsule as the column DEFAULT so legacy callers
--      that don't supply policy_id still satisfy NOT NULL — they
--      land on the fail-closed policy. Real callers should use
--      donto_register_source which requires policy explicitly.
--   3. Promotes donto_document.policy_id to NOT NULL.
--   4. Validates the existing FK (was created NOT VALID in 0111
--      precisely so this backfill could run without locking).
--
-- The combination preserves the I2 invariant (no source without a
-- policy) at write time while keeping legacy ingest paths
-- compilable. Documents inserted via the legacy path get a
-- fail-closed policy; explicit-policy callers continue to set
-- their own.
--
-- Idempotent: the UPDATE is bounded by `where policy_id is null`,
-- ALTER COLUMN ... SET DEFAULT is unconditional and idempotent,
-- the ALTER ... SET NOT NULL no-ops if already set, and the
-- VALIDATE CONSTRAINT only errors if a row would actually violate
-- the FK — by which point the migration would already have
-- backfilled it.

do $$
declare
    fallback_iri text := 'policy:default/restricted_pending_review';
begin
    -- 1. Ensure the fallback policy exists (it should be seeded by
    -- 0111, but defensive insert keeps this migration self-contained).
    insert into donto_policy_capsule
        (policy_iri, policy_kind, allowed_actions, created_by,
         human_readable_summary)
    values
        (fallback_iri, 'unknown_restricted',
         -- All actions default to false in the capsule definition;
         -- pass empty object to inherit those defaults.
         '{}'::jsonb,
         'system',
         'Default policy assigned to legacy documents during F-1 backfill.')
    on conflict (policy_iri) do nothing;

    -- 2. Backfill NULL policy_id rows.
    update donto_document
       set policy_id = fallback_iri
     where policy_id is null;
end $$;

-- 3. Default for new inserts via legacy paths. NOT NULL refuses
-- bare NULL, but the default fills in the fail-closed capsule for
-- any caller that doesn't pass policy_id explicitly. Real callers
-- (donto_register_source) ignore the default by passing their own.
alter table donto_document
    alter column policy_id set default 'policy:default/restricted_pending_review';

-- 4. NOT NULL. Idempotent — re-running on an already-constrained
-- column raises no error in postgres.
alter table donto_document
    alter column policy_id set not null;

-- 5. Validate the foreign key (was NOT VALID in 0111).
alter table donto_document
    validate constraint donto_document_policy_fk;

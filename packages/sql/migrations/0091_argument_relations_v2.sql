-- v1000 / I4: extend argument relations to the v1000 nine-kind set.
--
-- The PRD argument relations are:
--   supports | rebuts | undercuts | qualifies | explains
--   | alternative_analysis_of | same_evidence_different_analysis
--   | same_claim_different_schema | supersedes
--
-- Migration 0031 already shipped: supports, rebuts, undercuts,
-- endorses, supersedes, qualifies, potentially_same, same_referent,
-- same_event. We:
--   1. drop the existing CHECK constraint;
--   2. re-add it with the union of v0 and v1000 relations
--      (backwards-compatible — no v0 row becomes invalid);
--   3. add review_state column (overlay-style default)
--      for per-argument review tracking;
--   4. seed a controlled-vocabulary view that names the canonical
--      v1000 set so consumers can filter to it.

alter table donto_argument
    drop constraint if exists donto_argument_relation_check;

-- Re-add the check including both v0 and v1000 relation kinds.
alter table donto_argument
    add constraint donto_argument_relation_check
    check (relation in (
        -- v0 (preserved)
        'supports', 'rebuts', 'undercuts',
        'endorses', 'supersedes', 'qualifies',
        'potentially_same', 'same_referent', 'same_event',
        -- v1000 additions
        'explains',
        'alternative_analysis_of',
        'same_evidence_different_analysis',
        'same_claim_different_schema'
    ));

-- Per-argument review state (separate from the argument lifecycle's tx_time).
alter table donto_argument
    add column if not exists review_state text not null default 'unreviewed'
    check (review_state in (
        'unreviewed', 'triaged', 'accepted', 'rejected', 'qualified', 'deferred'
    ));

create index if not exists donto_argument_review_state_idx
    on donto_argument (review_state) where review_state <> 'unreviewed';

-- Per-argument evidence anchors. The v0 `evidence jsonb` column carried
-- arbitrary blobs; v1000 expects a typed list of evidence-anchor IDs.
-- We add a new column rather than reinterpret the old one.
alter table donto_argument
    add column if not exists evidence_anchor_ids text[] not null default '{}';

create index if not exists donto_argument_anchor_ids_gin
    on donto_argument using gin (evidence_anchor_ids);

-- Canonical v1000 relation list (reference / discovery).
create or replace view donto_v_argument_relation_v1000 as
    select * from (values
        ('supports', 'Source provides evidence for target.'),
        ('rebuts', 'Source contradicts target conclusion.'),
        ('undercuts', 'Source attacks target reasoning.'),
        ('qualifies', 'Source limits or constrains target scope.'),
        ('explains', 'Source provides explanatory mechanism for target.'),
        ('alternative_analysis_of', 'Source proposes a different analysis of the same evidence.'),
        ('same_evidence_different_analysis', 'Source and target use overlapping evidence with incompatible analyses.'),
        ('same_claim_different_schema', 'Source and target encode the same underlying claim under different schemas.'),
        ('supersedes', 'Source replaces target.')
    ) as t(relation, description);

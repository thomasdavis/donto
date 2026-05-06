-- v1000 / §6.8 identity hypothesis v2 — extension to the
-- existing donto_identity_hypothesis (clustering-solution table).
--
-- Migration 0093 added donto_identity_proposal (per-relation proposals).
-- This migration extends the clustering table with v1000 metadata:
--   * link to the proposal that originated the hypothesis (if any)
--   * extended status enum with v1000 values
--   * authority/method tracking
--
-- The existing strict/likely/exploratory rows continue to work.

alter table donto_identity_hypothesis
    add column if not exists method text not null default 'rule'
        check (method in ('human', 'rule', 'model', 'registry_match',
                          'cross_source_evidence', 'mixed'));

alter table donto_identity_hypothesis
    add column if not exists authority text;

alter table donto_identity_hypothesis
    add column if not exists provenance_proposal_id uuid;
        -- references donto_identity_proposal(proposal_id) — soft reference

create index if not exists donto_idhyp_method_idx
    on donto_identity_hypothesis (method);
create index if not exists donto_idhyp_authority_idx
    on donto_identity_hypothesis (authority) where authority is not null;
create index if not exists donto_idhyp_proposal_idx
    on donto_identity_hypothesis (provenance_proposal_id)
    where provenance_proposal_id is not null;

-- Helper: register a clustering hypothesis with v1000 metadata.
create or replace function donto_register_identity_hypothesis_v1000(
    p_name              text,
    p_description       text default null,
    p_threshold_same    double precision default 0.85,
    p_threshold_distinct double precision default 0.05,
    p_method            text default 'rule',
    p_authority         text default null,
    p_provenance_proposal_id uuid default null,
    p_policy_json       jsonb default '{}'::jsonb
) returns bigint
language plpgsql as $$
declare
    v_id bigint;
begin
    insert into donto_identity_hypothesis
        (name, description, threshold_same, threshold_distinct,
         policy_json, method, authority, provenance_proposal_id)
    values
        (p_name, p_description, p_threshold_same, p_threshold_distinct,
         p_policy_json, p_method, p_authority, p_provenance_proposal_id)
    on conflict (name) do update set
        description     = coalesce(excluded.description, donto_identity_hypothesis.description),
        method          = excluded.method,
        authority       = coalesce(excluded.authority, donto_identity_hypothesis.authority)
    returning hypothesis_id into v_id;

    perform donto_emit_event(
        'identity_hypothesis', v_id::text, 'created',
        coalesce(p_authority, 'system'),
        jsonb_build_object('name', p_name, 'method', p_method)
    );
    return v_id;
end;
$$;

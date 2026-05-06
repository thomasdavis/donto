-- v1000 / §6.13 Attestation — Trust Kernel.
--
-- An attestation is proof that a holder agent may perform certain
-- actions under a policy, granted by an issuer agent for a specific
-- purpose, with optional expiry. Revocation is immediate; no
-- grandfathering.

create table if not exists donto_attestation (
    attestation_id      uuid primary key default gen_random_uuid(),
    holder_agent        text not null,
    issuer_agent        text not null,
    policy_iri          text not null references donto_policy_capsule(policy_iri),
    actions             text[] not null check (cardinality(actions) >= 1),
    purpose             text not null check (purpose in (
        'review', 'community_curation', 'private_research',
        'publication', 'model_training', 'audit',
        'extraction', 'federation', 'inspection'
    )),
    issued_at           timestamptz not null default now(),
    expires_at          timestamptz,
    revoked_at          timestamptz,
    revoked_by          text,
    revocation_reason   text,
    credential_ref      text,
        -- VC-compatible credential reference for future federation
    rationale           text not null,
    metadata            jsonb not null default '{}'::jsonb
);

create index if not exists donto_attestation_holder_idx
    on donto_attestation (holder_agent, policy_iri)
    where revoked_at is null;
create index if not exists donto_attestation_issuer_idx
    on donto_attestation (issuer_agent);
create index if not exists donto_attestation_policy_idx
    on donto_attestation (policy_iri);
create index if not exists donto_attestation_expiry_idx
    on donto_attestation (expires_at) where expires_at is not null and revoked_at is null;
create index if not exists donto_attestation_purpose_idx
    on donto_attestation (purpose);

-- Issue an attestation.
create or replace function donto_issue_attestation(
    p_holder       text,
    p_issuer       text,
    p_policy_iri   text,
    p_actions      text[],
    p_purpose      text,
    p_rationale    text,
    p_expires_at   timestamptz default null,
    p_credential_ref text default null
) returns uuid
language plpgsql as $$
declare
    v_id uuid;
begin
    if p_rationale is null or length(trim(p_rationale)) = 0 then
        raise exception 'donto_issue_attestation: rationale required (audit requirement)';
    end if;

    insert into donto_attestation
        (holder_agent, issuer_agent, policy_iri, actions, purpose,
         expires_at, credential_ref, rationale)
    values
        (p_holder, p_issuer, p_policy_iri, p_actions, p_purpose,
         p_expires_at, p_credential_ref, p_rationale)
    returning attestation_id into v_id;

    perform donto_emit_event(
        'attestation', v_id::text, 'created',
        p_issuer,
        jsonb_build_object(
            'holder', p_holder,
            'policy', p_policy_iri,
            'actions', to_jsonb(p_actions),
            'purpose', p_purpose
        )
    );
    return v_id;
end;
$$;

-- Revoke an attestation. Effective immediately for new reads.
create or replace function donto_revoke_attestation(
    p_attestation_id uuid,
    p_revoked_by     text,
    p_reason         text default null
) returns boolean
language plpgsql as $$
declare
    v_n int;
begin
    update donto_attestation
    set revoked_at        = now(),
        revoked_by        = p_revoked_by,
        revocation_reason = p_reason
    where attestation_id = p_attestation_id and revoked_at is null;
    get diagnostics v_n = row_count;

    if v_n > 0 then
        perform donto_emit_event(
            'attestation', p_attestation_id::text, 'revoked',
            p_revoked_by,
            jsonb_build_object('reason', p_reason)
        );
    end if;
    return v_n > 0;
end;
$$;

-- Check whether an attestation is currently valid for a (policy, action)
-- pair. Returns true iff:
--   * not revoked
--   * not expired
--   * action is in the attestation's action list (or 'all')
create or replace function donto_attestation_valid(
    p_attestation_id uuid,
    p_policy_iri     text,
    p_action         text
) returns boolean
language sql stable as $$
    select exists (
        select 1 from donto_attestation
        where attestation_id = p_attestation_id
          and policy_iri = p_policy_iri
          and revoked_at is null
          and (expires_at is null or expires_at > now())
          and (p_action = any(actions) or 'all' = any(actions))
    )
$$;

-- Authorise: does this holder have *some* valid attestation for this
-- policy + action?
create or replace function donto_holder_can(
    p_holder     text,
    p_policy_iri text,
    p_action     text
) returns boolean
language sql stable as $$
    select exists (
        select 1 from donto_attestation
        where holder_agent = p_holder
          and policy_iri = p_policy_iri
          and revoked_at is null
          and (expires_at is null or expires_at > now())
          and (p_action = any(actions) or 'all' = any(actions))
    )
$$;

-- Top-level access check: combines effective-policy actions with the
-- caller's attestations. Use this from sidecar middleware.
create or replace function donto_authorise(
    p_holder     text,
    p_target_kind text,
    p_target_id   text,
    p_action     text
) returns boolean
language plpgsql stable as $$
declare
    v_effective_allowed boolean;
    v_policies text[];
    v_iri text;
begin
    v_effective_allowed := donto_action_allowed(p_target_kind, p_target_id, p_action);
    if v_effective_allowed then
        return true;
    end if;

    -- Action is not allowed by default policy stack. Check whether the
    -- holder has an attestation that grants the action under any of
    -- the policies assigned to this target.
    select array_agg(distinct policy_iri) into v_policies
    from donto_access_assignment
    where target_kind = p_target_kind and target_id = p_target_id;

    if v_policies is null then
        return false;
    end if;

    foreach v_iri in array v_policies loop
        if donto_holder_can(p_holder, v_iri, p_action) then
            return true;
        end if;
    end loop;

    return false;
end;
$$;

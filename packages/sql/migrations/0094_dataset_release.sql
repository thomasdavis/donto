-- v1000 / I10: a release is a reproducible view.
--
-- A release is not an exported file. It is a named query plus a
-- policy report, source manifest, transformation manifest, checksum
-- manifest, and reproducibility contract. Re-running the same query
-- against unchanged source state must yield manifest-identical
-- content (manifest-stable, not byte-stable).

create table if not exists donto_dataset_release (
    release_id              uuid primary key default gen_random_uuid(),
    release_name            text not null,
    release_version         text,
    query_spec              jsonb not null,                  -- DontoQL or saved view
    scope_description       text,
    output_formats          text[] not null default '{donto-jsonl}'
                            check (array_length(output_formats, 1) >= 1),
    policy_report           jsonb not null default '{}'::jsonb,
    source_manifest         jsonb not null default '[]'::jsonb,
    transformation_manifest jsonb not null default '[]'::jsonb,
    loss_report             jsonb not null default '[]'::jsonb,
    checksums               jsonb not null default '{}'::jsonb,
    citation_metadata       jsonb not null default '{}'::jsonb,
    reproducibility_status  text not null default 'reproducible'
                            check (reproducibility_status in (
                                'reproducible',
                                'policy_dependent',
                                'non_reproducible'
                            )),
    visibility              text not null default 'private'
                            check (visibility in (
                                'private', 'restricted', 'public'
                            )),
    created_at              timestamptz not null default now(),
    created_by              text not null default 'system',
    sealed_at               timestamptz,
    metadata                jsonb not null default '{}'::jsonb,
    constraint donto_release_name_version_uniq unique (release_name, release_version)
);

create index if not exists donto_release_name_idx
    on donto_dataset_release (release_name);
create index if not exists donto_release_visibility_idx
    on donto_dataset_release (visibility);
create index if not exists donto_release_created_idx
    on donto_dataset_release (created_at desc);

-- Per-release artifact: a single output file in a single format.
create table if not exists donto_release_artifact (
    artifact_id     uuid primary key default gen_random_uuid(),
    release_id      uuid not null references donto_dataset_release(release_id) on delete cascade,
    format          text not null,
    storage_uri     text not null,
    byte_size       bigint,
    sha256          bytea,
    record_count    bigint,
    redaction_summary jsonb not null default '{}'::jsonb,
    created_at      timestamptz not null default now(),
    constraint donto_release_artifact_uniq unique (release_id, format)
);

create index if not exists donto_release_artifact_release_idx
    on donto_release_artifact (release_id);

-- Seal a release: closes its mutable window and freezes the manifest.
create or replace function donto_seal_release(
    p_release_id uuid,
    p_actor      text default 'system'
) returns boolean
language plpgsql as $$
declare
    v_n int;
begin
    update donto_dataset_release
    set sealed_at = coalesce(sealed_at, now())
    where release_id = p_release_id and sealed_at is null;
    get diagnostics v_n = row_count;

    if v_n > 0 then
        perform donto_emit_event(
            'release', p_release_id::text, 'created', p_actor,
            jsonb_build_object('action', 'seal')
        );
    end if;
    return v_n > 0;
end;
$$;

-- Lookup helper.
create or replace function donto_release_summary(p_release_id uuid)
returns table(
    release_id uuid, release_name text, release_version text,
    visibility text, reproducibility_status text,
    sealed_at timestamptz, artifact_count bigint
)
language sql stable as $$
    select r.release_id, r.release_name, r.release_version,
           r.visibility, r.reproducibility_status, r.sealed_at,
           (select count(*) from donto_release_artifact a
            where a.release_id = r.release_id) as artifact_count
    from donto_dataset_release r
    where r.release_id = p_release_id
$$;

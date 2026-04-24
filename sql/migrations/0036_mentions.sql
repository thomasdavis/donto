-- Evidence substrate: mentions and coreference resolution.
--
-- A mention is a span identified as referring to an entity, event,
-- quantity, or other typed referent. Mentions are the observation layer
-- between raw spans and resolved entities.
--
-- Coreference clusters group mentions that refer to the same thing.
-- Resolution is uncertain — a mention may have candidate IRIs, and a
-- cluster may have a resolved IRI or remain open.

create table if not exists donto_mention (
    mention_id    uuid primary key default gen_random_uuid(),
    span_id       uuid not null references donto_span(span_id),
    mention_type  text not null check (mention_type in (
        'entity', 'event', 'relation', 'attribute',
        'temporal', 'quantity', 'citation', 'custom'
    )),
    entity_iri    text,
    candidate_iris text[],
    confidence    double precision,
    run_id        uuid references donto_extraction_run(run_id),
    metadata      jsonb not null default '{}'::jsonb,
    created_at    timestamptz not null default now()
);

create index if not exists donto_mention_span_idx
    on donto_mention (span_id);
create index if not exists donto_mention_type_idx
    on donto_mention (mention_type);
create index if not exists donto_mention_entity_idx
    on donto_mention (entity_iri) where entity_iri is not null;
create index if not exists donto_mention_run_idx
    on donto_mention (run_id) where run_id is not null;

create table if not exists donto_coref_cluster (
    cluster_id    uuid primary key default gen_random_uuid(),
    revision_id   uuid not null references donto_document_revision(revision_id),
    resolved_iri  text,
    confidence    double precision,
    run_id        uuid references donto_extraction_run(run_id),
    metadata      jsonb not null default '{}'::jsonb,
    created_at    timestamptz not null default now()
);

create index if not exists donto_coref_cluster_rev_idx
    on donto_coref_cluster (revision_id);
create index if not exists donto_coref_cluster_iri_idx
    on donto_coref_cluster (resolved_iri) where resolved_iri is not null;

create table if not exists donto_coref_member (
    cluster_id         uuid not null references donto_coref_cluster(cluster_id),
    mention_id         uuid not null references donto_mention(mention_id),
    is_representative  boolean not null default false,
    primary key (cluster_id, mention_id)
);

create index if not exists donto_coref_member_mention_idx
    on donto_coref_member (mention_id);

-- Create a mention from a span
create or replace function donto_create_mention(
    p_span_id      uuid,
    p_mention_type text,
    p_entity_iri   text default null,
    p_candidates   text[] default null,
    p_confidence   double precision default null,
    p_run_id       uuid default null
) returns uuid
language plpgsql as $$
declare v_id uuid;
begin
    insert into donto_mention
        (span_id, mention_type, entity_iri, candidate_iris, confidence, run_id)
    values (p_span_id, p_mention_type, p_entity_iri, p_candidates,
            p_confidence, p_run_id)
    returning mention_id into v_id;
    return v_id;
end;
$$;

-- Create a coref cluster and add members
create or replace function donto_create_coref_cluster(
    p_revision_id  uuid,
    p_mention_ids  uuid[],
    p_resolved_iri text default null,
    p_confidence   double precision default null,
    p_run_id       uuid default null
) returns uuid
language plpgsql as $$
declare
    v_id uuid;
    v_mid uuid;
    v_first boolean := true;
begin
    insert into donto_coref_cluster
        (revision_id, resolved_iri, confidence, run_id)
    values (p_revision_id, p_resolved_iri, p_confidence, p_run_id)
    returning cluster_id into v_id;

    foreach v_mid in array p_mention_ids loop
        insert into donto_coref_member (cluster_id, mention_id, is_representative)
        values (v_id, v_mid, v_first)
        on conflict do nothing;
        v_first := false;
    end loop;
    return v_id;
end;
$$;

-- All mentions in a revision, optionally filtered by type
create or replace function donto_mentions_in_revision(
    p_revision_id uuid,
    p_mention_type text default null
) returns table(
    mention_id uuid, span_id uuid, mention_type text,
    entity_iri text, confidence double precision
)
language sql stable as $$
    select m.mention_id, m.span_id, m.mention_type,
           m.entity_iri, m.confidence
    from donto_mention m
    join donto_span s on s.span_id = m.span_id
    where s.revision_id = p_revision_id
      and (p_mention_type is null or m.mention_type = p_mention_type)
$$;

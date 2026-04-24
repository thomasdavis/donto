-- Evidence substrate §5: relations between annotations.
--
-- Annotation edges model structural linguistic relations: dependency
-- arcs, coreference links, argument structure, discourse relations,
-- rhetorical zones, etc. They connect annotations within the same
-- space (e.g. two UD tokens linked by a deprel) or across spaces.

create table if not exists donto_annotation_edge (
    edge_id                uuid primary key default gen_random_uuid(),
    source_annotation_id   uuid not null references donto_annotation(annotation_id),
    target_annotation_id   uuid not null references donto_annotation(annotation_id),
    space_id               uuid not null references donto_annotation_space(space_id),
    relation               text not null,
    metadata               jsonb not null default '{}'::jsonb,
    created_at             timestamptz not null default now(),
    constraint donto_annotation_edge_no_self
        check (source_annotation_id <> target_annotation_id)
);

create index if not exists donto_annotation_edge_source_idx
    on donto_annotation_edge (source_annotation_id);
create index if not exists donto_annotation_edge_target_idx
    on donto_annotation_edge (target_annotation_id);
create index if not exists donto_annotation_edge_relation_idx
    on donto_annotation_edge (space_id, relation);

-- Create an edge between two annotations.
create or replace function donto_link_annotations(
    p_source   uuid,
    p_target   uuid,
    p_space_id uuid,
    p_relation text,
    p_metadata jsonb default '{}'::jsonb
) returns uuid
language plpgsql as $$
declare
    v_id uuid;
begin
    if p_source = p_target then
        raise exception 'donto_link_annotations: source and target must differ';
    end if;
    insert into donto_annotation_edge
        (source_annotation_id, target_annotation_id, space_id, relation, metadata)
    values (p_source, p_target, p_space_id, p_relation, p_metadata)
    returning edge_id into v_id;
    return v_id;
end;
$$;

-- Edges from a given annotation (outgoing arcs).
create or replace function donto_edges_from(p_annotation_id uuid)
returns table(
    edge_id uuid, target_annotation_id uuid,
    space_id uuid, relation text
)
language sql stable as $$
    select edge_id, target_annotation_id, space_id, relation
    from donto_annotation_edge
    where source_annotation_id = p_annotation_id
$$;

-- Edges to a given annotation (incoming arcs).
create or replace function donto_edges_to(p_annotation_id uuid)
returns table(
    edge_id uuid, source_annotation_id uuid,
    space_id uuid, relation text
)
language sql stable as $$
    select edge_id, source_annotation_id, space_id, relation
    from donto_annotation_edge
    where target_annotation_id = p_annotation_id
$$;

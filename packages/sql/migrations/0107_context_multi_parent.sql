-- Trust Kernel / §6.6 ContextScope multi-parent support.
--
-- donto_context (migration 0001) has a single `parent` column. v1000
-- contexts can belong to multiple parents (e.g., a hypothesis context
-- inside both a project and a release-view scope). This migration
-- introduces a junction table donto_context_parent for additional
-- parents. The original `parent` column remains the "primary" parent
-- for backwards compatibility.
--
-- Also extends the context kind vocabulary to the set and adds
-- a `created_by` column.

create table if not exists donto_context_parent (
    context        text not null references donto_context(iri) on delete cascade,
    parent_context text not null references donto_context(iri),
    parent_role    text not null default 'inherit'
                   check (parent_role in (
                       'inherit', 'lens', 'governance', 'review', 'release'
                   )),
    added_at       timestamptz not null default now(),
    primary key (context, parent_context, parent_role),
    constraint donto_context_parent_no_self check (context <> parent_context)
);

create index if not exists donto_context_parent_parent_idx
    on donto_context_parent (parent_context);
create index if not exists donto_context_parent_role_idx
    on donto_context_parent (parent_role);

-- Add a (context, parent) edge.
create or replace function donto_add_context_parent(
    p_context        text,
    p_parent_context text,
    p_parent_role    text default 'inherit'
) returns void
language plpgsql as $$
begin
    perform donto_ensure_context(p_context);
    perform donto_ensure_context(p_parent_context);
    insert into donto_context_parent (context, parent_context, parent_role)
    values (p_context, p_parent_context, p_parent_role)
    on conflict do nothing;
end;
$$;

-- Add created_by column to donto_context (sparse; legacy rows null).
alter table donto_context
    add column if not exists created_by text;

-- Extend kind vocabulary. The existing donto_context.kind has no CHECK
-- constraint (it's free text), so we just register the canonical
-- vocabulary as a view for client filtering.
create or replace view donto_v_context_kind_v1000 as
    select * from (values
        ('source',                  'A registered source object.'),
        ('source_version',          'An immutable source version (revision).'),
        ('dataset_release',         'A versioned dataset release.'),
        ('project',                 'A project workspace.'),
        ('hypothesis',              'A scholarly hypothesis context.'),
        ('identity_lens',           'An identity-resolution lens.'),
        ('schema_lens',             'A schema-alignment lens.'),
        ('review_lens',             'A reviewer view scope.'),
        ('community_policy_scope',  'A community/institutional policy scope.'),
        ('language_or_variety',     'A language variety scope.'),
        ('corpus',                  'A corpus.'),
        ('experiment',              'An experimental scope.'),
        ('jurisdiction',            'A legal jurisdiction.'),
        ('clinical_cohort',         'A clinical cohort.'),
        ('historical_period',       'A historical period.'),
        ('user_workspace',          'An individual user workspace.'),
        ('release_view',            'A release-view scope.'),
        -- v0 kinds preserved
        ('derived',                 'A derivation context.'),
        ('snapshot',                'A snapshot context.'),
        ('user',                    'A user context.'),
        ('custom',                  'A custom context kind.')
    ) as t(kind, description);

-- Resolve all parents (transitively) for a context.
create or replace function donto_context_ancestors(
    p_context text,
    p_max_depth int default 16
) returns table(parent text, depth int, parent_role text)
language sql stable as $$
    with recursive walk as (
        select context as cur, parent_context as parent, 1 as depth, parent_role
        from donto_context_parent
        where context = p_context
        union all
        select parent_context as cur, cp.parent_context as parent, w.depth + 1, cp.parent_role
        from donto_context_parent cp
        join walk w on cp.context = w.parent
        where w.depth < p_max_depth
    )
    select parent, depth, parent_role from walk
    union
    select parent, 1, 'inherit' from donto_context
    where iri = p_context and parent is not null
$$;

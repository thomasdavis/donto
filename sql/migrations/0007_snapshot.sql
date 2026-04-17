-- Phase 2: Snapshots (PRD §8). A snapshot is a context plus a frozen set of
-- statement_ids visible at creation time. Reads scoped to the snapshot
-- context return exactly that set, regardless of later retraction.
--
-- We model snapshots as:
--   1. A `donto_context` row of kind=snapshot.
--   2. A `donto_snapshot_member(snapshot_iri, statement_id)` table.
--   3. donto_match_in_snapshot(...) bypasses tx_time and intersects the
--      snapshot members instead.

create table if not exists donto_snapshot (
    iri              text primary key references donto_context(iri) on delete cascade,
    base_scope       jsonb not null,    -- the scope that defined membership
    captured_tx_time timestamptz not null,
    member_count     int not null default 0,
    note             text
);

create table if not exists donto_snapshot_member (
    snapshot_iri  text not null references donto_snapshot(iri) on delete cascade,
    statement_id  uuid not null references donto_statement(statement_id),
    primary key (snapshot_iri, statement_id)
);

create index if not exists donto_snapshot_member_stmt_idx
    on donto_snapshot_member (statement_id);

create or replace function donto_snapshot_create(
    p_iri text, p_base_scope jsonb, p_note text default null
) returns text
language plpgsql as $$
declare
    v_count int;
    v_now timestamptz := clock_timestamp();
begin
    perform donto_ensure_context(p_iri, 'snapshot', 'curated', null);

    insert into donto_snapshot (iri, base_scope, captured_tx_time, note)
    values (p_iri, p_base_scope, v_now, p_note)
    on conflict (iri) do nothing;

    -- Membership = every statement currently visible under base_scope (with
    -- open tx_time). The `default polarity` filter is intentionally absent —
    -- snapshots include every polarity so they can be re-queried under any.
    insert into donto_snapshot_member (snapshot_iri, statement_id)
    select p_iri, s.statement_id
    from donto_statement s
    where upper(s.tx_time) is null
      and s.context in (select context_iri from donto_resolve_scope(p_base_scope))
    on conflict do nothing;

    select count(*) into v_count from donto_snapshot_member where snapshot_iri = p_iri;
    update donto_snapshot set member_count = v_count where iri = p_iri;
    return p_iri;
end;
$$;

create or replace function donto_match_in_snapshot(
    p_snapshot_iri text,
    p_subject text default null,
    p_predicate text default null,
    p_object_iri text default null,
    p_polarity text default 'asserted',
    p_min_maturity int default 0
) returns table(
    statement_id uuid,
    subject text,
    predicate text,
    object_iri text,
    object_lit jsonb,
    context text,
    polarity text,
    maturity int,
    valid_lo date,
    valid_hi date,
    tx_lo timestamptz,
    tx_hi timestamptz
)
language sql stable as $$
    select
        s.statement_id, s.subject, s.predicate, s.object_iri, s.object_lit,
        s.context, donto_polarity(s.flags), donto_maturity(s.flags),
        lower(s.valid_time), upper(s.valid_time),
        lower(s.tx_time), upper(s.tx_time)
    from donto_snapshot_member m
    join donto_statement s on s.statement_id = m.statement_id
    where m.snapshot_iri = p_snapshot_iri
      and (p_subject    is null or s.subject    = p_subject)
      and (p_predicate  is null or s.predicate  = p_predicate)
      and (p_object_iri is null or s.object_iri = p_object_iri)
      and (p_polarity   is null or donto_polarity(s.flags) = p_polarity)
      and donto_maturity(s.flags) >= p_min_maturity
$$;

-- Drop a snapshot. Members cascade.
create or replace function donto_snapshot_drop(p_iri text)
returns boolean language plpgsql as $$
declare
    v_count int;
begin
    delete from donto_snapshot where iri = p_iri;
    get diagnostics v_count = row_count;
    return v_count > 0;
end;
$$;

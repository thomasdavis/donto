-- Alexandria §3.4: retrofit ingest mode (PRD-alexandria-extensions.md).
--
-- A named ingest path for "apply a new predicate to an existing subject, with
-- a backdated valid_time_from". Enforces two invariants that the plain
-- donto_assert doesn't:
--
--   1. valid_time must be explicit — at least one of valid_lo / valid_hi set.
--      Backdating an empty interval is meaningless.
--   2. tx_time is always now(). Transaction-time is audit; backdating it
--      breaks the whole point of bitemporality.
--   3. A retrofit_reason literal is required. It lands on a side-overlay so
--      "why was this retrofitted?" is queryable — auditors, downstream
--      consumers, or the rewrite-yourself path all look here.
--
-- Retrofit statements are ordinary statements otherwise. They still obey
-- paraconsistency, still live under a context, still promote up the maturity
-- ladder normally.

create table if not exists donto_retrofit (
    statement_id    uuid primary key
                    references donto_statement(statement_id) on delete cascade,
    retrofit_reason text not null,
    retrofitted_at  timestamptz not null default now(),
    retrofitted_by  text
);

create index if not exists donto_retrofit_reason_idx
    on donto_retrofit using gin (to_tsvector('english', retrofit_reason));
create index if not exists donto_retrofit_when_idx
    on donto_retrofit (retrofitted_at desc);

create or replace function donto_assert_retrofit(
    p_subject         text,
    p_predicate       text,
    p_object_iri      text,
    p_object_lit      jsonb,
    p_valid_lo        date,
    p_valid_hi        date,
    p_retrofit_reason text,
    p_context         text default 'donto:anonymous',
    p_polarity        text default 'asserted',
    p_maturity        int  default 0,
    p_actor           text default null
) returns uuid
language plpgsql as $$
declare
    v_id uuid;
begin
    if p_retrofit_reason is null or length(trim(p_retrofit_reason)) = 0 then
        raise exception 'donto_assert_retrofit: retrofit_reason is required';
    end if;
    if p_valid_lo is null and p_valid_hi is null then
        raise exception 'donto_assert_retrofit: at least one of valid_lo/valid_hi is required (that is the point of retrofit)';
    end if;

    v_id := donto_assert(
        p_subject    := p_subject,
        p_predicate  := p_predicate,
        p_object_iri := p_object_iri,
        p_object_lit := p_object_lit,
        p_context    := p_context,
        p_polarity   := p_polarity,
        p_maturity   := p_maturity,
        p_valid_lo   := p_valid_lo,
        p_valid_hi   := p_valid_hi,
        p_actor      := p_actor
    );

    -- Attach (or upsert) the retrofit reason. If the same statement is
    -- retrofitted twice the latest reason wins; the audit log keeps history.
    insert into donto_retrofit (statement_id, retrofit_reason, retrofitted_by)
    values (v_id, p_retrofit_reason, p_actor)
    on conflict (statement_id) do update
        set retrofit_reason = excluded.retrofit_reason,
            retrofitted_at  = now(),
            retrofitted_by  = excluded.retrofitted_by;

    insert into donto_audit (actor, action, statement_id, detail)
    values (p_actor, 'retrofit', v_id,
            jsonb_build_object(
                'reason',   p_retrofit_reason,
                'valid_lo', p_valid_lo,
                'valid_hi', p_valid_hi,
                'context',  p_context));

    return v_id;
end;
$$;

-- Convenience: list retrofitted statements with their reason, joined back
-- to the atom so callers don't have to stitch it manually.
create or replace view donto_retrofit_log as
select
    r.statement_id,
    r.retrofit_reason,
    r.retrofitted_at,
    r.retrofitted_by,
    s.subject,
    s.predicate,
    s.object_iri,
    s.object_lit,
    s.context,
    lower(s.valid_time) as valid_lo,
    upper(s.valid_time) as valid_hi,
    lower(s.tx_time)    as tx_lo,
    upper(s.tx_time)    as tx_hi
from donto_retrofit r
join donto_statement s using (statement_id);

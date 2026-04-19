-- Alexandria §3.8: time-binned aggregation over valid_time.
--
-- Pure ergonomics. Bucket a date into a fixed-width interval aligned to a
-- caller-supplied epoch, then aggregate statements into those buckets.
-- Half-open; PRD §3.10 (no hidden ordering) still applies — callers who
-- want ordered output say so with ORDER BY.

-- Floor a date into a bucket start (date). The interval is the bucket
-- width; the epoch anchors the bucket lattice.
--
-- Works for year/month/day/multi-day/multi-year. For day-divisible
-- intervals we compute (d - epoch) / days_in_bucket; for month-based
-- intervals we align on month counts; for year-based on year counts.
create or replace function donto_bucket_date(
    p_date   date,
    p_bucket interval,
    p_epoch  date default '2000-01-01'
) returns date
language plpgsql immutable as $$
declare
    v_months int := extract(year from p_bucket)::int * 12 + extract(month from p_bucket)::int;
    v_days   int := extract(day from p_bucket)::int;
    v_d_months int;
    v_d_days   int;
begin
    if p_date is null then return null; end if;

    if v_months > 0 and v_days = 0 then
        -- Month- or year-aligned buckets.
        v_d_months := (extract(year from p_date) - extract(year from p_epoch))::int * 12
                    + (extract(month from p_date) - extract(month from p_epoch))::int;
        -- Floor-divide toward -infinity so pre-epoch dates bucket correctly.
        v_d_months := v_d_months - ((v_d_months % v_months) + v_months) % v_months;
        return (p_epoch + make_interval(months => v_d_months))::date;
    elsif v_months = 0 and v_days > 0 then
        v_d_days := (p_date - p_epoch)::int;
        v_d_days := v_d_days - ((v_d_days % v_days) + v_days) % v_days;
        return p_epoch + v_d_days;
    else
        raise exception 'donto_bucket_date: interval must be pure months/years OR pure days, got %', p_bucket;
    end if;
end;
$$;

-- Aggregate counts of statements whose valid_time_from falls into each
-- bucket. Optional filters for predicate, subject, and a scope-resolved
-- context set. Only "current belief" (open tx_time) rows are counted.
--
-- Rows without a valid_time_from (unbounded lower) are excluded — a
-- caller asking about temporal drift has no use for timeless rows.
-- Drop any prior interval-typed overload so the ledger re-apply doesn't
-- leave two functions with the same name.
drop function if exists donto_valid_time_buckets(interval, date, text, text, jsonb);

create or replace function donto_valid_time_buckets(
    p_bucket    text,     -- interval as text (e.g. '10 years', '7 days')
    p_epoch     date default '2000-01-01',
    p_predicate text default null,
    p_subject   text default null,
    p_scope     jsonb default null
) returns table(
    bucket_start date,
    bucket_end   date,
    cnt          bigint
)
language plpgsql stable as $$
declare
    v_resolved text[];
    v_interval interval := p_bucket::interval;
begin
    if p_scope is null then
        v_resolved := null;
    else
        select array_agg(context_iri) into v_resolved from donto_resolve_scope(p_scope);
    end if;

    return query
    select
        donto_bucket_date(lower(s.valid_time), v_interval, p_epoch)        as bucket_start,
        (donto_bucket_date(lower(s.valid_time), v_interval, p_epoch) + v_interval)::date as bucket_end,
        count(*)                                                         as cnt
    from donto_statement s
    where upper(s.tx_time) is null
      and lower(s.valid_time) is not null
      and (p_predicate is null or s.predicate = p_predicate)
      and (p_subject   is null or s.subject   = p_subject)
      and (v_resolved  is null or s.context = any(v_resolved))
    group by bucket_start, bucket_end;
    -- No ORDER BY; PRD §3.10.
end;
$$;

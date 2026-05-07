-- Trust Kernel / §7.2 confidence multi-value model.
--
-- Migration 0038 introduced donto_stmt_confidence with a single
-- `confidence` value plus a `confidence_source`. The PRD §7.2 stores
-- four values per statement:
--   machine_confidence, calibrated_confidence, human_confidence,
--   source_reliability_weight
--
-- Approach: extend donto_stmt_confidence with three new nullable
-- columns. The existing `confidence` column stays as the primary
-- machine_confidence. Old callers continue to work; new callers can
-- populate any subset.

alter table donto_stmt_confidence
    add column if not exists calibrated_confidence double precision
        check (calibrated_confidence is null or
               (calibrated_confidence >= 0 and calibrated_confidence <= 1));

alter table donto_stmt_confidence
    add column if not exists human_confidence double precision
        check (human_confidence is null or
               (human_confidence >= 0 and human_confidence <= 1));

alter table donto_stmt_confidence
    add column if not exists source_reliability_weight double precision
        check (source_reliability_weight is null or
               (source_reliability_weight >= 0 and source_reliability_weight <= 1));

alter table donto_stmt_confidence
    add column if not exists confidence_lens text not null default 'machine'
        check (confidence_lens in (
            'machine', 'calibrated', 'human', 'source_weighted', 'multi'
        ));

create index if not exists donto_stmt_confidence_calibrated_idx
    on donto_stmt_confidence (calibrated_confidence)
    where calibrated_confidence is not null;
create index if not exists donto_stmt_confidence_human_idx
    on donto_stmt_confidence (human_confidence)
    where human_confidence is not null;

-- Set or update calibrated confidence (output of the calibration
-- pipeline; not directly user-set).
create or replace function donto_set_calibrated_confidence(
    p_statement_id uuid,
    p_value        double precision
) returns void
language plpgsql as $$
begin
    insert into donto_stmt_confidence (statement_id, confidence, calibrated_confidence)
    values (p_statement_id, p_value, p_value)
    on conflict (statement_id) do update set
        calibrated_confidence = excluded.calibrated_confidence,
        set_at                = now();
end;
$$;

create or replace function donto_set_human_confidence(
    p_statement_id uuid,
    p_value        double precision,
    p_set_by       text default null
) returns void
language plpgsql as $$
begin
    insert into donto_stmt_confidence
        (statement_id, confidence, human_confidence, confidence_source)
    values (p_statement_id, p_value, p_value, 'human')
    on conflict (statement_id) do update set
        human_confidence  = excluded.human_confidence,
        confidence_source = 'human',
        set_at            = now();
end;
$$;

create or replace function donto_set_source_reliability(
    p_statement_id uuid,
    p_value        double precision
) returns void
language plpgsql as $$
begin
    insert into donto_stmt_confidence (statement_id, confidence, source_reliability_weight)
    values (p_statement_id, p_value, p_value)
    on conflict (statement_id) do update set
        source_reliability_weight = excluded.source_reliability_weight,
        set_at                    = now();
end;
$$;

-- Multi-lens confidence resolver. Returns the value selected by
-- the requested lens.
create or replace function donto_confidence_lens(
    p_statement_id uuid,
    p_lens         text default 'machine'
) returns double precision
language sql stable as $$
    select case p_lens
        when 'machine'         then confidence
        when 'calibrated'      then coalesce(calibrated_confidence, confidence)
        when 'human'           then coalesce(human_confidence, confidence)
        when 'source_weighted' then coalesce(source_reliability_weight, confidence)
        when 'multi'           then (
            coalesce(confidence, 0) +
            coalesce(calibrated_confidence, 0) +
            coalesce(human_confidence, 0) +
            coalesce(source_reliability_weight, 0)
        ) / nullif(
            (case when confidence is not null then 1 else 0 end) +
            (case when calibrated_confidence is not null then 1 else 0 end) +
            (case when human_confidence is not null then 1 else 0 end) +
            (case when source_reliability_weight is not null then 1 else 0 end),
            0)
        else confidence
    end
    from donto_stmt_confidence
    where statement_id = p_statement_id
$$;

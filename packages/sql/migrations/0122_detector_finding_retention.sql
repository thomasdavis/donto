-- Retention policy for donto_detector_finding (I4).
--
-- donto_detector_finding_prune(p_keep_days) deletes findings older than
-- p_keep_days and returns the count of rows removed.  The default of 90 days
-- mirrors the standard analytics lookback window.  Scheduling is handled by
-- the mlops runbook; this migration only installs the function.

create or replace function donto_detector_finding_prune(p_keep_days int default 90)
returns bigint language plpgsql as $$
declare v_count bigint;
begin
    delete from donto_detector_finding
    where observed_at < now() - (p_keep_days || ' days')::interval;
    get diagnostics v_count = row_count;
    return v_count;
end;
$$;

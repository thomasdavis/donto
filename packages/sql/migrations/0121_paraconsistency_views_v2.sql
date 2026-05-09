-- Paraconsistency top-K views v2 (I2).
--
-- Adds max_score and avg_score to donto_v_top_contested_predicates, and
-- avg_score to donto_v_top_contested_subjects (which already had peak_score).
-- Created as a new migration rather than editing 0120 in place (sequential
-- migration discipline from CLAUDE.md).
--
-- Postgres CREATE OR REPLACE VIEW only permits appending columns at the end —
-- inserting max_score / avg_score between existing columns is treated as a
-- column rename and rejected (E42P16). Drop and recreate instead. Views are
-- not append-only data; recreating them carries no PRD penalty.

drop view if exists donto_v_top_contested_predicates;
create view donto_v_top_contested_predicates as
    select predicate,
           sum(conflict_score)  as total_score,
           max(conflict_score)  as max_score,
           avg(conflict_score)  as avg_score,
           count(*)             as windows
    from donto_paraconsistency_density
    where distinct_polarities >= 2
    group by predicate;

drop view if exists donto_v_top_contested_subjects;
create view donto_v_top_contested_subjects as
    select subject,
           max(conflict_score)  as peak_score,
           avg(conflict_score)  as avg_score,
           count(*)             as windows
    from donto_paraconsistency_density
    where distinct_polarities >= 2
    group by subject;

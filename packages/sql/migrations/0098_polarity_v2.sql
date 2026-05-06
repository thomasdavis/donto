-- v1000 / §6.4 ClaimRecord polarity v2.
--
-- Migration 0002 packed polarity into bits 0-1 of donto_statement.flags
-- with four values: asserted (0), negated (1), absent (2), unknown (3).
--
-- The PRD §6.4 lists five polarity values:
--   asserted | negated | unknown | absent | conflicting
--
-- "conflicting" is a derived view, not a stored polarity (it's emitted
-- by the contradiction frontier when two polarities collide on the same
-- subject/predicate/object). We add a helper that surfaces it without
-- adding a fifth polarity bit.

-- View: donto_statement with derived "conflicting" polarity per
-- (subject, predicate, object) tuple in a context.
create or replace view donto_v_statement_polarity_v1000 as
    select s.statement_id, s.subject, s.predicate, s.object_iri,
           s.object_lit, s.context,
           donto_polarity(s.flags) as stored_polarity,
           case
               when exists (
                   select 1 from donto_statement s2
                   where s2.subject = s.subject
                     and s2.predicate = s.predicate
                     and (s2.object_iri = s.object_iri
                          or s2.object_lit = s.object_lit)
                     and s2.context = s.context
                     and donto_polarity(s2.flags) <> donto_polarity(s.flags)
                     and upper(s2.tx_time) is null
               ) then 'conflicting'
               else donto_polarity(s.flags)
           end as effective_polarity
    from donto_statement s
    where upper(s.tx_time) is null;

-- Helper: count statements in conflicting state (frontier metric).
create or replace function donto_polarity_conflict_count(
    p_context text default null
) returns bigint
language sql stable as $$
    select count(*) from donto_v_statement_polarity_v1000 v
    where v.effective_polarity = 'conflicting'
      and (p_context is null or v.context = p_context)
$$;

-- Reference: enumerate v1000 polarity values for clients.
create or replace view donto_v_polarity_v1000 as
    select * from (values
        ('asserted',    'Source asserts the claim.'),
        ('negated',     'Source denies the claim.'),
        ('unknown',     'Source explicitly notes uncertainty.'),
        ('absent',      'Source mentions the topic without making a claim.'),
        ('conflicting', 'Derived: two stored polarities collide. View-only.')
    ) as t(polarity, description);

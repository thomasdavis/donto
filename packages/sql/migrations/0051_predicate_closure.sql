-- Materialized predicate closure index.
--
-- Flat table mapping every predicate IRI to every IRI that should match in
-- alignment-aware queries, with the relation type, a swap_direction flag
-- (true for inverse: subject and object are swapped), and confidence.
--
-- The key insight: at query time we want to look up all equivalents of a
-- given predicate in O(1), not walk the alignment graph. With <1000
-- predicates and single-hop dominance the rebuild is trivially fast.
--
-- Rebuild semantics: full TRUNCATE-then-INSERT. Triggered by:
--   - the batch rule registered at the bottom of this migration
--   - the initial build executed at the end of this migration
--   - explicit calls from the Rust client after registering new alignments

create table if not exists donto_predicate_closure (
    predicate_iri   text not null,
    equivalent_iri  text not null,
    relation        text not null,
    swap_direction  boolean not null default false,
    confidence      double precision not null default 1.0,
    primary key (predicate_iri, equivalent_iri)
);

create index if not exists donto_pc_equiv_idx
    on donto_predicate_closure (equivalent_iri);
create index if not exists donto_pc_swap_idx
    on donto_predicate_closure (predicate_iri) where swap_direction;
create index if not exists donto_pc_relation_idx
    on donto_predicate_closure (relation);

-- ---------------------------------------------------------------------------
-- Rebuild function.
-- ---------------------------------------------------------------------------

create or replace function donto_rebuild_predicate_closure()
returns int
language plpgsql as $$
declare
    v_count int;
begin
    delete from donto_predicate_closure;

    -- Self-identity: every active or implicit predicate matches itself.
    insert into donto_predicate_closure
        (predicate_iri, equivalent_iri, relation, swap_direction, confidence)
    select iri, iri, 'self', false, 1.0
    from donto_predicate
    where status in ('active', 'implicit')
    on conflict do nothing;

    -- exact_equivalent: bidirectional, no swap.
    insert into donto_predicate_closure
        (predicate_iri, equivalent_iri, relation, swap_direction, confidence)
    select source_iri, target_iri, 'exact_equivalent', false, confidence
    from donto_predicate_alignment
    where relation = 'exact_equivalent'
      and upper(tx_time) is null
    on conflict do nothing;

    insert into donto_predicate_closure
        (predicate_iri, equivalent_iri, relation, swap_direction, confidence)
    select target_iri, source_iri, 'exact_equivalent', false, confidence
    from donto_predicate_alignment
    where relation = 'exact_equivalent'
      and upper(tx_time) is null
    on conflict do nothing;

    -- inverse_equivalent: bidirectional, WITH swap.
    insert into donto_predicate_closure
        (predicate_iri, equivalent_iri, relation, swap_direction, confidence)
    select source_iri, target_iri, 'inverse_equivalent', true, confidence
    from donto_predicate_alignment
    where relation = 'inverse_equivalent'
      and upper(tx_time) is null
    on conflict do nothing;

    insert into donto_predicate_closure
        (predicate_iri, equivalent_iri, relation, swap_direction, confidence)
    select target_iri, source_iri, 'inverse_equivalent', true, confidence
    from donto_predicate_alignment
    where relation = 'inverse_equivalent'
      and upper(tx_time) is null
    on conflict do nothing;

    -- sub_property_of: upward only (a query for the parent predicate matches
    -- statements asserted with the child predicate, not the other way around).
    insert into donto_predicate_closure
        (predicate_iri, equivalent_iri, relation, swap_direction, confidence)
    select target_iri, source_iri, 'sub_property_of', false, confidence
    from donto_predicate_alignment
    where relation = 'sub_property_of'
      and upper(tx_time) is null
    on conflict do nothing;

    -- close_match: bidirectional, no swap, only above the confidence floor.
    insert into donto_predicate_closure
        (predicate_iri, equivalent_iri, relation, swap_direction, confidence)
    select source_iri, target_iri, 'close_match', false, confidence
    from donto_predicate_alignment
    where relation = 'close_match'
      and upper(tx_time) is null
      and confidence >= 0.8
    on conflict do nothing;

    insert into donto_predicate_closure
        (predicate_iri, equivalent_iri, relation, swap_direction, confidence)
    select target_iri, source_iri, 'close_match', false, confidence
    from donto_predicate_alignment
    where relation = 'close_match'
      and upper(tx_time) is null
      and confidence >= 0.8
    on conflict do nothing;

    -- Legacy compatibility: donto_predicate.canonical_of (also backfilled
    -- into donto_predicate_alignment by 0048; belt and suspenders so nothing
    -- is lost if a deployment skipped that backfill).
    insert into donto_predicate_closure
        (predicate_iri, equivalent_iri, relation, swap_direction, confidence)
    select canonical_of, iri, 'exact_equivalent', false, 1.0
    from donto_predicate
    where canonical_of is not null
      and canonical_of <> iri
    on conflict do nothing;

    insert into donto_predicate_closure
        (predicate_iri, equivalent_iri, relation, swap_direction, confidence)
    select iri, canonical_of, 'exact_equivalent', false, 1.0
    from donto_predicate
    where canonical_of is not null
      and canonical_of <> iri
    on conflict do nothing;

    -- Legacy compatibility: donto_predicate.inverse_of.
    insert into donto_predicate_closure
        (predicate_iri, equivalent_iri, relation, swap_direction, confidence)
    select iri, inverse_of, 'inverse_equivalent', true, 1.0
    from donto_predicate
    where inverse_of is not null
      and inverse_of <> iri
    on conflict do nothing;

    insert into donto_predicate_closure
        (predicate_iri, equivalent_iri, relation, swap_direction, confidence)
    select inverse_of, iri, 'inverse_equivalent', true, 1.0
    from donto_predicate
    where inverse_of is not null
      and inverse_of <> iri
    on conflict do nothing;

    select count(*) into v_count from donto_predicate_closure;
    return v_count;
end;
$$;

-- Initial build.
select donto_rebuild_predicate_closure();

-- Register as a batch rule so the rule engine knows about it.
select donto_register_rule(
    'builtin:predicate_closure',
    'builtin',
    '{"kind":"predicate_closure_rebuild"}'::jsonb,
    'PredicateClosureRebuild',
    'Rebuild the materialized predicate closure index from donto_predicate_alignment.',
    null,
    'batch'
);

-- Trigram index on literal text so /search scales.
-- `object_lit ->> 'v'` is the stringified value of a literal; ILIKE '%foo%'
-- against it sequentially scans donto_statement. With a trigram GIN index,
-- it's sublinear.

create extension if not exists pg_trgm;

-- Partial index — only over literal-bearing rows, only current belief.
create index if not exists donto_statement_object_lit_v_trgm
    on donto_statement
    using gin ((object_lit ->> 'v') gin_trgm_ops)
    where object_lit is not null and upper(tx_time) is null;

-- Label predicates are the common search vector; an additional btree on
-- (predicate) filters out the scan to just label-bearing rows first.
create index if not exists donto_statement_predicate_idx
    on donto_statement (predicate)
    where upper(tx_time) is null;

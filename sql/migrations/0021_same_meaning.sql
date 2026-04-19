-- Alexandria §3.6: parallel-literal alignment.
--
-- SameMeaning asserts that two statements (typically literal-bearing
-- translations / paraphrases of the same claim) express the same meaning.
-- Subject and object are both statement IRIs (RDF-star style); the
-- predicate is symmetric and transitively closable.
--
-- "Asserted" here means *someone said so*, not *it's been proven*. The
-- store is still paraconsistent — disagreement about meaning equivalence
-- is another SameMeaning with a different polarity.

-- Register the canonical predicate and flag it as symmetric so the
-- existing inverse-emission rule machinery can fill in the reverse edge
-- on demand.
insert into donto_predicate (iri, label, description, is_symmetric, status)
values ('donto:SameMeaning', 'SameMeaning',
        'Two statements assert the same meaning (translation, paraphrase, dialect).',
        true, 'active')
on conflict (iri) do update set
    is_symmetric = true,
    status       = case when donto_predicate.status = 'implicit' then 'active'
                        else donto_predicate.status end;

-- Helper: assert a SameMeaning edge between two statements. Emits the
-- symmetric sibling in the same call so consumers don't have to call
-- twice. Re-asserting is idempotent via the usual content-hash path.
create or replace function donto_align_meaning(
    p_stmt_a  uuid,
    p_stmt_b  uuid,
    p_context text default 'donto:anonymous',
    p_actor   text default null
) returns void
language plpgsql as $$
begin
    if p_stmt_a = p_stmt_b then
        raise exception 'donto_align_meaning: a statement cannot align with itself';
    end if;
    if not exists (select 1 from donto_statement where statement_id = p_stmt_a)
       or not exists (select 1 from donto_statement where statement_id = p_stmt_b) then
        raise exception 'donto_align_meaning: both statements must exist';
    end if;

    perform donto_assert(
        p_subject    := donto_stmt_iri(p_stmt_a),
        p_predicate  := 'donto:SameMeaning',
        p_object_iri := donto_stmt_iri(p_stmt_b),
        p_object_lit := null,
        p_context    := p_context,
        p_polarity   := 'asserted',
        p_maturity   := 1,
        p_actor      := p_actor
    );
    perform donto_assert(
        p_subject    := donto_stmt_iri(p_stmt_b),
        p_predicate  := 'donto:SameMeaning',
        p_object_iri := donto_stmt_iri(p_stmt_a),
        p_object_lit := null,
        p_context    := p_context,
        p_polarity   := 'asserted',
        p_maturity   := 1,
        p_actor      := p_actor
    );
end;
$$;

-- Transitive closure: "all statements that share meaning with p_stmt".
-- Traverses asserted SameMeaning edges through the graph; respects an
-- optional scope (only edges asserted in those contexts count).
create or replace function donto_meaning_cluster(
    p_stmt_id uuid,
    p_scope   jsonb default null
) returns table(statement_id uuid)
language plpgsql stable as $$
declare
    v_resolved text[];
begin
    if p_scope is null then
        v_resolved := null;
    else
        select array_agg(context_iri) into v_resolved from donto_resolve_scope(p_scope);
    end if;

    return query
    with recursive walk(statement_id) as (
        select p_stmt_id
        union
        select donto_stmt_iri_to_id(s.object_iri)
        from walk w
        join donto_statement s
          on s.subject = donto_stmt_iri(w.statement_id)
         and s.predicate = 'donto:SameMeaning'
         and upper(s.tx_time) is null
         and donto_polarity(s.flags) = 'asserted'
         and (v_resolved is null or s.context = any(v_resolved))
         and donto_stmt_iri_to_id(s.object_iri) is not null
    )
    select w.statement_id from walk w;
end;
$$;

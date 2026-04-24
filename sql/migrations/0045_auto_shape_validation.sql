-- Auto shape validation: batch-validate typed literals in a context.
--
-- After extraction, call donto_validate_context_datatypes() to attach
-- shape:pass or shape:warn annotations to every statement that has a
-- typed literal with a registered predicate that declares a range_datatype.

-- Validate all statements in a context whose predicate has range_datatype
-- metadata. Attaches shape annotations for pass/warn.
create or replace function donto_validate_context_datatypes(
    p_context text
) returns bigint
language plpgsql as $$
declare
    v_count bigint := 0;
    v_stmt record;
    v_expected text;
    v_actual text;
    v_verdict text;
begin
    for v_stmt in
        select s.statement_id, s.predicate, s.object_lit,
               p.range_datatype
        from donto_statement s
        join donto_predicate p on p.iri = s.predicate
        where s.context = p_context
          and upper(s.tx_time) is null
          and s.object_lit is not null
          and p.range_datatype is not null
    loop
        v_expected := v_stmt.range_datatype;
        v_actual := v_stmt.object_lit ->> 'dt';

        if v_actual = v_expected then
            v_verdict := 'pass';
        else
            v_verdict := 'warn';
        end if;

        perform donto_attach_shape_report(
            v_stmt.statement_id,
            'auto:datatype/' || v_stmt.predicate,
            v_verdict,
            p_context,
            jsonb_build_object('expected', v_expected, 'actual', v_actual)
        );
        v_count := v_count + 1;
    end loop;
    return v_count;
end;
$$;

-- Validate all numeric-looking literals have valid values.
-- Attaches shape:pass if the value parses as a number, shape:warn if not.
create or replace function donto_validate_context_numerics(
    p_context text
) returns bigint
language plpgsql as $$
declare
    v_count bigint := 0;
    v_stmt record;
    v_verdict text;
    v_val text;
begin
    for v_stmt in
        select s.statement_id, s.predicate, s.object_lit
        from donto_statement s
        where s.context = p_context
          and upper(s.tx_time) is null
          and s.object_lit is not null
          and s.object_lit ->> 'dt' in ('xsd:decimal', 'xsd:integer', 'xsd:float')
    loop
        v_val := v_stmt.object_lit ->> 'v';
        begin
            perform v_val::double precision;
            v_verdict := 'pass';
        exception when others then
            v_verdict := 'warn';
        end;

        perform donto_attach_shape_report(
            v_stmt.statement_id,
            'auto:numeric/' || v_stmt.predicate,
            v_verdict,
            p_context,
            jsonb_build_object('value', v_val, 'datatype', v_stmt.object_lit ->> 'dt')
        );
        v_count := v_count + 1;
    end loop;
    return v_count;
end;
$$;

-- Convenience: run all auto-validations on a context
create or replace function donto_auto_validate(p_context text)
returns jsonb
language plpgsql as $$
declare
    v_dt bigint;
    v_num bigint;
begin
    v_dt := donto_validate_context_datatypes(p_context);
    v_num := donto_validate_context_numerics(p_context);
    return jsonb_build_object(
        'datatype_checks', v_dt,
        'numeric_checks', v_num,
        'total', v_dt + v_num
    );
end;
$$;

-- Migration 0118: maturity-change audit trigger.
--
-- Fires an INSERT into donto_audit whenever the maturity bits (bits 2-4)
-- of donto_statement.flags change on UPDATE. This closes the gap noted in
-- the discovery context: donto_correct/donto_assert already write their own
-- audit rows, but a raw UPDATE of flags (e.g., bulk maturity promotion) was
-- invisible to the audit trail.
--
-- Actor resolution via session GUC:
--   The trigger cannot accept parameters. Instead, callers that want their
--   identity recorded must set the session GUC before updating flags:
--
--       SET LOCAL donto.actor = 'agent:human-curator-1';
--       UPDATE donto_statement SET flags = ... WHERE ...;
--
--   If the GUC is not set (or is empty), the actor defaults to 'system'.
--   The GUC is session-scoped; SET LOCAL confines it to the current
--   transaction. SET (without LOCAL) persists for the whole session.
--
-- Idempotent: DROP TRIGGER IF EXISTS + CREATE OR REPLACE FUNCTION.

-- Step 1: trigger function.
create or replace function donto_trg_maturity_audit()
returns trigger
language plpgsql as $$
declare
    v_actor text;
    v_from_stored int;
    v_to_stored   int;
begin
    -- Extract the 3 maturity bits (bits 2-4) from old and new flags.
    v_from_stored := ((old.flags >> 2) & 7)::int;
    v_to_stored   := ((new.flags >> 2) & 7)::int;

    -- Only fire when maturity actually changed.
    if v_from_stored = v_to_stored then
        return new;
    end if;

    -- Read actor from session GUC; fall back to 'system'.
    -- current_setting('donto.actor', true) returns NULL if not set (the
    -- second argument `true` suppresses the "unrecognized configuration
    -- parameter" error).
    v_actor := nullif(trim(coalesce(current_setting('donto.actor', true), '')), '');
    if v_actor is null then
        v_actor := 'system';
    end if;

    insert into donto_audit (actor, action, statement_id, detail)
    values (
        v_actor,
        'mature',
        new.statement_id,
        jsonb_build_object(
            'from_e',   donto_maturity_label(v_from_stored),
            'to_e',     donto_maturity_label(v_to_stored),
            'predicate', new.predicate,
            'context',   new.context
        )
    );

    return new;
end;
$$;

-- Step 2: attach trigger (idempotent).
drop trigger if exists donto_maturity_audit_trg on donto_statement;

create trigger donto_maturity_audit_trg
    after update of flags
    on donto_statement
    for each row
    execute function donto_trg_maturity_audit();

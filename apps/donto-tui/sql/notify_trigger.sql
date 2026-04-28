-- LISTEN/NOTIFY triggers for the donto TUI firehose.
-- Idempotent: safe to run multiple times.
-- Not a migration — this is an operational concern of the TUI.

-- 1. Audit table trigger (fires on assert/retract/correct via donto_assert)
CREATE OR REPLACE FUNCTION donto_audit_notify() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    PERFORM pg_notify('donto_audit', json_build_object(
        'audit_id', NEW.audit_id,
        'at', NEW.at,
        'actor', NEW.actor,
        'action', NEW.action,
        'statement_id', NEW.statement_id,
        'detail', NEW.detail
    )::text);
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS donto_audit_notify_trg ON donto_audit;
CREATE TRIGGER donto_audit_notify_trg
    AFTER INSERT ON donto_audit
    FOR EACH ROW EXECUTE FUNCTION donto_audit_notify();

-- 2. Statement table trigger (fires on ALL inserts, including bulk/COPY paths)
CREATE OR REPLACE FUNCTION donto_statement_notify() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    PERFORM pg_notify('donto_firehose', json_build_object(
        'audit_id', 0,
        'at', now(),
        'actor', coalesce(NEW.context, ''),
        'action', 'assert',
        'statement_id', NEW.statement_id,
        'detail', json_build_object(
            'subject', NEW.subject,
            'predicate', NEW.predicate,
            'object', coalesce(NEW.object_iri, NEW.object_lit::text),
            'context', NEW.context,
            'polarity', donto_polarity(NEW.flags)
        )
    )::text);
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS donto_statement_notify_trg ON donto_statement;
CREATE TRIGGER donto_statement_notify_trg
    AFTER INSERT ON donto_statement
    FOR EACH ROW EXECUTE FUNCTION donto_statement_notify();

-- Entity symbol registry: every subject/object IRI gets a row with provenance.
-- Open-world: symbols are auto-registered on first use.

CREATE TABLE IF NOT EXISTS donto_entity_symbol (
    symbol_id           BIGSERIAL PRIMARY KEY,
    iri                 TEXT NOT NULL UNIQUE,
    iri_hash            BYTEA NOT NULL UNIQUE,
    normalized_label    TEXT,
    symbol_kind_hint    TEXT,  -- person, place, org, event, concept, unknown
    introduced_tx       TIMESTAMPTZ NOT NULL DEFAULT now(),
    introduced_by_run   UUID,
    introduced_by_stmt  UUID,
    source_context      TEXT,
    status              TEXT NOT NULL DEFAULT 'active'
                        CHECK (status IN ('active','merged','deprecated','quarantined')),
    metadata            JSONB NOT NULL DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS donto_entity_symbol_label_trgm
    ON donto_entity_symbol USING gin (normalized_label gin_trgm_ops)
    WHERE normalized_label IS NOT NULL;

CREATE INDEX IF NOT EXISTS donto_entity_symbol_kind
    ON donto_entity_symbol (symbol_kind_hint)
    WHERE symbol_kind_hint IS NOT NULL;

-- Auto-register a symbol, returning existing if already present.
CREATE OR REPLACE FUNCTION donto_ensure_symbol(
    p_iri           TEXT,
    p_kind_hint     TEXT DEFAULT NULL,
    p_label         TEXT DEFAULT NULL,
    p_run_id        UUID DEFAULT NULL,
    p_stmt_id       UUID DEFAULT NULL,
    p_context       TEXT DEFAULT NULL
) RETURNS BIGINT
LANGUAGE plpgsql AS $$
DECLARE
    v_id BIGINT;
    v_hash BYTEA;
BEGIN
    v_hash := sha256(p_iri::bytea);
    SELECT symbol_id INTO v_id FROM donto_entity_symbol WHERE iri_hash = v_hash;
    IF v_id IS NOT NULL THEN RETURN v_id; END IF;
    INSERT INTO donto_entity_symbol (iri, iri_hash, normalized_label, symbol_kind_hint,
                                     introduced_by_run, introduced_by_stmt, source_context)
    VALUES (p_iri, v_hash, p_label, p_kind_hint, p_run_id, p_stmt_id, p_context)
    ON CONFLICT (iri_hash) DO NOTHING
    RETURNING symbol_id INTO v_id;
    IF v_id IS NULL THEN
        SELECT symbol_id INTO v_id FROM donto_entity_symbol WHERE iri_hash = v_hash;
    END IF;
    RETURN v_id;
END;
$$;

-- Resolve symbol by IRI
CREATE OR REPLACE FUNCTION donto_symbol_id(p_iri TEXT) RETURNS BIGINT
LANGUAGE sql STABLE AS $$
    SELECT symbol_id FROM donto_entity_symbol WHERE iri_hash = sha256(p_iri::bytea);
$$;

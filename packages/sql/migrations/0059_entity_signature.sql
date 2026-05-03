-- Entity signature: derived feature profile for a symbol, used for candidate generation.

CREATE TABLE IF NOT EXISTS donto_entity_signature (
    symbol_id              BIGINT PRIMARY KEY REFERENCES donto_entity_symbol(symbol_id),
    type_distribution      JSONB NOT NULL DEFAULT '{}',
    name_features          JSONB NOT NULL DEFAULT '{}',
    temporal_features      JSONB NOT NULL DEFAULT '{}',
    place_features         JSONB NOT NULL DEFAULT '{}',
    kinship_features       JSONB NOT NULL DEFAULT '{}',
    relational_fingerprint JSONB NOT NULL DEFAULT '{}',
    evidence_summary       JSONB NOT NULL DEFAULT '{}',
    statement_count        INTEGER NOT NULL DEFAULT 0,
    signature_hash         BYTEA,
    updated_tx             TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Rebuild signature for a single symbol from current statements.
CREATE OR REPLACE FUNCTION donto_rebuild_entity_signature(p_symbol_id BIGINT)
RETURNS VOID LANGUAGE plpgsql AS $$
DECLARE
    v_iri TEXT;
    v_types JSONB;
    v_names JSONB;
    v_count INTEGER;
BEGIN
    SELECT iri INTO v_iri FROM donto_entity_symbol WHERE symbol_id = p_symbol_id;
    IF v_iri IS NULL THEN RETURN; END IF;

    -- Count statements
    SELECT count(*) INTO v_count
    FROM donto_statement WHERE subject = v_iri AND upper(tx_time) IS NULL;

    -- Collect type distribution
    SELECT coalesce(jsonb_object_agg(object_iri, cnt), '{}') INTO v_types
    FROM (
        SELECT object_iri, count(*) as cnt
        FROM donto_statement
        WHERE subject = v_iri AND predicate = 'rdf:type'
          AND object_iri IS NOT NULL AND upper(tx_time) IS NULL
        GROUP BY object_iri
    ) t;

    -- Collect name features
    SELECT coalesce(jsonb_agg(object_lit ->> 'v'), '[]') INTO v_names
    FROM donto_statement
    WHERE subject = v_iri
      AND predicate IN ('rdfs:label','ex:label','ex:name','name','label','ex:knownAs')
      AND object_lit IS NOT NULL AND upper(tx_time) IS NULL
    LIMIT 20;

    INSERT INTO donto_entity_signature (symbol_id, type_distribution, name_features, statement_count, updated_tx)
    VALUES (p_symbol_id, v_types, jsonb_build_object('labels', v_names), v_count, now())
    ON CONFLICT (symbol_id) DO UPDATE SET
        type_distribution = EXCLUDED.type_distribution,
        name_features = EXCLUDED.name_features,
        statement_count = EXCLUDED.statement_count,
        updated_tx = now();
END;
$$;

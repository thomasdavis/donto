-- Identity edges: weighted, bitemporal assertions about whether two symbols co-refer.

DO $$ BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_type WHERE typname = 'donto_identity_relation') THEN
        CREATE TYPE donto_identity_relation AS ENUM (
            'same_referent',
            'possibly_same_referent',
            'distinct_referent',
            'not_enough_information'
        );
    END IF;
END $$;

CREATE TABLE IF NOT EXISTS donto_identity_edge (
    edge_id             BIGSERIAL PRIMARY KEY,
    left_symbol_id      BIGINT NOT NULL REFERENCES donto_entity_symbol(symbol_id),
    right_symbol_id     BIGINT NOT NULL REFERENCES donto_entity_symbol(symbol_id),
    relation            donto_identity_relation NOT NULL,
    confidence          DOUBLE PRECISION NOT NULL CHECK (confidence >= 0 AND confidence <= 1),
    method              TEXT NOT NULL,  -- trigram, embedding, neural, human, import, rule
    model_version       TEXT,
    evidence_json       JSONB NOT NULL DEFAULT '{}',
    explanation         TEXT,
    context             TEXT,
    valid_time          DATERANGE,
    tx_time             TSTZRANGE NOT NULL DEFAULT tstzrange(now(), NULL, '[)'),
    created_by_agent    TEXT,
    CHECK (left_symbol_id < right_symbol_id)
);

CREATE INDEX IF NOT EXISTS donto_identity_edge_left ON donto_identity_edge (left_symbol_id, relation);
CREATE INDEX IF NOT EXISTS donto_identity_edge_right ON donto_identity_edge (right_symbol_id, relation);
CREATE INDEX IF NOT EXISTS donto_identity_edge_tx ON donto_identity_edge USING GIST (tx_time);
CREATE INDEX IF NOT EXISTS donto_identity_edge_confidence ON donto_identity_edge (confidence DESC)
    WHERE upper(tx_time) IS NULL;

-- Assert an identity edge (ensures left < right ordering)
CREATE OR REPLACE FUNCTION donto_assert_identity(
    p_symbol_a      BIGINT,
    p_symbol_b      BIGINT,
    p_relation      donto_identity_relation,
    p_confidence    DOUBLE PRECISION,
    p_method        TEXT,
    p_explanation   TEXT DEFAULT NULL,
    p_evidence      JSONB DEFAULT '{}',
    p_context       TEXT DEFAULT NULL,
    p_agent         TEXT DEFAULT NULL
) RETURNS BIGINT
LANGUAGE plpgsql AS $$
DECLARE
    v_left BIGINT;
    v_right BIGINT;
    v_id BIGINT;
BEGIN
    IF p_symbol_a = p_symbol_b THEN RETURN NULL; END IF;
    IF p_symbol_a < p_symbol_b THEN
        v_left := p_symbol_a; v_right := p_symbol_b;
    ELSE
        v_left := p_symbol_b; v_right := p_symbol_a;
    END IF;
    INSERT INTO donto_identity_edge
        (left_symbol_id, right_symbol_id, relation, confidence, method,
         explanation, evidence_json, context, created_by_agent)
    VALUES (v_left, v_right, p_relation, p_confidence, p_method,
            p_explanation, p_evidence, p_context, p_agent)
    RETURNING edge_id INTO v_id;
    RETURN v_id;
END;
$$;

-- Retract an identity edge
CREATE OR REPLACE FUNCTION donto_retract_identity_edge(p_edge_id BIGINT) RETURNS BOOLEAN
LANGUAGE plpgsql AS $$
BEGIN
    UPDATE donto_identity_edge
    SET tx_time = tstzrange(lower(tx_time), now(), '[)')
    WHERE edge_id = p_edge_id AND upper(tx_time) IS NULL;
    RETURN FOUND;
END;
$$;

-- Find all identity edges for a symbol
CREATE OR REPLACE FUNCTION donto_identity_edges_for(p_symbol_id BIGINT)
RETURNS TABLE(
    edge_id BIGINT, other_symbol_id BIGINT, other_iri TEXT,
    relation donto_identity_relation, confidence DOUBLE PRECISION,
    method TEXT, explanation TEXT
) LANGUAGE sql STABLE AS $$
    SELECT e.edge_id,
           CASE WHEN e.left_symbol_id = p_symbol_id THEN e.right_symbol_id ELSE e.left_symbol_id END,
           s.iri,
           e.relation, e.confidence, e.method, e.explanation
    FROM donto_identity_edge e
    JOIN donto_entity_symbol s ON s.symbol_id =
        CASE WHEN e.left_symbol_id = p_symbol_id THEN e.right_symbol_id ELSE e.left_symbol_id END
    WHERE (e.left_symbol_id = p_symbol_id OR e.right_symbol_id = p_symbol_id)
      AND upper(e.tx_time) IS NULL
    ORDER BY e.confidence DESC;
$$;

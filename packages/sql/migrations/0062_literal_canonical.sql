-- Literal canonicalization: normalizes values, units, dates to canonical forms.

CREATE TABLE IF NOT EXISTS donto_literal_canonical (
    literal_id          BIGSERIAL PRIMARY KEY,
    datatype_iri        TEXT NOT NULL,
    raw_value           JSONB NOT NULL,
    raw_lexical         TEXT,
    canonical_value     JSONB NOT NULL,
    canonical_hash      BYTEA NOT NULL,
    unit_iri            TEXT,
    quantity_si          NUMERIC,
    precision_json      JSONB NOT NULL DEFAULT '{}',
    uncertainty_json    JSONB NOT NULL DEFAULT '{}',
    language_tag        TEXT,
    parser_version      TEXT NOT NULL DEFAULT 'v1',
    created_tx          TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (canonical_hash)
);

CREATE INDEX IF NOT EXISTS donto_literal_canonical_dt ON donto_literal_canonical (datatype_iri);
CREATE INDEX IF NOT EXISTS donto_literal_canonical_unit ON donto_literal_canonical (unit_iri) WHERE unit_iri IS NOT NULL;
CREATE INDEX IF NOT EXISTS donto_literal_canonical_qty ON donto_literal_canonical (quantity_si) WHERE quantity_si IS NOT NULL;

-- Register or retrieve a canonical literal
CREATE OR REPLACE FUNCTION donto_ensure_literal(
    p_datatype      TEXT,
    p_raw_value     JSONB,
    p_canonical     JSONB,
    p_unit          TEXT DEFAULT NULL,
    p_quantity_si   NUMERIC DEFAULT NULL,
    p_precision     JSONB DEFAULT '{}',
    p_uncertainty   JSONB DEFAULT '{}',
    p_language      TEXT DEFAULT NULL
) RETURNS BIGINT
LANGUAGE plpgsql AS $$
DECLARE
    v_hash BYTEA;
    v_id BIGINT;
BEGIN
    v_hash := sha256((p_datatype || '::' || p_canonical::text)::bytea);
    SELECT literal_id INTO v_id FROM donto_literal_canonical WHERE canonical_hash = v_hash;
    IF v_id IS NOT NULL THEN RETURN v_id; END IF;
    INSERT INTO donto_literal_canonical
        (datatype_iri, raw_value, raw_lexical, canonical_value, canonical_hash,
         unit_iri, quantity_si, precision_json, uncertainty_json, language_tag)
    VALUES (p_datatype, p_raw_value, p_raw_value::text, p_canonical, v_hash,
            p_unit, p_quantity_si, p_precision, p_uncertainty, p_language)
    ON CONFLICT (canonical_hash) DO NOTHING
    RETURNING literal_id INTO v_id;
    IF v_id IS NULL THEN
        SELECT literal_id INTO v_id FROM donto_literal_canonical WHERE canonical_hash = v_hash;
    END IF;
    RETURN v_id;
END;
$$;

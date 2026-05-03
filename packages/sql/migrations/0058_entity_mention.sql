-- Entity mention: each occurrence of a symbol in a document/span/extraction run.

CREATE TABLE IF NOT EXISTS donto_entity_mention (
    mention_id          BIGSERIAL PRIMARY KEY,
    symbol_id           BIGINT NOT NULL REFERENCES donto_entity_symbol(symbol_id),
    document_id         UUID,
    revision_id         UUID,
    span_id             UUID,
    extraction_run_id   UUID,
    surface_text        TEXT,
    normalized_text     TEXT,
    mention_type_hint   TEXT,  -- name, pronoun, definite_desc, abbreviation, alias
    confidence          DOUBLE PRECISION,
    tx_time             TSTZRANGE NOT NULL DEFAULT tstzrange(now(), NULL, '[)')
);

CREATE INDEX IF NOT EXISTS donto_entity_mention_symbol ON donto_entity_mention (symbol_id);
CREATE INDEX IF NOT EXISTS donto_entity_mention_doc ON donto_entity_mention (document_id) WHERE document_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS donto_entity_mention_span ON donto_entity_mention (span_id) WHERE span_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS donto_entity_mention_surface_trgm
    ON donto_entity_mention USING gin (normalized_text gin_trgm_ops)
    WHERE normalized_text IS NOT NULL;

CREATE OR REPLACE FUNCTION donto_record_mention(
    p_symbol_id     BIGINT,
    p_surface_text  TEXT,
    p_doc_id        UUID DEFAULT NULL,
    p_rev_id        UUID DEFAULT NULL,
    p_span_id       UUID DEFAULT NULL,
    p_run_id        UUID DEFAULT NULL,
    p_type_hint     TEXT DEFAULT NULL,
    p_confidence    DOUBLE PRECISION DEFAULT NULL
) RETURNS BIGINT
LANGUAGE sql AS $$
    INSERT INTO donto_entity_mention
        (symbol_id, document_id, revision_id, span_id, extraction_run_id,
         surface_text, normalized_text, mention_type_hint, confidence)
    VALUES
        (p_symbol_id, p_doc_id, p_rev_id, p_span_id, p_run_id,
         p_surface_text, lower(trim(p_surface_text)), p_type_hint, p_confidence)
    RETURNING mention_id;
$$;

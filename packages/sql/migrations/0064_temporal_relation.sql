-- Temporal relations between events using Allen's 13 interval relations.
-- Stored as a bitset: before=1, meets=2, overlaps=4, starts=8, during=16,
-- finishes=32, equals=64, after=128, met_by=256, overlapped_by=512,
-- started_by=1024, contains=2048, finished_by=4096

CREATE TABLE IF NOT EXISTS donto_temporal_relation (
    relation_id        BIGSERIAL PRIMARY KEY,
    left_event_iri     TEXT NOT NULL,
    right_event_iri    TEXT NOT NULL,
    allen_bitset       INTEGER NOT NULL,
    modality           TEXT NOT NULL DEFAULT 'asserted'
                       CHECK (modality IN ('asserted','derived','possible','necessary','probable')),
    confidence         DOUBLE PRECISION NOT NULL DEFAULT 1.0,
    evidence_json      JSONB NOT NULL DEFAULT '{}',
    context            TEXT,
    tx_time            TSTZRANGE NOT NULL DEFAULT tstzrange(now(), NULL, '[)')
);

CREATE INDEX IF NOT EXISTS donto_temporal_rel_left ON donto_temporal_relation (left_event_iri);
CREATE INDEX IF NOT EXISTS donto_temporal_rel_right ON donto_temporal_relation (right_event_iri);
CREATE INDEX IF NOT EXISTS donto_temporal_rel_tx ON donto_temporal_relation USING GIST (tx_time);

-- Assert a temporal relation
CREATE OR REPLACE FUNCTION donto_assert_temporal_relation(
    p_left_event    TEXT,
    p_right_event   TEXT,
    p_allen_bitset  INTEGER,
    p_modality      TEXT DEFAULT 'asserted',
    p_confidence    DOUBLE PRECISION DEFAULT 1.0,
    p_evidence      JSONB DEFAULT '{}',
    p_context       TEXT DEFAULT NULL
) RETURNS BIGINT
LANGUAGE sql AS $$
    INSERT INTO donto_temporal_relation
        (left_event_iri, right_event_iri, allen_bitset, modality, confidence, evidence_json, context)
    VALUES (p_left_event, p_right_event, p_allen_bitset, p_modality, p_confidence, p_evidence, p_context)
    RETURNING relation_id;
$$;

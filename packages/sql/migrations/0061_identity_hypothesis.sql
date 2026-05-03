-- Identity hypotheses: versioned clustering solutions over identity edges.

CREATE TABLE IF NOT EXISTS donto_identity_hypothesis (
    hypothesis_id       BIGSERIAL PRIMARY KEY,
    name                TEXT NOT NULL UNIQUE,
    description         TEXT,
    policy_json         JSONB NOT NULL DEFAULT '{}',
    threshold_same      DOUBLE PRECISION NOT NULL DEFAULT 0.85,
    threshold_distinct  DOUBLE PRECISION NOT NULL DEFAULT 0.05,
    created_tx          TIMESTAMPTZ NOT NULL DEFAULT now(),
    supersedes          BIGINT REFERENCES donto_identity_hypothesis(hypothesis_id),
    status              TEXT NOT NULL DEFAULT 'active'
                        CHECK (status IN ('active','superseded','draft','archived'))
);

CREATE TABLE IF NOT EXISTS donto_identity_membership (
    hypothesis_id       BIGINT NOT NULL REFERENCES donto_identity_hypothesis(hypothesis_id),
    referent_id         BIGINT NOT NULL,
    symbol_id           BIGINT NOT NULL REFERENCES donto_entity_symbol(symbol_id),
    posterior           DOUBLE PRECISION NOT NULL DEFAULT 1.0,
    membership_reason   JSONB NOT NULL DEFAULT '{}',
    tx_time             TSTZRANGE NOT NULL DEFAULT tstzrange(now(), NULL, '[)'),
    PRIMARY KEY (hypothesis_id, referent_id, symbol_id)
);

CREATE INDEX IF NOT EXISTS donto_identity_membership_symbol ON donto_identity_membership (hypothesis_id, symbol_id);
CREATE INDEX IF NOT EXISTS donto_identity_membership_referent ON donto_identity_membership (hypothesis_id, referent_id);

-- Resolve a symbol to a referent under a hypothesis
CREATE OR REPLACE FUNCTION donto_resolve_referent(
    p_hypothesis_id BIGINT,
    p_symbol_id     BIGINT
) RETURNS BIGINT
LANGUAGE sql STABLE AS $$
    SELECT referent_id
    FROM donto_identity_membership
    WHERE hypothesis_id = p_hypothesis_id
      AND symbol_id = p_symbol_id
      AND upper(tx_time) IS NULL
    ORDER BY posterior DESC
    LIMIT 1;
$$;

-- List all symbols in a referent cluster
CREATE OR REPLACE FUNCTION donto_referent_symbols(
    p_hypothesis_id BIGINT,
    p_referent_id   BIGINT
) RETURNS TABLE(symbol_id BIGINT, iri TEXT, posterior DOUBLE PRECISION)
LANGUAGE sql STABLE AS $$
    SELECT m.symbol_id, s.iri, m.posterior
    FROM donto_identity_membership m
    JOIN donto_entity_symbol s ON s.symbol_id = m.symbol_id
    WHERE m.hypothesis_id = p_hypothesis_id
      AND m.referent_id = p_referent_id
      AND upper(m.tx_time) IS NULL
    ORDER BY m.posterior DESC;
$$;

-- Create the three default hypotheses
INSERT INTO donto_identity_hypothesis (name, description, threshold_same, threshold_distinct, policy_json) VALUES
    ('strict', 'Only human-certified or >=0.98 same-referent edges', 0.98, 0.02,
     '{"require_human": false, "min_confidence": 0.98, "allow_cannot_link_override": false}'),
    ('likely', '>=0.85 same-referent edges, no strong cannot-link', 0.85, 0.05,
     '{"require_human": false, "min_confidence": 0.85, "allow_cannot_link_override": false}'),
    ('exploratory', '>=0.60 same-referent edges, useful for search/research', 0.60, 0.10,
     '{"require_human": false, "min_confidence": 0.60, "allow_cannot_link_override": true}')
ON CONFLICT (name) DO NOTHING;

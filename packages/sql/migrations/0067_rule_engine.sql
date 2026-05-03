-- Rule engine v2: inference rules, derivations, and agenda.
-- Uses donto_inference_rule to avoid collision with existing donto_rule (migration 0009).

CREATE TABLE IF NOT EXISTS donto_inference_rule (
    rule_id             BIGSERIAL PRIMARY KEY,
    name                TEXT NOT NULL UNIQUE,
    rule_class          TEXT NOT NULL CHECK (rule_class IN (
        'rdfs_subclass', 'rdfs_subproperty', 'rdfs_domain', 'rdfs_range',
        'inverse', 'symmetric', 'transitive',
        'functional_conflict', 'inverse_functional_identity',
        'event_frame', 'temporal', 'genealogical', 'custom'
    )),
    description         TEXT,
    body_ast            JSONB NOT NULL,
    head_template       JSONB NOT NULL,
    confidence_policy   JSONB NOT NULL DEFAULT '{"mode": "product"}',
    temporal_policy     JSONB NOT NULL DEFAULT '{"mode": "intersect"}',
    materialize_policy  JSONB NOT NULL DEFAULT '{"eager": false, "projection_only": true}',
    priority            INTEGER NOT NULL DEFAULT 100,
    enabled             BOOLEAN NOT NULL DEFAULT true,
    tx_time             TSTZRANGE NOT NULL DEFAULT tstzrange(now(), NULL, '[)')
);

CREATE TABLE IF NOT EXISTS donto_inference_derivation (
    derivation_id        BIGSERIAL PRIMARY KEY,
    derived_statement_id UUID NOT NULL,
    rule_id              BIGINT NOT NULL REFERENCES donto_inference_rule(rule_id),
    premise_statement_ids UUID[] NOT NULL,
    identity_edges       BIGINT[] NOT NULL DEFAULT '{}',
    predicate_edges      BIGINT[] NOT NULL DEFAULT '{}',
    confidence           DOUBLE PRECISION NOT NULL,
    explanation_json     JSONB NOT NULL DEFAULT '{}',
    created_tx           TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS donto_inf_derivation_stmt ON donto_inference_derivation (derived_statement_id);
CREATE INDEX IF NOT EXISTS donto_inf_derivation_rule ON donto_inference_derivation (rule_id);

CREATE TABLE IF NOT EXISTS donto_rule_agenda (
    agenda_id            BIGSERIAL PRIMARY KEY,
    trigger_type         TEXT NOT NULL CHECK (trigger_type IN (
        'new_statement', 'retracted_statement', 'new_alignment',
        'new_identity_edge', 'signature_change', 'rule_change', 'manual'
    )),
    changed_statement_id UUID,
    affected_predicate   TEXT,
    affected_symbol_id   BIGINT,
    affected_context     TEXT,
    valid_window         DATERANGE,
    priority             INTEGER NOT NULL DEFAULT 100,
    status               TEXT NOT NULL DEFAULT 'open'
                         CHECK (status IN ('open','processing','completed','failed','deferred')),
    created_tx           TIMESTAMPTZ NOT NULL DEFAULT now(),
    processed_tx         TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS donto_rule_agenda_status ON donto_rule_agenda (status, priority DESC) WHERE status = 'open';

-- Seed essential inference rules
INSERT INTO donto_inference_rule (name, rule_class, description, body_ast, head_template, priority) VALUES
    ('rdfs_subclass_type', 'rdfs_subclass',
     'x rdf:type C1, C1 rdfs:subClassOf C2 → x rdf:type C2',
     '{"patterns": [{"s": "?x", "p": "rdf:type", "o": "?c1"}, {"s": "?c1", "p": "rdfs:subClassOf", "o": "?c2"}]}',
     '{"s": "?x", "p": "rdf:type", "o": "?c2"}', 10),
    ('symmetric_spouse', 'symmetric',
     'x marriedTo y → y marriedTo x',
     '{"patterns": [{"s": "?x", "p": "marriedTo", "o": "?y"}]}',
     '{"s": "?y", "p": "marriedTo", "o": "?x"}', 20),
    ('inverse_parent_child', 'inverse',
     'x parentOf y → y childOf x',
     '{"patterns": [{"s": "?x", "p": "parentOf", "o": "?y"}]}',
     '{"s": "?y", "p": "childOf", "o": "?x"}', 20),
    ('inverse_child_parent', 'inverse',
     'x childOf y → y parentOf x',
     '{"patterns": [{"s": "?x", "p": "childOf", "o": "?y"}]}',
     '{"s": "?y", "p": "parentOf", "o": "?x"}', 20),
    ('functional_conflict_birth', 'functional_conflict',
     'Detect conflicting birth dates for the same person',
     '{"predicate": "hasBirthYear", "constraint": "functional_by_subject"}',
     '{"emit": "contradiction"}', 50)
ON CONFLICT (name) DO NOTHING;

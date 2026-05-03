-- Property constraints: formal domain, range, cardinality, and behavioral constraints.

CREATE TABLE IF NOT EXISTS donto_property_constraint (
    constraint_id       BIGSERIAL PRIMARY KEY,
    predicate_iri       TEXT NOT NULL,
    constraint_kind     TEXT NOT NULL CHECK (constraint_kind IN (
        'domain_class', 'range_class', 'range_datatype',
        'functional_by_subject', 'functional_by_subject_time',
        'inverse_functional', 'symmetric', 'transitive',
        'irreflexive', 'asymmetric', 'disjoint_property',
        'property_chain', 'event_decomposition_template',
        'unit_dimension', 'literal_parser', 'temporal_grain'
    )),
    value_json          JSONB NOT NULL,
    severity            TEXT NOT NULL DEFAULT 'warning'
                        CHECK (severity IN ('info','warning','violation','quarantine')),
    applies_context_kind TEXT,
    valid_time          DATERANGE,
    tx_time             TSTZRANGE NOT NULL DEFAULT tstzrange(now(), NULL, '[)'),
    metadata            JSONB NOT NULL DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS donto_prop_constraint_pred ON donto_property_constraint (predicate_iri);
CREATE INDEX IF NOT EXISTS donto_prop_constraint_kind ON donto_property_constraint (constraint_kind);

-- Check if a predicate has a specific constraint
CREATE OR REPLACE FUNCTION donto_has_constraint(
    p_predicate TEXT,
    p_kind      TEXT
) RETURNS BOOLEAN
LANGUAGE sql STABLE AS $$
    SELECT EXISTS (
        SELECT 1 FROM donto_property_constraint
        WHERE predicate_iri = p_predicate AND constraint_kind = p_kind
          AND upper(tx_time) IS NULL
    );
$$;

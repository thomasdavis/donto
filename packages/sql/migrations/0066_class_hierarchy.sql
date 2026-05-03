-- Class hierarchy: minimal upper ontology for type reasoning.

CREATE TABLE IF NOT EXISTS donto_class (
    class_iri           TEXT PRIMARY KEY,
    label               TEXT,
    description         TEXT,
    parent_class        TEXT REFERENCES donto_class(class_iri),
    disjoint_with       TEXT[],
    metadata            JSONB NOT NULL DEFAULT '{}'
);

-- Seed the upper ontology
INSERT INTO donto_class (class_iri, label, parent_class) VALUES
    ('donto:Entity', 'Entity', NULL),
    ('donto:Agent', 'Agent', 'donto:Entity'),
    ('donto:Person', 'Person', 'donto:Agent'),
    ('donto:Family', 'Family', 'donto:Agent'),
    ('donto:Organization', 'Organization', 'donto:Agent'),
    ('donto:GovernmentBody', 'Government Body', 'donto:Agent'),
    ('donto:Place', 'Place', 'donto:Entity'),
    ('donto:Settlement', 'Settlement', 'donto:Place'),
    ('donto:AdministrativeArea', 'Administrative Area', 'donto:Place'),
    ('donto:Property', 'Property', 'donto:Place'),
    ('donto:Building', 'Building', 'donto:Place'),
    ('donto:Region', 'Region', 'donto:Place'),
    ('donto:Event', 'Event', 'donto:Entity'),
    ('donto:BirthEvent', 'Birth Event', 'donto:Event'),
    ('donto:DeathEvent', 'Death Event', 'donto:Event'),
    ('donto:MarriageEvent', 'Marriage Event', 'donto:Event'),
    ('donto:ResidenceEvent', 'Residence Event', 'donto:Event'),
    ('donto:MigrationEvent', 'Migration Event', 'donto:Event'),
    ('donto:EmploymentEvent', 'Employment Event', 'donto:Event'),
    ('donto:LegalEvent', 'Legal Event', 'donto:Event'),
    ('donto:PublicationEvent', 'Publication Event', 'donto:Event'),
    ('donto:SourceArtifact', 'Source Artifact', 'donto:Entity'),
    ('donto:Concept', 'Concept', 'donto:Entity'),
    ('donto:Occupation', 'Occupation', 'donto:Concept'),
    ('donto:Role', 'Role', 'donto:Concept'),
    ('donto:Ethnicity', 'Ethnicity', 'donto:Concept'),
    ('donto:Religion', 'Religion', 'donto:Concept'),
    ('donto:Status', 'Status', 'donto:Concept'),
    ('donto:TemporalExpression', 'Temporal Expression', 'donto:Entity'),
    ('donto:QuantityExpression', 'Quantity Expression', 'donto:Entity')
ON CONFLICT (class_iri) DO NOTHING;

-- Update disjointness
UPDATE donto_class SET disjoint_with = ARRAY['donto:Place','donto:Event','donto:Concept'] WHERE class_iri = 'donto:Agent';
UPDATE donto_class SET disjoint_with = ARRAY['donto:Agent','donto:Event','donto:Concept'] WHERE class_iri = 'donto:Place';
UPDATE donto_class SET disjoint_with = ARRAY['donto:Agent','donto:Place','donto:Concept'] WHERE class_iri = 'donto:Event';

-- Recursive ancestor query
CREATE OR REPLACE FUNCTION donto_class_ancestors(p_class TEXT)
RETURNS TABLE(ancestor TEXT, depth INTEGER)
LANGUAGE sql STABLE AS $$
    WITH RECURSIVE ancestors AS (
        SELECT parent_class as ancestor, 1 as depth
        FROM donto_class WHERE class_iri = p_class AND parent_class IS NOT NULL
        UNION ALL
        SELECT c.parent_class, a.depth + 1
        FROM donto_class c JOIN ancestors a ON c.class_iri = a.ancestor
        WHERE c.parent_class IS NOT NULL
    )
    SELECT * FROM ancestors;
$$;

-- Check if class_a is a subclass of class_b
CREATE OR REPLACE FUNCTION donto_is_subclass(p_child TEXT, p_parent TEXT) RETURNS BOOLEAN
LANGUAGE sql STABLE AS $$
    SELECT p_child = p_parent OR EXISTS (
        SELECT 1 FROM donto_class_ancestors(p_child) WHERE ancestor = p_parent
    );
$$;

-- Detect disjointness violations for a subject
CREATE OR REPLACE FUNCTION donto_check_disjointness(p_subject TEXT)
RETURNS TABLE(class_a TEXT, class_b TEXT)
LANGUAGE sql STABLE AS $$
    SELECT DISTINCT t1.object_iri as class_a, t2.object_iri as class_b
    FROM donto_statement t1
    JOIN donto_statement t2 ON t1.subject = t2.subject
    JOIN donto_class c1 ON c1.class_iri = t1.object_iri
    WHERE t1.subject = p_subject
      AND t1.predicate = 'rdf:type' AND t2.predicate = 'rdf:type'
      AND t1.object_iri < t2.object_iri
      AND upper(t1.tx_time) IS NULL AND upper(t2.tx_time) IS NULL
      AND t2.object_iri = ANY(c1.disjoint_with);
$$;

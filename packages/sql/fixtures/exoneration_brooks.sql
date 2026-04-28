-- The Darnell Brooks exoneration, ingested as donto statements.
-- Used by the donto-faces visualisation as default demo data.
--
-- Five contexts, nine asserted statements, two retractions (one of which
-- spans 26 years), one correction, lineage between the conviction and the
-- evidence it rested on.

SELECT donto_ensure_context('ctx:state/court',         'source', 'permissive', NULL);
SELECT donto_ensure_context('ctx:darnell-brooks/own',  'source', 'permissive', NULL);
SELECT donto_ensure_context('ctx:witness/glasco',      'source', 'permissive', NULL);
SELECT donto_ensure_context('ctx:officer/raines',      'source', 'permissive', NULL);
SELECT donto_ensure_context('ctx:dna-lab',             'source', 'permissive', NULL);

-- The witness's original identification (will be corrected in 2019).
WITH ins AS (
  SELECT donto_assert(
    'ex:darnell-brooks', 'ex:identifiedAs',
    NULL, '{"v":"i''m sure it was him. i saw his face.","dt":"xsd:string"}'::jsonb,
    'ctx:witness/glasco', 'asserted', 0,
    '1996-04-11', '1996-04-12', NULL) AS id
)
SELECT id AS witness_id_1996 FROM ins \gset

-- The officer's report (lineage source for the conviction).
WITH ins AS (
  SELECT donto_assert(
    'ex:darnell-brooks', 'ex:reportedBy',
    NULL, '{"v":"officer raines: suspect matches description, in vicinity at 8th and main","dt":"xsd:string"}'::jsonb,
    'ctx:officer/raines', 'asserted', 0,
    '1996-04-11', '1996-04-12', NULL) AS id
)
SELECT id AS officer_report FROM ins \gset

-- The conviction.
WITH ins AS (
  SELECT donto_assert(
    'ex:darnell-brooks', 'ex:convictedOf',
    NULL, '{"v":"armed robbery, 8th and main, 1996-04-11; sentence 25-life","dt":"xsd:string"}'::jsonb,
    'ctx:state/court', 'asserted', 1,
    '1996-04-11', '1997-03-22', NULL) AS id
)
SELECT id AS conviction FROM ins \gset

-- Lineage: conviction rests on witness id + officer report.
INSERT INTO donto_stmt_lineage (statement_id, source_stmt) VALUES
    (:'conviction'::uuid, :'witness_id_1996'::uuid),
    (:'conviction'::uuid, :'officer_report'::uuid);

-- Brooks's own assertion of innocence, from day one.
SELECT donto_assert(
    'ex:darnell-brooks', 'ex:claimsInnocence',
    NULL, '{"v":"i was at my mother''s. i did not do this.","dt":"xsd:string"}'::jsonb,
    'ctx:darnell-brooks/own', 'asserted', 1,
    '1996-04-11', '1996-04-11', NULL);

-- Reasserted at every parole hearing (4 hearings, 5 years apart).
SELECT donto_assert(
    'ex:darnell-brooks', 'ex:claimsInnocence',
    NULL, '{"v":"i did not do this. [parole hearing 1 — denied]","dt":"xsd:string"}'::jsonb,
    'ctx:darnell-brooks/own', 'asserted', 1,
    '2003-06-12', '2003-06-12', NULL);

SELECT donto_assert(
    'ex:darnell-brooks', 'ex:claimsInnocence',
    NULL, '{"v":"i did not do this. [parole hearing 2 — denied]","dt":"xsd:string"}'::jsonb,
    'ctx:darnell-brooks/own', 'asserted', 1,
    '2008-06-04', '2008-06-04', NULL);

SELECT donto_assert(
    'ex:darnell-brooks', 'ex:claimsInnocence',
    NULL, '{"v":"i did not do this. [parole hearing 3 — denied]","dt":"xsd:string"}'::jsonb,
    'ctx:darnell-brooks/own', 'asserted', 1,
    '2013-05-29', '2013-05-29', NULL);

SELECT donto_assert(
    'ex:darnell-brooks', 'ex:claimsInnocence',
    NULL, '{"v":"i did not do this. [parole hearing 4 — denied]","dt":"xsd:string"}'::jsonb,
    'ctx:darnell-brooks/own', 'asserted', 1,
    '2018-06-15', '2018-06-15', NULL);

-- Witness correction in 2019: the original identification is retracted
-- and a corrected version asserted. valid_time stays anchored to 1996.
SELECT donto_correct(
    :'witness_id_1996'::uuid,
    NULL, NULL, NULL,
    '{"v":"i was scared. the officer said it was probably him. i wanted them to catch someone.","dt":"xsd:string"}'::jsonb,
    NULL, 'ex:dna-lab/2019');

-- DNA reanalysis 2023: backdated valid_time to 1996, believed only from 2023.
SELECT donto_assert(
    'ex:darnell-brooks', 'ex:dnaResult',
    NULL, '{"v":"evidence kit #4419-96 reanalysed; dna does not match brooks; matches ex:travis-dwyer","dt":"xsd:string"}'::jsonb,
    'ctx:dna-lab', 'asserted', 3,
    '1996-04-11', '2023-08-30', NULL);

-- The state retracts the conviction (closes tx_time) and asserts the exoneration.
DO $$
DECLARE
    conv_id uuid;
BEGIN
    SELECT statement_id INTO conv_id
    FROM donto_statement
    WHERE subject = 'ex:darnell-brooks'
      AND predicate = 'ex:convictedOf'
      AND context = 'ctx:state/court'
      AND upper(tx_time) IS NULL;
    IF conv_id IS NOT NULL THEN
        PERFORM donto_retract(conv_id, 'ex:state/court');
    END IF;
END$$;

SELECT donto_assert(
    'ex:darnell-brooks', 'ex:exonerated',
    NULL, '{"v":"conviction vacated; the state regrets the years lost.","dt":"xsd:string"}'::jsonb,
    'ctx:state/court', 'asserted', 3,
    '1996-04-11', '2023-09-08', NULL);

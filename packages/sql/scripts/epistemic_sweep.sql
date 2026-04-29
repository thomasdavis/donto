-- epistemic_sweep.sql — activate the full epistemic machinery on a live
-- donto database with 35M+ genealogy statements.
--
-- Run with:
--   psql -U donto -d donto -f epistemic_sweep.sql
--
-- PERFORMANCE CONTRACT:
--   * Never full-scans donto_statement (35M rows).
--   * Uses TABLESAMPLE, LIMIT, indexed WHERE, and context-scoped ops.
--   * Each section completes in under 30 seconds.
--   * All inserts use ON CONFLICT DO NOTHING for idempotency.
--   * Maturity updates use LIMIT-based batching (10 000 rows per chunk).
--
-- Sections:
--   1. Register predicate semantics
--   2. Register shapes
--   3. Run shape validation (sampled)
--   4. Run derivation rules
--   5. Find contradictions
--   6. Create proof obligations
--   7. Promote maturity

\timing on
\set ON_ERROR_STOP on

-- ============================================================================
-- SECTION 1: Register predicate semantics
-- ============================================================================
-- Mark functional predicates: a person has at most one birthYear, deathYear,
-- gender, birthPlace. Also ensure inverse/symmetric registrations are current.
DO $$
DECLARE
    v_n int;
BEGIN
    RAISE NOTICE '=== SECTION 1: Register predicate semantics ===';

    -- Ensure all genealogy predicates are registered (active, not implicit).
    INSERT INTO donto_predicate (iri, status, label) VALUES
        ('ex:birthYear',   'active', 'Birth year'),
        ('ex:deathYear',   'active', 'Death year'),
        ('ex:birthPlace',  'active', 'Birth place'),
        ('ex:deathPlace',  'active', 'Death place'),
        ('ex:gender',      'active', 'Gender'),
        ('ex:parentOf',    'active', 'Parent of'),
        ('ex:childOf',     'active', 'Child of'),
        ('ex:marriedTo',   'active', 'Married to'),
        ('ex:name',        'active', 'Name'),
        ('ex:knownAs',     'active', 'Known as'),
        ('ex:occupation',  'active', 'Occupation'),
        ('ex:livedIn',     'active', 'Lived in'),
        ('rdf:type',       'active', 'RDF type'),
        ('rdfs:label',     'active', 'Label'),
        ('foaf:name',      'active', 'FOAF name'),
        ('foaf:givenName', 'active', 'Given name'),
        ('foaf:familyName','active', 'Family name'),
        ('donto:content',  'active', 'Content'),
        ('donto:confidence','active','Confidence')
    ON CONFLICT (iri) DO UPDATE SET
        status = CASE WHEN donto_predicate.status = 'implicit' THEN 'active'
                      ELSE donto_predicate.status END;

    -- Mark functional predicates (at most one value per subject).
    UPDATE donto_predicate
       SET is_functional = true,
           card_max      = 1
     WHERE iri IN ('ex:birthYear', 'ex:deathYear', 'ex:gender', 'ex:birthPlace')
       AND (is_functional = false OR card_max IS DISTINCT FROM 1);
    GET DIAGNOSTICS v_n = ROW_COUNT;
    RAISE NOTICE 'Functional predicates updated: %', v_n;

    -- Ensure inverse registrations.
    UPDATE donto_predicate SET inverse_of = 'ex:childOf'
     WHERE iri = 'ex:parentOf' AND inverse_of IS DISTINCT FROM 'ex:childOf';
    UPDATE donto_predicate SET inverse_of = 'ex:parentOf'
     WHERE iri = 'ex:childOf'  AND inverse_of IS DISTINCT FROM 'ex:parentOf';

    -- Ensure symmetric registration.
    UPDATE donto_predicate SET is_symmetric = true
     WHERE iri = 'ex:marriedTo' AND is_symmetric = false;
    UPDATE donto_predicate SET is_symmetric = true
     WHERE iri = 'donto:SameMeaning' AND is_symmetric = false;

    -- Set range datatypes where applicable.
    UPDATE donto_predicate SET range_datatype = 'xsd:integer'
     WHERE iri IN ('ex:birthYear', 'ex:deathYear') AND range_datatype IS NULL;
    UPDATE donto_predicate SET range_datatype = 'xsd:string'
     WHERE iri IN ('ex:name', 'ex:knownAs', 'foaf:name', 'foaf:givenName', 'foaf:familyName')
       AND range_datatype IS NULL;

    RAISE NOTICE 'Section 1 complete.';
END $$;


-- ============================================================================
-- SECTION 2: Register shapes
-- ============================================================================
-- Insert builtin shapes for each functional predicate.
DO $$
DECLARE
    v_pred text;
    v_count int := 0;
BEGIN
    RAISE NOTICE '=== SECTION 2: Register shapes ===';

    -- Register a functional shape for each functional predicate.
    FOR v_pred IN
        SELECT iri FROM donto_predicate
         WHERE is_functional = true AND status = 'active'
    LOOP
        INSERT INTO donto_shape (iri, label, description, severity, body_kind, body)
        VALUES (
            'builtin:functional/' || v_pred,
            'Functional constraint: ' || v_pred,
            'Subject must have at most one value for ' || v_pred,
            'violation',
            'builtin',
            jsonb_build_object('kind', 'functional', 'predicate', v_pred)
        )
        ON CONFLICT (iri) DO UPDATE SET
            body = jsonb_build_object('kind', 'functional', 'predicate', v_pred),
            label = EXCLUDED.label,
            description = EXCLUDED.description;
        v_count := v_count + 1;
    END LOOP;

    -- Register datatype shapes for year predicates.
    INSERT INTO donto_shape (iri, label, description, severity, body_kind, body)
    VALUES
        ('builtin:datatype/ex:birthYear/xsd:integer',
         'Datatype: ex:birthYear must be xsd:integer',
         'Birth year literals must have datatype xsd:integer',
         'warning', 'builtin',
         '{"kind":"datatype","predicate":"ex:birthYear","datatype":"xsd:integer"}'::jsonb),
        ('builtin:datatype/ex:deathYear/xsd:integer',
         'Datatype: ex:deathYear must be xsd:integer',
         'Death year literals must have datatype xsd:integer',
         'warning', 'builtin',
         '{"kind":"datatype","predicate":"ex:deathYear","datatype":"xsd:integer"}'::jsonb)
    ON CONFLICT (iri) DO NOTHING;

    RAISE NOTICE 'Registered % functional shapes + 2 datatype shapes.', v_count;
    RAISE NOTICE 'Section 2 complete.';
END $$;


-- ============================================================================
-- SECTION 3: Run shape validation (sampled)
-- ============================================================================
-- For each functional predicate, find subjects with >1 distinct value. Use
-- indexed predicate lookups with LIMIT, never a full table scan.
DO $$
DECLARE
    v_pred       text;
    v_shape_iri  text;
    v_ctx        text := 'ctx:epistemic-sweep/shapes';
    v_report_id  bigint;
    v_viol_count bigint;
    v_focus_cnt  bigint;
    v_total_violations bigint := 0;
    v_rec        record;
BEGIN
    RAISE NOTICE '=== SECTION 3: Shape validation (sampled) ===';

    -- Ensure our working context exists.
    PERFORM donto_ensure_context(v_ctx, 'system', 'permissive');

    FOR v_pred IN
        SELECT iri FROM donto_predicate
         WHERE is_functional = true AND status = 'active'
    LOOP
        v_shape_iri := 'builtin:functional/' || v_pred;

        -- Find subjects with >1 distinct value for this functional predicate.
        -- Uses the (predicate, object_iri, subject) index (pos_idx) and
        -- the (subject, predicate, object_iri) index (spo_idx).
        -- We LIMIT to 1000 violating subjects per predicate to bound runtime.
        DROP TABLE IF EXISTS _func_violations;
        CREATE TEMP TABLE _func_violations AS
        SELECT subject, count(*) AS val_count
          FROM (
            -- Use a CTE scanning the predicate index, limited to 50000 rows
            -- to avoid runaway scans. This samples the top of the index.
            SELECT subject,
                   COALESCE(object_iri, object_lit::text) AS obj_val
              FROM donto_statement
             WHERE predicate = v_pred
               AND upper(tx_time) IS NULL
               AND (flags & 3) = 0  -- asserted polarity only
             LIMIT 50000
          ) sample
         GROUP BY subject
        HAVING count(DISTINCT obj_val) > 1
         LIMIT 1000;

        SELECT count(*) INTO v_viol_count FROM _func_violations;

        IF v_viol_count > 0 THEN
            -- Compute focus count (distinct subjects sampled).
            SELECT count(DISTINCT subject) INTO v_focus_cnt
              FROM (
                SELECT subject FROM donto_statement
                 WHERE predicate = v_pred
                   AND upper(tx_time) IS NULL
                   AND (flags & 3) = 0
                 LIMIT 50000
              ) s;

            -- Create a shape report entry.
            INSERT INTO donto_shape_report
                (shape_iri, scope_fingerprint, scope, report, focus_count, violation_count)
            VALUES (
                v_shape_iri,
                digest('epistemic_sweep:' || v_pred || ':' || now()::text, 'sha256'),
                jsonb_build_object('predicate', v_pred, 'sample_limit', 50000),
                jsonb_build_object(
                    'violation_subjects_sample', (
                        SELECT jsonb_agg(jsonb_build_object('subject', subject, 'count', val_count))
                          FROM (SELECT * FROM _func_violations LIMIT 100) top
                    )
                ),
                v_focus_cnt,
                v_viol_count
            )
            RETURNING report_id INTO v_report_id;

            -- Attach shape annotations for each violating statement.
            -- For each violating subject, annotate all their statements for
            -- this predicate. Use LIMIT to bound work.
            FOR v_rec IN
                SELECT fv.subject FROM _func_violations fv LIMIT 200
            LOOP
                -- Annotate each open statement for this subject+predicate.
                INSERT INTO donto_stmt_shape_annotation
                    (statement_id, shape_iri, verdict, context, detail)
                SELECT
                    s.statement_id,
                    v_shape_iri,
                    'violate',
                    v_ctx,
                    jsonb_build_object(
                        'reason', 'Functional predicate has multiple values',
                        'subject', v_rec.subject,
                        'predicate', v_pred,
                        'report_id', v_report_id
                    )
                FROM donto_statement s
                WHERE s.subject = v_rec.subject
                  AND s.predicate = v_pred
                  AND upper(s.tx_time) IS NULL
                ON CONFLICT (statement_id, shape_iri)
                    WHERE upper(tx_time) IS NULL
                    DO NOTHING;
            END LOOP;

            -- Also link report to violating statements via donto_stmt_shape_reports.
            INSERT INTO donto_stmt_shape_reports (statement_id, report_id, severity)
            SELECT s.statement_id, v_report_id, 'violation'
              FROM _func_violations fv
              JOIN donto_statement s
                ON s.subject = fv.subject
               AND s.predicate = v_pred
               AND upper(s.tx_time) IS NULL
             LIMIT 2000
            ON CONFLICT (statement_id, report_id) DO NOTHING;

            v_total_violations := v_total_violations + v_viol_count;
            RAISE NOTICE '  % — % violating subjects (of % sampled)',
                v_pred, v_viol_count, v_focus_cnt;
        ELSE
            RAISE NOTICE '  % — no violations in sample', v_pred;
        END IF;

        DROP TABLE IF EXISTS _func_violations;
    END LOOP;

    RAISE NOTICE 'Total functional violations found: %', v_total_violations;
    RAISE NOTICE 'Section 3 complete.';
END $$;


-- ============================================================================
-- SECTION 4: Run derivation rules
-- ============================================================================
-- Derive inverse and symmetric closures using donto_assert.
-- Process in bounded batches using LIMIT.
DO $$
DECLARE
    v_derive_ctx text := 'ctx:derivation/epistemic-sweep';
    v_n          int;
    v_total      int := 0;
    v_batch_size int := 10000;
    v_rec        record;
BEGIN
    RAISE NOTICE '=== SECTION 4: Derivation rules ===';

    PERFORM donto_ensure_context(v_derive_ctx, 'derivation', 'permissive');

    -- -----------------------------------------------------------------------
    -- 4a. builtin:inverse/ex:parentOf/ex:childOf
    -- For each (s, ex:parentOf, o), derive (o, ex:childOf, s).
    -- Only derive where the inverse does not already exist.
    -- Uses the predicate index on ex:parentOf.
    -- -----------------------------------------------------------------------
    RAISE NOTICE '  Deriving ex:childOf from ex:parentOf ...';

    FOR v_rec IN
        SELECT src.statement_id AS source_id,
               src.object_iri   AS child_subject,
               src.subject      AS parent_object,
               src.valid_time,
               src.flags
          FROM donto_statement src
         WHERE src.predicate = 'ex:parentOf'
           AND src.object_iri IS NOT NULL
           AND upper(src.tx_time) IS NULL
           AND (src.flags & 3) = 0  -- asserted only
           -- Exclude those where the inverse already exists.
           AND NOT EXISTS (
               SELECT 1 FROM donto_statement inv
                WHERE inv.subject   = src.object_iri
                  AND inv.predicate = 'ex:childOf'
                  AND inv.object_iri = src.subject
                  AND upper(inv.tx_time) IS NULL
           )
         LIMIT v_batch_size
    LOOP
        -- Insert the derived statement directly for speed (bypass donto_assert
        -- row-at-a-time overhead). Use content_hash conflict for idempotency.
        INSERT INTO donto_statement
            (subject, predicate, object_iri, object_lit, context, valid_time, flags)
        VALUES (
            v_rec.child_subject,
            'ex:childOf',
            v_rec.parent_object,
            NULL,
            v_derive_ctx,
            v_rec.valid_time,
            v_rec.flags  -- preserve polarity+maturity
        )
        ON CONFLICT (content_hash) WHERE upper(tx_time) IS NULL DO NOTHING;

        -- Record lineage: derived statement came from source.
        -- We need the new statement_id. If ON CONFLICT fired, the row
        -- already exists and lineage was already recorded.
        BEGIN
            INSERT INTO donto_stmt_lineage (statement_id, source_stmt)
            SELECT s.statement_id, v_rec.source_id
              FROM donto_statement s
             WHERE s.subject   = v_rec.child_subject
               AND s.predicate = 'ex:childOf'
               AND s.object_iri = v_rec.parent_object
               AND s.context   = v_derive_ctx
               AND upper(s.tx_time) IS NULL
             LIMIT 1
            ON CONFLICT (statement_id, source_stmt) DO NOTHING;
        EXCEPTION WHEN OTHERS THEN
            -- Lineage FK may fail if statement was deduplicated; safe to skip.
            NULL;
        END;

        v_total := v_total + 1;
    END LOOP;
    RAISE NOTICE '  Derived % ex:childOf statements (batch limit %)', v_total, v_batch_size;

    -- -----------------------------------------------------------------------
    -- 4b. Also derive ex:parentOf from ex:childOf (bidirectional).
    -- -----------------------------------------------------------------------
    v_n := 0;
    RAISE NOTICE '  Deriving ex:parentOf from ex:childOf ...';

    FOR v_rec IN
        SELECT src.statement_id AS source_id,
               src.object_iri   AS parent_subject,
               src.subject      AS child_object,
               src.valid_time,
               src.flags
          FROM donto_statement src
         WHERE src.predicate = 'ex:childOf'
           AND src.object_iri IS NOT NULL
           AND upper(src.tx_time) IS NULL
           AND (src.flags & 3) = 0
           AND NOT EXISTS (
               SELECT 1 FROM donto_statement inv
                WHERE inv.subject   = src.object_iri
                  AND inv.predicate = 'ex:parentOf'
                  AND inv.object_iri = src.subject
                  AND upper(inv.tx_time) IS NULL
           )
         LIMIT v_batch_size
    LOOP
        INSERT INTO donto_statement
            (subject, predicate, object_iri, object_lit, context, valid_time, flags)
        VALUES (
            v_rec.parent_subject,
            'ex:parentOf',
            v_rec.child_object,
            NULL,
            v_derive_ctx,
            v_rec.valid_time,
            v_rec.flags
        )
        ON CONFLICT (content_hash) WHERE upper(tx_time) IS NULL DO NOTHING;

        BEGIN
            INSERT INTO donto_stmt_lineage (statement_id, source_stmt)
            SELECT s.statement_id, v_rec.source_id
              FROM donto_statement s
             WHERE s.subject   = v_rec.parent_subject
               AND s.predicate = 'ex:parentOf'
               AND s.object_iri = v_rec.child_object
               AND s.context   = v_derive_ctx
               AND upper(s.tx_time) IS NULL
             LIMIT 1
            ON CONFLICT (statement_id, source_stmt) DO NOTHING;
        EXCEPTION WHEN OTHERS THEN
            NULL;
        END;

        v_n := v_n + 1;
    END LOOP;
    RAISE NOTICE '  Derived % ex:parentOf statements', v_n;
    v_total := v_total + v_n;

    -- -----------------------------------------------------------------------
    -- 4c. builtin:symmetric/ex:marriedTo
    -- For each (s, ex:marriedTo, o), derive (o, ex:marriedTo, s).
    -- -----------------------------------------------------------------------
    v_n := 0;
    RAISE NOTICE '  Deriving symmetric ex:marriedTo ...';

    FOR v_rec IN
        SELECT src.statement_id AS source_id,
               src.object_iri   AS reverse_subject,
               src.subject      AS reverse_object,
               src.valid_time,
               src.flags
          FROM donto_statement src
         WHERE src.predicate = 'ex:marriedTo'
           AND src.object_iri IS NOT NULL
           AND upper(src.tx_time) IS NULL
           AND (src.flags & 3) = 0
           AND NOT EXISTS (
               SELECT 1 FROM donto_statement rev
                WHERE rev.subject    = src.object_iri
                  AND rev.predicate  = 'ex:marriedTo'
                  AND rev.object_iri = src.subject
                  AND upper(rev.tx_time) IS NULL
           )
         LIMIT v_batch_size
    LOOP
        INSERT INTO donto_statement
            (subject, predicate, object_iri, object_lit, context, valid_time, flags)
        VALUES (
            v_rec.reverse_subject,
            'ex:marriedTo',
            v_rec.reverse_object,
            NULL,
            v_derive_ctx,
            v_rec.valid_time,
            v_rec.flags
        )
        ON CONFLICT (content_hash) WHERE upper(tx_time) IS NULL DO NOTHING;

        BEGIN
            INSERT INTO donto_stmt_lineage (statement_id, source_stmt)
            SELECT s.statement_id, v_rec.source_id
              FROM donto_statement s
             WHERE s.subject    = v_rec.reverse_subject
               AND s.predicate  = 'ex:marriedTo'
               AND s.object_iri = v_rec.reverse_object
               AND s.context    = v_derive_ctx
               AND upper(s.tx_time) IS NULL
             LIMIT 1
            ON CONFLICT (statement_id, source_stmt) DO NOTHING;
        EXCEPTION WHEN OTHERS THEN
            NULL;
        END;

        v_n := v_n + 1;
    END LOOP;
    RAISE NOTICE '  Derived % symmetric ex:marriedTo statements', v_n;
    v_total := v_total + v_n;

    -- Record derivation reports (one per rule per sweep run).
    INSERT INTO donto_derivation_report
        (rule_iri, inputs_fingerprint, scope, into_ctx, emitted_count)
    VALUES
        ('builtin:inverse/ex:parentOf/ex:childOf',
         digest('epistemic_sweep:inverse:parentOf:' || now()::text, 'sha256'),
         '{"sweep":"epistemic_sweep"}'::jsonb,
         v_derive_ctx, v_total);

    INSERT INTO donto_derivation_report
        (rule_iri, inputs_fingerprint, scope, into_ctx, emitted_count)
    VALUES
        ('builtin:symmetric/ex:marriedTo',
         digest('epistemic_sweep:symmetric:marriedTo:' || now()::text, 'sha256'),
         '{"sweep":"epistemic_sweep"}'::jsonb,
         v_derive_ctx, v_n);

    RAISE NOTICE 'Total derived statements: %', v_total;
    RAISE NOTICE 'Section 4 complete.';
END $$;


-- ============================================================================
-- SECTION 5: Find contradictions
-- ============================================================================
-- For functional predicates, find subjects with conflicting values and create
-- donto_argument entries with relation='rebuts'.
DO $$
DECLARE
    v_pred          text;
    v_ctx           text := 'ctx:epistemic-sweep/contradictions';
    v_arg_count     int := 0;
    v_rec           record;
    v_stmt_a        uuid;
    v_stmt_b        uuid;
BEGIN
    RAISE NOTICE '=== SECTION 5: Find contradictions ===';

    PERFORM donto_ensure_context(v_ctx, 'system', 'permissive');

    FOR v_pred IN
        SELECT iri FROM donto_predicate
         WHERE is_functional = true AND status = 'active'
    LOOP
        -- Find subjects with >1 distinct value. Use indexed predicate scan
        -- with LIMIT to cap work.
        FOR v_rec IN
            WITH candidates AS (
                SELECT subject,
                       COALESCE(object_iri, object_lit::text) AS obj_val,
                       statement_id
                  FROM donto_statement
                 WHERE predicate = v_pred
                   AND upper(tx_time) IS NULL
                   AND (flags & 3) = 0
                 LIMIT 50000
            ),
            multi AS (
                SELECT subject
                  FROM candidates
                 GROUP BY subject
                HAVING count(DISTINCT obj_val) > 1
                 LIMIT 500
            )
            SELECT m.subject FROM multi m
        LOOP
            -- Get the first two distinct-valued statements for this subject+predicate.
            -- These form the rebuttal pair.
            SELECT s1.statement_id, s2.statement_id
              INTO v_stmt_a, v_stmt_b
              FROM (
                  SELECT statement_id,
                         COALESCE(object_iri, object_lit::text) AS val,
                         row_number() OVER (ORDER BY statement_id) AS rn
                    FROM donto_statement
                   WHERE subject   = v_rec.subject
                     AND predicate = v_pred
                     AND upper(tx_time) IS NULL
                     AND (flags & 3) = 0
              ) ranked
              CROSS JOIN LATERAL (
                  SELECT statement_id
                    FROM donto_statement
                   WHERE subject   = v_rec.subject
                     AND predicate = v_pred
                     AND upper(tx_time) IS NULL
                     AND (flags & 3) = 0
                     AND COALESCE(object_iri, object_lit::text)
                         <> ranked.val
                   LIMIT 1
              ) s2(statement_id)
              CROSS JOIN LATERAL (
                  SELECT ranked.statement_id
              ) s1(statement_id)
              WHERE ranked.rn = 1
              LIMIT 1;

            IF v_stmt_a IS NOT NULL AND v_stmt_b IS NOT NULL THEN
                -- Insert argument: stmt_a rebuts stmt_b (and vice versa).
                -- Use the open_uniq index for idempotency.
                INSERT INTO donto_argument
                    (source_statement_id, target_statement_id, relation,
                     context, strength, evidence)
                VALUES (
                    v_stmt_a, v_stmt_b, 'rebuts',
                    v_ctx, 0.8,
                    jsonb_build_object(
                        'reason', 'Functional predicate conflict',
                        'predicate', v_pred,
                        'subject', v_rec.subject,
                        'sweep', 'epistemic_sweep'
                    )
                )
                ON CONFLICT (source_statement_id, target_statement_id, relation, context)
                    WHERE upper(tx_time) IS NULL
                    DO NOTHING;

                INSERT INTO donto_argument
                    (source_statement_id, target_statement_id, relation,
                     context, strength, evidence)
                VALUES (
                    v_stmt_b, v_stmt_a, 'rebuts',
                    v_ctx, 0.8,
                    jsonb_build_object(
                        'reason', 'Functional predicate conflict',
                        'predicate', v_pred,
                        'subject', v_rec.subject,
                        'sweep', 'epistemic_sweep'
                    )
                )
                ON CONFLICT (source_statement_id, target_statement_id, relation, context)
                    WHERE upper(tx_time) IS NULL
                    DO NOTHING;

                v_arg_count := v_arg_count + 1;
            END IF;
        END LOOP;

        IF v_arg_count > 0 THEN
            RAISE NOTICE '  % — % contradiction pairs', v_pred, v_arg_count;
        END IF;
    END LOOP;

    RAISE NOTICE 'Total contradiction pairs created: %', v_arg_count;
    RAISE NOTICE 'Section 5 complete.';
END $$;


-- ============================================================================
-- SECTION 6: Create proof obligations
-- ============================================================================
-- 6a. 'needs-source-support' for statements without evidence links.
-- 6b. 'needs-shape-review' (custom type mapped to 'needs-human-review')
--     for statements with shape violations.
-- Uses TABLESAMPLE and indexed joins to avoid full scans.
DO $$
DECLARE
    v_ctx       text := 'ctx:epistemic-sweep/obligations';
    v_n         int := 0;
    v_total     int := 0;
    v_batch     int := 10000;
BEGIN
    RAISE NOTICE '=== SECTION 6: Create proof obligations ===';

    PERFORM donto_ensure_context(v_ctx, 'system', 'permissive');

    -- -----------------------------------------------------------------------
    -- 6a. needs-source-support: statements in genealogy predicates that have
    --     no evidence links at all. Use TABLESAMPLE to avoid full scan.
    -- -----------------------------------------------------------------------
    RAISE NOTICE '  6a. Creating needs-source-support obligations ...';

    -- Sample ~1% of the table via TABLESAMPLE BERNOULLI.
    -- On a 35M row table this yields ~350K rows, then we filter.
    INSERT INTO donto_proof_obligation
        (statement_id, obligation_type, status, priority, context, detail)
    SELECT
        s.statement_id,
        'needs-source-support',
        'open',
        1::smallint,
        v_ctx,
        jsonb_build_object(
            'reason', 'No evidence link found',
            'predicate', s.predicate,
            'sweep', 'epistemic_sweep'
        )
    FROM (SELECT * FROM donto_statement TABLESAMPLE BERNOULLI (0.5)) s
    WHERE upper(s.tx_time) IS NULL
      AND (s.flags & 3) = 0
      AND s.predicate IN (
          'ex:birthYear', 'ex:deathYear', 'ex:birthPlace', 'ex:deathPlace',
          'ex:parentOf', 'ex:childOf', 'ex:marriedTo', 'ex:name', 'ex:gender'
      )
      AND NOT EXISTS (
          SELECT 1 FROM donto_evidence_link el
           WHERE el.statement_id = s.statement_id
             AND upper(el.tx_time) IS NULL
      )
      -- Skip statements that already have an open obligation of this type.
      AND NOT EXISTS (
          SELECT 1 FROM donto_proof_obligation po
           WHERE po.statement_id = s.statement_id
             AND po.obligation_type = 'needs-source-support'
             AND po.status = 'open'
      )
    LIMIT v_batch;

    GET DIAGNOSTICS v_n = ROW_COUNT;
    v_total := v_total + v_n;
    RAISE NOTICE '  Created % needs-source-support obligations', v_n;

    -- -----------------------------------------------------------------------
    -- 6b. needs-human-review for shape violations.
    -- Scan the shape annotation index, not the statement table.
    -- -----------------------------------------------------------------------
    RAISE NOTICE '  6b. Creating needs-human-review obligations for shape violations ...';

    INSERT INTO donto_proof_obligation
        (statement_id, obligation_type, status, priority, context, detail)
    SELECT DISTINCT ON (sa.statement_id)
        sa.statement_id,
        'needs-human-review',
        'open',
        2::smallint,  -- higher priority than source-support
        v_ctx,
        jsonb_build_object(
            'reason', 'Shape violation detected',
            'shape_iri', sa.shape_iri,
            'verdict', sa.verdict,
            'sweep', 'epistemic_sweep'
        )
    FROM donto_stmt_shape_annotation sa
    WHERE sa.verdict = 'violate'
      AND upper(sa.tx_time) IS NULL
      -- Skip those that already have an open obligation.
      AND NOT EXISTS (
          SELECT 1 FROM donto_proof_obligation po
           WHERE po.statement_id = sa.statement_id
             AND po.obligation_type = 'needs-human-review'
             AND po.status = 'open'
      )
    LIMIT v_batch;

    GET DIAGNOSTICS v_n = ROW_COUNT;
    v_total := v_total + v_n;
    RAISE NOTICE '  Created % needs-human-review obligations from shape violations', v_n;

    RAISE NOTICE 'Total proof obligations created: %', v_total;
    RAISE NOTICE 'Section 6 complete.';
END $$;


-- ============================================================================
-- SECTION 7: Promote maturity
-- ============================================================================
-- Batch-promote maturity levels using LIMIT to avoid locking the whole table.
-- Process 10 000 rows per chunk, multiple chunks per level.
--
-- Maturity ladder (PRD S2):
--   L0 (raw)  -> L1 (registered):  predicate is active (not 'implicit')
--   L1        -> L2 (evidenced):    has at least one evidence link
--   L2        -> L3 (validated):    has shape report with no violations

-- Helper: update flags to set maturity bits.
-- maturity is bits 2-4, polarity is bits 0-1.
-- new_flags = (old_flags & 0x03) | (new_maturity << 2)

-- L0 -> L1: promote statements whose predicate is registered (active).
DO $$
DECLARE
    v_chunk   int := 10000;
    v_updated int;
    v_total   int := 0;
    v_rounds  int := 0;
    v_max_rounds int := 50;  -- safety: max 500K rows per level
BEGIN
    RAISE NOTICE '=== SECTION 7: Promote maturity ===';
    RAISE NOTICE '  7a. L0 -> L1 (predicate registered) ...';

    LOOP
        EXIT WHEN v_rounds >= v_max_rounds;

        -- Find L0 statements whose predicate is registered (active).
        -- Maturity = (flags >> 2) & 7 = 0 means L0.
        -- New maturity L1 = flags with bits 2-4 set to 1:
        --   new_flags = (flags & 3) | (1 << 2) = (flags & 3) | 4
        UPDATE donto_statement s
           SET flags = (s.flags & 3) | (1 << 2)
         WHERE s.statement_id IN (
            SELECT s2.statement_id
              FROM donto_statement s2
              JOIN donto_predicate p ON p.iri = s2.predicate
             WHERE ((s2.flags >> 2) & 7) = 0         -- currently L0
               AND upper(s2.tx_time) IS NULL
               AND p.status = 'active'
             LIMIT v_chunk
         );
        GET DIAGNOSTICS v_updated = ROW_COUNT;
        v_total := v_total + v_updated;
        v_rounds := v_rounds + 1;

        EXIT WHEN v_updated < v_chunk;  -- no more rows to process
    END LOOP;

    RAISE NOTICE '  L0->L1: promoted % statements in % rounds', v_total, v_rounds;
END $$;

-- L1 -> L2: promote statements that have at least one evidence link.
DO $$
DECLARE
    v_chunk   int := 10000;
    v_updated int;
    v_total   int := 0;
    v_rounds  int := 0;
    v_max_rounds int := 50;
BEGIN
    RAISE NOTICE '  7b. L1 -> L2 (has evidence link) ...';

    LOOP
        EXIT WHEN v_rounds >= v_max_rounds;

        UPDATE donto_statement s
           SET flags = (s.flags & 3) | (2 << 2)
         WHERE s.statement_id IN (
            SELECT s2.statement_id
              FROM donto_statement s2
             WHERE ((s2.flags >> 2) & 7) = 1         -- currently L1
               AND upper(s2.tx_time) IS NULL
               AND EXISTS (
                   SELECT 1 FROM donto_evidence_link el
                    WHERE el.statement_id = s2.statement_id
                      AND upper(el.tx_time) IS NULL
               )
             LIMIT v_chunk
         );
        GET DIAGNOSTICS v_updated = ROW_COUNT;
        v_total := v_total + v_updated;
        v_rounds := v_rounds + 1;

        EXIT WHEN v_updated < v_chunk;
    END LOOP;

    RAISE NOTICE '  L1->L2: promoted % statements in % rounds', v_total, v_rounds;
END $$;

-- L2 -> L3: promote statements with shape report and no violations.
DO $$
DECLARE
    v_chunk   int := 10000;
    v_updated int;
    v_total   int := 0;
    v_rounds  int := 0;
    v_max_rounds int := 50;
BEGIN
    RAISE NOTICE '  7c. L2 -> L3 (shape clean) ...';

    LOOP
        EXIT WHEN v_rounds >= v_max_rounds;

        UPDATE donto_statement s
           SET flags = (s.flags & 3) | (3 << 2)
         WHERE s.statement_id IN (
            SELECT s2.statement_id
              FROM donto_statement s2
             WHERE ((s2.flags >> 2) & 7) = 2         -- currently L2
               AND upper(s2.tx_time) IS NULL
               -- Has at least one shape annotation ...
               AND EXISTS (
                   SELECT 1 FROM donto_stmt_shape_annotation sa
                    WHERE sa.statement_id = s2.statement_id
                      AND upper(sa.tx_time) IS NULL
               )
               -- ... and none of them are violations.
               AND NOT EXISTS (
                   SELECT 1 FROM donto_stmt_shape_annotation sa
                    WHERE sa.statement_id = s2.statement_id
                      AND sa.verdict = 'violate'
                      AND upper(sa.tx_time) IS NULL
               )
             LIMIT v_chunk
         );
        GET DIAGNOSTICS v_updated = ROW_COUNT;
        v_total := v_total + v_updated;
        v_rounds := v_rounds + 1;

        EXIT WHEN v_updated < v_chunk;
    END LOOP;

    RAISE NOTICE '  L2->L3: promoted % statements in % rounds', v_total, v_rounds;
    RAISE NOTICE 'Section 7 complete.';
END $$;


-- ============================================================================
-- FINAL SUMMARY
-- ============================================================================
DO $$
DECLARE
    v_stmt_count    bigint;
    v_pred_active   int;
    v_shapes        int;
    v_violations    bigint;
    v_derivations   bigint;
    v_arguments     bigint;
    v_obligations   bigint;
    v_mat           record;
BEGIN
    RAISE NOTICE '=== EPISTEMIC SWEEP SUMMARY ===';

    -- Statement count (approximate, from pg_class to avoid seq scan).
    SELECT reltuples::bigint INTO v_stmt_count
      FROM pg_class WHERE relname = 'donto_statement';
    RAISE NOTICE 'Statements (approx): %', v_stmt_count;

    SELECT count(*) INTO v_pred_active
      FROM donto_predicate WHERE status = 'active';
    RAISE NOTICE 'Active predicates: %', v_pred_active;

    SELECT count(*) INTO v_shapes FROM donto_shape;
    RAISE NOTICE 'Registered shapes: %', v_shapes;

    SELECT count(*) INTO v_violations
      FROM donto_stmt_shape_annotation
     WHERE verdict = 'violate' AND upper(tx_time) IS NULL;
    RAISE NOTICE 'Open shape violations: %', v_violations;

    SELECT count(*) INTO v_derivations
      FROM donto_statement
     WHERE context = 'ctx:derivation/epistemic-sweep'
       AND upper(tx_time) IS NULL;
    RAISE NOTICE 'Derived statements (this sweep): %', v_derivations;

    SELECT count(*) INTO v_arguments
      FROM donto_argument
     WHERE context = 'ctx:epistemic-sweep/contradictions'
       AND upper(tx_time) IS NULL;
    RAISE NOTICE 'Contradiction arguments (this sweep): %', v_arguments;

    SELECT count(*) INTO v_obligations
      FROM donto_proof_obligation
     WHERE context = 'ctx:epistemic-sweep/obligations'
       AND status = 'open';
    RAISE NOTICE 'Open proof obligations (this sweep): %', v_obligations;

    RAISE NOTICE 'Maturity distribution:';
    FOR v_mat IN
        SELECT ((flags >> 2) & 7) AS maturity,
               count(*) AS cnt
          FROM (SELECT * FROM donto_statement TABLESAMPLE BERNOULLI (1)) s
         WHERE upper(s.tx_time) IS NULL
         GROUP BY 1
         ORDER BY 1
    LOOP
        RAISE NOTICE '  L%: ~% (1%% sample)', v_mat.maturity, v_mat.cnt * 100;
    END LOOP;

    RAISE NOTICE '=== EPISTEMIC SWEEP COMPLETE ===';
END $$;

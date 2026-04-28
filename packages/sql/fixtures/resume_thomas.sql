-- Thomas Davis's resume, ingested as donto statements.
-- Used by scripts/demo-resume.sh + the Lean roleFit shape.
--
-- Strategy:
--   * One context per source: the resume itself; each hypothetical job;
--     a curated "candidate profile" rollup.
--   * Skills become first-class IRIs (ex:skill/<name>).
--   * Work entries are event-nodes (ex:work/<slug>).
--   * `ex:hasSkill`, `ex:requiresSkill` are the predicates the Lean
--     roleFit shape walks.

-- 0. Contexts.
SELECT donto_ensure_context('ctx:resume/thomas',           'source',     'permissive', NULL);
SELECT donto_ensure_context('ctx:job/anthropic-pe',        'source',     'permissive', NULL);
SELECT donto_ensure_context('ctx:job/vercel-de',           'source',     'permissive', NULL);
SELECT donto_ensure_context('ctx:job/supabase-pe',         'source',     'permissive', NULL);
SELECT donto_ensure_context('ctx:job/genealogy-startup',   'source',     'permissive', NULL);
SELECT donto_ensure_context('ctx:job/citadel-quant',       'source',     'permissive', NULL);

-- 1. Candidate identity.
SELECT donto_assert('ex:thomas', 'rdf:type',     'ex:Candidate', NULL, 'ctx:resume/thomas', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:thomas', 'rdfs:label',   NULL, '{"v":"Thomas Davis","dt":"xsd:string"}'::jsonb,        'ctx:resume/thomas', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:thomas', 'ex:label',     NULL, '{"v":"Full Stack Developer & AI Engineer","dt":"xsd:string"}'::jsonb, 'ctx:resume/thomas', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:thomas', 'ex:location',  NULL, '{"v":"Melbourne, AU","dt":"xsd:string"}'::jsonb,       'ctx:resume/thomas', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:thomas', 'ex:email',     NULL, '{"v":"thomasalwyndavis@gmail.com","dt":"xsd:string"}'::jsonb, 'ctx:resume/thomas', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:thomas', 'ex:website',   NULL, '{"v":"https://ajaxdavis.dev","dt":"xsd:string"}'::jsonb, 'ctx:resume/thomas', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:thomas', 'ex:githubHandle', NULL, '{"v":"thomasdavis","dt":"xsd:string"}'::jsonb,      'ctx:resume/thomas', 'asserted', 1, NULL, NULL, NULL);

-- Years of relevant experience: started professionally circa 2011 → 15 years.
SELECT donto_assert('ex:thomas', 'ex:yearsOfExperience', NULL, '{"v":15,"dt":"xsd:integer"}'::jsonb, 'ctx:resume/thomas', 'asserted', 1, NULL, NULL, NULL);

-- 2. Skills (every keyword from every group). Each is a real IRI so jobs
--    can require them by reference.
DO $$
DECLARE
    s text;
    skills text[] := ARRAY[
        -- AI & LLMs
        'vercel-ai-sdk','openai-api','anthropic-api','google-gemini','structured-outputs',
        'embeddings','rag-pipelines','ai-agents','mcp','prompt-engineering',
        'llm-training-from-scratch','autograd','tensor-operations',
        -- Frontend
        'react','nextjs','typescript','javascript','jsx','tailwind','styled-components',
        'redux','apollo-graphql','electron','threejs','backbone',
        -- Backend
        'nodejs','go','python','ruby-on-rails','postgresql','mongodb','redis','supabase',
        'rest-apis','websockets','deno',
        -- Cloud & DevOps
        'gcp','kubernetes','aws','docker','vercel','railway','heroku','github-actions',
        'cicd','serverless','cloudflare',
        -- Architecture & Tooling
        'monorepos','system-design','api-design','microservices','realtime-systems',
        'cli-tools','npm-publishing','open-source-maintenance',
        -- Low-level & GPU
        'c','vulkan','spir-v','gpu-compute','mixed-precision-fp16',
        -- Inferred from projects
        'lean4','formal-methods','postgres-extensions','knowledge-graphs'
    ];
BEGIN
    FOREACH s IN ARRAY skills LOOP
        PERFORM donto_assert(
            format('ex:skill/%s', s), 'rdf:type', 'ex:Skill', NULL,
            'ctx:resume/thomas', 'asserted', 1, NULL, NULL, NULL);
        PERFORM donto_assert(
            'ex:thomas', 'ex:hasSkill', format('ex:skill/%s', s), NULL,
            'ctx:resume/thomas', 'asserted', 1, NULL, NULL, NULL);
    END LOOP;
END$$;

-- 3. Work history (event-node pattern).
SELECT donto_assert('ex:work/misc-ai',       'rdf:type', 'ex:Work', NULL, 'ctx:resume/thomas', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:work/misc-ai',       'ex:role',  NULL, '{"v":"Product Engineer (AI Focus)","dt":"xsd:string"}'::jsonb, 'ctx:resume/thomas', 'asserted', 1, '2020-05-05', NULL, NULL);
SELECT donto_assert('ex:thomas',             'ex:worked', 'ex:work/misc-ai',  NULL, 'ctx:resume/thomas', 'asserted', 1, '2020-05-05', NULL, NULL);

SELECT donto_assert('ex:work/tokenized',     'rdf:type', 'ex:Work', NULL, 'ctx:resume/thomas', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:work/tokenized',     'ex:role',  NULL, '{"v":"Senior Javascript Developer","dt":"xsd:string"}'::jsonb, 'ctx:resume/thomas', 'asserted', 1, '2020-05-05','2021-05-05', NULL);
SELECT donto_assert('ex:thomas',             'ex:worked','ex:work/tokenized', NULL, 'ctx:resume/thomas', 'asserted', 1, '2020-05-05','2021-05-05', NULL);

SELECT donto_assert('ex:work/blockbid',      'rdf:type', 'ex:Work', NULL, 'ctx:resume/thomas', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:work/blockbid',      'ex:role',  NULL, '{"v":"Senior Javascript Developer","dt":"xsd:string"}'::jsonb, 'ctx:resume/thomas', 'asserted', 1, '2018-03-01','2020-01-01', NULL);
SELECT donto_assert('ex:thomas',             'ex:worked','ex:work/blockbid', NULL, 'ctx:resume/thomas', 'asserted', 1, '2018-03-01','2020-01-01', NULL);

SELECT donto_assert('ex:work/listium',       'rdf:type', 'ex:Work', NULL, 'ctx:resume/thomas', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:work/listium',       'ex:role',  NULL, '{"v":"Developer","dt":"xsd:string"}'::jsonb, 'ctx:resume/thomas', 'asserted', 1, '2016-01-01','2018-01-01', NULL);
SELECT donto_assert('ex:thomas',             'ex:worked','ex:work/listium', NULL, 'ctx:resume/thomas', 'asserted', 1, '2016-01-01','2018-01-01', NULL);

SELECT donto_assert('ex:work/eff',           'rdf:type', 'ex:Work', NULL, 'ctx:resume/thomas', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:work/eff',           'ex:role',  NULL, '{"v":"Developer","dt":"xsd:string"}'::jsonb, 'ctx:resume/thomas', 'asserted', 1, '2014-04-01','2016-01-01', NULL);
SELECT donto_assert('ex:thomas',             'ex:worked','ex:work/eff', NULL, 'ctx:resume/thomas', 'asserted', 1, '2014-04-01','2016-01-01', NULL);

SELECT donto_assert('ex:work/earbits',       'rdf:type', 'ex:Work', NULL, 'ctx:resume/thomas', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:work/earbits',       'ex:role',  NULL, '{"v":"CTO","dt":"xsd:string"}'::jsonb, 'ctx:resume/thomas', 'asserted', 1, '2013-03-08','2015-01-09', NULL);
SELECT donto_assert('ex:thomas',             'ex:worked','ex:work/earbits', NULL, 'ctx:resume/thomas', 'asserted', 1, '2013-03-08','2015-01-09', NULL);

-- Marker statements donto can use as evidence: leadership.
SELECT donto_assert('ex:thomas', 'ex:hasHeldRole', NULL, '{"v":"CTO","dt":"xsd:string"}'::jsonb, 'ctx:resume/thomas', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:thomas', 'ex:hasHeldRole', NULL, '{"v":"Founder","dt":"xsd:string"}'::jsonb, 'ctx:resume/thomas', 'asserted', 1, NULL, NULL, NULL);

-- 4. Notable projects → marker IRIs.
SELECT donto_assert('ex:project/jsonresume', 'rdf:type', 'ex:Project', NULL, 'ctx:resume/thomas', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:project/jsonresume', 'ex:githubStars', NULL, '{"v":5000,"dt":"xsd:integer"}'::jsonb, 'ctx:resume/thomas', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:thomas',             'ex:built', 'ex:project/jsonresume', NULL, 'ctx:resume/thomas', 'asserted', 1, NULL, NULL, NULL);

SELECT donto_assert('ex:project/cdnjs',      'rdf:type', 'ex:Project', NULL, 'ctx:resume/thomas', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:project/cdnjs',      'ex:librariesServed', NULL, '{"v":3000,"dt":"xsd:integer"}'::jsonb, 'ctx:resume/thomas', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:thomas',             'ex:built', 'ex:project/cdnjs', NULL, 'ctx:resume/thomas', 'asserted', 1, NULL, NULL, NULL);

SELECT donto_assert('ex:project/omega',      'rdf:type', 'ex:Project', NULL, 'ctx:resume/thomas', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:project/omega',      'ex:toolCount', NULL, '{"v":80,"dt":"xsd:integer"}'::jsonb, 'ctx:resume/thomas', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:thomas',             'ex:built', 'ex:project/omega', NULL, 'ctx:resume/thomas', 'asserted', 1, NULL, NULL, NULL);

SELECT donto_assert('ex:project/alpha',      'rdf:type', 'ex:Project', NULL, 'ctx:resume/thomas', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:project/alpha',      'ex:tagline', NULL, '{"v":"GPT training in TypeScript with Vulkan GPU, no PyTorch","dt":"xsd:string"}'::jsonb, 'ctx:resume/thomas', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:thomas',             'ex:built', 'ex:project/alpha', NULL, 'ctx:resume/thomas', 'asserted', 1, NULL, NULL, NULL);

SELECT donto_assert('ex:project/tpmjs',      'rdf:type', 'ex:Project', NULL, 'ctx:resume/thomas', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:thomas',             'ex:built', 'ex:project/tpmjs', NULL, 'ctx:resume/thomas', 'asserted', 1, NULL, NULL, NULL);

SELECT donto_assert('ex:project/blocks',     'rdf:type', 'ex:Project', NULL, 'ctx:resume/thomas', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:thomas',             'ex:built', 'ex:project/blocks', NULL, 'ctx:resume/thomas', 'asserted', 1, NULL, NULL, NULL);

-- 5. Awards.
SELECT donto_assert('ex:award/eff-defender', 'rdf:type', 'ex:Award', NULL, 'ctx:resume/thomas', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:award/eff-defender', 'rdfs:label', NULL, '{"v":"Defender of the Internet","dt":"xsd:string"}'::jsonb, 'ctx:resume/thomas', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:thomas',             'ex:awarded', 'ex:award/eff-defender', NULL, 'ctx:resume/thomas', 'asserted', 1, NULL, NULL, NULL);

-- 6. References (paraconsistent! multiple sources, all preserved).
SELECT donto_assert('ex:thomas', 'ex:peerSays', NULL, '{"v":"one of those A Players you hear of companies dying to hire — Joey Flores, CEO Earbits","dt":"xsd:string"}'::jsonb, 'ctx:resume/thomas', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:thomas', 'ex:peerSays', NULL, '{"v":"saved our company by quickly stepping up to fill CTO — Yotam Rosenbaum","dt":"xsd:string"}'::jsonb, 'ctx:resume/thomas', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:thomas', 'ex:peerSays', NULL, '{"v":"thought leader in the front-end community — Ryan Kirkman","dt":"xsd:string"}'::jsonb, 'ctx:resume/thomas', 'asserted', 1, NULL, NULL, NULL);

-- =============================================================================
-- Hypothetical job postings. Each is its own context so they can be queried
-- independently. ex:requiresSkill is the predicate the Lean roleFit walks.
-- =============================================================================

-- A) Anthropic-style Product Engineer (AI).
SELECT donto_assert('ex:job/anthropic-pe', 'rdf:type', 'ex:Job', NULL, 'ctx:job/anthropic-pe', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:job/anthropic-pe', 'rdfs:label', NULL, '{"v":"Product Engineer, AI Products","dt":"xsd:string"}'::jsonb, 'ctx:job/anthropic-pe', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:job/anthropic-pe', 'ex:requiresSkill', 'ex:skill/typescript',     NULL, 'ctx:job/anthropic-pe', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:job/anthropic-pe', 'ex:requiresSkill', 'ex:skill/react',          NULL, 'ctx:job/anthropic-pe', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:job/anthropic-pe', 'ex:requiresSkill', 'ex:skill/nextjs',         NULL, 'ctx:job/anthropic-pe', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:job/anthropic-pe', 'ex:requiresSkill', 'ex:skill/anthropic-api',  NULL, 'ctx:job/anthropic-pe', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:job/anthropic-pe', 'ex:requiresSkill', 'ex:skill/mcp',            NULL, 'ctx:job/anthropic-pe', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:job/anthropic-pe', 'ex:requiresSkill', 'ex:skill/prompt-engineering', NULL, 'ctx:job/anthropic-pe', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:job/anthropic-pe', 'ex:minYears', NULL, '{"v":5,"dt":"xsd:integer"}'::jsonb, 'ctx:job/anthropic-pe', 'asserted', 1, NULL, NULL, NULL);

-- B) Vercel-style Developer Experience.
SELECT donto_assert('ex:job/vercel-de', 'rdf:type', 'ex:Job', NULL, 'ctx:job/vercel-de', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:job/vercel-de', 'rdfs:label', NULL, '{"v":"Developer Experience Engineer","dt":"xsd:string"}'::jsonb, 'ctx:job/vercel-de', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:job/vercel-de', 'ex:requiresSkill', 'ex:skill/vercel-ai-sdk', NULL, 'ctx:job/vercel-de', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:job/vercel-de', 'ex:requiresSkill', 'ex:skill/nextjs',        NULL, 'ctx:job/vercel-de', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:job/vercel-de', 'ex:requiresSkill', 'ex:skill/typescript',    NULL, 'ctx:job/vercel-de', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:job/vercel-de', 'ex:requiresSkill', 'ex:skill/cli-tools',     NULL, 'ctx:job/vercel-de', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:job/vercel-de', 'ex:requiresSkill', 'ex:skill/open-source-maintenance', NULL, 'ctx:job/vercel-de', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:job/vercel-de', 'ex:minYears', NULL, '{"v":3,"dt":"xsd:integer"}'::jsonb, 'ctx:job/vercel-de', 'asserted', 1, NULL, NULL, NULL);

-- C) Supabase-style Product Engineer.
SELECT donto_assert('ex:job/supabase-pe', 'rdf:type', 'ex:Job', NULL, 'ctx:job/supabase-pe', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:job/supabase-pe', 'rdfs:label', NULL, '{"v":"Product Engineer (Postgres + AI)","dt":"xsd:string"}'::jsonb, 'ctx:job/supabase-pe', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:job/supabase-pe', 'ex:requiresSkill', 'ex:skill/postgresql', NULL, 'ctx:job/supabase-pe', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:job/supabase-pe', 'ex:requiresSkill', 'ex:skill/supabase',   NULL, 'ctx:job/supabase-pe', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:job/supabase-pe', 'ex:requiresSkill', 'ex:skill/typescript', NULL, 'ctx:job/supabase-pe', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:job/supabase-pe', 'ex:requiresSkill', 'ex:skill/embeddings', NULL, 'ctx:job/supabase-pe', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:job/supabase-pe', 'ex:requiresSkill', 'ex:skill/postgres-extensions', NULL, 'ctx:job/supabase-pe', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:job/supabase-pe', 'ex:minYears', NULL, '{"v":5,"dt":"xsd:integer"}'::jsonb, 'ctx:job/supabase-pe', 'asserted', 1, NULL, NULL, NULL);

-- D) Genealogy / knowledge-graph startup. Pretty obviously the donto fit.
SELECT donto_assert('ex:job/genealogy-startup', 'rdf:type', 'ex:Job', NULL, 'ctx:job/genealogy-startup', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:job/genealogy-startup', 'rdfs:label', NULL, '{"v":"Founding Engineer, Genealogy / Knowledge Graph","dt":"xsd:string"}'::jsonb, 'ctx:job/genealogy-startup', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:job/genealogy-startup', 'ex:requiresSkill', 'ex:skill/knowledge-graphs', NULL, 'ctx:job/genealogy-startup', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:job/genealogy-startup', 'ex:requiresSkill', 'ex:skill/postgresql',       NULL, 'ctx:job/genealogy-startup', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:job/genealogy-startup', 'ex:requiresSkill', 'ex:skill/typescript',       NULL, 'ctx:job/genealogy-startup', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:job/genealogy-startup', 'ex:requiresSkill', 'ex:skill/anthropic-api',    NULL, 'ctx:job/genealogy-startup', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:job/genealogy-startup', 'ex:requiresSkill', 'ex:skill/lean4',            NULL, 'ctx:job/genealogy-startup', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:job/genealogy-startup', 'ex:requiresSkill', 'ex:skill/formal-methods',   NULL, 'ctx:job/genealogy-startup', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:job/genealogy-startup', 'ex:minYears', NULL, '{"v":7,"dt":"xsd:integer"}'::jsonb, 'ctx:job/genealogy-startup', 'asserted', 1, NULL, NULL, NULL);

-- E) Citadel-style quant. Deliberately a bad fit so the demo shows misses.
SELECT donto_assert('ex:job/citadel-quant', 'rdf:type', 'ex:Job', NULL, 'ctx:job/citadel-quant', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:job/citadel-quant', 'rdfs:label', NULL, '{"v":"Quantitative Developer, HFT","dt":"xsd:string"}'::jsonb, 'ctx:job/citadel-quant', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:job/citadel-quant', 'ex:requiresSkill', 'ex:skill/c',           NULL, 'ctx:job/citadel-quant', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:job/citadel-quant', 'ex:requiresSkill', 'ex:skill/cpp',         NULL, 'ctx:job/citadel-quant', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:job/citadel-quant', 'ex:requiresSkill', 'ex:skill/rust',        NULL, 'ctx:job/citadel-quant', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:job/citadel-quant', 'ex:requiresSkill', 'ex:skill/kdb',         NULL, 'ctx:job/citadel-quant', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:job/citadel-quant', 'ex:requiresSkill', 'ex:skill/options-pricing', NULL, 'ctx:job/citadel-quant', 'asserted', 1, NULL, NULL, NULL);
SELECT donto_assert('ex:job/citadel-quant', 'ex:minYears', NULL, '{"v":5,"dt":"xsd:integer"}'::jsonb, 'ctx:job/citadel-quant', 'asserted', 1, NULL, NULL, NULL);

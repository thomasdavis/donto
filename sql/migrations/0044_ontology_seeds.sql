-- Ontology seeds: register standard predicates across domains.
--
-- These are the predicates that any extraction pipeline will need.
-- Domain-specific predicates (genealogy, salon, etc.) stay in their
-- own seed files or are implicitly registered in permissive contexts.

-- ── Schema.org / RDF / FOAF ──────────────────────────────

select donto_register_predicate('rdf:type', 'type', 'Instance-of relationship');
select donto_register_predicate('rdfs:label', 'label', 'Human-readable label');
select donto_register_predicate('rdfs:comment', 'comment', 'Human-readable description');
select donto_register_predicate('schema:name', 'name', 'Name of an entity');
select donto_register_predicate('schema:author', 'author', 'Author of a work');
select donto_register_predicate('schema:datePublished', 'datePublished', 'Publication date', null, null, null, 'xsd:date');
select donto_register_predicate('schema:license', 'license', 'License terms');
select donto_register_predicate('schema:citation', 'citation', 'Citation link between works');
select donto_register_predicate('schema:url', 'url', 'URL of a resource');
select donto_register_predicate('foaf:name', 'name', 'Full name of a person');

-- ── ML / AI domain ───────────────────────────────────────

-- Model properties
select donto_register_predicate('ml:parameterCount', 'parameterCount', 'Number of model parameters', null, null, null, 'xsd:integer');
select donto_register_predicate('ml:architecture', 'architecture', 'Model architecture');
select donto_register_predicate('ml:baseModel', 'baseModel', 'Base model that was fine-tuned');
select donto_register_predicate('ml:dModel', 'dModel', 'Model dimension', null, null, null, 'xsd:integer');
select donto_register_predicate('ml:nLayers', 'nLayers', 'Number of layers', null, null, null, 'xsd:integer');
select donto_register_predicate('ml:nHeads', 'nHeads', 'Number of attention heads', null, null, null, 'xsd:integer');
select donto_register_predicate('ml:nKvHeads', 'nKvHeads', 'Number of key-value heads', null, null, null, 'xsd:integer');
select donto_register_predicate('ml:dFF', 'dFF', 'Feed-forward dimension', null, null, null, 'xsd:integer');
select donto_register_predicate('ml:headDim', 'headDim', 'Per-head dimension', null, null, null, 'xsd:integer');
select donto_register_predicate('ml:hiddenDim', 'hiddenDim', 'Hidden dimension', null, null, null, 'xsd:integer');
select donto_register_predicate('ml:windowSize', 'windowSize', 'Attention window size', null, null, null, 'xsd:integer');
select donto_register_predicate('ml:contextLength', 'contextLength', 'Maximum context length', null, null, null, 'xsd:integer');
select donto_register_predicate('ml:vocabSize', 'vocabSize', 'Vocabulary size', null, null, null, 'xsd:integer');
select donto_register_predicate('ml:usesAttention', 'usesAttention', 'Attention mechanism used');
select donto_register_predicate('ml:usesTechnique', 'usesTechnique', 'Technique used');
select donto_register_predicate('ml:trainingTime', 'trainingTime', 'Training time description');
select donto_register_predicate('ml:trainingEfficiency', 'trainingEfficiency', 'Training efficiency claim');
select donto_register_predicate('ml:domain', 'domain', 'Domain of specialization');
select donto_register_predicate('ml:tunedOn', 'tunedOn', 'Training data domain');
select donto_register_predicate('ml:evaluatedModel', 'evaluatedModel', 'Model that was evaluated');
select donto_register_predicate('ml:reliesOn', 'reliesOn', 'Core dependency');
select donto_register_predicate('ml:dispensesWithRecurrence', 'dispensesWithRecurrence', null, null, null, null, 'xsd:boolean');
select donto_register_predicate('ml:dispensesWithConvolutions', 'dispensesWithConvolutions', null, null, null, null, 'xsd:boolean');
select donto_register_predicate('ml:variant', 'variant', 'Named variant of a technique');

-- Benchmarks
select donto_register_predicate('ml:benchmark', 'benchmark', 'Benchmark used for evaluation');
select donto_register_predicate('ml:score', 'score', 'Numeric benchmark result', null, null, null, 'xsd:decimal');
select donto_register_predicate('ml:scoreUnit', 'scoreUnit', 'Unit of measurement for score');
select donto_register_predicate('ml:evaluationSetting', 'evaluationSetting', 'Evaluation protocol details');
select donto_register_predicate('ml:reportedIn', 'reportedIn', 'Paper that reported this result');
select donto_register_predicate('ml:benchmarkType', 'benchmarkType', 'Type of benchmark');
select donto_register_predicate('ml:testCaseCount', 'testCaseCount', 'Number of test cases', null, null, null, 'xsd:integer');

-- Comparisons
select donto_register_predicate('ml:delta', 'delta', 'Numeric difference', null, null, null, 'xsd:decimal');
select donto_register_predicate('ml:relation', 'relation', 'Comparison outcome');
select donto_register_predicate('ml:leftResult', 'leftResult', 'Left side of comparison');
select donto_register_predicate('ml:rightResult', 'rightResult', 'Right side of comparison');

-- Claims
select donto_register_predicate('ml:outperforms', 'outperforms', 'Model X outperforms model Y');
select donto_register_predicate('ml:outperformsOn', 'outperformsOn', 'Scoped outperformance description');
select donto_register_predicate('ml:outperformsAllBenchmarks', 'outperformsAllBenchmarks', 'Outperforms on every benchmark');
select donto_register_predicate('ml:outperformsMostBenchmarks', 'outperformsMostBenchmarks', 'Outperforms on most benchmarks');
select donto_register_predicate('ml:approachesPerformance', 'approachesPerformance', 'Nearly matches performance');
select donto_register_predicate('ml:surpassesHumanExperts', 'surpassesHumanExperts', 'Exceeds human expert performance');
select donto_register_predicate('ml:achievesSOTA', 'achievesSOTA', 'State-of-the-art claim');
select donto_register_predicate('ml:introducedBy', 'introducedBy', 'Paper that introduced this');
select donto_register_predicate('ml:generalizes', 'generalizes', 'Generalizes to other tasks');
select donto_register_predicate('ml:finding', 'finding', 'Key research finding');
select donto_register_predicate('ml:guardrailResult', 'guardrailResult', 'Safety guardrail result');
select donto_register_predicate('ml:moderationPrecision', 'moderationPrecision', null, null, null, null, 'xsd:decimal');
select donto_register_predicate('ml:moderationRecall', 'moderationRecall', null, null, null, null, 'xsd:decimal');

-- ── Physics domain ───────────────────────────────────────

select donto_register_predicate('physics:measuredQuantity', 'measuredQuantity', 'Physical quantity measured');
select donto_register_predicate('physics:value', 'value', 'Measurement value', null, null, null, 'xsd:decimal');
select donto_register_predicate('physics:unit', 'unit', 'Measurement unit');
select donto_register_predicate('physics:target', 'target', 'Target of measurement');
select donto_register_predicate('physics:shell', 'shell', 'Electron shell');
select donto_register_predicate('physics:condition', 'condition', 'Experimental condition');
select donto_register_predicate('physics:facility', 'facility', 'Research facility');
select donto_register_predicate('physics:technique', 'technique', 'Experimental technique');
select donto_register_predicate('physics:finding', 'finding', 'Physics finding');
select donto_register_predicate('physics:method', 'method', 'Experimental method');
select donto_register_predicate('physics:implication', 'implication', 'Scientific implication');
select donto_register_predicate('physics:valueConverted', 'valueConverted', 'Value in alternative units');
select donto_register_predicate('physics:valueInFemtoseconds', 'valueInFemtoseconds', null, null, null, null, 'xsd:decimal');
select donto_register_predicate('physics:atomicComposition', 'atomicComposition', 'Atomic formula');

-- ── Genealogy domain ─────────────────────────────────────

select donto_register_predicate('ex:birthYear', 'birthYear', 'Year of birth', null, null, null, 'xsd:integer');
select donto_register_predicate('ex:deathYear', 'deathYear', 'Year of death', null, null, null, 'xsd:integer');
select donto_register_predicate('ex:birthPlace', 'birthPlace', 'Place of birth');
select donto_register_predicate('ex:deathPlace', 'deathPlace', 'Place of death');
select donto_register_predicate('ex:parentOf', 'parentOf', 'Parent-child relationship');
select donto_register_predicate('ex:childOf', 'childOf', 'Child-parent relationship', 'ex:parentOf');
select donto_register_predicate('ex:marriedTo', 'marriedTo', 'Marriage relationship');
select donto_register_predicate('ex:gender', 'gender', 'Gender');
select donto_register_predicate('ex:occupation', 'occupation', 'Occupation');
select donto_register_predicate('ex:knownAs', 'knownAs', 'Alternate name or alias');
select donto_register_predicate('ex:name', 'name', 'Name');

-- ── Genealogy relationship predicates with metadata ──────

update donto_predicate set is_symmetric = true where iri = 'ex:marriedTo';
update donto_predicate set inverse_of = 'ex:childOf' where iri = 'ex:parentOf';
update donto_predicate set inverse_of = 'ex:parentOf' where iri = 'ex:childOf';

-- ── General-purpose world knowledge ──────────────────────

select donto_register_predicate('geo:lat', 'latitude', null, null, null, null, 'xsd:decimal');
select donto_register_predicate('geo:long', 'longitude', null, null, null, null, 'xsd:decimal');
select donto_register_predicate('geo:country', 'country', 'Country');
select donto_register_predicate('geo:city', 'city', 'City');
select donto_register_predicate('geo:region', 'region', 'Region or state');
select donto_register_predicate('org:memberOf', 'memberOf', 'Membership in organization');
select donto_register_predicate('org:headOf', 'headOf', 'Heads an organization');
select donto_register_predicate('org:foundedIn', 'foundedIn', 'Founding year', null, null, null, 'xsd:integer');
select donto_register_predicate('event:date', 'eventDate', 'Date of event', null, null, null, 'xsd:date');
select donto_register_predicate('event:location', 'eventLocation', 'Location of event');
select donto_register_predicate('event:participant', 'participant', 'Participant in event');
select donto_register_predicate('event:outcome', 'outcome', 'Outcome of event');

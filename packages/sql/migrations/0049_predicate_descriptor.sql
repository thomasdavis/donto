-- Predicate descriptors: rich metadata for candidate matching.
--
-- A predicate descriptor pairs an IRI with a human-readable label, gloss,
-- domain hints (subject/object types, namespace), an example triple, a
-- prototypical source sentence, cardinality, and a vector embedding of the
-- gloss for semantic-similarity retrieval.
--
-- The descriptor embedding lets extraction pipelines find candidate
-- predicates by meaning, not by string match. Combined with the alignment
-- table (0048) and the closure index (0051), this is what closes the loop
-- from "free-text relation" → "registered predicate".

create table if not exists donto_predicate_descriptor (
    iri              text primary key,
    label            text not null,
    gloss            text,
    subject_type     text,
    object_type      text,
    domain           text,
    example_subject  text,
    example_object   text,
    source_sentence  text,
    cardinality      text check (cardinality is null or cardinality in (
        'one_to_one', 'one_to_many', 'many_to_one', 'many_to_many'
    )),
    embedding_model  text,
    embedding        float4[],
    metadata         jsonb not null default '{}'::jsonb,
    updated_at       timestamptz not null default now()
);

create index if not exists donto_pd_domain_idx
    on donto_predicate_descriptor (domain) where domain is not null;
create index if not exists donto_pd_subject_type_idx
    on donto_predicate_descriptor (subject_type) where subject_type is not null;
create index if not exists donto_pd_object_type_idx
    on donto_predicate_descriptor (object_type) where object_type is not null;

-- ---------------------------------------------------------------------------
-- Functions.
-- ---------------------------------------------------------------------------

create or replace function donto_upsert_descriptor(
    p_iri             text,
    p_label           text,
    p_gloss           text default null,
    p_subject_type    text default null,
    p_object_type     text default null,
    p_domain          text default null,
    p_example_subject text default null,
    p_example_object  text default null,
    p_source_sentence text default null,
    p_cardinality     text default null,
    p_embedding_model text default null,
    p_embedding       float4[] default null,
    p_metadata        jsonb default '{}'::jsonb
) returns text
language plpgsql as $$
begin
    perform donto_implicit_register(p_iri);
    insert into donto_predicate_descriptor
        (iri, label, gloss, subject_type, object_type, domain,
         example_subject, example_object, source_sentence,
         cardinality, embedding_model, embedding, metadata)
    values (p_iri, p_label, p_gloss, p_subject_type, p_object_type, p_domain,
            p_example_subject, p_example_object, p_source_sentence,
            p_cardinality, p_embedding_model, p_embedding, p_metadata)
    on conflict (iri) do update set
        label           = coalesce(excluded.label, donto_predicate_descriptor.label),
        gloss           = coalesce(excluded.gloss, donto_predicate_descriptor.gloss),
        subject_type    = coalesce(excluded.subject_type, donto_predicate_descriptor.subject_type),
        object_type     = coalesce(excluded.object_type, donto_predicate_descriptor.object_type),
        domain          = coalesce(excluded.domain, donto_predicate_descriptor.domain),
        example_subject = coalesce(excluded.example_subject, donto_predicate_descriptor.example_subject),
        example_object  = coalesce(excluded.example_object, donto_predicate_descriptor.example_object),
        source_sentence = coalesce(excluded.source_sentence, donto_predicate_descriptor.source_sentence),
        cardinality     = coalesce(excluded.cardinality, donto_predicate_descriptor.cardinality),
        embedding_model = coalesce(excluded.embedding_model, donto_predicate_descriptor.embedding_model),
        embedding       = coalesce(excluded.embedding, donto_predicate_descriptor.embedding),
        metadata        = donto_predicate_descriptor.metadata || excluded.metadata,
        updated_at      = now();
    return p_iri;
end;
$$;

create or replace function donto_nearest_predicates(
    p_query_embedding float4[],
    p_model_id        text,
    p_domain          text default null,
    p_subject_type    text default null,
    p_object_type     text default null,
    p_limit           int default 20
) returns table(
    iri        text,
    label      text,
    gloss      text,
    similarity double precision
)
language sql stable as $$
    select d.iri, d.label, d.gloss,
           donto_cosine_similarity(d.embedding, p_query_embedding) as similarity
    from donto_predicate_descriptor d
    where d.embedding is not null
      and d.embedding_model = p_model_id
      and array_length(d.embedding, 1) = array_length(p_query_embedding, 1)
      and (p_domain is null or d.domain = p_domain)
      and (p_subject_type is null or d.subject_type = p_subject_type)
      and (p_object_type is null or d.object_type = p_object_type)
    order by donto_cosine_similarity(d.embedding, p_query_embedding) desc nulls last
    limit p_limit
$$;

-- ---------------------------------------------------------------------------
-- Seed descriptors for predicates registered in migration 0044.
-- These cover the genealogy, geography, schema.org, and common ML domains.
-- Embeddings are populated later by an extraction-pipeline backfill job.
-- ---------------------------------------------------------------------------

-- Schema.org / RDF / FOAF
select donto_upsert_descriptor(
    'rdf:type', 'type',
    'Asserts that a subject is an instance of a class.',
    null, 'Class', 'rdf',
    'ex:marie-curie', 'ex:Person',
    'Marie Curie was a physicist.',
    'many_to_many');

select donto_upsert_descriptor(
    'rdfs:label', 'label',
    'A human-readable name for a resource.',
    null, null, 'rdfs',
    'ex:marie-curie', null,
    'The label "Marie Curie" identifies this person.',
    'many_to_many');

select donto_upsert_descriptor(
    'rdfs:comment', 'comment',
    'A human-readable description of a resource.',
    null, null, 'rdfs',
    null, null, null, 'one_to_many');

select donto_upsert_descriptor(
    'schema:name', 'name',
    'The human-readable name of an entity.',
    null, null, 'schema',
    'ex:marie-curie', null,
    'Her name was Marie Curie.',
    'many_to_many');

select donto_upsert_descriptor(
    'schema:author', 'author',
    'The author of a creative work.',
    'CreativeWork', 'Person', 'schema',
    'ex:relativity-paper', 'ex:einstein',
    'The paper was authored by Albert Einstein.',
    'many_to_many');

select donto_upsert_descriptor(
    'schema:datePublished', 'datePublished',
    'The date a work was first published.',
    'CreativeWork', null, 'schema',
    null, null, null, 'many_to_one');

select donto_upsert_descriptor(
    'schema:license', 'license',
    'License governing the use of a work.',
    'CreativeWork', null, 'schema',
    null, null, null, 'many_to_one');

select donto_upsert_descriptor(
    'schema:citation', 'citation',
    'A citation link from one work to another.',
    'CreativeWork', 'CreativeWork', 'schema',
    null, null, null, 'many_to_many');

select donto_upsert_descriptor(
    'schema:url', 'url',
    'The canonical URL of a resource.',
    null, null, 'schema',
    null, null, null, 'many_to_one');

select donto_upsert_descriptor(
    'foaf:name', 'name',
    'The full name of a person.',
    'Person', null, 'foaf',
    'ex:marie-curie', null,
    'Marie Curie was a physicist.',
    'many_to_one');

-- Genealogy
select donto_upsert_descriptor(
    'ex:birthYear', 'birthYear',
    'The year a person was born.',
    'Person', null, 'genealogy',
    'ex:marie-curie', '1867',
    'Marie Curie was born in 1867.',
    'many_to_one');

select donto_upsert_descriptor(
    'ex:deathYear', 'deathYear',
    'The year a person died.',
    'Person', null, 'genealogy',
    'ex:marie-curie', '1934',
    'Marie Curie died in 1934.',
    'many_to_one');

select donto_upsert_descriptor(
    'ex:birthPlace', 'birthPlace',
    'The place where a person was born.',
    'Person', 'Place', 'genealogy',
    'ex:marie-curie', 'ex:warsaw',
    'Marie Curie was born in Warsaw.',
    'many_to_one');

select donto_upsert_descriptor(
    'ex:deathPlace', 'deathPlace',
    'The place where a person died.',
    'Person', 'Place', 'genealogy',
    'ex:marie-curie', 'ex:passy',
    'Marie Curie died in Passy.',
    'many_to_one');

select donto_upsert_descriptor(
    'ex:parentOf', 'parentOf',
    'A parent-child relationship: subject is the parent of object.',
    'Person', 'Person', 'genealogy',
    'ex:pierre-curie', 'ex:irene-curie',
    'Pierre Curie was the father of Irene Curie.',
    'one_to_many');

select donto_upsert_descriptor(
    'ex:childOf', 'childOf',
    'A child-parent relationship: subject is the child of object.',
    'Person', 'Person', 'genealogy',
    'ex:irene-curie', 'ex:pierre-curie',
    'Irene Curie was the daughter of Pierre Curie.',
    'many_to_one');

select donto_upsert_descriptor(
    'ex:marriedTo', 'marriedTo',
    'A symmetric marriage relationship.',
    'Person', 'Person', 'genealogy',
    'ex:marie-curie', 'ex:pierre-curie',
    'Marie was married to Pierre Curie.',
    'one_to_one');

select donto_upsert_descriptor(
    'ex:gender', 'gender',
    'The gender identity of a person.',
    'Person', null, 'genealogy',
    null, null, null, 'many_to_one');

select donto_upsert_descriptor(
    'ex:occupation', 'occupation',
    'A profession or occupation held by a person.',
    'Person', null, 'genealogy',
    'ex:marie-curie', 'physicist',
    'Marie Curie was a physicist.',
    'many_to_many');

select donto_upsert_descriptor(
    'ex:knownAs', 'knownAs',
    'An alternate name or alias for a person.',
    'Person', null, 'genealogy',
    null, null, null, 'many_to_many');

select donto_upsert_descriptor(
    'ex:name', 'name',
    'The name of a person.',
    'Person', null, 'genealogy',
    null, null, null, 'many_to_many');

-- Geography
select donto_upsert_descriptor(
    'geo:lat', 'latitude',
    'Geographic latitude in decimal degrees.',
    'Place', null, 'geography',
    null, null, null, 'many_to_one');

select donto_upsert_descriptor(
    'geo:long', 'longitude',
    'Geographic longitude in decimal degrees.',
    'Place', null, 'geography',
    null, null, null, 'many_to_one');

select donto_upsert_descriptor(
    'geo:country', 'country',
    'The country in which a place is located.',
    'Place', 'Country', 'geography',
    'ex:warsaw', 'ex:poland',
    'Warsaw is in Poland.',
    'many_to_one');

select donto_upsert_descriptor(
    'geo:city', 'city',
    'The city in which a place is located.',
    'Place', 'City', 'geography',
    null, null, null, 'many_to_one');

select donto_upsert_descriptor(
    'geo:region', 'region',
    'The region or state in which a place is located.',
    'Place', 'Region', 'geography',
    null, null, null, 'many_to_one');

-- Organizations and events
select donto_upsert_descriptor(
    'org:memberOf', 'memberOf',
    'Subject is a member of an organization.',
    'Person', 'Organization', 'organization',
    'ex:marie-curie', 'ex:academy-of-sciences',
    'Marie Curie was a member of the Academy of Sciences.',
    'many_to_many');

select donto_upsert_descriptor(
    'org:headOf', 'headOf',
    'Subject leads or heads an organization.',
    'Person', 'Organization', 'organization',
    null, null, null, 'one_to_one');

select donto_upsert_descriptor(
    'org:foundedIn', 'foundedIn',
    'Year an organization was founded.',
    'Organization', null, 'organization',
    null, null, null, 'many_to_one');

select donto_upsert_descriptor(
    'event:date', 'eventDate',
    'The date on which an event occurred.',
    'Event', null, 'event',
    null, null, null, 'many_to_one');

select donto_upsert_descriptor(
    'event:location', 'eventLocation',
    'The place where an event occurred.',
    'Event', 'Place', 'event',
    null, null, null, 'many_to_one');

select donto_upsert_descriptor(
    'event:participant', 'participant',
    'A participant in an event.',
    'Event', 'Agent', 'event',
    null, null, null, 'many_to_many');

select donto_upsert_descriptor(
    'event:outcome', 'outcome',
    'The outcome or result of an event.',
    'Event', null, 'event',
    null, null, null, 'many_to_many');

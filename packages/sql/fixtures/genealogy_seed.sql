-- Tiny genealogy SQLite fixture matching the schema described in PRD §24.
-- Used by `crates/donto-migrate/tests/genealogy_e2e.rs` to verify the
-- migrator end-to-end against a real (small) database.
--
-- The schema is intentionally a subset of the real research.db — enough
-- to exercise every branch of the migrator (sources, entities, claims,
-- events, participants, relationships, aliases, discrepancies,
-- hypotheses, ingestion_log).

create table sources (
    id        text primary key,
    name      text,
    kind      text,
    citation  text
);

create table entities (
    id        text primary key,
    kind      text,
    label     text,
    source_id text references sources(id)
);

create table claims (
    subject_id text,
    predicate  text,
    object     text,
    source_id  text references sources(id),
    confidence text
);

create table events (
    id        text primary key,
    kind      text,
    date      text,
    place     text,
    source_id text references sources(id)
);

create table participants (
    event_id  text references events(id),
    entity_id text references entities(id),
    role      text
);

create table relationships (
    left_id    text,
    right_id   text,
    kind       text,
    confidence text
);

create table aliases (
    entity_id  text references entities(id),
    name       text,
    year_start integer,
    year_end   integer,
    location   text
);

create table discrepancies (
    id      text primary key,
    summary text,
    kind    text
);

create table hypotheses (
    id        text primary key,
    statement text,
    status    text
);

create table ingestion_log (
    id     text primary key,
    action text,
    at     text
);

-- Seed data. Two sources, three entities (alice young + alice old + bob),
-- a sameAs hypothesis between the two Alices, two events, an alias, a
-- discrepancy, and a few audit entries.

insert into sources (id, name, kind, citation) values
    ('src/wikipedia', 'Wikipedia',         'web',     'https://en.wikipedia.org/wiki/Alice'),
    ('src/census1900','UK Census 1900',    'archive', 'TNA/RG13/123/45');

insert into entities (id, kind, label, source_id) values
    ('alice_young', 'Person', 'Alice Brackenridge', 'src/wikipedia'),
    ('alice_old',   'Person', 'Alice Julian',       'src/census1900'),
    ('bob',         'Person', 'Bob Davis',          'src/census1900');

-- Three claims, one with strong confidence (→ maturity 1 in donto).
insert into claims (subject_id, predicate, object, source_id, confidence) values
    ('alice_young', 'ex:birthYear', '1899',                    'src/wikipedia',  'speculative'),
    ('alice_old',   'ex:birthYear', '1925',                    'src/census1900', 'strong'),
    ('bob',         'ex:occupation','blacksmith',              'src/census1900', 'moderate');

insert into events (id, kind, date, place, source_id) values
    ('ev_1900_marriage', 'Marriage', '1923-06-12', 'Glasgow', 'src/census1900'),
    ('ev_1925_birth',    'Birth',    '1925-04-03', 'Edinburgh','src/census1900');

insert into participants (event_id, entity_id, role) values
    ('ev_1900_marriage','alice_young','spouse'),
    ('ev_1900_marriage','bob',        'spouse'),
    ('ev_1925_birth',   'alice_old',  'subject');

-- Three relationships: a sameAs (probable identity) and a differentFrom.
insert into relationships (left_id, right_id, kind, confidence) values
    ('alice_young', 'alice_old',   'possiblySame',  'speculative'),
    ('alice_young', 'bob',         'differentFrom', 'strong'),
    ('alice_young', 'alice_old',   'sameAs',        'moderate');

insert into aliases (entity_id, name, year_start, year_end, location) values
    ('alice_young','Allie',   1900, 1920, 'Glasgow'),
    ('alice_old',  'A. Julian',1930,1960, 'Edinburgh');

insert into discrepancies (id, summary, kind) values
    ('disc_birth_year', 'alice has two birthYear claims',     'temporal'),
    ('disc_alias_loc',  'overlapping aliases for the alices', 'identity');

insert into hypotheses (id, statement, status) values
    ('hypo_alice_merge', 'alice_young and alice_old are the same person', 'open');

insert into ingestion_log (id, action, at) values
    ('log1','load_wikipedia','2026-01-01T00:00:00Z'),
    ('log2','load_census',   '2026-01-02T00:00:00Z'),
    ('log3','derive',        '2026-01-03T00:00:00Z');

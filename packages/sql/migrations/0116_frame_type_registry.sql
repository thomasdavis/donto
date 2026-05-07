-- Trust Kernel / FR-005 frame type registry.
--
-- Registers the canonical frame type vocabulary including the eighteen
-- language-pilot frame types from PRD §13.4 plus cross-domain types
-- (medical, legal, scientific, identity).
--
-- Each frame type declares its expected role names and their
-- requirement (required / optional). The donto_frame_role table
-- (migration 0106) stores actual role values; this registry tells
-- adapters and validators what shape to expect.

create table if not exists donto_frame_type (
    frame_type      text primary key,
    domain          text not null,
    description     text not null,
    required_roles  text[] not null default '{}',
    optional_roles  text[] not null default '{}',
    schema_version  text not null default 'frame-schema-1',
    is_active       boolean not null default true,
    metadata        jsonb not null default '{}'::jsonb
);

create index if not exists donto_frame_type_domain_idx
    on donto_frame_type (domain);

-- Validate a set of role names against a frame type.
create or replace function donto_validate_frame_roles(
    p_frame_type text,
    p_role_names text[]
) returns boolean
language plpgsql stable as $$
declare
    v_required text[];
    v_optional text[];
    v_role     text;
begin
    select required_roles, optional_roles
    into v_required, v_optional
    from donto_frame_type
    where frame_type = p_frame_type and is_active = true;

    if v_required is null then
        return false; -- Unknown frame type.
    end if;

    -- Every required role must be present.
    foreach v_role in array v_required loop
        if not (v_role = any(p_role_names)) then
            return false;
        end if;
    end loop;

    return true;
end;
$$;

-- Seed language-pilot frame types (PRD §13.4).
insert into donto_frame_type
    (frame_type, domain, description, required_roles, optional_roles) values
    ('phoneme_inventory', 'linguistics/phonology',
     'A language variety has a set of phonemes.',
     '{language_variety, phonemes}', '{source}'),
    ('phoneme_attestation', 'linguistics/phonology',
     'A specific phoneme is attested in a language variety.',
     '{language_variety, phoneme}', '{features, source, example}'),
    ('allophone_rule', 'linguistics/phonology',
     'A phoneme is realised by an allophone in an environment.',
     '{phoneme, allophone, environment}', '{language_variety, source}'),
    ('phonotactic_constraint', 'linguistics/phonology',
     'A phonological constraint on possible sequences.',
     '{language_variety, constraint}', '{example, source}'),
    ('morpheme_inventory', 'linguistics/morphology',
     'A language variety has a set of morphemes.',
     '{language_variety, morphemes}', '{category, source}'),
    ('allomorphy_rule', 'linguistics/morphology',
     'A morpheme is realised as an allomorph in an environment.',
     '{morpheme, allomorph, environment}', '{language_variety, source, example}'),
    ('paradigm_cell', 'linguistics/morphology',
     'A specific (lexeme, features) cell realised by a form.',
     '{lexeme, features, form}', '{language_variety, source}'),
    ('lexeme_entry', 'linguistics/lexicon',
     'A lexicon entry: lexeme with sense and forms.',
     '{lexeme, language_variety}', '{senses, forms, etymology, source}'),
    ('sense_mapping', 'linguistics/lexicon',
     'A sense maps to a concept in a concept inventory.',
     '{sense, concept}', '{confidence, source}'),
    ('interlinear_example', 'linguistics/syntax',
     'A glossed example: vernacular + segmentation + gloss + translation.',
     '{vernacular, gloss, translation}', '{segmentation, language_variety, source, anchor}'),
    ('construction_template', 'linguistics/syntax',
     'A grammatical-construction template with roles.',
     '{construction_id, roles}', '{language_variety, examples, source}'),
    ('valency_frame', 'linguistics/syntax',
     'A verb valency frame with argument roles and marking.',
     '{verb, arguments}', '{language_variety, alternations, source}'),
    ('argument_marking_pattern', 'linguistics/morphosyntax',
     'A pattern marking argument roles (case, agreement, etc.).',
     '{language_variety, pattern}', '{roles, examples, source}'),
    ('clause_type', 'linguistics/syntax',
     'A clause-type description with structural properties.',
     '{language_variety, clause_type}', '{constituents, examples, source}'),
    ('corpus_token_annotation', 'linguistics/corpus',
     'A token-level annotation in a corpus.',
     '{token_id, sentence_id, annotation}', '{features, source, anchor}'),
    ('dependency_edge', 'linguistics/corpus',
     'A syntactic dependency edge between tokens.',
     '{head, dependent, relation}', '{sentence_id, source}'),
    ('translation_alignment', 'linguistics/corpus',
     'An alignment between source-text segments and translation segments.',
     '{source_anchor, target_anchor}', '{language_pair, score, method}'),
    ('dialect_variant', 'linguistics/sociolinguistics',
     'A dialect variant of a feature within a language.',
     '{language_variety, feature, variant}', '{region, speakers, source}'),
    ('language_identity_hypothesis', 'linguistics/identity',
     'A hypothesis that two language IDs refer to the same variety.',
     '{candidate_a, candidate_b}', '{evidence, status, confidence}')
on conflict (frame_type) do update set
    description    = excluded.description,
    required_roles = excluded.required_roles,
    optional_roles = excluded.optional_roles;

-- Cross-domain frame types (deferred adapters but the schema is
-- ready when those domains land).
insert into donto_frame_type
    (frame_type, domain, description, required_roles, optional_roles) values
    ('diagnosis', 'medicine',
     'A clinical diagnosis with patient, condition, and confidence.',
     '{patient, condition}', '{confidence, evidence, source, date}'),
    ('legal_precedent', 'law',
     'A legal holding with court, case, jurisdiction, and ruling.',
     '{court, case, ruling}', '{jurisdiction, parties, date, source}'),
    ('experiment_result', 'science',
     'An experimental result with measurement, conditions, and units.',
     '{experiment, measurement}', '{conditions, units, n, p_value, source}'),
    ('clinical_observation', 'medicine',
     'A specific clinical observation with anchor.',
     '{patient, observation}', '{date, instrument, source, anchor}'),
    ('schema_mapping', 'cross_domain',
     'A schema mapping with relation and safety flags.',
     '{left, right, relation}', '{scope, evidence, safety_flags}'),
    ('access_policy_inheritance', 'governance',
     'A policy applied through derivation chain.',
     '{policy, derivation_chain}', '{authority, audit_ref}')
on conflict (frame_type) do nothing;

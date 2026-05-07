-- Trust Kernel / §7.4 modality overlay.
--
-- Modality describes the claim's relationship to the world:
--   descriptive | prescriptive | reconstructed | inferred | elicited
--   | corpus_observed | typological_summary | experimental_result
--   | clinical_observation | legal_holding | archival_metadata
--   | oral_history | community_protocol | model_output
--
-- Stored as an overlay table (sparse) so we don't widen donto_statement.

create table if not exists donto_stmt_modality (
    statement_id uuid primary key
                 references donto_statement(statement_id) on delete cascade,
    modality     text not null check (modality in (
        'descriptive', 'prescriptive', 'reconstructed', 'inferred',
        'elicited', 'corpus_observed', 'typological_summary',
        'experimental_result', 'clinical_observation', 'legal_holding',
        'archival_metadata', 'oral_history', 'community_protocol',
        'model_output', 'other'
    )),
    set_at       timestamptz not null default now(),
    set_by       text,
    metadata     jsonb not null default '{}'::jsonb
);

create index if not exists donto_stmt_modality_idx
    on donto_stmt_modality (modality);

create or replace function donto_set_modality(
    p_statement_id uuid,
    p_modality     text,
    p_set_by       text default null
) returns void
language plpgsql as $$
begin
    insert into donto_stmt_modality (statement_id, modality, set_by)
    values (p_statement_id, p_modality, p_set_by)
    on conflict (statement_id) do update set
        modality = excluded.modality,
        set_by   = excluded.set_by,
        set_at   = now();
end;
$$;

create or replace function donto_get_modality(p_statement_id uuid)
returns text
language sql stable as $$
    select modality from donto_stmt_modality where statement_id = p_statement_id
$$;

create or replace view donto_v_modality_v1000 as
    select * from (values
        ('descriptive',          'Source describes how the world is.'),
        ('prescriptive',         'Source prescribes how the world should be.'),
        ('reconstructed',        'Claim was reconstructed (e.g., proto-language).'),
        ('inferred',             'Claim was inferred analytically.'),
        ('elicited',             'Claim was elicited from a speaker / informant.'),
        ('corpus_observed',      'Claim is a corpus observation.'),
        ('typological_summary',  'Claim summarizes a feature across a language.'),
        ('experimental_result',  'Claim is an experimental result.'),
        ('clinical_observation', 'Claim is a clinical observation.'),
        ('legal_holding',        'Claim is a legal holding or precedent.'),
        ('archival_metadata',    'Claim is metadata from an archive record.'),
        ('oral_history',         'Claim is from oral testimony.'),
        ('community_protocol',   'Claim is a community protocol or rule.'),
        ('model_output',         'Claim was produced by a machine model.'),
        ('other',                'Other modality not in the standard list.')
    ) as t(modality, description);

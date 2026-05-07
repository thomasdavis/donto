-- Trust Kernel / §6.4 ClaimRecord claim_kind overlay.
--
-- The PRD §6.4 names eight claim kinds:
--   atomic | frame_summary | absence | identity | alignment
--   | policy | review | validation
--
-- "atomic" is the implicit default for an ordinary donto_statement.
-- Other kinds either summarize a frame (frame_summary) or live in
-- their own tables (alignment, policy, review). This overlay lets us
-- mark statement rows whose role differs from the default.

create table if not exists donto_stmt_claim_kind (
    statement_id uuid primary key
                 references donto_statement(statement_id) on delete cascade,
    claim_kind   text not null check (claim_kind in (
        'atomic', 'frame_summary', 'absence', 'identity',
        'alignment', 'policy', 'review', 'validation'
    )),
    set_at       timestamptz not null default now(),
    set_by       text,
    metadata     jsonb not null default '{}'::jsonb
);

create index if not exists donto_stmt_claim_kind_idx
    on donto_stmt_claim_kind (claim_kind);

create or replace function donto_set_claim_kind(
    p_statement_id uuid,
    p_claim_kind   text,
    p_set_by       text default null
) returns void
language plpgsql as $$
begin
    insert into donto_stmt_claim_kind (statement_id, claim_kind, set_by)
    values (p_statement_id, p_claim_kind, p_set_by)
    on conflict (statement_id) do update set
        claim_kind = excluded.claim_kind,
        set_by     = excluded.set_by,
        set_at     = now();
end;
$$;

create or replace function donto_get_claim_kind(p_statement_id uuid)
returns text
language sql stable as $$
    select coalesce(
        (select claim_kind from donto_stmt_claim_kind where statement_id = p_statement_id),
        'atomic'
    )
$$;

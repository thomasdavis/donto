-- Migration ledger. Phase 1 introduces durable tracking; earlier migrations
-- are backfilled idempotently because their DDL is `if not exists` shaped.

create table if not exists donto_migration (
    name        text primary key,
    applied_at  timestamptz not null default now(),
    sha256      bytea not null,
    notes       text
);

-- Backfill the prior three so re-running the migrator skips them.
insert into donto_migration (name, sha256, notes) values
    ('0001_core',      decode('00','hex'), 'phase 0 core schema'),
    ('0002_flags',     decode('00','hex'), 'phase 0 flag helpers'),
    ('0003_functions', decode('00','hex'), 'phase 0 function surface')
on conflict (name) do nothing;

create or replace function donto_version()
returns table(component text, version text, detail text)
language sql stable as $$
    select * from (values
        ('schema',     '0.1.0',  'phase 1 (extension foundation)'),
        ('atom',       '1',      'physical row + sparse overlays'),
        ('truth',      '1',      'polarity asserted/negated/absent/unknown; modality TBD'),
        ('bitemporal', '1',      'valid_time + tx_time; retraction closes tx_time'),
        ('contexts',   '1',      'forest, kind, mode')
    ) v(component, version, detail)
$$;

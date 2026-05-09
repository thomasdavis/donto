-- Telemetry analysis: detector finding table (PRD analytics deliverable C1).
--
-- Detectors write their findings here; the table is the sole sink.
-- target_kind discriminates the thing being flagged: 'rule', 'predicate_pair',
-- '_self' (detector self-metric). severity is one of info/warning/critical.

create table if not exists donto_detector_finding (
    finding_id    bigserial primary key,
    detector_iri  text not null,
    target_kind   text not null,        -- 'rule', 'predicate_pair', '_self'
    target_id     text not null,
    severity      text not null check (severity in ('info','warning','critical')),
    observed_at   timestamptz not null default now(),
    payload       jsonb not null default '{}'::jsonb
);

create index if not exists donto_detector_finding_target_idx
    on donto_detector_finding (detector_iri, target_kind, target_id, observed_at desc);

create index if not exists donto_detector_finding_severity_idx
    on donto_detector_finding (severity, observed_at desc);

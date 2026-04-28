-- Evidence substrate: unit registry and normalization.
--
-- A unit registry with SI base conversions so shapes and queries can
-- compare values across representations ("60.1%" vs "0.601").

create table if not exists donto_unit (
    iri           text primary key,
    label         text,
    symbol        text,
    dimension     text,
    si_base       text references donto_unit(iri),
    si_factor     double precision,
    metadata      jsonb not null default '{}'::jsonb
);

create index if not exists donto_unit_dimension_idx
    on donto_unit (dimension) where dimension is not null;

-- Seed common units
insert into donto_unit (iri, label, symbol, dimension, si_base, si_factor) values
    ('unit:ratio',       'ratio',       '',   'ratio',       null,           null),
    ('unit:accuracy',    'accuracy',    '',   'ratio',       'unit:ratio',   1.0),
    ('unit:percent',     'percent',     '%',  'ratio',       'unit:ratio',   0.01),
    ('unit:bleu',        'BLEU score',  '',   'score',       null,           null),
    ('unit:f1',          'F1 score',    '',   'ratio',       'unit:ratio',   1.0),
    ('unit:second',      'second',      's',  'time',        null,           null),
    ('unit:millisecond', 'millisecond', 'ms', 'time',        'unit:second',  1e-3),
    ('unit:microsecond', 'microsecond', 'us', 'time',        'unit:second',  1e-6),
    ('unit:nanosecond',  'nanosecond',  'ns', 'time',        'unit:second',  1e-9),
    ('unit:attosecond',  'attosecond',  'as', 'time',        'unit:second',  1e-18),
    ('unit:femtosecond', 'femtosecond', 'fs', 'time',        'unit:second',  1e-15),
    ('unit:meter',       'meter',       'm',  'length',      null,           null),
    ('unit:nanometer',   'nanometer',   'nm', 'length',      'unit:meter',   1e-9),
    ('unit:angstrom',    'angstrom',    'A',  'length',      'unit:meter',   1e-10),
    ('unit:kelvin',      'kelvin',      'K',  'temperature', null,           null),
    ('unit:celsius',     'celsius',     'C',  'temperature', 'unit:kelvin',  1.0),
    ('unit:ev',          'electronvolt','eV', 'energy',      null,           null),
    ('unit:joule',       'joule',       'J',  'energy',      null,           null),
    ('unit:usd',         'US dollar',   '$',  'currency',    null,           null),
    ('unit:eur',         'euro',        'E',  'currency',    null,           null),
    ('unit:year',        'year',        'yr', 'time',        'unit:second',  31557600),
    ('unit:day',         'day',         'd',  'time',        'unit:second',  86400),
    ('unit:hour',        'hour',        'h',  'time',        'unit:second',  3600),
    ('unit:kilogram',    'kilogram',    'kg', 'mass',        null,           null),
    ('unit:gram',        'gram',        'g',  'mass',        'unit:kilogram',0.001),
    ('unit:milligram',   'milligram',   'mg', 'mass',        'unit:kilogram',1e-6)
on conflict (iri) do nothing;

-- Convert a value between units. Returns null if conversion is
-- not possible (different dimensions or no SI path).
create or replace function donto_convert_unit(
    p_value     double precision,
    p_from_unit text,
    p_to_unit   text
) returns double precision
language plpgsql stable as $$
declare
    v_from donto_unit;
    v_to   donto_unit;
    v_si   double precision;
begin
    if p_from_unit = p_to_unit then return p_value; end if;

    select * into v_from from donto_unit where iri = p_from_unit;
    select * into v_to   from donto_unit where iri = p_to_unit;

    if v_from is null or v_to is null then return null; end if;
    if v_from.dimension is distinct from v_to.dimension then return null; end if;

    -- Convert to SI base, then to target
    if v_from.si_factor is not null then
        v_si := p_value * v_from.si_factor;
    else
        v_si := p_value;
    end if;

    if v_to.si_factor is not null and v_to.si_factor <> 0 then
        return v_si / v_to.si_factor;
    else
        return v_si;
    end if;
end;
$$;

-- Normalize a percentage string to a decimal. Handles "60.1%", "0.601",
-- "60.1 percent", etc.
create or replace function donto_normalize_percent(p_raw text)
returns double precision
language sql immutable as $$
    select case
        when p_raw like '%!%%' escape '!' then
            replace(replace(p_raw, '%', ''), ' ', '')::double precision / 100.0
        when lower(p_raw) like '%percent%' then
            regexp_replace(lower(p_raw), '[^0-9.]', '', 'g')::double precision / 100.0
        else
            p_raw::double precision
    end
$$;

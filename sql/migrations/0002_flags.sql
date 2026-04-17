-- Flag packing for donto_statement.flags (PRD §5).
-- Phase 0 packs polarity and maturity only; modality/confidence are overlay
-- tables in later phases.
--
-- Bit layout (smallint = 16 bits, signed but we treat low 5 bits only):
--   bits 0-1  polarity   0=asserted  1=negated  2=absent  3=unknown
--   bits 2-4  maturity   0..4 (PRD §2)
--   bits 5-15 reserved

create or replace function donto_pack_flags(polarity text, maturity int)
returns smallint
language sql immutable as $$
    select (
        (case lower(polarity)
            when 'asserted' then 0
            when 'negated'  then 1
            when 'absent'   then 2
            when 'unknown'  then 3
            else null
        end)
        | ((maturity & 7) << 2)
    )::smallint
$$;

create or replace function donto_polarity(flags smallint)
returns text language sql immutable as $$
    select case (flags & 3)
        when 0 then 'asserted'
        when 1 then 'negated'
        when 2 then 'absent'
        when 3 then 'unknown'
    end
$$;

create or replace function donto_maturity(flags smallint)
returns int language sql immutable as $$
    select ((flags >> 2) & 7)::int
$$;

-- Convenience domain check: ensure callers pass a valid polarity name.
create or replace function donto_is_polarity(p text)
returns boolean language sql immutable as $$
    select lower(p) in ('asserted','negated','absent','unknown')
$$;

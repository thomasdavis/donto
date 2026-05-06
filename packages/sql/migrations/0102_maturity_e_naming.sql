-- v1000 / §7.1 maturity ladder: E0–E5.
--
-- Migration 0002 packs maturity into bits 2-4 of donto_statement.flags
-- (3 bits = 0..7). The ladder was L0..L4. v1000 renames to E0..E5 and
-- adds E4 "Corroborated" between the existing reviewed (now E3) and
-- certified (now E5) tiers. The 3-bit storage already fits 0..7 so no
-- bit-layout change is required.
--
-- Mapping (storage stays the same; names move):
--   stored 0  L0 raw         -> E0 Raw
--   stored 1  L1 parsed      -> E1 Candidate
--   stored 2  L2 linked      -> E2 Evidence-supported
--   stored 3  L3 reviewed    -> E3 Reviewed
--   stored 4  L4 certified   -> E5 Certified
--   stored 5  (new)          -> E4 Corroborated
--   stored 6  reserved
--   stored 7  reserved
--
-- "Corroborated" sits between "Reviewed" and "Certified" semantically
-- but uses storage value 5 to avoid migrating existing rows. Helper
-- functions surface the canonical E-name regardless of storage value.

-- Map storage int -> E-level name.
create or replace function donto_maturity_label(p_stored int)
returns text language sql immutable as $$
    select case p_stored
        when 0 then 'E0'
        when 1 then 'E1'
        when 2 then 'E2'
        when 3 then 'E3'
        when 5 then 'E4'
        when 4 then 'E5'
        else null
    end
$$;

-- Map E-level name -> storage int (inverse).
create or replace function donto_maturity_from_label(p_label text)
returns int language sql immutable as $$
    select case lower(p_label)
        when 'e0' then 0
        when 'e1' then 1
        when 'e2' then 2
        when 'e3' then 3
        when 'e4' then 5
        when 'e5' then 4
        -- legacy L-names map to the same storage values
        when 'l0' then 0
        when 'l1' then 1
        when 'l2' then 2
        when 'l3' then 3
        when 'l4' then 4
        else null
    end
$$;

-- Map storage int -> human-readable description.
create or replace function donto_maturity_description(p_stored int)
returns text language sql immutable as $$
    select case p_stored
        when 0 then 'Raw — source or extraction artefact exists; not trusted as a claim.'
        when 1 then 'Candidate — model/rule/human proposed a claim.'
        when 2 then 'Evidence-supported — claim is grounded in a source span/row/timecode.'
        when 3 then 'Reviewed — domain reviewer accepted, rejected, or qualified.'
        when 5 then 'Corroborated — claim has cross-source support or survives contradiction review.'
        when 4 then 'Certified — claim passes formal or highly structured validation.'
        else null
    end
$$;

-- View the v1000 ladder. Note rows are ordered by E-level, not storage.
create or replace view donto_v_maturity_ladder_v1000 as
    select 0 as stored, 'E0' as level, 'Raw' as name,
           donto_maturity_description(0) as description
    union all select 1, 'E1', 'Candidate',         donto_maturity_description(1)
    union all select 2, 'E2', 'Evidence-supported',donto_maturity_description(2)
    union all select 3, 'E3', 'Reviewed',          donto_maturity_description(3)
    union all select 5, 'E4', 'Corroborated',      donto_maturity_description(5)
    union all select 4, 'E5', 'Certified',         donto_maturity_description(4);

-- Convenience wrapper: get a statement's E-label from its flags.
create or replace function donto_e_level(p_flags smallint)
returns text language sql immutable as $$
    select donto_maturity_label(donto_maturity(p_flags))
$$;

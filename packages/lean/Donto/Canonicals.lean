import Donto.Core

namespace Donto.Canonicals

open Donto

-- A temporal alias maps an IRI to a canonical IRI within a valid_time interval.
-- Intervals are represented as optional ISO date strings (none = unbounded).
structure TemporalAlias where
  aliasIri : IRI
  canonicalIri : IRI
  validFrom : Option String  -- ISO date, none = -infinity
  validTo : Option String    -- ISO date, none = +infinity
  hDistinct : aliasIri ≠ canonicalIri

-- A timeless alias (from donto_predicate.canonical_of)
structure TimelessAlias where
  aliasIri : IRI
  canonicalIri : IRI
  hDistinct : aliasIri ≠ canonicalIri

-- Check if a date falls within an interval.
-- Uses lexicographic string comparison, which works for ISO 8601 dates.
def inInterval (date : String) (from_ : Option String) (to_ : Option String) : Bool :=
  let afterFrom := match from_ with
    | none => true
    | some f => f ≤ date
  let beforeTo := match to_ with
    | none => true
    | some t => date < t
  afterFrom && beforeTo

-- Resolution order (matching donto_canonical_predicate_at):
-- 1. Temporal alias whose interval contains the as-of date
-- 2. Timeless canonical_of
-- 3. Pass-through (open-world)
def resolveAt (temporalAliases : List TemporalAlias)
    (timelessAliases : List TimelessAlias)
    (iri : IRI) (asOf : Option String) : IRI :=
  match asOf with
  | some date =>
    match temporalAliases.find? (fun a => a.aliasIri == iri && inInterval date a.validFrom a.validTo) with
    | some a => a.canonicalIri
    | none =>
      match timelessAliases.find? (fun a => a.aliasIri == iri) with
      | some a => a.canonicalIri
      | none => iri
  | none =>
    match timelessAliases.find? (fun a => a.aliasIri == iri) with
    | some a => a.canonicalIri
    | none => iri

-- Open-world: unregistered IRI resolves to itself
theorem open_world (temporalAliases : List TemporalAlias)
    (timelessAliases : List TimelessAlias) (iri : IRI)
    (asOf : Option String)
    (hNoTimeless : timelessAliases.find? (fun a => a.aliasIri == iri) = none)
    (hNoTemporal : ∀ date, temporalAliases.find? (fun a => a.aliasIri == iri && inInterval date a.validFrom a.validTo) = none) :
    resolveAt temporalAliases timelessAliases iri asOf = iri := by
  unfold resolveAt
  cases asOf with
  | none => simp [hNoTimeless]
  | some date => simp [hNoTemporal, hNoTimeless]

-- Resolution is deterministic (same inputs → same output)
theorem deterministic (temporalAliases : List TemporalAlias)
    (timelessAliases : List TimelessAlias) (iri : IRI) (asOf : Option String) :
    resolveAt temporalAliases timelessAliases iri asOf =
    resolveAt temporalAliases timelessAliases iri asOf := by rfl

-- No alias chains: a canonical_iri is never itself an alias.
-- This is enforced by construction (TemporalAlias.hDistinct) and by
-- the SQL trigger donto_predicate_alias_no_chain_trg.
-- We can state: if we resolve an alias to a canonical, resolving
-- the canonical again yields itself (assuming no chain).
theorem no_chain_single_hop (temporalAliases : List TemporalAlias)
    (timelessAliases : List TimelessAlias) (alias canonical : IRI)
    (asOf : Option String)
    (hResolves : resolveAt temporalAliases timelessAliases alias asOf = canonical)
    (hCanonicalNotAlias :
      timelessAliases.find? (fun a => a.aliasIri == canonical) = none)
    (hCanonicalNotTemporal :
      ∀ date, temporalAliases.find? (fun a =>
        a.aliasIri == canonical && inInterval date a.validFrom a.validTo) = none) :
    resolveAt temporalAliases timelessAliases canonical asOf = canonical := by
  unfold resolveAt
  cases asOf with
  | none => simp [hCanonicalNotAlias]
  | some date => simp [hCanonicalNotTemporal, hCanonicalNotAlias]

end Donto.Canonicals

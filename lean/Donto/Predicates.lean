-- Predicate registry types (PRD §9).
import Donto.Core

namespace Donto

structure Predicate where
  iri               : IRI
  canonicalOf       : Option IRI := none
  label             : Option String := none
  description       : Option String := none
  domain            : Option IRI := none      -- shape iri
  rangeIri          : Option IRI := none      -- shape iri
  rangeDatatype     : Option IRI := none
  inverseOf         : Option IRI := none
  isSymmetric       : Bool := false
  isTransitive      : Bool := false
  isFunctional      : Bool := false
  isInverseFunctional : Bool := false
  cardMin           : Option Nat := none
  cardMax           : Option Nat := none
  status            : String := "active"

end Donto

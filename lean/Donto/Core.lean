-- Core types: Statement, Context, Polarity, Modality, Maturity.
-- These mirror the Postgres schema (PRD §5, §6, §7) so DIR encoding is
-- a structural copy.

namespace Donto

abbrev IRI := String

inductive Polarity where
  | asserted | negated | absent | unknown
  deriving Repr, BEq, DecidableEq

inductive Modality where
  | observed | derived | hypothesized | retracted
  deriving Repr, BEq, DecidableEq

inductive Confidence where
  | uncertified | speculative | moderate | strong
  deriving Repr, BEq, DecidableEq, Ord

structure Maturity where
  level : Nat
  hLE   : level ≤ 4 := by decide
  deriving Repr

inductive Object where
  | iri (i : IRI)
  | lit (value : String) (datatype : IRI) (lang : Option String := none)
  deriving Repr

structure Statement where
  id        : Option String := none
  subject   : IRI
  predicate : IRI
  object    : Object
  context   : IRI
  polarity  : Polarity := .asserted
  modality  : Modality := .observed
  confidence: Option Confidence := none
  maturity  : Nat := 0   -- 0..4
  validFrom : Option String := none   -- ISO date
  validTo   : Option String := none
  deriving Repr

inductive ContextKind where
  | source | snapshot | hypothesis | user | pipeline
  | trust | derivation | quarantine | custom | system
  deriving Repr, BEq

structure Context where
  iri    : IRI
  kind   : ContextKind
  parent : Option IRI := none
  mode   : String     := "permissive"   -- "permissive" | "curated"
  deriving Repr

structure ContextScope where
  include             : List IRI := []
  exclude             : List IRI := []
  includeDescendants  : Bool := true
  includeAncestors    : Bool := false
  deriving Repr

end Donto

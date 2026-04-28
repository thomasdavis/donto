-- Derivation rule combinators (PRD §17).
import Donto.Core
import Donto.Predicates

namespace Donto.Rules

inductive Mode where
  | eager | batch | onDemand
  deriving Repr

structure DerivationReport where
  ruleIri    : IRI
  intoCtx    : IRI
  emitted    : List Statement := []
  duration   : Nat := 0   -- ms
  deriving Repr

structure Rule where
  iri        : IRI
  label      : Option String := none
  outputCtx  : IRI
  mode       : Mode := .onDemand
  apply      : List Statement → List Statement   -- pure transformation

namespace StdLib

/-- TransitiveClosure: emit p+ for the transitive closure of p. -/
partial def transitiveClosure (predicate : IRI) (outputCtx : IRI) : Rule :=
  { iri := s!"builtin:transitive/{predicate}",
    outputCtx := outputCtx,
    mode := .onDemand,
    apply := fun stmts =>
      let edges := stmts.filter (fun s => s.predicate == predicate &&
                                          s.polarity == .asserted)
      let pairs : List (IRI × IRI × Statement) := edges.filterMap fun s =>
        match s.object with
          | .iri o => some (s.subject, o, s)
          | _ => none
      let go : List (IRI × IRI) → List (IRI × IRI) := fun ps =>
        let newOnes := ps.foldl (init := []) fun acc (a, b) =>
          let extensions := pairs.filterMap fun (b', c, _) =>
            if b == b' && !ps.contains (a, c) then some (a, c) else none
          extensions ++ acc
        ps ++ newOnes
      let closure := go (pairs.map (fun (a, b, _) => (a, b)))
      let derivedPred := s!"{predicate}+"
      closure.map fun (a, b) =>
        { id := none, subject := a, predicate := derivedPred,
          object := .iri b, context := outputCtx,
          polarity := .asserted, modality := .derived,
          maturity := 3, validFrom := none, validTo := none,
          confidence := some .moderate } }

end StdLib
end Donto.Rules

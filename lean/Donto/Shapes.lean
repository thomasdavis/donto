-- Shape combinators and standard library (PRD §16).
import Donto.Core
import Donto.Predicates

namespace Donto.Shapes

inductive Severity where
  | info | warning | violation
  deriving Repr, BEq

structure Violation where
  focus    : IRI
  reason   : String
  evidence : List String := []
  deriving Repr

structure ShapeReport where
  shapeIri      : IRI
  focusCount    : Nat := 0
  violations    : List Violation := []
  deriving Repr

/-- A shape: focus selector + decidable constraint.
    The body is opaque from Lean's perspective; concrete shapes implement
    `evaluate` to produce a [`ShapeReport`] from a list of input statements
    that have already been scoped. -/
structure Shape where
  iri        : IRI
  label      : Option String := none
  severity   : Severity := .violation
  evaluate   : List Statement → ShapeReport

namespace StdLib

/-- FunctionalPredicate p: at most one object per subject. -/
def functional (predicate : IRI) : Shape :=
  { iri := s!"builtin:functional/{predicate}"
    label := some s!"FunctionalPredicate({predicate})"
    severity := .violation
    evaluate := fun stmts =>
      let matching := stmts.filter (fun s =>
        s.predicate == predicate && s.polarity == .asserted)
      let bySubject : List (IRI × List Statement) :=
        matching.foldl (init := []) (fun acc s =>
          if acc.any (fun p => p.fst == s.subject) then
            acc.map (fun p => if p.fst == s.subject then (p.fst, s :: p.snd) else p)
          else
            (s.subject, [s]) :: acc)
      let violations : List Violation := bySubject.filterMap (fun (subj, rows) =>
        if rows.length > 1 then
          some { focus := subj
                 reason := s!"predicate {predicate} is functional but has {rows.length} objects"
                 evidence := rows.map (fun s => s.id.getD "") }
        else none)
      { shapeIri := s!"builtin:functional/{predicate}"
        focusCount := bySubject.length
        violations := violations } }

/-- DatatypeShape p dt: literal-valued objects of p must have datatype dt. -/
def datatype (predicate : IRI) (dt : IRI) : Shape :=
  { iri := s!"builtin:datatype/{predicate}/{dt}"
    severity := .violation
    evaluate := fun stmts =>
      let matching := stmts.filter (fun s =>
        s.predicate == predicate && s.polarity == .asserted)
      let violations := matching.filterMap fun s => match s.object with
        | .lit _ d _ =>
            if d == dt then none
            else some { focus := s.subject
                        reason := s!"expected datatype {dt}, got {d}"
                        evidence := [s.id.getD ""] }
        | _ =>
            some { focus := s.subject
                   reason := s!"expected literal, got IRI"
                   evidence := [s.id.getD ""] }
      { shapeIri := s!"builtin:datatype/{predicate}/{dt}"
        focusCount := matching.length
        violations := violations } }

end StdLib
end Donto.Shapes

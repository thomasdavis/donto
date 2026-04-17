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
  evidence : List String := []   -- statement_ids as strings (UUIDs)
  deriving Repr

structure ShapeReport where
  shapeIri      : IRI
  focusCount    : Nat := 0
  violations    : List Violation := []
  deriving Repr

/-- A shape: focus selector + decidable constraint. -/
structure Shape where
  iri        : IRI
  label      : Option String := none
  severity   : Severity := .violation
  /-- Given the input statements (already scoped), produce a report. The body
      is opaque from Lean's perspective; concrete shapes implement it. -/
  evaluate   : List Statement → ShapeReport

namespace StdLib

/-- FunctionalPredicate p: at most one object per subject. -/
def functional (predicate : IRI) : Shape :=
  { iri := s!"builtin:functional/{predicate}",
    label := some s!"FunctionalPredicate({predicate})",
    severity := .violation,
    evaluate := fun stmts =>
      let scoped := stmts.filter (fun s => s.predicate == predicate &&
                                            s.polarity == .asserted)
      let bySubject : Std.HashMap IRI (List Statement) :=
        scoped.foldl (fun m s => m.insert s.subject (s :: (m.getD s.subject [])))
                     ∅
      let violations : List Violation := bySubject.fold (init := []) fun acc subj rows =>
        if rows.length > 1 then
          let evs : List String := rows.map (fun s => s.id.getD "")
          {focus := subj,
           reason := s!"predicate {predicate} is functional but has {rows.length} objects",
           evidence := evs} :: acc
        else acc
      { shapeIri := s!"builtin:functional/{predicate}",
        focusCount := bySubject.size,
        violations := violations } }

/-- DatatypeShape p dt: literal-valued objects of p must have datatype dt. -/
def datatype (predicate : IRI) (dt : IRI) : Shape :=
  { iri := s!"builtin:datatype/{predicate}/{dt}",
    severity := .violation,
    evaluate := fun stmts =>
      let scoped := stmts.filter (fun s => s.predicate == predicate &&
                                            s.polarity == .asserted)
      let violations := scoped.filterMap fun s => match s.object with
        | .lit _ d _ => if d == dt then none else
          some {focus := s.subject,
                reason := s!"expected datatype {dt}, got {d}",
                evidence := [s.id.getD ""]}
        | _ => some {focus := s.subject,
                     reason := s!"expected literal, got IRI",
                     evidence := [s.id.getD ""]}
      { shapeIri := s!"builtin:datatype/{predicate}/{dt}",
        focusCount := scoped.length,
        violations := violations } }

end StdLib
end Donto.Shapes

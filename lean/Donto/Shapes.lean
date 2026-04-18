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

/-- Genealogy domain shape (PRD §16): a parent must be at least 12 years
    older than their child and at most 80 years older.

    Looks for `ex:parentOf` edges between subjects. Each edge's parent and
    child must each carry an `ex:birthYear` integer literal somewhere in
    `stmts`. The shape flags edges where the gap is unreasonable, where
    either birth year is missing, or where the child is older than the
    parent.

    This is intentionally a *Lean-authored* shape (in contrast to the
    builtin shapes mirrored in Rust) so we can demonstrate the boundary
    end-to-end: dontosrv ships the relevant statements, the Lean engine
    runs the constraint, the report comes back with violations. -/
def parentChildAgeGap : Shape :=
  { iri := "lean:builtin/parent-child-age-gap"
    label := some "ParentChildAgeGap"
    severity := .violation
    evaluate := fun stmts =>
      -- Index birth years by subject. We accept either an integer literal
      -- or a numeric-looking string literal; non-numeric content means the
      -- shape can't decide and we record a violation.
      let births : List (IRI × Int) := stmts.filterMap fun s =>
        if s.predicate == "ex:birthYear" && s.polarity == .asserted then
          match s.object with
          | .lit v _ _ => v.toInt? |>.map (fun n => (s.subject, n))
          | _          => none
        else none
      let lookupBirth (iri : IRI) : Option Int :=
        (births.find? (fun p => p.fst == iri)).map (·.snd)
      let parentEdges := stmts.filter (fun s =>
        s.predicate == "ex:parentOf" && s.polarity == .asserted)
      let violations : List Violation := parentEdges.filterMap fun edge =>
        let child := match edge.object with | .iri i => i | _ => ""
        if child.isEmpty then
          some { focus := edge.subject
                 reason := "ex:parentOf object is not an IRI"
                 evidence := [edge.id.getD ""] }
        else
          match lookupBirth edge.subject, lookupBirth child with
          | none, _ =>
              some { focus := edge.subject
                     reason := s!"parent {edge.subject} has no ex:birthYear"
                     evidence := [edge.id.getD ""] }
          | _, none =>
              some { focus := child
                     reason := s!"child {child} has no ex:birthYear"
                     evidence := [edge.id.getD ""] }
          | some pYear, some cYear =>
              let gap := cYear - pYear
              if gap < 12 then
                some { focus := edge.subject
                       reason := s!"parent {edge.subject} ({pYear}) is only {gap}y older than child {child} ({cYear}); minimum 12"
                       evidence := [edge.id.getD ""] }
              else if gap > 80 then
                some { focus := edge.subject
                       reason := s!"parent {edge.subject} ({pYear}) is {gap}y older than child {child} ({cYear}); maximum 80"
                       evidence := [edge.id.getD ""] }
              else
                none
      { shapeIri := "lean:builtin/parent-child-age-gap"
        focusCount := parentEdges.length
        violations := violations } }

/-- Role-fit shape (resume demo).

    Given a target job IRI and a list of statements containing both the
    candidate(s) and the job, compute the missing required skills and
    surface each as a violation. A clean report (no violations) is a
    *constructive proof* that the candidate covers every requirement. -/
def roleFit (jobIri : IRI) : Shape :=
  { iri := s!"lean:role/fit/{jobIri}"
    label := some s!"RoleFit({jobIri})"
    severity := .warning
    evaluate := fun stmts =>
      let asserted := stmts.filter (·.polarity == .asserted)
      let candidates : List IRI :=
        (asserted.filter (fun s =>
          s.predicate == "rdf:type" &&
          (match s.object with | .iri "ex:Candidate" => true | _ => false))).map (·.subject)
      let candidates := candidates.eraseDups
      let required : List IRI :=
        (asserted.filter (fun s =>
          s.subject == jobIri && s.predicate == "ex:requiresSkill")).filterMap fun s =>
            match s.object with | .iri i => some i | _ => none
      let minYears : Option Int :=
        (asserted.find? (fun s =>
          s.subject == jobIri && s.predicate == "ex:minYears"))
        |>.bind fun s => match s.object with
          | .lit v _ _ => v.toInt?
          | _ => none
      let candidateSkills (c : IRI) : List IRI :=
        (asserted.filter (fun s =>
          s.subject == c && s.predicate == "ex:hasSkill")).filterMap fun s =>
            match s.object with | .iri i => some i | _ => none
      let candidateYears (c : IRI) : Option Int :=
        (asserted.find? (fun s =>
          s.subject == c && s.predicate == "ex:yearsOfExperience"))
        |>.bind fun s => match s.object with
          | .lit v _ _ => v.toInt?
          | _ => none
      let allViolations : List Violation := candidates.bind fun cand =>
        let skills := candidateSkills cand
        let missing := required.filter (fun r => !skills.contains r)
        let missingViols : List Violation := missing.map fun skill =>
          { focus := cand
            reason := s!"missing required skill {skill} for {jobIri}"
            evidence := [] }
        let yearsViols : List Violation :=
          match minYears, candidateYears cand with
          | some mn, some cy =>
              if cy < mn then
                [{ focus := cand
                   reason := s!"{cand} has {cy} years, {jobIri} requires {mn}"
                   evidence := [] }]
              else []
          | _, _ => []
        missingViols ++ yearsViols
      { shapeIri := s!"lean:role/fit/{jobIri}"
        focusCount := candidates.length * required.length
        violations := allViolations } }

end StdLib
end Donto.Shapes

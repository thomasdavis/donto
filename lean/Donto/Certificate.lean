-- Certificate types and verifiers (PRD §18).
import Donto.Core

namespace Donto.Certificate

inductive Kind where
  | directAssertion
  | substitution
  | transitiveClosure
  | confidenceJustification
  | shapeEntailment
  | hypothesisScoped
  | replay
  deriving Repr, BEq

structure Certificate where
  kind         : Kind
  subjectStmt  : String          -- statement_id (UUID stringified)
  ruleIri      : Option IRI := none
  inputs       : List String := []
  body         : String := ""    -- JSON-encoded payload
  signature    : Option String := none
  deriving Repr

inductive Verdict where | ok | reject (reason : String)
  deriving Repr

namespace Verifiers

/-- Direct assertion: the certificate's body must record at least one source. -/
def verifyDirect (c : Certificate) : Verdict :=
  if c.kind == .directAssertion ∧ c.body.length > 0 then .ok
  else .reject "direct assertion needs a non-empty body"

/-- Transitive closure: requires non-empty inputs. Real verification re-runs
    the rule and compares; that flow lives in dontosrv (Rust). -/
def verifyTransitive (c : Certificate) : Verdict :=
  if c.kind == .transitiveClosure ∧ c.inputs.length ≥ 2 then .ok
  else .reject "transitive closure certificate needs ≥ 2 inputs"

end Verifiers
end Donto.Certificate

-- DIR: Donto Intermediate Representation (PRD §13).
-- The Lean side mirrors crates/dontosrv/src/dir.rs; the JSON encoding
-- defined here is the only wire format crossing the boundary in v1.

import Donto.Core
import Lean.Data.Json

namespace Donto

namespace IR

inductive Directive where
  | declarePredicate (iri : String) (label : Option String) (canonicalOf : Option String)
  | declareContext   (iri : String) (kind : String) (parent : Option String) (mode : String)
  | declareShape     (iri : String) (focus : String) (severity : String) (body : Lean.Json)
  | declareRule      (iri : String) (pattern : String) (outputCtx : String) (body : Lean.Json)
  | assertBatch      (context : String) (statements : List Statement)
  | retract          (statementId : String)
  | correct          (statementId : String) (newStmt : Statement)
  | validateRequest  (shapeIri : String) (scope : Lean.Json)
  | validateResponse (shapeIri : String) (focusCount : Nat) (violations : Lean.Json)
  | deriveRequest    (ruleIri : String) (scope : Lean.Json) (intoCtx : String)
  | deriveResponse   (ruleIri : String) (intoCtx : String) (emitted : Nat)
  | certificate      (kind : String) (subjectStmt : String) (body : Lean.Json)

structure Envelope where
  version    : String := "0.1.0-json"
  directives : List Directive

end IR

end Donto

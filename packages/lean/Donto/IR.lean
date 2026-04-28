-- DIR: Donto Intermediate Representation (PRD §13).
-- The Lean side mirrors apps/dontosrv/src/dir.rs; the JSON encoding
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
  -- Evidence substrate directives (mirrors Rust dir.rs expansion)
  | ingestDocument    (iri : String) (mediaType : String) (label : Option String)
                      (sourceUrl : Option String) (language : Option String)
  | ingestRevision    (documentIri : String) (body : Option String)
                      (parserVersion : Option String)
  | createSpan        (revisionId : String) (spanType : String)
                      (startOffset : Option Nat) (endOffset : Option Nat)
                      (surfaceText : Option String)
  | createAnnotation  (spanId : String) (spaceIri : String) (feature : String)
                      (value : Option String) (confidence : Option Float)
  | startExtraction   (modelId : Option String) (sourceRevisionId : Option String)
                      (context : Option String)
  | completeExtraction (runId : String) (status : String)
  | linkEvidence      (statementId : String) (linkType : String)
                      (target : Lean.Json)
  | registerAgent     (iri : String) (agentType : String) (label : Option String)
                      (modelId : Option String)
  | assertArgument    (source : String) (target : String) (relation : String)
                      (context : String) (strength : Option Float)
  | emitObligation    (statementId : String) (obligationType : String)
                      (context : String) (priority : Option Nat)
  | resolveObligation (obligationId : String) (status : String)

structure Envelope where
  version    : String := "0.1.0-json"
  directives : List Directive

end IR

end Donto

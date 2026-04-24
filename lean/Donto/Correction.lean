-- Correction semantics (PRD §8, Phase L1).
-- Formalizes the retract-and-re-assert pattern used by `donto_correct`.
-- A correction always inherits the original's context and valid time.
import Donto.Core

namespace Donto.Correction

open Donto

/-- Correct: retract the old statement and produce a replacement with
    selectively updated fields. Context and valid time are always
    inherited from the original — the caller cannot override them.
    Returns (retracted_original, replacement). -/
def correct (old : Statement) (newSubject : Option IRI) (newPredicate : Option IRI)
    (newObject : Option Object) (newPolarity : Option Polarity) :
    Statement × Statement :=
  let retracted := { old with modality := .retracted }
  let replacement : Statement := {
    id := none
    subject := newSubject.getD old.subject
    predicate := newPredicate.getD old.predicate
    object := newObject.getD old.object
    context := old.context  -- always inherited
    polarity := newPolarity.getD old.polarity
    modality := .observed
    maturity := old.maturity
    validFrom := old.validFrom
    validTo := old.validTo
  }
  (retracted, replacement)

-- ---------------------------------------------------------------------------
-- Theorems
-- ---------------------------------------------------------------------------

/-- Context is always inherited from the original — corrections cannot
    re-home a statement. -/
theorem correct_inherits_context (old : Statement) (ns np : Option IRI)
    (no : Option Object) (npol : Option Polarity) :
    (correct old ns np no npol).2.context = old.context := by
  unfold correct; rfl

/-- The retracted copy preserves the original's subject. -/
theorem correct_retracted_preserves_subject (old : Statement) (ns np : Option IRI)
    (no : Option Object) (npol : Option Polarity) :
    (correct old ns np no npol).1.subject = old.subject := by
  unfold correct; rfl

/-- The retracted copy preserves the original's predicate. -/
theorem correct_retracted_preserves_predicate (old : Statement) (ns np : Option IRI)
    (no : Option Object) (npol : Option Polarity) :
    (correct old ns np no npol).1.predicate = old.predicate := by
  unfold correct; rfl

/-- The retracted copy preserves the original's object. -/
theorem correct_retracted_preserves_object (old : Statement) (ns np : Option IRI)
    (no : Option Object) (npol : Option Polarity) :
    (correct old ns np no npol).1.object = old.object := by
  unfold correct; rfl

/-- The retracted copy has modality = retracted. -/
theorem correct_retracted_is_retracted (old : Statement) (ns np : Option IRI)
    (no : Option Object) (npol : Option Polarity) :
    (correct old ns np no npol).1.modality = .retracted := by
  unfold correct; rfl

/-- No-op correction (all None) produces a replacement structurally
    equal to the original on content fields (subject, predicate,
    object, context, polarity, maturity). id and modality differ. -/
theorem correct_noop_preserves_content (old : Statement) :
    let (_, replacement) := correct old none none none none
    replacement.subject = old.subject ∧
    replacement.predicate = old.predicate ∧
    replacement.object = old.object ∧
    replacement.context = old.context ∧
    replacement.polarity = old.polarity ∧
    replacement.maturity = old.maturity := by
  unfold correct; simp

/-- Valid time is always inherited from the original. -/
theorem correct_inherits_valid_time (old : Statement) (ns np : Option IRI)
    (no : Option Object) (npol : Option Polarity) :
    (correct old ns np no npol).2.validFrom = old.validFrom ∧
    (correct old ns np no npol).2.validTo = old.validTo := by
  unfold correct; simp

end Donto.Correction

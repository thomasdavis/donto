-- Truth model: polarity table, confidence ordering, modality interactions.
-- This is a Lean restatement of PRD §6 used by shape and rule combinators.
import Donto.Core

namespace Donto

namespace Truth

/-- Default visibility predicate for a statement. Asserted is visible;
    everything else is hidden unless the query explicitly opts in. -/
def visibleByDefault (s : Statement) : Bool :=
  s.polarity == .asserted

/-- Confidence promotion: tier order is uncertified < speculative <
    moderate < strong. -/
def atLeast (a b : Confidence) : Bool :=
  let rank : Confidence → Nat
    | .uncertified => 0 | .speculative => 1
    | .moderate    => 2 | .strong      => 3
  rank a ≥ rank b

end Truth

end Donto

-- Bitemporal helpers (PRD §8). Phase 5 ships only date arithmetic; precise
-- daterange manipulation arrives in Phase 6 once the engine actually consumes
-- valid_time intervals from DIR.
import Donto.Core

namespace Donto

namespace Temporal

inductive Precision where
  | exact | day | month | year | decade | century | range
  deriving Repr

structure ValidTime where
  lower : Option String := none   -- ISO date or none = -infinity
  upper : Option String := none   -- ISO date or none = +infinity
  precision : Precision := .exact
  deriving Repr

end Temporal

end Donto

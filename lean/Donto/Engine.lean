-- Sidecar engine entry-point. dontosrv (Rust) spawns this Lean executable
-- and exchanges DIR documents over stdio. The actual main is in Main.lean.
import Donto.Core
import Donto.IR
import Donto.Shapes
import Donto.Rules
import Donto.Certificate

namespace Donto.Engine

structure Status where
  ready  : Bool
  shapes : Nat
  rules  : Nat

end Donto.Engine

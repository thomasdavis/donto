-- donto_engine: the Lean sidecar binary. Reads DIR envelopes from stdin
-- (one JSON per line), routes to the appropriate handler, writes responses
-- to stdout. dontosrv (Rust) starts this as a child process and proxies.
--
-- Phase 5 ships a minimal handler that recognizes envelopes and echoes the
-- shape/rule registrations it sees. Real evaluation lives in Donto.Shapes
-- and Donto.Rules; wiring is Phase 6+.

import Donto

def main : IO Unit := do
  IO.println "donto_engine ready"
  let stdin ← IO.getStdin
  let _ ← stdin.readToEnd
  IO.println "{\"ack\":true,\"engine\":\"lean\",\"version\":\"0.1.0\"}"

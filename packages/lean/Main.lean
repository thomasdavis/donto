-- donto_engine: the Lean sidecar binary.
--
-- One JSON envelope per line on stdin. One JSON response per line on stdout.
-- Banner on first launch (also single line) so the parent can synchronise.
--
-- Invariants:
--   * Every input line that parses as JSON produces exactly one output line.
--   * Parse failures produce an `error` envelope, never a panic.
--   * `flush` after every response so the parent never buffers indefinitely.

import Donto
import Lean.Data.Json

open Lean

partial def loop (stdin stdout : IO.FS.Stream) : IO Unit := do
  let line ← stdin.getLine
  if line.isEmpty then return ()    -- EOF
  let trimmed := line.trim
  if trimmed.isEmpty then
    loop stdin stdout
  else
    let resp : Json :=
      match Json.parse trimmed with
      | .ok env => Donto.Engine.dispatch env
      | .error e =>
          Json.mkObj [
            ("version", Json.str "0.1.0-json"),
            ("kind",    Json.str "error"),
            ("error",   Json.str s!"json parse: {e}")
          ]
    stdout.putStrLn resp.compress
    stdout.flush
    loop stdin stdout

def main : IO Unit := do
  let stdin  ← IO.getStdin
  let stdout ← IO.getStdout
  -- Banner. dontosrv reads exactly one line before sending the first request.
  stdout.putStrLn "{\"version\":\"0.1.0-json\",\"kind\":\"ready\",\"engine\":\"lean\"}"
  stdout.flush
  loop stdin stdout

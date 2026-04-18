-- Sidecar engine: read JSON DIR envelopes from stdin, dispatch each to a
-- handler, write JSON responses to stdout. dontosrv (Rust) spawns
-- donto_engine and proxies validate / derive / certificate requests over
-- this socket-style line protocol.
--
-- Wire format (line-delimited JSON, one envelope per line):
--   request : {"version":"0.1.0-json","kind":"validate_request",
--              "shape_iri":"lean:builtin/parent-child-age-gap",
--              "scope":{...},
--              "statements":[ {Statement json} ... ] }
--   response: {"version":"0.1.0-json","kind":"validate_response",
--              "shape_iri":"lean:builtin/parent-child-age-gap",
--              "focus_count":N, "violations":[ {focus, reason, evidence} ] }
--
-- The sender (dontosrv) is responsible for shipping the relevant
-- statements alongside the request — Lean does not query Postgres
-- directly. That keeps the boundary narrow and the failure modes simple
-- (PRD §15 sidecar contract).

import Donto.Core
import Donto.Shapes
import Lean.Data.Json

namespace Donto.Engine

open Lean

/-- Coerce any JSON scalar to a `String`. Numbers come back in their
    natural textual form ("1850", not "1850.0"). Used to extract literal
    values from `object_lit.v`, which may be any JSON scalar depending on
    the source datatype. -/
def jsonScalarToString : Json → String
  | .str s  => s
  | .num n  =>
      -- JsonNumber stores mantissa : Int and exponent : Nat. exponent = 0
      -- means the value is exactly `mantissa`.
      if n.exponent = 0 then toString n.mantissa
      else toString n.mantissa ++ "e-" ++ toString n.exponent
  | .bool b => if b then "true" else "false"
  | .null   => ""
  | other   => other.compress

/-- Decode a single Statement from JSON. Tolerant: missing optional fields
    fall back to defaults; unknown fields are ignored. -/
def parseStatement (j : Json) : Except String Statement := do
  let subject   ← j.getObjValAs? String "subject"
  let predicate ← j.getObjValAs? String "predicate"
  let context   ← j.getObjValAs? String "context"
  let id        := (j.getObjValAs? String "id").toOption
  let object    ←
    match j.getObjValAs? String "object_iri" with
    | .ok iri => .ok (Object.iri iri)
    | .error _ =>
        match j.getObjVal? "object_lit" with
        | .ok lit =>
            -- `v` may be any JSON scalar — string, number, bool. Coerce to
            -- the textual form the shape body expects.
            let v   := (lit.getObjVal? "v").toOption.map jsonScalarToString |>.getD ""
            let dt  := (lit.getObjValAs? String "dt").toOption.getD "xsd:string"
            let lang := (lit.getObjValAs? String "lang").toOption
            .ok (Object.lit v dt lang)
        | .error _ => .error "statement has neither object_iri nor object_lit"
  let polarity :=
    match (j.getObjValAs? String "polarity").toOption with
    | some "negated"  => Polarity.negated
    | some "absent"   => Polarity.absent
    | some "unknown"  => Polarity.unknown
    | _               => Polarity.asserted
  let validFrom := (j.getObjValAs? String "valid_lo").toOption
  let validTo   := (j.getObjValAs? String "valid_hi").toOption
  return { id := id, subject := subject, predicate := predicate,
           object := object, context := context, polarity := polarity,
           validFrom := validFrom, validTo := validTo }

/-- Decode a list-of-Statement from a JSON array. -/
def parseStatements (j : Json) : Except String (List Statement) :=
  match j.getArr? with
  | .ok arr => arr.toList.mapM parseStatement
  | .error e => .error s!"statements: {e}"

/-- Encode a ShapeReport to JSON. -/
def reportToJson (r : Donto.Shapes.ShapeReport) : Json :=
  Json.mkObj [
    ("version",     Json.str "0.1.0-json"),
    ("kind",        Json.str "validate_response"),
    ("shape_iri",   Json.str r.shapeIri),
    ("focus_count", Json.num (JsonNumber.fromNat r.focusCount)),
    ("violations",  Json.arr (r.violations.map (fun v =>
        Json.mkObj [
          ("focus",    Json.str v.focus),
          ("reason",   Json.str v.reason),
          ("evidence", Json.arr (v.evidence.map Json.str).toArray)
        ]
      )).toArray)
  ]

/-- Resolve a shape iri to a [`Donto.Shapes.Shape`]. Phase 5 ships the two
    standard-library shapes plus a domain-specific genealogy shape that
    formalises PRD §16 ParentChildAgeGap. New shapes register here. -/
def lookupShape (iri : String) : Option Donto.Shapes.Shape :=
  if iri.startsWith "lean:functional/" then
    some (Donto.Shapes.StdLib.functional (iri.drop "lean:functional/".length))
  else if iri.startsWith "lean:datatype/" then
    -- iri shape: lean:datatype/<predicate>/<datatype>
    let rest := iri.drop "lean:datatype/".length
    match rest.splitOn "/" with
    | [pred, dt] => some (Donto.Shapes.StdLib.datatype pred dt)
    | _          => none
  else if iri == "lean:builtin/parent-child-age-gap" then
    some Donto.Shapes.StdLib.parentChildAgeGap
  else
    none

/-- Dispatch a single envelope. Returns the response envelope as JSON. -/
def dispatch (env : Json) : Json :=
  let kind := (env.getObjValAs? String "kind").toOption.getD ""
  match kind with
  | "validate_request" =>
      let shapeIri := (env.getObjValAs? String "shape_iri").toOption.getD ""
      let stmtsJson := (env.getObjVal? "statements").toOption.getD (Json.arr #[])
      match parseStatements stmtsJson with
      | .error e => Json.mkObj [
          ("version", Json.str "0.1.0-json"),
          ("kind",    Json.str "error"),
          ("error",   Json.str s!"parse: {e}")
        ]
      | .ok stmts =>
          match lookupShape shapeIri with
          | none =>
              Json.mkObj [
                ("version", Json.str "0.1.0-json"),
                ("kind",    Json.str "error"),
                ("error",   Json.str s!"unknown shape iri: {shapeIri}")
              ]
          | some shape =>
              let report := shape.evaluate stmts
              -- Stamp the requested IRI on the report (the shape may have
              -- been parameterised with a different label).
              let report := { report with shapeIri := shapeIri }
              reportToJson report
  | "ping" =>
      Json.mkObj [("version", Json.str "0.1.0-json"), ("kind", Json.str "pong")]
  | _ =>
      Json.mkObj [
        ("version", Json.str "0.1.0-json"),
        ("kind",    Json.str "error"),
        ("error",   Json.str s!"unknown envelope kind: {kind}")
      ]

end Donto.Engine

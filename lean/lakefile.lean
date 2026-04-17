import Lake
open Lake DSL

package donto where
  -- The Lean side of donto. Provides Shape and Rule combinators, the DIR
  -- AST, encoders/decoders, and the standard library. dontosrv (Rust) calls
  -- into a compiled Lean executable that streams DIR over stdio.
  leanOptions := #[⟨`linter.unusedVariables, false⟩]

@[default_target]
lean_lib Donto

@[default_target]
lean_exe donto_engine where
  root := `Main

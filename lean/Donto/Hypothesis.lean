import Donto.Core

namespace Donto.Hypothesis

open Donto

-- A hypothesis context is a child of a base context. Statements in the
-- hypothesis are only visible to scopes that explicitly include it.

def isHypothesis (ctx : Context) : Bool :=
  ctx.kind == .hypothesis

-- A statement is visible under a scope if its context is in the resolved set.
-- This is the Lean mirror of donto_resolve_scope.
def visibleUnder (resolvedContexts : List IRI) (stmt : Statement) : Bool :=
  resolvedContexts.contains stmt.context

-- Non-leakage: if a scope does not include a hypothesis context (directly
-- or via descendant walk), statements in that hypothesis are not visible.
theorem hypothesis_not_in_scope_not_visible
    (resolvedContexts : List IRI) (stmt : Statement)
    (hNotIn : resolvedContexts.contains stmt.context = false) :
    visibleUnder resolvedContexts stmt = false := by
  unfold visibleUnder; exact hNotIn

-- If a scope includes the hypothesis, its statements are visible.
theorem hypothesis_in_scope_visible
    (resolvedContexts : List IRI) (stmt : Statement)
    (hIn : resolvedContexts.contains stmt.context = true) :
    visibleUnder resolvedContexts stmt = true := by
  unfold visibleUnder; exact hIn

-- Sibling isolation: two hypothesis contexts that are siblings (same parent)
-- do not see each other's statements if the scope only includes one.
theorem sibling_isolation
    (hypoA hypoB : IRI) (stmtInB : Statement)
    (hDiff : hypoA ≠ hypoB)
    (hCtx : stmtInB.context = hypoB)
    (resolvedContexts : List IRI)
    (hIncludesA : resolvedContexts.contains hypoA = true)
    (hNotIncludesB : resolvedContexts.contains hypoB = false) :
    visibleUnder resolvedContexts stmtInB = false := by
  unfold visibleUnder; rw [hCtx]; exact hNotIncludesB

-- Hypothesis branching: creating a hypothesis from a base scope adds
-- the hypothesis to the resolved set without removing base contexts.
def branchHypothesis (baseResolved : List IRI) (hypoIri : IRI) : List IRI :=
  hypoIri :: baseResolved

theorem branch_preserves_base (baseResolved : List IRI) (hypoIri : IRI) (ctx : IRI)
    (hBase : baseResolved.contains ctx = true) :
    (branchHypothesis baseResolved hypoIri).contains ctx = true := by
  unfold branchHypothesis
  rw [List.contains_iff_exists_mem_beq] at hBase ⊢
  obtain ⟨x, hx_mem, hx_eq⟩ := hBase
  exact ⟨x, List.mem_cons.mpr (Or.inr hx_mem), hx_eq⟩

theorem branch_includes_hypothesis (baseResolved : List IRI) (hypoIri : IRI) :
    (branchHypothesis baseResolved hypoIri).contains hypoIri = true := by
  unfold branchHypothesis
  simp [List.contains, List.elem, BEq.beq]

end Donto.Hypothesis

-- Scope resolution semantics (PRD §7, Phase L1).
-- Formalizes the flat (non-tree) scope resolution used by
-- `donto_resolve_scope` and proves key invariants that the Postgres
-- implementation must satisfy.
import Donto.Core

namespace Donto.Scope

open Donto

-- Flat resolution (no tree walk): a context is visible if it's in the
-- include list and not in the exclude list. When include is empty,
-- all contexts are visible (minus excludes).
def visibleFlat (scope : ContextScope) (ctx : IRI) : Bool :=
  let included := scope.includeCtxs.isEmpty || scope.includeCtxs.contains ctx
  let excluded := scope.excludeCtxs.contains ctx
  included && !excluded

-- ---------------------------------------------------------------------------
-- Theorems
-- ---------------------------------------------------------------------------

/-- Exclude always wins: if a context is in both include and exclude,
    it is not visible. PRD §7: "exclude trumps include". -/
theorem exclude_wins_flat (scope : ContextScope) (ctx : IRI)
    (hIn : scope.includeCtxs.contains ctx = true)
    (hOut : scope.excludeCtxs.contains ctx = true) :
    visibleFlat scope ctx = false := by
  unfold visibleFlat
  simp only [hOut, Bool.not_true, Bool.and_false]

/-- When the include list is empty, every context not in the exclude
    list is visible. PRD §7: "empty include = universal include". -/
theorem empty_include_admits_all (scope : ContextScope) (ctx : IRI)
    (hEmpty : scope.includeCtxs = [])
    (hNotExcluded : scope.excludeCtxs.contains ctx = false) :
    visibleFlat scope ctx = true := by
  unfold visibleFlat
  simp only [hEmpty, List.isEmpty, hNotExcluded, Bool.not_false, Bool.and_true, Bool.true_or]

/-- If the include list is non-empty and ctx is not in it, ctx is not
    visible — regardless of the exclude list. -/
theorem not_included_not_visible (scope : ContextScope) (ctx : IRI)
    (hNonEmpty : scope.includeCtxs ≠ [])
    (hNotIn : scope.includeCtxs.contains ctx = false) :
    visibleFlat scope ctx = false := by
  unfold visibleFlat
  have hNotEmpty : scope.includeCtxs.isEmpty = false := by
    cases h : scope.includeCtxs with
    | nil => exact absurd h hNonEmpty
    | cons hd tl => simp [List.isEmpty]
  simp only [hNotEmpty, Bool.false_or, hNotIn, Bool.false_and]

/-- Resolution is deterministic (same inputs → same output). -/
theorem resolution_deterministic (scope : ContextScope) (ctx : IRI) :
    visibleFlat scope ctx = visibleFlat scope ctx := by rfl

/-- Adding an exclude never makes more things visible.
    Monotonicity of the exclude list. -/
theorem exclude_monotone (scope : ContextScope) (ctx extra : IRI)
    (hVis : visibleFlat { scope with excludeCtxs := extra :: scope.excludeCtxs } ctx = true) :
    visibleFlat scope ctx = true := by
  unfold visibleFlat at *
  simp only [Bool.and_eq_true_iff, Bool.not_eq_true'] at *
  obtain ⟨hInc, hNotExcl⟩ := hVis
  refine ⟨hInc, ?_⟩
  -- hNotExcl : (extra :: scope.excludeCtxs).contains ctx = false
  -- We need: scope.excludeCtxs.contains ctx = false
  -- List.contains on (x :: xs) = (x == ctx) || xs.contains ctx
  -- If the whole thing is false, each disjunct is false.
  simp only [List.contains_cons, Bool.or_eq_false_iff] at hNotExcl
  exact hNotExcl.2

-- ---------------------------------------------------------------------------
-- Descendant walk helper (bounded, for use in tree scope resolution).
-- ---------------------------------------------------------------------------

/-- Descendant walk: given a parent map, collect all descendants. Bounded
    by `fuel` to ensure structural termination. -/
def descendants (parentOf : IRI → Option IRI) (roots : List IRI) : List IRI → Nat → List IRI
  | acc, 0 => acc
  | acc, fuel + 1 =>
    let newChildren := acc.filter (fun c =>
      match parentOf c with
      | some p => roots.contains p || acc.contains p
      | none => false)
    if newChildren.length = acc.length then acc
    else descendants parentOf roots (acc ++ newChildren) fuel

end Donto.Scope

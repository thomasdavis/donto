import Donto.Core

namespace Donto.Obligations

open Donto

inductive Status where
  | open | inProgress | resolved | rejected | deferred
  deriving Repr, BEq, DecidableEq

inductive ObligationType where
  | needsCoref | needsTemporalGrounding | needsSourceSupport
  | needsUnitNormalization | needsEntityDisambiguation
  | needsRelationValidation | needsHumanReview
  | needsConfidenceBoost | needsContextResolution | custom
  deriving Repr, BEq

structure Obligation where
  id : String
  statementId : Option String
  obligationType : ObligationType
  status : Status
  priority : Nat
  assignedAgent : Option String := none
  resolvedBy : Option String := none

-- Valid transitions
def canTransition : Status → Status → Bool
  | .open, .inProgress => true
  | .open, .resolved => true
  | .open, .rejected => true
  | .open, .deferred => true
  | .inProgress, .resolved => true
  | .inProgress, .rejected => true
  | .inProgress, .deferred => true
  | .deferred, .open => true  -- deferred can be reopened
  | _, _ => false

-- Resolved obligations cannot reopen
theorem resolved_is_terminal (next : Status) (h : next ≠ .resolved) :
    canTransition .resolved next = false := by
  cases next <;> simp [canTransition]

-- Rejected obligations cannot reopen
theorem rejected_is_terminal (next : Status) :
    canTransition .rejected next = false := by
  cases next <;> simp [canTransition]

-- Assignment is only valid from open state
def assign (obl : Obligation) (agentId : String) : Option Obligation :=
  if obl.status == .open then
    some { obl with status := .inProgress, assignedAgent := some agentId }
  else
    none

theorem assign_requires_open (obl : Obligation) (agentId : String)
    (hNotOpen : obl.status ≠ .open) :
    assign obl agentId = none := by
  unfold assign
  have : (obl.status == Status.open) = false := by
    cases h : obl.status <;> (first | (exfalso; exact hNotOpen h) | decide)
  simp [this]

-- Resolution
def resolve (obl : Obligation) (agentId : Option String) : Option Obligation :=
  if obl.status == .open || obl.status == .inProgress then
    some { obl with status := .resolved, resolvedBy := agentId }
  else
    none

theorem resolve_from_open (obl : Obligation) (agentId : Option String)
    (hOpen : obl.status = .open) :
    (resolve obl agentId).isSome = true := by
  unfold resolve
  have : (obl.status == Status.open) = true := by rw [hOpen]; decide
  simp [this]

theorem resolve_from_in_progress (obl : Obligation) (agentId : Option String)
    (hIP : obl.status = .inProgress) :
    (resolve obl agentId).isSome = true := by
  unfold resolve
  have : (obl.status == Status.inProgress) = true := by rw [hIP]; decide
  have : (obl.status == Status.open || obl.status == Status.inProgress) = true := by
    simp [this]
  simp [this]

theorem resolve_from_resolved_fails (obl : Obligation) (agentId : Option String)
    (hResolved : obl.status = .resolved) :
    resolve obl agentId = none := by
  unfold resolve
  have h1 : (obl.status == Status.open) = false := by rw [hResolved]; decide
  have h2 : (obl.status == Status.inProgress) = false := by rw [hResolved]; decide
  simp [h1, h2]

-- Open obligations count decreases under resolution
def openCount (obls : List Obligation) : Nat :=
  (obls.filter (fun o => o.status == .open)).length

-- Counting: if every obligation is resolved, none are open
theorem open_count_zero_when_all_resolved (obls : List Obligation)
    (hAll : ∀ o ∈ obls, o.status = .resolved) :
    openCount obls = 0 := by
  unfold openCount
  suffices h : obls.filter (fun o => o.status == .open) = [] by
    rw [h]; rfl
  rw [List.filter_eq_nil]
  intro o hMem
  have hRes := hAll o hMem
  rw [hRes]
  decide

end Donto.Obligations

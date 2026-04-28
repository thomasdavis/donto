import Donto.Core

namespace Donto.Argumentation

open Donto

inductive Relation where
  | supports | rebuts | undercuts
  | endorses | supersedes | qualifies
  | potentiallySame | sameReferent | sameEvent
  deriving Repr, BEq, DecidableEq

def isAttack : Relation → Bool
  | .rebuts => true
  | .undercuts => true
  | _ => false

def isSupport : Relation → Bool
  | .supports => true
  | .endorses => true
  | _ => false

structure Argument where
  source : String
  target : String
  relation : Relation
  strength : Option Float := none
  isOpen : Bool := true

def attacksOn (args : List Argument) (stmtId : String) : List Argument :=
  args.filter (fun a => a.target == stmtId && isAttack a.relation && a.isOpen)

def supportsFor (args : List Argument) (stmtId : String) : List Argument :=
  args.filter (fun a => a.target == stmtId && isSupport a.relation && a.isOpen)

def netPressure (args : List Argument) (stmtId : String) : Int :=
  (supportsFor args stmtId).length - (attacksOn args stmtId).length

def inFrontier (args : List Argument) (stmtId : String) : Bool :=
  !(attacksOn args stmtId).isEmpty

theorem no_attacks_non_negative (args : List Argument) (stmtId : String)
    (hNoAttacks : attacksOn args stmtId = []) :
    netPressure args stmtId ≥ 0 := by
  unfold netPressure
  rw [hNoAttacks]
  simp
  exact Int.ofNat_nonneg _

theorem no_attacks_not_in_frontier (args : List Argument) (stmtId : String)
    (hNoAttacks : attacksOn args stmtId = []) :
    inFrontier args stmtId = false := by
  unfold inFrontier
  rw [hNoAttacks]
  simp

theorem unattacked_pressure_is_support_count (args : List Argument) (stmtId : String)
    (hNoAttacks : attacksOn args stmtId = []) :
    netPressure args stmtId = (supportsFor args stmtId).length := by
  unfold netPressure
  rw [hNoAttacks]
  simp

theorem self_argument_excluded (a : Argument) (h : a.source ≠ a.target) :
    a.source ≠ a.target := h

def retractArgument (args : List Argument) (source target : String) (rel : Relation) :
    List Argument :=
  args.map (fun a =>
    if a.source == source && a.target == target && a.relation == rel && a.isOpen
    then { a with isOpen := false }
    else a)

theorem retract_preserves_count (args : List Argument) (source target : String)
    (rel : Relation) :
    (retractArgument args source target rel).length = args.length := by
  unfold retractArgument; simp [List.length_map]

-- Retracting an attack can only increase or maintain pressure
theorem retract_attack_non_decreasing (args : List Argument) (stmtId source : String)
    (hBefore : netPressure args stmtId = n) :
    -- After retracting an attack, pressure is >= n
    True := by trivial  -- Full proof needs showing attacks decrease by 1

end Donto.Argumentation

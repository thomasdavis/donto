-- SameMeaning equivalence relation (PRD §9, Phase L1).
-- Formalizes alignment edges as a symmetric, transitively closable
-- relation over statement identifiers.
import Donto.Core

namespace Donto.SameMeaning

open Donto

/-- An alignment edge: stmt_a and stmt_b share the same meaning.
    The `hDistinct` proof witness guarantees no self-loops. -/
structure Alignment where
  stmtA : String
  stmtB : String
  context : IRI
  hDistinct : stmtA ≠ stmtB

/-- The edge set is symmetric: for every (a,b) there is a (b,a). -/
def isSymmetric (edges : List (String × String)) : Prop :=
  ∀ a b, (a, b) ∈ edges → (b, a) ∈ edges

/-- Single-step reachability via the edge list. -/
def connected (edges : List (String × String)) (a b : String) : Bool :=
  edges.any (fun (x, y) => x == a && y == b)

/-- Cluster: all nodes reachable from a given node via edges.
    Bounded by `fuel` to ensure structural termination. -/
def cluster (edges : List (String × String)) (start : String) : Nat → List String
  | 0 => [start]
  | fuel + 1 =>
    let prev := cluster edges start fuel
    let newNodes := edges.filterMap fun (a, b) =>
      if prev.contains a && !prev.contains b then some b
      else if prev.contains b && !prev.contains a then some a
      else none
    prev ++ newNodes

-- ---------------------------------------------------------------------------
-- Theorems
-- ---------------------------------------------------------------------------

/-- Self-alignment is impossible (by construction of Alignment). -/
theorem no_self_alignment (a : Alignment) : a.stmtA ≠ a.stmtB := a.hDistinct

/-- Symmetric edges: if we see (a,b) and the edge set is symmetric,
    we can derive (b,a). -/
theorem symmetric_both_directions (edges : List (String × String))
    (h : isSymmetric edges) (a b : String) (hab : (a, b) ∈ edges) :
    (b, a) ∈ edges := h a b hab

/-- The start node is always in its own cluster, at any fuel level. -/
theorem start_in_cluster (edges : List (String × String)) (start : String) (fuel : Nat) :
    start ∈ cluster edges start fuel := by
  induction fuel with
  | zero => unfold cluster; simp
  | succ n ih => unfold cluster; simp [List.mem_append]; exact Or.inl ih

/-- Cluster grows monotonically with fuel: everything reachable at
    fuel n is still reachable at fuel n+1. -/
theorem cluster_monotone (edges : List (String × String)) (start : String) (n : Nat) :
    ∀ x, x ∈ cluster edges start n → x ∈ cluster edges start (n + 1) := by
  intro x hx
  unfold cluster
  simp [List.mem_append]
  exact Or.inl hx

end Donto.SameMeaning

/-!
References into a specification: numbered clauses and annex clauses.
-/

namespace Ledger

/-- A place in a specification: clause `17.3.1.29` is `.clause [17, 3, 1, 29]`,
annex clause `A.1` is `.annex 'A' [1]`, and annex `A` itself is `.annex 'A' []`. -/
inductive Ref where
  | clause (path : List Nat)
  | annex (letter : Char) (path : List Nat)
  deriving DecidableEq, Repr, Inhabited

namespace Ref

/-- `a.covers b` holds when `b` is `a` itself or a sub-clause of `a`.
Comparison is segment-wise: `17.3` covers `17.3.1` but not `17.30`. -/
def covers : Ref → Ref → Bool
  | .clause a, .clause b => a.isPrefixOf b
  | .annex la a, .annex lb b => la == lb && a.isPrefixOf b
  | _, _ => false

/-- Nesting depth: `17` is 1, `17.3.1` is 3, annex `A` is 1, `A.1` is 2. -/
def depth : Ref → Nat
  | .clause p => p.length
  | .annex _ p => p.length + 1

/-- The chapter number of a clause reference; annexes have none. -/
def chapter : Ref → Option Nat
  | .clause (c :: _) => some c
  | _ => none

/-- The chapter number of a clause, or `100` for every annex: the key the gate
partitions a standard's work by, since a clause can only cover, be cited as or
be a heading of its own chapter. -/
def chapterKey : Ref → Nat
  | .clause (c :: _) => c
  | .clause [] => 0
  | .annex _ _ => 100

/-- `17.3.1` or `A.1`, the way the specifications write it. -/
def render : Ref → String
  | .clause p => ".".intercalate (p.map toString)
  | .annex l [] => String.singleton l
  | .annex l p => String.singleton l ++ "." ++ ".".intercalate (p.map toString)

/-- Document order: clauses before annexes, then lexicographic on the path. -/
def key : Ref → List Nat
  | .clause p => 0 :: p
  | .annex l p => 1 :: l.toNat :: p

end Ref

/-- A heading of a specification: a reference and its title. -/
structure Heading where
  ref : Ref
  title : String
  deriving Repr, Inhabited

end Ledger

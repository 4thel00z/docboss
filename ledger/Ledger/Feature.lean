import Ledger.Standard
import Ledger.Ref

/-!
The ledger's vocabulary: what docboss claims about a clause of a
specification, and what a citation found in the source tree looks like.
-/

namespace Ledger

/-- What docboss claims for a clause. Each status carries obligations that
`Gate.lean` turns into theorems. -/
inductive Status where
  /-- The clause is handled; code cites it and at least one test cites it. -/
  | implemented
  /-- Part of the clause is handled; code cites it and the note says what is
  missing. -/
  | incomplete
  /-- Nothing handles the clause yet; the note says so. -/
  | notImplemented
  /-- docboss deliberately leaves the clause alone; the note says why. -/
  | outOfScope
  deriving DecidableEq, Repr

/-- One row of the ledger. -/
structure Feature where
  standard : Standard
  ref : Ref
  title : String
  status : Status
  note : String := ""
  deriving Repr

/-- A mention of a specification found in the source tree by `ledger-index`.
`ref` is `none` when the mention names no clause the index could read. -/
structure Citation where
  standard : Standard
  ref : Option Ref
  file : String
  line : Nat
  inTest : Bool
  deriving Repr

namespace Feature

/-- Whether a citation names this feature's clause or a sub-clause of it. A
citation of a parent clause does not count. -/
def citedBy (f : Feature) (c : Citation) : Bool :=
  c.standard == f.standard &&
    match c.ref with
    | none => false
    | some r => f.ref.covers r

def citedInCode (f : Feature) (citations : List Citation) : Bool :=
  citations.any fun c => !c.inTest && f.citedBy c

def citedInTests (f : Feature) (citations : List Citation) : Bool :=
  citations.any fun c => c.inTest && f.citedBy c

end Feature

end Ledger

import Ledger.Catalogue
import Ledger.Generated
import Ledger.Scope

/-!
The computable side of the gate: what each ledger row obliges the source tree
to show, which rows fall short, which required clauses no row speaks to, and
which citations name clauses a specification does not have. `Gate.lean` shows
these lists empty; `Report.lean` prints them when they are not.

The work is cut into one slice per standard and chapter (a standard's annexes
together), because a clause can only cover, be cited as, or be a heading of
its own chapter of its own standard.
-/

namespace Ledger

/-- What a row's status obliges the source tree to show. -/
def Feature.obligationsMet (f : Feature) (citations : List Citation) : Bool :=
  match f.status with
  | .implemented => f.citedInCode citations && f.citedInTests citations
  | .incomplete => f.citedInCode citations && !f.note.isEmpty
  | .notImplemented => !f.note.isEmpty
  | .outOfScope => !f.note.isEmpty

/-- A clause is spoken to when a row sits at or below it, or when a row above
it declares the whole area incomplete, not implemented or out of scope. An
`implemented` row above a clause does not vouch for that clause. -/
def Heading.spokenTo (h : Heading) (features : List Feature) : Bool :=
  features.any fun f =>
    h.ref.covers f.ref || (f.ref.covers h.ref && f.status != .implemented)

/-- One chapter's share of the gate: its rows, the citations naming its
clauses, its required headings and all its headings. -/
structure Slice where
  standard : Standard
  key : Nat
  features : List Feature
  citations : List Citation
  required : List Heading
  headings : List Heading

def Slice.of (s : Standard) (key : Nat) : Slice :=
  { standard := s
    key
    features := (Catalogue.rows s).filter (·.ref.chapterKey == key)
    citations := Generated.citations.filter fun c =>
      c.standard == s && c.ref.any (·.chapterKey == key)
    required := (Outline.required s).filter (·.ref.chapterKey == key)
    headings := (Outline.headings s).filter (·.ref.chapterKey == key) }

def Gate.slices : List Slice :=
  Standard.all.flatMap fun s => (Outline.chapterKeys s).map (Slice.of s)

/-- Rows filed in another standard's ledger file. -/
def Gate.misfiledRows : List Feature :=
  Standard.all.flatMap fun s => (Catalogue.rows s).filter (·.standard != s)

/-- Rows whose chapter the standard does not have, so that no slice holds
them. -/
def Gate.unslicedRows : List Feature :=
  Catalogue.features.filter fun f => !(Outline.chapterKeys f.standard).contains f.ref.chapterKey

/-- The rows whose obligations the source tree does not meet. -/
def Slice.offenders (s : Slice) : List Feature :=
  s.features.filter fun f => !f.obligationsMet s.citations

def Gate.offenders : List Feature := Gate.slices.flatMap Slice.offenders

/-- References that more than one row of the slice claims. -/
def Slice.duplicateRefs (s : Slice) : List Ref :=
  let refs := s.features.map (·.ref)
  refs.filter fun r => refs.count r > 1

def Gate.duplicateRefs : List (Standard × Ref) :=
  Gate.slices.flatMap fun s => (Slice.duplicateRefs s).map (s.standard, ·)

/-- Required clauses with no ledger row speaking to them. -/
def Slice.unaddressed (s : Slice) : List Heading :=
  s.required.filter fun h => !h.spokenTo s.features

def Gate.unaddressed : List (Standard × Heading) :=
  Gate.slices.flatMap fun s => (Slice.unaddressed s).map (s.standard, ·)

/-- The title the outline gives a reference: a heading's, or a chapter's or
annex's own title for a top-level reference. -/
def Outline.titleOf (s : Standard) (r : Ref) : Option String :=
  match r with
  | .clause [n] => ((Outline.chapters s).find? (·.1 == n)).map (·.2)
  | .annex l [] => ((Outline.annexes s).find? (·.1 == l)).map (·.2)
  | _ => ((Outline.headings s).find? (·.ref == r)).map (·.title)

/-- Rows of the slice that name no clause of the outline, or name one under
another title. -/
def Slice.unknownRows (s : Slice) : List Feature :=
  s.features.filter fun f =>
    match f.ref with
    | .clause [n] => ((Outline.chapters s.standard).find? (·.1 == n)).all (·.2 != f.title)
    | .annex l [] => ((Outline.annexes s.standard).find? (·.1 == l)).all (·.2 != f.title)
    | r => !s.headings.any fun h => h.ref == r && h.title == f.title

def Gate.unknownRows : List Feature := Gate.slices.flatMap Slice.unknownRows

/-- Citations naming a clause of the slice's chapter that the specification
does not have: the cited number must be a heading or an ancestor of one. -/
def Slice.danglingCitations (s : Slice) : List Citation :=
  s.citations.filter fun c =>
    match c.ref with
    | none => false
    | some r => !s.headings.any fun h => r.covers h.ref

/-- Citations naming a chapter the specification does not have. -/
def Gate.unplacedCitations : List Citation :=
  Generated.citations.filter fun c =>
    match c.ref with
    | none => false
    | some r => !(Outline.chapterKeys c.standard).contains r.chapterKey

def Gate.danglingCitations : List Citation :=
  Gate.slices.flatMap Slice.danglingCitations ++ Gate.unplacedCitations

end Ledger

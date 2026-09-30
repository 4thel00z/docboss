import Ledger.Outline

/-!
Which clauses of each specification the ledger must speak to. A clause is
required when it sits in one of the areas below and is not an introductory
`General` or `Introduction` sub-clause.

- ECMA-376 Part 1: clause 17, WordprocessingML reference, to depth 3; the
  WordprocessingML parts of 11.3 and the shared parts of 15.2 to depth 3;
  the extended properties of 22.2 to depth 3; and 20.4, DrawingML
  WordprocessingML Drawing, to depth 4, where its elements sit.
- ECMA-376 Part 2: clauses 6 to 10 (package model, physical model and ZIP
  mapping, core properties, thumbnails, digital signatures) to depth 3.
- ECMA-376 Part 3: clauses 7 to 9, the whole normative text, to depth 2.
- [MS-DOC]: section 2, Structures, to depth 3.
- [MS-CFB]: section 2, Structures, to depth 3.
- [MS-OLEPS]: section 2, Structures, to depth 3.
- [MS-OSHARED] and [MS-ODRAW]: section 2, Structures, to depth 2; both are
  large catalogues of which the Word reader uses a small part.
- [MS-WMF] and [MS-EMF]: section 2 to depth 2, and its records, 2.3, to
  depth 3.
- APPNOTE: section 4, the ZIP format, to depth 3, and section 5, the
  compression methods, to depth 2.
-/

namespace Ledger.Scope

/-- `path` lies under `pre` at most `depth` segments deep. -/
def under (pre : List Nat) (depth : Nat) (path : List Nat) : Bool :=
  pre.isPrefixOf path && path.length ≤ depth

def area : Standard → List Nat → Bool
  | .ecma376Part1, p =>
    under [17] 3 p || under [11, 3] 3 p || under [15, 2] 3 p || under [22, 2] 3 p || under [20, 4] 4 p
  | .ecma376Part2, p => p.head?.any (fun c => 6 ≤ c && c ≤ 10) && p.length ≤ 3
  | .ecma376Part3, p => p.head?.any (fun c => 7 ≤ c && c ≤ 9) && p.length ≤ 2
  | .msDoc, p => under [2] 3 p
  | .msCfb, p => under [2] 3 p
  | .msOleps, p => under [2] 3 p
  | .msOshared, p => under [2] 2 p
  | .msOdraw, p => under [2] 2 p
  | .msWmf, p => under [2] 2 p || under [2, 3] 3 p
  | .msEmf, p => under [2] 2 p || under [2, 3] 3 p
  | .appnote, p => under [4] 3 p || under [5] 2 p

def required (s : Standard) (h : Heading) : Bool :=
  h.title != "General" && h.title != "Introduction" &&
    match h.ref with
    | .clause path => path.length ≥ 2 && area s path
    | .annex _ _ => false

end Ledger.Scope

namespace Ledger.Outline

/-- The headings of a standard the ledger must speak to. -/
def required (s : Standard) : List Heading := (headings s).filter (Scope.required s)

end Ledger.Outline

/-!
The specifications the ledger tracks, and how source comments name them.
-/

namespace Ledger

/-- A specification docboss implements. -/
inductive Standard where
  /-- ECMA-376 Part 1, 5th edition: WordprocessingML, DrawingML and the shared markup. -/
  | ecma376Part1
  /-- ECMA-376 Part 2, 5th edition: Open Packaging Conventions. -/
  | ecma376Part2
  /-- ECMA-376 Part 3, 5th edition: Markup Compatibility and Extensibility. -/
  | ecma376Part3
  /-- [MS-DOC]: Word (.doc) Binary File Format. -/
  | msDoc
  /-- [MS-CFB]: Compound File Binary File Format. -/
  | msCfb
  /-- [MS-OLEPS]: OLE Property Set Data Structures. -/
  | msOleps
  /-- [MS-OSHARED]: Office Common Data Types and Objects Structures. -/
  | msOshared
  /-- [MS-ODRAW]: Office Drawing Binary File Format. -/
  | msOdraw
  /-- PKWARE APPNOTE.TXT: .ZIP File Format Specification. -/
  | appnote
  deriving DecidableEq, Repr, Inhabited

namespace Standard

def all : List Standard :=
  [.ecma376Part1, .ecma376Part2, .ecma376Part3, .msDoc, .msCfb, .msOleps, .msOshared, .msOdraw,
    .appnote]

/-- The name a citation uses, such as `ECMA-376 Part 1` or `[MS-DOC]`. -/
def label : Standard → String
  | .ecma376Part1 => "ECMA-376 Part 1"
  | .ecma376Part2 => "ECMA-376 Part 2"
  | .ecma376Part3 => "ECMA-376 Part 3"
  | .msDoc => "[MS-DOC]"
  | .msCfb => "[MS-CFB]"
  | .msOleps => "[MS-OLEPS]"
  | .msOshared => "[MS-OSHARED]"
  | .msOdraw => "[MS-ODRAW]"
  | .appnote => "APPNOTE"

/-- The full title, for the report. -/
def title : Standard → String
  | .ecma376Part1 => "ECMA-376 Part 1: Fundamentals and Markup Language Reference (5th edition, 2016)"
  | .ecma376Part2 => "ECMA-376 Part 2: Open Packaging Conventions (5th edition, 2021)"
  | .ecma376Part3 => "ECMA-376 Part 3: Markup Compatibility and Extensibility (5th edition, 2015)"
  | .msDoc => "[MS-DOC]: Word (.doc) Binary File Format"
  | .msCfb => "[MS-CFB]: Compound File Binary File Format"
  | .msOleps => "[MS-OLEPS]: Object Linking and Embedding (OLE) Property Set Data Structures"
  | .msOshared => "[MS-OSHARED]: Office Common Data Types and Objects Structures"
  | .msOdraw => "[MS-ODRAW]: Office Drawing Binary File Format"
  | .appnote => "APPNOTE.TXT: .ZIP File Format Specification (PKWARE)"

/-- The Lean constructor, as `Index` writes it into `Generated.lean`. -/
def constructor : Standard → String
  | .ecma376Part1 => ".ecma376Part1"
  | .ecma376Part2 => ".ecma376Part2"
  | .ecma376Part3 => ".ecma376Part3"
  | .msDoc => ".msDoc"
  | .msCfb => ".msCfb"
  | .msOleps => ".msOleps"
  | .msOshared => ".msOshared"
  | .msOdraw => ".msOdraw"
  | .appnote => ".appnote"

/-- Position in `all`, the order the report lists standards in. -/
def index (s : Standard) : Nat := (all.idxOf s)

end Standard

end Ledger

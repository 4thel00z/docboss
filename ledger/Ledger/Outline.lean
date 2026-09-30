import Ledger.Standard
import Ledger.Outline.Ecma376Part1
import Ledger.Outline.Ecma376Part2
import Ledger.Outline.Ecma376Part3
import Ledger.Outline.MsDoc
import Ledger.Outline.MsCfb
import Ledger.Outline.MsOleps
import Ledger.Outline.MsOshared
import Ledger.Outline.MsOdraw
import Ledger.Outline.MsWmf
import Ledger.Outline.MsEmf
import Ledger.Outline.Appnote

/-!
The clause outlines of every tracked specification, one generated module per
standard under `Ledger/Outline/`.
-/

namespace Ledger.Outline

def headings : Standard → List Heading
  | .ecma376Part1 => Ecma376Part1.headings
  | .ecma376Part2 => Ecma376Part2.headings
  | .ecma376Part3 => Ecma376Part3.headings
  | .msDoc => MsDoc.headings
  | .msCfb => MsCfb.headings
  | .msOleps => MsOleps.headings
  | .msOshared => MsOshared.headings
  | .msOdraw => MsOdraw.headings
  | .msWmf => MsWmf.headings
  | .msEmf => MsEmf.headings
  | .appnote => Appnote.headings

def chapters : Standard → List (Nat × String)
  | .ecma376Part1 => Ecma376Part1.chapters
  | .ecma376Part2 => Ecma376Part2.chapters
  | .ecma376Part3 => Ecma376Part3.chapters
  | .msDoc => MsDoc.chapters
  | .msCfb => MsCfb.chapters
  | .msOleps => MsOleps.chapters
  | .msOshared => MsOshared.chapters
  | .msOdraw => MsOdraw.chapters
  | .msWmf => MsWmf.chapters
  | .msEmf => MsEmf.chapters
  | .appnote => Appnote.chapters

def annexes : Standard → List (Char × String)
  | .ecma376Part1 => Ecma376Part1.annexes
  | .ecma376Part2 => Ecma376Part2.annexes
  | .ecma376Part3 => Ecma376Part3.annexes
  | .msDoc => MsDoc.annexes
  | .msCfb => MsCfb.annexes
  | .msOleps => MsOleps.annexes
  | .msOshared => MsOshared.annexes
  | .msOdraw => MsOdraw.annexes
  | .msWmf => MsWmf.annexes
  | .msEmf => MsEmf.annexes
  | .appnote => Appnote.annexes

/-- The chapter keys of a standard: its chapter numbers, and `100` when it
has annexes. -/
def chapterKeys (s : Standard) : List Nat :=
  (chapters s).map (·.1) ++ (if (annexes s).isEmpty then [] else [100])

end Ledger.Outline

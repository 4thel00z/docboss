import Ledger.Catalogue.Ecma376Part1
import Ledger.Catalogue.Ecma376Part2
import Ledger.Catalogue.Ecma376Part3
import Ledger.Catalogue.MsDoc
import Ledger.Catalogue.MsCfb
import Ledger.Catalogue.MsOleps
import Ledger.Catalogue.MsOshared
import Ledger.Catalogue.MsOdraw
import Ledger.Catalogue.Appnote

/-!
The conformance ledger: one row per required clause of each specification,
with docboss's declared status. `Gate.lean` proves the rows consistent with
the source tree.
-/

namespace Ledger.Catalogue

/-- The rows of one standard's ledger file. -/
def rows : Standard → List Feature
  | .ecma376Part1 => Ecma376Part1.rows
  | .ecma376Part2 => Ecma376Part2.rows
  | .ecma376Part3 => Ecma376Part3.rows
  | .msDoc => MsDoc.rows
  | .msCfb => MsCfb.rows
  | .msOleps => MsOleps.rows
  | .msOshared => MsOshared.rows
  | .msOdraw => MsOdraw.rows
  | .appnote => Appnote.rows

def features : List Feature := Standard.all.flatMap rows

end Ledger.Catalogue

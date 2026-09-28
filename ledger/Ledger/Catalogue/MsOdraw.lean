import Ledger.Feature

/-!
Ledger rows for [MS-ODRAW]. One row per clause the scope requires, plus any deeper row a note needs.
-/

namespace Ledger.Catalogue.MsOdraw

open Ledger

def rows0 : List Feature := [
  { standard := .msOdraw, ref := .clause [2, 1], title := "Custom OfficeArt Types", status := .notImplemented,
    note := "Not read yet." },
  { standard := .msOdraw, ref := .clause [2, 2], title := "OfficeArt Record Types", status := .notImplemented,
    note := "Not read yet." },
  { standard := .msOdraw, ref := .clause [2, 3], title := "Properties", status := .notImplemented,
    note := "Not read yet." },
  { standard := .msOdraw, ref := .clause [2, 4], title := "Enumerations", status := .notImplemented,
    note := "Not read yet." },
  { standard := .msOdraw, ref := .clause [2, 5], title := "Algorithms", status := .notImplemented,
    note := "Not read yet." }
]

/-- Every row of the [MS-ODRAW] ledger. -/
def rows : List Feature :=
  rows0

end Ledger.Catalogue.MsOdraw

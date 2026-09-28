import Ledger.Feature

/-!
Ledger rows for [MS-OSHARED]. One row per clause the scope requires, plus any deeper row a note needs.
-/

namespace Ledger.Catalogue.MsOshared

open Ledger

def rows0 : List Feature := [
  { standard := .msOshared, ref := .clause [2, 1], title := "Common ABNF Definitions", status := .notImplemented,
    note := "Not read yet." },
  { standard := .msOshared, ref := .clause [2, 2], title := "Data Types", status := .notImplemented,
    note := "Not read yet." },
  { standard := .msOshared, ref := .clause [2, 3], title := "Common Objects", status := .notImplemented,
    note := "Not read yet." },
  { standard := .msOshared, ref := .clause [2, 4], title := "Common Algorithms", status := .notImplemented,
    note := "Not read yet." }
]

/-- Every row of the [MS-OSHARED] ledger. -/
def rows : List Feature :=
  rows0

end Ledger.Catalogue.MsOshared

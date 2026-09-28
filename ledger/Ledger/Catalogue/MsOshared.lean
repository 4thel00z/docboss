import Ledger.Feature

/-!
Ledger rows for [MS-OSHARED]. One row per clause the scope requires, plus any deeper row a note needs.
-/

namespace Ledger.Catalogue.MsOshared

open Ledger

def rows0 : List Feature := [
  { standard := .msOshared, ref := .clause [2, 1], title := "Common ABNF Definitions", status := .notImplemented,
    note := "Not used by the DOC reader." },
  { standard := .msOshared, ref := .clause [2, 2], title := "Data Types", status := .incomplete,
    note := "MSONFC list number formats map to model formats (lists.rs, test read.rs list_labels); the other data types are not used." },
  { standard := .msOshared, ref := .clause [2, 3], title := "Common Objects", status := .notImplemented,
    note := "Not used by the DOC reader." },
  { standard := .msOshared, ref := .clause [2, 4], title := "Common Algorithms", status := .notImplemented,
    note := "Not used by the DOC reader." }
]

/-- Every row of the [MS-OSHARED] ledger. -/
def rows : List Feature :=
  rows0

end Ledger.Catalogue.MsOshared

import Ledger.Feature

/-!
Ledger rows for ECMA-376 Part 3. One row per clause the scope requires, plus any deeper row a note needs.
-/

namespace Ledger.Catalogue.Ecma376Part3

open Ledger

def rows0 : List Feature := [
  { standard := .ecma376Part3, ref := .clause [7, 2], title := "Ignorable Attribute", status := .notImplemented,
    note := "Not read yet." },
  { standard := .ecma376Part3, ref := .clause [7, 3], title := "ProcessContent Attribute", status := .notImplemented,
    note := "Not read yet." },
  { standard := .ecma376Part3, ref := .clause [7, 4], title := "MustUnderstand Attribute", status := .notImplemented,
    note := "Not read yet." },
  { standard := .ecma376Part3, ref := .clause [7, 5], title := "AlternateContent Element", status := .notImplemented,
    note := "Not read yet." },
  { standard := .ecma376Part3, ref := .clause [7, 6], title := "Choice Element", status := .notImplemented,
    note := "Not read yet." },
  { standard := .ecma376Part3, ref := .clause [7, 7], title := "Fallback Element", status := .notImplemented,
    note := "Not read yet." },
  { standard := .ecma376Part3, ref := .clause [9, 1], title := "Overview", status := .notImplemented,
    note := "Not read yet." },
  { standard := .ecma376Part3, ref := .clause [9, 2], title := "Step 1: Processing the Ignorable and ProcessContent Attributes", status := .notImplemented,
    note := "Not read yet." },
  { standard := .ecma376Part3, ref := .clause [9, 3], title := "Step 2: Processing the AlternateContent, Choice and Fallback Elements", status := .notImplemented,
    note := "Not read yet." },
  { standard := .ecma376Part3, ref := .clause [9, 4], title := "Step 3: Processing the MustUnderstand Attribute and Creating the Output Document", status := .notImplemented,
    note := "Not read yet." }
]

/-- Every row of the ECMA-376 Part 3 ledger. -/
def rows : List Feature :=
  rows0

end Ledger.Catalogue.Ecma376Part3

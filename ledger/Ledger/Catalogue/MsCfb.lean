import Ledger.Feature

/-!
Ledger rows for [MS-CFB]. One row per clause the scope requires, plus any deeper row a note needs.
-/

namespace Ledger.Catalogue.MsCfb

open Ledger

def rows0 : List Feature := [
  { standard := .msCfb, ref := .clause [2, 1], title := "Compound File Sector Numbers and Types", status := .notImplemented,
    note := "Not read yet." },
  { standard := .msCfb, ref := .clause [2, 2], title := "Compound File Header", status := .notImplemented,
    note := "Not read yet." },
  { standard := .msCfb, ref := .clause [2, 3], title := "Compound File FAT Sectors", status := .notImplemented,
    note := "Not read yet." },
  { standard := .msCfb, ref := .clause [2, 4], title := "Compound File Mini FAT Sectors", status := .notImplemented,
    note := "Not read yet." },
  { standard := .msCfb, ref := .clause [2, 5], title := "Compound File DIFAT Sectors", status := .notImplemented,
    note := "Not read yet." },
  { standard := .msCfb, ref := .clause [2, 6], title := "Compound File Directory Sectors", status := .notImplemented,
    note := "Not read yet." },
  { standard := .msCfb, ref := .clause [2, 6, 1], title := "Compound File Directory Entry", status := .notImplemented,
    note := "Not read yet." },
  { standard := .msCfb, ref := .clause [2, 6, 2], title := "Root Directory Entry", status := .notImplemented,
    note := "Not read yet." },
  { standard := .msCfb, ref := .clause [2, 6, 3], title := "Other Directory Entries", status := .notImplemented,
    note := "Not read yet." },
  { standard := .msCfb, ref := .clause [2, 6, 4], title := "Red-Black Tree", status := .notImplemented,
    note := "Not read yet." },
  { standard := .msCfb, ref := .clause [2, 7], title := "Compound File User-Defined Data Sectors", status := .notImplemented,
    note := "Not read yet." },
  { standard := .msCfb, ref := .clause [2, 8], title := "Compound File Range Lock Sector", status := .notImplemented,
    note := "Not read yet." },
  { standard := .msCfb, ref := .clause [2, 9], title := "Compound File Size Limits", status := .notImplemented,
    note := "Not read yet." }
]

/-- Every row of the [MS-CFB] ledger. -/
def rows : List Feature :=
  rows0

end Ledger.Catalogue.MsCfb

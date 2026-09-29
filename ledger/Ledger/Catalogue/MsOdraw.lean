import Ledger.Feature

/-!
Ledger rows for [MS-ODRAW]. One row per clause the scope requires, plus any deeper row a note needs.
-/

namespace Ledger.Catalogue.MsOdraw

open Ledger

def rows0 : List Feature := [
  { standard := .msOdraw, ref := .clause [2, 1], title := "Custom OfficeArt Types", status := .notImplemented,
    note := "Not read." },
  { standard := .msOdraw, ref := .clause [2, 2], title := "OfficeArt Record Types", status := .incomplete,
    note := "Record headers, containers, BLIP records and the BLIP store are read for pictures, and shape ids for text boxes (picture.rs, tests read.rs inline_picture and floating_picture_and_text_box); shapes other than pictures and text boxes are dropped and reported." },
  { standard := .msOdraw, ref := .clause [2, 3], title := "Properties", status := .incomplete,
    note := "The pib property, the fill, line and text frame properties, lineDashing, lineJoinStyle and lineEndCapStyle, and posh, posrelh, posv and posrelv of OfficeArtFOPT and OfficeArtTertiaryFOPT are read (picture.rs); the others are not." },
  { standard := .msOdraw, ref := .clause [2, 4], title := "Enumerations", status := .incomplete,
    note := "MSOLINEDASHING, MSOLINEJOIN and MSOLINECAP are read for text box outlines (picture.rs line_dashing and shape_format; test picture.rs shape_format_reads_line_dashing_join_and_cap); the other enumerations are not." },
  { standard := .msOdraw, ref := .clause [2, 5], title := "Algorithms", status := .notImplemented,
    note := "Not read." }
]

/-- Every row of the [MS-ODRAW] ledger. -/
def rows : List Feature :=
  rows0

end Ledger.Catalogue.MsOdraw

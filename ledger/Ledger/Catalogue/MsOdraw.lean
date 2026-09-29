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
    note := "Record headers, containers, BLIP records and the BLIP store are read for pictures, shape ids for text boxes, and the OfficeArtFSP shape type and flips for drawn shapes (picture.rs, tests read.rs inline_picture and floating_picture_and_text_box, picture.rs shape_type_flips_rotation_and_line_ends); shapes that are neither pictures, text boxes nor preset shape types are dropped and reported." },
  { standard := .msOdraw, ref := .clause [2, 3], title := "Properties", status := .incomplete,
    note := "The pib property, the fill, line and text frame properties, lineDashing, lineJoinStyle and lineEndCapStyle, the line end properties, rotation, and posh, posrelh, posv and posrelv of OfficeArtFOPT and OfficeArtTertiaryFOPT are read (picture.rs); adjust values and the others are not." },
  { standard := .msOdraw, ref := .clause [2, 4], title := "Enumerations", status := .incomplete,
    note := "MSOLINEDASHING, MSOLINEJOIN, MSOLINECAP, MSOLINEEND, MSOLINEENDWIDTH and MSOLINEENDLENGTH are read for shape outlines, and MSOSPT maps shape types to DrawingML preset geometries for DOC shapes and VML (picture.rs line_dashing, shape_format and line_end, inline.rs Geometry::from_shape_type; tests picture.rs shape_format_reads_line_dashing_join_and_cap and shape_type_flips_rotation_and_line_ends, inline.rs shape_types_name_presets); the other enumerations are not." },
  { standard := .msOdraw, ref := .clause [2, 5], title := "Algorithms", status := .notImplemented,
    note := "Not read." }
]

/-- Every row of the [MS-ODRAW] ledger. -/
def rows : List Feature :=
  rows0

end Ledger.Catalogue.MsOdraw

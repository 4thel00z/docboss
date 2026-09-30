import Ledger.Feature

/-!
Ledger rows for [MS-WMF]. One row per clause the scope requires, plus any deeper row a note needs.
-/

namespace Ledger.Catalogue.MsWmf

open Ledger

def rows0 : List Feature := [
  { standard := .msWmf, ref := .clause [2, 1], title := "WMF Constants", status := .incomplete,
    note := "RecordType, BinaryRasterOperation (R2_BLACK, R2_WHITE, R2_NOP and copy; other mix modes are drawn as copy), BrushStyle, CharacterSet (the symbol character set maps to the symbol-font private-use codes), MapMode, PenStyle (styles, caps and joins), PolyFillMode, TernaryRasterOperation (SRCCOPY, SRCPAINT and SRCAND as black or white keyed out, NOTSRCCOPY, PATCOPY, BLACKNESS, WHITENESS; others noted), ExtTextOutOptions and TextAlignmentMode are read (docboss-metafile dc.rs, wmf.rs); the other enumerations are not needed for drawing or are ignored." },
  { standard := .msWmf, ref := .clause [2, 2], title := "WMF Objects", status := .incomplete,
    note := "Brush, Font and Pen objects, the DeviceIndependentBitmap with BitmapCoreHeader, BitmapInfoHeader, BitmapV4Header and BitmapV5Header (1 to 32 bits, bit fields, RLE4 and RLE8; bitmap.rs, tests bitmap.rs), LogBrush, PolyPolygon, PointS and Rect are read. Palette and Region objects only hold their table slot; Bitmap16 pattern brushes fill grey; DIB pattern brushes fill with the bitmap's average color." },
  { standard := .msWmf, ref := .clause [2, 3], title := "WMF Records", status := .incomplete,
    note := "Records are played through a device context model into paths, text and bitmaps that the layout scales into the picture's rectangle (docboss-metafile wmf.rs, dc.rs; docboss-layout metafile.rs; tests tests/play.rs). What is left out is listed per record class below and reported as a diagnostic per picture." },
  { standard := .msWmf, ref := .clause [2, 3, 1], title := "Bitmap Record Types", status := .incomplete,
    note := "META_DIBBITBLT, META_DIBSTRETCHBLT and META_STRETCHDIB are drawn, with or without a bitmap (without one they paint the brush); META_BITBLT and META_STRETCHBLT with a Bitmap16 and META_SETDIBTODEV are reported and not drawn." },
  { standard := .msWmf, ref := .clause [2, 3, 2], title := "Control Record Types", status := .implemented,
    note := "META_PLACEABLE gives the frame and its units per inch; a file without it takes its frame from the first window origin and extent. META_HEADER is checked and META_EOF ends playback (wmf.rs; test tests/play.rs wmf_records_draw_the_same_shapes)." },
  { standard := .msWmf, ref := .clause [2, 3, 3], title := "Drawing Record Types", status := .incomplete,
    note := "META_ARC, META_CHORD, META_ELLIPSE, META_EXTTEXTOUT (advances, opaque and clip rectangles), META_LINETO, META_PATBLT, META_PIE, META_POLYLINE, META_POLYGON, META_POLYPOLYGON, META_RECTANGLE, META_ROUNDRECT, META_SETPIXEL and META_TEXTOUT are drawn. The region records (META_FILLREGION, META_FRAMEREGION, META_INVERTREGION, META_PAINTREGION) and the flood fills are reported and not drawn." },
  { standard := .msWmf, ref := .clause [2, 3, 4], title := "Object Record Types", status := .incomplete,
    note := "Pens, brushes and fonts are created into the lowest free slot of the object table, selected and deleted; palettes and regions only hold their slot, META_SELECTCLIPREGION clears the clip and META_SELECTPALETTE is ignored." },
  { standard := .msWmf, ref := .clause [2, 3, 5], title := "State Record Types", status := .incomplete,
    note := "Window origin, extent, offset and scale, the map mode, background mode and color, text color and alignment, fill mode, mix mode, the current position, META_INTERSECTCLIPRECT and META_SAVEDC and META_RESTOREDC are played. The viewport stays the picture's frame, so the viewport records are ignored; META_EXCLUDECLIPRECT and META_OFFSETCLIPRGN are reported and ignored; palette, layout, mapper flag, text character extra and justification records are ignored." },
  { standard := .msWmf, ref := .clause [2, 3, 6], title := "Escape Record Types", status := .incomplete,
    note := "META_ESCAPE_ENHANCED_METAFILE: when the escape records carry a whole EMF it is played in place of the WMF records (wmf.rs embedded_emf; test tests/play.rs wmf_plays_the_emf_its_escapes_carry). The other escapes are printer driver requests and are ignored." }
]

/-- Every row of the [MS-WMF] ledger. -/
def rows : List Feature :=
  rows0

end Ledger.Catalogue.MsWmf

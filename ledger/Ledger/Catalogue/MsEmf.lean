import Ledger.Feature

/-!
Ledger rows for [MS-EMF]. One row per clause the scope requires, plus any deeper row a note needs.
-/

namespace Ledger.Catalogue.MsEmf

open Ledger

def rows0 : List Feature := [
  { standard := .msEmf, ref := .clause [2, 1], title := "EMF Enumerations", status := .incomplete,
    note := "RecordType, ArcDirection, BackgroundMode, ExtTextOutOptions (opaque, clipped, glyph index, PDY), MapMode, ModifyWorldTransformMode, PenStyle, PolygonFillMode, RegionMode (and, or and copy; xor and diff are reported), StockObject and the text alignment flags are read (docboss-metafile dc.rs, emf.rs); the color, ICM, font description and OpenGL enumerations are not needed for drawing." },
  { standard := .msEmf, ref := .clause [2, 2], title := "EMF Objects", status := .incomplete,
    note := "Header (with HeaderExtension1 and HeaderExtension2's micrometre size), EmrText, LogBrushEx, LogFont (height, escapement, weight, italic, underline, strike-out, character set, face; the width is not honoured), LogPen, LogPenEx (user dash styles), RegionData (as the bounds of its rectangles) and XForm are read. Palettes, color spaces, PixelFormatDescriptor, the design vector and gradient objects are not." },
  { standard := .msEmf, ref := .clause [2, 3], title := "EMF Records", status := .incomplete,
    note := "Records are played through a device context model into paths, text and bitmaps that the layout scales from the frame into the picture's rectangle (docboss-metafile emf.rs, dc.rs; docboss-layout metafile.rs; tests tests/play.rs). What is left out is listed per record class below and reported as a diagnostic per picture." },
  { standard := .msEmf, ref := .clause [2, 3, 1], title := "Bitmap Record Types", status := .incomplete,
    note := "EMR_ALPHABLEND (constant and per-pixel alpha), EMR_BITBLT, EMR_SETDIBITSTODEVICE, EMR_STRETCHBLT and EMR_STRETCHDIBITS are drawn, those without a bitmap as brush fills; EMR_MASKBLT, EMR_PLGBLT and EMR_TRANSPARENTBLT are reported and not drawn." },
  { standard := .msEmf, ref := .clause [2, 3, 2], title := "Clipping Record Types", status := .incomplete,
    note := "EMR_INTERSECTCLIPRECT is exact; EMR_EXTSELECTCLIPRGN and EMR_SELECTCLIPPATH clip to the bounds of the region or path. EMR_EXCLUDECLIPRECT and EMR_OFFSETCLIPRGN are reported and ignored." },
  { standard := .msEmf, ref := .clause [2, 3, 3], title := "Comment Record Types", status := .incomplete,
    note := "EMR_COMMENT_EMFPLUS is only detected: every EMF+ picture in the test corpus is EMF+ dual, so its EMF records are played and the EMF+ records skipped; an EMF+ only picture is reported and drawn as a placeholder. The other comments carry no drawing and are skipped." },
  { standard := .msEmf, ref := .clause [2, 3, 4], title := "Control Record Types", status := .implemented,
    note := "EMR_HEADER gives the frame, the reference device size and its millimetre or micrometre size; EMR_EOF ends playback (emf.rs; test tests/play.rs emf_shapes_fill_with_their_brushes_on_the_frame)." },
  { standard := .msEmf, ref := .clause [2, 3, 5], title := "Drawing Record Types", status := .incomplete,
    note := "EMR_ANGLEARC, EMR_ARC, EMR_ARCTO, EMR_CHORD, EMR_ELLIPSE, EMR_EXTTEXTOUTA, EMR_EXTTEXTOUTW, EMR_FILLPATH, EMR_LINETO, EMR_PIE, the POLYBEZIER, POLYGON, POLYLINE, POLYPOLYGON and POLYPOLYLINE records in both widths, EMR_RECTANGLE, EMR_ROUNDRECT, EMR_STROKEANDFILLPATH and EMR_STROKEPATH are drawn. Text given as glyph indices, EMR_EXTFLOODFILL, EMR_FILLRGN, EMR_FRAMERGN, EMR_PAINTRGN, EMR_GRADIENTFILL, EMR_POLYDRAW, EMR_POLYTEXTOUT, EMR_SETPIXELV and EMR_SMALLTEXTOUT are reported and not drawn." },
  { standard := .msEmf, ref := .clause [2, 3, 6], title := "Escape Record Types", status := .notImplemented,
    note := "Printer escapes carry no drawing and are skipped." },
  { standard := .msEmf, ref := .clause [2, 3, 7], title := "Object Creation Record Types", status := .incomplete,
    note := "EMR_CREATEBRUSHINDIRECT, EMR_CREATEPEN, EMR_EXTCREATEPEN and EMR_EXTCREATEFONTINDIRECTW are read; EMR_CREATEDIBPATTERNBRUSHPT and EMR_CREATEMONOBRUSH fill with their bitmap's average color and hatched brushes with a tint of their color. Palettes and color spaces are skipped." },
  { standard := .msEmf, ref := .clause [2, 3, 8], title := "Object Manipulation Record Types", status := .incomplete,
    note := "EMR_SELECTOBJECT (table and stock objects) and EMR_DELETEOBJECT are played; the palette and color space records are skipped." },
  { standard := .msEmf, ref := .clause [2, 3, 9], title := "OpenGL Record Types", status := .notImplemented,
    note := "OpenGL records are skipped." },
  { standard := .msEmf, ref := .clause [2, 3, 10], title := "Path Bracket Record Types", status := .incomplete,
    note := "EMR_BEGINPATH, EMR_ENDPATH, EMR_CLOSEFIGURE and EMR_ABORTPATH bracket the figures that the path records fill, stroke or clip to; EMR_FLATTENPATH and EMR_WIDENPATH are reported and ignored, and text inside a bracket is not drawn." },
  { standard := .msEmf, ref := .clause [2, 3, 11], title := "State Record Types", status := .incomplete,
    note := "Window and viewport origins, extents and their scaling, the map mode, arc direction, background mode and color, text color and alignment, fill mode, mix mode, EMR_MOVETOEX, EMR_SAVEDC and EMR_RESTOREDC (relative and absolute) are played. Brush origin, color adjustment, ICM, layout, linked fonts, mapper flags, miter limit, stretch mode and text justification are skipped." },
  { standard := .msEmf, ref := .clause [2, 3, 12], title := "Transform Record Types", status := .implemented,
    note := "EMR_SETWORLDTRANSFORM and EMR_MODIFYWORLDTRANSFORM in all four modes (dc.rs world; test dc.rs world_transform_modes_compose_in_order)." }
]

/-- Every row of the [MS-EMF] ledger. -/
def rows : List Feature :=
  rows0

end Ledger.Catalogue.MsEmf

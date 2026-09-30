# Limitations

The reader reports what it could not read: `docboss diagnostics` lists every
approximated or dropped item. The [conformance ledger](conformance.md) has a
row for every clause of the standards docboss reads, with a note on what is
missing. The largest gaps:

## Reading

- DOCX: `w:altChunk` content is skipped and reported. DrawingML charts,
  SmartArt, math (OMML), ActiveX controls and custom XML data binding are not
  read as content. Ruby text is ignored.
- DOC: password-protected Word 6 and 95 files are refused. Word 6 and 95
  files are read with their formatting, sections, headers, footers, tables
  and pictures, but their numbered paragraphs (ANLD), comments and drawing
  objects are left out; Word 2 files are read as text only. Shapes that
  are neither pictures, text boxes nor preset shape types (freeforms, WordArt,
  groups) are reported as dropped.
- RTF and Flat OPC XML are detected and refused.

## Layout and rendering

- Shaping covers Arabic, Hebrew and combining marks; Indic, Thai and other
  scripts that reorder or stack glyphs are not shaped, cursive attachment
  (GPOS type 3) is not applied, and Latin text gets no discretionary
  ligatures. Right-to-left sections (`w:bidi` in `w:sectPr`: column order,
  gutter side) lay out left to right.
- No column balancing; text does not wrap around floating images.
- Groups, canvases, SmartArt, charts and equations are not drawn. Shapes
  draw their preset or custom geometry with solid fills only; gradient,
  picture and pattern fills and effects are not drawn, and text inside a
  turned shape stays upright.
- Conditional formatting from table styles is not applied.
- CFF2 fonts are refused; WMF, EMF and TIFF pictures draw a placeholder.
- Tracked changes render as the final text, without revision marks.

## Writing

- Embedded fonts are not written (the font table lists names only), and no
  theme part is written.
- Raw HTML in Markdown is dropped, except `<br>`.
- Encrypted output is not supported.

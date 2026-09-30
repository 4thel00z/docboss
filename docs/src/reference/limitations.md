# Limitations

The reader reports what it could not read: `docboss diagnostics` lists every
approximated or dropped item. The [conformance ledger](conformance.md) has a
row for every clause of the standards docboss reads, with a note on what is
missing. The largest gaps:

## Reading

- DOCX: `w:altChunk` content is skipped and reported. ActiveX controls and
  custom XML data binding are not read as content. Ruby text is ignored.
  Only the compatibilityMode compatibility setting is read.
- DOC: password-protected Word 6 and 95 files are refused. Word 6 and 95
  files are read with their formatting, sections, headers, footers, tables
  and pictures, but their numbered paragraphs (ANLD), comments and drawing
  objects are left out; Word 2 files are read as text only. Equation Editor
  objects keep their preview picture; their equations (MTEF) are not read
  as text. A compound file whose directory is lost gives its text only.
  Freeforms and WordArt shapes are reported as dropped.
- RTF and Flat OPC XML are detected and refused.

## Layout and rendering

- Shaping covers Arabic, Hebrew and combining marks; Indic, Thai and other
  scripts that reorder or stack glyphs are not shaped, cursive attachment
  (GPOS type 3) is not applied, and Latin text gets no discretionary
  ligatures. Right-to-left sections (`w:bidi` in `w:sectPr`: column order,
  gutter side) and vertical sections (`tbRl`) lay out left to right.
- No column balancing; text does not wrap around floating images and
  frames, whatever their wrapping (square, tight, top and bottom).
- Shapes draw solid and gradient fills; picture and pattern fills and
  effects (shadows, glow, 3-D) are not drawn. SmartArt without its saved
  drawing shows an empty frame; 3-D, radar, stock, surface, bubble and
  bar-of-pie charts show an empty frame too. Office Math stretches
  delimiters by scaling the glyph rather than with the font's variants.
- Conditional formatting from table styles is not applied.
- CFF2 fonts are refused; TIFF pictures draw a placeholder. WMF and EMF
  pictures are played (EMF+ records are skipped in favour of the EMF ones);
  their text is not extracted.
- Tracked changes render as the final text, without revision marks.

## Writing

- Embedded fonts are not written (the font table lists names only), and no
  theme part is written.
- Raw HTML in Markdown is dropped, except `<br>`.
- Encrypted output is not supported.

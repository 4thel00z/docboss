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
  files are read with their formatting, sections, headers, footers, tables,
  pictures and numbered paragraphs (ANLD), but their comments, endnotes and
  drawing objects, whose Word 6 records differ from Word 97's, and the
  equations of their Equation Editor objects are left out and reported;
  Word 2 files are read as text only. A Word 97 compound file whose
  directory is lost gives its text only (a Word 6 or 95 one is read in
  full). Freeforms and WordArt shapes are reported as dropped.
- DOCX and DOC: Equation Editor 3 objects keep their preview picture and
  give their equation as text and LaTeX; MathType's MTEF 4 and 5 are not
  read, and the object is reported.
- RTF is detected and refused. Flat OPC XML is not read: it is refused as
  an unknown format.

## Layout and rendering

- Shaping covers Arabic, Hebrew and combining marks; Indic, Thai and other
  scripts that reorder or stack glyphs are not shaped, cursive attachment
  (GPOS type 3) is not applied, and Latin text gets no discretionary
  ligatures. Right-to-left sections (`w:bidi` in `w:sectPr`: column order,
  gutter side) and vertical sections (`tbRl`) lay out left to right.
- No column balancing. Text wraps around floating images, text frames and
  floating tables in the body. Text in a table cell or text frame does not
  wrap around a float anchored there (the row grows to hold the float, which
  is drawn over the text); floats in headers and footers do not push body
  text aside; a table beside a float is not narrowed; text wraps around a
  turned drawing as if it were not turned. `docboss diagnostics --layout`
  reports each of these.
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

# Limitations

The reader reports what it could not read: `docboss diagnostics` lists every
approximated or dropped item. The [conformance ledger](conformance.md) has a
row for every clause of the standards docboss reads, with a note on what is
missing. The largest gaps:

## Reading

- DOCX: `w:altChunk` content is skipped and reported. DrawingML charts,
  SmartArt, math (OMML), ActiveX controls and custom XML data binding are not
  read as content. Ruby text is ignored.
- DOC: XOR-obfuscated files are refused. Word 2, 6 and 95 files are read as
  text only, with the code page taken from the document language. Shapes that
  are neither pictures nor text boxes (lines, WordArt) are reported as dropped.
- RTF and Flat OPC XML are detected and refused.

## Layout and rendering

- No right-to-left or bidirectional text and no complex-script shaping.
- No column balancing; text does not wrap around floating images.
- Text boxes, shapes, SmartArt, charts and equations are not drawn.
- Conditional formatting from table styles is not applied.
- CFF2 fonts are refused; WMF, EMF and TIFF pictures draw a placeholder.
- Tracked changes render as the final text, without revision marks.

## Writing

- Embedded fonts are not written (the font table lists names only), and no
  theme part is written.
- Raw HTML in Markdown is dropped, except `<br>`.
- Encrypted output is not supported.

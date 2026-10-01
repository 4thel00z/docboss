# Fixtures

Real DOCX files written by other producers, each next to its source:

- `libreoffice-rich.docx`: `soffice --headless --convert-to docx:"MS Word 2007 XML" libreoffice-rich.fodt`
- `libreoffice-rtf-notes.docx`: the same conversion of `libreoffice-rtf-notes.rtf`
- `textutil-lists.docx`: `textutil -convert docx -output textutil-lists.docx textutil-lists.html` (macOS)

`dml-picture-in-textframe.docx` is a Word file from LibreOffice's
`sw/qa/extras/ooxmlexport/data` (MPL-2.0): a text box centered on its column
with `wp:align`.

`red.png` is the image `textutil-lists.html` refers to.

`equation-editor.docx` is LibreOffice's `mathtype.docx` from the same
directory (MPL-2.0): an Equation Editor 3 object, `a = b/c`, with its WMF
preview.

`wrap.docx` is written by `python3 crates/docboss-docx/tools/wrap_fixture.py`:
pictures wrapped square and tight, a text frame, a floating table and a VML
shape wrapped top and bottom.

`vml-tight-wrap.docx` is LibreOffice's `tdf135660.docx` from the same
directory (MPL-2.0): a VML picture wrapped tight with no wrap polygon.

`framed-table.docx` is LibreOffice's `tdf164474.docx` from the same
directory (MPL-2.0): a table whose cell paragraphs carry a text frame.

`anchor-moves-on.docx`, `anchor-moves-on-tall.docx`, `float-in-frame.docx`,
`float-in-floating-table.docx` and `float-in-cell.docx` are written by
`python3 crates/docboss-docx/tools/float_fixtures.py crates/docboss-docx/tests/fixtures`.

# Writing DOCX

docboss writes WordprocessingML packages from its document model. Output is
deterministic: the same input produces identical bytes, ZIP entries carry a
fixed timestamp, and dates appear only when the metadata states them.

## Converting

```bash
docboss convert legacy.doc -o legacy.docx      # DOC to DOCX
docboss convert report.docx -o fresh.docx      # a fresh package from the model
docboss convert legacy.doc -o legacy.md        # or .html, .txt, .json by extension
```

```python
import docboss

docboss.Document("legacy.doc").save_docx("legacy.docx")
data = docboss.Document("report.docx").to_docx()
```

A converted file carries what the model holds: text, styles, lists, tables,
sections with headers and footers, notes, comments, fields, bookmarks,
revisions and images. Embedded fonts and the theme part are not written.

## Markdown to DOCX

```bash
docboss create md notes.md -o notes.docx --size a4 --title "Notes"
```

```python
import docboss

docx = docboss.md.to_docx("# Notes\n\n- one\n- two\n\n| a | b |\n|---|---|\n| 1 | 2 |\n")
```

CommonMark and GFM map onto Word's built-in vocabulary: headings to
`Heading1` through `Heading6`, emphasis, strong and strikethrough to run
properties, inline code and code blocks to monospace styles, links to
hyperlinks, ordered, bullet, nested and task lists to numbering definitions,
tables to tables with a header row, block quotes to the `Quote` style,
thematic breaks to a bottom border, footnotes to footnotes. Relative image
paths are read from the Markdown file's directory on the CLI, and from
`images_dir=` in Python; without it an image becomes its alt text.

## Composing from Rust

```rust
use docboss_write::{DocumentBuilder, ListKind, Para, TableBuilder};

let mut doc = DocumentBuilder::new();
doc.title("Q3 Report").heading(1, "Summary");
doc.paragraph(Para::new().text("Revenue grew ").bold("12%").text("."));
doc.items(ListKind::Bullet, &["North", "South"]);
let width = doc.text_width();
doc.block(TableBuilder::new(2, width).header(&["Region", "Growth"]).row(&["North", "14%"]));
let bytes = doc.to_bytes()?;
assert!(bytes.starts_with(b"PK"));
# Ok::<(), docboss_write::Error>(())
```

`DocumentBuilder` covers metadata, page size and margins, styles, headings,
paragraphs, lists, tables, images, footnotes, headers, footers and sections;
`docboss_write::to_bytes` and `save` serialize any `docboss_model::Document`,
and `docboss_write::markdown::to_docx` is the Markdown path.

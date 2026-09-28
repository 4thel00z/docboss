# Quickstart

One taste of each surface. `report.docx` and `legacy.doc` stand in for any
Word documents of yours; [Installation](./installation.md) covers getting the
binary, the wheel and the crates.

## CLI

```bash
docboss info   report.docx                   # format, metadata, page setup, page count, counts
docboss text   report.docx                   # plain text with list labels
docboss text   legacy.doc                    # Word 97-2003 files read the same way
docboss md     report.docx                   # GitHub-flavored Markdown
docboss render report.docx --page 1 -o page.png --scale 2
docboss images report.docx -o images         # the document's pictures, as stored
docboss convert legacy.doc -o legacy.docx    # DOC to a fresh DOCX
docboss create md notes.md -o notes.docx     # Markdown composed into a DOCX
```

`render` prints each file it wrote; `images` writes into the directory it is
given and creates it when missing. Page numbers are 1-based on the command
line.

## Python

```python
from pathlib import Path

import docboss

doc = docboss.Document("report.docx")
print(doc.format, doc.page_count(), "pages")

text = doc.extract_text()           # one line per paragraph, list labels included
markdown = doc.extract_markdown()   # headings, lists, tables, footnotes, links

Path("page.png").write_bytes(doc.render(0, scale=2.0))

for image in doc.images():
    print(image.name, image.content_type, len(image.data))

legacy = docboss.Document("legacy.doc")
Path("legacy.docx").write_bytes(legacy.to_docx())

docx = docboss.md.to_docx(Path("notes.md").read_text())
```

`Document` also opens from memory (`Document(data=raw_bytes)`), pages index
0-based with negative indexes from the end, and `render` returns PNG bytes
unless `format=` says otherwise.

## Rust

With `docboss-core`, `docboss-output`, `docboss-layout` and `docboss-render`
added:

```rust,no_run
use docboss_output::{to_markdown, MarkdownOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let doc = docboss_core::open("report.docx")?;
    println!("{}", to_markdown(&doc, &MarkdownOptions::default()));

    let layout = docboss_layout::layout_document(&doc);
    docboss_render::render_page(&layout, 0, 2.0)?.save("page.png")?;
    Ok(())
}
```

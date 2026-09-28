# Rendering pages

```bash
docboss render report.docx --page 1 -o page.png            # one page, 72 dpi
docboss render report.docx --scale 2 -o 'page-%d.png'      # every page at 144 dpi
docboss render report.docx --page 1 -o page.jpg --jpeg-quality 85
docboss render legacy.doc -o 'legacy-%d.ppm'
```

The `-o` extension picks the format: `.png`, `.ppm`, `.bmp` or `.jpg`/`.jpeg`.
`%d` in the path is replaced by the page number; without `--page` every page
is rendered, laid out once and painted on all cores. `--scale` is pixels per
point, so 1.0 is 72 dpi and 2.0 is 144 dpi.

## What layout does

docboss lays a document out the way Word does rather than the way a browser
would:

- Paragraph and run properties are resolved through document defaults, the
  style chain, table styles, list levels and direct formatting.
- Lines break at Unicode line-break opportunities with soft hyphens honored,
  and are aligned or justified; line spacing follows the paragraph's rule
  (auto, exact, at least) with Word's extra leading on single spacing.
- Tab stops, leaders and default tabs; list labels with their hanging indents.
- Keep with next, keep lines together, widow and orphan control, page breaks
  before.
- Tables: grid widths, merged cells in both directions, cell margins, borders
  and shading, rows split across pages, header rows repeated on each page.
- Sections with their own page size, margins and columns; headers and footers
  for first, even and default pages, with `PAGE` and `NUMPAGES` fields in the
  section's number format.
- Footnotes at the bottom of the page they are referenced on; inline and
  floating images.

Fonts come from the machine and from the document; see
[Installation](../installation.md#fonts-for-rendering) for where docboss looks
and which substitutes it uses. `docboss fonts report.docx` prints each font the
document names and the face it resolved to.

## Python

```python
from pathlib import Path

import docboss

doc = docboss.Document("report.docx")
print(doc.page_count(), "pages")
Path("first.png").write_bytes(doc.render(0, scale=2.0))
Path("last.jpg").write_bytes(doc.render(-1, format="jpeg", jpeg_quality=85))
for index, png in enumerate(doc.render_pages(scale=1.5)):
    Path(f"page-{index + 1}.png").write_bytes(png)
```

`render_pages` renders every page (or the indexes given in `pages=`) in
parallel and returns the encoded images in page order. Layout is computed once
per document and cached, so rendering pages one by one costs no second layout.

## Rust

```rust,no_run
use docboss_render::Format;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let doc = docboss_core::open("report.docx")?;
    let layout = docboss_layout::layout_document(&doc);
    for (index, page) in docboss_render::render_pages(&layout, 2.0).into_iter().enumerate() {
        std::fs::write(format!("page-{}.png", index + 1), page?.encode(Format::Png)?)?;
    }
    Ok(())
}
```

`docboss_layout::layout` takes an explicit `FontDatabase` when several
documents should share one; `layout_document` builds one for the document,
including its embedded fonts. Each laid-out page holds positioned glyph runs,
rectangles, lines and images in points, so a program can inspect geometry
without rasterizing.

## What is not drawn

Text boxes, shapes, SmartArt, charts and equations are not drawn, and text
does not wrap around floating images. WMF, EMF and TIFF pictures draw a
placeholder rectangle; `docboss diagnostics report.docx --layout` reports them
and everything else layout approximated.

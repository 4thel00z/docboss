# Pages and formats

`to()` converts a document, a selection of its pages or one page to a format.
The format is one dict with a `format` key, or the same keys as keyword
arguments:

```python
import docboss
from docboss import Markdown, Png

doc = docboss.Document("report.docx")

md = doc.to(format="markdown", images="omit")                # the whole document, str
md = doc.to({"format": "markdown", "images": "omit"})        # the same, as a dict

pages = doc.pages().to(Markdown(format="markdown", images="omit"))   # list[str], one per page
html = doc.pages()[2:6].to(format="html")                    # pages 3 to 6
text = doc.page(3).to(format="text")                         # str
png = doc.page(3).to(Png(format="png", scale=2))             # bytes
jpegs = doc.pages().to(format="jpeg", scale=1.5, quality=85) # list[bytes]
```

`docboss.Text`, `Markdown`, `Html`, `Json`, `Png`, `Ppm`, `Bmp` and `Jpeg` are
`TypedDict`s, so a type checker knows the keys each format takes and whether
`to()` returns `str` or `bytes`.

| `format` | Keys (defaults) | Output |
|---|---|---|
| `"text"` | `list_labels` (`True`), `headers_footers` (`False`), `notes` (`True`), `comments` (`False`) | `str` |
| `"markdown"` | `images` (`"reference"`), `image_prefix` (`""`), `title` (`False`), `page_breaks` (`False`), `headers_footers`, `notes`, `comments` | `str` |
| `"html"` | `standalone` (`False`), `images` (`"embed"`), `image_prefix` (`""`), `headers_footers`, `notes`, `comments` | `str` |
| `"json"` | `pretty` (`False`) | `str` |
| `"png"`, `"ppm"`, `"bmp"` | `scale` (`1.0`, pixels per point) | `bytes`, pages only |
| `"jpeg"` (or `"jpg"`) | `scale` (`1.0`), `quality` (`90`, 1 to 100) | `bytes`, pages only |

The defaults are those of `extract_text`, `extract_markdown`,
`extract_html`, `to_json` and `render`, which stay as shortcuts for the same
conversions. Passing both a dict and keyword arguments, leaving out `format`,
naming an unknown format or giving a key the format does not take raises
`DocbossError`. `page_breaks` applies to the whole document only.

## Selecting pages

`pages()` lays the document out once, caches the layout and returns every
page. A `Pages` selection behaves like a sequence: `len()`, iteration, an
integer index (negative counts from the end) giving a `Page`, and a slice
giving another `Pages`. `doc.page(i)` is `doc.pages()[i]`.

A `Page` has `index` (its position in the document, from 0), `number` (the
number `PAGE` fields show on it) and `to()`.

## Which text a page holds

A page's text, Markdown, HTML or JSON holds the top-level body blocks
(paragraphs and tables) that start on it. A paragraph or table that runs on to
the next pages stays whole on the page where it starts, and a page where no
block starts converts to `""`. Footnotes and endnotes go on the page of their
first reference, comments likewise. With `headers_footers=True` each page
carries the header and footer painted on it.

So the pages of a document together hold its body exactly once: no text is
lost or repeated between pages. List numbering and note numbers carry on
from page to page as in the whole document.

Converting several pages to a text format walks the body once for all of
them; image formats render the selected pages across the machine's cores.

## Async

`AsyncDocument` has the same calls as coroutines. `pages()`, slicing and
`page()` are lazy; the document is fetched and laid out when `to()` first
runs, on the runtime's blocking pool, so the event loop stays free:

```python
doc = await docboss.AsyncDocument.from_bytes(raw)
pages = await doc.pages().to(format="markdown", images="omit")
first = await doc.pages()[:3].to(format="text")
count = await doc.pages().count()
```

## Rust

The `docboss-pages` crate holds the same API:

```rust,no_run
use docboss_output::{ImageMode, MarkdownOptions};
use docboss_pages::{Document, Format};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let doc = Document::new(docboss_core::open("report.docx")?);
    let markdown = Format::Markdown(MarkdownOptions {
        images: ImageMode::Omit,
        ..MarkdownOptions::default()
    });
    let pages = doc.pages().to(&markdown)?;
    let third = doc.page(2).map(|page| page.to(&Format::Png { scale: 2.0 })).transpose()?;
    println!("{} pages, page 3 is {} bytes", pages.len(), third.and_then(|p| p.as_bytes().map(<[u8]>::len)).unwrap_or(0));
    Ok(())
}
```

`Format` also deserializes with serde from the tagged map form, such as
`{"format": "markdown", "images": "omit"}`, refusing keys the format does not
take.

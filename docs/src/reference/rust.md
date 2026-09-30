# Rust crate reference

The crates split docboss by concern. Most programs need `docboss-core` to
open a document and one of the consumers after it.

| Crate | Entry points |
|---|---|
| `docboss-core` | `open(path)`, `read(bytes)`, `open_with`/`read_with(&Options { password })`, `detect(bytes) -> Format`; re-exports the whole model |
| `docboss-model` | `Document` and its parts; `Styles::resolve_paragraph` and `resolve_run` for effective formatting; `Numbering::counter()` for list labels; `plain_text` |
| `docboss-output` | `to_text`/`write_text` with `TextOptions`, `to_markdown`/`write_markdown` with `MarkdownOptions`, `to_html`/`write_html` with `HtmlOptions`, `blocks_view`, and with the `serde` feature `to_json` and `blocks_json` |
| `docboss-layout` | `layout_document(&Document) -> Layout`, `layout(&Document, &Arc<FontDatabase>)`, `layout_with_options`, `fonts_for`; `Layout { pages, fonts, media, diagnostics }` with positioned items in points |
| `docboss-render` | `render_page(&Layout, index, scale) -> Pixmap`, `render_pages` (all cores), `Renderer`; `Pixmap::encode(Format)` and `save`; `Format::{Png, Ppm, Bmp, Jpeg { quality }}` |
| `docboss-write` | `to_bytes(&Document)`, `save`, `DocumentBuilder`, `Para`, `TableBuilder`, `markdown::to_docx` |
| `docboss-aio` | `AsyncDocument::{open, from_bytes, open_url, open_url_with, from_backend}`, `read(&ReadOptions)`, `load_media`, `part`, `part_names`, `bytes_fetched`, `requests`; the `http` feature adds `HttpBackend` |
| `docboss-docx` | `read(bytes)`, `read_with(bytes, Options)`: the WordprocessingML reader |
| `docboss-doc` | `read(bytes)`, `read_with_password`, `is_doc`: the Word binary reader |
| `docboss-zip` | `Archive` (reader), `write` (deterministic writer), `crc32` |
| `docboss-xml` | the pull tokenizer used by the DOCX reader |
| `docboss-cfb` | `CompoundFile` and property sets |
| `docboss-crypt` | `decrypt(bytes, password)` for encrypted DOCX |
| `docboss-font` | `Font`, `FontDatabase` with system discovery and substitution |
| `docboss-metafile` | `play(bytes) -> Picture` for WMF and EMF, `text(&Picture)`, `bitmap::{decode, decode_bmp, encode_bmp}` |

Errors are `thiserror` enums per crate; `docboss_core::Error` wraps the
readers' errors and adds `Encrypted` and `Unsupported`. No library function
panics on input bytes: damaged input yields an error or a document with
diagnostics.

```rust,no_run
use docboss_output::{to_html, HtmlOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = docboss_core::Options { password: Some("secret".into()) };
    let doc = docboss_core::open_with("locked.docx", &options)?;
    for diagnostic in &doc.diagnostics {
        eprintln!("{:?} {}: {}", diagnostic.severity, diagnostic.location, diagnostic.message);
    }
    let html = to_html(&doc, &HtmlOptions { standalone: true, ..HtmlOptions::default() });
    std::fs::write("locked.html", html)?;
    Ok(())
}
```

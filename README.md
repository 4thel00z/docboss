<h1 align="center">docboss</h1>

<p align="center">
  <strong>A DOCX and DOC engine written from scratch in Rust: parse, extract text, Markdown and HTML, lay out and render pages to PNG, PPM, BMP or JPEG, write DOCX. One core, a CLI, a terminal explorer and pythonic bindings.</strong>
</p>

<p align="center">
  <a href="https://github.com/4thel00z/docboss/actions/workflows/ci.yaml"><img src="https://github.com/4thel00z/docboss/actions/workflows/ci.yaml/badge.svg" alt="CI"></a>
  <a href="https://github.com/4thel00z/docboss/actions/workflows/python-ci.yml"><img src="https://github.com/4thel00z/docboss/actions/workflows/python-ci.yml/badge.svg" alt="python-ci"></a>
  <img src="https://img.shields.io/badge/rust-2021-000000?logo=rust&logoColor=white" alt="Rust 2021">
  <a href="#license"><img src="https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg" alt="MIT OR Apache-2.0"></a>
</p>

<p align="center">
  <a href="docs/src/introduction.md">Docs</a> ·
  <a href="#benchmarks">Benchmarks</a> ·
  <a href="docs/src/reference/conformance.md">Conformance ledger</a> ·
  <a href="failure-modes/README.md">Failure modes</a>
</p>

---

Reading a Word document should not require LibreOffice, a JVM or a C library. docboss is a clean-room reader built from the specifications (ECMA-376 for DOCX, Microsoft's [MS-DOC], [MS-CFB], [MS-OLEPS], [MS-ODRAW] and [MS-OFFCRYPTO] for the binary formats, PKWARE's APPNOTE for ZIP): safe Rust, no C dependencies, its own ZIP, XML, compound-file, font and raster code, one core behind the CLI, the terminal explorer and the Python extension. It is a **lenient reader**: a ZIP with a broken central directory is read from its local headers, a compound file with a bad FAT chain is followed as far as it goes, and every approximated or dropped item is reported instead of lost.

## Highlights

- **Both formats, one model**: DOCX, DOCM, DOTX and DOTM through ECMA-376 WordprocessingML (transitional and strict namespaces), DOC and DOT through [MS-DOC], including Word 97-2003 formatting, lists, tables, sections, notes, comments, fields, bookmarks and pictures. The format is detected from the bytes, never from the file name.
- **Fast**: DOCX text at 2,676 files a second over 80 real-world files, about 4× docx2txt and 24× mammoth; DOC text at 2,680 files a second, about 14× antiword and catdoc; 16,034 DOCX files a second through `docboss.extract_texts` on 12 cores ([benchmarks](#benchmarks)).
- **Complete text**: word recall of 0.992 against LibreOffice's text export on DOCX, the highest of the six engines measured, and 0.976 on DOC: tables, headers, footers, footnotes, comments and text boxes included.
- **Survives damaged files**: text from 140 of 150 damaged DOCX files, where the other engines return text for 1 to 8; no crash and no hang on 300 damaged DOCX and DOC files.
- **Markdown and HTML**: headings from styles, nested lists with the document's own labels, GFM tables with merged cells, footnotes, links and images, in one pass.
- **Layout and rendering**: Word-like line breaking, tab stops, list labels, keep and widow rules, tables split across pages with repeated header rows, sections and columns, headers and footers with page numbers, footnotes, inline and floating images; an anti-aliased rasterizer over its own TrueType and CFF parsers, with metric-compatible font substitution (Carlito for Calibri, Liberation for Arial and Times New Roman). PNG, PPM, BMP and JPEG output, pages rendered on all cores.
- **DOCX writing**: model to DOCX, a builder API, Markdown to DOCX, and DOC to DOCX conversion, with deterministic output: the same input gives identical bytes.
- **Encrypted files**: password-protected DOCX (Agile and Standard encryption, [MS-OFFCRYPTO]) and DOC (RC4 and RC4 CryptoAPI).
- **Range-fetching I/O**: documents open over files or `http(s)://` URLs and fetch only the byte ranges they need: the ZIP central directory and the XML parts of a DOCX, the sectors of the Word streams of a DOC.
- **Conformance ledger checked by Lean**: every clause of the specifications docboss reads has a row with a status, and a Lean 4 gate fails the build when a row claims more than the code and tests cite ([conformance](docs/src/reference/conformance.md)).
- **Terminal explorer** (`docboss tui`): element tree, resolved-style inspector, ZIP or compound-file container view with hex and XML, Markdown and page previews.

## Install

```bash
pip install docboss           # abi3 wheels (CPython 3.12+)
cargo install docboss-cli     # the `docboss` binary
```

Coding agents get a bundled [skill](skills/docboss/SKILL.md): `docboss skill install` writes it into `./.claude/skills/docboss` (`--global` for `~/.claude/skills/docboss`).

## Usage

```bash
docboss info    report.docx                 # format, metadata, page setup, page count, content counts
docboss text    report.docx                 # plain text with list labels; --headers-footers, --comments, --no-notes
docboss text    legacy.doc                  # the same for Word 97-2003 files
docboss md      report.docx                 # GitHub-flavored Markdown; --images DIR exports and links images
docboss html    report.docx --standalone    # semantic HTML, images embedded as data: URIs
docboss json    report.docx --blocks        # the compact per-block view; omit --blocks for the whole model
docboss q       report.docx '.metadata.title' -r   # jq over the JSON model
docboss render  report.docx --page 1 -o page.png --scale 2   # or .ppm / .bmp / .jpg; all pages with -o 'page-%d.png'
docboss images  report.docx -o images       # the document's images at their stored size and format
docboss convert legacy.doc -o legacy.docx   # DOC or DOCX to a fresh DOCX, or .md/.html/.txt/.json by extension
docboss create md notes.md -o notes.docx    # CommonMark + GFM composed into a DOCX
docboss fonts   report.docx                 # each font the document names and the face it resolves to here
docboss diagnostics report.docx             # everything the lenient reader approximated or dropped
docboss parts   report.docx                 # ZIP entries or compound-file streams, with sizes
docboss hex     legacy.doc WordDocument --length 64    # hexdump a part
docboss xml     report.docx word/styles.xml # a part, indented
docboss text    locked.docx --password secret          # encrypted DOCX or DOC
docboss tui     report.docx                 # interactive terminal explorer
```

Every read command accepts a local path or an `http(s)://` URL; a URL is read with range requests, not downloaded whole.

```python
import docboss

doc = docboss.Document("report.docx")          # or Document(data=raw_bytes), password="..."
print(doc.format, doc.metadata.title)          # "docx" or "doc"
text = doc.extract_text(headers_footers=True)
md   = doc.extract_markdown()                  # images="reference" | "embed" | "omit"
html = doc.extract_html(standalone=True)
for block in doc.blocks():                     # kind, style, heading_level, list_label, text
    print(block.kind, block.text)
png  = doc.render(0, scale=2.0)                # PNG bytes; format="ppm" | "bmp" | "jpeg"
pngs = doc.render_pages()                      # every page, rendered on all cores
imgs = doc.images()                            # .name, .content_type, .data
data = doc.to_docx()                           # a fresh DOCX, also for .doc input

texts = docboss.extract_texts(["report.docx", "legacy.doc"], threads=8)   # batch, on Rust threads
docx  = docboss.md.to_docx("# Notes\n\n- one\n- two\n")                  # Markdown -> DOCX bytes
```

Heavy calls release the GIL, so documents extract and render in parallel from Python threads. `extract_texts` is the throughput path for pipelines: one call, a Rust thread pool, a list of strings back.

`docboss.AsyncDocument` reads files, bytes or `http(s)://` URLs asynchronously, fetching only the byte ranges each call needs:

```python
import asyncio

import docboss

async def main() -> None:
    doc = await docboss.AsyncDocument.open_url("https://example.com/report.docx")
    text = await doc.extract_text()
    print(doc.format, doc.bytes_fetched, "bytes fetched in", len(doc.requests), "requests")
    full = await doc.document()                # the synchronous Document, for rendering and the rest

asyncio.run(main())
```

<details>
<summary><strong>Rust</strong></summary>

The library crates: `docboss-core` (open and detect), `docboss-model` (the document model), `docboss-output` (text, Markdown, HTML, JSON), `docboss-layout` and `docboss-render` (pages), `docboss-write` (DOCX), `docboss-aio` (async and remote reads), plus the readers underneath (`docboss-docx`, `docboss-doc`, `docboss-zip`, `docboss-xml`, `docboss-cfb`, `docboss-crypt`, `docboss-font`).

```rust,no_run
use docboss_output::{to_markdown, to_text, MarkdownOptions, TextOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let doc = docboss_core::open("report.docx")?;          // DOCX or DOC, detected from the bytes
    println!("{}", to_text(&doc, &TextOptions::default()));
    println!("{}", to_markdown(&doc, &MarkdownOptions::default()));

    let layout = docboss_layout::layout_document(&doc);
    println!("{} pages", layout.pages.len());
    docboss_render::render_page(&layout, 0, 2.0)?.save("page.png")?;

    docboss_write::save(&doc, "copy.docx")?;               // deterministic DOCX
    Ok(())
}
```

Composing a document:

```rust
use docboss_write::{DocumentBuilder, ListKind, Para, TableBuilder};

let mut doc = DocumentBuilder::new();
doc.title("Q3 Report").heading(1, "Summary");
doc.paragraph(Para::new().text("Revenue grew ").bold("12%").text("."));
doc.items(ListKind::Bullet, &["North", "South"]);
let width = doc.text_width();
doc.block(TableBuilder::new(2, width).header(&["Region", "Growth"]).row(&["North", "14%"]));
let bytes = doc.to_bytes()?;
# Ok::<(), docboss_write::Error>(())
```

Reading a remote DOCX with range requests (`docboss-aio` with the `http` feature):

```rust,no_run
use docboss_aio::{AsyncDocument, ReadOptions};

# async fn run() -> docboss_aio::Result<()> {
let remote = AsyncDocument::open_url("https://example.com/report.docx").await?;
let doc = remote.read(&ReadOptions::default()).await?;
println!("{} of {} bytes fetched", remote.bytes_fetched(), remote.len());
# Ok(()) }
```

</details>

## Benchmarks

Over 80 real-world DOCX files from the LibreOffice, Apache POI and python-docx test suites (word recall against LibreOffice's text export of each file):

| Engine | Text files/s | Word recall | Markdown files/s | HTML files/s |
|---|--:|--:|--:|--:|
| **docboss** | **2,676** | **0.992** | **3,166** | **3,531** |
| docx2txt | 663 | 0.974 | | |
| python-docx | 560 | 0.848 | | |
| docx2python | 112 | 0.974 | | |
| mammoth | 104 | 0.956 | 106 | 112 |
| pandoc | 6.6 | 0.894 | 7.5 | 7.7 |

Over 80 DOC files: docboss 2,680 files/s (recall 0.976), catdoc 186 (0.888), antiword 179 (0.948). LibreOffice converts either sample at 14 to 16 files/s in one batch, or about 0.7 to 0.8 s per file one process at a time.

<details>
<summary><strong>Parallel, large documents, memory, damaged files, and where the others win</strong></summary>

- **Parallel**: `docboss.extract_texts` reads 300 DOCX files at 16,034 files/s on 12 cores (5.5× its sequential speed); the fastest other route is docx2txt in a process pool at 1,204. On DOC, docboss reaches 5,148 files/s against catdoc's 437.
- **One large document** (3,000 sections, 1,166 pages): docboss extracts the DOCX in 80 ms against docx2txt's 754 ms and python-docx's 1,524 ms. **catdoc wins the DOC version, 53 ms against docboss's 155 ms**: it streams the text out of the piece table, where docboss builds its whole model first.
- **Memory**: on DOCX docboss peaks at 31.7 MB over 100 files, docx2txt slightly lower at 30.1 MB, the rest 55 to 82 MB. **On DOC antiword and catdoc win**, at about 24 MB with 2 to 3 MB of their own, against docboss's 42 MB (62 MB on a 6.2 MB file).
- **Damaged files** (150 per format): docboss returns text for 140 DOCX files, the other engines for 1 to 8. On DOC **catdoc returns text for 100 files against docboss's 94**, but hung once; antiword crashed 3 times. docboss neither crashed nor hung.

Measured on an Apple M3 Pro, one session, best of 3 after a warm-up. Method, versions and every table: [`benchmarks/README.md`](benchmarks/README.md).

</details>

### Rendering

Pages rendered by docboss against LibreOffice's rendering of the same documents (LibreOffice to PDF, rasterized by pdfboss), 60 DOCX and 30 DOC files from the same corpora, up to 5 pages each, windowed SSIM at scale 1.5:

| Format | SSIM median | SSIM p10 | Page counts equal |
|---|--:|--:|--:|
| DOCX | 0.975 | 0.857 | 56 of 59 |
| DOC | 0.950 | 0.634 | 25 of 29 |

The low tail is what layout does not draw yet (text boxes, shapes, SmartArt), tracked changes that LibreOffice shows with revision marks, and table row heights that differ from LibreOffice's. Method: [`benchmarks/README.md`](benchmarks/README.md#rendering-fidelity-bench_fidelitypy).

## What's inside

<details>
<summary><strong>Crate map</strong></summary>

| Crate | Responsibility |
|---|---|
| `docboss-model` | The document model both readers produce: sections, paragraphs, runs, tables, styles with property resolution, numbering with list labels, notes, comments, media, diagnostics |
| `docboss-zip` | ZIP reader and deterministic writer: central directory, ZIP64, data descriptors, slicing-by-8 CRC-32, recovery from local headers, zip-bomb limits (APPNOTE) |
| `docboss-xml` | Zero-copy pull XML tokenizer with namespace resolution; transitional and strict OOXML namespaces map to the same ids |
| `docboss-docx` | OPC packages and WordprocessingML (ECMA-376 Parts 1 to 3), parts parsed on separate threads |
| `docboss-cfb` | Compound files ([MS-CFB]) and property sets ([MS-OLEPS]) |
| `docboss-doc` | Word binary documents ([MS-DOC], [MS-ODRAW] pictures), RC4 and RC4 CryptoAPI decryption |
| `docboss-crypt` | Password-protected DOCX ([MS-OFFCRYPTO] Agile and Standard) |
| `docboss-core` | Format detection and one `open`/`read` over both readers |
| `docboss-output` | Text, Markdown, HTML, JSON and the per-block view |
| `docboss-font` | TrueType, OpenType CFF and collections; system font discovery and metric-compatible substitution |
| `docboss-layout` | Pages from the model: line breaking, tabs, lists, tables, sections, headers and footers, footnotes, images |
| `docboss-render` | Anti-aliased rasterizer and the PNG, PPM, BMP and JPEG encoders |
| `docboss-write` | DOCX from the model, a builder API, Markdown to DOCX |
| `docboss-aio` | Async reads over files or HTTP, fetching only the byte ranges needed |
| `docboss-cli` | The `docboss` command-line tool |
| `docboss-tui` | The terminal explorer |
| `docboss-py` | The PyO3 extension module (`docboss._docboss`) |

</details>

## Limitations

The reader is lenient and it says so: `docboss diagnostics` (the `diagnostics` property in Python, the `diagnostics` field of `Document` in Rust) lists every item that was approximated or dropped. The largest gaps:

- **Layout**: no right-to-left or bidirectional text and no complex-script shaping; no column balancing; text does not wrap around floating images; text boxes, shapes, SmartArt, charts and equations are not drawn; conditional formatting from table styles is not applied. CFF2 fonts are refused. WMF, EMF and TIFF images draw a placeholder and are reported.
- **DOCX reading**: `w:altChunk` content is skipped and reported; DrawingML charts, SmartArt, math (OMML) and ActiveX controls are not read as content; ruby text is ignored.
- **DOC reading**: XOR-obfuscated files are refused with an error; Word 2, 6 and 95 files are read as text only; shapes that are neither a picture nor a text box (lines, WordArt) are reported as dropped.
- **Writing**: embedded fonts are not written (the font table lists names only), no theme part is written, and raw HTML in Markdown is dropped except `<br>`.

The [conformance ledger](docs/src/reference/conformance.md) lists every clause with its status and a note saying what is missing.

## Conformance ledger

`ledger/` is a Lean 4 project. `lake exe ledger-index` scans the Rust and Python sources for specification citations (`ECMA-376 Part 1 §17.3.1.29`, `[MS-DOC] §2.5.1`, `APPNOTE §4.3.7`, ...), and the gate's theorems hold each ledger row to them: an `implemented` row needs a code citation and a test citation, an `incomplete` one a code citation and a note, every required clause of each standard needs a row, every row must name a real clause by its real title, and a citation of a clause the standard does not have fails the build. The outlines of the nine standards are generated from their published texts. `make ledger-gate` runs it; `make ledger-report` regenerates the [conformance table](docs/src/reference/conformance.md). The gate also checks Lean reference implementations of CRC-32 and the list number formats, whose differential vectors the Rust tests are held to.

## Failure modes

[`failure-modes/`](failure-modes/README.md) keeps a before and after render for each rendering bug fixed, named after the commit that fixed it, with the failure described in measured terms.

## Development

```bash
make ci           # rustfmt check, clippy -D warnings, tests, rustdoc
make test-py      # build the extension and run the Python tests
make ledger-gate  # the Lean conformance gate
make help         # everything else
```

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.

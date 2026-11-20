# Python API reference

`import docboss` exposes the classes and functions below; `docboss.md` holds
the Markdown-to-DOCX function. Type stubs ship with the package
(`_docboss.pyi`, `py.typed`).

## `docboss.Document`

```text
Document(path=None, *, data=None, password=None)
```

Opens a DOCX or DOC file from a path, or from `data` (`bytes`, `bytearray` or
`memoryview`); exactly one of the two must be given, otherwise `TypeError`.
The format is detected from the bytes. `password` opens an encrypted DOCX or
DOC. Bad input, a missing or wrong password, and unsupported formats raise
`DocbossError`. Documents are immutable and can be used from any thread;
heavy calls release the GIL.

| Member | Returns |
|---|---|
| `format` | `"docx"` or `"doc"` |
| `metadata` | `Metadata`: `title`, `subject`, `creator`, `keywords`, `description`, `last_modified_by`, `revision`, `created`, `modified`, `category`, `application`, `company`, `pages`, `words`, `characters` (each `None` when absent) |
| `diagnostics` | `list[Diagnostic]`: `severity` (`"approximated"` or `"dropped"`), `location`, `message` |
| `fonts` | `list[FontInfo]`: `name`, `alt_name`, `family`, `pitch`, `embedded` |
| `styles` | `list[StyleInfo]`: `id`, `name`, `kind` (`"paragraph"`, `"character"`, `"table"`, `"numbering"`), `based_on`, `is_default` |
| `to(spec=None, /, **options)` | the whole document as `str` in a text format: one dict such as `{"format": "markdown"}` or the same keys as keyword arguments (see [Pages and formats](../guide/pages.md)) |
| `pages()` | `Pages`: every laid-out page |
| `page(index)` | `Page`; negative counts from the end |
| `extract_text(*, list_labels=True, headers_footers=False, notes=True, comments=False)` | `str`; same as `to(format="text", ...)` |
| `extract_markdown(*, title=False, page_breaks=False, images="reference", image_prefix="", headers_footers=False, notes=True, comments=False)` | `str`; same as `to(format="markdown", ...)` |
| `extract_html(*, standalone=False, images="embed", image_prefix="", headers_footers=False, notes=True, comments=False)` | `str`; same as `to(format="html", ...)` |
| `to_json(*, pretty=False)` | the document model as a JSON string; same as `to(format="json", ...)` |
| `blocks()` | `list[Block]`: `kind` (`"paragraph"`, `"heading"`, `"list_item"`, `"table"`), `section`, `style`, `heading_level`, `list_level`, `list_label`, `text` |
| `images()` | `list[Image]`: `name`, `file_name`, `content_type`, `data` |
| `page_count()` | the number of pages after layout; same as `len(pages())` |
| `render(page=0, *, scale=1.0, format="png", jpeg_quality=90)` | encoded image `bytes`; `page` is 0-based, negative counts from the end; `format` is `"png"`, `"ppm"`, `"bmp"` or `"jpeg"`; same as `page(page).to(format=..., scale=...)` |
| `render_pages(pages=None, *, scale=1.0, format="png", jpeg_quality=90)` | `list[bytes]` in page order, rendered in parallel; `None` renders every page |
| `to_docx()` | a fresh DOCX as `bytes` |
| `save_docx(path)` | writes that DOCX |

`images` in the Markdown and HTML methods is `"reference"` (link to
`image_prefix` plus the image's file name), `"embed"` (data: URIs) or
`"omit"`.

Layout runs once per document and is cached; documents without embedded
fonts share one font database for the process. `to()`, `pages()` and
`page()` are the main API; the `extract_*`, `to_json`, `render`,
`render_pages` and `page_count` methods stay as shortcuts for the same
conversions.

## `docboss.Pages` and `docboss.Page`

`Pages` is a selection of laid-out pages: `len()`, iteration, `pages[i]` (a
`Page`, negative counts from the end) and `pages[a:b:c]` (another `Pages`).
`pages.to(...)` returns one output per page: `list[str]` for text formats,
`list[bytes]` for image formats.

`Page` has `index` (0-based position in the document), `number` (the number
`PAGE` fields show) and `to(...)`, returning `str` or `bytes`. A page's text
formats hold the body blocks that start on it and the notes they reference
first.

## Formats

`docboss.Text`, `Markdown`, `Html`, `Json`, `Png`, `Ppm`, `Bmp` and `Jpeg` are
`TypedDict`s for the `to()` argument; `TextFormat`, `ImageFormat` and
`Format` are their unions. The keys and defaults are listed in
[Pages and formats](../guide/pages.md).

## `docboss.AsyncDocument`

The asynchronous reader, over files, bytes or `http(s)://` URLs, fetching only
the byte ranges each call needs.

| Member | |
|---|---|
| `await AsyncDocument.open(path, *, password=None)` | a local file |
| `await AsyncDocument.from_bytes(data, *, password=None)` | a buffer |
| `await AsyncDocument.open_url(url, *, password=None)` | an http(s) URL, read with range requests |
| `format` | `"docx"`, `"doc"` or `"unknown"` |
| `bytes_fetched`, `requests` | bytes read so far, and each `(offset, length)` request |
| `part_names()` | the ZIP entries or compound-file streams |
| `await part(name)` | one entry or stream as `bytes` |
| `await extract_text(...)`, `await extract_markdown(...)`, `await extract_html(...)` | as on `Document` |
| `await to_json(*, pretty=False)`, `await blocks()`, `await images()` | as on `Document` |
| `await to(spec=None, /, **options)` | the whole document in a text format, as on `Document`; media is fetched only for `images="embed"` |
| `pages()`, `page(index)` | a lazy `AsyncPages` or `AsyncPage`; slicing and indexing an `AsyncPages` stay lazy, and `await to(...)` (or `await count()` on `AsyncPages`) fetches the document, lays it out on the blocking pool and converts |
| `await document()` | the synchronous `Document`, for rendering and DOCX writing |

## Functions

| Function | |
|---|---|
| `docboss.detect(data)` | `"docx"`, `"doc"`, `"encrypted_docx"`, `"rtf"` or `"unknown"` from the bytes |
| `docboss.extract_texts(paths, threads=None, strict=True)` | the text of every file, read on a Rust thread pool in one call; with `strict=False` a failed file gives `None` instead of raising |
| `docboss.md.to_docx(markdown, images_dir=None, title=None)` | CommonMark + GFM composed into DOCX `bytes`; relative image paths are read under `images_dir` |

## Exceptions

`docboss.DocbossError` (a subclass of `Exception`) is raised for every
document, password and I/O error.

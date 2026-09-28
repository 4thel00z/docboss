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
| `extract_text(*, list_labels=True, headers_footers=False, notes=True, comments=False)` | `str` |
| `extract_markdown(*, title=False, page_breaks=False, images="reference", image_prefix="", headers_footers=False, notes=True, comments=False)` | `str` |
| `extract_html(*, standalone=False, images="embed", image_prefix="", headers_footers=False, notes=True, comments=False)` | `str` |
| `to_json(*, pretty=False)` | the document model as a JSON string |
| `blocks()` | `list[Block]`: `kind` (`"paragraph"`, `"heading"`, `"list_item"`, `"table"`), `section`, `style`, `heading_level`, `list_level`, `list_label`, `text` |
| `images()` | `list[Image]`: `name`, `file_name`, `content_type`, `data` |
| `page_count()` | the number of pages after layout |
| `render(page=0, *, scale=1.0, format="png", jpeg_quality=90)` | encoded image `bytes`; `page` is 0-based, negative counts from the end; `format` is `"png"`, `"ppm"`, `"bmp"` or `"jpeg"` |
| `render_pages(pages=None, *, scale=1.0, format="png", jpeg_quality=90)` | `list[bytes]` in page order, rendered in parallel; `None` renders every page |
| `to_docx()` | a fresh DOCX as `bytes` |
| `save_docx(path)` | writes that DOCX |

`images` in the Markdown and HTML methods is `"reference"` (link to
`image_prefix` plus the image's file name), `"embed"` (data: URIs) or
`"omit"`.

Layout runs once per document and is cached; documents without embedded
fonts share one font database for the process.

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

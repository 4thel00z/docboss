# Remote documents over HTTP

Every read command accepts an `http(s)://` URL:

```bash
docboss text https://example.com/report.docx
docboss info https://example.com/legacy.doc
```

The file is not downloaded whole. For a DOCX, docboss fetches the end of the
file to find the ZIP central directory, then only the entries the reader
needs: content types, relationships, `document.xml`, styles, numbering,
notes and comments. Images are fetched only when a command needs them. For a
DOC, it fetches the compound-file header, the FAT and directory sectors, and
then only the sectors of the Word streams. Requests are coalesced and cached,
so no byte is fetched twice; a server that ignores `Range` costs one full
download. On a terminal, a line of coverage shows which parts of the file
were fetched.

On Apache POI's `saut_page.docx` (2.96 MB, mostly images), extracting the text
fetched 17.9 KB in 3 requests; a DOC reads its Data stream too, so it saves
less.

## Rust

`docboss-aio` with the `http` feature:

```rust,no_run
use docboss_aio::{AsyncDocument, ReadOptions};
use docboss_output::{to_text, TextOptions};

# async fn run() -> docboss_aio::Result<()> {
let remote = AsyncDocument::open_url("https://example.com/report.docx").await?;
let doc = remote.read(&ReadOptions::default()).await?;
println!("{}", to_text(&doc, &TextOptions::default()));
println!("{} of {} bytes in {} requests", remote.bytes_fetched(), remote.len(), remote.requests().len());
# Ok(()) }
```

`ReadOptions { media: true, .. }` fetches images too; otherwise
`AsyncDocument::load_media` fetches them later. `AsyncDocument::open` and
`from_bytes` give the same interface over local files and memory, and
`from_backend` over any byte source that implements `Backend`.

## Python

```python
import asyncio

import docboss

async def main() -> None:
    doc = await docboss.AsyncDocument.open_url("https://example.com/report.docx")
    print(doc.format, doc.part_names()[:3])
    text = await doc.extract_text()
    markdown = await doc.extract_markdown(images="omit")
    styles = await doc.part("word/styles.xml")
    print(doc.bytes_fetched, "bytes in", len(doc.requests), "requests")

asyncio.run(main())
```

`AsyncDocument.open(path)`, `from_bytes(data)` and `open_url(url)` are
coroutines and take `password=`. The extraction methods mirror `Document`'s
and are awaitable: `extract_text`, `extract_markdown`, `extract_html`,
`to_json`, `blocks`, `images`, and `part(name)` for one ZIP entry or stream.
`bytes_fetched` and `requests` report what was read so far, and
`await doc.document()` returns a synchronous `Document` for rendering,
DOCX writing and the rest of its API; images are fetched when a call needs
them.

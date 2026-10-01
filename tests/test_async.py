"""AsyncDocument: async reads over files, bytes and HTTP range requests.

The HTTP tests run a local server that honors ``Range`` and one that
ignores it, and compare every result with the synchronous ``Document``.
"""

import random
import struct
import threading
import zlib
from collections.abc import Iterator
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

import pytest

import docboss
from docboss import AsyncDocument, Document

ROOT = Path(__file__).parent.parent
CRYPT = ROOT / "crates" / "docboss-crypt" / "tests" / "fixtures"


def noise_png(width: int, height: int) -> bytes:
    """An RGB PNG of random pixels, so deflate cannot shrink it."""
    rng = random.Random(7)
    rows = b"".join(b"\x00" + rng.randbytes(width * 3) for _ in range(height))

    def chunk(kind: bytes, data: bytes) -> bytes:
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))

    header = struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", zlib.compress(rows)) + chunk(b"IEND", b"")


@pytest.fixture(scope="module")
def big_image_docx(tmp_path_factory: pytest.TempPathFactory) -> bytes:
    directory = tmp_path_factory.mktemp("big")
    (directory / "noise.png").write_bytes(noise_png(600, 600))
    markdown = "# Report\n\nText before the picture.\n\n![noise](noise.png)\n\nText after it.\n"
    return docboss.md.to_docx(markdown, images_dir=directory)


class RangeHandler(BaseHTTPRequestHandler):
    """Serves one payload, answering single `bytes=` ranges with 206."""

    protocol_version = "HTTP/1.1"
    payload: bytes = b""

    def log_message(self, format: str, *args: object) -> None:
        """Keeps pytest output clean."""

    def full(self) -> bytes:
        data = type(self).payload
        self.send_response(200)
        self.send_header("Content-Length", str(len(data)))
        self.send_header("Accept-Ranges", "bytes")
        self.end_headers()
        return data

    def do_HEAD(self) -> None:
        self.full()

    def do_GET(self) -> None:
        data = type(self).payload
        header = self.headers.get("Range", "")
        if not header.startswith("bytes="):
            self.wfile.write(self.full())
            return
        first, _, last = header.removeprefix("bytes=").partition("-")
        size = len(data)
        start, end = (size - int(last), size - 1) if first == "" else (int(first), int(last or size - 1))
        start, end = max(start, 0), min(end, size - 1)
        body = data[start : end + 1]
        self.send_response(206)
        self.send_header("Content-Range", f"bytes {start}-{end}/{size}")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Accept-Ranges", "bytes")
        self.end_headers()
        self.wfile.write(body)


class NoRangeHandler(BaseHTTPRequestHandler):
    """Ignores `Range`: every GET answers 200 with the whole payload."""

    protocol_version = "HTTP/1.1"
    payload: bytes = b""

    def log_message(self, format: str, *args: object) -> None:
        """Keeps pytest output clean."""

    def do_HEAD(self) -> None:
        self.send_response(200)
        self.send_header("Content-Length", str(len(type(self).payload)))
        self.end_headers()

    def do_GET(self) -> None:
        data = type(self).payload
        self.send_response(200)
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)


def serve(base: type[BaseHTTPRequestHandler], payload: bytes) -> Iterator[str]:
    handler = type("Handler", (base,), {"payload": payload})
    server = ThreadingHTTPServer(("127.0.0.1", 0), handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        yield f"http://127.0.0.1:{server.server_address[1]}/doc"
    finally:
        server.shutdown()
        thread.join()
        server.server_close()


@pytest.fixture
def range_url(big_image_docx: bytes) -> Iterator[str]:
    yield from serve(RangeHandler, big_image_docx)


@pytest.fixture
def no_range_url(big_image_docx: bytes) -> Iterator[str]:
    yield from serve(NoRangeHandler, big_image_docx)


@pytest.mark.asyncio
async def test_open_matches_the_sync_reader(rich_docx: Path, lists_doc: Path) -> None:
    for path in (rich_docx, lists_doc):
        sync = Document(path)
        doc = await AsyncDocument.open(path)
        assert doc.format == sync.format
        assert await doc.extract_text() == sync.extract_text()
        assert await doc.extract_markdown() == sync.extract_markdown()
        assert await doc.to_json() == sync.to_json()
        assert [b.text for b in await doc.blocks()] == [b.text for b in sync.blocks()]


@pytest.mark.asyncio
async def test_from_bytes_accepts_buffers(rich_docx: Path) -> None:
    data = rich_docx.read_bytes()
    expected = Document(rich_docx).extract_text()
    for buffer in (data, bytearray(data), memoryview(data)):
        doc = await AsyncDocument.from_bytes(buffer)
        assert await doc.extract_text() == expected


@pytest.mark.asyncio
async def test_options_pass_through(rich_docx: Path) -> None:
    sync = Document(rich_docx)
    doc = await AsyncDocument.open(rich_docx)
    assert await doc.extract_text(headers_footers=True, notes=False) == sync.extract_text(
        headers_footers=True, notes=False
    )
    assert await doc.extract_html(standalone=True) == sync.extract_html(standalone=True)
    with pytest.raises(ValueError, match="images"):
        await doc.extract_markdown(images="inline")


@pytest.mark.asyncio
async def test_text_over_range_requests_skips_the_image(range_url: str, big_image_docx: bytes) -> None:
    doc = await AsyncDocument.open_url(range_url)
    text = await doc.extract_text()
    assert text == Document(data=big_image_docx).extract_text()
    assert "Text after it." in text
    assert doc.bytes_fetched < len(big_image_docx) // 4
    assert all(length > 0 for _, length in doc.requests)
    assert sum(length for _, length in doc.requests) == doc.bytes_fetched


@pytest.mark.asyncio
async def test_images_are_fetched_on_demand(range_url: str, big_image_docx: bytes) -> None:
    doc = await AsyncDocument.open_url(range_url)
    await doc.extract_text()
    before = doc.bytes_fetched
    images = await doc.images()
    assert len(images) == 1
    assert images[0].data == Document(data=big_image_docx).images()[0].data
    assert doc.bytes_fetched > before


@pytest.mark.asyncio
async def test_document_renders_like_the_sync_reader(range_url: str, big_image_docx: bytes) -> None:
    doc = await AsyncDocument.open_url(range_url)
    rendered = await doc.document()
    assert rendered.page_count() == Document(data=big_image_docx).page_count()
    assert rendered.render(0, scale=0.5).startswith(b"\x89PNG")


EQUATION = ROOT / "crates" / "docboss-docx" / "tests" / "fixtures" / "equation-editor.docx"


@pytest.fixture
def equation_url() -> Iterator[str]:
    yield from serve(RangeHandler, EQUATION.read_bytes())


@pytest.mark.asyncio
async def test_equations_read_like_the_sync_reader(equation_url: str) -> None:
    sync = Document(EQUATION)
    for doc in (await AsyncDocument.open(EQUATION), await AsyncDocument.open_url(equation_url)):
        assert "a=b/c" in await doc.extract_text()
        assert await doc.extract_text() == sync.extract_text()
        assert await doc.extract_markdown() == sync.extract_markdown()


@pytest.mark.asyncio
async def test_server_ignoring_range_still_reads(no_range_url: str, big_image_docx: bytes) -> None:
    doc = await AsyncDocument.open_url(no_range_url)
    assert await doc.extract_text() == Document(data=big_image_docx).extract_text()
    assert (await doc.images())[0].data == Document(data=big_image_docx).images()[0].data


@pytest.mark.asyncio
async def test_parts_by_name(rich_docx: Path, lists_doc: Path) -> None:
    docx = await AsyncDocument.open(rich_docx)
    assert "word/document.xml" in docx.part_names()
    assert (await docx.part("word/document.xml")).lstrip().startswith(b"<?xml")
    doc = await AsyncDocument.open(lists_doc)
    assert "WordDocument" in doc.part_names()
    with pytest.raises(docboss.DocbossError, match="not found"):
        await docx.part("word/missing.xml")


@pytest.mark.asyncio
async def test_passwords_open_encrypted_files(rich_docx: Path) -> None:
    encrypted = CRYPT / "Encrypted_MSO2013_abc.docx"
    doc = await AsyncDocument.open(encrypted, password="abc")
    assert await doc.extract_text() == Document(encrypted, password="abc").extract_text()
    locked = await AsyncDocument.open(encrypted)
    with pytest.raises(docboss.DocbossError, match="password"):
        await locked.extract_text()


@pytest.mark.asyncio
async def test_open_errors_raise() -> None:
    with pytest.raises(docboss.DocbossError):
        await AsyncDocument.open("/nonexistent/file.docx")
    doc = await AsyncDocument.from_bytes(b"plain text, not a document")
    with pytest.raises(docboss.DocbossError):
        await doc.extract_text()

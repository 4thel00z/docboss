import os
from collections.abc import Sequence
from typing import Literal

__version__: str

Buffer = bytes | bytearray | memoryview
ImageFormat = Literal["png", "ppm", "bmp", "jpeg", "jpg"]
ImagesMode = Literal["reference", "embed", "omit"]

class DocbossError(Exception): ...

class Metadata:
    title: str | None
    subject: str | None
    creator: str | None
    keywords: str | None
    description: str | None
    last_modified_by: str | None
    revision: str | None
    created: str | None
    modified: str | None
    category: str | None
    application: str | None
    company: str | None
    pages: int | None
    words: int | None
    characters: int | None

class Diagnostic:
    severity: Literal["approximated", "dropped"]
    location: str
    message: str

class FontInfo:
    name: str
    alt_name: str | None
    family: str | None
    pitch: str | None
    embedded: bool

class StyleInfo:
    id: str
    name: str | None
    kind: Literal["paragraph", "character", "table", "numbering"]
    based_on: str | None
    is_default: bool

class Block:
    kind: Literal["paragraph", "heading", "list_item", "table"]
    section: int
    style: str | None
    heading_level: int | None
    list_level: int | None
    list_label: str | None
    text: str

class Image:
    name: str
    file_name: str
    content_type: str
    @property
    def data(self) -> bytes: ...

class AsyncDocument:
    """A DOCX or DOC read asynchronously over a file, bytes or an http(s) URL,
    fetching only the byte ranges each call needs."""

    @staticmethod
    async def open(path: str | os.PathLike, *, password: str | None = None) -> AsyncDocument: ...
    @staticmethod
    async def from_bytes(data: Buffer, *, password: str | None = None) -> AsyncDocument: ...
    @staticmethod
    async def open_url(url: str, *, password: str | None = None) -> AsyncDocument: ...
    @property
    def format(self) -> Literal["docx", "doc", "unknown"]: ...
    @property
    def bytes_fetched(self) -> int: ...
    @property
    def requests(self) -> list[tuple[int, int]]: ...
    def part_names(self) -> list[str]: ...
    async def part(self, name: str) -> bytes: ...
    async def extract_text(
        self,
        *,
        list_labels: bool = True,
        headers_footers: bool = False,
        notes: bool = True,
        comments: bool = False,
    ) -> str: ...
    async def extract_markdown(
        self,
        *,
        title: bool = False,
        page_breaks: bool = False,
        images: ImagesMode = "reference",
        image_prefix: str = "",
        headers_footers: bool = False,
        notes: bool = True,
        comments: bool = False,
    ) -> str: ...
    async def extract_html(
        self,
        *,
        standalone: bool = False,
        images: ImagesMode = "embed",
        image_prefix: str = "",
        headers_footers: bool = False,
        notes: bool = True,
        comments: bool = False,
    ) -> str: ...
    async def to_json(self, *, pretty: bool = False) -> str: ...
    async def blocks(self) -> list[Block]: ...
    async def images(self) -> list[Image]: ...
    async def document(self) -> Document: ...

class Document:
    def __init__(
        self,
        path: str | os.PathLike | None = None,
        *,
        data: Buffer | None = None,
        password: str | None = None,
    ) -> None: ...
    @property
    def format(self) -> Literal["docx", "doc"]: ...
    @property
    def metadata(self) -> Metadata: ...
    @property
    def diagnostics(self) -> list[Diagnostic]: ...
    @property
    def fonts(self) -> list[FontInfo]: ...
    @property
    def styles(self) -> list[StyleInfo]: ...
    def extract_text(
        self,
        *,
        list_labels: bool = True,
        headers_footers: bool = False,
        notes: bool = True,
        comments: bool = False,
    ) -> str: ...
    def extract_markdown(
        self,
        *,
        title: bool = False,
        page_breaks: bool = False,
        images: ImagesMode = "reference",
        image_prefix: str = "",
        headers_footers: bool = False,
        notes: bool = True,
        comments: bool = False,
    ) -> str: ...
    def extract_html(
        self,
        *,
        standalone: bool = False,
        images: ImagesMode = "embed",
        image_prefix: str = "",
        headers_footers: bool = False,
        notes: bool = True,
        comments: bool = False,
    ) -> str: ...
    def to_json(self, *, pretty: bool = False) -> str: ...
    def blocks(self) -> list[Block]: ...
    def images(self) -> list[Image]: ...
    def page_count(self) -> int: ...
    def render(
        self,
        page: int = 0,
        *,
        scale: float = 1.0,
        format: ImageFormat = "png",
        jpeg_quality: int = 90,
    ) -> bytes: ...
    def render_pages(
        self,
        pages: Sequence[int] | None = None,
        *,
        scale: float = 1.0,
        format: ImageFormat = "png",
        jpeg_quality: int = 90,
    ) -> list[bytes]: ...
    def to_docx(self) -> bytes: ...
    def save_docx(self, path: str | os.PathLike) -> None: ...

def detect(data: Buffer) -> Literal["docx", "doc", "encrypted_docx", "rtf", "unknown"]: ...
def extract_texts(
    paths: Sequence[str | os.PathLike],
    threads: int | None = None,
    strict: bool = True,
) -> list[str | None]: ...
def md_to_docx(markdown: str, images_dir: str | None = None, title: str | None = None) -> bytes: ...

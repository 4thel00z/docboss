"""DOCX and DOC parsing, text extraction and rendering in pure Rust."""

from docboss import md
from docboss._docboss import (
    AsyncDocument,
    Block,
    Diagnostic,
    DocbossError,
    Document,
    FontInfo,
    Image,
    Metadata,
    StyleInfo,
    __version__,
    detect,
    extract_texts,
)

__all__ = [
    "AsyncDocument",
    "Block",
    "Diagnostic",
    "DocbossError",
    "Document",
    "FontInfo",
    "Image",
    "Metadata",
    "StyleInfo",
    "__version__",
    "detect",
    "extract_texts",
    "md",
]

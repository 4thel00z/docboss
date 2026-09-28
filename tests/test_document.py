"""Opening documents and reading their model through the Python API."""

from pathlib import Path

import pytest

import docboss
from conftest import DOC, DOCX


def test_docx_opens_from_a_path(rich_docx: Path) -> None:
    doc = docboss.Document(rich_docx)
    assert doc.format == "docx"
    assert doc.metadata.title == "Fixture Document"
    assert "Fixture Document" in repr(doc)


def test_doc_opens_from_a_str_path(lists_doc: Path) -> None:
    doc = docboss.Document(str(lists_doc))
    assert doc.format == "doc"
    assert doc.metadata.creator == "Ada Lovelace"


@pytest.mark.parametrize("wrap", [bytes, bytearray, memoryview])
def test_bytes_like_input(rich_docx: Path, wrap: type) -> None:
    doc = docboss.Document(data=wrap(rich_docx.read_bytes()))
    assert doc.format == "docx"


def test_path_and_data_are_exclusive(rich_docx: Path) -> None:
    with pytest.raises(TypeError):
        docboss.Document(rich_docx, data=rich_docx.read_bytes())
    with pytest.raises(TypeError):
        docboss.Document()


def test_garbage_raises_docboss_error() -> None:
    with pytest.raises(docboss.DocbossError, match="unsupported"):
        docboss.Document(data=b"not a document")


def test_missing_file_raises_docboss_error(tmp_path: Path) -> None:
    with pytest.raises(docboss.DocbossError):
        docboss.Document(tmp_path / "absent.docx")


def test_encrypted_doc_needs_its_password() -> None:
    path = DOC / "encrypted-cryptoapi.doc"
    with pytest.raises(docboss.DocbossError, match="password"):
        docboss.Document(path)
    with pytest.raises(docboss.DocbossError, match="password"):
        docboss.Document(path, password="nope")
    doc = docboss.Document(path, password="password")
    assert doc.format == "doc"
    assert doc.extract_text().strip()


def test_detect_reads_the_bytes(rich_docx: Path, lists_doc: Path) -> None:
    assert docboss.detect(rich_docx.read_bytes()) == "docx"
    assert docboss.detect(lists_doc.read_bytes()) == "doc"
    assert docboss.detect(b"{\\rtf1 hi}") == "rtf"
    assert docboss.detect(b"hello") == "unknown"


def test_model_views(rich_docx: Path) -> None:
    doc = docboss.Document(rich_docx)
    assert isinstance(doc.diagnostics, list)
    assert all(isinstance(style, docboss.StyleInfo) for style in doc.styles)
    assert any(style.id == "Heading1" and style.kind == "paragraph" for style in doc.styles)
    assert any(style.kind == "character" for style in doc.styles)
    assert all(isinstance(font, docboss.FontInfo) for font in doc.fonts)


def test_blocks_carry_roles(rich_docx: Path) -> None:
    blocks = docboss.Document(rich_docx).blocks()
    headings = [block for block in blocks if block.kind == "heading"]
    assert headings[0].text == "Fixture Heading"
    assert headings[0].heading_level == 1
    assert any(block.kind == "list_item" and block.text == "Item one" for block in blocks)
    assert any(block.kind == "table" for block in blocks)


def test_images_carry_bytes(image_doc: Path) -> None:
    images = docboss.Document(image_doc).images()
    assert len(images) == 1
    image = images[0]
    assert image.content_type == "image/png"
    assert image.file_name == "image1.png"
    assert image.data.startswith(b"\x89PNG\r\n\x1a\n")


def test_every_fixture_opens() -> None:
    paths = sorted(DOCX.glob("*.docx")) + [p for p in sorted(DOC.glob("*.doc")) if "encrypted" not in p.name]
    assert paths
    for path in paths:
        assert docboss.Document(path).extract_text() is not None

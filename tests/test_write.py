"""DOCX writing and Markdown to DOCX."""

from pathlib import Path

import docboss
from conftest import DOCX


def test_doc_converts_to_docx(lists_doc: Path, tmp_path: Path) -> None:
    doc = docboss.Document(lists_doc)
    data = doc.to_docx()
    assert data.startswith(b"PK\x03\x04")
    again = docboss.Document(data=data)
    assert again.format == "docx"
    assert again.extract_text() == doc.extract_text()
    out = tmp_path / "lists.docx"
    doc.save_docx(out)
    assert out.read_bytes() == data


def test_markdown_to_docx_roundtrips() -> None:
    data = docboss.md.to_docx("# Title\n\nSome **bold** text.\n\n- one\n- two\n")
    doc = docboss.Document(data=data)
    md = doc.extract_markdown()
    assert md.startswith("# Title")
    assert "**bold**" in md
    assert "- one" in md


def test_markdown_images_resolve_under_a_directory() -> None:
    markdown = "![a red box](red.png)\n"
    with_image = docboss.Document(data=docboss.md.to_docx(markdown, images_dir=DOCX))
    assert [image.content_type for image in with_image.images()] == ["image/png"]
    without = docboss.Document(data=docboss.md.to_docx(markdown))
    assert without.images() == []
    assert "a red box" in without.extract_text()


def test_markdown_output_is_deterministic() -> None:
    assert docboss.md.to_docx("# Same\n") == docboss.md.to_docx("# Same\n")

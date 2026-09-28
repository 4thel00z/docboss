"""Text, Markdown, HTML and JSON extraction."""

import json
from pathlib import Path

import pytest

import docboss
from conftest import DOC


def test_doc_text_matches_libreoffice(lists_doc: Path) -> None:
    text = docboss.Document(lists_doc).extract_text()
    assert text.splitlines() == ["1. First item", "a) Nested a", "b) Nested b", "2. Second item", "• Bullet one", "• Bullet two"]


def test_list_labels_can_be_left_out(lists_doc: Path) -> None:
    text = docboss.Document(lists_doc).extract_text(list_labels=False)
    assert text.splitlines()[0] == "First item"


def test_notes_are_appended_or_left_out(rich_docx: Path) -> None:
    doc = docboss.Document(rich_docx)
    assert "Footnote text." in doc.extract_text()
    assert "Footnote text." not in doc.extract_text(notes=False)


def test_markdown_structure(rich_docx: Path) -> None:
    md = docboss.Document(rich_docx).extract_markdown()
    assert md.startswith("# Fixture Heading")
    assert "**bold words**" in md
    assert "- Item one" in md
    assert "| A1 | B1 |" in md
    assert "[^1]: Footnote text." in md
    assert "![a red box](image1.png)" in md


def test_markdown_image_modes(rich_docx: Path) -> None:
    doc = docboss.Document(rich_docx)
    assert "![a red box](media/image1.png)" in doc.extract_markdown(image_prefix="media/")
    assert "data:image/png;base64," in doc.extract_markdown(images="embed")
    assert "![" not in doc.extract_markdown(images="omit")
    with pytest.raises(ValueError):
        doc.extract_markdown(images="inline")


def test_html(rich_docx: Path) -> None:
    doc = docboss.Document(rich_docx)
    fragment = doc.extract_html()
    assert "<h1>Fixture Heading</h1>" in fragment
    assert "<strong>bold words</strong>" in fragment
    assert "<!DOCTYPE html>" not in fragment
    page = doc.extract_html(standalone=True)
    assert page.startswith("<!DOCTYPE html>")
    assert "<title>Fixture Document</title>" in page


def test_json_is_the_model(rich_docx: Path) -> None:
    model = json.loads(docboss.Document(rich_docx).to_json())
    assert model["format"] == "docx"
    assert model["metadata"]["title"] == "Fixture Document"
    assert model["sections"]
    assert docboss.Document(rich_docx).to_json(pretty=True).startswith("{\n")


def test_extract_texts_batch(rich_docx: Path, lists_doc: Path) -> None:
    texts = docboss.extract_texts([rich_docx, lists_doc, rich_docx], threads=2)
    assert texts[0] == texts[2] == docboss.Document(rich_docx).extract_text()
    assert texts[1] == docboss.Document(lists_doc).extract_text()


def test_extract_texts_strictness(rich_docx: Path, tmp_path: Path) -> None:
    missing = tmp_path / "missing.docx"
    with pytest.raises(docboss.DocbossError, match="missing.docx"):
        docboss.extract_texts([rich_docx, missing])
    texts = docboss.extract_texts([rich_docx, missing], strict=False)
    assert texts[0] and texts[1] is None
    assert docboss.extract_texts([]) == []


def test_doc_text_fixtures_agree_with_libreoffice_exports() -> None:
    for name in ["text", "tables", "sections"]:
        expected = (DOC / f"{name}.txt").read_text(encoding="utf-8-sig").split()
        got = docboss.Document(DOC / f"{name}.doc").extract_text().split()
        recall = sum(1 for word in expected if word in got) / len(expected)
        assert recall > 0.9, (name, recall)

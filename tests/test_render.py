"""Layout and rendering to every image format."""

from pathlib import Path

import pytest

import docboss

MAGIC = {
    "png": b"\x89PNG\r\n\x1a\n",
    "jpeg": b"\xff\xd8\xff",
    "jpg": b"\xff\xd8\xff",
    "bmp": b"BM",
    "ppm": b"P6",
}


def test_page_count(rich_docx: Path) -> None:
    assert docboss.Document(rich_docx).page_count() >= 1


@pytest.mark.parametrize("format", sorted(MAGIC))
def test_render_formats(rich_docx: Path, format: str) -> None:
    data = docboss.Document(rich_docx).render(0, format=format)
    assert data.startswith(MAGIC[format])


def test_scale_changes_size(rich_docx: Path) -> None:
    doc = docboss.Document(rich_docx)
    small = doc.render(0, format="bmp")
    large = doc.render(0, scale=2.0, format="bmp")
    assert len(large) > 3 * len(small)


def test_negative_index_and_range(rich_docx: Path) -> None:
    doc = docboss.Document(rich_docx)
    assert doc.render(-1, format="ppm") == doc.render(doc.page_count() - 1, format="ppm")
    with pytest.raises(IndexError):
        doc.render(doc.page_count())


def test_bad_arguments(rich_docx: Path) -> None:
    doc = docboss.Document(rich_docx)
    with pytest.raises(ValueError):
        doc.render(0, format="gif")
    with pytest.raises(ValueError):
        doc.render(0, scale=0)


def test_render_pages_keeps_order(lists_doc: Path, rich_docx: Path) -> None:
    doc = docboss.Document(rich_docx)
    count = doc.page_count()
    everything = doc.render_pages(format="ppm")
    assert len(everything) == count
    assert everything[0] == doc.render(0, format="ppm")
    assert doc.render_pages([0, 0], format="ppm") == [everything[0], everything[0]]


def test_render_is_deterministic(image_doc: Path) -> None:
    doc = docboss.Document(image_doc)
    assert doc.render(0) == docboss.Document(image_doc).render(0)

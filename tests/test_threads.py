"""Frozen classes shared across threads; heavy calls release the GIL."""

from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

import docboss


def test_one_document_many_threads(rich_docx: Path) -> None:
    doc = docboss.Document(rich_docx)
    expected_text = doc.extract_text()
    expected_png = doc.render(0)
    with ThreadPoolExecutor(8) as pool:
        texts = list(pool.map(lambda _: doc.extract_text(), range(32)))
        pngs = list(pool.map(lambda _: doc.render(0), range(16)))
    assert set(texts) == {expected_text}
    assert set(pngs) == {expected_png}


def test_first_layout_raced_from_threads(rich_docx: Path) -> None:
    doc = docboss.Document(rich_docx)
    with ThreadPoolExecutor(8) as pool:
        counts = set(pool.map(lambda _: doc.page_count(), range(16)))
    assert len(counts) == 1


def test_documents_opened_on_threads(rich_docx: Path, lists_doc: Path) -> None:
    paths = [rich_docx, lists_doc] * 16
    with ThreadPoolExecutor(8) as pool:
        formats = list(pool.map(lambda p: docboss.Document(p).format, paths))
    assert formats == ["docx", "doc"] * 16

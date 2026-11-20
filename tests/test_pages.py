"""Document.to, pages() and page(): formats as dicts, keyword arguments or
TypedDicts, page selection, and the async chain."""

from collections import Counter

import pytest

import docboss
from docboss import AsyncDocument, DocbossError, Document, Markdown, Png, Text


def report_markdown() -> str:
    parts = ["# Report\n"]
    for i in range(160):
        words = " ".join(f"w{i}x{k}" for k in range(18))
        note = f"[^n{i}]" if i % 25 == 3 else ""
        parts.append(f"Paragraph p{i} {words}.{note}\n")
        if i % 50 == 20:
            parts.append(f"- item a{i}\n- item b{i}\n")
    parts.extend(f"[^n{i}]: Note u{i} for p{i}.\n" for i in range(160) if i % 25 == 3)
    return "\n".join(parts)


@pytest.fixture(scope="module")
def raw() -> bytes:
    return docboss.md.to_docx(report_markdown())


@pytest.fixture(scope="module")
def doc(raw: bytes) -> Document:
    return Document(data=raw)


def words(text: str) -> Counter[str]:
    return Counter(text.split())


def test_a_dict_keywords_and_a_typeddict_give_the_same_output(doc: Document) -> None:
    by_dict = doc.to({"format": "markdown", "images": "omit"})
    by_keywords = doc.to(format="markdown", images="omit")
    by_typeddict = doc.to(Markdown(format="markdown", images="omit"))
    assert by_dict == by_keywords == by_typeddict
    assert by_dict == doc.extract_markdown(images="omit")


def test_the_old_methods_match_to(doc: Document) -> None:
    assert doc.extract_text() == doc.to(format="text")
    assert doc.extract_html(standalone=True) == doc.to(format="html", standalone=True)
    assert doc.to_json(pretty=True) == doc.to(format="json", pretty=True)
    assert doc.page_count() == len(doc.pages())


def test_pages_index_iterate_and_number(doc: Document) -> None:
    pages = doc.pages()
    assert len(pages) > 4
    listed = list(pages)
    assert [page.index for page in listed] == list(range(len(pages)))
    assert [page.number for page in listed] == list(range(1, len(pages) + 1))
    assert pages[-1].index == len(pages) - 1
    assert doc.page(-2).index == len(pages) - 2


def test_slicing_selects_pages_like_a_list(doc: Document) -> None:
    pages = doc.pages()
    positions = list(range(len(pages)))
    for key in [slice(2, 6), slice(None, None, 2), slice(-3, None), slice(None, None, -1), slice(5, 1, -2)]:
        assert [page.index for page in pages[key]] == positions[key]
    assert [page.index for page in pages[1:][1:3]] == positions[1:][1:3]
    assert len(pages[100:200]) == 0
    with pytest.raises(ValueError):
        pages[::0]


def test_an_index_out_of_range_raises(doc: Document) -> None:
    count = len(doc.pages())
    with pytest.raises(IndexError):
        doc.pages()[count]
    with pytest.raises(IndexError):
        doc.page(-count - 1)


def test_a_page_converts_like_its_place_in_pages(doc: Document) -> None:
    every = doc.pages().to(format="text")
    assert doc.page(3).to(format="text") == every[3]
    assert doc.pages()[2:6].to(Text(format="text")) == every[2:6]


def test_pages_together_hold_every_word_once(doc: Document) -> None:
    for spec in [Text(format="text"), Markdown(format="markdown", images="omit")]:
        pages = doc.pages().to(spec)
        assert words("\n".join(pages)) == words(doc.to(spec))


def test_notes_go_on_the_page_of_their_reference(doc: Document) -> None:
    pages = doc.pages().to(format="text")
    for i in range(3, 160, 25):
        holding = [text for text in pages if f"u{i} " in text]
        assert len(holding) == 1
        assert f"p{i} " in holding[0]


def test_image_formats_give_bytes(doc: Document) -> None:
    png = doc.page(0).to(Png(format="png", scale=0.5))
    assert isinstance(png, bytes)
    assert png.startswith(b"\x89PNG")
    jpegs = doc.pages()[:2].to(format="jpeg", scale=0.5, quality=85)
    assert len(jpegs) == 2
    assert all(jpeg.startswith(b"\xff\xd8") for jpeg in jpegs)
    assert doc.render(0, scale=0.5) == png


@pytest.mark.parametrize(
    ("args", "kwargs", "message"),
    [
        (({"format": "text"},), {"format": "text"}, "not both"),
        ((), {}, "missing format"),
        (({"images": "omit"},), {}, "format"),
        ((), {"format": "pdf"}, "pdf"),
        ((), {"format": "markdown", "standalone": True}, "standalone"),
        ((), {"format": "png", "scale": 0}, "scale"),
        ((), {"format": "jpeg", "quality": 101}, "quality"),
        (("markdown",), {}, "dict"),
    ],
)
def test_bad_formats_raise(doc: Document, args: tuple[object, ...], kwargs: dict[str, object], message: str) -> None:
    with pytest.raises(DocbossError, match=message):
        doc.page(0).to(*args, **kwargs)


def test_the_whole_document_does_not_render(doc: Document) -> None:
    with pytest.raises(DocbossError, match="page"):
        doc.to(format="png")


@pytest.mark.asyncio
async def test_the_async_chain_matches_the_sync_one(raw: bytes, doc: Document) -> None:
    pages = await (await AsyncDocument.from_bytes(raw)).pages().to(format="markdown", images="omit")
    assert pages == doc.pages().to(format="markdown", images="omit")
    adoc = await AsyncDocument.from_bytes(raw)
    assert await adoc.to(format="text") == doc.to(format="text")
    assert await adoc.pages()[2:6].to(Text(format="text")) == doc.pages()[2:6].to(format="text")
    assert await adoc.pages()[1:][-1].to(format="text") == doc.pages()[-1].to(format="text")
    assert await adoc.page(-1).to(format="html") == doc.page(-1).to(format="html")
    assert await adoc.pages()[::2].count() == len(doc.pages()[::2])
    png = await adoc.page(0).to(format="png", scale=0.5)
    assert png.startswith(b"\x89PNG")


@pytest.mark.asyncio
async def test_async_errors_raise(raw: bytes) -> None:
    adoc = await AsyncDocument.from_bytes(raw)
    with pytest.raises(IndexError):
        await adoc.page(10_000).to(format="text")
    with pytest.raises(DocbossError, match="not both"):
        adoc.pages().to({"format": "text"}, format="text")
    with pytest.raises(ValueError):
        adoc.pages()[::0]


def test_a_page_where_no_block_starts_is_empty() -> None:
    long = " ".join(f"long{i}" for i in range(4000))
    doc = Document(data=docboss.md.to_docx(f"Before.\n\n{long}\n\nAfter.\n"))
    for fmt in ["text", "markdown", "html", "json"]:
        pages = doc.pages().to(format=fmt)
        assert len(pages) >= 3
        assert "long3999" in pages[0]
        assert "" in pages[1:-1]
        assert doc.page(1).to(format=fmt) == ""

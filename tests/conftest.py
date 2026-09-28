from pathlib import Path

import pytest

ROOT = Path(__file__).parent.parent
DOCX = ROOT / "crates" / "docboss-docx" / "tests" / "fixtures"
DOC = ROOT / "crates" / "docboss-doc" / "tests" / "fixtures"


@pytest.fixture
def rich_docx() -> Path:
    return DOCX / "libreoffice-rich.docx"


@pytest.fixture
def lists_doc() -> Path:
    return DOC / "lists.doc"


@pytest.fixture
def image_doc() -> Path:
    return DOC / "image.doc"

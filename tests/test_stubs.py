"""The type stubs must cover every name and method the package exports."""

import inspect
from pathlib import Path

import docboss

STUB = Path(__file__).parent.parent / "python" / "docboss" / "_docboss.pyi"


def test_stub_declares_every_exported_class() -> None:
    stub = STUB.read_text()
    for name in docboss.__all__:
        value = getattr(docboss, name)
        if inspect.isclass(value):
            assert f"class {name}" in stub, f"missing stub for class {name}"
        elif inspect.isbuiltin(value):
            assert f"def {name}(" in stub, f"missing stub for function {name}"


def test_stub_declares_every_public_method() -> None:
    stub = STUB.read_text()
    for name in docboss.__all__:
        value = getattr(docboss, name)
        if not inspect.isclass(value) or issubclass(value, BaseException):
            continue
        for attribute in dir(value):
            if attribute.startswith("_"):
                continue
            assert f"def {attribute}(" in stub or f"    {attribute}: " in stub, f"missing stub for {name}.{attribute}"


def test_md_module_functions_are_stubbed() -> None:
    stub = STUB.read_text()
    assert "def md_to_docx(" in stub
    assert inspect.signature(docboss.md.to_docx).parameters.keys() == {"markdown", "images_dir", "title"}

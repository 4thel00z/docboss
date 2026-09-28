"""Markdown to DOCX."""

import os

from docboss._docboss import md_to_docx

__all__ = ["to_docx"]


def to_docx(
    markdown: str,
    images_dir: str | os.PathLike | None = None,
    title: str | None = None,
) -> bytes:
    """Compose CommonMark + GFM into DOCX bytes. Relative image paths are
    read under ``images_dir``; without it images become their alt text."""
    return md_to_docx(markdown, None if images_dir is None else os.fspath(images_dir), title)

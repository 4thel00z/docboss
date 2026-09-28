#!/usr/bin/env python3
"""Text extraction on one large document, best-of-N per engine.

The corpus benchmarks are dominated by small files, where per-call overhead
decides the ranking. This one builds a single large DOCX deterministically
(headings, paragraphs with bold and italic runs, bullet and numbered lists,
tables) with `docboss.md.to_docx`, and times every engine extracting its text.
LibreOffice converts the same file to .doc for the DOC engines.

Usage:
    python benchmarks/bench_large.py [--sections N] [--repeat K]
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import tempfile
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import common
import engines


def markdown(sections: int) -> str:
    parts = []
    for index in range(sections):
        parts.append(f"# Section {index}\n")
        parts.append(
            f"Paragraph {index} has **bold** and *italic* runs and enough ordinary words to wrap "
            "across a line or two of a page, which is what most documents are made of.\n"
        )
        parts.append("- first bullet\n- second bullet\n  1. nested one\n  2. nested two\n\n")
        parts.append("| Name | Value | Note |\n| --- | --- | --- |\n")
        parts.append("".join(f"| row {row} | {index * row} | cell text |\n" for row in range(4)))
        parts.append("\n")
    return "".join(parts)


def time_engine(fn: engines.Adapter, path: str, repeat: int) -> dict[str, object]:
    try:
        output = str(fn(path))
    except Exception as error:
        return {"error": type(error).__name__}
    best = float("inf")
    for _ in range(repeat):
        start = time.perf_counter()
        fn(path)
        best = min(best, time.perf_counter() - start)
    return {"time_ms": best * 1000, "chars": len(output)}


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--sections", type=int, default=3000)
    parser.add_argument("--repeat", type=int, default=3)
    parser.add_argument("--out", default=None)
    args = parser.parse_args()

    import docboss

    work = tempfile.mkdtemp(prefix="docboss-large-")
    docx_path = os.path.join(work, "large.docx")
    with open(docx_path, "wb") as handle:
        handle.write(docboss.md.to_docx(markdown(args.sections)))
    subprocess.run(
        ["soffice", common.soffice_profile(), "--headless", "--convert-to", "doc", "--outdir", work, docx_path],
        capture_output=True,
        timeout=1800,
    )
    doc_path = os.path.join(work, "large.doc")
    pages = docboss.Document(docx_path).page_count()
    print(f"{args.sections} sections, {pages} pages, {os.path.getsize(docx_path) / 1e6:.1f} MB docx")

    results: dict[str, object] = {
        "sections": args.sections,
        "pages": pages,
        "docx_bytes": os.path.getsize(docx_path),
        "doc_bytes": os.path.getsize(doc_path) if os.path.exists(doc_path) else None,
        "machine": common.machine(),
        "versions": common.versions(),
        "date": time.strftime("%Y-%m-%d"),
        "docx": {},
        "doc": {},
    }
    for kind, path, table in (
        ("docx", docx_path, engines.DOCX_ENGINES),
        ("doc", doc_path, engines.DOC_ENGINES),
    ):
        if not os.path.exists(path):
            continue
        for name, ops in engines.available(table).items():
            repeat = 1 if name == "pandoc" else args.repeat
            row = time_engine(ops["text"], path, repeat)
            results[kind][name] = row
            print(f"    {kind:4} {name:12} {row}")
    here = os.path.dirname(os.path.abspath(__file__))
    out = args.out or os.path.join(here, "results-large.json")
    with open(out, "w") as handle:
        json.dump(results, handle, indent=2)
    print(f"wrote {out}")


if __name__ == "__main__":
    main()

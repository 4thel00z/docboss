#!/usr/bin/env python3
"""Page rendering speed of docboss on a fixed corpus sample.

Takes a deterministic, evenly spaced sample of DOCX and DOC files, and for
each run opens every file, lays it out and renders its first pages to PNG in
one process through the Python bindings, one page after another, writing
each image into a temporary directory that is deleted after the run. Reports
pages per second and the total time, best of `--repeat` runs after one
warm-up pass. Files that fail to open or render in the warm-up are left out
of the timed runs and counted.

Usage:
    python benchmarks/bench_render.py CORPUS_DIR... [--docx 60] [--doc 30]
        [--max-pages 3] [--scale 1.5] [--repeat 5]
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import sys
import tempfile
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import common


def render_file(path: str, max_pages: int, scale: float, out_dir: str, index: int) -> int:
    """Renders the first `max_pages` pages of `path` into `out_dir`; returns the page count."""
    import docboss

    document = docboss.Document(path)
    pages = min(document.page_count(), max_pages)
    for page in range(pages):
        with open(os.path.join(out_dir, f"{index:04d}-{page + 1:03d}.png"), "wb") as handle:
            handle.write(document.render(page, scale=scale))
    return pages


def renderable(files: list[str], max_pages: int, scale: float) -> tuple[list[str], list[str]]:
    """The files that render in a warm-up pass, and the ones that fail."""
    good, failed = [], []
    work = tempfile.mkdtemp(prefix="docboss-render-")
    try:
        for index, path in enumerate(files):
            try:
                render_file(path, max_pages, scale, work, index)
            except Exception:
                failed.append(path)
                continue
            good.append(path)
    finally:
        shutil.rmtree(work, ignore_errors=True)
    return good, failed


def timed_run(files: list[str], max_pages: int, scale: float) -> tuple[float, int]:
    work = tempfile.mkdtemp(prefix="docboss-render-")
    try:
        start = time.perf_counter()
        pages = sum(render_file(path, max_pages, scale, work, index) for index, path in enumerate(files))
        return time.perf_counter() - start, pages
    finally:
        shutil.rmtree(work, ignore_errors=True)


def bench_kind(kind: str, files: list[str], args: argparse.Namespace) -> dict[str, object]:
    good, failed = renderable(files, args.max_pages, args.scale)
    runs = [timed_run(good, args.max_pages, args.scale) for _ in range(args.repeat)]
    best, pages = min(runs)
    row = {
        "files": len(good),
        "failed": [os.path.basename(path) for path in failed],
        "pages": pages,
        "time_ms": best * 1000,
        "pages_per_s": pages / best if best else 0.0,
        "runs_ms": [run * 1000 for run, _ in runs],
    }
    print(
        f"    {kind:4} {row['files']} files, {pages} pages: {row['time_ms']:.0f} ms, "
        f"{row['pages_per_s']:.1f} pages/s (failed {len(failed)})"
    )
    return row


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("corpus", nargs="+", help="directories searched recursively")
    parser.add_argument("--docx", type=int, default=60, help="DOCX files in the sample")
    parser.add_argument("--doc", type=int, default=30, help="DOC files in the sample")
    parser.add_argument("--max-pages", type=int, default=3)
    parser.add_argument("--scale", type=float, default=1.5)
    parser.add_argument("--repeat", type=int, default=5)
    parser.add_argument("--out", default=None)
    args = parser.parse_args()

    results: dict[str, object] = {
        "corpus": [os.path.basename(os.path.normpath(d)) for d in args.corpus],
        "max_pages": args.max_pages,
        "scale": args.scale,
        "repeat": args.repeat,
        "machine": common.machine(),
        "versions": common.versions(),
        "date": time.strftime("%Y-%m-%d"),
    }
    for kind, count in (("docx", args.docx), ("doc", args.doc)):
        files = common.sample(common.collect(args.corpus, kind), count)
        if not files:
            continue
        results[kind] = bench_kind(kind, files, args)
    here = os.path.dirname(os.path.abspath(__file__))
    out = args.out or os.path.join(here, "results-render.json")
    with open(out, "w") as handle:
        json.dump(results, handle, indent=2)
    print(f"wrote {out}")


if __name__ == "__main__":
    main()

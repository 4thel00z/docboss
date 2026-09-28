#!/usr/bin/env python3
"""Benchmark docboss against other DOCX and DOC readers.

Times open + parse, text extraction, Markdown and HTML conversion over a
deterministic sample of a corpus, best-of-N after one warm-up pass, and
keeps only the files every engine of an operation handled so each total
compares the same workload. Speed alone rewards an engine that skips
content, so every text result is also scored for word recall against
LibreOffice's text export of the same file.

LibreOffice itself is timed separately (`--libreoffice`): per-file
`soffice --headless --convert-to txt` pays process startup every time, so it
is also timed as one batch conversion of the whole sample, which is the
fastest way to drive it.

Page counts, for pages/s, come from docboss's own layout.

Usage:
    python benchmarks/bench.py docx CORPUS_DIR... [--sample N] [--repeat K]
    python benchmarks/bench.py doc CORPUS_DIR... [--sample N] [--repeat K]
"""

from __future__ import annotations

import argparse
import json
import os
import statistics
import sys
import tempfile
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import common
import engines

OPERATIONS = ("open", "text", "markdown", "html")


def time_one(fn: engines.Adapter, path: str, repeat: int) -> tuple[float, object] | None:
    """Best-of-`repeat` wall time and the last output, or None if it raised."""
    best = None
    output: object = None
    for _ in range(repeat):
        start = time.perf_counter()
        try:
            output = fn(path)
        except Exception:
            return None
        elapsed = time.perf_counter() - start
        if best is None or elapsed < best:
            best = elapsed
    return best, output


def page_counts(files: list[str]) -> dict[str, int]:
    import docboss

    counts = {}
    for path in files:
        try:
            counts[path] = docboss.Document(path).page_count()
        except Exception:
            counts[path] = 0
    return counts


def summarize(
    name: str,
    timings: dict[str, tuple[float, object]],
    common_files: set[str],
    pages: dict[str, int],
    refs: dict[str, object],
    with_recall: bool,
) -> dict[str, object]:
    chosen = sorted(common_files)
    total = sum(timings[path][0] for path in chosen)
    page_total = sum(pages[path] for path in chosen)
    size = sum(os.path.getsize(path) for path in chosen)
    per_file = [timings[path][0] * 1000 for path in chosen]
    row: dict[str, object] = {
        "time_s": total,
        "files": len(chosen),
        "pages": page_total,
        "pages_per_s": page_total / total if total else 0.0,
        "files_per_s": len(chosen) / total if total else 0.0,
        "mb_per_s": size / 1e6 / total if total else 0.0,
        "median_ms": statistics.median(per_file) if per_file else 0.0,
        "handled": len(timings),
    }
    if not with_recall:
        return row
    pairs = [common.matched(str(timings[path][1]), refs[path]) for path in chosen if path in refs]
    pairs = [(hits, total) for hits, total in pairs if total]
    row["word_recall"] = statistics.fmean(hits / total for hits, total in pairs) if pairs else None
    words_total = sum(total for _, total in pairs)
    row["word_recall_weighted"] = sum(hits for hits, _ in pairs) / words_total if words_total else None
    row["recall_files"] = len(pairs)
    return row


def run_operation(
    op: str,
    libs: dict[str, engines.Adapter],
    files: list[str],
    repeat: int,
    pages: dict[str, int],
    refs: dict[str, object],
) -> dict[str, object]:
    for fn in libs.values():
        for path in files:
            try:
                fn(path)
            except Exception:
                pass
    timings: dict[str, dict[str, tuple[float, object]]] = {name: {} for name in libs}
    for path in files:
        for name, fn in libs.items():
            result = time_one(fn, path, repeat)
            if result is None:
                continue
            timings[name][path] = result
    shared = set(files)
    for name in libs:
        shared &= set(timings[name])
    with_recall = op == "text"
    rows = {name: summarize(name, timings[name], shared, pages, refs, with_recall) for name in libs}
    print(f"[{op}] {len(shared)} files handled by all {len(libs)} engines")
    for name, row in sorted(rows.items(), key=lambda kv: kv[1]["time_s"] or 1e9):
        recall = row.get("word_recall")
        recall_text = f"  recall {recall:.3f}" if isinstance(recall, float) else ""
        print(
            f"    {name:12} {row['time_s']:8.3f}s {row['pages_per_s']:10.1f} pages/s "
            f"{row['files_per_s']:9.1f} files/s  median {row['median_ms']:7.2f} ms"
            f"  handled {row['handled']}/{len(files)}{recall_text}"
        )
    return {"files_compared": len(shared), "libraries": rows}


def own_recall(fn: engines.Adapter, files: list[str], refs: dict[str, object]) -> dict[str, object]:
    """Recall over every file the engine handled, not only the shared set."""
    pairs = []
    failures = 0
    for path in files:
        if path not in refs:
            continue
        try:
            text = str(fn(path))
        except Exception:
            failures += 1
            continue
        hits, total = common.matched(text, refs[path])
        if total:
            pairs.append((hits, total))
    words_total = sum(total for _, total in pairs)
    return {
        "word_recall": statistics.fmean(hits / total for hits, total in pairs) if pairs else None,
        "word_recall_weighted": sum(hits for hits, _ in pairs) / words_total if words_total else None,
        "files": len(pairs),
        "failures": failures,
    }


def libreoffice_timings(files: list[str], per_file: int) -> dict[str, object]:
    with tempfile.TemporaryDirectory() as out:
        common.libreoffice_batch(files[:1], out)
    with tempfile.TemporaryDirectory() as out:
        batch = common.libreoffice_batch(files, out)
    singles = []
    for path in files[:per_file]:
        with tempfile.TemporaryDirectory() as out:
            singles.append(common.libreoffice_batch([path], out))
    return {
        "batch_files": len(files),
        "batch_time_s": batch,
        "batch_files_per_s": len(files) / batch if batch else 0.0,
        "per_file_samples": len(singles),
        "per_file_median_ms": statistics.median(singles) * 1000 if singles else None,
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("kind", choices=("docx", "doc"))
    parser.add_argument("corpus", nargs="+", help="directories searched recursively")
    parser.add_argument("--sample", type=int, default=60)
    parser.add_argument("--repeat", type=int, default=3)
    parser.add_argument("--libreoffice", action="store_true", help="also time LibreOffice itself")
    parser.add_argument("--reference-cache", default=os.path.join(tempfile.gettempdir(), "docboss-bench-refs"))
    parser.add_argument("--out", default=None)
    args = parser.parse_args()

    files = common.sample(common.collect(args.corpus, args.kind), args.sample)
    if not files:
        raise SystemExit(f"no .{args.kind} files found")
    table = engines.available(engines.DOCX_ENGINES if args.kind == "docx" else engines.DOC_ENGINES)
    print(f"{len(files)} .{args.kind} files; engines: {', '.join(table)}")

    texts = common.references(files, os.path.join(args.reference_cache, args.kind))
    refs = {path: common.words(text) for path, text in texts.items()}
    pages = page_counts(files)

    results: dict[str, object] = {
        "kind": args.kind,
        "corpus": [os.path.basename(os.path.normpath(d)) for d in args.corpus],
        "files": len(files),
        "pages": sum(pages.values()),
        "bytes": sum(os.path.getsize(path) for path in files),
        "repeat": args.repeat,
        "machine": common.machine(),
        "versions": common.versions(),
        "date": time.strftime("%Y-%m-%d"),
        "operations": {},
        "recall_all_files": {},
    }
    for op in OPERATIONS:
        libs = {name: ops[op] for name, ops in table.items() if op in ops}
        if len(libs) < 2:
            continue
        results["operations"][op] = run_operation(op, libs, files, args.repeat, pages, refs)
    results["recall_all_files"] = {
        name: own_recall(ops["text"], files, refs) for name, ops in table.items() if "text" in ops
    }
    print("[recall over every file each engine handled]")
    for name, row in results["recall_all_files"].items():
        print(
            f"    {name:12} mean {row['word_recall']:.3f}  weighted {row['word_recall_weighted']:.3f}"
            f"  files {row['files']}  failures {row['failures']}"
        )
    if args.libreoffice:
        results["libreoffice"] = libreoffice_timings(files, per_file=10)
        print(f"[libreoffice] {results['libreoffice']}")

    here = os.path.dirname(os.path.abspath(__file__))
    out = args.out or os.path.join(here, f"results-{args.kind}.json")
    with open(out, "w") as handle:
        json.dump(results, handle, indent=2)
    print(f"wrote {out}")


if __name__ == "__main__":
    main()

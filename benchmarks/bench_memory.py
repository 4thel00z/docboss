#!/usr/bin/env python3
"""Peak memory of text extraction, one fresh process per (engine, workload).

A peak RSS is meaningless once several engines have allocated in the same
address space, so each measurement runs this script again in worker mode
(through isolation.py). The worker imports the engine, records its resident
high-water mark, extracts the text of every file in the workload, and
records the high-water mark again. Engines that are external programs are
measured by the largest child process's high-water mark instead.

Two workloads per format: the corpus sample, and the single largest file in
the whole corpus, since a reader that materializes whole documents shows up on the large
file first.

Usage:
    python benchmarks/bench_memory.py docx CORPUS_DIR... [--sample N]
"""

from __future__ import annotations

import argparse
import json
import os
import resource
import sys
import tempfile
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import common
import engines
import isolation

SUBPROCESS_ENGINES = {"antiword", "catdoc", "pandoc"}
MODULES = {
    "docboss": "docboss",
    "python-docx": "docx",
    "docx2python": "docx2python",
    "mammoth": "mammoth",
    "docx2txt": "docx2txt",
    "pandoc": "pypandoc",
}


def max_rss(who: int) -> int:
    peak = resource.getrusage(who).ru_maxrss
    return peak if sys.platform == "darwin" else peak * 1024


def worker(engine: str, listing: str) -> None:
    with open(listing) as handle:
        files = [line.strip() for line in handle if line.strip()]
    isolation.stage("import")
    if engine in MODULES:
        __import__(MODULES[engine])
    baseline = max_rss(resource.RUSAGE_SELF)
    table = {**engines.DOCX_ENGINES, **engines.DOC_ENGINES}
    fn = table[engine]["text"]
    isolation.stage("extract")
    handled = 0
    for path in files:
        try:
            fn(path)
        except Exception:
            continue
        handled += 1
    isolation.finish(
        {
            "baseline": baseline,
            "peak": max_rss(resource.RUSAGE_SELF),
            "child_peak": max_rss(resource.RUSAGE_CHILDREN),
            "handled": handled,
            "files": len(files),
        }
    )


def measure(engine: str, files: list[str], timeout: float) -> dict[str, object]:
    with tempfile.NamedTemporaryFile("w", suffix=".txt", delete=False) as listing:
        listing.write("\n".join(files))
    try:
        record = isolation.run_worker(os.path.abspath(__file__), [engine, listing.name], timeout)
    finally:
        os.unlink(listing.name)
    if record["outcome"] != "ok":
        return {"outcome": record["outcome"], "detail": record["detail"]}
    payload = record["payload"]
    row: dict[str, object] = {"outcome": "ok", "handled": payload["handled"], "files": payload["files"]}
    row["baseline_mb"] = payload["baseline"] / 1e6
    row["peak_mb"] = payload["peak"] / 1e6
    row["delta_mb"] = (payload["peak"] - payload["baseline"]) / 1e6
    if engine in SUBPROCESS_ENGINES:
        row["child_peak_mb"] = payload["child_peak"] / 1e6
    return row


def main() -> None:
    if len(sys.argv) > 1 and sys.argv[1] == "worker":
        worker(sys.argv[2], sys.argv[3])
        return
    parser = argparse.ArgumentParser()
    parser.add_argument("kind", choices=("docx", "doc"))
    parser.add_argument("corpus", nargs="+")
    parser.add_argument("--sample", type=int, default=100)
    parser.add_argument("--timeout", type=float, default=900.0)
    parser.add_argument("--out", default=None)
    args = parser.parse_args()

    everything = common.collect(args.corpus, args.kind)
    files = common.sample(everything, args.sample)
    largest = max(everything, key=os.path.getsize)
    workloads = {"sample": files, "largest": [largest]}
    table = engines.available(engines.DOCX_ENGINES if args.kind == "docx" else engines.DOC_ENGINES)
    print(f"{len(files)} .{args.kind} files; largest {os.path.basename(largest)} {os.path.getsize(largest) / 1e6:.1f} MB")
    rows: dict[str, dict[str, object]] = {}
    for engine in table:
        rows[engine] = {}
        for workload, chosen in workloads.items():
            row = measure(engine, chosen, args.timeout)
            rows[engine][workload] = row
            if row["outcome"] != "ok":
                print(f"    {engine:12} {workload:8} {row['outcome']} {row['detail']}")
                continue
            child = f"  child peak {row['child_peak_mb']:7.1f} MB" if "child_peak_mb" in row else ""
            print(
                f"    {engine:12} {workload:8} peak {row['peak_mb']:7.1f} MB  over import {row['delta_mb']:7.1f} MB"
                f"{child}  handled {row['handled']}/{row['files']}"
            )
    results = {
        "kind": args.kind,
        "files": len(files),
        "largest": {"name": os.path.basename(largest), "bytes": os.path.getsize(largest)},
        "machine": common.machine(),
        "versions": common.versions(),
        "date": time.strftime("%Y-%m-%d"),
        "engines": rows,
    }
    here = os.path.dirname(os.path.abspath(__file__))
    out = args.out or os.path.join(here, f"results-memory-{args.kind}.json")
    with open(out, "w") as handle:
        json.dump(results, handle, indent=2)
    print(f"wrote {out}")


if __name__ == "__main__":
    main()

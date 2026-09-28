#!/usr/bin/env python3
"""Parallel text extraction throughput: docboss against each competitor's best parallel route.

`bench.py` times one file at a time, which understates every engine that
can use more than one core. Here each engine extracts the text of the whole
sample twice: sequentially, and through its best parallel route. For
docboss that is `docboss.extract_texts`, which runs on a pool of Rust
threads, and a Python thread pool (docboss releases the GIL). Pure-Python
engines hold the GIL, so their route is a process pool; engines that are
external programs run as concurrent subprocesses from a thread pool.

Usage:
    python benchmarks/bench_parallel.py docx CORPUS_DIR... [--sample N] [--workers W]
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import time
from collections.abc import Callable
from concurrent.futures import ProcessPoolExecutor, ThreadPoolExecutor

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import common
import engines

SUBPROCESS_ENGINES = {"antiword", "catdoc", "pandoc"}


def safe(fn: engines.Adapter, path: str) -> bool:
    try:
        fn(path)
    except Exception:
        return False
    return True


def run_docboss_batch(files: list[str], workers: int) -> int:
    import docboss

    return sum(text is not None for text in docboss.extract_texts(files, threads=workers, strict=False))


def timed(fn: Callable[[], object], repeat: int) -> float:
    best = float("inf")
    for _ in range(repeat):
        start = time.perf_counter()
        fn()
        best = min(best, time.perf_counter() - start)
    return best


def sequential(fn: engines.Adapter, files: list[str]) -> int:
    return sum(safe(fn, path) for path in files)


def threaded(fn: engines.Adapter, files: list[str], workers: int) -> int:
    with ThreadPoolExecutor(workers) as pool:
        return sum(pool.map(lambda path: safe(fn, path), files))


def processes(name: str, files: list[str], workers: int) -> int:
    with ProcessPoolExecutor(workers) as pool:
        return sum(pool.map(worker_extract, [name] * len(files), files, chunksize=4))


def worker_extract(name: str, path: str) -> bool:
    table = {**engines.DOCX_ENGINES, **engines.DOC_ENGINES}
    return safe(table[name]["text"], path)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("kind", choices=("docx", "doc"))
    parser.add_argument("corpus", nargs="+")
    parser.add_argument("--sample", type=int, default=200)
    parser.add_argument("--workers", type=int, default=os.cpu_count() or 4)
    parser.add_argument("--repeat", type=int, default=3)
    parser.add_argument("--out", default=None)
    args = parser.parse_args()

    files = common.sample(common.collect(args.corpus, args.kind), args.sample)
    table = engines.available(engines.DOCX_ENGINES if args.kind == "docx" else engines.DOC_ENGINES)
    size = sum(os.path.getsize(path) for path in files)
    print(f"{len(files)} .{args.kind} files, {size / 1e6:.1f} MB, {args.workers} workers")

    rows = {}
    for name, ops in table.items():
        fn = ops["text"]
        sequential(fn, files[:5])
        repeat = 1 if name == "pandoc" else args.repeat
        base = timed(lambda: sequential(fn, files), repeat)
        routes: dict[str, float] = {}
        if name == "docboss":
            routes["extract_texts"] = timed(lambda: run_docboss_batch(files, args.workers), repeat)
            routes["threads"] = timed(lambda: threaded(fn, files, args.workers), repeat)
        elif name in SUBPROCESS_ENGINES:
            routes["threads"] = timed(lambda: threaded(fn, files, args.workers), repeat)
        else:
            routes["processes"] = timed(lambda: processes(name, files, args.workers), repeat)
        best_route, best = min(routes.items(), key=lambda kv: kv[1])
        rows[name] = {
            "sequential_s": base,
            "routes_s": routes,
            "best_route": best_route,
            "best_s": best,
            "speedup": base / best if best else 0.0,
            "files_per_s": len(files) / best if best else 0.0,
            "mb_per_s": size / 1e6 / best if best else 0.0,
        }
        print(
            f"    {name:12} sequential {base:8.3f}s  best {best_route:13} {best:8.3f}s"
            f"  x{base / best:5.1f}  {len(files) / best:9.1f} files/s"
        )

    results = {
        "kind": args.kind,
        "files": len(files),
        "bytes": size,
        "workers": args.workers,
        "machine": common.machine(),
        "versions": common.versions(),
        "date": time.strftime("%Y-%m-%d"),
        "engines": rows,
    }
    here = os.path.dirname(os.path.abspath(__file__))
    out = args.out or os.path.join(here, f"results-parallel-{args.kind}.json")
    with open(out, "w") as handle:
        json.dump(results, handle, indent=2)
    print(f"wrote {out}")


if __name__ == "__main__":
    main()

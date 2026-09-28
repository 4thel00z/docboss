#!/usr/bin/env python3
"""How docboss and other DOCX and DOC readers survive malformed files.

The other benchmarks keep only files the engines handle. This one feeds them
damaged copies of real documents and measures survival: does the engine
return text, raise a clean Python exception, or take the process down.

`mutate` writes the damaged corpus deterministically from seed files:

- byte flips (1 to 64 random bytes overwritten) and truncation, for both
  formats;
- for DOCX, packages rebuilt with a damaged `word/document.xml` (a span
  deleted, closing tags removed, a span duplicated, bytes of the XML
  flipped) and packages whose central directory is zeroed;
- for DOC, a 512-byte sector overwritten with random bytes or zeroed.

Every (file, engine) pair then runs in a fresh interpreter (this script in
worker mode, through isolation.py) under a timeout, and the parent
classifies how the worker came back:

- ok      — the engine returned text
- error   — the engine raised a clean exception
- crash   — the process died (signal, nonzero exit, no result), or an
            external engine it ran died on a signal
- timeout — still running after --timeout seconds; SIGKILLed

docboss also renders the first page in a second stage, so a layout or
raster crash would show up as well.

Usage:
    python benchmarks/bench_robustness.py mutate docx OUT_DIR SEED_DIR... [--count N]
    python benchmarks/bench_robustness.py run docx OUT_DIR [--timeout S] [--jobs J]
"""

from __future__ import annotations

import argparse
import io
import json
import os
import random
import re
import sys
import time
import zipfile
from concurrent.futures import ThreadPoolExecutor

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import common
import engines
import isolation

OUTCOMES = ("ok", "error", "crash", "timeout")


def flip(data: bytes, rng: random.Random) -> bytes:
    out = bytearray(data)
    for _ in range(rng.randint(1, 64)):
        out[rng.randrange(len(out))] = rng.randrange(256)
    return bytes(out)


def truncate(data: bytes, rng: random.Random) -> bytes:
    return data[: rng.randrange(1, len(data))]


def damage_xml(xml: bytes, rng: random.Random) -> bytes:
    choice = rng.randrange(4)
    start = rng.randrange(len(xml))
    end = min(len(xml), start + rng.randint(1, 4096))
    if choice == 0:
        return xml[:start] + xml[end:]
    if choice == 1:
        return re.sub(rb"</w:[a-zA-Z]+>", b"", xml, count=rng.randint(1, 200))
    if choice == 2:
        return xml[:end] + xml[start:end] + xml[end:]
    return flip(xml, rng)


def rezip(data: bytes, rng: random.Random) -> bytes:
    source = zipfile.ZipFile(io.BytesIO(data))
    out = io.BytesIO()
    with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as target:
        for info in source.infolist():
            payload = source.read(info)
            if info.filename == "word/document.xml":
                payload = damage_xml(payload, rng)
            target.writestr(info.filename, payload)
    return out.getvalue()


def zero_directory(data: bytes, rng: random.Random) -> bytes:
    at = data.rfind(b"PK\x01\x02")
    if at < 0:
        return flip(data, rng)
    out = bytearray(data)
    out[at:] = bytes(len(out) - at)
    return bytes(out)


def sector(data: bytes, rng: random.Random) -> bytes:
    out = bytearray(data)
    sectors = max(1, len(out) // 512)
    at = rng.randrange(sectors) * 512
    fill = bytes(512) if rng.random() < 0.5 else rng.randbytes(512)
    out[at : at + 512] = fill[: len(out) - at]
    return bytes(out)


def mutations(kind: str) -> list:
    if kind == "docx":
        return [flip, truncate, rezip, rezip, zero_directory]
    return [flip, truncate, sector, sector]


def mutate(kind: str, out_dir: str, seeds: list[str], count: int) -> None:
    files = [path for path in common.collect(seeds, kind) if os.path.getsize(path) < 4 << 20]
    rng = random.Random(20260928)
    os.makedirs(out_dir, exist_ok=True)
    written = 0
    for index in range(count):
        seed = files[rng.randrange(len(files))]
        with open(seed, "rb") as handle:
            data = handle.read()
        if not data:
            continue
        fn = rng.choice(mutations(kind))
        try:
            damaged = fn(data, rng)
        except (zipfile.BadZipFile, ValueError, KeyError, OSError, EOFError):
            damaged = flip(data, rng)
        name = f"{index:04d}-{fn.__name__}-{os.path.basename(seed)}"
        with open(os.path.join(out_dir, name), "wb") as handle:
            handle.write(damaged)
        written += 1
    print(f"wrote {written} damaged .{kind} files to {out_dir}")


def worker(engine: str, path: str) -> None:
    table = {**engines.DOCX_ENGINES, **engines.DOC_ENGINES}
    isolation.stage("text")
    try:
        table[engine]["text"](path)
    except engines.ToolCrashed as crashed:
        isolation.finish({"result": "crash", "detail": str(crashed)})
        return
    except BaseException as error:
        isolation.finish({"result": "error", "detail": type(error).__name__})
        return
    if engine != "docboss":
        isolation.finish({"result": "ok"})
        return
    isolation.stage("render")
    import docboss

    try:
        document = docboss.Document(path)
        if document.page_count():
            document.render(0, scale=0.5)
    except BaseException as error:
        isolation.finish({"result": "error", "detail": f"render: {type(error).__name__}"})
        return
    isolation.finish({"result": "ok"})


def classify(record: dict) -> tuple[str, str]:
    if record["outcome"] != "ok":
        return record["outcome"], f"{record['stage']}: {record['detail']}"
    payload = record["payload"] or {}
    return payload.get("result", "crash"), payload.get("detail", "")


def run(kind: str, directory: str, timeout: float, jobs: int, out: str | None) -> None:
    files = sorted(os.path.join(directory, name) for name in os.listdir(directory))
    table = engines.available(engines.DOCX_ENGINES if kind == "docx" else engines.DOC_ENGINES)
    script = os.path.abspath(__file__)
    pairs = [(engine, path) for path in files for engine in table]
    start = time.perf_counter()

    def one(pair: tuple[str, str]) -> tuple[str, str, str, str]:
        engine, path = pair
        outcome, detail = classify(isolation.run_worker(script, [engine, path], timeout))
        return engine, path, outcome, detail

    with ThreadPoolExecutor(jobs) as pool:
        records = list(pool.map(one, pairs))
    counts = {engine: dict.fromkeys(OUTCOMES, 0) for engine in table}
    failures: dict[str, list[dict[str, str]]] = {engine: [] for engine in table}
    for engine, path, outcome, detail in records:
        counts[engine][outcome] += 1
        if outcome in ("crash", "timeout"):
            failures[engine].append({"file": os.path.basename(path), "outcome": outcome, "detail": detail})
    print(f"{len(files)} damaged .{kind} files, {len(table)} engines, {time.perf_counter() - start:.0f}s")
    for engine, row in counts.items():
        survived = row["ok"] + row["error"]
        print(
            f"    {engine:12} ok {row['ok']:4}  error {row['error']:4}  crash {row['crash']:4}"
            f"  timeout {row['timeout']:4}  survival {survived / len(files):.1%}"
        )
    results = {
        "kind": kind,
        "files": len(files),
        "timeout_s": timeout,
        "machine": common.machine(),
        "versions": common.versions(),
        "date": time.strftime("%Y-%m-%d"),
        "engines": counts,
        "failures": failures,
    }
    here = os.path.dirname(os.path.abspath(__file__))
    path = out or os.path.join(here, f"results-robustness-{kind}.json")
    with open(path, "w") as handle:
        json.dump(results, handle, indent=2)
    print(f"wrote {path}")


def main() -> None:
    if len(sys.argv) > 1 and sys.argv[1] == "worker":
        worker(sys.argv[2], sys.argv[3])
        return
    parser = argparse.ArgumentParser()
    commands = parser.add_subparsers(dest="command", required=True)
    make = commands.add_parser("mutate")
    make.add_argument("kind", choices=("docx", "doc"))
    make.add_argument("out")
    make.add_argument("seeds", nargs="+")
    make.add_argument("--count", type=int, default=150)
    go = commands.add_parser("run")
    go.add_argument("kind", choices=("docx", "doc"))
    go.add_argument("directory")
    go.add_argument("--timeout", type=float, default=30.0)
    go.add_argument("--jobs", type=int, default=os.cpu_count() or 4)
    go.add_argument("--out", default=None)
    args = parser.parse_args()
    if args.command == "mutate":
        mutate(args.kind, args.out, args.seeds, args.count)
        return
    run(args.kind, args.directory, args.timeout, args.jobs, args.out)


if __name__ == "__main__":
    main()

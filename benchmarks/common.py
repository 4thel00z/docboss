"""Corpus sampling, LibreOffice references and word recall for the benchmarks."""

from __future__ import annotations

import glob
import os
import platform
import re
import subprocess
import tempfile
import time
from collections import Counter

WORD = re.compile(r"\w+", re.UNICODE)


def collect(directories: list[str], extension: str) -> list[str]:
    """Every file with `extension` under the directories, sorted, deduplicated by content size and name."""
    seen: set[tuple[str, int]] = set()
    files = []
    for directory in directories:
        for path in sorted(glob.glob(os.path.join(directory, "**", f"*.{extension}"), recursive=True)):
            key = (os.path.basename(path), os.path.getsize(path))
            if key in seen:
                continue
            seen.add(key)
            files.append(path)
    return sorted(files, key=lambda p: (os.path.basename(p), p))


def sample(files: list[str], n: int) -> list[str]:
    """`n` files evenly spaced across the sorted list, for a deterministic spread."""
    if n <= 0 or n >= len(files):
        return files
    step = len(files) / n
    return [files[int(i * step)] for i in range(n)]


def soffice_profile() -> str:
    return "-env:UserInstallation=file://" + os.path.join(tempfile.gettempdir(), "docboss-bench-lo")


def libreoffice_batch(files: list[str], outdir: str, target: str = "txt:Text (encoded):UTF8") -> float:
    """Converts every file in one `soffice --headless` process; returns the wall time."""
    os.makedirs(outdir, exist_ok=True)
    start = time.perf_counter()
    subprocess.run(
        ["soffice", soffice_profile(), "--headless", "--convert-to", target, "--outdir", outdir, *files],
        capture_output=True,
        timeout=3600,
    )
    return time.perf_counter() - start


def reference_name(path: str, index: int) -> str:
    return f"{index:04d}-{os.path.splitext(os.path.basename(path))[0]}"


def references(files: list[str], cache: str) -> dict[str, str]:
    """LibreOffice's text export of each file, converted once and cached by position and name."""
    os.makedirs(cache, exist_ok=True)
    staged = os.path.join(cache, "staged")
    os.makedirs(staged, exist_ok=True)
    missing = []
    for index, path in enumerate(files):
        name = reference_name(path, index)
        if os.path.exists(os.path.join(cache, name + ".txt")):
            continue
        link = os.path.join(staged, name + os.path.splitext(path)[1])
        if not os.path.exists(link):
            os.symlink(os.path.abspath(path), link)
        missing.append(link)
    if missing:
        libreoffice_batch(missing, cache)
    out = {}
    for index, path in enumerate(files):
        txt = os.path.join(cache, reference_name(path, index) + ".txt")
        if not os.path.exists(txt):
            continue
        with open(txt, encoding="utf-8", errors="replace") as handle:
            out[path] = handle.read()
    return out


def words(text: str) -> Counter[str]:
    return Counter(match.lower() for match in WORD.findall(text))


def matched(candidate: str, reference: Counter[str]) -> tuple[int, int]:
    """Reference words (with multiplicity) found in the candidate, and the reference's word count."""
    found = words(candidate)
    return sum(min(count, found[word]) for word, count in reference.items()), sum(reference.values())


def recall(candidate: str, reference: Counter[str]) -> float | None:
    """Share of the reference's words (with multiplicity) found in the candidate."""
    hits, total = matched(candidate, reference)
    if not total:
        return None
    return hits / total


def machine() -> dict[str, str]:
    info = {"platform": platform.platform(), "python": platform.python_version(), "cpu": platform.processor()}
    try:
        info["cpu"] = subprocess.run(
            ["sysctl", "-n", "machdep.cpu.brand_string"], capture_output=True, text=True
        ).stdout.strip() or info["cpu"]
    except OSError:
        pass
    info["cores"] = str(os.cpu_count())
    return info


def versions() -> dict[str, str]:
    from importlib.metadata import PackageNotFoundError, version

    out = {}
    for package in ("docboss", "python-docx", "docx2python", "mammoth", "docx2txt", "pypandoc_binary", "psutil"):
        try:
            out[package] = version(package)
        except PackageNotFoundError:
            continue
    try:
        import pypandoc

        out["pandoc"] = pypandoc.get_pandoc_version()
    except Exception:
        pass
    try:
        lo = subprocess.run(["soffice", "--version"], capture_output=True, text=True, timeout=60).stdout
        out["libreoffice"] = lo.split()[1] if lo else ""
    except (OSError, IndexError, subprocess.TimeoutExpired):
        pass
    return out

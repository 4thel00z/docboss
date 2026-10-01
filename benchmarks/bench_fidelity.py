#!/usr/bin/env python3
"""Score docboss's page rendering against LibreOffice.

For every .docx and .doc file in a directory, LibreOffice converts the
document to PDF (`soffice --headless --convert-to pdf`) and pdfboss
rasterizes that PDF: the reference. docboss renders the same pages from the
document itself. Each pair is aligned (grayscale, center-cropped to the
common size, Lanczos-downsampled to half resolution, as pdfboss's own
fidelity bench does) and scored with windowed SSIM and mean absolute pixel
difference. Page counts are compared too, since a layout that paginates
differently scores every later page against the wrong reference: docboss's
full count comes from `docboss info`, LibreOffice's from the PDF, and a row
that errored counts as a mismatch. The summary gives, for all files and per
format, the mean, median and 10th percentile SSIM (the order statistic:
numpy's `lower` method), with the machine, tool versions and date.

docboss is invoked through a command template so the script works with the
CLI (`docboss render`) or any other entry point:

    python benchmarks/bench_fidelity.py corpus/ --scale 1.5
    python benchmarks/bench_fidelity.py corpus/ --files benchmarks/fidelity-sample.txt
    python benchmarks/bench_fidelity.py corpus/ \
        --docboss 'docboss render {input} --page {page} -o {output} --scale {scale}'

`compare` scores two directories of same-named PNGs against each other:

    python benchmarks/bench_fidelity.py compare reference/ candidate/
"""

from __future__ import annotations

import argparse
import glob
import json
import os
import shlex
import shutil
import subprocess
import sys
import tempfile
import time

import numpy as np
from PIL import Image

import common

WINDOW = 8
K1, K2, LEVELS = 0.01, 0.03, 255.0
DEFAULT_DOCBOSS = "docboss render {input} --page {page} -o {output} --scale {scale}"
DEFAULT_PDFBOSS = "pdfboss render {input} --page {page} -o {output} --scale {scale}"
DEFAULT_DOCBOSS_INFO = "docboss info {input}"


def grayscale(path: str) -> Image.Image:
    image = Image.open(path)
    if "A" not in image.getbands():
        return image.convert("L")
    white = Image.new("RGBA", image.size, (255, 255, 255, 255))
    return Image.alpha_composite(white, image.convert("RGBA")).convert("L")


def align(a: Image.Image, b: Image.Image) -> tuple[np.ndarray, np.ndarray]:
    width, height = min(a.width, b.width), min(a.height, b.height)
    if width < 2 * WINDOW or height < 2 * WINDOW:
        raise ValueError(f"page too small to score: {width}x{height}")

    def crop(image: Image.Image) -> np.ndarray:
        left, top = (image.width - width) // 2, (image.height - height) // 2
        cropped = image.crop((left, top, left + width, top + height))
        return np.asarray(cropped.resize((width // 2, height // 2), Image.LANCZOS), dtype=np.float64)

    return crop(a), crop(b)


def box_mean(values: np.ndarray, k: int) -> np.ndarray:
    integral = np.zeros((values.shape[0] + 1, values.shape[1] + 1))
    integral[1:, 1:] = values.cumsum(axis=0).cumsum(axis=1)
    sums = integral[k:, k:] - integral[:-k, k:] - integral[k:, :-k] + integral[:-k, :-k]
    return sums / (k * k)


def ssim(x: np.ndarray, y: np.ndarray) -> float:
    c1, c2 = (K1 * LEVELS) ** 2, (K2 * LEVELS) ** 2
    mean_x, mean_y = box_mean(x, WINDOW), box_mean(y, WINDOW)
    var_x = np.maximum(box_mean(x * x, WINDOW) - mean_x * mean_x, 0.0)
    var_y = np.maximum(box_mean(y * y, WINDOW) - mean_y * mean_y, 0.0)
    cov = box_mean(x * y, WINDOW) - mean_x * mean_y
    numerator = (2.0 * mean_x * mean_y + c1) * (2.0 * cov + c2)
    denominator = (mean_x**2 + mean_y**2 + c1) * (var_x + var_y + c2)
    return float(np.mean(numerator / denominator))


def score(reference: str, candidate: str) -> dict[str, float]:
    x, y = align(grayscale(reference), grayscale(candidate))
    return {"ssim": ssim(x, y), "mad": float(np.mean(np.abs(x - y)))}


def run_template(template: str, **values: object) -> None:
    command = [part.format(**values) for part in shlex.split(template)]
    subprocess.run(command, check=True, capture_output=True, timeout=300)


def page_count(command: list[str]) -> int:
    out = subprocess.run(command, check=True, capture_output=True, text=True, timeout=300).stdout
    for line in out.splitlines():
        key, _, value = line.partition(":")
        if key.strip().lower() in {"pages", "page count"}:
            return int(value.strip().split()[0])
    raise ValueError(f"no page count in `{' '.join(command[:2])}`")


def pdf_page_count(pdf: str) -> int:
    return page_count(["pdfboss", "info", pdf])


def docboss_page_count(template: str, source: str) -> int:
    return page_count([part.format(input=source) for part in shlex.split(template)])


def render_pages(template: str, source: str, pages: int, scale: float, out_dir: str, prefix: str) -> list[str]:
    paths = []
    for page in range(1, pages + 1):
        output = os.path.join(out_dir, f"{prefix}-{page:03}.png")
        try:
            run_template(template, input=source, page=page, output=output, scale=scale)
        except (subprocess.CalledProcessError, subprocess.TimeoutExpired):
            break
        if not os.path.exists(output):
            break
        paths.append(output)
    return paths


def score_document(path: str, name: str, args: argparse.Namespace, work: str) -> dict:
    stem = os.path.splitext(name)[0]
    subprocess.run(
        ["soffice", "--headless", "--convert-to", "pdf", "--outdir", work, path],
        check=True,
        capture_output=True,
        timeout=300,
    )
    pdf = os.path.join(work, os.path.splitext(os.path.basename(path))[0] + ".pdf")
    reference_count = pdf_page_count(pdf)
    docboss_count = docboss_page_count(args.docboss_info, path)
    pages = min(reference_count, docboss_count, args.max_pages)
    reference = render_pages(args.pdfboss, pdf, pages, args.scale, work, stem + "-ref")
    candidate = render_pages(args.docboss, path, pages, args.scale, work, stem + "-docboss")
    scores = [score(r, c) for r, c in zip(reference, candidate)]
    return {
        "file": name,
        "reference_pages": reference_count,
        "docboss_pages": docboss_count,
        "pages_scored": len(scores),
        "ssim": float(np.mean([s["ssim"] for s in scores])) if scores else None,
        "mad": float(np.mean([s["mad"] for s in scores])) if scores else None,
        "per_page": scores,
    }


def relative(message: str, *roots: str) -> str:
    """The message with every path under `roots` written relative to it."""
    for root in roots:
        for spelling in {root, os.path.abspath(root), os.path.realpath(root)}:
            message = message.replace(spelling + os.sep, "").replace(spelling, ".")
    return message


def summarize(rows: list[dict]) -> dict:
    scored = [r["ssim"] for r in rows if r.get("ssim") is not None]
    matches = sum(1 for r in rows if "error" not in r and r.get("docboss_pages") == r.get("reference_pages"))
    return {
        "files": len(rows),
        "scored": len(scored),
        "mean_ssim": float(np.mean(scored)) if scored else None,
        "median_ssim": float(np.median(scored)) if scored else None,
        "p10_ssim": float(np.percentile(scored, 10, method="lower")) if scored else None,
        "page_count_matches": matches,
    }


def listed_files(args: argparse.Namespace) -> list[tuple[str, str]]:
    """The documents to score with the names their rows get: those `--files`
    lists, numbered in order, or the first `--sample` under the corpus."""
    if not args.files:
        found = sorted(glob.glob(os.path.join(args.corpus, "**", "*.doc*"), recursive=True))
        found = [f for f in found if f.lower().endswith((".docx", ".doc"))][: args.sample]
        return [(f, os.path.basename(f)) for f in found]
    repository = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    with open(args.files) as handle:
        paths = [line.strip() for line in handle if line.strip() and not line.startswith("#")]
    roots = [repository if p.startswith("crates/") else args.corpus for p in paths]
    return [(os.path.join(r, p), f"{i:03}-{os.path.basename(p)}") for i, (r, p) in enumerate(zip(roots, paths))]


def corpus_command(args: argparse.Namespace) -> None:
    files = listed_files(args)
    if not files:
        sys.exit(f"no .docx or .doc files under {args.corpus}")
    for tool in ("soffice", "pdfboss"):
        if not shutil.which(tool):
            sys.exit(f"`{tool}` is required on PATH")
    results = []
    with tempfile.TemporaryDirectory() as work:
        for path, name in files:
            try:
                results.append(score_document(path, name, args, work))
            except Exception as error:  # noqa: BLE001 - every failure is a data point
                results.append({"file": name, "error": relative(str(error), work, args.corpus)})
            row = results[-1]
            status = row.get("error") or f"ssim {row['ssim']} mad {row['mad']} pages {row['docboss_pages']}/{row['reference_pages']}"
            print(f"{row['file']}: {status}", flush=True)
    summary = summarize(results)
    for name, extension in (("docx", ".docx"), ("doc", ".doc")):
        summary[name] = summarize([r for r in results if r["file"].lower().endswith(extension)])
    print(json.dumps(summary, indent=2))
    report = {
        "machine": common.machine(),
        "versions": common.versions(),
        "date": time.strftime("%Y-%m-%d"),
        "settings": {
            "files": args.files,
            "sample": None if args.files else args.sample,
            "scale": args.scale,
            "max_pages": args.max_pages,
        },
        "summary": summary,
        "results": results,
    }
    with open(args.output, "w") as handle:
        json.dump(report, handle, indent=2)


def compare_command(args: argparse.Namespace) -> None:
    names = sorted(n for n in os.listdir(args.reference) if n.endswith(".png"))
    scores = {}
    for name in names:
        candidate = os.path.join(args.candidate, name)
        if os.path.exists(candidate):
            scores[name] = score(os.path.join(args.reference, name), candidate)
            print(f"{name}: ssim {scores[name]['ssim']:.4f} mad {scores[name]['mad']:.2f}")
    if scores:
        print(f"mean ssim {np.mean([s['ssim'] for s in scores.values()]):.4f} over {len(scores)} pages")


def main() -> None:
    if len(sys.argv) > 1 and sys.argv[1] == "compare":
        parser = argparse.ArgumentParser(prog="bench_fidelity.py compare")
        parser.add_argument("reference")
        parser.add_argument("candidate")
        compare_command(parser.parse_args(sys.argv[2:]))
        return
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("corpus")
    parser.add_argument("--sample", type=int, default=40)
    parser.add_argument("--files", help="a list of documents to score, one path a line, in place of --sample")
    parser.add_argument("--scale", type=float, default=1.5)
    parser.add_argument("--max-pages", type=int, default=5)
    parser.add_argument("--docboss", default=DEFAULT_DOCBOSS)
    parser.add_argument("--pdfboss", default=DEFAULT_PDFBOSS)
    parser.add_argument("--docboss-info", default=DEFAULT_DOCBOSS_INFO)
    parser.add_argument("--output", default=os.path.join(os.path.dirname(__file__), "results-fidelity.json"))
    corpus_command(parser.parse_args())


if __name__ == "__main__":
    main()

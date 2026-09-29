# Benchmarks

Eight scripts, because opening a corpus of small files, extracting one large
document, parallel throughput, memory, malformed input, rendering speed and
rendering quality are different workloads, and speed means nothing if the
output is wrong:

- `bench.py` times open + parse, text extraction, Markdown and HTML over a
  deterministic sample of a corpus, against every engine that produces the
  same kind of output, and scores every text output for **word recall**
  against LibreOffice's text export of the same file.
- `bench_large.py` times text extraction of one large generated document
  (headings, formatted paragraphs, nested lists, tables), where per-call
  overhead stops mattering and the extraction itself is measured.
- `bench_parallel.py` measures what the sequential scripts leave out: each
  engine is timed sequentially and through its best parallel route (threads,
  processes, or docboss's own `extract_texts`).
- `bench_memory.py` measures peak RSS per engine, one fresh process per
  (engine, workload), so no engine's allocations count against another.
- `bench_robustness.py` turns the filtering around: it feeds every engine
  damaged files (byte flips, truncations, broken XML, zeroed ZIP directories,
  overwritten compound-file sectors) and
  counts text returned, clean exceptions, crashes and hangs. It and
  `bench_memory.py` share a subprocess harness (`isolation.py`), so a crash is
  a data point instead of the end of the run.
- `bench_render.py` times docboss's page rendering: a fixed, evenly spaced
  sample of 60 DOCX and 30 DOC files, the first 3 pages of each rendered to
  PNG at scale 1.5 through the Python bindings, in one process and one page
  after another, best of 5 runs. It reports pages per second and the total
  time for each format.
- `bench_fidelity.py` scores docboss's page rendering against LibreOffice's.
- `fetch_corpus.sh` downloads the corpora at pinned revisions.

## Libraries

| Library | Open | Text | Markdown | HTML | Parallel | Memory | Robustness | Notes |
|---|:-:|:-:|:-:|:-:|:-:|:-:|:-:|---|
| docboss | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | this project (Rust), DOCX and DOC |
| python-docx | ✓ | ✓ | | | ✓ | ✓ | ✓ | pure Python, DOCX |
| docx2python | | ✓ | | | ✓ | ✓ | ✓ | pure Python, DOCX |
| mammoth | | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | pure Python, DOCX |
| docx2txt | | ✓ | | | ✓ | ✓ | ✓ | pure Python, DOCX |
| pandoc | | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | Haskell binary through `pypandoc_binary`, DOCX |
| antiword | | ✓ | | | ✓ | ✓ | ✓ | C, DOC, built from source |
| catdoc | | ✓ | | | ✓ | ✓ | ✓ | C, DOC, built from source |
| LibreOffice | | ✓ | | | | | | reference for recall and rendering; timed separately |

Versions: docboss 0.1.0 (release build of main with the list-counter and text-box
fixes, b4eeaf7 and 003377a), python-docx 1.2.0,
docx2python 3.7.1, mammoth 1.13.0, docx2txt 0.9, pypandoc_binary 1.17 (pandoc
3.9), antiword 0.37, catdoc 0.95, LibreOffice 26.8.0.3. Machine: Apple M3 Pro,
12 cores, macOS 26.4, Python 3.12.10. All tables below come from one session
on 2026-09-28.

## Method

- **Corpora**: real-world test documents of LibreOffice, Apache POI and
  python-docx (fetched by `fetch_corpus.sh`), 1,697 DOCX and 180 DOC files after
  removing duplicates. `bench.py`
  takes a deterministic, evenly spaced sample (`--sample 80`).
- **Timing**: best-of-3 per file after one warm-up pass, so the OS file cache
  and imports are hot, aggregated **only over the files every engine
  handled**, so each total compares the same workload. Reported as files per
  second, pages per second (pages from docboss's layout) and the median time
  per file.
- **Recall**: LibreOffice converts each file to text once (cached). An
  engine's recall on a file is the share of the reference's words, with
  multiplicity, found in its output. Files whose reference has no words are
  skipped: LibreOffice's text export leaves out text boxes, so a file whose
  only text sits in one has nothing to score against. The mean is over files;
  the weighted figure weights files by their word count.
- **LibreOffice** pays process startup on every `soffice --convert-to` call,
  so it is timed twice: one file per process, and the whole sample as one
  batch conversion, which is the fastest way to drive it.

## Text, Markdown and HTML (`bench.py`)

DOCX, 80 sampled files (120 pages); 76 were handled by every text engine,
recall scored on the 57 with reference text:

| Engine | Text files/s | Median ms | Word recall (mean) | Weighted |
|---|--:|--:|--:|--:|
| **docboss** | **2,676** | **0.23** | **0.992** | **0.997** |
| docx2txt | 663 | 0.42 | 0.974 | 0.991 |
| python-docx | 560 | 0.93 | 0.848 | 0.978 |
| docx2python | 112 | 2.19 | 0.974 | 0.993 |
| mammoth | 104 | 5.29 | 0.956 | 0.992 |
| pandoc | 6.6 | 131.46 | 0.894 | 0.971 |

| Operation | docboss | mammoth | pandoc | python-docx |
|---|--:|--:|--:|--:|
| Open + parse (files/s) | 2,504 | | | 851 |
| Markdown (files/s) | 3,166 | 106 | 7.5 | |
| HTML (files/s) | 3,531 | 112 | 7.7 | |

docboss extracts DOCX text about 4× faster than docx2txt, the fastest other
engine, and 24× faster than mammoth; it converts to Markdown and HTML about
30× faster than mammoth and 420× faster than pandoc. It also has the highest
recall. python-docx's low mean recall comes from what its API exposes: the adapter reads
body paragraphs and table cells, so headers, footers, footnotes, comments and
text boxes are left out.

DOC, 80 sampled files (396 pages); 66 were handled by every engine, recall
scored on the 61 with reference text:

| Engine | Text files/s | Median ms | Word recall (mean) | Weighted |
|---|--:|--:|--:|--:|
| **docboss** | **2,680** | **0.07** | **0.976** | **0.994** |
| catdoc | 186 | 4.74 | 0.888 | 0.347 |
| antiword | 179 | 5.18 | 0.948 | 0.359 |

docboss reads DOC text about 14× faster than catdoc and antiword. Their
weighted recall is dominated by `Bug50955.doc`, a 74,813-word Russian novel
that both decode to 0.1% of its words (docboss: all of them); their mean
recall is the fairer comparison. antiword refuses 12 of the 72 files the
others open.

LibreOffice converts the same samples at 14.2 (DOCX) and 15.9 (DOC) files/s
as one batch, and takes a median 812 ms (DOCX) and 689 ms (DOC) per file one
process at a time.

## One large document (`bench_large.py`)

A generated document of 3,000 sections (1,166 pages in docboss's layout), each
a heading, a paragraph with bold and italic runs, a bullet list with a nested
numbered list, and a four-row table; LibreOffice converts it to DOC for the
DOC engines. Best of 3:

| Engine | DOCX text | Engine | DOC text |
|---|--:|---|--:|
| **docboss** | **80 ms** | **catdoc** | **53 ms** |
| docx2txt | 754 ms | docboss | 155 ms |
| python-docx | 1,524 ms | antiword | 2,599 ms |
| mammoth | 5,685 ms | | |
| pandoc | 8,079 ms | | |
| docx2python | 8,500 ms | | |

**catdoc wins the large DOC**, about 3× faster than docboss: it streams the
text out of the piece table without building a document model, while docboss
parses formatting, styles, lists and tables into its model first. On the DOCX
docboss is about 9× faster than docx2txt. docboss's output is the longest of
the DOCX engines (1.64 million characters against about 0.98 million for most
others) because it keeps list labels and table cell separators.

## Parallel (`bench_parallel.py`)

Each engine's sequential time against its best parallel route on 12 workers:

| Engine | DOCX files/s (300 files) | Route | DOC files/s (180 files) | Route |
|---|--:|---|--:|---|
| **docboss** | **16,034** | `extract_texts`, 5.5× | **5,148** | threads, 2.4× |
| docx2txt | 1,204 | processes, 1.6× | | |
| python-docx | 1,070 | processes, 1.7× | | |
| docx2python | 295 | processes, 2.7× | | |
| mammoth | 289 | processes, 4.0× | | |
| pandoc | 34 | threads, 4.9× | | |
| catdoc | | | 437 | threads, 2.0× |
| antiword | | | 260 | threads, 1.4× |

`docboss.extract_texts` reads the files on a Rust thread pool in one call. The
DOC sample holds fewer, larger files, so its speedup is lower.

## Memory (`bench_memory.py`)

Peak RSS of a fresh process that imports the engine and extracts the text of
100 sampled files (the interpreter's own baseline is 22 to 35 MB depending on
the engine's imports), and of the largest file on its own. For command-line
engines the child process's peak is given too:

| Engine | DOCX sample | DOCX largest (2.96 MB) | Engine | DOC sample | DOC largest (6.24 MB) |
|---|--:|--:|---|--:|--:|
| docx2txt | **30.1 MB** | **24.6 MB** | catdoc | **23.2 MB** (child 2.0) | **23.9 MB** (child 2.9) |
| docboss | 31.7 MB | 34.3 MB | antiword | 23.6 MB (child 2.5) | 23.8 MB (child 2.9) |
| docx2python | 55.1 MB | 30.3 MB | docboss | 42.3 MB | 61.6 MB |
| python-docx | 58.8 MB | 41.6 MB | | | |
| mammoth | 82.2 MB | 32.6 MB | | | |
| pandoc | 29.7 MB (child 134.9) | 29.2 MB (child 139.9) | | | |

**The competitors win memory on DOC**: antiword and catdoc stream text in about
2 to 3 MB of their own, while docboss holds the whole document model and the
file's streams, 19 MB over its baseline on the sample and 38 MB on the 6.2 MB
file. On DOCX, docx2txt peaks slightly lower than docboss; the other engines
peak higher.

## Robustness (`bench_robustness.py`)

150 damaged files per format, derived from the corpus: byte flips (1 to 64
bytes) and truncations for both formats; for DOCX, packages rebuilt with a
damaged `word/document.xml` and packages with a zeroed central directory; for
DOC, a 512-byte sector overwritten or zeroed. Each file is extracted in its
own process with a 30 s timeout:

| Engine | DOCX: text | error | crash | hang | Engine | DOC: text | error | crash | hang |
|---|--:|--:|--:|--:|---|--:|--:|--:|--:|
| **docboss** | **140** | 10 | 0 | 0 | catdoc | **100** | 49 | 0 | 1 |
| docx2txt | 8 | 142 | 0 | 0 | **docboss** | 94 | 56 | 0 | 0 |
| docx2python | 4 | 146 | 0 | 0 | antiword | 71 | 76 | 3 | 0 |
| pandoc | 4 | 146 | 0 | 0 | | | | | |
| python-docx | 2 | 148 | 0 | 0 | | | | | |
| mammoth | 1 | 149 | 0 | 0 | | | | | |

docboss returns text from 140 of the damaged DOCX files, where the other
engines give up on all but 1 to 8: it reads a ZIP from its local headers when
the central directory is damaged, keeps what inflated before a corrupt
deflate stream, and balances broken XML. On damaged DOC files **catdoc returns
text more often** (100 against 94), because it scans the text without
checking the structures docboss refuses to trust; it also hung once. antiword
crashed with a segmentation fault 3 times. docboss neither crashed nor hung on
any of the 300 files.

## Rendering fidelity (`bench_fidelity.py`)

LibreOffice converts each document to PDF, pdfboss rasterizes that PDF, and
docboss renders the same pages from the document itself, both at scale 1.5.
Each pair is decoded to grayscale, center-cropped to the common size and
Lanczos-downsampled to half resolution, then scored with windowed SSIM (8×8
uniform window) and mean absolute pixel difference; up to 5 pages per file.
Page counts are compared too, since a layout that paginates differently
scores every later page against the wrong reference. The sample is 60 DOCX
and 30 DOC files spread evenly over the corpus; 88 were scored (LibreOffice
could not convert a fuzzer file and a password-protected one):

| Format | Files | SSIM mean | SSIM median | SSIM p10 | MAD median | Page counts equal |
|---|--:|--:|--:|--:|--:|--:|
| DOCX | 59 | 0.940 | 0.978 | 0.857 | 0.82 | 56 |
| DOC | 29 | 0.876 | 0.948 | 0.634 | 1.86 | 25 |

SSIM scores the whole page, and mostly white pages score high whatever their
content, so read the p10 column as the honest one: a tenth of the DOC files
score under 0.64. The low scores come from what layout does not draw yet
(shapes other than pictures and text boxes, SmartArt), from tracked changes that LibreOffice shows
with revision marks and docboss as final text, and from table row heights
that differ from LibreOffice's. `failure-modes/` records the fixes made so
far, with before and after renders.

## Reproducing

```bash
./benchmarks/fetch_corpus.sh corpus/
uv run maturin develop --release
uv pip install python-docx docx2python mammoth docx2txt pypandoc_binary psutil numpy pillow
export DOCBOSS_BENCH_TOOLS=/path/to/antiword:/path/to/catdoc/src   # locally built binaries
uv run python benchmarks/bench.py docx corpus/ --sample 80 --libreoffice
uv run python benchmarks/bench.py doc corpus/ --sample 80 --libreoffice
uv run python benchmarks/bench_large.py
uv run python benchmarks/bench_parallel.py docx corpus/ --sample 300
uv run python benchmarks/bench_memory.py doc corpus/ --sample 100
uv run python benchmarks/bench_robustness.py mutate docx damaged/docx corpus/ --count 150
uv run python benchmarks/bench_robustness.py run docx damaged/docx
uv run python benchmarks/bench_render.py corpus/
uv run python benchmarks/bench_fidelity.py corpus/ --sample 90 --max-pages 5
```

Every script writes its `results-*.json` next to itself, with the machine,
versions and date. Numbers are machine-dependent; compare rows against each
other, not against another machine's.

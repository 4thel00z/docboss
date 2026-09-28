"""Derive the clause outlines in `Ledger/Outline/` from the specification texts.

Usage, from the repository root:

    mkdir -p ledger/specs && cd ledger/specs
    curl -sSLO https://ecma-international.org/wp-content/uploads/ECMA-376-1_5th_edition_december_2016.zip
    curl -sSLO https://ecma-international.org/wp-content/uploads/ECMA-376-2_5th_edition_december_2021.zip
    curl -sSLO https://ecma-international.org/wp-content/uploads/ECMA-376-3_5th_edition_december_2015.zip
    for z in ECMA-376-*.zip; do unzip -oq "$z" '*.pdf'; done
    mv "Ecma Office Open XML Part 1 - Fundamentals And Markup Language Reference.pdf" ecma376-1.pdf
    mv "Ecma Office Open XML Part 2 - Open Packaging Conventions.pdf" ecma376-2.pdf
    mv "Ecma Office Open XML Part 3 - Markup Compatibility and Extensibility.pdf" ecma376-3.pdf
    curl -sSLo MS-DOC.pdf 'https://officeprotocoldoc.z19.web.core.windows.net/files/MS-DOC/%5bMS-DOC%5d.pdf'
    curl -sSLo MS-OSHARED.pdf 'https://officeprotocoldoc.z19.web.core.windows.net/files/MS-OSHARED/%5bMS-OSHARED%5d.pdf'
    curl -sSLo MS-ODRAW.pdf 'https://officeprotocoldoc.z19.web.core.windows.net/files/MS-ODRAW/%5bMS-ODRAW%5d.pdf'
    curl -sSLo MS-CFB.pdf 'https://winprotocoldocs-bhdugrdyduf5h2e4.b02.azurefd.net/MS-CFB/%5bMS-CFB%5d.pdf'
    curl -sSLo MS-OLEPS.pdf 'https://winprotocoldocs-bhdugrdyduf5h2e4.b02.azurefd.net/MS-OLEPS/%5bMS-OLEPS%5d.pdf'
    curl -sSLo APPNOTE.TXT https://pkware.cachefly.net/webdocs/casestudies/APPNOTE.TXT
    for f in ecma376-1 ecma376-2 ecma376-3 MS-DOC MS-OSHARED MS-ODRAW MS-CFB MS-OLEPS; do pdfboss text $f.pdf > $f.txt; done
    cd ../.. && python3 ledger/tools/outline.py ledger/specs ledger/Ledger/Outline

The PDF texts are read through their tables of contents: every line with a
dot leader and a page number that starts with a clause number (`17.3.1.29`,
`2.5.1`) or an annex clause (`A.1`, `Annex A.`) is a heading, and a title
wrapped onto a second line is joined. Headings deeper than the table of
contents goes are taken from the body when their parent is a heading the
table of contents lists without children. APPNOTE.TXT has no table of
contents: its numbered paragraphs (`4.3.7  Local file header:`) are the
headings, `4.0 ZIP Files` is chapter 4, and a title is cut at its first
colon or sentence end.
"""

from __future__ import annotations

import re
import sys
from dataclasses import dataclass
from pathlib import Path

LEADER = re.compile(r"\s*(?:\.\s*){4,}\s*([0-9]+|[ivxlc]+)\s*$")
CLAUSE = re.compile(r"^(\d{1,2}(?:\.\d+)*)\.?\s+(\S.*?)\s*$")
ANNEX_TITLE = re.compile(r"^Annex\s+([A-Z])\.?\s+(?:\((?:normative|informative)\)\s*)?(.*?)\s*$")
ANNEX_CLAUSE = re.compile(r"^([A-Z])\.(\d+(?:\.\d+)*)\s+(\S.*?)\s*$")
APPNOTE_HEADING = re.compile(r"^\s{0,12}(\d{1,2})\.(\d+)((?:\.\d+)*)\.?\s+(\S.*?)\s*$")


@dataclass(frozen=True)
class Heading:
    letter: str
    path: tuple[int, ...]
    title: str

    @property
    def key(self) -> tuple:
        return (0, *self.path) if not self.letter else (1, ord(self.letter), *self.path)


@dataclass(frozen=True)
class Standard:
    module: str
    name: str
    source: str
    description: str
    appnote: bool = False


STANDARDS = [
    Standard("Ecma376Part1", "ecma376Part1", "ecma376-1.txt",
             "ECMA-376 Part 1, 5th edition (2016): Fundamentals and Markup Language Reference"),
    Standard("Ecma376Part2", "ecma376Part2", "ecma376-2.txt",
             "ECMA-376 Part 2, 5th edition (2021): Open Packaging Conventions"),
    Standard("Ecma376Part3", "ecma376Part3", "ecma376-3.txt",
             "ECMA-376 Part 3, 5th edition (2015): Markup Compatibility and Extensibility"),
    Standard("MsDoc", "msDoc", "MS-DOC.txt", "[MS-DOC]: Word (.doc) Binary File Format"),
    Standard("MsCfb", "msCfb", "MS-CFB.txt", "[MS-CFB]: Compound File Binary File Format"),
    Standard("MsOleps", "msOleps", "MS-OLEPS.txt",
             "[MS-OLEPS]: Object Linking and Embedding (OLE) Property Set Data Structures"),
    Standard("MsOshared", "msOshared", "MS-OSHARED.txt", "[MS-OSHARED]: Office Common Data Types and Objects Structures"),
    Standard("MsOdraw", "msOdraw", "MS-ODRAW.txt", "[MS-ODRAW]: Office Drawing Binary File Format"),
    Standard("Appnote", "appnote", "APPNOTE.TXT", "PKWARE APPNOTE.TXT: .ZIP File Format Specification",
             appnote=True),
]


def clean(title: str) -> str:
    return re.sub(r"\s+", " ", title).strip()


def parse_heading(text: str) -> Heading | None:
    match = ANNEX_TITLE.match(text)
    if match:
        return Heading(match.group(1), (), clean(match.group(2)))
    match = ANNEX_CLAUSE.match(text)
    if match:
        return Heading(match.group(1), tuple(int(p) for p in match.group(2).split(".")), clean(match.group(3)))
    match = CLAUSE.match(text)
    if match and match.group(2)[0].isalpha():
        return Heading("", tuple(int(p) for p in match.group(1).split(".")), clean(match.group(2)))
    return None


def toc_headings(lines: list[str]) -> tuple[list[Heading], int]:
    headings: dict[tuple, Heading] = {}
    last_toc_line = 0
    pending: str | None = None
    for number, line in enumerate(lines):
        leader = LEADER.search(line)
        if not leader:
            pending = line.strip() if parse_heading(line.strip()) else None
            continue
        text = line[: leader.start()].strip()
        heading = parse_heading(text)
        if not heading and pending:
            heading = parse_heading(f"{pending} {text}")
        pending = None
        if not heading:
            continue
        headings.setdefault(heading.key, heading)
        last_toc_line = number
    return list(headings.values()), last_toc_line


def parent_key(heading: Heading) -> tuple | None:
    if not heading.path:
        return None
    parent = Heading(heading.letter, heading.path[:-1], "")
    if not heading.letter and not parent.path:
        return None
    return parent.key


def body_headings(lines: list[str], start: int, known: list[Heading]) -> list[Heading]:
    """Headings deeper than the table of contents, found in the body under a
    listed heading that has no listed children."""
    by_key = {h.key: h for h in known}
    has_children = {parent_key(h) for h in known}
    found: dict[tuple, Heading] = {}
    for line in lines[start:]:
        if LEADER.search(line):
            continue
        heading = parse_heading(line.strip())
        if not heading or heading.key in by_key or heading.key in found:
            continue
        parent = parent_key(heading)
        if parent is None:
            continue
        if parent in found or (parent in by_key and parent not in has_children):
            if len(heading.title) > 120 or not re.match(r"^[A-Za-z]", heading.title):
                continue
            found[heading.key] = heading
    return list(found.values())


def appnote_headings(lines: list[str]) -> tuple[list[Heading], list[tuple[int, str]]]:
    headings: dict[tuple, Heading] = {}
    chapters: list[tuple[int, str]] = []
    current = 0
    for line in lines:
        match = APPNOTE_HEADING.match(line.rstrip("\r\n"))
        if not match:
            continue
        chapter, second, rest, title = match.groups()
        path = [int(chapter), int(second)] + [int(p) for p in rest.split(".") if p]
        title = re.split(r":|\.\s|\.$", clean(title).lstrip("-. "))[0].strip()
        if not title or not title[0].isalpha():
            continue
        if path[1] == 0 and len(path) == 2 and path[0] == current + 1 and not line[0].isspace():
            chapters.append((path[0], title))
            current = path[0]
            continue
        if path[0] != current:
            continue
        heading = Heading("", tuple(path), title[:80])
        headings.setdefault(heading.key, heading)
    return list(headings.values()), chapters


def lean_string(text: str) -> str:
    return '"' + text.replace("\\", "\\\\").replace('"', '\\"') + '"'


def render_ref(heading: Heading) -> str:
    path = "[" + ", ".join(str(p) for p in heading.path) + "]"
    if heading.letter:
        return f".annex '{heading.letter}' {path}"
    return f".clause {path}"


CHUNK = 400


def render_module(standard: Standard, chapters: list[tuple[int, str]], annexes: list[tuple[str, str]],
                  headings: list[Heading]) -> str:
    chunks = [headings[i:i + CHUNK] for i in range(0, len(headings), CHUNK)] or [[]]
    out = [
        "import Ledger.Ref",
        "",
        "/-!",
        f"The clause outline of {standard.description},",
        "derived from the specification text by `tools/outline.py`. Regenerate rather than edit.",
        "-/",
        "",
        "namespace Ledger.Outline." + standard.module,
        "",
        "/-- Chapter titles by number. -/",
        "def chapters : List (Nat × String) := [",
        ",\n".join(f"  ({n}, {lean_string(t)})" for n, t in chapters),
        "]",
        "",
        "/-- Annex titles by letter. -/",
        "def annexes : List (Char × String) := [",
        ",\n".join(f"  ('{l}', {lean_string(t)})" for l, t in annexes),
        "]",
        "",
    ]
    for index, chunk in enumerate(chunks):
        out.append(f"def headings{index} : List Heading := [")
        out.append(",\n".join(f"  ⟨{render_ref(h)}, {lean_string(h.title)}⟩" for h in chunk))
        out.append("]")
        out.append("")
    joined = " ++ ".join(f"headings{i}" for i in range(len(chunks)))
    out += [
        "/-- Every numbered heading of the specification, in document order. -/",
        f"def headings : List Heading := {joined}",
        "",
        "end Ledger.Outline." + standard.module,
        "",
    ]
    return "\n".join(out)


def outline(standard: Standard, text: str) -> tuple[list[tuple[int, str]], list[tuple[str, str]], list[Heading]]:
    lines = text.splitlines()
    if standard.appnote:
        headings, chapters = appnote_headings(lines)
        return chapters, [], sorted(headings, key=lambda h: h.key)
    toc, last = toc_headings(lines)
    headings = toc + body_headings(lines, 0, toc)
    chapters = sorted({(h.path[0], h.title) for h in headings if not h.letter and len(h.path) == 1})
    annexes = sorted({(h.letter, h.title) for h in headings if h.letter and not h.path})
    chapter_numbers = {n for n, _ in chapters}
    annex_letters = {l for l, _ in annexes}
    kept = [
        h for h in headings
        if (h.letter and h.letter in annex_letters and h.path)
        or (not h.letter and len(h.path) > 1 and h.path[0] in chapter_numbers)
    ]
    return chapters, annexes, sorted(kept, key=lambda h: h.key)


def main(argv: list[str]) -> int:
    specs = Path(argv[1] if len(argv) > 1 else "ledger/specs")
    target = Path(argv[2] if len(argv) > 2 else "ledger/Ledger/Outline")
    target.mkdir(parents=True, exist_ok=True)
    for standard in STANDARDS:
        text = (specs / standard.source).read_text(encoding="utf-8", errors="replace")
        chapters, annexes, headings = outline(standard, text)
        (target / f"{standard.module}.lean").write_text(render_module(standard, chapters, annexes, headings))
        print(f"{standard.module}: {len(chapters)} chapters, {len(annexes)} annexes, {len(headings)} headings",
              file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))

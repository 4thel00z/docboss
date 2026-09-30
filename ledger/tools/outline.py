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
    curl -sSLo MS-WMF.pdf 'https://winprotocoldocs-bhdugrdyduf5h2e4.b02.azurefd.net/MS-WMF/%5bMS-WMF%5d.pdf'
    curl -sSLo MS-EMF.pdf 'https://winprotocoldocs-bhdugrdyduf5h2e4.b02.azurefd.net/MS-EMF/%5bMS-EMF%5d.pdf'
    curl -sSLo APPNOTE.TXT https://pkware.cachefly.net/webdocs/casestudies/APPNOTE.TXT
    for f in ecma376-1 ecma376-2 ecma376-3 MS-DOC MS-OSHARED MS-ODRAW MS-CFB MS-OLEPS MS-WMF MS-EMF; do pdfboss text $f.pdf > $f.txt; done
    cd ../.. && python3 ledger/tools/outline.py ledger/specs ledger/Ledger/Outline

The PDF texts are read through their tables of contents: every line with a
dot leader and a page number that starts with a clause number (`17.3.1.29`,
`2.5.1`) or an annex clause (`A.1`, `Annex A.`) is a heading. That covers
the front table and Part 1's informative per-clause tables. A title wrapped
onto a second line is joined, as is a title whose page number sits alone on
the next line. Headings the tables leave out (`20.1.1 Table of Contents`,
the deeper element clauses) are read from the body in one pass, and only as
the next numbered child of an open heading, so table rows, list items and
cross-references are not taken for headings. APPNOTE.TXT has no table of
contents: `4.0` in column 0 opens chapter 4, numbered paragraphs follow in
sequence (`4.5 - File uses ZIP64` table rows start with a dash and a space
and are skipped), and a title is its text up to the first colon or sentence
end, cut at 80 characters.

Every outline then passes `self_check`: references strictly increasing and
unique, every heading's parent present, sibling numbers consecutive from 1,
and no title that reads like a table row or a fragment. A failure stops the
run with the offending headings named.
"""

from __future__ import annotations

import re
import sys
from dataclasses import dataclass
from pathlib import Path

LEADER = re.compile(
    r"(?:\s*(?:\.\s*){2,}|\s\.\s|(?<=\))\s)\s*([0-9]+|[ivxlc]+)"
    r"(?:\s+(?:ECMA-376 Part \d+|\d+\s*/\s*\d+|\[MS-[A-Z0-9]+\].*))?\s*$"
)
CLAUSE = re.compile(r"^(\d{1,2}(?:\.\d+)*)\.?\s+(\S.*?)\s*$")
ANNEX_TITLE = re.compile(r"^Annex\s+([A-Z])\.?\s+(?:\((?:normative|informative)\)\s*)?(.*?)\s*$")
ANNEX_CLAUSE = re.compile(r"^([A-Z])\.(\d+(?:\.\d+)*)\s+(\S.*?)\s*$")
APPNOTE_HEADING = re.compile(r"^( *)(\d{1,2})\.(\d+)((?:\.\d+)*)\.?\s+(\S.*?)\s*$")
TABLE_CELLS = re.compile(r"\b(?:Yes|No)\b.*\b(?:Yes|No)\b")
FRAGMENT_END = re.compile(r"(?:[,;(\-–]|\b(?:and|or|of|the|to|a|an|in|for|with))$")


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
    Standard("MsWmf", "msWmf", "MS-WMF.txt", "[MS-WMF]: Windows Metafile Format"),
    Standard("MsEmf", "msEmf", "MS-EMF.txt", "[MS-EMF]: Enhanced Metafile Format"),
    Standard("Appnote", "appnote", "APPNOTE.TXT", "PKWARE APPNOTE.TXT: .ZIP File Format Specification",
             appnote=True),
]


def clean(title: str) -> str:
    return re.sub(r"\s+", " ", title).strip()


def title_start(title: str) -> bool:
    return title[0].isalpha() or title[0] in "\"“'" or bool(re.match(r"\d[A-Za-z]", title))


def parse_heading(text: str) -> Heading | None:
    match = ANNEX_TITLE.match(text)
    if match:
        return Heading(match.group(1), (), clean(match.group(2)))
    match = ANNEX_CLAUSE.match(text)
    if match:
        return Heading(match.group(1), tuple(int(p) for p in match.group(2).split(".")), clean(match.group(3)))
    match = CLAUSE.match(text)
    if match and title_start(match.group(2)):
        return Heading("", tuple(int(p) for p in match.group(1).split(".")), clean(match.group(2)))
    return None


def toc_headings(lines: list[str]) -> tuple[list[Heading], set[int]]:
    """Every table-of-contents entry: the front table and, in Part 1, the
    informative per-clause tables. Returns the headings and the line numbers
    they were read from."""
    headings: dict[tuple, Heading] = {}
    toc_lines: set[int] = set()
    pending: str | None = None
    for number, line in enumerate(lines):
        leader = LEADER.search(line)
        between_entries = any(LEADER.search(nearby) for nearby in lines[max(0, number - 3): number + 4])
        if not leader and pending and between_entries and re.fullmatch(r"\d+", line.strip()):
            heading = parse_heading(pending)
            if heading:
                headings.setdefault(heading.key, heading)
                toc_lines.update((number - 1, number))
            pending = None
            continue
        if not leader:
            pending = line.strip() if parse_heading(line.strip()) else None
            continue
        text = line[: leader.start()].strip()
        heading = parse_heading(text)
        if pending:
            joined = parse_heading(f"{pending} {text}")
            if joined and (not heading or not text[:1].isdigit()):
                heading = joined
                toc_lines.add(number - 1)
        pending = None
        if not heading or "…" in heading.title:
            continue
        headings.setdefault(heading.key, heading)
        toc_lines.add(number)
    return list(headings.values()), toc_lines


def parent_key(heading: Heading) -> tuple | None:
    if not heading.path:
        return None
    return Heading(heading.letter, heading.path[:-1], "").key


def acceptable_title(title: str) -> bool:
    return (
        bool(title)
        and len(title) <= 150
        and title_start(title)
        and not TABLE_CELLS.search(title)
        and not FRAGMENT_END.search(title)
    )


def body_headings(lines: list[str], toc: list[Heading], toc_lines: set[int]) -> list[Heading]:
    """Headings the tables of contents leave out, read from the body in one
    pass. A listed heading met in the body with its listed title opens its
    place in the tree; an unlisted one is kept only when it is the next
    numbered child of an open heading, so cross-references, table rows and
    list items do not count."""
    listed = {h.key: h for h in toc}
    by_parent: dict[tuple, list[Heading]] = {}
    for h in toc:
        by_parent.setdefault(parent_key(h), []).append(h)
    chain: list[Heading] = []
    next_child: dict[tuple, int] = {}
    last: tuple = ()
    found: dict[tuple, Heading] = {}
    for number, line in enumerate(lines):
        if number in toc_lines:
            continue
        heading = parse_heading(line.strip())
        if not heading:
            continue
        parent = parent_key(heading)
        known = listed.get(heading.key)
        if known:
            prefix = min(len(known.title), 20)
            if heading.key <= last or heading.title[:prefix].lower() != known.title[:prefix].lower():
                continue
            ancestors = [listed[k] for k in ancestor_keys(known) if k in listed]
            chain = ancestors + [known]
            if parent is not None:
                next_child[parent] = heading.path[-1] + 1
            next_child[known.key] = 1
            last = heading.key
            continue
        open_keys = [h.key for h in chain]
        if parent not in open_keys or heading.key in found:
            continue
        if heading.path[-1] != next_child.get(parent, 1) or heading.key <= last:
            continue
        siblings = by_parent.get(parent, [])
        if any(s.path[-1] == heading.path[-1] for s in siblings):
            continue
        if not acceptable_title(heading.title):
            continue
        del chain[open_keys.index(parent) + 1:]
        chain.append(heading)
        next_child[parent] = heading.path[-1] + 1
        next_child[heading.key] = 1
        last = heading.key
        found[heading.key] = heading
    return list(found.values())


def ancestor_keys(heading: Heading) -> list[tuple]:
    keys = []
    for depth in range(0 if heading.letter else 1, len(heading.path)):
        keys.append(Heading(heading.letter, heading.path[:depth], "").key)
    return keys


def appnote_title(lines: list[str], number: int, first: str) -> str:
    """A numbered APPNOTE paragraph's title: its text up to the first colon
    or sentence end, joined across at most two wrapped lines."""
    text = first
    for follow in lines[number + 1: number + 3]:
        underline = re.fullmatch(r"\s*-{3,}\s*", follow)
        if re.search(r":|\.\s|\.$", text) or not follow.strip() or underline or APPNOTE_HEADING.match(follow):
            break
        text = f"{text} {follow.strip()}"
    title = re.split(r":|\.\s|\.$", clean(text).lstrip("-. "))[0].strip()
    if len(title) <= 80:
        return title
    return title[:80].rsplit(" ", 1)[0].rstrip(",;") + "…"


def appnote_headings(lines: list[str]) -> tuple[list[Heading], list[tuple[int, str]]]:
    """Numbered paragraphs, in sequence: `4.0` in column 0 opens chapter 4, and each accepted
    heading is the next child of an open heading. Table rows such as
    `4.5 - File uses ZIP64` start with a dash and a space and are rejected;
    `6.0.1` sits directly under chapter 6."""
    headings: list[Heading] = []
    chapters: list[tuple[int, str]] = []
    chain: list[tuple[int, ...]] = []
    next_child: dict[tuple[int, ...], int] = {}
    for number, line in enumerate(lines):
        match = APPNOTE_HEADING.match(line.rstrip("\r\n"))
        if not match:
            continue
        indent, chapter, second, rest, text = match.groups()
        path = (int(chapter), int(second), *(int(p) for p in rest.split(".") if p))
        if re.match(r"-\s", text):
            continue
        if len(path) == 2 and path[1] == 0 and not indent:
            if chapters and path[0] != chapters[-1][0] + 1:
                continue
            chapters.append((path[0], appnote_title(lines, number, text)))
            chain = [(path[0],), (path[0], 0)]
            next_child = {(path[0],): 1, (path[0], 0): 1}
            continue
        if not chain:
            continue
        parent = path[:-1]
        if parent not in chain or path[-1] != next_child.get(parent, 1):
            continue
        title = appnote_title(lines, number, text)
        if not title or not title[0].isalpha():
            continue
        del chain[chain.index(parent) + 1:]
        chain.append(path)
        next_child[parent] = path[-1] + 1
        next_child[path] = 1
        headings.append(Heading("", path, title))
    return headings, chapters


class OutlineError(Exception):
    pass


def self_check(standard: Standard, chapters: list[tuple[int, str]], annexes: list[tuple[str, str]],
               headings: list[Heading]) -> None:
    """Fails when an outline is not a clean heading tree: duplicate or
    out-of-order references, a heading whose parent is missing, a gap in a
    numbered sequence, or a title that reads like a table row or a fragment."""
    problems: list[str] = []
    keys = [h.key for h in headings]
    if keys != sorted(keys) or len(set(keys)) != len(keys):
        problems.append("headings are not strictly increasing")
    present = set(keys) | {(0, n) for n, _ in chapters} | {(1, ord(l)) for l, _ in annexes}
    if standard.appnote:
        present |= {(0, n, 0) for n, _ in chapters}
    last_child: dict[tuple, int] = {}
    for h in headings:
        parent = parent_key(h)
        if parent not in present:
            problems.append(f"{render_ref(h)} has no parent heading")
        expected = last_child.get(parent, 0) + 1
        if h.path[-1] not in (expected, 1 if expected == 1 else expected):
            problems.append(f"{render_ref(h)} {h.title!r} skips from {expected - 1}")
        last_child[parent] = h.path[-1]
        if not standard.appnote and not acceptable_title(h.title):
            problems.append(f"{render_ref(h)} has a suspicious title {h.title!r}")
        if not h.title:
            problems.append(f"{render_ref(h)} has an empty title")
    for n, title in chapters:
        if not title:
            problems.append(f"chapter {n} has an empty title")
    if problems:
        shown = "\n  ".join(problems[:40])
        raise OutlineError(f"{standard.module}: {len(problems)} problems\n  {shown}")


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
    toc, toc_lines = toc_headings(lines)
    headings = toc + body_headings(lines, toc, toc_lines)
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
        self_check(standard, chapters, annexes, headings)
        (target / f"{standard.module}.lean").write_text(render_module(standard, chapters, annexes, headings))
        print(f"{standard.module}: {len(chapters)} chapters, {len(annexes)} annexes, {len(headings)} headings",
              file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))

---
name: docboss
description: Use when reading, extracting, converting, rendering, creating, or exploring Word documents (.docx, .docm, .dotx, .doc) with docboss, the from-scratch Rust DOCX and DOC engine with a CLI. Triggers include extracting text, Markdown, HTML or JSON from a Word file, converting DOC to DOCX, rendering pages to PNG, PPM, BMP or JPEG, exporting embedded images, composing a DOCX from Markdown, listing which fonts a document needs, and inspecting package internals (ZIP parts, compound file streams, XML parts, hexdumps, jq-style queries over the document model).
---

# docboss

A DOCX and DOC engine written from scratch in safe Rust: parse, extract text, Markdown, HTML and JSON, lay out and render pages, convert DOC to DOCX, and compose DOCX from Markdown. One core behind the `docboss` CLI. Clean-room from ECMA-376 (WordprocessingML, Open Packaging Conventions) and Microsoft's [MS-DOC] and [MS-CFB] specifications, no C dependencies. The reader is lenient: a ZIP with a broken central directory is recovered from its local headers, bad compound file chains are followed as far as they go, and every dropped or approximated item is reported (`docboss diagnostics`) instead of silently lost.

## Install

```bash
cargo install docboss-cli      # the `docboss` binary
```

Coding agents can install this skill with `docboss skill install` (writes `.claude/skills/docboss/SKILL.md`; `--global` for `~/.claude/skills`) or print it with `docboss skill print`.

## CLI

The format is detected from the file's bytes, never its name: a ZIP package reads as DOCX, a compound file with a `WordDocument` stream as DOC. Every read command takes `--password` for an encrypted DOC.

```bash
docboss info    report.docx                 # format, metadata, pages, sections, counts, diagnostics
docboss text    report.doc                  # plain text; --headers-footers, --comments, --no-notes, --no-labels
docboss md      report.docx                 # GitHub-flavored Markdown: headings from styles, lists, tables, footnotes
docboss md      report.docx --images media  # export images into media/ and link them; --embed-images for data: URIs
docboss html    report.docx --standalone    # semantic HTML; images embedded unless --images DIR
docboss json    report.docx                 # the full document model; --blocks for type/style/heading/list/text per block
docboss q       report.docx '.metadata.title'          # jq over the JSON model; -r raw strings, --blocks
docboss render  report.docx --page 1 -o p.png --scale 2   # 1-based pages; omit --page for all (p-%d.png)
docboss render  report.docx -o 'page-%d.jpg' --jpeg-quality 85   # .png/.ppm/.bmp/.jpg by extension, all cores
docboss images  report.docx -o out/         # embedded images at stored size and format
docboss convert report.doc -o report.docx   # fresh DOCX; also -o x.md / x.html / x.txt / x.json
docboss create md notes.md -o notes.docx --size a4     # CommonMark + GFM to DOCX, relative images resolved
docboss fonts   report.docx                 # each requested font -> the face it resolves to here
docboss diagnostics report.doc --layout     # what the reader (and layout) approximated or dropped
```

Container explorer:

```bash
docboss parts report.docx                   # ZIP entries with sizes; for .doc the compound file streams
docboss xml   report.docx word/styles.xml   # an XML part, re-indented (--raw as stored)
docboss hex   report.doc 1Table --length 256   # hexdump a part or stream (or the whole file)
```

Exit codes: 0 success, 1 document or I/O error, 2 usage error or invalid jq program. Errors go to stderr prefixed `docboss:`.

## Useful jq queries

```bash
docboss q doc.docx '.sections | length'
docboss q doc.docx '[.styles.styles[] | select(.kind == "paragraph") | .id]'
docboss q doc.docx --blocks -r '.[] | select(.heading_level != null) | .text'   # the outline
docboss q doc.docx '.diagnostics[] | .message'
```

## Rendering notes

Pages are laid out Word-style (line breaking, tab stops, list labels, keep and widow rules, tables with merged cells and repeated header rows, sections, columns, headers and footers with page numbers, footnotes). Fonts come from the system and from fonts the DOCX embeds; missing families fall back to metric-compatible faces (Calibri to Carlito, Cambria to Caladea, Arial to Liberation Sans or Arimo, Times New Roman to Liberation Serif or Tinos, Courier New to Liberation Mono or Cousine). `docboss fonts` shows what each family resolved to. WMF, EMF and TIFF images paint a placeholder and are reported as diagnostics. Right-to-left and complex-script shaping are not done yet.

## Python (coming)

A Python package (`import docboss`) exposing the same reader, output formats and renderer is being built; until it ships, drive the CLI with `subprocess`.

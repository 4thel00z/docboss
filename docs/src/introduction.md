# Introduction

docboss is a DOCX and DOC engine written from scratch in safe Rust against the
specifications: ECMA-376 for Office Open XML WordprocessingML, and Microsoft's
[MS-DOC], [MS-CFB], [MS-OLEPS], [MS-ODRAW] and [MS-OFFCRYPTO] for the Word
97-2003 binary format, its compound-file container and encryption. It has no C
dependencies and no bindings to another engine: the ZIP container, the XML
tokenizer, the compound-file reader, the font parsers, the layout engine and
the rasterizer are its own. One core sits behind every surface: the `docboss`
command-line tool, an interactive terminal explorer (`docboss tui`), a set of
Rust library crates, and a native Python extension.

## One model for two formats

A `.docx` file is a ZIP package of XML parts; a `.doc` file is a compound
file holding a binary stream of text, a piece table and formatting records.
Both readers produce the same document model (`docboss-model`): sections with
their page setup, paragraphs and tables, runs with their properties, a style
sheet that resolves effective formatting, list definitions that produce the
document's own labels, footnotes, endnotes, comments, headers, footers,
fields, bookmarks and media. Everything after reading (text, Markdown, HTML,
JSON, layout, rendering, DOCX writing) works on that model, so it behaves the
same for both formats. The format is detected from the leading bytes, never
from the file name.

## Leniency

Real-world files are damaged: truncated downloads, archivers that write a bad
central directory, converters that break FAT chains or leave dangling
relationships. docboss reads them anyway:

- A ZIP with a missing or broken central directory is read from its local
  headers; a damaged deflate stream keeps what inflated before the damage.
- A compound file with a looping or truncated sector chain is followed as far
  as it goes.
- XML that is not well formed still yields balanced elements.

Leniency never hides what it cost. Every approximated or dropped item is a
`Diagnostic` on the document: `docboss diagnostics` lists them, and the
libraries expose them as a value. An empty list means the reader took the
file as written.

## Scope

docboss reads, lays out, renders and writes word-processing documents. It
does not edit a document in place and does not run macros. The
[limitations](reference/limitations.md) page lists what is missing, and the
[conformance ledger](reference/conformance.md) records the status of every
clause of the standards it reads.

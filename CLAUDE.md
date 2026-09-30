# docboss

A DOCX and DOC engine written from scratch in Rust: parse, extract text and
Markdown, lay out and render pages, write DOCX. One core behind the CLI and
the Python bindings. Sister project of pdfboss; mirror its conventions.

## Layout

- `crates/docboss-model`: the document model both readers produce. Every
  consumer reads only this crate's types.
- `crates/docboss-zip`, `docboss-xml`, `docboss-docx`: ZIP container, XML
  tokenizer, WordprocessingML reader (ECMA-376).
- `crates/docboss-cfb`, `docboss-doc`: compound file ([MS-CFB]) and Word
  binary ([MS-DOC]) readers.
- `crates/docboss-crypt`: password-protected DOCX ([MS-OFFCRYPTO] Agile and
  Standard encryption) and the XOR obfuscation of DOC.
- `crates/docboss-core`: format sniffing and `Document::open` over both.
- `crates/docboss-aio`: async reads over files or http(s) URLs that fetch only
  the byte ranges they need.
- `crates/docboss-output`: text, Markdown and JSON from the model.
- `crates/docboss-font`, `docboss-layout`, `docboss-render`: font parsing,
  page layout, rasterization and image encoders.
- `crates/docboss-metafile`: WMF ([MS-WMF]) and EMF ([MS-EMF]) pictures
  played into paths, text and bitmaps, and the DIB decoder.
- `crates/docboss-mtef`: Equation Editor 3 equations (MTEF in an OLE
  object's `Equation Native` stream) read into the math model.
- `crates/docboss-write`: DOCX writer from the model.
- `crates/docboss-cli`, `docboss-py`, `docboss-tui`: the `docboss` binary,
  the Python extension and the terminal explorer.
- `ledger/`: the Lean conformance ledger and its gate.

## Specification citations

The Lean gate in `ledger/` scans `crates/`, `python/` and `tests/` for
citations and holds the ledger rows to them. Cite the clause a piece of code
or a test implements, in a doc comment or comment, in exactly these forms:

- `ECMA-376 Part 1 §17.3.1.29` (WordprocessingML, DrawingML; 5th edition
  numbering), `ECMA-376 Part 2 §9.1` (Open Packaging Conventions),
  `ECMA-376 Part 3 §7` (Markup Compatibility).
- `[MS-DOC] §2.5.1`, `[MS-CFB] §2.6.1`, `[MS-OSHARED] §2.3.1`,
  `[MS-ODRAW] §2.2.1`, `[MS-WMF] §2.3.3.5`, `[MS-EMF] §2.3.5.8`.
- `APPNOTE §4.3.7` for the PKWARE ZIP application note.

A citation in a file under `tests/` or inside a `#[cfg(test)] mod` counts as
a test citation. An `implemented` ledger row needs both a code citation and a
test citation.

## Reader rules

- Lenient: real files are damaged. Recover what can be read (a ZIP with a
  broken central directory is read from its local headers; a CFB with a bad
  FAT chain is followed as far as it goes) and report every approximated or
  dropped item as a `docboss_model::Diagnostic`, never silently.
- Never panic on input bytes. Bounds-check every offset; cap recursion,
  allocation sizes and loop counts derived from the file.
- Speed is a feature: zero-copy over the input where possible, `memchr` for
  scanning, no per-character allocation, parse independent parts on separate
  threads with `std::thread::scope`.

## Code style

- No leading underscores on any name.
- Guard clauses and early returns; no `if/else` arms where an early exit
  works.
- No `assert!`/`unwrap()` on input-derived values in library code; return
  errors or report diagnostics.
- Doc comments are short and factual; effectively no inline comments.
- Tests use real fixtures (committed small files, or files generated with
  `soffice --headless --convert-to`) and real components, not mocks.
- Conventional commits: `feat(docx): ...`, terse body with verification.

## Builds

Build into the shared cargo target from `~/.cargo/config.toml`; never set
`CARGO_TARGET_DIR` per worktree. Delete scratch renders after scoring them.

## Checks

`make ci` runs `cargo fmt --check`, clippy with `-D warnings`, the tests and
rustdoc with warnings denied.

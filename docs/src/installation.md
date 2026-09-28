# Installation

## Python

```bash
pip install docboss
```

Wheels are abi3 builds for CPython 3.12 and later, so one wheel covers every
later Python version. No Rust toolchain is needed to install one.

## Command-line tool

```bash
cargo install docboss-cli
```

This builds the `docboss` binary from crates.io with a stable Rust toolchain.
From a checkout, `make install` does the same for the working tree.

## Rust crates

Add the crates you need:

```bash
cargo add docboss-core docboss-output docboss-layout docboss-render docboss-write
cargo add docboss-aio --features http   # async and remote reads
```

`docboss-core` re-exports the document model, so most programs start from
`docboss_core::open`.

## Fonts for rendering

Rendering uses the fonts installed on the machine: the macOS, Linux and
Windows font directories, LibreOffice's bundled fonts, and any directory
named in `DOCBOSS_FONT_PATH` (`:`-separated). Fonts embedded in a DOCX are
used first. When a document names a font that is not installed, a
metric-compatible substitute is used where one exists (Carlito for Calibri,
Caladea for Cambria, Liberation Sans or Arimo for Arial and Helvetica,
Liberation Serif or Tinos for Times New Roman, Liberation Mono or Cousine for
Courier New), so line breaks and page counts stay close to Word's.
`docboss fonts report.docx` shows what each font resolved to.

## Coding agents

`docboss skill install` writes a skill document into
`./.claude/skills/docboss`, or into `~/.claude/skills/docboss` with `--global`,
so coding agents know the CLI and the Python API. `docboss skill print`
writes it to stdout.

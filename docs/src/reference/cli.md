# docboss CLI reference

Generated from `docboss --help` and each command's `--help`. Every command that reads a document detects DOCX or DOC from the bytes and accepts `--password`; where the help says so, it also takes an `http(s)://` URL, read with range requests. Errors go to stderr prefixed `docboss:`; document and I/O errors exit 1, usage errors and invalid jq programs exit 2.

```text
DOCX and DOC parsing, extraction, rendering and conversion

Usage: docboss <COMMAND>

Commands:
  info         Format, metadata, page setup, page count and content counts
  text         Extract plain text
  md           Convert to GitHub-flavored Markdown
  html         Convert to semantic HTML; images are embedded unless --images is given
  json         Dump the document model as JSON
  q            Run a jq program over the JSON form of the document
  render       Lay out and render pages to PNG, PPM, BMP or JPEG (picked by the -o extension)
  images       Export the document's images at their stored size and format
  parts        List the ZIP entries or compound file streams of the container
  hex          Hexdump the whole file or one part of it
  xml          Print an XML part of a DOCX, indented
  convert      Convert to a fresh DOCX, or to .md, .html, .txt or .json, by the -o extension
  create       Create a new DOCX
  fonts        The fonts the document names and the faces they resolve to here
  diagnostics  Everything the lenient reader approximated or dropped
  skill        Install or print the skill document for coding agents
  tui          Explore the document interactively in the terminal
  help         Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
```

## docboss info

```text
Format, metadata, page setup, page count and content counts

Usage: docboss info [OPTIONS] <FILE>

Arguments:
  <FILE>  Path or http(s):// URL of the .docx or .doc file; the format is detected from its bytes. A URL is read with range requests

Options:
      --password <PASSWORD>  Password for an encrypted DOC or DOCX
  -h, --help                 Print help
```

## docboss text

```text
Extract plain text

Usage: docboss text [OPTIONS] <FILE>

Arguments:
  <FILE>  Path or http(s):// URL of the .docx or .doc file; the format is detected from its bytes. A URL is read with range requests

Options:
      --password <PASSWORD>  Password for an encrypted DOC or DOCX
      --headers-footers      Include each section's headers and footers
      --no-notes             Leave out footnotes and endnotes
      --comments             Include comments
      --no-labels            Leave list labels (1., a), bullets) out
  -o, --out <OUT>            Write to a file instead of stdout
  -h, --help                 Print help
```

## docboss md

```text
Convert to GitHub-flavored Markdown

Usage: docboss md [OPTIONS] <FILE>

Arguments:
  <FILE>  Path or http(s):// URL of the .docx or .doc file; the format is detected from its bytes. A URL is read with range requests

Options:
      --password <PASSWORD>  Password for an encrypted DOC or DOCX
      --headers-footers      Include each section's headers and footers
      --no-notes             Leave out footnotes and endnotes
      --comments             Include comments
      --images <IMAGES>      Export images into this directory and link them from the Markdown
      --embed-images         Embed images as data: URIs
      --title                Start with the metadata title as a level-one heading
      --page-breaks          Write page breaks as thematic breaks (---)
  -o, --out <OUT>            Write to a file instead of stdout
  -h, --help                 Print help
```

## docboss html

```text
Convert to semantic HTML; images are embedded unless --images is given

Usage: docboss html [OPTIONS] <FILE>

Arguments:
  <FILE>  Path or http(s):// URL of the .docx or .doc file; the format is detected from its bytes. A URL is read with range requests

Options:
      --password <PASSWORD>  Password for an encrypted DOC or DOCX
      --headers-footers      Include each section's headers and footers
      --no-notes             Leave out footnotes and endnotes
      --comments             Include comments
      --standalone           Write a whole HTML document instead of a body fragment
      --images <IMAGES>      Export images into this directory and link them instead of embedding
  -o, --out <OUT>            Write to a file instead of stdout
  -h, --help                 Print help
```

## docboss json

```text
Dump the document model as JSON

Usage: docboss json [OPTIONS] <FILE>

Arguments:
  <FILE>  Path or http(s):// URL of the .docx or .doc file; the format is detected from its bytes. A URL is read with range requests

Options:
      --password <PASSWORD>  Password for an encrypted DOC or DOCX
      --blocks               The compact per-block view: type, style, heading level, list label, text
      --compact              One line instead of indented output
  -o, --out <OUT>            Write to a file instead of stdout
  -h, --help                 Print help
```

## docboss q

```text
Run a jq program over the JSON form of the document

Usage: docboss q [OPTIONS] <FILE> <PROGRAM>

Arguments:
  <FILE>     Path or http(s):// URL of the .docx or .doc file; the format is detected from its bytes. A URL is read with range requests
  <PROGRAM>  The jq program, e.g. '.metadata.title' or '.sections[].blocks | length'

Options:
      --password <PASSWORD>  Password for an encrypted DOC or DOCX
  -r, --raw                  Print strings without quotes
      --blocks               Query the compact per-block view instead of the full model
  -h, --help                 Print help
```

## docboss render

```text
Lay out and render pages to PNG, PPM, BMP or JPEG (picked by the -o extension)

Usage: docboss render [OPTIONS] <FILE>

Arguments:
  <FILE>  Path or http(s):// URL of the .docx or .doc file; the format is detected from its bytes. A URL is read with range requests

Options:
      --password <PASSWORD>          Password for an encrypted DOC or DOCX
      --page <PAGE>                  Page number (1-based); every page when omitted
      --scale <SCALE>                Pixels per point: 1.0 is 72 dpi [default: 1]
  -o, --out <OUT>                    Output path; %d is replaced by the page number [default: page-%d.png]
      --jpeg-quality <JPEG_QUALITY>  JPEG quality, 1 to 100
  -h, --help                         Print help
```

## docboss images

```text
Export the document's images at their stored size and format

Usage: docboss images [OPTIONS] <FILE>

Arguments:
  <FILE>  Path or http(s):// URL of the .docx or .doc file; the format is detected from its bytes. A URL is read with range requests

Options:
      --password <PASSWORD>  Password for an encrypted DOC or DOCX
  -o, --out <OUT>            Output directory [default: .]
  -h, --help                 Print help
```

## docboss parts

```text
List the ZIP entries or compound file streams of the container

Usage: docboss parts <FILE>

Arguments:
  <FILE>  Path to the .docx or .doc file

Options:
  -h, --help  Print help
```

## docboss hex

```text
Hexdump the whole file or one part of it

Usage: docboss hex [OPTIONS] <FILE> [PART]

Arguments:
  <FILE>  Path to the .docx or .doc file
  [PART]  A ZIP entry (word/document.xml) or compound file stream (1Table)

Options:
      --offset <OFFSET>  First byte to dump [default: 0]
      --length <LENGTH>  Number of bytes to dump; to the end when omitted
      --width <WIDTH>    Bytes per row [default: 16]
  -h, --help             Print help
```

## docboss xml

```text
Print an XML part of a DOCX, indented

Usage: docboss xml [OPTIONS] <FILE> [PART]

Arguments:
  <FILE>  Path to the .docx file
  [PART]  The part, e.g. word/document.xml [default: word/document.xml]

Options:
      --raw   Print the part as stored, without re-indenting
  -h, --help  Print help
```

## docboss convert

```text
Convert to a fresh DOCX, or to .md, .html, .txt or .json, by the -o extension

Usage: docboss convert [OPTIONS] --out <OUT> <FILE>

Arguments:
  <FILE>  Path or http(s):// URL of the .docx or .doc file; the format is detected from its bytes. A URL is read with range requests

Options:
      --password <PASSWORD>  Password for an encrypted DOC or DOCX
  -o, --out <OUT>            Output path
  -h, --help                 Print help
```

## docboss create md

```text
Compose CommonMark + GFM Markdown into a DOCX

Usage: docboss create md [OPTIONS] --out <OUT> <FILE>

Arguments:
  <FILE>  The Markdown file; relative image paths resolve against its directory

Options:
  -o, --out <OUT>      Output DOCX path
      --size <SIZE>    Page size [default: letter] [possible values: letter, a4]
      --title <TITLE>  Document title; the first heading's text by default
  -h, --help           Print help
```

## docboss fonts

```text
The fonts the document names and the faces they resolve to here

Usage: docboss fonts [OPTIONS] <FILE>

Arguments:
  <FILE>  Path or http(s):// URL of the .docx or .doc file; the format is detected from its bytes. A URL is read with range requests

Options:
      --password <PASSWORD>  Password for an encrypted DOC or DOCX
  -h, --help                 Print help
```

## docboss diagnostics

```text
Everything the lenient reader approximated or dropped

Usage: docboss diagnostics [OPTIONS] <FILE>

Arguments:
  <FILE>  Path or http(s):// URL of the .docx or .doc file; the format is detected from its bytes. A URL is read with range requests

Options:
      --password <PASSWORD>  Password for an encrypted DOC or DOCX
      --layout               Also lay the document out and report what layout approximated
  -h, --help                 Print help
```

## docboss tui

```text
Explore the document interactively in the terminal

Usage: docboss tui [OPTIONS] <FILE>

Arguments:
  <FILE>  Path or http(s):// URL of the .docx or .doc file; the format is detected from its bytes. A URL is read with range requests

Options:
      --password <PASSWORD>  Password for an encrypted DOC or DOCX
  -h, --help                 Print help
```

## docboss skill install

```text
Install the skill for coding agents: into ./.claude/skills/docboss (this project), or with --global into ~/.claude/skills/docboss

Usage: docboss skill install [OPTIONS]

Options:
  -g, --global  Install into ~/.claude/skills instead of ./.claude/skills
  -h, --help    Print help
```

## docboss skill print

```text
Print the skill document to stdout

Usage: docboss skill print

Options:
  -h, --help  Print help
```

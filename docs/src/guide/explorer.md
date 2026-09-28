# Exploring document internals

## Containers

```bash
docboss parts report.docx                        # ZIP entries with stored and unpacked sizes
docboss parts legacy.doc                         # compound-file streams
docboss xml report.docx word/styles.xml          # one XML part, indented (--raw as stored)
docboss hex legacy.doc WordDocument --length 64  # hexdump of a part
docboss hex legacy.doc SummaryInformation        # control characters in names are optional
```

Compound-file stream names that start with a control character print as
`\x05SummaryInformation` and can be addressed that way, without the prefix, or
case-insensitively.

## The model

```bash
docboss json report.docx | head
docboss q report.docx '.styles.styles | map(.id)'
docboss fonts report.docx
docboss diagnostics report.docx
docboss diagnostics report.docx --layout
```

`diagnostics` lists everything the reader approximated or dropped, with the
part or stream it came from; `--layout` adds what layout approximated, such as
fonts that fell back or pictures drawn as placeholders.

## The terminal explorer

```bash
docboss tui report.docx
```

The explorer has a tree of the document (sections, paragraphs, tables and
runs, plus headers and footers, notes, comments, styles, numbering, media,
fonts, the container and diagnostics), an inspector with each node's direct
and resolved properties and its style chain, an XML view of the selected
part, a Markdown view, a page preview drawn with half-block characters, and a
hex pane.

| Keys | Action |
|---|---|
| `j` `k` `h` `l`, arrows | move, collapse, expand |
| Enter | toggle a node, or open a part's XML |
| `g` `G` PgUp PgDn | jump |
| Tab | cycle focus |
| `i` `x` `m` `p` | inspector, XML, Markdown, page preview |
| `[` `]` | previous and next page |
| `/` `n` `N` | search the whole document, next and previous match |
| `y` | copy menu: text, inspector, a matching `docboss` command, hexdump |
| `<` `>` | resize the tree |
| `?` | help |
| `q`, Esc, Ctrl-C | quit |

It needs an interactive terminal and exits with an error otherwise.

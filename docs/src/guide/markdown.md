# Markdown, HTML and JSON

## Markdown

```bash
docboss md report.docx
docboss md report.docx --images images -o report.md   # export images and link them
docboss md report.docx --embed-images                 # data: URIs instead
docboss md report.docx --title --page-breaks
```

The Markdown is GitHub-flavored:

- Headings come from the paragraph's outline level or a `Heading N` or
  `Title` style.
- Bold, italic and strikethrough spans are merged across adjacent runs, with
  whitespace kept outside the markers. A paragraph set entirely in a
  monospace font, or in a code style, becomes a fenced code block.
- Lists nest by numbering level; bullets become `-`, numbered items `1.`.
- Tables become pipe tables; merged cells, nested tables and multi-paragraph
  cells are flattened with `<br>`.
- Footnotes and endnotes become `[^1]` references with definitions at the
  end; hyperlinks and `HYPERLINK` fields become links.
- Images become `![description](name)`; `--images DIR` writes the files and
  links them there.

```python
import docboss

doc = docboss.Document("report.docx")
markdown = doc.extract_markdown(images="reference", image_prefix="images/")
inline = doc.extract_markdown(images="embed")
bare = doc.extract_markdown(images="omit", title=True, page_breaks=True)
```

## HTML

```bash
docboss html report.docx --standalone -o report.html
docboss html report.docx --images images -o fragment.html
```

HTML output is semantic: `h1` to `h6`, `p`, `strong`, `em`, `u`, `s`, `sup`,
`sub`, `ul` and `ol` with `start` attributes, tables with `colspan` and
`rowspan` from merged cells, links, images and a footnotes section. Without
`--standalone` it is a body fragment. Images are embedded as data: URIs
unless `--images` exports them.

```python
import docboss

html = docboss.Document("report.docx").extract_html(standalone=True)
```

## JSON

```bash
docboss json report.docx            # the whole document model
docboss json report.docx --blocks   # one object per block
docboss q report.docx '.sections | length'
docboss q report.docx --blocks -r '.[] | select(.kind == "heading") | .text'
```

`docboss q` runs a jq program (through jaq) over the same JSON, so the model
can be queried without writing code. In Python, `Document.to_json(pretty=True)`
returns the model as a string.

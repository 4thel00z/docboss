# Extracting text

```bash
docboss text report.docx
docboss text report.docx --headers-footers --comments
docboss text legacy.doc --no-labels -o legacy.txt
```

Plain text has one line per paragraph. List paragraphs start with the label
the document's numbering gives them (`1.`, `a)`, `•`), computed from the list
definitions, their start values and overrides, the way Word numbers them.
Table rows become one line each with tab-separated cells. Footnote and
endnote references become `[1]` and `[i]`, and the notes' text follows the
main text; `--no-notes` leaves both out. `--headers-footers` writes each
section's headers before it and its footers after it, and `--comments` marks
comment anchors `[c1]` and appends the comments with their authors.

What is left out, as ECMA-376 and [MS-DOC] define it: hidden text (`vanish`),
deleted revisions (inserted ones are kept), and field instructions; a field
contributes its cached result, so a table of contents or a `PAGE` field
reads as the file shows it. Text boxes are part of the text, written after
the paragraph that anchors them.

```python
import docboss

doc = docboss.Document("report.docx")
text = doc.extract_text(headers_footers=True, notes=True, comments=False, list_labels=True)
```

## Many files at once

```python
import docboss

texts = docboss.extract_texts(["report.docx", "legacy.doc"], threads=8)
lenient = docboss.extract_texts(["report.docx", "not-a-document.txt"], strict=False)
print(lenient[1])   # None: that file failed, the others still came back
```

`extract_texts` reads every file on a Rust thread pool in one call, without
the GIL, and returns the texts in input order. With `strict=True` (the
default) the first failure raises `DocbossError`; with `strict=False` a
failed file gives `None`. It is the fastest way to feed a pipeline.

## Blocks

For retrieval pipelines, the block view keeps each paragraph's role:

```bash
docboss json report.docx --blocks
```

```python
import docboss

for block in docboss.Document("report.docx").blocks():
    print(block.kind, block.heading_level, block.list_label, block.text)
```

`kind` is `paragraph`, `heading`, `list_item` or `table`; headings come from
outline levels and heading styles, list items carry their level and label.
Empty paragraphs are left out.

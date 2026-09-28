# docboss

A DOCX and DOC engine written from scratch in Rust: parse, extract text,
Markdown and HTML, lay out and render pages, write DOCX. One core behind the
CLI and the Python bindings.

```python
import docboss

doc = docboss.Document("report.docx")      # or .doc, or Document(data=raw_bytes)
text = doc.extract_text()
md   = doc.extract_markdown()
png  = doc.render(0, scale=2.0)
```

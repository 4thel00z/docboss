# Changelog

## 0.1.0 (2026-09-28)

### Features

* **docx:** read DOCX, DOCM, DOTX and DOTM packages (ECMA-376 Parts 1 to 3, transitional and strict namespaces): paragraphs, runs, tables, sections, styles, numbering, footnotes, endnotes, comments, headers, footers, fields, bookmarks, revisions, drawings and text boxes, embedded fonts, themes and metadata, with independent parts parsed on separate threads.
* **zip, xml:** an in-tree ZIP reader and deterministic writer with ZIP64 and recovery from local headers, and a zero-copy XML tokenizer.
* **doc:** read Word 97-2003 binary documents ([MS-DOC], [MS-CFB], [MS-OLEPS], [MS-ODRAW]): piece table, character and paragraph formatting, stylesheet, lists, tables, sections, notes, comments, fields, bookmarks, pictures and text boxes; RC4 and RC4 CryptoAPI decryption; Word 2, 6 and 95 files as text.
* **crypt:** decrypt password-protected DOCX (Agile and Standard encryption, [MS-OFFCRYPTO]).
* **core:** one `open`/`read` over both formats, detected from the bytes.
* **output:** plain text, GitHub-flavored Markdown, semantic HTML, JSON and a per-block view.
* **font, layout, render:** TrueType and CFF parsing with system discovery and metric-compatible substitution, Word-like page layout, an anti-aliased rasterizer and PNG, PPM, BMP and JPEG encoders.
* **write:** DOCX from the model, a builder API and Markdown to DOCX, with deterministic output.
* **aio:** async reads over files and HTTP range requests.
* **cli:** the `docboss` binary: info, text, md, html, json, q, render, images, parts, hex, xml, convert, create md, fonts, diagnostics, skill and tui.
* **tui:** a terminal explorer with tree, inspector, XML, hex, Markdown and page preview panes.
* **py:** the `docboss` Python package: `Document`, `AsyncDocument`, `extract_texts`, `detect` and `md.to_docx`.
* **ledger:** a Lean 4 conformance ledger over ECMA-376, [MS-DOC], [MS-CFB], [MS-OLEPS], [MS-OSHARED], [MS-ODRAW] and APPNOTE, with a gate that fails the build on a false claim.

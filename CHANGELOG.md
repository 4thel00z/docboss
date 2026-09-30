# Changelog

## Unreleased

### Features

* **layout, render:** floating drawings placed by `wp:align` and every `relativeFrom` base; text box content clipped to the box less its insets; dashed, capped and joined outlines and dashed borders; page borders with their offsets, shadows and art-border reporting; preset and custom shape geometry with line ends, flips and rotation; groups, drawing canvases and gradient fills; turned and vertical text; right-to-left layout with the Unicode bidirectional algorithm and Arabic and Hebrew shaping.
* **render:** SmartArt drawn from its saved drawing, charts from their cached values (and listed as tables in text and Markdown), WMF and EMF pictures played through the new `docboss-metafile` crate.
* **layout:** Office Math read, laid out and extracted (Unicode linear format in text, LaTeX in Markdown).
* **crypt, doc:** XOR-obfuscated DOC files open with their password.
* **doc:** Word 6 and Word 95 files are read with their character, paragraph and style formatting, sections, headers and footers, tables and pictures; Word 2 files are detected.

### Bug Fixes

* **json:** documents with comment ranges serialize (`docboss json` failed on them).
* **layout:** inline pictures move with their run's position (DOC equations sat 3 pt high); horizontal borders count into table row heights, and tables in Word 2007-mode DOCX move left by their first cell's margin, as LibreOffice places them.
* **doc:** compound files whose directory, FAT or header is damaged or cut off give their text from the FIB found at the start of a sector (142 of 150 damaged DOC files read, from 94); percentage shading patterns mix their colors.

### Performance

* **doc:** picture bytes shared between the BLIP store and the media list and stories built one paragraph at a time (peak heap on a 6.2 MB file 41.9 to 32.6 MB); run formatting and list levels cached (the 1,188-page benchmark DOC 155 to 105 ms).

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

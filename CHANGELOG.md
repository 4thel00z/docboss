# Changelog

## [0.2.1](https://github.com/4thel00z/docboss/compare/v0.2.0...v0.2.1) (2026-10-01)


### Documentation

* publish the book to docboss.dev/docs through GitHub Pages ([9b83b94](https://github.com/4thel00z/docboss/commit/9b83b943578522ed80efee609acbb1835e5de7cf))

## [0.2.0](https://github.com/4thel00z/docboss/compare/v0.1.0...v0.2.0) (2026-10-01)


### Features

* **aio:** range-fetching DOCX and DOC reads over files and HTTP ([d9560b9](https://github.com/4thel00z/docboss/commit/d9560b96da12928939a2db2288bd09d1f028340a))
* **bench:** speed, recall, parallel, memory and robustness benchmarks against the Python, pandoc, LibreOffice, antiword and catdoc readers ([d8c9677](https://github.com/4thel00z/docboss/commit/d8c9677027993968d8b61209175541d0e65054b5))
* **bench:** time page rendering on a fixed corpus sample ([edd4ff8](https://github.com/4thel00z/docboss/commit/edd4ff8057d3a31097f5a0f65c46a21b4f849e97))
* **cfb:** compound file and OLE property set reader ([7faad66](https://github.com/4thel00z/docboss/commit/7faad6685d6705aa62f8900c7a9fe452651cf52f))
* **cfb:** Shift JIS, GBK, Unified Hangul and Big5 code pages ([bc42607](https://github.com/4thel00z/docboss/commit/bc426070eeebf1e27da260d00ebac1874d9c6a07))
* **cli:** every read command accepts http(s) URLs, with a coverage minimap ([436154a](https://github.com/4thel00z/docboss/commit/436154a633ed334bee4b8c8a439506d237c03c24))
* **cli:** the docboss binary: info, text, md, html, json, q, render, images, convert, create md, fonts, diagnostics and the parts/hex/xml explorer ([5c7c017](https://github.com/4thel00z/docboss/commit/5c7c0170411becb501b7d22bfc9bff548b435582))
* **core:** open DOCX and DOC through one call, format detected from the bytes ([f416adb](https://github.com/4thel00z/docboss/commit/f416adb32701008598a104e7b569dbba6509c100))
* **crypt:** decrypt password-protected DOCX, Agile and Standard [MS-OFFCRYPTO] encryption ([5636699](https://github.com/4thel00z/docboss/commit/563669950c3252da294a5f63956a3b653021d5bf))
* **crypt:** open XOR-obfuscated DOC files with their password ([2ff06a6](https://github.com/4thel00z/docboss/commit/2ff06a683856b7c35afa549048bee40a2a623fe1))
* **doc:** number Word 6 and 95 paragraphs from their ANLD ([cc90f6f](https://github.com/4thel00z/docboss/commit/cc90f6f206b718da76b60edd62227462759d4ba7))
* **doc:** read Word 6 and 95 formatting, sections, headers and tables ([abc9a43](https://github.com/4thel00z/docboss/commit/abc9a430dbe473c4676bd4720791625e043a7abc))
* **doc:** text boxes and header anchors ([009fac5](https://github.com/4thel00z/docboss/commit/009fac524741aab8782cc1a8a74517733e0a8b59))
* **doc:** Word 97-2003 binary reader into the document model ([72f53b2](https://github.com/4thel00z/docboss/commit/72f53b2df73e6f2d78ab079eccd7092a6d47c03b))
* **docx:** resolve shape colors through the theme and wps:style ([50ecf44](https://github.com/4thel00z/docboss/commit/50ecf44241b7c35680995e0777342e9e3fc8f98a))
* **docx:** WordprocessingML reader into the document model ([e8f9f38](https://github.com/4thel00z/docboss/commit/e8f9f388601d0732a5333c33c71d5c6d4543e220))
* **font:** TrueType and OpenType parsing, kerning and font discovery ([9c55a0c](https://github.com/4thel00z/docboss/commit/9c55a0caea66e1331591a651dde275e62a889771))
* **layout:** draw page border shadows and leave out art borders ([2a49109](https://github.com/4thel00z/docboss/commit/2a49109f6fdb422ff3783b4cc9a9c8c02e4cc928))
* **layout:** draw page borders ([5653b55](https://github.com/4thel00z/docboss/commit/5653b55420d423123fcc171d3f4c0f46767627c9))
* **layout:** lay out and paint text boxes inside their shapes ([8a8821e](https://github.com/4thel00z/docboss/commit/8a8821ecbbe1bdf251db678b6ffd755eefde68a5))
* **layout:** lay out right-to-left text and shape Arabic and Hebrew ([8691f69](https://github.com/4thel00z/docboss/commit/8691f69aeb466750e714cf7f7d4aa6443f122002))
* **layout:** paragraphs, tabs, lists, tables, sections, headers, footers and footnotes ([120ffd3](https://github.com/4thel00z/docboss/commit/120ffd32b08829c2dc1be5150a2f4c0f3ed83566))
* **layout:** place floating drawings by wp:align and every relativeFrom base ([d45b4d5](https://github.com/4thel00z/docboss/commit/d45b4d5243c40d0de39b73ea04ca5fe0e1e5a73b))
* **layout:** read, lay out and extract Office Math ([f6872fd](https://github.com/4thel00z/docboss/commit/f6872fd8f2f3cc2d1f744eaf602892b015dcb416))
* **layout:** wrap text around floating pictures, frames and floating tables ([542d5e5](https://github.com/4thel00z/docboss/commit/542d5e5aa1705442ef0c2b5129c9f0db4dbe3e7c))
* **ledger:** kernel-checked CRC-32 and list label references with Rust vector tests; Makefile and CI ([6717ab1](https://github.com/4thel00z/docboss/commit/6717ab1189b19ac91e82f99bc288f18037e40b6d))
* **ledger:** Lean conformance gate over ECMA-376, [MS-DOC], [MS-CFB], [MS-OLEPS], [MS-OSHARED], [MS-ODRAW] and APPNOTE ([d2cdc7f](https://github.com/4thel00z/docboss/commit/d2cdc7f6e4f3fd87874a333a46941d3b097103e1))
* **ledger:** reconcile the rows with the merged readers ([e0ff8e0](https://github.com/4thel00z/docboss/commit/e0ff8e0d524274daef0800d47c5ccd0b07297a5b))
* **mtef:** give Equation Editor 3 objects their equation as text and LaTeX ([0417812](https://github.com/4thel00z/docboss/commit/0417812cf06b1adb27af2f1c10fa6dff2be9ca4f))
* **output:** text, Markdown, HTML and JSON output from the document model ([c7a9c1b](https://github.com/4thel00z/docboss/commit/c7a9c1bc5a3fcaf01dbacddd80ca1490375da665))
* **py:** AsyncDocument over docboss-aio for files, bytes and HTTP range reads ([44ed8cb](https://github.com/4thel00z/docboss/commit/44ed8cbcc64d5b47f89311f72f14f226b4e5fb6c))
* **py:** Python bindings over docboss-core with extraction, rendering and DOCX writing ([9f60638](https://github.com/4thel00z/docboss/commit/9f60638ba1506e4ca28a3945e447a5b9b95806f8))
* **render:** clip overflowing text box content to the box less its insets ([b917764](https://github.com/4thel00z/docboss/commit/b91776420c8b0f982107350a16f699194130b891))
* **render:** coverage rasterizer, glyph cache, images and PNG/PPM/BMP/JPEG encoders ([958074b](https://github.com/4thel00z/docboss/commit/958074b83c654b426067f12cd4c577be68337423))
* **render:** dash, cap and join shape outlines and dashed borders ([f3a145a](https://github.com/4thel00z/docboss/commit/f3a145a15e705180ed5c243fc43ee076fa34c1cd))
* **render:** draw charts from their cached values and list them as tables ([982e206](https://github.com/4thel00z/docboss/commit/982e206d4300db81b3f2fd47e73e45dd5f4ca015))
* **render:** draw groups, drawing canvases and gradient fills ([6c5ee9c](https://github.com/4thel00z/docboss/commit/6c5ee9c507ceb6ca8105d49f113aba3613a570d7))
* **render:** draw preset and custom shape geometry with line ends ([f604506](https://github.com/4thel00z/docboss/commit/f604506c392e1de7fccb1b49521fbcc047f9924a))
* **render:** draw SmartArt from its saved drawing and extract its text ([23adda3](https://github.com/4thel00z/docboss/commit/23adda345b22de0410b22fc006f7358e21ab1bdd))
* **render:** play WMF and EMF pictures instead of drawing placeholders ([2d6d512](https://github.com/4thel00z/docboss/commit/2d6d51291a6de9348fa795d4e4b0e95fb28cedd1))
* **render:** sample document, criterion bench, fidelity script; cap table spans and nesting ([75c35d5](https://github.com/4thel00z/docboss/commit/75c35d511f6d03182f258aa4463baa646a3f01fe))
* **render:** turn text with its shape and lay out vertical text ([6f259f6](https://github.com/4thel00z/docboss/commit/6f259f6672216d414ad582e43947b0d102b29197))
* **tui:** terminal explorer for DOCX and DOC files ([45d81e6](https://github.com/4thel00z/docboss/commit/45d81e6d1b4b50aa2ab4408737de32caefb6ab5e))
* **write:** compose DOCX from CommonMark and GFM ([eaf0033](https://github.com/4thel00z/docboss/commit/eaf00335b30db6a9f15583bd289cd66e0ef1f670))
* **write:** serialize the document model to DOCX with a composition API ([6628362](https://github.com/4thel00z/docboss/commit/6628362bc68886ecd4a54abd4b2798ec835a493b))
* **xml:** zero-copy namespace-resolving XML pull tokenizer ([b10ab6a](https://github.com/4thel00z/docboss/commit/b10ab6a4365f2cf7c1e09cdbaccaffc8ab561c92))
* **zip:** in-tree ZIP reader with ZIP64, data descriptors, CRC-32 and local-header recovery ([62ade96](https://github.com/4thel00z/docboss/commit/62ade96a1e9ee4b574cbde26d89f516b5a0bd309))


### Bug Fixes

* **aio:** count the whole download when a server ignores Range ([2602976](https://github.com/4thel00z/docboss/commit/2602976dfc54d1685909cb013c8fb4b751f1983c))
* **aio:** fetch OLE embeddings so async reads keep DOCX equations ([7d903d3](https://github.com/4thel00z/docboss/commit/7d903d3e4f2908786881537280f327931b55cc90))
* **aio:** keep cached spans disjoint, never refetch cached bytes, parse on a large stack ([02564c1](https://github.com/4thel00z/docboss/commit/02564c1d8d72db92c61a63403b25d53ca0f5ea16))
* **doc:** floating pictures, decryption, Word 2/6/95 text, corpus survival ([dd950e8](https://github.com/4thel00z/docboss/commit/dd950e841a6daf0649a8ee733952d0490c6fc42e))
* **doc:** let zeroed TC80 borders fall back to the row's table borders ([2f78cdd](https://github.com/4thel00z/docboss/commit/2f78cdd87dc19fa15bb3a38fb901ce2293f39172))
* **doc:** read sprmTDxaGapHalf as the cell margins and place tables at their first cell edge ([456cd22](https://github.com/4thel00z/docboss/commit/456cd22adb969bfd3d21561c14427dbd23de3afd))
* **doc:** read style and Word 6.0 ANLDs, wrap polygons and hidden shapes ([48531fa](https://github.com/4thel00z/docboss/commit/48531fa6ee8fb9ff35658eb22cb34219b9ce278c))
* **doc:** recover text from compound files with a lost directory or header ([37864d4](https://github.com/4thel00z/docboss/commit/37864d480a664085aa36128cea407dc858610415))
* fill the new Drawing.text_box field at every construction site ([3864f91](https://github.com/4thel00z/docboss/commit/3864f917aafd8efe4a2308ebeac72e6dc9a39061))
* **font:** search LibreOffice's bundled fonts and macOS supplementary fonts for substitutes ([4a16833](https://github.com/4thel00z/docboss/commit/4a16833822074139d84572b606f5de11bfdfa0d6))
* **json:** serialize comment range markers as objects with an id ([9c115e0](https://github.com/4thel00z/docboss/commit/9c115e098770a415248079865abf607ac3ca0fec))
* **layout:** add Word's external leading to single-spaced lines ([d0a7e13](https://github.com/4thel00z/docboss/commit/d0a7e13956f100e230503ff15d0337a96bec915f))
* **layout:** apply the table style's paragraph and run properties inside its cells ([ea482a4](https://github.com/4thel00z/docboss/commit/ea482a438d291b43c7922a8e8c091279be3dc630))
* **layout:** count horizontal borders into row heights and outdent Word 2007 tables ([8f1f022](https://github.com/4thel00z/docboss/commit/8f1f0229438316eedf7f00adee52c4b5cc4bbe64))
* **layout:** draw auto-colored text white over dark shading ([ddbfd3d](https://github.com/4thel00z/docboss/commit/ddbfd3ded574df658cccdddb1542fe6c80cd41be))
* **layout:** draw no border inside a vertically merged cell ([7695eaf](https://github.com/4thel00z/docboss/commit/7695eaf783564f6221352989d79cb3189166a8ab))
* **layout:** draw one border around a group of paragraphs with equal borders ([c8c30dc](https://github.com/4thel00z/docboss/commit/c8c30dc7a332633ef533f98f9c88d6a8f97393ac))
* **layout:** draw symbol-font bullets through their Unicode form when the font is missing ([6174d98](https://github.com/4thel00z/docboss/commit/6174d9818a231100efb9b9a274e04881a026f8e1))
* **layout:** frame tables whole, paint nested floats, move floats with their anchor ([637b7e1](https://github.com/4thel00z/docboss/commit/637b7e1068d9c5bc9a5534ef98c7f2daa6abe7c9))
* **layout:** keep a table's grid widths when they exceed the text width ([47962a1](https://github.com/4thel00z/docboss/commit/47962a1eda54bb3025e59f64047691fb2e92fcca))
* **layout:** raise and lower inline pictures with their run position ([61771d1](https://github.com/4thel00z/docboss/commit/61771d1707e297bbaa4a945003478d25ec102702))
* **layout:** scale only a line's text by auto line spacing, not its pictures ([d6b926a](https://github.com/4thel00z/docboss/commit/d6b926a059b1475bcb82e7e0dbb7cc68a94faf74))
* **layout:** show PAGE fields in the section's page number format ([3bb1dda](https://github.com/4thel00z/docboss/commit/3bb1dda0576e08661759202f358b0c1dd4ea5ab0))
* **layout:** size spAutoFit text boxes to their text in both directions ([2ec8ba7](https://github.com/4thel00z/docboss/commit/2ec8ba78d77d3f22e165c82d97935ff7cd1736c7))
* **ledger:** outlines hold only the specifications' numbered headings ([263745b](https://github.com/4thel00z/docboss/commit/263745b9ce63ff3ed205722048a8e4f39799f86b))
* **mtef:** attach scripts to one object and dispatch every template ([e716690](https://github.com/4thel00z/docboss/commit/e716690eb7713bb3822151b6037c76b6fb2491be))
* **output:** drop a needless borrow clippy flags ([e9aa7a5](https://github.com/4thel00z/docboss/commit/e9aa7a58f6aecdb60fa65f5a899031424b81c04b))
* **output:** write text-box content after its anchor paragraph ([185e205](https://github.com/4thel00z/docboss/commit/185e205033d673aa1e8a5687213e14788c4c2236))
* **write:** give tight and through wraps their required polygon ([52f8d45](https://github.com/4thel00z/docboss/commit/52f8d45d7a991728af541cc161f77ca54b91155e))
* **zip,xml:** keep partial inflate output, never slice entities inside a character ([e02ac9c](https://github.com/4thel00z/docboss/commit/e02ac9c2fafb094c7824056ca5ceef2ec0faa469))


### Performance Improvements

* **doc:** cache run formatting and list levels for large documents ([9798d4a](https://github.com/4thel00z/docboss/commit/9798d4a217c86190db544a5e44d32a18e69865c7))
* **doc:** key image dedup on length and the ends of the data ([baa23c2](https://github.com/4thel00z/docboss/commit/baa23c24b0cbf7391c26eae8a57a1e67ac580ecc))
* **doc:** share picture bytes and stream stories to lower peak memory ([4b8abc9](https://github.com/4thel00z/docboss/commit/4b8abc9269bee5f12cc97644645a39e5afe20c41))
* **model:** resolve each list instance once and binary-search numbering lookups ([6f1e1fc](https://github.com/4thel00z/docboss/commit/6f1e1fce3170fe9f21f5f5049482330611501dfa))
* **output:** criterion bench over 500 synthetic pages, reuse piece buffers ([c52f24d](https://github.com/4thel00z/docboss/commit/c52f24d79f7a7cf9d78b375163ebd2fda7474778))


### Documentation

* **bench:** count real page totals and use the order-statistic p10 ([169ab34](https://github.com/4thel00z/docboss/commit/169ab3448f2029b00cf623176bb050c5e17f18ef))
* **bench:** fresh benchmark results and the benchmark README ([8a20744](https://github.com/4thel00z/docboss/commit/8a20744777d31cef739d64f12aa156420c6951a6))
* **bench:** refresh every benchmark and the limitations after the rendering and DOC work ([a1bad90](https://github.com/4thel00z/docboss/commit/a1bad90093b854f8a106544dd439936e9c54a775))
* **bench:** refresh fidelity and the limitations after text wrapping and equations ([7421af6](https://github.com/4thel00z/docboss/commit/7421af630e2ada209a438c58507c6a38bd7e9ca7))
* **bench:** refresh the rendering fidelity figures after the text-box work ([1721372](https://github.com/4thel00z/docboss/commit/1721372d843aa02992161ec59c6c4d09ea37ca60))
* correct the limitations, the skill and the changelog ([a1692d2](https://github.com/4thel00z/docboss/commit/a1692d2e501dddd0fbc1ae103ea09731f993830e))
* **docx,xml:** cite the clause numbers the ledger outlines use; report w:altChunk as dropped ([3fda027](https://github.com/4thel00z/docboss/commit/3fda0275f2c4c54d730f373a6db75253388e324c))
* **failure-modes:** the Calibri substitute fix 4a16833 ([d7cd6f5](https://github.com/4thel00z/docboss/commit/d7cd6f5b07b06049d5ac366fe3201870ac611872))
* **failure-modes:** the DOC cell gap fix 456cd22 ([1b29a1c](https://github.com/4thel00z/docboss/commit/1b29a1c2874e25de43dffa39eb84f5ac2e762cd1))
* **failure-modes:** the DOC table border, merged border and wide grid fixes ([42de9a9](https://github.com/4thel00z/docboss/commit/42de9a9f36ecb68ccd8a60068cfe744574b54650))
* **failure-modes:** the line leading fix d0a7e13 ([4bd3cbe](https://github.com/4thel00z/docboss/commit/4bd3cbe4b0074cb87d5665bef2c319403e360d14))
* **failure-modes:** the page number format fix 3bb1dda ([55ecabf](https://github.com/4thel00z/docboss/commit/55ecabfee7343824c1d4daf49fa0d2baa751a76a))
* **failure-modes:** the paragraph border group fix c8c30dc ([70ab1f5](https://github.com/4thel00z/docboss/commit/70ab1f54d2ec230631d316fe951b54cd5148ada3))
* **failure-modes:** the symbol bullet fix 6174d98 ([59be957](https://github.com/4thel00z/docboss/commit/59be9575d2718dff0d52f849e58f3e67c56bc931))
* **failure-modes:** the table style spacing fix ea482a4 and the auto color fix ddbfd3d ([d63202b](https://github.com/4thel00z/docboss/commit/d63202bf8e4cd074a3f9fc8398dbf2a86135d403))
* **failure-modes:** the text box, theme color, picture line and auto-fit fixes ([96672e6](https://github.com/4thel00z/docboss/commit/96672e62bd18bc3b722f6500e25c4568fdff3b73))
* **ledger:** bring the floating table, wrapping and OLE rows up to date ([40338ad](https://github.com/4thel00z/docboss/commit/40338ad59d4650e0760c73ab21c58db81da90041))
* list the crypt, aio and tui crates and the shared build target ([4354e53](https://github.com/4thel00z/docboss/commit/4354e53a45a2428bdff0883ca40f0bb9ca186e7a))
* README, mdBook guide and reference, CHANGELOG and refreshed skill ([3590d5b](https://github.com/4thel00z/docboss/commit/3590d5b160db0a1a111515bb7554245086bbd6ff))
* **write:** crate example covering the builder and Markdown ([201a0d5](https://github.com/4thel00z/docboss/commit/201a0d5cafbfa84b79ff76610972a1f5c216b9e2))
* **zip:** link the writer from the crate docs without the write! ambiguity ([2e04dd5](https://github.com/4thel00z/docboss/commit/2e04dd54ceb723a577a6da289f8a5426c478f6f8))

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

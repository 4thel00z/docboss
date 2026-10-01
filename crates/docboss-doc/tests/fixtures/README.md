# Fixtures

- `text`, `lists`, `tables`, `notes`, `image`, `sections`, `floating`,
  `group`: `.fodt` sources
  written for these tests, converted with
  `soffice --headless --convert-to doc`; the `.txt` files are
  `soffice --headless --convert-to "txt:Text (encoded):UTF8"` of the `.doc`.
- `encrypted-cryptoapi.doc` (password `password`) and `encrypted-rc4.doc`
  (password `tika`): Apache POI test data (`password_password_cryptoapi.doc`,
  `password_tika_binaryrc4.doc`), Apache License 2.0.
- `encrypted-xor.doc` and `encrypted-xor-image.doc` (password `docboss`):
  `text.doc` and `image.doc` XOR-obfuscated by
  `python3 crates/docboss-doc/tools/xor_obfuscate.py`, which follows
  [MS-DOC] §2.2.6.1 and [MS-OFFCRYPTO] §2.3.7; nothing on hand writes XOR
  obfuscation and no corpus file uses it.
- `word6-sections.doc`, `word6-three-sections.doc`, `word95-tables.doc`
  and `word2.doc`: Apache POI test data (`Word6_sections2.doc`,
  `Word6_sections.doc`, `Bug49933.doc`, `word2.doc`), Apache License 2.0.
- `wrap.doc` and `equation-editor.doc`: `soffice --headless --convert-to doc`
  of the DOCX reader's `wrap.docx` and `equation-editor.docx` fixtures.
- `drop-cap.doc`: Apache POI test data (`test.doc`), Apache License 2.0.
- `word6-anld.doc` and `word6-style-anld.doc`: Apache POI test data
  (`Bug60942.doc`, Apache License 2.0) with paragraphs numbered by ANLDs,
  patched by `crates/docboss-doc/tools/word6_anld.py` and
  `word6_style_anld.py`; no corpus Word 6/95 file numbers a paragraph and
  nothing on hand writes Word 6.
- `framed-footer.doc`: Apache POI test data (`PageSpecificHeadFoot.doc`,
  Apache License 2.0): an even footer whose page number sits in a floating
  table whose cell paragraphs carry frame properties.

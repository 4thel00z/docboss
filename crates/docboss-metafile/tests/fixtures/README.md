# Fixtures

- `shapes.svg`: written for these tests: a red rectangle with a black
  outline, a blue ellipse, a black polyline, the text "Hi" in Liberation Sans
  and a 4 by 4 green bitmap.
- `shapes.emf` and `shapes.wmf`: `soffice --headless --convert-to emf` and
  `--convert-to wmf` of `shapes.svg`. The WMF carries the EMF in its escape
  records as well as its own WMF records.

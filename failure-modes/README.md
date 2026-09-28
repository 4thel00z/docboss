# Failure modes

Before/after renders of layout and rendering bugs fixed in this repository.
Each pair is named after the short hash of the commit that fixed it plus a
slug: `<hash>-<slug>-before.png` is `docboss render` at that commit's
parent, `<hash>-<slug>-after.png` the render at the commit. Unless a row says
otherwise, the page is rendered at scale 1 (72 dpi) from a file in the
LibreOffice or Apache POI test corpora, and SSIM is the windowed SSIM of
`benchmarks/bench_fidelity.py` against LibreOffice's own rendering of the
same file at scale 1.5.

| Commit | Failure |
| --- | --- |
| `4a16833` (calibri-substitute) | Calibri resolved to Arial and Cambria to Times New Roman whenever Carlito and Caladea were not in a system font directory, although LibreOffice installs both with its own fonts and macOS ships Carlito as a supplementary font. LibreOffice `ooxmlexport/data/cell-grid-span.docx` sets its body in Calibri and its lower half in Cambria: the before render sets both in the wrong faces, so every line breaks at a different word (SSIM 0.547). With the LibreOffice font directory and macOS's supplementary font directory searched, both resolve to their metric-compatible substitutes and the line breaks match LibreOffice's (SSIM 0.683). Over the 88-file sample, mean SSIM rises from 0.8862 to 0.8883 and page counts agree on 82 files instead of 81. |

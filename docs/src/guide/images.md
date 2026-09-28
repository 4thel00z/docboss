# Extracting images

```bash
docboss images report.docx -o images
docboss images legacy.doc -o images
```

`images` writes every picture the document carries at its stored size and in
its stored format (PNG, JPEG, GIF, BMP, EMF, WMF, TIFF), named after its part
(`image1.png`). DOC pictures come from the Data stream and the OfficeArt BLIP
store; DIB pictures are written as BMP, and identical DOC pictures are stored
once.

```python
from pathlib import Path

import docboss

for image in docboss.Document("report.docx").images():
    Path(image.file_name).write_bytes(image.data)
    print(image.name, image.content_type)
```

`name` is the part name inside the package (`word/media/image1.png`),
`file_name` its last segment, and `content_type` is detected from the bytes.
The Markdown and HTML writers reference or embed the same images (see
[Markdown, HTML and JSON](markdown.md)).

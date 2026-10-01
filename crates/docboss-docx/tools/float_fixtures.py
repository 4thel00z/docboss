"""Writes the floating-object layout fixtures into a directory.

- anchor-moves-on.docx: forty one-line paragraphs, then a paragraph whose
  432 by 288 pt picture, wrapped top and bottom, sits at the top margin.
- anchor-moves-on-tall.docx: the same with a 432 by 551 pt picture.
- float-in-frame.docx: a square-wrapped picture anchored in a text frame.
- float-in-floating-table.docx: a square-wrapped picture anchored in the
  cell of a floating table.
- float-in-cell.docx: a bordered table whose first cell holds a 72 pt
  picture wrapped top and bottom, then a second row.

    python3 crates/docboss-docx/tools/float_fixtures.py crates/docboss-docx/tests/fixtures
"""

import os
import sys
import zipfile

from wrap_fixture import NS, paragraph, png, run

EMU = 12700


def anchor(width, height, wrap, horizontal, vertical):
    cx, cy = width * EMU, height * EMU
    return (
        '<w:r><w:drawing><wp:anchor distT="0" distB="0" distL="114300" distR="114300" '
        'simplePos="0" relativeHeight="1" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1">'
        '<wp:simplePos x="0" y="0"/>'
        f'<wp:positionH relativeFrom="{horizontal}"><wp:posOffset>0</wp:posOffset></wp:positionH>'
        f'<wp:positionV relativeFrom="{vertical}"><wp:posOffset>0</wp:posOffset></wp:positionV>'
        f'<wp:extent cx="{cx}" cy="{cy}"/><wp:effectExtent l="0" t="0" r="0" b="0"/>'
        f'{wrap}<wp:docPr id="1" name="Picture"/>'
        '<a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture">'
        '<pic:pic><pic:nvPicPr><pic:cNvPr id="1" name="p.png"/><pic:cNvPicPr/></pic:nvPicPr>'
        '<pic:blipFill><a:blip r:embed="rImg"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>'
        f'<pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm>'
        '<a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic>'
        "</a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>"
    )


SQUARE = '<wp:wrapSquare wrapText="bothSides"/>'
TOP_AND_BOTTOM = "<wp:wrapTopAndBottom/>"


def moves_on(height):
    lines = "".join(paragraph(run(f"line {i}")) for i in range(40))
    return lines + paragraph(anchor(432, height, TOP_AND_BOTTOM, "margin", "margin") + run("anchor"))


def in_frame():
    frame = (
        '<w:pPr><w:framePr w:w="4320" w:hSpace="180" w:wrap="around" w:vAnchor="text" '
        'w:hAnchor="margin" w:xAlign="right" w:y="1"/></w:pPr>'
    )
    return paragraph(run("Before the frame.")) + paragraph(
        anchor(72, 72, SQUARE, "column", "paragraph") + run("FRAMED"), frame
    ) + paragraph(run("After the frame."))


def in_floating_table():
    table = (
        '<w:tbl><w:tblPr><w:tblpPr w:leftFromText="180" w:rightFromText="180" w:vertAnchor="text" '
        'w:horzAnchor="margin" w:tblpY="1"/><w:tblW w:w="4000" w:type="dxa"/></w:tblPr>'
        '<w:tblGrid><w:gridCol w:w="4000"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="4000" w:type="dxa"/>'
        f'</w:tcPr>{paragraph(anchor(72, 72, SQUARE, "column", "paragraph") + run("CELL"))}</w:tc></w:tr></w:tbl>'
    )
    return paragraph(run("First paragraph.")) + table + paragraph(run("After the table."))


def in_cell():
    borders = "".join(f'<w:{side} w:val="single" w:sz="4"/>' for side in ("top", "left", "bottom", "right", "insideH"))
    picture = paragraph(anchor(72, 72, TOP_AND_BOTTOM, "column", "paragraph"))
    row = '<w:tr><w:tc><w:tcPr><w:tcW w:w="4000" w:type="dxa"/></w:tcPr>{}</w:tc></w:tr>'
    table = (
        f'<w:tbl><w:tblPr><w:tblW w:w="4000" w:type="dxa"/><w:tblBorders>{borders}</w:tblBorders></w:tblPr>'
        '<w:tblGrid><w:gridCol w:w="4000"/></w:tblGrid>'
        + row.format(picture + paragraph(run("LABEL")))
        + row.format(paragraph(run("NEXT ROW")))
        + "</w:tbl>"
    )
    return table + paragraph(run("After the table."))


def write(out, body):
    document = (
        f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document {NS}><w:body>{body}'
        '<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" '
        'w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr>'
        "</w:body></w:document>"
    )
    types = (
        '<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
        '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
        '<Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/>'
        '<Override PartName="/word/document.xml" '
        'ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>'
    )
    rels = (
        '<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" '
        'Target="word/document.xml"/></Relationships>'
    )
    document_rels = (
        '<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        '<Relationship Id="rImg" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" '
        'Target="media/p.png"/></Relationships>'
    )
    with zipfile.ZipFile(out, "w") as package:
        package.writestr("[Content_Types].xml", types)
        package.writestr("_rels/.rels", rels)
        package.writestr("word/_rels/document.xml.rels", document_rels)
        package.writestr("word/media/p.png", png(8, 6, (200, 60, 60)))
        package.writestr("word/document.xml", document)


def main(directory):
    write(os.path.join(directory, "anchor-moves-on.docx"), moves_on(288))
    write(os.path.join(directory, "anchor-moves-on-tall.docx"), moves_on(551))
    write(os.path.join(directory, "float-in-frame.docx"), in_frame())
    write(os.path.join(directory, "float-in-floating-table.docx"), in_floating_table())
    write(os.path.join(directory, "float-in-cell.docx"), in_cell())


if __name__ == "__main__":
    main(sys.argv[1])

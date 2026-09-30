"""Writes tests/fixtures/wrap.docx: floating objects text wraps around.

A picture wrapped square on both sides, a picture wrapped tight on its
left with a wrap polygon and distances, a text frame at the right margin,
a floating table and a VML rectangle wrapped top and bottom.

    python3 crates/docboss-docx/tools/wrap_fixture.py crates/docboss-docx/tests/fixtures/wrap.docx
"""

import struct
import sys
import zipfile
import zlib

NS = (
    'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" '
    'xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" '
    'xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" '
    'xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" '
    'xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture" '
    'xmlns:v="urn:schemas-microsoft-com:vml" '
    'xmlns:w10="urn:schemas-microsoft-com:office:word" '
    'xmlns:o="urn:schemas-microsoft-com:office:office"'
)
TEXT = "Text flows around the floating objects of this document. " * 6


def png(width, height, rgb):
    raw = b"".join(b"\x00" + bytes(rgb) * width for _ in range(height))

    def chunk(kind, data):
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body) & 0xFFFFFFFF)

    header = struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b"")


def run(text):
    return f'<w:r><w:t xml:space="preserve">{text}</w:t></w:r>'


def paragraph(content, properties=""):
    return f"<w:p>{properties}{content}</w:p>"


def anchor(wrap, relative, x, distances):
    dist_t, dist_b, dist_l, dist_r = distances
    return (
        f'<w:r><w:drawing><wp:anchor distT="{dist_t}" distB="{dist_b}" distL="{dist_l}" distR="{dist_r}" '
        'simplePos="0" relativeHeight="1" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1">'
        '<wp:simplePos x="0" y="0"/>'
        f'<wp:positionH relativeFrom="{relative}"><wp:posOffset>{x}</wp:posOffset></wp:positionH>'
        '<wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV>'
        '<wp:extent cx="1828800" cy="1371600"/><wp:effectExtent l="0" t="0" r="0" b="0"/>'
        f'{wrap}<wp:docPr id="1" name="Picture"/>'
        '<a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture">'
        '<pic:pic><pic:nvPicPr><pic:cNvPr id="1" name="p.png"/><pic:cNvPicPr/></pic:nvPicPr>'
        '<pic:blipFill><a:blip r:embed="rImg"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>'
        '<pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="1828800" cy="1371600"/></a:xfrm>'
        '<a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic>'
        "</a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>"
    )


def cell(text):
    return f'<w:tc><w:tcPr><w:tcW w:w="1440" w:type="dxa"/></w:tcPr>{paragraph(run(text))}</w:tc>'


def body():
    square = '<wp:wrapSquare wrapText="bothSides"/>'
    tight = (
        '<wp:wrapTight wrapText="left"><wp:wrapPolygon edited="0"><wp:start x="0" y="0"/>'
        '<wp:lineTo x="21600" y="0"/><wp:lineTo x="21600" y="21600"/><wp:lineTo x="0" y="21600"/>'
        '<wp:lineTo x="0" y="0"/></wp:wrapPolygon></wp:wrapTight>'
    )
    frame = (
        '<w:pPr><w:framePr w:w="2880" w:h="720" w:hRule="exact" w:hSpace="180" w:vSpace="90" '
        'w:wrap="around" w:vAnchor="text" w:hAnchor="margin" w:xAlign="right" w:y="1"/></w:pPr>'
    )
    borders = "".join(
        f'<w:{side} w:val="single" w:sz="4"/>' for side in ("top", "left", "bottom", "right", "insideH", "insideV")
    )
    table = (
        '<w:tbl><w:tblPr><w:tblpPr w:leftFromText="180" w:rightFromText="360" w:topFromText="60" '
        'w:bottomFromText="120" w:vertAnchor="text" w:horzAnchor="page" w:tblpX="1440" w:tblpY="200"/>'
        f'<w:tblW w:w="2880" w:type="dxa"/><w:tblBorders>{borders}</w:tblBorders></w:tblPr>'
        '<w:tblGrid><w:gridCol w:w="1440"/><w:gridCol w:w="1440"/></w:tblGrid>'
        + "".join(f"<w:tr>{cell('a' + str(i))}{cell('b' + str(i))}</w:tr>" for i in range(3))
        + "</w:tbl>"
    )
    rectangle = (
        '<w:r><w:pict><v:rect style="position:absolute;margin-left:300pt;margin-top:0;width:100pt;'
        "height:50pt;z-index:1;mso-wrap-distance-left:4pt;mso-position-horizontal-relative:text;"
        'mso-position-vertical-relative:text" fillcolor="#3070c0"><w10:wrap type="topAndBottom"/>'
        "</v:rect></w:pict></w:r>"
    )
    return (
        paragraph(anchor(square, "column", 2057400, (0, 0, 114300, 114300)) + run(TEXT))
        + paragraph(anchor(tight, "margin", 0, (12700, 25400, 38100, 50800)) + run(TEXT))
        + paragraph(run("A text frame at the right margin."), frame)
        + paragraph(run(TEXT))
        + table
        + paragraph(run(TEXT))
        + paragraph(rectangle + run(TEXT))
    )


def main(out):
    document = (
        f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document {NS}><w:body>{body()}'
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


if __name__ == "__main__":
    main(sys.argv[1])

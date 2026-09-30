use docboss_docx::read;
use docboss_model::{
    plain_text, Block, Document, DrawingPlacement, DrawingPosition, Inline, PositionAlign,
    PositionBase, RunContent,
};

fn open(name: &str) -> Document {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    read(&std::fs::read(path).unwrap()).unwrap()
}

fn drawings(document: &Document) -> Vec<&docboss_model::Drawing> {
    let mut out = Vec::new();
    for block in document.blocks() {
        let Block::Paragraph(p) = block else { continue };
        for inline in &p.inlines {
            let Inline::Run(run) = inline else { continue };
            for content in &run.content {
                if let RunContent::Drawing(d) = content {
                    out.push(d.as_ref());
                }
            }
        }
    }
    out
}

/// A LibreOffice document: heading styles, footnotes and endnotes
/// (ECMA-376 Part 1 §17.11), comments (§17.13.4), a header and footer
/// (§17.10), an embedded picture and a table with a merged row.
#[test]
fn libreoffice_rich_document() {
    let doc = open("libreoffice-rich.docx");
    assert!(doc.diagnostics.is_empty(), "{:?}", doc.diagnostics);
    assert_eq!(doc.metadata.title.as_deref(), Some("Fixture Document"));
    let text = plain_text(&doc);
    for expected in [
        "Fixture Heading",
        "Text with bold words and a note here.",
        "Item two",
        "A1",
        "Span",
        "Ends here.",
    ] {
        assert!(
            text.contains(expected),
            "{expected:?} missing from {text:?}"
        );
    }
    let Some(Block::Paragraph(heading)) = doc.blocks().next() else {
        panic!()
    };
    assert_eq!(doc.styles.heading_level(heading, &doc.numbering), Some(0));
    let footnote = doc.footnotes.iter().find(|n| !n.blocks.is_empty()).unwrap();
    let Block::Paragraph(note) = &footnote.blocks[0] else {
        panic!()
    };
    assert!(note.text().contains("Footnote text."));
    assert!(doc.endnotes.iter().any(|n| n
        .blocks
        .iter()
        .any(|b| matches!(b, Block::Paragraph(p) if p.text().contains("Endnote text.")))));
    let comment = &doc.comments[0];
    assert_eq!(comment.author.as_deref(), Some("Reviewer"));
    assert_eq!(doc.headers_footers.len(), 2);
    assert_eq!(doc.media.len(), 1);
    assert_eq!(doc.media[0].content_type, "image/png");
    let pictures = drawings(&doc);
    assert_eq!(pictures.len(), 1);
    assert_eq!(pictures[0].media, Some(docboss_model::MediaId(0)));
    assert_eq!(pictures[0].width, 1_440_180);
    let table = doc.blocks().find_map(|b| match b {
        Block::Table(t) => Some(t),
        _ => None,
    });
    assert_eq!(table.unwrap().rows[1].cells[0].span(), 2);
    let section = &doc.sections.last().unwrap().properties;
    assert_eq!(section.page_size.width, 11906);
}

/// An RTF document converted by LibreOffice: a page break and a centered
/// paragraph survive.
#[test]
fn libreoffice_rtf_document() {
    let doc = open("libreoffice-rtf-notes.docx");
    let text = plain_text(&doc);
    assert!(text.contains("Centered paragraph."));
    assert!(text.contains("Second page text."));
    assert!(doc.headers_footers.iter().any(|h| h
        .blocks
        .iter()
        .any(|b| matches!(b, Block::Paragraph(p) if p.text().contains("Header text")))));
}

/// A document written by macOS `textutil`, whose lists carry their labels
/// as text.
#[test]
fn textutil_document() {
    let doc = open("textutil-lists.docx");
    let text = plain_text(&doc);
    assert!(text.contains("First bullet"));
    assert!(text.contains("merged cell"));
}

/// A Word text box whose `wp:positionH` centers it on its column with
/// `wp:align` and whose `wp:positionV` is an offset from the paragraph.
/// ECMA-376 Part 1 §20.4.2.1, §20.4.2.10, §20.4.2.11, §20.4.3.1, §20.4.3.4, §20.4.3.5.
#[test]
fn word_aligned_text_box() {
    let doc = open("dml-picture-in-textframe.docx");
    let boxes = drawings(&doc);
    assert_eq!(boxes.len(), 1);
    assert_eq!(
        boxes[0].placement,
        DrawingPlacement::Anchored {
            horizontal: DrawingPosition {
                base: PositionBase::Column,
                align: Some(PositionAlign::Center),
                offset: 0,
            },
            vertical: DrawingPosition::offset(PositionBase::Paragraph, 0),
            behind_text: false,
        }
    );
    assert_eq!(boxes[0].width, 2_388_870);
    assert!(!boxes[0].text_box.is_empty());
}

/// The wrapping, distances and wrap polygon of anchored pictures, a text
/// frame, a floating table and a VML shape's `w10:wrap` with its
/// `mso-wrap-distance-left`.
/// ECMA-376 Part 1 §20.4.2.3, §20.4.2.17, §20.4.2.19, §20.4.2.16, §20.4.2.14, §20.4.2.9.
/// ECMA-376 Part 1 §20.4.3.6, §20.4.3.7, §17.3.1.11, §17.18.35, §17.18.100, §17.18.104.
/// ECMA-376 Part 1 §17.4.57.
#[test]
fn wrapping_frames_and_floating_tables_are_read() {
    use docboss_model::{FrameProperties, LineRule, TableFloat, WrapKind, WrapSide};
    let doc = open("wrap.docx");
    assert!(doc.diagnostics.is_empty(), "{:?}", doc.diagnostics);
    let found = drawings(&doc);
    assert_eq!(found.len(), 3);
    assert_eq!(found[0].wrap.kind, WrapKind::Square);
    assert_eq!(found[0].wrap.side, WrapSide::Both);
    assert_eq!(found[0].wrap.distance, [0, 0, 114_300, 114_300]);
    assert_eq!(found[1].wrap.kind, WrapKind::Tight);
    assert_eq!(found[1].wrap.side, WrapSide::Left);
    assert_eq!(found[1].wrap.distance, [12_700, 25_400, 38_100, 50_800]);
    assert_eq!(found[1].wrap.polygon.len(), 5);
    assert_eq!(found[1].wrap.polygon[2], (21_600, 21_600));
    assert_eq!(found[2].wrap.kind, WrapKind::TopAndBottom);
    assert_eq!(found[2].wrap.distance, [0, 0, 50_800, 114_300]);
    let blocks = &doc.sections[0].blocks;
    let Block::Paragraph(framed) = &blocks[2] else {
        panic!("the third block is the frame's paragraph");
    };
    assert_eq!(
        framed.properties.frame,
        Some(FrameProperties {
            width: Some(2880),
            height: 720,
            height_rule: LineRule::Exact,
            horizontal: DrawingPosition {
                base: PositionBase::Margin,
                align: Some(PositionAlign::End),
                offset: 0,
            },
            vertical: DrawingPosition::offset(PositionBase::Paragraph, 635),
            wrap: WrapKind::Square,
            h_space: 180,
            v_space: 90,
            drop_cap: docboss_model::DropCap::None,
            lines: 1,
        })
    );
    let Block::Table(table) = &blocks[4] else {
        panic!("the fifth block is the floating table");
    };
    assert_eq!(
        table.properties.floating,
        Some(TableFloat {
            horizontal: DrawingPosition::offset(PositionBase::Page, 1440 * 635),
            vertical: DrawingPosition::offset(PositionBase::Paragraph, 200 * 635),
            distance: [60, 120, 180, 360],
        })
    );
}

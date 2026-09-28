use docboss_docx::read;
use docboss_model::{plain_text, Block, Document, Inline, RunContent};

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
                    out.push(d);
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

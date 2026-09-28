mod common;

use docboss_model::{Block, Justification, NumberFormat, RunContent};
use docboss_write::markdown::{to_document, Options};

fn fixture() -> &'static str {
    include_str!("common/markdown.md")
}

fn options() -> Options<'static> {
    let png = common::tiny_png();
    Options {
        images: Some(Box::new(move |source: &str| {
            (source == "tiny.png").then(|| png.clone())
        })),
        ..Options::default()
    }
}

fn paragraphs(document: &docboss_model::Document) -> Vec<&docboss_model::Paragraph> {
    document
        .blocks()
        .filter_map(|block| match block {
            Block::Paragraph(paragraph) => Some(paragraph),
            Block::Table(_) => None,
        })
        .collect()
}

#[test]
fn headings_map_to_heading_styles_and_title() {
    let document = to_document(fixture(), &options());
    let styles: Vec<Option<&str>> = paragraphs(&document)
        .iter()
        .map(|p| p.style_id.as_deref())
        .collect();
    assert_eq!(styles[0], Some("Heading1"));
    assert!(styles.contains(&Some("Heading2")));
    assert_eq!(document.metadata.title.as_deref(), Some("Report title"));
}

/// ECMA-376 Part 1 §17.9: every list gets its own numbering instance;
/// ordered lists restart at their Markdown start number.
#[test]
fn lists_become_numbering_instances() {
    let document = to_document(fixture(), &options());
    let listed: Vec<_> = paragraphs(&document)
        .into_iter()
        .filter_map(|p| p.properties.numbering.map(|n| (p.text(), n)))
        .collect();
    let (_, bullet) = listed.iter().find(|(t, _)| t == "bullet one").unwrap();
    let (_, nested) = listed.iter().find(|(t, _)| t == "nested bullet").unwrap();
    let (_, third) = listed.iter().find(|(t, _)| t == "third").unwrap();
    assert_eq!(nested.level, 1);
    assert_ne!(bullet.num_id, third.num_id);
    let instance = document.numbering.instance(third.num_id).unwrap();
    assert!(instance.start_overrides.contains(&(0, 3)));
    assert_eq!(
        document.numbering.level(*third).unwrap().format,
        NumberFormat::Decimal
    );
    let mut counter = document.numbering.counter();
    assert_eq!(counter.next(*third).as_deref(), Some("3."));
    assert!(listed.iter().any(|(t, _)| t == "\u{2612} done task"));
}

#[test]
fn code_quotes_tables_rules_and_images() {
    let document = to_document(fixture(), &options());
    let all = paragraphs(&document);
    let code = all
        .iter()
        .find(|p| p.style_id.as_deref() == Some("SourceCode"))
        .unwrap();
    assert_eq!(code.text(), "fn main() {\n    println!(\"hi\");\n}");
    assert!(all
        .iter()
        .any(|p| p.style_id.as_deref() == Some("Quote") && p.text() == "A quoted paragraph."));
    let nested = all.iter().find(|p| p.text() == "Nested quote.").unwrap();
    assert_eq!(nested.properties.indentation.left, Some(1440));
    assert!(all
        .iter()
        .any(|p| p.properties.borders.is_some_and(|b| b.bottom.is_some())));
    let table = document
        .blocks()
        .find_map(|b| match b {
            Block::Table(t) => Some(t),
            Block::Paragraph(_) => None,
        })
        .unwrap();
    assert_eq!(table.rows.len(), 3);
    assert!(table.rows[0].properties.header);
    let right = match &table.rows[1].cells[2].blocks[0] {
        Block::Paragraph(p) => p.properties.justification,
        Block::Table(_) => None,
    };
    assert_eq!(right, Some(Justification::Right));
    let drawings = all
        .iter()
        .flat_map(|p| p.inlines.iter())
        .filter(|i| matches!(i, docboss_model::Inline::Run(r) if r.content.iter().any(|c| matches!(c, RunContent::Drawing(_)))))
        .count();
    assert_eq!(drawings, 1);
    assert_eq!(document.media.len(), 1);
    assert!(all.iter().any(|p| p.text().contains("missing alt")));
}

/// ECMA-376 Part 1 §17.11: a Markdown footnote becomes a footnote whose
/// body starts with the note number.
#[test]
fn footnotes_become_notes() {
    let document = to_document(fixture(), &options());
    assert_eq!(document.footnotes.len(), 1);
    let note = &document.footnotes[0];
    let Block::Paragraph(first) = &note.blocks[0] else {
        panic!("note starts with a table")
    };
    assert!(
        matches!(&first.inlines[0], docboss_model::Inline::Run(r) if r.content == [RunContent::NoteNumber])
    );
    assert_eq!(first.text().trim(), "The footnote body.");
    let referenced = paragraphs(&document).iter().any(|p| {
        p.inlines.iter().any(|i| matches!(i, docboss_model::Inline::Run(r) if r.content.contains(&RunContent::FootnoteReference(note.id))))
    });
    assert!(referenced);
}

#[test]
fn markdown_output_is_deterministic() {
    let first = docboss_write::markdown::to_docx(fixture(), &options()).unwrap();
    let second = docboss_write::markdown::to_docx(fixture(), &options()).unwrap();
    assert_eq!(first, second);
}

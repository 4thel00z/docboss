mod common;

use std::fs;

/// ECMA-376 Part 1 §17.2: the package LibreOffice opens carries every
/// story's text — the main text, notes, lists, tables and fields.
#[test]
fn libreoffice_reads_the_kitchen_sink() {
    let Some(soffice) = common::soffice() else {
        eprintln!("soffice not found; skipping");
        return;
    };
    let dir = common::scratch("libreoffice");
    let document = common::kitchen_sink();
    docboss_write::save(&document, dir.join("sink.docx")).unwrap();
    let txt = common::convert(soffice, &dir, "txt");
    let text = fs::read_to_string(txt.join("sink.txt")).unwrap();
    for expected in [
        "Kitchen sink title",
        "First heading",
        "Plain bold italic underlined a link & <escaped> text\tafter tab",
        "second line",
        "Has a footnote",
        "bullet one",
        "numbered one",
        "nested a",
        "numbered two",
        "H1",
        "merged",
        "Anchored picture",
        "inserted deleted bookmarked commented",
        "Second section, landscape.",
        "After the page break.",
    ] {
        assert!(text.contains(expected), "missing {expected:?} in:\n{text}");
    }
    let pdf = common::convert(soffice, &dir, "pdf");
    assert!(fs::metadata(pdf.join("sink.pdf")).unwrap().len() > 1000);
}

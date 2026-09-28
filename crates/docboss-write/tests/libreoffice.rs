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
    let png = common::tiny_png();
    let options = docboss_write::markdown::Options {
        images: Some(Box::new(move |source: &str| {
            (source == "tiny.png").then(|| png.clone())
        })),
        ..Default::default()
    };
    let markdown =
        docboss_write::markdown::to_docx(include_str!("common/markdown.md"), &options).unwrap();
    std::fs::write(dir.join("markdown.docx"), markdown).unwrap();
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
    let text = fs::read_to_string(txt.join("markdown.txt")).unwrap();
    for expected in [
        "Report title",
        "Intro with emphasis, strong, struck, inline code and a link.",
        "A soft break joins lines.",
        "After a br.",
        "• bullet one",
        "nested bullet",
        "3. third",
        "4. fourth",
        "a. inner one",
        "A quoted paragraph.",
        "    println!(\"hi\");",
        "Left\tCenter\tRight",
        "Footnote here.",
        "missing alt",
    ] {
        assert!(text.contains(expected), "missing {expected:?} in:\n{text}");
    }
    let pdf = common::convert(soffice, &dir, "pdf");
    assert!(fs::metadata(pdf.join("sink.pdf")).unwrap().len() > 1000);
    assert!(fs::metadata(pdf.join("markdown.pdf")).unwrap().len() > 1000);
    let Some(pdf_text) = common::pdf_text(&pdf.join("sink.pdf")) else {
        eprintln!("pdfboss not found; skipping the PDF text checks");
        return;
    };
    for expected in [
        "1 Footnote body text.",
        "Header text",
        "Footer text",
        "Page break next",
    ] {
        assert!(
            pdf_text.contains(expected),
            "missing {expected:?} in:\n{pdf_text}"
        );
    }
    let info = std::process::Command::new("pdfboss")
        .arg("info")
        .arg(pdf.join("sink.pdf"))
        .output()
        .unwrap();
    let info = String::from_utf8_lossy(&info.stdout);
    assert!(
        info.contains("page 1: 612 x 792 pt") && info.contains("page 2: 792 x 612 pt"),
        "{info}"
    );
    let markdown_text = common::pdf_text(&pdf.join("markdown.pdf")).unwrap();
    assert!(
        markdown_text.contains("1 The footnote body."),
        "{markdown_text}"
    );
}

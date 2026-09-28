mod common;

use common::*;
use docboss_model::*;
use docboss_output::{to_html, HtmlOptions, ImageMode};

fn html(doc: &Document) -> String {
    to_html(doc, &HtmlOptions::default())
}

#[test]
fn headings_paragraphs_and_inline_formatting() {
    let sup = text_run(
        "2",
        RunProperties {
            vertical_align: Some(VerticalAlign::Superscript),
            ..RunProperties::default()
        },
    );
    let underline = text_run(
        "u",
        RunProperties {
            underline: Some(Underline::Single),
            ..RunProperties::default()
        },
    );
    let doc = document(vec![
        styled("Heading2", vec![run("Title & <more>")]),
        para(vec![
            bold("a "),
            bold("b"),
            run(" x"),
            sup,
            run(" "),
            underline,
            run(" "),
            mono("code"),
        ]),
    ]);
    assert_eq!(
        html(&doc),
        "<h2>Title &amp; &lt;more&gt;</h2>\n<p><strong>a b</strong> x<sup>2</sup> <u>u</u> <code>code</code></p>\n"
    );
}

#[test]
fn nested_lists_with_types_and_kind_changes() {
    let doc = document(vec![
        item(1, 0, vec![run("one")]),
        item(1, 1, vec![run("sub")]),
        item(1, 0, vec![run("two")]),
        item(2, 0, vec![run("dot")]),
        para(vec![run("end")]),
    ]);
    assert_eq!(
        html(&doc),
        "<ol>\n<li>one<ol type=\"a\">\n<li>sub</li>\n</ol>\n</li>\n<li>two</li>\n</ol>\n<ul>\n<li>dot</li>\n</ul>\n<p>end</p>\n"
    );
}

#[test]
fn continued_list_starts_at_its_number() {
    let doc = document(vec![
        item(1, 0, vec![run("one")]),
        para(vec![run("break")]),
        item(1, 0, vec![run("two")]),
    ]);
    assert!(html(&doc).contains("<ol start=\"2\">\n<li>two</li>"));
}

#[test]
fn merged_cells_become_colspan_and_rowspan() {
    let mut wide = cell(vec![para(vec![run("wide")])]);
    wide.properties.grid_span = 2;
    let mut restart = cell(vec![para(vec![run("tall")])]);
    restart.properties.vertical_merge = Some(VerticalMerge::Restart);
    let mut continued = cell(vec![]);
    continued.properties.vertical_merge = Some(VerticalMerge::Continue);
    let mut header = row(vec![cell(vec![para(vec![run("h")])]), wide]);
    header.properties.header = true;
    let doc = document(vec![table(vec![
        header,
        row(vec![
            restart,
            cell(vec![para(vec![run("b")])]),
            cell(vec![para(vec![run("c")])]),
        ]),
        row(vec![
            continued,
            cell(vec![para(vec![run("e")])]),
            cell(vec![item(2, 0, vec![run("f")])]),
        ]),
    ])]);
    assert_eq!(
        html(&doc),
        "<table>\n<tr><th>h</th><th colspan=\"2\">wide</th></tr>\n<tr><td rowspan=\"2\">tall</td><td>b</td><td>c</td></tr>\n<tr><td>e</td><td><ul>\n<li>f</li>\n</ul>\n</td></tr>\n</table>\n"
    );
}

#[test]
fn footnotes_section_links_back() {
    let mut doc = document(vec![para(vec![
        run("x"),
        content(RunContent::FootnoteReference(1)),
    ])]);
    doc.footnotes = vec![note(1, NoteKind::Footnote, "why")];
    assert_eq!(
        html(&doc),
        "<p>x<sup><a href=\"#fn-1\" id=\"fnref-1\">1</a></sup></p>\n<section class=\"footnotes\">\n<ol>\n<li id=\"fn-1\">\n<p>why</p>\n<a href=\"#fnref-1\">\u{21a9}</a></li>\n</ol>\n</section>\n"
    );
}

#[test]
fn images_links_and_standalone_documents() {
    let mut doc = document(vec![para(vec![
        link("https://a.b/?q=\"1\"&r", vec![run("go")]),
        drawing(0, "Pic \"1\""),
    ])]);
    doc.media = vec![png("image1.png")];
    doc.metadata.title = Some("T".into());
    let options = HtmlOptions {
        standalone: true,
        images: ImageMode::Reference {
            prefix: "img/".into(),
        },
        ..HtmlOptions::default()
    };
    assert_eq!(
        to_html(&doc, &options),
        "<!DOCTYPE html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n<title>T</title>\n</head>\n<body>\n<p><a href=\"https://a.b/?q=&quot;1&quot;&amp;r\">go</a><img src=\"img/image1.png\" alt=\"Pic &quot;1&quot;\" width=\"100\" height=\"50\"></p>\n</body>\n</html>\n"
    );
    assert!(html(&doc).contains("src=\"data:image/png;base64,iVBORw0KGgo=\""));
}

#[test]
fn code_blocks_escape_their_text() {
    let doc = document(vec![para(vec![mono("a < b")]), para(vec![mono("&&")])]);
    assert_eq!(html(&doc), "<pre><code>a &lt; b\n&amp;&amp;</code></pre>\n");
}

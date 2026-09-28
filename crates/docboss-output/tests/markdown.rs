mod common;

use common::*;
use docboss_model::*;
use docboss_output::{to_markdown, ImageMode, MarkdownOptions};

fn md(doc: &Document) -> String {
    to_markdown(doc, &MarkdownOptions::default())
}

#[test]
fn headings_come_from_styles_and_drop_style_bold() {
    let doc = document(vec![
        styled("Heading1", vec![run("Intro")]),
        para(vec![run("Body.")]),
        styled("Heading3", vec![bold("Deep")]),
    ]);
    assert_eq!(md(&doc), "# Intro\n\nBody.\n\n### Deep\n");
}

#[test]
fn emphasis_merges_across_runs_with_whitespace_outside() {
    let doc = document(vec![para(vec![
        run("a "),
        bold("bold "),
        bold("text "),
        run("and "),
        italic("it"),
        run(" "),
        text_run(
            "both",
            RunProperties {
                bold: Some(true),
                italic: Some(true),
                ..RunProperties::default()
            },
        ),
        run(" "),
        text_run(
            "gone",
            RunProperties {
                strike: Some(true),
                ..RunProperties::default()
            },
        ),
    ])]);
    assert_eq!(md(&doc), "a **bold text** and *it* ***both*** ~~gone~~\n");
}

#[test]
fn links_and_anchors() {
    let internal = Inline::Hyperlink(Hyperlink {
        anchor: Some("sec2".into()),
        inlines: vec![run("see below")],
        ..Hyperlink::default()
    });
    let field = Inline::Field(Field {
        instruction: "HYPERLINK \"https://example.org/x(1)\"".into(),
        result: vec![run("field link")],
    });
    let doc = document(vec![para(vec![
        link("https://example.com", vec![bold("site")]),
        run(", "),
        internal,
        run(", "),
        field,
    ])]);
    assert_eq!(
        md(&doc),
        "[**site**](https://example.com), [see below](#sec2), [field link](<https://example.org/x(1)>)\n"
    );
}

#[test]
fn nested_lists_indent_under_their_parent_marker() {
    let doc = document(vec![
        para(vec![run("Before")]),
        item(1, 0, vec![run("one")]),
        item(1, 1, vec![run("sub")]),
        item(1, 2, vec![run("deeper")]),
        item(1, 0, vec![run("two")]),
        item(2, 0, vec![run("dot")]),
        item(2, 1, vec![run("hollow")]),
        para(vec![run("After")]),
    ]);
    assert_eq!(
        md(&doc),
        "Before\n\n1. one\n   1. sub\n      1. deeper\n2. two\n- dot\n  - hollow\n\nAfter\n"
    );
}

#[test]
fn tables_become_pipe_tables_with_flattened_cells() {
    let mut merged = cell(vec![para(vec![run("wide")])]);
    merged.properties.grid_span = 2;
    let mut restart = cell(vec![para(vec![run("tall")])]);
    restart.properties.vertical_merge = Some(VerticalMerge::Restart);
    let mut continued = cell(vec![]);
    continued.properties.vertical_merge = Some(VerticalMerge::Continue);
    let doc = document(vec![table(vec![
        row(vec![
            cell(vec![para(vec![bold("H1")])]),
            cell(vec![para(vec![run("H|2")])]),
            cell(vec![]),
        ]),
        row(vec![restart, merged]),
        row(vec![
            continued,
            cell(vec![para(vec![run("x")]), para(vec![run("y")])]),
            cell(vec![para(vec![run("z")])]),
        ]),
    ])]);
    assert_eq!(
        md(&doc),
        "| **H1** | H\\|2 |  |\n| --- | --- | --- |\n| tall | wide |  |\n|  | x<br>y | z |\n"
    );
}

#[test]
fn footnotes_become_definitions() {
    let mut doc = document(vec![para(vec![
        run("claim"),
        content(RunContent::FootnoteReference(4)),
    ])]);
    doc.footnotes = vec![note(4, NoteKind::Footnote, "source")];
    assert_eq!(md(&doc), "claim[^1]\n\n[^1]: source\n");
}

#[test]
fn images_are_referenced_under_a_prefix_or_embedded() {
    let mut doc = document(vec![para(vec![drawing(0, "A chart")])]);
    doc.media = vec![png("image1.png")];
    let options = MarkdownOptions {
        images: ImageMode::Reference {
            prefix: "media/".into(),
        },
        ..MarkdownOptions::default()
    };
    assert_eq!(
        to_markdown(&doc, &options),
        "![A chart](media/image1.png)\n"
    );
    let options = MarkdownOptions {
        images: ImageMode::Embed,
        ..MarkdownOptions::default()
    };
    assert_eq!(
        to_markdown(&doc, &options),
        "![A chart](data:image/png;base64,iVBORw0KGgo=)\n"
    );
    let options = MarkdownOptions {
        images: ImageMode::Omit,
        ..MarkdownOptions::default()
    };
    assert_eq!(to_markdown(&doc, &options), "");
}

#[test]
fn monospace_paragraphs_become_one_fenced_block() {
    let doc = document(vec![
        para(vec![run("Run "), mono("cargo test"), run(":")]),
        para(vec![mono("fn main() {")]),
        para(vec![mono("    *x = 1;")]),
        styled("SourceCode", vec![run("}")]),
        para(vec![run("Done.")]),
    ]);
    assert_eq!(
        md(&doc),
        "Run `cargo test`:\n\n```\nfn main() {\n    *x = 1;\n}\n```\n\nDone.\n"
    );
}

#[test]
fn specials_are_escaped_and_breaks_written() {
    let doc = document(vec![
        para(vec![run("# not heading *a* [b] 1_000 _x_")]),
        para(vec![
            run("1. not a list"),
            content(RunContent::Break(Break::Line)),
            run("- nor this"),
        ]),
        para(vec![
            run("page one"),
            content(RunContent::Break(Break::Page)),
        ]),
        para(vec![run("page two")]),
    ]);
    let expected = "\\# not heading \\*a\\* \\[b\\] 1_000 \\_x\\_\n\n1\\. not a list\\\n\\- nor this\n\npage one\n\npage two\n";
    assert_eq!(md(&doc), expected);
    let options = MarkdownOptions {
        page_breaks: true,
        ..MarkdownOptions::default()
    };
    assert!(to_markdown(&doc, &options).contains("page one\n\n---\n\npage two"));
}

#[test]
fn title_and_empty_paragraphs() {
    let mut doc = document(vec![
        para(vec![]),
        para(vec![run("  ")]),
        para(vec![run("text")]),
    ]);
    doc.metadata.title = Some("Report".into());
    let options = MarkdownOptions {
        title: true,
        ..MarkdownOptions::default()
    };
    assert_eq!(to_markdown(&doc, &options), "# Report\n\ntext\n");
}

#[test]
fn list_item_line_breaks_stay_inside_the_item() {
    let doc = document(vec![item(
        2,
        0,
        vec![
            run("first"),
            content(RunContent::Break(Break::Line)),
            run("second"),
        ],
    )]);
    assert_eq!(md(&doc), "- first\\\n  second\n");
}

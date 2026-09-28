mod common;

use common::*;
use docboss_model::*;
use docboss_output::{to_text, TextOptions};

fn text(doc: &Document) -> String {
    to_text(doc, &TextOptions::default())
}

/// ECMA-376 Part 1 §17.9: list paragraphs carry labels computed from their
/// numbering level, restarting deeper levels when a shallower one advances.
#[test]
fn list_labels_and_bullets() {
    let doc = document(vec![
        item(1, 0, vec![run("one")]),
        item(1, 1, vec![run("sub")]),
        item(1, 0, vec![run("two")]),
        item(1, 1, vec![run("sub again")]),
        item(2, 0, vec![run("bullet")]),
        item(2, 1, vec![run("hollow")]),
    ]);
    assert_eq!(
        text(&doc),
        "1. one\na) sub\n2. two\na) sub again\n• bullet\no hollow\n"
    );
}

#[test]
fn tables_are_tab_separated_rows() {
    let doc = document(vec![table(vec![
        row(vec![
            cell(vec![para(vec![run("a")])]),
            cell(vec![para(vec![run("b")]), para(vec![run("c")])]),
        ]),
        row(vec![cell(vec![para(vec![run("d")])]), cell(vec![])]),
    ])]);
    assert_eq!(text(&doc), "a\tb c\nd\t\n");
}

/// ECMA-376 Part 1 §17.3.2.41: vanish hides a run, directly or from a
/// character style; §17.13.5.14: deleted runs are not part of the text.
#[test]
fn hidden_text_and_deletions_are_dropped() {
    let hidden_direct = text_run(
        "secret",
        RunProperties {
            vanish: Some(true),
            ..RunProperties::default()
        },
    );
    let hidden_style = text_run(
        "styled",
        RunProperties {
            style_id: Some("Hidden".into()),
            ..RunProperties::default()
        },
    );
    let deleted = Inline::Revision(Revision {
        kind: RevisionKind::Deletion,
        author: None,
        date: None,
        inlines: vec![run("old")],
    });
    let inserted = Inline::Revision(Revision {
        kind: RevisionKind::Insertion,
        author: None,
        date: None,
        inlines: vec![run("new")],
    });
    let doc = document(vec![para(vec![
        run("a "),
        hidden_direct,
        hidden_style,
        deleted,
        inserted,
    ])]);
    assert_eq!(text(&doc), "a new\n");
}

/// ECMA-376 Part 1 §17.16: a field contributes its cached result.
#[test]
fn fields_use_their_result() {
    let field = Inline::Field(Field {
        instruction: " PAGE ".into(),
        result: vec![run("7")],
    });
    let doc = document(vec![para(vec![run("page "), field])]);
    assert_eq!(text(&doc), "page 7\n");
}

#[test]
fn notes_are_numbered_and_appended() {
    let mut doc = document(vec![para(vec![
        run("claim"),
        content(RunContent::FootnoteReference(5)),
        run(" more"),
        content(RunContent::EndnoteReference(2)),
        content(RunContent::FootnoteReference(9)),
    ])]);
    doc.footnotes = vec![
        note(9, NoteKind::Footnote, "second"),
        note(5, NoteKind::Footnote, "first"),
    ];
    doc.endnotes = vec![note(2, NoteKind::Endnote, "end")];
    assert_eq!(
        text(&doc),
        "claim[1] more[i][2]\n\n[1] first\n[i] end\n[2] second\n"
    );
}

#[test]
fn symbol_characters_become_unicode() {
    let symbol = content(RunContent::Symbol {
        font: Some("Wingdings".into()),
        char: 0xf0fc,
    });
    let greek = text_run(
        "abg",
        RunProperties {
            fonts: FontSlots {
                ascii: Some("Symbol".into()),
                ..FontSlots::default()
            },
            ..RunProperties::default()
        },
    );
    let doc = document(vec![para(vec![symbol, run(" "), greek])]);
    assert_eq!(text(&doc), "✓ αβγ\n");
}

#[test]
fn headers_footers_and_comments_on_request() {
    let mut doc = document(vec![para(vec![
        run("body"),
        content(RunContent::CommentReference(3)),
    ])]);
    doc.sections[0].properties.headers.default = Some("rId1".into());
    doc.sections[0].properties.footers.default = Some("rId2".into());
    doc.headers_footers = vec![
        HeaderFooter {
            id: "rId1".into(),
            kind: HeaderFooterKind::Header,
            blocks: vec![para(vec![run("head")])],
        },
        HeaderFooter {
            id: "rId2".into(),
            kind: HeaderFooterKind::Footer,
            blocks: vec![para(vec![run("foot")])],
        },
    ];
    doc.comments = vec![Comment {
        id: 3,
        author: Some("Ada".into()),
        initials: None,
        date: None,
        blocks: vec![para(vec![run("check this")])],
    }];
    assert_eq!(text(&doc), "body\n");
    let options = TextOptions {
        headers_footers: true,
        comments: true,
        ..TextOptions::default()
    };
    assert_eq!(
        to_text(&doc, &options),
        "head\nbody[c1]\nfoot\n\n[c1] Ada: check this\n"
    );
}

/// ECMA-376 Part 1 §20.4.2.38: a text box's content is part of the document
/// text, written after the paragraph that anchors it, in every output.
#[test]
fn text_box_content_follows_its_anchor() {
    let doc = document(vec![
        para(vec![
            run("Anchor"),
            text_box(vec![para(vec![run("Inside the box")])]),
        ]),
        para(vec![run("After")]),
        table(vec![row(vec![cell(vec![para(vec![
            run("Cell"),
            text_box(vec![para(vec![run("boxed cell")])]),
        ])])])]),
    ]);
    assert_eq!(
        text(&doc),
        "Anchor\nInside the box\nAfter\nCell boxed cell\n"
    );
    let markdown = docboss_output::to_markdown(&doc, &docboss_output::MarkdownOptions::default());
    assert!(
        markdown.contains("Anchor\n\nInside the box\n\nAfter"),
        "{markdown}"
    );
    let html = docboss_output::to_html(&doc, &docboss_output::HtmlOptions::default());
    assert!(
        html.contains("<p>Anchor</p>\n<p>Inside the box</p>\n<p>After</p>"),
        "{html}"
    );
    let views = docboss_output::blocks_view(&doc);
    let texts: Vec<&str> = views.iter().map(|view| view.text.as_str()).collect();
    assert_eq!(texts[..3], ["Anchor", "Inside the box", "After"]);
}

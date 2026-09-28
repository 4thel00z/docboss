mod common;

use common::*;
use docboss_output::{blocks_view, BlockKind};

#[test]
fn blocks_view_flattens_roles_and_text() {
    let doc = document(vec![
        styled("Heading1", vec![run("Intro")]),
        para(vec![]),
        item(1, 0, vec![run("first")]),
        table(vec![row(vec![
            cell(vec![para(vec![run("a")])]),
            cell(vec![para(vec![run("b  c")])]),
        ])]),
    ]);
    let views = blocks_view(&doc);
    let summary: Vec<(BlockKind, Option<u8>, Option<String>, &str)> = views
        .iter()
        .map(|v| {
            (
                v.kind,
                v.heading_level,
                v.list_label.clone(),
                v.text.as_str(),
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            (BlockKind::Heading, Some(1), None, "Intro"),
            (BlockKind::ListItem, None, Some("1.".into()), "first"),
            (BlockKind::Table, None, None, "a\tb c"),
        ]
    );
}

#[cfg(feature = "serde")]
#[test]
fn json_views_serialize() {
    let doc = document(vec![styled("Heading1", vec![run("Intro")])]);
    let json = docboss_output::blocks_json(&doc, false).unwrap();
    assert_eq!(
        json,
        r#"[{"kind":"heading","section":0,"style":"Heading1","heading_level":1,"list_level":null,"list_label":null,"text":"Intro"}]"#
    );
    let full = docboss_output::to_json(&doc, false).unwrap();
    assert!(full.contains(r#""format":"docx""#));
    assert!(full.contains(r#""Intro""#));
}

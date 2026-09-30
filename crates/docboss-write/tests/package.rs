mod common;

use common::{kitchen_sink, part, unzip};

/// Identical documents serialize to identical bytes.
#[test]
fn output_is_deterministic() {
    let document = kitchen_sink();
    let first = docboss_write::to_bytes(&document).unwrap();
    let second = docboss_write::to_bytes(&kitchen_sink()).unwrap();
    assert_eq!(first, second);
}

/// ECMA-376 Part 2 §7.3.4, §7.3.7.
/// ECMA-376 Part 2 §7.2.3: every part has a content type, by default
/// extension or override.
#[test]
fn every_part_has_a_content_type() {
    let bytes = docboss_write::to_bytes(&kitchen_sink()).unwrap();
    let types = part(&bytes, "[Content_Types].xml");
    for (name, _) in unzip(&bytes)
        .iter()
        .filter(|(n, _)| n != "[Content_Types].xml")
    {
        let extension = name.rsplit('.').next().unwrap();
        let by_default = types.contains(&format!("Extension=\"{extension}\""));
        let by_override = types.contains(&format!("PartName=\"/{name}\""));
        assert!(by_default || by_override, "{name} has no content type");
    }
}

/// ECMA-376 Part 2 §6.5: every internal relationship target exists and
/// hyperlinks are external.
#[test]
fn relationships_resolve() {
    let bytes = docboss_write::to_bytes(&kitchen_sink()).unwrap();
    let names: Vec<String> = unzip(&bytes).into_iter().map(|(n, _)| n).collect();
    let rels = part(&bytes, "word/_rels/document.xml.rels");
    for target in rels
        .split("Target=\"")
        .skip(1)
        .map(|rest| &rest[..rest.find('"').unwrap()])
    {
        if target.starts_with("http") {
            continue;
        }
        let path = format!("word/{target}");
        assert!(names.contains(&path), "{path} missing");
    }
    assert!(rels.contains("Target=\"https://example.com/a?b=1&amp;c=2\" TargetMode=\"External\""));
}

/// ECMA-376 Part 1 §17.13.5.14 and §17.11.23: deleted runs carry
/// `w:delText`, and the notes part opens with the two separator notes.
#[test]
fn revisions_and_notes_use_their_elements() {
    let bytes = docboss_write::to_bytes(&kitchen_sink()).unwrap();
    let body = part(&bytes, "word/document.xml");
    assert!(body.contains("<w:del w:id="));
    assert!(body.contains("<w:delText xml:space=\"preserve\"> deleted</w:delText>"));
    assert!(body.contains("<w:ins w:id="));
    assert!(body.contains("<w:footnoteReference w:id=\"1\"/>"));
    let notes = part(&bytes, "word/footnotes.xml");
    let separator = notes.find("w:type=\"separator\" w:id=\"-1\"").unwrap();
    let continuation = notes
        .find("w:type=\"continuationSeparator\" w:id=\"0\"")
        .unwrap();
    let first = notes.find("<w:footnote w:id=\"1\">").unwrap();
    assert!(separator < continuation && continuation < first);
    assert!(notes.contains("<w:footnoteRef/>"));
}

/// ECMA-376 Part 1 §17.6.17: a section other than the last ends with a
/// paragraph whose properties hold its `w:sectPr`; the last closes the body.
#[test]
fn sections_close_where_the_schema_puts_them() {
    let bytes = docboss_write::to_bytes(&kitchen_sink()).unwrap();
    let body = part(&bytes, "word/document.xml");
    assert_eq!(body.matches("<w:sectPr>").count(), 2);
    let first = body.find("<w:sectPr>").unwrap();
    let close = first + body[first..].find("</w:sectPr>").unwrap();
    assert!(body[close..].starts_with("</w:sectPr></w:pPr>"));
    assert!(body.ends_with("</w:sectPr></w:body></w:document>"));
    assert!(body.contains("w:orient=\"landscape\""));
    assert!(body.contains("<w:headerReference w:type=\"default\" r:id=\""));
}

/// ECMA-376 Part 1 §17.6.10: page borders are written after the margins
/// with where they are measured from, their pages and their depth.
#[test]
fn page_borders_follow_the_margins() {
    let mut builder = docboss_write::DocumentBuilder::new();
    builder.text("Bordered.");
    let mut document = builder.build();
    let side = docboss_model::Border {
        shadow: false,
        style: docboss_model::BorderStyle::Dashed,
        size: 8,
        space: 24,
        color: None,
    };
    document.sections[0].properties.page_borders = Some(docboss_model::PageBorders {
        sides: docboss_model::Borders {
            top: Some(side),
            ..Default::default()
        },
        offset_from: docboss_model::PageBorderOffset::Page,
        ..Default::default()
    });
    let body = part(
        &docboss_write::to_bytes(&document).unwrap(),
        "word/document.xml",
    );
    let margins = body.find("<w:pgMar").unwrap();
    let borders = body
        .find("<w:pgBorders w:offsetFrom=\"page\" w:display=\"allPages\" w:zOrder=\"front\"><w:top w:val=\"dashed\" w:sz=\"8\" w:space=\"24\" w:color=\"auto\"/></w:pgBorders>")
        .unwrap();
    assert!(margins < borders);
}

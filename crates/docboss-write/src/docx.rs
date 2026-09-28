//! Assembles the package: every part, its relationships and the content
//! types, zipped in a fixed order.

use std::collections::BTreeMap;

use docboss_model::{Block, Document, HeaderFooterKind, NoteKind};

use crate::body::{Shared, Story, StoryKind};
use crate::package::{
    media_extension, office_rel, ContentTypes, Rels, CORE_PROPERTIES_REL, NS_A, NS_PIC, NS_R, NS_W,
    NS_WP, WML,
};
use crate::parts;
use crate::props;
use crate::Result;
use docboss_zip::{Method, ZipWriter};

const REVISION_ID_BASE: i64 = 1 << 20;

fn story_root(xml: &mut crate::xml::Xml, name: &str) {
    xml.open(
        name,
        &[
            ("xmlns:w", NS_W),
            ("xmlns:r", NS_R),
            ("xmlns:wp", NS_WP),
            ("xmlns:a", NS_A),
            ("xmlns:pic", NS_PIC),
        ],
    );
}

fn renumber(document: &Document, kind: NoteKind) -> BTreeMap<i64, i64> {
    let notes = match kind {
        NoteKind::Footnote => &document.footnotes,
        NoteKind::Endnote => &document.endnotes,
    };
    notes
        .iter()
        .zip(1..)
        .map(|(note, id)| (note.id, id))
        .collect()
}

/// ECMA-376 Part 1 §17.11.23: the footnotes (or endnotes) part opens with
/// the separator and continuation separator notes Word requires.
fn notes_part(shared: &mut Shared<'_>, kind: NoteKind) -> (Vec<u8>, Rels) {
    let (root, item, story_kind) = match kind {
        NoteKind::Footnote => ("w:footnotes", "w:footnote", StoryKind::Footnote),
        NoteKind::Endnote => ("w:endnotes", "w:endnote", StoryKind::Endnote),
    };
    let document = shared.document;
    let notes = match kind {
        NoteKind::Footnote => &document.footnotes,
        NoteKind::Endnote => &document.endnotes,
    };
    let mut rels = Rels::default();
    let mut story = Story::new(shared, &mut rels, story_kind);
    story_root(&mut story.xml, root);
    for (id, separator) in [("-1", "separator"), ("0", "continuationSeparator")] {
        story.xml.open(item, &[("w:type", separator), ("w:id", id)]);
        story.xml.open("w:p", &[]);
        story.xml.open("w:pPr", &[]);
        story.xml.empty(
            "w:spacing",
            &[("w:after", "0"), ("w:line", "240"), ("w:lineRule", "auto")],
        );
        story.xml.close("w:pPr");
        story.xml.open("w:r", &[]);
        story.xml.empty(&format!("w:{separator}"), &[]);
        story.xml.close("w:r");
        story.xml.close("w:p");
        story.xml.close(item);
    }
    for (note, id) in notes.iter().zip(1..) {
        story.xml.open(item, &[("w:id", &id.to_string())]);
        story.blocks(&note.blocks, true);
        story.xml.close(item);
    }
    story.xml.close(root);
    let bytes = story.finish();
    (bytes, rels)
}

fn comments_part(shared: &mut Shared<'_>) -> (Vec<u8>, Rels) {
    let document = shared.document;
    let mut rels = Rels::default();
    let mut story = Story::new(shared, &mut rels, StoryKind::Comment);
    story_root(&mut story.xml, "w:comments");
    for comment in &document.comments {
        let id = comment.id.to_string();
        let mut attributes = vec![
            ("w:id", id.as_str()),
            ("w:author", comment.author.as_deref().unwrap_or("")),
        ];
        if let Some(date) = comment.date.as_deref() {
            attributes.push(("w:date", date));
        }
        if let Some(initials) = comment.initials.as_deref() {
            attributes.push(("w:initials", initials));
        }
        story.xml.open("w:comment", &attributes);
        story.blocks(&comment.blocks, true);
        story.xml.close("w:comment");
    }
    story.xml.close("w:comments");
    let bytes = story.finish();
    (bytes, rels)
}

struct Part {
    name: String,
    data: Vec<u8>,
    rels: Option<Rels>,
    method: Method,
}

impl Part {
    fn xml(name: impl Into<String>, data: Vec<u8>, rels: Option<Rels>) -> Self {
        Self {
            name: name.into(),
            data,
            rels: rels.filter(|r| !r.is_empty()),
            method: Method::Deflated,
        }
    }
}

fn rels_name(part: &str) -> String {
    match part.rsplit_once('/') {
        Some((dir, file)) => format!("{dir}/_rels/{file}.rels"),
        None => format!("_rels/{part}.rels"),
    }
}

/// Serializes a document into DOCX bytes.
pub fn to_bytes(document: &Document) -> Result<Vec<u8>> {
    let mut types = ContentTypes::new();
    let media_targets: Vec<String> = document
        .media
        .iter()
        .zip(1..)
        .map(|(media, n)| format!("media/image{n}.{}", media_extension(&media.content_type)))
        .collect();
    let mut shared = Shared {
        document,
        media_targets,
        footnote_ids: renumber(document, NoteKind::Footnote),
        endnote_ids: renumber(document, NoteKind::Endnote),
        drawing_id: 0,
        revision_id: REVISION_ID_BASE,
    };
    let mut parts: Vec<Part> = Vec::new();
    let mut document_rels = Rels::default();
    document_rels.add(&office_rel("styles"), "styles.xml", false);
    document_rels.add(&office_rel("settings"), "settings.xml", false);
    document_rels.add(&office_rel("fontTable"), "fontTable.xml", false);
    types.override_part("/word/styles.xml", &format!("{WML}styles+xml"));
    types.override_part("/word/settings.xml", &format!("{WML}settings+xml"));
    types.override_part("/word/fontTable.xml", &format!("{WML}fontTable+xml"));
    parts.push(Part::xml(
        "word/styles.xml",
        parts::styles(&document.styles),
        None,
    ));
    parts.push(Part::xml(
        "word/settings.xml",
        parts::settings(document),
        None,
    ));
    parts.push(Part::xml(
        "word/fontTable.xml",
        parts::font_table(document),
        None,
    ));
    let has_numbering = !document.numbering.instances.is_empty();
    if has_numbering {
        document_rels.add(&office_rel("numbering"), "numbering.xml", false);
        types.override_part("/word/numbering.xml", &format!("{WML}numbering+xml"));
        parts.push(Part::xml(
            "word/numbering.xml",
            parts::numbering(&document.numbering),
            None,
        ));
    }
    let note_kinds = [
        (
            NoteKind::Footnote,
            !document.footnotes.is_empty(),
            "footnotes",
        ),
        (NoteKind::Endnote, !document.endnotes.is_empty(), "endnotes"),
    ];
    for (kind, present, name) in note_kinds {
        if !present {
            continue;
        }
        document_rels.add(&office_rel(name), &format!("{name}.xml"), false);
        types.override_part(&format!("/word/{name}.xml"), &format!("{WML}{name}+xml"));
        let (data, rels) = notes_part(&mut shared, kind);
        parts.push(Part::xml(format!("word/{name}.xml"), data, Some(rels)));
    }
    if !document.comments.is_empty() {
        document_rels.add(&office_rel("comments"), "comments.xml", false);
        types.override_part("/word/comments.xml", &format!("{WML}comments+xml"));
        let (data, rels) = comments_part(&mut shared);
        parts.push(Part::xml("word/comments.xml", data, Some(rels)));
    }
    let mut header_rels: BTreeMap<String, String> = BTreeMap::new();
    let (mut headers, mut footers) = (0, 0);
    for part in &document.headers_footers {
        let (kind, root, counter) = match part.kind {
            HeaderFooterKind::Header => ("header", "w:hdr", &mut headers),
            HeaderFooterKind::Footer => ("footer", "w:ftr", &mut footers),
        };
        *counter += 1;
        let file = format!("{kind}{counter}.xml");
        let rid = document_rels.add(&office_rel(kind), &file, false);
        header_rels.insert(part.id.clone(), rid);
        types.override_part(&format!("/word/{file}"), &format!("{WML}{kind}+xml"));
        let mut rels = Rels::default();
        let mut story = Story::new(&mut shared, &mut rels, StoryKind::HeaderFooter);
        story_root(&mut story.xml, root);
        story.blocks(&part.blocks, true);
        story.xml.close(root);
        let data = story.finish();
        parts.push(Part::xml(format!("word/{file}"), data, Some(rels)));
    }
    let main = main_document(&mut shared, &mut document_rels, &header_rels);
    types.override_part("/word/document.xml", &format!("{WML}document.main+xml"));
    for (media, target) in document.media.iter().zip(&shared.media_targets) {
        let extension = media_extension(&media.content_type);
        let name = format!("word/{target}");
        match types.default_type(extension) {
            None if extension != "bin" => types.add_default(extension, &media.content_type),
            Some(existing) if existing == media.content_type => {}
            _ => types.override_part(&format!("/{name}"), &media.content_type),
        }
        parts.push(Part {
            name,
            data: media.data.to_vec(),
            rels: None,
            method: Method::Stored,
        });
    }
    types.override_part(
        "/docProps/core.xml",
        "application/vnd.openxmlformats-package.core-properties+xml",
    );
    types.override_part(
        "/docProps/app.xml",
        "application/vnd.openxmlformats-officedocument.extended-properties+xml",
    );
    let mut package_rels = Rels::default();
    package_rels.add(&office_rel("officeDocument"), "word/document.xml", false);
    package_rels.add(CORE_PROPERTIES_REL, "docProps/core.xml", false);
    package_rels.add(
        &office_rel("extended-properties"),
        "docProps/app.xml",
        false,
    );

    let mut zip = ZipWriter::new();
    zip.add("[Content_Types].xml", &types.to_xml(), Method::Deflated)?;
    zip.add("_rels/.rels", &package_rels.to_xml(), Method::Deflated)?;
    zip.add(
        "docProps/core.xml",
        &parts::core_properties(&document.metadata),
        Method::Deflated,
    )?;
    zip.add(
        "docProps/app.xml",
        &parts::app_properties(&document.metadata),
        Method::Deflated,
    )?;
    zip.add("word/document.xml", &main, Method::Deflated)?;
    zip.add(
        "word/_rels/document.xml.rels",
        &document_rels.to_xml(),
        Method::Deflated,
    )?;
    for part in parts {
        zip.add(&part.name, &part.data, part.method)?;
        if let Some(rels) = part.rels {
            zip.add(&rels_name(&part.name), &rels.to_xml(), Method::Deflated)?;
        }
    }
    Ok(zip.finish()?)
}

/// ECMA-376 Part 1 §17.2.2 and §17.6.17: the body, where every section but
/// the last ends with a paragraph carrying its `<w:sectPr>` and the last
/// section's properties close the body.
fn main_document(
    shared: &mut Shared<'_>,
    rels: &mut Rels,
    header_rels: &BTreeMap<String, String>,
) -> Vec<u8> {
    let document = shared.document;
    let relationship = |id: &str| header_rels.get(id).cloned();
    let mut story = Story::new(shared, rels, StoryKind::Main);
    story_root(&mut story.xml, "w:document");
    story.xml.open("w:body", &[]);
    let count = document.sections.len();
    for (index, section) in document.sections.iter().enumerate() {
        let rendered = props::section_properties(&section.properties, &relationship);
        let last = index + 1 == count;
        if last {
            story.blocks(&section.blocks, false);
            story.xml.out.push_str(&rendered);
            continue;
        }
        let Some((Block::Paragraph(closing), rest)) = section.blocks.split_last() else {
            story.blocks(&section.blocks, false);
            story.paragraph(&Default::default(), Some(&rendered));
            continue;
        };
        story.blocks(rest, false);
        story.paragraph(closing, Some(&rendered));
    }
    if count == 0 {
        let rendered = props::section_properties(&Default::default(), &relationship);
        story.xml.out.push_str(&rendered);
    }
    story.xml.close("w:body");
    story.xml.close("w:document");
    story.finish()
}

//! A DOCX reader: Office Open XML WordprocessingML (ECMA-376) into the
//! [`docboss_model`] document model.
//!
//! [`read`] opens the ZIP package, follows its relationships from the main
//! document to styles, numbering, notes, comments, headers, footers, fonts,
//! theme, settings and media, and parses independent parts on separate
//! threads when the document is large enough to pay for them. Transitional
//! and strict documents read alike. The reader is lenient: missing or
//! damaged parts are skipped and reported in [`Document::diagnostics`].

mod error;
mod geometry;
mod package;
mod parts;
mod props;
mod story;
mod xml;

use std::collections::HashMap;
use std::sync::Arc;

use docboss_model::{
    sniff_image, Block, Comment, Diagnostic, Document, FontEntry, HeaderFooter, HeaderFooterKind,
    Media, MediaId, Metadata, Note, NoteKind, Numbering, Section, Settings, SourceFormat, Styles,
};
use docboss_xml::{decode, Ns, Reader};
use docboss_zip::{Archive, Limits};

pub use error::{Error, Result};
pub use package::{rels_name, resolve_target, Package, Relationship, Relationships};
pub use props::Theme;

use story::{Context, StoryParser};
use xml::{attr, children, int_attr, root};

/// How [`read_with`] reads a package.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    pub limits: Limits,
    /// Parse independent parts on separate threads for main documents
    /// larger than `parallel_threshold` bytes.
    pub parallel: bool,
    pub parallel_threshold: u64,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            limits: Limits::default(),
            parallel: true,
            parallel_threshold: 256 << 10,
        }
    }
}

/// Reads a DOCX (or DOCM, DOTX, DOTM) package.
pub fn read(data: &[u8]) -> Result<Document> {
    read_with(data, Options::default())
}

const MAIN_TYPES: [&str; 4] = [
    "wordprocessingml.document.main+xml",
    "wordprocessingml.template.main+xml",
    "ms-word.document.macroEnabled.main+xml",
    "ms-word.template.macroEnabledTemplate.main+xml",
];

/// ECMA-376 Part 1 §11.3.10: the main document part, from the package's officeDocument relationship.
fn main_part(package: &Package<'_>, root_rels: &Relationships) -> Option<String> {
    let by_rel = root_rels
        .first("officeDocument")
        .map(|rel| rel.target.clone())
        .filter(|name| package.archive.contains(name));
    by_rel
        .or_else(|| {
            MAIN_TYPES
                .iter()
                .find_map(|suffix| package.part_with_type(suffix))
        })
        .or_else(|| {
            package
                .archive
                .find("word/document.xml")
                .map(|entry| entry.name.clone())
        })
}

/// One story part to parse: the main text, a header, footer, the notes or
/// the comments.
struct StoryPart {
    name: String,
    rels: Relationships,
    kind: StoryKind,
}

#[derive(Clone)]
enum StoryKind {
    Body,
    HeaderFooter { id: String, kind: HeaderFooterKind },
    Notes(NoteKind),
    Comments,
}

#[derive(Default)]
struct StoryOutput {
    sections: Vec<Section>,
    header_footer: Option<HeaderFooter>,
    notes: Vec<Note>,
    comments: Vec<Comment>,
    diagnostics: Vec<Diagnostic>,
}

fn parse_story(
    package: &Package<'_>,
    part: &StoryPart,
    media: &HashMap<String, MediaId>,
    theme: &Theme,
) -> StoryOutput {
    let mut out = StoryOutput::default();
    let Some(bytes) = package.part_into(&part.name, &mut out.diagnostics) else {
        out.diagnostics.push(Diagnostic::dropped(
            part.name.as_str(),
            "the part is missing from the package",
        ));
        return out;
    };
    let text = decode(&bytes);
    let mut reader = Reader::new(&text);
    let ctx = Context {
        part: &part.name,
        rels: &part.rels,
        media,
        theme,
        package,
    };
    let mut parser = StoryParser::new(ctx);
    if root(&mut reader).is_none() {
        out.diagnostics.push(Diagnostic::dropped(
            part.name.as_str(),
            "the part holds no XML element",
        ));
        return out;
    }
    match &part.kind {
        // ECMA-376 Part 1 §17.2.3, §17.2.2: the document's body.
        StoryKind::Body => {
            children(&mut reader, |reader, e| {
                if e.is(Ns::W, "body") {
                    out.sections.extend(parser.body(reader));
                }
            });
            if out.sections.is_empty() {
                out.sections.push(Section::default());
            }
        }
        // ECMA-376 Part 1 §17.10.4, §17.10.3, §11.3.9, §11.3.6: header and footer parts.
        StoryKind::HeaderFooter { id, kind } => {
            let blocks = parser.blocks(&mut reader);
            out.header_footer = Some(HeaderFooter {
                id: id.clone(),
                kind: *kind,
                blocks,
            });
        }
        // ECMA-376 Part 1 §17.11.15, §17.11.8, §17.11.10, §17.11.2, §17.11.23, §17.11.1, §11.3.7, §11.3.4: note bodies, separators skipped.
        StoryKind::Notes(kind) => children(&mut reader, |reader, e| {
            if !matches!(e.local, "footnote" | "endnote") {
                return;
            }
            let separator = matches!(
                attr(&e, "type").as_deref(),
                Some("separator" | "continuationSeparator" | "continuationNotice")
            );
            if separator {
                return;
            }
            let id = int_attr(&e, "id").unwrap_or(-1);
            let blocks = parser.blocks(reader);
            out.notes.push(Note {
                id,
                kind: *kind,
                blocks,
            });
        }),
        // ECMA-376 Part 1 §11.3.2, §17.13.4: the comments part.
        StoryKind::Comments => children(&mut reader, |reader, e| {
            if e.local != "comment" {
                return;
            }
            let id = int_attr(&e, "id").unwrap_or(-1);
            let author = attr(&e, "author").map(Into::into);
            let initials = attr(&e, "initials").map(Into::into);
            let date = attr(&e, "date").map(Into::into);
            let blocks: Vec<Block> = parser.blocks(reader);
            out.comments.push(Comment {
                id,
                author,
                initials,
                date,
                blocks,
            });
        }),
    }
    out.diagnostics.append(&mut parser.diagnostics);
    out
}

#[derive(Default)]
struct Shared {
    settings: Settings,
    metadata: Metadata,
    fonts: Vec<FontEntry>,
    diagnostics: Vec<Diagnostic>,
}

fn part_text(
    package: &Package<'_>,
    name: Option<&str>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<String> {
    let name = name?;
    let bytes = package.part_into(name, diagnostics)?;
    Some(decode(&bytes).into_owned())
}

fn parse_styles(
    package: &Package<'_>,
    main_rels: &Relationships,
    theme: &Theme,
) -> (Styles, Vec<Diagnostic>) {
    let mut diagnostics = Vec::new();
    let text = part_text(
        package,
        main_rels.first("styles").map(|r| r.target.as_str()),
        &mut diagnostics,
    );
    (
        text.map(|t| parts::styles(&t, theme)).unwrap_or_default(),
        diagnostics,
    )
}

fn parse_numbering(
    package: &Package<'_>,
    main_rels: &Relationships,
    theme: &Theme,
) -> (Numbering, Vec<(i64, String)>, Vec<Diagnostic>) {
    let mut diagnostics = Vec::new();
    let text = part_text(
        package,
        main_rels.first("numbering").map(|r| r.target.as_str()),
        &mut diagnostics,
    );
    let (numbering, links) = text
        .map(|t| parts::numbering(&t, theme))
        .unwrap_or_default();
    (numbering, links, diagnostics)
}

/// ECMA-376 Part 1 §11.3.12, §11.3.11, §11.3.3, §11.3.5: the styles, numbering, settings and font table parts.
fn parse_misc(
    package: &Package<'_>,
    root_rels: &Relationships,
    main_rels: &Relationships,
) -> Shared {
    let mut shared = Shared::default();
    let d = &mut shared.diagnostics;
    if let Some(text) = part_text(
        package,
        main_rels.first("settings").map(|r| r.target.as_str()),
        d,
    ) {
        shared.settings = parts::settings(&text);
    }
    let core_name = root_rels
        .first("core-properties")
        .map(|r| r.target.clone())
        .or_else(|| Some("docProps/core.xml".to_string()));
    let app_name = root_rels
        .first("extended-properties")
        .map(|r| r.target.clone())
        .or_else(|| Some("docProps/app.xml".to_string()));
    let core = core_name
        .filter(|n| package.archive.contains(n))
        .and_then(|n| part_text(package, Some(&n), d));
    let app = app_name
        .filter(|n| package.archive.contains(n))
        .and_then(|n| part_text(package, Some(&n), d));
    shared.metadata = parts::properties(core.as_deref(), app.as_deref());
    if let Some(font_table) = main_rels.first("fontTable").map(|r| r.target.clone()) {
        let rels = package.relationships(&font_table, d);
        if let Some(text) = part_text(package, Some(&font_table), d) {
            shared.fonts = parts::fonts(&text, &rels, package, d);
        }
    }
    shared
}

/// The image parts the stories refer to (ECMA-376 Part 1 §15.2.14).
fn load_media(package: &Package<'_>, names: &[String]) -> (Vec<Media>, Vec<Diagnostic>) {
    let mut diagnostics = Vec::new();
    let media = names
        .iter()
        .map(|name| {
            let data: Arc<[u8]> = match package.part_into(name, &mut diagnostics) {
                Some(bytes) => Arc::from(bytes.into_owned()),
                None => {
                    diagnostics.push(Diagnostic::dropped(
                        name.as_str(),
                        "the image part is missing",
                    ));
                    Arc::from(Vec::new())
                }
            };
            let sniffed = sniff_image(&data);
            let content_type = match sniffed {
                "application/octet-stream" => {
                    package.content_type(name).unwrap_or(sniffed).to_string()
                }
                known => known.to_string(),
            };
            Media {
                name: name.clone(),
                content_type,
                data,
            }
        })
        .collect();
    (media, diagnostics)
}

/// Reads a package with explicit options.
pub fn read_with(data: &[u8], options: Options) -> Result<Document> {
    if data.starts_with(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]) {
        return Err(Error::Encrypted);
    }
    let archive = Archive::with_limits(data, options.limits)?;
    let package = Package::open(archive);
    let mut diagnostics = package.diagnostics.clone();
    let root_rels = package.relationships("", &mut diagnostics);
    let main = main_part(&package, &root_rels)
        .ok_or_else(|| Error::NotWordDocument("no main document part".into()))?;
    let main_rels = package.relationships(&main, &mut diagnostics);

    let theme = match part_text(
        &package,
        main_rels.first("theme").map(|r| r.target.as_str()),
        &mut diagnostics,
    ) {
        Some(text) => parts::theme(&text),
        None => Theme::default(),
    };

    let mut stories = vec![StoryPart {
        name: main.clone(),
        rels: main_rels.clone(),
        kind: StoryKind::Body,
    }];
    for rel in main_rels.list.iter().filter(|rel| !rel.external) {
        let kind = match rel.kind.as_str() {
            "header" => StoryKind::HeaderFooter {
                id: rel.id.clone(),
                kind: HeaderFooterKind::Header,
            },
            "footer" => StoryKind::HeaderFooter {
                id: rel.id.clone(),
                kind: HeaderFooterKind::Footer,
            },
            "footnotes" => StoryKind::Notes(NoteKind::Footnote),
            "endnotes" => StoryKind::Notes(NoteKind::Endnote),
            "comments" => StoryKind::Comments,
            _ => continue,
        };
        let rels = package.relationships(&rel.target, &mut diagnostics);
        stories.push(StoryPart {
            name: rel.target.clone(),
            rels,
            kind,
        });
    }
    let drawing_rels: Vec<Relationships> = stories
        .iter()
        .flat_map(|story| story.rels.list.iter())
        .filter(|rel| !rel.external && rel.kind.eq_ignore_ascii_case("diagramDrawing"))
        .map(|rel| package.relationships(&rel.target, &mut diagnostics))
        .collect();
    let mut media_names: Vec<String> = stories
        .iter()
        .flat_map(|story| story.rels.list.iter())
        .chain(drawing_rels.iter().flat_map(|rels| rels.list.iter()))
        .filter(|rel| !rel.external && rel.kind.eq_ignore_ascii_case("image"))
        .map(|rel| rel.target.clone())
        .filter(|name| package.archive.contains(name))
        .collect();
    media_names.sort();
    media_names.dedup();
    let media_ids: HashMap<String, MediaId> = media_names
        .iter()
        .enumerate()
        .map(|(i, name)| (name.clone(), MediaId(i as u32)))
        .collect();

    let main_size = package
        .archive
        .find(&main)
        .map_or(0, |entry| entry.uncompressed_size);
    let parallel = options.parallel && main_size > options.parallel_threshold;

    let body = || parse_story(&package, &stories[0], &media_ids, &theme);
    let others = || {
        stories[1..]
            .iter()
            .map(|story| parse_story(&package, story, &media_ids, &theme))
            .collect::<Vec<_>>()
    };
    let styles = || parse_styles(&package, &main_rels, &theme);
    let numbering = || parse_numbering(&package, &main_rels, &theme);
    let misc = || parse_misc(&package, &root_rels, &main_rels);
    let media = || load_media(&package, &media_names);

    let (body, others, styles, numbering, misc, media) = match parallel {
        true => std::thread::scope(|scope| {
            let others = scope.spawn(others);
            let styles = scope.spawn(styles);
            let numbering = scope.spawn(numbering);
            let misc = scope.spawn(misc);
            let media = scope.spawn(media);
            let body = body();
            (
                body,
                others.join().unwrap_or_default(),
                styles.join().unwrap_or_default(),
                numbering.join().unwrap_or_default(),
                misc.join().unwrap_or_default(),
                media.join().unwrap_or_default(),
            )
        }),
        false => (body(), others(), styles(), numbering(), misc(), media()),
    };

    let (styles, style_diagnostics) = styles;
    let (mut numbering, links, numbering_diagnostics) = numbering;
    parts::resolve_style_links(&mut numbering, &links, &styles);
    let (media, media_diagnostics) = media;

    let mut document = Document {
        format: SourceFormat::Docx,
        metadata: misc.metadata,
        settings: misc.settings,
        styles,
        numbering,
        sections: body.sections,
        media,
        fonts: misc.fonts,
        ..Document::default()
    };
    diagnostics.extend(body.diagnostics);
    for story in others {
        document.headers_footers.extend(story.header_footer);
        for note in story.notes {
            match note.kind {
                NoteKind::Footnote => document.footnotes.push(note),
                NoteKind::Endnote => document.endnotes.push(note),
            }
        }
        document.comments.extend(story.comments);
        diagnostics.extend(story.diagnostics);
    }
    diagnostics.extend(style_diagnostics);
    diagnostics.extend(numbering_diagnostics);
    diagnostics.extend(misc.diagnostics);
    diagnostics.extend(media_diagnostics);
    document.diagnostics = diagnostics;
    Ok(document)
}

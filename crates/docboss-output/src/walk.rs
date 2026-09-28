//! The pass shared by every output format: effective formatting per run,
//! paragraph roles, list labels, note numbering, and the flattening of a
//! paragraph's inline tree into a sequence of pieces.

use std::borrow::Cow;
use std::collections::HashMap;

use docboss_model::{
    Document, Drawing, Field, Inline, NoteKind, NumberFormat, NumberingCounter, NumberingRef,
    Paragraph, RevisionKind, RunContent, RunProperties, Underline, VerticalAlign,
};

use crate::symbol::{self, SymbolFont};

/// The formatting of a piece of text that the output formats can express.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Fmt {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub superscript: bool,
    pub subscript: bool,
    pub mono: bool,
}

#[derive(Debug, Clone, Copy)]
struct RunInfo {
    fmt: Fmt,
    hidden: bool,
    symbol: Option<SymbolFont>,
}

/// What a paragraph is, from its style and direct properties.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ParagraphInfo {
    pub numbering: Option<NumberingRef>,
    pub heading: Option<u8>,
    pub code: bool,
}

const MONOSPACE: [&str; 12] = [
    "courier",
    "consolas",
    "mono",
    "menlo",
    "monaco",
    "lucida console",
    "lucida sans typewriter",
    "source code",
    "fira code",
    "inconsolata",
    "andale",
    "fixedsys",
];

fn is_monospace(font: &str) -> bool {
    let font = font.to_ascii_lowercase();
    MONOSPACE.iter().any(|name| font.contains(name))
}

const CODE_STYLES: [&str; 7] = [
    "code",
    "sourcecode",
    "htmlpreformatted",
    "preformatted",
    "plaintext",
    "codeblock",
    "macrotext",
];

/// Caches effective style formatting, keyed by style ids borrowed from the
/// document, so each distinct style combination is resolved once.
pub(crate) struct Resolver<'a> {
    pub doc: &'a Document,
    runs: HashMap<(Option<&'a str>, Option<&'a str>), RunProperties>,
    paragraphs: HashMap<Option<&'a str>, ParagraphInfo>,
}

impl<'a> Resolver<'a> {
    pub(crate) fn new(doc: &'a Document) -> Self {
        Self {
            doc,
            runs: HashMap::new(),
            paragraphs: HashMap::new(),
        }
    }

    /// ECMA-376 Part 1 §17.9.23: a paragraph's list membership comes from
    /// its own `numPr` or its style's, and `numId` 0 removes it.
    pub(crate) fn paragraph(&mut self, paragraph: &'a Paragraph) -> ParagraphInfo {
        let doc = self.doc;
        let key = paragraph.style_id.as_deref();
        let base = *self.paragraphs.entry(key).or_insert_with(|| {
            let probe = Paragraph {
                style_id: key.map(str::to_string),
                ..Paragraph::default()
            };
            let resolved = doc.styles.resolve_paragraph(&probe, &doc.numbering);
            let code = key.is_some_and(|id| {
                doc.styles.chain(id).iter().any(|style| {
                    let name = style.name.as_deref().unwrap_or(&style.id);
                    let name: String = name
                        .chars()
                        .filter(|c| !c.is_whitespace())
                        .collect::<String>()
                        .to_ascii_lowercase();
                    CODE_STYLES.contains(&name.as_str())
                })
            });
            ParagraphInfo {
                numbering: resolved.numbering,
                heading: doc.styles.heading_level(&probe, &doc.numbering),
                code,
            }
        });
        let numbering = paragraph
            .properties
            .numbering
            .or(base.numbering)
            .filter(|reference| reference.num_id != 0);
        let heading = match paragraph.properties.outline_level {
            Some(level) if level < 9 => Some(level),
            Some(_) => None,
            None => base.heading,
        };
        ParagraphInfo {
            numbering,
            heading,
            code: base.code,
        }
    }

    fn run(&mut self, paragraph_style: Option<&'a str>, direct: &'a RunProperties) -> RunInfo {
        let doc = self.doc;
        let character_style = direct.style_id.as_deref();
        let base = self
            .runs
            .entry((paragraph_style, character_style))
            .or_insert_with(|| {
                let probe = RunProperties {
                    style_id: character_style.map(str::to_string),
                    ..RunProperties::default()
                };
                doc.styles.resolve_run(paragraph_style, &probe)
            });
        let flag = |direct: Option<bool>, base: Option<bool>| direct.or(base).unwrap_or(false);
        let vertical = direct.vertical_align.or(base.vertical_align);
        let underline = direct
            .underline
            .or(base.underline)
            .is_some_and(|u| u != Underline::None);
        let font = direct
            .fonts
            .ascii
            .as_deref()
            .or(base.fonts.ascii.as_deref());
        RunInfo {
            fmt: Fmt {
                bold: flag(direct.bold, base.bold),
                italic: flag(direct.italic, base.italic),
                underline,
                strike: flag(direct.strike, base.strike)
                    || flag(direct.double_strike, base.double_strike),
                superscript: vertical == Some(VerticalAlign::Superscript),
                subscript: vertical == Some(VerticalAlign::Subscript),
                mono: font.is_some_and(is_monospace),
            },
            hidden: flag(direct.vanish, base.vanish),
            symbol: font.and_then(SymbolFont::from_name),
        }
    }
}

/// A hyperlink target: an external address, a bookmark, or both.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Link<'a> {
    pub target: Option<Cow<'a, str>>,
    pub anchor: Option<Cow<'a, str>>,
}

impl Link<'_> {
    pub(crate) fn href(&self) -> String {
        match (&self.target, &self.anchor) {
            (Some(target), Some(anchor)) => format!("{target}#{anchor}"),
            (Some(target), None) => target.to_string(),
            (None, Some(anchor)) => format!("#{anchor}"),
            (None, None) => String::new(),
        }
    }
}

/// One step of a paragraph's content in reading order.
#[derive(Debug, Clone)]
pub(crate) enum Piece<'a> {
    Text(Cow<'a, str>, Fmt),
    Tab,
    LineBreak,
    PageBreak,
    ColumnBreak,
    Note(NoteKind, i64),
    Comment(i64),
    Image(&'a Drawing),
    LinkStart(Link<'a>),
    LinkEnd,
    Bookmark(&'a str),
}

/// ECMA-376 Part 1 §17.16.5.25: a `HYPERLINK` field's first argument is
/// the address and `\l` names a bookmark.
fn hyperlink_field(instruction: &str) -> Option<Link<'static>> {
    let rest = instruction.trim_start().strip_prefix("HYPERLINK")?;
    let mut target = None;
    let mut anchor = None;
    let mut bookmark_next = false;
    for token in field_tokens(rest) {
        if token == "\\l" {
            bookmark_next = true;
            continue;
        }
        if token.starts_with('\\') {
            continue;
        }
        if bookmark_next {
            anchor = Some(Cow::Owned(token));
            bookmark_next = false;
            continue;
        }
        if target.is_none() {
            target = Some(Cow::Owned(token));
        }
    }
    if target.is_none() && anchor.is_none() {
        return None;
    }
    Some(Link { target, anchor })
}

fn field_tokens(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
            continue;
        }
        if c != '"' {
            let mut token = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_whitespace() {
                    break;
                }
                token.push(c);
                chars.next();
            }
            tokens.push(token);
            continue;
        }
        chars.next();
        let mut token = String::new();
        for c in chars.by_ref() {
            if c == '"' {
                break;
            }
            token.push(c);
        }
        tokens.push(token);
    }
    tokens
}

/// Flattens a paragraph's inlines into `out`: hidden runs (ECMA-376 Part 1
/// §17.3.2.41) and deleted revisions (§17.13.5.14) are left out, inserted
/// revisions (§17.13.5.18) kept, and fields (§17.16) contribute their
/// cached result.
pub(crate) fn flatten<'a>(
    resolver: &mut Resolver<'a>,
    paragraph: &'a Paragraph,
    out: &mut Vec<Piece<'a>>,
) {
    out.clear();
    flatten_inlines(
        resolver,
        paragraph.style_id.as_deref(),
        &paragraph.inlines,
        out,
    );
}

fn flatten_inlines<'a>(
    resolver: &mut Resolver<'a>,
    style: Option<&'a str>,
    inlines: &'a [Inline],
    out: &mut Vec<Piece<'a>>,
) {
    for inline in inlines {
        match inline {
            Inline::Run(run) => {
                let info = resolver.run(style, &run.properties);
                if info.hidden {
                    continue;
                }
                run.content
                    .iter()
                    .for_each(|content| flatten_content(info, content, out));
            }
            Inline::Hyperlink(link) => {
                out.push(Piece::LinkStart(Link {
                    target: link.target.as_deref().map(Cow::Borrowed),
                    anchor: link.anchor.as_deref().map(Cow::Borrowed),
                }));
                flatten_inlines(resolver, style, &link.inlines, out);
                out.push(Piece::LinkEnd);
            }
            Inline::Field(field) => flatten_field(resolver, style, field, out),
            Inline::Revision(revision) => {
                if revision.kind == RevisionKind::Deletion {
                    continue;
                }
                flatten_inlines(resolver, style, &revision.inlines, out);
            }
            Inline::BookmarkStart { name, .. } => out.push(Piece::Bookmark(name)),
            _ => {}
        }
    }
}

fn flatten_field<'a>(
    resolver: &mut Resolver<'a>,
    style: Option<&'a str>,
    field: &'a Field,
    out: &mut Vec<Piece<'a>>,
) {
    let Some(link) = hyperlink_field(&field.instruction) else {
        flatten_inlines(resolver, style, &field.result, out);
        return;
    };
    out.push(Piece::LinkStart(link));
    flatten_inlines(resolver, style, &field.result, out);
    out.push(Piece::LinkEnd);
}

fn flatten_content<'a>(info: RunInfo, content: &'a RunContent, out: &mut Vec<Piece<'a>>) {
    let piece = match content {
        RunContent::Text(text) if text.is_empty() => return,
        RunContent::Text(text) => match info.symbol {
            Some(font) => Piece::Text(Cow::Owned(symbol::map_str(font, text)), info.fmt),
            None => Piece::Text(Cow::Borrowed(text.as_str()), info.fmt),
        },
        RunContent::Tab => Piece::Tab,
        RunContent::Break(docboss_model::Break::Line) | RunContent::CarriageReturn => {
            Piece::LineBreak
        }
        RunContent::Break(docboss_model::Break::Page) => Piece::PageBreak,
        RunContent::Break(docboss_model::Break::Column) => Piece::ColumnBreak,
        RunContent::NoBreakHyphen => Piece::Text(Cow::Borrowed("-"), info.fmt),
        RunContent::Symbol { font, char } => {
            let c = char::from_u32(*char).unwrap_or('\u{fffd}');
            let mapped = font
                .as_deref()
                .and_then(SymbolFont::from_name)
                .or(info.symbol)
                .map_or(c, |font| symbol::map(font, c).unwrap_or('•'));
            Piece::Text(Cow::Owned(mapped.to_string()), info.fmt)
        }
        RunContent::FootnoteReference(id) => Piece::Note(NoteKind::Footnote, *id),
        RunContent::EndnoteReference(id) => Piece::Note(NoteKind::Endnote, *id),
        RunContent::CommentReference(id) => Piece::Comment(*id),
        RunContent::Drawing(drawing) => Piece::Image(drawing),
        RunContent::SoftHyphen | RunContent::NoteNumber => return,
    };
    out.push(piece);
}

/// A list paragraph's label and its position among its siblings.
#[derive(Debug, Clone)]
pub(crate) struct ListItem {
    pub level: u8,
    pub label: String,
    pub ordinal: u32,
    pub bullet: bool,
    pub format: NumberFormat,
}

/// Produces list labels (ECMA-376 Part 1 §17.9) and plain ordinals for
/// formats that can only number with digits.
pub(crate) struct Lists<'a> {
    doc: &'a Document,
    counter: NumberingCounter<'a>,
    ordinals: HashMap<i64, [u32; 9]>,
}

impl<'a> Lists<'a> {
    pub(crate) fn new(doc: &'a Document) -> Self {
        Self {
            doc,
            counter: doc.numbering.counter(),
            ordinals: HashMap::new(),
        }
    }

    pub(crate) fn next(&mut self, reference: NumberingRef) -> Option<ListItem> {
        let level = self.doc.numbering.level(reference)?;
        if level.format == NumberFormat::None {
            return None;
        }
        let instance = self.doc.numbering.instance(reference.num_id)?;
        let raw = self.counter.next(reference)?;
        let index = usize::from(reference.level.min(8));
        let counts = self.ordinals.entry(instance.abstract_id).or_insert([0; 9]);
        counts[index] = if counts[index] == 0 {
            level.start.max(1)
        } else {
            counts[index] + 1
        };
        counts[index + 1..].fill(0);
        let bullet = level.format == NumberFormat::Bullet;
        let font = level.run.fonts.ascii.as_deref();
        let label = if bullet {
            symbol::label(font, &raw)
        } else {
            raw
        };
        Some(ListItem {
            level: reference.level.min(8),
            label,
            ordinal: counts[index],
            bullet,
            format: level.format.clone(),
        })
    }
}

/// Note and comment numbers, assigned in order of first reference.
#[derive(Default)]
pub(crate) struct Notes {
    pub order: Vec<(NoteKind, i64)>,
    numbers: HashMap<(bool, i64), u32>,
    pub comments: Vec<i64>,
    comment_numbers: HashMap<i64, u32>,
}

impl Notes {
    pub(crate) fn number(&mut self, kind: NoteKind, id: i64) -> u32 {
        if let Some(&n) = self.numbers.get(&(kind == NoteKind::Endnote, id)) {
            return n;
        }
        let n = self.order.iter().filter(|(k, _)| *k == kind).count() as u32 + 1;
        self.order.push((kind, id));
        self.numbers.insert((kind == NoteKind::Endnote, id), n);
        n
    }

    pub(crate) fn comment_number(&mut self, id: i64) -> u32 {
        if let Some(&n) = self.comment_numbers.get(&id) {
            return n;
        }
        self.comments.push(id);
        let n = self.comments.len() as u32;
        self.comment_numbers.insert(id, n);
        n
    }
}

/// A note's visible label: footnotes count 1, 2, 3; endnotes i, ii, iii,
/// the WordprocessingML defaults (ECMA-376 Part 1 §17.11.17).
pub(crate) fn note_label(kind: NoteKind, n: u32) -> String {
    match kind {
        NoteKind::Footnote => n.to_string(),
        NoteKind::Endnote => docboss_model::number_label(&NumberFormat::LowerRoman, n),
    }
}

/// The plain text of pieces, tabs kept, used for table cells and views.
pub(crate) fn pieces_text(pieces: &[Piece<'_>], out: &mut String) {
    for piece in pieces {
        match piece {
            Piece::Text(text, _) => out.push_str(text),
            Piece::Tab => out.push('\t'),
            Piece::LineBreak => out.push('\n'),
            _ => {}
        }
    }
}

/// Whether a paragraph's visible text is all in monospace fonts, which
/// the Markdown and HTML writers turn into a code block.
pub(crate) fn all_monospace(pieces: &[Piece<'_>]) -> bool {
    let mut any = false;
    for piece in pieces {
        let Piece::Text(text, fmt) = piece else {
            if matches!(
                piece,
                Piece::Image(_) | Piece::Note(..) | Piece::LinkStart(_)
            ) {
                return false;
            }
            continue;
        };
        if text.trim().is_empty() {
            continue;
        }
        if !fmt.mono {
            return false;
        }
        any = true;
    }
    any
}

/// The file name part of a media part name, for image references.
pub fn media_file_name(name: &str) -> &str {
    name.rsplit('/').next().unwrap_or(name)
}

/// Standard base64 with padding, for `data:` image URIs.
pub(crate) fn base64(bytes: &[u8], out: &mut String) {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    out.reserve(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        let digits = [n >> 18, n >> 12, n >> 6, n].map(|v| char::from(ALPHABET[(v & 63) as usize]));
        let keep = chunk.len() + 1;
        digits.iter().take(keep).for_each(|&c| out.push(c));
        (keep..4).for_each(|_| out.push('='));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hyperlink_fields_parse() {
        let link = hyperlink_field(" HYPERLINK \"https://example.com/a b\" \\o \"tip\"").unwrap();
        assert_eq!(link.href(), "https://example.com/a b");
        let link = hyperlink_field("HYPERLINK \\l \"intro\"").unwrap();
        assert_eq!(link.href(), "#intro");
        assert!(hyperlink_field("PAGE \\* MERGEFORMAT").is_none());
    }

    #[test]
    fn base64_pads() {
        let mut out = String::new();
        base64(b"Ma", &mut out);
        assert_eq!(out, "TWE=");
        out.clear();
        base64(b"Man", &mut out);
        assert_eq!(out, "TWFu");
        out.clear();
        base64(b"M", &mut out);
        assert_eq!(out, "TQ==");
    }
}

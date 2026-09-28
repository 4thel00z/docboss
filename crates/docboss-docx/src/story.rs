//! Stories: the main text, headers, footers, notes, comments and text box
//! content (ECMA-376 Part 1 §17.2, §17.3, §17.4, §17.16).

use std::collections::HashMap;

use docboss_model::{
    Block, Break, Diagnostic, Drawing, DrawingPlacement, Field, Hyperlink, Inline, MediaId,
    Paragraph, Revision, RevisionKind, Run, RunContent, RunProperties, Section, SectionProperties,
    Table, TableCell, TableRow,
};
use docboss_xml::{Element, Ns, Reader};

use crate::package::Relationships;
use crate::props::{
    cell_properties, paragraph_properties, row_properties, run_properties, section_properties,
    table_properties, Theme,
};
use crate::xml::{attr, children, int, int_attr, twips_attr};

/// What a story parser needs to know about its part.
pub struct Context<'p> {
    pub part: &'p str,
    pub rels: &'p Relationships,
    pub media: &'p HashMap<String, MediaId>,
    pub theme: &'p Theme,
}

/// A paragraph's content before complex fields are folded.
enum Piece {
    Inline(Inline),
    Begin,
    Separate,
    End,
    Instruction(String),
}

/// A complex field being read (ECMA-376 Part 1 §17.16.18).
struct Frame {
    instruction: String,
    result: Vec<Inline>,
    separated: bool,
    /// The field began in an earlier paragraph; its result is emitted as
    /// plain content and its end only closes it.
    carried: bool,
}

impl Frame {
    fn new() -> Self {
        Self {
            instruction: String::new(),
            result: Vec::new(),
            separated: false,
            carried: false,
        }
    }

    fn carried_copy(&self) -> Self {
        Self {
            instruction: String::new(),
            result: Vec::new(),
            separated: self.separated,
            carried: true,
        }
    }
}

/// Splits a field instruction into its arguments, honoring quotes.
fn field_arguments(instruction: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for c in instruction.chars() {
        match c {
            '"' => {
                if quoted {
                    args.push(std::mem::take(&mut current));
                }
                quoted = !quoted;
            }
            c if c.is_whitespace() && !quoted => {
                if !current.is_empty() {
                    args.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }
    if !current.is_empty() {
        args.push(current);
    }
    args
}

/// A field as an inline: `HYPERLINK` fields (ECMA-376 Part 1 §17.16.5.25)
/// become hyperlinks, everything else a [`Field`] with its cached result.
fn field_inline(instruction: &str, result: Vec<Inline>) -> Inline {
    let instruction = instruction.trim();
    let args = field_arguments(instruction);
    let is_link = args
        .first()
        .is_some_and(|keyword| keyword.eq_ignore_ascii_case("HYPERLINK"));
    if !is_link {
        return Inline::Field(Field {
            instruction: instruction.to_string(),
            result,
        });
    }
    let mut link = Hyperlink {
        inlines: result,
        ..Hyperlink::default()
    };
    let mut rest = args[1..].iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "\\l" => link.anchor = rest.next().cloned(),
            "\\o" => link.tooltip = rest.next().cloned(),
            "\\t" | "\\m" | "\\n" => {}
            _ if link.target.is_none() && !arg.starts_with('\\') => link.target = Some(arg.clone()),
            _ => {}
        }
    }
    Inline::Hyperlink(link)
}

fn inline_text(inline: &Inline, out: &mut String) {
    let paragraph = Paragraph {
        inlines: vec![inline.clone()],
        ..Paragraph::default()
    };
    out.push_str(&paragraph.text());
}

fn push_inline(frames: &mut [Frame], out: &mut Vec<Inline>, inline: Inline) {
    match frames.last_mut() {
        Some(top) if !top.carried && !top.separated => inline_text(&inline, &mut top.instruction),
        Some(top) if !top.carried => top.result.push(inline),
        Some(top) if !top.separated => {}
        _ => out.push(inline),
    }
}

/// Folds field markers into fields. `frames` holds fields still open from
/// earlier paragraphs; with `close` set, fields left open at the end are
/// collapsed into what they hold so far and stay open as carried frames.
fn fold(pieces: Vec<Piece>, frames: &mut Vec<Frame>, close: bool) -> Vec<Inline> {
    let mut out = Vec::new();
    for piece in pieces {
        match piece {
            Piece::Inline(inline) => push_inline(frames, &mut out, inline),
            Piece::Begin => frames.push(Frame::new()),
            Piece::Instruction(text) => {
                if let Some(top) = frames.last_mut().filter(|top| !top.separated) {
                    top.instruction.push_str(&text);
                }
            }
            Piece::Separate => {
                if let Some(top) = frames.last_mut() {
                    top.separated = true;
                }
            }
            Piece::End => {
                let Some(frame) = frames.pop() else {
                    continue;
                };
                if frame.carried {
                    continue;
                }
                let inline = field_inline(&frame.instruction, frame.result);
                push_inline(frames, &mut out, inline);
            }
        }
    }
    if !close {
        return out;
    }
    let open = frames
        .iter()
        .rev()
        .take_while(|frame| !frame.carried)
        .count();
    let mut placeholders = Vec::with_capacity(open);
    for _ in 0..open {
        let Some(frame) = frames.pop() else {
            break;
        };
        placeholders.push(frame.carried_copy());
        let inline = field_inline(&frame.instruction, frame.result);
        push_inline(frames, &mut out, inline);
    }
    frames.extend(placeholders.into_iter().rev());
    out
}

/// Where parsed blocks go: a flat list, or sections when the story is the
/// main text.
#[derive(Default)]
struct Sink {
    blocks: Vec<Block>,
    sections: Vec<Section>,
    tracks_sections: bool,
}

impl Sink {
    fn end_section(&mut self, properties: SectionProperties) {
        let blocks = std::mem::take(&mut self.blocks);
        self.sections.push(Section { properties, blocks });
    }
}

pub struct StoryParser<'p> {
    ctx: Context<'p>,
    pub diagnostics: Vec<Diagnostic>,
    frames: Vec<Frame>,
}

const EMU_PER_POINT: f64 = 12_700.0;

/// A CSS length from a VML `style` attribute, in EMU.
fn css_length(value: &str) -> Option<i64> {
    let value = value.trim();
    let units: [(&str, f64); 6] = [
        ("pt", 1.0),
        ("in", 72.0),
        ("cm", 28.346_457),
        ("mm", 2.834_645_7),
        ("px", 0.75),
        ("pc", 12.0),
    ];
    let (number, factor) = units
        .iter()
        .find_map(|(suffix, factor)| value.strip_suffix(suffix).map(|n| (n, *factor)))
        .unwrap_or((value, 1.0 / EMU_PER_POINT));
    let n: f64 = number.trim().parse().ok()?;
    n.is_finite()
        .then(|| (n * factor * EMU_PER_POINT).round() as i64)
}

fn css_property<'s>(style: &'s str, name: &str) -> Option<&'s str> {
    style.split(';').find_map(|declaration| {
        let (key, value) = declaration.split_once(':')?;
        (key.trim().eq_ignore_ascii_case(name)).then(|| value.trim())
    })
}

struct DrawingInfo {
    drawing: Drawing,
    anchored: bool,
    x: i64,
    y: i64,
    behind: bool,
    page: bool,
}

impl DrawingInfo {
    fn new() -> Self {
        Self {
            drawing: Drawing {
                media: None,
                width: 0,
                height: 0,
                placement: DrawingPlacement::Inline,
                name: None,
                description: None,
                text_box: Vec::new(),
            },
            anchored: false,
            x: 0,
            y: 0,
            behind: false,
            page: false,
        }
    }

    fn finish(mut self) -> Drawing {
        if self.anchored {
            self.drawing.placement = DrawingPlacement::Anchored {
                x: self.x,
                y: self.y,
                behind_text: self.behind,
                relative_to_page: self.page,
            };
        }
        self.drawing
    }
}

/// Picks the branch of `mc:AlternateContent` (ECMA-376 Part 3 §9.3, §7.5) to
/// read: the first `mc:Choice` whose required namespaces this reader
/// understands, else `mc:Fallback`. Calls `f` with the chosen branch's start.
fn alternate_content<'a>(reader: &mut Reader<'a>, mut f: impl FnMut(&mut Reader<'a>)) {
    let mut chosen = false;
    children(reader, |reader, e| {
        if chosen || e.ns != Ns::MC {
            return;
        }
        let take = match e.local {
            "Choice" => e.attr(Ns::NONE, "Requires").is_some_and(|requires| {
                requires
                    .split_whitespace()
                    .all(|prefix| e.resolve_prefix(prefix).is_some_and(understood))
            }),
            "Fallback" => true,
            _ => false,
        };
        if take {
            chosen = true;
            f(reader);
        }
    });
}

fn understood(ns: Ns) -> bool {
    [
        Ns::W,
        Ns::WP,
        Ns::A,
        Ns::PIC,
        Ns::WPS,
        Ns::WPG,
        Ns::W14,
        Ns::WP14,
        Ns::V,
        Ns::O,
        Ns::W10,
        Ns::M,
        Ns::R,
    ]
    .contains(&ns)
}

impl<'p> StoryParser<'p> {
    pub fn new(ctx: Context<'p>) -> Self {
        Self {
            ctx,
            diagnostics: Vec::new(),
            frames: Vec::new(),
        }
    }

    /// Reads `w:body` into sections (ECMA-376 Part 1 §17.2.2, §17.6.17).
    /// The reader is positioned just after the body's start.
    pub fn body(&mut self, reader: &mut Reader<'_>) -> Vec<Section> {
        let mut sink = Sink {
            tracks_sections: true,
            ..Sink::default()
        };
        let mut last: Option<SectionProperties> = None;
        children(reader, |reader, e| {
            if e.is(Ns::W, "sectPr") {
                last = Some(section_properties(reader));
                return;
            }
            self.block(reader, &e, &mut sink);
        });
        let trailing = !sink.blocks.is_empty() || last.is_some() || sink.sections.is_empty();
        if trailing {
            sink.end_section(last.unwrap_or_default());
        }
        sink.sections
    }

    /// Reads the block-level children of the element just started.
    pub fn blocks(&mut self, reader: &mut Reader<'_>) -> Vec<Block> {
        let mut sink = Sink::default();
        children(reader, |reader, e| self.block(reader, &e, &mut sink));
        sink.blocks
    }

    fn block<'a>(&mut self, reader: &mut Reader<'a>, e: &Element<'a>, sink: &mut Sink) {
        match (e.ns, e.local) {
            (Ns::W, "p") => {
                let (paragraph, section) = self.paragraph(reader);
                sink.blocks.push(Block::Paragraph(paragraph));
                if let Some(section) = section.filter(|_| sink.tracks_sections) {
                    sink.end_section(section);
                }
            }
            (Ns::W, "tbl") => sink.blocks.push(Block::Table(self.table(reader))),
            (Ns::W, "sdt") => children(reader, |reader, e| {
                if e.is(Ns::W, "sdtContent") {
                    children(reader, |reader, e| self.block(reader, &e, sink));
                }
            }),
            (Ns::W, "customXml" | "ins" | "moveTo" | "smartTag") => {
                children(reader, |reader, e| self.block(reader, &e, sink))
            }
            (Ns::MC, "AlternateContent") => alternate_content(reader, |reader| {
                children(reader, |reader, e| self.block(reader, &e, sink))
            }),
            (Ns::W, "altChunk") => self.diagnostics.push(Diagnostic::dropped(
                self.ctx.part,
                "an alternative format chunk (w:altChunk, ECMA-376 Part 1 §17.17.2.1) was not imported",
            )),
            _ => {}
        }
    }

    /// `w:p` (ECMA-376 Part 1 §17.3.1.22).
    fn paragraph(&mut self, reader: &mut Reader<'_>) -> (Paragraph, Option<SectionProperties>) {
        let mut paragraph = Paragraph::default();
        let mut section = None;
        let mut pieces = Vec::new();
        children(reader, |reader, e| {
            if e.is(Ns::W, "pPr") {
                let format = paragraph_properties(reader, self.ctx.theme);
                paragraph.style_id = format.style_id;
                paragraph.properties = format.properties;
                paragraph.mark = format.mark;
                section = format.section;
                return;
            }
            self.inline(reader, &e, &mut pieces);
        });
        let mut frames = std::mem::take(&mut self.frames);
        paragraph.inlines = fold(pieces, &mut frames, true);
        self.frames = frames;
        (paragraph, section)
    }

    fn contained(&mut self, reader: &mut Reader<'_>) -> Vec<Inline> {
        let mut pieces = Vec::new();
        children(reader, |reader, e| self.inline(reader, &e, &mut pieces));
        let mut frames = Vec::new();
        fold(pieces, &mut frames, true)
    }

    fn revision(&mut self, reader: &mut Reader<'_>, e: &Element<'_>, kind: RevisionKind) -> Inline {
        let author = attr(e, "author").map(Into::into);
        let date = attr(e, "date").map(Into::into);
        Inline::Revision(Revision {
            kind,
            author,
            date,
            inlines: self.contained(reader),
        })
    }

    /// Paragraph content: runs, hyperlinks, simple fields, tracked changes,
    /// bookmarks, comment ranges, and the content of structured document
    /// tags, smart tags and custom XML elements with their properties dropped.
    /// ECMA-376 Part 1 §17.16.22, §17.16.19, §17.13.5, §17.13.6, §17.13.4, §17.5.2, §17.5.1.
    fn inline<'a>(&mut self, reader: &mut Reader<'a>, e: &Element<'a>, pieces: &mut Vec<Piece>) {
        if e.ns == Ns::MC && e.local == "AlternateContent" {
            alternate_content(reader, |reader| {
                children(reader, |reader, e| self.inline(reader, &e, pieces))
            });
            return;
        }
        if e.ns == Ns::M {
            if matches!(e.local, "oMath" | "oMathPara") {
                let text = math_text(reader);
                pieces.push(Piece::Inline(Inline::Run(Run {
                    properties: RunProperties::default(),
                    content: vec![RunContent::Text(text)],
                })));
            }
            return;
        }
        if e.ns != Ns::W {
            return;
        }
        match e.local {
            "r" => self.run(reader, pieces),
            "hyperlink" => {
                let target = e
                    .attr(Ns::R, "id")
                    .and_then(|id| self.ctx.rels.get(&id).map(|rel| rel.target.clone()));
                let anchor = attr(e, "anchor").map(Into::into);
                let tooltip = attr(e, "tooltip").map(Into::into);
                let inlines = self.contained(reader);
                pieces.push(Piece::Inline(Inline::Hyperlink(Hyperlink {
                    target,
                    anchor,
                    tooltip,
                    inlines,
                })));
            }
            "fldSimple" => {
                let instruction = attr(e, "instr").unwrap_or_default().into_owned();
                let result = self.contained(reader);
                pieces.push(Piece::Inline(field_inline(&instruction, result)));
            }
            "ins" | "moveTo" => {
                let revision = self.revision(reader, e, RevisionKind::Insertion);
                pieces.push(Piece::Inline(revision));
            }
            "del" | "moveFrom" => {
                let revision = self.revision(reader, e, RevisionKind::Deletion);
                pieces.push(Piece::Inline(revision));
            }
            "smartTag" | "customXml" | "dir" | "bdo" => {
                children(reader, |reader, e| self.inline(reader, &e, pieces))
            }
            "sdt" => children(reader, |reader, e| {
                if e.is(Ns::W, "sdtContent") {
                    children(reader, |reader, e| self.inline(reader, &e, pieces));
                }
            }),
            "bookmarkStart" => {
                let id = int_attr(e, "id").unwrap_or(-1);
                let name = attr(e, "name").unwrap_or_default().into_owned();
                pieces.push(Piece::Inline(Inline::BookmarkStart { id, name }));
            }
            "bookmarkEnd" => pieces.push(Piece::Inline(Inline::BookmarkEnd {
                id: int_attr(e, "id").unwrap_or(-1),
            })),
            "commentRangeStart" => pieces.push(Piece::Inline(Inline::CommentRangeStart(
                int_attr(e, "id").unwrap_or(-1),
            ))),
            "commentRangeEnd" => pieces.push(Piece::Inline(Inline::CommentRangeEnd(
                int_attr(e, "id").unwrap_or(-1),
            ))),
            _ => {}
        }
    }

    /// `w:r` (ECMA-376 Part 1 §17.3.2.25).
    fn run(&mut self, reader: &mut Reader<'_>, pieces: &mut Vec<Piece>) {
        let mut properties = RunProperties::default();
        let mut content = Vec::new();
        children(reader, |reader, e| {
            if e.is(Ns::W, "rPr") {
                properties = run_properties(reader, self.ctx.theme);
                return;
            }
            self.run_content(reader, &e, &properties, &mut content, pieces);
        });
        if content.is_empty() {
            return;
        }
        pieces.push(Piece::Inline(Inline::Run(Run {
            properties,
            content,
        })));
    }

    /// Run content (ECMA-376 Part 1 §17.3.3): text, field characters and
    /// codes, tabs, breaks, symbols, hyphens, note and comment references and
    /// marks, DrawingML and VML pictures.
    /// ECMA-376 Part 1 §17.16.18, §17.16.23, §17.11.14, §17.11.7, §17.11.13, §17.11.6, §17.13.4.
    fn run_content<'a>(
        &mut self,
        reader: &mut Reader<'a>,
        e: &Element<'a>,
        properties: &RunProperties,
        content: &mut Vec<RunContent>,
        pieces: &mut Vec<Piece>,
    ) {
        if e.ns == Ns::MC && e.local == "AlternateContent" {
            alternate_content(reader, |reader| {
                children(reader, |reader, e| {
                    self.run_content(reader, &e, properties, content, pieces)
                })
            });
            return;
        }
        if e.ns != Ns::W {
            return;
        }
        match e.local {
            "t" | "delText" => {
                let text = reader.read_text();
                if let Some(RunContent::Text(previous)) = content.last_mut() {
                    previous.push_str(&text);
                    return;
                }
                content.push(RunContent::Text(text.into_owned()));
            }
            "instrText" | "delInstrText" => {
                let text = reader.read_text().into_owned();
                flush(properties, content, pieces);
                pieces.push(Piece::Instruction(text));
            }
            "fldChar" => {
                flush(properties, content, pieces);
                match attr(e, "fldCharType").as_deref() {
                    Some("begin") => pieces.push(Piece::Begin),
                    Some("separate") => pieces.push(Piece::Separate),
                    Some("end") => pieces.push(Piece::End),
                    _ => {}
                }
            }
            "tab" | "ptab" => content.push(RunContent::Tab),
            "br" => content.push(RunContent::Break(match attr(e, "type").as_deref() {
                Some("page") => Break::Page,
                Some("column") => Break::Column,
                _ => Break::Line,
            })),
            "cr" => content.push(RunContent::CarriageReturn),
            "noBreakHyphen" => content.push(RunContent::NoBreakHyphen),
            "softHyphen" => content.push(RunContent::SoftHyphen),
            "sym" => {
                let font = attr(e, "font").map(Into::into);
                let code = e
                    .attr_raw(Ns::W, "char")
                    .and_then(|hex| u32::from_str_radix(hex.trim(), 16).ok());
                if let Some(char) = code {
                    content.push(RunContent::Symbol { font, char });
                }
            }
            "footnoteReference" => content.push(RunContent::FootnoteReference(
                int_attr(e, "id").unwrap_or(-1),
            )),
            "endnoteReference" => content.push(RunContent::EndnoteReference(
                int_attr(e, "id").unwrap_or(-1),
            )),
            "commentReference" => content.push(RunContent::CommentReference(
                int_attr(e, "id").unwrap_or(-1),
            )),
            "footnoteRef" | "endnoteRef" => content.push(RunContent::NoteNumber),
            "drawing" => {
                let mut info = DrawingInfo::new();
                self.drawing_children(reader, &mut info);
                content.push(RunContent::Drawing(info.finish()));
            }
            "pict" | "object" => {
                let mut info = DrawingInfo::new();
                self.vml(reader, &mut info);
                if info.drawing.media.is_some() || !info.drawing.text_box.is_empty() {
                    content.push(RunContent::Drawing(info.finish()));
                }
            }
            _ => {}
        }
    }

    fn media_for(&mut self, id: &str) -> Option<MediaId> {
        let Some(rel) = self.ctx.rels.get(id) else {
            self.diagnostics.push(Diagnostic::dropped(
                self.ctx.part,
                format!("image relationship {id} is not defined"),
            ));
            return None;
        };
        if rel.external {
            self.diagnostics.push(Diagnostic::dropped(
                self.ctx.part,
                format!("linked image {} is outside the package", rel.target),
            ));
            return None;
        }
        let found = self.ctx.media.get(&rel.target).copied();
        if found.is_none() {
            self.diagnostics.push(Diagnostic::dropped(
                self.ctx.part,
                format!("image {} is missing from the package", rel.target),
            ));
        }
        found
    }

    fn text_box(&mut self, reader: &mut Reader<'_>, info: &mut DrawingInfo) {
        let frames = std::mem::take(&mut self.frames);
        let blocks = self.blocks(reader);
        self.frames = frames;
        info.drawing.text_box.extend(blocks);
    }

    /// The DrawingML picture inside `w:drawing` (ECMA-376 Part 1 §17.3.3.9,
    /// §20.4): inline and anchored objects, their extent, non-visual
    /// properties, position offsets and text box content.
    /// ECMA-376 Part 1 §20.4.2.8, §20.4.2.3, §20.4.2.7, §20.4.2.5, §20.4.2.10, §20.4.2.11, §20.4.2.12, §20.4.2.38.
    fn drawing_children(&mut self, reader: &mut Reader<'_>, info: &mut DrawingInfo) {
        children(reader, |reader, e| {
            match (e.ns, e.local) {
                (Ns::WP, "anchor") => {
                    info.anchored = true;
                    info.behind = e
                        .attr_raw(Ns::NONE, "behindDoc")
                        .is_some_and(|v| v == "1" || v == "true");
                }
                (Ns::WP, "extent") => {
                    info.drawing.width =
                        e.attr_raw(Ns::NONE, "cx").and_then(int).unwrap_or(0).max(0);
                    info.drawing.height =
                        e.attr_raw(Ns::NONE, "cy").and_then(int).unwrap_or(0).max(0);
                    return;
                }
                (Ns::WP, "docPr") => {
                    info.drawing.name = e
                        .attr(Ns::NONE, "name")
                        .map(Into::into)
                        .filter(|n: &String| !n.is_empty());
                    info.drawing.description = e
                        .attr(Ns::NONE, "descr")
                        .map(Into::into)
                        .filter(|d: &String| !d.is_empty());
                    return;
                }
                (Ns::WP, "positionH" | "positionV") => {
                    let horizontal = e.local == "positionH";
                    if horizontal {
                        info.page = e.attr_raw(Ns::NONE, "relativeFrom") == Some("page");
                    }
                    children(reader, |reader, offset| {
                        if offset.local != "posOffset" {
                            return;
                        }
                        let value = int(&reader.read_text()).unwrap_or(0);
                        if horizontal {
                            info.x = value;
                        } else {
                            info.y = value;
                        }
                    });
                    return;
                }
                (Ns::A, "blip") => {
                    if info.drawing.media.is_none() {
                        if let Some(id) = e.attr(Ns::R, "embed") {
                            info.drawing.media = self.media_for(&id);
                        }
                    }
                }
                (Ns::W, "txbxContent") => {
                    self.text_box(reader, info);
                    return;
                }
                (Ns::MC, "AlternateContent") => {
                    alternate_content(reader, |reader| self.drawing_children(reader, info));
                    return;
                }
                _ => {}
            }
            self.drawing_children(reader, info);
        });
    }

    /// A VML picture or text box inside `w:pict` or `w:object` (ECMA-376
    /// Part 1 §17.3.3.19).
    fn vml(&mut self, reader: &mut Reader<'_>, info: &mut DrawingInfo) {
        children(reader, |reader, e| {
            match (e.ns, e.local) {
                (Ns::V, "shape" | "rect" | "roundrect" | "oval") => {
                    if let Some(style) = e.attr(Ns::NONE, "style") {
                        if info.drawing.width == 0 {
                            info.drawing.width = css_property(&style, "width")
                                .and_then(css_length)
                                .unwrap_or(0)
                                .max(0);
                            info.drawing.height = css_property(&style, "height")
                                .and_then(css_length)
                                .unwrap_or(0)
                                .max(0);
                        }
                        if css_property(&style, "position").is_some_and(|p| p == "absolute") {
                            info.anchored = true;
                            info.x = css_property(&style, "margin-left")
                                .and_then(css_length)
                                .unwrap_or(0);
                            info.y = css_property(&style, "margin-top")
                                .and_then(css_length)
                                .unwrap_or(0);
                            info.behind =
                                css_property(&style, "z-index").is_some_and(|z| z.starts_with('-'));
                        }
                    }
                    if info.drawing.description.is_none() {
                        info.drawing.description = e
                            .attr(Ns::NONE, "alt")
                            .map(Into::into)
                            .filter(|d: &String| !d.is_empty());
                    }
                }
                (Ns::V, "imagedata") => {
                    let id = e.attr(Ns::R, "id").or_else(|| e.attr(Ns::O, "relid"));
                    if let Some(id) = id.filter(|_| info.drawing.media.is_none()) {
                        info.drawing.media = self.media_for(&id);
                    }
                    return;
                }
                (Ns::W, "txbxContent") => {
                    self.text_box(reader, info);
                    return;
                }
                _ => {}
            }
            self.vml(reader, info);
        });
    }

    /// `w:tbl` (ECMA-376 Part 1 §17.4.37) with its grid of column widths
    /// (ECMA-376 Part 1 §17.4.48, §17.4.16).
    fn table(&mut self, reader: &mut Reader<'_>) -> Table {
        let mut table = Table::default();
        children(reader, |reader, e| {
            if e.ns != Ns::W {
                return;
            }
            match e.local {
                "tblPr" => table.properties = table_properties(reader),
                "tblGrid" => children(reader, |_, col| {
                    if col.local == "gridCol" {
                        table.grid.push(twips_attr(&col, "w").unwrap_or(0).max(0));
                    }
                }),
                _ => self.rows(reader, &e, &mut table.rows),
            }
        });
        table
    }

    fn rows<'a>(&mut self, reader: &mut Reader<'a>, e: &Element<'a>, rows: &mut Vec<TableRow>) {
        match e.local {
            "tr" => rows.push(self.row(reader)),
            "sdt" | "sdtContent" | "customXml" | "ins" | "moveTo" => {
                children(reader, |reader, e| self.rows(reader, &e, rows))
            }
            _ => {}
        }
    }

    /// `w:tr` (ECMA-376 Part 1 §17.4.78).
    fn row(&mut self, reader: &mut Reader<'_>) -> TableRow {
        let mut row = TableRow::default();
        children(reader, |reader, e| match e.local {
            "trPr" => row.properties = row_properties(reader),
            _ => self.cells(reader, &e, &mut row.cells),
        });
        row
    }

    /// `w:tc` (ECMA-376 Part 1 §17.4.65) cells of a row.
    fn cells<'a>(&mut self, reader: &mut Reader<'a>, e: &Element<'a>, cells: &mut Vec<TableCell>) {
        match e.local {
            "tc" => {
                let mut cell = TableCell::default();
                cell.properties.grid_span = 1;
                let mut sink = Sink::default();
                children(reader, |reader, e| {
                    if e.is(Ns::W, "tcPr") {
                        cell.properties = cell_properties(reader);
                        return;
                    }
                    self.block(reader, &e, &mut sink);
                });
                cell.blocks = sink.blocks;
                cells.push(cell);
            }
            "sdt" | "sdtContent" | "customXml" => {
                children(reader, |reader, e| self.cells(reader, &e, cells))
            }
            _ => {}
        }
    }
}

fn flush(properties: &RunProperties, content: &mut Vec<RunContent>, pieces: &mut Vec<Piece>) {
    if content.is_empty() {
        return;
    }
    let run = Run {
        properties: properties.clone(),
        content: std::mem::take(content),
    };
    pieces.push(Piece::Inline(Inline::Run(run)));
}

/// The text of an Office Math zone: its `m:t` runs in order.
fn math_text(reader: &mut Reader<'_>) -> String {
    let mut out = String::new();
    fn walk(reader: &mut Reader<'_>, out: &mut String) {
        children(reader, |reader, e| {
            if e.ns == Ns::M && e.local == "t" {
                out.push_str(&reader.read_text());
                return;
            }
            walk(reader, out);
        });
    }
    walk(reader, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ECMA-376 Part 1 §17.16.5.25: HYPERLINK field switches.
    #[test]
    fn hyperlink_field_arguments() {
        let Inline::Hyperlink(link) =
            field_inline(r#" HYPERLINK "https://x.test/a b" \o "tip" "#, Vec::new())
        else {
            panic!()
        };
        assert_eq!(link.target.as_deref(), Some("https://x.test/a b"));
        assert_eq!(link.tooltip.as_deref(), Some("tip"));
        let Inline::Hyperlink(link) = field_inline(r#"HYPERLINK \l "_Toc1""#, Vec::new()) else {
            panic!()
        };
        assert_eq!(link.anchor.as_deref(), Some("_Toc1"));
        assert!(link.target.is_none());
        assert!(matches!(field_inline("PAGE", Vec::new()), Inline::Field(_)));
    }

    #[test]
    fn css_lengths() {
        assert_eq!(css_length("72pt"), Some(914_400));
        assert_eq!(css_length("1in"), Some(914_400));
        assert_eq!(
            css_property("width:10pt; height:5pt", "height"),
            Some("5pt")
        );
    }
}

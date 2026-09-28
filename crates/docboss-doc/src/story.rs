//! Turns a CP range of the document text into model blocks: paragraph
//! boundaries ([MS-DOC] §2.4.2), direct formatting ([MS-DOC] §2.4.6),
//! fields, special characters and tables ([MS-DOC] §2.4.3 to §2.4.5).

use std::cell::RefCell;
use std::collections::HashMap;

use docboss_model::{
    Block, Break, Diagnostic, Drawing, DrawingPlacement, Field, Hyperlink, Inline, Media, MediaId,
    Paragraph, ParagraphProperties, Revision, RevisionKind, Run, RunContent, RunProperties, Styles,
    Table, TableCell, TableRow,
};

use crate::fkp::{find, FormatRun};
use crate::picture::{inline_picture, Image};
use crate::props::{apply_chp, apply_pap, dttm, numbering_ref, CharContext, CharExtra, ParaExtra};
use crate::sprm::prls;
use crate::styles::Stylesheet;
use crate::table::RowInfo;
use crate::text::PieceTable;

/// What a special character at a CP of the main text refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reference {
    Footnote(i64),
    Endnote(i64),
    Comment(i64),
}

/// A floating shape anchored at a CP ([MS-DOC] §2.9.253 Spa).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Anchor {
    pub shape_id: u32,
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
    pub relative_to_page: bool,
    pub behind_text: bool,
}

/// A position-keyed inline marker: bookmark or comment range ends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Marker {
    BookmarkStart(i64, String),
    BookmarkEnd(i64),
    CommentStart(i64),
    CommentEnd(i64),
}

/// Which kind of story is being built, for the characters whose meaning
/// depends on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoryKind {
    Main,
    Note,
    Comment,
    HeaderFooter,
}

pub struct Context<'a> {
    pub word: &'a [u8],
    pub data: &'a [u8],
    pub text: Vec<u16>,
    pub pieces: PieceTable,
    pub chpx: Vec<FormatRun>,
    pub papx: Vec<FormatRun>,
    pub sheet: Stylesheet,
    pub styles: Styles,
    pub numbering: docboss_model::Numbering,
    pub fonts: Vec<String>,
    pub references: HashMap<u32, Reference>,
    pub anchors: HashMap<u32, Anchor>,
    /// Markers by CP and rank: at one CP, ends of earlier ranges come
    /// first, then starts, then ends of empty ranges.
    pub markers: Vec<(u32, u8, Marker)>,
    pub authors: Vec<String>,
    pub shape_blips: HashMap<u32, usize>,
    pub blip_store: Vec<Option<Image>>,
    pub media: RefCell<Vec<Media>>,
    pub diagnostics: RefCell<Vec<Diagnostic>>,
    base_cache: RefCell<HashMap<(u16, Option<String>), RunProperties>>,
    media_index: RefCell<HashMap<u64, Vec<MediaId>>>,
}

struct ParagraphInfo {
    paragraph: Paragraph,
    extra: ParaExtra,
    mark: u16,
    grpprl: Vec<u8>,
}

/// A field being read: its instruction, and its result once the
/// separator was seen.
struct OpenField {
    instruction: String,
    result: Vec<Inline>,
    in_result: bool,
}

#[derive(Default)]
struct Inlines {
    root: Vec<Inline>,
    fields: Vec<OpenField>,
}

impl Inlines {
    fn target(&mut self) -> Option<&mut Vec<Inline>> {
        match self.fields.last_mut() {
            Some(field) if field.in_result => Some(&mut field.result),
            Some(_) => None,
            None => Some(&mut self.root),
        }
    }

    fn push(&mut self, inline: Inline) {
        if let Some(target) = self.target() {
            target.push(inline);
        }
    }

    /// Adds content to a run with these properties and revision state,
    /// joining the previous run when both match.
    fn content(&mut self, props: &RunProperties, revision: Option<&Revision>, content: RunContent) {
        if let Some(field) = self.fields.last_mut().filter(|f| !f.in_result) {
            if let RunContent::Text(text) = &content {
                field.instruction.push_str(text);
            }
            return;
        }
        let Some(target) = self.target() else {
            return;
        };
        let holder = match (revision, target.last_mut()) {
            (Some(rev), Some(Inline::Revision(last)))
                if last.kind == rev.kind && last.author == rev.author && last.date == rev.date =>
            {
                &mut last.inlines
            }
            (Some(rev), _) => {
                target.push(Inline::Revision(Revision {
                    inlines: Vec::new(),
                    ..rev.clone()
                }));
                let Some(Inline::Revision(last)) = target.last_mut() else {
                    return;
                };
                &mut last.inlines
            }
            (None, _) => target,
        };
        if let Some(Inline::Run(run)) = holder.last_mut() {
            if run.properties == *props {
                if let (RunContent::Text(add), Some(RunContent::Text(existing))) =
                    (&content, run.content.last_mut())
                {
                    existing.push_str(add);
                    return;
                }
                run.content.push(content);
                return;
            }
        }
        holder.push(Inline::Run(Run {
            properties: props.clone(),
            content: vec![content],
        }));
    }

    fn begin_field(&mut self) {
        self.fields.push(OpenField {
            instruction: String::new(),
            result: Vec::new(),
            in_result: false,
        });
    }

    fn separate_field(&mut self) {
        if let Some(field) = self.fields.last_mut() {
            field.in_result = true;
        }
    }

    fn end_field(&mut self) {
        let Some(field) = self.fields.pop() else {
            return;
        };
        let inline = field_inline(field);
        self.push(inline);
    }

    /// Closes fields still open at the end of a paragraph: their results
    /// stay as plain content.
    fn finish(mut self) -> Vec<Inline> {
        while let Some(field) = self.fields.pop() {
            let result = field.result;
            match self.target() {
                Some(target) => target.extend(result),
                None => {
                    if let Some(parent) = self.fields.last_mut() {
                        parent.result.extend(result);
                    }
                }
            }
        }
        self.root
    }
}

/// A field as a hyperlink when its instruction is HYPERLINK
/// ([MS-DOC] §2.9.90 flt 0x58), else as a field with its cached result.
fn field_inline(field: OpenField) -> Inline {
    let instruction = field.instruction.trim().to_string();
    let mut words = split_instruction(&instruction);
    let is_link = words
        .first()
        .is_some_and(|w| w.eq_ignore_ascii_case("HYPERLINK"));
    if !is_link {
        return Inline::Field(Field {
            instruction,
            result: field.result,
        });
    }
    words.remove(0);
    let mut link = Hyperlink {
        inlines: field.result,
        ..Hyperlink::default()
    };
    let mut words = words.into_iter();
    while let Some(word) = words.next() {
        match word.as_str() {
            "\\l" => link.anchor = words.next(),
            "\\o" => link.tooltip = words.next(),
            "\\t" | "\\m" | "\\n" => {
                if word == "\\t" {
                    words.next();
                }
            }
            _ if link.target.is_none() && !word.starts_with('\\') => link.target = Some(word),
            _ => {}
        }
    }
    Inline::Hyperlink(link)
}

fn split_instruction(instruction: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for c in instruction.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                if !quoted {
                    words.push(std::mem::take(&mut current));
                }
            }
            c if c.is_whitespace() && !quoted => {
                if !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

impl<'a> Context<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        word: &'a [u8],
        data: &'a [u8],
        pieces: PieceTable,
        chpx: Vec<FormatRun>,
        papx: Vec<FormatRun>,
        sheet: Stylesheet,
        styles: Styles,
        numbering: docboss_model::Numbering,
        fonts: Vec<String>,
    ) -> Context<'a> {
        let text = pieces.decode(word);
        Context {
            word,
            data,
            text,
            pieces,
            chpx,
            papx,
            sheet,
            styles,
            numbering,
            fonts,
            references: HashMap::new(),
            anchors: HashMap::new(),
            markers: Vec::new(),
            authors: Vec::new(),
            shape_blips: HashMap::new(),
            blip_store: Vec::new(),
            media: RefCell::new(Vec::new()),
            diagnostics: RefCell::new(Vec::new()),
            base_cache: RefCell::new(HashMap::new()),
            media_index: RefCell::new(HashMap::new()),
        }
    }

    fn report(&self, diagnostic: Diagnostic) {
        self.diagnostics.borrow_mut().push(diagnostic);
    }

    fn grpprl(&self, run: &FormatRun) -> &'a [u8] {
        crate::bytes::slice(self.word, run.grpprl.0, run.grpprl.1)
    }

    /// The paragraph containing the mark at `mark_cp`: its style, PAPX
    /// grpprl and the Prm of the piece holding the mark.
    fn paragraph_formatting(&self, mark_cp: u32) -> (u16, Vec<u8>) {
        let Some(piece) = self.pieces.piece_at(mark_cp) else {
            return (0, Vec::new());
        };
        let fc = piece.fc_of(mark_cp);
        let (istd, mut grpprl) = match find(&self.papx, fc) {
            Some(run) => (run.istd, self.grpprl(run).to_vec()),
            None => (0, Vec::new()),
        };
        let huge = prls(&grpprl)
            .find(|p| p.sprm == 0x6646)
            .map(|p| p.u32() as usize);
        if let Some(at) = huge {
            let size = crate::bytes::u16_at(self.data, at).unwrap_or(0) as usize;
            grpprl.extend_from_slice(crate::bytes::slice(self.data, at + 2, size));
        }
        let prm = self.pieces.prm_grpprl(piece.prm);
        grpprl.extend(
            prls(&prm)
                .filter(|p| p.sgc() == 1 || p.sgc() == 5)
                .flat_map(|p| {
                    let mut bytes = p.sprm.to_le_bytes().to_vec();
                    bytes.extend_from_slice(p.operand);
                    bytes
                }),
        );
        (istd, grpprl)
    }

    fn paragraph_info(&self, mark_cp: u32) -> ParagraphInfo {
        let (istd, grpprl) = self.paragraph_formatting(mark_cp);
        let mut properties = ParagraphProperties::default();
        let mut extra = ParaExtra {
            istd,
            ..ParaExtra::default()
        };
        apply_pap(&grpprl, &mut properties, &mut extra);
        properties.numbering = numbering_ref(&extra).or(properties.numbering);
        let style_id = self.sheet.id(extra.istd).or_else(|| self.sheet.id(0));
        let mark = self.text.get(mark_cp as usize).copied().unwrap_or(0x0D);
        ParagraphInfo {
            paragraph: Paragraph {
                style_id,
                properties,
                mark: RunProperties::default(),
                inlines: Vec::new(),
            },
            extra,
            mark,
            grpprl,
        }
    }

    /// The style-level run properties a toggle operand is relative to.
    fn base_run(&self, istd: u16, character_style: Option<String>) -> RunProperties {
        let key = (istd, character_style.clone());
        if let Some(cached) = self.base_cache.borrow().get(&key) {
            return cached.clone();
        }
        let paragraph_style = self.sheet.id(istd);
        let direct = RunProperties {
            style_id: character_style,
            ..RunProperties::default()
        };
        let resolved = self.styles.resolve_run(paragraph_style.as_deref(), &direct);
        self.base_cache.borrow_mut().insert(key, resolved.clone());
        resolved
    }

    /// Run properties for the characters at `cp`, and the run's extras.
    fn run_formatting(&self, cp: u32, istd: u16) -> (RunProperties, CharExtra, u32) {
        let Some(piece) = self.pieces.piece_at(cp) else {
            return (RunProperties::default(), CharExtra::default(), cp + 1);
        };
        let fc = piece.fc_of(cp);
        let run = find(&self.chpx, fc);
        let chpx: &[u8] = run.map_or(&[], |r| self.grpprl(r));
        let prm = self.pieces.prm_grpprl(piece.prm);
        let character_style = prls(chpx)
            .chain(prls(&prm))
            .filter(|p| p.sprm == 0x4A30)
            .last()
            .and_then(|p| self.sheet.id(p.u16()));
        let base = self.base_run(istd, character_style);
        let context = CharContext {
            fonts: &self.fonts,
            styles: &self.sheet.ids,
            base: &base,
        };
        let mut props = RunProperties::default();
        let mut extra = CharExtra::default();
        apply_chp(chpx, &mut props, &mut extra, &context);
        let character: Vec<u8> = prls(&prm)
            .filter(|p| p.sgc() == 2)
            .flat_map(|p| {
                let mut bytes = p.sprm.to_le_bytes().to_vec();
                bytes.extend_from_slice(p.operand);
                bytes
            })
            .collect();
        apply_chp(&character, &mut props, &mut extra, &context);
        let run_end = run.map_or(u32::MAX, |r| r.fc_end);
        let cps_left_in_run = run_end.saturating_sub(fc).div_ceil(piece.char_size());
        let end = piece.cp_end.min(cp.saturating_add(cps_left_in_run.max(1)));
        (props, extra, end)
    }

    fn revision(&self, extra: &CharExtra) -> Option<Revision> {
        let (kind, author, date) = match (extra.deleted, extra.inserted) {
            (true, _) => (
                RevisionKind::Deletion,
                extra.author_deleted.or(extra.author),
                extra.date_deleted.or(extra.date),
            ),
            (false, true) => (RevisionKind::Insertion, extra.author, extra.date),
            _ => return None,
        };
        Some(Revision {
            kind,
            author: author.and_then(|a| self.authors.get(usize::from(a)).cloned()),
            date: date.and_then(dttm),
            inlines: Vec::new(),
        })
    }

    /// Adds an image to the media list, reusing the entry of an identical
    /// image drawn earlier.
    fn add_media(&self, image: Image) -> MediaId {
        let key = image
            .data
            .iter()
            .fold(0xCBF2_9CE4_8422_2325u64, |hash, &b| {
                (hash ^ u64::from(b)).wrapping_mul(0x0100_0000_01B3)
            });
        let mut media = self.media.borrow_mut();
        let mut seen = self.media_index.borrow_mut();
        let candidates = seen.entry(key).or_default();
        if let Some(&existing) = candidates
            .iter()
            .find(|id: &&MediaId| *media[id.0 as usize].data == image.data[..])
        {
            return existing;
        }
        candidates.push(MediaId(media.len() as u32));
        let extension = image
            .content_type
            .rsplit(['/', '-'])
            .next()
            .unwrap_or("bin");
        let id = MediaId(media.len() as u32);
        let name = format!("media/image{}.{extension}", media.len() + 1);
        media.push(Media {
            name,
            content_type: image.content_type.to_string(),
            data: image.data.into(),
        });
        id
    }

    fn picture(&self, location: u32) -> Option<RunContent> {
        let Some(picture) = inline_picture(self.data, location as usize) else {
            self.report(Diagnostic::dropped(
                "Data",
                format!("picture at {location:#x} has no readable PICF"),
            ));
            return None;
        };
        let media = picture.image.map(|image| self.add_media(image));
        if media.is_none() {
            self.report(Diagnostic::dropped(
                "Data",
                format!("picture at {location:#x} has no decodable BLIP"),
            ));
        }
        Some(RunContent::Drawing(Drawing {
            media,
            width: i64::from(picture.width) * 635,
            height: i64::from(picture.height) * 635,
            placement: DrawingPlacement::Inline,
            name: None,
            description: None,
        }))
    }

    fn floating(&self, cp: u32) -> Option<RunContent> {
        let anchor = self.anchors.get(&cp)?;
        let media = self
            .shape_blips
            .get(&anchor.shape_id)
            .and_then(|&index| self.blip_store.get(index).cloned().flatten())
            .map(|image| self.add_media(image));
        if media.is_none() {
            self.report(Diagnostic::dropped(
                "WordDocument",
                format!(
                    "shape {} at CP {cp} is not a picture docboss reads",
                    anchor.shape_id
                ),
            ));
            return None;
        }
        Some(RunContent::Drawing(Drawing {
            media,
            width: i64::from(anchor.right - anchor.left) * 635,
            height: i64::from(anchor.bottom - anchor.top) * 635,
            placement: DrawingPlacement::Anchored {
                x: i64::from(anchor.left) * 635,
                y: i64::from(anchor.top) * 635,
                behind_text: anchor.behind_text,
                relative_to_page: anchor.relative_to_page,
            },
            name: None,
            description: None,
        }))
    }

    fn markers_in(&self, start: u32, end: u32) -> &[(u32, u8, Marker)] {
        let from = self.markers.partition_point(|(cp, _, _)| *cp < start);
        let to = self.markers.partition_point(|(cp, _, _)| *cp < end);
        &self.markers[from..to]
    }

    /// The inline content of `start..end` (the paragraph without its mark).
    fn inlines(&self, start: u32, end: u32, istd: u16, kind: StoryKind) -> Vec<Inline> {
        let mut out = Inlines::default();
        let markers = self.markers_in(start, end + 1);
        let mut next_marker = 0;
        let mut cp = start;
        while cp < end {
            let (props, extra, run_end) = self.run_formatting(cp, istd);
            let revision = self.revision(&extra);
            let run_end = run_end.min(end).max(cp + 1);
            let mut text = String::new();
            let mut units: Vec<u16> = Vec::new();
            let flush = |units: &mut Vec<u16>, text: &mut String, out: &mut Inlines| {
                text.extend(char::decode_utf16(units.drain(..)).map(|r| r.unwrap_or('\u{FFFD}')));
                if !text.is_empty() {
                    out.content(
                        &props,
                        revision.as_ref(),
                        RunContent::Text(std::mem::take(text)),
                    );
                }
            };
            for at in cp..run_end {
                while let Some((marker_cp, _, marker)) = markers.get(next_marker) {
                    if *marker_cp > at {
                        break;
                    }
                    flush(&mut units, &mut text, &mut out);
                    out.push(marker_inline(marker));
                    next_marker += 1;
                }
                let unit = self.text.get(at as usize).copied().unwrap_or(0);
                if unit >= 0x20 && !(extra.special && unit == 0x28) {
                    units.push(unit);
                    continue;
                }
                flush(&mut units, &mut text, &mut out);
                let content = match unit {
                    0x09 => Some(RunContent::Tab),
                    0x0B => Some(RunContent::Break(Break::Line)),
                    0x0C => Some(RunContent::Break(Break::Page)),
                    0x0E => Some(RunContent::Break(Break::Column)),
                    0x1E => Some(RunContent::NoBreakHyphen),
                    0x1F => Some(RunContent::SoftHyphen),
                    0x13 => {
                        out.begin_field();
                        None
                    }
                    0x14 => {
                        out.separate_field();
                        None
                    }
                    0x15 => {
                        out.end_field();
                        None
                    }
                    0x01 if extra.special && !extra.ole2 && !extra.data => extra
                        .pic_location
                        .and_then(|location| self.picture(location)),
                    0x02 if extra.special => match (kind, self.references.get(&at)) {
                        (StoryKind::Main, Some(Reference::Footnote(id))) => {
                            Some(RunContent::FootnoteReference(*id))
                        }
                        (StoryKind::Main, Some(Reference::Endnote(id))) => {
                            Some(RunContent::EndnoteReference(*id))
                        }
                        (StoryKind::Note, _) => Some(RunContent::NoteNumber),
                        _ => None,
                    },
                    0x05 if extra.special => match (kind, self.references.get(&at)) {
                        (StoryKind::Main, Some(Reference::Comment(id))) => {
                            Some(RunContent::CommentReference(*id))
                        }
                        _ => None,
                    },
                    0x08 if extra.special => self.floating(at),
                    0x28 => extra.symbol.map(|(font, char)| RunContent::Symbol {
                        font: self.fonts.get(usize::from(font)).cloned(),
                        char: u32::from(char),
                    }),
                    _ => None,
                };
                if let Some(content) = content {
                    out.content(&props, revision.as_ref(), content);
                }
            }
            flush(&mut units, &mut text, &mut out);
            cp = run_end;
        }
        for (_, _, marker) in &markers[next_marker..] {
            out.push(marker_inline(marker));
        }
        out.finish()
    }

    /// Splits `start..end` into paragraphs and assembles blocks.
    pub fn story(&self, start: u32, end: u32, kind: StoryKind) -> Vec<Block> {
        let end = end.min(self.text.len() as u32);
        let mut paragraphs = Vec::new();
        let mut paragraph_start = start;
        for cp in start..end {
            let unit = self.text[cp as usize];
            let ends = unit == 0x0D || unit == 0x07 || (unit == 0x0C && cp + 1 == end);
            if !ends {
                continue;
            }
            paragraphs.push(self.paragraph(paragraph_start, cp, kind));
            paragraph_start = cp + 1;
        }
        if paragraph_start < end {
            paragraphs.push(self.paragraph(paragraph_start, end, kind));
        }
        blocks(&mut paragraphs.into_iter().peekable(), self)
    }

    fn paragraph(&self, start: u32, mark: u32, kind: StoryKind) -> ParagraphInfo {
        let mut info = self.paragraph_info(mark.min(self.text.len().saturating_sub(1) as u32));
        info.paragraph.inlines = self.inlines(start, mark, info.extra.istd, kind);
        info
    }
}

fn marker_inline(marker: &Marker) -> Inline {
    match marker {
        Marker::BookmarkStart(id, name) => Inline::BookmarkStart {
            id: *id,
            name: name.clone(),
        },
        Marker::BookmarkEnd(id) => Inline::BookmarkEnd { id: *id },
        Marker::CommentStart(id) => Inline::CommentRangeStart(*id),
        Marker::CommentEnd(id) => Inline::CommentRangeEnd(*id),
    }
}

type Paragraphs = std::iter::Peekable<std::vec::IntoIter<ParagraphInfo>>;

fn is_row_end(info: &ParagraphInfo, level: i32) -> bool {
    match level {
        1 => {
            info.extra.ttp || (info.mark == 0x07 && info.extra.inner_ttp && info.extra.depth() == 1)
        }
        _ => info.extra.inner_ttp,
    }
}

fn is_cell_end(info: &ParagraphInfo, level: i32) -> bool {
    match level {
        1 => info.mark == 0x07,
        _ => info.extra.inner_cell,
    }
}

/// Top-level blocks: paragraphs outside tables, and a table for each run
/// of paragraphs inside one.
fn blocks(paragraphs: &mut Paragraphs, context: &Context<'_>) -> Vec<Block> {
    let mut out = Vec::new();
    while let Some(next) = paragraphs.peek() {
        if next.extra.depth() > 0 {
            out.push(Block::Table(table(paragraphs, 1, context)));
            continue;
        }
        let Some(info) = paragraphs.next() else {
            break;
        };
        out.push(Block::Paragraph(info.paragraph));
    }
    out
}

/// A table at `level`: cells end at cell marks, rows at row marks.
fn table(paragraphs: &mut Paragraphs, level: i32, context: &Context<'_>) -> Table {
    let mut out = Table::default();
    let mut cells: Vec<TableCell> = Vec::new();
    let mut cell_blocks: Vec<Block> = Vec::new();
    let mut edges: Vec<Vec<i32>> = Vec::new();
    while let Some(next) = paragraphs.peek() {
        let depth = next.extra.depth();
        if depth < level {
            break;
        }
        if depth > level {
            cell_blocks.push(Block::Table(table(paragraphs, level + 1, context)));
            continue;
        }
        let Some(info) = paragraphs.next() else {
            break;
        };
        if is_row_end(&info, level) {
            if !cell_blocks.is_empty() {
                cells.push(TableCell {
                    blocks: std::mem::take(&mut cell_blocks),
                    ..TableCell::default()
                });
            }
            let row = RowInfo::parse(&info.grpprl);
            edges.push(row.edges.clone());
            out.rows.push(finish_row(std::mem::take(&mut cells), &row));
            if out.properties == Default::default() {
                out.properties = row.table.clone();
            }
            continue;
        }
        let ends_cell = is_cell_end(&info, level);
        cell_blocks.push(Block::Paragraph(info.paragraph));
        if ends_cell {
            cells.push(TableCell {
                blocks: std::mem::take(&mut cell_blocks),
                ..TableCell::default()
            });
        }
    }
    if !cell_blocks.is_empty() {
        cells.push(TableCell {
            blocks: cell_blocks,
            ..TableCell::default()
        });
    }
    if !cells.is_empty() {
        context.report(Diagnostic::approximated(
            "WordDocument",
            "table row without a row mark; kept as a row",
        ));
        out.rows.push(finish_row(cells, &RowInfo::default()));
        edges.push(Vec::new());
    }
    apply_grid(&mut out, &edges);
    out
}

fn finish_row(cells: Vec<TableCell>, row: &RowInfo) -> TableRow {
    let mut out: Vec<TableCell> = Vec::with_capacity(cells.len());
    for (index, mut cell) in cells.into_iter().enumerate() {
        cell.properties = row.cell_properties(index);
        let merged = row
            .cells
            .get(index)
            .is_some_and(|c| c.horizontal_merge == 1);
        if let (true, Some(previous)) = (merged, out.last_mut()) {
            previous.properties.grid_span += 1;
            previous.properties.width = match (previous.properties.width, cell.properties.width) {
                (Some(a), Some(b)) => Some(a + b),
                (a, _) => a,
            };
            continue;
        }
        out.push(cell);
    }
    TableRow {
        properties: row.row.clone(),
        cells: out,
    }
}

/// Derives the table grid from every row's cell edges and sets each cell's
/// span to the grid columns it covers.
fn apply_grid(table: &mut Table, edges: &[Vec<i32>]) {
    let mut all: Vec<i32> = edges.iter().flatten().copied().collect();
    all.sort_unstable();
    all.dedup();
    if all.len() < 2 {
        return;
    }
    table.grid = all.windows(2).map(|w| w[1] - w[0]).collect();
    for (row, row_edges) in table.rows.iter_mut().zip(edges) {
        if row_edges.len() < 2 {
            continue;
        }
        let mut edge = 0usize;
        for cell in &mut row.cells {
            let span = cell.properties.grid_span.max(1) as usize;
            let (Some(&left), Some(&right)) = (row_edges.get(edge), row_edges.get(edge + span))
            else {
                break;
            };
            let first = all.partition_point(|&e| e < left);
            let last = all.partition_point(|&e| e < right);
            cell.properties.grid_span = (last - first).max(1) as u32;
            edge += span;
        }
    }
}

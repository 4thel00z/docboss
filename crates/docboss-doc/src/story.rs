//! Turns a CP range of the document text into model blocks: paragraph
//! boundaries ([MS-DOC] §2.4.2), direct formatting ([MS-DOC] §2.4.6),
//! fields, special characters and tables ([MS-DOC] §2.4.3 to §2.4.5).
//! [MS-DOC] §2.4.2, §2.4.3, §2.4.4, §2.4.5, §2.4.6, §2.9.88, §2.9.89.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use docboss_model::{
    Block, Borders, Break, ChildBox, Diagnostic, Drawing, DrawingPlacement, DrawingPosition, Field,
    GroupFrame, GroupMember, Hyperlink, Inline, Media, MediaId, Paragraph, ParagraphProperties,
    PositionAlign, PositionBase, Revision, RevisionKind, Run, RunContent, RunProperties,
    ShapeFormat, Styles, Table, TableCell, TableRow,
};

use crate::fkp::{find, FormatRun};
use crate::picture::{inline_picture, Bounds, GroupChild, Image, ShapeGroup};
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

/// A floating shape's alignment and the base it aligns on, horizontal then
/// vertical; None on an axis positioned by offset.
pub type ShapeAlignment = [Option<(PositionAlign, PositionBase)>; 2];

/// One axis of a floating shape's position: its Spa offset in twips from
/// `base`, which already places a left, centered or right shape where the
/// writer laid it out, or its alignment when that is inside or outside,
/// which depends on the page it lands on.
fn position(
    base: PositionBase,
    twips: i32,
    alignment: Option<(PositionAlign, PositionBase)>,
) -> DrawingPosition {
    let Some((align, base)) =
        alignment.filter(|(a, _)| matches!(a, PositionAlign::Inside | PositionAlign::Outside))
    else {
        return DrawingPosition::offset(base, i64::from(twips) * 635);
    };
    DrawingPosition {
        base,
        align: Some(align),
        offset: 0,
    }
}

const MAX_GROUP_DEPTH: usize = 16;
/// The bytes at each end of an image that its media key hashes.
const MEDIA_KEY_BYTES: usize = 4096;

/// A group's frame: its box at the left and top of `anchor` of `extent` in
/// its parent's coordinates, holding the coordinate space `space`
/// ([MS-ODRAW] §2.2.38, §2.2.39).
fn group_frame(
    anchor: Bounds,
    extent: (f64, f64),
    space: Bounds,
    rotation: i32,
    flip_horizontal: bool,
    flip_vertical: bool,
) -> GroupFrame {
    GroupFrame {
        offset: (f64::from(anchor[0]), f64::from(anchor[1])),
        extent,
        child_offset: (f64::from(space[0]), f64::from(space[1])),
        child_extent: (
            f64::from(space[2]) - f64::from(space[0]),
            f64::from(space[3]) - f64::from(space[1]),
        ),
        rotation,
        flip_horizontal,
        flip_vertical,
    }
}

/// The unturned extent of a shape whose anchor holds `width` by `height`:
/// Word stores the anchor of a shape turned by 45 to 135 or 225 to 315
/// degrees with its sides swapped about its center, as LibreOffice reads
/// it.
fn turned_extent(width: i64, height: i64, rotation: i32) -> (i64, i64) {
    let degrees = rotation / 60_000;
    match (45..135).contains(&degrees) || (225..315).contains(&degrees) {
        true => (height, width),
        false => (width, height),
    }
}

/// A floating shape anchored at a CP ([MS-DOC] §2.9.253 Spa).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Anchor {
    pub shape_id: u32,
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
    /// The origins of `left` and `top` from the Spa's bx and by.
    pub horizontal: docboss_model::PositionBase,
    pub vertical: docboss_model::PositionBase,
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
    TextBox,
}

type RunKey = ((usize, usize), u16, u16);
type BaseKey = (u16, Option<u16>);

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
    /// Fill, line and text frame of each floating shape by shape id.
    pub shape_formats: HashMap<u32, docboss_model::ShapeFormat>,
    /// The preset geometry of each floating shape by shape id.
    pub shape_geometries: HashMap<u32, docboss_model::Geometry>,
    /// Horizontal and vertical alignment of each floating shape by shape id.
    pub shape_alignments: HashMap<u32, ShapeAlignment>,
    /// Groups of shapes by the shape id of the group shape.
    pub shape_groups: HashMap<u32, crate::picture::ShapeGroup>,
    /// Text box stories by shape id: the CP range of each.
    pub text_boxes: HashMap<u32, (u32, u32)>,
    pub blip_store: Vec<Option<Image>>,
    pub media: RefCell<Vec<Media>>,
    pub diagnostics: RefCell<Vec<Diagnostic>>,
    base_cache: RefCell<HashMap<BaseKey, Rc<RunProperties>>>,
    /// Run formatting by CHPX grpprl, piece Prm and paragraph style: runs
    /// sharing a CHPX in the FKPs are formatted once.
    run_cache: RefCell<HashMap<RunKey, Rc<(RunProperties, CharExtra)>>>,
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
            shape_formats: HashMap::new(),
            shape_groups: HashMap::new(),
            shape_geometries: HashMap::new(),
            shape_alignments: HashMap::new(),
            text_boxes: HashMap::new(),
            blip_store: Vec::new(),
            media: RefCell::new(Vec::new()),
            diagnostics: RefCell::new(Vec::new()),
            base_cache: RefCell::new(HashMap::new()),
            run_cache: RefCell::new(HashMap::new()),
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
    /// Cached by paragraph style and character style istd.
    fn base_run(&self, istd: u16, character_istd: Option<u16>) -> Rc<RunProperties> {
        let key = (istd, character_istd);
        if let Some(cached) = self.base_cache.borrow().get(&key) {
            return Rc::clone(cached);
        }
        let paragraph_style = self.sheet.id(istd);
        let direct = RunProperties {
            style_id: character_istd.and_then(|c| self.sheet.id(c)),
            ..RunProperties::default()
        };
        let resolved = Rc::new(self.styles.resolve_run(paragraph_style.as_deref(), &direct));
        self.base_cache
            .borrow_mut()
            .insert(key, Rc::clone(&resolved));
        resolved
    }

    /// Run properties for the characters at `cp`, and the run's extras.
    fn run_formatting(&self, cp: u32, istd: u16) -> (Rc<(RunProperties, CharExtra)>, u32) {
        let Some(piece) = self.pieces.piece_at(cp) else {
            return (Rc::default(), cp + 1);
        };
        let fc = piece.fc_of(cp);
        let run = find(&self.chpx, fc);
        let run_end = run.map_or(u32::MAX, |r| r.fc_end);
        let cps_left_in_run = run_end.saturating_sub(fc).div_ceil(piece.char_size());
        let end = piece.cp_end.min(cp.saturating_add(cps_left_in_run.max(1)));
        let key = (run.map_or((0, 0), |r| r.grpprl), piece.prm, istd);
        if let Some(cached) = self.run_cache.borrow().get(&key) {
            return (Rc::clone(cached), end);
        }
        let chpx: &[u8] = run.map_or(&[], |r| self.grpprl(r));
        let prm = self.pieces.prm_grpprl(piece.prm);
        let character_istd = prls(chpx)
            .chain(prls(&prm))
            .filter(|p| p.sprm == 0x4A30)
            .last()
            .map(|p| p.u16())
            .filter(|&c| self.sheet.id(c).is_some());
        let base = self.base_run(istd, character_istd);
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
        let formatted = Rc::new((props, extra));
        self.run_cache
            .borrow_mut()
            .insert(key, Rc::clone(&formatted));
        (formatted, end)
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
    /// image drawn earlier. Images are bucketed by their length and a hash
    /// of their first and last few kilobytes, and compared in full.
    fn add_media(&self, image: Image) -> MediaId {
        let data = &image.data;
        let edge = data.len().min(MEDIA_KEY_BYTES);
        let key = data[..edge]
            .chunks(8)
            .chain(data[data.len() - edge..].chunks(8))
            .fold(
                0xCBF2_9CE4_8422_2325u64 ^ data.len() as u64,
                |hash, chunk| {
                    let mut word = [0u8; 8];
                    word[..chunk.len()].copy_from_slice(chunk);
                    (hash ^ u64::from_le_bytes(word)).wrapping_mul(0x0100_0000_01B3)
                },
            );
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
            data: image.data,
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
        Some(RunContent::Drawing(Box::new(Drawing {
            media,
            width: i64::from(picture.width) * 635,
            height: i64::from(picture.height) * 635,
            placement: DrawingPlacement::Inline,
            name: None,
            description: None,
            text_box: Vec::new(),
            shape: Default::default(),
            geometry: None,
            members: Vec::new(),
            data_text: Vec::new(),
            chart: None,
        })))
    }

    /// A floating shape: its picture from the BLIP store, or its text box
    /// story ([MS-DOC] §2.3.6, §2.8.32 PlcftxbxTxt), or a preset shape, or
    /// a group of them.
    fn floating(&self, cp: u32) -> Option<RunContent> {
        let anchor = self.anchors.get(&cp)?;
        let alignment = self
            .shape_alignments
            .get(&anchor.shape_id)
            .copied()
            .unwrap_or_default();
        let rotation = self
            .shape_formats
            .get(&anchor.shape_id)
            .map_or(0, |format| format.rotation);
        let stated = (
            i64::from(anchor.right - anchor.left) * 635,
            i64::from(anchor.bottom - anchor.top) * 635,
        );
        let (width, height) = turned_extent(stated.0, stated.1, rotation);
        let mut drawing = match self.shape_groups.get(&anchor.shape_id) {
            Some(group) => {
                let shape = self
                    .shape_formats
                    .get(&anchor.shape_id)
                    .copied()
                    .unwrap_or_default();
                let frame = group_frame(
                    [0, 0, 0, 0],
                    (width as f64, height as f64),
                    group.space,
                    shape.rotation,
                    shape.flip_horizontal,
                    shape.flip_vertical,
                );
                let mut drawing = Drawing::default();
                self.group_members(group, &mut vec![frame], &mut drawing.members, 0);
                drawing
            }
            None => self.shape_drawing(anchor.shape_id),
        };
        if drawing.media.is_none()
            && drawing.text_box.is_empty()
            && drawing.geometry.is_none()
            && drawing.members.is_empty()
        {
            self.report(Diagnostic::dropped(
                "WordDocument",
                format!(
                    "shape {} at CP {cp} is neither a picture, a text box, a preset shape nor a group",
                    anchor.shape_id
                ),
            ));
            return None;
        }
        let (dx, dy) = ((stated.0 - width) / 2, (stated.1 - height) / 2);
        let mut horizontal = position(anchor.horizontal, anchor.left, alignment[0]);
        let mut vertical = position(anchor.vertical, anchor.top, alignment[1]);
        horizontal.offset += dx;
        vertical.offset += dy;
        drawing.width = width;
        drawing.height = height;
        drawing.placement = DrawingPlacement::Anchored {
            horizontal,
            vertical,
            behind_text: anchor.behind_text,
        };
        Some(RunContent::Drawing(Box::new(drawing)))
    }

    /// The drawing of one shape by id, without its size and placement: its
    /// picture, text box or preset geometry with its format.
    fn shape_drawing(&self, id: u32) -> Drawing {
        let media = self
            .shape_blips
            .get(&id)
            .and_then(|&index| self.blip_store.get(index).cloned().flatten())
            .map(|image| self.add_media(image));
        let text_box = match self.text_boxes.get(&id) {
            Some(&(start, end)) => self.story(start, end, StoryKind::TextBox),
            None => Vec::new(),
        };
        let geometry = media
            .is_none()
            .then(|| self.shape_geometries.get(&id).cloned().map(Box::new))
            .flatten();
        let format = self.shape_formats.get(&id).copied().unwrap_or_default();
        let shape = match text_box.is_empty() && geometry.is_none() {
            true => ShapeFormat {
                rotation: format.rotation,
                flip_horizontal: format.flip_horizontal,
                flip_vertical: format.flip_vertical,
                ..Default::default()
            },
            false => format,
        };
        Drawing {
            media,
            shape,
            text_box,
            geometry,
            ..Default::default()
        }
    }

    /// Adds the members of a group, placed through `frames` (the group's
    /// own frame last), to `out`, descending into nested groups.
    fn group_members(
        &self,
        group: &ShapeGroup,
        frames: &mut Vec<GroupFrame>,
        out: &mut Vec<GroupMember>,
        depth: usize,
    ) {
        if depth > MAX_GROUP_DEPTH {
            return;
        }
        for member in &group.members {
            match member {
                GroupChild::Shape { id, anchor } => {
                    let drawing = self.shape_drawing(*id);
                    let shape = drawing.shape;
                    let (width, height) = (
                        f64::from(anchor[2]) - f64::from(anchor[0]),
                        f64::from(anchor[3]) - f64::from(anchor[1]),
                    );
                    let (w, h) = turned_extent(width as i64, height as i64, shape.rotation);
                    let placed = ChildBox {
                        x: f64::from(anchor[0]) + (width - w as f64) / 2.0,
                        y: f64::from(anchor[1]) + (height - h as f64) / 2.0,
                        width: w as f64,
                        height: h as f64,
                        rotation: shape.rotation,
                        flip_horizontal: shape.flip_horizontal,
                        flip_vertical: shape.flip_vertical,
                    };
                    let drawn = drawing.media.is_some()
                        || !drawing.text_box.is_empty()
                        || drawing.geometry.is_some();
                    if drawn {
                        let placed = GroupFrame::place_through(frames, placed);
                        out.push(GroupMember::new(drawing, placed));
                    }
                }
                GroupChild::Group { anchor, group } => {
                    let shape = self
                        .shape_formats
                        .get(&group.id)
                        .copied()
                        .unwrap_or_default();
                    let extent = (
                        f64::from(anchor[2]) - f64::from(anchor[0]),
                        f64::from(anchor[3]) - f64::from(anchor[1]),
                    );
                    frames.push(group_frame(
                        *anchor,
                        extent,
                        group.space,
                        shape.rotation,
                        shape.flip_horizontal,
                        shape.flip_vertical,
                    ));
                    self.group_members(group, frames, out, depth + 1);
                    frames.pop();
                }
            }
        }
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
            let (formatted, run_end) = self.run_formatting(cp, istd);
            let (props, extra) = &*formatted;
            let revision = self.revision(extra);
            let run_end = run_end.min(end).max(cp + 1);
            let mut pending: Option<u32> = None;
            let flush = |pending: &mut Option<u32>, upto: u32, out: &mut Inlines| {
                let Some(from) = pending.take() else {
                    return;
                };
                let units = self.text.get(from as usize..upto as usize).unwrap_or(&[]);
                let text: String = char::decode_utf16(units.iter().copied())
                    .map(|r| r.unwrap_or('\u{FFFD}'))
                    .collect();
                if !text.is_empty() {
                    out.content(props, revision.as_ref(), RunContent::Text(text));
                }
            };
            for at in cp..run_end {
                while let Some((marker_cp, _, marker)) = markers.get(next_marker) {
                    if *marker_cp > at {
                        break;
                    }
                    flush(&mut pending, at, &mut out);
                    out.push(marker_inline(marker));
                    next_marker += 1;
                }
                let unit = self.text.get(at as usize).copied().unwrap_or(0);
                if unit >= 0x20 && !(extra.special && unit == 0x28) {
                    pending.get_or_insert(at);
                    continue;
                }
                flush(&mut pending, at, &mut out);
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
                    out.content(props, revision.as_ref(), content);
                }
            }
            flush(&mut pending, run_end, &mut out);
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
        let mut paragraph_start = start;
        let mut cp = start;
        let paragraphs = std::iter::from_fn(move || {
            while cp < end {
                let at = cp;
                cp += 1;
                let unit = self.text[at as usize];
                let ends = unit == 0x0D || unit == 0x07 || (unit == 0x0C && at + 1 == end);
                if !ends {
                    continue;
                }
                let paragraph = self.paragraph(paragraph_start, at, kind);
                paragraph_start = at + 1;
                return Some(paragraph);
            }
            if paragraph_start < end {
                let paragraph = self.paragraph(paragraph_start, end, kind);
                paragraph_start = end;
                return Some(paragraph);
            }
            None
        });
        let boxed: Box<dyn Iterator<Item = ParagraphInfo> + '_> = Box::new(paragraphs);
        let mut paragraphs: Paragraphs<'_> = boxed.peekable();
        blocks(&mut paragraphs, self)
    }

    fn paragraph(&self, start: u32, mark: u32, kind: StoryKind) -> ParagraphInfo {
        let mark_cp = mark.min(self.text.len().saturating_sub(1) as u32);
        let mut info = self.paragraph_info(mark_cp);
        info.paragraph.mark = self.run_formatting(mark_cp, info.extra.istd).0 .0.clone();
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
        Marker::CommentStart(id) => Inline::CommentRangeStart { id: *id },
        Marker::CommentEnd(id) => Inline::CommentRangeEnd { id: *id },
    }
}

type Paragraphs<'p> = std::iter::Peekable<Box<dyn Iterator<Item = ParagraphInfo> + 'p>>;

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
fn blocks(paragraphs: &mut Paragraphs<'_>, context: &Context<'_>) -> Vec<Block> {
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
fn table(paragraphs: &mut Paragraphs<'_>, level: i32, context: &Context<'_>) -> Table {
    let mut out = Table::default();
    let mut cells: Vec<TableCell> = Vec::new();
    let mut cell_blocks: Vec<Block> = Vec::new();
    let mut edges: Vec<Vec<i32>> = Vec::new();
    let mut row_borders: Vec<Option<Borders>> = Vec::new();
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
            row_borders.push(row.table.borders);
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
        row_borders.push(None);
    }
    apply_grid(&mut out, &edges);
    if row_borders.windows(2).any(|pair| pair[0] != pair[1]) {
        apply_row_borders(&mut out, &row_borders);
    }
    out
}

/// Gives each cell its own row's table borders when the rows state
/// different ones: `sprmTTableBorders` belongs to a row ([MS-DOC]
/// §2.6.3), and a row without it has no borders beyond its cells' own.
fn apply_row_borders(table: &mut Table, row_borders: &[Option<Borders>]) {
    table.properties.borders = None;
    let count = table.rows.len();
    for (r, (row, borders)) in table.rows.iter_mut().zip(row_borders).enumerate() {
        let Some(outer) = borders else {
            continue;
        };
        let cells = row.cells.len();
        for (c, cell) in row.cells.iter_mut().enumerate() {
            let own = cell.properties.borders.unwrap_or_default();
            let inherited = Borders {
                top: own.top.or(if r == 0 {
                    outer.top
                } else {
                    outer.inside_horizontal
                }),
                bottom: own.bottom.or(if r + 1 == count {
                    outer.bottom
                } else {
                    outer.inside_horizontal
                }),
                left: own.left.or(if c == 0 {
                    outer.left
                } else {
                    outer.inside_vertical
                }),
                right: own.right.or(if c + 1 == cells {
                    outer.right
                } else {
                    outer.inside_vertical
                }),
                ..own
            };
            cell.properties.borders = Some(inherited);
        }
    }
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
    if table.properties.indent.is_none() {
        table.properties.indent = Some(all[0]);
    }
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

//! Pagination: stacking laid-out slabs into columns and pages, with keep
//! rules, page and column breaks, sections, headers and footers, footnotes
//! and endnotes.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use docboss_font::FontDatabase;
use docboss_model::{
    Block, Break, Color, Diagnostic, Document, DrawingPosition, DropCap, FrameProperties,
    HeaderFooterRefs, Inline, LineCap, MediaId, NoteKind, NumberFormat, NumberingCounter,
    PageBorderDisplay, PageBorderOffset, PageBorders, PositionAlign, PositionBase, RunContent,
    SectionBreak, SectionProperties, TableFloat, TextWrap, WrapKind, WrapSide,
};

use crate::paragraph::{layout_paragraph, Band, Resume};
use crate::shape::Shaper;
use crate::table::{border_line, border_width, layout_table, RowPlan};
use crate::units::{emu_to_pt, twips_to_pt};
use crate::wrap::{band, Exclusion};
use crate::{Item, LayoutOptions, LineStyle, Page, Rect};

const MAX_PAGES: usize = 100_000;
const SEPARATOR_HEIGHT: f32 = 12.0;
const MAX_NESTING: usize = 24;
const MAX_RESTARTS: u32 = 16;

/// A drawing positioned independently of the text.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Floating {
    pub media: Option<MediaId>,
    /// Painted as its picture, or as a placeholder when it has none.
    pub picture: bool,
    pub width: f32,
    pub height: f32,
    pub horizontal: DrawingPosition,
    pub vertical: DrawingPosition,
    pub behind: bool,
    /// A text box's shape and text, relative to its top-left corner.
    pub content: Vec<Item>,
    /// How text flows around the drawing.
    pub wrap: docboss_model::TextWrap,
    /// For a drawing inside a table cell, the cell's column and the line
    /// holding the drawing, relative to the row's slab.
    pub local: Option<Local>,
    /// Tells the float apart from the document's other floats.
    pub id: u32,
}

/// The column and line a float inside a table cell is placed against,
/// relative to the top-left corner of its row.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Local {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Floating {
    /// The float moved `dx`, `dy` further into an enclosing row, its column
    /// `width` wide when it has none yet.
    pub(crate) fn nested(mut self, dx: f32, dy: f32, width: f32, height: f32) -> Floating {
        self.local = Some(match self.local {
            Some(local) => Local {
                x: local.x + dx,
                y: local.y + dy,
                ..local
            },
            None => Local {
                x: dx,
                y: dy,
                width,
                height,
            },
        });
        self.wrap = docboss_model::TextWrap::default();
        self
    }
}

/// The page geometry a floating drawing is placed against, in page
/// coordinates: each span is a start and a length.
pub(crate) struct Frame {
    pub page: (f32, f32),
    pub margin_x: (f32, f32),
    pub margin_y: (f32, f32),
    pub column: (f32, f32),
    /// The top and height of the line that holds the drawing's anchor.
    pub line: (f32, f32),
    pub odd: bool,
}

impl Frame {
    /// ECMA-376 Part 1 §20.4.3.4: the horizontal span `base` names.
    fn horizontal(&self, base: PositionBase) -> (f32, f32) {
        let (left, width) = self.margin_x;
        let right = left + width;
        let left_margin = (0.0, left);
        let right_margin = (right, (self.page.0 - right).max(0.0));
        match base {
            PositionBase::Page => (0.0, self.page.0),
            PositionBase::Margin => self.margin_x,
            PositionBase::LeftMargin => left_margin,
            PositionBase::RightMargin => right_margin,
            PositionBase::InsideMargin if self.odd => left_margin,
            PositionBase::InsideMargin => right_margin,
            PositionBase::OutsideMargin if self.odd => right_margin,
            PositionBase::OutsideMargin => left_margin,
            _ => self.column,
        }
    }

    /// ECMA-376 Part 1 §20.4.3.5: the vertical span `base` names.
    fn vertical(&self, base: PositionBase) -> (f32, f32) {
        let (top, height) = self.margin_y;
        let bottom = top + height;
        match base {
            PositionBase::Page => (0.0, self.page.1),
            PositionBase::Margin => self.margin_y,
            PositionBase::TopMargin | PositionBase::InsideMargin => (0.0, top),
            PositionBase::BottomMargin | PositionBase::OutsideMargin => {
                (bottom, (self.page.1 - bottom).max(0.0))
            }
            _ => self.line,
        }
    }

    /// ECMA-376 Part 1 §20.4.3.1, §20.4.3.2: the start of an object of
    /// `size` placed on `span` by `position`.
    fn place(&self, span: (f32, f32), size: f32, position: &DrawingPosition) -> f32 {
        let (start, length) = span;
        let Some(align) = position.align else {
            return start + emu_to_pt(position.offset);
        };
        let align = match (align, self.odd) {
            (PositionAlign::Inside, true) | (PositionAlign::Outside, false) => PositionAlign::Start,
            (PositionAlign::Inside, false) | (PositionAlign::Outside, true) => PositionAlign::End,
            (other, _) => other,
        };
        match align {
            PositionAlign::Center => start + (length - size) / 2.0,
            PositionAlign::End => start + length - size,
            _ => start,
        }
    }
}

impl Floating {
    /// The drawing's rectangle in page coordinates.
    fn rect(&self, frame: &Frame) -> Rect {
        let x = frame.place(
            frame.horizontal(self.horizontal.base),
            self.width,
            &self.horizontal,
        );
        let y = frame.place(
            frame.vertical(self.vertical.base),
            self.height,
            &self.vertical,
        );
        Rect::new(x, y, self.width, self.height)
    }

    /// The items the float paints on `frame`: its picture, then its text
    /// box or shape.
    fn items(&self, frame: &Frame) -> Vec<Item> {
        self.items_at(self.rect(frame))
    }

    /// The items the float paints with its top-left corner at `rect`'s.
    fn items_at(&self, rect: Rect) -> Vec<Item> {
        let picture = self.picture.then_some(Item::Image {
            media: self.media,
            rect,
        });
        picture
            .into_iter()
            .chain(self.content.iter().cloned().map(|mut item| {
                item.offset(rect.x, rect.y);
                item
            }))
            .collect()
    }
}

/// A vertical unit of content: one line of a paragraph or one table row.
/// Item coordinates are relative to the slab's top-left corner.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Slab {
    pub height: f32,
    /// How far across the slab its content reaches, for a line of text;
    /// zero when unknown.
    pub extent: f32,
    pub gap_before: f32,
    pub items: Vec<Item>,
    pub floats: Vec<Floating>,
    pub keep_with_next: bool,
    pub break_before: Option<Break>,
    pub break_after: Option<Break>,
    pub notes: Vec<i64>,
    pub row: Option<Box<RowPlan>>,
    pub repeat: Option<Arc<[Slab]>>,
    /// For a line of a body paragraph in a document with floats that text
    /// wraps around, where to break the paragraph again from.
    pub resume: Option<Box<Resume>>,
}

impl Slab {
    pub(crate) fn new(height: f32, items: Vec<Item>) -> Slab {
        Slab {
            height,
            extent: 0.0,
            gap_before: 0.0,
            items,
            floats: Vec::new(),
            keep_with_next: false,
            break_before: None,
            break_after: None,
            notes: Vec::new(),
            row: None,
            repeat: None,
            resume: None,
        }
    }
}

pub(crate) fn stack_height(slabs: &[Slab]) -> f32 {
    slabs.iter().map(|s| s.gap_before + s.height).sum()
}

/// State shared by every paragraph and table laid out for one document.
pub(crate) struct Ctx<'a> {
    pub doc: &'a Document,
    pub shaper: Shaper<'a>,
    pub counter: NumberingCounter<'a>,
    pub page_number: u32,
    /// The format of the current section's page numbers.
    pub page_format: Option<docboss_model::NumberFormat>,
    pub total_pages: Option<u32>,
    pub current_note: Option<String>,
    /// The style of the table whose cell is being laid out.
    pub table_style: Option<String>,
    /// The fill behind the blocks being laid out, such as a cell's shading.
    pub background: Option<docboss_model::Color>,
    /// Whether the paragraph being laid out shares its borders with the
    /// paragraph before it and the one after it.
    pub border_group: (bool, bool),
    /// A float text wraps around has been laid out.
    pub wraps: bool,
    /// The blocks being laid out are a section's body.
    pub flowing: bool,
    /// The height of the current section's text area.
    pub body_height: f32,
    /// The number of floats laid out so far.
    pub floats: u32,
    /// The paragraphs being laid out are a text frame's own.
    pub in_frame: bool,
    depth: usize,
    note_labels: HashMap<(bool, i64), String>,
    pub diagnostics: Vec<Diagnostic>,
}

impl Ctx<'_> {
    /// Whether paragraphs laid out now keep what breaking their lines
    /// again needs: body paragraphs, once text wraps around a float.
    pub(crate) fn retain_lines(&self) -> bool {
        self.wraps && self.flowing && self.depth == 1
    }

    pub(crate) fn next_float(&mut self) -> u32 {
        self.floats += 1;
        self.floats
    }

    pub(crate) fn note_label(&self, kind: NoteKind, id: i64) -> String {
        self.note_labels
            .get(&(kind == NoteKind::Footnote, id))
            .cloned()
            .unwrap_or_else(|| id.to_string())
    }
}

fn collect_notes(blocks: &[Block], out: &mut Vec<(NoteKind, i64)>) {
    fn inlines(list: &[Inline], out: &mut Vec<(NoteKind, i64)>) {
        for inline in list {
            match inline {
                Inline::Run(run) => {
                    for content in &run.content {
                        let found = match content {
                            RunContent::FootnoteReference(id) => (NoteKind::Footnote, *id),
                            RunContent::EndnoteReference(id) => (NoteKind::Endnote, *id),
                            _ => continue,
                        };
                        if !out.contains(&found) {
                            out.push(found);
                        }
                    }
                }
                Inline::Hyperlink(link) => inlines(&link.inlines, out),
                Inline::Field(field) => inlines(&field.result, out),
                Inline::Revision(revision) => inlines(&revision.inlines, out),
                _ => {}
            }
        }
    }
    for block in blocks {
        match block {
            Block::Paragraph(p) => inlines(&p.inlines, out),
            Block::Table(t) => t
                .rows
                .iter()
                .flat_map(|r| &r.cells)
                .for_each(|c| collect_notes(&c.blocks, out)),
        }
    }
}

/// Lays out a list of blocks at `width`, joining paragraph spacing, and
/// returns the slabs and the spacing after the last block.
pub(crate) fn layout_blocks(ctx: &mut Ctx<'_>, blocks: &[Block], width: f32) -> (Vec<Slab>, f32) {
    if ctx.depth >= MAX_NESTING {
        ctx.diagnostics.push(Diagnostic::dropped(
            "layout",
            "tables nested too deeply; inner content was not laid out",
        ));
        return (Vec::new(), 0.0);
    }
    ctx.depth += 1;
    let laid = layout_blocks_at_depth(ctx, blocks, width);
    ctx.depth -= 1;
    laid
}

fn layout_blocks_at_depth(ctx: &mut Ctx<'_>, blocks: &[Block], width: f32) -> (Vec<Slab>, f32) {
    let mut out: Vec<Slab> = Vec::new();
    let mut prev_after = 0.0;
    let mut prev_style: Option<Option<String>> = None;
    let mut prev_contextual = false;
    let (borders, frames): (
        Vec<Option<docboss_model::Borders>>,
        Vec<Option<FrameProperties>>,
    ) = blocks
        .iter()
        .map(|block| match block {
            Block::Paragraph(p) => {
                let props = ctx.doc.styles.resolve_paragraph_in(
                    p,
                    &ctx.doc.numbering,
                    ctx.table_style.as_deref(),
                );
                let borders = props
                    .borders
                    .filter(|b| *b != docboss_model::Borders::default());
                (borders, props.frame.filter(|_| !ctx.in_frame))
            }
            Block::Table(_) => (None, None),
        })
        .unzip();
    if ctx.flowing && ctx.depth == 1 && !ctx.wraps {
        ctx.wraps = frames
            .iter()
            .any(|f| f.is_some_and(|f| f.wrap != WrapKind::None))
            || blocks.iter().any(block_wraps);
    }
    let joins = |a: usize, b: usize| {
        borders[a].is_some() && borders[a] == borders[b] && frames[a] == frames[b]
    };
    let mut pending: Vec<Floating> = Vec::new();
    let mut index = 0;
    while index < blocks.len() {
        let block = &blocks[index];
        if let Some(frame) = frames[index] {
            let end = (index..blocks.len())
                .find(|&i| frames[i] != Some(frame))
                .unwrap_or(blocks.len());
            let float = frame_float(ctx, &blocks[index..end], &frame, width);
            pending.push(float);
            index = end;
            continue;
        }
        match block {
            Block::Paragraph(p) => {
                ctx.border_group = (
                    index > 0 && joins(index, index - 1),
                    index + 1 < blocks.len() && joins(index, index + 1),
                );
                let mut laid = layout_paragraph(ctx, p, width);
                ctx.border_group = (false, false);
                let same = prev_style.as_ref() == Some(&p.style_id);
                let before = if laid.contextual && same {
                    0.0
                } else {
                    laid.before
                };
                let after_prev = if prev_contextual && same {
                    0.0
                } else {
                    prev_after
                };
                if let Some(first) = laid.slabs.first_mut() {
                    first.gap_before = before + after_prev;
                    first.floats.splice(0..0, pending.drain(..));
                }
                prev_after = laid.after;
                prev_contextual = laid.contextual;
                prev_style = Some(p.style_id.clone());
                out.extend(laid.slabs);
            }
            Block::Table(t) => {
                if let Some(float) = t
                    .properties
                    .floating
                    .and_then(|position| table_float(ctx, t, &position, width))
                {
                    pending.push(float);
                    index += 1;
                    continue;
                }
                anchor_pending(&mut out, &mut pending);
                let (mut slabs, ..) = layout_table(ctx, t, width);
                if let Some(first) = slabs.first_mut() {
                    first.gap_before = prev_after;
                }
                prev_after = 0.0;
                prev_contextual = false;
                prev_style = None;
                out.extend(slabs);
            }
        }
        index += 1;
    }
    anchor_pending(&mut out, &mut pending);
    (out, prev_after)
}

/// Places floats that no paragraph follows on an empty slab of their own.
fn anchor_pending(out: &mut Vec<Slab>, pending: &mut Vec<Floating>) {
    if pending.is_empty() {
        return;
    }
    let mut slab = Slab::new(0.0, Vec::new());
    slab.floats = std::mem::take(pending);
    slab.keep_with_next = true;
    out.push(slab);
}

/// Whether a block holds a floating drawing or table text wraps around.
fn block_wraps(block: &Block) -> bool {
    fn inlines(list: &[Inline]) -> bool {
        list.iter().any(|inline| match inline {
            Inline::Run(run) => run.content.iter().any(|c| match c {
                RunContent::Drawing(d) => {
                    d.wrap.wraps()
                        && !matches!(d.placement, docboss_model::DrawingPlacement::Inline)
                }
                _ => false,
            }),
            Inline::Hyperlink(link) => inlines(&link.inlines),
            Inline::Field(field) => inlines(&field.result),
            Inline::Revision(revision) => inlines(&revision.inlines),
            _ => false,
        })
    }
    match block {
        Block::Paragraph(p) => inlines(&p.inlines),
        Block::Table(t) => t.properties.floating.is_some(),
    }
}

/// The paragraphs of one text frame laid out as a float at the frame's
/// width, or at their widest line when it has none, and height. A drop cap
/// stands at the start of the next paragraph, a margin one too as
/// LibreOffice places it; text wraps around the frame as `wrap` says,
/// `hSpace` and `vSpace` away.
/// ECMA-376 Part 1 §17.3.1.11, §17.18.20, §17.18.104.
fn frame_float(
    ctx: &mut Ctx<'_>,
    blocks: &[Block],
    frame: &FrameProperties,
    width: f32,
) -> Floating {
    let outer = std::mem::replace(&mut ctx.in_frame, true);
    let (mut slabs, _) =
        layout_blocks(ctx, blocks, frame.width.map_or(width, twips_to_pt).max(1.0));
    let mut inner = frame.width.map(twips_to_pt);
    if inner.is_none() {
        let widest = slabs
            .iter()
            .map(|s| s.extent)
            .fold(0.0, f32::max)
            .ceil()
            .clamp(1.0, width);
        if widest < width - 0.5 {
            slabs = layout_blocks(ctx, blocks, widest).0;
        }
        inner = Some(widest);
    }
    ctx.in_frame = outer;
    let inner = inner.unwrap_or(width).max(1.0);
    let content_height = stack_height(&slabs);
    let stated = twips_to_pt(frame.height);
    let height = match frame.height_rule {
        docboss_model::LineRule::Exact if stated > 0.0 => stated,
        docboss_model::LineRule::AtLeast => content_height.max(stated),
        _ => content_height,
    };
    let mut content = Vec::new();
    let mut y = 0.0;
    for slab in slabs {
        y += slab.gap_before;
        content.extend(slab.items.into_iter().map(|mut item| {
            item.offset(0.0, y);
            item
        }));
        y += slab.height;
    }
    if height > content_height + 0.5 {
        stretch_borders(&mut content, content_height, height);
    }
    let h_space = i64::from(frame.h_space) * 635;
    let v_space = i64::from(frame.v_space) * 635;
    let (horizontal, vertical) = match frame.drop_cap {
        DropCap::None => (frame.horizontal, frame.vertical),
        DropCap::Drop | DropCap::Margin => (
            DrawingPosition::offset(PositionBase::Column, 0),
            DrawingPosition::offset(PositionBase::Paragraph, 0),
        ),
    };
    let kind = match frame.drop_cap {
        DropCap::None => frame.wrap,
        _ => WrapKind::Square,
    };
    ctx.wraps |= kind != WrapKind::None && ctx.flowing;
    Floating {
        media: None,
        picture: false,
        width: inner,
        height,
        horizontal,
        vertical,
        behind: false,
        content,
        wrap: TextWrap {
            kind,
            side: WrapSide::Both,
            distance: [v_space, v_space, h_space, h_space],
            polygon: Vec::new(),
        },
        local: None,
        id: ctx.next_float(),
    }
}

/// Carries the borders of a frame's last paragraph down to the frame's
/// bottom, as the borders of a frame taller than its text enclose it.
fn stretch_borders(items: &mut [Item], bottom: f32, height: f32) {
    let near = |y: f32| (y - bottom).abs() < 1.5;
    for item in items.iter_mut() {
        let Item::Line { from, to, .. } = item else {
            continue;
        };
        let vertical = (from.0 - to.0).abs() < 0.01;
        match vertical {
            true if near(to.1) => to.1 += height - bottom,
            false if near(from.1) && near(to.1) => {
                from.1 += height - bottom;
                to.1 += height - bottom;
            }
            _ => {}
        }
    }
}

/// ECMA-376 Part 1 §17.4.57: a floating table laid out as a float at its
/// position, text wrapping around it at its distances. `None` when it is
/// taller than the text area; it then flows with the text.
fn table_float(
    ctx: &mut Ctx<'_>,
    table: &docboss_model::Table,
    position: &TableFloat,
    width: f32,
) -> Option<Floating> {
    let (slabs, left, table_width) = layout_table(ctx, table, width);
    let height = stack_height(&slabs);
    if height > ctx.body_height {
        ctx.diagnostics.push(Diagnostic::approximated(
            "layout",
            "a floating table taller than the page flows with the text",
        ));
        return None;
    }
    let mut content = Vec::new();
    let mut y = 0.0;
    for slab in slabs {
        y += slab.gap_before;
        content.extend(slab.items.into_iter().map(|mut item| {
            item.offset(-left, y);
            item
        }));
        y += slab.height;
    }
    let distance = position.distance.map(|d| i64::from(d) * 635);
    ctx.wraps |= ctx.flowing;
    let mut horizontal = position.horizontal;
    if horizontal.align.is_none() {
        horizontal.offset += crate::units::pt_to_emu(left);
    }
    Some(Floating {
        media: None,
        picture: false,
        width: table_width,
        height,
        horizontal,
        vertical: position.vertical,
        behind: false,
        content,
        wrap: TextWrap {
            kind: WrapKind::Square,
            side: WrapSide::Both,
            distance,
            polygon: Vec::new(),
        },
        local: None,
        id: ctx.next_float(),
    })
}

/// A header, footer or note body laid out from `y = 0`.
struct Story {
    items: Vec<Item>,
    height: f32,
    /// Floats with the top and height of the line that holds each.
    floats: Vec<(Floating, f32, f32)>,
}

impl Story {
    /// The story's items with its floats placed on `frame`, the story
    /// starting at `origin` in the frame's coordinates.
    fn placed(self, frame: &mut Frame, origin: (f32, f32)) -> Vec<Item> {
        let mut items: Vec<Item> = self
            .items
            .into_iter()
            .map(|mut item| {
                item.offset(origin.0, origin.1);
                item
            })
            .collect();
        for (float, y, height) in &self.floats {
            frame.line = (origin.1 + y, *height);
            items.extend(float.items(frame));
        }
        items
    }
}

/// Lays out a header, footer or note body as items stacked from `y = 0`,
/// its floating drawings and text boxes kept apart for placing.
fn layout_story(ctx: &mut Ctx<'_>, blocks: &[Block], width: f32) -> Story {
    let fresh = ctx.doc.numbering.counter();
    let saved = std::mem::replace(&mut ctx.counter, fresh);
    let (slabs, _) = layout_blocks(ctx, blocks, width);
    ctx.counter = saved;
    let mut story = Story {
        items: Vec::new(),
        height: 0.0,
        floats: Vec::new(),
    };
    let mut y = 0.0;
    for slab in slabs {
        y += slab.gap_before;
        story.items.extend(slab.items.into_iter().map(|mut item| {
            item.offset(0.0, y);
            item
        }));
        story
            .floats
            .extend(slab.floats.into_iter().map(|float| (float, y, slab.height)));
        y += slab.height;
    }
    story.height = y;
    story
}

fn merge_refs(base: &HeaderFooterRefs, over: &HeaderFooterRefs) -> HeaderFooterRefs {
    HeaderFooterRefs {
        default: over.default.clone().or_else(|| base.default.clone()),
        first: over.first.clone().or_else(|| base.first.clone()),
        even: over.even.clone().or_else(|| base.even.clone()),
    }
}

struct PageState {
    width: f32,
    height: f32,
    number: u32,
    number_format: Option<docboss_model::NumberFormat>,
    text_left: f32,
    text_width: f32,
    /// The top and bottom page margins as y coordinates.
    margin_top: f32,
    margin_bottom: f32,
    header_top: f32,
    footer_bottom: f32,
    header: Option<String>,
    footer: Option<String>,
    body_bottom: f32,
    body: Vec<Item>,
    back: Vec<Item>,
    front: Vec<Item>,
    notes: Vec<i64>,
    note_height: f32,
}

impl PageState {
    /// The page's geometry for placing floats, with the text area as the
    /// column.
    fn frame(&self) -> Frame {
        Frame {
            page: (self.width, self.height),
            margin_x: (self.text_left, self.text_width),
            margin_y: (
                self.margin_top,
                (self.margin_bottom - self.margin_top).max(0.0),
            ),
            column: (self.text_left, self.text_width),
            line: (self.margin_top, 0.0),
            odd: self.number % 2 == 1,
        }
    }
}

struct Cursor {
    frames: Vec<(f32, f32)>,
    column: usize,
    top: f32,
    y: f32,
    empty: bool,
}

struct Geometry {
    props: SectionProperties,
    headers: HeaderFooterRefs,
    footers: HeaderFooterRefs,
}

struct Paginator<'c, 'd> {
    ctx: &'c mut Ctx<'d>,
    pages: Vec<PageState>,
    cursor: Cursor,
    geometry: Option<Geometry>,
    section_first_page: bool,
    next_number: u32,
    notes: HashMap<i64, (Vec<Item>, f32)>,
    /// The areas floats keep text out of on the current page.
    exclusions: Vec<Exclusion>,
    /// Counts changes to `exclusions` and to the column lines are placed
    /// in, so that a line knows whether it was broken for the current ones.
    generation: u32,
    /// The first exclusion of the slab being placed, until it is committed.
    pending: Option<usize>,
    /// The slabs committed to the current page since it or its section
    /// started, kept while text wraps so the page can be laid out again.
    page_slabs: Vec<Slab>,
    /// Floats that reach above text already on the page, with their areas
    /// and rectangles: they stay where they are when the page is laid out
    /// again.
    sticky: Vec<(u32, Exclusion, Rect)>,
    /// How the page looked where laying it out again starts: the number of
    /// items behind and in front, the column and its top.
    restart: (usize, usize, usize, f32),
    restarts: u32,
}

fn frames_of(props: &SectionProperties, text_left: f32, text_width: f32) -> Vec<(f32, f32)> {
    let columns = &props.columns;
    if !columns.widths.is_empty() {
        let mut x = text_left;
        return columns
            .widths
            .iter()
            .map(|&(w, space)| {
                let frame = (x, twips_to_pt(w).max(1.0));
                x += twips_to_pt(w) + twips_to_pt(space);
                frame
            })
            .collect();
    }
    let count = columns.count.clamp(1, 45) as usize;
    let space = twips_to_pt(columns.space).max(0.0);
    let width = ((text_width - space * (count - 1) as f32) / count as f32).max(1.0);
    (0..count)
        .map(|i| (text_left + i as f32 * (width + space), width))
        .collect()
}

impl Paginator<'_, '_> {
    fn geometry(&self) -> &Geometry {
        self.geometry
            .as_ref()
            .expect("a section is active while paginating")
    }

    fn choose(&self, refs: &HeaderFooterRefs, number: u32) -> Option<String> {
        let g = self.geometry();
        if self.section_first_page && g.props.title_page {
            return refs.first.clone();
        }
        if self.ctx.doc.settings.even_and_odd_headers && number.is_multiple_of(2) {
            return refs.even.clone();
        }
        refs.default.clone()
    }

    fn story_height(&mut self, id: Option<&str>, width: f32) -> f32 {
        let Some(part) = id.and_then(|id| self.ctx.doc.header_footer(id)) else {
            return 0.0;
        };
        layout_story(self.ctx, &part.blocks, width).height
    }

    fn new_page(&mut self) {
        if self.pages.len() >= MAX_PAGES {
            return;
        }
        let g = self.geometry();
        let props = g.props.clone();
        let (headers, footers) = (g.headers.clone(), g.footers.clone());
        if self.section_first_page {
            if let Some(start) = props.page_number_start {
                self.next_number = start;
            }
        }
        let number = self.next_number;
        self.next_number += 1;
        let width = twips_to_pt(props.page_size.width).max(36.0);
        let height = twips_to_pt(props.page_size.height).max(36.0);
        let m = props.margins;
        let text_left = twips_to_pt(m.left + m.gutter);
        let text_width = (width - text_left - twips_to_pt(m.right)).max(36.0);
        let header = self.choose(&headers, number);
        let footer = self.choose(&footers, number);
        self.ctx.page_number = number;
        self.ctx.page_format = props.page_number_format.clone();
        let header_h = self.story_height(header.as_deref(), text_width);
        let footer_h = self.story_height(footer.as_deref(), text_width);
        let header_top = twips_to_pt(m.header);
        let footer_bottom = height - twips_to_pt(m.footer);
        let top = twips_to_pt(m.top.abs()).max(if header_h > 0.0 {
            header_top + header_h
        } else {
            0.0
        });
        let bottom = (height - twips_to_pt(m.bottom.abs())).min(if footer_h > 0.0 {
            footer_bottom - footer_h
        } else {
            height
        });
        let bottom = bottom.max(top + 12.0);
        let text = (
            text_left,
            twips_to_pt(m.top.abs()),
            width - twips_to_pt(m.right),
            height - twips_to_pt(m.bottom.abs()),
        );
        let (mut back, mut front) = (Vec::new(), Vec::new());
        if let Some(borders) = props
            .page_borders
            .filter(|b| shows_on(b.display, self.section_first_page))
        {
            let art = [
                borders.sides.top,
                borders.sides.left,
                borders.sides.bottom,
                borders.sides.right,
            ]
            .iter()
            .flatten()
            .any(|b| b.style == docboss_model::BorderStyle::Art);
            if art && self.section_first_page {
                self.ctx.diagnostics.push(Diagnostic::dropped(
                    "page borders",
                    "art page borders are not drawn",
                ));
            }
            let lines = page_border_lines(&borders, (width, height), text);
            match borders.behind_text {
                true => back = lines,
                false => front = lines,
            }
        }
        self.pages.push(PageState {
            width,
            height,
            number,
            number_format: props.page_number_format.clone(),
            text_left,
            text_width,
            margin_top: twips_to_pt(m.top.abs()),
            margin_bottom: height - twips_to_pt(m.bottom.abs()),
            header_top,
            footer_bottom,
            header,
            footer,
            body_bottom: bottom,
            body: Vec::new(),
            back,
            front,
            notes: Vec::new(),
            note_height: 0.0,
        });
        self.section_first_page = false;
        self.cursor = Cursor {
            frames: frames_of(&props, text_left, text_width),
            column: 0,
            top,
            y: top,
            empty: true,
        };
        self.exclusions.clear();
        self.pending = None;
        self.generation += 1;
        self.page_slabs.clear();
        self.sticky.clear();
        self.restarts = 0;
        self.mark_restart();
    }

    /// Where laying the page out again starts from: here.
    fn mark_restart(&mut self) {
        let (column, top) = (self.cursor.column, self.cursor.y);
        let Some(page) = self.pages.last() else {
            return;
        };
        self.restart = (page.back.len(), page.front.len(), column, top);
        self.page_slabs.clear();
    }

    /// Lays the page out again from its restart point with `exclusion`
    /// kept on it: the committed slabs go back in front of `slab`.
    fn restart_page(
        &mut self,
        (id, exclusion, rect): (u32, Exclusion, Rect),
        slab: Slab,
        queue: &mut VecDeque<Slab>,
    ) {
        self.restarts += 1;
        self.sticky.push((id, exclusion, rect));
        let (back, front, column, top) = self.restart;
        let Some(page) = self.pages.last_mut() else {
            return;
        };
        page.body.clear();
        page.back.truncate(back);
        page.front.truncate(front);
        page.notes.clear();
        page.note_height = 0.0;
        self.cursor.column = column;
        self.cursor.y = top;
        self.cursor.empty = true;
        self.exclusions = self.sticky.iter().map(|(_, e, _)| e.clone()).collect();
        self.pending = None;
        self.generation += 1;
        queue.push_front(slab);
        for committed in std::mem::take(&mut self.page_slabs).into_iter().rev() {
            queue.push_front(committed);
        }
    }

    fn page(&mut self) -> &mut PageState {
        if self.pages.is_empty() {
            self.new_page();
        }
        self.pages.last_mut().expect("a page exists after new_page")
    }

    fn next_frame(&mut self) {
        if self.cursor.column + 1 < self.cursor.frames.len() {
            self.cursor.column += 1;
            self.cursor.y = self.cursor.top;
            self.cursor.empty = true;
            self.generation += 1;
            return;
        }
        self.new_page();
    }

    /// The page geometry a slab placed at `y`, `height` tall, puts its
    /// floats against.
    fn float_frame(&mut self, y: f32, height: f32) -> Frame {
        let column = self
            .cursor
            .frames
            .get(self.cursor.column)
            .copied()
            .unwrap_or((0.0, 0.0));
        Frame {
            column,
            line: (y, height),
            ..self.page().frame()
        }
    }

    /// Drops the exclusions of a slab that was not placed where they were
    /// measured.
    fn drop_pending(&mut self) {
        let Some(from) = self.pending.take() else {
            return;
        };
        self.exclusions.truncate(from);
        self.generation += 1;
    }

    /// Adds the areas the slab's floats keep text out of, as they would
    /// lie with the slab placed next. A float that reaches above text
    /// already on the page lays the page out again around it; the slab
    /// then goes back to the queue and `None` is returned.
    fn register_floats(&mut self, slab: Slab, queue: &mut VecDeque<Slab>) -> Option<Slab> {
        let wrapping = |f: &Floating| f.wrap.wraps() && f.local.is_none();
        if !slab.floats.iter().any(wrapping) {
            return Some(slab);
        }
        let y = self.cursor.y + slab.gap_before;
        let frame = self.float_frame(y, slab.height);
        let from = self.exclusions.len();
        let span = match (self.cursor.frames.first(), self.cursor.frames.last()) {
            (Some(first), Some(last)) => (first.0, last.0 + last.1),
            _ => (0.0, 0.0),
        };
        let top = self.restart.3;
        for float in slab.floats.iter().filter(|f| wrapping(f)) {
            if self.sticky.iter().any(|(id, ..)| *id == float.id) {
                continue;
            }
            let rect = float.rect(&frame);
            let exclusion = Exclusion::new(rect, &float.wrap);
            let above = exclusion.top() < self.cursor.y - 0.5
                && exclusion.bottom() > top
                && band(
                    std::slice::from_ref(&exclusion),
                    top,
                    self.cursor.y,
                    span.0,
                    span.1,
                )
                .is_some();
            if above && !self.page_slabs.is_empty() && self.restarts < MAX_RESTARTS {
                self.exclusions.truncate(from);
                let id = float.id;
                self.restart_page((id, exclusion, rect), slab, queue);
                return None;
            }
            self.exclusions.push(exclusion);
        }
        self.pending = Some(from);
        self.generation += 1;
        Some(slab)
    }

    /// Breaks the rest of a paragraph again when the lines from `slab` on
    /// were broken for other exclusions than the current ones and either
    /// were shortened or now meet one.
    fn rewrap(&mut self, slab: Slab, queue: &mut VecDeque<Slab>) -> Slab {
        let Some(resume) = slab.resume.as_deref() else {
            return slab;
        };
        if resume.generation == self.generation {
            return slab;
        }
        let rest = resume.rest.min(queue.len());
        let top = self.cursor.y + slab.gap_before;
        let reach = slab.height
            + queue
                .iter()
                .take(rest)
                .map(|s| s.gap_before + s.height)
                .sum::<f32>();
        let (x, width) = self
            .cursor
            .frames
            .get(self.cursor.column)
            .copied()
            .unwrap_or((0.0, 0.0));
        let meets = band(&self.exclusions, top, top + reach, x, x + width).is_some();
        if !resume.narrow && !meets {
            let mut slab = slab;
            if let Some(resume) = slab.resume.as_mut() {
                resume.generation = self.generation;
            }
            return slab;
        }
        let exclusions = &self.exclusions;
        let room = |y: f32, height: f32| -> Option<Band> {
            let found = band(exclusions, top + y, top + y + height, x, x + width)?;
            Some(Band {
                spans: found.spans.iter().map(|&(a, b)| (a - x, b - x)).collect(),
                below: found.below - top,
            })
        };
        let mut fresh: VecDeque<Slab> = resume.rewrap(self.ctx, &room, self.generation).into();
        queue.drain(..rest);
        let Some(mut first) = fresh.pop_front() else {
            return slab;
        };
        first.gap_before = slab.gap_before;
        first.floats = slab.floats;
        first.break_before = slab.break_before;
        while let Some(line) = fresh.pop_back() {
            queue.push_front(line);
        }
        first
    }

    fn frame_bottom(&mut self) -> f32 {
        let page = self.page();
        page.body_bottom - page.note_height
    }

    fn frame_room(&mut self) -> f32 {
        let bottom = self.frame_bottom();
        bottom - self.cursor.top
    }

    fn note_block(&mut self, id: i64) -> (Vec<Item>, f32) {
        if let Some(found) = self.notes.get(&id) {
            return found.clone();
        }
        let width = self.page().text_width;
        let doc = self.ctx.doc;
        let Some(note) = doc.footnote(id) else {
            return (Vec::new(), 0.0);
        };
        self.ctx.current_note = Some(self.ctx.note_label(NoteKind::Footnote, id));
        let story = layout_story(self.ctx, &note.blocks, width);
        let height = story.height;
        let mut frame = Frame {
            page: (width, height),
            margin_x: (0.0, width),
            margin_y: (0.0, height),
            column: (0.0, width),
            line: (0.0, 0.0),
            odd: true,
        };
        let laid = (story.placed(&mut frame, (0.0, 0.0)), height);
        self.ctx.current_note = None;
        self.notes.insert(id, laid.clone());
        laid
    }

    fn notes_needed(&mut self, notes: &[i64]) -> f32 {
        let fresh: Vec<i64> = {
            let page = self.page();
            notes
                .iter()
                .copied()
                .filter(|id| !page.notes.contains(id))
                .collect()
        };
        if fresh.is_empty() {
            return 0.0;
        }
        let separator = if self.page().notes.is_empty() {
            SEPARATOR_HEIGHT
        } else {
            0.0
        };
        separator + fresh.iter().map(|&id| self.note_block(id).1).sum::<f32>()
    }

    fn group_height(slab: &Slab, queue: &VecDeque<Slab>) -> f32 {
        let mut total = slab.gap_before + slab.height;
        let mut linked = slab.keep_with_next;
        for next in queue.iter().take(64) {
            if !linked {
                break;
            }
            total += next.gap_before + next.height;
            linked = next.keep_with_next;
        }
        total
    }

    fn run(&mut self, slabs: Vec<Slab>) {
        let mut queue: VecDeque<Slab> = slabs.into();
        let mut in_group = false;
        while let Some(mut slab) = queue.pop_front() {
            self.drop_pending();
            if self.pages.len() >= MAX_PAGES {
                self.ctx.diagnostics.push(Diagnostic::dropped(
                    "layout",
                    "page limit reached; the rest was not laid out",
                ));
                return;
            }
            match slab.break_before {
                Some(Break::Page) if !self.cursor.empty || self.cursor.column > 0 => {
                    self.new_page()
                }
                Some(Break::Column) => self.next_frame(),
                _ => {}
            }
            if !in_group && slab.keep_with_next && !self.cursor.empty {
                let group = Self::group_height(&slab, &queue);
                let room = self.frame_bottom() - self.cursor.y;
                if group > room && group <= self.frame_room() {
                    self.next_frame();
                }
            }
            let Some(registered) = self.register_floats(slab, &mut queue) else {
                in_group = false;
                continue;
            };
            slab = self.rewrap(registered, &mut queue);
            in_group = slab.keep_with_next;
            self.place(slab, &mut queue);
        }
    }

    fn place(&mut self, slab: Slab, queue: &mut VecDeque<Slab>) {
        let notes = self.notes_needed(&slab.notes);
        let room = self.frame_bottom() - self.cursor.y;
        let need = slab.gap_before + slab.height + notes;
        if need <= room + 0.01 {
            self.commit(slab);
            return;
        }
        if let Some(row) = slab.row.as_ref() {
            let available = room - slab.gap_before - notes;
            if let Some((head, tail, height)) = row.split(available).filter(|_| available > 12.0) {
                let mut first = Slab::new(height, head.paint(height));
                first.gap_before = slab.gap_before;
                first.notes = slab.notes.clone();
                first.floats = head.floats();
                let tail_height = tail_height(&tail);
                let mut rest = Slab::new(tail_height, tail.paint(tail_height));
                rest.floats = tail.floats();
                rest.row = Some(Box::new(tail));
                rest.repeat = slab.repeat.clone();
                self.commit(first);
                queue.push_front(rest);
                return;
            }
        }
        if self.cursor.empty {
            self.commit(slab);
            return;
        }
        self.next_frame();
        if let Some(header) = slab.repeat.clone() {
            header.iter().cloned().for_each(|row| self.commit(row));
        }
        let mut slab = slab;
        if slab.row.is_some() {
            slab.gap_before = 0.0;
        }
        queue.push_front(slab);
    }

    fn commit(&mut self, slab: Slab) {
        if self.ctx.wraps {
            self.page_slabs.push(slab.clone());
        }
        let Slab {
            height,
            gap_before,
            items,
            floats,
            notes,
            ..
        } = slab;
        let x = self
            .cursor
            .frames
            .get(self.cursor.column)
            .map_or(0.0, |frame| frame.0);
        let y = self.cursor.y + gap_before;
        let fresh: Vec<i64> = {
            let page = self.page();
            notes
                .iter()
                .copied()
                .filter(|id| !page.notes.contains(id))
                .collect()
        };
        for id in fresh {
            let (_, h) = self.note_block(id);
            let page = self.page();
            if page.notes.is_empty() {
                page.note_height += SEPARATOR_HEIGHT;
            }
            page.notes.push(id);
            page.note_height += h;
        }
        self.pending = None;
        let frame = self.float_frame(y, height);
        let page = self.pages.last_mut().expect("commit runs with a page");
        page.body.extend(items.into_iter().map(|mut item| {
            item.offset(x, y);
            item
        }));
        for float in floats {
            let frame = match float.local {
                Some(local) => Frame {
                    column: (x + local.x, local.width),
                    line: (y + local.y, local.height),
                    ..frame
                },
                None => Frame { ..frame },
            };
            let items = match self.sticky.iter().find(|(id, ..)| *id == float.id) {
                Some((.., rect)) => float.items_at(*rect),
                None => float.items(&frame),
            };
            if float.behind {
                page.back.extend(items);
                continue;
            }
            page.front.extend(items);
        }
        self.cursor.y = y + height;
        self.cursor.empty = false;
    }

    /// ECMA-376 Part 1 §17.6 (`w:type`): starts a section as its break type
    /// asks.
    fn start_section(&mut self, props: &SectionProperties, first: bool) {
        let previous = self.geometry.take();
        let headers = previous.as_ref().map_or(props.headers.clone(), |g| {
            merge_refs(&g.headers, &props.headers)
        });
        let footers = previous.as_ref().map_or(props.footers.clone(), |g| {
            merge_refs(&g.footers, &props.footers)
        });
        self.geometry = Some(Geometry {
            props: props.clone(),
            headers,
            footers,
        });
        self.section_first_page = true;
        if first || self.pages.is_empty() {
            self.new_page();
            return;
        }
        match props.start {
            SectionBreak::Continuous => {
                let (left, width) = {
                    let page = self.page();
                    (page.text_left, page.text_width)
                };
                let y = self.cursor.y;
                self.cursor = Cursor {
                    frames: frames_of(props, left, width),
                    column: 0,
                    top: y,
                    y,
                    empty: true,
                };
                self.section_first_page = false;
                self.generation += 1;
                self.mark_restart();
            }
            SectionBreak::NextColumn => self.next_frame(),
            SectionBreak::NextPage => self.new_page(),
            SectionBreak::EvenPage | SectionBreak::OddPage => {
                self.new_page();
                let odd = self.next_number.is_multiple_of(2);
                let wants_odd = props.start == SectionBreak::OddPage;
                if odd != wants_odd {
                    self.section_first_page = true;
                    self.new_page();
                }
            }
        }
    }
}

fn tail_height(plan: &RowPlan) -> f32 {
    plan.split(f32::MAX)
        .map_or_else(|| plan_height(plan), |(_, _, h)| h)
}

fn plan_height(plan: &RowPlan) -> f32 {
    plan.natural()
}

fn roman_label(n: u32) -> String {
    docboss_model::number_label(&NumberFormat::LowerRoman, n)
}

/// Lays out the whole document into pages.
pub(crate) fn run(
    document: &Document,
    fonts: &Arc<FontDatabase>,
    options: &LayoutOptions,
) -> (Vec<Page>, Vec<Diagnostic>, Vec<docboss_model::Media>) {
    let mut refs = Vec::new();
    document
        .sections
        .iter()
        .for_each(|s| collect_notes(&s.blocks, &mut refs));
    let mut labels = HashMap::new();
    let (mut footnotes, mut endnotes) = (0u32, 0u32);
    for (kind, id) in &refs {
        let label = match kind {
            NoteKind::Footnote => {
                footnotes += 1;
                footnotes.to_string()
            }
            NoteKind::Endnote => {
                endnotes += 1;
                roman_label(endnotes)
            }
        };
        labels.insert((*kind == NoteKind::Footnote, *id), label);
    }
    let mut ctx = Ctx {
        doc: document,
        shaper: Shaper::new(
            fonts,
            &document.fonts,
            &options.default_font,
            options.kerning,
        ),
        counter: document.numbering.counter(),
        page_number: 1,
        page_format: None,
        total_pages: None,
        current_note: None,
        depth: 0,
        note_labels: labels,
        table_style: None,
        background: None,
        border_group: (false, false),
        wraps: false,
        flowing: false,
        body_height: f32::MAX,
        floats: 0,
        in_frame: false,
        diagnostics: Vec::new(),
    };
    let mut paginator = Paginator {
        ctx: &mut ctx,
        pages: Vec::new(),
        cursor: Cursor {
            frames: vec![(0.0, 0.0)],
            column: 0,
            top: 0.0,
            y: 0.0,
            empty: true,
        },
        geometry: None,
        section_first_page: true,
        next_number: 1,
        notes: HashMap::new(),
        exclusions: Vec::new(),
        generation: 0,
        pending: None,
        page_slabs: Vec::new(),
        sticky: Vec::new(),
        restart: (0, 0, 0, 0.0),
        restarts: 0,
    };
    let default_section = docboss_model::Section::default();
    let sections: Vec<&docboss_model::Section> = if document.sections.is_empty() {
        vec![&default_section]
    } else {
        document.sections.iter().collect()
    };
    for (index, section) in sections.iter().enumerate() {
        paginator.start_section(&section.properties, index == 0);
        let width = paginator.cursor.frames.first().map_or(1.0, |f| f.1);
        paginator.ctx.body_height = paginator
            .pages
            .last()
            .map_or(f32::MAX, |page| page.margin_bottom - page.margin_top);
        paginator.ctx.flowing = true;
        let (slabs, _) = layout_blocks(paginator.ctx, &section.blocks, width);
        paginator.ctx.flowing = false;
        paginator.run(slabs);
    }
    let endnote_refs: Vec<i64> = refs
        .iter()
        .filter(|(k, _)| *k == NoteKind::Endnote)
        .map(|(_, id)| *id)
        .collect();
    if !endnote_refs.is_empty() {
        let width = paginator.cursor.frames.first().map_or(1.0, |f| f.1);
        let mut slabs = vec![Slab::new(SEPARATOR_HEIGHT, vec![separator_line(width)])];
        for id in endnote_refs {
            let Some(note) = document.endnote(id) else {
                continue;
            };
            paginator.ctx.current_note = Some(paginator.ctx.note_label(NoteKind::Endnote, id));
            let (note_slabs, _) = layout_blocks(paginator.ctx, &note.blocks, width);
            paginator.ctx.current_note = None;
            slabs.extend(note_slabs);
        }
        paginator.run(slabs);
    }
    let states = std::mem::take(&mut paginator.pages);
    let notes = std::mem::take(&mut paginator.notes);
    drop(paginator);
    let total = states.len() as u32;
    ctx.total_pages = Some(total);
    let mut pages: Vec<Page> = states
        .into_iter()
        .map(|state| finish_page(&mut ctx, state, &notes))
        .collect();
    let media = crate::metafile::expand(&mut ctx, &mut pages);
    (pages, ctx.diagnostics, media)
}

fn shows_on(display: PageBorderDisplay, first: bool) -> bool {
    match display {
        PageBorderDisplay::AllPages => true,
        PageBorderDisplay::FirstPage => first,
        PageBorderDisplay::NotFirstPage => !first,
    }
}

/// ECMA-376 Part 1 §17.6.10: the lines of a page's borders on a page of
/// `size` whose text area spans `text` (left, top, right, bottom). From
/// the page, a border's outer edge lies `space` points inside the page
/// edge; from the text, its inner edge lies `space` points outside the
/// text area, as LibreOffice places them. Each line runs between the outer
/// edges of its neighbours so that the corners close.
///
/// A border with `w:shadow` (§17.6.15, §17.6.2) casts a black shadow as
/// wide as the border to the right and below, as LibreOffice draws it:
/// from the page, the right and bottom lines move in by that width so the
/// shadow ends where they did. Art borders (§17.18.2) are left out, as
/// LibreOffice leaves them out.
fn page_border_lines(
    borders: &PageBorders,
    (width, height): (f32, f32),
    (left, top, right, bottom): (f32, f32, f32, f32),
) -> Vec<Item> {
    let sides = borders.sides;
    let half = |side: Option<docboss_model::Border>| side.map_or(0.0, |b| border_width(&b) / 2.0);
    let space = |side: Option<docboss_model::Border>| side.map_or(0.0, |b| b.space as f32);
    let (ht, hl, hb, hr) = (
        half(sides.top),
        half(sides.left),
        half(sides.bottom),
        half(sides.right),
    );
    let (st, sl, sb, sr) = (
        space(sides.top),
        space(sides.left),
        space(sides.bottom),
        space(sides.right),
    );
    let shadow = [sides.right, sides.bottom, sides.top, sides.left]
        .into_iter()
        .flatten()
        .find(|b| {
            b.shadow
                && b.style != docboss_model::BorderStyle::None
                && b.style != docboss_model::BorderStyle::Art
        })
        .map(|b| border_width(&b));
    let inset = match borders.offset_from {
        PageBorderOffset::Page => shadow.unwrap_or(0.0),
        PageBorderOffset::Text => 0.0,
    };
    let (y0, x0, y1, x1) = match borders.offset_from {
        PageBorderOffset::Page => (
            st + ht,
            sl + hl,
            height - sb - hb - inset,
            width - sr - hr - inset,
        ),
        PageBorderOffset::Text => (
            top - st - ht,
            left - sl - hl,
            bottom + sb + hb,
            right + sr + hr,
        ),
    };
    let mut items: Vec<Item> = Vec::new();
    if let Some(w) = shadow {
        let (outer_right, outer_bottom) = (x1 + hr, y1 + hb);
        let (outer_left, outer_top) = (x0 - hl, y0 - ht);
        items.push(Item::Rect {
            rect: Rect::new(outer_right, outer_top + w, w, outer_bottom - outer_top),
            color: Color::BLACK,
        });
        items.push(Item::Rect {
            rect: Rect::new(outer_left + w, outer_bottom, outer_right - outer_left, w),
            color: Color::BLACK,
        });
    }
    items.extend(
        [
            (sides.top, (x0 - hl, y0), (x1 + hr, y0)),
            (sides.right, (x1, y0 - ht), (x1, y1 + hb)),
            (sides.bottom, (x0 - hl, y1), (x1 + hr, y1)),
            (sides.left, (x0, y0 - ht), (x0, y1 + hb)),
        ]
        .into_iter()
        .filter_map(|(side, from, to)| border_line(&side?, from, to)),
    );
    items
}

fn separator_line(width: f32) -> Item {
    Item::Line {
        from: (0.0, SEPARATOR_HEIGHT / 2.0),
        to: (width.min(144.0), SEPARATOR_HEIGHT / 2.0),
        width: 0.5,
        color: Color::BLACK,
        style: LineStyle::Solid,
        cap: LineCap::Flat,
    }
}

/// ECMA-376 Part 1 §17.10 and §17.11: paints the header, footer and
/// footnotes of a finished page, now that the page count is known.
fn finish_page(
    ctx: &mut Ctx<'_>,
    mut state: PageState,
    notes: &HashMap<i64, (Vec<Item>, f32)>,
) -> Page {
    ctx.page_number = state.number;
    ctx.page_format = state.number_format.clone();
    let mut items = std::mem::take(&mut state.back);
    let doc = ctx.doc;
    let mut frame = state.frame();
    if let Some(part) = state.header.as_deref().and_then(|id| doc.header_footer(id)) {
        let header = layout_story(ctx, &part.blocks, state.text_width);
        items.extend(header.placed(&mut frame, (state.text_left, state.header_top)));
    }
    if let Some(part) = state.footer.as_deref().and_then(|id| doc.header_footer(id)) {
        let footer = layout_story(ctx, &part.blocks, state.text_width);
        let top = state.footer_bottom - footer.height;
        items.extend(footer.placed(&mut frame, (state.text_left, top)));
    }
    items.extend(state.body);
    if !state.notes.is_empty() {
        let mut y = state.body_bottom - state.note_height;
        let mut separator = separator_line(state.text_width);
        separator.offset(state.text_left, y);
        items.push(separator);
        y += SEPARATOR_HEIGHT;
        for id in &state.notes {
            let Some((note, height)) = notes.get(id) else {
                continue;
            };
            items.extend(note.iter().cloned().map(|mut item| {
                item.offset(state.text_left, y);
                item
            }));
            y += height;
        }
    }
    items.extend(state.front);
    Page {
        width: state.width,
        height: state.height,
        number: state.number,
        items,
    }
}

//! Pagination: stacking laid-out slabs into columns and pages, with keep
//! rules, page and column breaks, sections, headers and footers, footnotes
//! and endnotes.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use docboss_font::FontDatabase;
use docboss_model::{
    Block, Break, Color, Diagnostic, Document, HeaderFooterRefs, Inline, MediaId, NoteKind,
    NumberFormat, NumberingCounter, RunContent, SectionBreak, SectionProperties,
};

use crate::paragraph::layout_paragraph;
use crate::shape::Shaper;
use crate::table::{layout_table, RowPlan};
use crate::units::twips_to_pt;
use crate::{Item, LayoutOptions, LineStyle, Page, Rect};

const MAX_PAGES: usize = 100_000;
const SEPARATOR_HEIGHT: f32 = 12.0;
const MAX_NESTING: usize = 24;

/// A drawing positioned independently of the text.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Floating {
    pub media: Option<MediaId>,
    pub rect: Rect,
    pub relative_to_page: bool,
    pub behind: bool,
}

/// A vertical unit of content: one line of a paragraph or one table row.
/// Item coordinates are relative to the slab's top-left corner.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Slab {
    pub height: f32,
    pub gap_before: f32,
    pub items: Vec<Item>,
    pub floats: Vec<Floating>,
    pub keep_with_next: bool,
    pub break_before: Option<Break>,
    pub break_after: Option<Break>,
    pub notes: Vec<i64>,
    pub row: Option<Box<RowPlan>>,
    pub repeat: Option<Arc<[Slab]>>,
}

impl Slab {
    pub(crate) fn new(height: f32, items: Vec<Item>) -> Slab {
        Slab {
            height,
            gap_before: 0.0,
            items,
            floats: Vec::new(),
            keep_with_next: false,
            break_before: None,
            break_after: None,
            notes: Vec::new(),
            row: None,
            repeat: None,
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
    pub total_pages: Option<u32>,
    pub current_note: Option<String>,
    /// The style of the table whose cell is being laid out.
    pub table_style: Option<String>,
    depth: usize,
    note_labels: HashMap<(bool, i64), String>,
    pub diagnostics: Vec<Diagnostic>,
}

impl Ctx<'_> {
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
    for block in blocks {
        match block {
            Block::Paragraph(p) => {
                let mut laid = layout_paragraph(ctx, p, width);
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
                }
                prev_after = laid.after;
                prev_contextual = laid.contextual;
                prev_style = Some(p.style_id.clone());
                out.extend(laid.slabs);
            }
            Block::Table(t) => {
                let mut slabs = layout_table(ctx, t, width);
                if let Some(first) = slabs.first_mut() {
                    first.gap_before = prev_after;
                }
                prev_after = 0.0;
                prev_contextual = false;
                prev_style = None;
                out.extend(slabs);
            }
        }
    }
    (out, prev_after)
}

/// Lays out a header, footer or note body as items stacked from `y = 0`.
fn layout_story(ctx: &mut Ctx<'_>, blocks: &[Block], width: f32) -> (Vec<Item>, f32) {
    let fresh = ctx.doc.numbering.counter();
    let saved = std::mem::replace(&mut ctx.counter, fresh);
    let (slabs, _) = layout_blocks(ctx, blocks, width);
    ctx.counter = saved;
    let mut items = Vec::new();
    let mut y = 0.0;
    for slab in slabs {
        y += slab.gap_before;
        items.extend(slab.items.into_iter().map(|mut item| {
            item.offset(0.0, y);
            item
        }));
        y += slab.height;
    }
    (items, y)
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
    text_left: f32,
    text_width: f32,
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
        layout_story(self.ctx, &part.blocks, width).1
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
        self.pages.push(PageState {
            width,
            height,
            number,
            text_left,
            text_width,
            header_top,
            footer_bottom,
            header,
            footer,
            body_bottom: bottom,
            body: Vec::new(),
            back: Vec::new(),
            front: Vec::new(),
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
            return;
        }
        self.new_page();
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
        let laid = layout_story(self.ctx, &note.blocks, width);
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
        while let Some(slab) = queue.pop_front() {
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
                let tail_height = tail_height(&tail);
                let mut rest = Slab::new(tail_height, tail.paint(tail_height));
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
        let Slab {
            height,
            gap_before,
            items,
            floats,
            notes,
            ..
        } = slab;
        let (x, _) = self
            .cursor
            .frames
            .get(self.cursor.column)
            .copied()
            .unwrap_or((0.0, 0.0));
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
        let page = self.pages.last_mut().expect("commit runs with a page");
        page.body.extend(items.into_iter().map(|mut item| {
            item.offset(x, y);
            item
        }));
        for float in floats {
            let rect = if float.relative_to_page {
                float.rect
            } else {
                float.rect.offset(x, y)
            };
            let item = Item::Image {
                media: float.media,
                rect,
            };
            if float.behind {
                page.back.push(item);
                continue;
            }
            page.front.push(item);
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
) -> (Vec<Page>, Vec<Diagnostic>) {
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
        total_pages: None,
        current_note: None,
        depth: 0,
        note_labels: labels,
        table_style: None,
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
        let (slabs, _) = layout_blocks(paginator.ctx, &section.blocks, width);
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
    let pages = states
        .into_iter()
        .map(|state| finish_page(&mut ctx, state, &notes))
        .collect();
    (pages, ctx.diagnostics)
}

fn separator_line(width: f32) -> Item {
    Item::Line {
        from: (0.0, SEPARATOR_HEIGHT / 2.0),
        to: (width.min(144.0), SEPARATOR_HEIGHT / 2.0),
        width: 0.5,
        color: Color::BLACK,
        style: LineStyle::Solid,
    }
}

/// ECMA-376 Part 1 §17.10 and §17.11: paints the header, footer and
/// footnotes of a finished page, now that the page count is known.
fn finish_page(
    ctx: &mut Ctx<'_>,
    state: PageState,
    notes: &HashMap<i64, (Vec<Item>, f32)>,
) -> Page {
    ctx.page_number = state.number;
    let mut items = state.back;
    let doc = ctx.doc;
    if let Some(part) = state.header.as_deref().and_then(|id| doc.header_footer(id)) {
        let (header, _) = layout_story(ctx, &part.blocks, state.text_width);
        items.extend(header.into_iter().map(|mut item| {
            item.offset(state.text_left, state.header_top);
            item
        }));
    }
    if let Some(part) = state.footer.as_deref().and_then(|id| doc.header_footer(id)) {
        let (footer, height) = layout_story(ctx, &part.blocks, state.text_width);
        items.extend(footer.into_iter().map(|mut item| {
            item.offset(state.text_left, state.footer_bottom - height);
            item
        }));
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

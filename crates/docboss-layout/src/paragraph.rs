//! Paragraph layout: flattening inline content into characters and
//! objects, breaking it into lines, placing tabs, list labels and
//! decorations, and producing one slab per line.

use docboss_model::{
    Break, DashPattern, Drawing, DrawingPlacement, Inline, Justification, LineCap, LineRule,
    NoteKind, Paragraph, RevisionKind, RunContent, TabAlignment, TabLeader, TabStop, Underline,
};

use crate::breaks;
use crate::flow::{Ctx, Floating, Slab};
use crate::shape::{Glyph, RunStyle};
use crate::textbox::layout_drawing;
use crate::units::{emu_to_pt, twips_to_pt};
use crate::{GlyphRun, Item, LineStyle, PositionedGlyph, Rect};

#[derive(Debug, Clone)]
enum Elem {
    Char(char, usize),
    Tab(usize),
    Break(Break, usize),
    Object(Box<Drawing>, usize),
    Float(Box<Drawing>),
    Note(i64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Word,
    Space,
    Tab,
    Break(Break),
    Object,
}

#[derive(Debug, Clone)]
struct AtomGlyph {
    glyph: Glyph,
    style: usize,
    ch: char,
    x: f32,
}

#[derive(Debug, Clone)]
struct Atom {
    kind: Kind,
    glyphs: Vec<AtomGlyph>,
    width: f32,
    ascent: f32,
    descent: f32,
    break_after: bool,
    style: usize,
    object: Option<Box<Drawing>>,
    notes: Vec<i64>,
}

impl Atom {
    fn new(kind: Kind, style: usize) -> Atom {
        Atom {
            kind,
            glyphs: Vec::new(),
            width: 0.0,
            ascent: 0.0,
            descent: 0.0,
            break_after: false,
            style,
            object: None,
            notes: Vec::new(),
        }
    }

    fn is_space(&self) -> bool {
        self.kind == Kind::Space
    }
}

/// A paragraph laid out at a width.
pub(crate) struct Laid {
    pub slabs: Vec<Slab>,
    pub before: f32,
    pub after: f32,
    pub contextual: bool,
}

#[derive(Debug, Clone, Copy)]
struct Placed {
    atom: usize,
    x: f32,
    width: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LineEnd {
    Wrap,
    Mandatory(Break),
    Last,
}

struct Line {
    placed: Vec<Placed>,
    end: LineEnd,
}

struct Geometry {
    width: f32,
    left: f32,
    right: f32,
    first_offset: f32,
    tabs: Vec<TabStop>,
    default_tab: f32,
}

impl Geometry {
    fn max_x(&self) -> f32 {
        (self.width - self.right).max(self.left + 1.0)
    }

    /// ECMA-376 Part 1 §17.3.1 (`w:tabs`): the first custom stop right of
    /// `x`, else the hanging indent, else the next default stop
    /// (`w:defaultTabStop`, ECMA-376 Part 1 §17.15.1).
    fn next_stop(&self, x: f32) -> (f32, TabAlignment, TabLeader) {
        let hanging =
            (self.first_offset < 0.0).then_some((self.left, TabAlignment::Left, TabLeader::None));
        let custom = self
            .tabs
            .iter()
            .filter(|t| !matches!(t.alignment, TabAlignment::Clear | TabAlignment::Bar))
            .map(|t| (twips_to_pt(t.position), t.alignment, t.leader))
            .chain(hanging)
            .filter(|(position, ..)| *position > x + 0.01)
            .min_by(|a, b| a.0.total_cmp(&b.0));
        if let Some(stop) = custom {
            return stop;
        }
        let step = self.default_tab.max(1.0);
        let next = ((x + 0.01) / step).floor() * step + step;
        (next, TabAlignment::Left, TabLeader::None)
    }
}

fn field_kind(instruction: &str) -> String {
    instruction
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_ascii_uppercase()
}

/// The number format a field's `\*` switch names: `ROMAN`, `roman`,
/// `ALPHABETIC`, `alphabetic`, `Arabic` or `Ordinal`.
fn field_format(instruction: &str) -> Option<docboss_model::NumberFormat> {
    use docboss_model::NumberFormat;
    let mut words = instruction.split_whitespace();
    while let Some(word) = words.next() {
        if word != "\\*" {
            continue;
        }
        let format = match words.next()? {
            "ROMAN" => NumberFormat::UpperRoman,
            "roman" => NumberFormat::LowerRoman,
            "ALPHABETIC" => NumberFormat::UpperLetter,
            "alphabetic" => NumberFormat::LowerLetter,
            "Arabic" => NumberFormat::Decimal,
            "Ordinal" | "ordinal" => NumberFormat::Ordinal,
            _ => continue,
        };
        return Some(format);
    }
    None
}

struct Flattener<'c, 'd> {
    ctx: &'c mut Ctx<'d>,
    paragraph_style: Option<String>,
    background: Option<docboss_model::Color>,
    styles: Vec<RunStyle>,
    elems: Vec<Elem>,
}

impl Flattener<'_, '_> {
    fn style_index(&mut self, style: RunStyle) -> usize {
        if let Some(i) = self.styles.iter().rposition(|s| *s == style) {
            return i;
        }
        self.styles.push(style);
        self.styles.len() - 1
    }

    fn resolve(&mut self, props: &docboss_model::RunProperties) -> RunStyle {
        let resolved = self.ctx.doc.styles.resolve_run_in(
            self.paragraph_style.as_deref(),
            props,
            self.ctx.table_style.as_deref(),
        );
        RunStyle::from_properties(&resolved).with_background(&resolved, self.background)
    }

    fn push_text(&mut self, text: &str, style: usize) {
        self.elems
            .extend(text.chars().map(|c| Elem::Char(c, style)));
    }

    fn inlines(&mut self, inlines: &[Inline]) {
        for inline in inlines {
            match inline {
                Inline::Run(run) => self.run(run),
                Inline::Hyperlink(link) => self.inlines(&link.inlines),
                Inline::Revision(revision) if revision.kind == RevisionKind::Insertion => {
                    self.inlines(&revision.inlines)
                }
                Inline::Field(field) => self.field(field),
                _ => {}
            }
        }
    }

    /// ECMA-376 Part 1 §17.16: `PAGE`, `NUMPAGES` and `SECTIONPAGES` are
    /// evaluated at layout time; every other field shows its cached result.
    /// `PAGE` shows the section's page number format (§17.6.12) unless a
    /// `\*` format switch names another (§17.16.4.3).
    fn field(&mut self, field: &docboss_model::Field) {
        let kind = field_kind(&field.instruction);
        let value = match kind.as_str() {
            "PAGE" => Some(self.ctx.page_number),
            "NUMPAGES" | "SECTIONPAGES" => self.ctx.total_pages,
            _ => None,
        };
        let format = field_format(&field.instruction).or_else(|| {
            (kind == "PAGE")
                .then(|| self.ctx.page_format.clone())
                .flatten()
        });
        let Some(value) = value else {
            self.inlines(&field.result);
            return;
        };
        let props = field.result.iter().find_map(|inline| match inline {
            Inline::Run(run) => Some(run.properties.clone()),
            _ => None,
        });
        let style = self.resolve(&props.unwrap_or_default());
        if style.hidden {
            return;
        }
        let index = self.style_index(style);
        let shown = match format {
            Some(format) => docboss_model::number_label(&format, value),
            None => value.to_string(),
        };
        self.push_text(&shown, index);
    }

    fn note_style(&mut self, props: &docboss_model::RunProperties) -> usize {
        let mut style = self.resolve(props);
        if style.rise == 0.0 && style.size == style.base_size {
            style.size = style.base_size * 2.0 / 3.0;
            style.rise = style.base_size / 3.0;
        }
        self.style_index(style)
    }

    fn run(&mut self, run: &docboss_model::Run) {
        let style = self.resolve(&run.properties);
        if style.hidden {
            return;
        }
        let index = self.style_index(style.clone());
        for content in &run.content {
            match content {
                RunContent::Text(text) => self.push_text(text, index),
                RunContent::Tab => self.elems.push(Elem::Tab(index)),
                RunContent::Break(kind) => self.elems.push(Elem::Break(*kind, index)),
                RunContent::CarriageReturn => self.elems.push(Elem::Break(Break::Line, index)),
                RunContent::NoBreakHyphen => self.elems.push(Elem::Char('\u{2011}', index)),
                RunContent::SoftHyphen => self.elems.push(Elem::Char('\u{00AD}', index)),
                RunContent::Symbol { font, char } => {
                    let mut symbol = style.clone();
                    if let Some(font) = font {
                        symbol.ascii = Some(font.clone());
                        symbol.high_ansi = Some(font.clone());
                    }
                    let symbol_index = self.style_index(symbol);
                    let c = char::from_u32(*char).unwrap_or('\u{FFFD}');
                    self.elems.push(Elem::Char(c, symbol_index));
                }
                RunContent::FootnoteReference(id) => {
                    self.note_reference(NoteKind::Footnote, *id, &run.properties)
                }
                RunContent::EndnoteReference(id) => {
                    self.note_reference(NoteKind::Endnote, *id, &run.properties)
                }
                RunContent::NoteNumber => {
                    let number = self.ctx.current_note.clone().unwrap_or_default();
                    let note_index = self.note_style(&run.properties);
                    self.push_text(&number, note_index);
                }
                RunContent::Drawing(drawing) => match drawing.placement {
                    DrawingPlacement::Inline => {
                        self.elems.push(Elem::Object(drawing.clone(), index))
                    }
                    DrawingPlacement::Anchored { .. } => {
                        self.elems.push(Elem::Float(drawing.clone()))
                    }
                },
                RunContent::CommentReference(_) => {}
            }
        }
    }

    fn note_reference(&mut self, kind: NoteKind, id: i64, props: &docboss_model::RunProperties) {
        let label = self.ctx.note_label(kind, id);
        let index = self.note_style(props);
        self.push_text(&label, index);
        if kind == NoteKind::Footnote {
            self.elems.push(Elem::Note(id));
        }
    }
}

/// ECMA-376 Part 1 §17.3.1: lays out one paragraph at `width` points. Under
/// auto line spacing (§17.3.1.33) the multiple scales the text of a line;
/// an inline picture keeps its own height on the text baseline.
pub(crate) fn layout_paragraph(ctx: &mut Ctx<'_>, paragraph: &Paragraph, width: f32) -> Laid {
    let doc = ctx.doc;
    let table_style = ctx.table_style.clone();
    let props = doc
        .styles
        .resolve_paragraph_in(paragraph, &doc.numbering, table_style.as_deref());
    let paragraph_style = paragraph.style_id.clone();
    let mark_resolved = doc.styles.resolve_run_in(
        paragraph_style.as_deref(),
        &paragraph.mark,
        table_style.as_deref(),
    );
    let background = props.shading.and_then(|s| s.fill).or(ctx.background);
    let mark_style =
        RunStyle::from_properties(&mark_resolved).with_background(&mark_resolved, background);
    let mut flat = Flattener {
        ctx,
        paragraph_style,
        background,
        styles: vec![mark_style],
        elems: Vec::new(),
    };

    let list = props.numbering.filter(|n| n.num_id != 0);
    if let Some(reference) = list {
        let label = flat.ctx.counter.next(reference);
        let level_run = doc
            .numbering
            .level(reference)
            .map(|l| l.run.clone())
            .unwrap_or_default();
        if let Some(label) = label.filter(|l| !l.is_empty()) {
            let mut label_props = mark_resolved.clone();
            label_props.apply(&level_run);
            let label_style =
                RunStyle::from_properties(&label_props).with_background(&label_props, background);
            let index = flat.style_index(label_style);
            flat.push_text(&label, index);
            flat.elems.push(Elem::Tab(index));
        }
    }
    flat.inlines(&paragraph.inlines);
    let Flattener {
        ctx, styles, elems, ..
    } = flat;

    let geometry = Geometry {
        width,
        left: twips_to_pt(props.indentation.left.unwrap_or(0)),
        right: twips_to_pt(props.indentation.right.unwrap_or(0)),
        first_offset: twips_to_pt(props.first_line_offset()),
        tabs: props.tabs.clone(),
        default_tab: twips_to_pt(doc.settings.default_tab_stop),
    };
    let (mut atoms, floats, notes) = build_atoms(ctx, &styles, &elems);
    let lines = break_lines(ctx, &styles, &mut atoms, &geometry);

    let spacing = props.spacing;
    let line_value = spacing.line.unwrap_or(240);
    let rule = spacing.line_rule.unwrap_or(LineRule::Auto);
    let mark_font = ctx.shaper.primary(&styles[0]);
    let mark_metrics = ctx.shaper.metrics(mark_font, styles[0].base_size);
    let justification = props.justification.unwrap_or(Justification::Left);
    let shading = props.shading.and_then(|s| s.fill);
    let borders = props.borders;
    let count = lines.len();
    let widow_control = props.widow_control.unwrap_or(false);
    let keep_lines = props.keep_lines.unwrap_or(false);
    let keep_next = props.keep_next.unwrap_or(false);

    let mut slabs = Vec::with_capacity(count);
    let mut pending_break: Option<Break> = None;
    for (index, line) in lines.iter().enumerate() {
        let extent = |objects: bool| {
            line.placed
                .iter()
                .map(|p| &atoms[p.atom])
                .filter(|a| a.kind != Kind::Tab && (a.kind == Kind::Object) == objects)
                .fold((0.0f32, 0.0f32), |(a, d), atom| {
                    (a.max(atom.ascent), d.max(atom.descent))
                })
        };
        let (mut ascent, mut descent) = extent(false);
        let (object_height, _) = extent(true);
        if ascent + descent <= 0.0 {
            ascent = mark_metrics.ascent;
            descent = mark_metrics.descent;
        }
        let natural = ascent.max(object_height) + descent;
        let (height, baseline) = match rule {
            LineRule::Auto => {
                let text = (ascent + descent) * line_value.max(1) as f32 / 240.0;
                let h = text.max(object_height + descent);
                (h, h - descent)
            }
            LineRule::Exact => {
                let h = twips_to_pt(line_value).max(0.1);
                (h, h - descent)
            }
            LineRule::AtLeast => {
                let h = natural.max(twips_to_pt(line_value));
                (h, h - descent)
            }
        };
        let mut items = Vec::new();
        if let Some(fill) = shading {
            let x = geometry.left;
            items.push(Item::Rect {
                rect: Rect::new(x, 0.0, geometry.max_x() - x, height),
                color: fill,
            });
        }
        let is_last = index + 1 == count;
        emit_line(
            ctx,
            &styles,
            &atoms,
            line,
            &geometry,
            justification,
            baseline,
            is_last,
            &mut items,
        );
        if let Some(borders) = borders {
            let (joins_previous, joins_next) = ctx.border_group;
            paragraph_borders(
                &borders,
                &geometry,
                height,
                index == 0 && !joins_previous,
                is_last && !joins_next,
                is_last && joins_next,
                &mut items,
            );
        }
        let mut slab = Slab::new(height, items);
        slab.break_before = pending_break.take().filter(|b| *b != Break::Line);
        if let LineEnd::Mandatory(kind) = line.end {
            pending_break = Some(kind);
        }
        slab.notes = line
            .placed
            .iter()
            .flat_map(|p| atoms[p.atom].notes.iter().copied())
            .collect();
        let first_two = widow_control && count >= 2 && index == 0;
        let last_two = widow_control && count >= 2 && index + 2 == count;
        slab.keep_with_next =
            (keep_lines && !is_last) || first_two || last_two || (keep_next && is_last);
        slabs.push(slab);
    }
    if props.page_break_before.unwrap_or(false) {
        if let Some(first) = slabs.first_mut() {
            first.break_before = Some(Break::Page);
        }
    }
    if let Some(first) = slabs.first_mut() {
        first.floats = floats;
        first.notes.extend(notes);
    }
    if let (Some(last), Some(kind)) = (slabs.last_mut(), pending_break) {
        last.break_after = Some(kind).filter(|k| *k != Break::Line);
    }
    Laid {
        slabs,
        before: twips_to_pt(spacing.before.unwrap_or(0)),
        after: twips_to_pt(spacing.after.unwrap_or(0)),
        contextual: props.contextual_spacing.unwrap_or(false),
    }
}

/// ECMA-376 Part 1 §17.3.1.24: paragraphs with identical borders form one
/// group, with the top border above its first line, the bottom border
/// below its last and the between border (§17.3.1.5) separating its
/// paragraphs.
fn paragraph_borders(
    borders: &docboss_model::Borders,
    g: &Geometry,
    height: f32,
    first: bool,
    last: bool,
    between: bool,
    items: &mut Vec<Item>,
) {
    let (x0, x1) = (g.left, g.max_x());
    let mut edge = |border: Option<docboss_model::Border>, from: (f32, f32), to: (f32, f32)| {
        let Some(border) = border else { return };
        if let Some(item) = crate::table::border_line(&border, from, to) {
            items.push(item);
        }
    };
    if first {
        edge(borders.top, (x0, 0.0), (x1, 0.0));
    }
    if last {
        edge(borders.bottom, (x0, height), (x1, height));
    }
    if between {
        edge(borders.inside_horizontal, (x0, height), (x1, height));
    }
    edge(borders.left, (x0, 0.0), (x0, height));
    edge(borders.right, (x1, 0.0), (x1, height));
}

fn metrics_for(ctx: &mut Ctx<'_>, glyph: &Glyph, style: &RunStyle) -> (f32, f32) {
    let base = ctx.shaper.metrics(Some(glyph.font), style.base_size);
    let drawn = ctx.shaper.metrics(Some(glyph.font), glyph.size);
    (
        base.ascent.max(style.rise + drawn.ascent),
        base.descent.max(drawn.descent - style.rise),
    )
}

fn build_atoms(
    ctx: &mut Ctx<'_>,
    styles: &[RunStyle],
    elems: &[Elem],
) -> (Vec<Atom>, Vec<Floating>, Vec<i64>) {
    let mut atoms: Vec<Atom> = Vec::new();
    let mut floats = Vec::new();
    let mut loose_notes = Vec::new();
    let mut word: Option<Atom> = None;
    let mut prev: Option<char> = None;
    let flush = |word: &mut Option<Atom>, atoms: &mut Vec<Atom>, break_after: bool| {
        if let Some(mut w) = word.take() {
            w.break_after = break_after;
            atoms.push(w);
        }
    };
    for elem in elems {
        match elem {
            Elem::Char(c, style) => {
                let c = *c;
                let glyph = ctx.shaper.glyph(c, &styles[*style]);
                let (ascent, descent) = metrics_for(ctx, &glyph, &styles[*style]);
                if breaks::is_space(c) {
                    flush(&mut word, &mut atoms, false);
                    let continues = atoms
                        .last()
                        .is_some_and(|a| a.is_space() && a.style == *style);
                    if !continues {
                        atoms.push(Atom::new(Kind::Space, *style));
                    }
                    let Some(space) = atoms.last_mut() else {
                        continue;
                    };
                    space.glyphs.push(AtomGlyph {
                        glyph,
                        style: *style,
                        ch: c,
                        x: space.width,
                    });
                    space.width += glyph.advance;
                    space.ascent = space.ascent.max(ascent);
                    space.descent = space.descent.max(descent);
                    prev = Some(c);
                    continue;
                }
                if let Some(space) = atoms.last_mut().filter(|a| a.is_space() && word.is_none()) {
                    space.break_after = prev.is_none_or(|p| breaks::allowed(p, c));
                }
                if word.is_some() && prev.is_some_and(|p| breaks::allowed(p, c)) {
                    flush(&mut word, &mut atoms, true);
                }
                let w = word.get_or_insert_with(|| Atom::new(Kind::Word, *style));
                let kern = w
                    .glyphs
                    .last()
                    .map_or(0.0, |last| ctx.shaper.kern(&last.glyph, &glyph));
                w.width += kern;
                w.glyphs.push(AtomGlyph {
                    glyph,
                    style: *style,
                    ch: c,
                    x: w.width,
                });
                w.width += glyph.advance;
                w.ascent = w.ascent.max(ascent);
                w.descent = w.descent.max(descent);
                w.notes.append(&mut loose_notes);
                prev = Some(c);
            }
            Elem::Tab(style) => {
                flush(&mut word, &mut atoms, false);
                let mut tab = Atom::new(Kind::Tab, *style);
                tab.break_after = true;
                atoms.push(tab);
                prev = Some('\t');
            }
            Elem::Break(kind, style) => {
                flush(&mut word, &mut atoms, true);
                atoms.push(Atom::new(Kind::Break(*kind), *style));
                prev = None;
            }
            Elem::Object(drawing, style) => {
                flush(&mut word, &mut atoms, true);
                let mut object = Atom::new(Kind::Object, *style);
                object.width = emu_to_pt(drawing.width).max(0.0);
                object.ascent = emu_to_pt(drawing.height).max(0.0);
                object.object = Some(drawing.clone());
                object.break_after = true;
                atoms.push(object);
                prev = None;
            }
            Elem::Float(drawing) => {
                let DrawingPlacement::Anchored {
                    horizontal,
                    vertical,
                    behind_text,
                } = drawing.placement
                else {
                    continue;
                };
                let text_box = layout_drawing(ctx, drawing);
                let height = text_box
                    .as_ref()
                    .map_or(emu_to_pt(drawing.height), |text_box| text_box.height);
                floats.push(Floating {
                    media: drawing.media,
                    picture: drawing.media.is_some() || text_box.is_none(),
                    width: emu_to_pt(drawing.width),
                    height,
                    horizontal,
                    vertical,
                    behind: behind_text,
                    content: text_box.map(|text_box| text_box.items).unwrap_or_default(),
                });
            }
            Elem::Note(id) => match word.as_mut() {
                Some(w) => w.notes.push(*id),
                None => loose_notes.push(*id),
            },
        }
    }
    flush(&mut word, &mut atoms, true);
    (atoms, floats, loose_notes)
}

/// Widths of the atoms after a tab up to the next tab or break, and up to
/// the first decimal point, for right, center and decimal stops.
fn lookahead(atoms: &[Atom], from: usize) -> (f32, f32) {
    let mut width = 0.0;
    let mut to_decimal = None;
    for atom in atoms.iter().skip(from) {
        if matches!(atom.kind, Kind::Tab | Kind::Break(_)) {
            break;
        }
        if to_decimal.is_none() {
            if let Some(dot) = atom.glyphs.iter().find(|g| g.ch == '.' || g.ch == ',') {
                to_decimal = Some(width + dot.x);
            }
        }
        width += atom.width;
    }
    (width, to_decimal.unwrap_or(width))
}

fn split_atom(atoms: &mut Vec<Atom>, index: usize, room: f32) {
    let atom = &atoms[index];
    let fits = atom
        .glyphs
        .iter()
        .take_while(|g| g.x + g.glyph.advance <= room)
        .count()
        .max(1);
    if fits >= atom.glyphs.len() {
        return;
    }
    let mut head = atom.clone();
    let mut tail = atom.clone();
    let offset = atom.glyphs[fits].x;
    head.glyphs.truncate(fits);
    head.width = offset;
    head.break_after = true;
    tail.glyphs.drain(..fits);
    tail.glyphs.iter_mut().for_each(|g| g.x -= offset);
    tail.width -= offset;
    tail.notes.clear();
    atoms[index] = head;
    atoms.insert(index + 1, tail);
}

fn break_lines(
    ctx: &mut Ctx<'_>,
    styles: &[RunStyle],
    atoms: &mut Vec<Atom>,
    g: &Geometry,
) -> Vec<Line> {
    let mut lines: Vec<Line> = Vec::new();
    let mut start = 0usize;
    let max_x = g.max_x();
    loop {
        let first = lines.is_empty();
        let line_left = if first {
            g.left + g.first_offset
        } else {
            g.left
        };
        let mut x = line_left;
        let mut placed: Vec<Placed> = Vec::new();
        let mut last_break: Option<usize> = None;
        let mut end = LineEnd::Last;
        let mut i = start;
        while i < atoms.len() {
            let kind = atoms[i].kind;
            if let Kind::Break(b) = kind {
                placed.push(Placed {
                    atom: i,
                    x,
                    width: 0.0,
                });
                i += 1;
                end = LineEnd::Mandatory(b);
                break;
            }
            let width = match kind {
                Kind::Tab => {
                    let (stop, alignment, _) = g.next_stop(x);
                    let (w, decimal) = lookahead(atoms, i + 1);
                    let target = match alignment {
                        TabAlignment::Right => stop - w,
                        TabAlignment::Center => stop - w / 2.0,
                        TabAlignment::Decimal => stop - decimal,
                        _ => stop,
                    };
                    (target - x).max(0.0).min((max_x - x).max(0.0))
                }
                _ => atoms[i].width,
            };
            let has_content = placed.iter().any(|p| !atoms[p.atom].is_space());
            let overflows = x + width > max_x + 0.01;
            if overflows && !matches!(kind, Kind::Space | Kind::Tab) && has_content {
                if let Some(bp) = last_break {
                    placed.truncate(bp + 1);
                    i = placed.last().map_or(i, |p| p.atom + 1);
                }
                end = LineEnd::Wrap;
                break;
            }
            if overflows && kind == Kind::Word && !has_content {
                split_atom(atoms, i, (max_x - x).max(0.0));
            }
            let width = if kind == Kind::Word {
                atoms[i].width
            } else {
                width
            };
            placed.push(Placed { atom: i, x, width });
            x += width;
            if atoms[i].break_after {
                last_break = Some(placed.len() - 1);
            }
            i += 1;
        }
        if end == LineEnd::Wrap && placed.is_empty() && i < atoms.len() {
            placed.push(Placed {
                atom: i,
                x,
                width: atoms[i].width,
            });
            i += 1;
        }
        if end == LineEnd::Wrap {
            hyphenate(ctx, styles, atoms, &mut placed);
        }
        start = i;
        let done = start >= atoms.len();
        let mandatory = matches!(end, LineEnd::Mandatory(_));
        lines.push(Line {
            placed,
            end: if done && !mandatory {
                LineEnd::Last
            } else {
                end
            },
        });
        if done && !mandatory {
            break;
        }
        if done {
            lines.push(Line {
                placed: Vec::new(),
                end: LineEnd::Last,
            });
            break;
        }
    }
    lines
}

/// A line broken at a soft hyphen shows a hyphen there.
fn hyphenate(ctx: &mut Ctx<'_>, styles: &[RunStyle], atoms: &mut [Atom], placed: &mut [Placed]) {
    let Some(last) = placed.last_mut() else {
        return;
    };
    let atom = &mut atoms[last.atom];
    let Some(tail) = atom.glyphs.last().filter(|g| g.ch == '\u{00AD}').cloned() else {
        return;
    };
    let hyphen = ctx.shaper.glyph('-', &styles[tail.style]);
    atom.glyphs.pop();
    let x = atom.glyphs.last().map_or(0.0, |g| g.x + g.glyph.advance);
    atom.glyphs.push(AtomGlyph {
        glyph: hyphen,
        style: tail.style,
        ch: '-',
        x,
    });
    atom.width = x + hyphen.advance;
    last.width = atom.width;
}

#[allow(clippy::too_many_arguments)]
fn emit_line(
    ctx: &mut Ctx<'_>,
    styles: &[RunStyle],
    atoms: &[Atom],
    line: &Line,
    g: &Geometry,
    justification: Justification,
    baseline: f32,
    is_last: bool,
    items: &mut Vec<Item>,
) {
    let content_end = line.placed.iter().rposition(|p| {
        let atom = &atoms[p.atom];
        !atom.is_space() && !matches!(atom.kind, Kind::Break(_))
    });
    let Some(content_end) = content_end else {
        return;
    };
    let placed = &line.placed[..=content_end];
    let end_x = placed.last().map_or(0.0, |p| p.x + p.width);
    let slack = (g.max_x() - end_x).max(0.0);
    let last_tab = placed
        .iter()
        .rposition(|p| atoms[p.atom].kind == Kind::Tab)
        .map_or(0, |i| i + 1);
    let stretchable: Vec<usize> = (last_tab..placed.len())
        .filter(|&i| atoms[placed[i].atom].is_space())
        .collect();
    let justify = match justification {
        Justification::Both => line.end == LineEnd::Wrap && !is_last,
        Justification::Distribute => true,
        _ => false,
    };
    let (shift, per_space) = match justification {
        Justification::Center => (slack / 2.0, 0.0),
        Justification::Right => (slack, 0.0),
        _ if justify && !stretchable.is_empty() => (0.0, slack / stretchable.len() as f32),
        _ => (0.0, 0.0),
    };
    let mut backgrounds = Vec::new();
    let mut glyphs: Vec<Item> = Vec::new();
    let mut decorations = Vec::new();
    let mut extra = 0.0;
    for (index, p) in placed.iter().enumerate() {
        let atom = &atoms[p.atom];
        let x0 = p.x + shift + extra;
        let width = p.width
            + if stretchable.contains(&index) {
                per_space
            } else {
                0.0
            };
        if stretchable.contains(&index) {
            extra += per_space;
        }
        let style = &styles[atom.style];
        match atom.kind {
            Kind::Object => {
                let Some(drawing) = &atom.object else {
                    continue;
                };
                let h = emu_to_pt(drawing.height);
                let text_box = layout_drawing(ctx, drawing);
                if drawing.media.is_some() || text_box.is_none() {
                    glyphs.push(Item::Image {
                        media: drawing.media,
                        rect: Rect::new(x0, baseline - h, width, h),
                    });
                }
                if let Some(text_box) = text_box {
                    glyphs.extend(text_box.items.into_iter().map(|mut item| {
                        item.offset(x0, baseline - h);
                        item
                    }));
                }
                continue;
            }
            Kind::Tab => {
                let (_, _, leader) = g.next_stop(p.x);
                leader_glyphs(ctx, style, leader, x0, width, baseline, &mut glyphs);
                decorate(
                    ctx,
                    style,
                    atom,
                    x0,
                    width,
                    baseline,
                    &mut backgrounds,
                    &mut decorations,
                );
                continue;
            }
            Kind::Break(_) => continue,
            Kind::Word | Kind::Space => {}
        }
        decorate(
            ctx,
            style,
            atom,
            x0,
            width,
            baseline,
            &mut backgrounds,
            &mut decorations,
        );
        if atom.is_space() {
            continue;
        }
        for glyph in &atom.glyphs {
            if glyph.ch == '\u{00AD}' || glyph.ch == '\u{200B}' {
                continue;
            }
            let run_style = &styles[glyph.style];
            push_glyph(
                ctx,
                &mut glyphs,
                run_style,
                glyph,
                x0 + glyph.x,
                baseline - run_style.rise,
            );
        }
    }
    items.extend(backgrounds);
    items.extend(glyphs);
    items.extend(decorations);
}

fn push_glyph(
    ctx: &mut Ctx<'_>,
    items: &mut Vec<Item>,
    style: &RunStyle,
    glyph: &AtomGlyph,
    x: f32,
    baseline: f32,
) {
    let (bold, italic) = ctx.shaper.synthetic(glyph.glyph.font, style);
    if let Some(Item::Glyphs(run)) = items.last_mut() {
        let same = run.font == glyph.glyph.font
            && run.size == glyph.glyph.size
            && run.color == style.color
            && run.baseline == baseline
            && run.synthetic_bold == bold
            && run.synthetic_italic == italic;
        if same {
            run.glyphs.push(PositionedGlyph {
                id: glyph.glyph.id,
                x,
            });
            run.text.push(glyph.ch);
            return;
        }
    }
    items.push(Item::Glyphs(GlyphRun {
        font: glyph.glyph.font,
        size: glyph.glyph.size,
        color: style.color,
        baseline,
        glyphs: vec![PositionedGlyph {
            id: glyph.glyph.id,
            x,
        }],
        text: glyph.ch.to_string(),
        synthetic_bold: bold,
        synthetic_italic: italic,
    }));
}

fn leader_glyphs(
    ctx: &mut Ctx<'_>,
    style: &RunStyle,
    leader: TabLeader,
    x: f32,
    width: f32,
    baseline: f32,
    items: &mut Vec<Item>,
) {
    let c = match leader {
        TabLeader::Dot => '.',
        TabLeader::Hyphen => '-',
        TabLeader::Underscore | TabLeader::Heavy => '_',
        TabLeader::MiddleDot => '\u{00B7}',
        TabLeader::None => return,
    };
    let glyph = ctx.shaper.glyph(c, style);
    if glyph.advance <= 0.1 || width < glyph.advance * 2.0 {
        return;
    }
    let count = ((width - glyph.advance * 0.5) / glyph.advance).floor() as usize;
    let start = x + width - count as f32 * glyph.advance;
    for k in 0..count.min(2000) {
        let atom_glyph = AtomGlyph {
            glyph,
            style: 0,
            ch: c,
            x: 0.0,
        };
        push_glyph(
            ctx,
            items,
            style,
            &atom_glyph,
            start + k as f32 * glyph.advance,
            baseline - style.rise,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn decorate(
    ctx: &mut Ctx<'_>,
    style: &RunStyle,
    atom: &Atom,
    x: f32,
    width: f32,
    baseline: f32,
    backgrounds: &mut Vec<Item>,
    decorations: &mut Vec<Item>,
) {
    if width <= 0.0 {
        return;
    }
    let font = atom
        .glyphs
        .first()
        .map(|g| g.glyph.font)
        .or_else(|| ctx.shaper.primary(style));
    let m = ctx.shaper.metrics(font, style.base_size);
    let fill = style.highlight.or(style.shading);
    if let Some(color) = fill {
        backgrounds.push(Item::Rect {
            rect: Rect::new(x, baseline - m.ascent, width, m.ascent + m.descent),
            color,
        });
    }
    let words_only = style.underline == Some(Underline::Words);
    let underline = style.underline.filter(|_| !(words_only && atom.is_space()));
    if let Some(kind) = underline {
        let y = baseline - style.rise + m.underline_position + m.underline_thickness / 2.0;
        let line_style = match kind {
            Underline::Double => LineStyle::Double,
            Underline::Dotted => {
                DashPattern::new(&[(100, 100)]).map_or(LineStyle::Solid, LineStyle::Dash)
            }
            Underline::Dashed => {
                DashPattern::new(&[(300, 200)]).map_or(LineStyle::Solid, LineStyle::Dash)
            }
            Underline::Wave => LineStyle::Wave,
            _ => LineStyle::Solid,
        };
        let thickness = if kind == Underline::Thick {
            m.underline_thickness * 2.0
        } else {
            m.underline_thickness
        };
        decorations.push(Item::Line {
            from: (x, y),
            to: (x + width, y),
            width: thickness,
            color: style.color,
            style: line_style,
            cap: LineCap::Flat,
        });
    }
    if style.strike || style.double_strike {
        let y = baseline - style.rise - m.strikeout_position;
        let line_style = if style.double_strike {
            LineStyle::Double
        } else {
            LineStyle::Solid
        };
        decorations.push(Item::Line {
            from: (x, y),
            to: (x + width, y),
            width: m.strikeout_thickness,
            color: style.color,
            style: line_style,
            cap: LineCap::Flat,
        });
    }
}

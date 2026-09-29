//! Page layout over the docboss document model.
//!
//! [`layout`] turns a [`Document`] into pages of positioned items: glyph
//! runs, filled rectangles, lines and images, in points with the origin at
//! the top-left corner of the page and y growing downwards. It follows
//! WordprocessingML semantics: resolved paragraph and run properties,
//! greedy line breaking, tab stops, lists, tables, sections with columns,
//! headers, footers and footnotes.

mod breaks;
mod flow;
mod paragraph;
mod shape;
mod table;
mod textbox;
mod units;

use std::sync::Arc;

use docboss_font::{FontDatabase, FontId};
use docboss_model::{Color, DashPattern, Diagnostic, Document, LineCap, LineJoin, Media, MediaId};

pub use units::{emu_to_pt, twips_to_pt};

/// An axis-aligned rectangle in points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    pub fn right(&self) -> f32 {
        self.x + self.width
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }

    fn offset(self, dx: f32, dy: f32) -> Rect {
        Rect {
            x: self.x + dx,
            y: self.y + dy,
            ..self
        }
    }
}

/// One glyph of a run and its pen position.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PositionedGlyph {
    pub id: u16,
    /// Horizontal pen position of the glyph origin in points.
    pub x: f32,
}

/// Glyphs sharing a face, size, color and baseline.
#[derive(Debug, Clone, PartialEq)]
pub struct GlyphRun {
    pub font: FontId,
    /// Font size in points.
    pub size: f32,
    pub color: Color,
    /// Baseline position in points from the top of the page.
    pub baseline: f32,
    pub glyphs: Vec<PositionedGlyph>,
    /// The text the glyphs show, for search and debugging.
    pub text: String,
    /// The run asks for bold but the face is not: embolden when painting.
    pub synthetic_bold: bool,
    /// The run asks for italic but the face is upright: slant when painting.
    pub synthetic_italic: bool,
}

/// How a line is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineStyle {
    Solid,
    /// Dashes in the pattern's lengths, each capped by the line's cap.
    Dash(DashPattern),
    Double,
    Wave,
}

/// Something painted on a page.
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Glyphs(GlyphRun),
    Rect {
        rect: Rect,
        color: Color,
    },
    Line {
        from: (f32, f32),
        to: (f32, f32),
        width: f32,
        color: Color,
        style: LineStyle,
        cap: LineCap,
    },
    Image {
        media: Option<MediaId>,
        rect: Rect,
    },
    /// A rectangle's outline, stroked clockwise from its top-left corner
    /// with the dash pattern running on across the corners.
    Outline {
        rect: Rect,
        width: f32,
        color: Color,
        style: LineStyle,
        cap: LineCap,
        join: LineJoin,
    },
    /// Confines the items that follow, up to the matching [`Item::ClipEnd`],
    /// to a rectangle.
    ClipBegin(Rect),
    ClipEnd,
}

impl Item {
    fn offset(&mut self, dx: f32, dy: f32) {
        match self {
            Item::Glyphs(run) => {
                run.baseline += dy;
                run.glyphs.iter_mut().for_each(|g| g.x += dx);
            }
            Item::Rect { rect, .. }
            | Item::Image { rect, .. }
            | Item::Outline { rect, .. }
            | Item::ClipBegin(rect) => *rect = rect.offset(dx, dy),
            Item::Line { from, to, .. } => {
                *from = (from.0 + dx, from.1 + dy);
                *to = (to.0 + dx, to.1 + dy);
            }
            Item::ClipEnd => {}
        }
    }
}

/// A laid-out page.
#[derive(Debug, Clone, PartialEq)]
pub struct Page {
    /// Page width in points.
    pub width: f32,
    /// Page height in points.
    pub height: f32,
    /// The page number shown by `PAGE` fields.
    pub number: u32,
    /// Items in painting order.
    pub items: Vec<Item>,
}

impl Page {
    /// Every glyph run on the page, in painting order.
    pub fn glyph_runs(&self) -> impl Iterator<Item = &GlyphRun> {
        self.items.iter().filter_map(|item| match item {
            Item::Glyphs(run) => Some(run),
            _ => None,
        })
    }

    /// The page's text in painting order, one line per glyph run.
    pub fn text(&self) -> String {
        self.glyph_runs()
            .map(|run| run.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Knobs of the layout engine.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutOptions {
    /// Apply the fonts' pair kerning. Word kerns only runs that ask for it,
    /// so this is off by default to match its line breaks.
    pub kerning: bool,
    /// The face used when a run names no font.
    pub default_font: String,
}

impl Default for LayoutOptions {
    fn default() -> Self {
        Self {
            kerning: false,
            default_font: "Times New Roman".to_string(),
        }
    }
}

/// A laid-out document: its pages and what painting them needs.
#[derive(Debug, Clone)]
pub struct Layout {
    pub pages: Vec<Page>,
    pub fonts: Arc<FontDatabase>,
    pub media: Vec<Media>,
    pub diagnostics: Vec<Diagnostic>,
}

/// The system fonts plus the fonts the document embeds.
pub fn fonts_for(document: &Document) -> Arc<FontDatabase> {
    let mut fonts = FontDatabase::system();
    fonts.add_document_fonts(&document.fonts);
    Arc::new(fonts)
}

/// Lays out a document with the given fonts.
pub fn layout(document: &Document, fonts: &Arc<FontDatabase>) -> Layout {
    layout_with_options(document, fonts, &LayoutOptions::default())
}

pub fn layout_with_options(
    document: &Document,
    fonts: &Arc<FontDatabase>,
    options: &LayoutOptions,
) -> Layout {
    let (pages, diagnostics) = flow::run(document, fonts, options);
    Layout {
        pages,
        fonts: fonts.clone(),
        media: document.media.clone(),
        diagnostics,
    }
}

/// Lays out a document with the system fonts and its own embedded fonts.
pub fn layout_document(document: &Document) -> Layout {
    layout(document, &fonts_for(document))
}

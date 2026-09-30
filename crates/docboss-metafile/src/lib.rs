//! Windows metafiles for docboss.
//!
//! [`play`] reads a WMF ([MS-WMF]) or EMF ([MS-EMF]) picture and plays its
//! records through a model of the GDI device context: mapping modes,
//! window and viewport origins and extents, world transforms, pens,
//! brushes, fonts, clipping and the saved-state stack. The result is a
//! [`Picture`]: paths, text and bitmaps in points over the picture's frame,
//! with the origin at its top-left corner and y growing downwards.
//! [`bitmap`] decodes the device-independent bitmaps they carry.

pub mod bitmap;
mod bytes;
mod dc;
mod emf;
mod wmf;

use std::sync::Arc;

use docboss_model::{Color, DashPattern, LineCap, LineJoin};

/// The most records one metafile plays.
pub const MAX_RECORDS: usize = 1_000_000;
/// The most path segments one metafile draws.
pub const MAX_SEGMENTS: usize = 2_000_000;

/// A point on the frame, in points.
pub type Point = (f32, f32);

/// An axis-aligned rectangle on the frame, in points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub fn right(&self) -> f32 {
        self.x + self.width
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }

    fn intersect(self, other: Rect) -> Rect {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());
        Rect {
            x,
            y,
            width: (right - x).max(0.0),
            height: (bottom - y).max(0.0),
        }
    }

    fn union(self, other: Rect) -> Rect {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        Rect {
            x,
            y,
            width: self.right().max(other.right()) - x,
            height: self.bottom().max(other.bottom()) - y,
        }
    }

    fn around(points: &[Point]) -> Option<Rect> {
        let (first, rest) = points.split_first()?;
        let mut r = Rect {
            x: first.0,
            y: first.1,
            width: 0.0,
            height: 0.0,
        };
        for p in rest {
            r = r.union(Rect {
                x: p.0,
                y: p.1,
                width: 0.0,
                height: 0.0,
            });
        }
        Some(r)
    }
}

/// A piece of an outline on the frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Seg {
    Move(f32, f32),
    Line(f32, f32),
    Cubic(f32, f32, f32, f32, f32, f32),
    Close,
}

/// How a path is filled.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fill {
    pub color: Color,
    /// The alternate (even-odd) rule; the winding rule otherwise.
    pub even_odd: bool,
}

/// How a path is stroked.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pen {
    /// Width in points on the frame; zero for a one-pixel cosmetic pen.
    pub width: f32,
    pub color: Color,
    pub dash: Option<DashPattern>,
    pub cap: LineCap,
    pub join: LineJoin,
}

/// The font a text record draws with.
#[derive(Debug, Clone, PartialEq)]
pub struct Font {
    /// The face name; empty for the device's default face.
    pub face: String,
    /// Height in points on the frame: the em when `cell` is false, the
    /// character cell (em plus internal leading) when true.
    pub size: f32,
    pub cell: bool,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
}

/// Where a text record's reference point sits on the text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    Left,
    Center,
    Right,
}

/// Where a text record's reference point sits vertically.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Baseline {
    Top,
    Baseline,
    Bottom,
}

/// A line of text.
#[derive(Debug, Clone, PartialEq)]
pub struct Text {
    /// The reference point on the frame.
    pub at: Point,
    pub text: String,
    /// The advance of each character along the baseline, in points; the
    /// font's own advances when absent.
    pub advances: Option<Vec<f32>>,
    pub font: Font,
    pub color: Color,
    /// The fill behind the character cells in opaque background mode.
    pub background: Option<Color>,
    pub anchor: Anchor,
    pub baseline: Baseline,
    /// The baseline's angle in radians, counterclockwise from the x axis.
    pub angle: f32,
}

/// A bitmap drawn into a rectangle.
#[derive(Debug, Clone, PartialEq)]
pub struct Bitmap {
    /// A BMP file of the source pixels, alpha kept.
    pub bmp: Arc<[u8]>,
    /// The full bitmap's size in pixels.
    pub size: (u32, u32),
    /// The rectangle on the frame the source rectangle is drawn into.
    pub dest: Rect,
    /// The source rectangle in pixels from the top-left: x, y, width, height.
    pub source: (f32, f32, f32, f32),
}

/// One painting operation, in the order the metafile paints them.
#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    Path {
        segs: Vec<Seg>,
        fill: Option<Fill>,
        stroke: Option<Pen>,
    },
    Text(Text),
    Bitmap(Bitmap),
    /// The clip rectangle for the operations that follow; `None` for none.
    Clip(Option<Rect>),
}

/// Which format a picture was read as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Wmf,
    Emf,
    /// A WMF whose escape records carry an EMF, played instead.
    EmbeddedEmf,
}

/// A played metafile.
#[derive(Debug, Clone, PartialEq)]
pub struct Picture {
    pub kind: Kind,
    /// The frame's size in points.
    pub width: f32,
    pub height: f32,
    pub ops: Vec<Op>,
    /// What was approximated or left out, one line per kind of record.
    pub notes: Vec<String>,
}

/// Why a metafile could not be played.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    NotAMetafile,
    Malformed(&'static str),
    /// An EMF that draws only through EMF+ records.
    EmfPlusOnly,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::NotAMetafile => f.write_str("not a WMF or EMF metafile"),
            Error::Malformed(what) => write!(f, "malformed metafile: {what}"),
            Error::EmfPlusOnly => f.write_str("an EMF+ only metafile, which is not drawn"),
        }
    }
}

impl std::error::Error for Error {}

/// Whether `bytes` start like a WMF (placeable or not) or an EMF.
pub fn is_metafile(bytes: &[u8]) -> bool {
    wmf::sniff(bytes) || emf::sniff(bytes)
}

/// Plays a WMF or EMF metafile.
pub fn play(bytes: &[u8]) -> Result<Picture, Error> {
    if emf::sniff(bytes) {
        return emf::play(bytes, Kind::Emf);
    }
    if wmf::sniff(bytes) {
        return wmf::play(bytes);
    }
    Err(Error::NotAMetafile)
}

/// The text a picture draws, one line per run of records on one baseline.
pub fn text(picture: &Picture) -> String {
    let mut out = String::new();
    let mut last: Option<(f32, f32)> = None;
    for op in &picture.ops {
        let Op::Text(t) = op else {
            continue;
        };
        let line = t.text.trim_end_matches(['\0', '\r', '\n']);
        if line.trim().is_empty() {
            continue;
        }
        let same_line = last.is_some_and(|(y, size)| (t.at.1 - y).abs() < size * 0.5);
        if !out.is_empty() {
            out.push(if same_line { ' ' } else { '\n' });
        }
        out.push_str(line.trim());
        last = Some((t.at.1, t.font.size.max(1.0)));
    }
    out
}

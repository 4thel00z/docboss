//! The playback device context the WMF and EMF players share: the
//! mapping from logical units to the frame, the object table, the drawing
//! state and its saved copies, path brackets and the painting operations
//! they produce.

use std::collections::BTreeMap;
use std::sync::Arc;

use docboss_model::{Color, DashPattern, LineCap, LineJoin};

use crate::bitmap::{encode_bmp, Rgba};
use crate::{Anchor, Baseline, Bitmap, Fill, Font, Op, Pen, Point, Rect, Seg, Text, MAX_SEGMENTS};

/// `[a, b, c, d, e, f]` takes `(x, y)` to `(a x + c y + e, b x + d y + f)`.
pub(crate) type Affine = [f64; 6];

pub(crate) const IDENTITY: Affine = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

const MAX_OPS: usize = 500_000;
const MAX_SAVES: usize = 1024;
const MAX_OBJECTS: usize = 65_536;
const MAX_PENDING: usize = 50_000;

pub(crate) fn apply(m: &Affine, x: f64, y: f64) -> (f64, f64) {
    (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
}

/// The map that applies `first`, then `second`.
pub(crate) fn compose(first: &Affine, second: &Affine) -> Affine {
    [
        second[0] * first[0] + second[2] * first[1],
        second[1] * first[0] + second[3] * first[1],
        second[0] * first[2] + second[2] * first[3],
        second[1] * first[2] + second[3] * first[3],
        second[0] * first[4] + second[2] * first[5] + second[4],
        second[1] * first[4] + second[3] * first[5] + second[5],
    ]
}

fn ratio(a: f64, b: f64) -> f64 {
    let r = a / b;
    if b == 0.0 || !r.is_finite() {
        return 1.0;
    }
    r
}

/// A logical pen ([MS-WMF] §2.2.1.4, [MS-EMF] §2.2.19 and §2.2.20).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PenObject {
    pub style: u32,
    /// Width in logical units.
    pub width: f64,
    pub color: Color,
    pub dash: Option<DashPattern>,
}

/// A logical brush ([MS-WMF] §2.2.1.1); pattern brushes keep the average
/// color of their bitmap.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum BrushObject {
    Null,
    Solid(Color),
    Hatched(Color),
}

/// A logical font ([MS-WMF] §2.2.1.2, [MS-EMF] §2.2.13).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FontObject {
    /// Height in logical units: negative for the em, positive for the cell.
    pub height: f64,
    /// Escapement in tenths of a degree.
    pub escapement: f64,
    pub weight: i32,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub charset: u8,
    pub face: String,
}

impl FontObject {
    pub(crate) fn default_face(face: &str) -> FontObject {
        FontObject {
            height: 0.0,
            escapement: 0.0,
            weight: 400,
            italic: false,
            underline: false,
            strike: false,
            charset: 0,
            face: face.to_string(),
        }
    }

    /// The characters of 8-bit text in this font: Windows-1252, or the
    /// symbol-font private-use codes for the symbol character set.
    pub(crate) fn decode(&self, bytes: &[u8]) -> String {
        bytes
            .iter()
            .map(|&b| match self.symbolic() {
                true => char::from_u32(0xF000 + u32::from(b)).unwrap_or(' '),
                false => cp1252(b),
            })
            .collect()
    }

    /// Whether the font is a symbol font whose codes are font positions.
    pub(crate) fn symbolic(&self) -> bool {
        const SYMBOL_CHARSET: u8 = 2;
        self.charset == SYMBOL_CHARSET
    }
}

const CP1252_HIGH: [u16; 32] = [
    0x20AC, 0x0081, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160, 0x2039,
    0x0152, 0x008D, 0x017D, 0x008F, 0x0090, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014,
    0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0x009D, 0x017E, 0x0178,
];

fn cp1252(b: u8) -> char {
    match b {
        0x80..=0x9F => char::from_u32(u32::from(CP1252_HIGH[usize::from(b - 0x80)])).unwrap_or(' '),
        _ => char::from(b),
    }
}

/// An entry of the object table.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Object {
    Pen(PenObject),
    Brush(BrushObject),
    Font(FontObject),
    /// A palette, region or color space: held so indices stay right.
    Other,
}

/// The drawing state RESTOREDC brings back.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct State {
    pub map_mode: u32,
    pub window_org: (f64, f64),
    pub window_ext: (f64, f64),
    pub viewport_org: (f64, f64),
    pub viewport_ext: (f64, f64),
    pub world: Affine,
    pub pen: PenObject,
    pub brush: BrushObject,
    pub font: FontObject,
    pub text_color: Color,
    pub bk_color: Color,
    pub opaque: bool,
    pub align: u32,
    pub even_odd: bool,
    pub rop2: u32,
    pub position: (f64, f64),
    pub clip: Option<Rect>,
    pub clockwise: bool,
}

pub(crate) const MM_TEXT: u32 = 1;
const MM_LOMETRIC: u32 = 2;
const MM_HIMETRIC: u32 = 3;
const MM_LOENGLISH: u32 = 4;
const MM_HIENGLISH: u32 = 5;
const MM_TWIPS: u32 = 6;
const MM_ISOTROPIC: u32 = 7;
pub(crate) const MM_ANISOTROPIC: u32 = 8;

const PS_NULL: u32 = 5;
const PS_USERSTYLE: u32 = 7;
const R2_BLACK: u32 = 1;
const R2_NOP: u32 = 11;
const R2_COPYPEN: u32 = 13;
const R2_WHITE: u32 = 16;

pub(crate) const SRCCOPY: u32 = 0x00CC_0020;
const SRCPAINT: u32 = 0x00EE_0086;
const SRCAND: u32 = 0x0088_00C6;
const SRCINVERT: u32 = 0x0066_0046;
const NOTSRCCOPY: u32 = 0x0033_0008;
const PATCOPY: u32 = 0x00F0_0021;
const BLACKNESS: u32 = 0x0000_0042;
const WHITENESS: u32 = 0x00FF_0062;
const DSTCOPY: u32 = 0x00AA_0029;
const PATAND: u32 = 0x00A0_00C9;

pub(crate) const BLACK_PEN: PenObject = PenObject {
    style: 0,
    width: 0.0,
    color: Color::BLACK,
    dash: None,
};

/// The device context: state, saved states, objects and the operations
/// played so far.
pub(crate) struct Dc {
    pub state: State,
    saved: Vec<State>,
    objects: Vec<Option<Object>>,
    ops: Vec<Op>,
    /// Device units to frame points.
    device: Affine,
    /// Device units per millimetre, for the metric and English map modes.
    per_mm: (f64, f64),
    /// WMF: the viewport stays the picture's frame.
    fixed_viewport: bool,
    path: Option<Vec<Seg>>,
    ended: Option<Vec<Seg>>,
    pending: Option<(Vec<Seg>, Pen, Point)>,
    emitted_clip: Option<Rect>,
    segments: usize,
    notes: BTreeMap<String, u32>,
    pub full: bool,
}

impl Dc {
    pub(crate) fn new(
        state: State,
        device: Affine,
        per_mm: (f64, f64),
        fixed_viewport: bool,
    ) -> Dc {
        Dc {
            state,
            saved: Vec::new(),
            objects: Vec::new(),
            ops: Vec::new(),
            device,
            per_mm,
            fixed_viewport,
            path: None,
            ended: None,
            pending: None,
            emitted_clip: None,
            segments: 0,
            notes: BTreeMap::new(),
            full: false,
        }
    }

    /// Records one approximated or skipped item under `what`.
    pub(crate) fn note(&mut self, what: impl Into<String>) {
        *self.notes.entry(what.into()).or_insert(0) += 1;
    }

    /// The operations and notes played so far.
    pub(crate) fn finish(mut self) -> (Vec<Op>, Vec<String>) {
        self.flush();
        let notes = self
            .notes
            .iter()
            .map(|(what, n)| match n {
                1 => what.clone(),
                n => format!("{what} ({n} times)"),
            })
            .collect();
        (self.ops, notes)
    }

    pub(crate) fn has_ops(&self) -> bool {
        !self.ops.is_empty() || self.pending.is_some()
    }

    /// [MS-EMF] §2.1.21 MapMode, [MS-WMF] §2.1.1.16: page space to device
    /// space for the current mapping mode, window and viewport.
    fn page(&self) -> Affine {
        let s = &self.state;
        let (wx, wy) = s.window_org;
        let (vx, vy) = s.viewport_org;
        let metric = |mm_per_unit: f64| (self.per_mm.0 * mm_per_unit, -self.per_mm.1 * mm_per_unit);
        let (sx, sy) = match s.map_mode {
            MM_ANISOTROPIC => (
                ratio(s.viewport_ext.0, s.window_ext.0),
                ratio(s.viewport_ext.1, s.window_ext.1),
            ),
            MM_ISOTROPIC => {
                let sx = ratio(s.viewport_ext.0, s.window_ext.0);
                let sy = ratio(s.viewport_ext.1, s.window_ext.1);
                let k = sx.abs().min(sy.abs());
                (k.copysign(sx), k.copysign(sy))
            }
            MM_LOMETRIC => metric(0.1),
            MM_HIMETRIC => metric(0.01),
            MM_LOENGLISH => metric(0.254),
            MM_HIENGLISH => metric(0.0254),
            MM_TWIPS => metric(25.4 / 1440.0),
            _ => (1.0, 1.0),
        };
        [sx, 0.0, 0.0, sy, vx - wx * sx, vy - wy * sy]
    }

    /// Logical units to frame points: world, page and device transforms.
    pub(crate) fn transform(&self) -> Affine {
        compose(&compose(&self.state.world, &self.page()), &self.device)
    }

    fn frame(m: &Affine, x: f64, y: f64) -> Point {
        let (x, y) = apply(m, x, y);
        (x as f32, y as f32)
    }

    /// A logical rectangle's bounds on the frame.
    pub(crate) fn logical_rect(&self, l: f64, t: f64, r: f64, b: f64) -> Rect {
        let m = self.transform();
        let corners = [(l, t), (r, t), (r, b), (l, b)].map(|(x, y)| Self::frame(&m, x, y));
        Rect::around(&corners).unwrap_or(Rect {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
        })
    }

    /// A device rectangle's bounds on the frame.
    pub(crate) fn device_rect(&self, l: f64, t: f64, r: f64, b: f64) -> Rect {
        let corners = [(l, t), (r, b)].map(|(x, y)| Self::frame(&self.device, x, y));
        Rect::around(&corners).unwrap_or(Rect {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
        })
    }

    pub(crate) fn set_window_org(&mut self, x: f64, y: f64) {
        self.state.window_org = (x, y);
    }

    pub(crate) fn set_window_ext(&mut self, x: f64, y: f64) {
        if x == 0.0 || y == 0.0 {
            return;
        }
        self.state.window_ext = (x, y);
    }

    pub(crate) fn set_viewport_org(&mut self, x: f64, y: f64) {
        if self.fixed_viewport {
            return;
        }
        self.state.viewport_org = (x, y);
    }

    pub(crate) fn set_viewport_ext(&mut self, x: f64, y: f64) {
        if self.fixed_viewport || x == 0.0 || y == 0.0 {
            return;
        }
        self.state.viewport_ext = (x, y);
    }

    pub(crate) fn set_map_mode(&mut self, mode: u32) {
        if !(MM_TEXT..=MM_ANISOTROPIC).contains(&mode) {
            return;
        }
        self.state.map_mode = mode;
    }

    /// [MS-EMF] §2.3.12.1 and §2.3.12.2: sets or combines the world
    /// transform; `mode` 1 resets it, 2 applies `m` first, 3 last, 4 sets it.
    pub(crate) fn world(&mut self, m: Affine, mode: u32) {
        if !m.iter().all(|v| v.is_finite()) {
            return;
        }
        self.state.world = match mode {
            1 => IDENTITY,
            2 => compose(&m, &self.state.world),
            3 => compose(&self.state.world, &m),
            4 => m,
            _ => return,
        };
    }

    /// Adds an object to the table: at `index`, or at the lowest free slot.
    pub(crate) fn create(&mut self, index: Option<usize>, object: Object) {
        let slot = match index {
            Some(i) => i,
            None => self
                .objects
                .iter()
                .position(Option::is_none)
                .unwrap_or(self.objects.len()),
        };
        if slot >= MAX_OBJECTS {
            self.note("objects past the table size were dropped");
            return;
        }
        if slot >= self.objects.len() {
            self.objects.resize(slot + 1, None);
        }
        self.objects[slot] = Some(object);
    }

    pub(crate) fn delete(&mut self, index: usize) {
        if let Some(slot) = self.objects.get_mut(index) {
            *slot = None;
        }
    }

    /// Selects object `index` of the table into the context.
    pub(crate) fn select(&mut self, index: usize) {
        let Some(Some(object)) = self.objects.get(index).cloned() else {
            return;
        };
        self.select_object(object);
    }

    pub(crate) fn select_object(&mut self, object: Object) {
        match object {
            Object::Pen(pen) => {
                self.flush();
                self.state.pen = pen;
            }
            Object::Brush(brush) => self.state.brush = brush,
            Object::Font(font) => self.state.font = font,
            Object::Other => {}
        }
    }

    pub(crate) fn save(&mut self) {
        if self.saved.len() >= MAX_SAVES {
            self.note("saved states past 1024 were dropped");
            return;
        }
        self.saved.push(self.state.clone());
    }

    /// [MS-EMF] §2.3.11.6: a negative `n` goes back that many saves, a
    /// positive one to the `n`th save.
    pub(crate) fn restore(&mut self, n: i32) {
        let len = self.saved.len() as i64;
        let target = match n < 0 {
            true => len + i64::from(n),
            false => i64::from(n) - 1,
        };
        if target < 0 || target >= len {
            return;
        }
        self.flush();
        self.saved.truncate(target as usize + 1);
        if let Some(state) = self.saved.pop() {
            self.state = state;
        }
    }

    pub(crate) fn set_clip(&mut self, clip: Option<Rect>) {
        if clip == self.state.clip {
            return;
        }
        self.flush();
        self.state.clip = clip;
    }

    pub(crate) fn intersect_clip(&mut self, rect: Rect) {
        let clip = match self.state.clip {
            Some(current) => current.intersect(rect),
            None => rect,
        };
        self.set_clip(Some(clip));
    }

    /// [MS-EMF] §2.1.29 RegionMode: combines rectangles, as their bounds,
    /// with the clip: 1 and, 2 or, 5 copy; xor and diff are dropped.
    pub(crate) fn combine_clip(&mut self, rects: &[Rect], mode: u32) {
        if rects.len() > 1 {
            self.note("clip regions of several rectangles are clipped to their bounds");
        }
        let bounds = rects.iter().copied().reduce(Rect::union);
        match (mode, bounds) {
            (5, None) => self.set_clip(None),
            (5, Some(b)) => self.set_clip(Some(b)),
            (1, Some(b)) => self.intersect_clip(b),
            (2, Some(b)) => {
                let clip = self.state.clip.map(|c| c.union(b));
                self.set_clip(clip);
            }
            (_, None) => {}
            _ => self.note("xor and difference clip regions are ignored"),
        }
    }

    fn sync_clip(&mut self) {
        if self.state.clip == self.emitted_clip {
            return;
        }
        self.ops.push(Op::Clip(self.state.clip));
        self.emitted_clip = self.state.clip;
    }

    pub(crate) fn flush(&mut self) {
        let Some((segs, pen, _)) = self.pending.take() else {
            return;
        };
        self.sync_clip();
        self.ops.push(Op::Path {
            segs,
            fill: None,
            stroke: Some(pen),
        });
    }

    fn emit(&mut self, op: Op) {
        self.flush();
        if self.ops.len() >= MAX_OPS {
            self.note("operations past 500000 were dropped");
            self.full = true;
            return;
        }
        self.sync_clip();
        self.ops.push(op);
    }

    fn count(&mut self, n: usize) -> bool {
        self.segments += n;
        if self.segments <= MAX_SEGMENTS {
            return true;
        }
        if !self.full {
            self.note("path segments past the limit were dropped");
        }
        self.full = true;
        false
    }

    /// The pen as it strokes on the frame, `None` for the null pen.
    pub(crate) fn stroke(&self) -> Option<Pen> {
        let p = self.state.pen;
        let style = p.style & 0xF;
        if style == PS_NULL || self.state.rop2 == R2_NOP {
            return None;
        }
        let m = self.transform();
        let scale = (m[0] * m[3] - m[1] * m[2]).abs().sqrt();
        let width = (p.width * scale) as f32;
        let cosmetic = p.width <= 0.0;
        let stops: &[(u32, u32)] = match (style, cosmetic) {
            (1, true) => &[(1800, 600)],
            (2, true) => &[(300, 300)],
            (3, true) => &[(900, 600), (300, 600)],
            (4, true) => &[(900, 300), (300, 300), (300, 300)],
            (1, false) => &[(300, 100)],
            (2, false) => &[(100, 100)],
            (3, false) => &[(300, 100), (100, 100)],
            (4, false) => &[(300, 100), (100, 100), (100, 100)],
            _ => &[],
        };
        let dash = match style {
            PS_USERSTYLE => p.dash,
            _ => DashPattern::new(stops),
        };
        let cap = match p.style & 0xF00 {
            0x100 => LineCap::Square,
            0x200 => LineCap::Flat,
            _ => LineCap::Round,
        };
        let join = match p.style & 0xF000 {
            0x1000 => LineJoin::Bevel,
            0x2000 => LineJoin::Miter,
            _ => LineJoin::Round,
        };
        Some(Pen {
            width: width.max(0.0),
            color: self.mix(p.color),
            dash,
            cap,
            join,
        })
    }

    /// The color the foreground mix mode paints `color` as.
    fn mix(&self, color: Color) -> Color {
        match self.state.rop2 {
            R2_BLACK => Color::BLACK,
            R2_WHITE => Color::WHITE,
            _ => color,
        }
    }

    /// The brush as it fills, `None` for the null brush.
    pub(crate) fn fill(&mut self) -> Option<Fill> {
        if self.state.rop2 == R2_NOP {
            return None;
        }
        if !matches!(self.state.rop2, R2_COPYPEN | R2_BLACK | R2_WHITE | 0) {
            self.note("mix modes other than copy are drawn as copy");
        }
        let color = match self.state.brush {
            BrushObject::Null => return None,
            BrushObject::Solid(c) => c,
            BrushObject::Hatched(c) => {
                self.note("hatched brushes are filled with a tint of their color");
                let behind = match self.state.opaque {
                    true => self.state.bk_color,
                    false => Color::WHITE,
                };
                tint(c, behind, 0.3)
            }
        };
        Some(Fill {
            color: self.mix(color),
            even_odd: self.state.even_odd,
        })
    }

    fn map_points(&mut self, points: &[(f64, f64)]) -> Option<Vec<Point>> {
        if !self.count(points.len()) {
            return None;
        }
        let m = self.transform();
        Some(points.iter().map(|&(x, y)| Self::frame(&m, x, y)).collect())
    }

    /// Adds a figure to the open path bracket or paints it.
    fn figure(&mut self, segs: Vec<Seg>, closed: bool) {
        if let Some(path) = self.path.as_mut() {
            path.extend(segs);
            return;
        }
        let fill = match closed {
            true => self.fill(),
            false => None,
        };
        let stroke = self.stroke();
        if fill.is_none() && stroke.is_none() {
            return;
        }
        self.emit(Op::Path { segs, fill, stroke });
    }

    fn polyline_segs(points: &[Point], closed: bool) -> Vec<Seg> {
        let mut segs: Vec<Seg> = Vec::with_capacity(points.len() + 1);
        for (i, &(x, y)) in points.iter().enumerate() {
            segs.push(match i {
                0 => Seg::Move(x, y),
                _ => Seg::Line(x, y),
            });
        }
        if closed {
            segs.push(Seg::Close);
        }
        segs
    }

    pub(crate) fn move_to(&mut self, x: f64, y: f64) {
        self.state.position = (x, y);
        let (px, py) = apply(&self.transform(), x, y);
        if let Some(path) = self.path.as_mut() {
            path.push(Seg::Move(px as f32, py as f32));
        }
    }

    /// Continues the open path, or strokes from the current position.
    pub(crate) fn line_to(&mut self, x: f64, y: f64) {
        let m = self.transform();
        let (px, py) = self.state.position;
        self.state.position = (x, y);
        if !self.count(1) {
            return;
        }
        let from = Self::frame(&m, px, py);
        let to = Self::frame(&m, x, y);
        if let Some(path) = self.path.as_mut() {
            if path.is_empty() {
                path.push(Seg::Move(from.0, from.1));
            }
            path.push(Seg::Line(to.0, to.1));
            return;
        }
        let Some(pen) = self.stroke() else {
            return;
        };
        if let Some((segs, current, end)) = self.pending.as_mut() {
            if *current == pen && segs.len() < MAX_PENDING {
                if *end != from {
                    segs.push(Seg::Move(from.0, from.1));
                }
                segs.push(Seg::Line(to.0, to.1));
                *end = to;
                return;
            }
        }
        self.flush();
        self.pending = Some((
            vec![Seg::Move(from.0, from.1), Seg::Line(to.0, to.1)],
            pen,
            to,
        ));
    }

    /// A polyline from its first point, or from the current position when
    /// `to`, which then moves to its last point.
    pub(crate) fn polyline(&mut self, points: &[(f64, f64)], to: bool) {
        if to {
            points.iter().for_each(|&(x, y)| self.line_to(x, y));
            return;
        }
        if points.len() < 2 {
            return;
        }
        let Some(mapped) = self.map_points(points) else {
            return;
        };
        self.figure(Self::polyline_segs(&mapped, false), false);
    }

    pub(crate) fn polygon(&mut self, points: &[(f64, f64)]) {
        self.polypolygon(&[points]);
    }

    /// Filled and outlined polygons, filled together under the fill mode.
    pub(crate) fn polypolygon(&mut self, polygons: &[&[(f64, f64)]]) {
        let mut segs = Vec::new();
        for points in polygons {
            if points.len() < 2 {
                continue;
            }
            let Some(mapped) = self.map_points(points) else {
                return;
            };
            segs.extend(Self::polyline_segs(&mapped, true));
        }
        if segs.is_empty() {
            return;
        }
        self.figure(segs, true);
    }

    /// Several polylines, stroked as one outline.
    pub(crate) fn polypolyline(&mut self, lines: &[&[(f64, f64)]]) {
        let mut segs = Vec::new();
        for points in lines {
            if points.len() < 2 {
                continue;
            }
            let Some(mapped) = self.map_points(points) else {
                return;
            };
            segs.extend(Self::polyline_segs(&mapped, false));
        }
        if segs.is_empty() {
            return;
        }
        self.figure(segs, false);
    }

    /// Cubic Béziers: a start point and three points per curve, or with
    /// `to` curves from the current position, which moves to the last point.
    pub(crate) fn polybezier(&mut self, points: &[(f64, f64)], to: bool) {
        let (start, rest) = match to {
            true => (self.state.position, points),
            false => match points.split_first() {
                Some((first, rest)) => (*first, rest),
                None => return,
            },
        };
        let curves = rest.len() / 3;
        if curves == 0 || !self.count(curves + 1) {
            return;
        }
        let m = self.transform();
        let s = Self::frame(&m, start.0, start.1);
        let mut segs = vec![Seg::Move(s.0, s.1)];
        for c in rest.as_chunks::<3>().0.iter() {
            let [a, b, e] = [c[0], c[1], c[2]].map(|(x, y)| Self::frame(&m, x, y));
            segs.push(Seg::Cubic(a.0, a.1, b.0, b.1, e.0, e.1));
        }
        if let Some(&last) = rest.get(curves * 3 - 1) {
            if to {
                self.state.position = last;
            }
        }
        if to {
            if let Some(path) = self.path.as_mut() {
                if !path.is_empty() {
                    segs.remove(0);
                }
                path.extend(segs);
                return;
            }
        }
        self.figure(segs, false);
    }

    /// [MS-EMF] §2.3.5.34, [MS-WMF] §2.3.3.17: a rectangle, filled and
    /// outlined.
    pub(crate) fn rectangle(&mut self, l: f64, t: f64, r: f64, b: f64) {
        self.polygon(&[(l, t), (r, t), (r, b), (l, b)]);
    }

    /// [MS-EMF] §2.3.5.35, [MS-WMF] §2.3.3.18: a rectangle with corners
    /// rounded by an ellipse `w` by `h`.
    pub(crate) fn round_rect(&mut self, l: f64, t: f64, r: f64, b: f64, w: f64, h: f64) {
        let (l, r) = (l.min(r), l.max(r));
        let (t, b) = (t.min(b), t.max(b));
        let rx = (w.abs() / 2.0).min((r - l) / 2.0);
        let ry = (h.abs() / 2.0).min((b - t) / 2.0);
        if rx <= 0.0 || ry <= 0.0 {
            return self.rectangle(l, t, r, b);
        }
        let quarter = -std::f64::consts::FRAC_PI_2;
        let corners = [
            (r - rx, t + ry, std::f64::consts::FRAC_PI_2),
            (r - rx, b - ry, 0.0),
            (l + rx, b - ry, quarter),
            (l + rx, t + ry, std::f64::consts::PI),
        ];
        let mut segs = vec![LogicalSeg::Move((l + rx, t))];
        let mut curves = Vec::new();
        for (cx, cy, from) in corners {
            segs.push(LogicalSeg::Line(ellipse_point(cx, cy, rx, ry, from)));
            curves.clear();
            arc_curves(cx, cy, rx, ry, from, quarter, &mut curves);
            segs.extend(curves.iter().map(|c| LogicalSeg::Cubic(*c)));
        }
        segs.push(LogicalSeg::Close);
        self.logical_figure(&segs, true);
    }

    /// [MS-EMF] §2.3.5.5, [MS-WMF] §2.3.3.3: the ellipse inscribed in a box.
    pub(crate) fn ellipse(&mut self, l: f64, t: f64, r: f64, b: f64) {
        let (cx, cy, rx, ry) = (
            (l + r) / 2.0,
            (t + b) / 2.0,
            (r - l).abs() / 2.0,
            (b - t).abs() / 2.0,
        );
        let mut curves = Vec::new();
        arc_curves(cx, cy, rx, ry, 0.0, std::f64::consts::TAU, &mut curves);
        let mut segs = vec![LogicalSeg::Move(ellipse_point(cx, cy, rx, ry, 0.0))];
        segs.extend(curves.into_iter().map(LogicalSeg::Cubic));
        segs.push(LogicalSeg::Close);
        self.logical_figure(&segs, true);
    }

    /// [MS-EMF] §2.3.5.2 to §2.3.5.4 and §2.3.5.15, [MS-WMF] §2.3.3.1,
    /// §2.3.3.2 and §2.3.3.13: the arc of the ellipse in a box between the
    /// rays through `start` and `end`, counterclockwise unless the arc
    /// direction says otherwise; drawn open, as a chord, a pie or, for
    /// `ArcTo`, joined to the current position.
    pub(crate) fn arc(
        &mut self,
        bx: (f64, f64, f64, f64),
        start: (f64, f64),
        end: (f64, f64),
        kind: ArcKind,
    ) {
        let (l, t, r, b) = bx;
        let (cx, cy, rx, ry) = (
            (l + r) / 2.0,
            (t + b) / 2.0,
            (r - l).abs() / 2.0,
            (b - t).abs() / 2.0,
        );
        if rx <= 0.0 || ry <= 0.0 {
            return;
        }
        let angle = |(x, y): (f64, f64)| (-(y - cy) / ry).atan2((x - cx) / rx);
        let (a0, a1) = (angle(start), angle(end));
        let mut sweep = a1 - a0;
        let tau = std::f64::consts::TAU;
        if self.state.clockwise {
            while sweep >= 0.0 {
                sweep -= tau;
            }
        }
        if !self.state.clockwise {
            while sweep <= 0.0 {
                sweep += tau;
            }
        }
        self.arc_sweep(cx, cy, rx, ry, a0, sweep, kind);
    }

    /// [MS-EMF] §2.3.5.1: a line from the current position and an arc of a
    /// circle from `start` degrees through `sweep` degrees.
    pub(crate) fn angle_arc(&mut self, c: (f64, f64), radius: f64, start: f64, sweep: f64) {
        if !(radius > 0.0 && start.is_finite() && sweep.is_finite()) {
            return;
        }
        let sweep = sweep.clamp(-3600.0, 3600.0).to_radians();
        self.arc_sweep(
            c.0,
            c.1,
            radius,
            radius,
            start.to_radians(),
            sweep,
            ArcKind::To,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn arc_sweep(
        &mut self,
        cx: f64,
        cy: f64,
        rx: f64,
        ry: f64,
        a0: f64,
        sweep: f64,
        kind: ArcKind,
    ) {
        let mut curves = Vec::new();
        arc_curves(cx, cy, rx, ry, a0, sweep, &mut curves);
        let first = ellipse_point(cx, cy, rx, ry, a0);
        let last = ellipse_point(cx, cy, rx, ry, a0 + sweep);
        let mut segs = Vec::with_capacity(curves.len() + 3);
        match kind {
            ArcKind::Pie => {
                segs.push(LogicalSeg::Move((cx, cy)));
                segs.push(LogicalSeg::Line(first));
            }
            ArcKind::To => {
                let continuing = self.path.as_ref().is_some_and(|p| !p.is_empty());
                if !continuing {
                    segs.push(LogicalSeg::Move(self.state.position));
                }
                segs.push(LogicalSeg::Line(first));
            }
            ArcKind::Open | ArcKind::Chord => segs.push(LogicalSeg::Move(first)),
        }
        segs.extend(curves.into_iter().map(LogicalSeg::Cubic));
        let closed = matches!(kind, ArcKind::Pie | ArcKind::Chord);
        if closed {
            segs.push(LogicalSeg::Close);
        }
        if kind == ArcKind::To {
            self.state.position = last;
        }
        self.logical_figure(&segs, closed);
    }

    fn logical_figure(&mut self, segs: &[LogicalSeg], closed: bool) {
        if !self.count(segs.len()) {
            return;
        }
        let m = self.transform();
        let p = |(x, y): (f64, f64)| Self::frame(&m, x, y);
        let mapped: Vec<Seg> = segs
            .iter()
            .map(|s| match *s {
                LogicalSeg::Move(a) => Seg::Move(p(a).0, p(a).1),
                LogicalSeg::Line(a) => Seg::Line(p(a).0, p(a).1),
                LogicalSeg::Cubic([a, b, c]) => {
                    let (a, b, c) = (p(a), p(b), p(c));
                    Seg::Cubic(a.0, a.1, b.0, b.1, c.0, c.1)
                }
                LogicalSeg::Close => Seg::Close,
            })
            .collect();
        self.figure(mapped, closed);
    }

    /// [MS-EMF] §2.3.10: opens a path bracket; figures go into it.
    pub(crate) fn begin_path(&mut self) {
        self.flush();
        self.path = Some(Vec::new());
        self.ended = None;
    }

    pub(crate) fn end_path(&mut self) {
        self.ended = self.path.take();
    }

    pub(crate) fn close_figure(&mut self) {
        if let Some(path) = self.path.as_mut() {
            path.push(Seg::Close);
        }
    }

    pub(crate) fn abort_path(&mut self) {
        self.path = None;
        self.ended = None;
    }

    fn take_path(&mut self) -> Option<Vec<Seg>> {
        self.ended.take().or_else(|| self.path.take())
    }

    /// [MS-EMF] §2.3.5.9, §2.3.5.38 and §2.3.5.39: fills, strokes or both
    /// the bracketed path.
    pub(crate) fn paint_path(&mut self, fill: bool, stroke: bool) {
        let Some(segs) = self.take_path() else {
            return;
        };
        if segs.is_empty() {
            return;
        }
        let fill = fill.then(|| self.fill()).flatten();
        let stroke = stroke.then(|| self.stroke()).flatten();
        if fill.is_none() && stroke.is_none() {
            return;
        }
        self.emit(Op::Path { segs, fill, stroke });
    }

    /// [MS-EMF] §2.3.2.5: clips to the bracketed path, as its bounds.
    pub(crate) fn clip_to_path(&mut self, mode: u32) {
        let Some(segs) = self.take_path() else {
            return;
        };
        let points: Vec<Point> = segs
            .iter()
            .filter_map(|s| match *s {
                Seg::Move(x, y) | Seg::Line(x, y) | Seg::Cubic(_, _, _, _, x, y) => Some((x, y)),
                Seg::Close => None,
            })
            .collect();
        let Some(bounds) = Rect::around(&points) else {
            return;
        };
        let rectangular = points.iter().all(|p| {
            [bounds.x, bounds.right()].contains(&p.0) && [bounds.y, bounds.bottom()].contains(&p.1)
        });
        if !rectangular {
            self.note("clip paths are clipped to their bounds");
        }
        self.combine_clip(&[bounds], mode);
    }

    /// Paints a rectangle with the brush under a raster operation without a
    /// source ([MS-WMF] §2.3.3.12 META_PATBLT, §2.1.1.31).
    pub(crate) fn pat_blt(&mut self, l: f64, t: f64, w: f64, h: f64, rop: u32) {
        let color = match rop {
            PATCOPY => match self.fill() {
                Some(fill) => fill.color,
                None => return,
            },
            BLACKNESS => Color::BLACK,
            WHITENESS => Color::WHITE,
            DSTCOPY => return,
            PATAND => {
                self.note("raster operation 0xA000C9 is drawn as a copy of the brush");
                match self.fill() {
                    Some(fill) => fill.color,
                    None => return,
                }
            }
            _ => {
                self.note(format!(
                    "raster operation {rop:#08X} without a source is not drawn"
                ));
                return;
            }
        };
        let m = self.transform();
        let corners =
            [(l, t), (l + w, t), (l + w, t + h), (l, t + h)].map(|(x, y)| Self::frame(&m, x, y));
        let segs = Self::polyline_segs(&corners, true);
        self.emit(Op::Path {
            segs,
            fill: Some(Fill {
                color,
                even_odd: false,
            }),
            stroke: None,
        });
    }

    /// Draws `image`'s `source` rectangle (pixels from the top-left) into
    /// the logical rectangle at `(x, y)` sized `w` by `h`, under `rop`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn bitmap(
        &mut self,
        (x, y, w, h): (f64, f64, f64, f64),
        source: (f64, f64, f64, f64),
        mut image: Rgba,
        rop: u32,
        constant_alpha: u8,
    ) {
        if self.path.is_some() {
            return;
        }
        match rop {
            SRCCOPY => {}
            SRCPAINT | SRCINVERT => key(&mut image, [0, 0, 0]),
            SRCAND => key(&mut image, [255, 255, 255]),
            NOTSRCCOPY => image
                .pixels
                .as_chunks_mut::<4>()
                .0
                .iter_mut()
                .for_each(|p| {
                    p[0] = 255 - p[0];
                    p[1] = 255 - p[1];
                    p[2] = 255 - p[2];
                }),
            _ => self.note(format!("raster operation {rop:#08X} is drawn as a copy")),
        }
        if rop == SRCINVERT {
            self.note("xor raster operations are drawn with black transparent");
        }
        if constant_alpha < 255 {
            image
                .pixels
                .as_chunks_mut::<4>()
                .0
                .iter_mut()
                .for_each(|p| {
                    p[3] = (u16::from(p[3]) * u16::from(constant_alpha) / 255) as u8;
                });
        }
        let m = self.transform();
        let a = Self::frame(&m, x, y);
        let b = Self::frame(&m, x + w, y + h);
        if a.0 > b.0 {
            mirror(&mut image, true);
        }
        if a.1 > b.1 {
            mirror(&mut image, false);
        }
        let (sx, sy, sw, sh) = source;
        let (iw, ih) = (f64::from(image.width), f64::from(image.height));
        let source = (
            if a.0 > b.0 { iw - sx - sw } else { sx },
            if a.1 > b.1 { ih - sy - sh } else { sy },
            sw,
            sh,
        );
        let dest = Rect::around(&[a, b]).unwrap_or(Rect {
            x: a.0,
            y: a.1,
            width: 0.0,
            height: 0.0,
        });
        if dest.width <= 0.0 || dest.height <= 0.0 || source.2 <= 0.0 || source.3 <= 0.0 {
            return;
        }
        let size = (image.width, image.height);
        let bmp: Arc<[u8]> = encode_bmp(&image).into();
        self.emit(Op::Bitmap(Bitmap {
            bmp,
            size,
            dest,
            source: (
                source.0 as f32,
                source.1 as f32,
                source.2 as f32,
                source.3 as f32,
            ),
        }));
    }

    /// [MS-EMF] §2.3.5.8, [MS-WMF] §2.3.3.5: a line of text at the logical
    /// point `(x, y)`, or at the current position under TA_UPDATECP, with
    /// optional advances in logical units, an opaque rectangle and a clip
    /// rectangle.
    pub(crate) fn text(
        &mut self,
        (x, y): (f64, f64),
        text: String,
        advances: Option<Vec<f64>>,
        opaque: Option<Rect>,
        clip: Option<Rect>,
    ) {
        if let Some(rect) = opaque {
            let color = self.state.bk_color;
            let corners = [
                (rect.x, rect.y),
                (rect.right(), rect.y),
                (rect.right(), rect.bottom()),
                (rect.x, rect.bottom()),
            ];
            self.emit(Op::Path {
                segs: Self::polyline_segs(&corners, true),
                fill: Some(Fill {
                    color,
                    even_odd: false,
                }),
                stroke: None,
            });
        }
        if text.is_empty() {
            return;
        }
        if self.path.is_some() {
            self.note("text inside path brackets is not drawn");
            return;
        }
        const TA_UPDATECP: u32 = 1;
        let update = self.state.align & TA_UPDATECP != 0;
        let origin = if update { self.state.position } else { (x, y) };
        let m = self.transform();
        let at = Self::frame(&m, origin.0, origin.1);
        let x_scale = m[0].hypot(m[1]);
        let y_scale = m[2].hypot(m[3]);
        let font = &self.state.font;
        let size = match font.height {
            0.0 => 12.0,
            h => (h.abs() * y_scale) as f32,
        };
        let world = &self.state.world;
        let world_angle = (-world[1]).atan2(world[0]);
        let angle = (font.escapement / 10.0).to_radians() + world_angle;
        let sum: f64 = advances.as_ref().map_or(0.0, |a| a.iter().sum());
        let advances = advances.map(|a| a.iter().map(|v| (v * x_scale) as f32).collect());
        let anchor = match self.state.align & 6 {
            2 => Anchor::Right,
            6 => Anchor::Center,
            _ => Anchor::Left,
        };
        let baseline = match self.state.align & 24 {
            24 => Baseline::Baseline,
            8 => Baseline::Bottom,
            _ => Baseline::Top,
        };
        let op = Op::Text(Text {
            at,
            text,
            advances,
            font: Font {
                face: font.face.clone(),
                size,
                cell: font.height > 0.0,
                bold: font.weight >= 600,
                italic: font.italic,
                underline: font.underline,
                strike: font.strike,
            },
            color: self.state.text_color,
            background: self.state.opaque.then_some(self.state.bk_color),
            anchor,
            baseline,
            angle: angle as f32,
        });
        let outer = self.state.clip;
        if let Some(clip) = clip {
            self.intersect_clip(clip);
        }
        self.emit(op);
        self.set_clip(outer);
        if update {
            let (dx, dy) = (sum * angle.cos(), -sum * angle.sin());
            self.state.position = (origin.0 + dx, origin.1 + dy);
        }
    }
}

/// How an arc closes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArcKind {
    Open,
    Chord,
    Pie,
    To,
}

#[derive(Debug, Clone, Copy)]
enum LogicalSeg {
    Move((f64, f64)),
    Line((f64, f64)),
    Cubic([(f64, f64); 3]),
    Close,
}

fn ellipse_point(cx: f64, cy: f64, rx: f64, ry: f64, a: f64) -> (f64, f64) {
    (cx + rx * a.cos(), cy - ry * a.sin())
}

/// The cubic Béziers of an elliptical arc from angle `a0` through `sweep`,
/// in pieces of at most a quarter turn.
fn arc_curves(
    cx: f64,
    cy: f64,
    rx: f64,
    ry: f64,
    a0: f64,
    sweep: f64,
    out: &mut Vec<[(f64, f64); 3]>,
) {
    let pieces = (sweep.abs() / std::f64::consts::FRAC_PI_2)
        .ceil()
        .clamp(1.0, 64.0) as usize;
    let delta = sweep / pieces as f64;
    let k = 4.0 / 3.0 * (delta / 4.0).tan();
    for i in 0..pieces {
        let (s, e) = (a0 + delta * i as f64, a0 + delta * (i + 1) as f64);
        let (p0, p3) = (
            ellipse_point(cx, cy, rx, ry, s),
            ellipse_point(cx, cy, rx, ry, e),
        );
        let d0 = (-rx * s.sin(), -ry * s.cos());
        let d1 = (-rx * e.sin(), -ry * e.cos());
        out.push([
            (p0.0 + k * d0.0, p0.1 + k * d0.1),
            (p3.0 - k * d1.0, p3.1 - k * d1.1),
            p3,
        ]);
    }
}

/// `color` laid over `behind` at `amount`.
fn tint(color: Color, behind: Color, amount: f32) -> Color {
    let mix = |a: u8, b: u8| (f32::from(a) * amount + f32::from(b) * (1.0 - amount)).round() as u8;
    Color(
        mix(color.0, behind.0),
        mix(color.1, behind.1),
        mix(color.2, behind.2),
    )
}

/// Makes the pixels of one color transparent.
fn key(image: &mut Rgba, rgb: [u8; 3]) {
    image
        .pixels
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .filter(|p| p[..3] == rgb)
        .for_each(|p| p[3] = 0);
}

fn mirror(image: &mut Rgba, horizontal: bool) {
    let (w, h) = (image.width as usize, image.height as usize);
    if horizontal {
        for row in image.pixels.chunks_exact_mut(w * 4) {
            for x in 0..w / 2 {
                for c in 0..4 {
                    row.swap(x * 4 + c, (w - 1 - x) * 4 + c);
                }
            }
        }
        return;
    }
    for y in 0..h / 2 {
        let (top, bottom) = image.pixels.split_at_mut((h - 1 - y) * w * 4);
        top[y * w * 4..(y + 1) * w * 4].swap_with_slice(&mut bottom[..w * 4]);
    }
}

/// The average color of a bitmap, for pattern brushes.
pub(crate) fn average(image: &Rgba) -> Color {
    let n = (image.pixels.len() / 4).max(1) as u64;
    let sum = image
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .fold([0u64; 3], |mut s, p| {
            s[0] += u64::from(p[0]);
            s[1] += u64::from(p[1]);
            s[2] += u64::from(p[2]);
            s
        });
    Color((sum[0] / n) as u8, (sum[1] / n) as u8, (sum[2] / n) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dc() -> Dc {
        Dc::new(crate::emf::initial_state(), IDENTITY, (1.0, 1.0), false)
    }

    /// [MS-EMF] §2.3.12.1 EMR_MODIFYWORLDTRANSFORM: mode 2 applies the new
    /// transform before the current one, mode 3 after it, mode 1 resets and
    /// §2.3.12.2 EMR_SETWORLDTRANSFORM replaces it.
    #[test]
    fn world_transform_modes_compose_in_order() {
        let scale = [2.0, 0.0, 0.0, 2.0, 0.0, 0.0];
        let shift = [1.0, 0.0, 0.0, 1.0, 10.0, 0.0];
        let mut d = dc();
        d.world(scale, 4);
        d.world(shift, 2);
        assert_eq!(apply(&d.transform(), 0.0, 0.0), (20.0, 0.0));
        d.world(scale, 4);
        d.world(shift, 3);
        assert_eq!(apply(&d.transform(), 0.0, 0.0), (10.0, 0.0));
        d.world(shift, 1);
        assert_eq!(apply(&d.transform(), 3.0, 4.0), (3.0, 4.0));
    }

    /// [MS-EMF] §2.1.21 MapMode: the anisotropic mode maps the window onto
    /// the viewport, and the isotropic mode keeps the aspect ratio.
    #[test]
    fn anisotropic_and_isotropic_windows_map_onto_the_viewport() {
        let mut d = dc();
        d.set_map_mode(MM_ANISOTROPIC);
        d.set_window_ext(100.0, 50.0);
        d.set_viewport_ext(200.0, 200.0);
        assert_eq!(apply(&d.transform(), 100.0, 50.0), (200.0, 200.0));
        d.set_map_mode(7);
        assert_eq!(apply(&d.transform(), 100.0, 50.0), (200.0, 100.0));
    }

    /// [MS-EMF] §2.3.11.6 EMR_RESTOREDC: a negative count goes back that many
    /// saves.
    #[test]
    fn restore_goes_back_through_saved_states() {
        let mut d = dc();
        d.state.text_color = Color(1, 0, 0);
        d.save();
        d.state.text_color = Color(2, 0, 0);
        d.save();
        d.state.text_color = Color(3, 0, 0);
        d.restore(-2);
        assert_eq!(d.state.text_color, Color(1, 0, 0));
        d.restore(-1);
        assert_eq!(d.state.text_color, Color(1, 0, 0));
    }
}

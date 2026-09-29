//! Painting layout items onto a pixmap.

use std::collections::HashMap;
use std::sync::Arc;

use docboss_font::{FontId, Seg};
use docboss_layout::{GlyphRun, Item, Layout, LineStyle, Rect, Stroke};
use docboss_model::{Color, DashPattern, Diagnostic, Gradient, LineCap, LineJoin, MediaId};

use crate::image::{decode, Decoded, ImageError};
use crate::raster::{flatten, polylines, rasterize, Mask, Point, Polygons};
use crate::shade::{apply, compose, invert, Shader};
use crate::{Error, Pixmap, Result};

const SUBPIXEL: f32 = 4.0;
const SLANT: f32 = 0.21;
/// The most dashes one line is cut into.
const MAX_DASHES: usize = 100_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct GlyphKey {
    font: FontId,
    glyph: u16,
    size: u32,
    phase_x: u8,
    phase_y: u8,
    slanted: bool,
}

/// Reusable caches for painting pages: glyph coverage masks and decoded
/// images. One renderer serves any number of pages of one layout.
#[derive(Debug, Default)]
pub struct Renderer {
    glyphs: HashMap<GlyphKey, Arc<Mask>>,
    images: HashMap<MediaId, Arc<std::result::Result<Decoded, ImageError>>>,
}

impl Renderer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Paints page `index` at `scale` pixels per point.
    pub fn render(&mut self, layout: &Layout, index: usize, scale: f32) -> Result<Pixmap> {
        let page = layout.pages.get(index).ok_or(Error::NoSuchPage(index))?;
        let scale = if scale.is_finite() && scale > 0.0 {
            scale
        } else {
            1.0
        };
        let width = (page.width * scale).ceil().max(1.0) as u32;
        let height = (page.height * scale).ceil().max(1.0) as u32;
        let mut pixmap = Pixmap::new(width, height)?;
        let full = Clip::of(&pixmap);
        let mut clip = full;
        let mut outer: Vec<Clip> = Vec::new();
        for item in &page.items {
            let mut canvas = Canvas {
                pixmap: &mut pixmap,
                clip,
            };
            match item {
                Item::Glyphs(run) => self.glyphs(layout, &mut canvas, run, scale),
                Item::Rect { rect, color } => canvas.fill_rect(scaled(*rect, scale), *color),
                Item::Line {
                    from,
                    to,
                    width,
                    color,
                    style,
                    cap,
                } => {
                    let p = (from.0 * scale, from.1 * scale);
                    let q = (to.0 * scale, to.1 * scale);
                    canvas.line(p, q, (width * scale).max(0.5), *color, *style, *cap);
                }
                Item::Image { media, rect } => {
                    self.image(layout, &mut canvas, *media, scaled(*rect, scale))
                }
                Item::Outline {
                    rect,
                    width,
                    color,
                    style,
                    cap,
                    join,
                } => canvas.outline(
                    scaled(*rect, scale),
                    (width * scale).max(0.5),
                    *color,
                    *style,
                    *cap,
                    *join,
                ),
                Item::Path { segs, fill, stroke } => {
                    canvas.path(segs, *fill, stroke.as_ref(), scale)
                }
                Item::Shade {
                    segs,
                    gradient,
                    frame,
                    size,
                } => canvas.shade(segs, gradient, *frame, *size, scale),
                Item::ClipBegin(rect) => {
                    outer.push(clip);
                    clip = clip.intersect(scaled(*rect, scale));
                }
                Item::ClipEnd => clip = outer.pop().unwrap_or(full),
            }
        }
        Ok(pixmap)
    }

    fn glyphs(&mut self, layout: &Layout, canvas: &mut Canvas<'_>, run: &GlyphRun, scale: f32) {
        let Some(font) = layout.fonts.font(run.font) else {
            return;
        };
        let size_px = run.size * scale;
        if !(0.5..=4000.0).contains(&size_px) {
            return;
        }
        let em = f32::from(font.units_per_em());
        let k = size_px / em;
        let baseline = run.baseline * scale;
        let embolden = if run.synthetic_bold {
            (size_px / 36.0).max(0.6)
        } else {
            0.0
        };
        for glyph in &run.glyphs {
            let x = glyph.x * scale;
            let (ix, fx) = (x.floor(), x - x.floor());
            let (iy, fy) = (baseline.floor(), baseline - baseline.floor());
            let key = GlyphKey {
                font: run.font,
                glyph: glyph.id,
                size: (size_px * 64.0) as u32,
                phase_x: (fx * SUBPIXEL) as u8,
                phase_y: (fy * SUBPIXEL) as u8,
                slanted: run.synthetic_italic,
            };
            let mask = self
                .glyphs
                .entry(key)
                .or_insert_with(|| {
                    let outline = font.outline(glyph.id);
                    let shear = if run.synthetic_italic { SLANT * k } else { 0.0 };
                    let dx = f32::from(key.phase_x) / SUBPIXEL;
                    let dy = f32::from(key.phase_y) / SUBPIXEL;
                    Arc::new(rasterize(
                        &flatten(&outline, [k, 0.0, shear, -k, dx, dy]),
                        None,
                    ))
                })
                .clone();
            canvas.blit(&mask, ix as i32, iy as i32, run.color);
            if embolden > 0.0 {
                canvas.blit(&mask, (ix + embolden).round() as i32, iy as i32, run.color);
            }
        }
    }

    fn image(
        &mut self,
        layout: &Layout,
        canvas: &mut Canvas<'_>,
        media: Option<MediaId>,
        rect: Rect,
    ) {
        let decoded = media.and_then(|id| {
            let data = layout.media.get(id.0 as usize)?;
            Some(
                self.images
                    .entry(id)
                    .or_insert_with(|| Arc::new(decode(&data.data)))
                    .clone(),
            )
        });
        let Some(result) = decoded else {
            canvas.placeholder(rect);
            return;
        };
        match result.as_ref() {
            Ok(image) => canvas.draw_image(image, rect),
            Err(error) => {
                let name = media
                    .and_then(|id| layout.media.get(id.0 as usize))
                    .map_or("image", |m| m.name.as_str());
                canvas.pixmap.diagnostics.push(Diagnostic::approximated(
                    name,
                    format!("{error}; drawn as a placeholder"),
                ));
                canvas.placeholder(rect);
            }
        }
    }
}

fn scaled(rect: Rect, scale: f32) -> Rect {
    Rect::new(
        rect.x * scale,
        rect.y * scale,
        rect.width * scale,
        rect.height * scale,
    )
}

#[inline]
fn blend(dst: &mut [u8], color: Color, alpha: u32) {
    if alpha == 0 {
        return;
    }
    if alpha >= 255 {
        dst[..3].copy_from_slice(&[color.0, color.1, color.2]);
        return;
    }
    let inv = 255 - alpha;
    dst[0] = ((u32::from(color.0) * alpha + u32::from(dst[0]) * inv + 127) / 255) as u8;
    dst[1] = ((u32::from(color.1) * alpha + u32::from(dst[1]) * inv + 127) / 255) as u8;
    dst[2] = ((u32::from(color.2) * alpha + u32::from(dst[2]) * inv + 127) / 255) as u8;
}

/// The unit vector from `p` to `q`, or `(1, 0)` when they coincide.
fn direction(p: (f32, f32), q: (f32, f32)) -> (f32, f32) {
    let (dx, dy) = (q.0 - p.0, q.1 - p.1);
    let length = (dx * dx + dy * dy).sqrt();
    if length <= 0.0 || !length.is_finite() {
        return (1.0, 0.0);
    }
    (dx / length, dy / length)
}

/// The device rectangle, in pixels, that painting is confined to.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Clip {
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
}

impl Clip {
    fn of(pixmap: &Pixmap) -> Clip {
        Clip {
            x0: 0.0,
            y0: 0.0,
            x1: pixmap.width as f32,
            y1: pixmap.height as f32,
        }
    }

    fn intersect(self, rect: Rect) -> Clip {
        Clip {
            x0: self.x0.max(rect.x),
            y0: self.y0.max(rect.y),
            x1: self.x1.min(rect.right()),
            y1: self.y1.min(rect.bottom()),
        }
    }

    /// The clip rounded to whole pixels: left, top, right, bottom.
    fn pixels(self) -> (i32, i32, i32, i32) {
        (
            self.x0.round() as i32,
            self.y0.round() as i32,
            self.x1.round() as i32,
            self.y1.round() as i32,
        )
    }
}

/// A pixmap and the clip the item being painted is confined to.
struct Canvas<'a> {
    pixmap: &'a mut Pixmap,
    clip: Clip,
}

impl Canvas<'_> {
    fn blit(&mut self, mask: &Mask, dx: i32, dy: i32, color: Color) {
        let (cx0, cy0, cx1, cy1) = self.clip.pixels();
        let stride = self.pixmap.width as usize;
        let x0 = mask.x + dx;
        let start = (cx0 - x0).max(0) as usize;
        let end = ((cx1 - x0).max(0) as usize).min(mask.width);
        if start >= end {
            return;
        }
        for row in 0..mask.height as i32 {
            let y = mask.y + row + dy;
            if y < cy0 || y >= cy1 {
                continue;
            }
            let line = &mask.coverage[row as usize * mask.width..(row as usize + 1) * mask.width];
            for (col, &c) in line.iter().enumerate().take(end).skip(start) {
                if c == 0 {
                    continue;
                }
                let at = (y as usize * stride + (x0 + col as i32) as usize) * 4;
                blend(&mut self.pixmap.data[at..at + 4], color, u32::from(c));
            }
        }
    }

    /// Fills an axis-aligned rectangle with exact fractional edge coverage.
    fn fill_rect(&mut self, rect: Rect, color: Color) {
        let x0 = rect.x.max(self.clip.x0);
        let y0 = rect.y.max(self.clip.y0);
        let x1 = rect.right().min(self.clip.x1);
        let y1 = rect.bottom().min(self.clip.y1);
        if !(x1 > x0 && y1 > y0) {
            return;
        }
        let stride = self.pixmap.width as usize;
        for py in y0.floor() as u32..y1.ceil() as u32 {
            let cy = (y1.min(py as f32 + 1.0) - y0.max(py as f32)).clamp(0.0, 1.0);
            for px in x0.floor() as u32..x1.ceil() as u32 {
                let cx = (x1.min(px as f32 + 1.0) - x0.max(px as f32)).clamp(0.0, 1.0);
                let alpha = (cx * cy * 255.0 + 0.5) as u32;
                let at = (py as usize * stride + px as usize) * 4;
                blend(&mut self.pixmap.data[at..at + 4], color, alpha);
            }
        }
    }

    fn fill_polygons(&mut self, polys: &Polygons, color: Color) {
        let mask = rasterize(polys, Some(self.clip.pixels()));
        self.blit(&mask, 0, 0, color);
    }

    /// Fills `segs`, in points, with the gradient laid over the box of
    /// `size` that `frame` maps into the page.
    fn shade(
        &mut self,
        segs: &[Seg],
        gradient: &Gradient,
        frame: [f32; 6],
        size: (f32, f32),
        scale: f32,
    ) {
        let m = [scale, 0.0, 0.0, scale, 0.0, 0.0];
        let Some(back) = invert(compose(m, frame)) else {
            return;
        };
        let mask = rasterize(&flatten(segs, m), Some(self.clip.pixels()));
        let shader = Shader::new(gradient, size);
        let stride = self.pixmap.width as usize;
        for row in 0..mask.height {
            let y = mask.y + row as i32;
            let line = &mask.coverage[row * mask.width..(row + 1) * mask.width];
            for (col, &c) in line.iter().enumerate() {
                if c == 0 {
                    continue;
                }
                let x = mask.x + col as i32;
                let (u, v) = apply(back, x as f32 + 0.5, y as f32 + 0.5);
                let at = (y as usize * stride + x as usize) * 4;
                blend(
                    &mut self.pixmap.data[at..at + 4],
                    shader.color(u, v),
                    u32::from(c),
                );
            }
        }
    }

    fn segment(&mut self, p: (f32, f32), q: (f32, f32), width: f32, color: Color) {
        if p.1 == q.1 {
            let (x0, x1) = (p.0.min(q.0), p.0.max(q.0));
            self.fill_rect(Rect::new(x0, p.1 - width / 2.0, x1 - x0, width), color);
            return;
        }
        if p.0 == q.0 {
            let (y0, y1) = (p.1.min(q.1), p.1.max(q.1));
            self.fill_rect(Rect::new(p.0 - width / 2.0, y0, width, y1 - y0), color);
            return;
        }
        let (dx, dy) = (q.0 - p.0, q.1 - p.1);
        let length = (dx * dx + dy * dy).sqrt();
        let (nx, ny) = (-dy / length * width / 2.0, dx / length * width / 2.0);
        let quad = vec![
            Point {
                x: p.0 + nx,
                y: p.1 + ny,
            },
            Point {
                x: q.0 + nx,
                y: q.1 + ny,
            },
            Point {
                x: q.0 - nx,
                y: q.1 - ny,
            },
            Point {
                x: p.0 - nx,
                y: p.1 - ny,
            },
        ];
        self.fill_polygons(&vec![quad], color);
    }

    fn line(
        &mut self,
        p: (f32, f32),
        q: (f32, f32),
        width: f32,
        color: Color,
        style: LineStyle,
        cap: LineCap,
    ) {
        let (dx, dy) = (q.0 - p.0, q.1 - p.1);
        let length = (dx * dx + dy * dy).sqrt();
        if length <= 0.0 || !length.is_finite() {
            return;
        }
        let (ux, uy) = (dx / length, dy / length);
        let at = |t: f32| (p.0 + ux * t, p.1 + uy * t);
        match style {
            LineStyle::Solid => self.capped(p, q, (ux, uy), width, color, cap),
            LineStyle::Dash(pattern) => {
                self.dashed_path(&[p, q], false, width, color, pattern, cap, None)
            }
            LineStyle::Double => {
                let (nx, ny) = (-uy * width / 3.0, ux * width / 3.0);
                let thin = width / 3.0;
                self.segment((p.0 + nx, p.1 + ny), (q.0 + nx, q.1 + ny), thin, color);
                self.segment((p.0 - nx, p.1 - ny), (q.0 - nx, q.1 - ny), thin, color);
            }
            LineStyle::Wave => {
                let step = (width * 2.0).max(1.0);
                let (nx, ny) = (-uy * width, ux * width);
                let mut t = 0.0;
                let mut up = true;
                while t < length && t < 1e6 {
                    let a = at(t);
                    let b = at((t + step).min(length));
                    let (s0, s1) = if up { (1.0, -1.0) } else { (-1.0, 1.0) };
                    self.segment(
                        (a.0 + nx * s0, a.1 + ny * s0),
                        (b.0 + nx * s1, b.1 + ny * s1),
                        width * 0.7,
                        color,
                    );
                    t += step;
                    up = !up;
                }
            }
        }
    }

    /// Strokes the polyline through `points`, closed back to the first when
    /// `closed`, with the pattern's dashes running on across its corners.
    /// A dash that runs through a corner gets `join` there. A pattern whose
    /// period is under a pixel is drawn solid.
    #[allow(clippy::too_many_arguments)]
    fn dashed_path(
        &mut self,
        points: &[(f32, f32)],
        closed: bool,
        width: f32,
        color: Color,
        pattern: DashPattern,
        cap: LineCap,
        join: Option<LineJoin>,
    ) {
        let unit = width / 100.0;
        let stops = pattern.stops();
        let period: f32 = stops
            .iter()
            .map(|(dash, space)| (f32::from(*dash) + f32::from(*space)) * unit)
            .sum();
        let sides = if closed {
            points.len()
        } else {
            points.len().saturating_sub(1)
        };
        let side = |i: usize| (points[i], points[(i + 1) % points.len()]);
        if period < 1.0 {
            for i in 0..sides {
                let (p, q) = side(i);
                self.line(p, q, width, color, LineStyle::Solid, cap);
            }
            return;
        }
        let mut index = 0;
        let mut on = true;
        let mut left = f32::from(stops[0].0) * unit;
        let mut pieces = 0;
        for i in 0..sides {
            let (p, q) = side(i);
            let (dx, dy) = (q.0 - p.0, q.1 - p.1);
            let length = (dx * dx + dy * dy).sqrt();
            if length <= 0.0 || !length.is_finite() {
                continue;
            }
            let u = (dx / length, dy / length);
            let at = |t: f32| (p.0 + u.0 * t, p.1 + u.1 * t);
            let mut t = 0.0;
            while t < length && pieces < MAX_DASHES {
                let step = left.min(length - t);
                if on {
                    self.capped(at(t), at(t + step), u, width, color, cap);
                }
                t += step;
                left -= step;
                if left > 0.0 {
                    continue;
                }
                pieces += 1;
                if on {
                    on = false;
                    left = f32::from(stops[index].1) * unit;
                    continue;
                }
                index = (index + 1) % stops.len();
                on = true;
                left = f32::from(stops[index].0) * unit;
            }
            let Some(join) = join.filter(|_| on && left > 0.0) else {
                continue;
            };
            let next = side((i + 1) % points.len());
            self.join(q, u, direction(next.0, next.1), width, color, join);
        }
    }

    /// Fills `segs`, in points, with the nonzero rule, then strokes each of
    /// its subpaths: dashed ones through the dash walker, solid ones as one
    /// coverage mask of their pieces, joins and caps.
    fn path(&mut self, segs: &[Seg], fill: Option<Color>, stroke: Option<&Stroke>, scale: f32) {
        let m = [scale, 0.0, 0.0, scale, 0.0, 0.0];
        if let Some(color) = fill {
            self.fill_polygons(&flatten(segs, m), color);
        }
        let Some(stroke) = stroke else {
            return;
        };
        let width = (stroke.width * scale).max(0.5);
        let mut polygons: Polygons = Vec::new();
        for (points, closed) in polylines(segs, m) {
            if let LineStyle::Dash(pattern) = stroke.style {
                let points: Vec<(f32, f32)> = points.iter().map(|p| (p.x, p.y)).collect();
                self.dashed_path(
                    &points,
                    closed,
                    width,
                    stroke.color,
                    pattern,
                    stroke.cap,
                    Some(stroke.join),
                );
                continue;
            }
            stroke_polyline(
                &mut polygons,
                &points,
                closed,
                width,
                stroke.cap,
                stroke.join,
            );
        }
        if !polygons.is_empty() {
            self.fill_polygons(&polygons, stroke.color);
        }
    }

    /// A rectangle's outline, stroked clockwise from the top-left corner.
    fn outline(
        &mut self,
        rect: Rect,
        width: f32,
        color: Color,
        style: LineStyle,
        cap: LineCap,
        join: LineJoin,
    ) {
        let corners = [
            (rect.x, rect.y),
            (rect.right(), rect.y),
            (rect.right(), rect.bottom()),
            (rect.x, rect.bottom()),
        ];
        if let LineStyle::Dash(pattern) = style {
            self.dashed_path(&corners, true, width, color, pattern, cap, Some(join));
            return;
        }
        for i in 0..4 {
            let (p, q) = (corners[i], corners[(i + 1) % 4]);
            self.line(p, q, width, color, style, LineCap::Flat);
        }
        if style != LineStyle::Solid {
            return;
        }
        for i in 0..4 {
            let incoming = direction(corners[(i + 3) % 4], corners[i]);
            let outgoing = direction(corners[i], corners[(i + 1) % 4]);
            self.join(corners[i], incoming, outgoing, width, color, join);
        }
    }

    /// The corner at `c` where a stroke arriving along `a` leaves along `b`.
    fn join(
        &mut self,
        c: (f32, f32),
        a: (f32, f32),
        b: (f32, f32),
        width: f32,
        color: Color,
        join: LineJoin,
    ) {
        let half = width / 2.0;
        let normal = |d: (f32, f32)| (d.1 * half, -d.0 * half);
        let (na, nb) = (normal(a), normal(b));
        let point = |x: f32, y: f32| Point { x, y };
        let polygon = match join {
            LineJoin::Round if half >= 1.0 => return self.capsule(c, c, a, half, color),
            LineJoin::Bevel => vec![
                point(c.0, c.1),
                point(c.0 + na.0, c.1 + na.1),
                point(c.0 + nb.0, c.1 + nb.1),
            ],
            LineJoin::Miter | LineJoin::Round => vec![
                point(c.0 - na.0 - nb.0, c.1 - na.1 - nb.1),
                point(c.0 + na.0 - nb.0, c.1 + na.1 - nb.1),
                point(c.0 + na.0 + nb.0, c.1 + na.1 + nb.1),
                point(c.0 - na.0 + nb.0, c.1 - na.1 + nb.1),
            ],
        };
        self.fill_polygons(&vec![polygon], color);
    }

    /// A straight piece from `p` to `q` along the unit vector `u`, its
    /// ends drawn with `cap`. A round cap under a pixel across is drawn
    /// square.
    fn capped(
        &mut self,
        p: (f32, f32),
        q: (f32, f32),
        u: (f32, f32),
        width: f32,
        color: Color,
        cap: LineCap,
    ) {
        let half = width / 2.0;
        match cap {
            LineCap::Flat if p != q => self.segment(p, q, width, color),
            LineCap::Flat => {}
            LineCap::Round if half >= 1.0 => self.capsule(p, q, u, half, color),
            LineCap::Square | LineCap::Round => {
                let (ex, ey) = (u.0 * half, u.1 * half);
                self.segment((p.0 - ex, p.1 - ey), (q.0 + ex, q.1 + ey), width, color);
            }
        }
    }

    /// The piece from `p` to `q` with a half disc of `radius` on each end.
    fn capsule(&mut self, p: (f32, f32), q: (f32, f32), u: (f32, f32), radius: f32, color: Color) {
        let steps = ((radius * 1.5).ceil() as usize).clamp(4, 32);
        let base = u.1.atan2(u.0) - std::f32::consts::FRAC_PI_2;
        let arc = |center: (f32, f32), from: f32| {
            (0..=steps).map(move |i| {
                let angle = from + std::f32::consts::PI * i as f32 / steps as f32;
                Point {
                    x: center.0 + radius * angle.cos(),
                    y: center.1 + radius * angle.sin(),
                }
            })
        };
        let outline: Vec<Point> = arc(q, base)
            .chain(arc(p, base + std::f32::consts::PI))
            .collect();
        self.fill_polygons(&vec![outline], color);
    }

    fn placeholder(&mut self, rect: Rect) {
        let grey = Color(0xC0, 0xC0, 0xC0);
        self.fill_rect(rect, Color(0xF0, 0xF0, 0xF0));
        let (x0, y0, x1, y1) = (rect.x, rect.y, rect.right(), rect.bottom());
        for (p, q) in [
            ((x0, y0), (x1, y0)),
            ((x1, y0), (x1, y1)),
            ((x1, y1), (x0, y1)),
            ((x0, y1), (x0, y0)),
            ((x0, y0), (x1, y1)),
            ((x0, y1), (x1, y0)),
        ] {
            self.segment(p, q, 1.0, grey);
        }
    }

    fn draw_image(&mut self, image: &Decoded, rect: Rect) {
        if rect.width <= 0.0 || rect.height <= 0.0 {
            return;
        }
        let reduced = reduce(image, rect.width, rect.height);
        let src = reduced.as_ref().unwrap_or(image);
        let (sw, sh) = (src.width as usize, src.height as usize);
        let (cx0, cy0, cx1, cy1) = self.clip.pixels();
        let x0 = (rect.x.floor() as i32).max(cx0).max(0) as u32;
        let y0 = (rect.y.floor() as i32).max(cy0).max(0) as u32;
        let x1 = (rect.right().ceil() as i32).min(cx1).max(0) as u32;
        let y1 = (rect.bottom().ceil() as i32).min(cy1).max(0) as u32;
        let stride = self.pixmap.width as usize;
        let sx = sw as f32 / rect.width;
        let sy = sh as f32 / rect.height;
        let sample = |x: usize, y: usize, c: usize| {
            f32::from(src.rgba[(y.min(sh - 1) * sw + x.min(sw - 1)) * 4 + c])
        };
        for py in y0..y1 {
            let v = ((py as f32 + 0.5 - rect.y) * sy - 0.5).max(0.0);
            let (vy, fy) = (v.floor() as usize, v - v.floor());
            for px in x0..x1 {
                let u = ((px as f32 + 0.5 - rect.x) * sx - 0.5).max(0.0);
                let (ux, fx) = (u.floor() as usize, u - u.floor());
                let mut rgba = [0f32; 4];
                for (c, slot) in rgba.iter_mut().enumerate() {
                    let top = sample(ux, vy, c) * (1.0 - fx) + sample(ux + 1, vy, c) * fx;
                    let bottom =
                        sample(ux, vy + 1, c) * (1.0 - fx) + sample(ux + 1, vy + 1, c) * fx;
                    *slot = top * (1.0 - fy) + bottom * fy;
                }
                let at = (py as usize * stride + px as usize) * 4;
                let color = Color(rgba[0] as u8, rgba[1] as u8, rgba[2] as u8);
                blend(&mut self.pixmap.data[at..at + 4], color, rgba[3] as u32);
            }
        }
    }
}

/// The turn under which a solid stroke's vertex gets no join, in radians:
/// a flattened curve's gaps there are well under a pixel.
const MIN_JOIN_TURN: f32 = 0.035;

/// Adds the polygons of a solid stroke along `points`, `width` pixels
/// wide, closed back to the first point when `closed`: a quad per piece,
/// `join` at each corner that turns and `cap` at each end of an open one.
/// Every polygon winds the same way, so the nonzero rule unites them.
fn stroke_polyline(
    out: &mut Polygons,
    points: &[Point],
    closed: bool,
    width: f32,
    cap: LineCap,
    join: LineJoin,
) {
    let mut points: Vec<(f32, f32)> = points.iter().map(|p| (p.x, p.y)).collect();
    points.dedup_by(|a, b| (a.0 - b.0).abs() < 1e-3 && (a.1 - b.1).abs() < 1e-3);
    if closed && points.len() > 2 && points.first() == points.last() {
        points.pop();
    }
    let n = points.len();
    if n < 2 {
        return;
    }
    let half = width / 2.0;
    let pieces = if closed { n } else { n - 1 };
    let point = |x: f32, y: f32| Point { x, y };
    for i in 0..pieces {
        let (p, q) = (points[i], points[(i + 1) % n]);
        let u = direction(p, q);
        let (nx, ny) = (-u.1 * half, u.0 * half);
        push_wound(
            out,
            vec![
                point(p.0 + nx, p.1 + ny),
                point(q.0 + nx, q.1 + ny),
                point(q.0 - nx, q.1 - ny),
                point(p.0 - nx, p.1 - ny),
            ],
        );
    }
    let corners = if closed { 0..n } else { 1..n - 1 };
    for i in corners {
        let before = points[(i + n - 1) % n];
        let (c, after) = (points[i], points[(i + 1) % n]);
        let (a, b) = (direction(before, c), direction(c, after));
        let turn = (a.0 * b.1 - a.1 * b.0).atan2(a.0 * b.0 + a.1 * b.1).abs();
        if turn < MIN_JOIN_TURN {
            continue;
        }
        push_wound(out, join_polygon(c, a, b, half, join));
    }
    if closed {
        return;
    }
    let ends = [
        (points[0], direction(points[1], points[0])),
        (points[n - 1], direction(points[n - 2], points[n - 1])),
    ];
    for (end, outward) in ends {
        match cap {
            LineCap::Flat => {}
            LineCap::Round => push_wound(out, disc(end, half)),
            LineCap::Square => {
                let (nx, ny) = (-outward.1 * half, outward.0 * half);
                let (ex, ey) = (end.0 + outward.0 * half, end.1 + outward.1 * half);
                push_wound(
                    out,
                    vec![
                        point(end.0 + nx, end.1 + ny),
                        point(ex + nx, ey + ny),
                        point(ex - nx, ey - ny),
                        point(end.0 - nx, end.1 - ny),
                    ],
                );
            }
        }
    }
}

/// Adds a polygon wound counterclockwise on screen, reversing it if needed.
fn push_wound(out: &mut Polygons, mut polygon: Vec<Point>) {
    let area: f32 = (0..polygon.len())
        .map(|i| {
            let (p, q) = (polygon[i], polygon[(i + 1) % polygon.len()]);
            p.x * q.y - q.x * p.y
        })
        .sum();
    if area < 0.0 {
        polygon.reverse();
    }
    out.push(polygon);
}

/// The corner at `c` where a stroke `half` wide arriving along `a` leaves
/// along `b`.
fn join_polygon(
    c: (f32, f32),
    a: (f32, f32),
    b: (f32, f32),
    half: f32,
    join: LineJoin,
) -> Vec<Point> {
    if join == LineJoin::Round && half >= 1.0 {
        return disc(c, half);
    }
    let cross = a.0 * b.1 - a.1 * b.0;
    let side = if cross > 0.0 { half } else { -half };
    let pa = (c.0 + a.1 * side, c.1 - a.0 * side);
    let pb = (c.0 + b.1 * side, c.1 - b.0 * side);
    let point = |p: (f32, f32)| Point { x: p.0, y: p.1 };
    let bevel = vec![point(c), point(pa), point(pb)];
    let sharp = a.0 * b.0 + a.1 * b.1 < -0.75;
    if join == LineJoin::Bevel || sharp || cross.abs() < 1e-4 {
        return bevel;
    }
    let t = ((pb.0 - pa.0) * b.1 - (pb.1 - pa.1) * b.0) / cross;
    let tip = (pa.0 + a.0 * t, pa.1 + a.1 * t);
    vec![point(c), point(pa), point(tip), point(pb)]
}

/// A disc of `radius` about `c`.
fn disc(c: (f32, f32), radius: f32) -> Vec<Point> {
    let steps = ((radius * 3.0).ceil() as usize).clamp(8, 64);
    (0..steps)
        .map(|i| {
            let angle = std::f32::consts::TAU * i as f32 / steps as f32;
            Point {
                x: c.0 + radius * angle.cos(),
                y: c.1 + radius * angle.sin(),
            }
        })
        .collect()
}

/// Halves an image until it is at most twice the target size, averaging
/// 2x2 blocks, so the bilinear pass that follows does not alias.
fn reduce(image: &Decoded, target_w: f32, target_h: f32) -> Option<Decoded> {
    let mut current: Option<Decoded> = None;
    loop {
        let src = current.as_ref().unwrap_or(image);
        if (src.width as f32) < target_w * 2.0
            || (src.height as f32) < target_h * 2.0
            || src.width < 2
            || src.height < 2
        {
            return current;
        }
        let (w, h) = (src.width / 2, src.height / 2);
        let mut rgba = vec![0u8; w as usize * h as usize * 4];
        let sw = src.width as usize;
        for y in 0..h as usize {
            for x in 0..w as usize {
                for c in 0..4 {
                    let sum: u32 = [(0, 0), (1, 0), (0, 1), (1, 1)]
                        .iter()
                        .map(|(ox, oy)| {
                            u32::from(src.rgba[((y * 2 + oy) * sw + x * 2 + ox) * 4 + c])
                        })
                        .sum();
                    rgba[(y * w as usize + x) * 4 + c] = ((sum + 2) / 4) as u8;
                }
            }
        }
        current = Some(Decoded {
            width: w,
            height: h,
            rgba,
        });
    }
}

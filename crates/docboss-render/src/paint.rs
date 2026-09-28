//! Painting layout items onto a pixmap.

use std::collections::HashMap;
use std::sync::Arc;

use docboss_font::FontId;
use docboss_layout::{GlyphRun, Item, Layout, LineStyle, Rect};
use docboss_model::{Color, Diagnostic, MediaId};

use crate::image::{decode, Decoded, ImageError};
use crate::raster::{flatten, rasterize, Mask, Point, Polygons};
use crate::{Error, Pixmap, Result};

const SUBPIXEL: f32 = 4.0;
const SLANT: f32 = 0.21;

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
        for item in &page.items {
            match item {
                Item::Glyphs(run) => self.glyphs(layout, &mut pixmap, run, scale),
                Item::Rect { rect, color } => fill_rect(&mut pixmap, scaled(*rect, scale), *color),
                Item::Line {
                    from,
                    to,
                    width,
                    color,
                    style,
                } => {
                    let p = (from.0 * scale, from.1 * scale);
                    let q = (to.0 * scale, to.1 * scale);
                    line(&mut pixmap, p, q, (width * scale).max(0.5), *color, *style);
                }
                Item::Image { media, rect } => {
                    self.image(layout, &mut pixmap, *media, scaled(*rect, scale))
                }
            }
        }
        Ok(pixmap)
    }

    fn glyphs(&mut self, layout: &Layout, pixmap: &mut Pixmap, run: &GlyphRun, scale: f32) {
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
            blit(pixmap, &mask, ix as i32, iy as i32, run.color);
            if embolden > 0.0 {
                blit(
                    pixmap,
                    &mask,
                    (ix + embolden).round() as i32,
                    iy as i32,
                    run.color,
                );
            }
        }
    }

    fn image(&mut self, layout: &Layout, pixmap: &mut Pixmap, media: Option<MediaId>, rect: Rect) {
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
            placeholder(pixmap, rect);
            return;
        };
        match result.as_ref() {
            Ok(image) => draw_image(pixmap, image, rect),
            Err(error) => {
                let name = media
                    .and_then(|id| layout.media.get(id.0 as usize))
                    .map_or("image", |m| m.name.as_str());
                pixmap.diagnostics.push(Diagnostic::approximated(
                    name,
                    format!("{error}; drawn as a placeholder"),
                ));
                placeholder(pixmap, rect);
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

fn blit(pixmap: &mut Pixmap, mask: &Mask, dx: i32, dy: i32, color: Color) {
    let (w, h) = (pixmap.width as i32, pixmap.height as i32);
    for row in 0..mask.height as i32 {
        let y = mask.y + row + dy;
        if y < 0 || y >= h {
            continue;
        }
        let line = &mask.coverage[row as usize * mask.width..(row as usize + 1) * mask.width];
        let x0 = mask.x + dx;
        let start = (-x0).max(0) as usize;
        let end = ((w - x0).max(0) as usize).min(mask.width);
        for (col, &c) in line.iter().enumerate().take(end).skip(start) {
            if c == 0 {
                continue;
            }
            let at = (y as usize * pixmap.width as usize + (x0 + col as i32) as usize) * 4;
            blend(&mut pixmap.data[at..at + 4], color, u32::from(c));
        }
    }
}

/// Fills an axis-aligned rectangle with exact fractional edge coverage.
fn fill_rect(pixmap: &mut Pixmap, rect: Rect, color: Color) {
    let (w, h) = (pixmap.width as f32, pixmap.height as f32);
    let x0 = rect.x.max(0.0);
    let y0 = rect.y.max(0.0);
    let x1 = rect.right().min(w);
    let y1 = rect.bottom().min(h);
    if !(x1 > x0 && y1 > y0) {
        return;
    }
    for py in y0.floor() as u32..y1.ceil() as u32 {
        let cy = (y1.min(py as f32 + 1.0) - y0.max(py as f32)).clamp(0.0, 1.0);
        for px in x0.floor() as u32..x1.ceil() as u32 {
            let cx = (x1.min(px as f32 + 1.0) - x0.max(px as f32)).clamp(0.0, 1.0);
            let alpha = (cx * cy * 255.0 + 0.5) as u32;
            let at = (py as usize * pixmap.width as usize + px as usize) * 4;
            blend(&mut pixmap.data[at..at + 4], color, alpha);
        }
    }
}

fn fill_polygons(pixmap: &mut Pixmap, polys: &Polygons, color: Color) {
    let mask = rasterize(polys, Some((pixmap.width as i32, pixmap.height as i32)));
    blit(pixmap, &mask, 0, 0, color);
}

fn segment(pixmap: &mut Pixmap, p: (f32, f32), q: (f32, f32), width: f32, color: Color) {
    if p.1 == q.1 {
        let (x0, x1) = (p.0.min(q.0), p.0.max(q.0));
        fill_rect(
            pixmap,
            Rect::new(x0, p.1 - width / 2.0, x1 - x0, width),
            color,
        );
        return;
    }
    if p.0 == q.0 {
        let (y0, y1) = (p.1.min(q.1), p.1.max(q.1));
        fill_rect(
            pixmap,
            Rect::new(p.0 - width / 2.0, y0, width, y1 - y0),
            color,
        );
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
    fill_polygons(pixmap, &vec![quad], color);
}

fn line(
    pixmap: &mut Pixmap,
    p: (f32, f32),
    q: (f32, f32),
    width: f32,
    color: Color,
    style: LineStyle,
) {
    let (dx, dy) = (q.0 - p.0, q.1 - p.1);
    let length = (dx * dx + dy * dy).sqrt();
    if length <= 0.0 || !length.is_finite() {
        return;
    }
    let (ux, uy) = (dx / length, dy / length);
    let at = |t: f32| (p.0 + ux * t, p.1 + uy * t);
    match style {
        LineStyle::Solid => segment(pixmap, p, q, width, color),
        LineStyle::Double => {
            let (nx, ny) = (-uy * width / 3.0, ux * width / 3.0);
            let thin = width / 3.0;
            segment(
                pixmap,
                (p.0 + nx, p.1 + ny),
                (q.0 + nx, q.1 + ny),
                thin,
                color,
            );
            segment(
                pixmap,
                (p.0 - nx, p.1 - ny),
                (q.0 - nx, q.1 - ny),
                thin,
                color,
            );
        }
        LineStyle::Dotted | LineStyle::Dashed => {
            let (on, off) = if style == LineStyle::Dotted {
                (width, width)
            } else {
                (width * 3.0, width * 2.0)
            };
            let mut t = 0.0;
            while t < length && t < 1e6 {
                segment(pixmap, at(t), at((t + on).min(length)), width, color);
                t += on + off;
            }
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
                segment(
                    pixmap,
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

fn placeholder(pixmap: &mut Pixmap, rect: Rect) {
    let grey = Color(0xC0, 0xC0, 0xC0);
    fill_rect(pixmap, rect, Color(0xF0, 0xF0, 0xF0));
    let (x0, y0, x1, y1) = (rect.x, rect.y, rect.right(), rect.bottom());
    for (p, q) in [
        ((x0, y0), (x1, y0)),
        ((x1, y0), (x1, y1)),
        ((x1, y1), (x0, y1)),
        ((x0, y1), (x0, y0)),
        ((x0, y0), (x1, y1)),
        ((x0, y1), (x1, y0)),
    ] {
        segment(pixmap, p, q, 1.0, grey);
    }
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

fn draw_image(pixmap: &mut Pixmap, image: &Decoded, rect: Rect) {
    if rect.width <= 0.0 || rect.height <= 0.0 {
        return;
    }
    let reduced = reduce(image, rect.width, rect.height);
    let src = reduced.as_ref().unwrap_or(image);
    let (sw, sh) = (src.width as usize, src.height as usize);
    let x0 = rect.x.max(0.0).floor() as u32;
    let y0 = rect.y.max(0.0).floor() as u32;
    let x1 = (rect.right().min(pixmap.width as f32)).ceil().max(0.0) as u32;
    let y1 = (rect.bottom().min(pixmap.height as f32)).ceil().max(0.0) as u32;
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
                let bottom = sample(ux, vy + 1, c) * (1.0 - fx) + sample(ux + 1, vy + 1, c) * fx;
                *slot = top * (1.0 - fy) + bottom * fy;
            }
            let at = (py as usize * pixmap.width as usize + px as usize) * 4;
            let color = Color(rgba[0] as u8, rgba[1] as u8, rgba[2] as u8);
            blend(&mut pixmap.data[at..at + 4], color, rgba[3] as u32);
        }
    }
}

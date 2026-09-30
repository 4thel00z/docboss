//! WMF and EMF pictures ([MS-WMF], [MS-EMF]) laid out as page items: an
//! image whose media is a metafile becomes the paths, text and bitmaps the
//! metafile plays, scaled from its frame into the image's rectangle and
//! clipped to it.

use std::collections::HashMap;
use std::sync::Arc;

use docboss_font::Seg;
use docboss_metafile::{Anchor, Baseline, Op, Picture};
use docboss_model::{Diagnostic, LineCap, Media, MediaId};

use crate::flow::Ctx;
use crate::label;
use crate::{GlyphRun, Item, LineStyle, Page, PositionedGlyph, Rect, Stroke};

/// The width a one-pixel cosmetic pen draws at, in points.
const HAIRLINE: f32 = 0.6;

fn is_metafile(media: &Media) -> bool {
    matches!(media.content_type.as_str(), "image/x-wmf" | "image/x-emf")
        || docboss_metafile::is_metafile(&media.data)
}

/// The metafile media an item draws, and where.
fn target(item: &Item, metafiles: &[bool]) -> Option<(MediaId, Rect)> {
    let (media, rect) = match item {
        Item::Image {
            media: Some(media),
            rect,
        }
        | Item::Picture {
            media: Some(media),
            rect,
            ..
        } => (*media, *rect),
        _ => return None,
    };
    metafiles
        .get(media.0 as usize)
        .copied()
        .unwrap_or(false)
        .then_some((media, rect))
}

/// The pictures played so far and the media their bitmaps became.
struct Played {
    pictures: HashMap<u32, Option<Arc<Picture>>>,
    bitmaps: HashMap<(u32, usize), MediaId>,
    media: Vec<Media>,
    base: usize,
}

/// Replaces every metafile image on `pages` with what the metafile plays,
/// and returns the media its bitmaps add after the document's own.
pub(crate) fn expand(ctx: &mut Ctx<'_>, pages: &mut [Page]) -> Vec<Media> {
    let doc = ctx.doc;
    let metafiles: Vec<bool> = doc.media.iter().map(is_metafile).collect();
    if !metafiles.contains(&true) {
        return Vec::new();
    }
    let mut played = Played {
        pictures: HashMap::new(),
        bitmaps: HashMap::new(),
        media: Vec::new(),
        base: doc.media.len(),
    };
    for page in pages.iter_mut() {
        if !page
            .items
            .iter()
            .any(|item| target(item, &metafiles).is_some())
        {
            continue;
        }
        let items = std::mem::take(&mut page.items);
        let mut out = Vec::with_capacity(items.len());
        for item in items {
            let Some((id, rect)) = target(&item, &metafiles) else {
                out.push(item);
                continue;
            };
            let picture = played
                .pictures
                .entry(id.0)
                .or_insert_with(|| play(ctx, &doc.media[id.0 as usize]))
                .clone();
            let Some(picture) = picture else {
                out.push(item);
                continue;
            };
            draw(ctx, &picture, id, rect, &mut played, &mut out);
        }
        page.items = out;
    }
    played.media
}

fn play(ctx: &mut Ctx<'_>, media: &Media) -> Option<Arc<Picture>> {
    match docboss_metafile::play(&media.data) {
        Ok(picture) => {
            if !picture.notes.is_empty() {
                ctx.diagnostics.push(Diagnostic::approximated(
                    media.name.as_str(),
                    format!("metafile drawn in part: {}", picture.notes.join("; ")),
                ));
            }
            Some(Arc::new(picture))
        }
        Err(e) => {
            ctx.diagnostics.push(Diagnostic::approximated(
                media.name.as_str(),
                format!("{e}; drawn as a placeholder"),
            ));
            None
        }
    }
}

/// The map from a picture's frame into `rect`.
#[derive(Debug, Clone, Copy)]
struct Place {
    x: f32,
    y: f32,
    sx: f32,
    sy: f32,
}

impl Place {
    fn point(&self, (x, y): (f32, f32)) -> (f32, f32) {
        (self.x + x * self.sx, self.y + y * self.sy)
    }

    fn rect(&self, r: docboss_metafile::Rect) -> Rect {
        let (x, y) = self.point((r.x, r.y));
        Rect::new(x, y, r.width * self.sx, r.height * self.sy)
    }

    fn seg(&self, seg: &docboss_metafile::Seg) -> Seg {
        let p = |x: f32, y: f32| self.point((x, y));
        match *seg {
            docboss_metafile::Seg::Move(x, y) => {
                let (x, y) = p(x, y);
                Seg::Move(x, y)
            }
            docboss_metafile::Seg::Line(x, y) => {
                let (x, y) = p(x, y);
                Seg::Line(x, y)
            }
            docboss_metafile::Seg::Cubic(a, b, c, d, e, f) => {
                let ((a, b), (c, d), (e, f)) = (p(a, b), p(c, d), p(e, f));
                Seg::Cubic(a, b, c, d, e, f)
            }
            docboss_metafile::Seg::Close => Seg::Close,
        }
    }
}

fn draw(
    ctx: &mut Ctx<'_>,
    picture: &Picture,
    id: MediaId,
    rect: Rect,
    played: &mut Played,
    out: &mut Vec<Item>,
) {
    if !(picture.width > 0.0 && picture.height > 0.0) {
        return;
    }
    let place = Place {
        x: rect.x,
        y: rect.y,
        sx: rect.width / picture.width,
        sy: rect.height / picture.height,
    };
    let unit = (place.sx * place.sy).abs().sqrt();
    out.push(Item::ClipBegin(rect));
    let mut clipped = false;
    for (index, op) in picture.ops.iter().enumerate() {
        match op {
            Op::Clip(clip) => {
                if clipped {
                    out.push(Item::ClipEnd);
                }
                clipped = clip.is_some();
                if let Some(clip) = clip {
                    out.push(Item::ClipBegin(place.rect(*clip)));
                }
            }
            Op::Path { segs, fill, stroke } => out.push(Item::Path {
                segs: segs.iter().map(|s| place.seg(s)).collect(),
                fill: fill.map(|f| f.color),
                stroke: stroke.map(|pen| Stroke {
                    width: (pen.width * unit).max(HAIRLINE),
                    color: pen.color,
                    style: pen.dash.map_or(LineStyle::Solid, LineStyle::Dash),
                    cap: match pen.width * unit < HAIRLINE {
                        true => LineCap::Flat,
                        false => pen.cap,
                    },
                    join: pen.join,
                }),
                even_odd: fill.is_some_and(|f| f.even_odd),
            }),
            Op::Text(text) => draw_text(ctx, text, &place, out),
            Op::Bitmap(bitmap) => draw_bitmap(bitmap, id, index, &place, played, out),
        }
    }
    if clipped {
        out.push(Item::ClipEnd);
    }
    out.push(Item::ClipEnd);
}

fn draw_bitmap(
    bitmap: &docboss_metafile::Bitmap,
    id: MediaId,
    index: usize,
    place: &Place,
    played: &mut Played,
    out: &mut Vec<Item>,
) {
    let (sx, sy, sw, sh) = bitmap.source;
    let (w, h) = (bitmap.size.0 as f32, bitmap.size.1 as f32);
    if !(sw > 0.0 && sh > 0.0 && w > 0.0 && h > 0.0) {
        return;
    }
    let media = *played.bitmaps.entry((id.0, index)).or_insert_with(|| {
        let media = MediaId((played.base + played.media.len()) as u32);
        played.media.push(Media {
            name: format!("metafile bitmap {}", media.0),
            content_type: "image/bmp".to_string(),
            data: bitmap.bmp.clone(),
        });
        media
    });
    let dest = place.rect(bitmap.dest);
    let (kx, ky) = (dest.width / sw, dest.height / sh);
    let full = Rect::new(dest.x - sx * kx, dest.y - sy * ky, w * kx, h * ky);
    let cropped = sx > 0.0 || sy > 0.0 || sw < w || sh < h;
    if cropped {
        out.push(Item::ClipBegin(dest));
    }
    out.push(Item::Image {
        media: Some(media),
        rect: full,
    });
    if cropped {
        out.push(Item::ClipEnd);
    }
}

fn draw_text(ctx: &mut Ctx<'_>, text: &docboss_metafile::Text, place: &Place, out: &mut Vec<Item>) {
    let font = &text.font;
    let face = Some(font.face.as_str()).filter(|f| !f.is_empty());
    let styled = |size: f32| label::style(face, size, font.bold, font.italic, text.color);
    let mut size = font.size * place.sy.abs();
    if font.cell {
        let (ascent, descent) = label::extent(ctx, &styled(size));
        if ascent + descent > 0.0 {
            size = size * size / (ascent + descent);
        }
    }
    let style = styled(size);
    if style.size < 0.25 {
        return;
    }
    let (ascent, descent) = label::extent(ctx, &style);
    let chars: Vec<char> = text.text.chars().filter(|c| *c != '\0').collect();
    let glyphs: Vec<_> = chars.iter().map(|&c| ctx.shaper.glyph(c, &style)).collect();
    let advances: Vec<f32> = match &text.advances {
        Some(a) if a.len() == glyphs.len() => a.iter().map(|v| v * place.sx.abs()).collect(),
        _ => glyphs.iter().map(|g| g.advance).collect(),
    };
    let width: f32 = advances.iter().sum();
    let (ox, oy) = place.point(text.at);
    let x0 = ox
        - match text.anchor {
            Anchor::Left => 0.0,
            Anchor::Center => width / 2.0,
            Anchor::Right => width,
        };
    let baseline = oy
        + match text.baseline {
            Baseline::Baseline => 0.0,
            Baseline::Top => ascent,
            Baseline::Bottom => -descent,
        };
    let turned = text.angle.abs() > 1e-4;
    if turned {
        let (sin, cos) = text.angle.sin_cos();
        out.push(Item::TransformBegin([
            cos,
            -sin,
            sin,
            cos,
            ox - (cos * ox + sin * oy),
            oy - (-sin * ox + cos * oy),
        ]));
    }
    if let Some(color) = text.background {
        out.push(Item::Rect {
            rect: Rect::new(x0, baseline - ascent, width, ascent + descent),
            color,
        });
    }
    let mut pen = x0;
    for ((c, glyph), advance) in chars.iter().zip(&glyphs).zip(&advances) {
        let (bold, italic) = ctx.shaper.synthetic(glyph.font, &style);
        let positioned = PositionedGlyph {
            id: glyph.id,
            x: pen,
            y: 0.0,
        };
        pen += advance;
        if let Some(Item::Glyphs(run)) = out.last_mut() {
            if run.font == glyph.font
                && run.size == glyph.size
                && run.baseline == baseline
                && run.synthetic_bold == bold
                && run.synthetic_italic == italic
                && run.color == text.color
            {
                run.glyphs.push(positioned);
                run.text.push(*c);
                continue;
            }
        }
        out.push(Item::Glyphs(GlyphRun {
            font: glyph.font,
            size: glyph.size,
            color: text.color,
            baseline,
            glyphs: vec![positioned],
            text: c.to_string(),
            synthetic_bold: bold,
            synthetic_italic: italic,
        }));
    }
    let primary = ctx.shaper.primary(&style);
    let metrics = ctx.shaper.metrics(primary, style.size);
    let lines = [
        (
            font.underline,
            metrics.underline_position,
            metrics.underline_thickness,
        ),
        (
            font.strike,
            -metrics.strikeout_position - metrics.strikeout_thickness / 2.0,
            metrics.strikeout_thickness,
        ),
    ];
    for (on, offset, thickness) in lines {
        if !on {
            continue;
        }
        out.push(Item::Rect {
            rect: Rect::new(x0, baseline + offset, width, thickness),
            color: text.color,
        });
    }
    if turned {
        out.push(Item::TransformEnd);
    }
}

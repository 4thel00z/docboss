//! WMF records ([MS-WMF] §2.3) played through the device context.

use docboss_model::Color;

use crate::bitmap;
use crate::bytes::{i16_at, span, u16_at, u32_at, u8_at};
use crate::dc::{
    average, ArcKind, BrushObject, Dc, FontObject, Object, PenObject, State, BLACK_PEN, IDENTITY,
    MM_ANISOTROPIC, SRCCOPY,
};
use crate::{emf, Error, Kind, Picture, Rect, MAX_RECORDS};

const PLACEABLE_KEY: u32 = 0x9AC6_CDD7;
const PLACEABLE_SIZE: usize = 22;
const META_ESCAPE: u16 = 0x0626;
const MFCOMMENT: u16 = 0x000F;
const WMF_COMMENT: u32 = 0x4346_4D57;
const MAX_EMBEDDED: usize = 64 << 20;

/// Whether `bytes` start with a META_PLACEABLE record ([MS-WMF] §2.3.2.3)
/// or a META_HEADER (§2.3.2.2) of a memory or disk metafile.
pub(crate) fn sniff(bytes: &[u8]) -> bool {
    if u32_at(bytes, 0) == Some(PLACEABLE_KEY) {
        return true;
    }
    matches!(u16_at(bytes, 0), Some(1 | 2)) && u16_at(bytes, 2) == Some(9)
}

fn color(d: &[u8], at: usize) -> Color {
    let b = |k| u8_at(d, at + k).unwrap_or(0);
    Color(b(0), b(1), b(2))
}

/// The picture's bounding box in logical units and the units per inch.
#[derive(Debug, Clone, Copy)]
struct Bounds {
    left: f64,
    top: f64,
    width: f64,
    height: f64,
    inch: f64,
}

/// [MS-WMF] §2.3.2.3 META_PLACEABLE: the bounding box and its units per
/// inch.
fn placeable(bytes: &[u8]) -> Option<Bounds> {
    if u32_at(bytes, 0)? != PLACEABLE_KEY {
        return None;
    }
    let v = |at| f64::from(i16_at(bytes, at).unwrap_or(0));
    let (l, t, r, b) = (v(6), v(8), v(10), v(12));
    let inch = match u16_at(bytes, 14)? {
        0 => 1440.0,
        n => f64::from(n),
    };
    Some(Bounds {
        left: l.min(r),
        top: t.min(b),
        width: (r - l).abs(),
        height: (b - t).abs(),
        inch,
    })
}

/// The records after the header: offset, size in bytes and function.
fn records(bytes: &[u8], start: usize) -> impl Iterator<Item = (usize, usize, u16)> + '_ {
    let mut at = start;
    let mut count = 0usize;
    std::iter::from_fn(move || {
        if count >= MAX_RECORDS {
            return None;
        }
        count += 1;
        let words = u32_at(bytes, at)? as usize;
        let function = u16_at(bytes, at + 4)?;
        let size = words.checked_mul(2)?;
        if size < 6 || at + size > bytes.len() {
            return None;
        }
        let here = at;
        at += size;
        match function {
            0 => None,
            f => Some((here, size, f)),
        }
    })
}

/// A frame for a WMF without a placeable header: its window origin and
/// extent, as twips.
fn window_bounds(bytes: &[u8], start: usize) -> Option<Bounds> {
    let (mut org, mut ext) = (None, None);
    for (at, _, function) in records(bytes, start) {
        let y = f64::from(i16_at(bytes, at + 6).unwrap_or(0));
        let x = f64::from(i16_at(bytes, at + 8).unwrap_or(0));
        match function {
            0x020B if org.is_none() => org = Some((x, y)),
            0x020C if ext.is_none() => ext = Some((x, y)),
            _ => {}
        }
        if ext.is_some() && org.is_some() {
            break;
        }
    }
    let (x, y) = org.unwrap_or((0.0, 0.0));
    let (w, h) = ext?;
    Some(Bounds {
        left: x.min(x + w),
        top: y.min(y + h),
        width: w.abs(),
        height: h.abs(),
        inch: 1440.0,
    })
}

/// [MS-WMF] §2.3.6.25 META_ESCAPE_ENHANCED_METAFILE: the EMF that a
/// sequence of escape records carries, when it is whole.
fn embedded_emf(bytes: &[u8], start: usize) -> Option<Vec<u8>> {
    let mut out: Vec<u8> = Vec::new();
    let mut total = None;
    for (at, size, function) in records(bytes, start) {
        if function != META_ESCAPE || u16_at(bytes, at + 6) != Some(MFCOMMENT) {
            continue;
        }
        if u32_at(bytes, at + 10) != Some(WMF_COMMENT) || u32_at(bytes, at + 14) != Some(1) {
            continue;
        }
        let whole = u32_at(bytes, at + 40)? as usize;
        if whole > MAX_EMBEDDED || total.is_some_and(|t| t != whole) {
            return None;
        }
        total = Some(whole);
        let chunk = (u32_at(bytes, at + 32)? as usize).min(size.saturating_sub(44));
        out.extend_from_slice(span(bytes, at + 44, chunk)?);
    }
    let total = total?;
    (out.len() == total && emf::sniff(&out)).then_some(out)
}

/// The context a WMF plays into: the anisotropic mapping of its window
/// onto the frame, as a placeable metafile is played.
fn initial_state(b: &Bounds) -> State {
    State {
        map_mode: MM_ANISOTROPIC,
        window_org: (b.left, b.top),
        window_ext: (b.width.max(1.0), b.height.max(1.0)),
        viewport_org: (b.left, b.top),
        viewport_ext: (b.width.max(1.0), b.height.max(1.0)),
        world: IDENTITY,
        pen: BLACK_PEN,
        brush: BrushObject::Solid(Color::WHITE),
        font: FontObject::default_face(""),
        text_color: Color::BLACK,
        bk_color: Color::WHITE,
        opaque: true,
        align: 0,
        even_odd: true,
        rop2: 13,
        position: (0.0, 0.0),
        clip: None,
        clockwise: false,
    }
}

/// Plays a WMF, or the EMF its escape records carry.
pub(crate) fn play(bytes: &[u8]) -> Result<Picture, Error> {
    let placeable_bounds = placeable(bytes);
    let start = if placeable_bounds.is_some() {
        PLACEABLE_SIZE
    } else {
        0
    };
    let header_words = u16_at(bytes, start + 2).ok_or(Error::Malformed("WMF header"))?;
    if header_words != 9 {
        return Err(Error::Malformed("WMF header"));
    }
    let first = start + 18;
    if let Some(embedded) = embedded_emf(bytes, first) {
        if let Ok(picture) = emf::play(&embedded, Kind::EmbeddedEmf) {
            return Ok(picture);
        }
    }
    let bounds = placeable_bounds
        .filter(|b| b.width > 0.0 && b.height > 0.0)
        .or_else(|| window_bounds(bytes, first))
        .ok_or(Error::Malformed("WMF without a frame"))?;
    if bounds.width <= 0.0 || bounds.height <= 0.0 {
        return Err(Error::Malformed("WMF frame"));
    }
    let pt = 72.0 / bounds.inch;
    let device = [pt, 0.0, 0.0, pt, -bounds.left * pt, -bounds.top * pt];
    let per_mm = (bounds.inch / 25.4, bounds.inch / 25.4);
    let mut dc = Dc::new(initial_state(&bounds), device, per_mm, true);
    for (at, size, function) in records(bytes, first) {
        record(&mut dc, function, &bytes[at..at + size]);
        if dc.full {
            break;
        }
    }
    let (ops, notes) = dc.finish();
    Ok(Picture {
        kind: Kind::Wmf,
        width: (bounds.width * pt) as f32,
        height: (bounds.height * pt) as f32,
        ops,
        notes,
    })
}

fn name(function: u16) -> &'static str {
    match function {
        0x0228 => "META_FILLREGION",
        0x0229 | 0x0429 => "META_FRAMEREGION",
        0x012A => "META_INVERTREGION",
        0x012B => "META_PAINTREGION",
        0x0419 => "META_FLOODFILL",
        0x0548 => "META_EXTFLOODFILL",
        0x0415 => "META_EXCLUDECLIPRECT",
        0x0220 => "META_OFFSETCLIPRGN",
        0x0922 => "META_BITBLT",
        0x0B23 => "META_STRETCHBLT",
        0x0D33 => "META_SETDIBTODEV",
        0x0436 => "META_ANIMATEPALETTE",
        _ => "an unknown WMF record",
    }
}

/// Plays one record; `r` is the whole record, size and function included.
/// The state records ([MS-WMF] §2.3.5) set the window (§2.3.5.30,
/// §2.3.5.31), the map mode (§2.3.5.17) and the drawing state, and save
/// and restore it (§2.3.5.10, §2.3.5.11).
fn record(dc: &mut Dc, function: u16, r: &[u8]) {
    let s = |at: usize| f64::from(i16_at(r, at).unwrap_or(0));
    let w = |at: usize| u16_at(r, at).unwrap_or(0);
    match function {
        0x020B => dc.set_window_org(s(8), s(6)),
        0x020C => dc.set_window_ext(s(8), s(6)),
        0x020F => {
            let (x, y) = dc.state.window_org;
            dc.set_window_org(x + s(8), y + s(6));
        }
        0x0410 => {
            let (x, y) = dc.state.window_ext;
            let (yd, yn, xd, xn) = (s(6), s(8), s(10), s(12));
            if xd != 0.0 && yd != 0.0 {
                dc.set_window_ext(x * xn / xd, y * yn / yd);
            }
        }
        0x020D | 0x020E | 0x0211 | 0x0412 => {}
        0x0103 => dc.set_map_mode(u32::from(w(6))),
        0x0102 => dc.state.opaque = w(6) == 2,
        0x0106 => dc.state.even_odd = w(6) != 2,
        0x0104 => dc.state.rop2 = u32::from(w(6)),
        0x012E => dc.state.align = u32::from(w(6)),
        0x0209 => dc.state.text_color = color(r, 6),
        0x0201 => dc.state.bk_color = color(r, 6),
        0x001E => dc.save(),
        0x0127 => dc.restore(i32::from(i16_at(r, 6).unwrap_or(-1))),
        0x0214 => dc.move_to(s(8), s(6)),
        0x0213 => dc.line_to(s(8), s(6)),
        0x0324 | 0x0325 => {
            let n = (i16_at(r, 6).unwrap_or(0).max(0) as usize).min(r.len() / 4);
            let pts = points(r, 8, n);
            match function {
                0x0324 => dc.polygon(&pts),
                _ => dc.polyline(&pts, false),
            }
        }
        0x0538 => polypolygon(dc, r),
        0x041B => dc.rectangle(s(12), s(10), s(8), s(6)),
        0x0418 => dc.ellipse(s(12), s(10), s(8), s(6)),
        0x061C => dc.round_rect(s(16), s(14), s(12), s(10), s(8), s(6)),
        0x0817 | 0x0830 | 0x081A => {
            let kind = match function {
                0x0817 => ArcKind::Open,
                0x0830 => ArcKind::Chord,
                _ => ArcKind::Pie,
            };
            dc.arc(
                (s(20), s(18), s(16), s(14)),
                (s(12), s(10)),
                (s(8), s(6)),
                kind,
            );
        }
        0x041F => {
            let c = color(r, 6);
            let saved = dc.state.brush;
            dc.state.brush = BrushObject::Solid(c);
            let (y, x) = (s(10), s(12));
            dc.pat_blt(x, y, 1.0, 1.0, 0x00F0_0021);
            dc.state.brush = saved;
        }
        0x061D => {
            let rop = u32_at(r, 6).unwrap_or(0);
            dc.pat_blt(s(16), s(14), s(12), s(10), rop);
        }
        0x0416 => {
            let rect = dc.logical_rect(s(12), s(10), s(8), s(6));
            dc.intersect_clip(rect);
        }
        0x012C => dc.set_clip(None),
        0x012D => dc.select(usize::from(w(6))),
        0x01F0 => dc.delete(usize::from(w(6))),
        0x02FA => create_pen(dc, r),
        0x02FC => create_brush(dc, r),
        0x02FB => create_font(dc, r),
        0x0142 => pattern_brush(dc, r),
        0x00F7 | 0x01F9 | 0x06FF => {
            if function == 0x01F9 {
                dc.note("bitmap pattern brushes are filled grey");
                dc.create(
                    None,
                    Object::Brush(BrushObject::Solid(Color(0x80, 0x80, 0x80))),
                );
                return;
            }
            dc.create(None, Object::Other);
        }
        0x0521 => text_out(dc, r),
        0x0A32 => ext_text_out(dc, r),
        0x0940 | 0x0B41 | 0x0F43 => blit(dc, function, r),
        0x0035 | 0x0037 | 0x0105 | 0x0107 | 0x0108 | 0x020A | 0x0231 | 0x0234 | 0x0139 | 0x0149
        | 0x0626 => {}
        f => dc.note(format!("{} records are not drawn", name(f))),
    }
}

fn points(r: &[u8], at: usize, n: usize) -> Vec<(f64, f64)> {
    (0..n)
        .filter_map(|k| {
            let x = i16_at(r, at + k * 4)?;
            let y = i16_at(r, at + k * 4 + 2)?;
            Some((f64::from(x), f64::from(y)))
        })
        .collect()
}

/// [MS-WMF] §2.3.3.16 META_POLYPOLYGON with a PolyPolygon object
/// (§2.2.2.17).
fn polypolygon(dc: &mut Dc, r: &[u8]) {
    let count = usize::from(u16_at(r, 6).unwrap_or(0)).min(r.len() / 2);
    let counts: Vec<usize> = (0..count)
        .map(|k| usize::from(u16_at(r, 8 + k * 2).unwrap_or(0)))
        .collect();
    let mut at = 8 + count * 2;
    let lists: Vec<Vec<(f64, f64)>> = counts
        .iter()
        .map(|&n| {
            let n = n.min(r.len().saturating_sub(at) / 4);
            let list = points(r, at, n);
            at += n * 4;
            list
        })
        .collect();
    let refs: Vec<&[(f64, f64)]> = lists.iter().map(Vec::as_slice).collect();
    dc.polypolygon(&refs);
}

/// [MS-WMF] §2.3.4.5 META_CREATEPENINDIRECT with a Pen object (§2.2.1.4).
fn create_pen(dc: &mut Dc, r: &[u8]) {
    let pen = PenObject {
        style: u32::from(u16_at(r, 6).unwrap_or(0)),
        width: f64::from(i16_at(r, 8).unwrap_or(0).max(0)),
        color: color(r, 12),
        dash: None,
    };
    dc.create(None, Object::Pen(pen));
}

/// [MS-WMF] §2.3.4.1 META_CREATEBRUSHINDIRECT with a LogBrush object
/// (§2.2.2.10).
fn create_brush(dc: &mut Dc, r: &[u8]) {
    let c = color(r, 8);
    let brush = match u16_at(r, 6).unwrap_or(0) {
        1 => BrushObject::Null,
        2 => BrushObject::Hatched(c),
        _ => BrushObject::Solid(c),
    };
    dc.create(None, Object::Brush(brush));
}

/// [MS-WMF] §2.3.4.8 META_DIBCREATEPATTERNBRUSH: filled with the average
/// color of its bitmap.
fn pattern_brush(dc: &mut Dc, r: &[u8]) {
    dc.note("pattern brushes are filled with the average color of their bitmap");
    let image = r
        .get(10..)
        .and_then(|d| bitmap::decode_packed(d, false).ok());
    let brush = image.map_or(BrushObject::Solid(Color(0x80, 0x80, 0x80)), |i| {
        BrushObject::Solid(average(&i))
    });
    dc.create(None, Object::Brush(brush));
}

/// [MS-WMF] §2.3.4.2 META_CREATEFONTINDIRECT with a Font object
/// (§2.2.1.2).
fn create_font(dc: &mut Dc, r: &[u8]) {
    let face: String = r
        .get(24..r.len().min(56))
        .unwrap_or(&[])
        .iter()
        .take_while(|&&b| b != 0)
        .map(|&b| char::from(b))
        .collect();
    let font = FontObject {
        height: f64::from(i16_at(r, 6).unwrap_or(0)),
        escapement: f64::from(i16_at(r, 10).unwrap_or(0)),
        weight: i32::from(i16_at(r, 14).unwrap_or(400)),
        italic: u8_at(r, 16).unwrap_or(0) != 0,
        underline: u8_at(r, 17).unwrap_or(0) != 0,
        strike: u8_at(r, 18).unwrap_or(0) != 0,
        charset: u8_at(r, 19).unwrap_or(0),
        face: face.trim().to_string(),
    };
    dc.create(None, Object::Font(font));
}

/// [MS-WMF] §2.3.3.20 META_TEXTOUT.
fn text_out(dc: &mut Dc, r: &[u8]) {
    let n = i16_at(r, 6).unwrap_or(0).max(0) as usize;
    let Some(bytes) = span(r, 8, n) else {
        return;
    };
    let at = 8 + n.next_multiple_of(2);
    let y = f64::from(i16_at(r, at).unwrap_or(0));
    let x = f64::from(i16_at(r, at + 2).unwrap_or(0));
    let text = dc.state.font.decode(bytes);
    dc.text((x, y), text, None, None, None);
}

/// [MS-WMF] §2.3.3.5 META_EXTTEXTOUT: the rectangle is present only for
/// ETO_OPAQUE or ETO_CLIPPED (§2.1.2.2), the advances only when the record
/// has room for them.
fn ext_text_out(dc: &mut Dc, r: &[u8]) {
    const ETO_OPAQUE: u16 = 0x2;
    const ETO_CLIPPED: u16 = 0x4;
    let y = f64::from(i16_at(r, 6).unwrap_or(0));
    let x = f64::from(i16_at(r, 8).unwrap_or(0));
    let n = i16_at(r, 10).unwrap_or(0).max(0) as usize;
    let options = u16_at(r, 12).unwrap_or(0);
    let has_rect = options & (ETO_OPAQUE | ETO_CLIPPED) != 0;
    let mut at = 14;
    let rect = has_rect.then(|| {
        let v = |k: usize| f64::from(i16_at(r, 14 + k * 2).unwrap_or(0));
        at += 8;
        (v(0), v(1), v(2), v(3))
    });
    let Some(bytes) = span(r, at, n) else {
        return;
    };
    let text = dc.state.font.decode(bytes);
    let dx_at = at + n.next_multiple_of(2);
    let advances = (dx_at + n * 2 <= r.len() && n > 0).then(|| {
        (0..n)
            .map(|k| f64::from(i16_at(r, dx_at + k * 2).unwrap_or(0)))
            .collect::<Vec<f64>>()
    });
    let frame_rect: Option<Rect> = rect
        .filter(|(l, t, rr, b)| rr > l && b > t)
        .map(|(l, t, rr, b)| dc.logical_rect(l, t, rr, b));
    let opaque = frame_rect.filter(|_| options & ETO_OPAQUE != 0);
    let clip = frame_rect.filter(|_| options & ETO_CLIPPED != 0);
    dc.text((x, y), text, advances, opaque, clip);
}

/// [MS-WMF] §2.3.1.2 META_DIBBITBLT, §2.3.1.3 META_DIBSTRETCHBLT and
/// §2.3.1.6 META_STRETCHDIB. A record without a bitmap is one word longer
/// than its function's high byte says and paints the brush.
fn blit(dc: &mut Dc, function: u16, r: &[u8]) {
    let s = |at: usize| f64::from(i16_at(r, at).unwrap_or(0));
    let rop = u32_at(r, 6).unwrap_or(SRCCOPY);
    let with_bitmap = r.len() / 2 != usize::from(function >> 8) + 3;
    let (dest, source, dib_at) = match (function, with_bitmap) {
        (0x0F43, _) => (
            (s(26), s(24), s(22), s(20)),
            (s(18), s(16), s(14), s(12)),
            28,
        ),
        (0x0B41, true) => (
            (s(24), s(22), s(20), s(18)),
            (s(16), s(14), s(12), s(10)),
            26,
        ),
        (0x0B41, false) => ((s(26), s(24), s(22), s(20)), (0.0, 0.0, 0.0, 0.0), 0),
        (_, true) => (
            (s(20), s(18), s(16), s(14)),
            (s(12), s(10), s(16), s(14)),
            22,
        ),
        (_, false) => ((s(22), s(20), s(18), s(16)), (0.0, 0.0, 0.0, 0.0), 0),
    };
    if dib_at == 0 {
        return dc.pat_blt(dest.0, dest.1, dest.2, dest.3, rop);
    }
    let dib = r.get(dib_at..).unwrap_or(&[]);
    let image = match bitmap::decode_packed(dib, false) {
        Ok(image) => image,
        Err(e) => return dc.note(format!("{e}")),
    };
    let from_bottom = match u32_at(dib, 0) {
        Some(12) => true,
        _ => crate::bytes::i32_at(dib, 8).unwrap_or(0) > 0,
    };
    let (sx, sy, sw, sh) = source;
    let sy = match from_bottom && function == 0x0F43 {
        true => f64::from(image.height) - sy - sh,
        false => sy,
    };
    dc.bitmap(dest, (sx, sy, sw, sh), image, rop, 255);
}

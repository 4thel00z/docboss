//! EMF records ([MS-EMF] §2.3) played through the device context.

use docboss_model::Color;

use crate::bitmap;
use crate::bytes::{f32_at, i16_at, i32_at, span, u16_at, u32_at, u8_at};
use crate::dc::{
    average, ArcKind, BrushObject, Dc, FontObject, Object, PenObject, State, BLACK_PEN, IDENTITY,
    MM_TEXT, SRCCOPY,
};
use crate::{Error, Kind, Picture, Rect, MAX_RECORDS};

const EMR_HEADER: u32 = 1;
const EMR_EOF: u32 = 14;
const EMR_COMMENT: u32 = 70;
const SIGNATURE: u32 = 0x464D_4520;
const EMF_PLUS: u32 = 0x2B46_4D45;

/// Whether `bytes` start with an EMR_HEADER record carrying the EMF
/// signature ([MS-EMF] §2.3.4.2).
pub(crate) fn sniff(bytes: &[u8]) -> bool {
    u32_at(bytes, 0) == Some(EMR_HEADER) && u32_at(bytes, 40) == Some(SIGNATURE)
}

fn color(d: &[u8], at: usize) -> Color {
    let b = |k| u8_at(d, at + k).unwrap_or(0);
    Color(b(0), b(1), b(2))
}

fn rectl(d: &[u8], at: usize) -> Option<(f64, f64, f64, f64)> {
    Some((
        f64::from(i32_at(d, at)?),
        f64::from(i32_at(d, at + 4)?),
        f64::from(i32_at(d, at + 8)?),
        f64::from(i32_at(d, at + 12)?),
    ))
}

fn pointl(d: &[u8], at: usize) -> Option<(f64, f64)> {
    Some((f64::from(i32_at(d, at)?), f64::from(i32_at(d, at + 4)?)))
}

/// `count` points from `at`: 16-bit PointS pairs or 32-bit PointL pairs.
fn points(d: &[u8], at: usize, count: usize, short: bool) -> Vec<(f64, f64)> {
    let size = if short { 4 } else { 8 };
    let count = count.min(d.len().saturating_sub(at) / size);
    (0..count)
        .filter_map(|i| {
            let o = at + i * size;
            match short {
                true => Some((f64::from(i16_at(d, o)?), f64::from(i16_at(d, o + 2)?))),
                false => pointl(d, o),
            }
        })
        .collect()
}

/// The frame of an EMF ([MS-EMF] §2.2.9 Header Object): its size in
/// points and the map from device units to it.
struct Frame {
    width: f32,
    height: f32,
    device: [f64; 6],
    per_mm: (f64, f64),
}

fn frame(h: &[u8]) -> Result<Frame, Error> {
    let bad = Error::Malformed("EMF header");
    let bounds = rectl(h, 8).ok_or(bad.clone())?;
    let frame = rectl(h, 24).ok_or(bad.clone())?;
    let device = (
        f64::from(i32_at(h, 72).ok_or(bad.clone())?),
        f64::from(i32_at(h, 76).ok_or(bad.clone())?),
    );
    let mm = (
        f64::from(i32_at(h, 80).ok_or(bad.clone())?),
        f64::from(i32_at(h, 84).ok_or(bad)?),
    );
    let size = u32_at(h, 4).unwrap_or(0) as usize;
    let description = match u32_at(h, 60).unwrap_or(0) {
        0 => size,
        _ => (u32_at(h, 64).unwrap_or(0) as usize).clamp(88, size.max(88)),
    };
    let micrometres = match size.min(description) >= 108 {
        true => (
            f64::from(u32_at(h, 100).unwrap_or(0)),
            f64::from(u32_at(h, 104).unwrap_or(0)),
        ),
        false => (0.0, 0.0),
    };
    let mm_per_px = |i: usize| {
        let (dev, milli, micro) = match i {
            0 => (device.0, mm.0, micrometres.0),
            _ => (device.1, mm.1, micrometres.1),
        };
        if dev <= 0.0 {
            return 25.4 / 96.0;
        }
        match (micro > 0.0, milli > 0.0) {
            (true, _) => micro / 1000.0 / dev,
            (false, true) => milli / dev,
            _ => 25.4 / 96.0,
        }
    };
    let (kx, ky) = (mm_per_px(0), mm_per_px(1));
    let pt = 72.0 / 25.4;
    let (fl, ft, fr, fb) = frame;
    let usable = fr > fl && fb > ft;
    let (left, top, width, height) = match usable {
        true => (fl / 100.0, ft / 100.0, (fr - fl) / 100.0, (fb - ft) / 100.0),
        false => {
            let (bl, bt, br, bb) = bounds;
            (bl * kx, bt * ky, (br - bl + 1.0) * kx, (bb - bt + 1.0) * ky)
        }
    };
    if !(width > 0.0 && height > 0.0 && width.is_finite() && height.is_finite()) {
        return Err(Error::Malformed("EMF frame"));
    }
    Ok(Frame {
        width: (width * pt) as f32,
        height: (height * pt) as f32,
        device: [kx * pt, 0.0, 0.0, ky * pt, -left * pt, -top * pt],
        per_mm: (1.0 / kx, 1.0 / ky),
    })
}

/// The device context an EMF starts with.
pub(crate) fn initial_state() -> State {
    State {
        map_mode: MM_TEXT,
        window_org: (0.0, 0.0),
        window_ext: (1.0, 1.0),
        viewport_org: (0.0, 0.0),
        viewport_ext: (1.0, 1.0),
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

/// [MS-EMF] §2.1.31 StockObject: the objects selected by index with the
/// high bit set.
fn stock(index: u32) -> Option<Object> {
    let gray = |v: u8| Object::Brush(BrushObject::Solid(Color(v, v, v)));
    let pen = |color: Color| Object::Pen(PenObject { color, ..BLACK_PEN });
    Some(match index & 0x7FFF_FFFF {
        0 => gray(255),
        1 => gray(0xC0),
        2 => gray(0x80),
        3 => gray(0x40),
        4 => gray(0),
        5 => Object::Brush(BrushObject::Null),
        6 => pen(Color::WHITE),
        7 => pen(Color::BLACK),
        8 => Object::Pen(PenObject {
            style: 5,
            ..BLACK_PEN
        }),
        10 | 11 | 16 => Object::Font(FontObject::default_face("Courier New")),
        12..=14 | 17 => Object::Font(FontObject::default_face("")),
        18 => gray(255),
        19 => pen(Color::BLACK),
        _ => return None,
    })
}

/// Plays an EMF. `kind` says whether it stood alone or came out of a WMF.
pub(crate) fn play(bytes: &[u8], kind: Kind) -> Result<Picture, Error> {
    let header_size = u32_at(bytes, 4).ok_or(Error::Malformed("EMF header"))? as usize;
    let header =
        span(bytes, 0, header_size.min(bytes.len())).ok_or(Error::Malformed("EMF header"))?;
    let frame = frame(header)?;
    let mut dc = Dc::new(initial_state(), frame.device, frame.per_mm, false);
    let mut at = 0usize;
    let mut plus = false;
    for _ in 0..MAX_RECORDS {
        let (Some(kind), Some(size)) = (u32_at(bytes, at), u32_at(bytes, at + 4)) else {
            break;
        };
        let size = size as usize;
        if size < 8 || at + size > bytes.len() {
            if at + 8 <= bytes.len() {
                dc.note("a truncated EMF record ended playback");
            }
            break;
        }
        let r = &bytes[at..at + size];
        if kind == EMR_EOF {
            break;
        }
        if kind == EMR_COMMENT && is_emf_plus(r) {
            plus = true;
        }
        record(&mut dc, kind, r);
        if dc.full {
            break;
        }
        at += size;
    }
    if plus && !dc.has_ops() {
        return Err(Error::EmfPlusOnly);
    }
    let (ops, notes) = dc.finish();
    Ok(Picture {
        kind,
        width: frame.width,
        height: frame.height,
        ops,
        notes,
    })
}

/// [MS-EMF] §2.3.3.2 EMR_COMMENT_EMFPLUS: a comment carrying EMF+ records.
fn is_emf_plus(r: &[u8]) -> bool {
    u32_at(r, 12) == Some(EMF_PLUS)
}

fn name(kind: u32) -> &'static str {
    match kind {
        0x0F => "EMR_SETPIXELV",
        0x1A => "EMR_OFFSETCLIPRGN",
        0x1D => "EMR_EXCLUDECLIPRECT",
        0x35 => "EMR_EXTFLOODFILL",
        0x38 | 0x5C => "EMR_POLYDRAW",
        0x41 => "EMR_FLATTENPATH",
        0x42 => "EMR_WIDENPATH",
        0x47 => "EMR_FILLRGN",
        0x48 => "EMR_FRAMERGN",
        0x49 => "EMR_INVERTRGN",
        0x4A => "EMR_PAINTRGN",
        0x4E => "EMR_MASKBLT",
        0x4F => "EMR_PLGBLT",
        0x60 | 0x61 => "EMR_POLYTEXTOUT",
        0x6C => "EMR_SMALLTEXTOUT",
        0x74 => "EMR_TRANSPARENTBLT",
        0x76 => "EMR_GRADIENTFILL",
        _ => "an unknown EMF record",
    }
}

/// Plays one record; `r` is the whole record, type and size included.
fn record(dc: &mut Dc, kind: u32, r: &[u8]) {
    let u = |at: usize| u32_at(r, at).unwrap_or(0);
    let i = |at: usize| f64::from(i32_at(r, at).unwrap_or(0));
    match kind {
        0x02..=0x08 | 0x55..=0x5B => poly(dc, kind, r),
        0x09 => dc.set_window_ext(i(8), i(12)),
        0x0A => dc.set_window_org(i(8), i(12)),
        0x0B => dc.set_viewport_ext(i(8), i(12)),
        0x0C => dc.set_viewport_org(i(8), i(12)),
        0x11 => dc.set_map_mode(u(8)),
        0x12 => dc.state.opaque = u(8) == 2,
        0x13 => dc.state.even_odd = u(8) != 2,
        0x14 => dc.state.rop2 = u(8),
        0x16 => dc.state.align = u(8),
        0x18 => dc.state.text_color = color(r, 8),
        0x19 => dc.state.bk_color = color(r, 8),
        0x1B => dc.move_to(i(8), i(12)),
        0x1E => {
            let rect = dc.logical_rect(i(8), i(12), i(16), i(20));
            dc.intersect_clip(rect);
        }
        0x1F => scale_extent(dc, r, false),
        0x20 => scale_extent(dc, r, true),
        0x21 => dc.save(),
        0x22 => dc.restore(i32_at(r, 8).unwrap_or(-1)),
        0x23 | 0x24 => {
            let f = |k: usize| f64::from(f32_at(r, 8 + k * 4).unwrap_or(0.0));
            let m = [f(0), f(1), f(2), f(3), f(4), f(5)];
            let mode = if kind == 0x23 { 4 } else { u(32) };
            dc.world(m, mode);
        }
        0x25 => select(dc, u(8)),
        0x26 => create_pen(dc, r),
        0x27 => create_brush(dc, r),
        0x28 => delete(dc, u(8)),
        0x29 => {
            let f = |at: usize| f64::from(f32_at(r, at).unwrap_or(0.0));
            let c = pointl(r, 8).unwrap_or((0.0, 0.0));
            dc.angle_arc(c, f64::from(u(16)), f(20), f(24));
        }
        0x2A => dc.ellipse(i(8), i(12), i(16), i(20)),
        0x2B => dc.rectangle(i(8), i(12), i(16), i(20)),
        0x2C => dc.round_rect(i(8), i(12), i(16), i(20), i(24), i(28)),
        0x2D | 0x2E | 0x2F | 0x37 => {
            let arc_kind = match kind {
                0x2D => ArcKind::Open,
                0x2E => ArcKind::Chord,
                0x2F => ArcKind::Pie,
                _ => ArcKind::To,
            };
            dc.arc(
                (i(8), i(12), i(16), i(20)),
                (i(24), i(28)),
                (i(32), i(36)),
                arc_kind,
            );
        }
        0x30..=0x34
        | 0x10
        | 0x15
        | 0x17
        | 0x0D
        | 0x1C
        | 0x3A
        | 0x62..=0x6B
        | 0x6D..=0x71
        | 0x73
        | 0x77..=0x7A => {}
        0x36 => dc.line_to(i(8), i(12)),
        0x39 => dc.state.clockwise = u(8) == 2,
        0x3B => dc.begin_path(),
        0x3C => dc.end_path(),
        0x3D => dc.close_figure(),
        0x3E => dc.paint_path(true, false),
        0x3F => dc.paint_path(true, true),
        0x40 => dc.paint_path(false, true),
        0x43 => dc.clip_to_path(u(8)),
        0x44 => dc.abort_path(),
        0x01 | 0x46 => {}
        0x4B => select_clip_region(dc, r),
        0x4C | 0x4D | 0x72 => blit(dc, kind, r),
        0x50 => set_dibits(dc, r),
        0x51 => stretch_dibits(dc, r),
        0x52 => create_font(dc, r),
        0x53 | 0x54 => text(dc, r, kind == 0x54),
        0x5D | 0x5E => pattern_brush(dc, r),
        0x5F => ext_create_pen(dc, r),
        _ => dc.note(format!("{} records are not drawn", name(kind))),
    }
}

fn scale_extent(dc: &mut Dc, r: &[u8], window: bool) {
    let i = |at: usize| f64::from(i32_at(r, at).unwrap_or(0));
    let (xn, xd, yn, yd) = (i(8), i(12), i(16), i(20));
    if xd == 0.0 || yd == 0.0 {
        return;
    }
    match window {
        true => {
            let (x, y) = dc.state.window_ext;
            dc.set_window_ext(x * xn / xd, y * yn / yd);
        }
        false => {
            let (x, y) = dc.state.viewport_ext;
            dc.set_viewport_ext(x * xn / xd, y * yn / yd);
        }
    }
}

/// [MS-EMF] §2.3.5.16 to §2.3.5.31: the polyline, polygon and Bézier
/// records in their 32-bit and 16-bit forms.
fn poly(dc: &mut Dc, kind: u32, r: &[u8]) {
    let short = kind >= 0x55;
    let base = if short { kind - 0x53 } else { kind };
    let count = u32_at(r, 24).unwrap_or(0) as usize;
    if matches!(base, 7 | 8) {
        let polygons = count.min(r.len() / 4);
        let total = u32_at(r, 28).unwrap_or(0) as usize;
        let counts: Vec<usize> = (0..polygons)
            .map(|k| u32_at(r, 32 + k * 4).unwrap_or(0) as usize)
            .collect();
        let all = points(r, 32 + polygons * 4, total, short);
        let mut lists: Vec<&[(f64, f64)]> = Vec::with_capacity(counts.len());
        let mut start = 0usize;
        for n in counts {
            let end = start.saturating_add(n).min(all.len());
            lists.push(&all[start..end]);
            start = end;
        }
        match base {
            7 => dc.polypolyline(&lists),
            _ => dc.polypolygon(&lists),
        }
        return;
    }
    let pts = points(r, 28, count, short);
    match base {
        2 => dc.polybezier(&pts, false),
        3 => dc.polygon(&pts),
        4 => dc.polyline(&pts, false),
        5 => dc.polybezier(&pts, true),
        _ => dc.polyline(&pts, true),
    }
}

/// [MS-EMF] §2.3.8.5 EMR_SELECTOBJECT: a table index, or a stock object
/// with the high bit set.
fn select(dc: &mut Dc, index: u32) {
    if index & 0x8000_0000 == 0 {
        return dc.select(index as usize);
    }
    if let Some(object) = stock(index) {
        dc.select_object(object);
    }
}

/// [MS-EMF] §2.3.8.3 EMR_DELETEOBJECT.
fn delete(dc: &mut Dc, index: u32) {
    dc.delete(index as usize);
}

/// [MS-EMF] §2.3.7.7 EMR_CREATEPEN with a LogPen (§2.2.19).
fn create_pen(dc: &mut Dc, r: &[u8]) {
    let index = u32_at(r, 8).unwrap_or(0) as usize;
    let pen = PenObject {
        style: u32_at(r, 12).unwrap_or(0),
        width: f64::from(i32_at(r, 16).unwrap_or(0)),
        color: color(r, 24),
        dash: None,
    };
    dc.create(Some(index), Object::Pen(pen));
}

/// [MS-EMF] §2.3.7.9 EMR_EXTCREATEPEN with a LogPenEx (§2.2.20): the
/// width of a cosmetic pen is one device pixel.
fn ext_create_pen(dc: &mut Dc, r: &[u8]) {
    let index = u32_at(r, 8).unwrap_or(0) as usize;
    let style = u32_at(r, 28).unwrap_or(0);
    let geometric = style & 0xF_0000 == 0x1_0000;
    let width = match geometric {
        true => f64::from(u32_at(r, 32).unwrap_or(0)),
        false => 0.0,
    };
    let entries = (u32_at(r, 48).unwrap_or(0) as usize).min(16);
    let stops: Vec<(u32, u32)> = (0..entries / 2)
        .map(|k| {
            let v = |j: usize| u32_at(r, 52 + (k * 2 + j) * 4).unwrap_or(0);
            let unit = if width > 0.0 { width } else { 1.0 };
            let hundredths = |v: u32| (f64::from(v) * 100.0 / unit) as u32;
            (hundredths(v(0)), hundredths(v(1)))
        })
        .collect();
    let brush_style = u32_at(r, 36).unwrap_or(0);
    let pen = PenObject {
        style: match brush_style {
            1 => 5,
            _ => style,
        },
        width,
        color: color(r, 40),
        dash: docboss_model::DashPattern::new(&stops),
    };
    dc.create(Some(index), Object::Pen(pen));
}

/// [MS-EMF] §2.3.7.1 EMR_CREATEBRUSHINDIRECT with a LogBrush.
fn create_brush(dc: &mut Dc, r: &[u8]) {
    let index = u32_at(r, 8).unwrap_or(0) as usize;
    let c = color(r, 16);
    let brush = match u32_at(r, 12).unwrap_or(0) {
        1 => BrushObject::Null,
        2 => BrushObject::Hatched(c),
        _ => BrushObject::Solid(c),
    };
    dc.create(Some(index), Object::Brush(brush));
}

/// [MS-EMF] §2.3.7.4 and §2.3.7.5: pattern brushes, filled with the
/// average color of their bitmap.
fn pattern_brush(dc: &mut Dc, r: &[u8]) {
    let index = u32_at(r, 8).unwrap_or(0) as usize;
    let fetch = |off: usize| {
        let at = u32_at(r, off).unwrap_or(0) as usize;
        let len = u32_at(r, off + 4).unwrap_or(0) as usize;
        span(r, at, len)
    };
    let image = match (fetch(16), fetch(24)) {
        (Some(info), Some(bits)) => bitmap::decode(info, bits, false).ok(),
        _ => None,
    };
    dc.note("pattern brushes are filled with the average color of their bitmap");
    let brush = image.map_or(BrushObject::Solid(Color(0x80, 0x80, 0x80)), |i| {
        BrushObject::Solid(average(&i))
    });
    dc.create(Some(index), Object::Brush(brush));
}

/// [MS-EMF] §2.3.7.8 EMR_EXTCREATEFONTINDIRECTW with a LogFont (§2.2.13).
fn create_font(dc: &mut Dc, r: &[u8]) {
    let index = u32_at(r, 8).unwrap_or(0) as usize;
    let face: String = char::decode_utf16(
        (0..32)
            .map_while(|k| u16_at(r, 40 + k * 2))
            .take_while(|&c| c != 0),
    )
    .map(|c| c.unwrap_or('\u{FFFD}'))
    .collect();
    let font = FontObject {
        height: f64::from(i32_at(r, 12).unwrap_or(0)),
        escapement: f64::from(i32_at(r, 20).unwrap_or(0)),
        weight: i32_at(r, 28).unwrap_or(400),
        italic: u8_at(r, 32).unwrap_or(0) != 0,
        underline: u8_at(r, 33).unwrap_or(0) != 0,
        strike: u8_at(r, 34).unwrap_or(0) != 0,
        charset: u8_at(r, 35).unwrap_or(0),
        face,
    };
    dc.create(Some(index), Object::Font(font));
}

/// [MS-EMF] §2.3.5.7 and §2.3.5.8 with an EmrText object (§2.2.5): the
/// string and advances sit at offsets from the start of the record.
fn text(dc: &mut Dc, r: &[u8], wide: bool) {
    const ETO_OPAQUE: u32 = 0x2;
    const ETO_CLIPPED: u32 = 0x4;
    const ETO_GLYPH_INDEX: u32 = 0x10;
    const ETO_PDY: u32 = 0x2000;
    let reference = pointl(r, 36).unwrap_or((0.0, 0.0));
    let chars = (u32_at(r, 44).unwrap_or(0) as usize).min(r.len());
    let off_string = u32_at(r, 48).unwrap_or(0) as usize;
    let options = u32_at(r, 52).unwrap_or(0);
    let rect = rectl(r, 56).unwrap_or((0.0, 0.0, 0.0, 0.0));
    let off_dx = u32_at(r, 72).unwrap_or(0) as usize;
    if options & ETO_GLYPH_INDEX != 0 {
        dc.note("text given as glyph indices is not drawn");
        return;
    }
    let string = match wide {
        true => char::decode_utf16((0..chars).map_while(|k| u16_at(r, off_string + k * 2)))
            .map(|c| c.unwrap_or('\u{FFFD}'))
            .map(|c| match dc.state.font.symbolic() && (c as u32) < 0x100 {
                true => char::from_u32(0xF000 + c as u32).unwrap_or(c),
                false => c,
            })
            .collect::<String>(),
        false => dc
            .state
            .font
            .decode(span(r, off_string, chars).unwrap_or(&[])),
    };
    let step = if options & ETO_PDY != 0 { 8 } else { 4 };
    let advances = (off_dx >= 76 && off_dx + chars * step <= r.len()).then(|| {
        (0..chars)
            .map(|k| f64::from(i32_at(r, off_dx + k * step).unwrap_or(0)))
            .collect::<Vec<f64>>()
    });
    let advances = advances.filter(|a| a.len() == string.chars().count());
    let has_rect = rect.2 > rect.0 && rect.3 > rect.1;
    let frame_rect = has_rect.then(|| dc.logical_rect(rect.0, rect.1, rect.2, rect.3));
    let opaque = frame_rect.filter(|_| options & ETO_OPAQUE != 0);
    let clip = frame_rect.filter(|_| options & ETO_CLIPPED != 0);
    dc.text(reference, string, advances, opaque, clip);
}

/// [MS-EMF] §2.3.2.2 EMR_EXTSELECTCLIPRGN: the region's rectangles are in
/// device units (§2.2.24 RegionData).
fn select_clip_region(dc: &mut Dc, r: &[u8]) {
    let size = u32_at(r, 8).unwrap_or(0) as usize;
    let mode = u32_at(r, 12).unwrap_or(5);
    if size < 32 {
        return dc.combine_clip(&[], mode);
    }
    let count = (u32_at(r, 24).unwrap_or(0) as usize).min(r.len() / 16);
    let rects: Vec<Rect> = (0..count)
        .filter_map(|k| rectl(r, 48 + k * 16))
        .map(|(l, t, rr, b)| dc.device_rect(l, t, rr, b))
        .collect();
    dc.combine_clip(&rects, mode);
}

/// The DIB a bitmap record points at: its header and color table, then
/// its pixels, both by offset and size from the start of the record.
fn dib(r: &[u8], off: usize, alpha: bool) -> Option<Result<bitmap::Rgba, bitmap::BitmapError>> {
    let (bmi_at, bmi_len) = (u32_at(r, off)? as usize, u32_at(r, off + 4)? as usize);
    let (bits_at, bits_len) = (u32_at(r, off + 8)? as usize, u32_at(r, off + 12)? as usize);
    if bmi_len == 0 {
        return None;
    }
    let info = span(r, bmi_at, bmi_len)?;
    let bits = span(r, bits_at, bits_len).unwrap_or_else(|| r.get(bits_at..).unwrap_or(&[]));
    Some(bitmap::decode(info, bits, alpha))
}

/// Whether a DIB's rows run from the bottom, so that source rectangles
/// count from its last row.
fn bottom_up(r: &[u8], off: usize) -> bool {
    let at = u32_at(r, off).unwrap_or(0) as usize;
    match u32_at(r, at) {
        Some(12) => true,
        _ => i32_at(r, at + 8).unwrap_or(0) > 0,
    }
}

fn draw(
    dc: &mut Dc,
    dest: (f64, f64, f64, f64),
    source: (f64, f64, f64, f64),
    image: Result<bitmap::Rgba, bitmap::BitmapError>,
    rop: u32,
    alpha: u8,
    from_bottom: bool,
) {
    let image = match image {
        Ok(image) => image,
        Err(e) => return dc.note(format!("{e}")),
    };
    let (sx, sy, sw, sh) = source;
    let sy = match from_bottom {
        true => f64::from(image.height) - sy - sh,
        false => sy,
    };
    dc.bitmap(dest, (sx, sy, sw, sh), image, rop, alpha);
}

/// [MS-EMF] §2.3.1.2 EMR_BITBLT, §2.3.1.6 EMR_STRETCHBLT and §2.3.1.1
/// EMR_ALPHABLEND. Without a bitmap BITBLT paints the brush.
fn blit(dc: &mut Dc, kind: u32, r: &[u8]) {
    let i = |at: usize| f64::from(i32_at(r, at).unwrap_or(0));
    let dest = (i(24), i(28), i(32), i(36));
    let rop = u32_at(r, 40).unwrap_or(SRCCOPY);
    let alpha_blend = kind == 0x72;
    let (rop, constant, per_pixel) = match alpha_blend {
        true => (
            SRCCOPY,
            u8_at(r, 42).unwrap_or(255),
            u8_at(r, 43).unwrap_or(0) & 1 != 0,
        ),
        false => (rop, 255, false),
    };
    let Some(image) = dib(r, 84, per_pixel) else {
        return dc.pat_blt(dest.0, dest.1, dest.2, dest.3, rop);
    };
    let image = image.map(|mut img| {
        if per_pixel {
            unpremultiply(&mut img);
        }
        img
    });
    let xform_scale = (
        f64::from(f32_at(r, 52).unwrap_or(1.0)),
        f64::from(f32_at(r, 64).unwrap_or(1.0)),
    );
    let (cx, cy) = match kind {
        0x4C => (dest.2 * xform_scale.0, dest.3 * xform_scale.1),
        _ => (i(100), i(104)),
    };
    let source = (i(44), i(48), cx, cy);
    draw(dc, dest, source, image, rop, constant, false);
}

fn unpremultiply(image: &mut bitmap::Rgba) {
    for p in image.pixels.as_chunks_mut::<4>().0.iter_mut() {
        let a = u16::from(p[3]);
        if a == 0 || a == 255 {
            continue;
        }
        for c in &mut p[..3] {
            *c = (u16::from(*c) * 255 / a).min(255) as u8;
        }
    }
}

/// [MS-EMF] §2.3.1.7 EMR_STRETCHDIBITS.
fn stretch_dibits(dc: &mut Dc, r: &[u8]) {
    let i = |at: usize| f64::from(i32_at(r, at).unwrap_or(0));
    let dest = (i(24), i(28), i(72), i(76));
    let source = (i(32), i(36), i(40), i(44));
    let rop = u32_at(r, 68).unwrap_or(SRCCOPY);
    let Some(image) = dib(r, 48, false) else {
        return dc.pat_blt(dest.0, dest.1, dest.2, dest.3, rop);
    };
    let from_bottom = bottom_up(r, 48);
    draw(dc, dest, source, image, rop, 255, from_bottom);
}

/// [MS-EMF] §2.3.1.5 EMR_SETDIBITSTODEVICE: pixels copied at one logical
/// unit each.
fn set_dibits(dc: &mut Dc, r: &[u8]) {
    let i = |at: usize| f64::from(i32_at(r, at).unwrap_or(0));
    let (w, h) = (i(40), i(44));
    let dest = (i(24), i(28), w, h);
    let source = (i(32), i(36), w, h);
    let Some(image) = dib(r, 48, false) else {
        return;
    };
    let from_bottom = bottom_up(r, 48);
    draw(dc, dest, source, image, SRCCOPY, 255, from_bottom);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dc::compose;

    #[test]
    fn stock_objects_cover_brushes_pens_and_fonts() {
        assert_eq!(stock(0x8000_0005), Some(Object::Brush(BrushObject::Null)));
        assert!(matches!(stock(0x8000_0008), Some(Object::Pen(p)) if p.style == 5));
        assert!(matches!(stock(0x8000_000D), Some(Object::Font(_))));
        assert_eq!(stock(0x8000_0009), None);
    }

    #[test]
    fn composing_applies_the_first_map_first() {
        let scale = [2.0, 0.0, 0.0, 2.0, 0.0, 0.0];
        let shift = [1.0, 0.0, 0.0, 1.0, 10.0, 0.0];
        assert_eq!(compose(&scale, &shift)[4], 10.0);
        assert_eq!(compose(&shift, &scale)[4], 20.0);
    }
}

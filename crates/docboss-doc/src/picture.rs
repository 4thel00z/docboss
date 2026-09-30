//! Pictures: the PICF header in the Data stream ([MS-DOC] §2.9.190) and the
//! OfficeArt BLIP records that hold the image bytes ([MS-ODRAW] §2.2.23 to
//! §2.2.31).

use std::io::Read;

use docboss_model::{
    Color, DashPattern, Geometry, Gradient, GradientPath, LineCap, LineEnd, LineEndKind,
    LineEndSize, LineJoin, PositionAlign, PositionBase, ShapeFormat, TextDirection, VerticalAlign,
};

use crate::bytes::{i16_at, i32_at, slice, u16_at, u32_at, u8_at};

const HEADER: usize = 8;
const MAX_DEPTH: usize = 16;

/// An OfficeArt record header ([MS-ODRAW] §2.2.1 OfficeArtRecordHeader).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Record {
    pub version: u8,
    pub instance: u16,
    pub kind: u16,
    /// Offset of the record body.
    pub body: usize,
    pub length: usize,
}

pub fn record(bytes: &[u8], at: usize) -> Option<Record> {
    let vi = u16_at(bytes, at)?;
    let kind = u16_at(bytes, at + 2)?;
    let length = u32_at(bytes, at + 4)? as usize;
    let body = at + HEADER;
    let length = length.min(bytes.len().saturating_sub(body));
    Some(Record {
        version: (vi & 0xF) as u8,
        instance: vi >> 4,
        kind,
        body,
        length,
    })
}

/// The records directly inside `bytes[start..end]`.
pub fn children(bytes: &[u8], start: usize, end: usize) -> Vec<Record> {
    let mut out = Vec::new();
    let mut at = start;
    while at + HEADER <= end.min(bytes.len()) {
        let Some(child) = record(bytes, at) else {
            break;
        };
        out.push(child);
        at = child.body + child.length;
    }
    out
}

/// A decoded image: its MIME type and bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub content_type: &'static str,
    /// Shared with the model's media entry, so a BLIP drawn many times is
    /// held once.
    pub data: std::sync::Arc<[u8]>,
}

/// Decodes a BLIP record ([MS-ODRAW] §2.2.23 OfficeArtBlip): metafiles are
/// inflated, DIBs get a BMP file header, bitmaps are passed through.
pub fn blip(bytes: &[u8], rec: &Record) -> Option<Image> {
    let two_uids = rec.instance & 1 == 1;
    let uid_size = if two_uids { 32 } else { 16 };
    let body = slice(bytes, rec.body, rec.length);
    let content_type = match rec.kind {
        0xF01A => "image/x-emf",
        0xF01B => "image/x-wmf",
        0xF01C => "image/x-pict",
        0xF01D | 0xF02A => "image/jpeg",
        0xF01E => "image/png",
        0xF01F => "image/bmp",
        0xF029 => "image/tiff",
        _ => return None,
    };
    if matches!(rec.kind, 0xF01A..=0xF01C) {
        let header = body.get(uid_size..uid_size + 34)?;
        let raw_size = u32_at(header, 0)? as usize;
        let saved = u32_at(header, 28)? as usize;
        let compressed = header[32] == 0;
        let payload = slice(body, uid_size + 34, saved);
        let data = match compressed {
            true => inflate(payload, raw_size)?,
            false => payload.to_vec(),
        };
        return Some(Image {
            content_type,
            data: data.into(),
        });
    }
    let data = body.get(uid_size + 1..)?.to_vec();
    if rec.kind == 0xF01F {
        return Some(Image {
            content_type,
            data: dib_to_bmp(&data)?.into(),
        });
    }
    Some(Image {
        content_type,
        data: data.into(),
    })
}

fn inflate(data: &[u8], size_hint: usize) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(size_hint.min(64 << 20));
    flate2::read::ZlibDecoder::new(data)
        .take(256 << 20)
        .read_to_end(&mut out)
        .ok()?;
    Some(out)
}

/// Prepends a BITMAPFILEHEADER to a device-independent bitmap.
fn dib_to_bmp(dib: &[u8]) -> Option<Vec<u8>> {
    let header_size = u32_at(dib, 0)? as usize;
    let bit_count = u16_at(dib, 14).unwrap_or(24);
    let compression = u32_at(dib, 16).unwrap_or(0);
    let used = u32_at(dib, 32).unwrap_or(0) as usize;
    let palette = match (used, bit_count) {
        (0, 1..=8) => 1usize << bit_count,
        (n, _) => n,
    };
    let masks = if compression == 3 && header_size == 40 {
        12
    } else {
        0
    };
    let offset = 14 + header_size + palette * 4 + masks;
    let mut out = Vec::with_capacity(dib.len() + 14);
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&((dib.len() + 14) as u32).to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&(offset as u32).to_le_bytes());
    out.extend_from_slice(dib);
    Some(out)
}

/// The first BLIP found under the records in `bytes[start..end]`,
/// descending into containers and BLIP store entries.
pub fn find_blip(bytes: &[u8], start: usize, end: usize, depth: usize) -> Option<Image> {
    if depth > MAX_DEPTH {
        return None;
    }
    for child in children(bytes, start, end) {
        if (0xF018..=0xF117).contains(&child.kind) {
            if let Some(image) = blip(bytes, &child) {
                return Some(image);
            }
            continue;
        }
        if child.kind == 0xF007 {
            let name = usize::from(u8_at(bytes, child.body + 33).unwrap_or(0));
            let inner = child.body + 36 + name;
            if let Some(image) = find_blip(bytes, inner, child.body + child.length, depth + 1) {
                return Some(image);
            }
            continue;
        }
        if child.version == 0xF {
            if let Some(image) = find_blip(bytes, child.body, child.body + child.length, depth + 1)
            {
                return Some(image);
            }
        }
    }
    None
}

/// An inline picture: its image and displayed size in twips.
pub struct Picture {
    pub image: Option<Image>,
    pub width: i32,
    pub height: i32,
}

/// Reads a PICFAndOfficeArtData at `at` in the Data stream
/// ([MS-DOC] §2.9.192). A Word 6 or 95 PIC, in the WordDocument stream,
/// has the same fields in a 58-byte header and the picture itself after
/// it: a Windows metafile or a DIB.
pub fn inline_picture(data: &[u8], at: usize) -> Option<Picture> {
    let lcb = i32_at(data, at)?.max(0) as usize;
    let cb_header = usize::from(u16_at(data, at + 4)?);
    if cb_header < 0x3A || lcb < cb_header {
        return None;
    }
    let mm = u16_at(data, at + 6)?;
    let goal_x = i32::from(i16_at(data, at + 28)?);
    let goal_y = i32::from(i16_at(data, at + 30)?);
    let scale_x = i32::from(u16_at(data, at + 32).unwrap_or(1000));
    let scale_y = i32::from(u16_at(data, at + 34).unwrap_or(1000));
    let crop = |offset: usize| i32::from(i16_at(data, at + offset).unwrap_or(0));
    let width = (goal_x - crop(36) - crop(40)).max(0) * scale_x / 1000;
    let height = (goal_y - crop(38) - crop(42)).max(0) * scale_y / 1000;
    let mut start = at + cb_header;
    if mm == 0x66 {
        start += 1 + usize::from(u8_at(data, start).unwrap_or(0));
    }
    let end = at.saturating_add(lcb).min(data.len());
    let image = find_blip(data, start, end, 0)
        .or_else(|| raw_picture(slice(data, start, end - start.min(end))));
    Some(Picture {
        image,
        width,
        height,
    })
}

/// A picture stored bare after its header: a metafile starting with its
/// METAHEADER ([MS-WMF] §2.3.2.2, type 1 or 2 and a 9-word header) or a
/// DIB starting with a 40-byte BITMAPINFOHEADER.
fn raw_picture(bytes: &[u8]) -> Option<Image> {
    let kind = u16_at(bytes, 0)?;
    let header = u16_at(bytes, 2)?;
    if matches!(kind, 1 | 2) && header == 9 {
        return Some(Image {
            content_type: "image/x-wmf",
            data: bytes.into(),
        });
    }
    if u32_at(bytes, 0)? == 40 {
        return Some(Image {
            content_type: "image/bmp",
            data: dib_to_bmp(bytes)?.into(),
        });
    }
    None
}

/// A floating shape's BLIP store index from its shape container: the
/// `pib` property (0x0104) of its OfficeArtFOPT ([MS-ODRAW] §2.3.23.1).
pub fn shape_blip_index(bytes: &[u8], container: &Record) -> Option<u32> {
    let options = children(bytes, container.body, container.body + container.length)
        .into_iter()
        .find(|r| r.kind == 0xF00B)?;
    let count = usize::from(options.instance);
    (0..count).find_map(|i| {
        let at = options.body + i * 6;
        let id = u16_at(bytes, at)? & 0x3FFF;
        (id == 0x0104).then(|| u32_at(bytes, at + 2)).flatten()
    })
}

/// A shape's fill, line and text frame from the simple properties of its
/// OfficeArtFOPT, with the [MS-ODRAW] defaults for unstated ones: a white
/// fill, a black 0.75 pt line, insets of 0.1 by 0.05 inch and text at the
/// top. A color given as a scheme or system index is left unstated.
/// [MS-ODRAW] §2.3.7.2, §2.3.7.43, §2.3.8.1, §2.3.8.14, §2.3.8.38, §2.3.21.2, §2.3.21.8, §2.3.21.15;
/// the dash, join and cap from [MS-ODRAW] §2.3.8.17, §2.3.8.26, §2.3.8.27, §2.4.19 and §2.4.20;
/// the line ends from [MS-ODRAW] §2.3.8.20, §2.3.8.21, §2.3.8.22, §2.3.8.23, §2.3.8.24, §2.3.8.25;
/// the rotation from [MS-ODRAW] §2.3.18.5 and the flips from the OfficeArtFSP ([MS-ODRAW] §2.2.40).
pub fn shape_format(bytes: &[u8], container: &Record) -> ShapeFormat {
    let mut format = ShapeFormat {
        fill: Some(Color::WHITE),
        outline: Some(Color::BLACK),
        outline_width: Some(9525),
        outline_dash: None,
        outline_cap: LineCap::Flat,
        outline_join: LineJoin::Round,
        insets: Some([91440, 45720, 91440, 45720]),
        text_anchor: None,
        auto_fit: false,
        ..Default::default()
    };
    let records = children(bytes, container.body, container.body + container.length);
    if let Some(flags) = records
        .iter()
        .find(|r| r.kind == 0xF00A)
        .and_then(|fsp| u32_at(bytes, fsp.body + 4))
    {
        format.flip_horizontal = flags & 0x40 != 0;
        format.flip_vertical = flags & 0x80 != 0;
    }
    let Some(options) = records.into_iter().find(|r| r.kind == 0xF00B) else {
        return format;
    };
    let mut ends = [0u32, 0, 1, 1, 1, 1];
    let color = |value: u32| {
        let [r, g, b, flags] = value.to_le_bytes();
        (flags == 0).then_some(Color(r, g, b))
    };
    let mut insets = [91440i64, 45720, 91440, 45720];
    let mut shade = (0u32, Color::WHITE, 0i32, 0i32);
    for i in 0..usize::from(options.instance) {
        let at = options.body + i * 6;
        let (Some(id), Some(value)) = (u16_at(bytes, at), u32_at(bytes, at + 2)) else {
            break;
        };
        match id & 0x3FFF {
            0x0081..=0x0084 => insets[usize::from((id & 0x3FFF) - 0x0081)] = i64::from(value),
            0x0087 => {
                format.text_anchor = match value {
                    1 | 4 => Some(VerticalAlign::Center),
                    2 | 5 => Some(VerticalAlign::Bottom),
                    _ => Some(VerticalAlign::Top),
                }
            }
            0x0088 => format.text_direction = text_flow(value),
            0x00BF => format.auto_fit = value & 0x0002_0002 == 0x0002_0002,
            0x0181 => format.fill = color(value),
            0x0180 => shade.0 = value,
            0x0183 => shade.1 = color(value).unwrap_or(Color::WHITE),
            0x018B => shade.2 = value as i32,
            0x018C => shade.3 = value as i32,
            0x01BF if value & 0x0010_0000 != 0 && value & 0x10 == 0 => format.fill = None,
            0x01C0 => format.outline = color(value),
            0x01CB => format.outline_width = Some(i64::from(value)),
            0x01CE => format.outline_dash = line_dashing(value),
            0x01D6 => {
                format.outline_join = match value {
                    0 => LineJoin::Bevel,
                    1 => LineJoin::Miter,
                    _ => LineJoin::Round,
                }
            }
            0x01D7 => {
                format.outline_cap = match value {
                    0 => LineCap::Round,
                    1 => LineCap::Square,
                    _ => LineCap::Flat,
                }
            }
            0x01FF if value & 0x0008_0000 != 0 && value & 0x08 == 0 => format.outline = None,
            0x0004 => {
                let degrees = f64::from(value as i32) / 65_536.0;
                format.rotation =
                    ((degrees * 60_000.0).round() as i64).rem_euclid(21_600_000) as i32;
            }
            0x01D0..=0x01D5 => ends[usize::from((id & 0x3FFF) - 0x01D0)] = value,
            _ => {}
        }
    }
    format.insets = Some(insets);
    format.gradient = format.fill.and_then(|fill| shaded_fill(shade, fill));
    if let Some(gradient) = format.gradient {
        format.fill = Some(gradient.average());
    }
    format.head_end = line_end(ends[0], ends[2], ends[3]);
    format.tail_end = line_end(ends[1], ends[4], ends[5]);
    format
}

/// [MS-ODRAW] §2.3.21.9, §2.4.5: the txflTextFlow of a text box; the
/// vertical flows other than msotxflBtoT run top to bottom.
fn text_flow(value: u32) -> TextDirection {
    match value {
        1 | 3 | 5 => TextDirection::TopToBottom,
        2 => TextDirection::BottomToTop,
        _ => TextDirection::LeftToRight,
    }
}

/// The gradient of a shaded fill ([MS-ODRAW] §2.3.7.1 fillType of
/// msofillShade to msofillShadeTitle, §2.4.11) from `fill` to its back
/// color (§2.3.7.4), along the direction fillAngle turns counterclockwise
/// (§2.3.7.14), with fillFocus placing the back color (§2.3.7.15); the
/// center and shape shades spread from the middle. An angle of zero runs
/// top to bottom: §2.3.7.14 names the vector from bottom to top, but the
/// DrawingML copy Word writes of the same fill, and LibreOffice, run it
/// downwards.
fn shaded_fill(
    (kind, back, angle, focus): (u32, Color, i32, i32),
    fill: Color,
) -> Option<Gradient> {
    if !(4..=8).contains(&kind) {
        return None;
    }
    let focus = f64::from(focus.clamp(-100, 100)) / 100.0;
    let mut gradient = Gradient::focused(&[(0, fill), (100_000, back)], focus)?;
    let degrees = f64::from(angle) / 65_536.0;
    gradient.angle = (((90.0 - degrees) * 60_000.0).round() as i64).rem_euclid(21_600_000) as i32;
    if matches!(kind, 5 | 6 | 8) {
        gradient.path = Some(GradientPath::Rect);
        gradient.focus = [50_000; 4];
    }
    Some(gradient)
}

/// [MS-ODRAW] §2.4.16, §2.4.17, §2.4.18: a line end from its MSOLINEEND,
/// MSOLINEENDWIDTH and MSOLINEENDLENGTH values; `None` for no end or an
/// ignored chevron.
fn line_end(kind: u32, width: u32, length: u32) -> Option<LineEnd> {
    let kind = match kind {
        1 => LineEndKind::Triangle,
        2 => LineEndKind::Stealth,
        3 => LineEndKind::Diamond,
        4 => LineEndKind::Oval,
        5 => LineEndKind::Arrow,
        _ => return None,
    };
    let size = |value: u32| match value {
        0 => LineEndSize::Small,
        2 => LineEndSize::Large,
        _ => LineEndSize::Medium,
    };
    Some(LineEnd {
        kind,
        width: size(width),
        length: size(length),
    })
}

/// A shape's preset geometry from the shape type in its OfficeArtFSP
/// header ([MS-ODRAW] §2.2.40, §2.4.24) and the adjust values of its
/// OfficeArtFOPT (§2.3.6.10 to §2.3.6.17); `None` for a group, a picture
/// frame, a custom shape or WordArt.
pub fn shape_geometry(bytes: &[u8], container: &Record) -> Option<Geometry> {
    let records = children(bytes, container.body, container.body + container.length);
    let fsp = records.iter().find(|r| r.kind == 0xF00A)?;
    let mut values = [None; 8];
    if let Some(options) = records.iter().find(|r| r.kind == 0xF00B) {
        for i in 0..usize::from(options.instance) {
            let at = options.body + i * 6;
            let (Some(id), Some(value)) = (u16_at(bytes, at), u32_at(bytes, at + 2)) else {
                break;
            };
            if let Some(slot) = (id & 0x3FFF).checked_sub(0x0147).filter(|s| *s < 8) {
                values[usize::from(slot)] = Some(i64::from(value as i32));
            }
        }
    }
    Geometry::from_shape_type_adjusted(u32::from(fsp.instance), &values)
}

/// [MS-ODRAW] §2.4.15: the pattern of an MSOLINEDASHING value, `None` for
/// a solid line or an unknown value.
fn line_dashing(value: u32) -> Option<DashPattern> {
    let bits = match value {
        1 => "1110",
        2 => "10",
        3 => "111010",
        4 => "11101010",
        5 => "1000",
        6 => "1111000",
        7 => "11111111000",
        8 => "11110001000",
        9 => "111111110001000",
        10 => "1111111100010001000",
        _ => return None,
    };
    DashPattern::from_bits(bits)
}

/// A shape's alignment on each axis, horizontal then vertical: the posh
/// and posrelh, posv and posrelv properties of its OfficeArtFOPT or
/// OfficeArtTertiaryFOPT. None on an axis positioned by offset.
/// [MS-ODRAW] §2.3.4.19, §2.3.4.20, §2.3.4.21, §2.3.4.22.
pub fn shape_alignment(
    bytes: &[u8],
    container: &Record,
) -> [Option<(PositionAlign, PositionBase)>; 2] {
    let mut values = [0u32, 3, 0, 3];
    let tables = children(bytes, container.body, container.body + container.length)
        .into_iter()
        .filter(|r| r.kind == 0xF00B || r.kind == 0xF122);
    for options in tables {
        for i in 0..usize::from(options.instance) {
            let at = options.body + i * 6;
            let (Some(id), Some(value)) = (u16_at(bytes, at), u32_at(bytes, at + 2)) else {
                break;
            };
            if let Some(slot) = (id & 0x3FFF).checked_sub(0x038F).filter(|slot| *slot < 4) {
                values[usize::from(slot)] = value;
            }
        }
    }
    let align = |value: u32| match value {
        1 => Some(PositionAlign::Start),
        2 => Some(PositionAlign::Center),
        3 => Some(PositionAlign::End),
        4 => Some(PositionAlign::Inside),
        5 => Some(PositionAlign::Outside),
        _ => None,
    };
    let base = |value: u32, horizontal: bool| match (value, horizontal) {
        (1, _) => PositionBase::Margin,
        (2, _) => PositionBase::Page,
        (4, true) => PositionBase::Character,
        (4, false) => PositionBase::Line,
        (_, true) => PositionBase::Column,
        (_, false) => PositionBase::Paragraph,
    };
    [
        align(values[0]).map(|a| (a, base(values[1], true))),
        align(values[2]).map(|a| (a, base(values[3], false))),
    ]
}

/// A shape id from a shape container's OfficeArtFSP ([MS-ODRAW] §2.2.40).
pub fn shape_id(bytes: &[u8], container: &Record) -> Option<u32> {
    let fsp = children(bytes, container.body, container.body + container.length)
        .into_iter()
        .find(|r| r.kind == 0xF00A)?;
    u32_at(bytes, fsp.body)
}

/// A rectangle as OfficeArtFSPGR and OfficeArtChildAnchor write it: left,
/// top, right and bottom.
pub type Bounds = [i32; 4];

/// A group of shapes ([MS-ODRAW] §2.2.16 OfficeArtSpgrContainer): the shape
/// id of the group shape, the coordinate space its members' anchors are in
/// (§2.2.38 OfficeArtFSPGR), and the members in drawing order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShapeGroup {
    pub id: u32,
    pub space: Bounds,
    pub members: Vec<GroupChild>,
}

/// A member of a group with its anchor in the group's coordinates
/// ([MS-ODRAW] §2.2.39 OfficeArtChildAnchor).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GroupChild {
    Shape { id: u32, anchor: Bounds },
    Group { anchor: Bounds, group: ShapeGroup },
}

fn bounds(bytes: &[u8], rec: &Record) -> Option<Bounds> {
    let value = |i: usize| u32_at(bytes, rec.body + i * 4).map(|v| v as i32);
    Some([value(0)?, value(1)?, value(2)?, value(3)?])
}

fn child_record(bytes: &[u8], container: &Record, kind: u16) -> Option<Record> {
    children(bytes, container.body, container.body + container.length)
        .into_iter()
        .find(|r| r.kind == kind)
}

/// The groups directly inside the patriarch group of a drawing container
/// (0xF002) in `bytes[start..end]`, with the groups nested in them.
pub fn shape_groups(bytes: &[u8], start: usize, end: usize, out: &mut Vec<ShapeGroup>) {
    for drawing in children(bytes, start, end) {
        if drawing.kind != 0xF003 {
            continue;
        }
        for child in children(bytes, drawing.body, drawing.body + drawing.length) {
            if child.kind != 0xF003 {
                continue;
            }
            if let Some((group, _)) = shape_group(bytes, &child, 0) {
                out.push(group);
            }
        }
    }
}

/// A group container ([MS-ODRAW] §2.2.16): its first shape container is
/// the group shape, with its coordinate space and, for a nested group, its
/// own anchor; the other records are its members. Returns the group and
/// its anchor in its parent's coordinates.
fn shape_group(bytes: &[u8], container: &Record, depth: usize) -> Option<(ShapeGroup, Bounds)> {
    if depth > MAX_DEPTH {
        return None;
    }
    let records = children(bytes, container.body, container.body + container.length);
    let (first, rest) = records.split_first()?;
    if first.kind != 0xF004 {
        return None;
    }
    let id = shape_id(bytes, first)?;
    let space = child_record(bytes, first, 0xF009)
        .and_then(|r| bounds(bytes, &r))
        .unwrap_or_default();
    let anchor = child_record(bytes, first, 0xF00F)
        .and_then(|r| bounds(bytes, &r))
        .unwrap_or_default();
    let members = rest
        .iter()
        .filter_map(|member| match member.kind {
            0xF004 => Some(GroupChild::Shape {
                id: shape_id(bytes, member)?,
                anchor: child_record(bytes, member, 0xF00F).and_then(|r| bounds(bytes, &r))?,
            }),
            0xF003 => {
                let (group, anchor) = shape_group(bytes, member, depth + 1)?;
                Some(GroupChild::Group { anchor, group })
            }
            _ => None,
        })
        .collect();
    Some((ShapeGroup { id, space, members }, anchor))
}

/// Every shape container under `bytes[start..end]`.
pub fn shape_containers(
    bytes: &[u8],
    start: usize,
    end: usize,
    depth: usize,
    out: &mut Vec<Record>,
) {
    if depth > MAX_DEPTH {
        return;
    }
    for child in children(bytes, start, end) {
        if child.kind == 0xF004 {
            out.push(child);
            continue;
        }
        if child.version == 0xF {
            shape_containers(bytes, child.body, child.body + child.length, depth + 1, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(version: u16, instance: u16, kind: u16, length: usize) -> Vec<u8> {
        let mut out = (version | instance << 4).to_le_bytes().to_vec();
        out.extend(kind.to_le_bytes());
        out.extend((length as u32).to_le_bytes());
        out
    }

    fn container(properties: &[(u16, u32)]) -> Vec<u8> {
        shape(202, 0, properties)
    }

    fn shape(spt: u16, flags: u32, properties: &[(u16, u32)]) -> Vec<u8> {
        let fsp: Vec<u8> = header(2, spt, 0xF00A, 8)
            .into_iter()
            .chain([7, 0, 0, 0])
            .chain(flags.to_le_bytes())
            .collect();
        let mut fopt = header(3, properties.len() as u16, 0xF00B, properties.len() * 6);
        for (id, value) in properties {
            fopt.extend(id.to_le_bytes());
            fopt.extend(value.to_le_bytes());
        }
        let mut out = header(0xF, 0, 0xF004, fsp.len() + fopt.len());
        out.extend(fsp);
        out.extend(fopt);
        out
    }

    /// [MS-ODRAW] §2.3.7.2, §2.3.7.43, §2.3.8.38, §2.3.21.2, §2.3.21.8,
    /// §2.3.21.15: unstated properties take their defaults, and the boolean
    /// sets only count where their `fUse` bit is set; §2.3.21.9 and §2.4.5:
    /// the text flow.
    #[test]
    fn shape_format_reads_fill_line_insets_and_anchor() {
        let bytes = container(&[]);
        let shape = shape_format(&bytes, &record(&bytes, 0).unwrap());
        assert_eq!(shape.fill, Some(Color::WHITE));
        assert_eq!(shape.outline, Some(Color::BLACK));
        assert_eq!(shape.insets, Some([91_440, 45_720, 91_440, 45_720]));

        let bytes = container(&[
            (0x0081, 0),
            (0x0087, 1),
            (0x00BF, 0x0002_0002),
            (0x0181, 0x0000_FF00),
            (0x01C0, 0x0800_0000),
            (0x01FF, 0x0008_0000),
            (0x01CB, 25_400),
        ]);
        let shape = shape_format(&bytes, &record(&bytes, 0).unwrap());
        assert_eq!(shape.fill, Some(Color(0, 255, 0)));
        assert_eq!(shape.outline, None);
        assert_eq!(shape.outline_width, Some(25_400));
        assert_eq!(shape.insets, Some([0, 45_720, 91_440, 45_720]));
        assert_eq!(shape.text_anchor, Some(VerticalAlign::Center));
        assert!(shape.auto_fit);

        let bytes = container(&[(0x01BF, 0x0010_0000)]);
        assert_eq!(shape_format(&bytes, &record(&bytes, 0).unwrap()).fill, None);

        let bytes = container(&[(0x0088, 2)]);
        assert_eq!(
            shape_format(&bytes, &record(&bytes, 0).unwrap()).text_direction,
            TextDirection::BottomToTop
        );
    }

    /// [MS-ODRAW] §2.2.40, §2.4.24: the shape type and flips of the
    /// OfficeArtFSP; §2.3.18.5: the rotation; §2.3.8.20 to §2.3.8.25 and
    /// §2.4.16 to §2.4.18: the line ends.
    /// [MS-ODRAW] §2.3.8.20, §2.3.8.21, §2.3.8.22, §2.3.8.23, §2.3.8.24, §2.3.8.25.
    /// [MS-ODRAW] §2.4.16, §2.4.17, §2.4.18, §2.2.40, §2.4.24, §2.3.18.5, §2.3.6.10.
    #[test]
    fn shape_type_flips_rotation_and_line_ends() {
        let bytes = shape(
            20,
            0x80 | 0x800,
            &[
                (0x0004, 90 << 16),
                (0x01D1, 1),
                (0x01D4, 2),
                (0x01D5, 0),
                (0x01D0, 6),
            ],
        );
        let container = record(&bytes, 0).unwrap();
        let format = shape_format(&bytes, &container);
        assert!(format.flip_vertical && !format.flip_horizontal);
        assert_eq!(format.rotation, 5_400_000);
        assert_eq!(format.head_end, None);
        assert_eq!(
            format.tail_end,
            Some(LineEnd {
                kind: LineEndKind::Triangle,
                width: LineEndSize::Large,
                length: LineEndSize::Small,
            })
        );
        assert_eq!(
            shape_geometry(&bytes, &container),
            Geometry::from_shape_type(20)
        );
        let bytes = shape(0, 0x1, &[]);
        assert_eq!(shape_geometry(&bytes, &record(&bytes, 0).unwrap()), None);
        let bytes = shape(5, 0, &[(0x0147, 5400)]);
        assert_eq!(
            shape_geometry(&bytes, &record(&bytes, 0).unwrap()),
            Some(Geometry::Preset {
                name: "triangle".into(),
                adjust: vec![("adj".into(), 25_000)],
            })
        );
        let bytes = shape(1, 0, &[(0x0004, (-45i32 << 16) as u32)]);
        let format = shape_format(&bytes, &record(&bytes, 0).unwrap());
        assert_eq!(format.rotation, 18_900_000);
    }

    /// [MS-ODRAW] §2.3.8.17 and §2.4.15: lineDashing presets; §2.3.8.26,
    /// §2.4.19, §2.3.8.27 and §2.4.20: the join and end cap, round and flat
    /// when unstated.
    #[test]
    fn shape_format_reads_line_dashing_join_and_cap() {
        let bytes = container(&[]);
        let shape = shape_format(&bytes, &record(&bytes, 0).unwrap());
        assert_eq!(shape.outline_dash, None);
        assert_eq!(shape.outline_join, LineJoin::Round);
        assert_eq!(shape.outline_cap, LineCap::Flat);

        let bytes = container(&[(0x01CE, 6), (0x01D6, 1), (0x01D7, 0)]);
        let shape = shape_format(&bytes, &record(&bytes, 0).unwrap());
        assert_eq!(shape.outline_dash.unwrap().stops(), &[(400, 300)]);
        assert_eq!(shape.outline_join, LineJoin::Miter);
        assert_eq!(shape.outline_cap, LineCap::Round);

        let bytes = container(&[(0x01CE, 0), (0x01D6, 0), (0x01D7, 1)]);
        let shape = shape_format(&bytes, &record(&bytes, 0).unwrap());
        assert_eq!(shape.outline_dash, None);
        assert_eq!(shape.outline_join, LineJoin::Bevel);
        assert_eq!(shape.outline_cap, LineCap::Square);
        assert_eq!(line_dashing(2).unwrap().stops(), &[(100, 100)]);
        assert_eq!(line_dashing(11), None);
    }

    /// [MS-ODRAW] §2.3.4.19, §2.3.4.20, §2.3.4.21, §2.3.4.22: msophAbs
    /// leaves an axis to its offset; posrelh and posrelv default to the
    /// text.
    #[test]
    fn shape_alignment_reads_posh_and_posv() {
        let bytes = container(&[]);
        assert_eq!(
            shape_alignment(&bytes, &record(&bytes, 0).unwrap()),
            [None, None]
        );

        let bytes = container(&[(0x038F, 2), (0x0391, 3), (0x0392, 2)]);
        assert_eq!(
            shape_alignment(&bytes, &record(&bytes, 0).unwrap()),
            [
                Some((PositionAlign::Center, PositionBase::Column)),
                Some((PositionAlign::End, PositionBase::Page)),
            ]
        );
    }
}

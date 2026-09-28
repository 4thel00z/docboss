//! Pictures: the PICF header in the Data stream ([MS-DOC] §2.9.190) and the
//! OfficeArt BLIP records that hold the image bytes ([MS-ODRAW] §2.2.23 to
//! §2.2.31).

use std::io::Read;

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
    pub data: Vec<u8>,
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
        return Some(Image { content_type, data });
    }
    let data = body.get(uid_size + 1..)?.to_vec();
    if rec.kind == 0xF01F {
        return Some(Image {
            content_type,
            data: dib_to_bmp(&data)?,
        });
    }
    Some(Image { content_type, data })
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
/// ([MS-DOC] §2.9.192).
pub fn inline_picture(data: &[u8], at: usize) -> Option<Picture> {
    let lcb = i32_at(data, at)?.max(0) as usize;
    let cb_header = usize::from(u16_at(data, at + 4)?);
    if cb_header < 0x44 || lcb < cb_header {
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
    let image = find_blip(data, start, end, 0);
    Some(Picture {
        image,
        width,
        height,
    })
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

/// A shape id from a shape container's OfficeArtFSP ([MS-ODRAW] §2.2.40).
pub fn shape_id(bytes: &[u8], container: &Record) -> Option<u32> {
    let fsp = children(bytes, container.body, container.body + container.length)
        .into_iter()
        .find(|r| r.kind == 0xF00A)?;
    u32_at(bytes, fsp.body)
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

//! Pixel dimensions from image headers, for sizing pictures.

fn be16(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from(u16::from_be_bytes(
        bytes.get(at..at + 2)?.try_into().ok()?,
    )))
}

fn be32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

fn le16(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from(u16::from_le_bytes(
        bytes.get(at..at + 2)?.try_into().ok()?,
    )))
}

fn le32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(i32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?).unsigned_abs())
}

fn jpeg(bytes: &[u8]) -> Option<(u32, u32)> {
    let mut at = 2;
    while at + 9 < bytes.len() {
        if bytes[at] != 0xFF {
            return None;
        }
        let marker = bytes[at + 1];
        let length = be16(bytes, at + 2)? as usize;
        let is_frame = matches!(marker, 0xC0..=0xCF) && !matches!(marker, 0xC4 | 0xC8 | 0xCC);
        if is_frame {
            return Some((be16(bytes, at + 7)?, be16(bytes, at + 5)?));
        }
        at += 2 + length;
    }
    None
}

/// Width and height in pixels of a PNG, JPEG, GIF or BMP image.
pub fn image_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    match docboss_model::sniff_image(bytes) {
        "image/png" => Some((be32(bytes, 16)?, be32(bytes, 20)?)),
        "image/jpeg" => jpeg(bytes),
        "image/gif" => Some((le16(bytes, 6)?, le16(bytes, 8)?)),
        "image/bmp" => Some((le32(bytes, 18)?, le32(bytes, 22)?)),
        _ => None,
    }
}

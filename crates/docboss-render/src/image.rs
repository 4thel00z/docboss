//! Decoding embedded images to RGBA: PNG and JPEG through the `png` and
//! `jpeg-decoder` crates, GIF (first frame) in-tree and BMP through the
//! metafile crate's DIB decoder. WMF and EMF are played at layout time.

const MAX_PIXELS: u64 = 64 << 20;

/// A decoded image, straight RGBA.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decoded {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Why an image could not be decoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageError {
    /// A format this decoder does not read: TIFF, or a WMF or EMF picture
    /// that the layout could not play.
    Unsupported(&'static str),
    Malformed(String),
}

impl std::fmt::Display for ImageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImageError::Unsupported(kind) => write!(f, "{kind} images are not painted"),
            ImageError::Malformed(why) => write!(f, "undecodable image: {why}"),
        }
    }
}

fn checked(width: u32, height: u32) -> Result<(), ImageError> {
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err(ImageError::Malformed(format!(
            "{width}x{height} is out of range"
        )));
    }
    Ok(())
}

pub fn decode(bytes: &[u8]) -> Result<Decoded, ImageError> {
    match docboss_model::sniff_image(bytes) {
        "image/png" => decode_png(bytes),
        "image/jpeg" => decode_jpeg(bytes),
        "image/gif" => decode_gif(bytes),
        "image/bmp" => decode_bmp(bytes),
        "image/tiff" => Err(ImageError::Unsupported("TIFF")),
        "image/x-wmf" => Err(ImageError::Unsupported("WMF")),
        "image/x-emf" => Err(ImageError::Unsupported("EMF")),
        _ => Err(ImageError::Unsupported("unrecognized")),
    }
}

fn decode_png(bytes: &[u8]) -> Result<Decoded, ImageError> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let bad = |e: png::DecodingError| ImageError::Malformed(e.to_string());
    let mut reader = decoder.read_info().map_err(bad)?;
    let (width, height) = (reader.info().width, reader.info().height);
    checked(width, height)?;
    let size = reader
        .output_buffer_size()
        .ok_or_else(|| ImageError::Malformed("PNG size".into()))?;
    let mut buffer = vec![0u8; size];
    let frame = reader.next_frame(&mut buffer).map_err(bad)?;
    buffer.truncate(frame.buffer_size());
    let rgba = match frame.color_type {
        png::ColorType::Rgba => buffer,
        png::ColorType::Rgb => buffer
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => buffer
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        png::ColorType::Grayscale => buffer.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return Err(ImageError::Malformed("unexpanded palette".into())),
    };
    Ok(Decoded {
        width,
        height,
        rgba,
    })
}

fn decode_jpeg(bytes: &[u8]) -> Result<Decoded, ImageError> {
    let mut decoder = jpeg_decoder::Decoder::new(std::io::Cursor::new(bytes));
    let pixels = decoder
        .decode()
        .map_err(|e| ImageError::Malformed(e.to_string()))?;
    let info = decoder
        .info()
        .ok_or_else(|| ImageError::Malformed("JPEG header".into()))?;
    let (width, height) = (u32::from(info.width), u32::from(info.height));
    checked(width, height)?;
    let rgba = match info.pixel_format {
        jpeg_decoder::PixelFormat::RGB24 => pixels
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        jpeg_decoder::PixelFormat::L8 => pixels.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        jpeg_decoder::PixelFormat::L16 => pixels
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[0], p[0], 255])
            .collect(),
        jpeg_decoder::PixelFormat::CMYK32 => pixels
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| {
                let k = f32::from(p[3]) / 255.0;
                let channel = |c: u8| (f32::from(c) * k) as u8;
                [channel(p[0]), channel(p[1]), channel(p[2]), 255]
            })
            .collect(),
    };
    Ok(Decoded {
        width,
        height,
        rgba,
    })
}

fn le16(d: &[u8], o: usize) -> Option<u16> {
    d.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]]))
}

/// BMP through the metafile crate's DIB decoder: 1 to 32 bits per pixel,
/// bit fields and RLE.
fn decode_bmp(d: &[u8]) -> Result<Decoded, ImageError> {
    let image = docboss_metafile::bitmap::decode_bmp(d).map_err(|e| match e {
        docboss_metafile::bitmap::BitmapError::Unsupported(what) => ImageError::Unsupported(what),
        docboss_metafile::bitmap::BitmapError::Malformed(why) => ImageError::Malformed(why.into()),
    })?;
    checked(image.width, image.height)?;
    Ok(Decoded {
        width: image.width,
        height: image.height,
        rgba: image.pixels,
    })
}

/// The first frame of a GIF: global or local palette, LZW, interlacing.
fn decode_gif(d: &[u8]) -> Result<Decoded, ImageError> {
    let bad = |what: &str| ImageError::Malformed(format!("GIF {what}"));
    let width = le16(d, 6).ok_or_else(|| bad("header"))? as u32;
    let height = le16(d, 8).ok_or_else(|| bad("header"))? as u32;
    checked(width, height)?;
    let flags = *d.get(10).ok_or_else(|| bad("header"))?;
    let mut p = 13;
    let mut palette: Vec<[u8; 3]> = Vec::new();
    if flags & 0x80 != 0 {
        let size = 3 << ((flags & 7) + 1);
        let table = d.get(p..p + size).ok_or_else(|| bad("palette"))?;
        palette = table
            .as_chunks::<3>()
            .0
            .iter()
            .map(|c| [c[0], c[1], c[2]])
            .collect();
        p += size;
    }
    let mut transparent: Option<u8> = None;
    loop {
        match d.get(p).copied() {
            Some(0x21) => {
                let label = *d.get(p + 1).ok_or_else(|| bad("extension"))?;
                if label == 0xF9 && d.get(p + 3).is_some_and(|f| f & 1 != 0) {
                    transparent = d.get(p + 6).copied();
                }
                p += 2;
                while let Some(&len) = d.get(p) {
                    p += 1 + len as usize;
                    if len == 0 {
                        break;
                    }
                }
            }
            Some(0x2C) => break,
            _ => return Err(bad("image descriptor")),
        }
    }
    let left = le16(d, p + 1).ok_or_else(|| bad("descriptor"))? as u32;
    let top = le16(d, p + 3).ok_or_else(|| bad("descriptor"))? as u32;
    let fw = le16(d, p + 5).ok_or_else(|| bad("descriptor"))? as u32;
    let fh = le16(d, p + 7).ok_or_else(|| bad("descriptor"))? as u32;
    let local = *d.get(p + 9).ok_or_else(|| bad("descriptor"))?;
    p += 10;
    if local & 0x80 != 0 {
        let size = 3 << ((local & 7) + 1);
        let table = d.get(p..p + size).ok_or_else(|| bad("palette"))?;
        palette = table
            .as_chunks::<3>()
            .0
            .iter()
            .map(|c| [c[0], c[1], c[2]])
            .collect();
        p += size;
    }
    let min_code = *d.get(p).ok_or_else(|| bad("LZW"))?;
    p += 1;
    let mut data = Vec::new();
    while let Some(&len) = d.get(p) {
        p += 1;
        if len == 0 {
            break;
        }
        data.extend_from_slice(d.get(p..p + len as usize).ok_or_else(|| bad("data"))?);
        p += len as usize;
    }
    let indices = lzw(&data, min_code, (fw * fh) as usize)?;
    let rows: Vec<u32> = if local & 0x40 != 0 {
        [(0, 8), (4, 8), (2, 4), (1, 2)]
            .iter()
            .flat_map(|&(s, step)| (s..fh).step_by(step))
            .collect()
    } else {
        (0..fh).collect()
    };
    let mut rgba = vec![0u8; width as usize * height as usize * 4];
    for (i, &index) in indices.iter().enumerate() {
        let (fx, row) = (i as u32 % fw.max(1), i as u32 / fw.max(1));
        let Some(&fy) = rows.get(row as usize) else {
            break;
        };
        let (x, y) = (left + fx, top + fy);
        if x >= width || y >= height || Some(index) == transparent {
            continue;
        }
        let [r, g, b] = palette.get(index as usize).copied().unwrap_or([0, 0, 0]);
        rgba[((y * width + x) * 4) as usize..][..4].copy_from_slice(&[r, g, b, 255]);
    }
    Ok(Decoded {
        width,
        height,
        rgba,
    })
}

fn lzw(data: &[u8], min_code: u8, limit: usize) -> Result<Vec<u8>, ImageError> {
    if !(1..=11).contains(&min_code) {
        return Err(ImageError::Malformed("GIF code size".into()));
    }
    let clear = 1u16 << min_code;
    let end = clear + 1;
    let mut prefix: Vec<u16> = vec![0; 4096];
    let mut suffix: Vec<u8> = vec![0; 4096];
    let mut first: Vec<u8> = vec![0; 4096];
    for i in 0..clear {
        suffix[i as usize] = i as u8;
        first[i as usize] = i as u8;
    }
    let mut out = Vec::with_capacity(limit);
    let mut size = min_code + 1;
    let mut next = end + 1;
    let mut previous: Option<u16> = None;
    let (mut bits, mut count, mut pos) = (0u32, 0u8, 0usize);
    let mut stack = Vec::with_capacity(4096);
    while out.len() < limit {
        while count < size {
            let Some(&byte) = data.get(pos) else {
                return Ok(out);
            };
            bits |= u32::from(byte) << count;
            count += 8;
            pos += 1;
        }
        let code = (bits & ((1 << size) - 1)) as u16;
        bits >>= size;
        count -= size;
        if code == clear {
            size = min_code + 1;
            next = end + 1;
            previous = None;
            continue;
        }
        if code == end {
            break;
        }
        let Some(prev) = previous else {
            if code >= clear {
                return Err(ImageError::Malformed("GIF LZW".into()));
            }
            out.push(code as u8);
            previous = Some(code);
            continue;
        };
        let known = code < next;
        let mut walk = if known { code } else { prev };
        stack.clear();
        if !known {
            stack.push(first[prev as usize]);
        }
        while walk > end {
            stack.push(suffix[walk as usize]);
            walk = prefix[walk as usize];
        }
        stack.push(walk as u8);
        let head = walk as u8;
        out.extend(stack.iter().rev());
        if next < 4096 {
            prefix[next as usize] = prev;
            suffix[next as usize] = head;
            first[next as usize] = first[prev as usize];
            next += 1;
            if next == 1 << size && size < 12 {
                size += 1;
            }
        }
        previous = Some(code);
    }
    out.truncate(limit);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_a_two_by_two_gif() {
        let gif: &[u8] = &[
            0x47, 0x49, 0x46, 0x38, 0x39, 0x61, 0x02, 0x00, 0x02, 0x00, 0x80, 0x00, 0x00, 0xFF,
            0x00, 0x00, 0x00, 0x00, 0xFF, 0x2C, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x02, 0x00,
            0x00, 0x02, 0x03, 0x44, 0x34, 0x05, 0x00, 0x3B,
        ];
        let image = decode(gif).unwrap();
        assert_eq!((image.width, image.height), (2, 2));
        assert_eq!(&image.rgba[..4], &[255, 0, 0, 255]);
        assert_eq!(&image.rgba[4..8], &[0, 0, 255, 255]);
    }

    #[test]
    fn decodes_a_bmp() {
        let mut bmp = vec![0u8; 54];
        bmp[..2].copy_from_slice(b"BM");
        bmp[10..14].copy_from_slice(&54u32.to_le_bytes());
        bmp[14..18].copy_from_slice(&40u32.to_le_bytes());
        bmp[18..22].copy_from_slice(&1u32.to_le_bytes());
        bmp[22..26].copy_from_slice(&1u32.to_le_bytes());
        bmp[28..30].copy_from_slice(&24u16.to_le_bytes());
        bmp.extend_from_slice(&[10, 20, 30, 0]);
        let image = decode(&bmp).unwrap();
        assert_eq!(image.rgba, vec![30, 20, 10, 255]);
    }

    #[test]
    fn metafiles_are_reported_unsupported() {
        assert_eq!(
            decode(b"\xd7\xcd\xc6\x9a\0\0"),
            Err(ImageError::Unsupported("WMF"))
        );
        assert!(decode(b"garbage").is_err());
    }
}

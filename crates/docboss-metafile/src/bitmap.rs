//! Device-independent bitmaps ([MS-WMF] §2.2.2.9 DeviceIndependentBitmap):
//! the BitmapCoreHeader (§2.2.2.2), BitmapInfoHeader (§2.2.2.3) and the
//! V4 and V5 headers (§2.2.2.4, §2.2.2.5), 1 to 32 bits per pixel,
//! palettes, bit fields and RLE4 and RLE8 compression (§2.1.1.7), decoded
//! to straight RGBA.

use crate::bytes::{i32_at, u16_at, u32_at, u8_at};

/// The most pixels a bitmap may hold.
pub const MAX_PIXELS: u64 = 64 << 20;

const BI_RGB: u32 = 0;
const BI_RLE8: u32 = 1;
const BI_RLE4: u32 = 2;
const BI_BITFIELDS: u32 = 3;
const BI_ALPHABITFIELDS: u32 = 6;

/// A decoded bitmap, straight RGBA, rows from the top.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// Why a bitmap could not be decoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BitmapError {
    Unsupported(&'static str),
    Malformed(&'static str),
}

impl std::fmt::Display for BitmapError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BitmapError::Unsupported(what) => write!(f, "{what} bitmaps are not decoded"),
            BitmapError::Malformed(what) => write!(f, "malformed bitmap: {what}"),
        }
    }
}

impl std::error::Error for BitmapError {}

/// The fields of a DIB header that decoding needs.
#[derive(Debug, Clone, Copy)]
struct Header {
    size: usize,
    width: u32,
    height: u32,
    bottom_up: bool,
    bits: u16,
    compression: u32,
    colors: usize,
    core: bool,
    masks: Option<[u32; 4]>,
}

fn header(info: &[u8]) -> Result<Header, BitmapError> {
    let bad = BitmapError::Malformed("header");
    let size = u32_at(info, 0).ok_or(bad)? as usize;
    if size == 12 {
        let width = u32::from(u16_at(info, 4).ok_or(bad)?);
        let height = u32::from(u16_at(info, 6).ok_or(bad)?);
        let bits = u16_at(info, 10).ok_or(bad)?;
        if width == 0 || height == 0 {
            return Err(BitmapError::Malformed("size"));
        }
        let colors = if bits <= 8 { 1usize << bits } else { 0 };
        return Ok(Header {
            size,
            width,
            height,
            bottom_up: true,
            bits,
            compression: BI_RGB,
            colors,
            core: true,
            masks: None,
        });
    }
    if size < 40 || size > info.len() {
        return Err(bad);
    }
    let width = i32_at(info, 4).ok_or(bad)?;
    let height = i32_at(info, 8).ok_or(bad)?;
    let bits = u16_at(info, 14).ok_or(bad)?;
    let compression = u32_at(info, 16).ok_or(bad)?;
    let used = u32_at(info, 32).ok_or(bad)? as usize;
    let colors = match (used, bits) {
        (0, 1..=8) => 1usize << bits,
        (n, 1..=8) => n.min(1 << bits),
        _ => 0,
    };
    let mask = |at: usize| u32_at(info, at).unwrap_or(0);
    let masks = match compression {
        BI_BITFIELDS if size >= 52 => Some([mask(40), mask(44), mask(48), mask(52)]),
        BI_BITFIELDS => Some([mask(size), mask(size + 4), mask(size + 8), 0]),
        BI_ALPHABITFIELDS => Some([mask(size), mask(size + 4), mask(size + 8), mask(size + 12)]),
        _ => None,
    };
    if width <= 0 || height == 0 {
        return Err(BitmapError::Malformed("size"));
    }
    Ok(Header {
        size,
        width: width as u32,
        height: height.unsigned_abs(),
        bottom_up: height > 0,
        bits,
        compression,
        colors,
        core: false,
        masks,
    })
}

impl Header {
    /// Where the color table starts: after the header and any masks that
    /// follow a 40-byte header.
    fn palette_at(&self) -> usize {
        match (self.compression, self.size) {
            (BI_BITFIELDS, 40) => 52,
            (BI_ALPHABITFIELDS, 40) => 56,
            _ => self.size,
        }
    }

    fn entry(&self) -> usize {
        if self.core {
            3
        } else {
            4
        }
    }

    /// The bytes the header, masks and color table take.
    fn info_len(&self) -> usize {
        self.palette_at() + self.colors * self.entry()
    }

    fn palette(&self, info: &[u8]) -> Vec<[u8; 4]> {
        let at = self.palette_at();
        (0..self.colors)
            .map(|i| {
                let o = at + i * self.entry();
                let b = |k: usize| u8_at(info, o + k).unwrap_or(0);
                [b(2), b(1), b(0), 255]
            })
            .collect()
    }
}

/// The size in bytes of the header and color table at the start of a
/// packed DIB, where its pixels begin.
pub fn info_size(dib: &[u8]) -> Option<usize> {
    header(dib).ok().map(|h| h.info_len())
}

/// Decodes a BMP file: a BITMAPFILEHEADER and a DIB.
pub fn decode_bmp(file: &[u8]) -> Result<Rgba, BitmapError> {
    if file.get(..2) != Some(b"BM") {
        return Err(BitmapError::Malformed("file header"));
    }
    let info = file
        .get(14..)
        .ok_or(BitmapError::Malformed("file header"))?;
    let h = header(info)?;
    let offset = u32_at(file, 10).unwrap_or(0) as usize;
    let bits_at = match offset >= 14 + h.info_len() && offset < file.len() {
        true => offset,
        false => 14 + h.info_len(),
    };
    decode_with(info, h, file.get(bits_at..).unwrap_or(&[]), false)
}

/// Decodes a packed DIB: header, color table and pixels in one block.
pub fn decode_packed(dib: &[u8], alpha: bool) -> Result<Rgba, BitmapError> {
    let h = header(dib)?;
    decode_with(dib, h, dib.get(h.info_len()..).unwrap_or(&[]), alpha)
}

/// Decodes a DIB given as its header and color table and, apart, its
/// pixels. With `alpha`, the fourth byte of 32-bit pixels is their alpha.
pub fn decode(info: &[u8], bits: &[u8], alpha: bool) -> Result<Rgba, BitmapError> {
    decode_with(info, header(info)?, bits, alpha)
}

fn decode_with(info: &[u8], h: Header, bits: &[u8], alpha: bool) -> Result<Rgba, BitmapError> {
    let (w, rows) = (h.width as usize, h.height as usize);
    if u64::from(h.width) * u64::from(h.height) > MAX_PIXELS {
        return Err(BitmapError::Malformed("too many pixels"));
    }
    let pixels = u64::from(h.width) * u64::from(h.height);
    let needed = match h.compression {
        BI_RLE8 | BI_RLE4 => pixels / 512,
        _ => {
            let stride = (w * usize::from(h.bits)).div_ceil(32) * 4;
            (stride * (rows - 1) + (w * usize::from(h.bits)).div_ceil(8)) as u64
        }
    };
    if (bits.len() as u64) < needed {
        return Err(BitmapError::Malformed("pixels past the end of the data"));
    }
    let mut out = vec![0u8; w * rows * 4];
    let palette = h.palette(info);
    let color = |index: usize| palette.get(index).copied().unwrap_or([0, 0, 0, 255]);
    match (h.compression, h.bits) {
        (BI_RLE8, 8) => rle(bits, &h, &mut out, false, &color),
        (BI_RLE4, 4) => rle(bits, &h, &mut out, true, &color),
        (BI_RGB | BI_BITFIELDS | BI_ALPHABITFIELDS, 1 | 2 | 4 | 8 | 16 | 24 | 32) => {
            let stride = (w * usize::from(h.bits)).div_ceil(32) * 4;
            let masks = h.masks.unwrap_or(match h.bits {
                16 => [0x7C00, 0x03E0, 0x001F, 0],
                _ => [0xFF_0000, 0xFF00, 0xFF, if alpha { 0xFF00_0000 } else { 0 }],
            });
            let fields = masks.map(Field::of);
            for y in 0..rows {
                let source = if h.bottom_up { rows - 1 - y } else { y };
                let Some(row) =
                    bits.get(source * stride..(source * stride + stride).min(bits.len()))
                else {
                    break;
                };
                if row.len() * 8 < w * usize::from(h.bits) {
                    break;
                }
                let line = &mut out[y * w * 4..(y + 1) * w * 4];
                for (x, px) in line.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                    let value = match h.bits {
                        1 | 2 | 4 => {
                            let bits = usize::from(h.bits);
                            let at = x * bits;
                            let byte = row[at / 8];
                            let shift = 8 - bits - at % 8;
                            color(usize::from((byte >> shift) & ((1 << bits) - 1) as u8))
                        }
                        8 => color(usize::from(row[x])),
                        24 => [row[x * 3 + 2], row[x * 3 + 1], row[x * 3], 255],
                        16 => {
                            let v = u32::from(u16::from_le_bytes([row[x * 2], row[x * 2 + 1]]));
                            fields.map(|f| f.extract(v)).with_opaque(&fields)
                        }
                        _ => {
                            let v = u32::from_le_bytes([
                                row[x * 4],
                                row[x * 4 + 1],
                                row[x * 4 + 2],
                                row[x * 4 + 3],
                            ]);
                            fields.map(|f| f.extract(v)).with_opaque(&fields)
                        }
                    };
                    px.copy_from_slice(&value);
                }
            }
        }
        (4 | 5, _) => return Err(BitmapError::Unsupported("JPEG or PNG compressed")),
        _ => return Err(BitmapError::Unsupported("this bit depth or compression of")),
    }
    Ok(Rgba {
        width: h.width,
        height: h.height,
        pixels: out,
    })
}

/// One color channel of a bit-field pixel.
#[derive(Debug, Clone, Copy)]
struct Field {
    mask: u32,
    shift: u32,
    max: u32,
}

impl Field {
    fn of(mask: u32) -> Field {
        if mask == 0 {
            return Field {
                mask: 0,
                shift: 0,
                max: 0,
            };
        }
        let shift = mask.trailing_zeros();
        Field {
            mask,
            shift,
            max: mask >> shift,
        }
    }

    fn extract(self, value: u32) -> u8 {
        if self.max == 0 {
            return 255;
        }
        (((value & self.mask) >> self.shift) * 255 / self.max) as u8
    }
}

trait Opaque {
    fn with_opaque(self, fields: &[Field; 4]) -> [u8; 4];
}

impl Opaque for [u8; 4] {
    fn with_opaque(mut self, fields: &[Field; 4]) -> [u8; 4] {
        if fields[3].max == 0 {
            self[3] = 255;
        }
        self
    }
}

/// Runs of RLE8 or RLE4 pixels; pixels the runs skip take the first
/// color of the table.
fn rle(bits: &[u8], h: &Header, out: &mut [u8], four: bool, color: &dyn Fn(usize) -> [u8; 4]) {
    let (w, rows) = (h.width as usize, h.height as usize);
    let background = color(0);
    out.as_chunks_mut::<4>()
        .0
        .iter_mut()
        .for_each(|px| px.copy_from_slice(&background));
    let mut put = |x: usize, y: usize, index: usize| {
        if x >= w || y >= rows {
            return;
        }
        let row = if h.bottom_up { rows - 1 - y } else { y };
        out[(row * w + x) * 4..][..4].copy_from_slice(&color(index));
    };
    let (mut x, mut y, mut at) = (0usize, 0usize, 0usize);
    while at + 1 < bits.len() && y < rows {
        let (count, value) = (usize::from(bits[at]), bits[at + 1]);
        at += 2;
        if count > 0 {
            for i in 0..count {
                let index = match four {
                    true if i % 2 == 0 => value >> 4,
                    true => value & 0xF,
                    false => value,
                };
                put(x, y, usize::from(index));
                x += 1;
            }
            continue;
        }
        match value {
            0 => {
                x = 0;
                y += 1;
            }
            1 => return,
            2 => {
                x += usize::from(bits.get(at).copied().unwrap_or(0));
                y += usize::from(bits.get(at + 1).copied().unwrap_or(0));
                at += 2;
            }
            n => {
                let n = usize::from(n);
                let bytes = if four { n.div_ceil(2) } else { n };
                for i in 0..n {
                    let Some(&byte) = bits.get(at + if four { i / 2 } else { i }) else {
                        return;
                    };
                    let index = match four {
                        true if i % 2 == 0 => byte >> 4,
                        true => byte & 0xF,
                        false => byte,
                    };
                    put(x, y, usize::from(index));
                    x += 1;
                }
                at += bytes.next_multiple_of(2);
            }
        }
    }
}

/// Encodes straight RGBA as a top-down 32-bit BMP with a BITMAPV4HEADER
/// whose alpha mask keeps the alpha.
pub fn encode_bmp(image: &Rgba) -> Vec<u8> {
    const INFO: usize = 108;
    let pixels = image.width as usize * image.height as usize * 4;
    let mut out = Vec::with_capacity(14 + INFO + pixels);
    let u32s = |out: &mut Vec<u8>, values: &[u32]| {
        values
            .iter()
            .for_each(|v| out.extend_from_slice(&v.to_le_bytes()))
    };
    out.extend_from_slice(b"BM");
    u32s(
        &mut out,
        &[(14 + INFO + pixels) as u32, 0, (14 + INFO) as u32],
    );
    u32s(
        &mut out,
        &[
            INFO as u32,
            image.width,
            (image.height as i32).wrapping_neg() as u32,
        ],
    );
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    u32s(
        &mut out,
        &[
            BI_BITFIELDS,
            pixels as u32,
            2835,
            2835,
            0,
            0,
            0x00FF_0000,
            0x0000_FF00,
            0x0000_00FF,
            0xFF00_0000,
            0x7352_4742,
        ],
    );
    out.resize(14 + INFO, 0);
    for px in image.pixels.as_chunks::<4>().0.iter() {
        out.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(width: i32, height: i32, bits: u16, compression: u32, palette: &[[u8; 4]]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&40u32.to_le_bytes());
        out.extend_from_slice(&width.to_le_bytes());
        out.extend_from_slice(&height.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&bits.to_le_bytes());
        out.extend_from_slice(&compression.to_le_bytes());
        out.extend_from_slice(&[0; 12]);
        out.extend_from_slice(&(palette.len() as u32).to_le_bytes());
        out.extend_from_slice(&[0; 4]);
        palette.iter().for_each(|p| out.extend_from_slice(p));
        out
    }

    /// [MS-WMF] §2.2.2.3: a bottom-up 1-bit DIB reads its rows from the
    /// last and its colors from the table.
    #[test]
    fn monochrome_bottom_up_rows_use_the_color_table() {
        let head = info(2, 2, 1, BI_RGB, &[[0, 0, 0, 0], [255, 255, 255, 0]]);
        let bits = [0b1000_0000, 0, 0, 0, 0b0100_0000, 0, 0, 0];
        let image = decode(&head, &bits, false).unwrap();
        assert_eq!(&image.pixels[..8], &[0, 0, 0, 255, 255, 255, 255, 255]);
        assert_eq!(&image.pixels[8..], &[255, 255, 255, 255, 0, 0, 0, 255]);
    }

    /// [MS-WMF] §2.1.1.7: RLE8 runs, an end of line and an absolute run.
    #[test]
    fn rle8_runs_and_absolute_runs_decode() {
        let palette = [[0, 0, 255, 0], [0, 255, 0, 0], [255, 0, 0, 0]];
        let head = info(3, 2, 8, BI_RLE8, &palette);
        let bits = [3, 0, 0, 0, 0, 3, 1, 2, 1, 0, 0, 1];
        let image = decode(&head, &bits, false).unwrap();
        assert_eq!(&image.pixels[..4], &[0, 255, 0, 255]);
        assert_eq!(&image.pixels[4..8], &[0, 0, 255, 255]);
        assert_eq!(&image.pixels[12..16], &[255, 0, 0, 255]);
    }

    #[test]
    fn encoded_bmp_decodes_to_the_same_pixels() {
        let image = Rgba {
            width: 2,
            height: 1,
            pixels: vec![10, 20, 30, 40, 50, 60, 70, 255],
        };
        assert_eq!(decode_bmp(&encode_bmp(&image)).unwrap(), image);
    }

    #[test]
    fn truncated_headers_and_huge_sizes_are_errors() {
        assert!(decode(&[40, 0, 0], &[], false).is_err());
        let head = info(1 << 20, 1 << 20, 24, BI_RGB, &[]);
        assert!(decode(&head, &[], false).is_err());
        assert!(decode(&info(4, 4, 24, BI_RGB, &[]), &[1, 2, 3], false).is_err());
    }
}

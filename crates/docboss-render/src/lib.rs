//! Rasterizes docboss page layouts.
//!
//! [`render_page`] paints one page of a [`Layout`] into an RGBA
//! [`Pixmap`] with anti-aliased coverage: glyph outlines from the layout's
//! fonts (cached per glyph, size and subpixel offset), filled rectangles,
//! lines and embedded images. [`render_pages`] paints many pages across
//! all cores. [`Pixmap::encode`] writes PNG, PPM, BMP or JPEG.

mod encode;
mod image;
mod jpeg;
mod paint;
mod raster;
mod shade;

use std::path::Path;

use docboss_layout::Layout;
use docboss_model::Diagnostic;

pub use image::{decode as decode_image, Decoded, ImageError};
pub use paint::Renderer;

/// Largest pixmap side, in pixels.
pub const MAX_SIDE: u32 = 32_768;
/// Largest pixmap area, in pixels.
pub const MAX_PIXELS: u64 = 400_000_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    NoSuchPage(usize),
    TooLarge { width: u64, height: u64 },
    Io(String),
    Other(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::NoSuchPage(index) => write!(f, "no page {index}"),
            Error::TooLarge { width, height } => {
                write!(f, "a {width}x{height} pixmap is too large")
            }
            Error::Io(why) => write!(f, "i/o: {why}"),
            Error::Other(why) => f.write_str(why),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

/// Output image formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Png,
    Ppm,
    Bmp,
    /// Baseline JPEG at a quality from 1 to 100.
    Jpeg {
        quality: u8,
    },
}

impl Format {
    /// The format a file extension names: `png`, `ppm`, `bmp`, `jpg` or
    /// `jpeg`.
    pub fn from_extension(extension: &str) -> Option<Format> {
        match extension.to_ascii_lowercase().as_str() {
            "png" => Some(Format::Png),
            "ppm" => Some(Format::Ppm),
            "bmp" => Some(Format::Bmp),
            "jpg" | "jpeg" => Some(Format::Jpeg { quality: 90 }),
            _ => None,
        }
    }
}

/// An RGBA raster with 8 bits per channel, row-major from the top-left.
#[derive(Debug, Clone, PartialEq)]
pub struct Pixmap {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
    /// Content that could not be painted, such as metafile images.
    pub diagnostics: Vec<Diagnostic>,
}

impl Pixmap {
    /// An opaque white pixmap.
    pub fn new(width: u32, height: u32) -> Result<Pixmap> {
        let area = u64::from(width) * u64::from(height);
        if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE || area > MAX_PIXELS {
            return Err(Error::TooLarge {
                width: width.into(),
                height: height.into(),
            });
        }
        Ok(Pixmap {
            width,
            height,
            data: vec![255; area as usize * 4],
            diagnostics: Vec::new(),
        })
    }

    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let at = (y as usize * self.width as usize + x as usize) * 4;
        self.data.get(at..at + 4).and_then(|p| p.try_into().ok())
    }

    pub fn encode(&self, format: Format) -> Result<Vec<u8>> {
        match format {
            Format::Png => Ok(encode::encode_rgba(self.width, self.height, &self.data)),
            Format::Ppm => Ok(encode::encode_ppm(self.width, self.height, &self.data)),
            Format::Bmp => Ok(encode::encode_bmp(self.width, self.height, &self.data)),
            Format::Jpeg { quality } => {
                jpeg::encode_jpeg(self.width, self.height, &self.data, quality)
            }
        }
    }

    /// Writes the pixmap in the format its extension names, PNG otherwise.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        let format = path
            .extension()
            .and_then(|e| e.to_str())
            .and_then(Format::from_extension)
            .unwrap_or(Format::Png);
        std::fs::write(path, self.encode(format)?).map_err(|e| Error::Io(e.to_string()))
    }
}

/// Paints page `index` at `scale` pixels per point (1.0 is 72 dpi).
pub fn render_page(layout: &Layout, index: usize, scale: f32) -> Result<Pixmap> {
    Renderer::new().render(layout, index, scale)
}

/// Paints every page, spreading the pages over the machine's cores. The
/// result is in page order.
pub fn render_pages(layout: &Layout, scale: f32) -> Vec<Result<Pixmap>> {
    let count = layout.pages.len();
    let workers = std::thread::available_parallelism()
        .map_or(1, |n| n.get())
        .min(count.max(1));
    let next = std::sync::atomic::AtomicUsize::new(0);
    let mut results: Vec<Option<Result<Pixmap>>> = (0..count).map(|_| None).collect();
    let slots = std::sync::Mutex::new(&mut results);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                let mut renderer = Renderer::new();
                loop {
                    let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if index >= count {
                        return;
                    }
                    let pixmap = renderer.render(layout, index, scale);
                    let mut slots = slots
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    slots[index] = Some(pixmap);
                }
            });
        }
    });
    results
        .into_iter()
        .enumerate()
        .map(|(i, r)| r.unwrap_or(Err(Error::NoSuchPage(i))))
        .collect()
}

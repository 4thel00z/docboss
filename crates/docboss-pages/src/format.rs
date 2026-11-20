//! The formats a document or page converts to, one tagged enum shared by
//! Rust callers and the dicts the Python bindings accept.

use docboss_output::{HtmlOptions, ImageMode, MarkdownOptions, TextOptions};
use serde::Deserialize;

/// A target format with its options. Deserializes from a map tagged by
/// `format`, such as `{"format": "markdown", "images": "omit"}`; keys the
/// format does not know are refused.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(try_from = "Spec")]
pub enum Format {
    Text(TextOptions),
    Markdown(MarkdownOptions),
    Html(HtmlOptions),
    /// The document model as JSON; for a page, the body blocks that start
    /// on it.
    Json {
        pretty: bool,
    },
    /// A page as PNG, `scale` pixels per point.
    Png {
        scale: f32,
    },
    /// A page as binary PPM.
    Ppm {
        scale: f32,
    },
    /// A page as BMP.
    Bmp {
        scale: f32,
    },
    /// A page as baseline JPEG at a quality from 1 to 100.
    Jpeg {
        scale: f32,
        quality: u8,
    },
}

impl Format {
    /// The name the `format` key gives the format.
    pub fn name(&self) -> &'static str {
        match self {
            Format::Text(_) => "text",
            Format::Markdown(_) => "markdown",
            Format::Html(_) => "html",
            Format::Json { .. } => "json",
            Format::Png { .. } => "png",
            Format::Ppm { .. } => "ppm",
            Format::Bmp { .. } => "bmp",
            Format::Jpeg { .. } => "jpeg",
        }
    }

    /// Whether the format renders a page to an image.
    pub fn is_image(&self) -> bool {
        self.image().is_some()
    }

    /// The scale and encoder of an image format.
    pub(crate) fn image(&self) -> Option<(f32, docboss_render::Format)> {
        match *self {
            Format::Png { scale } => Some((scale, docboss_render::Format::Png)),
            Format::Ppm { scale } => Some((scale, docboss_render::Format::Ppm)),
            Format::Bmp { scale } => Some((scale, docboss_render::Format::Bmp)),
            Format::Jpeg { scale, quality } => {
                Some((scale, docboss_render::Format::Jpeg { quality }))
            }
            _ => None,
        }
    }

    /// Refuses a scale that is not a positive number and a JPEG quality
    /// outside 1 to 100.
    pub fn check(&self) -> Result<(), String> {
        let Some((scale, encoder)) = self.image() else {
            return Ok(());
        };
        if !(scale.is_finite() && scale > 0.0) {
            return Err(format!("scale must be a positive number, not {scale}"));
        }
        if let docboss_render::Format::Jpeg { quality } = encoder {
            if !(1..=100).contains(&quality) {
                return Err(format!("quality must be from 1 to 100, not {quality}"));
            }
        }
        Ok(())
    }
}

impl TryFrom<Spec> for Format {
    type Error = String;

    fn try_from(spec: Spec) -> Result<Self, String> {
        let format = match spec {
            Spec::Text(s) => Format::Text(TextOptions {
                list_labels: s.list_labels,
                headers_footers: s.headers_footers,
                notes: s.notes,
                comments: s.comments,
            }),
            Spec::Markdown(s) => Format::Markdown(MarkdownOptions {
                title: s.title,
                page_breaks: s.page_breaks,
                images: s.images.mode(s.image_prefix),
                headers_footers: s.headers_footers,
                notes: s.notes,
                comments: s.comments,
            }),
            Spec::Html(s) => Format::Html(HtmlOptions {
                standalone: s.standalone,
                images: s.images.mode(s.image_prefix),
                headers_footers: s.headers_footers,
                notes: s.notes,
                comments: s.comments,
            }),
            Spec::Json(s) => Format::Json { pretty: s.pretty },
            Spec::Png(s) => Format::Png { scale: s.scale },
            Spec::Ppm(s) => Format::Ppm { scale: s.scale },
            Spec::Bmp(s) => Format::Bmp { scale: s.scale },
            Spec::Jpeg(s) => Format::Jpeg {
                scale: s.scale,
                quality: s.quality,
            },
        };
        format.check()?;
        Ok(format)
    }
}

/// The wire form of [`Format`]: the keys each format accepts, with the
/// defaults of the extraction methods.
#[derive(Deserialize)]
#[serde(tag = "format", rename_all = "lowercase")]
enum Spec {
    Text(TextSpec),
    Markdown(MarkdownSpec),
    Html(HtmlSpec),
    Json(JsonSpec),
    Png(ImageSpec),
    Ppm(ImageSpec),
    Bmp(ImageSpec),
    #[serde(alias = "jpg")]
    Jpeg(JpegSpec),
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
enum Images {
    Reference,
    Embed,
    Omit,
}

impl Images {
    fn mode(self, prefix: String) -> ImageMode {
        match self {
            Images::Reference => ImageMode::Reference { prefix },
            Images::Embed => ImageMode::Embed,
            Images::Omit => ImageMode::Omit,
        }
    }
}

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
struct TextSpec {
    list_labels: bool,
    headers_footers: bool,
    notes: bool,
    comments: bool,
}

impl Default for TextSpec {
    fn default() -> Self {
        Self {
            list_labels: true,
            headers_footers: false,
            notes: true,
            comments: false,
        }
    }
}

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
struct MarkdownSpec {
    images: Images,
    image_prefix: String,
    title: bool,
    page_breaks: bool,
    headers_footers: bool,
    notes: bool,
    comments: bool,
}

impl Default for MarkdownSpec {
    fn default() -> Self {
        Self {
            images: Images::Reference,
            image_prefix: String::new(),
            title: false,
            page_breaks: false,
            headers_footers: false,
            notes: true,
            comments: false,
        }
    }
}

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
struct HtmlSpec {
    standalone: bool,
    images: Images,
    image_prefix: String,
    headers_footers: bool,
    notes: bool,
    comments: bool,
}

impl Default for HtmlSpec {
    fn default() -> Self {
        Self {
            standalone: false,
            images: Images::Embed,
            image_prefix: String::new(),
            headers_footers: false,
            notes: true,
            comments: false,
        }
    }
}

#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct JsonSpec {
    pretty: bool,
}

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
struct ImageSpec {
    scale: f32,
}

impl Default for ImageSpec {
    fn default() -> Self {
        Self { scale: 1.0 }
    }
}

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
struct JpegSpec {
    scale: f32,
    quality: u8,
}

impl Default for JpegSpec {
    fn default() -> Self {
        Self {
            scale: 1.0,
            quality: 90,
        }
    }
}

use std::sync::Arc;

/// Index of an entry in [`crate::Document::media`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct MediaId(pub u32);

/// A binary part referenced by the text, usually an image.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Media {
    /// The part name (`word/media/image1.png`) or a generated one for DOC.
    pub name: String,
    /// The MIME type detected from the bytes, such as `image/png`.
    pub content_type: String,
    #[cfg_attr(feature = "serde", serde(skip))]
    pub data: Arc<[u8]>,
}

/// A font the document declares in its font table.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct FontEntry {
    pub name: String,
    pub alt_name: Option<String>,
    /// `roman`, `swiss`, `modern`, `script`, `decorative` or `auto`.
    pub family: Option<String>,
    /// `fixed`, `variable` or `default`.
    pub pitch: Option<String>,
    /// The embedded font program, deobfuscated, when the file carries one.
    #[cfg_attr(feature = "serde", serde(skip))]
    pub embedded_regular: Option<Arc<[u8]>>,
    #[cfg_attr(feature = "serde", serde(skip))]
    pub embedded_bold: Option<Arc<[u8]>>,
    #[cfg_attr(feature = "serde", serde(skip))]
    pub embedded_italic: Option<Arc<[u8]>>,
    #[cfg_attr(feature = "serde", serde(skip))]
    pub embedded_bold_italic: Option<Arc<[u8]>>,
}

/// The MIME type of an image from its leading bytes, or
/// `application/octet-stream` when the format is not recognized.
pub fn sniff_image(bytes: &[u8]) -> &'static str {
    const SIGNATURES: [(&[u8], &str); 9] = [
        (b"\x89PNG\r\n\x1a\n", "image/png"),
        (b"\xff\xd8\xff", "image/jpeg"),
        (b"GIF87a", "image/gif"),
        (b"GIF89a", "image/gif"),
        (b"BM", "image/bmp"),
        (b"II*\0", "image/tiff"),
        (b"MM\0*", "image/tiff"),
        (b"\xd7\xcd\xc6\x9a", "image/x-wmf"),
        (b"\x01\0\0\0", "image/x-emf"),
    ];
    SIGNATURES
        .iter()
        .find(|(magic, _)| bytes.starts_with(magic))
        .map_or("application/octet-stream", |(_, mime)| mime)
}

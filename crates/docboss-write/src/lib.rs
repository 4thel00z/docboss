//! DOCX creation for docboss: the write-side twin of the readers.
//!
//! - [`to_bytes`] and [`save`] serialize a [`docboss_model::Document`] into
//!   a WordprocessingML package.
//! - [`DocumentBuilder`] and [`Para`] compose documents without assembling
//!   the model by hand.
//! - [`markdown`] turns CommonMark + GFM into a document.
//!
//! Output is deterministic: the same document produces identical bytes.
//! The crate never reads clocks or randomness; ZIP entries carry a fixed
//! 1980-01-01 timestamp and dates appear only when the metadata states them.

mod body;
mod builder;
mod docx;
mod image;
pub mod markdown;
mod package;
mod parts;
mod props;
mod xml;
pub mod zip;

use std::path::Path;

pub use builder::{default_styles, DocumentBuilder, ListKind, Para, TableBuilder};
pub use image::image_dimensions;

/// Errors raised while writing a package.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("i/o: {0}")]
    Io(#[from] std::io::Error),
    /// A part or the archive exceeds the 4 GiB limit of a non-ZIP64 archive.
    #[error("the package exceeds the ZIP size limits")]
    TooLarge,
}

pub type Result<T> = std::result::Result<T, Error>;

/// Serializes a document into DOCX bytes.
pub fn to_bytes(document: &docboss_model::Document) -> Result<Vec<u8>> {
    docx::to_bytes(document)
}

/// Serializes a document and writes it to `path`.
pub fn save(document: &docboss_model::Document, path: impl AsRef<Path>) -> Result<()> {
    std::fs::write(path, to_bytes(document)?)?;
    Ok(())
}

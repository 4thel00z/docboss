//! Test helpers for docboss: a small ZIP writer and builders for DOCX
//! packages assembled from XML strings.

mod docx;
mod zip;

pub use docx::{Docx, W_NS};
pub use zip::{ZipOptions, ZipWriter};

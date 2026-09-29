//! Font parsing, metrics and discovery for docboss.
//!
//! [`Font`] reads TrueType and OpenType faces (`glyf` and `CFF ` outlines,
//! collections included): character mapping, advance widths, vertical
//! metrics, kerning from `kern` and GPOS, glyph substitution and
//! positioning from `GSUB` and `GPOS`, and glyph outlines.
//! [`FontDatabase`] finds faces on the system and in documents and picks
//! one for a requested family, with metric-compatible substitutes and
//! per-character fallback.

mod bytes;
#[allow(dead_code)]
mod cff;
mod cmap;
mod database;
mod font;
mod glyf;
mod gpos;
mod gsub;
mod kern;
mod otl;
mod outline;
mod shape;
mod symbol;

pub use database::{FaceInfo, FontDatabase, FontId};
pub use font::{face_count, FaceNames, FaceStyle, Font, Metrics};
pub use outline::{bounds, Seg};
pub use shape::{Feature, ShapeInput, Shaped};
pub use symbol::symbol_to_unicode;

/// Why a font could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FontError {
    NoSuchFace(u32),
    Malformed(&'static str),
}

impl std::fmt::Display for FontError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FontError::NoSuchFace(index) => write!(f, "font file has no face {index}"),
            FontError::Malformed(what) => write!(f, "malformed font: {what}"),
        }
    }
}

impl std::error::Error for FontError {}

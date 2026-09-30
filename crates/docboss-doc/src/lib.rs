//! Word 97-2003 binary document reader ([MS-DOC]).
//!
//! [`read`] opens the compound file ([MS-CFB]), reads the File Information
//! Block, the piece table, the formatted disk pages, the stylesheet, the
//! list tables, notes, comments, headers, sections, bookmarks and pictures,
//! and returns a [`docboss_model::Document`] with `format` set to
//! [`docboss_model::SourceFormat::Doc`].
//!
//! The reader is lenient: damaged tables are read as far as they go and
//! everything approximated or dropped is listed in
//! [`docboss_model::Document::diagnostics`]. Word 6 and Word 95 files are
//! read with their character, paragraph, style, section, header, footer
//! and table formatting; their numbered paragraphs (ANLD), comments and
//! drawing objects are left out and reported. Word 2 files are read as
//! plain text.
//!
//! Password-protected files, encrypted with RC4 or RC4 CryptoAPI or
//! XOR-obfuscated, open with [`read_with_password`].

mod bytes;
mod crypt;
mod document;
mod error;
mod fib;
mod fkp;
mod lists;
mod picture;
mod props;
mod sprm;
mod story;
mod sttb;
mod styles;
mod table;
mod text;
mod word6;

pub use error::{Error, Result};

/// Whether `bytes` looks like a Word binary document: a compound file whose
/// root holds a `WordDocument` stream, or a Word 2 file.
pub fn is_doc(bytes: &[u8]) -> bool {
    is_word2(bytes)
        || docboss_cfb::CompoundFile::parse(bytes)
            .is_ok_and(|file| file.find("WordDocument").is_some())
}

/// Whether `bytes` looks like a Word for Windows 1.x or 2.0 file, which is a
/// bare FIB and text rather than a compound file: wIdent 0xA5DB and an nFib
/// below Word 6's.
pub fn is_word2(bytes: &[u8]) -> bool {
    let n_fib = bytes
        .get(2..4)
        .map_or(0, |b| u16::from_le_bytes([b[0], b[1]]));
    bytes.len() >= 32 && bytes.starts_with(&[0xDB, 0xA5]) && (0x0011..0x0065).contains(&n_fib)
}

/// Reads a Word binary document.
pub fn read(bytes: &[u8]) -> Result<docboss_model::Document> {
    document::read(bytes, None)
}

/// Reads a Word binary document protected with a password to open.
pub fn read_with_password(bytes: &[u8], password: &str) -> Result<docboss_model::Document> {
    document::read(bytes, Some(password))
}

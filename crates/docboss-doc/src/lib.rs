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
//! read as plain text with a diagnostic saying so.
//!
//! Password-protected files encrypted with RC4 or RC4 CryptoAPI open with
//! [`read_with_password`]; XOR-obfuscated files are refused with
//! [`Error::UnsupportedEncryption`].

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

pub use error::{Error, Result};

/// Whether `bytes` looks like a Word binary document: a compound file whose
/// root holds a `WordDocument` stream.
pub fn is_doc(bytes: &[u8]) -> bool {
    docboss_cfb::CompoundFile::parse(bytes).is_ok_and(|file| file.find("WordDocument").is_some())
}

/// Reads a Word binary document.
pub fn read(bytes: &[u8]) -> Result<docboss_model::Document> {
    document::read(bytes, None)
}

/// Reads a Word binary document protected with a password to open.
pub fn read_with_password(bytes: &[u8], password: &str) -> Result<docboss_model::Document> {
    document::read(bytes, Some(password))
}

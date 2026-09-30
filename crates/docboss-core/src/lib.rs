//! Opens word-processing documents in either format docboss reads.
//!
//! [`open`] and [`read`] detect the format from the leading bytes, not the
//! file name: a ZIP archive is read as WordprocessingML (DOCX, DOCM, DOTX,
//! DOTM), a compound file with a `WordDocument` stream as Word binary (DOC,
//! DOT). Both return the same [`Document`] model.

use std::path::Path;

pub use docboss_model::*;

/// Why a document could not be opened.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("docx: {0}")]
    Docx(#[from] docboss_docx::Error),
    #[error("doc: {0}")]
    Doc(#[from] docboss_doc::Error),
    #[error("the document is encrypted and needs a password")]
    Encrypted,
    #[error("encrypted package: {0}")]
    Crypt(#[from] docboss_crypt::Error),
    #[error("unsupported format: {0}")]
    Unsupported(&'static str),
}

pub type Result<T> = std::result::Result<T, Error>;

/// The container format of a file, detected from its bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// A ZIP archive, read as an OPC WordprocessingML package.
    Docx,
    /// A compound file holding a `WordDocument` stream.
    Doc,
    /// A compound file holding an `EncryptedPackage` stream: a DOCX
    /// protected with a password to open.
    EncryptedDocx,
    Rtf,
    Unknown,
}

const ZIP_MAGIC: [&[u8]; 3] = [b"PK\x03\x04", b"PK\x05\x06", b"PK\x07\x08"];

/// Detects the format of a file from its bytes.
pub fn detect(bytes: &[u8]) -> Format {
    if ZIP_MAGIC.iter().any(|magic| bytes.starts_with(magic)) {
        return Format::Docx;
    }
    if bytes.starts_with(b"{\\rtf") {
        return Format::Rtf;
    }
    if docboss_doc::is_word2(bytes) {
        return Format::Doc;
    }
    if !docboss_cfb::is_compound_file(bytes) {
        return Format::Unknown;
    }
    let Ok(file) = docboss_cfb::CompoundFile::parse(bytes) else {
        return Format::Unknown;
    };
    if file.find("WordDocument").is_some() {
        return Format::Doc;
    }
    if file.find("EncryptedPackage").is_some() {
        return Format::EncryptedDocx;
    }
    Format::Unknown
}

/// Options for [`read_with`].
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// The password to open an encrypted DOC or DOCX.
    pub password: Option<String>,
}

/// Reads a document from its bytes.
pub fn read(bytes: &[u8]) -> Result<Document> {
    read_with(bytes, &Options::default())
}

/// Reads a document from its bytes with options.
pub fn read_with(bytes: &[u8], options: &Options) -> Result<Document> {
    match detect(bytes) {
        Format::Docx => Ok(docboss_docx::read(bytes)?),
        Format::Doc => match options.password.as_deref() {
            Some(password) => Ok(docboss_doc::read_with_password(bytes, password)?),
            None => Ok(docboss_doc::read(bytes)?),
        },
        Format::EncryptedDocx => {
            let password = options.password.as_deref().ok_or(Error::Encrypted)?;
            let package = docboss_crypt::decrypt(bytes, password)?;
            Ok(docboss_docx::read(&package)?)
        }
        Format::Rtf => Err(Error::Unsupported("RTF")),
        Format::Unknown => Err(Error::Unsupported(
            "neither a ZIP package nor a Word compound file",
        )),
    }
}

/// Reads a document from a file.
pub fn open(path: impl AsRef<Path>) -> Result<Document> {
    read(&std::fs::read(path)?)
}

/// Reads a document from a file with options.
pub fn open_with(path: impl AsRef<Path>, options: &Options) -> Result<Document> {
    read_with(&std::fs::read(path)?, options)
}

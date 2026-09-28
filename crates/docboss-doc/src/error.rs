/// Convenience alias used throughout docboss-doc.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors that stop a Word binary document from being read. Damage inside
/// an otherwise readable file is reported as diagnostics instead.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("compound file: {0}")]
    Cfb(#[from] docboss_cfb::Error),
    #[error("not a Word binary document: {0}")]
    NotWord(&'static str),
    #[error("the document is encrypted and needs a password")]
    Encrypted,
    #[error("the password does not open this document")]
    WrongPassword,
    #[error("unsupported encryption: {0}")]
    UnsupportedEncryption(&'static str),
}

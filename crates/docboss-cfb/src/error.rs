/// Convenience alias used throughout docboss-cfb.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors that stop a compound file from being read at all. Damage past the
/// header is recovered from and reported as diagnostics instead.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    #[error("not a compound file: bad signature")]
    NotCompoundFile,
    #[error("compound file truncated: {0}")]
    Truncated(&'static str),
    #[error("unsupported sector shift {0}")]
    SectorShift(u16),
    #[error("compound file has no readable directory")]
    NoDirectory,
    #[error("no stream or storage named {0:?}")]
    NotFound(String),
    #[error("{0:?} is a storage, not a stream")]
    NotAStream(String),
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum Error {
    #[error("zip: {0}")]
    Zip(#[from] docboss_zip::Error),
    #[error("not a WordprocessingML package: {0}")]
    NotWordDocument(String),
    #[error("the package is encrypted (an OLE compound file, not a ZIP archive)")]
    Encrypted,
}

//! Errors of docboss-aio, prefixed by the layer they come from ("read:",
//! "io:", "http:").

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The document could not be read once its bytes were fetched.
    #[error("read: {0}")]
    Core(#[from] docboss_core::Error),
    /// A part named by the caller is not in the container.
    #[error("read: part {0:?} not found")]
    PartNotFound(String),
    /// The container is damaged in a way the range reader cannot plan around.
    #[error("read: {0}")]
    Container(String),
    #[error("io: {0}")]
    Io(std::io::Error),
    #[cfg(feature = "http")]
    #[error("http{}: {msg}", status.map(|code| format!(" {code}")).unwrap_or_default())]
    Http { status: Option<u16>, msg: String },
}

impl From<std::io::Error> for Error {
    fn from(inner: std::io::Error) -> Error {
        #[cfg(feature = "http")]
        if let Some(marker) = inner
            .get_ref()
            .and_then(|source| source.downcast_ref::<TransportMarker>())
        {
            return Error::Http {
                status: marker.status,
                msg: marker.msg.clone(),
            };
        }
        Error::Io(inner)
    }
}

impl From<docboss_zip::Error> for Error {
    fn from(inner: docboss_zip::Error) -> Error {
        Error::Container(format!("zip: {inner}"))
    }
}

/// An HTTP failure carried through `io::Error` across the [`crate::Backend`]
/// trait and recovered by `From<io::Error> for Error`.
#[cfg(feature = "http")]
#[derive(Debug)]
pub(crate) struct TransportMarker {
    pub status: Option<u16>,
    pub msg: String,
}

#[cfg(feature = "http")]
impl std::fmt::Display for TransportMarker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.msg)
    }
}

#[cfg(feature = "http")]
impl std::error::Error for TransportMarker {}

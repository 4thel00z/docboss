//! Async, range-fetching DOCX and DOC reading.
//!
//! [`AsyncDocument`] opens a document over a local file, bytes in memory or
//! (with the `http` feature) an `http(s)://` URL, and fetches only the byte
//! ranges a read needs. A DOCX is a ZIP archive whose central directory
//! sits at the end, so the tail is read first and then only the XML and
//! relationship entries, never the images, unless asked. A DOC is a
//! compound file: the header, FAT, directory and mini stream are read,
//! then only the sectors of the Word streams, skipping embedded objects.
//! Requests are coalesced, cached and counted.

mod backend;
mod cfb;
mod document;
mod error;
mod fetch;
mod zip;

#[cfg(feature = "http")]
pub use backend::HttpBackend;
pub use backend::{Backend, BoxFuture, FileBackend, MemBackend};
pub use document::{AsyncDocument, ReadOptions};
pub use error::{Error, Result};
pub use fetch::FetchObserver;

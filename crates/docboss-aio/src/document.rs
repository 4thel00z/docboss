use std::path::Path;
use std::sync::Arc;

use docboss_core::Format;
use docboss_model::{sniff_image, Document};

use crate::backend::{Backend, FileBackend, MemBackend};
use crate::cfb::{self, CfbIndex};
use crate::fetch::{FetchObserver, Fetcher};
use crate::zip::{self, ZipIndex};
use crate::{Error, Result};

/// The stack of the parsing thread: the readers recurse into nested
/// tables, fields and text boxes, deeper than a runtime worker's stack.
const PARSE_STACK: usize = 64 << 20;

/// Runs the synchronous reader on its own thread so the runtime is never
/// blocked, with a stack large enough for deeply nested documents.
async fn parse(bytes: Vec<u8>, options: docboss_core::Options) -> Result<Document> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    std::thread::Builder::new()
        .name("docboss-parse".into())
        .stack_size(PARSE_STACK)
        .spawn(move || {
            let _ = sender.send(docboss_core::read_with(&bytes, &options));
        })?;
    let result = receiver
        .await
        .map_err(|_| std::io::Error::other("the parsing thread panicked"))?;
    Ok(result?)
}

/// What [`AsyncDocument::read`] fetches and how it opens the document.
#[derive(Debug, Clone, Default)]
pub struct ReadOptions {
    /// Fetch images and embedded fonts too. Without it a DOCX read fetches
    /// only its XML and relationship parts, and the media entries of the
    /// returned document are empty until [`AsyncDocument::load_media`].
    pub media: bool,
    /// The password of an encrypted DOC or DOCX.
    pub password: Option<String>,
}

enum Container {
    Zip(ZipIndex),
    Cfb(CfbIndex),
    Whole,
}

/// A document over a random-access byte source that fetches only the
/// byte ranges a read needs: a local file, bytes in memory, or (with the
/// `http` feature) an `http(s)://` URL read with range requests.
pub struct AsyncDocument {
    fetcher: Fetcher,
    container: Container,
    format: Format,
}

impl AsyncDocument {
    pub async fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::from_backend(Arc::new(FileBackend::open(path).await?), None).await
    }

    pub async fn from_bytes(data: Vec<u8>) -> Result<Self> {
        Self::from_backend(Arc::new(MemBackend::from(data)), None).await
    }

    /// Opens a remote document; a server that ignores `Range` costs one
    /// full download.
    #[cfg(feature = "http")]
    pub async fn open_url(url: &str) -> Result<Self> {
        Self::open_url_with(url, None).await
    }

    /// Opens a remote document, calling `observer(offset, length)` after
    /// every request.
    #[cfg(feature = "http")]
    pub async fn open_url_with(url: &str, observer: Option<FetchObserver>) -> Result<Self> {
        let backend = crate::backend::HttpBackend::new(url).await?;
        Self::from_backend(Arc::new(backend), observer).await
    }

    /// Opens a document over any backend: reads the first bytes to find the
    /// container, then its directory.
    pub async fn from_backend(
        backend: Arc<dyn Backend>,
        observer: Option<FetchObserver>,
    ) -> Result<Self> {
        let fetcher = Fetcher::new(backend, observer).await?;
        let head = fetcher.fetch_one(0..8).await?;
        let (container, format) = if head.starts_with(b"PK") {
            match zip::index(&fetcher).await {
                Ok(index) => (Container::Zip(index), Format::Docx),
                Err(_) => (Container::Whole, Format::Docx),
            }
        } else if cfb::is_compound_file(&head) && fetcher.len() <= cfb::MAX_IMAGE {
            let index = cfb::index(&fetcher).await?;
            let format = if index.entries.iter().any(|e| e.name == "WordDocument") {
                Format::Doc
            } else if index.entries.iter().any(|e| e.name == "EncryptedPackage") {
                Format::EncryptedDocx
            } else {
                Format::Unknown
            };
            (Container::Cfb(index), format)
        } else {
            (Container::Whole, Format::Unknown)
        };
        Ok(Self {
            fetcher,
            container,
            format,
        })
    }

    /// The container format, detected from the leading bytes and directory.
    pub fn format(&self) -> Format {
        self.format
    }

    /// The size of the source in bytes.
    pub fn len(&self) -> u64 {
        self.fetcher.len()
    }

    pub fn is_empty(&self) -> bool {
        self.fetcher.len() == 0
    }

    /// Bytes fetched from the source so far.
    pub fn bytes_fetched(&self) -> u64 {
        self.fetcher.bytes_fetched()
    }

    /// The `(offset, length)` of every request made so far.
    pub fn requests(&self) -> Vec<(u64, u64)> {
        self.fetcher.requests()
    }

    /// The ZIP entry names or compound file stream names.
    pub fn part_names(&self) -> Vec<String> {
        match &self.container {
            Container::Zip(index) => index.entries.iter().map(|e| e.name.clone()).collect(),
            Container::Cfb(index) => index
                .entries
                .iter()
                .filter(|e| e.is_stream)
                .map(|e| e.name.clone())
                .collect(),
            Container::Whole => Vec::new(),
        }
    }

    /// A compact archive of the `wanted` entries, the `placeholders` kept as
    /// empty entries.
    async fn zip_parts(
        &self,
        index: &ZipIndex,
        wanted: &[&str],
        placeholders: &[&str],
    ) -> Result<Vec<u8>> {
        let entries: Vec<_> = wanted.iter().filter_map(|n| index.find(n)).collect();
        let ranges: Vec<_> = entries.iter().map(|e| index.region(e)).collect();
        let regions = self.fetcher.fetch(&ranges).await?;
        let mut parts: Vec<_> = entries
            .iter()
            .copied()
            .zip(regions.iter().map(|r| Some(r.as_slice())))
            .collect();
        parts.extend(
            placeholders
                .iter()
                .filter_map(|n| index.find(n))
                .map(|e| (e, None)),
        );
        zip::repack(&parts)
    }

    /// The uncompressed bytes of one ZIP entry or compound file stream.
    pub async fn part(&self, name: &str) -> Result<Vec<u8>> {
        match &self.container {
            Container::Zip(index) => {
                let entry = index
                    .find(name)
                    .ok_or_else(|| Error::PartNotFound(name.into()))?;
                let archive_bytes = self.zip_parts(index, &[entry.name.as_str()], &[]).await?;
                let archive = docboss_zip::Archive::new(&archive_bytes)?;
                Ok(archive.entry(&entry.name)?.into_owned())
            }
            Container::Cfb(index) => {
                index.fetch_streams(&self.fetcher, &[name]).await?;
                let image = self.fetcher.image();
                let file = docboss_cfb::CompoundFile::parse(&image)
                    .map_err(|e| Error::Container(e.to_string()))?;
                let stream = file
                    .open_stream(name)
                    .map_err(|_| Error::PartNotFound(name.into()))?;
                Ok(stream.into_owned())
            }
            Container::Whole => Err(Error::PartNotFound(name.into())),
        }
    }

    /// Fetches what the document needs and reads it with the synchronous
    /// readers on a blocking thread.
    pub async fn read(&self, options: &ReadOptions) -> Result<Document> {
        let core_options = docboss_core::Options {
            password: options.password.clone(),
        };
        let (bytes, skipped) = match &self.container {
            Container::Zip(index) => {
                let (wanted, skipped): (Vec<_>, Vec<_>) = index
                    .entries
                    .iter()
                    .filter(|e| !e.is_dir())
                    .map(|e| e.name.as_str())
                    .partition(|name| options.media || zip::is_needed(name));
                let bytes = self.zip_parts(index, &wanted, &skipped).await?;
                (bytes, skipped.into_iter().map(String::from).collect())
            }
            Container::Cfb(index) => {
                index.fetch_streams(&self.fetcher, &cfb::WANTED).await?;
                (self.fetcher.image(), Vec::new())
            }
            Container::Whole => (
                self.fetcher.fetch_one(0..self.fetcher.len()).await?,
                Vec::new(),
            ),
        };
        let mut document = parse(bytes, core_options).await?;
        if !skipped.is_empty() {
            document.diagnostics.retain(|d| {
                !skipped
                    .iter()
                    .any(|name| d.location == *name || d.message.contains(name.as_str()))
            });
        }
        Ok(document)
    }

    /// Fills in the images a text-only [`read`](Self::read) left empty,
    /// fetching all of them in one coalesced round.
    pub async fn load_media(&self, document: &mut Document) -> Result<()> {
        let Container::Zip(index) = &self.container else {
            return Ok(());
        };
        let names: Vec<&str> = document
            .media
            .iter()
            .filter(|m| m.data.is_empty())
            .map(|m| m.name.as_str())
            .collect();
        if names.is_empty() {
            return Ok(());
        }
        let archive_bytes = self.zip_parts(index, &names, &[]).await?;
        let archive = docboss_zip::Archive::new(&archive_bytes)?;
        for media in document.media.iter_mut().filter(|m| m.data.is_empty()) {
            let Ok(data) = archive.entry(&media.name) else {
                continue;
            };
            let sniffed = sniff_image(&data);
            if sniffed != "application/octet-stream" {
                media.content_type = sniffed.to_string();
            }
            media.data = Arc::from(data.into_owned());
        }
        Ok(())
    }
}

//! `AsyncDocument`: documents over files, bytes or `http(s)://` URLs that
//! fetch only the byte ranges a read needs, with coroutine methods running
//! on the tokio runtime of pyo3-async-runtimes.

use std::path::PathBuf;
use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::PyBytes;
use tokio::sync::OnceCell;

use docboss_aio::{AsyncDocument as Inner, ReadOptions};
use docboss_core::Format;
use docboss_model::Document as Model;
use docboss_output::{HtmlOptions, MarkdownOptions, TextOptions};

use crate::document::image_mode;
use crate::types::{Block, Image};
use crate::{byte_arg, doc_err, Document};

/// The range reader and the models read from it: the text-only model
/// (markup parts only) and, on first need, the model with its media.
struct Shared {
    inner: Inner,
    password: Option<String>,
    text: OnceCell<Arc<Model>>,
    full: OnceCell<Arc<Model>>,
}

impl Shared {
    async fn text(&self) -> PyResult<Arc<Model>> {
        let model = self
            .text
            .get_or_try_init(|| async {
                let options = ReadOptions {
                    media: false,
                    password: self.password.clone(),
                };
                self.inner
                    .read(&options)
                    .await
                    .map(Arc::new)
                    .map_err(doc_err)
            })
            .await?;
        Ok(model.clone())
    }

    async fn full(&self) -> PyResult<Arc<Model>> {
        let model = self
            .full
            .get_or_try_init(|| async {
                let mut document = (*self.text().await?).clone();
                self.inner
                    .load_media(&mut document)
                    .await
                    .map_err(doc_err)?;
                Ok::<_, PyErr>(Arc::new(document))
            })
            .await?;
        Ok(model.clone())
    }
}

/// Runs CPU-bound work on the runtime's blocking pool.
async fn blocking<R: Send + 'static>(work: impl FnOnce() -> R + Send + 'static) -> PyResult<R> {
    tokio::task::spawn_blocking(work).await.map_err(doc_err)
}

fn opened(inner: Inner, password: Option<String>) -> AsyncDocument {
    AsyncDocument {
        shared: Arc::new(Shared {
            inner,
            password,
            text: OnceCell::new(),
            full: OnceCell::new(),
        }),
    }
}

/// A DOCX or DOC document read asynchronously. Opening fetches only the
/// container's index (a ZIP central directory or the compound file's
/// directory); text extraction then fetches the markup parts, and images
/// are fetched only when `images()` or `document()` asks for them.
#[pyclass(frozen, module = "docboss._docboss")]
pub struct AsyncDocument {
    shared: Arc<Shared>,
}

#[pymethods]
impl AsyncDocument {
    /// Opens a local file. Coroutine resolving to an AsyncDocument.
    #[staticmethod]
    #[pyo3(signature = (path, *, password=None))]
    fn open(py: Python<'_>, path: PathBuf, password: Option<String>) -> PyResult<Bound<'_, PyAny>> {
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let inner = Inner::open(path).await.map_err(doc_err)?;
            Ok(opened(inner, password))
        })
    }

    /// Reads a document from bytes in memory. Coroutine resolving to an
    /// AsyncDocument.
    #[staticmethod]
    #[pyo3(signature = (data, *, password=None))]
    fn from_bytes<'py>(
        py: Python<'py>,
        data: &Bound<'py, PyAny>,
        password: Option<String>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let data = byte_arg(data)?;
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let inner = Inner::from_bytes(data).await.map_err(doc_err)?;
            Ok(opened(inner, password))
        })
    }

    /// Opens a document over HTTP(S) with range requests. A server that
    /// ignores `Range` costs one full download. Coroutine resolving to an
    /// AsyncDocument.
    #[staticmethod]
    #[pyo3(signature = (url, *, password=None))]
    fn open_url(
        py: Python<'_>,
        url: String,
        password: Option<String>,
    ) -> PyResult<Bound<'_, PyAny>> {
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let inner = Inner::open_url(&url).await.map_err(doc_err)?;
            Ok(opened(inner, password))
        })
    }

    /// `docx` or `doc`, detected from the container.
    #[getter]
    fn format(&self) -> &'static str {
        match self.shared.inner.format() {
            Format::Docx | Format::EncryptedDocx => "docx",
            Format::Doc => "doc",
            Format::Rtf | Format::Unknown => "unknown",
        }
    }

    /// The number of bytes the reader has requested from the source so far:
    /// the sum of the lengths in `requests`.
    #[getter]
    fn bytes_fetched(&self) -> u64 {
        self.shared.inner.bytes_fetched()
    }

    /// Every range requested so far, as `(offset, length)` pairs in order.
    #[getter]
    fn requests(&self) -> Vec<(u64, u64)> {
        self.shared.inner.requests()
    }

    /// The names of the container's ZIP entries or compound file streams.
    fn part_names(&self) -> Vec<String> {
        self.shared.inner.part_names()
    }

    /// The bytes of one ZIP entry or compound file stream. Coroutine.
    fn part<'py>(&self, py: Python<'py>, name: String) -> PyResult<Bound<'py, PyAny>> {
        let shared = self.shared.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let bytes = shared.inner.part(&name).await.map_err(doc_err)?;
            Ok(Python::with_gil(|py| PyBytes::new(py, &bytes).unbind()))
        })
    }

    /// The main text, like `Document.extract_text`. Coroutine.
    #[pyo3(signature = (*, list_labels=true, headers_footers=false, notes=true, comments=false))]
    fn extract_text<'py>(
        &self,
        py: Python<'py>,
        list_labels: bool,
        headers_footers: bool,
        notes: bool,
        comments: bool,
    ) -> PyResult<Bound<'py, PyAny>> {
        let options = TextOptions {
            list_labels,
            headers_footers,
            notes,
            comments,
        };
        let shared = self.shared.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let document = shared.text().await?;
            blocking(move || docboss_output::to_text(&document, &options)).await
        })
    }

    /// Markdown, like `Document.extract_markdown`. Media is fetched only
    /// for `images="embed"`. Coroutine.
    #[pyo3(signature = (*, title=false, page_breaks=false, images="reference", image_prefix="", headers_footers=false, notes=true, comments=false))]
    #[allow(clippy::too_many_arguments)]
    fn extract_markdown<'py>(
        &self,
        py: Python<'py>,
        title: bool,
        page_breaks: bool,
        images: &str,
        image_prefix: &str,
        headers_footers: bool,
        notes: bool,
        comments: bool,
    ) -> PyResult<Bound<'py, PyAny>> {
        let embed = images == "embed";
        let options = MarkdownOptions {
            title,
            page_breaks,
            images: image_mode(images, image_prefix)?,
            headers_footers,
            notes,
            comments,
        };
        let shared = self.shared.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let document = match embed {
                true => shared.full().await?,
                false => shared.text().await?,
            };
            blocking(move || docboss_output::to_markdown(&document, &options)).await
        })
    }

    /// HTML, like `Document.extract_html`. Media is fetched only for
    /// `images="embed"`, the default. Coroutine.
    #[pyo3(signature = (*, standalone=false, images="embed", image_prefix="", headers_footers=false, notes=true, comments=false))]
    #[allow(clippy::too_many_arguments)]
    fn extract_html<'py>(
        &self,
        py: Python<'py>,
        standalone: bool,
        images: &str,
        image_prefix: &str,
        headers_footers: bool,
        notes: bool,
        comments: bool,
    ) -> PyResult<Bound<'py, PyAny>> {
        let embed = images == "embed";
        let options = HtmlOptions {
            standalone,
            images: image_mode(images, image_prefix)?,
            headers_footers,
            notes,
            comments,
        };
        let shared = self.shared.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let document = match embed {
                true => shared.full().await?,
                false => shared.text().await?,
            };
            blocking(move || docboss_output::to_html(&document, &options)).await
        })
    }

    /// The document model as JSON, like `Document.to_json`. Coroutine.
    #[pyo3(signature = (*, pretty=false))]
    fn to_json<'py>(&self, py: Python<'py>, pretty: bool) -> PyResult<Bound<'py, PyAny>> {
        let shared = self.shared.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let document = shared.text().await?;
            blocking(move || docboss_output::to_json(&document, pretty))
                .await?
                .map_err(doc_err)
        })
    }

    /// The blocks of the main text, like `Document.blocks`. Coroutine.
    fn blocks<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let shared = self.shared.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let document = shared.text().await?;
            let views = blocking(move || docboss_output::blocks_view(&document)).await?;
            Ok(views.into_iter().map(Block::from).collect::<Vec<_>>())
        })
    }

    /// Every image the text refers to, fetched on first call. Coroutine.
    fn images<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let shared = self.shared.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let document = shared.full().await?;
            Ok(document.media.iter().map(Image::from).collect::<Vec<_>>())
        })
    }

    /// A synchronous `Document` over the fully fetched model, for layout,
    /// rendering and DOCX writing. Coroutine.
    fn document<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let shared = self.shared.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            Ok(Document::from_model(shared.full().await?))
        })
    }

    fn __repr__(&self) -> String {
        format!(
            "<docboss.AsyncDocument format={} bytes_fetched={}>",
            self.format(),
            self.bytes_fetched()
        )
    }
}

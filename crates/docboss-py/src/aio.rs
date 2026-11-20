//! `AsyncDocument`: documents over files, bytes or `http(s)://` URLs that
//! fetch only the byte ranges a read needs, with coroutine methods running
//! on the tokio runtime of pyo3-async-runtimes.

use std::path::PathBuf;
use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PySlice};
use tokio::sync::OnceCell;

use docboss_aio::{AsyncDocument as Inner, ReadOptions};
use docboss_model::Document as Model;
use docboss_output::{HtmlOptions, ImageMode, MarkdownOptions, TextOptions};
use docboss_pages::{Format, Output};

use crate::document::image_mode;
use crate::pages::{format_arg, output_list, output_object, position, Slice};
use crate::types::{Block, Image};
use crate::{byte_arg, doc_err, Document};

/// The range reader and the models read from it: the text-only model
/// (markup parts only) and, on first need, the model with its media.
struct Shared {
    inner: Inner,
    password: Option<String>,
    text: OnceCell<Arc<Model>>,
    full: OnceCell<Arc<Model>>,
    pages: OnceCell<docboss_pages::Document>,
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

    /// The fully fetched model with its layout, computed on first use.
    async fn pages(&self) -> PyResult<docboss_pages::Document> {
        let document = self
            .pages
            .get_or_try_init(|| async {
                Ok::<_, PyErr>(docboss_pages::Document::new(self.full().await?))
            })
            .await?;
        Ok(document.clone())
    }

    /// Converts the whole document, fetching media only when the format
    /// embeds images.
    async fn convert(&self, format: Format) -> PyResult<String> {
        let model = match embeds_images(&format) {
            true => self.full().await?,
            false => self.text().await?,
        };
        let output = blocking(move || docboss_pages::Document::new(model).to(&format))
            .await?
            .map_err(doc_err)?;
        match output {
            Output::Text(text) => Ok(text),
            Output::Bytes(_) => Ok(String::new()),
        }
    }
}

fn embeds_images(format: &Format) -> bool {
    match format {
        Format::Markdown(options) => options.images == ImageMode::Embed,
        Format::Html(options) => options.images == ImageMode::Embed,
        _ => false,
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
            pages: OnceCell::new(),
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
            docboss_core::Format::Docx | docboss_core::Format::EncryptedDocx => "docx",
            docboss_core::Format::Doc => "doc",
            docboss_core::Format::Rtf | docboss_core::Format::Unknown => "unknown",
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

    /// The whole document as text, Markdown, HTML or JSON, like
    /// `Document.to`. Media is fetched only for `images="embed"`.
    /// Coroutine.
    #[pyo3(signature = (spec=None, /, **options))]
    fn to<'py>(
        &self,
        py: Python<'py>,
        spec: Option<&Bound<'py, PyAny>>,
        options: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let format = format_arg(spec, options)?;
        let shared = self.shared.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move { shared.convert(format).await })
    }

    /// Every laid-out page. The document is fetched in full and laid out
    /// when a conversion first needs it.
    fn pages(&self) -> AsyncPages {
        AsyncPages {
            shared: self.shared.clone(),
            slices: Vec::new(),
        }
    }

    /// One laid-out page; negative indexes count from the end.
    fn page(&self, index: isize) -> AsyncPage {
        AsyncPage {
            shared: self.shared.clone(),
            slices: Vec::new(),
            index,
        }
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
        let format = Format::Text(TextOptions {
            list_labels,
            headers_footers,
            notes,
            comments,
        });
        let shared = self.shared.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move { shared.convert(format).await })
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
        let format = Format::Markdown(MarkdownOptions {
            title,
            page_breaks,
            images: image_mode(images, image_prefix)?,
            headers_footers,
            notes,
            comments,
        });
        let shared = self.shared.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move { shared.convert(format).await })
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
        let format = Format::Html(HtmlOptions {
            standalone,
            images: image_mode(images, image_prefix)?,
            headers_footers,
            notes,
            comments,
        });
        let shared = self.shared.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move { shared.convert(format).await })
    }

    /// The document model as JSON, like `Document.to_json`. Coroutine.
    #[pyo3(signature = (*, pretty=false))]
    fn to_json<'py>(&self, py: Python<'py>, pretty: bool) -> PyResult<Bound<'py, PyAny>> {
        let format = Format::Json { pretty };
        let shared = self.shared.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move { shared.convert(format).await })
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
            Ok(Document::from_pages(shared.pages().await?))
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

/// The positions a chain of slices selects from `len` pages.
fn select(slices: &[Slice], len: usize) -> Vec<usize> {
    slices
        .iter()
        .fold((0..len).collect(), |positions: Vec<usize>, slice| {
            slice
                .positions(positions.len())
                .into_iter()
                .filter_map(|at| positions.get(at).copied())
                .collect()
        })
}

/// The pages a chain of slices selects, laid out on the blocking pool.
async fn resolve(shared: &Shared, slices: Vec<Slice>) -> PyResult<docboss_pages::Pages> {
    let document = shared.pages().await?;
    blocking(move || {
        let pages = document.pages();
        let positions = select(&slices, pages.len());
        pages.pick(positions)
    })
    .await
}

/// A selection of an `AsyncDocument`'s pages. Slicing is lazy; the
/// document is fetched and laid out when `to()` or `count()` first runs.
#[pyclass(frozen, module = "docboss._docboss")]
pub struct AsyncPages {
    shared: Arc<Shared>,
    slices: Vec<Slice>,
}

#[pymethods]
impl AsyncPages {
    fn __getitem__(&self, py: Python<'_>, key: &Bound<'_, PyAny>) -> PyResult<PyObject> {
        if let Ok(slice) = key.downcast::<PySlice>() {
            let mut slices = self.slices.clone();
            slices.push(Slice::from_py(slice)?);
            let pages = AsyncPages {
                shared: self.shared.clone(),
                slices,
            };
            return Ok(Bound::new(py, pages)?.into_any().unbind());
        }
        let page = AsyncPage {
            shared: self.shared.clone(),
            slices: self.slices.clone(),
            index: key.extract()?,
        };
        Ok(Bound::new(py, page)?.into_any().unbind())
    }

    /// The number of pages selected. Coroutine.
    fn count<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let shared = self.shared.clone();
        let slices = self.slices.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            Ok(resolve(&shared, slices).await?.len())
        })
    }

    /// Converts every selected page, like `Pages.to`. Coroutine.
    #[pyo3(signature = (spec=None, /, **options))]
    fn to<'py>(
        &self,
        py: Python<'py>,
        spec: Option<&Bound<'py, PyAny>>,
        options: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let format = format_arg(spec, options)?;
        let shared = self.shared.clone();
        let slices = self.slices.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let pages = resolve(&shared, slices).await?;
            let outputs = blocking(move || pages.to(&format))
                .await?
                .map_err(doc_err)?;
            Python::with_gil(|py| output_list(py, outputs))
        })
    }

    fn __repr__(&self) -> String {
        format!("<docboss.AsyncPages slices={}>", self.slices.len())
    }
}

/// One page of an `AsyncDocument`, resolved when `to()` runs.
#[pyclass(frozen, module = "docboss._docboss")]
pub struct AsyncPage {
    shared: Arc<Shared>,
    slices: Vec<Slice>,
    index: isize,
}

#[pymethods]
impl AsyncPage {
    /// Converts the page, like `Page.to`. Raises `IndexError` when the
    /// selection has no such page. Coroutine.
    #[pyo3(signature = (spec=None, /, **options))]
    fn to<'py>(
        &self,
        py: Python<'py>,
        spec: Option<&Bound<'py, PyAny>>,
        options: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let format = format_arg(spec, options)?;
        let shared = self.shared.clone();
        let slices = self.slices.clone();
        let index = self.index;
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let pages = resolve(&shared, slices).await?;
            let at = position(index, pages.len())?;
            let page = pages.pick([at]);
            let outputs = blocking(move || page.to(&format)).await?.map_err(doc_err)?;
            let output = outputs
                .into_iter()
                .next()
                .unwrap_or(Output::Text(String::new()));
            Python::with_gil(|py| output_object(py, output))
        })
    }

    fn __repr__(&self) -> String {
        format!("<docboss.AsyncPage index={}>", self.index)
    }
}

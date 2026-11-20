use std::path::PathBuf;

use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};

use docboss_model::SourceFormat;
use docboss_output::{HtmlOptions, ImageMode, MarkdownOptions, TextOptions};
use docboss_pages::{Format, Output};

use crate::pages::{format_arg, output_object, position, Page, Pages};
use crate::types::{Block, Diagnostic, FontInfo, Image, Metadata, StyleInfo};
use crate::{byte_arg, doc_err};

fn image_format(format: &str, scale: f32, jpeg_quality: u8) -> PyResult<Format> {
    if !(scale.is_finite() && scale > 0.0) {
        return Err(PyValueError::new_err("scale must be a positive number"));
    }
    match format.to_ascii_lowercase().as_str() {
        "png" => Ok(Format::Png { scale }),
        "ppm" => Ok(Format::Ppm { scale }),
        "bmp" => Ok(Format::Bmp { scale }),
        "jpeg" | "jpg" => Ok(Format::Jpeg {
            scale,
            quality: jpeg_quality.clamp(1, 100),
        }),
        other => Err(PyValueError::new_err(format!(
            "unknown format {other:?}; expected png, ppm, bmp or jpeg"
        ))),
    }
}

pub(crate) fn image_mode(images: &str, prefix: &str) -> PyResult<ImageMode> {
    match images {
        "reference" => Ok(ImageMode::Reference {
            prefix: prefix.to_string(),
        }),
        "embed" => Ok(ImageMode::Embed),
        "omit" => Ok(ImageMode::Omit),
        other => Err(PyValueError::new_err(format!(
            "unknown images mode {other:?}; expected reference, embed or omit"
        ))),
    }
}

/// The text of a whole-document conversion.
fn convert(py: Python<'_>, document: &docboss_pages::Document, format: Format) -> PyResult<String> {
    let document = document.clone();
    let output = py
        .allow_threads(move || document.to(&format))
        .map_err(doc_err)?;
    match output {
        Output::Text(text) => Ok(text),
        Output::Bytes(_) => Ok(String::new()),
    }
}

/// A DOCX or DOC document, opened from a path or from bytes. The format is
/// detected from the bytes.
#[pyclass(frozen, module = "docboss._docboss")]
pub struct Document {
    inner: docboss_pages::Document,
}

impl Document {
    /// A document over a model another reader already parsed.
    pub(crate) fn from_pages(inner: docboss_pages::Document) -> Self {
        Self { inner }
    }

    fn model(&self) -> &docboss_model::Document {
        self.inner.model()
    }

    /// Every laid-out page, computing the layout without the GIL on first
    /// use.
    fn all_pages(&self, py: Python<'_>) -> docboss_pages::Pages {
        let inner = self.inner.clone();
        py.allow_threads(move || inner.pages())
    }
}

#[pymethods]
impl Document {
    #[new]
    #[pyo3(signature = (path=None, *, data=None, password=None))]
    fn new(
        py: Python<'_>,
        path: Option<PathBuf>,
        data: Option<&Bound<'_, PyAny>>,
        password: Option<String>,
    ) -> PyResult<Self> {
        let bytes = match (path, data) {
            (Some(path), None) => py.allow_threads(|| std::fs::read(&path)).map_err(doc_err)?,
            (None, Some(data)) => byte_arg(data)?,
            _ => return Err(PyTypeError::new_err("pass exactly one of path or data=")),
        };
        let options = docboss_core::Options { password };
        let document = py
            .allow_threads(|| docboss_core::read_with(&bytes, &options))
            .map_err(doc_err)?;
        Ok(Self {
            inner: docboss_pages::Document::new(document),
        })
    }

    /// `docx` or `doc`: the format the document was read from.
    #[getter]
    fn format(&self) -> &'static str {
        match self.model().format {
            SourceFormat::Docx => "docx",
            SourceFormat::Doc => "doc",
        }
    }

    #[getter]
    fn metadata(&self) -> Metadata {
        Metadata::from(&self.model().metadata)
    }

    /// What the reader approximated or dropped.
    #[getter]
    fn diagnostics(&self) -> Vec<Diagnostic> {
        self.model()
            .diagnostics
            .iter()
            .map(Diagnostic::from)
            .collect()
    }

    #[getter]
    fn fonts(&self) -> Vec<FontInfo> {
        self.model().fonts.iter().map(FontInfo::from).collect()
    }

    #[getter]
    fn styles(&self) -> Vec<StyleInfo> {
        self.model()
            .styles
            .styles
            .iter()
            .map(StyleInfo::from)
            .collect()
    }

    /// The whole document as text, Markdown, HTML or JSON. Pass one dict
    /// such as `{"format": "markdown", "images": "omit"}` or the same keys
    /// as keyword arguments. Image formats render pages: use `page()` or
    /// `pages()` for them.
    #[pyo3(signature = (spec=None, /, **options))]
    fn to(
        &self,
        py: Python<'_>,
        spec: Option<&Bound<'_, PyAny>>,
        options: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<String> {
        let format = format_arg(spec, options)?;
        convert(py, &self.inner, format)
    }

    /// Every laid-out page. The layout is computed on first use and cached.
    fn pages(&self, py: Python<'_>) -> Pages {
        Pages {
            inner: self.all_pages(py),
        }
    }

    /// One laid-out page; negative indexes count from the end.
    fn page(&self, py: Python<'_>, index: isize) -> PyResult<Page> {
        let pages = self.all_pages(py);
        let at = position(index, pages.len())?;
        let Some(inner) = pages.get(at) else {
            return Err(pyo3::exceptions::PyIndexError::new_err(format!(
                "page {index} out of range"
            )));
        };
        Ok(Page { inner })
    }

    /// The main text, one line per paragraph, list labels included. Same
    /// as `to(format="text", ...)`.
    #[pyo3(signature = (*, list_labels=true, headers_footers=false, notes=true, comments=false))]
    fn extract_text(
        &self,
        py: Python<'_>,
        list_labels: bool,
        headers_footers: bool,
        notes: bool,
        comments: bool,
    ) -> PyResult<String> {
        let options = TextOptions {
            list_labels,
            headers_footers,
            notes,
            comments,
        };
        convert(py, &self.inner, Format::Text(options))
    }

    /// GitHub-flavored Markdown: headings from styles, lists from
    /// numbering, tables, links, footnotes. `images` is `reference` (names
    /// under `image_prefix`), `embed` (data URIs) or `omit`. Same as
    /// `to(format="markdown", ...)`.
    #[pyo3(signature = (*, title=false, page_breaks=false, images="reference", image_prefix="", headers_footers=false, notes=true, comments=false))]
    #[allow(clippy::too_many_arguments)]
    fn extract_markdown(
        &self,
        py: Python<'_>,
        title: bool,
        page_breaks: bool,
        images: &str,
        image_prefix: &str,
        headers_footers: bool,
        notes: bool,
        comments: bool,
    ) -> PyResult<String> {
        let options = MarkdownOptions {
            title,
            page_breaks,
            images: image_mode(images, image_prefix)?,
            headers_footers,
            notes,
            comments,
        };
        convert(py, &self.inner, Format::Markdown(options))
    }

    /// Semantic HTML5: a body fragment, or a whole document with
    /// `standalone=True`. Same as `to(format="html", ...)`.
    #[pyo3(signature = (*, standalone=false, images="embed", image_prefix="", headers_footers=false, notes=true, comments=false))]
    #[allow(clippy::too_many_arguments)]
    fn extract_html(
        &self,
        py: Python<'_>,
        standalone: bool,
        images: &str,
        image_prefix: &str,
        headers_footers: bool,
        notes: bool,
        comments: bool,
    ) -> PyResult<String> {
        let options = HtmlOptions {
            standalone,
            images: image_mode(images, image_prefix)?,
            headers_footers,
            notes,
            comments,
        };
        convert(py, &self.inner, Format::Html(options))
    }

    /// The whole document model as JSON. Same as `to(format="json", ...)`.
    #[pyo3(signature = (*, pretty=false))]
    fn to_json(&self, py: Python<'_>, pretty: bool) -> PyResult<String> {
        convert(py, &self.inner, Format::Json { pretty })
    }

    /// The blocks of the main text with their role and plain text.
    fn blocks(&self, py: Python<'_>) -> Vec<Block> {
        let inner = self.inner.clone();
        py.allow_threads(move || docboss_output::blocks_view(inner.model()))
            .into_iter()
            .map(Block::from)
            .collect()
    }

    /// Every image and binary part the text refers to.
    fn images(&self) -> Vec<Image> {
        self.model().media.iter().map(Image::from).collect()
    }

    /// The number of pages the document lays out to. Same as
    /// `len(pages())`.
    fn page_count(&self, py: Python<'_>) -> usize {
        self.all_pages(py).len()
    }

    /// Renders one page (negative indexes count from the end) at `scale`
    /// pixels per point, encoded as `png`, `ppm`, `bmp` or `jpeg`. Same as
    /// `page(page).to(format=..., scale=...)`.
    #[pyo3(signature = (page=0, *, scale=1.0, format="png", jpeg_quality=90))]
    fn render(
        &self,
        py: Python<'_>,
        page: isize,
        scale: f32,
        format: &str,
        jpeg_quality: u8,
    ) -> PyResult<PyObject> {
        let format = image_format(format, scale, jpeg_quality)?;
        let page = self.page(py, page)?.inner;
        let output = py
            .allow_threads(move || page.to(&format))
            .map_err(doc_err)?;
        output_object(py, output)
    }

    /// Renders several pages (all by default) across the machine's cores,
    /// returned in the order asked for. Same as `pages().to(...)` over the
    /// pages picked.
    #[pyo3(signature = (pages=None, *, scale=1.0, format="png", jpeg_quality=90))]
    fn render_pages<'py>(
        &self,
        py: Python<'py>,
        pages: Option<Vec<isize>>,
        scale: f32,
        format: &str,
        jpeg_quality: u8,
    ) -> PyResult<Vec<Bound<'py, PyBytes>>> {
        let format = image_format(format, scale, jpeg_quality)?;
        let all = self.all_pages(py);
        let picked = match pages {
            None => all,
            Some(pages) => {
                let positions: PyResult<Vec<usize>> = pages
                    .into_iter()
                    .map(|page| position(page, all.len()))
                    .collect();
                all.pick(positions?)
            }
        };
        let outputs = py
            .allow_threads(move || picked.to(&format))
            .map_err(doc_err)?;
        Ok(outputs
            .iter()
            .filter_map(Output::as_bytes)
            .map(|bytes| PyBytes::new(py, bytes))
            .collect())
    }

    /// The document written as DOCX bytes: a DOC converts to DOCX this way.
    fn to_docx<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let inner = self.inner.clone();
        let bytes = py
            .allow_threads(move || docboss_write::to_bytes(inner.model()))
            .map_err(doc_err)?;
        Ok(PyBytes::new(py, &bytes))
    }

    /// Writes the document as a DOCX file.
    fn save_docx(&self, py: Python<'_>, path: PathBuf) -> PyResult<()> {
        let inner = self.inner.clone();
        py.allow_threads(move || docboss_write::save(inner.model(), &path))
            .map_err(doc_err)
    }

    fn __repr__(&self) -> String {
        let title = self.model().metadata.title.as_deref().unwrap_or("");
        format!(
            "<docboss.Document format={} title={title:?} sections={}>",
            self.format(),
            self.model().sections.len()
        )
    }
}

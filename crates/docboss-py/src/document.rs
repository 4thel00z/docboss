use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use pyo3::exceptions::{PyIndexError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyBytes;

use docboss_font::FontDatabase;
use docboss_layout::Layout;
use docboss_model::SourceFormat;
use docboss_output::{HtmlOptions, ImageMode, MarkdownOptions, TextOptions};
use docboss_render::{Format, Renderer};

use crate::types::{Block, Diagnostic, FontInfo, Image, Metadata, StyleInfo};
use crate::{byte_arg, doc_err, parallel_map};

/// The system font database, scanned once and shared by every document
/// that embeds no fonts of its own, so parsed faces are reused across
/// documents.
fn system_fonts() -> Arc<FontDatabase> {
    static SYSTEM: OnceLock<Arc<FontDatabase>> = OnceLock::new();
    SYSTEM
        .get_or_init(|| Arc::new(FontDatabase::system()))
        .clone()
}

fn fonts_for(document: &docboss_model::Document) -> Arc<FontDatabase> {
    let embeds = document.fonts.iter().any(|f| {
        f.embedded_regular.is_some()
            || f.embedded_bold.is_some()
            || f.embedded_italic.is_some()
            || f.embedded_bold_italic.is_some()
    });
    if !embeds {
        return system_fonts();
    }
    docboss_layout::fonts_for(document)
}

fn image_format(format: &str, jpeg_quality: u8) -> PyResult<Format> {
    match format.to_ascii_lowercase().as_str() {
        "png" => Ok(Format::Png),
        "ppm" => Ok(Format::Ppm),
        "bmp" => Ok(Format::Bmp),
        "jpeg" | "jpg" => Ok(Format::Jpeg {
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

fn check_scale(scale: f32) -> PyResult<()> {
    if scale.is_finite() && scale > 0.0 {
        return Ok(());
    }
    Err(PyValueError::new_err("scale must be a positive number"))
}

fn encode_page(
    renderer: &mut Renderer,
    layout: &Layout,
    index: usize,
    scale: f32,
    format: Format,
) -> Result<Vec<u8>, String> {
    let pixmap = renderer
        .render(layout, index, scale)
        .map_err(|e| e.to_string())?;
    pixmap.encode(format).map_err(|e| e.to_string())
}

/// A DOCX or DOC document, opened from a path or from bytes. The format is
/// detected from the bytes.
#[pyclass(frozen, module = "docboss._docboss")]
pub struct Document {
    document: Arc<docboss_model::Document>,
    layout: OnceLock<Arc<Layout>>,
}

impl Document {
    /// A document over a model another reader already parsed.
    pub(crate) fn from_model(document: Arc<docboss_model::Document>) -> Self {
        Self {
            document,
            layout: OnceLock::new(),
        }
    }

    fn layout(&self, py: Python<'_>) -> Arc<Layout> {
        if let Some(layout) = self.layout.get() {
            return layout.clone();
        }
        let document = self.document.clone();
        let layout =
            py.allow_threads(|| Arc::new(docboss_layout::layout(&document, &fonts_for(&document))));
        self.layout.get_or_init(|| layout).clone()
    }

    fn page_index(&self, py: Python<'_>, page: isize) -> PyResult<(Arc<Layout>, usize)> {
        let layout = self.layout(py);
        let count = layout.pages.len() as isize;
        let index = if page < 0 { page + count } else { page };
        if !(0..count).contains(&index) {
            return Err(PyIndexError::new_err(format!(
                "page {page} out of range for {count} pages"
            )));
        }
        Ok((layout, index as usize))
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
            document: Arc::new(document),
            layout: OnceLock::new(),
        })
    }

    /// `docx` or `doc`: the format the document was read from.
    #[getter]
    fn format(&self) -> &'static str {
        match self.document.format {
            SourceFormat::Docx => "docx",
            SourceFormat::Doc => "doc",
        }
    }

    #[getter]
    fn metadata(&self) -> Metadata {
        Metadata::from(&self.document.metadata)
    }

    /// What the reader approximated or dropped.
    #[getter]
    fn diagnostics(&self) -> Vec<Diagnostic> {
        self.document
            .diagnostics
            .iter()
            .map(Diagnostic::from)
            .collect()
    }

    #[getter]
    fn fonts(&self) -> Vec<FontInfo> {
        self.document.fonts.iter().map(FontInfo::from).collect()
    }

    #[getter]
    fn styles(&self) -> Vec<StyleInfo> {
        self.document
            .styles
            .styles
            .iter()
            .map(StyleInfo::from)
            .collect()
    }

    /// The main text, one line per paragraph, list labels included.
    #[pyo3(signature = (*, list_labels=true, headers_footers=false, notes=true, comments=false))]
    fn extract_text(
        &self,
        py: Python<'_>,
        list_labels: bool,
        headers_footers: bool,
        notes: bool,
        comments: bool,
    ) -> String {
        let options = TextOptions {
            list_labels,
            headers_footers,
            notes,
            comments,
        };
        let document = self.document.clone();
        py.allow_threads(|| docboss_output::to_text(&document, &options))
    }

    /// GitHub-flavored Markdown: headings from styles, lists from
    /// numbering, tables, links, footnotes. `images` is `reference` (names
    /// under `image_prefix`), `embed` (data URIs) or `omit`.
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
        let document = self.document.clone();
        Ok(py.allow_threads(|| docboss_output::to_markdown(&document, &options)))
    }

    /// Semantic HTML5: a body fragment, or a whole document with
    /// `standalone=True`.
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
        let document = self.document.clone();
        Ok(py.allow_threads(|| docboss_output::to_html(&document, &options)))
    }

    /// The whole document model as JSON.
    #[pyo3(signature = (*, pretty=false))]
    fn to_json(&self, py: Python<'_>, pretty: bool) -> PyResult<String> {
        let document = self.document.clone();
        py.allow_threads(|| docboss_output::to_json(&document, pretty))
            .map_err(doc_err)
    }

    /// The blocks of the main text with their role and plain text.
    fn blocks(&self, py: Python<'_>) -> Vec<Block> {
        let document = self.document.clone();
        py.allow_threads(|| docboss_output::blocks_view(&document))
            .into_iter()
            .map(Block::from)
            .collect()
    }

    /// Every image and binary part the text refers to.
    fn images(&self) -> Vec<Image> {
        self.document.media.iter().map(Image::from).collect()
    }

    /// The number of pages the document lays out to. The layout is computed
    /// on first use and cached.
    fn page_count(&self, py: Python<'_>) -> usize {
        self.layout(py).pages.len()
    }

    /// Renders one page (negative indexes count from the end) at `scale`
    /// pixels per point, encoded as `png`, `ppm`, `bmp` or `jpeg`.
    #[pyo3(signature = (page=0, *, scale=1.0, format="png", jpeg_quality=90))]
    fn render<'py>(
        &self,
        py: Python<'py>,
        page: isize,
        scale: f32,
        format: &str,
        jpeg_quality: u8,
    ) -> PyResult<Bound<'py, PyBytes>> {
        check_scale(scale)?;
        let format = image_format(format, jpeg_quality)?;
        let (layout, index) = self.page_index(py, page)?;
        let bytes = py
            .allow_threads(|| encode_page(&mut Renderer::new(), &layout, index, scale, format))
            .map_err(crate::DocbossError::new_err)?;
        Ok(PyBytes::new(py, &bytes))
    }

    /// Renders several pages (all by default) across the machine's cores,
    /// returned in the order asked for.
    #[pyo3(signature = (pages=None, *, scale=1.0, format="png", jpeg_quality=90))]
    fn render_pages<'py>(
        &self,
        py: Python<'py>,
        pages: Option<Vec<isize>>,
        scale: f32,
        format: &str,
        jpeg_quality: u8,
    ) -> PyResult<Vec<Bound<'py, PyBytes>>> {
        check_scale(scale)?;
        let format = image_format(format, jpeg_quality)?;
        let layout = self.layout(py);
        let indexes: Vec<usize> = match pages {
            None => (0..layout.pages.len()).collect(),
            Some(pages) => pages
                .into_iter()
                .map(|page| self.page_index(py, page).map(|(_, index)| index))
                .collect::<PyResult<_>>()?,
        };
        let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
        let results = py.allow_threads(|| {
            parallel_map(&indexes, cores, |&index| {
                encode_page(&mut Renderer::new(), &layout, index, scale, format)
            })
        });
        results
            .into_iter()
            .map(|slot| match slot {
                Ok(bytes) => Ok(PyBytes::new(py, &bytes)),
                Err(why) => Err(crate::DocbossError::new_err(why)),
            })
            .collect()
    }

    /// The document written as DOCX bytes: a DOC converts to DOCX this way.
    fn to_docx<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let document = self.document.clone();
        let bytes = py
            .allow_threads(|| docboss_write::to_bytes(&document))
            .map_err(doc_err)?;
        Ok(PyBytes::new(py, &bytes))
    }

    /// Writes the document as a DOCX file.
    fn save_docx(&self, py: Python<'_>, path: PathBuf) -> PyResult<()> {
        let document = self.document.clone();
        py.allow_threads(|| docboss_write::save(&document, &path))
            .map_err(doc_err)
    }

    fn __repr__(&self) -> String {
        let title = self.document.metadata.title.as_deref().unwrap_or("");
        format!(
            "<docboss.Document format={} title={title:?} sections={}>",
            self.format(),
            self.document.sections.len()
        )
    }
}

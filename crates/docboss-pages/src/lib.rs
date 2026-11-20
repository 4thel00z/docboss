//! A document's laid-out pages and the formats a document or a selection
//! of pages converts to.
//!
//! Select what to read, then convert it:
//!
//! ```no_run
//! use docboss_output::MarkdownOptions;
//! use docboss_pages::{Document, Format};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let model = docboss_core::open("report.docx")?;
//! let document = Document::new(model);
//! let markdown = Format::Markdown(MarkdownOptions::default());
//! let whole = document.to(&markdown)?;
//! let per_page = document.pages().to(&markdown)?;
//! let third = document.page(2).map(|page| page.to(&markdown)).transpose()?;
//! # Ok(())
//! # }
//! ```
//!
//! Each top-level body block belongs to the page it starts on, so the text
//! formats of every page together hold the document's body once.

#![deny(clippy::indexing_slicing)]

mod format;

use std::ops::Range;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use docboss_font::FontDatabase;
use docboss_layout::Layout;
use docboss_model::Block;
use docboss_output::Part;

pub use format::Format;

/// Why a conversion failed.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid format: {0}")]
    Format(String),
    #[error("{0} renders one page; convert a page or pages instead")]
    WholeDocument(&'static str),
    #[error("render: {0}")]
    Render(#[from] docboss_render::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

/// A converted document or page: a string for the text formats, encoded
/// bytes for the image formats.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Output {
    Text(String),
    Bytes(Vec<u8>),
}

impl Output {
    /// The text of a text format.
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Output::Text(text) => Some(text),
            Output::Bytes(_) => None,
        }
    }

    /// The bytes of an image format.
    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            Output::Bytes(bytes) => Some(bytes),
            Output::Text(_) => None,
        }
    }
}

/// The system font database, scanned once and shared by every document
/// that embeds no fonts of its own.
fn system_fonts() -> Arc<FontDatabase> {
    static SYSTEM: OnceLock<Arc<FontDatabase>> = OnceLock::new();
    SYSTEM
        .get_or_init(|| Arc::new(FontDatabase::system()))
        .clone()
}

/// The fonts a document lays out with: the shared system fonts, plus its
/// own when it embeds any.
pub fn fonts(document: &docboss_model::Document) -> Arc<FontDatabase> {
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

struct Inner {
    model: Arc<docboss_model::Document>,
    layout: OnceLock<Arc<Layout>>,
    /// The last text format converted page by page, with every page's
    /// output, so that converting pages one at a time walks the body once.
    texts: Mutex<Option<(Format, Arc<[String]>)>>,
}

/// A document model with its layout, computed on first use and cached.
/// Cloning is cheap and shares the layout.
#[derive(Clone)]
pub struct Document {
    inner: Arc<Inner>,
}

impl From<docboss_model::Document> for Document {
    fn from(model: docboss_model::Document) -> Self {
        Document::new(model)
    }
}

impl From<Arc<docboss_model::Document>> for Document {
    fn from(model: Arc<docboss_model::Document>) -> Self {
        Document::new(model)
    }
}

impl Document {
    pub fn new(model: impl Into<Arc<docboss_model::Document>>) -> Self {
        Self {
            inner: Arc::new(Inner {
                model: model.into(),
                layout: OnceLock::new(),
                texts: Mutex::new(None),
            }),
        }
    }

    /// The document model.
    pub fn model(&self) -> &Arc<docboss_model::Document> {
        &self.inner.model
    }

    /// The layout, computed on first call.
    pub fn layout(&self) -> Arc<Layout> {
        self.inner
            .layout
            .get_or_init(|| {
                let model = &self.inner.model;
                Arc::new(docboss_layout::layout(model, &fonts(model)))
            })
            .clone()
    }

    /// The whole document in a text format. Image formats render pages
    /// and are refused here.
    pub fn to(&self, format: &Format) -> Result<Output> {
        format.check().map_err(Error::Format)?;
        let model = &self.inner.model;
        let text = match format {
            Format::Text(options) => docboss_output::to_text(model, options),
            Format::Markdown(options) => docboss_output::to_markdown(model, options),
            Format::Html(options) => docboss_output::to_html(model, options),
            Format::Json { pretty } => docboss_output::to_json(model, *pretty)?,
            _ => return Err(Error::WholeDocument(format.name())),
        };
        Ok(Output::Text(text))
    }

    /// Every laid-out page.
    pub fn pages(&self) -> Pages {
        let count = self.layout().pages.len();
        Pages {
            document: self.clone(),
            indices: (0..count).collect(),
        }
    }

    /// The page at `index`, counting from 0.
    pub fn page(&self, index: usize) -> Option<Page> {
        let layout = self.layout();
        layout.pages.get(index)?;
        Some(Page {
            document: self.clone(),
            index,
        })
    }

    /// The text format of every page, from the cache when the format is
    /// the one converted last.
    fn page_texts(&self, format: &Format) -> Result<Arc<[String]>> {
        if let Some((cached, texts)) = self.lock_texts().as_ref() {
            if cached == format {
                return Ok(texts.clone());
            }
        }
        let layout = self.layout();
        let model = &self.inner.model;
        let parts: Vec<Part> = layout
            .pages
            .iter()
            .map(|page| Part {
                blocks: page.blocks.clone(),
                header: page.header.clone(),
                footer: page.footer.clone(),
            })
            .collect();
        let texts: Vec<String> = match format {
            Format::Text(options) => docboss_output::to_text_parts(model, options, &parts),
            Format::Markdown(options) => docboss_output::to_markdown_parts(model, options, &parts),
            Format::Html(options) => docboss_output::to_html_parts(model, options, &parts),
            Format::Json { pretty } => page_json(model, &parts, *pretty)?,
            _ => {
                return Err(Error::Format(format!(
                    "{} is not a text format",
                    format.name()
                )))
            }
        };
        let texts: Arc<[String]> = texts
            .into_iter()
            .zip(&parts)
            .map(|(text, part)| match part.blocks.is_empty() {
                true => String::new(),
                false => text,
            })
            .collect();
        *self.lock_texts() = Some((format.clone(), texts.clone()));
        Ok(texts)
    }

    fn lock_texts(&self) -> std::sync::MutexGuard<'_, Option<(Format, Arc<[String]>)>> {
        self.inner
            .texts
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Converts the pages at `indices`, in that order.
    fn convert(&self, indices: &[usize], format: &Format) -> Result<Vec<Output>> {
        format.check().map_err(Error::Format)?;
        let Some((scale, encoder)) = format.image() else {
            let texts = self.page_texts(format)?;
            return Ok(indices
                .iter()
                .map(|&index| Output::Text(texts.get(index).cloned().unwrap_or_default()))
                .collect());
        };
        let layout = self.layout();
        let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
        parallel_map(indices, cores, |&index| {
            let pixmap = docboss_render::render_page(&layout, index, scale)?;
            Ok(Output::Bytes(pixmap.encode(encoder)?))
        })
        .into_iter()
        .collect()
    }
}

/// The JSON of the body blocks that start on each page, as an array.
fn page_json(model: &docboss_model::Document, parts: &[Part], pretty: bool) -> Result<Vec<String>> {
    let body: Vec<&Block> = model
        .sections
        .iter()
        .flat_map(|section| section.blocks.iter())
        .collect();
    parts
        .iter()
        .map(|part| {
            let blocks = body.get(part.blocks.clone()).unwrap_or_default();
            let json = match pretty {
                true => serde_json::to_string_pretty(blocks)?,
                false => serde_json::to_string(blocks)?,
            };
            Ok(json)
        })
        .collect()
}

/// Applies `f` to every item on `workers` threads, results in input order.
fn parallel_map<T: Sync, R: Send>(
    items: &[T],
    workers: usize,
    f: impl Fn(&T) -> R + Sync,
) -> Vec<R> {
    let next = AtomicUsize::new(0);
    let slots: Mutex<Vec<Option<R>>> = Mutex::new((0..items.len()).map(|_| None).collect());
    std::thread::scope(|scope| {
        for _ in 0..workers.clamp(1, items.len().max(1)) {
            scope.spawn(|| loop {
                let index = next.fetch_add(1, Ordering::Relaxed);
                let Some(item) = items.get(index) else {
                    return;
                };
                let result = f(item);
                let mut slots = slots.lock().unwrap_or_else(|e| e.into_inner());
                if let Some(slot) = slots.get_mut(index) {
                    *slot = Some(result);
                }
            });
        }
    });
    let slots = slots.into_inner().unwrap_or_else(|e| e.into_inner());
    slots.into_iter().flatten().collect()
}

/// A selection of a document's pages, in the order selected.
#[derive(Clone)]
pub struct Pages {
    document: Document,
    indices: Vec<usize>,
}

impl Pages {
    pub fn len(&self) -> usize {
        self.indices.len()
    }

    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    /// The page indices selected, counting from 0 in the document.
    pub fn indices(&self) -> &[usize] {
        &self.indices
    }

    /// The page at `position` in the selection.
    pub fn get(&self, position: usize) -> Option<Page> {
        let index = *self.indices.get(position)?;
        Some(Page {
            document: self.document.clone(),
            index,
        })
    }

    pub fn iter(&self) -> impl Iterator<Item = Page> + '_ {
        self.indices.iter().map(|&index| Page {
            document: self.document.clone(),
            index,
        })
    }

    /// The pages at `positions` in this selection, in that order; positions
    /// past the end are left out.
    pub fn pick(&self, positions: impl IntoIterator<Item = usize>) -> Pages {
        Pages {
            document: self.document.clone(),
            indices: positions
                .into_iter()
                .filter_map(|position| self.indices.get(position).copied())
                .collect(),
        }
    }

    /// Converts every selected page, one output per page in selection
    /// order. Text formats walk the body once for all pages; image formats
    /// render the pages across the machine's cores.
    pub fn to(&self, format: &Format) -> Result<Vec<Output>> {
        self.document.convert(&self.indices, format)
    }
}

/// One laid-out page.
#[derive(Clone)]
pub struct Page {
    document: Document,
    index: usize,
}

impl Page {
    /// The page's position in the document, counting from 0.
    pub fn index(&self) -> usize {
        self.index
    }

    fn layout_page<T>(&self, read: impl FnOnce(&docboss_layout::Page) -> T) -> Option<T> {
        self.document.layout().pages.get(self.index).map(read)
    }

    /// The number `PAGE` fields show on the page.
    pub fn number(&self) -> u32 {
        self.layout_page(|page| page.number).unwrap_or_default()
    }

    /// The top-level body blocks that start on the page, as indices into
    /// the blocks of every section in order.
    pub fn blocks(&self) -> Range<usize> {
        self.layout_page(|page| page.blocks.clone())
            .unwrap_or_default()
    }

    /// Converts the page. A text format holds the blocks that start on the
    /// page and the notes first referenced in them; a page where no block
    /// starts gives an empty string in every text format, JSON included.
    pub fn to(&self, format: &Format) -> Result<Output> {
        let outputs = self.document.convert(&[self.index], format)?;
        Ok(outputs
            .into_iter()
            .next()
            .unwrap_or(Output::Text(String::new())))
    }
}

use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::PyBytes;

use docboss_model::{Severity, StyleKind};
use docboss_output::{BlockKind, BlockView};

fn py_repr(value: &Option<String>) -> String {
    value
        .as_ref()
        .map_or_else(|| "None".to_string(), |text| format!("{text:?}"))
}

/// Core and extended document properties.
#[pyclass(frozen, get_all, module = "docboss._docboss")]
#[derive(Clone)]
pub struct Metadata {
    pub title: Option<String>,
    pub subject: Option<String>,
    pub creator: Option<String>,
    pub keywords: Option<String>,
    pub description: Option<String>,
    pub last_modified_by: Option<String>,
    pub revision: Option<String>,
    pub created: Option<String>,
    pub modified: Option<String>,
    pub category: Option<String>,
    pub application: Option<String>,
    pub company: Option<String>,
    pub pages: Option<u32>,
    pub words: Option<u32>,
    pub characters: Option<u32>,
}

impl From<&docboss_model::Metadata> for Metadata {
    fn from(m: &docboss_model::Metadata) -> Self {
        Self {
            title: m.title.clone(),
            subject: m.subject.clone(),
            creator: m.creator.clone(),
            keywords: m.keywords.clone(),
            description: m.description.clone(),
            last_modified_by: m.last_modified_by.clone(),
            revision: m.revision.clone(),
            created: m.created.clone(),
            modified: m.modified.clone(),
            category: m.category.clone(),
            application: m.application.clone(),
            company: m.company.clone(),
            pages: m.pages,
            words: m.words,
            characters: m.characters,
        }
    }
}

#[pymethods]
impl Metadata {
    fn __repr__(&self) -> String {
        format!(
            "Metadata(title={}, creator={})",
            py_repr(&self.title),
            py_repr(&self.creator)
        )
    }
}

/// Something the lenient reader approximated or dropped.
#[pyclass(frozen, get_all, module = "docboss._docboss")]
#[derive(Clone)]
pub struct Diagnostic {
    /// `approximated` or `dropped`.
    pub severity: &'static str,
    pub location: String,
    pub message: String,
}

impl From<&docboss_model::Diagnostic> for Diagnostic {
    fn from(d: &docboss_model::Diagnostic) -> Self {
        let severity = match d.severity {
            Severity::Approximated => "approximated",
            Severity::Dropped => "dropped",
        };
        Self {
            severity,
            location: d.location.clone(),
            message: d.message.clone(),
        }
    }
}

#[pymethods]
impl Diagnostic {
    fn __repr__(&self) -> String {
        format!(
            "Diagnostic({}, {:?}, {:?})",
            self.severity, self.location, self.message
        )
    }
}

/// A font the document declares.
#[pyclass(frozen, get_all, module = "docboss._docboss")]
#[derive(Clone)]
pub struct FontInfo {
    pub name: String,
    pub alt_name: Option<String>,
    pub family: Option<String>,
    pub pitch: Option<String>,
    /// Whether the document embeds at least one face of the font.
    pub embedded: bool,
}

impl From<&docboss_model::FontEntry> for FontInfo {
    fn from(f: &docboss_model::FontEntry) -> Self {
        let embedded = [
            &f.embedded_regular,
            &f.embedded_bold,
            &f.embedded_italic,
            &f.embedded_bold_italic,
        ]
        .iter()
        .any(|face| face.is_some());
        Self {
            name: f.name.clone(),
            alt_name: f.alt_name.clone(),
            family: f.family.clone(),
            pitch: f.pitch.clone(),
            embedded,
        }
    }
}

#[pymethods]
impl FontInfo {
    fn __repr__(&self) -> String {
        format!("FontInfo({:?}, embedded={})", self.name, self.embedded)
    }
}

/// A style of the document's style sheet.
#[pyclass(frozen, get_all, module = "docboss._docboss")]
#[derive(Clone)]
pub struct StyleInfo {
    pub id: String,
    pub name: Option<String>,
    /// `paragraph`, `character`, `table` or `numbering`.
    pub kind: &'static str,
    pub based_on: Option<String>,
    pub is_default: bool,
}

impl From<&docboss_model::Style> for StyleInfo {
    fn from(s: &docboss_model::Style) -> Self {
        let kind = match s.kind {
            StyleKind::Paragraph => "paragraph",
            StyleKind::Character => "character",
            StyleKind::Table => "table",
            StyleKind::Numbering => "numbering",
        };
        Self {
            id: s.id.clone(),
            name: s.name.clone(),
            kind,
            based_on: s.based_on.clone(),
            is_default: s.is_default,
        }
    }
}

#[pymethods]
impl StyleInfo {
    fn __repr__(&self) -> String {
        format!("StyleInfo({:?}, kind={})", self.id, self.kind)
    }
}

/// One block of the main text with its role and plain text.
#[pyclass(frozen, get_all, module = "docboss._docboss")]
#[derive(Clone)]
pub struct Block {
    /// `paragraph`, `heading`, `list_item` or `table`.
    pub kind: &'static str,
    pub section: usize,
    pub style: Option<String>,
    pub heading_level: Option<u8>,
    pub list_level: Option<u8>,
    pub list_label: Option<String>,
    pub text: String,
}

impl From<BlockView> for Block {
    fn from(b: BlockView) -> Self {
        let kind = match b.kind {
            BlockKind::Paragraph => "paragraph",
            BlockKind::Heading => "heading",
            BlockKind::ListItem => "list_item",
            BlockKind::Table => "table",
        };
        Self {
            kind,
            section: b.section,
            style: b.style,
            heading_level: b.heading_level,
            list_level: b.list_level,
            list_label: b.list_label,
            text: b.text,
        }
    }
}

#[pymethods]
impl Block {
    fn __repr__(&self) -> String {
        let preview: String = self.text.chars().take(40).collect();
        format!("Block({}, {:?})", self.kind, preview)
    }
}

/// An image or other binary part the text refers to.
#[pyclass(frozen, module = "docboss._docboss")]
pub struct Image {
    #[pyo3(get)]
    pub name: String,
    /// The file name to export the image under, as Markdown and HTML refer
    /// to it.
    #[pyo3(get)]
    pub file_name: String,
    #[pyo3(get)]
    pub content_type: String,
    pub data: Arc<[u8]>,
}

impl From<&docboss_model::Media> for Image {
    fn from(m: &docboss_model::Media) -> Self {
        Self {
            name: m.name.clone(),
            file_name: docboss_output::media_file_name(&m.name).to_string(),
            content_type: m.content_type.clone(),
            data: m.data.clone(),
        }
    }
}

#[pymethods]
impl Image {
    #[getter]
    fn data<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.data)
    }

    fn __repr__(&self) -> String {
        format!(
            "Image({:?}, {}, {} bytes)",
            self.name,
            self.content_type,
            self.data.len()
        )
    }
}

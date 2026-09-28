use crate::{
    Block, Diagnostic, FontEntry, HeaderFooter, Media, MediaId, Numbering, Section, Styles,
};

/// The file format a document was read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum SourceFormat {
    /// Office Open XML WordprocessingML (ECMA-376), `.docx` and `.docm`.
    #[default]
    Docx,
    /// Word 97-2003 binary ([MS-DOC]), `.doc`.
    Doc,
}

/// Core and extended document properties. Dates are kept as the file
/// states them, normally ISO 8601 (`2024-05-01T10:00:00Z`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
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

/// Document-wide settings that affect layout.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Settings {
    /// Distance between automatic tab stops, in twips.
    pub default_tab_stop: i32,
    /// Whether even pages use the even header and footer.
    pub even_and_odd_headers: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            default_tab_stop: 720,
            even_and_odd_headers: false,
        }
    }
}

/// Whether a note is a footnote or an endnote.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum NoteKind {
    Footnote,
    Endnote,
}

/// A footnote or endnote body, referenced from a run by `id`.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Note {
    pub id: i64,
    pub kind: NoteKind,
    pub blocks: Vec<Block>,
}

/// A comment (annotation) anchored to a range of the main text.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Comment {
    pub id: i64,
    pub author: Option<String>,
    pub initials: Option<String>,
    pub date: Option<String>,
    pub blocks: Vec<Block>,
}

/// A parsed word-processing document.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Document {
    pub format: SourceFormat,
    pub metadata: Metadata,
    pub settings: Settings,
    pub styles: Styles,
    pub numbering: Numbering,
    /// The main text, one entry per section in document order.
    pub sections: Vec<Section>,
    /// Header and footer bodies, referenced by id from section properties.
    pub headers_footers: Vec<HeaderFooter>,
    pub footnotes: Vec<Note>,
    pub endnotes: Vec<Note>,
    pub comments: Vec<Comment>,
    /// Images and other binary parts referenced by drawings.
    pub media: Vec<Media>,
    pub fonts: Vec<FontEntry>,
    pub diagnostics: Vec<Diagnostic>,
}

impl Document {
    /// Every block of the main text, section after section.
    pub fn blocks(&self) -> impl Iterator<Item = &Block> {
        self.sections
            .iter()
            .flat_map(|section| section.blocks.iter())
    }

    pub fn media(&self, id: MediaId) -> Option<&Media> {
        self.media.get(id.0 as usize)
    }

    pub fn header_footer(&self, id: &str) -> Option<&HeaderFooter> {
        self.headers_footers.iter().find(|part| part.id == id)
    }

    pub fn footnote(&self, id: i64) -> Option<&Note> {
        self.footnotes.iter().find(|note| note.id == id)
    }

    pub fn endnote(&self, id: i64) -> Option<&Note> {
        self.endnotes.iter().find(|note| note.id == id)
    }

    pub fn comment(&self, id: i64) -> Option<&Comment> {
        self.comments.iter().find(|comment| comment.id == id)
    }
}

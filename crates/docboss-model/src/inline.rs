use crate::{MediaId, RunProperties};

/// The kind of break a run carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum Break {
    Line,
    Page,
    Column,
}

/// Where a drawing sits relative to the text.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(tag = "type", rename_all = "snake_case"))]
pub enum DrawingPlacement {
    /// In line with the text, like a large character.
    Inline,
    /// Floating, positioned by offsets in EMU from the paragraph or page.
    Anchored {
        x: i64,
        y: i64,
        behind_text: bool,
        relative_to_page: bool,
    },
}

/// A picture placed in the text.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Drawing {
    /// The image, when the drawing refers to one the reader could load.
    pub media: Option<MediaId>,
    /// Displayed width in EMU.
    pub width: i64,
    /// Displayed height in EMU.
    pub height: i64,
    pub placement: DrawingPlacement,
    pub name: Option<String>,
    pub description: Option<String>,
    /// The content of a text box the drawing carries, empty for a plain
    /// picture.
    pub text_box: Vec<crate::Block>,
    /// How the shape around a text box is drawn.
    pub shape: ShapeFormat,
}

/// Fill, outline and text frame of a shape. Every field is optional: an
/// unstated fill or outline is not drawn.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ShapeFormat {
    pub fill: Option<crate::Color>,
    pub outline: Option<crate::Color>,
    /// Outline width in EMU.
    pub outline_width: Option<i64>,
    /// Space between the shape edge and its text in EMU: left, top, right,
    /// bottom.
    pub insets: Option<[i64; 4]>,
    /// Where the text sits vertically: `Top`, `Center` or `Bottom`.
    pub text_anchor: Option<crate::VerticalAlign>,
    /// The shape grows to fit its text.
    pub auto_fit: bool,
}

/// One piece of run content.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(
    feature = "serde",
    serde(tag = "type", content = "value", rename_all = "snake_case")
)]
pub enum RunContent {
    Text(String),
    Tab,
    Break(Break),
    /// A carriage return inside a run, rendered as a line break.
    CarriageReturn,
    /// A non-breaking hyphen.
    NoBreakHyphen,
    /// An optional hyphen, shown only where the line breaks.
    SoftHyphen,
    /// A character from a symbol font: the font name and the character code.
    Symbol {
        font: Option<String>,
        char: u32,
    },
    FootnoteReference(i64),
    EndnoteReference(i64),
    CommentReference(i64),
    /// The automatic number of the note whose body contains this run.
    NoteNumber,
    Drawing(Drawing),
}

/// A run: content sharing one set of properties.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Run {
    pub properties: RunProperties,
    pub content: Vec<RunContent>,
}

/// A hyperlink around inline content: an external target, an internal
/// bookmark anchor, or both.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Hyperlink {
    pub target: Option<String>,
    pub anchor: Option<String>,
    pub tooltip: Option<String>,
    pub inlines: Vec<Inline>,
}

/// A field: its instruction text (such as `PAGE` or `HYPERLINK "url"`) and
/// the result the file cached for it.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Field {
    pub instruction: String,
    pub result: Vec<Inline>,
}

/// Whether tracked content was inserted or deleted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum RevisionKind {
    Insertion,
    Deletion,
}

/// A tracked change around inline content.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Revision {
    pub kind: RevisionKind,
    pub author: Option<String>,
    pub date: Option<String>,
    pub inlines: Vec<Inline>,
}

/// Inline content of a paragraph.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(tag = "type", rename_all = "snake_case"))]
pub enum Inline {
    Run(Run),
    Hyperlink(Hyperlink),
    Field(Field),
    Revision(Revision),
    BookmarkStart { id: i64, name: String },
    BookmarkEnd { id: i64 },
    CommentRangeStart(i64),
    CommentRangeEnd(i64),
}

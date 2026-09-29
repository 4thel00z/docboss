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
    /// Floating, positioned on each axis by an offset or an alignment.
    Anchored {
        horizontal: DrawingPosition,
        vertical: DrawingPosition,
        behind_text: bool,
    },
}

/// What a floating drawing's position on one axis is measured from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum PositionBase {
    Page,
    Margin,
    Column,
    Character,
    Paragraph,
    Line,
    LeftMargin,
    RightMargin,
    TopMargin,
    BottomMargin,
    /// The left margin on odd pages, the right on even pages; vertically
    /// the top margin.
    InsideMargin,
    /// The right margin on odd pages, the left on even pages; vertically
    /// the bottom margin.
    OutsideMargin,
}

/// Where a floating drawing sits within its base on one axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum PositionAlign {
    /// Left or top.
    Start,
    Center,
    /// Right or bottom.
    End,
    /// `Start` on odd pages, `End` on even pages.
    Inside,
    /// `End` on odd pages, `Start` on even pages.
    Outside,
}

/// A floating drawing's position on one axis: aligned within `base` when
/// `align` is set, else `offset` EMU from the start of `base`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct DrawingPosition {
    pub base: PositionBase,
    pub align: Option<PositionAlign>,
    pub offset: i64,
}

impl DrawingPosition {
    /// An offset in EMU from the start of `base`.
    pub fn offset(base: PositionBase, offset: i64) -> Self {
        Self {
            base,
            align: None,
            offset,
        }
    }
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

//! The document model shared by the docboss readers and consumers.
//!
//! Both the DOCX reader (ECMA-376 WordprocessingML) and the DOC reader
//! ([MS-DOC]) produce a [`Document`]. Text and Markdown output, layout and
//! rendering, and the DOCX writer read it. The model follows WordprocessingML
//! semantics: formatting is stored as it appears in the file (direct
//! properties plus a style reference) and [`Styles`] resolves the effective
//! values.
//!
//! Lengths are in twips (1/20 point) unless a field says otherwise; font
//! sizes are in half-points; drawing extents are in EMU (1/914400 inch).

mod block;
mod diagnostic;
mod document;
mod inline;
mod media;
mod numbering;
mod props;
mod section;
mod style;
mod text;

pub use block::{
    Block, Paragraph, Table, TableCell, TableCellProperties, TableProperties, TableRow,
    TableRowProperties, VerticalMerge,
};
pub use diagnostic::{Diagnostic, Severity};
pub use document::{Comment, Document, Metadata, Note, NoteKind, Settings, SourceFormat};
pub use inline::{
    Break, DashPattern, Drawing, DrawingPlacement, DrawingPosition, Field, Hyperlink, Inline,
    LineCap, LineJoin, PositionAlign, PositionBase, Revision, RevisionKind, Run, RunContent,
    ShapeFormat,
};
pub use media::{sniff_image, FontEntry, Media, MediaId};
pub use numbering::{
    AbstractNumbering, Level, NumberFormat, Numbering, NumberingCounter, NumberingInstance,
};
pub use props::{
    Border, BorderStyle, Borders, Color, FontSlots, Highlight, Indentation, Justification,
    LineRule, NumberingRef, ParagraphProperties, RunProperties, Shading, Spacing, TabAlignment,
    TabLeader, TabStop, Underline, VerticalAlign,
};
pub use section::{
    Columns, HeaderFooter, HeaderFooterKind, HeaderFooterRefs, Orientation, PageMargins, PageSize,
    Section, SectionBreak, SectionProperties,
};
pub use style::{Style, StyleKind, Styles};
pub use text::{number_label, plain_text};

use crate::{Borders, Inline, ParagraphProperties, RunProperties, Shading};

/// A block-level item of a story: the main text, a header or footer, a
/// note, a comment or a table cell. Paragraphs are stored inline because
/// nearly every block is one.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(tag = "type", rename_all = "snake_case"))]
pub enum Block {
    Paragraph(Paragraph),
    Table(Table),
}

/// A paragraph: direct properties, a paragraph style reference and its
/// inline content.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Paragraph {
    pub style_id: Option<String>,
    pub properties: ParagraphProperties,
    /// Direct run properties of the paragraph mark itself.
    pub mark: RunProperties,
    pub inlines: Vec<Inline>,
}

impl Paragraph {
    pub fn text(&self) -> String {
        let mut out = String::new();
        crate::text::inlines_text(&self.inlines, &mut out);
        out
    }
}

/// How a cell takes part in a vertically merged range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum VerticalMerge {
    /// The cell starts a merged range.
    Restart,
    /// The cell continues the range started above it.
    Continue,
}

#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct TableProperties {
    pub style_id: Option<String>,
    /// Preferred width in twips, when stated in twips.
    pub width: Option<i32>,
    /// Preferred width in fiftieths of a percent, when stated as a percentage.
    pub width_pct: Option<i32>,
    pub justification: Option<crate::Justification>,
    pub indent: Option<i32>,
    pub borders: Option<Borders>,
    pub shading: Option<Shading>,
    /// Default cell margins in twips: top, left, bottom, right.
    pub cell_margins: Option<[i32; 4]>,
    /// Fixed layout: column widths come from the grid, not the content.
    pub fixed_layout: bool,
    /// The cells run right to left (`w:bidiVisual`).
    pub bidi_visual: bool,
    /// The position of a floating table.
    pub floating: Option<TableFloat>,
}

/// A floating table's position and the distances it keeps from the text
/// around it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct TableFloat {
    /// The position on each axis, offsets in EMU.
    pub horizontal: crate::DrawingPosition,
    pub vertical: crate::DrawingPosition,
    /// The distances kept from the text above, below, left and right, in
    /// twips.
    pub distance: [i32; 4],
}

#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct TableRowProperties {
    /// Row height in twips.
    pub height: Option<i32>,
    /// Whether `height` is exact rather than a minimum.
    pub height_exact: bool,
    /// Whether the row repeats at the top of each page.
    pub header: bool,
    pub cant_split: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct TableCellProperties {
    /// Preferred width in twips.
    pub width: Option<i32>,
    /// Number of grid columns the cell spans; 1 when absent.
    pub grid_span: u32,
    pub vertical_merge: Option<VerticalMerge>,
    pub borders: Option<Borders>,
    pub shading: Option<Shading>,
    /// Vertical alignment of the cell content: `top`, `center` or `bottom`.
    pub vertical_align: Option<crate::VerticalAlign>,
    /// Cell margins in twips: top, left, bottom, right.
    pub margins: Option<[i32; 4]>,
    /// How the cell's text runs.
    pub text_direction: crate::TextDirection,
}

#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct TableCell {
    pub properties: TableCellProperties,
    pub blocks: Vec<Block>,
}

impl TableCell {
    pub fn span(&self) -> u32 {
        self.properties.grid_span.max(1)
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct TableRow {
    pub properties: TableRowProperties,
    pub cells: Vec<TableCell>,
}

#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Table {
    pub properties: TableProperties,
    /// Grid column widths in twips.
    pub grid: Vec<i32>,
    pub rows: Vec<TableRow>,
}

use crate::Block;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum Orientation {
    Portrait,
    Landscape,
}

/// Page size in twips.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct PageSize {
    pub width: i32,
    pub height: i32,
    pub orientation: Orientation,
}

impl Default for PageSize {
    fn default() -> Self {
        Self {
            width: 12240,
            height: 15840,
            orientation: Orientation::Portrait,
        }
    }
}

/// Page margins in twips. `header` and `footer` are the distances of the
/// header and footer from the page edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct PageMargins {
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
    pub left: i32,
    pub header: i32,
    pub footer: i32,
    pub gutter: i32,
}

impl Default for PageMargins {
    fn default() -> Self {
        Self {
            top: 1440,
            right: 1440,
            bottom: 1440,
            left: 1440,
            header: 720,
            footer: 720,
            gutter: 0,
        }
    }
}

/// Text columns of a section.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Columns {
    pub count: u32,
    /// Space between equal-width columns in twips.
    pub space: i32,
    /// Explicit column widths and spacing in twips, when not equal width.
    pub widths: Vec<(i32, i32)>,
    pub separator: bool,
}

impl Default for Columns {
    fn default() -> Self {
        Self {
            count: 1,
            space: 720,
            widths: Vec::new(),
            separator: false,
        }
    }
}

/// Where the section starts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum SectionBreak {
    #[default]
    NextPage,
    Continuous,
    EvenPage,
    OddPage,
    NextColumn,
}

/// Header or footer part ids for each page kind.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct HeaderFooterRefs {
    pub default: Option<String>,
    pub first: Option<String>,
    pub even: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct SectionProperties {
    pub page_size: PageSize,
    pub margins: PageMargins,
    pub columns: Columns,
    pub start: SectionBreak,
    pub headers: HeaderFooterRefs,
    pub footers: HeaderFooterRefs,
    /// Whether the first page uses the first-page header and footer.
    pub title_page: bool,
    /// The page number the section starts at, when restarted.
    pub page_number_start: Option<u32>,
    /// The format `PAGE` fields show the section's page numbers in, such as
    /// lower-case Roman for front matter; decimal when absent (ECMA-376
    /// Part 1 §17.6.12).
    pub page_number_format: Option<crate::NumberFormat>,
}

/// A section: its page setup and the blocks laid out with it.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Section {
    pub properties: SectionProperties,
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum HeaderFooterKind {
    Header,
    Footer,
}

/// A header or footer body.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct HeaderFooter {
    /// The relationship id (DOCX) or a generated id (DOC) that section
    /// properties refer to.
    pub id: String,
    pub kind: HeaderFooterKind,
    pub blocks: Vec<Block>,
}

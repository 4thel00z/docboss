//! Paragraph and run properties. Every field is optional so that a set of
//! properties can express "not stated here"; [`crate::Styles`] layers the
//! document defaults, styles and direct formatting into effective values.

/// An sRGB color, or `None` in a property for `auto`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Color(pub u8, pub u8, pub u8);

impl Color {
    pub const BLACK: Color = Color(0, 0, 0);
    pub const WHITE: Color = Color(255, 255, 255);

    /// Parses `RRGGBB` hex, as WordprocessingML writes colors.
    pub fn from_hex(hex: &str) -> Option<Color> {
        let hex = hex.trim();
        if hex.len() != 6 {
            return None;
        }
        let value = u32::from_str_radix(hex, 16).ok()?;
        Some(Color((value >> 16) as u8, (value >> 8) as u8, value as u8))
    }

    pub fn to_hex(self) -> String {
        format!("{:02X}{:02X}{:02X}", self.0, self.1, self.2)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum Justification {
    Left,
    Center,
    Right,
    Both,
    Distribute,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum LineRule {
    /// `line` is in 240ths of a line.
    Auto,
    /// `line` is the exact height in twips.
    Exact,
    /// `line` is the minimum height in twips.
    AtLeast,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Spacing {
    pub before: Option<i32>,
    pub after: Option<i32>,
    pub line: Option<i32>,
    pub line_rule: Option<LineRule>,
    pub before_auto: Option<bool>,
    pub after_auto: Option<bool>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Indentation {
    pub left: Option<i32>,
    pub right: Option<i32>,
    pub first_line: Option<i32>,
    pub hanging: Option<i32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum TabAlignment {
    Left,
    Center,
    Right,
    Decimal,
    Bar,
    /// Removes an inherited stop at this position.
    Clear,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum TabLeader {
    None,
    Dot,
    Hyphen,
    Underscore,
    MiddleDot,
    Heavy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct TabStop {
    pub position: i32,
    pub alignment: TabAlignment,
    pub leader: TabLeader,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum BorderStyle {
    None,
    Single,
    Thick,
    Double,
    Dotted,
    Dashed,
    DotDash,
    DotDotDash,
    /// Dashes with small gaps between them.
    DashSmallGap,
    /// Alternating groups of thin diagonal strokes (`dashDotStroked`),
    /// drawn dash-dotted as LibreOffice draws it.
    DashDotStroked,
    /// A page border drawn from an image of Word's art set, such as
    /// `mapleMuffins`.
    Art,
    /// Any other line style, drawn as single.
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Border {
    pub style: BorderStyle,
    /// Width in eighths of a point.
    pub size: u32,
    /// Space from the text in points.
    pub space: u32,
    pub color: Option<Color>,
    /// The border has a shadow.
    pub shadow: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Borders {
    pub top: Option<Border>,
    pub left: Option<Border>,
    pub bottom: Option<Border>,
    pub right: Option<Border>,
    /// Between rows of a table, or between paragraphs with equal borders.
    pub inside_horizontal: Option<Border>,
    pub inside_vertical: Option<Border>,
}

/// A background fill. Only the solid fill color is modeled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Shading {
    pub fill: Option<Color>,
}

/// A paragraph's place in a numbered or bulleted list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct NumberingRef {
    /// The numbering instance; 0 removes numbering.
    pub num_id: i64,
    /// The list level, 0 to 8.
    pub level: u8,
}

#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ParagraphProperties {
    pub justification: Option<Justification>,
    pub indentation: Indentation,
    pub spacing: Spacing,
    pub keep_next: Option<bool>,
    pub keep_lines: Option<bool>,
    pub page_break_before: Option<bool>,
    pub widow_control: Option<bool>,
    pub contextual_spacing: Option<bool>,
    pub numbering: Option<NumberingRef>,
    /// Outline level 0 to 8 for headings; 9 or absent for body text.
    pub outline_level: Option<u8>,
    pub tabs: Vec<TabStop>,
    pub borders: Option<Borders>,
    pub shading: Option<Shading>,
    pub bidi: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum Underline {
    None,
    Single,
    Double,
    Thick,
    Dotted,
    Dashed,
    Wave,
    Words,
    /// Any other underline style, drawn as single.
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum VerticalAlign {
    Baseline,
    Superscript,
    Subscript,
    Top,
    Center,
    Bottom,
}

/// How the lines of a text box or table cell run: across and stacked
/// downwards, or turned a quarter so they run down the box and stack
/// leftwards (`TopToBottom`) or run up the box and stack rightwards
/// (`BottomToTop`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum TextDirection {
    #[default]
    LeftToRight,
    TopToBottom,
    BottomToTop,
}

/// A highlight color from the fixed WordprocessingML palette, as RGB.
pub type Highlight = Color;

/// The four font slots of a run. A theme font reference such as
/// `minorHAnsi` is resolved to a face name by the reader when the theme is
/// available.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct FontSlots {
    pub ascii: Option<String>,
    pub high_ansi: Option<String>,
    pub east_asia: Option<String>,
    pub complex: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct RunProperties {
    /// A character style.
    pub style_id: Option<String>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    /// Bold for complex script text (`w:bCs`).
    pub bold_complex: Option<bool>,
    /// Italic for complex script text (`w:iCs`).
    pub italic_complex: Option<bool>,
    pub underline: Option<Underline>,
    pub strike: Option<bool>,
    pub double_strike: Option<bool>,
    pub caps: Option<bool>,
    pub small_caps: Option<bool>,
    /// Hidden text.
    pub vanish: Option<bool>,
    pub fonts: FontSlots,
    /// Font size in half-points.
    pub size: Option<u32>,
    /// Font size of complex script text in half-points (`w:szCs`).
    pub size_complex: Option<u32>,
    /// Text color; `Some(None)` states `auto`.
    pub color: Option<Option<Color>>,
    pub highlight: Option<Highlight>,
    pub shading: Option<Shading>,
    pub vertical_align: Option<VerticalAlign>,
    /// Extra spacing between characters in twips.
    pub spacing: Option<i32>,
    /// Baseline shift in half-points.
    pub position: Option<i32>,
    pub language: Option<String>,
    pub right_to_left: Option<bool>,
    /// Format every character as complex script text (`w:cs`).
    pub complex_script: Option<bool>,
}

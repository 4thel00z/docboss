//! Paragraph, run, table and section properties (ECMA-376 Part 1 §17.3.1,
//! §17.3.2, §17.4 and §17.6).

use docboss_model::{
    Border, BorderStyle, Borders, Color, Columns, FontSlots, Indentation, Justification, LineRule,
    NumberingRef, Orientation, PageBorderDisplay, PageBorderOffset, PageBorders, PageMargins,
    PageSize, ParagraphProperties, RunProperties, SectionBreak, SectionProperties, Shading,
    Spacing, TabAlignment, TabLeader, TabStop, TableCellProperties, TableProperties,
    TableRowProperties, TextDirection, Underline, VerticalAlign, VerticalMerge,
};
use docboss_xml::{Element, Ns, Reader};

use crate::xml::{
    attr, children, color, highlight, int_attr, on_off, on_off_value, twips, twips_attr, u32_attr,
    val,
};

/// The fonts of a theme's font scheme (ECMA-376 Part 1 §20.1.4.1.18): latin,
/// east Asian and complex script faces of the major and minor fonts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Theme {
    pub major: [Option<String>; 3],
    pub minor: [Option<String>; 3],
    /// The color scheme's slots (`dk1`, `lt1`, `accent1`, ...) and their
    /// colors (ECMA-376 Part 1 §20.1.6.2).
    pub colors: Vec<(String, Color)>,
}

impl Theme {
    /// The color of a scheme slot, with the text and background aliases
    /// (`tx1`, `bg1`, `tx2`, `bg2`) mapped to the dark and light slots as
    /// Word's default color mapping does (ECMA-376 Part 1 §17.15.1.20).
    pub fn color(&self, slot: &str) -> Option<Color> {
        let slot = match slot {
            "tx1" => "dk1",
            "bg1" => "lt1",
            "tx2" => "dk2",
            "bg2" => "lt2",
            other => other,
        };
        let found = self
            .colors
            .iter()
            .find(|(name, _)| name == slot)
            .map(|(_, color)| *color);
        found.or(match slot {
            "dk1" => Some(Color::BLACK),
            "lt1" => Some(Color::WHITE),
            _ => None,
        })
    }
    /// The face a theme font reference such as `minorHAnsi` names.
    pub fn font(&self, reference: &str) -> Option<String> {
        let (scheme, rest) = match reference.strip_prefix("major") {
            Some(rest) => (&self.major, rest),
            None => (&self.minor, reference.strip_prefix("minor")?),
        };
        let slot = match rest {
            "EastAsia" => 1,
            "Bidi" => 2,
            _ => 0,
        };
        scheme[slot].clone()
    }
}

/// The paragraph properties of a `w:pPr` (ECMA-376 Part 1 §17.3.1.26), with
/// the paragraph mark's run properties, the paragraph style and a section
/// break when the paragraph ends a section.
#[derive(Debug, Default)]
pub struct ParagraphFormat {
    pub style_id: Option<String>,
    pub properties: ParagraphProperties,
    pub mark: RunProperties,
    pub section: Option<SectionProperties>,
}

pub fn justification(value: &str) -> Option<Justification> {
    Some(match value {
        "left" | "start" => Justification::Left,
        "center" => Justification::Center,
        "right" | "end" => Justification::Right,
        "both" | "justify" | "lowKashida" | "mediumKashida" | "highKashida" | "thaiDistribute" => {
            Justification::Both
        }
        "distribute" => Justification::Distribute,
        _ => return None,
    })
}

pub fn paragraph_properties<'a>(reader: &mut Reader<'a>, theme: &Theme) -> ParagraphFormat {
    let mut format = ParagraphFormat::default();
    children(reader, |reader, e| {
        if e.ns != Ns::W {
            return;
        }
        let p = &mut format.properties;
        match e.local {
            "pStyle" => format.style_id = val(&e).map(Into::into),
            "jc" => p.justification = val(&e).and_then(|v| justification(&v)),
            "ind" => p.indentation = indentation(&e),
            "spacing" => p.spacing = spacing(&e),
            "keepNext" => p.keep_next = Some(on_off(&e)),
            "keepLines" => p.keep_lines = Some(on_off(&e)),
            "pageBreakBefore" => p.page_break_before = Some(on_off(&e)),
            "widowControl" => p.widow_control = Some(on_off(&e)),
            "contextualSpacing" => p.contextual_spacing = Some(on_off(&e)),
            "bidi" => p.bidi = Some(on_off(&e)),
            "outlineLvl" => p.outline_level = val(&e).and_then(|v| v.parse::<u8>().ok()),
            "numPr" => p.numbering = numbering_reference(reader),
            "tabs" => p.tabs = tabs(reader),
            "pBdr" => p.borders = Some(borders(reader)),
            "shd" => p.shading = shading(&e),
            "rPr" => format.mark = run_properties(reader, theme),
            "sectPr" => format.section = Some(section_properties(reader)),
            _ => {}
        }
    });
    format
}

fn indentation(e: &Element<'_>) -> Indentation {
    Indentation {
        left: twips_attr(e, "left").or_else(|| twips_attr(e, "start")),
        right: twips_attr(e, "right").or_else(|| twips_attr(e, "end")),
        first_line: twips_attr(e, "firstLine"),
        hanging: twips_attr(e, "hanging"),
    }
}

fn spacing(e: &Element<'_>) -> Spacing {
    let line_rule = attr(e, "lineRule").and_then(|rule| match rule.as_ref() {
        "auto" => Some(LineRule::Auto),
        "exact" => Some(LineRule::Exact),
        "atLeast" => Some(LineRule::AtLeast),
        _ => None,
    });
    let line = twips_attr(e, "line");
    Spacing {
        before: twips_attr(e, "before"),
        after: twips_attr(e, "after"),
        line,
        line_rule: line_rule.or(line.map(|_| LineRule::Auto)),
        before_auto: e
            .attr_raw(Ns::W, "beforeAutospacing")
            .map(|v| crate::xml::on_off_value(Some(v))),
        after_auto: e
            .attr_raw(Ns::W, "afterAutospacing")
            .map(|v| crate::xml::on_off_value(Some(v))),
    }
}

/// `w:numPr` (ECMA-376 Part 1 §17.3.1.19): the numbering instance and level
/// (ECMA-376 Part 1 §17.9.18, §17.9.3).
fn numbering_reference(reader: &mut Reader<'_>) -> Option<NumberingRef> {
    let mut num_id = None;
    let mut level = 0u8;
    children(reader, |_, e| match e.local {
        "numId" => num_id = int_attr(&e, "val"),
        "ilvl" => level = int_attr(&e, "val").unwrap_or(0).clamp(0, 8) as u8,
        _ => {}
    });
    num_id.map(|num_id| NumberingRef { num_id, level })
}

/// `w:tabs` (ECMA-376 Part 1 §17.3.1.38).
fn tabs(reader: &mut Reader<'_>) -> Vec<TabStop> {
    let mut stops = Vec::new();
    children(reader, |_, e| {
        if e.local != "tab" {
            return;
        }
        let Some(position) = twips_attr(&e, "pos") else {
            return;
        };
        let alignment = match val(&e).as_deref() {
            Some("center") => TabAlignment::Center,
            Some("right" | "end") => TabAlignment::Right,
            Some("decimal") => TabAlignment::Decimal,
            Some("bar") => TabAlignment::Bar,
            Some("clear") => TabAlignment::Clear,
            _ => TabAlignment::Left,
        };
        let leader = match attr(&e, "leader").as_deref() {
            Some("dot") => TabLeader::Dot,
            Some("hyphen") => TabLeader::Hyphen,
            Some("underscore") => TabLeader::Underscore,
            Some("middleDot") => TabLeader::MiddleDot,
            Some("heavy") => TabLeader::Heavy,
            _ => TabLeader::None,
        };
        stops.push(TabStop {
            position,
            alignment,
            leader,
        });
    });
    stops
}

/// One border (ECMA-376 Part 1 §17.3.4): style (§17.18.2), width, spacing,
/// color and shadow. A value outside the line styles names an art border.
fn border(e: &Element<'_>) -> Border {
    let style = match val(e).as_deref() {
        Some("nil" | "none") => BorderStyle::None,
        Some("single") => BorderStyle::Single,
        Some("thick") => BorderStyle::Thick,
        Some("double") => BorderStyle::Double,
        Some("dotted") => BorderStyle::Dotted,
        Some("dashed") => BorderStyle::Dashed,
        Some("dotDash") => BorderStyle::DotDash,
        Some("dotDotDash") => BorderStyle::DotDotDash,
        Some("dashSmallGap") => BorderStyle::DashSmallGap,
        Some("dashDotStroked") => BorderStyle::DashDotStroked,
        Some(v) if !LINE_STYLES.contains(&v) => BorderStyle::Art,
        _ => BorderStyle::Other,
    };
    Border {
        style,
        size: u32_attr(e, "sz").unwrap_or(4),
        space: u32_attr(e, "space").unwrap_or(0),
        color: attr(e, "color").and_then(|c| color(&c)).flatten(),
        shadow: attr(e, "shadow").is_some_and(|v| on_off_value(Some(v.as_ref()))),
    }
}

/// The ST_Border values that are line styles; the others are art borders.
const LINE_STYLES: [&str; 27] = [
    "nil",
    "none",
    "single",
    "thick",
    "double",
    "dotted",
    "dashed",
    "dotDash",
    "dotDotDash",
    "triple",
    "thinThickSmallGap",
    "thickThinSmallGap",
    "thinThickThinSmallGap",
    "thinThickMediumGap",
    "thickThinMediumGap",
    "thinThickThinMediumGap",
    "thinThickLargeGap",
    "thickThinLargeGap",
    "thinThickThinLargeGap",
    "wave",
    "doubleWave",
    "dashSmallGap",
    "dashDotStroked",
    "threeDEmboss",
    "threeDEngrave",
    "outset",
    "inset",
];

/// Paragraph (`w:pBdr`), table (`w:tblBorders`) and cell (`w:tcBorders`)
/// borders, each side and the inside edges.
/// ECMA-376 Part 1 §17.4.76, §17.4.4, §17.4.36, §17.4.13, §17.4.22, §17.4.24, §17.4.74, §17.4.3, §17.4.33, §17.4.12, §17.4.23, §17.4.25.
pub fn borders(reader: &mut Reader<'_>) -> Borders {
    let mut borders = Borders::default();
    children(reader, |_, e| {
        let side = Some(border(&e));
        match e.local {
            "top" => borders.top = side,
            "left" | "start" => borders.left = side,
            "bottom" => borders.bottom = side,
            "right" | "end" => borders.right = side,
            "insideH" | "between" => borders.inside_horizontal = side,
            "insideV" => borders.inside_vertical = side,
            _ => {}
        }
    });
    borders
}

/// `w:shd` (ECMA-376 Part 1 §17.3.5): the fill, or for a solid pattern the
/// pattern color.
pub fn shading(e: &Element<'_>) -> Option<Shading> {
    let fill = attr(e, "fill").and_then(|c| color(&c)).flatten();
    let solid = val(e).as_deref() == Some("solid");
    let pattern = attr(e, "color").and_then(|c| color(&c)).flatten();
    let fill = if solid { pattern.or(fill) } else { fill };
    Some(Shading { fill })
}

fn underline(value: &str) -> Underline {
    match value {
        "none" => Underline::None,
        "single" => Underline::Single,
        "double" => Underline::Double,
        "thick" => Underline::Thick,
        "dotted" | "dottedHeavy" => Underline::Dotted,
        "dash" | "dashedHeavy" | "dashLong" | "dashLongHeavy" => Underline::Dashed,
        "wave" | "wavyHeavy" | "wavyDouble" => Underline::Wave,
        "words" => Underline::Words,
        _ => Underline::Other,
    }
}

fn fonts(e: &Element<'_>, theme: &Theme) -> FontSlots {
    let slot = |face: &str, themed: &str| {
        attr(e, themed)
            .and_then(|reference| theme.font(&reference))
            .or_else(|| attr(e, face).map(Into::into))
    };
    FontSlots {
        ascii: slot("ascii", "asciiTheme"),
        high_ansi: slot("hAnsi", "hAnsiTheme"),
        east_asia: slot("eastAsia", "eastAsiaTheme"),
        complex: slot("cs", "cstheme"),
    }
}

/// `w:rPr` (ECMA-376 Part 1 §17.3.2.28).
/// ECMA-376 Part 1 §17.3.2.2, §17.3.2.17, §17.3.2.39, §17.3.2.7: complex script bold, italic, size and formatting.
pub fn run_properties<'a>(reader: &mut Reader<'a>, theme: &Theme) -> RunProperties {
    let mut p = RunProperties::default();
    children(reader, |_, e| {
        if e.ns != Ns::W {
            return;
        }
        match e.local {
            "rStyle" => p.style_id = val(&e).map(Into::into),
            "b" => p.bold = Some(on_off(&e)),
            "i" => p.italic = Some(on_off(&e)),
            "bCs" => p.bold_complex = Some(on_off(&e)),
            "iCs" => p.italic_complex = Some(on_off(&e)),
            "cs" => p.complex_script = Some(on_off(&e)),
            "u" => p.underline = Some(val(&e).map_or(Underline::Single, |v| underline(&v))),
            "strike" => p.strike = Some(on_off(&e)),
            "dstrike" => p.double_strike = Some(on_off(&e)),
            "caps" => p.caps = Some(on_off(&e)),
            "smallCaps" => p.small_caps = Some(on_off(&e)),
            "vanish" | "specVanish" => p.vanish = Some(on_off(&e)),
            "rFonts" => {
                let slots = fonts(&e, theme);
                let f = &mut p.fonts;
                f.ascii = slots.ascii.or(f.ascii.take());
                f.high_ansi = slots.high_ansi.or(f.high_ansi.take());
                f.east_asia = slots.east_asia.or(f.east_asia.take());
                f.complex = slots.complex.or(f.complex.take());
            }
            "sz" => p.size = u32_attr(&e, "val"),
            "szCs" => p.size_complex = u32_attr(&e, "val"),
            "color" => {
                let themed = attr(&e, "themeColor").is_some();
                p.color = val(&e).and_then(|c| color(&c)).or(themed.then_some(None));
            }
            "highlight" => p.highlight = val(&e).and_then(|name| highlight(&name)),
            "shd" => p.shading = shading(&e),
            "vertAlign" => {
                p.vertical_align = match val(&e).as_deref() {
                    Some("superscript") => Some(VerticalAlign::Superscript),
                    Some("subscript") => Some(VerticalAlign::Subscript),
                    Some("baseline") => Some(VerticalAlign::Baseline),
                    _ => None,
                }
            }
            "spacing" => p.spacing = twips_attr(&e, "val"),
            "position" => p.position = int_attr(&e, "val").map(|n| n.clamp(-10_000, 10_000) as i32),
            "lang" => p.language = val(&e).map(Into::into),
            "rtl" => p.right_to_left = Some(on_off(&e)),
            _ => {}
        }
    });
    p
}

fn width(e: &Element<'_>) -> (Option<i32>, Option<i32>) {
    let raw = e.attr_raw(Ns::W, "w").unwrap_or("");
    let kind = e.attr_raw(Ns::W, "type").unwrap_or("dxa");
    if let Some(percent) = raw.strip_suffix('%') {
        return (
            None,
            percent
                .trim()
                .parse::<f64>()
                .ok()
                .map(|p| (p * 50.0).round() as i32),
        );
    }
    match kind {
        "pct" => (None, crate::xml::int(raw).map(|n| n.clamp(0, 5000) as i32)),
        "dxa" => (twips(raw), None),
        _ => (None, None),
    }
}

/// Cell margins: top, leading, bottom and trailing.
/// ECMA-376 Part 1 §17.4.75, §17.4.34, §17.4.5, §17.4.11.
fn margins(reader: &mut Reader<'_>) -> [i32; 4] {
    let mut out = [0, 108, 0, 108];
    children(reader, |_, e| {
        let (value, _) = width(&e);
        let Some(value) = value else {
            return;
        };
        match e.local {
            "top" => out[0] = value,
            "left" | "start" => out[1] = value,
            "bottom" => out[2] = value,
            "right" | "end" => out[3] = value,
            _ => {}
        }
    });
    out
}

/// `w:tblPr` (ECMA-376 Part 1 §17.4.59): style, width, alignment, indent,
/// borders, shading, default cell margins, layout and direction.
/// ECMA-376 Part 1 §17.4.62, §17.4.63, §17.4.28, §17.4.50, §17.4.38, §17.4.31, §17.4.42, §17.4.52, §17.4.1.
pub fn table_properties(reader: &mut Reader<'_>) -> TableProperties {
    let mut p = TableProperties::default();
    children(reader, |reader, e| match e.local {
        "tblStyle" => p.style_id = val(&e).map(Into::into),
        "tblW" => (p.width, p.width_pct) = width(&e),
        "jc" => p.justification = val(&e).and_then(|v| justification(&v)),
        "tblInd" => p.indent = width(&e).0,
        "tblBorders" => p.borders = Some(borders(reader)),
        "shd" => p.shading = shading(&e),
        "tblCellMar" => p.cell_margins = Some(margins(reader)),
        "tblLayout" => p.fixed_layout = attr(&e, "type").as_deref() == Some("fixed"),
        "bidiVisual" => p.bidi_visual = on_off(&e),
        _ => {}
    });
    p
}

/// `w:trPr` (ECMA-376 Part 1 §17.4.81): height, repeated header rows and
/// rows that cannot split (ECMA-376 Part 1 §17.4.80, §17.4.49, §17.4.6).
pub fn row_properties(reader: &mut Reader<'_>) -> TableRowProperties {
    let mut p = TableRowProperties::default();
    children(reader, |_, e| match e.local {
        "trHeight" => {
            p.height = twips_attr(&e, "val");
            p.height_exact = attr(&e, "hRule").as_deref() == Some("exact");
        }
        "tblHeader" => p.header = on_off(&e),
        "cantSplit" => p.cant_split = on_off(&e),
        _ => {}
    });
    p
}

/// `w:tcPr` (ECMA-376 Part 1 §17.4.69): width, grid span, vertical merge,
/// borders, shading, vertical alignment and margins.
/// ECMA-376 Part 1 §17.4.71, §17.4.17, §17.4.84, §17.4.66, §17.4.32, §17.4.83, §17.4.68.
pub fn cell_properties(reader: &mut Reader<'_>) -> TableCellProperties {
    let mut p = TableCellProperties {
        grid_span: 1,
        ..TableCellProperties::default()
    };
    children(reader, |reader, e| match e.local {
        "tcW" => p.width = width(&e).0,
        "gridSpan" => p.grid_span = u32_attr(&e, "val").unwrap_or(1).clamp(1, 64),
        "vMerge" => {
            p.vertical_merge = match val(&e).as_deref() {
                Some("restart") => Some(VerticalMerge::Restart),
                _ => Some(VerticalMerge::Continue),
            }
        }
        "tcBorders" => p.borders = Some(borders(reader)),
        "shd" => p.shading = shading(&e),
        "vAlign" => {
            p.vertical_align = match val(&e).as_deref() {
                Some("center") => Some(VerticalAlign::Center),
                Some("bottom") => Some(VerticalAlign::Bottom),
                Some("top") => Some(VerticalAlign::Top),
                _ => None,
            }
        }
        "tcMar" => p.margins = Some(margins(reader)),
        "textDirection" => p.text_direction = text_direction(val(&e).as_deref()),
        _ => {}
    });
    p
}

/// ECMA-376 Part 1 §17.4.72, §17.18.93: a cell's `w:textDirection`, in its
/// transitional and strict names. Lines that stack left to right while
/// their text runs down (`tbLrV`, `lrV`) are read as running across.
pub fn text_direction(value: Option<&str>) -> TextDirection {
    match value {
        Some("tbRl" | "tbRlV" | "rl" | "rlV") => TextDirection::TopToBottom,
        Some("btLr" | "lr") => TextDirection::BottomToTop,
        _ => TextDirection::LeftToRight,
    }
}

/// `w:sectPr` (ECMA-376 Part 1 §17.6.18): page size and margins, columns,
/// section type, page numbering, the first-page switch and the header and
/// footer references.
/// ECMA-376 Part 1 §17.6.13, §17.6.11, §17.6.4, §17.6.3, §17.6.22, §17.6.12, §17.10.6, §17.10.5, §17.10.2.
/// Page borders: ECMA-376 Part 1 §17.6.10, §17.6.21, §17.6.7, §17.6.2, §17.6.15, §17.18.62, §17.18.63, §17.18.64.
pub fn section_properties(reader: &mut Reader<'_>) -> SectionProperties {
    let mut p = SectionProperties::default();
    children(reader, |reader, e| {
        if e.ns != Ns::W {
            return;
        }
        match e.local {
            "pgSz" => {
                let orientation = match attr(&e, "orient").as_deref() {
                    Some("landscape") => Orientation::Landscape,
                    _ => Orientation::Portrait,
                };
                p.page_size = PageSize {
                    width: twips_attr(&e, "w").filter(|&w| w > 0).unwrap_or(12240),
                    height: twips_attr(&e, "h").filter(|&h| h > 0).unwrap_or(15840),
                    orientation,
                };
            }
            "pgMar" => {
                let d = PageMargins::default();
                p.margins = PageMargins {
                    top: twips_attr(&e, "top").unwrap_or(d.top),
                    right: twips_attr(&e, "right").unwrap_or(d.right),
                    bottom: twips_attr(&e, "bottom").unwrap_or(d.bottom),
                    left: twips_attr(&e, "left").unwrap_or(d.left),
                    header: twips_attr(&e, "header").unwrap_or(d.header),
                    footer: twips_attr(&e, "footer").unwrap_or(d.footer),
                    gutter: twips_attr(&e, "gutter").unwrap_or(d.gutter),
                };
            }
            "cols" => p.columns = columns(reader, &e),
            "pgBorders" => {
                p.page_borders = Some(PageBorders {
                    offset_from: match attr(&e, "offsetFrom").as_deref() {
                        Some("page") => PageBorderOffset::Page,
                        _ => PageBorderOffset::Text,
                    },
                    display: match attr(&e, "display").as_deref() {
                        Some("firstPage") => PageBorderDisplay::FirstPage,
                        Some("notFirstPage") => PageBorderDisplay::NotFirstPage,
                        _ => PageBorderDisplay::AllPages,
                    },
                    behind_text: attr(&e, "zOrder").as_deref() == Some("back"),
                    sides: borders(reader),
                })
            }
            "type" => {
                p.start = match val(&e).as_deref() {
                    Some("continuous") => SectionBreak::Continuous,
                    Some("evenPage") => SectionBreak::EvenPage,
                    Some("oddPage") => SectionBreak::OddPage,
                    Some("nextColumn") => SectionBreak::NextColumn,
                    _ => SectionBreak::NextPage,
                }
            }
            "titlePg" => p.title_page = on_off(&e),
            "pgNumType" => {
                p.page_number_start = u32_attr(&e, "start");
                p.page_number_format = e
                    .attr(Ns::W, "fmt")
                    .map(|f| crate::parts::number_format(&f));
            }
            "headerReference" | "footerReference" => {
                let Some(id) = e.attr(Ns::R, "id") else {
                    return;
                };
                let refs = if e.local == "headerReference" {
                    &mut p.headers
                } else {
                    &mut p.footers
                };
                let slot = match attr(&e, "type").as_deref() {
                    Some("first") => &mut refs.first,
                    Some("even") => &mut refs.even,
                    _ => &mut refs.default,
                };
                *slot = Some(id.into_owned());
            }
            _ => {}
        }
    });
    p
}

fn columns(reader: &mut Reader<'_>, e: &Element<'_>) -> Columns {
    let mut columns = Columns {
        count: u32_attr(e, "num").unwrap_or(1).clamp(1, 64),
        space: twips_attr(e, "space").unwrap_or(720),
        widths: Vec::new(),
        separator: e
            .attr_raw(Ns::W, "sep")
            .is_some_and(|v| crate::xml::on_off_value(Some(v))),
    };
    let equal = e
        .attr_raw(Ns::W, "equalWidth")
        .is_none_or(|v| crate::xml::on_off_value(Some(v)));
    children(reader, |_, col| {
        if col.local != "col" {
            return;
        }
        columns.widths.push((
            twips_attr(&col, "w").unwrap_or(0),
            twips_attr(&col, "space").unwrap_or(0),
        ));
    });
    if equal {
        columns.widths.clear();
    }
    columns
}

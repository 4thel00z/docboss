//! Stories: the main text, headers, footers, notes, comments and text box
//! content (ECMA-376 Part 1 §17.2, §17.3, §17.4, §17.16).

use std::collections::HashMap;

use docboss_model::{
    Block, Break, ChildBox, Color, DashPattern, Diagnostic, Drawing, DrawingPlacement,
    DrawingPosition, Field, Geometry, Gradient, GradientPath, Hyperlink, Inline, LineCap, LineJoin,
    MediaId, Paragraph, PositionAlign, PositionBase, Revision, RevisionKind, Run, RunContent,
    RunProperties, Section, SectionProperties, Table, TableCell, TableRow, TextDirection,
    VerticalAlign,
};
use docboss_xml::{Element, Ns, Reader};

use crate::geometry;

mod chart;
mod diagram;
mod group;
mod text;
use crate::package::{Package, Relationships};
use crate::props::{
    cell_properties, paragraph_properties, row_properties, run_properties, section_properties,
    table_properties, Theme,
};
use crate::xml::{attr, children, int, int_attr, twips_attr};

/// What a story parser needs to know about its part.
pub struct Context<'p> {
    pub part: &'p str,
    pub rels: &'p Relationships,
    pub media: &'p HashMap<String, MediaId>,
    pub theme: &'p Theme,
    /// The package, for the parts a drawing refers to, such as a
    /// diagram's data and drawing.
    pub package: &'p Package<'p>,
}

/// A paragraph's content before complex fields are folded.
#[allow(clippy::large_enum_variant)]
enum Piece {
    Inline(Inline),
    Begin,
    Separate,
    End,
    Instruction(String),
}

/// A complex field being read (ECMA-376 Part 1 §17.16.18).
struct Frame {
    instruction: String,
    result: Vec<Inline>,
    separated: bool,
    /// The field began in an earlier paragraph; its result is emitted as
    /// plain content and its end only closes it.
    carried: bool,
}

impl Frame {
    fn new() -> Self {
        Self {
            instruction: String::new(),
            result: Vec::new(),
            separated: false,
            carried: false,
        }
    }

    fn carried_copy(&self) -> Self {
        Self {
            instruction: String::new(),
            result: Vec::new(),
            separated: self.separated,
            carried: true,
        }
    }
}

/// Splits a field instruction into its arguments, honoring quotes.
fn field_arguments(instruction: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for c in instruction.chars() {
        match c {
            '"' => {
                if quoted {
                    args.push(std::mem::take(&mut current));
                }
                quoted = !quoted;
            }
            c if c.is_whitespace() && !quoted => {
                if !current.is_empty() {
                    args.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }
    if !current.is_empty() {
        args.push(current);
    }
    args
}

/// A field as an inline: `HYPERLINK` fields (ECMA-376 Part 1 §17.16.5.25)
/// become hyperlinks, everything else a [`Field`] with its cached result.
fn field_inline(instruction: &str, result: Vec<Inline>) -> Inline {
    let instruction = instruction.trim();
    let args = field_arguments(instruction);
    let is_link = args
        .first()
        .is_some_and(|keyword| keyword.eq_ignore_ascii_case("HYPERLINK"));
    if !is_link {
        return Inline::Field(Field {
            instruction: instruction.to_string(),
            result,
        });
    }
    let mut link = Hyperlink {
        inlines: result,
        ..Hyperlink::default()
    };
    let mut rest = args[1..].iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "\\l" => link.anchor = rest.next().cloned(),
            "\\o" => link.tooltip = rest.next().cloned(),
            "\\t" | "\\m" | "\\n" => {}
            _ if link.target.is_none() && !arg.starts_with('\\') => link.target = Some(arg.clone()),
            _ => {}
        }
    }
    Inline::Hyperlink(link)
}

fn inline_text(inline: &Inline, out: &mut String) {
    let paragraph = Paragraph {
        inlines: vec![inline.clone()],
        ..Paragraph::default()
    };
    out.push_str(&paragraph.text());
}

fn push_inline(frames: &mut [Frame], out: &mut Vec<Inline>, inline: Inline) {
    match frames.last_mut() {
        Some(top) if !top.carried && !top.separated => inline_text(&inline, &mut top.instruction),
        Some(top) if !top.carried => top.result.push(inline),
        Some(top) if !top.separated => {}
        _ => out.push(inline),
    }
}

/// Folds field markers into fields. `frames` holds fields still open from
/// earlier paragraphs; with `close` set, fields left open at the end are
/// collapsed into what they hold so far and stay open as carried frames.
fn fold(pieces: Vec<Piece>, frames: &mut Vec<Frame>, close: bool) -> Vec<Inline> {
    let mut out = Vec::new();
    for piece in pieces {
        match piece {
            Piece::Inline(inline) => push_inline(frames, &mut out, inline),
            Piece::Begin => frames.push(Frame::new()),
            Piece::Instruction(text) => {
                if let Some(top) = frames.last_mut().filter(|top| !top.separated) {
                    top.instruction.push_str(&text);
                }
            }
            Piece::Separate => {
                if let Some(top) = frames.last_mut() {
                    top.separated = true;
                }
            }
            Piece::End => {
                let Some(frame) = frames.pop() else {
                    continue;
                };
                if frame.carried {
                    continue;
                }
                let inline = field_inline(&frame.instruction, frame.result);
                push_inline(frames, &mut out, inline);
            }
        }
    }
    if !close {
        return out;
    }
    let open = frames
        .iter()
        .rev()
        .take_while(|frame| !frame.carried)
        .count();
    let mut placeholders = Vec::with_capacity(open);
    for _ in 0..open {
        let Some(frame) = frames.pop() else {
            break;
        };
        placeholders.push(frame.carried_copy());
        let inline = field_inline(&frame.instruction, frame.result);
        push_inline(frames, &mut out, inline);
    }
    frames.extend(placeholders.into_iter().rev());
    out
}

/// Where parsed blocks go: a flat list, or sections when the story is the
/// main text.
#[derive(Default)]
struct Sink {
    blocks: Vec<Block>,
    sections: Vec<Section>,
    tracks_sections: bool,
}

impl Sink {
    fn end_section(&mut self, properties: SectionProperties) {
        let blocks = std::mem::take(&mut self.blocks);
        self.sections.push(Section { properties, blocks });
    }
}

pub struct StoryParser<'p> {
    ctx: Context<'p>,
    pub diagnostics: Vec<Diagnostic>,
    frames: Vec<Frame>,
    /// The last diagram or chart read cannot be drawn, so an
    /// `mc:AlternateContent` fallback is read in its place.
    graphic_unusable: bool,
}

const EMU_PER_POINT: f64 = 12_700.0;

/// A CSS length from a VML `style` attribute, in EMU.
fn css_length(value: &str) -> Option<i64> {
    let value = value.trim();
    let units: [(&str, f64); 6] = [
        ("pt", 1.0),
        ("in", 72.0),
        ("cm", 28.346_457),
        ("mm", 2.834_645_7),
        ("px", 0.75),
        ("pc", 12.0),
    ];
    let (number, factor) = units
        .iter()
        .find_map(|(suffix, factor)| value.strip_suffix(suffix).map(|n| (n, *factor)))
        .unwrap_or((value, 1.0 / EMU_PER_POINT));
    let n: f64 = number.trim().parse().ok()?;
    n.is_finite()
        .then(|| (n * factor * EMU_PER_POINT).round() as i64)
}

fn css_property<'s>(style: &'s str, name: &str) -> Option<&'s str> {
    style.split(';').find_map(|declaration| {
        let (key, value) = declaration.split_once(':')?;
        (key.trim().eq_ignore_ascii_case(name)).then(|| value.trim())
    })
}

const DEFAULT_INSETS: [i64; 4] = [91_440, 45_720, 91_440, 45_720];

/// A named color: DrawingML preset names (ECMA-376 Part 1 §20.1.10.48) and
/// VML names, with the light and dark theme slots read as white and black.
fn named_color(name: &str) -> Option<Color> {
    match name.trim() {
        "white" | "lt1" | "bg1" => Some(Color::WHITE),
        "black" | "dk1" | "tx1" => Some(Color::BLACK),
        "red" => Some(Color(255, 0, 0)),
        "green" => Some(Color(0, 128, 0)),
        "blue" => Some(Color(0, 0, 255)),
        "yellow" => Some(Color(255, 255, 0)),
        "gray" | "grey" => Some(Color(128, 128, 128)),
        _ => None,
    }
}

/// Applies a color transform (ECMA-376 Part 1 §20.1.2.3): `shade` darkens
/// towards black, `tint` lightens towards white, `lumMod` and `lumOff`
/// scale and shift the HSL luminance. Values are in thousandths of a
/// percent.
/// ECMA-376 Part 1 §20.1.2.3.31, §20.1.2.3.34, §20.1.2.3.20, §20.1.2.3.21.
fn transform(color: Color, kind: &str, value: f32) -> Color {
    let channels = [color.0, color.1, color.2].map(|c| c as f32 / 255.0);
    let apply = |f: &dyn Fn(f32) -> f32| {
        let [r, g, b] = channels.map(|c| (f(c).clamp(0.0, 1.0) * 255.0).round() as u8);
        Color(r, g, b)
    };
    match kind {
        "shade" => apply(&|c| c * value),
        "tint" => apply(&|c| c * value + (1.0 - value)),
        "lumMod" | "lumOff" => {
            let (h, s, l) = to_hsl(channels);
            let l = match kind {
                "lumMod" => l * value,
                _ => l + value,
            };
            let [r, g, b] = from_hsl(h, s, l.clamp(0.0, 1.0)).map(|c| (c * 255.0).round() as u8);
            Color(r, g, b)
        }
        _ => color,
    }
}

fn to_hsl([r, g, b]: [f32; 3]) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let d = max - min;
    if d == 0.0 {
        return (0.0, 0.0, l);
    }
    let s = d / (1.0 - (2.0 * l - 1.0).abs());
    let h = match max {
        m if m == r => ((g - b) / d).rem_euclid(6.0),
        m if m == g => (b - r) / d + 2.0,
        _ => (r - g) / d + 4.0,
    };
    (h * 60.0, s, l)
}

fn from_hsl(h: f32, s: f32, l: f32) -> [f32; 3] {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0).rem_euclid(2.0) - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    [r + m, g + m, b + m]
}

/// The first color element among the children: an sRGB value, a system
/// color's last value, a preset, or a theme slot resolved through the
/// theme, each with its transforms applied; a transform's value may be a
/// strict percentage such as `15%`.
/// ECMA-376 Part 1 §20.1.8.54, §20.1.2.3.32, §20.1.2.3.33, §20.1.2.3.22, §20.1.2.3.29.
fn color_choice(reader: &mut Reader<'_>, theme: &Theme) -> Option<Color> {
    let mut color = None;
    children(reader, |reader, e| {
        if e.ns != Ns::A || color.is_some() {
            return;
        }
        let base = match e.local {
            "srgbClr" => e.attr_raw(Ns::NONE, "val").and_then(Color::from_hex),
            "sysClr" => e.attr_raw(Ns::NONE, "lastClr").and_then(Color::from_hex),
            "prstClr" => e.attr_raw(Ns::NONE, "val").and_then(named_color),
            "schemeClr" => e
                .attr_raw(Ns::NONE, "val")
                .and_then(|slot| theme.color(slot).or_else(|| named_color(slot))),
            _ => return,
        };
        let Some(mut base) = base else {
            return;
        };
        children(reader, |_, modifier| {
            let value = modifier.attr_raw(Ns::NONE, "val").and_then(percentage);
            if let Some(value) = value {
                base = transform(base, modifier.local, value as f32 / 100_000.0);
            }
        });
        color = Some(base);
    });
    color
}

/// A shape's fill and outline from `wps:spPr` (ECMA-376 Part 1 §20.4.2.35,
/// §20.1.2.2.24 `a:ln`), recording which of the two it states: the dash
/// (§20.1.8.48, §20.1.8.21), the cap (§20.1.10.31) and the join
/// (§20.1.8.9, §20.1.8.43, §20.1.8.52), the line ends, a gradient fill
/// (§20.1.8.33), the geometry and its rotation and flips (ECMA-376 Part 1
/// §20.1.7.6). An outline without `cap` takes round caps, as LibreOffice
/// draws it, not the square the clause names. Returns a note when a preset
/// dash is unknown or a custom dash has more stops than a pattern holds.
fn shape_properties(
    reader: &mut Reader<'_>,
    theme: &Theme,
    info: &mut DrawingInfo,
) -> Option<String> {
    let mut note = None;
    info.drawing.shape.outline_cap = LineCap::Round;
    info.drawing.geometry = Some(Box::new(Geometry::rectangle()));
    children(reader, |reader, e| {
        if e.ns != Ns::A {
            return;
        }
        match e.local {
            "xfrm" => transform_2d(reader, &e, info),
            "prstGeom" => {
                info.drawing.geometry = Some(Box::new(geometry::preset(reader, &e)));
            }
            "custGeom" => {
                let (custom, dropped) = geometry::custom(reader);
                info.drawing.geometry = Some(Box::new(custom));
                note = dropped.or(note.take());
            }
            "solidFill" => {
                info.drawing.shape.fill = color_choice(reader, theme);
                info.drawing.shape.gradient = None;
                info.fill_stated = true;
            }
            "gradFill" => {
                let gradient = gradient_fill(reader, theme);
                info.drawing.shape.fill = gradient.map(|g| g.average());
                info.drawing.shape.gradient = gradient;
                info.fill_stated = true;
            }
            "noFill" | "blipFill" | "pattFill" => {
                info.drawing.shape.fill = None;
                info.drawing.shape.gradient = None;
                info.fill_stated = e.local == "noFill";
                if e.local == "blipFill" {
                    info.blip = blip_reference(reader);
                }
            }
            "ln" => {
                let shape = &mut info.drawing.shape;
                shape.outline_width = e.attr_raw(Ns::NONE, "w").and_then(int);
                shape.outline_cap = match e.attr_raw(Ns::NONE, "cap") {
                    Some("flat") => LineCap::Flat,
                    Some("sq") => LineCap::Square,
                    _ => LineCap::Round,
                };
                children(reader, |reader, fill| {
                    if fill.ns != Ns::A {
                        return;
                    }
                    let shape = &mut info.drawing.shape;
                    match fill.local {
                        "solidFill" => {
                            shape.outline = color_choice(reader, theme);
                            info.line_stated = true;
                        }
                        "noFill" => {
                            shape.outline = None;
                            info.line_stated = true;
                        }
                        "prstDash" => {
                            let name = fill.attr_raw(Ns::NONE, "val").unwrap_or_default();
                            let Some(dash) = preset_dash(name) else {
                                note = Some(format!("preset dash \"{name}\" is drawn solid"));
                                shape.outline_dash = None;
                                return;
                            };
                            shape.outline_dash = dash;
                        }
                        "custDash" => {
                            let stops = dash_stops(reader);
                            if stops.len() > DashPattern::MAX_STOPS {
                                note = Some(format!(
                                    "a custom dash of {} stops is drawn with its first {}",
                                    stops.len(),
                                    DashPattern::MAX_STOPS
                                ));
                            }
                            shape.outline_dash = DashPattern::new(&stops);
                        }
                        "headEnd" => shape.head_end = geometry::line_end(&fill),
                        "tailEnd" => shape.tail_end = geometry::line_end(&fill),
                        "round" => shape.outline_join = LineJoin::Round,
                        "bevel" => shape.outline_join = LineJoin::Bevel,
                        "miter" => shape.outline_join = LineJoin::Miter,
                        _ => {}
                    }
                });
            }
            _ => {}
        }
    });
    note
}

/// The relationship id of the `a:blip` (ECMA-376 Part 1 §20.1.8.13) a
/// picture fill (§20.1.8.14) embeds.
fn blip_reference(reader: &mut Reader<'_>) -> Option<String> {
    let mut found = None;
    children(reader, |_, e| {
        if e.ns == Ns::A && e.local == "blip" && found.is_none() {
            found = e.attr(Ns::R, "embed").map(Into::into);
        }
    });
    found
}

/// `a:xfrm` (ECMA-376 Part 1 §20.1.7.6): the rotation and flips of a shape
/// or picture, and its offset and extent in the coordinates of the group
/// holding it.
fn transform_2d(reader: &mut Reader<'_>, e: &Element<'_>, info: &mut DrawingInfo) {
    let shape = &mut info.drawing.shape;
    shape.rotation = e
        .attr_raw(Ns::NONE, "rot")
        .and_then(int)
        .map_or(0, |r| r.rem_euclid(21_600_000) as i32);
    shape.flip_horizontal = e.attr_raw(Ns::NONE, "flipH").is_some_and(xml_true);
    shape.flip_vertical = e.attr_raw(Ns::NONE, "flipV").is_some_and(xml_true);
    let mut placed = ChildBox::default();
    children(reader, |_, part| {
        let pair = |x: &str, y: &str| {
            let value =
                |name: &str| part.attr_raw(Ns::NONE, name).and_then(int).unwrap_or(0) as f64;
            (value(x), value(y))
        };
        match part.local {
            "off" => (placed.x, placed.y) = pair("x", "y"),
            "ext" => (placed.width, placed.height) = pair("cx", "cy"),
            _ => {}
        }
    });
    info.placed = Some(placed);
}

/// A gradient fill (ECMA-376 Part 1 §20.1.8.33): its stops (§20.1.8.37,
/// §20.1.8.36) and a linear direction (§20.1.8.41) or a path with its
/// focus rectangle (§20.1.8.46, §20.1.8.31). `None` without stops.
fn gradient_fill(reader: &mut Reader<'_>, theme: &Theme) -> Option<Gradient> {
    let mut stops: Vec<(i64, Color)> = Vec::new();
    let mut angle = 0;
    let mut path = None;
    let mut focus = [0; 4];
    children(reader, |reader, e| match e.local {
        "gsLst" => children(reader, |reader, gs| {
            let position = gs
                .attr_raw(Ns::NONE, "pos")
                .and_then(percentage)
                .unwrap_or(0);
            if let Some(color) = color_choice(reader, theme) {
                stops.push((position, color));
            }
        }),
        "lin" => {
            angle = e
                .attr_raw(Ns::NONE, "ang")
                .and_then(int)
                .map_or(0, |a| a.rem_euclid(21_600_000) as i32);
        }
        "path" => {
            path = Some(match e.attr_raw(Ns::NONE, "path") {
                Some("circle") => GradientPath::Circle,
                Some("rect") => GradientPath::Rect,
                _ => GradientPath::Shape,
            });
            children(reader, |_, rect| {
                if rect.local != "fillToRect" {
                    return;
                }
                let inset = |name: &str| {
                    rect.attr_raw(Ns::NONE, name)
                        .and_then(percentage)
                        .map_or(0, |v| v.clamp(-1_000_000, 1_000_000) as i32)
                };
                focus = [inset("l"), inset("t"), inset("r"), inset("b")];
            });
        }
        _ => {}
    });
    let mut gradient = Gradient::new(&stops)?;
    gradient.angle = angle;
    gradient.path = path;
    gradient.focus = focus;
    Some(gradient)
}

/// ECMA-376 Part 1 §20.1.8.48 and §20.1.10.49: the pattern of a preset
/// dash, `Some(None)` for `solid`, `None` for an unknown name.
fn preset_dash(name: &str) -> Option<Option<DashPattern>> {
    let bits = match name {
        "solid" => "1",
        "dot" => "1000",
        "dash" => "1111000",
        "lgDash" => "11111111000",
        "dashDot" => "11110001000",
        "lgDashDot" => "111111110001000",
        "lgDashDotDot" => "1111111100010001000",
        "sysDash" => "1110",
        "sysDot" => "10",
        "sysDashDot" => "111010",
        "sysDashDotDot" => "11101010",
        _ => return None,
    };
    Some(DashPattern::from_bits(bits))
}

/// The `a:ds` stops of an `a:custDash` (ECMA-376 Part 1 §20.1.8.21,
/// §20.1.8.22) in hundredths of the line width, capped at a thousand.
fn dash_stops(reader: &mut Reader<'_>) -> Vec<(u32, u32)> {
    let mut stops = Vec::new();
    children(reader, |_, ds| {
        if ds.local != "ds" || stops.len() >= 1000 {
            return;
        }
        let length = |name: &str| {
            ds.attr_raw(Ns::NONE, name)
                .and_then(percentage)
                .map_or(0, |v| (v / 1000).clamp(0, i64::from(u32::MAX)) as u32)
        };
        stops.push((length("d"), length("sp")));
    });
    stops
}

/// An ST_PositivePercentage in thousandths of a percent: `800000`, or
/// `800%` as some producers write it.
fn percentage(value: &str) -> Option<i64> {
    let Some(percent) = value.trim().strip_suffix('%') else {
        return int(value);
    };
    let percent: f64 = percent
        .trim()
        .parse()
        .ok()
        .filter(|p: &f64| p.is_finite())?;
    Some((percent * 1000.0) as i64)
}

/// A VML `dashstyle` (ECMA-376 Part 1 §17.3.3.19 carries VML shapes): a
/// preset name or a list of dash and space lengths in line widths.
fn vml_dash(value: &str) -> Option<DashPattern> {
    let value = value.trim();
    let bits = match value.to_ascii_lowercase().as_str() {
        "solid" => return None,
        "shortdash" => "1110",
        "shortdot" => "10",
        "shortdashdot" => "111010",
        "shortdashdotdot" => "11101010",
        "dot" => "1000",
        "dash" => "1111000",
        "longdash" => "11111111000",
        "dashdot" => "11110001000",
        "longdashdot" => "111111110001000",
        "longdashdotdot" => "1111111100010001000",
        _ => {
            let lengths: Vec<u32> = value
                .split([' ', ','])
                .filter(|v| !v.is_empty())
                .take(2 * DashPattern::MAX_STOPS)
                .map(|v| {
                    v.parse::<f32>()
                        .map_or(0, |f| (f.clamp(0.0, 600.0) * 100.0) as u32)
                })
                .collect();
            let stops: Vec<(u32, u32)> = lengths
                .chunks(2)
                .map(|pair| (pair[0], pair.get(1).copied().unwrap_or(pair[0])))
                .collect();
            return DashPattern::new(&stops);
        }
    };
    DashPattern::from_bits(bits)
}

/// The fill and line a shape takes from its `wps:style` references when its
/// `wps:spPr` states none (ECMA-376 Part 1 §20.1.2.2.37, §20.1.4.2.10,
/// §20.1.4.2.19): index 0 means none, any other index the reference's
/// color. Returns the face and color of the font reference
/// (§20.1.4.1.17), which a diagram's text takes.
fn shape_style(
    reader: &mut Reader<'_>,
    theme: &Theme,
    info: &mut DrawingInfo,
) -> (Option<String>, Option<Color>) {
    let mut font = (None, None);
    children(reader, |reader, e| {
        let index = e.attr_raw(Ns::NONE, "idx").and_then(int).unwrap_or(0);
        match e.local {
            "fillRef" if !info.fill_stated => {
                info.drawing.shape.fill =
                    (index > 0).then(|| color_choice(reader, theme)).flatten();
            }
            "lnRef" if !info.line_stated => {
                info.drawing.shape.outline =
                    (index > 0).then(|| color_choice(reader, theme)).flatten();
                if info.drawing.shape.outline_width.is_none() {
                    info.drawing.shape.outline_width = Some(index.clamp(0, 3) * 6_350);
                }
            }
            "fontRef" => {
                let face = match e.attr_raw(Ns::NONE, "idx") {
                    Some("major") => theme.major[0].clone(),
                    Some("minor") => theme.minor[0].clone(),
                    _ => None,
                };
                font = (face, color_choice(reader, theme));
            }
            _ => {}
        }
    });
    font
}

/// A VML color: `#RRGGBB`, `#RGB` or a name, ignoring a trailing
/// `[index]` hint.
fn vml_color(value: &str) -> Option<Color> {
    let value = value.split_whitespace().next()?;
    let Some(hex) = value.strip_prefix('#') else {
        return named_color(value);
    };
    if hex.len() == 3 {
        let doubled: String = hex.chars().flat_map(|c| [c, c]).collect();
        return Color::from_hex(&doubled);
    }
    Color::from_hex(hex)
}

/// The size and position a top-level VML shape or group's `style` gives
/// the drawing: its first stated size, and its absolute position.
fn vml_box(style: &str, info: &mut DrawingInfo) {
    if info.drawing.width == 0 {
        info.drawing.width = css_property(style, "width")
            .and_then(css_length)
            .unwrap_or(0)
            .max(0);
        info.drawing.height = css_property(style, "height")
            .and_then(css_length)
            .unwrap_or(0)
            .max(0);
    }
    if css_property(style, "position").is_some_and(|p| p == "absolute") {
        info.anchored = true;
        info.horizontal = vml_position(style, true);
        info.vertical = vml_position(style, false);
        info.behind = css_property(style, "z-index").is_some_and(|z| z.starts_with('-'));
    }
}

/// The description, fill, outline and geometry of a VML shape element
/// from its attributes.
fn vml_shape(e: &Element<'_>, info: &mut DrawingInfo) {
    if info.drawing.description.is_none() {
        info.drawing.description = e
            .attr(Ns::NONE, "alt")
            .map(Into::into)
            .filter(|d: &String| !d.is_empty());
    }
    let shape = &mut info.drawing.shape;
    let filled = vml_true(e.attr_raw(Ns::NONE, "filled"), true);
    let stroked = vml_true(e.attr_raw(Ns::NONE, "stroked"), true);
    shape.fill = filled
        .then(|| {
            e.attr_raw(Ns::NONE, "fillcolor")
                .map_or(Some(Color::WHITE), vml_color)
        })
        .flatten();
    shape.outline = stroked
        .then(|| {
            e.attr_raw(Ns::NONE, "strokecolor")
                .map_or(Some(Color::BLACK), vml_color)
        })
        .flatten();
    shape.outline_width = Some(
        e.attr_raw(Ns::NONE, "strokeweight")
            .and_then(css_length)
            .unwrap_or(9_525),
    );
    shape.outline_cap = LineCap::Round;
    vml_geometry(e, info);
}

/// A VML `v:fill` of type `gradient` or `gradientRadial` ([MS-ODRAW]
/// §2.3.7.1, §2.3.7.14, §2.3.7.15, §2.3.7.26 give the same fill in binary
/// form): from the shape's fill color to `color2`, or through the stops of
/// `colors`, along the direction `angle` turns counterclockwise from
/// bottom-to-top, with `focus` placing the last color, as LibreOffice
/// reads it.
fn vml_fill(e: &Element<'_>, info: &mut DrawingInfo) {
    let kind = e.attr_raw(Ns::NONE, "type").unwrap_or_default();
    if !matches!(kind, "gradient" | "gradientRadial") {
        return;
    }
    let shape = &mut info.drawing.shape;
    let Some(first) = shape.fill else {
        return;
    };
    let last = e
        .attr_raw(Ns::NONE, "color2")
        .and_then(vml_color)
        .unwrap_or(Color::WHITE);
    let listed: Vec<(i64, Color)> = e
        .attr_raw(Ns::NONE, "colors")
        .map(|colors| {
            colors
                .split(';')
                .filter_map(|stop| {
                    let (position, color) = stop.trim().split_once(' ')?;
                    let position = vml_fraction(position)?;
                    Some(((position * 100_000.0).round() as i64, vml_color(color)?))
                })
                .collect()
        })
        .unwrap_or_default();
    let stops = match listed.len() >= 2 {
        true => listed,
        false => vec![(0, first), (100_000, last)],
    };
    let focus = e
        .attr_raw(Ns::NONE, "focus")
        .and_then(|f| f.trim().strip_suffix('%')?.trim().parse::<f64>().ok())
        .filter(|f| f.is_finite())
        .unwrap_or(0.0)
        .clamp(-100.0, 100.0);
    let angle = e
        .attr_raw(Ns::NONE, "angle")
        .and_then(|a| a.trim().parse::<f64>().ok())
        .filter(|a| a.is_finite())
        .unwrap_or(0.0);
    let Some(mut gradient) = Gradient::focused(&stops, focus / 100.0) else {
        return;
    };
    gradient.angle = (((270.0 - angle) * 60_000.0).round() as i64).rem_euclid(21_600_000) as i32;
    if kind == "gradientRadial" {
        gradient.path = Some(GradientPath::Rect);
        gradient.focus = [50_000; 4];
    }
    shape.fill = Some(gradient.average());
    shape.gradient = Some(gradient);
}

/// The geometry of a VML shape element: `v:rect`, `v:roundrect` with its
/// `arcsize`, `v:oval`, `v:line` between `from` and `to`, or a `v:shape`
/// whose `type` or `o:spt` names a preset shape type ([MS-ODRAW] §2.4.24);
/// with the `flip` and `rotation` of its style.
fn vml_geometry(e: &Element<'_>, info: &mut DrawingInfo) {
    let geometry = match e.local {
        "rect" => Some(Geometry::rectangle()),
        "oval" => Geometry::from_shape_type(3),
        "line" => Geometry::from_shape_type(20),
        "roundrect" => {
            let arc = e
                .attr_raw(Ns::NONE, "arcsize")
                .and_then(vml_fraction)
                .unwrap_or(0.2);
            Some(Geometry::Preset {
                name: "roundRect".into(),
                adjust: vec![("adj".into(), (arc * 50_000.0).round() as i64)],
            })
        }
        _ => e
            .attr_raw(Ns::O, "spt")
            .and_then(|spt| spt.trim().parse::<f64>().ok())
            .map(|spt| spt as u32)
            .or_else(|| {
                e.attr_raw(Ns::NONE, "type")?
                    .strip_prefix("#_x0000_t")?
                    .parse()
                    .ok()
            })
            .and_then(|spt| Geometry::from_shape_type_adjusted(spt, &vml_adjust(e))),
    };
    info.drawing.geometry = geometry.map(Box::new);
    let style = e.attr(Ns::NONE, "style").unwrap_or_default();
    let shape = &mut info.drawing.shape;
    if let Some(flip) = css_property(&style, "flip") {
        shape.flip_horizontal = flip.contains('x');
        shape.flip_vertical = flip.contains('y');
    }
    if let Some(rotation) =
        css_property(&style, "rotation").and_then(|r| r.trim().parse::<f64>().ok())
    {
        shape.rotation = ((rotation * 60_000.0).round() as i64).rem_euclid(21_600_000) as i32;
    }
    if e.local != "line" {
        return;
    }
    let point = |name: &str| -> Option<(i64, i64)> {
        let (x, y) = e.attr_raw(Ns::NONE, name)?.split_once(',')?;
        Some((css_length(x)?, css_length(y)?))
    };
    let (Some(from), Some(to)) = (point("from"), point("to")) else {
        return;
    };
    info.drawing.width = (to.0 - from.0).abs();
    info.drawing.height = (to.1 - from.1).abs();
    info.horizontal.offset = info.horizontal.offset.saturating_add(from.0.min(to.0));
    info.vertical.offset = info.vertical.offset.saturating_add(from.1.min(to.1));
    shape.flip_horizontal ^= to.0 < from.0;
    shape.flip_vertical ^= to.1 < from.1;
}

/// The adjust values of a VML shape's `adj`, comma-separated with blanks
/// for unstated ones ([MS-ODRAW] §2.3.6.10 gives them in binary form).
fn vml_adjust(e: &Element<'_>) -> Vec<Option<i64>> {
    e.attr_raw(Ns::NONE, "adj")
        .map(|adj| {
            adj.split(',')
                .take(8)
                .map(|v| {
                    v.trim()
                        .parse::<f64>()
                        .ok()
                        .filter(|v| v.is_finite())
                        .map(|v| v as i64)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// A VML fraction: `0.25`, `25%` or the 65536ths form `16384f`.
fn vml_fraction(value: &str) -> Option<f64> {
    let value = value.trim();
    let fraction = match (value.strip_suffix('%'), value.strip_suffix('f')) {
        (Some(percent), _) => percent.trim().parse::<f64>().ok()? / 100.0,
        (_, Some(fixed)) => fixed.trim().parse::<f64>().ok()? / 65_536.0,
        _ => value.parse::<f64>().ok()?,
    };
    fraction.is_finite().then_some(fraction.clamp(0.0, 1.0))
}

fn xml_true(value: &str) -> bool {
    matches!(value, "1" | "true")
}

fn vml_true(value: Option<&str>, default: bool) -> bool {
    value.map_or(default, |v| !matches!(v.trim(), "f" | "false" | "0"))
}

struct DrawingInfo {
    drawing: Drawing,
    /// Whether `wps:spPr` stated the fill and the line, so `wps:style` does
    /// not override them.
    fill_stated: bool,
    line_stated: bool,
    anchored: bool,
    /// The box `a:xfrm` gives the shape in its group's coordinates.
    placed: Option<ChildBox>,
    /// The relationship id of the picture that fills the shape.
    blip: Option<String>,
    horizontal: DrawingPosition,
    vertical: DrawingPosition,
    behind: bool,
}

impl DrawingInfo {
    fn new() -> Self {
        Self {
            drawing: Drawing {
                media: None,
                width: 0,
                height: 0,
                placement: DrawingPlacement::Inline,
                name: None,
                description: None,
                text_box: Vec::new(),
                shape: Default::default(),
                geometry: None,
                members: Vec::new(),
                data_text: Vec::new(),
                chart: None,
            },
            fill_stated: false,
            line_stated: false,
            anchored: false,
            placed: None,
            blip: None,
            horizontal: DrawingPosition::offset(PositionBase::Column, 0),
            vertical: DrawingPosition::offset(PositionBase::Paragraph, 0),
            behind: false,
        }
    }

    fn finish(mut self) -> Drawing {
        if self.anchored {
            self.drawing.placement = DrawingPlacement::Anchored {
                horizontal: self.horizontal,
                vertical: self.vertical,
                behind_text: self.behind,
            };
        }
        self.drawing
    }
}

/// The base of a `wp:positionH` or `wp:positionV` position from its
/// `relativeFrom` (ECMA-376 Part 1 §20.4.3.4, §20.4.3.5), or None for a
/// value the axis does not allow.
fn position_base(value: &str, horizontal: bool) -> Option<PositionBase> {
    let base = match value {
        "page" => PositionBase::Page,
        "margin" => PositionBase::Margin,
        "insideMargin" => PositionBase::InsideMargin,
        "outsideMargin" => PositionBase::OutsideMargin,
        "column" if horizontal => PositionBase::Column,
        "character" if horizontal => PositionBase::Character,
        "leftMargin" if horizontal => PositionBase::LeftMargin,
        "rightMargin" if horizontal => PositionBase::RightMargin,
        "paragraph" if !horizontal => PositionBase::Paragraph,
        "line" if !horizontal => PositionBase::Line,
        "topMargin" if !horizontal => PositionBase::TopMargin,
        "bottomMargin" if !horizontal => PositionBase::BottomMargin,
        _ => return None,
    };
    Some(base)
}

/// A VML shape's position on one axis from its `margin-left` or
/// `margin-top` and the `mso-position-*` properties Word writes beside
/// them.
fn vml_position(style: &str, horizontal: bool) -> DrawingPosition {
    let (margin, axis) = match horizontal {
        true => ("margin-left", "horizontal"),
        false => ("margin-top", "vertical"),
    };
    let relative = css_property(style, &format!("mso-position-{axis}-relative"));
    let base = match relative {
        Some("page") => PositionBase::Page,
        Some("margin") => PositionBase::Margin,
        Some("char") if horizontal => PositionBase::Character,
        Some("line") if !horizontal => PositionBase::Line,
        Some("left-margin-area") if horizontal => PositionBase::LeftMargin,
        Some("right-margin-area") if horizontal => PositionBase::RightMargin,
        Some("top-margin-area") if !horizontal => PositionBase::TopMargin,
        Some("bottom-margin-area") if !horizontal => PositionBase::BottomMargin,
        Some("inner-margin-area") => PositionBase::InsideMargin,
        Some("outer-margin-area") => PositionBase::OutsideMargin,
        _ if horizontal => PositionBase::Column,
        _ => PositionBase::Paragraph,
    };
    let offset = css_property(style, margin)
        .and_then(css_length)
        .unwrap_or(0);
    let align = css_property(style, &format!("mso-position-{axis}")).and_then(position_align);
    DrawingPosition {
        base,
        align,
        offset,
    }
}

/// A `wp:align` value (ECMA-376 Part 1 §20.4.3.1, §20.4.3.2).
fn position_align(value: &str) -> Option<PositionAlign> {
    let align = match value {
        "left" | "top" => PositionAlign::Start,
        "center" => PositionAlign::Center,
        "right" | "bottom" => PositionAlign::End,
        "inside" => PositionAlign::Inside,
        "outside" => PositionAlign::Outside,
        _ => return None,
    };
    Some(align)
}

/// ECMA-376 Part 3 §7.5, §7.6, §7.7, §9.3.
/// Picks the branch of `mc:AlternateContent` (ECMA-376 Part 3 §9.3, §7.5) to
/// read: the first `mc:Choice` whose required namespaces this reader
/// understands, else `mc:Fallback`. Calls `f` with the chosen branch's start.
fn alternate_content<'a>(reader: &mut Reader<'a>, mut f: impl FnMut(&mut Reader<'a>)) {
    alternate_branches(reader, |reader| {
        f(reader);
        true
    });
}

/// [`alternate_content`] where `f` returns whether the branch it read is
/// usable; when it is not, the next branch that applies is read.
fn alternate_branches<'a>(reader: &mut Reader<'a>, mut f: impl FnMut(&mut Reader<'a>) -> bool) {
    let mut chosen = false;
    children(reader, |reader, e| {
        if chosen || e.ns != Ns::MC {
            return;
        }
        let take = match e.local {
            "Choice" => e.attr(Ns::NONE, "Requires").is_some_and(|requires| {
                requires
                    .split_whitespace()
                    .all(|prefix| e.resolve_prefix(prefix).is_some_and(understood))
            }),
            "Fallback" => true,
            _ => false,
        };
        if take {
            chosen = f(reader);
        }
    });
}

fn understood(ns: Ns) -> bool {
    [
        Ns::W,
        Ns::WP,
        Ns::A,
        Ns::PIC,
        Ns::WPS,
        Ns::WPG,
        Ns::WPC,
        Ns::W14,
        Ns::WP14,
        Ns::V,
        Ns::O,
        Ns::W10,
        Ns::M,
        Ns::R,
        Ns::DGM,
        Ns::DSP,
        Ns::C,
    ]
    .contains(&ns)
}

impl<'p> StoryParser<'p> {
    pub fn new(ctx: Context<'p>) -> Self {
        Self {
            ctx,
            diagnostics: Vec::new(),
            frames: Vec::new(),
            graphic_unusable: false,
        }
    }

    /// Reads `w:body` into sections (ECMA-376 Part 1 §17.2.2, §17.6.17).
    /// The reader is positioned just after the body's start.
    pub fn body(&mut self, reader: &mut Reader<'_>) -> Vec<Section> {
        let mut sink = Sink {
            tracks_sections: true,
            ..Sink::default()
        };
        let mut last: Option<SectionProperties> = None;
        children(reader, |reader, e| {
            if e.is(Ns::W, "sectPr") {
                last = Some(section_properties(reader));
                return;
            }
            self.block(reader, &e, &mut sink);
        });
        let trailing = !sink.blocks.is_empty() || last.is_some() || sink.sections.is_empty();
        if trailing {
            sink.end_section(last.unwrap_or_default());
        }
        sink.sections
    }

    /// Reads the block-level children of the element just started.
    pub fn blocks(&mut self, reader: &mut Reader<'_>) -> Vec<Block> {
        let mut sink = Sink::default();
        children(reader, |reader, e| self.block(reader, &e, &mut sink));
        sink.blocks
    }

    fn block<'a>(&mut self, reader: &mut Reader<'a>, e: &Element<'a>, sink: &mut Sink) {
        match (e.ns, e.local) {
            (Ns::W, "p") => {
                let (paragraph, section) = self.paragraph(reader);
                sink.blocks.push(Block::Paragraph(paragraph));
                if let Some(section) = section.filter(|_| sink.tracks_sections) {
                    sink.end_section(section);
                }
            }
            (Ns::W, "tbl") => sink.blocks.push(Block::Table(self.table(reader))),
            (Ns::W, "sdt") => children(reader, |reader, e| {
                if e.is(Ns::W, "sdtContent") {
                    children(reader, |reader, e| self.block(reader, &e, sink));
                }
            }),
            (Ns::W, "customXml" | "ins" | "moveTo" | "smartTag") => {
                children(reader, |reader, e| self.block(reader, &e, sink))
            }
            (Ns::MC, "AlternateContent") => alternate_content(reader, |reader| {
                children(reader, |reader, e| self.block(reader, &e, sink))
            }),
            (Ns::W, "altChunk") => self.diagnostics.push(Diagnostic::dropped(
                self.ctx.part,
                "an alternative format chunk (w:altChunk, ECMA-376 Part 1 §17.17.2.1) was not imported",
            )),
            _ => {}
        }
    }

    /// `w:p` (ECMA-376 Part 1 §17.3.1.22).
    fn paragraph(&mut self, reader: &mut Reader<'_>) -> (Paragraph, Option<SectionProperties>) {
        let mut paragraph = Paragraph::default();
        let mut section = None;
        let mut pieces = Vec::new();
        children(reader, |reader, e| {
            if e.is(Ns::W, "pPr") {
                let format = paragraph_properties(reader, self.ctx.theme);
                paragraph.style_id = format.style_id;
                paragraph.properties = format.properties;
                paragraph.mark = format.mark;
                section = format.section;
                return;
            }
            self.inline(reader, &e, &mut pieces);
        });
        let mut frames = std::mem::take(&mut self.frames);
        paragraph.inlines = fold(pieces, &mut frames, true);
        self.frames = frames;
        (paragraph, section)
    }

    fn contained(&mut self, reader: &mut Reader<'_>) -> Vec<Inline> {
        let mut pieces = Vec::new();
        children(reader, |reader, e| self.inline(reader, &e, &mut pieces));
        let mut frames = Vec::new();
        fold(pieces, &mut frames, true)
    }

    fn revision(&mut self, reader: &mut Reader<'_>, e: &Element<'_>, kind: RevisionKind) -> Inline {
        let author = attr(e, "author").map(Into::into);
        let date = attr(e, "date").map(Into::into);
        Inline::Revision(Revision {
            kind,
            author,
            date,
            inlines: self.contained(reader),
        })
    }

    /// Paragraph content: runs, hyperlinks, simple fields, tracked changes,
    /// bookmarks, comment ranges, and the content of structured document
    /// tags, smart tags and custom XML elements with their properties dropped.
    /// ECMA-376 Part 1 §17.16.22, §17.16.19, §17.13.5, §17.13.6, §17.13.4, §17.5.2, §17.5.1.
    fn inline<'a>(&mut self, reader: &mut Reader<'a>, e: &Element<'a>, pieces: &mut Vec<Piece>) {
        if e.ns == Ns::MC && e.local == "AlternateContent" {
            alternate_content(reader, |reader| {
                children(reader, |reader, e| self.inline(reader, &e, pieces))
            });
            return;
        }
        if e.ns == Ns::M {
            if matches!(e.local, "oMath" | "oMathPara") {
                let text = math_text(reader);
                pieces.push(Piece::Inline(Inline::Run(Run {
                    properties: RunProperties::default(),
                    content: vec![RunContent::Text(text)],
                })));
            }
            return;
        }
        if e.ns != Ns::W {
            return;
        }
        match e.local {
            "r" => self.run(reader, pieces),
            "hyperlink" => {
                let target = e
                    .attr(Ns::R, "id")
                    .and_then(|id| self.ctx.rels.get(&id).map(|rel| rel.target.clone()));
                let anchor = attr(e, "anchor").map(Into::into);
                let tooltip = attr(e, "tooltip").map(Into::into);
                let inlines = self.contained(reader);
                pieces.push(Piece::Inline(Inline::Hyperlink(Hyperlink {
                    target,
                    anchor,
                    tooltip,
                    inlines,
                })));
            }
            "fldSimple" => {
                let instruction = attr(e, "instr").unwrap_or_default().into_owned();
                let result = self.contained(reader);
                pieces.push(Piece::Inline(field_inline(&instruction, result)));
            }
            "ins" | "moveTo" => {
                let revision = self.revision(reader, e, RevisionKind::Insertion);
                pieces.push(Piece::Inline(revision));
            }
            "del" | "moveFrom" => {
                let revision = self.revision(reader, e, RevisionKind::Deletion);
                pieces.push(Piece::Inline(revision));
            }
            "smartTag" | "customXml" | "dir" | "bdo" => {
                children(reader, |reader, e| self.inline(reader, &e, pieces))
            }
            "sdt" => children(reader, |reader, e| {
                if e.is(Ns::W, "sdtContent") {
                    children(reader, |reader, e| self.inline(reader, &e, pieces));
                }
            }),
            "bookmarkStart" => {
                let id = int_attr(e, "id").unwrap_or(-1);
                let name = attr(e, "name").unwrap_or_default().into_owned();
                pieces.push(Piece::Inline(Inline::BookmarkStart { id, name }));
            }
            "bookmarkEnd" => pieces.push(Piece::Inline(Inline::BookmarkEnd {
                id: int_attr(e, "id").unwrap_or(-1),
            })),
            "commentRangeStart" => pieces.push(Piece::Inline(Inline::CommentRangeStart(
                int_attr(e, "id").unwrap_or(-1),
            ))),
            "commentRangeEnd" => pieces.push(Piece::Inline(Inline::CommentRangeEnd(
                int_attr(e, "id").unwrap_or(-1),
            ))),
            _ => {}
        }
    }

    /// `w:r` (ECMA-376 Part 1 §17.3.2.25).
    fn run(&mut self, reader: &mut Reader<'_>, pieces: &mut Vec<Piece>) {
        let mut properties = RunProperties::default();
        let mut content = Vec::new();
        children(reader, |reader, e| {
            if e.is(Ns::W, "rPr") {
                properties = run_properties(reader, self.ctx.theme);
                return;
            }
            self.run_content(reader, &e, &properties, &mut content, pieces);
        });
        if content.is_empty() {
            return;
        }
        pieces.push(Piece::Inline(Inline::Run(Run {
            properties,
            content,
        })));
    }

    /// Run content (ECMA-376 Part 1 §17.3.3): text, field characters and
    /// codes, tabs, breaks, symbols, hyphens, note and comment references and
    /// marks, DrawingML and VML pictures.
    /// ECMA-376 Part 1 §17.16.18, §17.16.23, §17.16.13, §17.11.14, §17.11.7, §17.11.13, §17.11.6, §17.13.4.
    fn run_content<'a>(
        &mut self,
        reader: &mut Reader<'a>,
        e: &Element<'a>,
        properties: &RunProperties,
        content: &mut Vec<RunContent>,
        pieces: &mut Vec<Piece>,
    ) {
        if e.ns == Ns::MC && e.local == "AlternateContent" {
            let mut unusable: Option<Vec<RunContent>> = None;
            let mut read = false;
            alternate_branches(reader, |reader| {
                let start = content.len();
                self.graphic_unusable = false;
                children(reader, |reader, e| {
                    self.run_content(reader, &e, properties, content, pieces)
                });
                read = !self.graphic_unusable;
                if !read {
                    let branch = content.split_off(start);
                    unusable.get_or_insert(branch);
                }
                read
            });
            let Some(branch) = unusable else {
                return;
            };
            if !read {
                content.extend(branch);
                return;
            }
            let text = branch.into_iter().find_map(|item| match item {
                RunContent::Drawing(drawing) if !drawing.data_text.is_empty() => {
                    Some(drawing.data_text)
                }
                _ => None,
            });
            let last = content.iter_mut().rev().find_map(|item| match item {
                RunContent::Drawing(drawing) if drawing.data_text.is_empty() => Some(drawing),
                _ => None,
            });
            if let (Some(text), Some(drawing)) = (text, last) {
                drawing.data_text = text;
            }
            return;
        }
        if e.ns != Ns::W {
            return;
        }
        match e.local {
            "t" | "delText" => {
                let text = reader.read_text();
                if let Some(RunContent::Text(previous)) = content.last_mut() {
                    previous.push_str(&text);
                    return;
                }
                content.push(RunContent::Text(text.into_owned()));
            }
            "instrText" | "delInstrText" => {
                let text = reader.read_text().into_owned();
                flush(properties, content, pieces);
                pieces.push(Piece::Instruction(text));
            }
            "fldChar" => {
                flush(properties, content, pieces);
                match attr(e, "fldCharType").as_deref() {
                    Some("begin") => pieces.push(Piece::Begin),
                    Some("separate") => pieces.push(Piece::Separate),
                    Some("end") => pieces.push(Piece::End),
                    _ => {}
                }
            }
            "tab" | "ptab" => content.push(RunContent::Tab),
            "br" => content.push(RunContent::Break(match attr(e, "type").as_deref() {
                Some("page") => Break::Page,
                Some("column") => Break::Column,
                _ => Break::Line,
            })),
            "cr" => content.push(RunContent::CarriageReturn),
            "noBreakHyphen" => content.push(RunContent::NoBreakHyphen),
            "softHyphen" => content.push(RunContent::SoftHyphen),
            "sym" => {
                let font = attr(e, "font").map(Into::into);
                let code = e
                    .attr_raw(Ns::W, "char")
                    .and_then(|hex| u32::from_str_radix(hex.trim(), 16).ok());
                if let Some(char) = code {
                    content.push(RunContent::Symbol { font, char });
                }
            }
            "footnoteReference" => content.push(RunContent::FootnoteReference(
                int_attr(e, "id").unwrap_or(-1),
            )),
            "endnoteReference" => content.push(RunContent::EndnoteReference(
                int_attr(e, "id").unwrap_or(-1),
            )),
            "commentReference" => content.push(RunContent::CommentReference(
                int_attr(e, "id").unwrap_or(-1),
            )),
            "footnoteRef" | "endnoteRef" => content.push(RunContent::NoteNumber),
            "drawing" => {
                let mut info = DrawingInfo::new();
                self.drawing_children(reader, &mut info);
                content.push(RunContent::Drawing(Box::new(info.finish())));
            }
            "pict" | "object" => {
                let mut info = DrawingInfo::new();
                self.vml(reader, &mut info);
                let drawn = info.drawing.media.is_some()
                    || !info.drawing.text_box.is_empty()
                    || info.drawing.geometry.is_some()
                    || !info.drawing.members.is_empty();
                if drawn {
                    content.push(RunContent::Drawing(Box::new(info.finish())));
                }
            }
            _ => {}
        }
    }

    fn media_for(&mut self, id: &str) -> Option<MediaId> {
        let Some(rel) = self.ctx.rels.get(id) else {
            self.diagnostics.push(Diagnostic::dropped(
                self.ctx.part,
                format!("image relationship {id} is not defined"),
            ));
            return None;
        };
        if rel.external {
            self.diagnostics.push(Diagnostic::dropped(
                self.ctx.part,
                format!("linked image {} is outside the package", rel.target),
            ));
            return None;
        }
        let found = self.ctx.media.get(&rel.target).copied();
        if found.is_none() {
            self.diagnostics.push(Diagnostic::dropped(
                self.ctx.part,
                format!("image {} is missing from the package", rel.target),
            ));
        }
        found
    }

    /// Loads the picture a shape's `a:blipFill` names, unless the drawing
    /// has one already.
    fn fill_picture(&mut self, info: &mut DrawingInfo) {
        let Some(id) = info.blip.take() else {
            return;
        };
        if info.drawing.media.is_none() {
            info.drawing.media = self.media_for(&id);
        }
    }

    fn text_box(&mut self, reader: &mut Reader<'_>, info: &mut DrawingInfo) {
        let frames = std::mem::take(&mut self.frames);
        let blocks = self.blocks(reader);
        self.frames = frames;
        info.drawing.text_box.extend(blocks);
    }

    /// The DrawingML picture inside `w:drawing` (ECMA-376 Part 1 §17.3.3.9,
    /// §20.4): inline and anchored objects, their extent, non-visual
    /// properties, position offsets and text box content.
    /// ECMA-376 Part 1 §20.4.2.8, §20.4.2.3, §20.4.2.7, §20.4.2.5, §20.4.2.10, §20.4.2.11, §20.4.2.12, §20.4.2.38.
    /// ECMA-376 Part 1 §20.4.2.1, §20.4.2.2: `wp:align` in place of an offset.
    /// ECMA-376 Part 1 §20.4.2.42, §20.4.2.37: text boxes inside WordprocessingML shapes.
    /// ECMA-376 Part 1 §20.4.2.22: `wps:bodyPr` insets, text anchor and `a:spAutoFit`;
    /// ECMA-376 Part 1 §20.1.10.83: `vert`, and `upright`.
    fn drawing_children(&mut self, reader: &mut Reader<'_>, info: &mut DrawingInfo) {
        children(reader, |reader, e| {
            match (e.ns, e.local) {
                (Ns::WP, "anchor") => {
                    info.anchored = true;
                    info.behind = e
                        .attr_raw(Ns::NONE, "behindDoc")
                        .is_some_and(|v| v == "1" || v == "true");
                }
                (Ns::WP, "extent") => {
                    info.drawing.width =
                        e.attr_raw(Ns::NONE, "cx").and_then(int).unwrap_or(0).max(0);
                    info.drawing.height =
                        e.attr_raw(Ns::NONE, "cy").and_then(int).unwrap_or(0).max(0);
                    return;
                }
                (Ns::WP, "docPr") => {
                    info.drawing.name = e
                        .attr(Ns::NONE, "name")
                        .map(Into::into)
                        .filter(|n: &String| !n.is_empty());
                    info.drawing.description = e
                        .attr(Ns::NONE, "descr")
                        .map(Into::into)
                        .filter(|d: &String| !d.is_empty());
                    return;
                }
                (Ns::WP, "positionH" | "positionV") => {
                    let horizontal = e.local == "positionH";
                    let position = match horizontal {
                        true => &mut info.horizontal,
                        false => &mut info.vertical,
                    };
                    let from = e.attr_raw(Ns::NONE, "relativeFrom").unwrap_or_default();
                    match position_base(from, horizontal) {
                        Some(base) => position.base = base,
                        None => self.diagnostics.push(Diagnostic::approximated(
                            self.ctx.part,
                            format!(
                                "drawing position relativeFrom=\"{from}\" is read as the default"
                            ),
                        )),
                    }
                    let mut unknown = None;
                    children(reader, |reader, child| match child.local {
                        "posOffset" => position.offset = int(&reader.read_text()).unwrap_or(0),
                        "align" => {
                            let text = reader.read_text();
                            position.align = position_align(text.trim());
                            if position.align.is_none() {
                                unknown = Some(text.into_owned());
                            }
                        }
                        _ => {}
                    });
                    if let Some(text) = unknown {
                        self.diagnostics.push(Diagnostic::approximated(
                            self.ctx.part,
                            format!("drawing alignment \"{text}\" is read as an offset"),
                        ));
                    }
                    return;
                }
                (Ns::A, "blip") => {
                    if info.drawing.media.is_none() {
                        if let Some(id) = e.attr(Ns::R, "embed") {
                            info.drawing.media = self.media_for(&id);
                        }
                    }
                }
                (Ns::W, "txbxContent") => {
                    self.text_box(reader, info);
                    return;
                }
                (Ns::WPS, "spPr") => {
                    if let Some(note) = shape_properties(reader, self.ctx.theme, info) {
                        self.diagnostics
                            .push(Diagnostic::approximated(self.ctx.part, note));
                    }
                    self.fill_picture(info);
                    return;
                }
                (Ns::WPS, "style") => {
                    shape_style(reader, self.ctx.theme, info);
                    return;
                }
                (Ns::WPS, "bodyPr") => {
                    self.body_properties(reader, &e, info);
                    return;
                }
                (Ns::MC, "AlternateContent") => {
                    alternate_content(reader, |reader| self.drawing_children(reader, info));
                    return;
                }
                (Ns::A, "graphicData") => {
                    let uri = e.attr_raw(Ns::NONE, "uri").unwrap_or_default();
                    if uri.ends_with("/chart") {
                        children(reader, |_, chart| {
                            if chart.ns == Ns::C && chart.local == "chart" {
                                self.graphic_unusable = !self.chart(&chart, info);
                            }
                        });
                        return;
                    }
                    if uri.ends_with("/diagram") {
                        children(reader, |_, rel| {
                            if rel.ns == Ns::DGM && rel.local == "relIds" {
                                self.graphic_unusable = !self.diagram(&rel, info);
                            }
                        });
                        return;
                    }
                }
                (Ns::WPG, "wgp") | (Ns::WPC, "wpc") => {
                    self.group(reader, &e, info, &mut Vec::new());
                    return;
                }
                (Ns::WPG, "xfrm") => {
                    transform_2d(reader, &e, info);
                    return;
                }
                (Ns::PIC, "spPr") => {
                    children(reader, |reader, x| {
                        if x.ns == Ns::A && x.local == "xfrm" {
                            transform_2d(reader, &x, info);
                        }
                    });
                    return;
                }
                _ => {}
            }
            self.drawing_children(reader, info);
        });
    }

    /// `wps:bodyPr` or `a:bodyPr` (ECMA-376 Part 1 §20.4.2.22,
    /// §21.1.2.1.1): the insets, the text direction, `upright`, the
    /// vertical anchor and `a:spAutoFit` (§21.1.2.1.4).
    fn body_properties(
        &mut self,
        reader: &mut Reader<'_>,
        e: &Element<'_>,
        info: &mut DrawingInfo,
    ) {
        let inset =
            |name: &str, default: i64| e.attr_raw(Ns::NONE, name).and_then(int).unwrap_or(default);
        let shape = &mut info.drawing.shape;
        shape.insets = Some([
            inset("lIns", DEFAULT_INSETS[0]),
            inset("tIns", DEFAULT_INSETS[1]),
            inset("rIns", DEFAULT_INSETS[2]),
            inset("bIns", DEFAULT_INSETS[3]),
        ]);
        shape.text_upright = e.attr_raw(Ns::NONE, "upright").is_some_and(xml_true);
        let vert = e.attr_raw(Ns::NONE, "vert").unwrap_or("horz");
        shape.text_direction = match vert {
            "vert" | "eaVert" | "mongolianVert" => TextDirection::TopToBottom,
            "vert270" => TextDirection::BottomToTop,
            _ => TextDirection::LeftToRight,
        };
        if vert.starts_with("wordArtVert") {
            self.diagnostics.push(Diagnostic::approximated(
                self.ctx.part,
                format!("stacked text (vert=\"{vert}\") is laid out across"),
            ));
        }
        shape.text_anchor = match e.attr_raw(Ns::NONE, "anchor") {
            Some("ctr") => Some(VerticalAlign::Center),
            Some("b") => Some(VerticalAlign::Bottom),
            Some("t") => Some(VerticalAlign::Top),
            _ => None,
        };
        children(reader, |_, fit| {
            if fit.ns == Ns::A && fit.local == "spAutoFit" {
                shape.auto_fit = true;
            }
        });
    }

    /// A VML picture or text box inside `w:pict` or `w:object` (ECMA-376
    /// Part 1 §17.3.3.19).
    fn vml(&mut self, reader: &mut Reader<'_>, info: &mut DrawingInfo) {
        children(reader, |reader, e| {
            match (e.ns, e.local) {
                (Ns::V, "shape" | "rect" | "roundrect" | "oval" | "line") => {
                    if let Some(style) = e.attr(Ns::NONE, "style") {
                        vml_box(&style, info);
                    }
                    vml_shape(&e, info);
                }
                (Ns::V, "group") => {
                    if let Some(style) = e.attr(Ns::NONE, "style") {
                        vml_box(&style, info);
                    }
                    self.vml_group(reader, &e, info, &mut Vec::new());
                    return;
                }
                (Ns::V, "fill") => {
                    vml_fill(&e, info);
                    return;
                }
                (Ns::V, "stroke") => {
                    let shape = &mut info.drawing.shape;
                    if let Some(style) = e.attr_raw(Ns::NONE, "dashstyle") {
                        shape.outline_dash = vml_dash(style);
                    }
                    shape.head_end = geometry::vml_line_end(&e, "start");
                    shape.tail_end = geometry::vml_line_end(&e, "end");
                    match e.attr_raw(Ns::NONE, "endcap") {
                        Some("flat") => shape.outline_cap = LineCap::Flat,
                        Some("round") => shape.outline_cap = LineCap::Round,
                        Some("square") => shape.outline_cap = LineCap::Square,
                        _ => {}
                    }
                    match e.attr_raw(Ns::NONE, "joinstyle") {
                        Some("round") => shape.outline_join = LineJoin::Round,
                        Some("bevel") => shape.outline_join = LineJoin::Bevel,
                        Some("miter") => shape.outline_join = LineJoin::Miter,
                        _ => {}
                    }
                    return;
                }
                (Ns::V, "textbox") => {
                    let shape = &mut info.drawing.shape;
                    let insets: Vec<i64> = e
                        .attr_raw(Ns::NONE, "inset")
                        .map(|inset| {
                            inset
                                .split(',')
                                .map(|v| css_length(v).unwrap_or(-1))
                                .collect()
                        })
                        .unwrap_or_default();
                    shape.insets = Some(std::array::from_fn(|i| {
                        insets
                            .get(i)
                            .copied()
                            .filter(|v| *v >= 0)
                            .unwrap_or(DEFAULT_INSETS[i])
                    }));
                    let style = e.attr(Ns::NONE, "style").unwrap_or_default();
                    shape.auto_fit = css_property(&style, "mso-fit-shape-to-text")
                        .is_some_and(|v| v == "t" || v == "true");
                    shape.text_direction = match (
                        css_property(&style, "layout-flow"),
                        css_property(&style, "mso-layout-flow-alt"),
                    ) {
                        (Some("vertical" | "vertical-ideographic"), Some("bottom-to-top")) => {
                            TextDirection::BottomToTop
                        }
                        (Some("vertical" | "vertical-ideographic"), _) => {
                            TextDirection::TopToBottom
                        }
                        _ => TextDirection::LeftToRight,
                    };
                }
                (Ns::V, "imagedata") => {
                    let id = e.attr(Ns::R, "id").or_else(|| e.attr(Ns::O, "relid"));
                    if let Some(id) = id.filter(|_| info.drawing.media.is_none()) {
                        info.drawing.media = self.media_for(&id);
                    }
                    return;
                }
                (Ns::W, "txbxContent") => {
                    self.text_box(reader, info);
                    return;
                }
                _ => {}
            }
            self.vml(reader, info);
        });
    }

    /// `w:tbl` (ECMA-376 Part 1 §17.4.37) with its grid of column widths
    /// (ECMA-376 Part 1 §17.4.48, §17.4.16).
    fn table(&mut self, reader: &mut Reader<'_>) -> Table {
        let mut table = Table::default();
        children(reader, |reader, e| {
            if e.ns != Ns::W {
                return;
            }
            match e.local {
                "tblPr" => table.properties = table_properties(reader),
                "tblGrid" => children(reader, |_, col| {
                    if col.local == "gridCol" {
                        table.grid.push(twips_attr(&col, "w").unwrap_or(0).max(0));
                    }
                }),
                _ => self.rows(reader, &e, &mut table.rows),
            }
        });
        table
    }

    fn rows<'a>(&mut self, reader: &mut Reader<'a>, e: &Element<'a>, rows: &mut Vec<TableRow>) {
        match e.local {
            "tr" => rows.push(self.row(reader)),
            "sdt" | "sdtContent" | "customXml" | "ins" | "moveTo" => {
                children(reader, |reader, e| self.rows(reader, &e, rows))
            }
            _ => {}
        }
    }

    /// `w:tr` (ECMA-376 Part 1 §17.4.78).
    fn row(&mut self, reader: &mut Reader<'_>) -> TableRow {
        let mut row = TableRow::default();
        children(reader, |reader, e| match e.local {
            "trPr" => row.properties = row_properties(reader),
            _ => self.cells(reader, &e, &mut row.cells),
        });
        row
    }

    /// `w:tc` (ECMA-376 Part 1 §17.4.65) cells of a row.
    fn cells<'a>(&mut self, reader: &mut Reader<'a>, e: &Element<'a>, cells: &mut Vec<TableCell>) {
        match e.local {
            "tc" => {
                let mut cell = TableCell::default();
                cell.properties.grid_span = 1;
                let mut sink = Sink::default();
                children(reader, |reader, e| {
                    if e.is(Ns::W, "tcPr") {
                        cell.properties = cell_properties(reader);
                        return;
                    }
                    self.block(reader, &e, &mut sink);
                });
                cell.blocks = sink.blocks;
                cells.push(cell);
            }
            "sdt" | "sdtContent" | "customXml" => {
                children(reader, |reader, e| self.cells(reader, &e, cells))
            }
            _ => {}
        }
    }
}

fn flush(properties: &RunProperties, content: &mut Vec<RunContent>, pieces: &mut Vec<Piece>) {
    if content.is_empty() {
        return;
    }
    let run = Run {
        properties: properties.clone(),
        content: std::mem::take(content),
    };
    pieces.push(Piece::Inline(Inline::Run(run)));
}

/// The text of an Office Math zone: its `m:t` runs in order.
fn math_text(reader: &mut Reader<'_>) -> String {
    let mut out = String::new();
    fn walk(reader: &mut Reader<'_>, out: &mut String) {
        children(reader, |reader, e| {
            if e.ns == Ns::M && e.local == "t" {
                out.push_str(&reader.read_text());
                return;
            }
            walk(reader, out);
        });
    }
    walk(reader, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ECMA-376 Part 1 §17.16.5.25: HYPERLINK field switches.
    #[test]
    fn hyperlink_field_arguments() {
        let Inline::Hyperlink(link) =
            field_inline(r#" HYPERLINK "https://x.test/a b" \o "tip" "#, Vec::new())
        else {
            panic!()
        };
        assert_eq!(link.target.as_deref(), Some("https://x.test/a b"));
        assert_eq!(link.tooltip.as_deref(), Some("tip"));
        let Inline::Hyperlink(link) = field_inline(r#"HYPERLINK \l "_Toc1""#, Vec::new()) else {
            panic!()
        };
        assert_eq!(link.anchor.as_deref(), Some("_Toc1"));
        assert!(link.target.is_none());
        assert!(matches!(field_inline("PAGE", Vec::new()), Inline::Field(_)));
    }

    #[test]
    fn css_lengths() {
        assert_eq!(css_length("72pt"), Some(914_400));
        assert_eq!(css_length("1in"), Some(914_400));
        assert_eq!(
            css_property("width:10pt; height:5pt", "height"),
            Some("5pt")
        );
    }
}

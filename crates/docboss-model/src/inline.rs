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
#[derive(Debug, Clone, Copy, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(tag = "type", rename_all = "snake_case"))]
pub enum DrawingPlacement {
    /// In line with the text, like a large character.
    #[default]
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
#[derive(Debug, Clone, Default, PartialEq)]
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
    /// The outline of a drawn shape; `None` for a picture.
    pub geometry: Option<Box<Geometry>>,
    /// The drawings of a group or drawing canvas, in painting order; empty
    /// for any other drawing.
    pub members: Vec<crate::GroupMember>,
    /// The text output reads for the drawing in place of its text boxes,
    /// such as a diagram's text from its data model; empty for most
    /// drawings.
    pub data_text: Vec<crate::Block>,
    /// The chart the drawing shows, from its cached values.
    pub chart: Option<Box<crate::Chart>>,
}

/// A shape's geometry (ECMA-376 Part 1 §20.1.9).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(tag = "type", rename_all = "snake_case"))]
pub enum Geometry {
    /// A preset shape by its `ST_ShapeType` name, such as `rightArrow`, and
    /// the adjust values the document gives it by guide name.
    Preset {
        name: String,
        adjust: Vec<(String, i64)>,
    },
    Custom(CustomGeometry),
}

impl Geometry {
    /// The preset rectangle.
    pub fn rectangle() -> Geometry {
        Geometry::Preset {
            name: "rect".into(),
            adjust: Vec::new(),
        }
    }

    /// Whether this is the plain preset rectangle.
    pub fn is_rectangle(&self) -> bool {
        matches!(self, Geometry::Preset { name, .. } if name == "rect")
    }

    /// The preset geometry of an [MS-ODRAW] §2.4.24 MSOSPT shape type, as
    /// VML `o:spt` and the DOC shape records give it, with default adjust
    /// values; `None` for custom, picture, WordArt and unmatched types.
    pub fn from_shape_type(shape_type: u32) -> Option<Geometry> {
        let name = match shape_type {
            1 | 202 => "rect",
            2 => "roundRect",
            3 => "ellipse",
            4 => "diamond",
            5 => "triangle",
            6 => "rtTriangle",
            7 => "parallelogram",
            9 => "hexagon",
            10 => "octagon",
            11 => "plus",
            12 => "star5",
            13 | 14 => "rightArrow",
            15 => "homePlate",
            16 => "cube",
            17 | 62 => "wedgeRoundRectCallout",
            18 | 59 => "star16",
            19 => "arc",
            20 => "line",
            21 => "plaque",
            22 => "can",
            23 => "donut",
            32 => "straightConnector1",
            33 => "bentConnector2",
            34 => "bentConnector3",
            35 => "bentConnector4",
            36 => "bentConnector5",
            37 => "curvedConnector2",
            38 => "curvedConnector3",
            39 => "curvedConnector4",
            40 => "curvedConnector5",
            41 => "callout1",
            42 => "callout2",
            43 => "callout3",
            44 => "accentCallout1",
            45 => "accentCallout2",
            46 => "accentCallout3",
            47 => "borderCallout1",
            48 => "borderCallout2",
            49 => "borderCallout3",
            50 => "accentBorderCallout1",
            51 => "accentBorderCallout2",
            52 => "accentBorderCallout3",
            53 => "ribbon",
            54 => "ribbon2",
            55 => "chevron",
            56 => "pentagon",
            57 => "noSmoking",
            58 => "star8",
            60 => "star32",
            61 => "wedgeRectCallout",
            63 => "wedgeEllipseCallout",
            64 => "wave",
            65 => "foldedCorner",
            66 => "leftArrow",
            67 => "downArrow",
            68 => "upArrow",
            69 => "leftRightArrow",
            70 => "upDownArrow",
            71 => "irregularSeal1",
            72 => "irregularSeal2",
            73 => "lightningBolt",
            74 => "heart",
            76 => "quadArrow",
            77 => "leftArrowCallout",
            78 => "rightArrowCallout",
            79 => "upArrowCallout",
            80 => "downArrowCallout",
            81 => "leftRightArrowCallout",
            82 => "upDownArrowCallout",
            83 => "quadArrowCallout",
            84 => "bevel",
            85 => "leftBracket",
            86 => "rightBracket",
            87 => "leftBrace",
            88 => "rightBrace",
            89 => "leftUpArrow",
            90 => "bentUpArrow",
            91 => "bentArrow",
            92 => "star24",
            93 => "stripedRightArrow",
            94 => "notchedRightArrow",
            95 => "blockArc",
            96 => "smileyFace",
            97 => "verticalScroll",
            98 => "horizontalScroll",
            99 => "circularArrow",
            101 => "uturnArrow",
            102 => "curvedRightArrow",
            103 => "curvedLeftArrow",
            104 => "curvedUpArrow",
            105 => "curvedDownArrow",
            106 => "cloudCallout",
            107 => "ellipseRibbon",
            108 => "ellipseRibbon2",
            109 => "flowChartProcess",
            110 => "flowChartDecision",
            111 => "flowChartInputOutput",
            112 => "flowChartPredefinedProcess",
            113 => "flowChartInternalStorage",
            114 => "flowChartDocument",
            115 => "flowChartMultidocument",
            116 => "flowChartTerminator",
            117 => "flowChartPreparation",
            118 => "flowChartManualInput",
            119 => "flowChartManualOperation",
            120 => "flowChartConnector",
            121 => "flowChartPunchedCard",
            122 => "flowChartPunchedTape",
            123 => "flowChartSummingJunction",
            124 => "flowChartOr",
            125 => "flowChartCollate",
            126 => "flowChartSort",
            127 => "flowChartExtract",
            128 => "flowChartMerge",
            129 => "flowChartOfflineStorage",
            130 => "flowChartOnlineStorage",
            131 => "flowChartMagneticTape",
            132 => "flowChartMagneticDisk",
            133 => "flowChartMagneticDrum",
            134 => "flowChartDisplay",
            135 => "flowChartDelay",
            176 => "flowChartAlternateProcess",
            177 => "flowChartOffpageConnector",
            182 => "leftRightUpArrow",
            183 => "sun",
            184 => "moon",
            185 => "bracketPair",
            186 => "bracePair",
            187 => "star4",
            188 => "doubleWave",
            189 => "actionButtonBlank",
            190 => "actionButtonHome",
            191 => "actionButtonHelp",
            192 => "actionButtonInformation",
            193 => "actionButtonForwardNext",
            194 => "actionButtonBackPrevious",
            195 => "actionButtonEnd",
            196 => "actionButtonBeginning",
            197 => "actionButtonReturn",
            198 => "actionButtonDocument",
            199 => "actionButtonSound",
            200 => "actionButtonMovie",
            _ => return None,
        };
        Some(Geometry::Preset {
            name: name.into(),
            adjust: Vec::new(),
        })
    }

    /// [`Geometry::from_shape_type`] with the shape's adjust values on
    /// their 21600 grid ([MS-ODRAW] §2.3.6.10 adjustValue to
    /// §2.3.6.17 adjust8Value, and VML `adj`), unstated ones `None`. They
    /// are carried over for the presets whose guides take them as the same
    /// fraction of the shape: the triangle's apex, the rounded
    /// rectangle's corner, the bent connectors' elbows and the wedge
    /// callouts' tip. Other types keep their default proportions.
    pub fn from_shape_type_adjusted(shape_type: u32, values: &[Option<i64>]) -> Option<Geometry> {
        let mut geometry = Geometry::from_shape_type(shape_type)?;
        let Geometry::Preset { adjust, .. } = &mut geometry else {
            return Some(geometry);
        };
        let scaled: fn(i64) -> i64 = |v| v.saturating_mul(100_000) / 21_600;
        let centered: fn(i64) -> i64 =
            |v| v.saturating_sub(10_800).saturating_mul(100_000) / 21_600;
        let rules: &[AdjustRule] = match shape_type {
            2 | 5 => &[("adj", scaled)],
            34 => &[("adj1", scaled)],
            35 => &[("adj1", scaled), ("adj2", scaled)],
            36 => &[("adj1", scaled), ("adj2", scaled), ("adj3", scaled)],
            61..=63 => &[("adj1", centered), ("adj2", centered)],
            _ => &[],
        };
        for ((name, map), value) in rules.iter().zip(values) {
            if let Some(value) = value {
                adjust.push(((*name).into(), map(*value)));
            }
        }
        Some(geometry)
    }
}

/// A preset guide name and the map from an MSOSPT adjust value to it.
type AdjustRule = (&'static str, fn(i64) -> i64);

/// A shape guide: a name and its formula, as `a:gd` writes them, such as
/// `*/ w adj 100000`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Guide {
    pub name: String,
    pub formula: String,
}

/// A geometry given by its own guides and paths (`a:custGeom`).
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct CustomGeometry {
    /// Adjust values by guide name.
    pub adjust: Vec<(String, i64)>,
    pub guides: Vec<Guide>,
    pub paths: Vec<GeometryPath>,
}

/// How a geometry path is filled.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum PathFill {
    None,
    #[default]
    Normal,
    Lighten,
    LightenLess,
    Darken,
    DarkenLess,
}

/// One path of a geometry, in its own coordinate space of `width` by
/// `height` (the shape's extent when zero).
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct GeometryPath {
    pub width: i64,
    pub height: i64,
    pub fill: PathFill,
    pub stroke: bool,
    pub commands: Vec<PathCommand>,
}

/// A path command. Every value is a number or the name of a guide.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(
    feature = "serde",
    serde(tag = "type", content = "args", rename_all = "snake_case")
)]
pub enum PathCommand {
    MoveTo([String; 2]),
    LineTo([String; 2]),
    /// Width radius, height radius, start angle and swing angle.
    ArcTo([String; 4]),
    /// Control point and end point.
    QuadTo([String; 4]),
    /// Two control points and the end point.
    CubicTo([String; 6]),
    Close,
}

/// The decoration at one end of a line (`a:headEnd`, `a:tailEnd`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct LineEnd {
    pub kind: LineEndKind,
    pub width: LineEndSize,
    pub length: LineEndSize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum LineEndKind {
    Triangle,
    Stealth,
    Diamond,
    Oval,
    /// An open arrowhead of two strokes.
    Arrow,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum LineEndSize {
    Small,
    #[default]
    Medium,
    Large,
}

/// Fill, outline and text frame of a shape. Every field is optional: an
/// unstated fill or outline is not drawn.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ShapeFormat {
    pub fill: Option<crate::Color>,
    /// A gradient drawn in place of `fill`, which then holds its average.
    pub gradient: Option<crate::Gradient>,
    pub outline: Option<crate::Color>,
    /// Outline width in EMU.
    pub outline_width: Option<i64>,
    /// The outline's dashes; `None` for a solid outline.
    pub outline_dash: Option<DashPattern>,
    pub outline_cap: LineCap,
    pub outline_join: LineJoin,
    /// Space between the shape edge and its text in EMU: left, top, right,
    /// bottom.
    pub insets: Option<[i64; 4]>,
    /// Where the text sits vertically: `Top`, `Center` or `Bottom`.
    pub text_anchor: Option<crate::VerticalAlign>,
    /// The shape grows to fit its text.
    pub auto_fit: bool,
    /// How the text runs inside the shape.
    pub text_direction: crate::TextDirection,
    /// The text stays upright when the shape turns.
    pub text_upright: bool,
    /// Clockwise rotation about the shape's center, in 60000ths of a degree.
    pub rotation: i32,
    pub flip_horizontal: bool,
    pub flip_vertical: bool,
    /// The decoration at the start of an open outline.
    pub head_end: Option<LineEnd>,
    /// The decoration at the end of an open outline.
    pub tail_end: Option<LineEnd>,
}

/// A repeating dash pattern: dash and space lengths, in hundredths of the
/// line width.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct DashPattern {
    stops: [(u16, u16); DashPattern::MAX_STOPS],
    count: u8,
}

impl DashPattern {
    /// The most dash and space pairs a pattern holds.
    pub const MAX_STOPS: usize = 4;

    /// A pattern from dash and space pairs in hundredths of the line width,
    /// keeping the first [`DashPattern::MAX_STOPS`]. `None` when no pair is
    /// given or every length is zero.
    pub fn new(stops: &[(u32, u32)]) -> Option<DashPattern> {
        let mut pattern = DashPattern::default();
        for (slot, (dash, space)) in pattern.stops.iter_mut().zip(stops) {
            *slot = (clamp_u16(*dash), clamp_u16(*space));
            pattern.count += 1;
        }
        let period: u32 = pattern
            .stops()
            .iter()
            .map(|(d, s)| u32::from(*d) + u32::from(*s))
            .sum();
        if period == 0 {
            return None;
        }
        Some(pattern)
    }

    /// A pattern from a string of `1`s and `0`s, each a line width of dash
    /// or of space, as the preset dash tables write them: `"1111000"` is a
    /// dash of four widths and a space of three. `None` for a solid line.
    pub fn from_bits(bits: &str) -> Option<DashPattern> {
        let mut stops: Vec<(u32, u32)> = Vec::new();
        let mut bytes = bits.bytes().peekable();
        while bytes.peek().is_some() {
            let mut dash = 0;
            while bytes.next_if_eq(&b'1').is_some() {
                dash += 100;
            }
            let mut space = 0;
            while bytes.next_if_eq(&b'0').is_some() {
                space += 100;
            }
            if dash == 0 && space == 0 {
                return None;
            }
            stops.push((dash, space));
        }
        if stops.iter().all(|(_, space)| *space == 0) {
            return None;
        }
        DashPattern::new(&stops)
    }

    /// The dash and space pairs, in hundredths of the line width.
    pub fn stops(&self) -> &[(u16, u16)] {
        &self.stops[..usize::from(self.count)]
    }
}

fn clamp_u16(value: u32) -> u16 {
    value.min(u32::from(u16::MAX)) as u16
}

/// How the ends of a line, and of each of its dashes, are drawn.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum LineCap {
    /// The line ends at its end point.
    #[default]
    Flat,
    /// A square half the line width deep past the end point.
    Square,
    /// A half disc past the end point.
    Round,
}

/// How an outline's corners are drawn.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum LineJoin {
    #[default]
    Round,
    Bevel,
    Miter,
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
    Drawing(Box<Drawing>),
    /// An Office Math zone.
    Math(Box<crate::Math>),
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
    CommentRangeStart { id: i64 },
    CommentRangeEnd { id: i64 },
}

#[cfg(test)]
mod tests {
    use super::*;

    /// [MS-ODRAW] §2.4.24: MSOSPT shape types map to the preset geometries
    /// of the same shape.
    #[test]
    fn shape_types_name_presets() {
        let name = |spt: u32| match Geometry::from_shape_type(spt) {
            Some(Geometry::Preset { name, .. }) => Some(name),
            _ => None,
        };
        assert_eq!(name(1).as_deref(), Some("rect"));
        assert_eq!(name(5).as_deref(), Some("triangle"));
        assert_eq!(name(13).as_deref(), Some("rightArrow"));
        assert_eq!(name(58).as_deref(), Some("star8"));
        assert_eq!(name(202).as_deref(), Some("rect"));
        assert_eq!(name(0), None);
        assert_eq!(name(75), None);
        assert_eq!(name(136), None);
        assert!(Geometry::rectangle().is_rectangle());
    }

    /// [MS-ODRAW] §2.3.6.10: adjust values on the 21600 grid become the
    /// preset's adjust values where the guides take the same fraction.
    #[test]
    fn shape_type_adjust_values_carry_over() {
        let adjust = |spt: u32, values: &[Option<i64>]| match Geometry::from_shape_type_adjusted(
            spt, values,
        ) {
            Some(Geometry::Preset { adjust, .. }) => adjust,
            _ => panic!(),
        };
        assert_eq!(adjust(5, &[Some(21_600)]), vec![("adj".into(), 100_000)]);
        assert_eq!(adjust(34, &[None, Some(1)]), vec![]);
        assert_eq!(
            adjust(61, &[Some(10_800), Some(21_600)]),
            vec![("adj1".into(), 0), ("adj2".into(), 50_000)]
        );
        assert_eq!(adjust(13, &[Some(5)]), vec![]);
    }

    /// ECMA-376 Part 1 §20.1.10.49 and [MS-ODRAW] §2.4.15: the preset
    /// dashes as strings of line-width dashes and spaces.
    #[test]
    fn dash_patterns_from_bit_strings() {
        let long = DashPattern::from_bits("1111111100010001000").unwrap();
        assert_eq!(long.stops(), &[(800, 300), (100, 300), (100, 300)]);
        assert_eq!(DashPattern::from_bits("10").unwrap().stops(), &[(100, 100)]);
        assert_eq!(DashPattern::from_bits("1"), None);
        assert_eq!(DashPattern::from_bits(""), None);
        assert_eq!(DashPattern::new(&[(0, 0)]), None);
        let many: Vec<(u32, u32)> = (0..20).map(|_| (100, 100)).collect();
        assert_eq!(
            DashPattern::new(&many).unwrap().stops().len(),
            DashPattern::MAX_STOPS
        );
    }
}

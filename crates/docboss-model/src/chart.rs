use crate::Color;

/// A chart as its part caches it (ECMA-376 Part 1 §21.2): the plots with
/// their series' cached categories and values, the axes, title and
/// legend, with every color resolved.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Chart {
    pub title: Option<ChartText>,
    pub plots: Vec<ChartPlot>,
    /// The axis the categories, or a scatter chart's x values, run along.
    pub category_axis: Option<ChartAxis>,
    pub value_axis: Option<ChartAxis>,
    pub legend: Option<ChartLegend>,
    /// The chart area's fill and outline.
    pub area: ChartFrame,
    /// The plot area's fill and outline.
    pub plot_area: ChartFrame,
}

/// The fill and outline of a chart's area or plot area.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ChartFrame {
    pub fill: Option<Color>,
    pub outline: Option<Color>,
    /// Outline width in EMU.
    pub outline_width: Option<i64>,
    pub outline_dash: Option<crate::DashPattern>,
}

/// Text in a chart: the words, and the size in hundredths of a point,
/// weight, color and face they are drawn in.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ChartText {
    pub text: String,
    pub size: u32,
    pub bold: bool,
    pub color: Color,
    pub font: Option<String>,
}

impl Default for ChartText {
    /// Empty text, 10 points, black.
    fn default() -> Self {
        ChartText {
            text: String::new(),
            size: 1000,
            bold: false,
            color: Color::BLACK,
            font: None,
        }
    }
}

/// How the series of a bar, line or area plot combine.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum ChartGrouping {
    /// Side by side, or each from the axis.
    #[default]
    Standard,
    /// Each series on top of the ones before.
    Stacked,
    /// Stacked, and scaled so each category totals 100%.
    PercentStacked,
}

/// The kind of one plot of a chart.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(tag = "type", rename_all = "snake_case"))]
pub enum ChartKind {
    /// Bars, horizontal or as columns; `gap` is the space between
    /// categories and `overlap` the overlap of a category's bars, both in
    /// percent of a bar's width.
    Bar {
        horizontal: bool,
        grouping: ChartGrouping,
        gap: u16,
        overlap: i16,
    },
    Line {
        grouping: ChartGrouping,
    },
    Area {
        grouping: ChartGrouping,
    },
    /// A pie, or a doughnut when `hole` (percent of the radius) is not
    /// zero; slices start `first_angle` degrees clockwise from the top.
    Pie {
        hole: u8,
        first_angle: u16,
    },
    /// Points at their x and y values, joined by lines when `lines`.
    Scatter {
        lines: bool,
    },
}

/// One plot: a chart kind and its series.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ChartPlot {
    pub kind: ChartKind,
    pub series: Vec<ChartSeries>,
}

/// A series from its cached values: its name, categories, values (or a
/// scatter series' x and y values), and its colors.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ChartSeries {
    pub name: Option<String>,
    pub categories: Vec<String>,
    pub values: Vec<Option<f64>>,
    /// A scatter series' x values; empty for other series.
    pub x_values: Vec<Option<f64>>,
    /// The fill of its bars, areas and markers.
    pub fill: Option<Color>,
    /// The color of its line, or of its bars' outline.
    pub line: Option<Color>,
    /// Line width in EMU.
    pub line_width: Option<i64>,
    /// Per point fills, such as a pie's slices; `None` keeps `fill`.
    pub point_fills: Vec<Option<Color>>,
    /// Markers are drawn at a line or scatter series' points.
    pub markers: bool,
}

/// Where an axis sits beside the plot area.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum ChartSide {
    #[default]
    Bottom,
    Left,
    Top,
    Right,
}

/// A chart axis: whether it shows, its line, gridlines and tick labels,
/// and a value axis' bounds.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ChartAxis {
    pub deleted: bool,
    pub side: ChartSide,
    pub line: Option<Color>,
    pub gridlines: Option<Color>,
    /// Tick labels are drawn, in the style of `text`.
    pub labels: bool,
    pub text: ChartText,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub major_unit: Option<f64>,
    /// The axis runs from its maximum to its minimum.
    pub reversed: bool,
    /// The number format of the tick labels, such as `0%`.
    pub format: Option<String>,
    pub title: Option<ChartText>,
}

/// A chart legend: where it sits and the style of its entries.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ChartLegend {
    pub side: ChartSide,
    pub text: ChartText,
}

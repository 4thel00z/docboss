//! Charts: a chart part's cached categories and values (ECMA-376 Part 1
//! §21.2), read into a [`Chart`] to draw and a title and table of its
//! values for text output.

use docboss_model::{
    Block, Chart, ChartAxis, ChartFrame, ChartGrouping, ChartKind, ChartLegend, ChartPlot,
    ChartSeries, ChartSide, ChartText, Color, DashPattern, Diagnostic, Inline, Paragraph, Run,
    RunContent, Table, TableCell, TableCellProperties, TableRow,
};
use docboss_xml::{decode, Element, Ns, Reader};

use super::text::{body_defaults, rich_text, TextRun};
use super::{shape_properties, transform, DrawingInfo, StoryParser};
use crate::props::Theme;
use crate::xml::{children, root};

/// The most points a cache keeps.
const MAX_POINTS: usize = 4_096;
/// The most series a chart keeps.
const MAX_SERIES: usize = 256;
/// The most categories and series the text output table shows.
const TABLE_ROWS: usize = 50;
const TABLE_COLUMNS: usize = 12;

/// Text as the part styles it, before the chart's own defaults apply.
#[derive(Debug, Clone, Default)]
struct Styled {
    text: String,
    run: TextRun,
}

impl Styled {
    fn resolve(&self, base: &TextRun, size: u32, bold: bool) -> ChartText {
        let mut run = base.clone();
        run.apply(&self.run);
        ChartText {
            text: self.text.clone(),
            size: run.size.unwrap_or(size).clamp(100, 40_000),
            bold: run.bold.unwrap_or(bold),
            color: run.color.unwrap_or(Color::BLACK),
            font: run.font.clone(),
        }
    }
}

#[derive(Debug, Clone, Default)]
struct AxisRead {
    id: Option<String>,
    axis: ChartAxis,
    text: Styled,
    title: Option<Styled>,
    value: bool,
}

#[derive(Debug, Clone, Default)]
struct ChartRead {
    title: Option<Styled>,
    auto_title_deleted: bool,
    plots: Vec<ChartPlot>,
    axes: Vec<AxisRead>,
    legend: Option<(ChartSide, Styled)>,
    area: Option<ChartFrame>,
    plot_area: ChartFrame,
    text: TextRun,
    unsupported: Vec<String>,
}

fn val<'a>(e: &'a Element<'_>) -> Option<&'a str> {
    e.attr_raw(Ns::NONE, "val")
}

fn flag(e: &Element<'_>) -> bool {
    val(e).is_none_or(|v| matches!(v, "1" | "true"))
}

fn number(text: &str) -> Option<f64> {
    text.trim().parse::<f64>().ok().filter(|v| v.is_finite())
}

/// The fill, outline color and outline width `c:spPr` (§21.2.2.197)
/// states, each `None` when it says none or nothing: the outer `Option`
/// tells whether the fill or outline was stated at all.
struct Format {
    fill: Option<Option<Color>>,
    line: Option<Option<Color>>,
    width: Option<i64>,
    dash: Option<DashPattern>,
}

impl Format {
    fn frame(&self) -> ChartFrame {
        ChartFrame {
            fill: self.fill.flatten(),
            outline: self.line.flatten(),
            outline_width: self.width,
            outline_dash: self.dash,
        }
    }
}

fn format(reader: &mut Reader<'_>, theme: &Theme) -> Format {
    let mut info = DrawingInfo::new();
    shape_properties(reader, theme, &mut info);
    let shape = info.drawing.shape;
    Format {
        fill: info.fill_stated.then_some(shape.fill),
        line: info.line_stated.then_some(shape.outline),
        width: shape.outline_width,
        dash: shape.outline_dash,
    }
}

/// The color the theme's accent cycle gives series or point `index`:
/// accents 1 to 6, then darker and lighter turns of them.
fn accent(theme: &Theme, index: usize) -> Color {
    let base = theme
        .color(&format!("accent{}", index % 6 + 1))
        .unwrap_or(Color(0x4F, 0x81, 0xBD));
    match (index / 6) % 4 {
        1 => transform(base, "lumMod", 0.6),
        2 => transform(transform(base, "lumMod", 0.8), "lumOff", 0.2),
        3 => transform(base, "lumMod", 0.8),
        _ => base,
    }
}

/// A string or number cache (§21.2.2.199, §21.2.2.120, §21.2.2.114):
/// its points by index (§21.2.2.151, §21.2.2.150), the first level of a
/// multi-level cache, and the number format of a number cache.
fn cache(reader: &mut Reader<'_>, out: &mut Vec<Option<String>>, format: &mut Option<String>) {
    let mut count = None;
    let mut level = false;
    children(reader, |reader, e| {
        if e.ns != Ns::C {
            return;
        }
        match e.local {
            "strRef" | "numRef" | "multiLvlStrRef" => cache(reader, out, format),
            "strCache" | "numCache" | "multiLvlStrCache" | "strLit" | "numLit" => {
                cache(reader, out, format)
            }
            "lvl" if !level => {
                level = true;
                cache(reader, out, format)
            }
            "formatCode" => *format = Some(reader.read_text().into_owned()),
            "ptCount" => {
                count = val(&e)
                    .and_then(|v| v.trim().parse::<usize>().ok())
                    .map(|n| n.min(MAX_POINTS));
                if let Some(n) = count {
                    out.resize(n.max(out.len()), None);
                }
            }
            "pt" => {
                let Some(index) = e
                    .attr_raw(Ns::NONE, "idx")
                    .and_then(|v| v.trim().parse::<usize>().ok())
                    .filter(|i| *i < MAX_POINTS)
                else {
                    return;
                };
                let mut text = None;
                children(reader, |reader, v| {
                    if v.ns == Ns::C && v.local == "v" {
                        text = Some(reader.read_text().into_owned());
                    }
                });
                if out.len() <= index {
                    out.resize(index + 1, None);
                }
                out[index] = text;
            }
            _ => {}
        }
    });
}

impl StoryParser<'_> {
    /// Reads the chart a `c:chart` reference (§21.2.2.26) names into the
    /// drawing's chart and data text. Returns false when no plot of it can
    /// be drawn, so a fallback picture is drawn in its place.
    pub(super) fn chart(&mut self, e: &Element<'_>, info: &mut DrawingInfo) -> bool {
        info.drawing.geometry = None;
        let part = e
            .attr(Ns::R, "id")
            .and_then(|id| self.ctx.rels.get(&id))
            .filter(|rel| !rel.external)
            .map(|rel| rel.target.clone());
        let Some(part) = part else {
            self.diagnostics.push(Diagnostic::dropped(
                self.ctx.part,
                "a chart reference without its part",
            ));
            return false;
        };
        let Some(bytes) = self.ctx.package.part_into(&part, &mut self.diagnostics) else {
            self.diagnostics.push(Diagnostic::dropped(
                part.as_str(),
                "the chart part is missing",
            ));
            return false;
        };
        let text = decode(&bytes);
        let read = chart_space(&text, self.ctx.theme);
        for kind in &read.unsupported {
            self.diagnostics.push(Diagnostic::dropped(
                part.as_str(),
                format!("a {kind} chart is not drawn"),
            ));
        }
        let drawable = !read.plots.is_empty();
        let chart = finish(read);
        let (table, dropped) = chart_table(&chart);
        if dropped {
            self.diagnostics.push(Diagnostic::approximated(
                part.as_str(),
                format!(
                    "the chart's text keeps its first {TABLE_ROWS} categories and {TABLE_COLUMNS} series"
                ),
            ));
        }
        info.drawing.data_text = table;
        info.drawing.chart = Some(Box::new(chart));
        drawable
    }
}

/// `c:chartSpace` (§21.2.2.29): the chart (§21.2.2.27), the chart area's
/// `c:spPr` and the `c:txPr` (§21.2.2.216) every text of the chart starts
/// from.
fn chart_space(text: &str, theme: &Theme) -> ChartRead {
    let mut read = ChartRead::default();
    let mut reader = Reader::new(text);
    if root(&mut reader).is_none() {
        return read;
    }
    children(&mut reader, |reader, e| match (e.ns, e.local) {
        (Ns::C, "chart") => chart_element(reader, theme, &mut read),
        (Ns::C, "spPr") => {
            let f = format(reader, theme);
            let mut frame = f.frame();
            frame.fill = f.fill.unwrap_or(Some(Color::WHITE));
            read.area = Some(frame);
        }
        (Ns::C, "txPr") => read.text = body_defaults(reader, theme).defaults,
        _ => {}
    });
    read
}

fn chart_element(reader: &mut Reader<'_>, theme: &Theme, read: &mut ChartRead) {
    children(reader, |reader, e| {
        if e.ns != Ns::C {
            return;
        }
        match e.local {
            "title" => read.title = Some(title(reader, theme)),
            "autoTitleDeleted" => read.auto_title_deleted = flag(&e),
            "plotArea" => plot_area(reader, theme, read),
            "legend" => read.legend = Some(legend(reader, theme)),
            _ => {}
        }
    });
}

/// `c:title` (§21.2.2.210): its `c:tx` rich text (§21.2.2.214,
/// §21.2.2.156) or, without one, the style its `c:txPr` gives the
/// automatic title.
fn title(reader: &mut Reader<'_>, theme: &Theme) -> Styled {
    let mut styled = Styled::default();
    children(reader, |reader, e| {
        if e.ns != Ns::C {
            return;
        }
        match e.local {
            "tx" => children(reader, |reader, rich| {
                if rich.ns == Ns::C && rich.local == "rich" {
                    let (text, run) = rich_text(reader, theme);
                    styled.text = text;
                    styled.run.apply(&run);
                }
            }),
            "txPr" => styled.run.apply(&body_defaults(reader, theme).defaults),
            _ => {}
        }
    });
    styled
}

/// `c:legend` (§21.2.2.93) with its `c:legendPos` (§21.2.2.95), right
/// when unstated.
fn legend(reader: &mut Reader<'_>, theme: &Theme) -> (ChartSide, Styled) {
    let mut side = ChartSide::Right;
    let mut styled = Styled::default();
    children(reader, |reader, e| match (e.ns, e.local) {
        (Ns::C, "legendPos") => {
            side = match val(&e) {
                Some("b") => ChartSide::Bottom,
                Some("t") => ChartSide::Top,
                Some("l") => ChartSide::Left,
                _ => ChartSide::Right,
            }
        }
        (Ns::C, "txPr") => styled.run = body_defaults(reader, theme).defaults,
        _ => {}
    });
    (side, styled)
}

/// `c:plotArea` (§21.2.2.145): its plots, axes and `c:spPr`.
fn plot_area(reader: &mut Reader<'_>, theme: &Theme, read: &mut ChartRead) {
    children(reader, |reader, e| {
        if e.ns != Ns::C {
            return;
        }
        match e.local {
            "barChart" | "lineChart" | "areaChart" | "pieChart" | "doughnutChart"
            | "scatterChart" => {
                if let Some(plot) = plot(reader, &e, theme) {
                    read.plots.push(plot);
                }
            }
            "catAx" | "dateAx" | "valAx" | "serAx" => {
                let mut axis = axis(reader, theme);
                axis.value = e.local == "valAx";
                if e.local != "serAx" {
                    read.axes.push(axis);
                }
            }
            "spPr" => read.plot_area = format(reader, theme).frame(),
            "layout" | "dTable" | "extLst" => {}
            other => read
                .unsupported
                .push(other.trim_end_matches("Chart").replace("3D", " 3-D")),
        }
    });
}

/// A plot element: `c:barChart` (§21.2.2.16) with `c:barDir`
/// (§21.2.2.17), `c:grouping` (§21.2.2.77), `c:gapWidth` (§21.2.2.75,
/// 150 by default) and `c:overlap` (§21.2.2.131); `c:lineChart`
/// (§21.2.2.97) and `c:areaChart` (§21.2.2.5) with `c:grouping`
/// (§21.2.2.76); `c:pieChart` (§21.2.2.141) and `c:doughnutChart`
/// (§21.2.2.50) with `c:firstSliceAng` (§21.2.2.68) and `c:holeSize`
/// (§21.2.2.82); `c:scatterChart` (§21.2.2.161) with `c:scatterStyle`
/// (§21.2.2.162); `c:varyColors` (§21.2.2.227) colors each point; and
/// their series.
fn plot(reader: &mut Reader<'_>, e: &Element<'_>, theme: &Theme) -> Option<ChartPlot> {
    let local = e.local;
    let mut horizontal = false;
    let mut grouping = ChartGrouping::Standard;
    let mut gap = 150u16;
    let mut overlap = 0i16;
    let mut hole = if local == "doughnutChart" { 50u8 } else { 0 };
    let mut first_angle = 0u16;
    let mut lines = true;
    let mut vary = false;
    let mut markers = true;
    let mut series: Vec<ChartSeries> = Vec::new();
    let mut indices: Vec<usize> = Vec::new();
    children(reader, |reader, child| {
        if child.ns != Ns::C {
            return;
        }
        let value = val(&child);
        let int = || value.and_then(|v| v.trim().parse::<i64>().ok());
        match child.local {
            "barDir" => horizontal = value == Some("bar"),
            "grouping" => {
                grouping = match value {
                    Some("stacked") => ChartGrouping::Stacked,
                    Some("percentStacked") => ChartGrouping::PercentStacked,
                    _ => ChartGrouping::Standard,
                }
            }
            "gapWidth" => gap = int().map_or(150, |v| v.clamp(0, 500) as u16),
            "overlap" => overlap = int().map_or(0, |v| v.clamp(-100, 100) as i16),
            "holeSize" => hole = int().map_or(50, |v| v.clamp(1, 90) as u8),
            "firstSliceAng" => first_angle = int().map_or(0, |v| v.clamp(0, 360) as u16),
            "scatterStyle" => lines = !matches!(value, Some("marker" | "none")),
            "varyColors" => vary = flag(&child),
            "marker" => markers = flag(&child),
            "ser" if series.len() < MAX_SERIES => {
                let (read, index) = series_element(reader, theme, series.len());
                series.push(read);
                indices.push(index);
            }
            _ => {}
        }
    });
    let kind = match local {
        "barChart" => ChartKind::Bar {
            horizontal,
            grouping,
            gap,
            overlap: match grouping {
                ChartGrouping::Standard => overlap,
                _ => 100,
            },
        },
        "lineChart" => ChartKind::Line { grouping },
        "areaChart" => ChartKind::Area { grouping },
        "pieChart" | "doughnutChart" => ChartKind::Pie { hole, first_angle },
        "scatterChart" => ChartKind::Scatter { lines },
        _ => return None,
    };
    let pie = matches!(kind, ChartKind::Pie { .. });
    let marked = matches!(kind, ChartKind::Line { .. } | ChartKind::Scatter { .. });
    for (s, index) in series.iter_mut().zip(indices) {
        let color = accent(theme, index);
        s.markers &= marked && markers;
        match kind {
            ChartKind::Line { .. } | ChartKind::Scatter { .. } => {
                s.line = s.line.or(Some(color));
                s.fill = s.fill.or(s.line);
            }
            _ => s.fill = s.fill.or(Some(color)),
        }
        if let ChartKind::Scatter { lines: false } = kind {
            s.line = None;
        }
        if !(vary || pie) || s.values.len() > MAX_POINTS {
            continue;
        }
        let fills = (0..s.values.len()).map(|i| Some(accent(theme, i)));
        let mut points: Vec<Option<Color>> = fills.collect();
        for (slot, own) in points.iter_mut().zip(&s.point_fills) {
            if own.is_some() {
                *slot = *own;
            }
        }
        s.point_fills = points;
    }
    Some(ChartPlot { kind, series })
}

/// `c:ser` (§21.2.2.170 and its kin): `c:idx` (§21.2.2.84), the `c:tx`
/// name (§21.2.2.215), `c:spPr` colors, `c:dPt` point fills
/// (§21.2.2.52), the `c:marker` symbol (§21.2.2.106, §21.2.2.205), and
/// the cached `c:cat` or `c:xVal` categories (§21.2.2.24, §21.2.2.234)
/// and `c:val` or `c:yVal` values (§21.2.2.224, §21.2.2.237). Returns the
/// series and the index its default color takes.
fn series_element(reader: &mut Reader<'_>, theme: &Theme, position: usize) -> (ChartSeries, usize) {
    let mut series = ChartSeries {
        markers: true,
        ..ChartSeries::default()
    };
    let mut index = position;
    let mut categories: Vec<Option<String>> = Vec::new();
    let mut values: Vec<Option<String>> = Vec::new();
    let mut scatter = false;
    let mut unused = None;
    children(reader, |reader, e| {
        if e.ns != Ns::C {
            return;
        }
        match e.local {
            "idx" => {
                index = val(&e)
                    .and_then(|v| v.trim().parse::<usize>().ok())
                    .unwrap_or(position)
                    .min(1 << 20)
            }
            "tx" => {
                let mut name = Vec::new();
                children(reader, |reader, t| match (t.ns, t.local) {
                    (Ns::C, "v") => name.push(Some(reader.read_text().into_owned())),
                    (Ns::C, "strRef") => cache(reader, &mut name, &mut unused),
                    _ => {}
                });
                series.name = name.into_iter().flatten().next();
            }
            "spPr" => {
                let f = format(reader, theme);
                series.fill = f.fill.flatten();
                series.line = f.line.flatten();
                series.line_width = f.width;
            }
            "dPt" => {
                let mut at = None;
                let mut fill = None;
                children(reader, |reader, p| match (p.ns, p.local) {
                    (Ns::C, "idx") => at = val(&p).and_then(|v| v.trim().parse::<usize>().ok()),
                    (Ns::C, "spPr") => fill = format(reader, theme).fill.flatten(),
                    _ => {}
                });
                if let Some(at) = at.filter(|a| *a < MAX_POINTS) {
                    if series.point_fills.len() <= at {
                        series.point_fills.resize(at + 1, None);
                    }
                    series.point_fills[at] = fill;
                }
            }
            "marker" => children(reader, |_, m| {
                if m.ns == Ns::C && m.local == "symbol" && val(&m) == Some("none") {
                    series.markers = false;
                }
            }),
            "cat" => cache(reader, &mut categories, &mut unused),
            "xVal" => {
                scatter = true;
                cache(reader, &mut categories, &mut unused)
            }
            "val" | "yVal" => cache(reader, &mut values, &mut unused),
            _ => {}
        }
    });
    series.values = values
        .iter()
        .map(|v| v.as_deref().and_then(number))
        .collect();
    if scatter {
        series.x_values = categories
            .iter()
            .map(|v| v.as_deref().and_then(number))
            .collect();
    }
    series.categories = categories
        .into_iter()
        .map(Option::unwrap_or_default)
        .collect();
    (series, index)
}

/// An axis: `c:axId`, `c:scaling` (§21.2.2.160) with `c:orientation`
/// (§21.2.2.130), `c:min` and `c:max` (§21.2.2.108, §21.2.2.107),
/// `c:delete` (§21.2.2.40), `c:axPos` (§21.2.2.10), `c:majorGridlines`
/// (§21.2.2.100), `c:numFmt` (§21.2.2.121), `c:tickLblPos`
/// (§21.2.2.207), `c:majorUnit` (§21.2.2.103), its line and text.
fn axis(reader: &mut Reader<'_>, theme: &Theme) -> AxisRead {
    let mut read = AxisRead {
        axis: ChartAxis {
            labels: true,
            line: Some(Color(0x86, 0x86, 0x86)),
            ..ChartAxis::default()
        },
        ..AxisRead::default()
    };
    children(reader, |reader, e| {
        if e.ns != Ns::C {
            return;
        }
        let axis = &mut read.axis;
        match e.local {
            "axId" => read.id = val(&e).map(Into::into),
            "scaling" => children(reader, |_, s| match s.local {
                "orientation" => axis.reversed = val(&s) == Some("maxMin"),
                "min" => axis.min = val(&s).and_then(number),
                "max" => axis.max = val(&s).and_then(number),
                _ => {}
            }),
            "delete" => axis.deleted = flag(&e),
            "axPos" => {
                axis.side = match val(&e) {
                    Some("l") => ChartSide::Left,
                    Some("r") => ChartSide::Right,
                    Some("t") => ChartSide::Top,
                    _ => ChartSide::Bottom,
                }
            }
            "majorGridlines" => {
                let mut color = Some(Color(0xB3, 0xB3, 0xB3));
                children(reader, |reader, g| {
                    if g.ns == Ns::C && g.local == "spPr" {
                        if let Some(line) = format(reader, theme).line {
                            color = line;
                        }
                    }
                });
                axis.gridlines = color;
            }
            "numFmt" => axis.format = e.attr(Ns::NONE, "formatCode").map(Into::into),
            "tickLblPos" => axis.labels = val(&e) != Some("none"),
            "majorUnit" => axis.major_unit = val(&e).and_then(number).filter(|u| *u > 0.0),
            "spPr" => {
                if let Some(line) = format(reader, theme).line {
                    axis.line = line;
                }
            }
            "txPr" => read.text.run = body_defaults(reader, theme).defaults,
            "title" => read.title = Some(title(reader, theme)),
            _ => {}
        }
    });
    read
}

/// The chart from what its part says, the chart's text defaults (10
/// points) applied to its titles, labels and legend.
fn finish(read: ChartRead) -> Chart {
    let base = &read.text;
    let size = base.size.unwrap_or(1000);
    let single_name = || {
        let series: Vec<&ChartSeries> = read.plots.iter().flat_map(|p| &p.series).collect();
        match series.as_slice() {
            [only] => only.name.clone(),
            _ => None,
        }
    };
    let title = read
        .title
        .as_ref()
        .filter(|_| !read.auto_title_deleted)
        .map(|t| {
            let mut t = t.resolve(base, size * 18 / 10, true);
            if t.text.trim().is_empty() {
                t.text = single_name().unwrap_or_else(|| "Chart Title".into());
            }
            t
        });
    let scatter = read
        .plots
        .first()
        .is_some_and(|p| matches!(p.kind, ChartKind::Scatter { .. }));
    let horizontal_side = |a: &&AxisRead| matches!(a.axis.side, ChartSide::Bottom | ChartSide::Top);
    let pick = |value: bool| -> Option<ChartAxis> {
        let found = match scatter {
            true => read.axes.iter().find(|a| horizontal_side(a) != value),
            false => read.axes.iter().find(|a| a.value == value),
        }?;
        let mut axis = found.axis.clone();
        axis.text = found.text.resolve(base, size, false);
        axis.title = found
            .title
            .as_ref()
            .map(|t| t.resolve(base, size, true))
            .filter(|t| !t.text.trim().is_empty());
        Some(axis)
    };
    let pie = read
        .plots
        .iter()
        .all(|p| matches!(p.kind, ChartKind::Pie { .. }));
    Chart {
        title,
        category_axis: pick(false).filter(|_| !pie),
        value_axis: pick(true).filter(|_| !pie),
        legend: read.legend.as_ref().map(|(side, styled)| ChartLegend {
            side: *side,
            text: styled.resolve(base, size, false),
        }),
        area: read.area.unwrap_or(ChartFrame {
            fill: Some(Color::WHITE),
            outline: Some(Color(0x86, 0x86, 0x86)),
            outline_width: Some(9_525),
            outline_dash: None,
        }),
        plot_area: read.plot_area,
        plots: read.plots,
    }
}

fn cell(text: &str) -> TableCell {
    let inlines = match text.is_empty() {
        true => Vec::new(),
        false => vec![Inline::Run(Run {
            content: vec![RunContent::Text(text.to_string())],
            ..Run::default()
        })],
    };
    TableCell {
        properties: TableCellProperties {
            grid_span: 1,
            ..TableCellProperties::default()
        },
        blocks: vec![Block::Paragraph(Paragraph {
            inlines,
            ..Paragraph::default()
        })],
    }
}

fn shown(value: Option<f64>) -> String {
    let Some(value) = value else {
        return String::new();
    };
    let rounded = format!("{value:.10}");
    let trimmed = rounded.trim_end_matches('0').trim_end_matches('.');
    match trimmed {
        "-0" => "0".into(),
        other => other.into(),
    }
}

/// The chart's title and a table of its cached values: a row per
/// category (or x value) and a column per series. The flag tells whether
/// rows or columns past the caps were left out.
fn chart_table(chart: &Chart) -> (Vec<Block>, bool) {
    let mut blocks = Vec::new();
    if let Some(title) = chart.title.as_ref().filter(|t| !t.text.trim().is_empty()) {
        blocks.push(Block::Paragraph(Paragraph {
            inlines: vec![Inline::Run(Run {
                content: vec![RunContent::Text(title.text.clone())],
                ..Run::default()
            })],
            ..Paragraph::default()
        }));
    }
    let series: Vec<&ChartSeries> = chart.plots.iter().flat_map(|p| &p.series).collect();
    let Some(first) = series.first() else {
        return (blocks, false);
    };
    let scatter = !first.x_values.is_empty();
    let rows = series
        .iter()
        .map(|s| s.values.len().max(s.categories.len()))
        .max()
        .unwrap_or(0);
    let dropped = rows > TABLE_ROWS || series.len() > TABLE_COLUMNS;
    let columns = &series[..series.len().min(TABLE_COLUMNS)];
    let mut header = vec![cell(if scatter { "x" } else { "" })];
    header.extend(columns.iter().enumerate().map(|(i, s)| {
        cell(
            &s.name
                .clone()
                .unwrap_or_else(|| format!("Series {}", i + 1)),
        )
    }));
    let mut table = Table {
        rows: vec![TableRow {
            cells: header,
            ..TableRow::default()
        }],
        ..Table::default()
    };
    for row in 0..rows.min(TABLE_ROWS) {
        let label = match scatter {
            true => shown(first.x_values.get(row).copied().flatten()),
            false => first.categories.get(row).cloned().unwrap_or_default(),
        };
        let mut cells = vec![cell(&label)];
        cells.extend(
            columns
                .iter()
                .map(|s| cell(&shown(s.values.get(row).copied().flatten()))),
        );
        table.rows.push(TableRow {
            cells,
            ..TableRow::default()
        });
    }
    table.grid = vec![1440; columns.len() + 1];
    blocks.push(Block::Table(table));
    (blocks, dropped)
}

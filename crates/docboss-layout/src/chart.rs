//! Charts drawn from their cached values: the chart and plot areas, the
//! title, the legend, axes with tick labels and gridlines, and bars,
//! lines, areas, pie slices and scatter points in the series' colors.

use docboss_font::Seg;
use docboss_model::{
    Chart, ChartAxis, ChartFrame, ChartGrouping, ChartKind, ChartPlot, ChartSeries, ChartSide,
    ChartText, Color, LineCap, LineJoin,
};

use crate::flow::Ctx;
use crate::label;
use crate::shape::RunStyle;
use crate::units::emu_to_pt;
use crate::{Item, LineStyle, Rect, Stroke};

/// The most tick marks an axis draws.
const MAX_TICKS: usize = 200;
/// The most categories or points drawn per series.
const MAX_POINTS: usize = 4_096;

fn text_style(text: &ChartText) -> RunStyle {
    label::style(
        text.font.as_deref(),
        text.size as f32 / 100.0,
        text.bold,
        false,
        text.color,
    )
}

fn stroke(color: Color, width: f32) -> Stroke {
    Stroke {
        width: width.max(0.25),
        color,
        style: LineStyle::Solid,
        cap: LineCap::Round,
        join: LineJoin::Round,
    }
}

fn line(items: &mut Vec<Item>, from: (f32, f32), to: (f32, f32), color: Color, width: f32) {
    items.push(Item::Line {
        from,
        to,
        width,
        color,
        style: LineStyle::Solid,
        cap: LineCap::Flat,
    });
}

fn rectangle(rect: Rect) -> Vec<Seg> {
    vec![
        Seg::Move(rect.x, rect.y),
        Seg::Line(rect.right(), rect.y),
        Seg::Line(rect.right(), rect.bottom()),
        Seg::Line(rect.x, rect.bottom()),
        Seg::Close,
    ]
}

/// Cubic pieces of the circular arc about `(cx, cy)` of radius `r` from
/// angle `from` to `to` (radians, clockwise on the page from the right),
/// continuing the current path.
fn arc(segs: &mut Vec<Seg>, (cx, cy): (f32, f32), r: f32, from: f32, to: f32) {
    let sweep = to - from;
    let pieces = (sweep.abs() / std::f32::consts::FRAC_PI_2)
        .ceil()
        .clamp(1.0, 16.0) as usize;
    let step = sweep / pieces as f32;
    let k = 4.0 / 3.0 * (step / 4.0).tan();
    for i in 0..pieces {
        let a0 = from + step * i as f32;
        let a1 = a0 + step;
        let (s0, c0) = a0.sin_cos();
        let (s1, c1) = a1.sin_cos();
        segs.push(Seg::Cubic(
            cx + r * (c0 - k * s0),
            cy + r * (s0 + k * c0),
            cx + r * (c1 + k * s1),
            cy + r * (s1 - k * c1),
            cx + r * c1,
            cy + r * s1,
        ));
    }
}

fn circle(center: (f32, f32), r: f32) -> Vec<Seg> {
    let mut segs = vec![Seg::Move(center.0 + r, center.1)];
    arc(&mut segs, center, r, 0.0, std::f32::consts::TAU);
    segs.push(Seg::Close);
    segs
}

/// A value scale: its bounds and the step between tick marks.
#[derive(Debug, Clone, Copy)]
struct Scale {
    low: f64,
    high: f64,
    step: f64,
    reversed: bool,
    /// The values the scale was made for.
    data: (f64, f64),
}

impl Scale {
    /// The scale an axis gives values from `low` to `high`: its stated
    /// bounds and unit, else rounded bounds with a step of 1, 2 or 5 times
    /// a power of ten, as spreadsheets pick them.
    fn new(axis: Option<&ChartAxis>, low: f64, high: f64, zero: bool) -> Scale {
        Scale::fitted(axis, low, high, zero, 5.0)
    }

    /// [`Scale::new`] with about `intervals` steps when the axis states
    /// no unit.
    fn fitted(axis: Option<&ChartAxis>, low: f64, high: f64, zero: bool, intervals: f64) -> Scale {
        let data = (low, high);
        let (mut low, mut high) = match zero {
            true => (low.min(0.0), high.max(0.0)),
            false => (low, high),
        };
        if !low.is_finite() || !high.is_finite() {
            (low, high) = (0.0, 1.0);
        }
        let stated_low = axis.and_then(|a| a.min);
        let stated_high = axis.and_then(|a| a.max);
        low = stated_low.unwrap_or(low);
        high = stated_high.unwrap_or(high);
        if high <= low {
            high = low + 1.0;
        }
        let step = axis
            .and_then(|a| a.major_unit)
            .filter(|u| (high - low) / u < MAX_TICKS as f64)
            .unwrap_or_else(|| nice_step((high - low) / intervals.max(1.0)));
        if stated_high.is_none() {
            let headroom = high + (high - low) * 0.05;
            high = (headroom / step).ceil() * step;
        }
        if stated_low.is_none() && low < 0.0 {
            let room = low - (high - low) * 0.05;
            low = (room / step).floor() * step;
        }
        if stated_low.is_none() && !zero {
            low = (low / step).floor() * step;
        }
        Scale {
            low,
            high,
            step,
            reversed: axis.is_some_and(|a| a.reversed),
            data,
        }
    }

    /// Where `value` falls between `start` and `end`.
    fn at(&self, value: f64, start: f32, end: f32) -> f32 {
        let t = ((value - self.low) / (self.high - self.low)).clamp(-0.5, 1.5) as f32;
        let t = if self.reversed { 1.0 - t } else { t };
        start + (end - start) * t
    }

    fn ticks(&self) -> Vec<f64> {
        let count = ((self.high - self.low) / self.step)
            .round()
            .clamp(0.0, MAX_TICKS as f64) as usize;
        (0..=count)
            .map(|i| self.low + self.step * i as f64)
            .collect()
    }
}

fn nice_step(raw: f64) -> f64 {
    if !raw.is_finite() || raw <= 0.0 {
        return 1.0;
    }
    let power = 10f64.powf(raw.log10().floor());
    let unit = [1.0, 2.0, 5.0, 10.0]
        .into_iter()
        .find(|m| m * power >= raw * 0.999)
        .unwrap_or(10.0);
    unit * power
}

/// A tick value in an axis number format: percentages, a fixed count of
/// decimals, thousands separators, or the shortest form for `General`.
fn number(value: f64, format: Option<&str>) -> String {
    let format = format.unwrap_or("General");
    let percent = format.contains('%');
    let value = if percent { value * 100.0 } else { value };
    let body = format.split(';').next().unwrap_or("");
    let decimals = body
        .split_once('.')
        .map(|(_, rest)| rest.chars().take_while(|c| *c == '0' || *c == '#').count());
    let grouped = body.contains(',');
    let mut text = match decimals {
        Some(places) if body != "General" => format!("{value:.places$}"),
        _ if body.contains('0') && body != "General" => format!("{value:.0}"),
        _ => {
            let rounded = format!("{value:.9}");
            rounded
                .trim_end_matches('0')
                .trim_end_matches('.')
                .to_string()
        }
    };
    if text == "-0" {
        text = "0".into();
    }
    if grouped {
        text = group_thousands(&text);
    }
    if percent {
        text.push('%');
    }
    text
}

fn group_thousands(text: &str) -> String {
    let (sign, rest) = match text.strip_prefix('-') {
        Some(rest) => ("-", rest),
        None => ("", text),
    };
    let (whole, fraction) = rest
        .split_once('.')
        .map_or((rest, None), |(w, f)| (w, Some(f)));
    let mut out = String::with_capacity(text.len() + whole.len() / 3);
    out.push_str(sign);
    for (i, c) in whole.chars().enumerate() {
        if i > 0 && (whole.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    if let Some(fraction) = fraction {
        out.push('.');
        out.push_str(fraction);
    }
    out
}

/// A legend entry: its label, its color and whether its key is a line.
struct Entry {
    text: String,
    color: Color,
    line: bool,
}

fn entries(chart: &Chart) -> Vec<Entry> {
    let single = chart.plots.len() == 1 && chart.plots[0].series.len() == 1;
    let mut out = Vec::new();
    for plot in &chart.plots {
        let pie = matches!(plot.kind, ChartKind::Pie { .. });
        let by_point = pie || (single && !plot.series[0].point_fills.is_empty());
        let line_key = matches!(
            plot.kind,
            ChartKind::Line { .. } | ChartKind::Scatter { lines: true }
        );
        if by_point {
            if let Some(series) = plot.series.first() {
                for (i, name) in series.categories.iter().enumerate().take(MAX_POINTS) {
                    let fill = series.point_fills.get(i).copied().flatten();
                    out.push(Entry {
                        text: name.clone(),
                        color: fill.or(series.fill).unwrap_or(Color::BLACK),
                        line: false,
                    });
                }
            }
            continue;
        }
        for (i, series) in plot.series.iter().enumerate() {
            let color = match line_key {
                true => series.line.or(series.fill),
                false => series.fill.or(series.line),
            };
            out.push(Entry {
                text: series
                    .name
                    .clone()
                    .unwrap_or_else(|| format!("Series{}", i + 1)),
                color: color.unwrap_or(Color::BLACK),
                line: line_key,
            });
        }
    }
    out
}

/// The length of an entry's key: a square, or a line two and a half
/// squares long.
fn key_width(entry: &Entry, key: f32) -> f32 {
    match entry.line {
        true => key * 2.5,
        false => key,
    }
}

/// Draws the legend at `side` inside `area` and returns the area left
/// for the plot.
fn legend(ctx: &mut Ctx<'_>, chart: &Chart, area: Rect, items: &mut Vec<Item>) -> Rect {
    let Some(legend) = &chart.legend else {
        return area;
    };
    let entries = entries(chart);
    if entries.is_empty() {
        return area;
    }
    let style = text_style(&legend.text);
    let (ascent, descent) = label::extent(ctx, &style);
    let height = ascent + descent;
    let key = style.size * 0.6;
    let gap = style.size * 0.4;
    let widths: Vec<f32> = entries
        .iter()
        .map(|e| key_width(e, key) + gap + label::width(ctx, &e.text, &style))
        .collect();
    let draw_entry = |ctx: &mut Ctx<'_>, items: &mut Vec<Item>, entry: &Entry, x: f32, top: f32| {
        let middle = top + height / 2.0;
        match entry.line {
            true => line(
                items,
                (x, middle),
                (x + key_width(entry, key), middle),
                entry.color,
                1.5,
            ),
            false => items.push(Item::Rect {
                rect: Rect::new(x, middle - key / 2.0, key, key),
                color: entry.color,
            }),
        }
        let text_x = x + key_width(entry, key) + gap;
        label::draw(ctx, items, &entry.text, &style, text_x, top + ascent);
    };
    match legend.side {
        ChartSide::Right | ChartSide::Left => {
            let width = widths
                .iter()
                .copied()
                .fold(0.0, f32::max)
                .min(area.width * 0.5);
            let step = height * 1.25;
            let total = step * entries.len() as f32;
            let top = area.y + ((area.height - total) / 2.0).max(0.0);
            let x = match legend.side {
                ChartSide::Right => area.right() - width,
                _ => area.x,
            };
            for (i, entry) in entries.iter().enumerate() {
                let y = top + step * i as f32;
                if y + height > area.bottom() {
                    break;
                }
                draw_entry(ctx, items, entry, x, y + (step - height) / 2.0);
            }
            let room = width + gap * 2.0;
            match legend.side {
                ChartSide::Right => {
                    Rect::new(area.x, area.y, (area.width - room).max(1.0), area.height)
                }
                _ => Rect::new(
                    area.x + room,
                    area.y,
                    (area.width - room).max(1.0),
                    area.height,
                ),
            }
        }
        ChartSide::Bottom | ChartSide::Top => {
            let spacing = gap * 2.5;
            let total: f32 = widths.iter().sum::<f32>() + spacing * (widths.len() - 1) as f32;
            let mut x = area.x + ((area.width - total) / 2.0).max(0.0);
            let y = match legend.side {
                ChartSide::Bottom => area.bottom() - height,
                _ => area.y,
            };
            for (entry, width) in entries.iter().zip(&widths) {
                if x + width > area.right() + 0.5 {
                    break;
                }
                draw_entry(ctx, items, entry, x, y);
                x += width + spacing;
            }
            let room = height + gap * 1.5;
            match legend.side {
                ChartSide::Bottom => {
                    Rect::new(area.x, area.y, area.width, (area.height - room).max(1.0))
                }
                _ => Rect::new(
                    area.x,
                    area.y + room,
                    area.width,
                    (area.height - room).max(1.0),
                ),
            }
        }
    }
}

/// Lays out a chart `width` by `height` points (ECMA-376 Part 1 §21.2.2.27):
/// the chart area, the title at the top, the legend at its side, and the
/// plot area with its plots.
pub(crate) fn chart_items(ctx: &mut Ctx<'_>, chart: &Chart, width: f32, height: f32) -> Vec<Item> {
    let mut items = Vec::new();
    let frame = Rect::new(0.0, 0.0, width, height);
    if let Some(color) = chart.area.fill {
        items.push(Item::Rect { rect: frame, color });
    }
    let pad = (width.min(height) * 0.04).clamp(3.0, 10.0);
    let mut area = Rect::new(
        pad,
        pad,
        (width - 2.0 * pad).max(1.0),
        (height - 2.0 * pad).max(1.0),
    );
    if let Some(title) = chart.title.as_ref().filter(|t| !t.text.trim().is_empty()) {
        let style = text_style(title);
        let (ascent, descent) = label::extent(ctx, &style);
        let mut y = area.y;
        for part in title.text.lines() {
            let w = label::width(ctx, part, &style);
            label::draw(
                ctx,
                &mut items,
                part,
                &style,
                area.x + (area.width - w) / 2.0,
                y + ascent,
            );
            y += ascent + descent;
        }
        let used = y - area.y + style.size * 0.4;
        area = Rect::new(
            area.x,
            area.y + used,
            area.width,
            (area.height - used).max(1.0),
        );
    }
    let area = legend(ctx, chart, area, &mut items);
    plot_area(ctx, chart, area, &mut items);
    outline(&chart.area, frame, &mut items);
    items
}

/// The outline of a chart or plot area, in its width and dashes.
fn outline(frame: &ChartFrame, rect: Rect, items: &mut Vec<Item>) {
    let Some(color) = frame.outline else {
        return;
    };
    items.push(Item::Outline {
        rect,
        width: frame.outline_width.map_or(0.75, emu_to_pt).max(0.25),
        color,
        style: frame.outline_dash.map_or(LineStyle::Solid, LineStyle::Dash),
        cap: LineCap::Flat,
        join: LineJoin::Miter,
    });
}

/// The values a plot spans, with its series stacked when it stacks them.
fn value_range(plot: &ChartPlot) -> (f64, f64) {
    let (grouping, stacks) = match plot.kind {
        ChartKind::Bar { grouping, .. }
        | ChartKind::Line { grouping }
        | ChartKind::Area { grouping } => (grouping, grouping != ChartGrouping::Standard),
        _ => (ChartGrouping::Standard, false),
    };
    if grouping == ChartGrouping::PercentStacked {
        let negative = plot
            .series
            .iter()
            .flat_map(|s| &s.values)
            .flatten()
            .any(|v| *v < 0.0);
        return (if negative { -1.0 } else { 0.0 }, 1.0);
    }
    let mut low = f64::INFINITY;
    let mut high = f64::NEG_INFINITY;
    if !stacks {
        for v in plot.series.iter().flat_map(|s| &s.values).flatten() {
            low = low.min(*v);
            high = high.max(*v);
        }
        return (low, high);
    }
    let count = plot
        .series
        .iter()
        .map(|s| s.values.len())
        .max()
        .unwrap_or(0)
        .min(MAX_POINTS);
    for i in 0..count {
        let (mut up, mut down) = (0.0f64, 0.0f64);
        for s in &plot.series {
            let v = s.values.get(i).copied().flatten().unwrap_or(0.0);
            if v >= 0.0 {
                up += v;
            } else {
                down += v;
            }
        }
        low = low.min(down);
        high = high.max(up);
    }
    (low, high)
}

fn category_count(plot: &ChartPlot) -> usize {
    plot.series
        .iter()
        .map(|s| s.values.len().max(s.categories.len()))
        .max()
        .unwrap_or(0)
        .min(MAX_POINTS)
}

fn category_labels(chart: &Chart, count: usize) -> Vec<String> {
    let first = chart
        .plots
        .iter()
        .flat_map(|p| &p.series)
        .find(|s| !s.categories.is_empty());
    (0..count)
        .map(|i| {
            first
                .and_then(|s| s.categories.get(i))
                .filter(|c| !c.is_empty())
                .cloned()
                .unwrap_or_else(|| (i + 1).to_string())
        })
        .collect()
}

fn shown(axis: Option<&ChartAxis>) -> Option<&ChartAxis> {
    axis.filter(|a| !a.deleted)
}

/// The plot area inside `area`: tick labels outside it, then the plot
/// area's fill, gridlines, plots, axis lines and outline.
fn plot_area(ctx: &mut Ctx<'_>, chart: &Chart, area: Rect, items: &mut Vec<Item>) {
    let Some(first) = chart.plots.first() else {
        return;
    };
    if let ChartKind::Pie { hole, first_angle } = first.kind {
        pie(first, hole, first_angle, area, items);
        return;
    }
    let horizontal = matches!(
        first.kind,
        ChartKind::Bar {
            horizontal: true,
            ..
        }
    );
    let scatter = matches!(first.kind, ChartKind::Scatter { .. });
    let (mut low, mut high) = (f64::INFINITY, f64::NEG_INFINITY);
    for plot in &chart.plots {
        let (l, h) = value_range(plot);
        low = low.min(l);
        high = high.max(h);
    }
    let values = Scale::new(chart.value_axis.as_ref(), low, high, true);
    let count = chart
        .plots
        .iter()
        .map(category_count)
        .max()
        .unwrap_or(0)
        .max(1);
    let x_scale = scatter.then(|| {
        let xs = chart
            .plots
            .iter()
            .flat_map(|p| &p.series)
            .flat_map(|s| &s.x_values)
            .flatten();
        let (lo, hi) = xs.fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| {
            (lo.min(*v), hi.max(*v))
        });
        let (lo, hi) = match lo.is_finite() {
            true => (lo, hi),
            false => (1.0, count as f64),
        };
        Scale::new(chart.category_axis.as_ref(), lo, hi, false)
    });
    let value_axis = shown(chart.value_axis.as_ref());
    let category_axis = shown(chart.category_axis.as_ref());
    let mut values = values;
    let mut x_scale = x_scale;
    let mut laid = sides(
        ctx,
        chart,
        area,
        &values,
        x_scale.as_ref(),
        count,
        horizontal,
    );
    let value_len = match horizontal {
        false => laid.plot.height,
        true => laid.plot.width,
    };
    let value_room = match horizontal {
        false => laid.value_h * 1.8,
        true => laid.value_w * 1.6,
    };
    let refit_values = chart
        .value_axis
        .as_ref()
        .is_none_or(|a| a.major_unit.is_none());
    if refit_values && value_room > 0.0 {
        let intervals = f64::from((value_len / value_room).clamp(2.0, 10.0));
        values = Scale::fitted(chart.value_axis.as_ref(), low, high, true, intervals);
    }
    if let Some(scale) = x_scale.filter(|_| laid.category_w > 0.0) {
        let intervals = f64::from((laid.plot.width / (laid.category_w * 1.6)).clamp(2.0, 10.0));
        x_scale = Some(Scale::fitted(
            chart.category_axis.as_ref(),
            scale.data.0,
            scale.data.1,
            false,
            intervals,
        ));
    }
    laid = sides(
        ctx,
        chart,
        area,
        &values,
        x_scale.as_ref(),
        count,
        horizontal,
    );
    let Sides {
        value_labels,
        categories,
        value_style,
        category_style,
        category_w,
        gap,
        plot,
        ..
    } = laid;
    if let Some(color) = chart.plot_area.fill {
        items.push(Item::Rect { rect: plot, color });
    }
    let value_at = |v: f64| match horizontal {
        false => values.at(v, plot.bottom(), plot.y),
        true => values.at(v, plot.x, plot.right()),
    };
    if let Some(color) = value_axis.and_then(|a| a.gridlines) {
        for v in values.ticks() {
            let p = value_at(v);
            match horizontal {
                false => line(items, (plot.x, p), (plot.right(), p), color, 0.75),
                true => line(items, (p, plot.y), (p, plot.bottom()), color, 0.75),
            }
        }
    }
    if let (Some(color), Some(scale)) = (category_axis.and_then(|a| a.gridlines), x_scale) {
        for v in scale.ticks() {
            let p = scale.at(v, plot.x, plot.right());
            line(items, (p, plot.y), (p, plot.bottom()), color, 0.75);
        }
    }
    let reversed = chart.category_axis.as_ref().is_some_and(|a| a.reversed);
    let slot = |i: usize| -> (f32, f32) {
        let i = if reversed {
            count - 1 - i.min(count - 1)
        } else {
            i
        };
        match horizontal {
            false => {
                let w = plot.width / count as f32;
                (plot.x + w * i as f32, w)
            }
            true => {
                let h = plot.height / count as f32;
                (plot.bottom() - h * (i + 1) as f32, h)
            }
        }
    };
    let frame = Frame {
        plot,
        horizontal,
        values,
        x_scale,
        count,
    };
    for p in &chart.plots {
        match p.kind {
            ChartKind::Bar {
                grouping,
                gap,
                overlap,
                ..
            } => bars(p, grouping, gap, overlap, &frame, &slot, items),
            ChartKind::Line { grouping } => lines(p, grouping, &frame, &slot, items),
            ChartKind::Area { grouping } => areas(p, grouping, &frame, &slot, items),
            ChartKind::Scatter { .. } => scatter_points(p, &frame, items),
            ChartKind::Pie { .. } => {}
        }
    }
    if let Some(style) = &value_style {
        let (ascent, descent) = label::extent(ctx, style);
        for (v, text) in values.ticks().iter().zip(&value_labels) {
            let p = value_at(*v);
            let w = label::width(ctx, text, style);
            match horizontal {
                false => label::draw(
                    ctx,
                    items,
                    text,
                    style,
                    plot.x - gap - w,
                    p + (ascent - descent) / 2.0,
                ),
                true => label::draw(
                    ctx,
                    items,
                    text,
                    style,
                    p - w / 2.0,
                    plot.bottom() + gap + ascent,
                ),
            };
        }
    }
    if let Some(style) = &category_style {
        let (ascent, descent) = label::extent(ctx, style);
        let every = match (horizontal, x_scale) {
            (false, None) => {
                let room = plot.width / count as f32;
                ((category_w + gap) / room.max(0.1)).ceil().max(1.0) as usize
            }
            _ => 1,
        };
        for (i, text) in categories.iter().enumerate() {
            if i % every != 0 {
                continue;
            }
            let w = label::width(ctx, text, style);
            if let Some(scale) = x_scale {
                let Some(v) = scale.ticks().get(i).copied() else {
                    continue;
                };
                let p = scale.at(v, plot.x, plot.right());
                label::draw(
                    ctx,
                    items,
                    text,
                    style,
                    p - w / 2.0,
                    plot.bottom() + gap + ascent,
                );
                continue;
            }
            let (start, size) = slot(i);
            match horizontal {
                false => label::draw(
                    ctx,
                    items,
                    text,
                    style,
                    start + (size - w) / 2.0,
                    plot.bottom() + gap + ascent,
                ),
                true => label::draw(
                    ctx,
                    items,
                    text,
                    style,
                    plot.x - gap - w,
                    start + size / 2.0 + (ascent - descent) / 2.0,
                ),
            };
        }
    }
    if let Some(color) = category_axis.and_then(|a| a.line) {
        let zero = value_at(0.0_f64.clamp(values.low, values.high));
        match (horizontal, x_scale.is_some()) {
            (false, false) => line(items, (plot.x, zero), (plot.right(), zero), color, 0.75),
            (true, _) => line(items, (zero, plot.y), (zero, plot.bottom()), color, 0.75),
            (false, true) => line(
                items,
                (plot.x, plot.bottom()),
                (plot.right(), plot.bottom()),
                color,
                0.75,
            ),
        }
    }
    if let Some(color) = value_axis.and_then(|a| a.line) {
        match horizontal {
            false => line(
                items,
                (plot.x, plot.y),
                (plot.x, plot.bottom()),
                color,
                0.75,
            ),
            true => line(
                items,
                (plot.x, plot.bottom()),
                (plot.right(), plot.bottom()),
                color,
                0.75,
            ),
        }
    }
    outline(&chart.plot_area, plot, items);
}

/// The tick labels of a plot's axes, their styles and sizes, and the plot
/// rectangle they leave inside `area`.
struct Sides {
    value_labels: Vec<String>,
    categories: Vec<String>,
    value_style: Option<RunStyle>,
    category_style: Option<RunStyle>,
    value_w: f32,
    value_h: f32,
    category_w: f32,
    gap: f32,
    plot: Rect,
}

fn sides(
    ctx: &mut Ctx<'_>,
    chart: &Chart,
    area: Rect,
    values: &Scale,
    x_scale: Option<&Scale>,
    count: usize,
    horizontal: bool,
) -> Sides {
    let value_axis = shown(chart.value_axis.as_ref());
    let category_axis = shown(chart.category_axis.as_ref());
    let value_style = value_axis.map(|a| text_style(&a.text));
    let category_style = category_axis.map(|a| text_style(&a.text));
    let value_labels: Vec<String> = match value_axis.filter(|a| a.labels) {
        Some(axis) => values
            .ticks()
            .iter()
            .map(|v| number(*v, axis.format.as_deref()))
            .collect(),
        None => Vec::new(),
    };
    let categories: Vec<String> = match (category_axis.filter(|a| a.labels), x_scale) {
        (Some(axis), Some(scale)) => scale
            .ticks()
            .iter()
            .map(|v| number(*v, axis.format.as_deref()))
            .collect(),
        (Some(_), None) => category_labels(chart, count),
        (None, _) => Vec::new(),
    };
    let measure = |ctx: &mut Ctx<'_>, texts: &[String], style: Option<&RunStyle>| -> (f32, f32) {
        let Some(style) = style else {
            return (0.0, 0.0);
        };
        if texts.is_empty() {
            return (0.0, 0.0);
        }
        let (a, d) = label::extent(ctx, style);
        let w = texts
            .iter()
            .map(|t| label::width(ctx, t, style))
            .fold(0.0, f32::max);
        (w, a + d)
    };
    let (value_w, value_h) = measure(ctx, &value_labels, value_style.as_ref());
    let (category_w, category_h) = measure(ctx, &categories, category_style.as_ref());
    let gap = value_style
        .as_ref()
        .or(category_style.as_ref())
        .map_or(3.0, |s| s.size * 0.3);
    let (left, bottom) = match horizontal {
        false => (value_w, category_h),
        true => (category_w, value_h),
    };
    let left = if left > 0.0 { left + gap } else { 0.0 };
    let bottom = if bottom > 0.0 { bottom + gap } else { 0.0 };
    let top_room = (value_h.max(category_h) / 2.0).min(area.height * 0.1);
    let plot = Rect::new(
        area.x + left,
        area.y + top_room,
        (area.width - left).max(1.0),
        (area.height - bottom - top_room).max(1.0),
    );
    Sides {
        value_labels,
        categories,
        value_style,
        category_style,
        value_w,
        value_h,
        category_w,
        gap,
        plot,
    }
}

/// Where a plot's values fall.
struct Frame {
    plot: Rect,
    horizontal: bool,
    values: Scale,
    x_scale: Option<Scale>,
    count: usize,
}

impl Frame {
    fn value(&self, v: f64) -> f32 {
        match self.horizontal {
            false => self.values.at(v, self.plot.bottom(), self.plot.y),
            true => self.values.at(v, self.plot.x, self.plot.right()),
        }
    }

    /// The base values of a stacked plot are clamped to the scale, so bars
    /// start at its low end when zero lies below it.
    fn base(&self) -> f64 {
        0.0_f64.clamp(self.values.low, self.values.high)
    }
}

/// Each category's values of every series, scaled to the category's total
/// for a percent stacked plot.
fn category_values(plot: &ChartPlot, grouping: ChartGrouping, i: usize) -> Vec<f64> {
    let raw: Vec<f64> = plot
        .series
        .iter()
        .map(|s| s.values.get(i).copied().flatten().unwrap_or(0.0))
        .collect();
    if grouping != ChartGrouping::PercentStacked {
        return raw;
    }
    let total: f64 = raw.iter().map(|v| v.abs()).sum();
    if total <= 0.0 {
        return raw.iter().map(|_| 0.0).collect();
    }
    raw.iter().map(|v| v / total).collect()
}

fn point_fill(series: &ChartSeries, i: usize) -> Option<Color> {
    series.point_fills.get(i).copied().flatten().or(series.fill)
}

/// Bars (ECMA-376 Part 1 §21.2.2.16): side by side with `gap` percent of
/// a bar between categories and `overlap` percent within one, or stacked.
fn bars(
    plot: &ChartPlot,
    grouping: ChartGrouping,
    gap: u16,
    overlap: i16,
    frame: &Frame,
    slot: &dyn Fn(usize) -> (f32, f32),
    items: &mut Vec<Item>,
) {
    let n = plot.series.len().max(1) as f32;
    let stacked = grouping != ChartGrouping::Standard;
    let ov = f32::from(overlap) / 100.0;
    let gap = f32::from(gap) / 100.0;
    for i in 0..frame.count {
        let (start, size) = slot(i);
        let values = category_values(plot, grouping, i);
        let (thick, step, first) = match stacked {
            true => {
                let thick = size / (1.0 + gap);
                (thick, 0.0, start + (size - thick) / 2.0)
            }
            false => {
                let thick = size / (n - (n - 1.0) * ov + gap);
                let group = thick * (n - (n - 1.0) * ov);
                (thick, thick * (1.0 - ov), start + (size - group) / 2.0)
            }
        };
        let (mut up, mut down) = (0.0f64, 0.0f64);
        for (j, (series, value)) in plot.series.iter().zip(&values).enumerate() {
            if series.values.get(i).copied().flatten().is_none() {
                continue;
            }
            let (from, to) = match stacked {
                true if *value >= 0.0 => {
                    up += value;
                    (up - value, up)
                }
                true => {
                    down += value;
                    (down - value, down)
                }
                false => (frame.base(), *value),
            };
            let (a, b) = (frame.value(from), frame.value(to));
            let offset = match stacked {
                true => first,
                false => first + step * j as f32,
            };
            let rect = match frame.horizontal {
                false => Rect::new(offset, a.min(b), thick, (a - b).abs()),
                true => {
                    let offset = start + size - (offset - start) - thick;
                    Rect::new(a.min(b), offset, (a - b).abs(), thick)
                }
            };
            if let Some(color) = point_fill(series, i) {
                items.push(Item::Rect { rect, color });
            }
            if let Some(color) = series.line {
                let width = series.line_width.map_or(0.75, emu_to_pt);
                items.push(Item::Path {
                    segs: rectangle(rect),
                    fill: None,
                    stroke: Some(stroke(color, width)),
                });
            }
        }
    }
}

/// The page points of a line or area series, stacked on `bases`, which
/// it updates.
fn series_points(
    series: &ChartSeries,
    plot: &ChartPlot,
    grouping: ChartGrouping,
    index: usize,
    frame: &Frame,
    slot: &dyn Fn(usize) -> (f32, f32),
    bases: &mut [f64],
) -> Vec<Option<(f32, f32)>> {
    (0..frame.count)
        .map(|i| {
            let own = series.values.get(i).copied().flatten()?;
            let value = match grouping {
                ChartGrouping::Standard => own,
                _ => category_values(plot, grouping, i)
                    .get(index)
                    .copied()
                    .unwrap_or(0.0),
            };
            let stacked = match grouping {
                ChartGrouping::Standard => value,
                _ => {
                    bases[i] += value;
                    bases[i]
                }
            };
            let (start, size) = slot(i);
            Some((start + size / 2.0, frame.value(stacked)))
        })
        .collect()
}

fn marker(items: &mut Vec<Item>, at: (f32, f32), fill: Color, outline: Option<Color>) {
    items.push(Item::Path {
        segs: circle(at, 2.5),
        fill: Some(fill),
        stroke: outline.map(|c| stroke(c, 0.75)),
    });
}

/// Lines (ECMA-376 Part 1 §21.2.2.97): each series' points joined in
/// category order, with markers.
fn lines(
    plot: &ChartPlot,
    grouping: ChartGrouping,
    frame: &Frame,
    slot: &dyn Fn(usize) -> (f32, f32),
    items: &mut Vec<Item>,
) {
    let mut bases = vec![0.0; frame.count];
    for (index, series) in plot.series.iter().enumerate() {
        let points = series_points(series, plot, grouping, index, frame, slot, &mut bases);
        polyline(series, &points, items);
    }
}

fn polyline(series: &ChartSeries, points: &[Option<(f32, f32)>], items: &mut Vec<Item>) {
    if let Some(color) = series.line {
        let mut segs = Vec::new();
        let mut open = false;
        for point in points {
            match point {
                Some((x, y)) if open => segs.push(Seg::Line(*x, *y)),
                Some((x, y)) => {
                    segs.push(Seg::Move(*x, *y));
                    open = true;
                }
                None => open = false,
            }
        }
        let width = series.line_width.map_or(2.25, emu_to_pt);
        items.push(Item::Path {
            segs,
            fill: None,
            stroke: Some(stroke(color, width)),
        });
    }
    if !series.markers {
        return;
    }
    let fill = series.fill.or(series.line).unwrap_or(Color::BLACK);
    for point in points.iter().flatten() {
        marker(items, *point, fill, series.line);
    }
}

/// Areas (ECMA-376 Part 1 §21.2.2.5): each series filled down to the axis
/// or to the series below it.
fn areas(
    plot: &ChartPlot,
    grouping: ChartGrouping,
    frame: &Frame,
    slot: &dyn Fn(usize) -> (f32, f32),
    items: &mut Vec<Item>,
) {
    let mut bases = vec![0.0; frame.count];
    for (index, series) in plot.series.iter().enumerate() {
        let below: Vec<f32> = bases
            .iter()
            .map(|b| match grouping {
                ChartGrouping::Standard => frame.value(frame.base()),
                _ => frame.value(*b),
            })
            .collect();
        let points = series_points(series, plot, grouping, index, frame, slot, &mut bases);
        let Some(color) = series.fill else {
            continue;
        };
        let top: Vec<(f32, f32)> = points
            .iter()
            .enumerate()
            .map(|(i, p)| {
                p.unwrap_or_else(|| {
                    let (start, size) = slot(i);
                    (start + size / 2.0, below[i])
                })
            })
            .collect();
        let (Some(first), Some(last)) = (top.first(), top.last()) else {
            continue;
        };
        let mut segs = vec![Seg::Move(first.0, below[0])];
        segs.extend(top.iter().map(|(x, y)| Seg::Line(*x, *y)));
        segs.push(Seg::Line(last.0, below[top.len() - 1]));
        for i in (0..top.len()).rev() {
            segs.push(Seg::Line(top[i].0, below[i]));
        }
        segs.push(Seg::Close);
        items.push(Item::Path {
            segs,
            fill: Some(color),
            stroke: series
                .line
                .map(|c| stroke(c, series.line_width.map_or(0.75, emu_to_pt))),
        });
    }
}

/// Scatter points (ECMA-376 Part 1 §21.2.2.161) at their x and y values,
/// joined in order when the series has a line.
fn scatter_points(plot: &ChartPlot, frame: &Frame, items: &mut Vec<Item>) {
    let Some(x_scale) = frame.x_scale else {
        return;
    };
    for series in &plot.series {
        let points: Vec<Option<(f32, f32)>> = series
            .values
            .iter()
            .enumerate()
            .take(MAX_POINTS)
            .map(|(i, y)| {
                let y = (*y)?;
                let x = match series.x_values.get(i) {
                    Some(x) => (*x)?,
                    None => (i + 1) as f64,
                };
                Some((
                    x_scale.at(x, frame.plot.x, frame.plot.right()),
                    frame.value(y),
                ))
            })
            .collect();
        polyline(series, &points, items);
    }
}

/// Pie slices (ECMA-376 Part 1 §21.2.2.141, §21.2.2.50) of the first
/// series, clockwise from `first_angle` degrees past the top, with a
/// hole of `hole` percent of the radius for a doughnut.
fn pie(plot: &ChartPlot, hole: u8, first_angle: u16, area: Rect, items: &mut Vec<Item>) {
    let Some(series) = plot.series.first() else {
        return;
    };
    let values: Vec<f64> = series
        .values
        .iter()
        .take(MAX_POINTS)
        .map(|v| v.unwrap_or(0.0).max(0.0))
        .collect();
    let total: f64 = values.iter().sum();
    if total <= 0.0 {
        return;
    }
    let r = area.width.min(area.height) / 2.0 * 0.95;
    let center = (area.x + area.width / 2.0, area.y + area.height / 2.0);
    let inner = r * f32::from(hole) / 100.0;
    let mut angle = (f32::from(first_angle) - 90.0).to_radians();
    for (i, value) in values.iter().enumerate() {
        let sweep = (*value / total) as f32 * std::f32::consts::TAU;
        if sweep <= 0.0 {
            continue;
        }
        let end = angle + sweep;
        let mut segs = Vec::new();
        match inner > 0.0 {
            true => {
                segs.push(Seg::Move(
                    center.0 + r * angle.cos(),
                    center.1 + r * angle.sin(),
                ));
                arc(&mut segs, center, r, angle, end);
                segs.push(Seg::Line(
                    center.0 + inner * end.cos(),
                    center.1 + inner * end.sin(),
                ));
                arc(&mut segs, center, inner, end, angle);
            }
            false => {
                segs.push(Seg::Move(center.0, center.1));
                segs.push(Seg::Line(
                    center.0 + r * angle.cos(),
                    center.1 + r * angle.sin(),
                ));
                arc(&mut segs, center, r, angle, end);
            }
        }
        segs.push(Seg::Close);
        let fill = point_fill(series, i);
        let outline = series
            .line
            .map(|c| stroke(c, series.line_width.map_or(0.75, emu_to_pt)));
        items.push(Item::Path {
            segs,
            fill,
            stroke: outline,
        });
        angle = end;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Automatic value scales step by 1, 2 or 5 times a power of ten and
    /// tick labels follow the axis number format.
    #[test]
    fn scales_and_tick_labels() {
        let scale = Scale::new(None, 0.0, 4.5, true);
        assert_eq!((scale.low, scale.step), (0.0, 1.0));
        assert_eq!(scale.high, 5.0);
        assert_eq!(nice_step(0.23), 0.5);
        assert_eq!(nice_step(1300.0), 2000.0);
        assert_eq!(number(0.25, Some("0%")), "25%");
        assert_eq!(number(1234.5, Some("#,##0.00")), "1,234.50");
        assert_eq!(number(2.5, None), "2.5");
        assert_eq!(number(-0.0, Some("0")), "0");
    }
}

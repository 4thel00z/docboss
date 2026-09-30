//! Table layout: grid columns, cell content, row heights, vertical merges,
//! borders and shading, and rows that split across pages.

use std::sync::Arc;

use docboss_model::{
    Border, BorderStyle, Borders, Color, DashPattern, Justification, LineCap, SourceFormat, Table,
    TableProperties, TextDirection, VerticalAlign, VerticalMerge,
};

use crate::flow::{layout_blocks, stack_height, Ctx, Floating, Slab};
use crate::units::twips_to_pt;
use crate::{Item, LineStyle, Rect};

const DEFAULT_CELL_MARGIN: i32 = 108;
/// The line length a turned cell's text is first laid out at, in points,
/// to find its longest line.
const UNWRAPPED: f32 = 1584.0;
/// The shortest stated row height, less the cell margins, that a turned
/// cell's lines wrap at, in points; below it they run unwrapped and the
/// row grows to hold them.
const MIN_TURNED_LENGTH: f32 = 6.0;
/// Word's limit on the columns of a table grid.
const MAX_COLUMNS: usize = 63;

fn span(cell: &docboss_model::TableCell) -> usize {
    (cell.span() as usize).min(MAX_COLUMNS)
}

/// A border line along an edge, or `None` for no border. ECMA-376 Part 1
/// §17.18.2 names the dashed border styles without lengths; they are drawn
/// as LibreOffice draws them, with round caps and dash and space lengths in
/// points that do not grow with the border's width.
pub(crate) fn border_line(border: &Border, from: (f32, f32), to: (f32, f32)) -> Option<Item> {
    let width = border_width(border);
    let dashes: &[(f32, f32)] = match border.style {
        BorderStyle::None | BorderStyle::Art => return None,
        BorderStyle::Dotted => &[(0.5, 1.0)],
        BorderStyle::Dashed => &[(8.0, 2.5)],
        BorderStyle::DashSmallGap => &[(3.0, 1.0)],
        BorderStyle::DotDash | BorderStyle::DashDotStroked => &[(2.5, 2.5), (8.0, 2.5)],
        BorderStyle::DotDotDash => &[(2.5, 2.5), (2.5, 2.5), (8.0, 2.5)],
        _ => &[],
    };
    let style = match border.style {
        BorderStyle::Double => LineStyle::Double,
        _ => dash_in_points(dashes, width).map_or(LineStyle::Solid, LineStyle::Dash),
    };
    let cap = match style {
        LineStyle::Dash(_) => LineCap::Round,
        _ => LineCap::Flat,
    };
    Some(Item::Line {
        from,
        to,
        width,
        color: border.color.unwrap_or(Color::BLACK),
        style,
        cap,
    })
}

/// ECMA-376 Part 1 §17.4.66 (`w:tcBorders`): the height a horizontal
/// border takes from its row, the width it is drawn with, or nothing for
/// a border that is not drawn. The border above a row is counted in it, and
/// the table's last row counts its bottom border too, as LibreOffice lays
/// Word tables out.
fn drawn_width(border: Option<&Border>) -> f32 {
    border
        .filter(|b| !matches!(b.style, BorderStyle::None | BorderStyle::Art))
        .map_or(0.0, border_width)
}

/// The width a border line is drawn with, in points.
pub(crate) fn border_width(border: &Border) -> f32 {
    let width = (border.size as f32 / 8.0).max(0.25);
    match border.style {
        BorderStyle::Thick => width * 1.5,
        _ => width,
    }
}

/// A dash pattern from dash and space lengths in points, for a line
/// `width` points wide.
fn dash_in_points(dashes: &[(f32, f32)], width: f32) -> Option<DashPattern> {
    let hundredths = |points: f32| (points / width * 100.0).round() as u32;
    let stops: Vec<(u32, u32)> = dashes
        .iter()
        .map(|(dash, space)| (hundredths(*dash), hundredths(*space)))
        .collect();
    DashPattern::new(&stops)
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Sides {
    top: Option<Border>,
    left: Option<Border>,
    bottom: Option<Border>,
    right: Option<Border>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CellPlan {
    x: f32,
    width: f32,
    margins: [f32; 4],
    content: Vec<Slab>,
    /// Spacing after the cell's last paragraph.
    trailing: f32,
    shading: Option<Color>,
    borders: Sides,
    align: Option<VerticalAlign>,
    continues: bool,
    /// For a cell that starts a vertical merge, the height of the rows it
    /// spans below its own.
    span_below: f32,
    direction: TextDirection,
    /// The length of a turned cell's lines.
    length: f32,
}

impl CellPlan {
    fn content_height(&self) -> f32 {
        let along = match self.direction {
            TextDirection::LeftToRight => stack_height(&self.content) + self.trailing,
            _ => self.length,
        };
        along + self.margins[0] + self.margins[2]
    }

    /// The map from a turned cell's text frame into the row, `height` tall
    /// (ECMA-376 Part 1 §17.4.72): bottom-to-top lines start at the cell's
    /// bottom and stack rightwards, top-to-bottom lines start at its top
    /// and stack leftwards.
    fn turn(&self, height: f32) -> Option<[f32; 6]> {
        let m = self.margins;
        match self.direction {
            TextDirection::LeftToRight => None,
            TextDirection::BottomToTop => Some([0.0, -1.0, 1.0, 0.0, self.x + m[1], height - m[2]]),
            TextDirection::TopToBottom => {
                Some([0.0, 1.0, -1.0, 0.0, self.x + self.width - m[3], m[0]])
            }
        }
    }
}

/// A laid-out row that can be painted at any height and split.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RowPlan {
    cells: Vec<CellPlan>,
}

impl RowPlan {
    /// ECMA-376 Part 1 §17.4 (`w:shd`, `w:tcBorders`): shading first, then
    /// content, then borders on every cell edge.
    pub(crate) fn paint(&self, height: f32) -> Vec<Item> {
        let mut items = Vec::new();
        for cell in self.cells.iter().filter(|c| !c.continues) {
            let h = height + cell.span_below;
            if let Some(color) = cell.shading {
                items.push(Item::Rect {
                    rect: Rect::new(cell.x, 0.0, cell.width, h),
                    color,
                });
            }
            let inner = stack_height(&cell.content);
            let turn = cell.turn(h);
            let (room, start, left) = match turn {
                Some(_) => (cell.width - cell.margins[1] - cell.margins[3], 0.0, 0.0),
                None => (
                    h - cell.margins[0] - cell.margins[2],
                    cell.margins[0],
                    cell.x + cell.margins[1],
                ),
            };
            let offset = match cell.align {
                Some(VerticalAlign::Center) => ((room - inner) / 2.0).max(0.0),
                Some(VerticalAlign::Bottom) => (room - inner).max(0.0),
                _ => 0.0,
            };
            items.extend(turn.map(Item::TransformBegin));
            let mut y = start + offset;
            for slab in &cell.content {
                y += slab.gap_before;
                items.extend(slab.items.iter().cloned().map(|mut item| {
                    item.offset(left, y);
                    item
                }));
                y += slab.height;
            }
            if turn.is_some() {
                items.push(Item::TransformEnd);
            }
        }
        for cell in &self.cells {
            let h = height + cell.span_below;
            let (x0, x1) = (cell.x, cell.x + cell.width);
            let b = cell.borders;
            let top = if cell.continues { None } else { b.top };
            let edges = [
                (top, (x0, 0.0), (x1, 0.0)),
                (
                    b.bottom
                        .filter(|_| cell.span_below == 0.0 || !cell.continues),
                    (x0, h),
                    (x1, h),
                ),
                (b.left, (x0, 0.0), (x0, h)),
                (b.right, (x1, 0.0), (x1, h)),
            ];
            items.extend(
                edges
                    .into_iter()
                    .filter_map(|(border, from, to)| border_line(&border?, from, to)),
            );
        }
        items
    }

    /// The floats of the row's cells, each placed against its cell.
    pub(crate) fn floats(&self) -> Vec<Floating> {
        let mut out = Vec::new();
        for cell in self
            .cells
            .iter()
            .filter(|c| !c.continues && c.turn(0.0).is_none())
        {
            let (x, width) = (
                cell.x + cell.margins[1],
                cell.width - cell.margins[1] - cell.margins[3],
            );
            let mut y = cell.margins[0];
            for slab in &cell.content {
                y += slab.gap_before;
                out.extend(
                    slab.floats
                        .iter()
                        .cloned()
                        .map(|float| float.nested(x, y, width.max(1.0), slab.height)),
                );
                y += slab.height;
            }
        }
        out
    }

    pub(crate) fn natural(&self) -> f32 {
        self.natural_height()
    }

    fn natural_height(&self) -> f32 {
        self.cells
            .iter()
            .filter(|c| !c.continues && c.span_below == 0.0)
            .map(CellPlan::content_height)
            .fold(0.0, f32::max)
    }

    /// Splits the row so the first part fits in `room` points: each cell
    /// keeps the slabs that fit and passes the rest on.
    pub(crate) fn split(&self, room: f32) -> Option<(RowPlan, RowPlan, f32)> {
        let fixed = |c: &CellPlan| {
            c.continues || c.span_below > 0.0 || c.direction != TextDirection::LeftToRight
        };
        if self.cells.iter().any(fixed) {
            return None;
        }
        let mut head = self.clone();
        let mut tail = self.clone();
        let mut moved = false;
        let mut kept = false;
        for (h, t) in head.cells.iter_mut().zip(tail.cells.iter_mut()) {
            let mut y = h.margins[0];
            let fits = h
                .content
                .iter()
                .take_while(|slab| {
                    y += slab.gap_before + slab.height;
                    y + h.margins[2] <= room
                })
                .count();
            kept |= fits > 0;
            moved |= fits < h.content.len();
            h.content.truncate(fits);
            t.content.drain(..fits);
            if let Some(first) = t.content.first_mut() {
                first.gap_before = 0.0;
            }
        }
        if !kept || !moved {
            return None;
        }
        let height = head.natural_height().min(room);
        Some((head, tail, height))
    }
}

fn overlay(base: Option<Borders>, over: Option<Borders>) -> Option<Borders> {
    let (Some(base), Some(over)) = (base, over) else {
        return over.or(base);
    };
    Some(Borders {
        top: over.top.or(base.top),
        left: over.left.or(base.left),
        bottom: over.bottom.or(base.bottom),
        right: over.right.or(base.right),
        inside_horizontal: over.inside_horizontal.or(base.inside_horizontal),
        inside_vertical: over.inside_vertical.or(base.inside_vertical),
    })
}

fn effective_properties(ctx: &Ctx<'_>, table: &Table) -> TableProperties {
    let styles = &ctx.doc.styles;
    let id = table.properties.style_id.as_deref();
    let chain = id.map(|id| styles.chain(id)).unwrap_or_default();
    let mut props = TableProperties::default();
    for style in chain.iter().filter_map(|s| s.table.as_ref()) {
        props.borders = overlay(props.borders, style.borders);
        props.shading = style.shading.or(props.shading);
        props.cell_margins = style.cell_margins.or(props.cell_margins);
        props.justification = style.justification.or(props.justification);
        props.indent = style.indent.or(props.indent);
        props.bidi_visual |= style.bidi_visual;
    }
    let direct = &table.properties;
    props.borders = overlay(props.borders, direct.borders);
    props.shading = direct.shading.or(props.shading);
    props.cell_margins = direct.cell_margins.or(props.cell_margins);
    props.justification = direct.justification.or(props.justification);
    props.indent = direct.indent.or(props.indent);
    props.width = direct.width;
    props.width_pct = direct.width_pct;
    props.fixed_layout = direct.fixed_layout;
    props.bidi_visual |= direct.bidi_visual;
    props.style_id = direct.style_id.clone();
    props.floating = direct.floating;
    props
}

fn grid_columns(table: &Table, props: &TableProperties, available: f32) -> Vec<f32> {
    let mut grid: Vec<f32> = table
        .grid
        .iter()
        .map(|&w| twips_to_pt(w).max(0.0))
        .collect();
    let columns = table
        .rows
        .iter()
        .map(|r| r.cells.iter().map(span).sum::<usize>().min(MAX_COLUMNS * 4))
        .max()
        .unwrap_or(0);
    if grid.len() < columns || grid.iter().sum::<f32>() <= 0.0 {
        let widths: Option<Vec<f32>> = table.rows.first().and_then(|row| {
            let spans_one = row.cells.iter().all(|c| span(c) == 1);
            let all = row
                .cells
                .iter()
                .map(|c| c.properties.width.map(twips_to_pt))
                .collect::<Option<Vec<_>>>();
            all.filter(|w| spans_one && w.len() == columns && w.iter().sum::<f32>() > 0.0)
        });
        let equal = available / columns.max(1) as f32;
        grid = widths.unwrap_or_else(|| vec![equal; columns.max(1)]);
    }
    let total: f32 = grid.iter().sum();
    let wanted = match (props.width_pct, props.width) {
        (Some(pct), _) if pct > 0 => Some(available * pct as f32 / 5000.0),
        _ => None,
    };
    let target = wanted;
    if let Some(target) = target.filter(|t| *t > 0.0 && total > 0.0) {
        grid.iter_mut().for_each(|w| *w *= target / total);
    }
    grid
}

/// The cell of a row whose first grid column is `column`.
fn cell_starting_at(
    row: &docboss_model::TableRow,
    column: usize,
) -> Option<&docboss_model::TableCell> {
    let mut at = 0usize;
    for cell in &row.cells {
        if at == column {
            return Some(cell);
        }
        at += span(cell);
        if at > column {
            return None;
        }
    }
    None
}

/// ECMA-376 Part 1 §17.4: lays out a table at `width` points as one slab
/// per row. Borders inside a vertically merged cell (§17.4.84) are not
/// drawn.
/// ECMA-376 Part 1 §17.4.1, §17.4.28: a visually right-to-left table runs
/// its cells from the right, with each cell's left and right borders and
/// margins swapped, its indent from the right margin and its alignment
/// reversed.
/// Returns the slabs with the table's left edge and width.
pub(crate) fn layout_table(ctx: &mut Ctx<'_>, table: &Table, width: f32) -> (Vec<Slab>, f32, f32) {
    let props = effective_properties(ctx, table);
    let outer_style = std::mem::replace(&mut ctx.table_style, props.style_id.clone());
    let laid = layout_table_rows(ctx, table, &props, width);
    ctx.table_style = outer_style;
    laid
}

fn layout_table_rows(
    ctx: &mut Ctx<'_>,
    table: &Table,
    props: &TableProperties,
    width: f32,
) -> (Vec<Slab>, f32, f32) {
    let grid = grid_columns(table, props, width);
    let total: f32 = grid.iter().sum();
    let offsets: Vec<f32> = grid
        .iter()
        .scan(0.0, |acc, w| {
            let x = *acc;
            *acc += w;
            Some(x)
        })
        .collect();
    let indent = props.indent.map_or(0.0, twips_to_pt);
    let rtl = props.bidi_visual;
    let default_margins =
        props
            .cell_margins
            .unwrap_or([0, DEFAULT_CELL_MARGIN, 0, DEFAULT_CELL_MARGIN]);
    let first_margin = table
        .rows
        .first()
        .and_then(|row| row.cells.first())
        .and_then(|cell| cell.properties.margins)
        .unwrap_or(default_margins)[1];
    let outdent = match ctx.doc.format {
        SourceFormat::Docx if ctx.doc.settings.compatibility_mode.unwrap_or(15) < 15 => {
            twips_to_pt(first_margin)
        }
        _ => 0.0,
    };
    let table_x = match (props.justification, rtl) {
        (Some(Justification::Center), _) => ((width - total) / 2.0).max(0.0),
        (Some(Justification::Right), false) => (width - total).max(0.0),
        (Some(Justification::Right), true) => 0.0,
        (_, true) => (width - total).max(0.0) - indent,
        (_, false) => indent - outdent,
    };
    let borders = props.borders.unwrap_or_default();
    let row_count = table.rows.len();

    let mut plans: Vec<(RowPlan, f32, bool, bool, bool)> = Vec::with_capacity(row_count);
    let mut starts: Vec<Vec<usize>> = Vec::with_capacity(row_count);
    for (r, row) in table.rows.iter().enumerate() {
        let mut column = 0usize;
        let mut cells = Vec::with_capacity(row.cells.len());
        let mut columns = Vec::with_capacity(row.cells.len());
        let cell_count = row.cells.len();
        for (c, cell) in row.cells.iter().enumerate() {
            let span = span(cell);
            let end = (column + span).min(grid.len());
            let w = grid
                .get(column..end)
                .map_or(0.0, |g| g.iter().sum::<f32>())
                .max(1.0);
            let offset = offsets.get(column).copied().unwrap_or(total);
            let x = match rtl {
                true => table_x + total - offset - w,
                false => table_x + offset,
            };
            columns.push(column);
            column += span;
            let mut m = cell
                .properties
                .margins
                .unwrap_or(default_margins)
                .map(twips_to_pt);
            if rtl {
                m.swap(1, 3);
            }
            let continues = cell.properties.vertical_merge == Some(VerticalMerge::Continue);
            let fill = cell
                .properties
                .shading
                .and_then(|s| s.fill)
                .or(props.shading.and_then(|s| s.fill));
            let direction = cell.properties.text_direction;
            let turned = direction != TextDirection::LeftToRight && !continues;
            let stated = row.properties.height.map_or(0.0, twips_to_pt) - m[0] - m[2];
            let behind = fill.or(ctx.background);
            let outer = std::mem::replace(&mut ctx.background, behind);
            let mut length = 0.0;
            let (content, trailing) = match (continues, turned) {
                (true, _) => (Vec::new(), 0.0),
                (false, false) => layout_blocks(ctx, &cell.blocks, (w - m[1] - m[3]).max(1.0)),
                (false, true) => {
                    length = match stated >= MIN_TURNED_LENGTH {
                        true => stated,
                        false => {
                            let (lines, _) = layout_blocks(ctx, &cell.blocks, UNWRAPPED);
                            let longest = lines.iter().map(|l| l.extent).fold(0.0, f32::max);
                            longest.ceil().clamp(1.0, UNWRAPPED)
                        }
                    };
                    layout_blocks(ctx, &cell.blocks, length)
                }
            };
            ctx.background = outer;
            let own = cell.properties.borders.unwrap_or_default();
            let edge_h = |outer: bool| {
                if outer {
                    None
                } else {
                    borders.inside_horizontal
                }
            };
            let merged_below = table
                .rows
                .get(r + 1)
                .and_then(|next| cell_starting_at(next, column - span))
                .is_some_and(|below| {
                    below.properties.vertical_merge == Some(VerticalMerge::Continue)
                });
            let top = own.top.or(if r == 0 { borders.top } else { edge_h(false) });
            let bottom = own.bottom.or(if r + 1 == row_count {
                borders.bottom
            } else {
                edge_h(false)
            });
            let mut sides = Sides {
                top: top.filter(|_| !continues),
                bottom: bottom.filter(|_| !merged_below),
                left: own.left.or(if c == 0 {
                    borders.left
                } else {
                    borders.inside_vertical
                }),
                right: own.right.or(if c + 1 == cell_count {
                    borders.right
                } else {
                    borders.inside_vertical
                }),
            };
            if rtl {
                std::mem::swap(&mut sides.left, &mut sides.right);
            }
            m[0] += drawn_width(sides.top.as_ref());
            if r + 1 == row_count {
                m[2] += drawn_width(sides.bottom.as_ref());
            }
            cells.push(CellPlan {
                x,
                width: w,
                margins: m,
                content,
                trailing,
                shading: fill,
                borders: sides,
                align: cell.properties.vertical_align,
                continues,
                span_below: 0.0,
                direction: if turned {
                    direction
                } else {
                    TextDirection::LeftToRight
                },
                length,
            });
        }
        let plan = RowPlan { cells };
        let rp = &row.properties;
        let natural = plan.natural_height();
        let stated = rp.height.map(twips_to_pt).unwrap_or(0.0);
        let height = if rp.height_exact && stated > 0.0 {
            stated
        } else {
            natural.max(stated)
        };
        plans.push((plan, height, rp.height_exact, rp.cant_split, rp.header));
        starts.push(columns);
    }

    for r in 0..plans.len() {
        for c in 0..plans[r].0.cells.len() {
            if table.rows[r].cells[c].properties.vertical_merge != Some(VerticalMerge::Restart) {
                continue;
            }
            let column = starts[r][c];
            let mut last = r;
            while last + 1 < plans.len() {
                let below = starts[last + 1].iter().position(|&col| col == column);
                let continues = below.is_some_and(|i| plans[last + 1].0.cells[i].continues);
                if !continues {
                    break;
                }
                last += 1;
            }
            if last == r {
                continue;
            }
            let needed = plans[r].0.cells[c].content_height();
            let spanned: f32 = plans[r..=last].iter().map(|p| p.1).sum();
            if needed > spanned && !plans[last].2 {
                plans[last].1 += needed - spanned;
            }
            let below: f32 = plans[r + 1..=last].iter().map(|p| p.1).sum();
            plans[r].0.cells[c].span_below = below;
        }
    }

    for (r, (plan, height, ..)) in plans.iter_mut().enumerate() {
        for (c, cell) in plan.cells.iter_mut().enumerate() {
            if cell.direction == TextDirection::LeftToRight {
                continue;
            }
            let full = *height + cell.span_below - cell.margins[0] - cell.margins[2];
            if (full - cell.length).abs() < 0.5 || full < 1.0 {
                continue;
            }
            let Some(source) = table.rows.get(r).and_then(|row| row.cells.get(c)) else {
                continue;
            };
            let behind = cell.shading.or(ctx.background);
            let outer = std::mem::replace(&mut ctx.background, behind);
            (cell.content, cell.trailing) = layout_blocks(ctx, &source.blocks, full);
            ctx.background = outer;
            cell.length = full;
        }
    }

    let header_rows = plans.iter().take_while(|p| p.4).count();
    let mut slabs: Vec<Slab> = Vec::with_capacity(plans.len());
    for (index, (plan, height, exact, cant_split, _)) in plans.into_iter().enumerate() {
        let mut slab = Slab::new(height, plan.paint(height));
        slab.floats = plan.floats();
        let splittable = !exact && !cant_split;
        slab.notes = plan
            .cells
            .iter()
            .flat_map(|c| c.content.iter().flat_map(|s| s.notes.iter().copied()))
            .collect();
        slab.row = splittable.then(|| Box::new(plan));
        slab.keep_with_next = index + 1 < header_rows;
        slabs.push(slab);
    }
    if header_rows > 0 && header_rows < slabs.len() {
        let header: Arc<[Slab]> = slabs[..header_rows]
            .iter()
            .cloned()
            .map(|mut s| {
                s.gap_before = 0.0;
                s.keep_with_next = false;
                s
            })
            .collect();
        slabs
            .iter_mut()
            .skip(header_rows)
            .for_each(|s| s.repeat = Some(header.clone()));
        if let Some(last_header) = slabs.get_mut(header_rows - 1) {
            last_header.keep_with_next = true;
        }
    }
    (slabs, table_x, total)
}

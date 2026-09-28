//! Table layout: grid columns, cell content, row heights, vertical merges,
//! borders and shading, and rows that split across pages.

use std::sync::Arc;

use docboss_model::{
    Border, BorderStyle, Borders, Color, Justification, Table, TableProperties, VerticalAlign,
    VerticalMerge,
};

use crate::flow::{layout_blocks, stack_height, Ctx, Slab};
use crate::units::twips_to_pt;
use crate::{Item, LineStyle, Rect};

const DEFAULT_CELL_MARGIN: i32 = 108;
/// Word's limit on the columns of a table grid.
const MAX_COLUMNS: usize = 63;

fn span(cell: &docboss_model::TableCell) -> usize {
    (cell.span() as usize).min(MAX_COLUMNS)
}

/// A border line along an edge, or `None` for no border.
pub(crate) fn border_line(border: &Border, from: (f32, f32), to: (f32, f32)) -> Option<Item> {
    let style = match border.style {
        BorderStyle::None => return None,
        BorderStyle::Dotted => LineStyle::Dotted,
        BorderStyle::Dashed => LineStyle::Dashed,
        BorderStyle::Double => LineStyle::Double,
        _ => LineStyle::Solid,
    };
    let width = (border.size as f32 / 8.0).max(0.25);
    let width = if border.style == BorderStyle::Thick {
        width * 1.5
    } else {
        width
    };
    Some(Item::Line {
        from,
        to,
        width,
        color: border.color.unwrap_or(Color::BLACK),
        style,
    })
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
}

impl CellPlan {
    fn content_height(&self) -> f32 {
        stack_height(&self.content) + self.trailing + self.margins[0] + self.margins[2]
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
            let room = h - cell.margins[0] - cell.margins[2];
            let offset = match cell.align {
                Some(VerticalAlign::Center) => ((room - inner) / 2.0).max(0.0),
                Some(VerticalAlign::Bottom) => (room - inner).max(0.0),
                _ => 0.0,
            };
            let mut y = cell.margins[0] + offset;
            for slab in &cell.content {
                y += slab.gap_before;
                items.extend(slab.items.iter().cloned().map(|mut item| {
                    item.offset(cell.x + cell.margins[1], y);
                    item
                }));
                y += slab.height;
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
        if self.cells.iter().any(|c| c.continues || c.span_below > 0.0) {
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
    props.style_id = direct.style_id.clone();
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
    let target = wanted.or((!props.fixed_layout && total > available * 1.001).then_some(available));
    if let Some(target) = target.filter(|t| *t > 0.0 && total > 0.0) {
        grid.iter_mut().for_each(|w| *w *= target / total);
    }
    grid
}

/// ECMA-376 Part 1 §17.4: lays out a table at `width` points as one slab
/// per row.
pub(crate) fn layout_table(ctx: &mut Ctx<'_>, table: &Table, width: f32) -> Vec<Slab> {
    let props = effective_properties(ctx, table);
    let grid = grid_columns(table, &props, width);
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
    let table_x = match props.justification {
        Some(Justification::Center) => ((width - total) / 2.0).max(0.0),
        Some(Justification::Right) => (width - total).max(0.0),
        _ => indent,
    };
    let default_margins =
        props
            .cell_margins
            .unwrap_or([0, DEFAULT_CELL_MARGIN, 0, DEFAULT_CELL_MARGIN]);
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
            let x = offsets.get(column).copied().unwrap_or(total) + table_x;
            let end = (column + span).min(grid.len());
            let w = grid
                .get(column..end)
                .map_or(0.0, |g| g.iter().sum::<f32>())
                .max(1.0);
            columns.push(column);
            column += span;
            let m = cell
                .properties
                .margins
                .unwrap_or(default_margins)
                .map(twips_to_pt);
            let continues = cell.properties.vertical_merge == Some(VerticalMerge::Continue);
            let (content, trailing) = if continues {
                (Vec::new(), 0.0)
            } else {
                layout_blocks(ctx, &cell.blocks, (w - m[1] - m[3]).max(1.0))
            };
            let own = cell.properties.borders.unwrap_or_default();
            let edge_h = |outer: bool| {
                if outer {
                    None
                } else {
                    borders.inside_horizontal
                }
            };
            let sides = Sides {
                top: own.top.or(if r == 0 { borders.top } else { edge_h(false) }),
                bottom: own.bottom.or(if r + 1 == row_count {
                    borders.bottom
                } else {
                    edge_h(false)
                }),
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
            cells.push(CellPlan {
                x,
                width: w,
                margins: m,
                content,
                trailing,
                shading: cell
                    .properties
                    .shading
                    .and_then(|s| s.fill)
                    .or(props.shading.and_then(|s| s.fill)),
                borders: sides,
                align: cell.properties.vertical_align,
                continues,
                span_below: 0.0,
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

    let header_rows = plans.iter().take_while(|p| p.4).count();
    let mut slabs: Vec<Slab> = Vec::with_capacity(plans.len());
    for (index, (plan, height, exact, cant_split, _)) in plans.into_iter().enumerate() {
        let mut slab = Slab::new(height, plan.paint(height));
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
    slabs
}

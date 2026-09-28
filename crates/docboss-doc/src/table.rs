//! Table row properties from a table terminating paragraph's grpprl
//! ([MS-DOC] §2.6.3, §2.4.3 Overview of Tables).

use docboss_model::{
    Borders, Justification, Shading, TableCellProperties, TableProperties, TableRowProperties,
    VerticalAlign, VerticalMerge,
};

use crate::bytes::{i16_at, u16_at, u8_at};
use crate::props::{brc, brc80, shd, shd80};
use crate::sprm::prls;

/// One TC80 ([MS-DOC] §2.9.313) with its TCGRF bits unpacked.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CellFormat {
    pub horizontal_merge: u8,
    pub vertical_merge: u8,
    pub vertical_align: u8,
    pub borders: Option<Borders>,
    pub shading: Option<Shading>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct RowInfo {
    /// Cell edges in twips: the left edge of the row, then each cell's
    /// right edge.
    pub edges: Vec<i32>,
    pub cells: Vec<CellFormat>,
    pub row: TableRowProperties,
    pub table: TableProperties,
}

fn tc80(bytes: &[u8]) -> CellFormat {
    let grf = u16_at(bytes, 0).unwrap_or(0);
    let side = |at: usize| bytes.get(at..at + 4).and_then(brc80);
    let borders = Borders {
        top: side(4),
        left: side(8),
        bottom: side(12),
        right: side(16),
        ..Borders::default()
    };
    let has_borders = borders != Borders::default();
    CellFormat {
        horizontal_merge: (grf & 3) as u8,
        vertical_merge: ((grf >> 5) & 3) as u8,
        vertical_align: ((grf >> 7) & 3) as u8,
        borders: has_borders.then_some(borders),
        shading: None,
    }
}

/// Applies an ItcFirstLim-ranged operation to cells `first..lim`.
fn cells_in(row: &mut RowInfo, operand: &[u8], apply: impl Fn(&mut CellFormat)) {
    let first = usize::from(u8_at(operand, 0).unwrap_or(0));
    let lim = usize::from(u8_at(operand, 1).unwrap_or(0)).min(row.cells.len());
    for cell in row.cells.iter_mut().take(lim).skip(first) {
        apply(cell);
    }
}

impl RowInfo {
    /// Reads the table sprms of a TTP paragraph.
    pub fn parse(grpprl: &[u8]) -> RowInfo {
        let mut row = RowInfo::default();
        for prl in prls(grpprl) {
            match prl.sprm {
                0xD608 => {
                    let operand = prl.operand.get(2..).unwrap_or(&[]);
                    let count = usize::from(u8_at(operand, 0).unwrap_or(0)).min(63);
                    row.edges = (0..=count)
                        .filter_map(|i| i16_at(operand, 1 + i * 2).map(i32::from))
                        .collect();
                    let tcs_at = 1 + (count + 1) * 2;
                    row.cells = (0..count)
                        .map(|i| {
                            operand
                                .get(tcs_at + i * 20..tcs_at + i * 20 + 20)
                                .map(tc80)
                                .unwrap_or_default()
                        })
                        .collect();
                }
                0x9407 => {
                    let value = i32::from(prl.i16());
                    row.row.height = (value != 0).then(|| value.abs());
                    row.row.height_exact = value < 0;
                }
                0x3404 => row.row.header = prl.u8() != 0,
                0x3403 | 0x3466 => row.row.cant_split = prl.u8() != 0,
                0x5400 | 0x548A => {
                    row.table.justification = Some(match prl.u16() {
                        1 => Justification::Center,
                        2 => Justification::Right,
                        _ => Justification::Left,
                    })
                }
                0x9601 => row.table.indent = Some(i32::from(prl.i16())),
                0xF614 => {
                    let unit = u8_at(prl.operand, 0).unwrap_or(0);
                    let width = i32::from(i16_at(prl.operand, 1).unwrap_or(0));
                    match unit {
                        2 => row.table.width_pct = Some(width),
                        3 => row.table.width = Some(width),
                        _ => {}
                    }
                }
                0x3615 => row.table.fixed_layout = prl.u8() == 0,
                0x563A => {}
                0xD605 => {
                    let operand = prl.variable();
                    let side = |i: usize| operand.get(i * 4..i * 4 + 4).and_then(brc80);
                    row.table.borders = Some(Borders {
                        top: side(0),
                        left: side(1),
                        bottom: side(2),
                        right: side(3),
                        inside_horizontal: side(4),
                        inside_vertical: side(5),
                    });
                }
                0xD613 => {
                    let operand = prl.variable();
                    let side = |i: usize| operand.get(i * 8..i * 8 + 8).and_then(brc);
                    row.table.borders = Some(Borders {
                        top: side(0),
                        left: side(1),
                        bottom: side(2),
                        right: side(3),
                        inside_horizontal: side(4),
                        inside_vertical: side(5),
                    });
                }
                0xD612 => {
                    let operand = prl.variable();
                    for (i, cell) in row.cells.iter_mut().enumerate() {
                        if let Some(bytes) = operand.get(i * 10..i * 10 + 10) {
                            cell.shading = shd(bytes).filter(|s| s.fill.is_some());
                        }
                    }
                }
                0xD609 => {
                    let operand = prl.variable();
                    for (i, cell) in row.cells.iter_mut().enumerate() {
                        if let Some(value) = u16_at(operand, i * 2) {
                            cell.shading = shd80(value).filter(|s| s.fill.is_some());
                        }
                    }
                }
                0xD62C => {
                    let align = u8_at(prl.variable(), 2).unwrap_or(0);
                    cells_in(&mut row, prl.variable(), |cell| cell.vertical_align = align);
                }
                0x5624 => {
                    let first = usize::from(u8_at(prl.operand, 0).unwrap_or(0));
                    let lim = usize::from(u8_at(prl.operand, 1).unwrap_or(0)).min(row.cells.len());
                    for (i, cell) in row.cells.iter_mut().enumerate().take(lim).skip(first) {
                        cell.horizontal_merge = if i == first { 2 } else { 1 };
                    }
                }
                0xD634 => {
                    let operand = prl.variable();
                    let sides = u8_at(operand, 2).unwrap_or(0);
                    let value = i32::from(u16_at(operand, 4).unwrap_or(0));
                    let margins = row.table.cell_margins.get_or_insert([0, 108, 0, 108]);
                    for (bit, slot) in [(1u8, 0usize), (2, 1), (4, 2), (8, 3)] {
                        if sides & bit != 0 {
                            margins[slot] = value;
                        }
                    }
                }
                _ => {}
            }
        }
        row
    }

    /// The model properties of cell `index`, before grid spans are known.
    pub fn cell_properties(&self, index: usize) -> TableCellProperties {
        let format = self.cells.get(index).cloned().unwrap_or_default();
        let width = match (self.edges.get(index), self.edges.get(index + 1)) {
            (Some(left), Some(right)) => Some(right - left),
            _ => None,
        };
        TableCellProperties {
            width,
            grid_span: 1,
            vertical_merge: match format.vertical_merge {
                1 => Some(VerticalMerge::Continue),
                3 | 2 => Some(VerticalMerge::Restart),
                _ => None,
            },
            borders: format.borders,
            shading: format.shading,
            vertical_align: match format.vertical_align {
                1 => Some(VerticalAlign::Center),
                2 => Some(VerticalAlign::Bottom),
                _ => None,
            },
            margins: None,
        }
    }
}

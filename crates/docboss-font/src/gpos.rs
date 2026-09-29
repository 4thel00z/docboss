//! Glyph positioning (`GPOS`): single and pair adjustments, mark to base,
//! mark to ligature and mark to mark attachment, and context and chained
//! context positioning, directly or through extension lookups (types 1,
//! 2, 4 to 9). Cursive attachment (type 3) is not applied.

use crate::bytes::{i16_at, u16_at};
use crate::otl::{
    class_of, coverage_index, match_context, Buffer, LayoutTable, CLASS_MARK, MAX_NESTING,
};

/// The position of one glyph in font units: its advance, its offset from
/// its pen position, and for an attached mark the glyph it hangs from, its
/// offset then measured from that glyph's origin.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Place {
    pub advance: i32,
    pub dx: i32,
    pub dy: i32,
    pub attach: Option<u32>,
}

/// Applies lookup `index` to every glyph whose mask meets `mask`.
pub(crate) fn apply_lookup(
    table: &LayoutTable,
    buf: &Buffer<'_>,
    places: &mut [Place],
    index: u16,
    mask: u32,
) {
    let Some(lookup) = table.lookup(index) else {
        return;
    };
    let mut i = 0;
    while i < buf.slots.len() {
        if buf.slots[i].mask & mask == 0 || buf.ignored(i, lookup.flag, lookup.mark_set) {
            i += 1;
            continue;
        }
        i = apply_at(table, buf, places, index, i, 0).unwrap_or(i + 1);
    }
}

fn apply_at(
    table: &LayoutTable,
    buf: &Buffer<'_>,
    places: &mut [Place],
    index: u16,
    i: usize,
    depth: u8,
) -> Option<usize> {
    let lookup = table.lookup(index)?;
    for &sub in &lookup.subtables {
        let next = match lookup.kind {
            1 => single(buf, places, sub, i),
            2 => pair(buf, places, sub, i, lookup.flag, lookup.mark_set),
            4 => mark_to_base(buf, places, sub, i),
            5 => mark_to_ligature(buf, places, sub, i),
            6 => mark_to_mark(buf, places, sub, i, lookup.flag, lookup.mark_set),
            7 | 8 => {
                if depth >= MAX_NESTING {
                    return None;
                }
                let found =
                    match_context(buf, sub, i, lookup.kind == 8, lookup.flag, lookup.mark_set);
                found.map(|found| {
                    for (sequence, nested) in &found.records {
                        if let Some(&at) = found.positions.get(*sequence as usize) {
                            apply_at(table, buf, places, *nested, at, depth + 1);
                        }
                    }
                    found.positions.last().map_or(i + 1, |&p| p + 1)
                })
            }
            _ => None,
        };
        if next.is_some() {
            return next;
        }
    }
    None
}

fn value_size(format: u16) -> usize {
    (format & 0xFF).count_ones() as usize * 2
}

/// Adds a value record's placement and advance to a glyph.
fn add_value(data: &[u8], record: usize, format: u16, place: &mut Place) {
    let mut at = record;
    let mut field = |bit: u16| {
        if format & bit == 0 {
            return 0;
        }
        let value = i16_at(data, at).unwrap_or(0) as i32;
        at += 2;
        value
    };
    place.dx += field(0x0001);
    place.dy += field(0x0002);
    place.advance += field(0x0004);
}

fn single(buf: &Buffer<'_>, places: &mut [Place], sub: usize, i: usize) -> Option<usize> {
    let data = buf.data;
    let format = u16_at(data, sub)?;
    let coverage = sub + u16_at(data, sub + 2)? as usize;
    let index = coverage_index(data, coverage, buf.slots[i].id)?;
    let value_format = u16_at(data, sub + 4)?;
    let record = match format {
        1 => sub + 6,
        2 => {
            let count = u16_at(data, sub + 6)? as usize;
            if index >= count {
                return None;
            }
            sub + 8 + index * value_size(value_format)
        }
        _ => return None,
    };
    add_value(data, record, value_format, &mut places[i]);
    Some(i + 1)
}

fn pair(
    buf: &Buffer<'_>,
    places: &mut [Place],
    sub: usize,
    i: usize,
    flag: u16,
    mark_set: u16,
) -> Option<usize> {
    let data = buf.data;
    let format = u16_at(data, sub)?;
    let coverage = sub + u16_at(data, sub + 2)? as usize;
    let first = coverage_index(data, coverage, buf.slots[i].id)?;
    let j = buf.next(i, flag, mark_set)?;
    let right = buf.slots[j].id;
    let vf1 = u16_at(data, sub + 4)?;
    let vf2 = u16_at(data, sub + 6)?;
    let size1 = value_size(vf1);
    let record = match format {
        1 => {
            let set_count = u16_at(data, sub + 8)? as usize;
            if first >= set_count {
                return None;
            }
            let set = sub + u16_at(data, sub + 10 + first * 2)? as usize;
            let count = u16_at(data, set)? as usize;
            let stride = 2 + size1 + value_size(vf2);
            let (mut lo, mut hi) = (0usize, count);
            let mut found = None;
            while lo < hi {
                let m = (lo + hi) / 2;
                let rec = set + 2 + m * stride;
                let g = u16_at(data, rec)?;
                if g == right {
                    found = Some(rec + 2);
                    break;
                }
                if g < right {
                    lo = m + 1;
                    continue;
                }
                hi = m;
            }
            found?
        }
        2 => {
            let class_def1 = sub + u16_at(data, sub + 8)? as usize;
            let class_def2 = sub + u16_at(data, sub + 10)? as usize;
            let class1_count = u16_at(data, sub + 12)? as usize;
            let class2_count = u16_at(data, sub + 14)? as usize;
            let c1 = class_of(data, class_def1, buf.slots[i].id) as usize;
            let c2 = class_of(data, class_def2, right) as usize;
            if c1 >= class1_count || c2 >= class2_count {
                return None;
            }
            sub + 16 + (c1 * class2_count + c2) * (size1 + value_size(vf2))
        }
        _ => return None,
    };
    add_value(data, record, vf1, &mut places[i]);
    add_value(data, record + size1, vf2, &mut places[j]);
    Some(if vf2 != 0 { j + 1 } else { j })
}

fn anchor(data: &[u8], at: usize) -> Option<(i32, i32)> {
    let format = u16_at(data, at)?;
    if !(1..=3).contains(&format) {
        return None;
    }
    Some((i16_at(data, at + 2)? as i32, i16_at(data, at + 4)? as i32))
}

/// The mark class and anchor of a glyph in a MarkArray.
fn mark_record(data: &[u8], array: usize, index: usize) -> Option<(usize, (i32, i32))> {
    let count = u16_at(data, array)? as usize;
    if index >= count {
        return None;
    }
    let rec = array + 2 + index * 4;
    let class = u16_at(data, rec)? as usize;
    let anchor_at = array + u16_at(data, rec + 2)? as usize;
    Some((class, anchor(data, anchor_at)?))
}

fn attach(places: &mut [Place], mark: usize, base: usize, to: (i32, i32), from: (i32, i32)) {
    places[mark].dx = to.0 - from.0;
    places[mark].dy = to.1 - from.1;
    places[mark].attach = Some(base as u32);
}

/// The nearest glyph before `i` that is not a mark.
fn previous_base(buf: &Buffer<'_>, i: usize) -> Option<usize> {
    (0..i).rev().find(|&j| buf.slots[j].class != CLASS_MARK)
}

fn mark_to_base(buf: &Buffer<'_>, places: &mut [Place], sub: usize, i: usize) -> Option<usize> {
    let data = buf.data;
    if u16_at(data, sub)? != 1 {
        return None;
    }
    let mark_coverage = sub + u16_at(data, sub + 2)? as usize;
    let base_coverage = sub + u16_at(data, sub + 4)? as usize;
    let classes = u16_at(data, sub + 6)? as usize;
    let marks = sub + u16_at(data, sub + 8)? as usize;
    let bases = sub + u16_at(data, sub + 10)? as usize;
    let mark_index = coverage_index(data, mark_coverage, buf.slots[i].id)?;
    let base = previous_base(buf, i)?;
    let base_index = coverage_index(data, base_coverage, buf.slots[base].id)?;
    let (class, mark_anchor) = mark_record(data, marks, mark_index)?;
    if class >= classes {
        return None;
    }
    let count = u16_at(data, bases)? as usize;
    if base_index >= count {
        return None;
    }
    let offset = u16_at(data, bases + 2 + (base_index * classes + class) * 2)?;
    if offset == 0 {
        return None;
    }
    let base_anchor = anchor(data, bases + offset as usize)?;
    attach(places, i, base, base_anchor, mark_anchor);
    Some(i + 1)
}

fn mark_to_ligature(buf: &Buffer<'_>, places: &mut [Place], sub: usize, i: usize) -> Option<usize> {
    let data = buf.data;
    if u16_at(data, sub)? != 1 {
        return None;
    }
    let mark_coverage = sub + u16_at(data, sub + 2)? as usize;
    let lig_coverage = sub + u16_at(data, sub + 4)? as usize;
    let classes = u16_at(data, sub + 6)? as usize;
    let marks = sub + u16_at(data, sub + 8)? as usize;
    let ligatures = sub + u16_at(data, sub + 10)? as usize;
    let mark_index = coverage_index(data, mark_coverage, buf.slots[i].id)?;
    let lig = previous_base(buf, i)?;
    let lig_index = coverage_index(data, lig_coverage, buf.slots[lig].id)?;
    let (class, mark_anchor) = mark_record(data, marks, mark_index)?;
    if class >= classes {
        return None;
    }
    let count = u16_at(data, ligatures)? as usize;
    if lig_index >= count {
        return None;
    }
    let attach_table = ligatures + u16_at(data, ligatures + 2 + lig_index * 2)? as usize;
    let components = u16_at(data, attach_table)? as usize;
    if components == 0 {
        return None;
    }
    let (mark, base) = (buf.slots[i], buf.slots[lig]);
    let component = match base.lig_id != 0 && mark.lig_id == base.lig_id && mark.component > 0 {
        true => (mark.component as usize - 1).min(components - 1),
        false => components - 1,
    };
    let offset = u16_at(data, attach_table + 2 + (component * classes + class) * 2)?;
    if offset == 0 {
        return None;
    }
    let lig_anchor = anchor(data, attach_table + offset as usize)?;
    attach(places, i, lig, lig_anchor, mark_anchor);
    Some(i + 1)
}

fn mark_to_mark(
    buf: &Buffer<'_>,
    places: &mut [Place],
    sub: usize,
    i: usize,
    flag: u16,
    mark_set: u16,
) -> Option<usize> {
    let data = buf.data;
    if u16_at(data, sub)? != 1 {
        return None;
    }
    let mark1_coverage = sub + u16_at(data, sub + 2)? as usize;
    let mark2_coverage = sub + u16_at(data, sub + 4)? as usize;
    let classes = u16_at(data, sub + 6)? as usize;
    let marks1 = sub + u16_at(data, sub + 8)? as usize;
    let marks2 = sub + u16_at(data, sub + 10)? as usize;
    let mark1_index = coverage_index(data, mark1_coverage, buf.slots[i].id)?;
    let j = buf.prev(i, flag, mark_set)?;
    let (mark1, mark2) = (buf.slots[i], buf.slots[j]);
    if mark2.class != CLASS_MARK {
        return None;
    }
    let same = match mark1.lig_id == mark2.lig_id {
        true => mark1.lig_id == 0 || mark1.component == mark2.component,
        false => {
            (mark1.lig_id > 0 && mark1.component == 0) || (mark2.lig_id > 0 && mark2.component == 0)
        }
    };
    if !same {
        return None;
    }
    let mark2_index = coverage_index(data, mark2_coverage, mark2.id)?;
    let (class, mark_anchor) = mark_record(data, marks1, mark1_index)?;
    if class >= classes {
        return None;
    }
    let count = u16_at(data, marks2)? as usize;
    if mark2_index >= count {
        return None;
    }
    let offset = u16_at(data, marks2 + 2 + (mark2_index * classes + class) * 2)?;
    if offset == 0 {
        return None;
    }
    let base_anchor = anchor(data, marks2 + offset as usize)?;
    attach(places, i, j, base_anchor, mark_anchor);
    Some(i + 1)
}

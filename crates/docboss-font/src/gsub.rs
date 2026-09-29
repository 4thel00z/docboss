//! Glyph substitution (`GSUB`): single, multiple, alternate, ligature,
//! context and chained context lookups, directly or through extension
//! lookups (types 1 to 7).

use crate::bytes::{i16_at, u16_at};
use crate::otl::{
    coverage_index, match_context, Buffer, LayoutTable, Slot, CLASS_LIGATURE, CLASS_MARK,
    MAX_GLYPHS, MAX_NESTING,
};

/// Applies lookup `index` to every glyph whose mask meets `mask`.
pub(crate) fn apply_lookup(table: &LayoutTable, buf: &mut Buffer<'_>, index: u16, mask: u32) {
    let Some(lookup) = table.lookup(index) else {
        return;
    };
    let mut i = 0;
    while i < buf.slots.len() {
        let slot = buf.slots[i];
        if slot.mask & mask == 0 || buf.ignored(i, lookup.flag, lookup.mark_set) {
            i += 1;
            continue;
        }
        i = apply_at(table, buf, index, i, 0).unwrap_or(i + 1);
    }
}

/// Applies the first subtable of lookup `index` that matches at glyph `i`;
/// returns where to continue.
fn apply_at(
    table: &LayoutTable,
    buf: &mut Buffer<'_>,
    index: u16,
    i: usize,
    depth: u8,
) -> Option<usize> {
    let lookup = table.lookup(index)?;
    for &sub in &lookup.subtables {
        let next = match lookup.kind {
            1 => single(buf, sub, i),
            2 => multiple(buf, sub, i),
            3 => alternate(buf, sub, i),
            4 => ligature(buf, sub, i, lookup.flag, lookup.mark_set),
            5 | 6 => context(
                table,
                buf,
                sub,
                i,
                lookup.kind == 6,
                lookup.flag,
                lookup.mark_set,
                depth,
            ),
            _ => None,
        };
        if next.is_some() {
            return next;
        }
    }
    None
}

fn replace(buf: &mut Buffer<'_>, i: usize, glyph: u16) {
    let hint = buf.slots[i].class;
    buf.slots[i].id = glyph;
    buf.slots[i].class = buf.class_for(glyph, hint);
}

fn single(buf: &mut Buffer<'_>, sub: usize, i: usize) -> Option<usize> {
    let data = buf.data;
    let format = u16_at(data, sub)?;
    let coverage = sub + u16_at(data, sub + 2)? as usize;
    let index = coverage_index(data, coverage, buf.slots[i].id)?;
    let glyph = match format {
        1 => (buf.slots[i].id as i32 + i16_at(data, sub + 4)? as i32) as u16,
        2 => {
            let count = u16_at(data, sub + 4)? as usize;
            if index >= count {
                return None;
            }
            u16_at(data, sub + 6 + index * 2)?
        }
        _ => return None,
    };
    replace(buf, i, glyph);
    Some(i + 1)
}

fn multiple(buf: &mut Buffer<'_>, sub: usize, i: usize) -> Option<usize> {
    let data = buf.data;
    if u16_at(data, sub)? != 1 {
        return None;
    }
    let coverage = sub + u16_at(data, sub + 2)? as usize;
    let index = coverage_index(data, coverage, buf.slots[i].id)?;
    let count = u16_at(data, sub + 4)? as usize;
    if index >= count {
        return None;
    }
    let sequence = sub + u16_at(data, sub + 6 + index * 2)? as usize;
    let glyphs = u16_at(data, sequence)? as usize;
    if buf.slots.len() + glyphs > MAX_GLYPHS {
        return None;
    }
    let original = buf.slots[i];
    let mut out = Vec::with_capacity(glyphs);
    for k in 0..glyphs {
        let glyph = u16_at(data, sequence + 2 + k * 2)?;
        let class = buf.class_for(glyph, original.class);
        out.push(Slot {
            id: glyph,
            class,
            ..original
        });
    }
    let added = out.len();
    buf.slots.splice(i..=i, out);
    Some(i + added)
}

fn alternate(buf: &mut Buffer<'_>, sub: usize, i: usize) -> Option<usize> {
    let data = buf.data;
    if u16_at(data, sub)? != 1 {
        return None;
    }
    let coverage = sub + u16_at(data, sub + 2)? as usize;
    let index = coverage_index(data, coverage, buf.slots[i].id)?;
    let count = u16_at(data, sub + 4)? as usize;
    if index >= count {
        return None;
    }
    let set = sub + u16_at(data, sub + 6 + index * 2)? as usize;
    if u16_at(data, set)? == 0 {
        return None;
    }
    let glyph = u16_at(data, set + 2)?;
    replace(buf, i, glyph);
    Some(i + 1)
}

fn ligature(buf: &mut Buffer<'_>, sub: usize, i: usize, flag: u16, mark_set: u16) -> Option<usize> {
    let data = buf.data;
    if u16_at(data, sub)? != 1 {
        return None;
    }
    let coverage = sub + u16_at(data, sub + 2)? as usize;
    let index = coverage_index(data, coverage, buf.slots[i].id)?;
    let count = u16_at(data, sub + 4)? as usize;
    if index >= count {
        return None;
    }
    let set = sub + u16_at(data, sub + 6 + index * 2)? as usize;
    let ligatures = u16_at(data, set)? as usize;
    for l in 0..ligatures {
        let lig = set + u16_at(data, set + 2 + l * 2)? as usize;
        let glyph = u16_at(data, lig)?;
        let components = u16_at(data, lig + 2)? as usize;
        if components == 0 {
            continue;
        }
        let mut positions = Vec::with_capacity(components);
        positions.push(i);
        let mut at = i;
        let mut matched = true;
        for k in 1..components {
            let Some(want) = u16_at(data, lig + 4 + (k - 1) * 2) else {
                matched = false;
                break;
            };
            let Some(next) = buf.next(at, flag, mark_set) else {
                matched = false;
                break;
            };
            if buf.slots[next].id != want {
                matched = false;
                break;
            }
            positions.push(next);
            at = next;
        }
        if !matched {
            continue;
        }
        form_ligature(buf, &positions, glyph);
        return Some(i + 1);
    }
    None
}

/// Replaces the glyphs at `positions` with one ligature glyph at the first;
/// marks skipped between the components stay after it and remember the
/// component they follow.
fn form_ligature(buf: &mut Buffer<'_>, positions: &[usize], glyph: u16) {
    let Some(&last) = positions.last() else {
        return;
    };
    let first = positions[0];
    buf.next_lig_id = buf.next_lig_id.wrapping_add(1).max(1);
    let id = buf.next_lig_id;
    let cluster = buf.slots[first..=last]
        .iter()
        .map(|s| s.cluster)
        .min()
        .unwrap_or(buf.slots[first].cluster);
    let mut component = 0u8;
    let mut kept = Vec::with_capacity(last - first + 1);
    for (k, slot) in buf.slots[first..=last].iter().enumerate() {
        let at = first + k;
        if positions.contains(&at) {
            component = component.saturating_add(1);
            continue;
        }
        let mut mark = *slot;
        mark.cluster = cluster;
        if mark.class == CLASS_MARK {
            mark.lig_id = id;
            mark.component = component;
        }
        kept.push(mark);
    }
    let class = buf.class_for(glyph, CLASS_LIGATURE);
    let mut ligature = Slot {
        id: glyph,
        cluster,
        lig_id: id,
        component: 0,
        class,
        ..buf.slots[first]
    };
    if ligature.class == 0 {
        ligature.class = CLASS_LIGATURE;
    }
    let mut replacement = Vec::with_capacity(kept.len() + 1);
    replacement.push(ligature);
    replacement.extend(kept);
    buf.slots.splice(first..=last, replacement);
}

#[allow(clippy::too_many_arguments)]
fn context(
    table: &LayoutTable,
    buf: &mut Buffer<'_>,
    sub: usize,
    i: usize,
    chained: bool,
    flag: u16,
    mark_set: u16,
    depth: u8,
) -> Option<usize> {
    if depth >= MAX_NESTING {
        return None;
    }
    let found = match_context(buf, sub, i, chained, flag, mark_set)?;
    let mut positions = found.positions;
    let mut end = *positions.last()? + 1;
    for (sequence, lookup) in found.records {
        let Some(&at) = positions.get(sequence as usize) else {
            continue;
        };
        if at >= buf.slots.len() {
            continue;
        }
        let before = buf.slots.len();
        if apply_at(table, buf, lookup, at, depth + 1).is_none() {
            continue;
        }
        let delta = buf.slots.len() as isize - before as isize;
        if delta == 0 {
            continue;
        }
        for p in positions.iter_mut().filter(|p| **p > at) {
            *p = (*p as isize + delta).max(at as isize) as usize;
        }
        end = (end as isize + delta).max(at as isize + 1) as usize;
    }
    Some(end.min(buf.slots.len()).max(i + 1))
}

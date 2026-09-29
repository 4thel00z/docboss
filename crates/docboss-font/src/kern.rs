//! Pair kerning from the `kern` table (format 0) and from GPOS pair
//! adjustment lookups (lookup type 2, formats 1 and 2, directly or through
//! extension lookups) of the `kern` feature.

use crate::bytes::{i16_at, u16_at, u32_at};
use crate::otl::{class_of, coverage_index};

#[derive(Debug, Clone, Default)]
pub(crate) struct Kerning {
    /// Offset of a `kern` format 0 subtable's pair array and its pair count.
    kern: Option<(usize, usize)>,
    /// GPOS pair adjustment subtables, in lookup order.
    pairs: Vec<usize>,
}

const MAX_SUBTABLES: usize = 512;

impl Kerning {
    pub(crate) fn parse(
        data: &[u8],
        kern: Option<(usize, usize)>,
        gpos: Option<(usize, usize)>,
    ) -> Kerning {
        let mut out = Kerning::default();
        if let Some((offset, _)) = kern {
            out.kern = kern_format0(data, offset);
        }
        if let Some((offset, _)) = gpos {
            out.pairs = gpos_pair_subtables(data, offset).unwrap_or_default();
        }
        out
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.kern.is_none() && self.pairs.is_empty()
    }

    /// The horizontal adjustment between two glyphs in font units.
    pub(crate) fn pair(&self, data: &[u8], left: u16, right: u16) -> i16 {
        if !self.pairs.is_empty() {
            return self
                .pairs
                .iter()
                .find_map(|&sub| pair_pos(data, sub, left, right))
                .unwrap_or(0);
        }
        let Some((base, count)) = self.kern else {
            return 0;
        };
        let key = u32::from(left) << 16 | u32::from(right);
        let (mut lo, mut hi) = (0usize, count);
        while lo < hi {
            let m = (lo + hi) / 2;
            let rec = base + m * 6;
            let Some(found) = u32_at(data, rec) else {
                return 0;
            };
            if found == key {
                return i16_at(data, rec + 4).unwrap_or(0);
            }
            if found < key {
                lo = m + 1;
                continue;
            }
            hi = m;
        }
        0
    }
}

fn kern_format0(data: &[u8], offset: usize) -> Option<(usize, usize)> {
    let version = u16_at(data, offset)?;
    if version != 0 {
        return None;
    }
    let tables = u16_at(data, offset + 2)?;
    let mut sub = offset + 4;
    for _ in 0..tables {
        let length = u16_at(data, sub + 2)? as usize;
        let coverage = u16_at(data, sub + 4)?;
        let format = coverage >> 8;
        let horizontal = coverage & 1 != 0;
        if format == 0 && horizontal {
            let pairs = u16_at(data, sub + 6)? as usize;
            return Some((sub + 14, pairs));
        }
        sub += length.max(6);
    }
    None
}

fn gpos_pair_subtables(data: &[u8], gpos: usize) -> Option<Vec<usize>> {
    let features = gpos + u16_at(data, gpos + 6)? as usize;
    let lookups = gpos + u16_at(data, gpos + 8)? as usize;
    let feature_count = u16_at(data, features)? as usize;
    let mut lookup_indices: Vec<u16> = Vec::new();
    for i in 0..feature_count {
        let rec = features + 2 + i * 6;
        if data.get(rec..rec + 4)? != b"kern" {
            continue;
        }
        let table = features + u16_at(data, rec + 4)? as usize;
        let count = u16_at(data, table + 2)? as usize;
        for j in 0..count {
            let index = u16_at(data, table + 4 + j * 2)?;
            if !lookup_indices.contains(&index) {
                lookup_indices.push(index);
            }
        }
    }
    lookup_indices.sort_unstable();
    let lookup_count = u16_at(data, lookups)?;
    let mut out = Vec::new();
    for index in lookup_indices.into_iter().filter(|&i| i < lookup_count) {
        let lookup = lookups + u16_at(data, lookups + 2 + index as usize * 2)? as usize;
        let kind = u16_at(data, lookup)?;
        let count = u16_at(data, lookup + 4)? as usize;
        for k in 0..count {
            let mut sub = lookup + u16_at(data, lookup + 6 + k * 2)? as usize;
            let mut sub_kind = kind;
            if kind == 9 {
                sub_kind = u16_at(data, sub + 2)?;
                sub += u32_at(data, sub + 4)? as usize;
            }
            if sub_kind == 2 && out.len() < MAX_SUBTABLES {
                out.push(sub);
            }
        }
    }
    Some(out)
}

fn value_size(format: u16) -> usize {
    (format & 0xFF).count_ones() as usize * 2
}

/// The XAdvance of a value record, the only field horizontal kerning uses.
fn x_advance(data: &[u8], record: usize, format: u16) -> Option<i16> {
    if format & 0x0004 == 0 {
        return None;
    }
    let before = (format & 0x0003).count_ones() as usize * 2;
    i16_at(data, record + before)
}

fn pair_pos(data: &[u8], sub: usize, left: u16, right: u16) -> Option<i16> {
    let format = u16_at(data, sub)?;
    let coverage = sub + u16_at(data, sub + 2)? as usize;
    let first = coverage_index(data, coverage, left)?;
    let vf1 = u16_at(data, sub + 4)?;
    let vf2 = u16_at(data, sub + 6)?;
    let record_size = value_size(vf1) + value_size(vf2);
    if format == 1 {
        let set_count = u16_at(data, sub + 8)? as usize;
        if first >= set_count {
            return None;
        }
        let set = sub + u16_at(data, sub + 10 + first * 2)? as usize;
        let count = u16_at(data, set)? as usize;
        let stride = 2 + record_size;
        let (mut lo, mut hi) = (0usize, count);
        while lo < hi {
            let m = (lo + hi) / 2;
            let rec = set + 2 + m * stride;
            let g = u16_at(data, rec)?;
            if g == right {
                return Some(x_advance(data, rec + 2, vf1).unwrap_or(0));
            }
            if g < right {
                lo = m + 1;
                continue;
            }
            hi = m;
        }
        return None;
    }
    if format != 2 {
        return None;
    }
    let class_def1 = sub + u16_at(data, sub + 8)? as usize;
    let class_def2 = sub + u16_at(data, sub + 10)? as usize;
    let class1_count = u16_at(data, sub + 12)? as usize;
    let class2_count = u16_at(data, sub + 14)? as usize;
    let c1 = class_of(data, class_def1, left) as usize;
    let c2 = class_of(data, class_def2, right) as usize;
    if c1 >= class1_count || c2 >= class2_count {
        return None;
    }
    let rec = sub + 16 + (c1 * class2_count + c2) * record_size;
    Some(x_advance(data, rec, vf1).unwrap_or(0))
}

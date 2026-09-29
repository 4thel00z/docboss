//! OpenType layout common tables: coverage and class definitions, the
//! script, feature and lookup lists of `GSUB` and `GPOS`, the glyph
//! classes of `GDEF`, and the glyph buffer and sequence matching both
//! lookup tables share.

use crate::bytes::{u16_at, u32_at};

/// Caps on work derived from the font: nested lookups, subtables per
/// lookup, glyphs a buffer may grow to and lookups a plan may hold.
pub(crate) const MAX_NESTING: u8 = 6;
pub(crate) const MAX_SUBTABLES: usize = 512;
pub(crate) const MAX_GLYPHS: usize = 4096;
const MAX_RECORDS: usize = 4096;

const IGNORE_BASE: u16 = 0x0002;
const IGNORE_LIGATURES: u16 = 0x0004;
const IGNORE_MARKS: u16 = 0x0008;
const USE_MARK_FILTERING_SET: u16 = 0x0010;

/// GDEF glyph classes.
pub(crate) const CLASS_BASE: u8 = 1;
pub(crate) const CLASS_LIGATURE: u8 = 2;
pub(crate) const CLASS_MARK: u8 = 3;

/// The coverage index of `glyph` in the coverage table at `table`.
pub(crate) fn coverage_index(data: &[u8], table: usize, glyph: u16) -> Option<usize> {
    let format = u16_at(data, table)?;
    let count = u16_at(data, table + 2)? as usize;
    let (mut lo, mut hi) = (0usize, count);
    while lo < hi {
        let m = (lo + hi) / 2;
        if format == 1 {
            let g = u16_at(data, table + 4 + m * 2)?;
            if g == glyph {
                return Some(m);
            }
            if g < glyph {
                lo = m + 1;
                continue;
            }
            hi = m;
            continue;
        }
        if format != 2 {
            return None;
        }
        let rec = table + 4 + m * 6;
        let start = u16_at(data, rec)?;
        let end = u16_at(data, rec + 2)?;
        if glyph < start {
            hi = m;
            continue;
        }
        if glyph > end {
            lo = m + 1;
            continue;
        }
        return Some(u16_at(data, rec + 4)? as usize + (glyph - start) as usize);
    }
    None
}

/// The class of `glyph` in the class definition table at `table`, 0 when
/// it has none.
pub(crate) fn class_of(data: &[u8], table: usize, glyph: u16) -> u16 {
    let Some(format) = u16_at(data, table) else {
        return 0;
    };
    if format == 1 {
        let (Some(start), Some(count)) = (u16_at(data, table + 2), u16_at(data, table + 4)) else {
            return 0;
        };
        if glyph < start || glyph - start >= count {
            return 0;
        }
        return u16_at(data, table + 6 + (glyph - start) as usize * 2).unwrap_or(0);
    }
    if format != 2 {
        return 0;
    }
    let Some(count) = u16_at(data, table + 2) else {
        return 0;
    };
    let (mut lo, mut hi) = (0usize, count as usize);
    while lo < hi {
        let m = (lo + hi) / 2;
        let rec = table + 4 + m * 6;
        let (Some(start), Some(end)) = (u16_at(data, rec), u16_at(data, rec + 2)) else {
            return 0;
        };
        if glyph < start {
            hi = m;
            continue;
        }
        if glyph > end {
            lo = m + 1;
            continue;
        }
        return u16_at(data, rec + 4).unwrap_or(0);
    }
    0
}

/// Offsets of the class tables of `GDEF`.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Gdef {
    glyph_classes: Option<usize>,
    mark_classes: Option<usize>,
    mark_sets: Option<usize>,
}

impl Gdef {
    pub(crate) fn parse(data: &[u8], table: Option<(usize, usize)>) -> Gdef {
        let Some((base, _)) = table else {
            return Gdef::default();
        };
        let offset = |at: usize| {
            u16_at(data, base + at)
                .filter(|&o| o != 0)
                .map(|o| base + o as usize)
        };
        let minor = u16_at(data, base + 2).unwrap_or(0);
        Gdef {
            glyph_classes: offset(4),
            mark_classes: offset(10),
            mark_sets: (minor >= 2).then(|| offset(12)).flatten(),
        }
    }

    pub(crate) fn has_classes(&self) -> bool {
        self.glyph_classes.is_some()
    }

    pub(crate) fn glyph_class(&self, data: &[u8], glyph: u16) -> u8 {
        self.glyph_classes
            .map_or(0, |t| class_of(data, t, glyph).min(4) as u8)
    }

    fn mark_class(&self, data: &[u8], glyph: u16) -> u16 {
        self.mark_classes.map_or(0, |t| class_of(data, t, glyph))
    }

    fn in_mark_set(&self, data: &[u8], set: u16, glyph: u16) -> bool {
        let Some(sets) = self.mark_sets else {
            return false;
        };
        let count = u16_at(data, sets + 2).unwrap_or(0);
        if set >= count {
            return false;
        }
        let Some(offset) = u32_at(data, sets + 4 + set as usize * 4) else {
            return false;
        };
        coverage_index(data, sets + offset as usize, glyph).is_some()
    }
}

/// A lookup: its type, flag, mark filtering set and subtables, extension
/// subtables already resolved to the type they wrap.
#[derive(Debug, Clone)]
pub(crate) struct Lookup {
    pub kind: u16,
    pub flag: u16,
    pub mark_set: u16,
    pub subtables: Vec<usize>,
}

/// The script, feature and lookup lists of a `GSUB` or `GPOS` table.
#[derive(Debug, Clone, Default)]
pub(crate) struct LayoutTable {
    base: usize,
    scripts: usize,
    features: usize,
    lookups: Vec<Option<Lookup>>,
}

impl LayoutTable {
    /// Reads the table's lists; `extension` is the lookup type that wraps
    /// another (7 in `GSUB`, 9 in `GPOS`).
    pub(crate) fn parse(
        data: &[u8],
        table: Option<(usize, usize)>,
        extension: u16,
    ) -> Option<LayoutTable> {
        let (base, _) = table?;
        let scripts = base + u16_at(data, base + 4)? as usize;
        let features = base + u16_at(data, base + 6)? as usize;
        let list = base + u16_at(data, base + 8)? as usize;
        let count = (u16_at(data, list)? as usize).min(MAX_RECORDS);
        let lookups = (0..count)
            .map(|i| {
                let lookup = list + u16_at(data, list + 2 + i * 2)? as usize;
                read_lookup(data, lookup, extension)
            })
            .collect();
        Some(LayoutTable {
            base,
            scripts,
            features,
            lookups,
        })
    }

    pub(crate) fn lookup(&self, index: u16) -> Option<&Lookup> {
        self.lookups.get(index as usize)?.as_ref()
    }

    /// The features of the default language system of `script`, falling
    /// back to `DFLT`, `dflt` and `latn`: each feature's tag and lookup
    /// indices, the required feature under the tag `    `.
    pub(crate) fn script_features(&self, data: &[u8], script: [u8; 4]) -> Vec<([u8; 4], Vec<u16>)> {
        let Some(langsys) = [script, *b"DFLT", *b"dflt", *b"latn"]
            .iter()
            .find_map(|tag| self.default_langsys(data, *tag))
        else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let required = u16_at(data, langsys + 2).unwrap_or(0xFFFF);
        if required != 0xFFFF {
            if let Some((_, lookups)) = self.feature(data, required) {
                out.push((*b"    ", lookups));
            }
        }
        let count = (u16_at(data, langsys + 4).unwrap_or(0) as usize).min(MAX_RECORDS);
        for i in 0..count {
            let Some(index) = u16_at(data, langsys + 6 + i * 2) else {
                break;
            };
            if let Some(feature) = self.feature(data, index) {
                out.push(feature);
            }
        }
        out
    }

    fn default_langsys(&self, data: &[u8], tag: [u8; 4]) -> Option<usize> {
        let count = (u16_at(data, self.scripts)? as usize).min(MAX_RECORDS);
        for i in 0..count {
            let rec = self.scripts + 2 + i * 6;
            if data.get(rec..rec + 4)? != tag {
                continue;
            }
            let script = self.scripts + u16_at(data, rec + 4)? as usize;
            let offset = u16_at(data, script)?;
            if offset == 0 {
                return None;
            }
            return Some(script + offset as usize);
        }
        None
    }

    fn feature(&self, data: &[u8], index: u16) -> Option<([u8; 4], Vec<u16>)> {
        let count = u16_at(data, self.features)?;
        if index >= count {
            return None;
        }
        let rec = self.features + 2 + index as usize * 6;
        let tag: [u8; 4] = data.get(rec..rec + 4)?.try_into().ok()?;
        let feature = self.features + u16_at(data, rec + 4)? as usize;
        let lookups = (u16_at(data, feature + 2)? as usize).min(MAX_RECORDS);
        let indices = (0..lookups)
            .map_while(|j| u16_at(data, feature + 4 + j * 2))
            .collect();
        let _ = self.base;
        Some((tag, indices))
    }
}

fn read_lookup(data: &[u8], lookup: usize, extension: u16) -> Option<Lookup> {
    let kind = u16_at(data, lookup)?;
    let flag = u16_at(data, lookup + 2)?;
    let count = (u16_at(data, lookup + 4)? as usize).min(MAX_SUBTABLES);
    let mark_set = match flag & USE_MARK_FILTERING_SET != 0 {
        true => u16_at(data, lookup + 6 + count * 2).unwrap_or(0),
        false => 0,
    };
    let mut subtables = Vec::with_capacity(count);
    let mut resolved = kind;
    for k in 0..count {
        let mut sub = lookup + u16_at(data, lookup + 6 + k * 2)? as usize;
        if kind == extension {
            let Some(inner) = u16_at(data, sub + 2) else {
                continue;
            };
            let Some(offset) = u32_at(data, sub + 4) else {
                continue;
            };
            resolved = inner;
            sub += offset as usize;
        }
        subtables.push(sub);
    }
    Some(Lookup {
        kind: resolved,
        flag,
        mark_set,
        subtables,
    })
}

/// One glyph of a buffer being shaped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Slot {
    pub id: u16,
    pub cluster: u32,
    pub mask: u32,
    /// GDEF glyph class, or the caller's mark hint when there is no GDEF.
    pub class: u8,
    /// The ligature a glyph is part of, 0 for none, and for a mark the
    /// ligature component it follows.
    pub lig_id: u8,
    pub component: u8,
}

/// The glyphs being shaped and what glyph skipping needs.
pub(crate) struct Buffer<'a> {
    pub data: &'a [u8],
    pub gdef: &'a Gdef,
    pub slots: Vec<Slot>,
    pub next_lig_id: u8,
}

impl Buffer<'_> {
    pub(crate) fn class_for(&self, glyph: u16, hint: u8) -> u8 {
        if !self.gdef.has_classes() {
            return hint;
        }
        self.gdef.glyph_class(self.data, glyph)
    }

    /// Whether a lookup with `flag` skips the glyph at `i`.
    pub(crate) fn ignored(&self, i: usize, flag: u16, mark_set: u16) -> bool {
        let slot = &self.slots[i];
        match slot.class {
            CLASS_BASE => flag & IGNORE_BASE != 0,
            CLASS_LIGATURE => flag & IGNORE_LIGATURES != 0,
            CLASS_MARK => {
                if flag & IGNORE_MARKS != 0 {
                    return true;
                }
                if flag & USE_MARK_FILTERING_SET != 0 {
                    return !self.gdef.in_mark_set(self.data, mark_set, slot.id);
                }
                let wanted = flag >> 8;
                wanted != 0 && self.gdef.mark_class(self.data, slot.id) != wanted
            }
            _ => false,
        }
    }

    /// The next glyph after `i` the lookup does not skip.
    pub(crate) fn next(&self, i: usize, flag: u16, mark_set: u16) -> Option<usize> {
        (i + 1..self.slots.len()).find(|&j| !self.ignored(j, flag, mark_set))
    }

    /// The previous glyph before `i` the lookup does not skip.
    pub(crate) fn prev(&self, i: usize, flag: u16, mark_set: u16) -> Option<usize> {
        (0..i).rev().find(|&j| !self.ignored(j, flag, mark_set))
    }
}

/// How the glyphs of a context rule are given.
#[derive(Clone, Copy)]
pub(crate) enum Match {
    Glyphs,
    Classes(usize),
    Coverages(usize),
}

impl Match {
    fn hit(self, data: &[u8], value: u16, glyph: u16) -> bool {
        match self {
            Match::Glyphs => value == glyph,
            Match::Classes(table) => class_of(data, table, glyph) == value,
            Match::Coverages(base) => coverage_index(data, base + value as usize, glyph).is_some(),
        }
    }
}

/// A sequence of values at `at`, `count` long, matched against the glyphs
/// after `start` (forward) or before it (backward).
pub(crate) struct Sequence {
    pub at: usize,
    pub count: usize,
    pub how: Match,
}

impl Buffer<'_> {
    /// Matches an input sequence whose first glyph is at `start`; returns
    /// the positions of all its glyphs.
    pub(crate) fn match_input(
        &self,
        start: usize,
        input: &Sequence,
        flag: u16,
        mark_set: u16,
    ) -> Option<Vec<usize>> {
        let mut positions = Vec::with_capacity(input.count + 1);
        positions.push(start);
        let mut at = start;
        for k in 0..input.count {
            let value = u16_at(self.data, input.at + k * 2)?;
            at = self.next(at, flag, mark_set)?;
            if !input.how.hit(self.data, value, self.slots[at].id) {
                return None;
            }
            positions.push(at);
        }
        Some(positions)
    }

    /// Whether the glyphs before `start` match a backtrack sequence, the
    /// nearest glyph first.
    pub(crate) fn match_backtrack(
        &self,
        start: usize,
        seq: &Sequence,
        flag: u16,
        mark_set: u16,
    ) -> bool {
        let mut at = start;
        for k in 0..seq.count {
            let Some(value) = u16_at(self.data, seq.at + k * 2) else {
                return false;
            };
            let Some(prev) = self.prev(at, flag, mark_set) else {
                return false;
            };
            if !seq.how.hit(self.data, value, self.slots[prev].id) {
                return false;
            }
            at = prev;
        }
        true
    }

    /// Whether the glyphs after `end` match a lookahead sequence.
    pub(crate) fn match_lookahead(
        &self,
        end: usize,
        seq: &Sequence,
        flag: u16,
        mark_set: u16,
    ) -> bool {
        let mut at = end;
        for k in 0..seq.count {
            let Some(value) = u16_at(self.data, seq.at + k * 2) else {
                return false;
            };
            let Some(next) = self.next(at, flag, mark_set) else {
                return false;
            };
            if !seq.how.hit(self.data, value, self.slots[next].id) {
                return false;
            }
            at = next;
        }
        true
    }
}

/// A matched context rule: the positions of its input glyphs and its
/// nested lookup records (sequence index, lookup index).
pub(crate) struct ContextMatch {
    pub positions: Vec<usize>,
    pub records: Vec<(u16, u16)>,
}

fn records(data: &[u8], at: usize, count: usize) -> Vec<(u16, u16)> {
    (0..count.min(MAX_RECORDS))
        .map_while(|k| Some((u16_at(data, at + k * 4)?, u16_at(data, at + k * 4 + 2)?)))
        .collect()
}

/// Matches a context (`chained` false: GSUB 5 and GPOS 7) or chained
/// context (GSUB 6, GPOS 8) subtable, formats 1 to 3, at glyph `i`.
pub(crate) fn match_context(
    buf: &Buffer<'_>,
    sub: usize,
    i: usize,
    chained: bool,
    flag: u16,
    mark_set: u16,
) -> Option<ContextMatch> {
    let data = buf.data;
    let format = u16_at(data, sub)?;
    let glyph = buf.slots[i].id;
    if format == 3 {
        return match_format3(buf, sub, i, chained, flag, mark_set);
    }
    let coverage = sub + u16_at(data, sub + 2)? as usize;
    let index = coverage_index(data, coverage, glyph)?;
    let (how_back, how_in, how_ahead, set_index, sets_at) = match (format, chained) {
        (1, false) => (Match::Glyphs, Match::Glyphs, Match::Glyphs, index, sub + 4),
        (1, true) => (Match::Glyphs, Match::Glyphs, Match::Glyphs, index, sub + 4),
        (2, false) => {
            let classes = sub + u16_at(data, sub + 4)? as usize;
            let c = class_of(data, classes, glyph) as usize;
            let how = Match::Classes(classes);
            (how, how, how, c, sub + 6)
        }
        (2, true) => {
            let back = sub + u16_at(data, sub + 4)? as usize;
            let input = sub + u16_at(data, sub + 6)? as usize;
            let ahead = sub + u16_at(data, sub + 8)? as usize;
            let c = class_of(data, input, glyph) as usize;
            (
                Match::Classes(back),
                Match::Classes(input),
                Match::Classes(ahead),
                c,
                sub + 10,
            )
        }
        _ => return None,
    };
    let set_count = u16_at(data, sets_at)? as usize;
    if set_index >= set_count {
        return None;
    }
    let set_offset = u16_at(data, sets_at + 2 + set_index * 2)?;
    if set_offset == 0 {
        return None;
    }
    let set = sub + set_offset as usize;
    let rules = (u16_at(data, set)? as usize).min(MAX_RECORDS);
    for r in 0..rules {
        let rule = set + u16_at(data, set + 2 + r * 2)? as usize;
        let found = match chained {
            false => {
                let glyphs = u16_at(data, rule)? as usize;
                let lookups = u16_at(data, rule + 2)? as usize;
                let input = Sequence {
                    at: rule + 4,
                    count: glyphs.saturating_sub(1),
                    how: how_in,
                };
                buf.match_input(i, &input, flag, mark_set)
                    .map(|positions| ContextMatch {
                        positions,
                        records: records(data, rule + 4 + glyphs.saturating_sub(1) * 2, lookups),
                    })
            }
            true => {
                let back_count = u16_at(data, rule)? as usize;
                let back = Sequence {
                    at: rule + 2,
                    count: back_count,
                    how: how_back,
                };
                let input_at = rule + 2 + back_count * 2;
                let input_count = u16_at(data, input_at)? as usize;
                let input = Sequence {
                    at: input_at + 2,
                    count: input_count.saturating_sub(1),
                    how: how_in,
                };
                let ahead_at = input_at + 2 + input_count.saturating_sub(1) * 2;
                let ahead_count = u16_at(data, ahead_at)? as usize;
                let ahead = Sequence {
                    at: ahead_at + 2,
                    count: ahead_count,
                    how: how_ahead,
                };
                let records_at = ahead_at + 2 + ahead_count * 2;
                let lookups = u16_at(data, records_at)? as usize;
                chain(buf, i, &back, &input, &ahead, flag, mark_set).map(|positions| ContextMatch {
                    positions,
                    records: records(data, records_at + 2, lookups),
                })
            }
        };
        if found.is_some() {
            return found;
        }
    }
    None
}

fn chain(
    buf: &Buffer<'_>,
    i: usize,
    back: &Sequence,
    input: &Sequence,
    ahead: &Sequence,
    flag: u16,
    mark_set: u16,
) -> Option<Vec<usize>> {
    let positions = buf.match_input(i, input, flag, mark_set)?;
    let end = *positions.last()?;
    if !buf.match_backtrack(i, back, flag, mark_set)
        || !buf.match_lookahead(end, ahead, flag, mark_set)
    {
        return None;
    }
    Some(positions)
}

fn match_format3(
    buf: &Buffer<'_>,
    sub: usize,
    i: usize,
    chained: bool,
    flag: u16,
    mark_set: u16,
) -> Option<ContextMatch> {
    let data = buf.data;
    let glyph = buf.slots[i].id;
    if !chained {
        let glyphs = u16_at(data, sub + 2)? as usize;
        let lookups = u16_at(data, sub + 4)? as usize;
        let first = u16_at(data, sub + 6)?;
        coverage_index(data, sub + first as usize, glyph)?;
        let input = Sequence {
            at: sub + 8,
            count: glyphs.saturating_sub(1),
            how: Match::Coverages(sub),
        };
        let positions = buf.match_input(i, &input, flag, mark_set)?;
        return Some(ContextMatch {
            positions,
            records: records(data, sub + 6 + glyphs * 2, lookups),
        });
    }
    let back_count = u16_at(data, sub + 2)? as usize;
    let back = Sequence {
        at: sub + 4,
        count: back_count,
        how: Match::Coverages(sub),
    };
    let input_at = sub + 4 + back_count * 2;
    let input_count = u16_at(data, input_at)? as usize;
    if input_count == 0 {
        return None;
    }
    let first = u16_at(data, input_at + 2)?;
    coverage_index(data, sub + first as usize, glyph)?;
    let input = Sequence {
        at: input_at + 4,
        count: input_count - 1,
        how: Match::Coverages(sub),
    };
    let ahead_at = input_at + 2 + input_count * 2;
    let ahead_count = u16_at(data, ahead_at)? as usize;
    let ahead = Sequence {
        at: ahead_at + 2,
        count: ahead_count,
        how: Match::Coverages(sub),
    };
    let records_at = ahead_at + 2 + ahead_count * 2;
    let lookups = u16_at(data, records_at)? as usize;
    let positions = chain(buf, i, &back, &input, &ahead, flag, mark_set)?;
    Some(ContextMatch {
        positions,
        records: records(data, records_at + 2, lookups),
    })
}

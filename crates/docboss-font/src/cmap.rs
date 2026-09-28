//! The `cmap` table: character to glyph mapping through formats 0, 4, 6,
//! 12 and 13, with format 14 variation sequences.

use crate::bytes::{u16_at, u32_at, u8_at};

#[derive(Debug, Clone, Copy)]
struct Subtable {
    offset: usize,
    format: u16,
}

/// The subtables a font offers, best Unicode table first.
#[derive(Debug, Clone, Default)]
pub(crate) struct Cmap {
    unicode: Option<Subtable>,
    symbol: Option<Subtable>,
    mac: Option<Subtable>,
    variations: Option<usize>,
}

impl Cmap {
    pub(crate) fn parse(data: &[u8], base: usize) -> Cmap {
        let mut cmap = Cmap::default();
        let Some(count) = u16_at(data, base + 2) else {
            return cmap;
        };
        let mut best = 0;
        for i in 0..count as usize {
            let rec = base + 4 + i * 8;
            let (Some(platform), Some(encoding), Some(offset)) = (
                u16_at(data, rec),
                u16_at(data, rec + 2),
                u32_at(data, rec + 4),
            ) else {
                break;
            };
            let offset = base + offset as usize;
            let Some(format) = u16_at(data, offset) else {
                continue;
            };
            if format == 14 {
                cmap.variations = Some(offset);
                continue;
            }
            if !matches!(format, 0 | 4 | 6 | 12 | 13) {
                continue;
            }
            let table = Subtable { offset, format };
            let score = match (platform, encoding) {
                (3, 10) | (0, 4) | (0, 6) => 4,
                (3, 1) | (0, 3) => 3,
                (0, _) => 2,
                (3, 0) => {
                    cmap.symbol.get_or_insert(table);
                    continue;
                }
                (1, 0) => {
                    cmap.mac.get_or_insert(table);
                    continue;
                }
                _ => continue,
            };
            if score > best {
                best = score;
                cmap.unicode = Some(table);
            }
        }
        cmap
    }

    pub(crate) fn is_symbol(&self) -> bool {
        self.unicode.is_none() && self.symbol.is_some()
    }

    /// The glyph for a character; symbol fonts are also tried in the
    /// U+F000 private-use range where Word symbol fonts place their codes.
    pub(crate) fn lookup(&self, data: &[u8], cp: u32) -> Option<u16> {
        if let Some(gid) = self
            .unicode
            .and_then(|t| t.lookup(data, cp))
            .filter(|&g| g != 0)
        {
            return Some(gid);
        }
        if let Some(table) = self.symbol {
            let direct = table.lookup(data, cp).filter(|&g| g != 0);
            let private = (cp < 0x100)
                .then(|| table.lookup(data, cp + 0xF000))
                .flatten()
                .filter(|&g| g != 0);
            if let Some(gid) = direct.or(private) {
                return Some(gid);
            }
        }
        let code = mac_roman_code(cp)?;
        self.mac
            .and_then(|t| t.lookup(data, u32::from(code)))
            .filter(|&g| g != 0)
    }

    /// The glyph for a variation sequence, from format 14's non-default
    /// mappings; `None` when the default glyph applies.
    pub(crate) fn lookup_variation(&self, data: &[u8], cp: u32, selector: u32) -> Option<u16> {
        let base = self.variations?;
        let count = u32_at(data, base + 6)? as usize;
        for i in 0..count.min(256) {
            let rec = base + 10 + i * 11;
            let vs = u24_at(data, rec)?;
            if vs != selector {
                continue;
            }
            let non_default = u32_at(data, rec + 7)? as usize;
            if non_default == 0 {
                return None;
            }
            let table = base + non_default;
            let mappings = u32_at(data, table)? as usize;
            let (mut lo, mut hi) = (0usize, mappings);
            while lo < hi {
                let m = (lo + hi) / 2;
                let entry = table + 4 + m * 5;
                let unicode = u24_at(data, entry)?;
                if unicode == cp {
                    return u16_at(data, entry + 3);
                }
                if unicode < cp {
                    lo = m + 1;
                    continue;
                }
                hi = m;
            }
            return None;
        }
        None
    }
}

fn u24_at(d: &[u8], o: usize) -> Option<u32> {
    let b = d.get(o..o + 3)?;
    Some(u32::from(b[0]) << 16 | u32::from(b[1]) << 8 | u32::from(b[2]))
}

fn mac_roman_code(cp: u32) -> Option<u8> {
    if cp < 0x80 {
        return Some(cp as u8);
    }
    None
}

impl Subtable {
    fn lookup(&self, data: &[u8], cp: u32) -> Option<u16> {
        match self.format {
            0 => (cp < 256)
                .then(|| u8_at(data, self.offset + 6 + cp as usize).map(u16::from))
                .flatten(),
            4 => self.format4(data, cp),
            6 => self.format6(data, cp),
            12 | 13 => self.format12(data, cp),
            _ => None,
        }
    }

    fn format4(&self, data: &[u8], cp: u32) -> Option<u16> {
        if cp > 0xFFFF {
            return None;
        }
        let cp = cp as u16;
        let o = self.offset;
        let segs = u16_at(data, o + 6)? as usize / 2;
        let ends = o + 14;
        let starts = ends + segs * 2 + 2;
        let deltas = starts + segs * 2;
        let ranges = deltas + segs * 2;
        let (mut lo, mut hi) = (0usize, segs);
        while lo < hi {
            let m = (lo + hi) / 2;
            if u16_at(data, ends + m * 2)? < cp {
                lo = m + 1;
                continue;
            }
            hi = m;
        }
        let s = lo;
        if s >= segs {
            return None;
        }
        let start = u16_at(data, starts + s * 2)?;
        if cp < start {
            return None;
        }
        let delta = u16_at(data, deltas + s * 2)?;
        let range = u16_at(data, ranges + s * 2)?;
        if range == 0 {
            return Some(cp.wrapping_add(delta));
        }
        let at = ranges + s * 2 + range as usize + (cp - start) as usize * 2;
        let g = u16_at(data, at)?;
        Some(if g == 0 { 0 } else { g.wrapping_add(delta) })
    }

    fn format6(&self, data: &[u8], cp: u32) -> Option<u16> {
        let first = u16_at(data, self.offset + 6)? as u32;
        let count = u16_at(data, self.offset + 8)? as u32;
        if cp < first || cp >= first + count {
            return None;
        }
        u16_at(data, self.offset + 10 + (cp - first) as usize * 2)
    }

    fn format12(&self, data: &[u8], cp: u32) -> Option<u16> {
        let o = self.offset;
        let groups = u32_at(data, o + 12)? as usize;
        let (mut lo, mut hi) = (0usize, groups);
        while lo < hi {
            let m = (lo + hi) / 2;
            let rec = o + 16 + m * 12;
            let start = u32_at(data, rec)?;
            let end = u32_at(data, rec + 4)?;
            if cp < start {
                hi = m;
                continue;
            }
            if cp > end {
                lo = m + 1;
                continue;
            }
            let gid = u32_at(data, rec + 8)?;
            if self.format == 13 {
                return Some(gid as u16);
            }
            return Some((gid + (cp - start)) as u16);
        }
        None
    }
}

//! One parsed font face: tables, metrics, character mapping, kerning and
//! outlines.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use crate::bytes::{i16_at, u16_at, u32_at};
use crate::cff::CffFont;
use crate::cmap::Cmap;
use crate::glyf::Glyf;
use crate::kern::Kerning;
use crate::otl::{Gdef, LayoutTable};
use crate::outline::Seg;
use crate::shape::ScriptFeatures;
use crate::FontError;

const TAG_TTCF: u32 = 0x7474_6366;
const MAX_TABLES: usize = 512;

/// Vertical metrics in font units. `ascent` is above the baseline and
/// positive; `descent` is below it and positive.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Metrics {
    pub units_per_em: u16,
    pub ascent: f32,
    pub descent: f32,
    /// Leading added below `descent` for single line spacing, as Word
    /// computes it: the part of the `hhea` line gap the Windows metrics do
    /// not already cover.
    pub line_gap: f32,
    pub cap_height: f32,
    pub x_height: f32,
    /// Distance of the underline's top below the baseline, positive down.
    pub underline_position: f32,
    pub underline_thickness: f32,
    /// Height of the strikeout's center above the baseline.
    pub strikeout_position: f32,
    pub strikeout_thickness: f32,
}

/// Naming and style information of a face.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FaceNames {
    pub family: String,
    pub subfamily: String,
    /// The typographic family (name id 16), when it differs from `family`.
    pub typographic_family: Option<String>,
    pub full_name: Option<String>,
    pub postscript_name: Option<String>,
}

/// Style bits of a face.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FaceStyle {
    /// OS/2 weight class, 100 to 900.
    pub weight: u16,
    pub italic: bool,
    pub monospace: bool,
}

impl FaceStyle {
    pub fn is_bold(self) -> bool {
        self.weight >= 600
    }
}

/// Number of faces in a font file: the TTC face count, or 1.
pub fn face_count(data: &[u8]) -> u32 {
    if u32_at(data, 0) != Some(TAG_TTCF) {
        return 1;
    }
    u32_at(data, 8).unwrap_or(0).min(256)
}

/// Offset of face `index`'s table directory.
pub(crate) fn face_offset(data: &[u8], index: u32) -> Option<usize> {
    if u32_at(data, 0) != Some(TAG_TTCF) {
        return (index == 0).then_some(0);
    }
    if index >= face_count(data) {
        return None;
    }
    u32_at(data, 12 + index as usize * 4).map(|o| o as usize)
}

/// The table records of a face: tag to `(offset, length)`.
pub(crate) fn table_directory(
    data: &[u8],
    face: usize,
) -> Option<HashMap<[u8; 4], (usize, usize)>> {
    let count = (u16_at(data, face + 4)? as usize).min(MAX_TABLES);
    let mut tables = HashMap::with_capacity(count);
    for i in 0..count {
        let rec = face + 12 + i * 16;
        let tag: [u8; 4] = data.get(rec..rec + 4)?.try_into().ok()?;
        let offset = u32_at(data, rec + 8)? as usize;
        let length = u32_at(data, rec + 12)? as usize;
        tables.insert(tag, (offset, length));
    }
    Some(tables)
}

/// Reads the name table records for a face's names.
pub(crate) fn read_names(data: &[u8]) -> FaceNames {
    let name = 0usize;
    let mut found: HashMap<u16, (u8, String)> = HashMap::new();
    let (Some(count), Some(storage)) = (u16_at(data, name + 2), u16_at(data, name + 4)) else {
        return FaceNames::default();
    };
    let storage = name + storage as usize;
    for i in 0..count as usize {
        let rec = name + 6 + i * 12;
        let (Some(platform), Some(encoding), Some(language), Some(id), Some(length), Some(offset)) = (
            u16_at(data, rec),
            u16_at(data, rec + 2),
            u16_at(data, rec + 4),
            u16_at(data, rec + 6),
            u16_at(data, rec + 8),
            u16_at(data, rec + 10),
        ) else {
            break;
        };
        if !matches!(id, 1 | 2 | 4 | 6 | 16 | 17) {
            continue;
        }
        let start = storage + offset as usize;
        let Some(bytes) = data.get(start..start + length as usize) else {
            continue;
        };
        let (rank, text) = match (platform, encoding) {
            (3, 1) | (3, 10) | (0, _) => {
                let english = platform == 0 || language == 0x0409;
                let units: Vec<u16> = bytes
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|c| u16::from_be_bytes(*c))
                    .collect();
                (
                    if english { 3 } else { 2 },
                    String::from_utf16_lossy(&units),
                )
            }
            (1, 0) => (
                1,
                bytes
                    .iter()
                    .map(|&b| if b < 0x80 { b as char } else { '?' })
                    .collect(),
            ),
            (3, 0) => {
                let units: Vec<u16> = bytes
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|c| u16::from_be_bytes(*c))
                    .collect();
                (1, String::from_utf16_lossy(&units))
            }
            _ => continue,
        };
        let better = found.get(&id).is_none_or(|(existing, _)| rank > *existing);
        if better && !text.trim().is_empty() {
            found.insert(id, (rank, text.trim().to_string()));
        }
    }
    let mut take = |id: u16| found.remove(&id).map(|(_, text)| text);
    let family = take(1).unwrap_or_default();
    let subfamily = take(2).unwrap_or_default();
    let full_name = take(4);
    let postscript_name = take(6);
    let typographic_family = take(16).filter(|t| *t != family);
    FaceNames {
        family,
        subfamily,
        typographic_family,
        full_name,
        postscript_name,
    }
}

/// Reads the style from `OS/2` and `head`, falling back to the subfamily.
pub(crate) fn read_style(
    os2: Option<&[u8]>,
    head: Option<&[u8]>,
    post: Option<&[u8]>,
    subfamily: &str,
) -> FaceStyle {
    let sub = subfamily.to_ascii_lowercase();
    let mut weight = os2.and_then(|t| u16_at(t, 4)).unwrap_or(0);
    if !(1..=1000).contains(&weight) {
        weight = if sub.contains("bold") { 700 } else { 400 };
    }
    let fs_selection = os2.and_then(|t| u16_at(t, 62)).unwrap_or(0);
    let mac_style = head.and_then(|t| u16_at(t, 44)).unwrap_or(0);
    let italic = fs_selection & 0x0201 != 0
        || mac_style & 0x2 != 0
        || sub.contains("italic")
        || sub.contains("oblique");
    if fs_selection & 0x20 != 0 || mac_style & 0x1 != 0 {
        weight = weight.max(700);
    }
    let monospace = post.and_then(|t| u32_at(t, 12)).unwrap_or(0) != 0;
    FaceStyle {
        weight,
        italic,
        monospace,
    }
}

/// A parsed font face.
pub struct Font {
    pub(crate) data: Arc<[u8]>,
    names: FaceNames,
    style: FaceStyle,
    metrics: Metrics,
    num_glyphs: u16,
    advances: Vec<u16>,
    loca: Vec<u32>,
    cff: Option<CffFont>,
    cmap: Cmap,
    pub(crate) kerning: Kerning,
    gsub_table: Option<(usize, usize)>,
    gpos_table: Option<(usize, usize)>,
    gsub: OnceLock<Option<LayoutTable>>,
    gpos: OnceLock<Option<LayoutTable>>,
    pub(crate) gdef: Gdef,
    pub(crate) script_cache: Mutex<HashMap<(bool, [u8; 4]), ScriptFeatures>>,
    outlines: Mutex<HashMap<u16, Arc<[Seg]>>>,
    kern_cache: Mutex<HashMap<(u16, u16), i16>>,
}

impl std::fmt::Debug for Font {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Font")
            .field("names", &self.names)
            .field("style", &self.style)
            .finish()
    }
}

impl Font {
    /// Parses face `index` of a TrueType, OpenType or collection file.
    pub fn parse(data: impl Into<Arc<[u8]>>, index: u32) -> Result<Font, FontError> {
        let data: Arc<[u8]> = data.into();
        let bytes = &data[..];
        let face = face_offset(bytes, index).ok_or(FontError::NoSuchFace(index))?;
        let tables = table_directory(bytes, face).ok_or(FontError::Malformed("table directory"))?;
        let table = |tag: &[u8; 4]| {
            tables
                .get(tag)
                .copied()
                .filter(|&(o, l)| o.saturating_add(l) <= bytes.len())
        };
        let head = table(b"head")
            .ok_or(FontError::Malformed("missing head table"))?
            .0;
        let units_per_em = u16_at(bytes, head + 18)
            .filter(|&u| u >= 16)
            .unwrap_or(1000);
        let num_glyphs = table(b"maxp")
            .and_then(|(o, _)| u16_at(bytes, o + 4))
            .unwrap_or(0);
        let slice = |tag: &[u8; 4]| table(tag).map(|(o, l)| &bytes[o..o + l]);
        let names = slice(b"name").map(read_names).unwrap_or_default();
        let os2 = table(b"OS/2").map(|(o, _)| o);
        let post = table(b"post").map(|(o, _)| o);
        let style = read_style(
            slice(b"OS/2"),
            slice(b"head"),
            slice(b"post"),
            &names.subfamily,
        );
        let hhea = table(b"hhea").map(|(o, _)| o);
        let metrics = read_metrics(bytes, units_per_em, hhea, os2, post);
        let advances = read_advances(bytes, hhea, table(b"hmtx"), num_glyphs);
        let loca = match (table(b"loca"), table(b"glyf")) {
            (Some(loca), Some(glyf)) => read_loca(bytes, head, loca, glyf, num_glyphs),
            _ => Vec::new(),
        };
        let cff = match table(b"CFF ") {
            Some((o, l)) if loca.is_empty() => CffFont::parse(bytes[o..o + l].to_vec()),
            _ => None,
        };
        if loca.is_empty() && cff.is_none() {
            let reason = if table(b"CFF2").is_some() {
                "CFF2 outlines are not supported"
            } else {
                "no glyf or CFF outlines"
            };
            return Err(FontError::Malformed(reason));
        }
        let cmap = table(b"cmap")
            .map(|(o, _)| Cmap::parse(bytes, o))
            .unwrap_or_default();
        let kerning = Kerning::parse(bytes, table(b"kern"), table(b"GPOS"));
        let gsub_table = table(b"GSUB");
        let gpos_table = table(b"GPOS");
        let gdef = Gdef::parse(bytes, table(b"GDEF"));
        Ok(Font {
            data,
            names,
            style,
            metrics,
            num_glyphs,
            advances,
            loca,
            cff,
            cmap,
            kerning,
            gsub_table,
            gpos_table,
            gsub: OnceLock::new(),
            gpos: OnceLock::new(),
            gdef,
            script_cache: Mutex::new(HashMap::new()),
            outlines: Mutex::new(HashMap::new()),
            kern_cache: Mutex::new(HashMap::new()),
        })
    }

    /// The `GSUB` lists, read on first use.
    pub(crate) fn gsub(&self) -> Option<&LayoutTable> {
        self.gsub
            .get_or_init(|| LayoutTable::parse(&self.data, self.gsub_table, 7))
            .as_ref()
    }

    /// The `GPOS` lists, read on first use.
    pub(crate) fn gpos(&self) -> Option<&LayoutTable> {
        self.gpos
            .get_or_init(|| LayoutTable::parse(&self.data, self.gpos_table, 9))
            .as_ref()
    }

    /// Whether the face has glyph substitutions or positions to apply.
    pub fn has_layout(&self) -> bool {
        self.gsub_table.is_some() || self.gpos_table.is_some()
    }

    pub fn names(&self) -> &FaceNames {
        &self.names
    }

    pub fn style(&self) -> FaceStyle {
        self.style
    }

    pub fn metrics(&self) -> &Metrics {
        &self.metrics
    }

    pub fn units_per_em(&self) -> u16 {
        self.metrics.units_per_em
    }

    pub fn num_glyphs(&self) -> u16 {
        self.num_glyphs
    }

    /// Whether the face maps characters only through a symbol subtable.
    pub fn is_symbol(&self) -> bool {
        self.cmap.is_symbol()
    }

    /// The glyph for a character, or `None` when the face lacks it.
    pub fn glyph_index(&self, c: char) -> Option<u16> {
        self.cmap
            .lookup(&self.data, c as u32)
            .filter(|&g| g < self.num_glyphs.max(1))
    }

    /// The glyph for a character followed by a variation selector.
    pub fn glyph_variant(&self, c: char, selector: char) -> Option<u16> {
        self.cmap
            .lookup_variation(&self.data, c as u32, selector as u32)
            .or_else(|| self.glyph_index(c))
    }

    /// Advance width of a glyph in font units.
    pub fn advance(&self, gid: u16) -> u16 {
        self.advances
            .get(gid as usize)
            .or(self.advances.last())
            .copied()
            .unwrap_or(self.metrics.units_per_em / 2)
    }

    pub fn has_kerning(&self) -> bool {
        !self.kerning.is_empty()
    }

    /// Kerning between two glyphs in font units, negative to tighten.
    pub fn kerning(&self, left: u16, right: u16) -> i16 {
        if self.kerning.is_empty() {
            return 0;
        }
        let mut cache = self
            .kern_cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        *cache
            .entry((left, right))
            .or_insert_with(|| self.kerning.pair(&self.data, left, right))
    }

    /// The outline of a glyph in font units, y up, cached per glyph.
    pub fn outline(&self, gid: u16) -> Arc<[Seg]> {
        let mut cache = self.outlines.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(found) = cache.get(&gid) {
            return found.clone();
        }
        let mut segs = Vec::new();
        if let Some(cff) = &self.cff {
            segs = cff.glyph_path(gid);
        }
        if !self.loca.is_empty() {
            Glyf {
                data: &self.data,
                loca: &self.loca,
            }
            .outline(gid, &mut segs);
        }
        let segs: Arc<[Seg]> = segs.into();
        cache.insert(gid, segs.clone());
        segs
    }
}

fn read_metrics(
    data: &[u8],
    upem: u16,
    hhea: Option<usize>,
    os2: Option<usize>,
    post: Option<usize>,
) -> Metrics {
    let em = f32::from(upem);
    let hhea_ascent = hhea.and_then(|o| i16_at(data, o + 4)).map(f32::from);
    let hhea_descent = hhea
        .and_then(|o| i16_at(data, o + 6))
        .map(|d| -f32::from(d));
    let hhea_gap = hhea
        .and_then(|o| i16_at(data, o + 8))
        .map(f32::from)
        .unwrap_or(0.0);
    let win_ascent = os2
        .and_then(|o| u16_at(data, o + 74))
        .map(f32::from)
        .filter(|&v| v > 0.0);
    let win_descent = os2
        .and_then(|o| u16_at(data, o + 76))
        .map(f32::from)
        .filter(|&v| v > 0.0);
    let version = os2.and_then(|o| u16_at(data, o)).unwrap_or(0);
    let x_height = os2
        .filter(|_| version >= 2)
        .and_then(|o| i16_at(data, o + 86))
        .map(f32::from)
        .filter(|&v| v > 0.0);
    let cap_height = os2
        .filter(|_| version >= 2)
        .and_then(|o| i16_at(data, o + 88))
        .map(f32::from)
        .filter(|&v| v > 0.0);
    let strike_size = os2
        .and_then(|o| i16_at(data, o + 26))
        .map(f32::from)
        .filter(|&v| v > 0.0);
    let strike_pos = os2
        .and_then(|o| i16_at(data, o + 28))
        .map(f32::from)
        .filter(|&v| v > 0.0);
    let underline_position = post
        .and_then(|o| i16_at(data, o + 8))
        .map(|v| -f32::from(v))
        .filter(|&v| v > 0.0);
    let underline_thickness = post
        .and_then(|o| i16_at(data, o + 10))
        .map(f32::from)
        .filter(|&v| v > 0.0);
    let line_gap = external_leading(
        win_ascent.zip(win_descent),
        (
            hhea_ascent.unwrap_or(0.0),
            hhea_descent.unwrap_or(0.0).abs(),
            hhea_gap,
        ),
    );
    let ascent = win_ascent.or(hhea_ascent).unwrap_or(em * 0.8);
    let descent = win_descent.or(hhea_descent).unwrap_or(em * 0.2).abs();
    let x_height = x_height.unwrap_or(em * 0.5);
    Metrics {
        units_per_em: upem,
        ascent,
        descent,
        line_gap,
        cap_height: cap_height.unwrap_or(em * 0.7),
        x_height,
        underline_position: underline_position.unwrap_or(em * 0.1),
        underline_thickness: underline_thickness.unwrap_or(em * 0.05),
        strikeout_position: strike_pos.unwrap_or(x_height * 0.5),
        strikeout_thickness: strike_size.or(underline_thickness).unwrap_or(em * 0.05),
    }
}

/// The leading Word adds below a line set in the Windows metrics: the part
/// of the `hhea` line gap that `usWinAscent + usWinDescent` does not already
/// cover beyond the `hhea` ascender and descender. Arial gains 67 units of
/// 2048 this way (12 pt lines are 13.8 pt, not 13.4 pt); Calibri, whose
/// Windows metrics absorb its line gap, gains none.
pub(crate) fn external_leading(win: Option<(f32, f32)>, hhea: (f32, f32, f32)) -> f32 {
    let (hhea_ascent, hhea_descent, hhea_gap) = hhea;
    let Some((win_ascent, win_descent)) = win else {
        return hhea_gap.max(0.0);
    };
    let covered = (win_ascent + win_descent) - (hhea_ascent + hhea_descent);
    (hhea_gap - covered.max(0.0)).max(0.0)
}

fn read_advances(
    data: &[u8],
    hhea: Option<usize>,
    hmtx: Option<(usize, usize)>,
    num_glyphs: u16,
) -> Vec<u16> {
    let (Some(hhea), Some((hmtx, length))) = (hhea, hmtx) else {
        return Vec::new();
    };
    let long = u16_at(data, hhea + 34).unwrap_or(0).min(num_glyphs.max(1)) as usize;
    let long = long.min(length / 4);
    let mut advances: Vec<u16> = (0..long)
        .map_while(|i| u16_at(data, hmtx + i * 4))
        .collect();
    let last = advances.last().copied().unwrap_or(0);
    advances.resize((num_glyphs as usize).max(advances.len()), last);
    advances
}

fn read_loca(
    data: &[u8],
    head: usize,
    loca: (usize, usize),
    glyf: (usize, usize),
    num_glyphs: u16,
) -> Vec<u32> {
    let long = i16_at(data, head + 50).unwrap_or(0) != 0;
    let (base, length) = loca;
    let count = num_glyphs as usize + 1;
    let end = (glyf.0 + glyf.1).min(data.len()) as u32;
    let offsets: Vec<u32> = (0..count)
        .map_while(|i| {
            if long {
                return (i * 4 + 4 <= length)
                    .then(|| u32_at(data, base + i * 4))
                    .flatten();
            }
            (i * 2 + 2 <= length)
                .then(|| u16_at(data, base + i * 2).map(|v| u32::from(v) * 2))
                .flatten()
        })
        .map(|o| (glyf.0 as u32).saturating_add(o).min(end))
        .collect();
    if offsets.len() < 2 {
        return Vec::new();
    }
    offsets
}

#[cfg(test)]
mod leading_tests {
    use super::external_leading;

    #[test]
    fn arial_keeps_the_uncovered_part_of_its_line_gap() {
        assert_eq!(
            external_leading(Some((1854.0, 434.0)), (1854.0, 434.0, 67.0)),
            67.0
        );
    }

    #[test]
    fn calibri_windows_metrics_absorb_its_line_gap() {
        assert_eq!(
            external_leading(Some((1950.0, 550.0)), (1536.0, 512.0, 452.0)),
            0.0
        );
    }

    #[test]
    fn without_windows_metrics_the_hhea_gap_counts() {
        assert_eq!(external_leading(None, (800.0, 200.0, 90.0)), 90.0);
    }
}

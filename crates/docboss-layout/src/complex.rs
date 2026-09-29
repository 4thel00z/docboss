//! Shaping of words that need more than one glyph per character from the
//! character map: right-to-left text with mirrored characters, Arabic
//! joining forms, combining marks, and the fonts' `GSUB` and `GPOS`
//! features.

use std::sync::Arc;

use docboss_font::{Feature, FontId, ShapeInput, Shaped};

use crate::bidi::{class, joining, mirror};
use crate::shape::{Glyph, RunStyle, Shaper};
use crate::ucd::{Bidi, Joining};

const GLOBAL: u32 = 1;
const ISOL: u32 = 2;
const FINA: u32 = 4;
const MEDI: u32 = 8;
const INIT: u32 = 16;

const ARABIC_STAGES: [&[Feature]; 8] = [
    &[Feature::new(b"ccmp", GLOBAL), Feature::new(b"locl", GLOBAL)],
    &[Feature::new(b"isol", ISOL)],
    &[Feature::new(b"fina", FINA)],
    &[Feature::new(b"medi", MEDI)],
    &[Feature::new(b"init", INIT)],
    &[Feature::new(b"rlig", GLOBAL)],
    &[Feature::new(b"calt", GLOBAL)],
    &[Feature::new(b"liga", GLOBAL), Feature::new(b"clig", GLOBAL)],
];

const COMPLEX_STAGES: [&[Feature]; 1] = [&[
    Feature::new(b"ccmp", GLOBAL),
    Feature::new(b"locl", GLOBAL),
    Feature::new(b"rlig", GLOBAL),
    Feature::new(b"calt", GLOBAL),
    Feature::new(b"liga", GLOBAL),
    Feature::new(b"clig", GLOBAL),
]];

const MARK_STAGES: [&[Feature]; 1] = [&[
    Feature::new(b"ccmp", GLOBAL),
    Feature::new(b"locl", GLOBAL),
    Feature::new(b"rlig", GLOBAL),
]];

const POSITION: [Feature; 2] = [Feature::new(b"mark", GLOBAL), Feature::new(b"mkmk", GLOBAL)];
const POSITION_KERNED: [Feature; 3] = [
    Feature::new(b"kern", GLOBAL),
    Feature::new(b"mark", GLOBAL),
    Feature::new(b"mkmk", GLOBAL),
];

/// What a shaped segment is cached under: the face, script, whether pair
/// kerning applies, and each input glyph with its feature mask and mark
/// flag.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ShapeKey {
    font: FontId,
    script: [u8; 4],
    kern: bool,
    glyphs: Box<[(u16, u32, bool)]>,
}

/// One character of a word to shape.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Letter {
    pub ch: char,
    pub style: usize,
    pub glyph: Glyph,
}

/// One shaped glyph of a word, positioned in logical order: `x` is its pen
/// position from the word's start in the reading direction, `dx` and `dy`
/// its offset (y down), and `text` the characters of the cluster it starts,
/// as a byte range of the word's text.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Placed {
    pub glyph: Glyph,
    pub style: usize,
    pub ch: char,
    pub text: Option<(u16, u16)>,
    pub x: f32,
    pub dx: f32,
    pub dy: f32,
}

/// Whether a character needs a word to go through shaping: it is right to
/// left, an Arabic number or a combining mark, or is written in a script
/// whose letters join or combine.
pub(crate) fn needs_shaping(c: char) -> bool {
    if (c as u32) < 0x0300 {
        return false;
    }
    matches!(class(c), Bidi::R | Bidi::AL | Bidi::AN | Bidi::NSM)
}

/// The OpenType script tag of a character, `None` for characters common
/// to scripts.
fn script_of(c: char) -> Option<[u8; 4]> {
    let tag = match c as u32 {
        0x0041..=0x005A | 0x0061..=0x007A | 0x00C0..=0x024F | 0x1E00..=0x1EFF => b"latn",
        0x0370..=0x03FF | 0x1F00..=0x1FFF => b"grek",
        0x0400..=0x052F => b"cyrl",
        0x0590..=0x05FF | 0xFB1D..=0xFB4F => b"hebr",
        0x0600..=0x06FF | 0x0750..=0x077F | 0x0870..=0x08FF | 0xFB50..=0xFDFF | 0xFE70..=0xFEFF => {
            b"arab"
        }
        0x0700..=0x074F => b"syrc",
        0x0780..=0x07BF => b"thaa",
        0x07C0..=0x07FF => b"nko ",
        _ => return None,
    };
    Some(*tag)
}

/// Default ignorable characters, which shape to nothing: boundary
/// neutrals such as the zero width joiners, the directional marks and
/// the explicit embeddings, overrides and isolates.
fn ignorable(c: char) -> bool {
    matches!(c, '\u{200E}' | '\u{200F}' | '\u{061C}')
        || matches!(
            class(c),
            Bidi::BN
                | Bidi::LRE
                | Bidi::LRO
                | Bidi::RLE
                | Bidi::RLO
                | Bidi::PDF
                | Bidi::LRI
                | Bidi::RLI
                | Bidi::FSI
                | Bidi::PDI
        )
}

/// The joining feature of each letter of a word: isolated, final, medial or
/// initial for letters that join, from their joining types and those of the
/// letters around them, transparent marks skipped.
fn joining_masks(letters: &[Letter]) -> Vec<u32> {
    let types: Vec<Joining> = letters.iter().map(|l| joining(l.ch)).collect();
    let mut masks = vec![0u32; letters.len()];
    let mut previous: Option<Joining> = None;
    for i in 0..letters.len() {
        let kind = types[i];
        if kind == Joining::Transparent {
            continue;
        }
        let next = types[i + 1..]
            .iter()
            .copied()
            .find(|t| *t != Joining::Transparent);
        let joins_before = matches!(kind, Joining::Right | Joining::Dual | Joining::Causing)
            && matches!(
                previous,
                Some(Joining::Left | Joining::Dual | Joining::Causing)
            );
        let joins_after = matches!(kind, Joining::Left | Joining::Dual | Joining::Causing)
            && matches!(
                next,
                Some(Joining::Right | Joining::Dual | Joining::Causing)
            );
        if matches!(kind, Joining::Right | Joining::Left | Joining::Dual) {
            masks[i] = match (joins_before, joins_after) {
                (true, true) => MEDI,
                (true, false) => FINA,
                (false, true) => INIT,
                (false, false) => ISOL,
            };
        }
        previous = Some(kind);
    }
    masks
}

impl Shaper<'_> {
    /// Shapes a word's letters, all at one embedding level, `rtl` when it is
    /// odd; returns its glyphs in logical order, its width in points and its
    /// text.
    pub(crate) fn shape_word(
        &mut self,
        letters: &[Letter],
        styles: &[RunStyle],
        rtl: bool,
    ) -> (Vec<Placed>, f32, String) {
        let mut letters = letters.to_vec();
        for i in 1..letters.len() {
            if class(letters[i].ch) != Bidi::NSM {
                continue;
            }
            let base = letters[i - 1].glyph;
            if base.font == letters[i].glyph.font {
                continue;
            }
            if let Some((id, units)) = self.lookup(base.font, letters[i].ch) {
                let upem = self.upem(base.font);
                letters[i].glyph = Glyph {
                    font: base.font,
                    id,
                    advance: f32::from(units) * base.size / upem,
                    size: base.size,
                };
            }
        }
        if rtl {
            for letter in letters.iter_mut() {
                let Some(mirrored) = mirror(letter.ch) else {
                    continue;
                };
                let glyph = self.glyph(mirrored, &styles[letter.style]);
                if glyph.id != 0 && glyph.font == letter.glyph.font {
                    letter.glyph = glyph;
                }
            }
        }
        let masks = joining_masks(&letters);
        let mut text = String::with_capacity(letters.len() * 2);
        let mut offsets = Vec::with_capacity(letters.len() + 1);
        for letter in &letters {
            offsets.push(text.len() as u16);
            text.push(letter.ch);
        }
        offsets.push(text.len() as u16);

        let mut out: Vec<Placed> = Vec::with_capacity(letters.len());
        let mut pen = 0.0f32;
        let mut start = 0;
        while start < letters.len() {
            let font = letters[start].glyph.font;
            let size = letters[start].glyph.size;
            let mut end = start + 1;
            while end < letters.len()
                && letters[end].glyph.font == font
                && letters[end].glyph.size == size
            {
                end += 1;
            }
            let script = letters[start..end]
                .iter()
                .find_map(|l| script_of(l.ch))
                .unwrap_or(*b"DFLT");
            let input: Vec<(usize, ShapeInput)> = (start..end)
                .filter(|&i| !ignorable(letters[i].ch))
                .map(|i| {
                    let letter = &letters[i];
                    (
                        i,
                        ShapeInput {
                            id: letter.glyph.id,
                            cluster: (i - start) as u32,
                            mask: GLOBAL | masks[i],
                            mark: class(letter.ch) == Bidi::NSM,
                        },
                    )
                })
                .collect();
            let shaped = self.shape_segment(font, script, &input);
            let upem = self.upem(font);
            let k = size / upem;
            let base = out.len();
            let mut clusters: Vec<u32> = shaped.iter().map(|g| g.cluster).collect();
            clusters.sort_unstable();
            clusters.dedup();
            for (n, g) in shaped.iter().enumerate() {
                let first = (start + g.cluster as usize).min(end - 1);
                let letter = letters[first];
                let begins = shaped[..n].iter().all(|h| h.cluster != g.cluster);
                let ends = shaped[n + 1..].iter().all(|h| h.cluster != g.cluster);
                let next = clusters
                    .iter()
                    .copied()
                    .find(|&c| c > g.cluster)
                    .map_or(end, |c| (start + c as usize).min(end));
                let attached = g
                    .attach
                    .and_then(|a| out.get(base + a as usize))
                    .map(|t| (t.x, t.glyph.advance, t.dx, t.dy));
                let spacing = if ends {
                    styles[letter.style].spacing
                } else {
                    0.0
                };
                let advance = match attached {
                    Some(_) => 0.0,
                    None => g.advance as f32 * k + spacing,
                };
                let (x, dx, dy) = match attached {
                    Some((tx, tadvance, tdx, tdy)) => (
                        tx + if rtl { tadvance } else { 0.0 },
                        tdx + g.dx as f32 * k,
                        tdy - g.dy as f32 * k,
                    ),
                    None => (pen, g.dx as f32 * k, -(g.dy as f32) * k),
                };
                out.push(Placed {
                    glyph: Glyph {
                        font,
                        id: g.id,
                        advance,
                        size,
                    },
                    style: letter.style,
                    ch: letter.ch,
                    text: begins.then(|| (offsets[first], offsets[next])),
                    x,
                    dx,
                    dy,
                });
                pen += advance;
            }
            start = end;
        }
        (out, pen, text)
    }

    fn upem(&mut self, font: FontId) -> f32 {
        self.font(font)
            .map_or(1000.0, |f| f32::from(f.units_per_em()))
    }

    fn shape_segment(
        &mut self,
        font: FontId,
        script: [u8; 4],
        input: &[(usize, ShapeInput)],
    ) -> Arc<[Shaped]> {
        let key = ShapeKey {
            font,
            script,
            kern: self.kerning,
            glyphs: input.iter().map(|(_, g)| (g.id, g.mask, g.mark)).collect(),
        };
        if let Some(found) = self.shaped.get(&key) {
            return found.clone();
        }
        let glyphs: Vec<ShapeInput> = input.iter().map(|(_, g)| *g).collect();
        let shaped: Arc<[Shaped]> = match self.font(font) {
            Some(face) => {
                let stages: &[&[Feature]] = match &script {
                    b"arab" | b"syrc" | b"nko " => &ARABIC_STAGES,
                    b"hebr" | b"thaa" => &COMPLEX_STAGES,
                    _ => &MARK_STAGES,
                };
                let position: &[Feature] = match self.kerning {
                    true => &POSITION_KERNED,
                    false => &POSITION,
                };
                face.shape(&glyphs, script, stages, position).into()
            }
            None => glyphs
                .iter()
                .map(|g| Shaped {
                    id: g.id,
                    cluster: g.cluster,
                    advance: 0,
                    dx: 0,
                    dy: 0,
                    attach: None,
                })
                .collect(),
        };
        self.shaped.insert(key, shaped.clone());
        shaped
    }
}

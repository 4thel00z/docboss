//! Shaping a run of glyphs with the face's `GSUB` and `GPOS` features.

use std::sync::{Arc, PoisonError};

use crate::gpos::{self, Place};
use crate::gsub;
use crate::otl::{Buffer, Slot, CLASS_BASE, CLASS_MARK, MAX_GLYPHS};
use crate::Font;

/// A glyph handed to shaping: its id, the index of the character it came
/// from, the features that may touch it (bits of [`Feature::mask`]) and
/// whether its character is a combining mark, used when the face has no
/// glyph classes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShapeInput {
    pub id: u16,
    pub cluster: u32,
    pub mask: u32,
    pub mark: bool,
}

/// A shaped glyph in font units: its advance, its offset from its pen
/// position (y up), and for an attached mark the index of the glyph it
/// hangs from, the offset then measured from that glyph's origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shaped {
    pub id: u16,
    pub cluster: u32,
    pub advance: i32,
    pub dx: i32,
    pub dy: i32,
    pub attach: Option<u32>,
}

/// A feature applied to the glyphs whose mask shares a bit with `mask`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Feature {
    pub tag: [u8; 4],
    pub mask: u32,
}

impl Feature {
    pub const fn new(tag: &[u8; 4], mask: u32) -> Feature {
        Feature { tag: *tag, mask }
    }
}

pub(crate) type ScriptFeatures = Arc<Vec<([u8; 4], Vec<u16>)>>;

impl Font {
    fn script_features(&self, positioning: bool, script: [u8; 4]) -> ScriptFeatures {
        let table = match positioning {
            true => self.gpos(),
            false => self.gsub(),
        };
        let Some(table) = table else {
            return Arc::default();
        };
        let mut cache = self
            .script_cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        cache
            .entry((positioning, script))
            .or_insert_with(|| Arc::new(table.script_features(&self.data, script)))
            .clone()
    }

    /// Whether the face's language system for `script` has any of `tags`
    /// in `GSUB`.
    pub fn substitutes(&self, script: [u8; 4], tags: &[[u8; 4]]) -> bool {
        self.script_features(false, script)
            .iter()
            .any(|(tag, lookups)| !lookups.is_empty() && (tags.contains(tag) || tag == b"    "))
    }

    /// The lookups of `features` for `script`, in lookup order, each with the
    /// mask of the features that name it; the required feature joins the
    /// first stage for every glyph.
    fn plan(
        &self,
        positioning: bool,
        script: [u8; 4],
        features: &[Feature],
        first: bool,
    ) -> Vec<(u16, u32)> {
        let available = self.script_features(positioning, script);
        let mut lookups: Vec<(u16, u32)> = Vec::new();
        for (tag, indices) in available.iter() {
            let mask = match tag == b"    " {
                true if first => u32::MAX,
                true => continue,
                false => features
                    .iter()
                    .filter(|f| f.tag == *tag)
                    .fold(0, |m, f| m | f.mask),
            };
            if mask == 0 {
                continue;
            }
            for &index in indices {
                match lookups.iter_mut().find(|(i, _)| *i == index) {
                    Some(found) => found.1 |= mask,
                    None => lookups.push((index, mask)),
                }
            }
        }
        lookups.sort_unstable_by_key(|&(index, _)| index);
        lookups
    }

    /// Shapes glyphs for `script`: the `substitute` stages of `GSUB`
    /// features in order, each stage's lookups in lookup order, then the
    /// `position` features of `GPOS`, or pair kerning from the `kern`
    /// table when the face has no `GPOS` and `kern` is asked for.
    /// Advances start from `hmtx`; an attached mark advances nothing.
    pub fn shape(
        &self,
        input: &[ShapeInput],
        script: [u8; 4],
        substitute: &[&[Feature]],
        position: &[Feature],
    ) -> Vec<Shaped> {
        let mut buf = Buffer {
            data: &self.data,
            gdef: &self.gdef,
            slots: Vec::with_capacity(input.len()),
            next_lig_id: 0,
        };
        for glyph in input.iter().take(MAX_GLYPHS) {
            let hint = if glyph.mark { CLASS_MARK } else { CLASS_BASE };
            let class = buf.class_for(glyph.id, hint);
            buf.slots.push(Slot {
                id: glyph.id,
                cluster: glyph.cluster,
                mask: glyph.mask,
                class,
                lig_id: 0,
                component: 0,
            });
        }
        if let Some(gsub) = self.gsub() {
            for (stage, features) in substitute.iter().enumerate() {
                for (index, mask) in self.plan(false, script, features, stage == 0) {
                    gsub::apply_lookup(gsub, &mut buf, index, mask);
                }
            }
        }
        let mut places: Vec<Place> = buf
            .slots
            .iter()
            .map(|s| Place {
                advance: i32::from(self.advance(s.id)),
                ..Place::default()
            })
            .collect();
        match self.gpos() {
            Some(gpos) => {
                for (index, mask) in self.plan(true, script, position, true) {
                    gpos::apply_lookup(gpos, &buf, &mut places, index, mask);
                }
            }
            None if position.iter().any(|f| &f.tag == b"kern") && !self.kerning.is_empty() => {
                let bases: Vec<usize> = (0..buf.slots.len())
                    .filter(|&i| buf.slots[i].class != CLASS_MARK)
                    .collect();
                for pair in bases.windows(2) {
                    let (a, b) = (buf.slots[pair[0]].id, buf.slots[pair[1]].id);
                    places[pair[0]].advance += i32::from(self.kerning.pair(&self.data, a, b));
                }
            }
            None => {}
        }
        buf.slots
            .iter()
            .zip(&places)
            .map(|(slot, place)| Shaped {
                id: slot.id,
                cluster: slot.cluster,
                advance: match place.attach.is_some() && slot.class == CLASS_MARK {
                    true => 0,
                    false => place.advance,
                },
                dx: place.dx,
                dy: place.dy,
                attach: place.attach,
            })
            .collect()
    }
}

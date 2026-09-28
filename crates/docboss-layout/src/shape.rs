//! Resolved run styles and glyph selection: which face each character
//! uses, its glyph and its advance.

use std::collections::HashMap;
use std::sync::Arc;

use docboss_font::{Font, FontDatabase, FontId};
use docboss_model::{Color, FontEntry, RunProperties, Underline, VerticalAlign};

/// The effective style of a run, in points.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RunStyle {
    pub ascii: Option<String>,
    pub high_ansi: Option<String>,
    pub east_asia: Option<String>,
    pub complex: Option<String>,
    pub bold: bool,
    pub italic: bool,
    /// The size glyphs are drawn at, after superscript or subscript.
    pub size: f32,
    /// The run's own size, which line metrics use.
    pub base_size: f32,
    pub color: Color,
    pub underline: Option<Underline>,
    pub strike: bool,
    pub double_strike: bool,
    pub caps: bool,
    pub small_caps: bool,
    pub highlight: Option<Color>,
    pub shading: Option<Color>,
    /// Baseline shift in points, positive upwards.
    pub rise: f32,
    /// Extra space after each character in points.
    pub spacing: f32,
    pub hidden: bool,
}

impl RunStyle {
    /// ECMA-376 Part 1 §17.3.2: run properties to drawing parameters;
    /// superscript and subscript draw at two thirds of the size, raised by
    /// a third or lowered by a seventh of the em.
    pub(crate) fn from_properties(p: &RunProperties) -> RunStyle {
        let base_size = p.size_points().clamp(1.0, 1638.0);
        let (size, script_rise) = match p.vertical_align {
            Some(VerticalAlign::Superscript) => (base_size * 2.0 / 3.0, base_size / 3.0),
            Some(VerticalAlign::Subscript) => (base_size * 2.0 / 3.0, -base_size / 7.0),
            _ => (base_size, 0.0),
        };
        let underline = p.underline.filter(|u| *u != Underline::None);
        RunStyle {
            ascii: p.fonts.ascii.clone(),
            high_ansi: p.fonts.high_ansi.clone(),
            east_asia: p.fonts.east_asia.clone(),
            complex: p.fonts.complex.clone(),
            bold: p.is_bold(),
            italic: p.is_italic(),
            size,
            base_size,
            color: p.color.flatten().unwrap_or(Color::BLACK),
            underline,
            strike: p.strike.unwrap_or(false),
            double_strike: p.double_strike.unwrap_or(false),
            caps: p.caps.unwrap_or(false),
            small_caps: p.small_caps.unwrap_or(false),
            highlight: p.highlight,
            shading: p.shading.and_then(|s| s.fill),
            rise: script_rise + p.position.unwrap_or(0) as f32 / 2.0,
            spacing: p.spacing.unwrap_or(0) as f32 / 20.0,
            hidden: p.is_hidden(),
        }
    }

    fn slot(&self, c: char) -> Option<&str> {
        let name = match c as u32 {
            0..=0x7F => self.ascii.as_deref(),
            0x0590..=0x08FF | 0xFB1D..=0xFDFF | 0xFE70..=0xFEFF => {
                self.complex.as_deref().or(self.ascii.as_deref())
            }
            0x1100..=0x11FF
            | 0x2E80..=0x9FFF
            | 0xAC00..=0xD7AF
            | 0xF900..=0xFAFF
            | 0xFF00..=0xFFEF
            | 0x20000.. => self.east_asia.as_deref().or(self.high_ansi.as_deref()),
            _ => self.high_ansi.as_deref().or(self.ascii.as_deref()),
        };
        name.or(self.ascii.as_deref())
    }
}

/// A glyph chosen for one character.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Glyph {
    pub font: FontId,
    pub id: u16,
    /// Advance at the drawn size, in points, spacing included.
    pub advance: f32,
    pub size: f32,
}

/// Vertical metrics of a face at a size, in points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LineMetrics {
    pub ascent: f32,
    pub descent: f32,
    pub underline_position: f32,
    pub underline_thickness: f32,
    pub strikeout_position: f32,
    pub strikeout_thickness: f32,
}

/// Face selection and glyph lookup, cached for one layout.
pub(crate) struct Shaper<'a> {
    db: &'a FontDatabase,
    entries: &'a [FontEntry],
    default_font: String,
    pub kerning: bool,
    selected: HashMap<(String, bool, bool), Option<FontId>>,
    loaded: HashMap<FontId, Arc<Font>>,
    glyphs: HashMap<(FontId, char), Option<(u16, u16)>>,
}

impl<'a> Shaper<'a> {
    pub(crate) fn new(
        db: &'a FontDatabase,
        entries: &'a [FontEntry],
        default_font: &str,
        kerning: bool,
    ) -> Self {
        Shaper {
            db,
            entries,
            default_font: default_font.to_string(),
            kerning,
            selected: HashMap::new(),
            loaded: HashMap::new(),
            glyphs: HashMap::new(),
        }
    }

    pub(crate) fn font(&mut self, id: FontId) -> Option<Arc<Font>> {
        if let Some(found) = self.loaded.get(&id) {
            return Some(found.clone());
        }
        let font = self.db.font(id)?;
        self.loaded.insert(id, font.clone());
        Some(font)
    }

    fn select(&mut self, name: Option<&str>, bold: bool, italic: bool) -> Option<FontId> {
        let name = name.unwrap_or(&self.default_font).to_string();
        let key = (name, bold, italic);
        if let Some(found) = self.selected.get(&key) {
            return *found;
        }
        let class = self
            .entries
            .iter()
            .find(|e| e.name.eq_ignore_ascii_case(&key.0))
            .and_then(|e| e.family.as_deref());
        let found = self.db.select(&key.0, class, bold, italic);
        self.selected.insert(key, found);
        found
    }

    fn lookup(&mut self, font: FontId, c: char) -> Option<(u16, u16)> {
        if let Some(found) = self.glyphs.get(&(font, c)) {
            return *found;
        }
        let face = self.font(font)?;
        let found = face.glyph_index(c).map(|g| (g, face.advance(g)));
        self.glyphs.insert((font, c), found);
        found
    }

    /// The primary face of a style, for metrics of empty lines.
    pub(crate) fn primary(&mut self, style: &RunStyle) -> Option<FontId> {
        self.select(style.ascii.as_deref(), style.bold, style.italic)
    }

    pub(crate) fn metrics(&mut self, font: Option<FontId>, size: f32) -> LineMetrics {
        let face = font.and_then(|f| self.font(f));
        let Some(face) = face else {
            return LineMetrics {
                ascent: size * 0.9,
                descent: size * 0.25,
                underline_position: size * 0.1,
                underline_thickness: size * 0.05,
                strikeout_position: size * 0.25,
                strikeout_thickness: size * 0.05,
            };
        };
        let m = face.metrics();
        let k = size / f32::from(m.units_per_em);
        LineMetrics {
            ascent: m.ascent * k,
            descent: (m.descent + m.line_gap) * k,
            underline_position: m.underline_position * k,
            underline_thickness: (m.underline_thickness * k).max(0.25),
            strikeout_position: m.strikeout_position * k,
            strikeout_thickness: (m.strikeout_thickness * k).max(0.25),
        }
    }

    /// The glyph for one character in a style: the style's face, else a
    /// fallback face that has the character, else the face's missing glyph.
    pub(crate) fn glyph(&mut self, c: char, style: &RunStyle) -> Glyph {
        let lower = style.small_caps && !style.caps && c.is_lowercase();
        let size = if lower { style.size * 0.8 } else { style.size };
        let shown = if style.caps || lower {
            c.to_uppercase().next().unwrap_or(c)
        } else {
            c
        };
        let primary = self.select(style.slot(shown), style.bold, style.italic);
        let hit = primary.and_then(|f| self.lookup(f, shown).map(|g| (f, g)));
        let hit = hit.or_else(|| {
            let fallback = self.db.fallback(shown, style.bold, style.italic)?;
            self.lookup(fallback, shown).map(|g| (fallback, g))
        });
        let Some((font, (id, units))) = hit else {
            let font = primary.unwrap_or(FontId(u32::MAX));
            let upem = self
                .font(font)
                .map_or(1000.0, |f| f32::from(f.units_per_em()));
            let advance = self
                .font(font)
                .map_or(size * 0.5, |f| f32::from(f.advance(0)) * size / upem);
            return Glyph {
                font,
                id: 0,
                advance: advance + style.spacing,
                size,
            };
        };
        let upem = self
            .font(font)
            .map_or(1000.0, |f| f32::from(f.units_per_em()));
        Glyph {
            font,
            id,
            advance: f32::from(units) * size / upem + style.spacing,
            size,
        }
    }

    /// Pair kerning between two glyphs of one face, in points.
    pub(crate) fn kern(&mut self, left: &Glyph, right: &Glyph) -> f32 {
        if !self.kerning || left.font != right.font || left.size != right.size {
            return 0.0;
        }
        let Some(face) = self.font(left.font) else {
            return 0.0;
        };
        f32::from(face.kerning(left.id, right.id)) * left.size / f32::from(face.units_per_em())
    }

    /// Whether the face lacks the weight or slant the style asks for.
    pub(crate) fn synthetic(&self, font: FontId, style: &RunStyle) -> (bool, bool) {
        let Some(info) = self.db.info(font) else {
            return (false, false);
        };
        (
            style.bold && !info.style.is_bold(),
            style.italic && !info.style.italic,
        )
    }
}

//! Single lines of text placed at a point, for the labels of charts and
//! the pieces of equations.

use docboss_model::{Color, FontSlots, RunProperties};

use crate::flow::Ctx;
use crate::shape::RunStyle;
use crate::{GlyphRun, Item, PositionedGlyph};

/// The style of a label in `font` at `size` points.
pub(crate) fn style(
    font: Option<&str>,
    size: f32,
    bold: bool,
    italic: bool,
    color: Color,
) -> RunStyle {
    let font = font.map(str::to_string);
    let props = RunProperties {
        fonts: FontSlots {
            ascii: font.clone(),
            high_ansi: font.clone(),
            east_asia: font.clone(),
            complex: font,
        },
        size: Some((size * 2.0).round().clamp(2.0, 3276.0) as u32),
        bold: Some(bold),
        italic: Some(italic),
        color: Some(Some(color)),
        ..RunProperties::default()
    };
    let mut style = RunStyle::from_properties(&props);
    style.size = size.clamp(0.5, 1638.0);
    style.base_size = style.size;
    style
}

/// The advance of `text` in `style`, in points.
pub(crate) fn width(ctx: &mut Ctx<'_>, text: &str, style: &RunStyle) -> f32 {
    text.chars()
        .map(|c| ctx.shaper.glyph(c, style).advance)
        .sum()
}

/// The ascent and descent of `style`'s face at its size, in points.
pub(crate) fn extent(ctx: &mut Ctx<'_>, style: &RunStyle) -> (f32, f32) {
    let font = ctx.shaper.primary(style);
    let metrics = ctx.shaper.metrics(font, style.size);
    (metrics.ascent, metrics.descent)
}

/// Adds the glyphs of `text` in `style` from `x` on `baseline` to `items`
/// and returns its advance.
pub(crate) fn draw(
    ctx: &mut Ctx<'_>,
    items: &mut Vec<Item>,
    text: &str,
    style: &RunStyle,
    x: f32,
    baseline: f32,
) -> f32 {
    let mut pen = x;
    for c in text.chars() {
        let glyph = ctx.shaper.glyph(c, style);
        let (bold, italic) = ctx.shaper.synthetic(glyph.font, style);
        let positioned = PositionedGlyph {
            id: glyph.id,
            x: pen,
            y: 0.0,
        };
        pen += glyph.advance;
        if let Some(Item::Glyphs(run)) = items.last_mut() {
            let same = run.font == glyph.font
                && run.size == glyph.size
                && run.color == style.color
                && run.baseline == baseline
                && run.synthetic_bold == bold
                && run.synthetic_italic == italic;
            if same {
                run.glyphs.push(positioned);
                run.text.push(c);
                continue;
            }
        }
        items.push(Item::Glyphs(GlyphRun {
            font: glyph.font,
            size: glyph.size,
            color: style.color,
            baseline,
            glyphs: vec![positioned],
            text: c.to_string(),
            synthetic_bold: bold,
            synthetic_italic: italic,
        }));
    }
    pen - x
}

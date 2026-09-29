//! Text boxes: the blocks a drawing carries, laid out inside its extent,
//! over the shape's fill and under its outline.

use docboss_model::{Drawing, VerticalAlign};

use crate::flow::{layout_blocks, stack_height, Ctx};
use crate::units::emu_to_pt;
use crate::{Item, LineStyle, Rect};

const DEFAULT_INSETS: [i64; 4] = [91_440, 45_720, 91_440, 45_720];
const DEFAULT_OUTLINE: i64 = 9_525;

/// A laid-out text box: its items relative to the drawing's top-left
/// corner, and its height, which is its text's when the shape fits its
/// text.
pub(crate) struct TextBox {
    pub items: Vec<Item>,
    pub height: f32,
}

/// Lays out a drawing's text box. ECMA-376 Part 1 §20.4.2.38 (`txbxContent`)
/// supplies the blocks and §20.4.2.22 (`wps:bodyPr`) the insets, the
/// vertical anchor and `a:spAutoFit`, which sizes the shape to its text;
/// the fill and outline come from
/// `wps:spPr` (§20.4.2.35). Text that does not fit is cut off at the shape
/// less its insets: Word and LibreOffice clip text box content whatever
/// `vertOverflow` and `horzOverflow` say, so their `overflow` default is not
/// followed. A box whose stated height leaves no room inside its insets is
/// left unclipped.
pub(crate) fn layout_text_box(ctx: &mut Ctx<'_>, drawing: &Drawing) -> Option<TextBox> {
    if drawing.text_box.is_empty() {
        return None;
    }
    let shape = drawing.shape;
    let width = emu_to_pt(drawing.width).max(1.0);
    let [left, top, right, bottom] = shape.insets.unwrap_or(DEFAULT_INSETS).map(emu_to_pt);
    let inner = (width - left - right).max(1.0);
    let behind = shape.fill.or(ctx.background);
    let outer = std::mem::replace(&mut ctx.background, behind);
    let (slabs, trailing) = layout_blocks(ctx, &drawing.text_box, inner);
    ctx.background = outer;
    let content = stack_height(&slabs) + trailing;
    let stated = emu_to_pt(drawing.height).max(0.0);
    let height = match shape.auto_fit {
        true => content + top + bottom,
        false => stated,
    };
    let room = height - top - bottom;
    let shift = match shape.text_anchor {
        Some(VerticalAlign::Center) => ((room - content) / 2.0).max(0.0),
        Some(VerticalAlign::Bottom) => (room - content).max(0.0),
        _ => 0.0,
    };
    let mut items = Vec::new();
    if let Some(color) = shape.fill {
        items.push(Item::Rect {
            rect: Rect::new(0.0, 0.0, width, height),
            color,
        });
    }
    let text_width = width - left - right;
    let clipped = room > 0.0 && text_width > 0.0;
    if clipped {
        items.push(Item::ClipBegin(Rect::new(left, top, text_width, room)));
    }
    let mut y = top + shift;
    for slab in slabs {
        y += slab.gap_before;
        items.extend(slab.items.into_iter().map(|mut item| {
            item.offset(left, y);
            item
        }));
        y += slab.height;
    }
    if clipped {
        items.push(Item::ClipEnd);
    }
    if let Some(color) = shape.outline {
        let line = emu_to_pt(shape.outline_width.unwrap_or(DEFAULT_OUTLINE)).max(0.25);
        let style = shape.outline_dash.map_or(LineStyle::Solid, LineStyle::Dash);
        items.push(Item::Outline {
            rect: Rect::new(0.0, 0.0, width, height),
            width: line,
            color,
            style,
            cap: shape.outline_cap,
            join: shape.outline_join,
        });
    }
    Some(TextBox { items, height })
}

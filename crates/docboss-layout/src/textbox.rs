//! Text boxes: the blocks a drawing carries, laid out inside its extent,
//! over the shape's fill and under its outline.

use docboss_model::{Drawing, PathFill, TextDirection, VerticalAlign};

use crate::chart::chart_items;
use crate::drawn::{compose, shape_items, transform};
use crate::flow::{layout_blocks, stack_height, Ctx};
use crate::geometry::outline;
use crate::units::emu_to_pt;
use crate::{Item, Rect};

const DEFAULT_INSETS: [i64; 4] = [91_440, 45_720, 91_440, 45_720];

/// A laid-out text box: its items relative to the drawing's top-left
/// corner, and its height, which is its text's when the shape fits its
/// text.
pub(crate) struct TextBox {
    pub items: Vec<Item>,
    pub height: f32,
}

/// Lays out what a drawing paints besides its picture: a chart, a text
/// box with its shape, or a shape without text (ECMA-376 Part 1 §20.1.9).
/// `None` for a picture, and for a text box without text.
pub(crate) fn layout_drawing(ctx: &mut Ctx<'_>, drawing: &Drawing) -> Option<TextBox> {
    if let Some(chart) = &drawing.chart {
        let width = emu_to_pt(drawing.width).max(0.0);
        let height = emu_to_pt(drawing.height).max(0.0);
        let items = chart_items(ctx, chart, width, height);
        return Some(TextBox { items, height });
    }
    if !drawing.members.is_empty() {
        return Some(layout_group(ctx, drawing));
    }
    if !drawing.text_box.is_empty() {
        return layout_text_box(ctx, drawing);
    }
    drawing.geometry.as_ref()?;
    let width = emu_to_pt(drawing.width).max(0.0);
    let height = emu_to_pt(drawing.height).max(0.0);
    let shape = shape_items(ctx, drawing, width, height);
    let mut items = shape.under;
    items.extend(shape.over);
    Some(TextBox { items, height })
}

/// Lays out the members of a group or drawing canvas (ECMA-376 Part 1
/// §20.4.2.39, §20.4.2.41, §20.4.2.32) in their order, each at its box in
/// the group: its picture, then its shape and text.
fn layout_group(ctx: &mut Ctx<'_>, drawing: &Drawing) -> TextBox {
    let mut items = Vec::new();
    for member in &drawing.members {
        let (x, y) = (emu_to_pt(member.x), emu_to_pt(member.y));
        let inner = &member.drawing;
        let mut own = picture_items(inner);
        own.extend(layout_drawing(ctx, inner).map_or_else(Vec::new, |content| content.items));
        items.extend(own.into_iter().map(|mut item| {
            item.offset(x, y);
            item
        }));
    }
    TextBox {
        items,
        height: emu_to_pt(drawing.height).max(0.0),
    }
}

/// A drawing's picture relative to its top-left corner, flipped and
/// turned about its center as `a:xfrm` says (ECMA-376 Part 1 §20.1.7.6),
/// and confined to the drawing's geometry when that is not a rectangle
/// (a `a:blipFill` shape, §20.1.8.14); empty for a drawing without one.
pub(crate) fn picture_items(drawing: &Drawing) -> Vec<Item> {
    if drawing.media.is_none() {
        return Vec::new();
    }
    let (width, height) = (
        emu_to_pt(drawing.width).max(0.0),
        emu_to_pt(drawing.height).max(0.0),
    );
    let rect = Rect::new(0.0, 0.0, width, height);
    let outline = drawing
        .geometry
        .as_ref()
        .filter(|g| !g.is_rectangle())
        .and_then(|g| {
            outline(g, width, height)
                .0
                .paths
                .into_iter()
                .find(|path| path.fill != PathFill::None)
        });
    let image = match outline {
        Some(path) => Item::Picture {
            segs: path.segs,
            media: drawing.media,
            rect,
        },
        None => Item::Image {
            media: drawing.media,
            rect,
        },
    };
    let shape = drawing.shape;
    if shape.rotation == 0 && !shape.flip_horizontal && !shape.flip_vertical {
        return vec![image];
    }
    let map = transform(
        width,
        height,
        shape.rotation,
        shape.flip_horizontal,
        shape.flip_vertical,
    );
    vec![Item::TransformBegin(map), image, Item::TransformEnd]
}

/// The map from a text box's text frame into the shape's box: a quarter
/// turn for vertical text (ECMA-376 Part 1 §20.1.10.83), then the shape's
/// rotation about its center unless the text stays upright (§20.4.2.22).
/// A vertical flip turns the text a further half turn, as Word and
/// LibreOffice draw it; a horizontal flip leaves it.
fn text_map(drawing: &Drawing, width: f32, height: f32) -> [f32; 6] {
    let shape = drawing.shape;
    let turn = match shape.text_direction {
        TextDirection::LeftToRight => IDENTITY,
        TextDirection::TopToBottom => [0.0, 1.0, -1.0, 0.0, width, 0.0],
        TextDirection::BottomToTop => [0.0, -1.0, 1.0, 0.0, 0.0, height],
    };
    let rotation = match shape.flip_vertical {
        true => (i64::from(shape.rotation) + 10_800_000).rem_euclid(21_600_000) as i32,
        false => shape.rotation,
    };
    if shape.text_upright || rotation == 0 {
        return turn;
    }
    compose(transform(width, height, rotation, false, false), turn)
}

const IDENTITY: [f32; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

/// Lays out a drawing's text box. ECMA-376 Part 1 §20.4.2.38 (`txbxContent`)
/// supplies the blocks and §20.4.2.22 (`wps:bodyPr`) the insets, the
/// vertical anchor and `a:spAutoFit`, which sizes the shape to its text;
/// the fill and outline come from
/// `wps:spPr` (§20.4.2.35), drawn along its geometry, whose text rectangle
/// (§20.1.9.22) narrows the text further. Text that does not fit is cut
/// off at the shape less its insets: Word and LibreOffice clip text box
/// content whatever `vertOverflow` and `horzOverflow` say, so their
/// `overflow` default is not followed. A box whose stated height leaves no
/// room inside its insets is left unclipped. Vertical text is laid out in
/// a frame as long as the box is tall, with the insets turned with it;
/// fitted to its text, the box keeps its height and takes the width of
/// its lines. Text turns with the shape unless it is upright.
fn layout_text_box(ctx: &mut Ctx<'_>, drawing: &Drawing) -> Option<TextBox> {
    let shape = drawing.shape;
    let width = emu_to_pt(drawing.width).max(1.0);
    let stated = emu_to_pt(drawing.height).max(0.0);
    let geometry_text = drawing
        .geometry
        .as_ref()
        .filter(|g| !g.is_rectangle())
        .and_then(|g| outline(g, width, stated).0.text);
    let [left, top, right, bottom] = shape.insets.unwrap_or(DEFAULT_INSETS).map(emu_to_pt);
    let [left, top, right, bottom] = match geometry_text {
        Some([l, t, r, b]) => [
            left + l.max(0.0),
            top + t.max(0.0),
            right + (width - r).max(0.0),
            bottom + (stated - b).max(0.0),
        ],
        None => [left, top, right, bottom],
    };
    let vertical = shape.text_direction != TextDirection::LeftToRight;
    let (frame_width, [fl, ft, fr, fb]) = match shape.text_direction {
        TextDirection::LeftToRight => (width, [left, top, right, bottom]),
        TextDirection::TopToBottom => (stated, [top, right, bottom, left]),
        TextDirection::BottomToTop => (stated, [bottom, left, top, right]),
    };
    let inner = (frame_width - fl - fr).max(1.0);
    let behind = shape.fill.or(ctx.background);
    let outer = std::mem::replace(&mut ctx.background, behind);
    let (slabs, trailing) = layout_blocks(ctx, &drawing.text_box, inner);
    ctx.background = outer;
    let content = stack_height(&slabs) + trailing;
    let height = match shape.auto_fit && !vertical {
        true => content + top + bottom,
        false => stated,
    };
    let width = match shape.auto_fit && vertical {
        true => (content + ft + fb).max(1.0),
        false => width,
    };
    let frame_height = if vertical { width } else { height };
    let room = frame_height - ft - fb;
    let shift = match shape.text_anchor {
        Some(VerticalAlign::Center) => ((room - content) / 2.0).max(0.0),
        Some(VerticalAlign::Bottom) => (room - content).max(0.0),
        _ => 0.0,
    };
    let drawn = shape_items(ctx, drawing, width, height);
    let mut items = drawn.under;
    let map = text_map(drawing, width, height);
    let turned = map != IDENTITY;
    if turned {
        items.push(Item::TransformBegin(map));
    }
    let text_width = frame_width - fl - fr;
    let clipped = room > 0.0 && text_width > 0.0;
    if clipped {
        items.push(Item::ClipBegin(Rect::new(fl, ft, text_width, room)));
    }
    let mut y = ft + shift;
    for slab in slabs {
        y += slab.gap_before;
        items.extend(slab.items.into_iter().map(|mut item| {
            item.offset(fl, y);
            item
        }));
        y += slab.height;
    }
    if clipped {
        items.push(Item::ClipEnd);
    }
    if turned {
        items.push(Item::TransformEnd);
    }
    items.extend(drawn.over);
    Some(TextBox { items, height })
}

//! Drawn shapes: a drawing's geometry filled and outlined, turned and
//! flipped, with the decorations at the ends of an open outline.

use docboss_font::Seg;
use docboss_model::{Color, Diagnostic, Drawing, LineEnd, LineEndKind, LineEndSize, PathFill};

use crate::flow::Ctx;
use crate::geometry::{outline, ShapePath};
use crate::units::emu_to_pt;
use crate::{Item, LineStyle, Rect, Stroke};

const DEFAULT_OUTLINE: i64 = 9_525;
/// The narrowest line whose width sizes its line ends, in points: 0.7 mm,
/// as LibreOffice sizes them.
const MIN_END_BASE: f32 = 1.984;

/// A shape's items relative to its top-left corner: the fills that go
/// under its text and the outlines that go over it.
pub(crate) struct ShapeItems {
    pub under: Vec<Item>,
    pub over: Vec<Item>,
}

/// The stroke a shape's outline is drawn with, if it has one.
fn stroke(drawing: &Drawing) -> Option<Stroke> {
    let shape = drawing.shape;
    let color = shape.outline?;
    Some(Stroke {
        width: emu_to_pt(shape.outline_width.unwrap_or(DEFAULT_OUTLINE)).max(0.25),
        color,
        style: shape.outline_dash.map_or(LineStyle::Solid, LineStyle::Dash),
        cap: shape.outline_cap,
        join: shape.outline_join,
    })
}

/// The fill of a path whose mode shades the shape's fill (ECMA-376 Part 1
/// §20.1.10.37): darker or lighter by LibreOffice's amounts.
fn shaded(color: Color, fill: PathFill) -> Option<Color> {
    let toward = |target: f32, amount: f32, c: u8| {
        (f32::from(c) + (target - f32::from(c)) * amount).round() as u8
    };
    let (target, amount) = match fill {
        PathFill::None => return None,
        PathFill::Normal => return Some(color),
        PathFill::Darken => (0.0, 0.4),
        PathFill::DarkenLess => (0.0, 0.2),
        PathFill::Lighten => (255.0, 0.4),
        PathFill::LightenLess => (255.0, 0.2),
    };
    Some(Color(
        toward(target, amount, color.0),
        toward(target, amount, color.1),
        toward(target, amount, color.2),
    ))
}

/// Lays out the shape of a drawing `width` by `height` points. A plain
/// rectangle, or no geometry, keeps the rectangle fill and outline items;
/// any other geometry (ECMA-376 Part 1 §20.1.9.18 `prstGeom`, §20.1.9.8
/// `custGeom`) becomes paths, flipped and then turned about the shape's
/// center as `a:xfrm` says (§20.1.7.6); its text is not turned. A shape
/// with a picture shows the picture in place of its fill.
pub(crate) fn shape_items(
    ctx: &mut Ctx<'_>,
    drawing: &Drawing,
    width: f32,
    height: f32,
) -> ShapeItems {
    let shape = drawing.shape;
    let stroke = stroke(drawing);
    let rotation = shape.rotation;
    let fill = shape.fill.filter(|_| drawing.media.is_none());
    let geometry = drawing
        .geometry
        .as_ref()
        .filter(|g| !(g.is_rectangle() && rotation == 0));
    let Some(geometry) = geometry else {
        let rect = Rect::new(0.0, 0.0, width, height);
        let under = fill
            .map(|color| Item::Rect { rect, color })
            .into_iter()
            .collect();
        let over = stroke
            .map(|s| Item::Outline {
                rect,
                width: s.width,
                color: s.color,
                style: s.style,
                cap: s.cap,
                join: s.join,
            })
            .into_iter()
            .collect();
        return ShapeItems { under, over };
    };
    let (mut evaluated, note) = outline(geometry, width, height);
    if let Some(note) = note {
        let diagnostic = Diagnostic::approximated("layout", note);
        if !ctx.diagnostics.contains(&diagnostic) {
            ctx.diagnostics.push(diagnostic);
        }
    }
    let map = transform(
        width,
        height,
        rotation,
        shape.flip_horizontal,
        shape.flip_vertical,
    );
    for path in &mut evaluated.paths {
        path.segs
            .iter_mut()
            .for_each(|seg| *seg = seg.transformed(map));
    }
    let mut under = Vec::new();
    let mut over = Vec::new();
    for path in &evaluated.paths {
        if let Some(color) = fill.and_then(|c| shaded(c, path.fill)) {
            under.push(Item::Path {
                segs: path.segs.clone(),
                fill: Some(color),
                stroke: None,
            });
        }
    }
    let mut ends = (shape.head_end, shape.tail_end);
    for path in evaluated.paths.into_iter().filter(|p| p.stroke) {
        let Some(stroke) = stroke else {
            break;
        };
        let (segs, heads) = line_ends(path, stroke, std::mem::take(&mut ends));
        over.push(Item::Path {
            segs,
            fill: None,
            stroke: Some(stroke),
        });
        over.extend(heads);
    }
    ShapeItems { under, over }
}

/// The map that flips a `width` by `height` shape and then turns it
/// clockwise by `rotation` 60000ths of a degree about its center.
fn transform(width: f32, height: f32, rotation: i32, flip_x: bool, flip_y: bool) -> [f32; 6] {
    let (sx, sy) = (
        if flip_x { -1.0 } else { 1.0 },
        if flip_y { -1.0 } else { 1.0 },
    );
    let angle = (f64::from(rotation) / 60_000.0).to_radians();
    let (sin, cos) = (angle.sin() as f32, angle.cos() as f32);
    let (cx, cy) = (width / 2.0, height / 2.0);
    let (a, b, c, d) = (cos * sx, sin * sx, -sin * sy, cos * sy);
    [a, b, c, d, cx - a * cx - c * cy, cy - b * cx - d * cy]
}

/// An end of a path: its point and the point it is drawn from or toward.
type End = ((f32, f32), (f32, f32));

/// The first point of a path and the point it leaves toward, and its last
/// point and the point it arrives from; `None` for a closed path.
fn ends_of(segs: &[Seg]) -> Option<(End, End)> {
    if segs.iter().any(|s| matches!(s, Seg::Close)) {
        return None;
    }
    let Some(Seg::Move(x, y)) = segs.first() else {
        return None;
    };
    let start = (*x, *y);
    let toward = |seg: &Seg| match *seg {
        Seg::Move(x, y) | Seg::Line(x, y) => (x, y),
        Seg::Quad(cx, cy, ..) => (cx, cy),
        Seg::Cubic(x1, y1, ..) => (x1, y1),
        Seg::Close => start,
    };
    let first = segs.get(1).map(toward)?;
    let (end, from) = match segs.last()? {
        Seg::Line(x, y) => {
            let previous = segs.len().checked_sub(2).and_then(|i| segs.get(i))?;
            ((*x, *y), end_point(previous))
        }
        Seg::Quad(cx, cy, x, y) => ((*x, *y), (*cx, *cy)),
        Seg::Cubic(_, _, x2, y2, x, y) => ((*x, *y), (*x2, *y2)),
        _ => return None,
    };
    Some(((start, first), (end, from)))
}

fn end_point(seg: &Seg) -> (f32, f32) {
    match *seg {
        Seg::Move(x, y)
        | Seg::Line(x, y)
        | Seg::Quad(_, _, x, y)
        | Seg::Cubic(_, _, _, _, x, y) => (x, y),
        Seg::Close => (0.0, 0.0),
    }
}

/// The unit vector from `from` to `to`, or `None` when they coincide.
fn unit(from: (f32, f32), to: (f32, f32)) -> Option<(f32, f32)> {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let length = (dx * dx + dy * dy).sqrt();
    (length > 1e-6 && length.is_finite()).then(|| (dx / length, dy / length))
}

/// Draws the line ends of an open path (ECMA-376 Part 1 §20.1.8.38
/// `headEnd` at its start, §20.1.8.57 `tailEnd` at its end), sized from
/// the line width as LibreOffice sizes them, and pulls the path's ends in
/// under a pointed end so its cap does not show past the tip.
fn line_ends(
    path: ShapePath,
    stroke: Stroke,
    (head, tail): (Option<LineEnd>, Option<LineEnd>),
) -> (Vec<Seg>, Vec<Item>) {
    let mut segs = path.segs;
    if head.is_none() && tail.is_none() {
        return (segs, Vec::new());
    }
    let Some(((start, first), (end, from))) = ends_of(&segs) else {
        return (segs, Vec::new());
    };
    let mut items = Vec::new();
    if let (Some(end_style), Some(direction)) = (head, unit(first, start)) {
        let pull = decoration(&mut items, start, direction, end_style, stroke);
        let moved = (start.0 - direction.0 * pull, start.1 - direction.1 * pull);
        segs[0] = Seg::Move(moved.0, moved.1);
    }
    if let (Some(end_style), Some(direction)) = (tail, unit(from, end)) {
        let pull = decoration(&mut items, end, direction, end_style, stroke);
        let (dx, dy) = (-direction.0 * pull, -direction.1 * pull);
        if let Some(last) = segs.last_mut() {
            *last = match *last {
                Seg::Line(x, y) => Seg::Line(x + dx, y + dy),
                Seg::Quad(cx, cy, x, y) => Seg::Quad(cx, cy, x + dx, y + dy),
                Seg::Cubic(x1, y1, x2, y2, x, y) => {
                    Seg::Cubic(x1, y1, x2 + dx, y2 + dy, x + dx, y + dy)
                }
                other => other,
            };
        }
    }
    (segs, items)
}

/// Adds the decoration of one line end at `tip`, pointing along
/// `direction`, and returns how far the line is pulled back under it.
fn decoration(
    items: &mut Vec<Item>,
    tip: (f32, f32),
    direction: (f32, f32),
    end: LineEnd,
    stroke: Stroke,
) -> f32 {
    let open = end.kind == LineEndKind::Arrow;
    let factor = |size: LineEndSize, small: f32, medium: f32, large: f32| match size {
        LineEndSize::Small => small,
        LineEndSize::Medium => medium,
        LineEndSize::Large => large,
    };
    let base = stroke.width.max(MIN_END_BASE);
    let (width, length) = match open {
        true => (
            factor(end.width, 2.5, 3.5, 5.5) * base,
            factor(end.length, 3.5, 4.5, 6.0) * base,
        ),
        false => (
            factor(end.width, 2.0, 3.0, 5.0) * base,
            factor(end.length, 2.0, 3.0, 5.0) * base,
        ),
    };
    let (ux, uy) = direction;
    let (nx, ny) = (-uy, ux);
    let at = |along: f32, across: f32| {
        (
            tip.0 + ux * along + nx * across,
            tip.1 + uy * along + ny * across,
        )
    };
    let polygon = |points: &[(f32, f32)]| {
        let mut segs: Vec<Seg> = Vec::with_capacity(points.len() + 1);
        segs.extend(points.iter().enumerate().map(|(i, p)| match i {
            0 => Seg::Move(p.0, p.1),
            _ => Seg::Line(p.0, p.1),
        }));
        segs.push(Seg::Close);
        segs
    };
    let filled = |segs: Vec<Seg>| Item::Path {
        segs,
        fill: Some(stroke.color),
        stroke: None,
    };
    let (w, l) = (width / 2.0, length);
    match end.kind {
        LineEndKind::Triangle => {
            items.push(filled(polygon(&[tip, at(-l, w), at(-l, -w)])));
            l / 2.0
        }
        LineEndKind::Stealth => {
            items.push(filled(polygon(&[
                tip,
                at(-l, w),
                at(-0.7 * l, 0.0),
                at(-l, -w),
            ])));
            l / 2.0
        }
        LineEndKind::Diamond => {
            let half = l / 2.0;
            items.push(filled(polygon(&[
                at(half, 0.0),
                at(0.0, w),
                at(-half, 0.0),
                at(0.0, -w),
            ])));
            0.0
        }
        LineEndKind::Oval => {
            let k = 0.552_284_8;
            let (rx, ry) = (l / 2.0, w);
            let p = |a: f32, b: f32| at(a, b);
            let mut segs = vec![Seg::Move(p(rx, 0.0).0, p(rx, 0.0).1)];
            let quarter = |segs: &mut Vec<Seg>, c1: (f32, f32), c2: (f32, f32), e: (f32, f32)| {
                segs.push(Seg::Cubic(c1.0, c1.1, c2.0, c2.1, e.0, e.1));
            };
            quarter(&mut segs, p(rx, k * ry), p(k * rx, ry), p(0.0, ry));
            quarter(&mut segs, p(-k * rx, ry), p(-rx, k * ry), p(-rx, 0.0));
            quarter(&mut segs, p(-rx, -k * ry), p(-k * rx, -ry), p(0.0, -ry));
            quarter(&mut segs, p(k * rx, -ry), p(rx, -k * ry), p(rx, 0.0));
            segs.push(Seg::Close);
            items.push(filled(segs));
            0.0
        }
        LineEndKind::Arrow => {
            let (a, b) = (at(-l, w), at(-l, -w));
            items.push(Item::Path {
                segs: vec![
                    Seg::Move(a.0, a.1),
                    Seg::Line(tip.0, tip.1),
                    Seg::Line(b.0, b.1),
                ],
                fill: None,
                stroke: Some(Stroke {
                    style: LineStyle::Solid,
                    ..stroke
                }),
            });
            0.0
        }
    }
}

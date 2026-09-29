//! DrawingML shape geometry: the guides of a preset or custom geometry
//! evaluated for a shape's size, and its paths built from them (ECMA-376
//! Part 1 §20.1.9).

use std::f64::consts::{PI, TAU};

use docboss_font::Seg;
pub(crate) use docboss_model::PathFill;
use docboss_model::{CustomGeometry, Geometry, PathCommand};

use crate::presets::PRESETS;

/// EMU per point.
const EMU: f64 = 12_700.0;
/// Angles are in 60000ths of a degree.
const DEGREE: f64 = 60_000.0;
/// The most guides and path commands a custom geometry is evaluated with.
const MAX_ITEMS: usize = 10_000;

/// A guide argument: a literal or a slot of the evaluated values.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Arg {
    Lit(f64),
    Slot(u16),
}

/// The guide formulas of ECMA-376 Part 1 §20.1.9.11.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Op {
    MulDiv,
    AddSub,
    AddDiv,
    IfElse,
    Abs,
    At2,
    Cat2,
    Cos,
    Max,
    Min,
    Mod,
    Pin,
    Sat2,
    Sin,
    Sqrt,
    Tan,
    Val,
}

/// A guide: its formula and arguments. Its value takes the next slot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Guide(pub Op, pub [Arg; 3]);

/// A path command over guide arguments.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Cmd {
    Move(Arg, Arg),
    Line(Arg, Arg),
    /// Width radius, height radius, start angle, swing angle.
    Arc(Arg, Arg, Arg, Arg),
    Quad(Arg, Arg, Arg, Arg),
    Cubic(Arg, Arg, Arg, Arg, Arg, Arg),
    Close,
}

/// One path of a geometry in its own `width` by `height` space (the
/// shape's when zero).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Path<'a> {
    pub width: f64,
    pub height: f64,
    pub fill: PathFill,
    pub stroke: bool,
    pub commands: &'a [Cmd],
}

/// A preset geometry: its adjust values with their defaults, its guides,
/// its paths and its text rectangle.
#[derive(Debug)]
pub(crate) struct Preset {
    pub name: &'static str,
    pub adjust: &'static [(&'static str, f64)],
    pub guides: &'static [Guide],
    pub paths: &'static [Path<'static>],
    pub text: Option<[Arg; 4]>,
}

/// The built-in guides of ECMA-376 Part 1 §20.1.10.56, in slot order; the
/// generator numbers them the same way.
const BUILTINS: usize = 41;

fn builtins(w: f64, h: f64) -> [f64; BUILTINS] {
    let (ss, ls) = (w.min(h), w.max(h));
    [
        w,
        h,
        0.0,
        0.0,
        w,
        h,
        w / 2.0,
        h / 2.0,
        w / 2.0,
        w / 3.0,
        w / 4.0,
        w / 5.0,
        w / 6.0,
        w / 8.0,
        w / 10.0,
        w / 12.0,
        w / 32.0,
        h / 2.0,
        h / 3.0,
        h / 4.0,
        h / 5.0,
        h / 6.0,
        h / 8.0,
        h / 10.0,
        h / 12.0,
        h / 32.0,
        ss,
        ls,
        ss / 2.0,
        ss / 4.0,
        ss / 6.0,
        ss / 8.0,
        ss / 16.0,
        ss / 32.0,
        10_800_000.0,
        5_400_000.0,
        2_700_000.0,
        16_200_000.0,
        8_100_000.0,
        13_500_000.0,
        18_900_000.0,
    ]
}

const BUILTIN_NAMES: [&str; BUILTINS] = [
    "w", "h", "l", "t", "r", "b", "hc", "vc", "wd2", "wd3", "wd4", "wd5", "wd6", "wd8", "wd10",
    "wd12", "wd32", "hd2", "hd3", "hd4", "hd5", "hd6", "hd8", "hd10", "hd12", "hd32", "ss", "ls",
    "ssd2", "ssd4", "ssd6", "ssd8", "ssd16", "ssd32", "cd2", "cd4", "cd8", "3cd4", "3cd8", "5cd8",
    "7cd8",
];

/// A path of a laid-out shape in points from its top-left corner.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ShapePath {
    pub segs: Vec<Seg>,
    pub fill: PathFill,
    pub stroke: bool,
}

/// A geometry evaluated for one shape: its paths and its text rectangle
/// (left, top, right, bottom in points).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Outline {
    pub paths: Vec<ShapePath>,
    pub text: Option<[f32; 4]>,
}

/// Evaluates `geometry` for a shape `width` by `height` points. The second
/// value names what could not be evaluated: an unknown preset (drawn as a
/// rectangle) or a guide name no guide defines (taken as zero).
pub(crate) fn outline(geometry: &Geometry, width: f32, height: f32) -> (Outline, Option<String>) {
    let (w, h) = (f64::from(width) * EMU, f64::from(height) * EMU);
    match geometry {
        Geometry::Preset { name, adjust } => {
            let (preset, note) = match find(name) {
                Some(preset) => (preset, None),
                None => (
                    find("rect").unwrap_or(&PRESETS[0]),
                    Some(format!(
                        "preset geometry \"{name}\" is drawn as a rectangle"
                    )),
                ),
            };
            let values: Vec<f64> = preset
                .adjust
                .iter()
                .map(|(key, default)| {
                    adjust
                        .iter()
                        .rev()
                        .find(|(given, _)| given == key)
                        .map_or(*default, |(_, value)| *value as f64)
                })
                .collect();
            let program = Program {
                adjust: values,
                guides: preset.guides,
                paths: preset.paths.to_vec(),
                text: preset.text,
            };
            (program.run(w, h), note)
        }
        Geometry::Custom(custom) => {
            let compiled = Compiled::new(custom);
            let paths = compiled
                .paths
                .iter()
                .map(|(header, commands)| Path {
                    commands,
                    ..*header
                })
                .collect();
            let program = Program {
                adjust: compiled.adjust.clone(),
                guides: &compiled.guides,
                paths,
                text: None,
            };
            let note = compiled
                .unknown
                .map(|name| format!("custom geometry guide \"{name}\" is taken as zero"));
            (program.run(w, h), note)
        }
    }
}

fn find(name: &str) -> Option<&'static Preset> {
    PRESETS
        .binary_search_by(|preset| preset.name.cmp(name))
        .ok()
        .map(|i| &PRESETS[i])
}

struct Program<'a> {
    adjust: Vec<f64>,
    guides: &'a [Guide],
    paths: Vec<Path<'a>>,
    text: Option<[Arg; 4]>,
}

impl Program<'_> {
    fn run(&self, w: f64, h: f64) -> Outline {
        let mut slots: Vec<f64> =
            Vec::with_capacity(BUILTINS + self.adjust.len() + self.guides.len());
        slots.extend(builtins(w, h));
        slots.extend(&self.adjust);
        for Guide(op, args) in self.guides {
            let [x, y, z] = args.map(|a| value(&slots, a));
            let result = evaluate(*op, x, y, z);
            slots.push(if result.is_finite() { result } else { 0.0 });
        }
        let paths = self
            .paths
            .iter()
            .map(|path| build(path, &slots, w, h))
            .collect();
        let text = self
            .text
            .map(|rect| rect.map(|a| (value(&slots, a) / EMU) as f32));
        Outline { paths, text }
    }
}

fn value(slots: &[f64], arg: Arg) -> f64 {
    match arg {
        Arg::Lit(v) => v,
        Arg::Slot(i) => slots.get(usize::from(i)).copied().unwrap_or(0.0),
    }
}

fn radians(angle: f64) -> f64 {
    angle / DEGREE * PI / 180.0
}

fn angle(radians: f64) -> f64 {
    radians * 180.0 / PI * DEGREE
}

fn ratio(x: f64, y: f64) -> f64 {
    if y == 0.0 {
        return 0.0;
    }
    x / y
}

/// ECMA-376 Part 1 §20.1.9.11: one guide formula.
fn evaluate(op: Op, x: f64, y: f64, z: f64) -> f64 {
    match op {
        Op::MulDiv => ratio(x * y, z),
        Op::AddSub => x + y - z,
        Op::AddDiv => ratio(x + y, z),
        Op::IfElse if x > 0.0 => y,
        Op::IfElse => z,
        Op::Abs => x.abs(),
        Op::At2 => angle(y.atan2(x)),
        Op::Cat2 => x * z.atan2(y).cos(),
        Op::Cos => x * radians(y).cos(),
        Op::Max => x.max(y),
        Op::Min => x.min(y),
        Op::Mod => (x * x + y * y + z * z).sqrt(),
        Op::Pin => y.clamp(x.min(z), z.max(x)),
        Op::Sat2 => x * z.atan2(y).sin(),
        Op::Sin => x * radians(y).sin(),
        Op::Sqrt => x.max(0.0).sqrt(),
        Op::Tan => x * radians(y).tan(),
        Op::Val => x,
    }
}

/// ECMA-376 Part 1 §20.1.9.15: one path's commands as outline segments in
/// points; its coordinates scale from the path's space to the shape's.
fn build(path: &Path<'_>, slots: &[f64], w: f64, h: f64) -> ShapePath {
    let kx = if path.width > 0.0 {
        w / path.width
    } else {
        1.0
    } / EMU;
    let ky = if path.height > 0.0 {
        h / path.height
    } else {
        1.0
    } / EMU;
    let v = |a: Arg| value(slots, a);
    let point = |x: Arg, y: Arg| ((v(x) * kx) as f32, (v(y) * ky) as f32);
    let mut segs = Vec::with_capacity(path.commands.len() + 4);
    let mut pen = (0.0f32, 0.0f32);
    let mut start = pen;
    for command in path.commands {
        match *command {
            Cmd::Move(x, y) => {
                pen = point(x, y);
                start = pen;
                segs.push(Seg::Move(pen.0, pen.1));
            }
            Cmd::Line(x, y) => {
                pen = point(x, y);
                segs.push(Seg::Line(pen.0, pen.1));
            }
            Cmd::Quad(x1, y1, x, y) => {
                let c = point(x1, y1);
                pen = point(x, y);
                segs.push(Seg::Quad(c.0, c.1, pen.0, pen.1));
            }
            Cmd::Cubic(x1, y1, x2, y2, x, y) => {
                let (c1, c2) = (point(x1, y1), point(x2, y2));
                pen = point(x, y);
                segs.push(Seg::Cubic(c1.0, c1.1, c2.0, c2.1, pen.0, pen.1));
            }
            Cmd::Arc(wr, hr, st, sw) => {
                let radii = (v(wr) * kx, v(hr) * ky);
                pen = arc(&mut segs, pen, radii, v(st), v(sw));
            }
            Cmd::Close => {
                segs.push(Seg::Close);
                pen = start;
            }
        }
    }
    ShapePath {
        segs,
        fill: path.fill,
        stroke: path.stroke,
    }
}

/// ECMA-376 Part 1 §20.1.9.4: an arc of the ellipse with radii `(rx, ry)`
/// on which the pen sits at the angle `start`, swinging by `swing`
/// (clockwise when positive, y growing downwards). The angles are those of
/// the rays from the ellipse's center, as Word draws them. Returns the new
/// pen position.
fn arc(
    segs: &mut Vec<Seg>,
    pen: (f32, f32),
    (rx, ry): (f64, f64),
    start: f64,
    swing: f64,
) -> (f32, f32) {
    let parametric = |angle: f64| (rx * angle.sin()).atan2(ry * angle.cos());
    let (theta0, sweep) = (radians(start), radians(swing).clamp(-TAU, TAU));
    let t0 = parametric(theta0);
    let turn = parametric(theta0 + sweep) - t0;
    let delta = match sweep {
        s if s.abs() >= TAU - 1e-9 => s.signum() * TAU,
        s if s > 1e-12 => turn.rem_euclid(TAU),
        s if s < -1e-12 => -(-turn).rem_euclid(TAU),
        _ => 0.0,
    };
    if delta == 0.0 || !(rx.is_finite() && ry.is_finite()) {
        return pen;
    }
    let (px, py) = (f64::from(pen.0), f64::from(pen.1));
    let (cx, cy) = (px - rx * t0.cos(), py - ry * t0.sin());
    let pieces = (delta.abs() / (PI / 2.0)).ceil().clamp(1.0, 8.0);
    let step = delta / pieces;
    let k = 4.0 / 3.0 * (step / 4.0).tan();
    let at = |t: f64| (cx + rx * t.cos(), cy + ry * t.sin());
    let tangent = |t: f64| (-rx * t.sin(), ry * t.cos());
    let mut end = (px, py);
    for i in 0..pieces as usize {
        let (a, b) = (t0 + step * i as f64, t0 + step * (i + 1) as f64);
        let (p0, p3) = (at(a), at(b));
        let (d0, d3) = (tangent(a), tangent(b));
        let c1 = (p0.0 + k * d0.0, p0.1 + k * d0.1);
        let c2 = (p3.0 - k * d3.0, p3.1 - k * d3.1);
        segs.push(Seg::Cubic(
            c1.0 as f32,
            c1.1 as f32,
            c2.0 as f32,
            c2.1 as f32,
            p3.0 as f32,
            p3.1 as f32,
        ));
        end = p3;
    }
    (end.0 as f32, end.1 as f32)
}

/// A custom geometry's guides and paths resolved to slots.
struct Compiled {
    adjust: Vec<f64>,
    guides: Vec<Guide>,
    paths: Vec<(Path<'static>, Vec<Cmd>)>,
    unknown: Option<String>,
}

impl Compiled {
    fn new(custom: &CustomGeometry) -> Compiled {
        let mut names: Vec<&str> = BUILTIN_NAMES.to_vec();
        let adjust: Vec<f64> = custom
            .adjust
            .iter()
            .take(MAX_ITEMS)
            .map(|(name, value)| {
                names.push(name);
                *value as f64
            })
            .collect();
        let mut unknown = None;
        let mut guides = Vec::new();
        for guide in custom.guides.iter().take(MAX_ITEMS) {
            let mut tokens = guide.formula.split_whitespace();
            let op = tokens.next().and_then(formula).unwrap_or(Op::Val);
            let mut args = [Arg::Lit(0.0); 3];
            for (slot, token) in args.iter_mut().zip(tokens) {
                *slot = resolve(&names, token, &mut unknown);
            }
            guides.push(Guide(op, args));
            names.push(&guide.name);
        }
        let mut budget = MAX_ITEMS;
        let paths = custom
            .paths
            .iter()
            .map(|path| {
                let mut arg = |token: &String| resolve(&names, token, &mut unknown);
                let commands: Vec<Cmd> = path
                    .commands
                    .iter()
                    .take(budget)
                    .map(|command| match command {
                        PathCommand::MoveTo([x, y]) => Cmd::Move(arg(x), arg(y)),
                        PathCommand::LineTo([x, y]) => Cmd::Line(arg(x), arg(y)),
                        PathCommand::ArcTo([wr, hr, st, sw]) => {
                            Cmd::Arc(arg(wr), arg(hr), arg(st), arg(sw))
                        }
                        PathCommand::QuadTo([x1, y1, x, y]) => {
                            Cmd::Quad(arg(x1), arg(y1), arg(x), arg(y))
                        }
                        PathCommand::CubicTo([x1, y1, x2, y2, x, y]) => {
                            Cmd::Cubic(arg(x1), arg(y1), arg(x2), arg(y2), arg(x), arg(y))
                        }
                        PathCommand::Close => Cmd::Close,
                    })
                    .collect();
                budget = budget.saturating_sub(commands.len());
                let header = Path {
                    width: path.width.max(0) as f64,
                    height: path.height.max(0) as f64,
                    fill: path.fill,
                    stroke: path.stroke,
                    commands: &[],
                };
                (header, commands)
            })
            .collect();
        Compiled {
            adjust,
            guides,
            paths,
            unknown,
        }
    }
}

fn formula(token: &str) -> Option<Op> {
    Some(match token {
        "*/" => Op::MulDiv,
        "+-" => Op::AddSub,
        "+/" => Op::AddDiv,
        "?:" => Op::IfElse,
        "abs" => Op::Abs,
        "at2" => Op::At2,
        "cat2" => Op::Cat2,
        "cos" => Op::Cos,
        "max" => Op::Max,
        "min" => Op::Min,
        "mod" => Op::Mod,
        "pin" => Op::Pin,
        "sat2" => Op::Sat2,
        "sin" => Op::Sin,
        "sqrt" => Op::Sqrt,
        "tan" => Op::Tan,
        "val" => Op::Val,
        _ => return None,
    })
}

/// A literal, or the slot of the latest guide named `token`.
fn resolve(names: &[&str], token: &str, unknown: &mut Option<String>) -> Arg {
    if let Ok(literal) = token.parse::<f64>() {
        return Arg::Lit(literal);
    }
    let Some(slot) = names.iter().rposition(|name| *name == token) else {
        unknown.get_or_insert_with(|| token.to_string());
        return Arg::Lit(0.0);
    };
    Arg::Slot(u16::try_from(slot).unwrap_or(u16::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;
    use docboss_model::{GeometryPath, Guide as ModelGuide};

    fn preset(name: &str, adjust: &[(&str, i64)]) -> Geometry {
        Geometry::Preset {
            name: name.into(),
            adjust: adjust.iter().map(|(n, v)| (n.to_string(), *v)).collect(),
        }
    }

    fn points(segs: &[Seg]) -> Vec<(f32, f32)> {
        segs.iter()
            .filter_map(|seg| match *seg {
                Seg::Move(x, y) | Seg::Line(x, y) => Some((x, y)),
                Seg::Cubic(_, _, _, _, x, y) | Seg::Quad(_, _, x, y) => Some((x, y)),
                Seg::Close => None,
            })
            .collect()
    }

    fn near(a: (f32, f32), b: (f32, f32)) -> bool {
        (a.0 - b.0).abs() < 0.01 && (a.1 - b.1).abs() < 0.01
    }

    /// ECMA-376 Part 1 §20.1.9.11: the seventeen guide formulas, with
    /// angles in 60000ths of a degree.
    #[test]
    fn guide_formulas() {
        assert_eq!(evaluate(Op::MulDiv, 6.0, 4.0, 3.0), 8.0);
        assert_eq!(evaluate(Op::MulDiv, 6.0, 4.0, 0.0), 0.0);
        assert_eq!(evaluate(Op::AddSub, 6.0, 4.0, 3.0), 7.0);
        assert_eq!(evaluate(Op::AddDiv, 6.0, 4.0, 2.0), 5.0);
        assert_eq!(evaluate(Op::IfElse, 1.0, 4.0, 3.0), 4.0);
        assert_eq!(evaluate(Op::IfElse, 0.0, 4.0, 3.0), 3.0);
        assert_eq!(evaluate(Op::Abs, -2.0, 0.0, 0.0), 2.0);
        assert!((evaluate(Op::At2, 1.0, 1.0, 0.0) - 2_700_000.0).abs() < 1e-6);
        assert!((evaluate(Op::Cat2, 2.0, 1.0, 1.0) - 2.0f64.sqrt()).abs() < 1e-9);
        assert!((evaluate(Op::Cos, 2.0, 5_400_000.0, 0.0)).abs() < 1e-9);
        assert!((evaluate(Op::Sin, 2.0, 5_400_000.0, 0.0) - 2.0).abs() < 1e-9);
        assert!((evaluate(Op::Tan, 2.0, 2_700_000.0, 0.0) - 2.0).abs() < 1e-9);
        assert_eq!(evaluate(Op::Max, 2.0, 3.0, 0.0), 3.0);
        assert_eq!(evaluate(Op::Min, 2.0, 3.0, 0.0), 2.0);
        assert_eq!(evaluate(Op::Mod, 2.0, 3.0, 6.0), 7.0);
        assert_eq!(evaluate(Op::Pin, 0.0, 5.0, 3.0), 3.0);
        assert_eq!(evaluate(Op::Pin, 0.0, -5.0, 3.0), 0.0);
        assert!((evaluate(Op::Sat2, 2.0, 1.0, 1.0) - 2.0f64.sqrt()).abs() < 1e-9);
        assert_eq!(evaluate(Op::Sqrt, 16.0, 0.0, 0.0), 4.0);
        assert_eq!(evaluate(Op::Val, 7.0, 0.0, 0.0), 7.0);
    }

    /// ECMA-376 Part 1 §20.1.10.56: the presets are sorted, and the right
    /// arrow's guides give its head and shaft from its adjust values,
    /// defaults or the document's.
    #[test]
    fn right_arrow_follows_its_adjust_values() {
        let unsorted: Vec<_> = PRESETS
            .windows(2)
            .filter(|w| w[0].name >= w[1].name)
            .map(|w| (w[0].name, w[1].name))
            .collect();
        assert!(unsorted.is_empty(), "{unsorted:?}");
        assert_eq!(PRESETS.len(), 186);
        let (arrow, note) = outline(&preset("rightArrow", &[]), 200.0, 100.0);
        assert_eq!(note, None);
        let corners = points(&arrow.paths[0].segs);
        let expected = [
            (0.0, 25.0),
            (150.0, 25.0),
            (150.0, 0.0),
            (200.0, 50.0),
            (150.0, 100.0),
            (150.0, 75.0),
            (0.0, 75.0),
        ];
        assert_eq!(corners.len(), expected.len());
        assert!(
            corners.iter().zip(expected).all(|(a, b)| near(*a, b)),
            "{corners:?}"
        );
        let (thin, _) = outline(
            &preset("rightArrow", &[("adj1", 20_000), ("adj2", 100_000)]),
            200.0,
            100.0,
        );
        let corners = points(&thin.paths[0].segs);
        assert!(near(corners[1], (100.0, 40.0)), "{corners:?}");
        let (unknown, note) = outline(&preset("noSuchShape", &[]), 10.0, 10.0);
        assert_eq!(points(&unknown.paths[0].segs).len(), 4);
        assert!(note.is_some());
    }

    /// ECMA-376 Part 1 §20.1.9.4: an arc starts where the pen is and turns
    /// clockwise through its swing; the ellipse preset closes on itself.
    #[test]
    fn arcs_turn_from_the_pen() {
        let (ellipse, _) = outline(&preset("ellipse", &[]), 100.0, 50.0);
        let path = &ellipse.paths[0];
        let ends = points(&path.segs);
        assert!(near(ends[0], (0.0, 25.0)));
        assert!(near(*ends.last().unwrap(), (0.0, 25.0)), "{ends:?}");
        assert!(ends.iter().any(|p| near(*p, (50.0, 0.0))), "{ends:?}");
        let mut segs = Vec::new();
        let end = arc(&mut segs, (100.0, 50.0), (100.0, 50.0), 0.0, 5_400_000.0);
        assert!(near(end, (0.0, 100.0)), "{end:?}");
        let end = arc(&mut segs, (0.0, 0.0), (10.0, 10.0), 0.0, 0.0);
        assert_eq!(end, (0.0, 0.0));
    }

    /// ECMA-376 Part 1 §20.1.9.8, §20.1.9.12, §20.1.9.15: a custom
    /// geometry's guides feed its path, in the path's own coordinate space.
    #[test]
    fn custom_geometry_scales_its_path() {
        let custom = CustomGeometry {
            adjust: vec![("adj".into(), 50_000)],
            guides: vec![ModelGuide {
                name: "mid".into(),
                formula: "*/ h adj 100000".into(),
            }],
            paths: vec![GeometryPath {
                width: 0,
                height: 0,
                fill: PathFill::Normal,
                stroke: true,
                commands: vec![
                    PathCommand::MoveTo(["0".into(), "mid".into()]),
                    PathCommand::LineTo(["w".into(), "missing".into()]),
                    PathCommand::Close,
                ],
            }],
        };
        let (shape, note) = outline(&Geometry::Custom(custom.clone()), 40.0, 20.0);
        assert_eq!(points(&shape.paths[0].segs), vec![(0.0, 10.0), (40.0, 0.0)]);
        assert!(note.unwrap().contains("missing"));
        let mut scaled = custom;
        scaled.paths[0].width = 1000;
        scaled.paths[0].commands[1] = PathCommand::LineTo(["500".into(), "0".into()]);
        let (shape, _) = outline(&Geometry::Custom(scaled), 40.0, 20.0);
        assert!(near(points(&shape.paths[0].segs)[1], (20.0, 0.0)));
    }
}

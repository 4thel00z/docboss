//! DrawingML shape geometry and line ends inside `wps:spPr`.

use docboss_model::{
    CustomGeometry, Geometry, GeometryPath, Guide, LineEnd, LineEndKind, LineEndSize, PathCommand,
    PathFill,
};
use docboss_xml::{Element, Ns, Reader};

use crate::xml::{children, int};

/// The most guides, paths and path commands one custom geometry keeps.
const MAX_ITEMS: usize = 10_000;

/// Adjust values of `a:avLst` by guide name: each `a:gd` whose formula is
/// a literal `val` (ECMA-376 Part 1 §20.1.9.5, §20.1.9.11).
fn adjust_values(reader: &mut Reader<'_>) -> Vec<(String, i64)> {
    let mut values = Vec::new();
    children(reader, |_, gd| {
        let (Some(name), Some(formula)) = (gd.attr(Ns::NONE, "name"), gd.attr(Ns::NONE, "fmla"))
        else {
            return;
        };
        let Some(value) = formula.strip_prefix("val ").and_then(int) else {
            return;
        };
        if values.len() < MAX_ITEMS {
            values.push((name.into_owned(), value));
        }
    });
    values
}

/// ECMA-376 Part 1 §20.1.9.18: `a:prstGeom`, its preset name and adjust
/// values.
pub fn preset(reader: &mut Reader<'_>, e: &Element<'_>) -> Geometry {
    let name = e
        .attr(Ns::NONE, "prst")
        .map_or_else(|| "rect".to_string(), |n| n.into_owned());
    let mut adjust = Vec::new();
    children(reader, |reader, child| {
        if child.ns == Ns::A && child.local == "avLst" {
            adjust = adjust_values(reader);
        }
    });
    Geometry::Preset { name, adjust }
}

/// ECMA-376 Part 1 §20.1.9.8: `a:custGeom`, with its adjust values, guides
/// (§20.1.9.12) and paths (§20.1.9.16, §20.1.9.15): moveTo (§20.1.9.14),
/// lnTo (§20.1.9.13), arcTo (§20.1.9.4), quadBezTo (§20.1.9.21), cubicBezTo
/// (§20.1.9.7) and close (§20.1.9.6) over pt coordinates (§20.1.9.20).
/// Returns a note when items past the cap were dropped.
pub fn custom(reader: &mut Reader<'_>) -> (Geometry, Option<String>) {
    let mut geometry = CustomGeometry::default();
    let mut dropped = false;
    let mut budget = MAX_ITEMS;
    children(reader, |reader, list| {
        if list.ns != Ns::A {
            return;
        }
        match list.local {
            "avLst" => geometry.adjust = adjust_values(reader),
            "gdLst" => children(reader, |_, gd| {
                let (Some(name), Some(formula)) =
                    (gd.attr(Ns::NONE, "name"), gd.attr(Ns::NONE, "fmla"))
                else {
                    return;
                };
                if geometry.guides.len() >= MAX_ITEMS {
                    dropped = true;
                    return;
                }
                geometry.guides.push(Guide {
                    name: name.into_owned(),
                    formula: formula.into_owned(),
                });
            }),
            "pathLst" => children(reader, |reader, path| {
                if path.ns != Ns::A || path.local != "path" {
                    return;
                }
                let dimension = |name: &str| {
                    path.attr_raw(Ns::NONE, name)
                        .and_then(int)
                        .unwrap_or(0)
                        .max(0)
                };
                let fill = match path.attr_raw(Ns::NONE, "fill") {
                    Some("none") => PathFill::None,
                    Some("lighten") => PathFill::Lighten,
                    Some("lightenLess") => PathFill::LightenLess,
                    Some("darken") => PathFill::Darken,
                    Some("darkenLess") => PathFill::DarkenLess,
                    _ => PathFill::Normal,
                };
                let stroke = !matches!(path.attr_raw(Ns::NONE, "stroke"), Some("0" | "false"));
                let mut out = GeometryPath {
                    width: dimension("w"),
                    height: dimension("h"),
                    fill,
                    stroke,
                    commands: Vec::new(),
                };
                children(reader, |reader, command| {
                    if budget == 0 {
                        dropped = true;
                        return;
                    }
                    if let Some(command) = path_command(reader, &command) {
                        out.commands.push(command);
                        budget -= 1;
                    }
                });
                geometry.paths.push(out);
            }),
            _ => {}
        }
    });
    let note = dropped.then(|| {
        format!("a custom geometry is drawn with its first {MAX_ITEMS} guides and path commands")
    });
    (Geometry::Custom(geometry), note)
}

fn path_command(reader: &mut Reader<'_>, e: &Element<'_>) -> Option<PathCommand> {
    let mut points: Vec<String> = Vec::new();
    let arc = |name: &str| e.attr(Ns::NONE, name).unwrap_or_default().into_owned();
    if e.local == "arcTo" {
        return Some(PathCommand::ArcTo([
            arc("wR"),
            arc("hR"),
            arc("stAng"),
            arc("swAng"),
        ]));
    }
    if e.local == "close" {
        return Some(PathCommand::Close);
    }
    children(reader, |_, pt| {
        if pt.local != "pt" || points.len() >= 6 {
            return;
        }
        for name in ["x", "y"] {
            points.push(pt.attr(Ns::NONE, name).unwrap_or_default().into_owned());
        }
    });
    points.resize(6, "0".to_string());
    let mut p = points.into_iter();
    let mut next = || p.next().unwrap_or_default();
    Some(match e.local {
        "moveTo" => PathCommand::MoveTo([next(), next()]),
        "lnTo" => PathCommand::LineTo([next(), next()]),
        "quadBezTo" => PathCommand::QuadTo([next(), next(), next(), next()]),
        "cubicBezTo" => PathCommand::CubicTo([next(), next(), next(), next(), next(), next()]),
        _ => return None,
    })
}

/// ECMA-376 Part 1 §20.1.8.38 and §20.1.8.57: `a:headEnd` or `a:tailEnd`,
/// with its type (§20.1.10.33), width (§20.1.10.34) and length
/// (§20.1.10.32); `None` for no decoration.
pub fn line_end(e: &Element<'_>) -> Option<LineEnd> {
    let kind = match e.attr_raw(Ns::NONE, "type")? {
        "triangle" => LineEndKind::Triangle,
        "stealth" => LineEndKind::Stealth,
        "diamond" => LineEndKind::Diamond,
        "oval" => LineEndKind::Oval,
        "arrow" => LineEndKind::Arrow,
        _ => return None,
    };
    let size = |name: &str| match e.attr_raw(Ns::NONE, name) {
        Some("sm") => LineEndSize::Small,
        Some("lg") => LineEndSize::Large,
        _ => LineEndSize::Medium,
    };
    Some(LineEnd {
        kind,
        width: size("w"),
        length: size("len"),
    })
}

/// A VML `v:stroke` arrowhead from its `startarrow` or `endarrow` and the
/// matching `…arrowwidth` and `…arrowlength` ([MS-ODRAW] §2.4.16,
/// §2.4.17, §2.4.18 name the same values); `None` for none.
pub fn vml_line_end(e: &Element<'_>, end: &str) -> Option<LineEnd> {
    let kind = match e.attr_raw(Ns::NONE, &format!("{end}arrow"))? {
        "block" => LineEndKind::Triangle,
        "classic" => LineEndKind::Stealth,
        "diamond" => LineEndKind::Diamond,
        "oval" => LineEndKind::Oval,
        "open" => LineEndKind::Arrow,
        _ => return None,
    };
    let size = |name: &str| match e.attr_raw(Ns::NONE, &format!("{end}arrow{name}")) {
        Some("narrow" | "short") => LineEndSize::Small,
        Some("wide" | "long") => LineEndSize::Large,
        _ => LineEndSize::Medium,
    };
    Some(LineEnd {
        kind,
        width: size("width"),
        length: size("length"),
    })
}

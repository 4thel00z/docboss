//! Office Math (ECMA-376 Part 1 §22.1): `m:oMath` and `m:oMathPara` read
//! into [`Math`] trees.

use docboss_model::{FractionKind, Math, MathJustification, MathNode, MathStyle, RunProperties};
use docboss_xml::{Element, Ns, Reader};

use crate::props::{run_properties, Theme};
use crate::xml::children;

/// The deepest nesting of math objects read; deeper ones are dropped.
const MAX_DEPTH: usize = 48;
/// The most nodes one zone keeps.
const MAX_NODES: usize = 20_000;

/// What reading a zone found besides its nodes.
struct Reading<'t> {
    theme: &'t Theme,
    nodes: usize,
    dropped: bool,
    /// The run properties of the zone's first run, which size its text.
    properties: Option<RunProperties>,
}

/// The zones of an `m:oMathPara` (ECMA-376 Part 1 §22.1.2.78), display
/// equations placed by its `m:oMathParaPr` `m:jc` (§22.1.2.79,
/// §22.1.2.51, centered by default), or the one zone of an `m:oMath`
/// (§22.1.2.77), with the run properties of their first run. The flag
/// tells whether anything was dropped for nesting too deep.
pub(super) fn zones(
    reader: &mut Reader<'_>,
    e: &Element<'_>,
    theme: &Theme,
) -> (Vec<(Math, RunProperties)>, bool) {
    let mut out = Vec::new();
    let mut dropped = false;
    if e.local == "oMath" {
        let (math, properties, lost) = zone(reader, theme, false);
        out.push((math, properties));
        return (out, lost);
    }
    let mut justification = MathJustification::Center;
    children(reader, |reader, child| {
        if child.ns != Ns::M {
            return;
        }
        match child.local {
            "oMathParaPr" => children(reader, |reader, pr| {
                if pr.local != "jc" {
                    return;
                }
                justification = match pr.attr_raw(Ns::M, "val") {
                    Some("left") => MathJustification::Left,
                    Some("right") => MathJustification::Right,
                    _ => MathJustification::Center,
                };
                reader.skip();
            }),
            "oMath" => {
                let (mut math, properties, lost) = zone(reader, theme, true);
                math.justification = justification;
                dropped |= lost;
                out.push((math, properties));
            }
            _ => {}
        }
    });
    for (math, _) in &mut out {
        math.justification = justification;
    }
    (out, dropped)
}

fn zone(reader: &mut Reader<'_>, theme: &Theme, display: bool) -> (Math, RunProperties, bool) {
    let mut reading = Reading {
        theme,
        nodes: 0,
        dropped: false,
        properties: None,
    };
    let nodes = list(reader, &mut reading, 0);
    let math = Math {
        display,
        justification: MathJustification::Center,
        nodes,
    };
    (
        math,
        reading.properties.unwrap_or_default(),
        reading.dropped,
    )
}

/// The objects of an argument (§22.1.2.32 `e`, and `num`, `den`, `sub`,
/// `sup`, `deg`, `fName`, `lim`), in order.
fn list(reader: &mut Reader<'_>, reading: &mut Reading<'_>, depth: usize) -> Vec<MathNode> {
    let mut nodes = Vec::new();
    if depth >= MAX_DEPTH {
        reading.dropped = true;
        reader.skip();
        return nodes;
    }
    children(reader, |reader, e| {
        if reading.nodes >= MAX_NODES {
            reading.dropped = true;
            return;
        }
        object(reader, &e, reading, depth, &mut nodes);
    });
    nodes
}

/// The value of a property element's `m:val`, or `on` when it has none.
fn on(e: &Element<'_>) -> bool {
    e.attr_raw(Ns::M, "val")
        .is_none_or(|v| matches!(v, "1" | "on" | "true"))
}

/// A property element and its first child element's local name and value.
struct Properties {
    entries: Vec<(String, Option<String>)>,
}

impl Properties {
    fn read(reader: &mut Reader<'_>) -> Properties {
        let mut entries = Vec::new();
        children(reader, |_, p| {
            if p.ns == Ns::M {
                entries.push((p.local.to_string(), p.attr(Ns::M, "val").map(Into::into)));
            }
        });
        Properties { entries }
    }

    /// The entry's value: `None` when absent, `Some(None)` when present
    /// without a value.
    fn get(&self, name: &str) -> Option<Option<&str>> {
        self.entries
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_deref())
    }

    fn character(&self, name: &str, default: char) -> Option<char> {
        match self.get(name) {
            None => Some(default),
            Some(value) => value.and_then(|v| v.chars().next()),
        }
    }

    fn flag(&self, name: &str) -> bool {
        self.get(name)
            .is_some_and(|v| v.is_none_or(|v| matches!(v, "1" | "on" | "true")))
    }
}

/// The arguments of an object by element name, in document order, and
/// its properties.
struct Parts {
    properties: Properties,
    arguments: Vec<(String, Vec<MathNode>)>,
    rows: Vec<Vec<Vec<MathNode>>>,
}

impl Parts {
    fn read(reader: &mut Reader<'_>, reading: &mut Reading<'_>, depth: usize) -> Parts {
        let mut parts = Parts {
            properties: Properties {
                entries: Vec::new(),
            },
            arguments: Vec::new(),
            rows: Vec::new(),
        };
        children(reader, |reader, e| {
            if e.ns != Ns::M {
                return;
            }
            if e.local.ends_with("Pr") {
                parts.properties = Properties::read(reader);
                return;
            }
            if e.local == "mr" {
                let mut row = Vec::new();
                children(reader, |reader, cell| {
                    if cell.ns == Ns::M && cell.local == "e" {
                        row.push(list(reader, reading, depth + 1));
                    }
                });
                parts.rows.push(row);
                return;
            }
            let nodes = list(reader, reading, depth + 1);
            parts.arguments.push((e.local.to_string(), nodes));
        });
        parts
    }

    fn take(&mut self, name: &str) -> Option<Vec<MathNode>> {
        let at = self.arguments.iter().position(|(n, _)| n == name)?;
        Some(self.arguments.remove(at).1)
    }

    fn all(&mut self, name: &str) -> Vec<Vec<MathNode>> {
        let (taken, kept) = std::mem::take(&mut self.arguments)
            .into_iter()
            .partition(|(n, _)| n == name);
        self.arguments = kept;
        taken.into_iter().map(|(_, nodes)| nodes).collect()
    }
}

fn is_integral(c: char) -> bool {
    matches!(c, '∫' | '∬' | '∭' | '∮' | '∯' | '∰' | '∱' | '∲' | '∳' | '⨌')
}

/// One math object (§22.1.2): a run (§22.1.2.87), fraction (§22.1.2.36,
/// `type` §22.1.2.118), scripts (§22.1.2.105, §22.1.2.101, §22.1.2.103,
/// §22.1.2.99), radical (§22.1.2.88, `degHide` §22.1.2.27), n-ary
/// operator (§22.1.2.70, `chr` §22.1.2.20, `limLoc` §22.1.2.53,
/// `subHide` §22.1.2.113, `supHide` §22.1.2.115), delimiter
/// (§22.1.2.24, §22.1.2.10, §22.1.2.33, §22.1.2.95), matrix (§22.1.2.60,
/// §22.1.2.69), accent (§22.1.2.1), bar (§22.1.2.7, `pos` §22.1.2.84),
/// function (§22.1.2.39, §22.1.2.37), group character (§22.1.2.41),
/// limits (§22.1.2.54, §22.1.2.56, §22.1.2.52), array (§22.1.2.34) and
/// border box (§22.1.2.11); the box and phantom objects (§22.1.2.13,
/// §22.1.2.81) give their argument.
fn object(
    reader: &mut Reader<'_>,
    e: &Element<'_>,
    reading: &mut Reading<'_>,
    depth: usize,
    out: &mut Vec<MathNode>,
) {
    if e.ns == Ns::W && e.local == "r" {
        let text = run_text_w(reader);
        push_run(out, text, MathStyle::Text);
        return;
    }
    if e.ns != Ns::M {
        return;
    }
    reading.nodes += 1;
    let node = match e.local {
        "r" => {
            let (text, style) = run(reader, reading);
            push_run(out, text, style);
            return;
        }
        "box" | "phant" | "e" => {
            let mut parts = Parts::read(reader, reading, depth);
            if e.local == "phant"
                && parts
                    .properties
                    .get("show")
                    .is_some_and(|v| v == Some("0") || v == Some("off"))
            {
                return;
            }
            out.extend(parts.all("e").into_iter().flatten());
            return;
        }
        "f" => {
            let mut parts = Parts::read(reader, reading, depth);
            let kind = match parts.properties.get("type").flatten() {
                Some("skw") => FractionKind::Skewed,
                Some("lin") => FractionKind::Linear,
                Some("noBar") => FractionKind::NoBar,
                _ => FractionKind::Bar,
            };
            MathNode::Fraction {
                kind,
                numerator: parts.take("num").unwrap_or_default(),
                denominator: parts.take("den").unwrap_or_default(),
            }
        }
        "sSup" | "sSub" | "sSubSup" | "sPre" => {
            let mut parts = Parts::read(reader, reading, depth);
            MathNode::Script {
                base: parts.take("e").unwrap_or_default(),
                sub: parts.take("sub"),
                sup: parts.take("sup"),
                pre: e.local == "sPre",
            }
        }
        "rad" => {
            let mut parts = Parts::read(reader, reading, depth);
            let hidden = parts.properties.flag("degHide");
            let degree = parts.take("deg").filter(|d| !hidden && !d.is_empty());
            MathNode::Radical {
                degree,
                body: parts.take("e").unwrap_or_default(),
            }
        }
        "nary" => {
            let mut parts = Parts::read(reader, reading, depth);
            let operator = parts.properties.character("chr", '∫').unwrap_or('∫');
            let limits_under = match parts.properties.get("limLoc") {
                Some(Some("subSup")) => false,
                Some(_) => true,
                None => !is_integral(operator),
            };
            let sub = parts
                .take("sub")
                .filter(|_| !parts.properties.flag("subHide"));
            let sup = parts
                .take("sup")
                .filter(|_| !parts.properties.flag("supHide"));
            MathNode::Nary {
                operator,
                sub: sub.filter(|s| !s.is_empty()),
                sup: sup.filter(|s| !s.is_empty()),
                limits_under,
                body: parts.take("e").unwrap_or_default(),
            }
        }
        "d" => {
            let mut parts = Parts::read(reader, reading, depth);
            let p = &parts.properties;
            MathNode::Delimiter {
                open: p.character("begChr", '('),
                close: p.character("endChr", ')'),
                separator: p.character("sepChr", '|').unwrap_or('|'),
                items: parts.all("e"),
            }
        }
        "m" => {
            let parts = Parts::read(reader, reading, depth);
            MathNode::Matrix { rows: parts.rows }
        }
        "eqArr" => {
            let mut parts = Parts::read(reader, reading, depth);
            MathNode::Array {
                rows: parts.all("e"),
            }
        }
        "acc" => {
            let mut parts = Parts::read(reader, reading, depth);
            MathNode::Accent {
                accent: parts
                    .properties
                    .character("chr", '\u{0302}')
                    .unwrap_or('\u{0302}'),
                body: parts.take("e").unwrap_or_default(),
            }
        }
        "bar" => {
            let mut parts = Parts::read(reader, reading, depth);
            MathNode::Bar {
                top: parts.properties.get("pos").flatten() == Some("top"),
                body: parts.take("e").unwrap_or_default(),
            }
        }
        "groupChr" => {
            let mut parts = Parts::read(reader, reading, depth);
            MathNode::GroupCharacter {
                character: parts.properties.character("chr", '⏟').unwrap_or('⏟'),
                top: parts.properties.get("pos").flatten() == Some("top"),
                body: parts.take("e").unwrap_or_default(),
            }
        }
        "func" => {
            let mut parts = Parts::read(reader, reading, depth);
            let mut name = parts.take("fName").unwrap_or_default();
            for node in &mut name {
                if let MathNode::Run { style, .. } = node {
                    if *style == MathStyle::Math {
                        *style = MathStyle::Plain;
                    }
                }
            }
            MathNode::Function {
                name,
                body: parts.take("e").unwrap_or_default(),
            }
        }
        "limLow" | "limUpp" => {
            let mut parts = Parts::read(reader, reading, depth);
            MathNode::Limit {
                base: parts.take("e").unwrap_or_default(),
                limit: parts.take("lim").unwrap_or_default(),
                lower: e.local == "limLow",
            }
        }
        "borderBox" => {
            let mut parts = Parts::read(reader, reading, depth);
            MathNode::BorderBox {
                body: parts.take("e").unwrap_or_default(),
            }
        }
        "oMath" => {
            out.extend(list(reader, reading, depth + 1));
            return;
        }
        _ => {
            reader.skip();
            return;
        }
    };
    out.push(node);
}

fn push_run(out: &mut Vec<MathNode>, text: String, style: MathStyle) {
    if text.is_empty() {
        return;
    }
    if let Some(MathNode::Run {
        text: previous,
        style: before,
    }) = out.last_mut()
    {
        if *before == style {
            previous.push_str(&text);
            return;
        }
    }
    out.push(MathNode::Run { text, style });
}

/// A math run (§22.1.2.87): its text (§22.1.2.116), its `m:rPr`
/// (§22.1.2.91) style (§22.1.2.111) or normal text (§22.1.2.74), and the
/// `w:rPr` the zone's size comes from.
fn run(reader: &mut Reader<'_>, reading: &mut Reading<'_>) -> (String, MathStyle) {
    let mut text = String::new();
    let mut style = MathStyle::Math;
    let theme = reading.theme;
    children(reader, |reader, e| match (e.ns, e.local) {
        (Ns::M, "t") => text.push_str(&reader.read_text()),
        (Ns::M, "rPr") => children(reader, |_, p| match p.local {
            "sty" => {
                style = match p.attr_raw(Ns::M, "val") {
                    Some("p") => MathStyle::Plain,
                    Some("b") => MathStyle::Bold,
                    Some("bi") => MathStyle::BoldItalic,
                    _ => style,
                }
            }
            "nor" if on(&p) => style = MathStyle::Text,
            _ => {}
        }),
        (Ns::W, "rPr") => {
            let properties = run_properties(reader, theme);
            if reading.properties.is_none() {
                reading.properties = Some(properties);
            }
        }
        _ => {}
    });
    (text, style)
}

fn run_text_w(reader: &mut Reader<'_>) -> String {
    let mut text = String::new();
    children(reader, |reader, e| {
        if e.ns == Ns::W && e.local == "t" {
            text.push_str(&reader.read_text());
        }
    });
    text
}

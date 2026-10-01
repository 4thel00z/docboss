//! Equation Editor 3 equations: the MTEF data of an OLE object's
//! `Equation Native` stream read into the docboss math model, so that text
//! output gives their linear form and Markdown their LaTeX, as for Office
//! Math.
//!
//! The stream starts with a 28-byte EQNOLEFILEHDR; MTEF follows: a
//! 5-byte header (version, platform, product and its version), then
//! records. In versions 1 to 3 a record's tag byte holds its type in the
//! low nibble and its option flags in the high one. MathType's MTEF 4 and
//! 5, whose records and template numbers differ, are not read.

use docboss_model::{FractionKind, Math, MathNode, MathStyle};

const EQNOLEFILEHDR: usize = 28;
const MAX_DEPTH: usize = 48;
const MAX_RECORDS: usize = 200_000;

/// Record types.
const END: u8 = 0;
const LINE: u8 = 1;
const CHAR: u8 = 2;
const TMPL: u8 = 3;
const PILE: u8 = 4;
const MATRIX: u8 = 5;
const EMBELL: u8 = 6;
const RULER: u8 = 7;
const FONT: u8 = 8;
const SIZE: u8 = 9;

/// Option flags of the tag's high nibble.
const XF_LMOVE: u8 = 0x80;
const XF_LSPACE: u8 = 0x40;
const XF_EMBELL: u8 = 0x20;
const XF_RULER: u8 = 0x20;
const XF_NULL: u8 = 0x10;

/// Typefaces.
const FN_TEXT: u8 = 1;
const FN_FUNCTION: u8 = 2;
const FN_SYMBOL: u8 = 6;
const FN_VECTOR: u8 = 7;
const FN_SPACE: u8 = 24;

/// Reads the equation of an `Equation Native` stream, or `None` when it
/// holds no MTEF this reader understands.
pub fn read(stream: &[u8]) -> Option<Math> {
    let header = usize::from(u16::from_le_bytes([*stream.first()?, *stream.get(1)?]));
    let start = match header {
        0 => EQNOLEFILEHDR,
        size => size,
    };
    let mtef = stream.get(start..)?;
    let version = *mtef.first()?;
    if !(1..=3).contains(&version) {
        return None;
    }
    let mut reader = Reader {
        bytes: mtef,
        at: 5,
        records: 0,
        version,
    };
    let items = reader.objects(0)?;
    let nodes = line_nodes(&items);
    if nodes.is_empty() {
        return None;
    }
    Some(Math {
        display: false,
        justification: Default::default(),
        nodes,
    })
}

/// One parsed record of an object list.
#[derive(Debug, Clone, PartialEq)]
enum Item {
    /// A line's objects, or `None` for an empty slot.
    Line(Option<Vec<Item>>),
    Char {
        value: u16,
        face: u8,
        embellishments: Vec<u8>,
    },
    Template {
        selector: u8,
        variation: u8,
        items: Vec<Item>,
    },
    Pile(Vec<Item>),
    Matrix {
        columns: usize,
        items: Vec<Item>,
    },
    Embellishment(u8),
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
    records: usize,
    version: u8,
}

impl Reader<'_> {
    fn u8(&mut self) -> Option<u8> {
        let value = *self.bytes.get(self.at)?;
        self.at += 1;
        Some(value)
    }

    fn u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes([self.u8()?, self.u8()?]))
    }

    fn skip(&mut self, count: usize) -> Option<()> {
        let end = self.at.checked_add(count)?;
        if end > self.bytes.len() {
            return None;
        }
        self.at = end;
        Some(())
    }

    /// A nudge: two offset bytes, followed by two 16-bit offsets when both
    /// are 128.
    fn nudge(&mut self) -> Option<()> {
        let (dx, dy) = (self.u8()?, self.u8()?);
        if dx == 128 && dy == 128 {
            self.skip(4)?;
        }
        Some(())
    }

    fn ruler(&mut self) -> Option<()> {
        let stops = usize::from(self.u8()?);
        self.skip(stops * 3)
    }

    /// The records of an object list up to its END record.
    fn objects(&mut self, depth: usize) -> Option<Vec<Item>> {
        if depth > MAX_DEPTH {
            return None;
        }
        let mut items = Vec::new();
        loop {
            self.records += 1;
            if self.records > MAX_RECORDS {
                return None;
            }
            let Some(tag) = self.u8() else {
                return Some(items);
            };
            let (kind, flags) = match self.version {
                1..=3 => (tag & 0x0F, tag & 0xF0),
                _ => (tag, 0),
            };
            if kind == END {
                return Some(items);
            }
            if let Some(item) = self.record(kind, flags, depth)? {
                items.push(item);
            }
        }
    }

    fn record(&mut self, kind: u8, flags: u8, depth: usize) -> Option<Option<Item>> {
        let moved = flags & XF_LMOVE != 0;
        let item = match kind {
            LINE => {
                if moved {
                    self.nudge()?;
                }
                if flags & XF_LSPACE != 0 {
                    self.skip(2)?;
                }
                if flags & XF_RULER != 0 {
                    self.ruler()?;
                }
                match flags & XF_NULL != 0 {
                    true => Item::Line(None),
                    false => Item::Line(Some(self.objects(depth + 1)?)),
                }
            }
            CHAR => {
                if moved {
                    self.nudge()?;
                }
                let face = self.u8()?.wrapping_sub(128);
                let value = match self.version {
                    3 => self.u16()?,
                    _ => u16::from(self.u8()?),
                };
                let embellishments = match flags & XF_EMBELL != 0 {
                    true => self
                        .objects(depth + 1)?
                        .into_iter()
                        .filter_map(|item| match item {
                            Item::Embellishment(kind) => Some(kind),
                            _ => None,
                        })
                        .collect(),
                    false => Vec::new(),
                };
                Item::Char {
                    value,
                    face,
                    embellishments,
                }
            }
            TMPL => {
                if moved {
                    self.nudge()?;
                }
                let selector = self.u8()?;
                let variation = self.u8()?;
                self.skip(1)?;
                Item::Template {
                    selector,
                    variation,
                    items: self.objects(depth + 1)?,
                }
            }
            PILE => {
                if moved {
                    self.nudge()?;
                }
                self.skip(2)?;
                if flags & XF_RULER != 0 {
                    self.ruler()?;
                }
                Item::Pile(self.objects(depth + 1)?)
            }
            MATRIX => {
                if moved {
                    self.nudge()?;
                }
                self.skip(3)?;
                let rows = usize::from(self.u8()?);
                let columns = usize::from(self.u8()?);
                self.skip(((rows + 1) * 2).div_ceil(8))?;
                self.skip(((columns + 1) * 2).div_ceil(8))?;
                Item::Matrix {
                    columns,
                    items: self.objects(depth + 1)?,
                }
            }
            EMBELL => {
                if moved {
                    self.nudge()?;
                }
                Item::Embellishment(self.u8()?)
            }
            RULER => {
                self.ruler()?;
                return Some(None);
            }
            FONT => {
                self.skip(2)?;
                while self.u8()? != 0 {}
                return Some(None);
            }
            SIZE => {
                match self.u8()? {
                    101 => self.skip(2)?,
                    100 => self.skip(3)?,
                    _ => self.skip(1)?,
                }
                return Some(None);
            }
            10..=14 => return Some(None),
            _ => return None,
        };
        Some(Some(item))
    }
}

/// The math style a typeface draws its characters in.
fn style(face: u8) -> MathStyle {
    match face {
        FN_TEXT | FN_FUNCTION => MathStyle::Plain,
        FN_VECTOR => MathStyle::Bold,
        _ => MathStyle::Math,
    }
}

/// The Unicode character of an MTEF character code: codes of the Symbol
/// typeface, and the U+F000 private-use forms of Symbol, go through the
/// Symbol font's mapping.
fn decode(value: u16, face: u8) -> Option<char> {
    let symbol = (0xF020..=0xF0FF).contains(&value) || (face == FN_SYMBOL && value <= 0xFF);
    if symbol {
        let code = char::from_u32(u32::from(value))?;
        return docboss_font::symbol_to_unicode("Symbol", code);
    }
    char::from_u32(u32::from(value)).filter(|c| *c != '\0')
}

/// The combining mark or trailing text an embellishment adds to its
/// character.
fn embellishment(kind: u8) -> Option<Result<char, &'static str>> {
    let found = match kind {
        2 => Ok('\u{0307}'),
        3 => Ok('\u{0308}'),
        4 => Ok('\u{20DB}'),
        5 => Err("\u{2032}"),
        6 => Err("\u{2033}"),
        7 => Err("\u{2035}"),
        8 => Ok('\u{0303}'),
        9 => Ok('\u{0302}'),
        10 => Ok('\u{0338}'),
        11 => Ok('\u{20D7}'),
        12 => Ok('\u{20D6}'),
        13 => Ok('\u{20E1}'),
        14 => Ok('\u{20D1}'),
        15 => Ok('\u{20D0}'),
        16 | 17 => Ok('\u{0305}'),
        18 => Err("\u{2034}"),
        19 => Ok('\u{0311}'),
        20 => Ok('\u{0306}'),
        _ => return None,
    };
    Some(found)
}

/// The math nodes of a line's objects. A subscript or superscript template
/// attaches to the one object before it, a leading script template to the
/// one after it; adjacent characters of one style are then joined.
fn line_nodes(items: &[Item]) -> Vec<MathNode> {
    let mut nodes: Vec<MathNode> = Vec::new();
    let mut leading: Option<Scripts> = None;
    for item in items {
        let mut produced = match item {
            Item::Line(Some(inner)) => line_nodes(inner),
            Item::Line(None) | Item::Embellishment(_) => continue,
            Item::Char {
                value,
                face,
                embellishments,
            } => char_node(*value, *face, embellishments)
                .into_iter()
                .collect(),
            Item::Pile(lines) => pile(lines),
            Item::Matrix { columns, items } => vec![matrix(*columns, items)],
            Item::Template {
                selector: SCRIPT,
                items,
                ..
            } => {
                let (sub, sup) = scripts(items);
                let base = nodes.pop().map(|node| vec![node]).unwrap_or_default();
                vec![MathNode::Script {
                    base,
                    sub,
                    sup,
                    pre: false,
                }]
            }
            Item::Template {
                selector: LEADING_SCRIPT,
                items,
                ..
            } => {
                leading = Some(scripts(items));
                continue;
            }
            Item::Template {
                selector,
                variation,
                items,
            } => template(*selector, *variation, items),
        };
        if !produced.is_empty() {
            if let Some((sub, sup)) = leading.take() {
                let base = produced.remove(0);
                produced.insert(
                    0,
                    MathNode::Script {
                        base: vec![base],
                        sub,
                        sup,
                        pre: true,
                    },
                );
            }
        }
        nodes.append(&mut produced);
    }
    if let Some((sub, sup)) = leading {
        nodes.push(MathNode::Script {
            base: Vec::new(),
            sub,
            sup,
            pre: true,
        });
    }
    join_runs(nodes)
}

/// Joins adjacent runs of one style into one run.
fn join_runs(nodes: Vec<MathNode>) -> Vec<MathNode> {
    let mut out: Vec<MathNode> = Vec::with_capacity(nodes.len());
    for node in nodes {
        let MathNode::Run { text, style } = node else {
            out.push(node);
            continue;
        };
        if let Some(MathNode::Run {
            text: last,
            style: previous,
        }) = out.last_mut()
        {
            if *previous == style {
                last.push_str(&text);
                continue;
            }
        }
        out.push(MathNode::Run { text, style });
    }
    out
}

/// The node of one character with its embellishments: accents over it,
/// primes after it.
fn char_node(value: u16, face: u8, embellishments: &[u8]) -> Option<MathNode> {
    if face == FN_SPACE {
        return None;
    }
    let c = decode(value, face)?;
    let mut text = c.to_string();
    let mut accents = Vec::new();
    for kind in embellishments {
        match embellishment(*kind) {
            Some(Ok(mark)) => accents.push(mark),
            Some(Err(suffix)) => text.push_str(suffix),
            None => {}
        }
    }
    let run = MathNode::Run {
        text,
        style: style(face),
    };
    Some(
        accents
            .into_iter()
            .fold(run, |node, accent| MathNode::Accent {
                accent,
                body: vec![node],
            }),
    )
}

/// A template's slots, each a line or pile, and the characters after them
/// that draw its fences or operator.
fn slots(items: &[Item]) -> (Vec<Vec<MathNode>>, Vec<char>) {
    let mut slots = Vec::new();
    let mut chars = Vec::new();
    for item in items {
        match item {
            Item::Line(Some(inner)) => slots.push(line_nodes(inner)),
            Item::Line(None) => slots.push(Vec::new()),
            Item::Pile(lines) => slots.push(pile(lines)),
            Item::Char { value, face, .. } => chars.extend(decode(*value, *face)),
            _ => {}
        }
    }
    (slots, chars)
}

/// The subscript and superscript of a script template.
type Scripts = (Option<Vec<MathNode>>, Option<Vec<MathNode>>);

fn scripts(items: &[Item]) -> Scripts {
    let (mut slots, _) = slots(items);
    slots.resize(2, Vec::new());
    let sup = slots.pop().filter(|s| !s.is_empty());
    let sub = slots.pop().filter(|s| !s.is_empty());
    (sub, sup)
}

fn pile(lines: &[Item]) -> Vec<MathNode> {
    let mut rows: Vec<Vec<MathNode>> = lines
        .iter()
        .filter_map(|item| match item {
            Item::Line(Some(inner)) => Some(line_nodes(inner)),
            Item::Line(None) => Some(Vec::new()),
            _ => None,
        })
        .collect();
    match rows.len() {
        0 => Vec::new(),
        1 => rows.remove(0),
        _ => vec![MathNode::Array { rows }],
    }
}

fn matrix(columns: usize, items: &[Item]) -> MathNode {
    let cells: Vec<Vec<MathNode>> = items
        .iter()
        .filter_map(|item| match item {
            Item::Line(Some(inner)) => Some(line_nodes(inner)),
            Item::Line(None) => Some(Vec::new()),
            _ => None,
        })
        .collect();
    let rows = cells
        .chunks(columns.max(1))
        .map(|row| row.to_vec())
        .collect();
    MathNode::Matrix { rows }
}

fn slot(slots: &mut [Vec<MathNode>], index: usize) -> Vec<MathNode> {
    slots.get_mut(index).map(std::mem::take).unwrap_or_default()
}

fn optional(slots: &mut [Vec<MathNode>], index: usize) -> Option<Vec<MathNode>> {
    Some(slot(slots, index)).filter(|s| !s.is_empty())
}

/// The template of subscripts and superscripts.
const SCRIPT: u8 = 15;
/// The template of leading subscripts and superscripts.
const LEADING_SCRIPT: u8 = 44;

/// The fences of the fence templates 0 to 12, left and right.
const FENCES: [(char, char); 13] = [
    ('\u{27E8}', '\u{27E9}'),
    ('(', ')'),
    ('{', '}'),
    ('[', ']'),
    ('|', '|'),
    ('\u{2016}', '\u{2016}'),
    ('\u{230A}', '\u{230B}'),
    ('\u{2308}', '\u{2309}'),
    ('[', '['),
    (']', ']'),
    (']', '['),
    ('[', ')'),
    ('(', ']'),
];

/// The n-ary operator of the integral and big operator templates 21 to 43
/// and whether its limits go under and over it rather than beside it.
fn operator(selector: u8) -> (char, bool) {
    match selector {
        21 => ('\u{222B}', false),
        22 => ('\u{222C}', false),
        23 => ('\u{222D}', false),
        24 => ('\u{222B}', true),
        25 => ('\u{222C}', true),
        26 => ('\u{222D}', true),
        29 => ('\u{2211}', true),
        30 => ('\u{2211}', false),
        31 => ('\u{220F}', true),
        32 => ('\u{220F}', false),
        33 => ('\u{2210}', true),
        34 => ('\u{2210}', false),
        35 => ('\u{22C3}', true),
        36 => ('\u{22C3}', false),
        37 => ('\u{22C2}', true),
        38 => ('\u{22C2}', false),
        42 => ('\u{222B}', false),
        _ => ('\u{2211}', true),
    }
}

fn run(text: &str) -> MathNode {
    MathNode::Run {
        text: text.into(),
        style: MathStyle::Plain,
    }
}

/// A body with a limit over or under it, or the body alone when the limit
/// is empty.
fn limited(base: Vec<MathNode>, limit: Vec<MathNode>, lower: bool) -> Vec<MathNode> {
    if limit.is_empty() {
        return base;
    }
    vec![MathNode::Limit { base, limit, lower }]
}

/// The nodes of an MTEF 3 template by its selector and variation: fences
/// (0 to 12; variation 1 draws only the left fence, 2 only the right),
/// radicals (13), fractions (14), under and over bars (16, 17; variation 1
/// doubles them), arrows with text over or under them (18 to 20), integrals
/// and big operators (21 to 26, 29 to 38, 42, 43), horizontal braces (27,
/// 28), limits (39), long division (40), slash fractions (41), bra-kets
/// (45), arrows under and over (46, 47) and arcs (48). An unknown template
/// gives its slots in order.
fn template(selector: u8, variation: u8, items: &[Item]) -> Vec<MathNode> {
    let (mut slots, chars) = slots(items);
    let node = match selector {
        0..=12 => {
            let (left, right) = FENCES[usize::from(selector)];
            let (open, close) = match variation & 0x03 {
                1 => (chars.first().copied().or(Some(left)), None),
                2 => (None, chars.last().copied().or(Some(right))),
                _ => (
                    chars.first().copied().or(Some(left)),
                    chars.get(1).copied().or(Some(right)),
                ),
            };
            MathNode::Delimiter {
                open,
                close,
                separator: '|',
                items: vec![slot(&mut slots, 0)],
            }
        }
        13 => MathNode::Radical {
            degree: optional(&mut slots, 1).filter(|_| variation & 0x01 != 0),
            body: slot(&mut slots, 0),
        },
        14 => MathNode::Fraction {
            kind: FractionKind::Bar,
            numerator: slot(&mut slots, 0),
            denominator: slot(&mut slots, 1),
        },
        16 | 17 => {
            let top = selector == 17;
            let bar = MathNode::Bar {
                top,
                body: slot(&mut slots, 0),
            };
            match variation {
                1 => MathNode::Bar {
                    top,
                    body: vec![bar],
                },
                _ => bar,
            }
        }
        18..=20 => {
            let arrow = match selector {
                18 => "\u{2190}",
                19 => "\u{2192}",
                _ => "\u{2194}",
            };
            let limit = slots.into_iter().flatten().collect();
            return limited(vec![run(arrow)], limit, variation == 1);
        }
        27 | 28 => {
            let top = selector == 27;
            let brace = MathNode::GroupCharacter {
                character: if top { '\u{23DE}' } else { '\u{23DF}' },
                top,
                body: slot(&mut slots, 0),
            };
            return limited(vec![brace], slot(&mut slots, 1), !top);
        }
        21..=26 | 29..=38 | 42 | 43 => {
            let (fallback, limits_under) = operator(selector);
            let written = slots.get(3).and_then(|operator| match operator.as_slice() {
                [MathNode::Run { text, .. }] => text.chars().next(),
                _ => None,
            });
            MathNode::Nary {
                operator: chars.last().copied().or(written).unwrap_or(fallback),
                sub: optional(&mut slots, 1),
                sup: optional(&mut slots, 2),
                limits_under,
                body: slot(&mut slots, 0),
            }
        }
        39 => {
            let base = slot(&mut slots, 0);
            let (lower, upper) = match variation {
                0 => (Vec::new(), slot(&mut slots, 1)),
                1 => (slot(&mut slots, 1), Vec::new()),
                _ => (slot(&mut slots, 1), slot(&mut slots, 2)),
            };
            let under = limited(base, lower, true);
            return limited(under, upper, false);
        }
        40 => {
            let dividend = vec![
                run(")"),
                MathNode::Bar {
                    top: true,
                    body: slot(&mut slots, 0),
                },
            ];
            return limited(dividend, slot(&mut slots, 1), false);
        }
        41 => MathNode::Fraction {
            kind: FractionKind::Linear,
            numerator: slot(&mut slots, 0),
            denominator: slot(&mut slots, 1),
        },
        45 => {
            let items = slots.into_iter().take(2).collect();
            MathNode::Delimiter {
                open: Some(if variation == 2 { '|' } else { '\u{27E8}' }),
                close: Some(if variation == 1 { '|' } else { '\u{27E9}' }),
                separator: '|',
                items,
            }
        }
        46 | 47 => MathNode::GroupCharacter {
            character: match variation {
                0 => '\u{2190}',
                1 => '\u{2192}',
                _ => '\u{2194}',
            },
            top: selector == 47,
            body: slot(&mut slots, 0),
        },
        48 => MathNode::GroupCharacter {
            character: '\u{2322}',
            top: true,
            body: slot(&mut slots, 0),
        },
        _ => return slots.into_iter().flatten().collect(),
    };
    vec![node]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stream(mtef: &[u8]) -> Vec<u8> {
        let mut out = vec![0u8; EQNOLEFILEHDR];
        out[0] = EQNOLEFILEHDR as u8;
        out[4..8].copy_from_slice(&0x0002_0000u32.to_le_bytes());
        out[10..14].copy_from_slice(&(mtef.len() as u32).to_le_bytes());
        out.extend_from_slice(mtef);
        out
    }

    fn char3(face: u8, c: char) -> Vec<u8> {
        let code = c as u16;
        vec![CHAR, 128 + face, code as u8, (code >> 8) as u8]
    }

    fn line(objects: &[Vec<u8>]) -> Vec<u8> {
        let mut out = vec![LINE];
        objects.iter().for_each(|o| out.extend_from_slice(o));
        out.push(END);
        out
    }

    /// x² over 2, beside a square root of y: a fraction, a superscript
    /// attached to the character before it, and a radical, with a SIZE and
    /// a FONT record skipped.
    #[test]
    fn fractions_scripts_and_radicals() {
        let mut sup = vec![TMPL, 15, 0, 0];
        sup.extend(vec![LINE | XF_NULL]);
        sup.extend(line(&[char3(8, '2')]));
        sup.push(END);
        let mut fraction = vec![TMPL, 14, 0, 0];
        fraction.extend(line(&[char3(3, 'x'), sup]));
        fraction.extend(line(&[char3(8, '2')]));
        fraction.push(END);
        let mut root = vec![TMPL, 13, 0, 0];
        root.extend(line(&[char3(3, 'y')]));
        root.push(LINE | XF_NULL);
        root.push(END);
        let mut mtef = vec![3, 1, 1, 3, 0];
        mtef.extend([SIZE, 100, 1, 0x10, 0x00]);
        mtef.extend([FONT, 3, 1, b'T', b'i', 0]);
        mtef.extend(line(&[fraction, char3(6, '+'), root]));
        mtef.push(END);
        let math = read(&stream(&mtef)).expect("the equation reads");
        assert_eq!(docboss_model::linear_text(&math.nodes), "(x^2)/2+√y");
    }

    /// Parentheses around a sum from i=1 to n of i, and a Symbol-coded
    /// Greek letter.
    #[test]
    fn fences_sums_and_symbol_characters() {
        let mut sum = vec![TMPL, 29, 1, 0];
        sum.extend(line(&[char3(3, 'i')]));
        sum.extend(line(&[char3(3, 'i'), char3(6, '='), char3(8, '1')]));
        sum.extend(line(&[char3(3, 'n')]));
        sum.extend(char3(6, '\u{2211}'));
        sum.push(END);
        let mut paren = vec![TMPL, 1, 0, 0];
        paren.extend(line(&[sum]));
        paren.extend(char3(6, '('));
        paren.extend(char3(6, ')'));
        paren.push(END);
        let mut mtef = vec![3, 1, 1, 3, 0];
        mtef.extend(line(&[paren, char3(4, '\u{F061}')]));
        mtef.push(END);
        let math = read(&stream(&mtef)).expect("the equation reads");
        let text = docboss_model::linear_text(&math.nodes);
        assert!(
            text.starts_with('(') && text.contains('∑') && text.ends_with('α'),
            "{text}"
        );
    }

    fn template(selector: u8, variation: u8, slots: &[Vec<u8>], chars: &[Vec<u8>]) -> Vec<u8> {
        let mut out = vec![TMPL, selector, variation, 0];
        slots.iter().for_each(|s| out.extend_from_slice(s));
        chars.iter().for_each(|c| out.extend_from_slice(c));
        out.push(END);
        out
    }

    fn text(objects: &[Vec<u8>]) -> String {
        let mut mtef = vec![3, 1, 1, 3, 0];
        mtef.extend(line(objects));
        mtef.push(END);
        let math = read(&stream(&mtef)).expect("the equation reads");
        docboss_model::linear_text(&math.nodes)
    }

    fn word(face: u8, text: &str) -> Vec<Vec<u8>> {
        text.chars().map(|c| char3(face, c)).collect()
    }

    /// E = mc²: the superscript takes the c alone.
    #[test]
    fn a_script_takes_one_object() {
        let sup = template(
            SCRIPT,
            0,
            &[vec![LINE | XF_NULL], line(&[char3(8, '2')])],
            &[],
        );
        let mut objects = word(3, "E=mc");
        objects.push(sup);
        assert_eq!(text(&objects), "E=mc^2");
    }

    /// lim over x → 0 of sin x, a slash fraction, leading scripts and a
    /// brace with its left fence only.
    #[test]
    fn templates_dispatch_by_selector_and_variation() {
        let lim = template(
            39,
            1,
            &[line(&word(2, "lim")), line(&word(3, "x\u{2192}0"))],
            &[],
        );
        let mut objects = vec![lim];
        objects.extend(word(2, "sin"));
        objects.extend(word(3, "x"));
        assert_eq!(text(&objects), "(lim)┬(x→0)sinx");

        let slash = template(41, 0, &[line(&word(3, "a")), line(&word(3, "b"))], &[]);
        assert_eq!(text(&[slash]), "a∕b");

        let leading = template(44, 2, &[line(&word(8, "1")), line(&word(8, "2"))], &[]);
        assert_eq!(text(&[leading, char3(3, 'X')]), "_1^2 X");

        let pile = |rows: &[&str]| {
            let mut out = vec![PILE, 0, 0];
            rows.iter().for_each(|r| out.extend(line(&word(8, r))));
            out.push(END);
            out
        };
        let brace = template(2, 1, &[line(&[pile(&["1", "0"])])], &[char3(6, '{')]);
        let mut objects = word(3, "f=");
        objects.push(brace);
        let found = text(&objects);
        assert!(found.starts_with("f={") && !found.ends_with('{'), "{found}");
    }

    #[test]
    fn damaged_streams_give_nothing() {
        assert!(read(&[]).is_none());
        assert!(read(&stream(&[5, 1, 1, 3, 0])).is_none());
        let mut deep = vec![3, 1, 1, 3, 0];
        deep.extend(std::iter::repeat_n(LINE, 500));
        assert!(read(&stream(&deep)).is_none());
    }
}

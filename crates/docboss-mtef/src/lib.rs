//! Equation Editor 3 equations: the MTEF data of an OLE object's
//! `Equation Native` stream read into the docboss math model, so that text
//! output gives their linear form and Markdown their LaTeX, as for Office
//! Math.
//!
//! The stream starts with a 28-byte EQNOLEFILEHDR; MTEF follows: a
//! 5-byte header (version, platform, product and its version), then
//! records. In versions 1 to 3 a record's tag byte holds its type in the
//! low nibble and its option flags in the high one. MathType's MTEF 5 is
//! not read.

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

/// The math nodes of a line's objects. Subscripts and superscripts attach
/// to the object before them.
fn line_nodes(items: &[Item]) -> Vec<MathNode> {
    let mut nodes: Vec<MathNode> = Vec::new();
    for item in items {
        let mut produced = match item {
            Item::Line(Some(inner)) => line_nodes(inner),
            Item::Line(None) | Item::Embellishment(_) => continue,
            Item::Char {
                value,
                face,
                embellishments,
            } => {
                char_node(&mut nodes, *value, *face, embellishments);
                continue;
            }
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
                selector,
                variation,
                items,
            } => template(*selector, *variation, items),
        };
        nodes.append(&mut produced);
    }
    nodes
}

/// Adds a character to the line: appended to the run before it when both
/// share a style and neither carries an accent.
fn char_node(nodes: &mut Vec<MathNode>, value: u16, face: u8, embellishments: &[u8]) {
    if face == FN_SPACE {
        return;
    }
    let Some(c) = decode(value, face) else {
        return;
    };
    let style = style(face);
    let mut text = c.to_string();
    let mut accents = Vec::new();
    for kind in embellishments {
        match embellishment(*kind) {
            Some(Ok(mark)) => accents.push(mark),
            Some(Err(suffix)) => text.push_str(suffix),
            None => {}
        }
    }
    if accents.is_empty() {
        if let Some(MathNode::Run {
            text: last,
            style: s,
        }) = nodes.last_mut()
        {
            if *s == style {
                last.push_str(&text);
                return;
            }
        }
        nodes.push(MathNode::Run { text, style });
        return;
    }
    let mut node = MathNode::Run { text, style };
    for accent in accents {
        node = MathNode::Accent {
            accent,
            body: vec![node],
        };
    }
    nodes.push(node);
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

fn scripts(items: &[Item]) -> (Option<Vec<MathNode>>, Option<Vec<MathNode>>) {
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

/// The default fences of the fence templates 0 to 7, left and right.
const FENCES: [(char, char); 8] = [
    ('\u{27E8}', '\u{27E9}'),
    ('(', ')'),
    ('{', '}'),
    ('[', ']'),
    ('|', '|'),
    ('\u{2016}', '\u{2016}'),
    ('\u{230A}', '\u{230B}'),
    ('\u{2308}', '\u{2309}'),
];

/// The nodes of an MTEF 3 template by its selector: fences (0 to 12, the
/// last five mixing brackets), radicals (13), fractions (14), under and
/// over bars (16, 17), arrows with text (18 to 20), integrals (21 to 28)
/// and the other n-ary operators (29 and up), each with its main slot,
/// lower and upper limits and operator character. Any other template gives
/// its slots in order.
fn template(selector: u8, variation: u8, items: &[Item]) -> Vec<MathNode> {
    let (mut slots, chars) = slots(items);
    let node = match selector {
        0..=12 => {
            let (left, right) = FENCES.get(usize::from(selector)).copied().unzip();
            MathNode::Delimiter {
                open: chars.first().copied().or(left),
                close: chars
                    .get(1)
                    .or(chars.first().filter(|_| chars.len() == 1))
                    .copied()
                    .or(right),
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
        16 | 17 => MathNode::Bar {
            top: selector == 17,
            body: slot(&mut slots, 0),
        },
        18..=20 => MathNode::Limit {
            base: vec![MathNode::Run {
                text: match selector {
                    18 => "\u{2190}",
                    19 => "\u{2192}",
                    _ => "\u{2194}",
                }
                .into(),
                style: MathStyle::Plain,
            }],
            limit: slots.into_iter().flatten().collect(),
            lower: false,
        },
        21..=48 => {
            let written = slots.get(3).and_then(|operator| match operator.as_slice() {
                [MathNode::Run { text, .. }] => text.chars().next(),
                _ => None,
            });
            let operator = chars.last().copied().or(written).unwrap_or(match selector {
                21 | 25 => '\u{222B}',
                22 | 26 => '\u{222C}',
                23 | 27 => '\u{222D}',
                24 | 28 => '\u{222E}',
                _ => '\u{2211}',
            });
            MathNode::Nary {
                operator,
                sub: optional(&mut slots, 1),
                sup: optional(&mut slots, 2),
                limits_under: selector > 28,
                body: slot(&mut slots, 0),
            }
        }
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

    #[test]
    fn damaged_streams_give_nothing() {
        assert!(read(&[]).is_none());
        assert!(read(&stream(&[5, 1, 1, 3, 0])).is_none());
        let mut deep = vec![3, 1, 1, 3, 0];
        deep.extend(std::iter::repeat_n(LINE, 500));
        assert!(read(&stream(&deep)).is_none());
    }
}

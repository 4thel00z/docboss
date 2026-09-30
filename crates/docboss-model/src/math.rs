/// An Office Math zone (`m:oMath`): its content, and whether it is shown
/// as a display equation on a line of its own (inside `m:oMathPara`).
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Math {
    pub display: bool,
    /// How a display equation sits on its line.
    pub justification: MathJustification,
    pub nodes: Vec<MathNode>,
}

/// How a display equation is placed across its line.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum MathJustification {
    Left,
    #[default]
    Center,
    Right,
}

/// How a run of math text is drawn.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum MathStyle {
    /// Letters italic, everything else upright, as math is by default.
    #[default]
    Math,
    /// Upright (`m:sty p`), as function names are.
    Plain,
    Bold,
    BoldItalic,
    /// Ordinary text with the run's own formatting (`m:nor`).
    Text,
}

/// How a fraction is written.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum FractionKind {
    /// Stacked with a bar.
    #[default]
    Bar,
    /// Slanted, numerator up and to the left.
    Skewed,
    /// On one line, `a/b`.
    Linear,
    /// Stacked without a bar.
    NoBar,
}

/// One object of a math zone. Each argument is a list of nodes.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(tag = "type", rename_all = "snake_case"))]
pub enum MathNode {
    Run {
        text: String,
        style: MathStyle,
    },
    Fraction {
        kind: FractionKind,
        numerator: Vec<MathNode>,
        denominator: Vec<MathNode>,
    },
    /// A base with a subscript, a superscript or both after it, or before
    /// it when `pre`.
    Script {
        base: Vec<MathNode>,
        sub: Option<Vec<MathNode>>,
        sup: Option<Vec<MathNode>>,
        pre: bool,
    },
    Radical {
        degree: Option<Vec<MathNode>>,
        body: Vec<MathNode>,
    },
    /// An n-ary operator such as a sum or integral with its limits, placed
    /// under and over it when `limits_under`, and its operand.
    Nary {
        operator: char,
        sub: Option<Vec<MathNode>>,
        sup: Option<Vec<MathNode>>,
        limits_under: bool,
        body: Vec<MathNode>,
    },
    /// Delimiters around items separated by `separator`; a missing
    /// delimiter is drawn as nothing.
    Delimiter {
        open: Option<char>,
        close: Option<char>,
        separator: char,
        items: Vec<Vec<MathNode>>,
    },
    Matrix {
        rows: Vec<Vec<Vec<MathNode>>>,
    },
    Accent {
        accent: char,
        body: Vec<MathNode>,
    },
    /// A bar over the body when `top`, else under it.
    Bar {
        top: bool,
        body: Vec<MathNode>,
    },
    Function {
        name: Vec<MathNode>,
        body: Vec<MathNode>,
    },
    /// A character stretched over (`top`) or under the body, such as a
    /// brace.
    GroupCharacter {
        character: char,
        top: bool,
        body: Vec<MathNode>,
    },
    /// A base with a limit under it (`lower`) or over it.
    Limit {
        base: Vec<MathNode>,
        limit: Vec<MathNode>,
        lower: bool,
    },
    /// Equations stacked one per row.
    Array {
        rows: Vec<Vec<MathNode>>,
    },
    BorderBox {
        body: Vec<MathNode>,
    },
}

/// Math text as Unicode linear format (Unicode Technical Note 28).
pub fn linear_text(nodes: &[MathNode]) -> String {
    let mut out = String::new();
    linear_into(nodes, &mut out);
    out
}

/// Whether nodes read as one operand without parentheses: a single run
/// of letters and digits, or one object that brackets itself.
fn operand(nodes: &[MathNode]) -> bool {
    match nodes {
        [MathNode::Run { text, .. }] => {
            let single = text.chars().count() == 1;
            let digits = text.chars().all(|c| c.is_ascii_digit() || c == '.');
            single || digits
        }
        [MathNode::Delimiter { .. }] | [MathNode::Matrix { .. }] => true,
        [] => true,
        _ => false,
    }
}

fn argument(nodes: &[MathNode], out: &mut String) {
    if operand(nodes) {
        linear_into(nodes, out);
        return;
    }
    out.push('(');
    linear_into(nodes, out);
    out.push(')');
}

fn linear_into(nodes: &[MathNode], out: &mut String) {
    for (i, node) in nodes.iter().enumerate() {
        linear_node(node, out);
        let scripted = matches!(node, MathNode::Script { pre: false, .. });
        let next_word = match nodes.get(i + 1) {
            Some(MathNode::Run { text, .. }) => text.starts_with(char::is_alphanumeric),
            Some(MathNode::Script {
                base, pre: false, ..
            }) => matches!(
                base.first(),
                Some(MathNode::Run { text, .. }) if text.starts_with(char::is_alphanumeric)
            ),
            _ => false,
        };
        if scripted && next_word {
            out.push(' ');
        }
    }
}

fn linear_node(node: &MathNode, out: &mut String) {
    match node {
        MathNode::Run { text, .. } => out.push_str(text),
        MathNode::Fraction {
            kind,
            numerator,
            denominator,
        } => {
            argument(numerator, out);
            out.push(match kind {
                FractionKind::Skewed => '⁄',
                FractionKind::Linear => '∕',
                FractionKind::NoBar => '¦',
                FractionKind::Bar => '/',
            });
            argument(denominator, out);
        }
        MathNode::Script {
            base,
            sub,
            sup,
            pre,
        } => {
            if !pre {
                argument(base, out);
            }
            if let Some(sub) = sub {
                out.push('_');
                argument(sub, out);
            }
            if let Some(sup) = sup {
                out.push('^');
                argument(sup, out);
            }
            if *pre {
                out.push(' ');
                argument(base, out);
            }
        }
        MathNode::Radical { degree, body } => {
            out.push('√');
            match degree.as_deref().filter(|d| !d.is_empty()) {
                Some(degree) => {
                    out.push('(');
                    linear_into(degree, out);
                    out.push('&');
                    linear_into(body, out);
                    out.push(')');
                }
                None => argument(body, out),
            }
        }
        MathNode::Nary {
            operator,
            sub,
            sup,
            body,
            ..
        } => {
            out.push(*operator);
            if let Some(sub) = sub {
                out.push('_');
                argument(sub, out);
            }
            if let Some(sup) = sup {
                out.push('^');
                argument(sup, out);
            }
            out.push('▒');
            argument(body, out);
        }
        MathNode::Delimiter {
            open,
            close,
            separator,
            items,
        } => {
            out.push(open.unwrap_or('〖'));
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(*separator);
                }
                linear_into(item, out);
            }
            out.push(close.unwrap_or('〗'));
        }
        MathNode::Matrix { rows } => {
            out.push_str("■(");
            grid(rows, out);
            out.push(')');
        }
        MathNode::Accent { accent, body } => {
            argument(body, out);
            out.push(*accent);
        }
        MathNode::Bar { top, body } => {
            out.push(if *top { '¯' } else { '▁' });
            out.push('(');
            linear_into(body, out);
            out.push(')');
        }
        MathNode::Function { name, body } => {
            linear_into(name, out);
            out.push('\u{2061}');
            argument(body, out);
        }
        MathNode::GroupCharacter {
            character, body, ..
        } => {
            out.push(*character);
            out.push('(');
            linear_into(body, out);
            out.push(')');
        }
        MathNode::Limit { base, limit, lower } => {
            argument(base, out);
            out.push(if *lower { '┬' } else { '┴' });
            argument(limit, out);
        }
        MathNode::Array { rows } => {
            out.push_str("█(");
            for (i, row) in rows.iter().enumerate() {
                if i > 0 {
                    out.push('@');
                }
                linear_into(row, out);
            }
            out.push(')');
        }
        MathNode::BorderBox { body } => {
            out.push_str("▭(");
            linear_into(body, out);
            out.push(')');
        }
    }
}

fn grid(rows: &[Vec<Vec<MathNode>>], out: &mut String) {
    for (i, row) in rows.iter().enumerate() {
        if i > 0 {
            out.push('@');
        }
        for (j, cell) in row.iter().enumerate() {
            if j > 0 {
                out.push('&');
            }
            linear_into(cell, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(text: &str) -> Vec<MathNode> {
        vec![MathNode::Run {
            text: text.into(),
            style: MathStyle::Math,
        }]
    }

    /// Unicode Technical Note 28: fractions, scripts, radicals, n-ary
    /// operators and delimiters in linear format, operands of more than
    /// one term in parentheses.
    #[test]
    fn linear_format() {
        let nodes = vec![
            MathNode::Fraction {
                kind: FractionKind::Bar,
                numerator: run("a+b"),
                denominator: run("2"),
            },
            MathNode::Run {
                text: "=".into(),
                style: MathStyle::Math,
            },
            MathNode::Script {
                base: run("x"),
                sub: Some(run("i")),
                sup: Some(run("2n")),
                pre: false,
            },
            MathNode::Nary {
                operator: '∑',
                sub: Some(run("i=1")),
                sup: Some(run("n")),
                limits_under: true,
                body: run("i"),
            },
            MathNode::Radical {
                degree: None,
                body: run("x+1"),
            },
            MathNode::Delimiter {
                open: Some('('),
                close: Some(')'),
                separator: ',',
                items: vec![run("a"), run("b")],
            },
        ];
        assert_eq!(
            linear_text(&nodes),
            "(a+b)/2=x_i^(2n)∑_(i=1)^n▒i√(x+1)(a,b)"
        );
    }
}

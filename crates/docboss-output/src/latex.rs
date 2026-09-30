//! Office Math as LaTeX for Markdown output.

use docboss_model::{FractionKind, MathNode, MathStyle};

/// The LaTeX command for a character, when it has one.
fn command(c: char) -> Option<&'static str> {
    let name = match c {
        'α' => "\\alpha",
        'β' => "\\beta",
        'γ' => "\\gamma",
        'δ' => "\\delta",
        'ε' => "\\epsilon",
        'ϵ' => "\\epsilon",
        'ζ' => "\\zeta",
        'η' => "\\eta",
        'θ' => "\\theta",
        'ϑ' => "\\vartheta",
        'ι' => "\\iota",
        'κ' => "\\kappa",
        'λ' => "\\lambda",
        'μ' => "\\mu",
        'ν' => "\\nu",
        'ξ' => "\\xi",
        'π' => "\\pi",
        'ρ' => "\\rho",
        'σ' => "\\sigma",
        'ς' => "\\varsigma",
        'τ' => "\\tau",
        'υ' => "\\upsilon",
        'φ' => "\\phi",
        'ϕ' => "\\phi",
        'χ' => "\\chi",
        'ψ' => "\\psi",
        'ω' => "\\omega",
        'Γ' => "\\Gamma",
        'Δ' => "\\Delta",
        'Θ' => "\\Theta",
        'Λ' => "\\Lambda",
        'Ξ' => "\\Xi",
        'Π' => "\\Pi",
        'Σ' => "\\Sigma",
        'Υ' => "\\Upsilon",
        'Φ' => "\\Phi",
        'Ψ' => "\\Psi",
        'Ω' => "\\Omega",
        '∞' => "\\infty",
        '±' => "\\pm",
        '∓' => "\\mp",
        '×' => "\\times",
        '÷' => "\\div",
        '⋅' | '·' => "\\cdot",
        '∙' => "\\bullet",
        '≤' => "\\le",
        '≥' => "\\ge",
        '≠' => "\\ne",
        '≈' => "\\approx",
        '≡' => "\\equiv",
        '∼' => "\\sim",
        '≅' => "\\cong",
        '∝' => "\\propto",
        '→' => "\\to",
        '←' => "\\leftarrow",
        '↔' => "\\leftrightarrow",
        '⇒' => "\\Rightarrow",
        '⇐' => "\\Leftarrow",
        '⇔' => "\\Leftrightarrow",
        '∂' => "\\partial",
        '∇' => "\\nabla",
        '∈' => "\\in",
        '∉' => "\\notin",
        '⊂' => "\\subset",
        '⊃' => "\\supset",
        '⊆' => "\\subseteq",
        '⊇' => "\\supseteq",
        '∪' => "\\cup",
        '∩' => "\\cap",
        '∅' => "\\emptyset",
        '∀' => "\\forall",
        '∃' => "\\exists",
        '¬' => "\\neg",
        '∧' => "\\wedge",
        '∨' => "\\vee",
        '…' => "\\ldots",
        '⋯' => "\\cdots",
        '′' => "'",
        '°' => "^{\\circ}",
        '∑' => "\\sum",
        '∏' => "\\prod",
        '∐' => "\\coprod",
        '∫' => "\\int",
        '∬' => "\\iint",
        '∭' => "\\iiint",
        '∮' => "\\oint",
        '⋃' => "\\bigcup",
        '⋂' => "\\bigcap",
        '⨁' => "\\bigoplus",
        '⨂' => "\\bigotimes",
        '⟨' => "\\langle",
        '⟩' => "\\rangle",
        '⌊' => "\\lfloor",
        '⌋' => "\\rfloor",
        '⌈' => "\\lceil",
        '⌉' => "\\rceil",
        '‖' => "\\|",
        '−' => "-",
        '\u{2061}' | '\u{2062}' | '\u{2063}' => "",
        _ => return None,
    };
    Some(name)
}

fn character(c: char, out: &mut String) {
    if let Some(name) = command(c) {
        out.push_str(name);
        if name.starts_with('\\') && name[1..].chars().all(char::is_alphabetic) {
            out.push(' ');
        }
        return;
    }
    match c {
        '{' | '}' | '#' | '$' | '%' | '&' | '_' => {
            out.push('\\');
            out.push(c);
        }
        '\\' => out.push_str("\\backslash "),
        '^' => out.push_str("\\hat{}"),
        '~' => out.push_str("\\sim "),
        _ => out.push(c),
    }
}

/// Function names LaTeX has a command for.
const FUNCTIONS: &[&str] = &[
    "sin", "cos", "tan", "cot", "sec", "csc", "arcsin", "arccos", "arctan", "sinh", "cosh", "tanh",
    "coth", "log", "ln", "lg", "exp", "lim", "max", "min", "sup", "inf", "det", "dim", "ker",
    "deg", "gcd", "arg", "Pr",
];

fn plain_text(nodes: &[MathNode]) -> Option<String> {
    let mut text = String::new();
    for node in nodes {
        let MathNode::Run { text: t, .. } = node else {
            return None;
        };
        text.push_str(t);
    }
    Some(text)
}

fn text_run(text: &str, style: MathStyle, out: &mut String) {
    let wrap = match style {
        MathStyle::Text => Some("\\text{"),
        MathStyle::Plain if text.chars().any(char::is_alphabetic) => Some("\\mathrm{"),
        MathStyle::Bold => Some("\\mathbf{"),
        MathStyle::BoldItalic => Some("\\boldsymbol{"),
        _ => None,
    };
    if let Some(open) = wrap {
        out.push_str(open);
    }
    for c in text.chars() {
        if style == MathStyle::Text && c == ' ' {
            out.push(' ');
            continue;
        }
        character(c, out);
    }
    if wrap.is_some() {
        out.push('}');
    }
}

fn group(nodes: &[MathNode], out: &mut String) {
    out.push('{');
    write(nodes, out);
    out.push('}');
}

/// The name `\left` and `\right` take for a delimiter character, `.`
/// for none; `None` for a character LaTeX has no delimiter for.
fn delimiter_name(c: Option<char>) -> Option<&'static str> {
    let Some(c) = c else {
        return Some(".");
    };
    let name = match c {
        '(' => "(",
        ')' => ")",
        '[' => "[",
        ']' => "]",
        '|' => "|",
        '/' => "/",
        '{' => "\\{",
        '}' => "\\}",
        '\u{2329}' | '\u{3008}' | '\u{27E8}' => "\\langle ",
        '\u{232A}' | '\u{3009}' | '\u{27E9}' => "\\rangle ",
        '∥' | '‖' => "\\|",
        '⟦' => "\\llbracket ",
        '⟧' => "\\rrbracket ",
        '⌊' => "\\lfloor ",
        '⌋' => "\\rfloor ",
        '⌈' => "\\lceil ",
        '⌉' => "\\rceil ",
        _ => return None,
    };
    Some(name)
}

/// The accent command for an accent character.
fn accent(c: char) -> &'static str {
    match c {
        '\u{0300}' | '`' => "\\grave",
        '\u{0301}' | '´' => "\\acute",
        '\u{0303}' | '~' | '˜' => "\\tilde",
        '\u{0304}' | '\u{0305}' | '¯' => "\\bar",
        '\u{0306}' | '˘' => "\\breve",
        '\u{0307}' | '˙' => "\\dot",
        '\u{0308}' | '¨' => "\\ddot",
        '\u{030C}' | 'ˇ' => "\\check",
        '\u{20D7}' | '\u{20D1}' | '→' => "\\vec",
        _ => "\\hat",
    }
}

/// Writes math nodes as LaTeX.
pub(crate) fn write(nodes: &[MathNode], out: &mut String) {
    for node in nodes {
        node_latex(node, out);
    }
}

fn node_latex(node: &MathNode, out: &mut String) {
    match node {
        MathNode::Run { text, style } => text_run(text, *style, out),
        MathNode::Fraction {
            kind,
            numerator,
            denominator,
        } => match kind {
            FractionKind::Linear => {
                group(numerator, out);
                out.push('/');
                group(denominator, out);
            }
            FractionKind::NoBar => {
                out.push_str("\\genfrac{}{}{0pt}{}");
                group(numerator, out);
                group(denominator, out);
            }
            _ => {
                out.push_str("\\frac");
                group(numerator, out);
                group(denominator, out);
            }
        },
        MathNode::Script {
            base,
            sub,
            sup,
            pre,
        } => {
            if *pre {
                out.push_str("{}");
            } else {
                group(base, out);
            }
            if let Some(sub) = sub {
                out.push('_');
                group(sub, out);
            }
            if let Some(sup) = sup {
                out.push('^');
                group(sup, out);
            }
            if *pre {
                group(base, out);
            }
        }
        MathNode::Radical { degree, body } => {
            out.push_str("\\sqrt");
            if let Some(degree) = degree {
                out.push('[');
                write(degree, out);
                out.push(']');
            }
            group(body, out);
        }
        MathNode::Nary {
            operator,
            sub,
            sup,
            limits_under,
            body,
        } => {
            character(*operator, out);
            if *limits_under && command(*operator).is_some_and(|c| c.contains("int")) {
                out.push_str("\\limits");
            }
            if let Some(sub) = sub {
                out.push('_');
                group(sub, out);
            }
            if let Some(sup) = sup {
                out.push('^');
                group(sup, out);
            }
            group(body, out);
        }
        MathNode::Delimiter {
            open,
            close,
            separator,
            items,
        } => {
            match delimiter_name(*open) {
                Some(name) => {
                    out.push_str("\\left");
                    out.push_str(name);
                }
                None => {
                    out.push_str("\\left.");
                    open.iter().for_each(|c| character(*c, out));
                }
            }
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    match separator {
                        '|' | '│' => out.push_str("\\middle|"),
                        c => character(*c, out),
                    }
                }
                write(item, out);
            }
            match delimiter_name(*close) {
                Some(name) => {
                    out.push_str("\\right");
                    out.push_str(name);
                }
                None => {
                    close.iter().for_each(|c| character(*c, out));
                    out.push_str("\\right.");
                }
            }
        }
        MathNode::Matrix { rows } => {
            out.push_str("\\begin{matrix}");
            for (i, row) in rows.iter().enumerate() {
                if i > 0 {
                    out.push_str("\\\\");
                }
                for (j, cell) in row.iter().enumerate() {
                    if j > 0 {
                        out.push('&');
                    }
                    write(cell, out);
                }
            }
            out.push_str("\\end{matrix}");
        }
        MathNode::Accent { accent: c, body } => {
            out.push_str(accent(*c));
            group(body, out);
        }
        MathNode::Bar { top, body } => {
            out.push_str(if *top { "\\overline" } else { "\\underline" });
            group(body, out);
        }
        MathNode::Function { name, body } => {
            match plain_text(name) {
                Some(text) if FUNCTIONS.contains(&text.as_str()) => {
                    out.push('\\');
                    out.push_str(&text);
                    out.push(' ');
                }
                Some(text) => {
                    out.push_str("\\operatorname{");
                    for c in text.chars() {
                        character(c, out);
                    }
                    out.push('}');
                }
                None => group(name, out),
            }
            group(body, out);
        }
        MathNode::GroupCharacter { top, body, .. } => {
            out.push_str(if *top { "\\overbrace" } else { "\\underbrace" });
            group(body, out);
        }
        MathNode::Limit { base, limit, lower } => {
            if let Some(name) = plain_text(base).filter(|t| FUNCTIONS.contains(&t.as_str())) {
                out.push('\\');
                out.push_str(&name);
                out.push_str(if *lower { "_" } else { "^" });
                group(limit, out);
                return;
            }
            out.push_str(if *lower { "\\underset" } else { "\\overset" });
            group(limit, out);
            group(base, out);
        }
        MathNode::Array { rows } => {
            out.push_str("\\begin{gathered}");
            for (i, row) in rows.iter().enumerate() {
                if i > 0 {
                    out.push_str("\\\\");
                }
                write(row, out);
            }
            out.push_str("\\end{gathered}");
        }
        MathNode::BorderBox { body } => {
            out.push_str("\\boxed");
            group(body, out);
        }
    }
}

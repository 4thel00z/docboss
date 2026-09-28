//! Unicode for characters drawn from the Symbol and Wingdings fonts, which
//! place their glyphs at codes of their own (also mirrored into the private
//! use area at U+F000 plus the code).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SymbolFont {
    Symbol,
    Wingdings,
}

impl SymbolFont {
    pub(crate) fn from_name(name: &str) -> Option<SymbolFont> {
        let name = name.trim().to_ascii_lowercase();
        if name == "symbol" {
            return Some(SymbolFont::Symbol);
        }
        if name.starts_with("wingdings") || name == "webdings" {
            return Some(SymbolFont::Wingdings);
        }
        None
    }
}

#[rustfmt::skip]
const SYMBOL: [char; 95] = [
    '\u{a0}', '!', '∀', '#', '∃', '%', '&', '∋', '(', ')', '∗', '+', ',', '−', '.', '/',
    '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', ':', ';', '<', '=', '>', '?',
    '≅', 'Α', 'Β', 'Χ', 'Δ', 'Ε', 'Φ', 'Γ', 'Η', 'Ι', 'ϑ', 'Κ', 'Λ', 'Μ', 'Ν', 'Ο',
    'Π', 'Θ', 'Ρ', 'Σ', 'Τ', 'Υ', 'ς', 'Ω', 'Ξ', 'Ψ', 'Ζ', '[', '∴', ']', '⊥', '_',
    '‾', 'α', 'β', 'χ', 'δ', 'ε', 'φ', 'γ', 'η', 'ι', 'ϕ', 'κ', 'λ', 'μ', 'ν', 'ο',
    'π', 'θ', 'ρ', 'σ', 'τ', 'υ', 'ϖ', 'ω', 'ξ', 'ψ', 'ζ', '{', '|', '}', '∼',
];

#[rustfmt::skip]
const SYMBOL_HIGH: [char; 95] = [
    '€', 'ϒ', '′', '≤', '⁄', '∞', 'ƒ', '♣', '♦', '♥', '♠', '↔', '←', '↑', '→', '↓',
    '°', '±', '″', '≥', '×', '∝', '∂', '•', '÷', '≠', '≡', '≈', '…', '⏐', '⎯', '↵',
    'ℵ', 'ℑ', 'ℜ', '℘', '⊗', '⊕', '∅', '∩', '∪', '⊃', '⊇', '⊄', '⊂', '⊆', '∈', '∉',
    '∠', '∇', '®', '©', '™', '∏', '√', '⋅', '¬', '∧', '∨', '⇔', '⇐', '⇑', '⇒', '⇓',
    '◊', '〈', '®', '©', '™', '∑', '⎛', '⎜', '⎝', '⎡', '⎢', '⎣', '⎧', '⎨', '⎩', '⎪',
    '\u{f8ff}', '〉', '∫', '⌠', '⎮', '⌡', '⎞', '⎟', '⎠', '⎤', '⎥', '⎦', '⎫', '⎬', '⎭',
];

const WINGDINGS: [(u8, char); 24] = [
    (0x28, '☎'),
    (0x2a, '✉'),
    (0x46, '☞'),
    (0x4a, '☺'),
    (0x4c, '☹'),
    (0x6c, '●'),
    (0x6e, '■'),
    (0x6f, '□'),
    (0x71, '❑'),
    (0x72, '❒'),
    (0x75, '◆'),
    (0x76, '❖'),
    (0x77, '⬥'),
    (0x9e, '·'),
    (0x9f, '•'),
    (0xa1, '○'),
    (0xa7, '▪'),
    (0xa8, '◻'),
    (0xd8, '➢'),
    (0xe8, '➔'),
    (0xfb, '✗'),
    (0xfc, '✓'),
    (0xfd, '☒'),
    (0xfe, '☑'),
];

/// The Unicode character a symbol-font code stands for, when known.
pub(crate) fn map(font: SymbolFont, c: char) -> Option<char> {
    let code = u32::from(c);
    let code = if (0xf000..=0xf0ff).contains(&code) {
        code - 0xf000
    } else {
        code
    };
    if code > 0xff {
        return None;
    }
    let code = code as u8;
    match font {
        SymbolFont::Symbol => match code {
            0x20..=0x7e => Some(SYMBOL[usize::from(code - 0x20)]),
            0xa0..=0xfe => Some(SYMBOL_HIGH[usize::from(code - 0xa0)]),
            _ => None,
        },
        SymbolFont::Wingdings => WINGDINGS
            .iter()
            .find(|(key, _)| *key == code)
            .map(|(_, mapped)| *mapped),
    }
}

/// Maps a whole string drawn in a symbol font; unknown codes in the
/// private use area become a bullet, other characters are kept.
pub(crate) fn map_str(font: SymbolFont, text: &str) -> String {
    text.chars()
        .map(|c| {
            map(font, c).unwrap_or(if ('\u{f000}'..='\u{f0ff}').contains(&c) {
                '•'
            } else {
                c
            })
        })
        .collect()
}

/// A list label drawn in the level's font, with private-use and
/// symbol-font characters turned into their Unicode equivalents.
pub(crate) fn label(font: Option<&str>, text: &str) -> String {
    let symbol_font = font.and_then(SymbolFont::from_name);
    if let Some(font) = symbol_font {
        return text.chars().map(|c| map(font, c).unwrap_or('•')).collect();
    }
    text.chars()
        .map(|c| {
            if ('\u{e000}'..='\u{f8ff}').contains(&c) {
                '•'
            } else {
                c
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_bullets_map() {
        assert_eq!(map(SymbolFont::Symbol, '\u{f0b7}'), Some('•'));
        assert_eq!(map(SymbolFont::Symbol, 'a'), Some('α'));
        assert_eq!(map(SymbolFont::Wingdings, '\u{f0a7}'), Some('▪'));
        assert_eq!(map(SymbolFont::Wingdings, '\u{f0fc}'), Some('✓'));
        assert_eq!(label(Some("Symbol"), "\u{f0b7}"), "•");
        assert_eq!(label(Some("Courier New"), "o"), "o");
        assert_eq!(label(None, "\u{f0a7}"), "•");
    }
}

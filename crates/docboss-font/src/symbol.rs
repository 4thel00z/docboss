//! Unicode for the codes of Word's symbol fonts.
//!
//! WordprocessingML stores a character of a symbol font such as Symbol or
//! Wingdings as its one-byte font code, usually moved into the private-use
//! range U+F000 to U+F0FF (ECMA-376 Part 1 §17.3.3.30). When the font
//! itself is not installed, its codes are translated to the Unicode
//! characters they draw so another face can show them.

const SYMBOL_ASCII: [u16; 95] = [
    0x0020, 0x0021, 0x2200, 0x0023, 0x2203, 0x0025, 0x0026, 0x220B, 0x0028, 0x0029, 0x2217, 0x002B,
    0x002C, 0x2212, 0x002E, 0x002F, 0x0030, 0x0031, 0x0032, 0x0033, 0x0034, 0x0035, 0x0036, 0x0037,
    0x0038, 0x0039, 0x003A, 0x003B, 0x003C, 0x003D, 0x003E, 0x003F, 0x2245, 0x0391, 0x0392, 0x03A7,
    0x0394, 0x0395, 0x03A6, 0x0393, 0x0397, 0x0399, 0x03D1, 0x039A, 0x039B, 0x039C, 0x039D, 0x039F,
    0x03A0, 0x0398, 0x03A1, 0x03A3, 0x03A4, 0x03A5, 0x03C2, 0x03A9, 0x039E, 0x03A8, 0x0396, 0x005B,
    0x2234, 0x005D, 0x22A5, 0x005F, 0x203E, 0x03B1, 0x03B2, 0x03C7, 0x03B4, 0x03B5, 0x03C6, 0x03B3,
    0x03B7, 0x03B9, 0x03D5, 0x03BA, 0x03BB, 0x03BC, 0x03BD, 0x03BF, 0x03C0, 0x03B8, 0x03C1, 0x03C3,
    0x03C4, 0x03C5, 0x03D6, 0x03C9, 0x03BE, 0x03C8, 0x03B6, 0x007B, 0x007C, 0x007D, 0x223C,
];

const SYMBOL_HIGH: [u16; 95] = [
    0x20AC, 0x03D2, 0x2032, 0x2264, 0x2044, 0x221E, 0x0192, 0x2663, 0x2666, 0x2665, 0x2660, 0x2194,
    0x2190, 0x2191, 0x2192, 0x2193, 0x00B0, 0x00B1, 0x2033, 0x2265, 0x00D7, 0x221D, 0x2202, 0x2022,
    0x00F7, 0x2260, 0x2261, 0x2248, 0x2026, 0x23D0, 0x23AF, 0x21B5, 0x2135, 0x2111, 0x211C, 0x2118,
    0x2297, 0x2295, 0x2205, 0x2229, 0x222A, 0x2283, 0x2287, 0x2284, 0x2282, 0x2286, 0x2208, 0x2209,
    0x2220, 0x2207, 0x00AE, 0x00A9, 0x2122, 0x220F, 0x221A, 0x22C5, 0x00AC, 0x2227, 0x2228, 0x21D4,
    0x21D0, 0x21D1, 0x21D2, 0x21D3, 0x25CA, 0x2329, 0x00AE, 0x00A9, 0x2122, 0x2211, 0x239B, 0x239C,
    0x239D, 0x23A1, 0x23A2, 0x23A3, 0x23A7, 0x23A8, 0x23A9, 0x23AA, 0x0000, 0x232A, 0x222B, 0x2320,
    0x23AE, 0x2321, 0x239E, 0x239F, 0x23A0, 0x23A4, 0x23A5, 0x23A6, 0x23AB, 0x23AC, 0x23AD,
];

const WINGDINGS: [(u8, char); 21] = [
    (0x4A, '\u{263A}'),
    (0x4C, '\u{2639}'),
    (0x6C, '\u{25CF}'),
    (0x6E, '\u{25A0}'),
    (0x6F, '\u{25A1}'),
    (0x71, '\u{2751}'),
    (0x72, '\u{2752}'),
    (0x75, '\u{25C6}'),
    (0x76, '\u{2756}'),
    (0x77, '\u{2B25}'),
    (0x9F, '\u{2022}'),
    (0xA1, '\u{25CB}'),
    (0xA7, '\u{25AA}'),
    (0xA8, '\u{25FB}'),
    (0xD8, '\u{27A2}'),
    (0xE8, '\u{2794}'),
    (0xF0, '\u{21E8}'),
    (0xFB, '\u{2717}'),
    (0xFC, '\u{2714}'),
    (0xFD, '\u{2612}'),
    (0xFE, '\u{2611}'),
];

const CP1252_HIGH: [u16; 32] = [
    0x20AC, 0x0000, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160, 0x2039,
    0x0152, 0x0000, 0x017D, 0x0000, 0x0000, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014,
    0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0x0000, 0x017E, 0x0178,
];

fn normalize(family: &str) -> String {
    family
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn symbol_code(code: u8) -> Option<char> {
    let table = match code {
        0x20..=0x7E => SYMBOL_ASCII.get(usize::from(code - 0x20)),
        0xA0..=0xFE => SYMBOL_HIGH.get(usize::from(code - 0xA0)),
        _ => None,
    };
    table
        .copied()
        .filter(|&u| u != 0)
        .and_then(|u| char::from_u32(u32::from(u)))
}

fn wingdings_code(code: u8) -> Option<char> {
    WINGDINGS
        .iter()
        .find(|(c, _)| *c == code)
        .map_or(Some('\u{2022}'), |(_, u)| Some(*u))
}

fn windows_1252(code: u8) -> Option<char> {
    match code {
        0x20..=0x7E | 0xA0..=0xFF => Some(char::from(code)),
        0x80..=0x9F => {
            let u = CP1252_HIGH[usize::from(code - 0x80)];
            (u != 0).then(|| char::from_u32(u32::from(u))).flatten()
        }
        _ => None,
    }
}

/// The Unicode character a symbol font's code draws: the Adobe Symbol
/// encoding for Symbol, the common bullets and marks of Wingdings (a
/// round bullet for its other codes), and Windows-1252 for any other font,
/// which is how StarOffice's `starbats` and similar fonts store bullets.
/// `c` may be the bare code or its U+F000 private-use form.
pub fn symbol_to_unicode(family: &str, c: char) -> Option<char> {
    let cp = c as u32;
    let code = match cp {
        0xF020..=0xF0FF => (cp - 0xF000) as u8,
        0x20..=0xFF => cp as u8,
        _ => return None,
    };
    match normalize(family).as_str() {
        "symbol" => symbol_code(code),
        "wingdings" => wingdings_code(code),
        _ => windows_1252(code),
    }
}

#[cfg(test)]
mod tests {
    use super::symbol_to_unicode;

    #[test]
    fn symbol_codes_map_through_the_adobe_symbol_encoding() {
        assert_eq!(symbol_to_unicode("Symbol", '\u{F0B7}'), Some('\u{2022}'));
        assert_eq!(symbol_to_unicode("Symbol", '\u{F061}'), Some('α'));
        assert_eq!(symbol_to_unicode("Symbol", '\u{F0A5}'), Some('∞'));
        assert_eq!(symbol_to_unicode("Symbol", 'W'), Some('Ω'));
    }

    #[test]
    fn wingdings_bullets_have_unicode_forms() {
        assert_eq!(symbol_to_unicode("Wingdings", '\u{F0A7}'), Some('\u{25AA}'));
        assert_eq!(symbol_to_unicode("Wingdings", '\u{F0D8}'), Some('\u{27A2}'));
        assert_eq!(symbol_to_unicode("Wingdings", '\u{F0FC}'), Some('\u{2714}'));
        assert_eq!(symbol_to_unicode("Wingdings", '\u{F021}'), Some('\u{2022}'));
    }

    #[test]
    fn other_symbol_fonts_read_as_windows_1252() {
        assert_eq!(symbol_to_unicode("starbats", '\u{F095}'), Some('\u{2022}'));
        assert_eq!(symbol_to_unicode("starbats", '\u{0100}'), None);
    }
}

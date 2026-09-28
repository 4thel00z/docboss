//! Line break opportunities: a compact approximation of the Unicode line
//! breaking algorithm (UAX #14) covering spaces, hyphens, dashes, glue
//! characters, CJK ideographs and their opening and closing punctuation.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Class {
    Space,
    /// A break is allowed after it: hyphens, soft hyphen, zero-width space.
    After,
    /// A break is allowed before and after it: the em dash.
    Both,
    Ideograph,
    Open,
    Close,
    /// Prohibits a break on either side.
    Glue,
    Other,
}

fn class(c: char) -> Class {
    match c {
        ' '
        | '\u{3000}'
        | '\u{1680}'
        | '\u{2000}'..='\u{2006}'
        | '\u{2008}'..='\u{200A}'
        | '\u{205F}' => Class::Space,
        '-' | '\u{2010}' | '\u{2012}' | '\u{2013}' | '\u{00AD}' | '\u{200B}' | '\u{058A}' => {
            Class::After
        }
        '\u{2014}' => Class::Both,
        '\u{00A0}' | '\u{2011}' | '\u{202F}' | '\u{2060}' | '\u{FEFF}' => Class::Glue,
        '(' | '[' | '{' | '\u{3008}' | '\u{300A}' | '\u{300C}' | '\u{300E}' | '\u{3010}'
        | '\u{FF08}' | '\u{FF3B}' | '\u{FF5B}' | '\u{201C}' | '\u{2018}' => Class::Open,
        ')' | ']' | '}' | ',' | '.' | ':' | ';' | '!' | '?' | '\u{3001}' | '\u{3002}'
        | '\u{3009}' | '\u{300B}' | '\u{300D}' | '\u{300F}' | '\u{3011}' | '\u{FF09}'
        | '\u{FF0C}' | '\u{FF0E}' | '\u{FF1A}' | '\u{FF1B}' | '\u{FF01}' | '\u{FF1F}'
        | '\u{FF3D}' | '\u{FF5D}' | '\u{30FC}' | '\u{201D}' | '\u{2019}' | '%' => Class::Close,
        '\u{2E80}'..='\u{2FFF}'
        | '\u{3040}'..='\u{30FF}'
        | '\u{3400}'..='\u{4DBF}'
        | '\u{4E00}'..='\u{9FFF}'
        | '\u{F900}'..='\u{FAFF}'
        | '\u{FF10}'..='\u{FF19}'
        | '\u{FF21}'..='\u{FF3A}'
        | '\u{FF41}'..='\u{FF5A}'
        | '\u{20000}'..='\u{3FFFD}' => Class::Ideograph,
        _ => Class::Other,
    }
}

/// Whether a line may break between `prev` and `next`.
pub(crate) fn allowed(prev: char, next: char) -> bool {
    let (p, n) = (class(prev), class(next));
    if n == Class::Space
        || n == Class::Close
        || n == Class::Glue
        || p == Class::Glue
        || p == Class::Open
    {
        return false;
    }
    if p == Class::Space || p == Class::Both || n == Class::Both {
        return true;
    }
    if p == Class::After {
        return !next.is_ascii_digit() || prev != '-';
    }
    p == Class::Ideograph || n == Class::Ideograph
}

/// Whether a character is a space that hangs past the end of a line.
pub(crate) fn is_space(c: char) -> bool {
    class(c) == Class::Space
}

#[cfg(test)]
mod tests {
    use super::allowed;

    #[test]
    fn opportunities() {
        assert!(allowed(' ', 'a'));
        assert!(!allowed('a', ' '));
        assert!(!allowed('a', 'b'));
        assert!(allowed('-', 'b'));
        assert!(!allowed('-', '5'));
        assert!(!allowed('\u{00A0}', 'a'));
        assert!(allowed('中', '文'));
        assert!(!allowed('中', '。'));
        assert!(!allowed('「', '中'));
        assert!(allowed('a', '\u{2014}'));
    }
}

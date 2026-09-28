use std::borrow::Cow;

/// The document as UTF-8 text: a UTF-8 byte order mark is dropped, UTF-16
/// (by BOM or by a leading `<` padded with NUL) is transcoded, and invalid
/// UTF-8 is decoded as Windows-1252 when the declaration names that
/// encoding, else replaced with U+FFFD.
pub fn decode(bytes: &[u8]) -> Cow<'_, str> {
    if let Some(rest) = bytes.strip_prefix(b"\xEF\xBB\xBF") {
        return utf8(rest);
    }
    match bytes {
        [0xFF, 0xFE, rest @ ..] => Cow::Owned(utf16(rest, u16::from_le_bytes)),
        [0xFE, 0xFF, rest @ ..] => Cow::Owned(utf16(rest, u16::from_be_bytes)),
        [b'<', 0, ..] => Cow::Owned(utf16(bytes, u16::from_le_bytes)),
        [0, b'<', ..] => Cow::Owned(utf16(bytes, u16::from_be_bytes)),
        _ => utf8(bytes),
    }
}

fn utf8(bytes: &[u8]) -> Cow<'_, str> {
    if let Ok(text) = std::str::from_utf8(bytes) {
        return Cow::Borrowed(text);
    }
    let head = &bytes[..bytes.len().min(200)];
    let declared = String::from_utf8_lossy(head).to_ascii_lowercase();
    if declared.contains("windows-1252") || declared.contains("iso-8859-1") {
        return Cow::Owned(bytes.iter().map(|&b| windows1252(b)).collect());
    }
    String::from_utf8_lossy(bytes)
}

fn utf16(bytes: &[u8], word: fn([u8; 2]) -> u16) -> String {
    let (units, _) = bytes.as_chunks::<2>();
    char::decode_utf16(units.iter().map(|&pair| word(pair)))
        .map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect()
}

/// A Windows-1252 byte as a character.
pub fn windows1252(byte: u8) -> char {
    const HIGH: [u16; 32] = [
        0x20AC, 0x81, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160,
        0x2039, 0x0152, 0x8D, 0x017D, 0x8F, 0x90, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013,
        0x2014, 0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0x9D, 0x017E, 0x0178,
    ];
    match byte {
        0x80..=0x9F => {
            char::from_u32(u32::from(HIGH[usize::from(byte - 0x80)])).unwrap_or('\u{FFFD}')
        }
        _ => char::from(byte),
    }
}

/// Replaces the predefined entities and character references in `raw`.
/// A malformed or unknown reference is kept literally. CR LF and lone CR
/// become LF (XML 1.0 §2.11); with `attribute` set, tab, CR and LF become
/// spaces (XML 1.0 §3.3.3).
pub fn unescape(raw: &str, attribute: bool) -> Cow<'_, str> {
    let bytes = raw.as_bytes();
    let needs_work = memchr::memchr2(b'&', b'\r', bytes).is_some()
        || attribute && memchr::memchr2(b'\t', b'\n', bytes).is_some();
    if !needs_work {
        return Cow::Borrowed(raw);
    }
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(at) = rest.find(['&', '\r', '\t', '\n']) {
        out.push_str(&rest[..at]);
        let tail = &rest[at..];
        let c = tail.as_bytes()[0];
        if c != b'&' {
            let skip = if c == b'\r' && tail.as_bytes().get(1) == Some(&b'\n') {
                2
            } else {
                1
            };
            out.push(if attribute {
                ' '
            } else if c == b'\r' {
                '\n'
            } else {
                char::from(c)
            });
            rest = &tail[skip..];
            continue;
        }
        let Some(end) = tail[..tail.len().min(12)].find(';') else {
            out.push('&');
            rest = &tail[1..];
            continue;
        };
        match reference(&tail[1..end]) {
            Some(c) => {
                out.push(c);
                rest = &tail[end + 1..];
            }
            None => {
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    Cow::Owned(out)
}

fn reference(name: &str) -> Option<char> {
    match name {
        "lt" => Some('<'),
        "gt" => Some('>'),
        "amp" => Some('&'),
        "apos" => Some('\''),
        "quot" => Some('"'),
        _ => {
            let number = name.strip_prefix('#')?;
            let code = match number.strip_prefix(['x', 'X']) {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => number.parse().ok()?,
            };
            char::from_u32(code)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entities_and_references() {
        assert_eq!(
            unescape("a &lt;b&gt; &amp; &#65;&#x42; &bogus; & end", false),
            "a <b> & AB &bogus; & end"
        );
        assert!(matches!(unescape("plain", false), Cow::Borrowed("plain")));
        assert_eq!(unescape("a\r\nb\rc", false), "a\nb\nc");
        assert_eq!(unescape("a\tb\nc", true), "a b c");
        assert_eq!(unescape("&#xD800;", false), "&#xD800;");
    }

    #[test]
    fn transcodes_byte_order_marks() {
        assert_eq!(decode(b"\xEF\xBB\xBF<a/>"), "<a/>");
        assert_eq!(
            decode(&[0xFF, 0xFE, b'<', 0, b'a', 0, b'/', 0, b'>', 0]),
            "<a/>"
        );
        assert_eq!(
            decode(&[0xFE, 0xFF, 0, b'<', 0, b'a', 0, b'/', 0, b'>']),
            "<a/>"
        );
        assert_eq!(
            decode(b"<?xml encoding=\"windows-1252\"?><a>\x93q\x94</a>"),
            "<?xml encoding=\"windows-1252\"?><a>\u{201C}q\u{201D}</a>"
        );
        assert_eq!(decode(b"<a>\xFF</a>"), "<a>\u{FFFD}</a>");
    }
}

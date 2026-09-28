//! Standard base64 decoding for the Agile encryption descriptor, which
//! stores salts and keys as `xsd:base64Binary`.

fn value(c: u8) -> Option<u32> {
    match c {
        b'A'..=b'Z' => Some(u32::from(c - b'A')),
        b'a'..=b'z' => Some(u32::from(c - b'a') + 26),
        b'0'..=b'9' => Some(u32::from(c - b'0') + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

/// Decodes base64, skipping white space; `None` on any other invalid byte.
pub fn decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let mut acc = 0u32;
    let mut bits = 0;
    for &c in text.as_bytes() {
        if c.is_ascii_whitespace() {
            continue;
        }
        if c == b'=' {
            break;
        }
        acc = (acc << 6) | value(c)?;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    #[test]
    fn decodes_padded_and_unpadded() {
        assert_eq!(super::decode("aGVsbG8=").unwrap(), b"hello");
        assert_eq!(super::decode("aGVsbG8").unwrap(), b"hello");
        assert_eq!(super::decode("aGVs\nbG8h").unwrap(), b"hello!");
        assert!(super::decode("a*b").is_none());
    }
}

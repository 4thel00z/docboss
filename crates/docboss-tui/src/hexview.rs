//! Hex view lines: 16 bytes per line with an offset column and the ASCII
//! rendering of the bytes.

pub const BYTES_PER_LINE: usize = 16;

/// How many lines `len` bytes take.
pub fn line_count(len: usize) -> usize {
    len.div_ceil(BYTES_PER_LINE)
}

/// One hex line for the 16 bytes starting at `line * 16`.
pub fn line(bytes: &[u8], line: usize) -> Option<String> {
    let start = line.checked_mul(BYTES_PER_LINE)?;
    let chunk = bytes.get(start..(start + BYTES_PER_LINE).min(bytes.len()))?;
    if chunk.is_empty() {
        return None;
    }
    let mut out = format!("{start:08x}  ");
    for column in 0..BYTES_PER_LINE {
        match chunk.get(column) {
            Some(byte) => out.push_str(&format!("{byte:02x} ")),
            None => out.push_str("   "),
        }
        if column == 7 {
            out.push(' ');
        }
    }
    out.push(' ');
    out.push('|');
    out.extend(chunk.iter().map(|&byte| match byte {
        0x20..=0x7e => byte as char,
        _ => '.',
    }));
    out.push('|');
    Some(out)
}

/// `count` lines starting at line `first`.
pub fn lines(bytes: &[u8], first: usize, count: usize) -> Vec<String> {
    (first..first.saturating_add(count))
        .map_while(|n| line(bytes, n))
        .collect()
}

/// A hexdump of at most `limit` bytes, for copying.
pub fn hexdump(bytes: &[u8], limit: usize) -> String {
    let shown = &bytes[..bytes.len().min(limit)];
    let mut out: String = lines(shown, 0, line_count(shown.len())).join("\n");
    out.push('\n');
    if bytes.len() > limit {
        out.push_str(&format!("... {} more bytes\n", bytes.len() - limit));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_show_offset_hex_and_ascii() {
        let bytes = b"PK\x03\x04hello world, docboss!";
        assert_eq!(line_count(bytes.len()), 2);
        let first = line(bytes, 0).unwrap();
        assert!(first.starts_with("00000000  50 4b 03 04 "));
        assert!(first.ends_with("|PK..hello world,|"));
        let second = line(bytes, 1).unwrap();
        assert!(second.starts_with("00000010  "));
        assert!(second.ends_with("| docboss!|"));
        assert_eq!(line(bytes, 2), None);
        assert_eq!(lines(bytes, 1, 10).len(), 1);
        assert_eq!(line(bytes, usize::MAX), None);
    }
}

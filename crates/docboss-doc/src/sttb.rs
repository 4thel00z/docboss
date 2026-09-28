//! String tables ([MS-DOC] §2.2.4) and the font table ([MS-DOC] §2.9.82).
//! [MS-DOC] §2.2.4, §2.9.286, §2.9.82, §2.9.353.

use docboss_model::FontEntry;

use crate::bytes::{slice, u16_at, u8_at, utf16};

/// One STTB element: its string and the extra data after it.
pub struct Element<'a> {
    pub text: String,
    pub extra: &'a [u8],
}

/// Reads an STTB. Extended tables hold UTF-16 strings with 2-byte
/// lengths; others hold 1-byte lengths and 8-bit data.
pub fn read(bytes: &[u8]) -> Vec<Element<'_>> {
    let extended = u16_at(bytes, 0) == Some(0xFFFF);
    let mut at = if extended { 2 } else { 0 };
    let count = usize::from(u16_at(bytes, at).unwrap_or(0));
    let cb_extra = usize::from(u16_at(bytes, at + 2).unwrap_or(0));
    at += 4;
    let mut out = Vec::with_capacity(count.min(bytes.len()));
    for _ in 0..count {
        let (length, text) = if extended {
            let Some(cch) = u16_at(bytes, at) else {
                break;
            };
            let cch = usize::from(cch);
            (2 + cch * 2, utf16(bytes, at + 2, cch))
        } else {
            let Some(cch) = u8_at(bytes, at) else {
                break;
            };
            let cch = usize::from(cch);
            (
                1 + cch,
                slice(bytes, at + 1, cch)
                    .iter()
                    .map(|&b| docboss_cfb::codepage::cp1252_char(b))
                    .collect(),
            )
        };
        at += length;
        out.push(Element {
            text,
            extra: slice(bytes, at, cb_extra),
        });
        at += cb_extra;
        if at > bytes.len() {
            break;
        }
    }
    out
}

/// Reads SttbfFfn into font entries, in font-index order.
pub fn fonts(bytes: &[u8]) -> Vec<FontEntry> {
    let count = usize::from(u16_at(bytes, 0).unwrap_or(0));
    let mut at = 4;
    let mut out = Vec::new();
    for _ in 0..count {
        let Some(length) = u8_at(bytes, at) else {
            break;
        };
        let ffn = slice(bytes, at + 1, usize::from(length));
        at += 1 + usize::from(length);
        out.push(ffn_entry(ffn));
    }
    out
}

fn null_terminated(bytes: &[u8], at: usize) -> String {
    let units: Vec<u16> = bytes
        .get(at..)
        .unwrap_or(&[])
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .take_while(|&u| u != 0)
        .collect();
    String::from_utf16_lossy(&units)
}

/// [MS-DOC] §2.9.82 FFN.
fn ffn_entry(ffn: &[u8]) -> FontEntry {
    let ffid = u8_at(ffn, 0).unwrap_or(0);
    let alt_index = usize::from(u8_at(ffn, 4).unwrap_or(0));
    let name = null_terminated(ffn, 39);
    let alt_name = (alt_index != 0)
        .then(|| null_terminated(ffn, 39 + alt_index * 2))
        .filter(|s| !s.is_empty());
    let family = match (ffid >> 4) & 7 {
        1 => "roman",
        2 => "swiss",
        3 => "modern",
        4 => "script",
        5 => "decorative",
        _ => "auto",
    };
    let pitch = match ffid & 3 {
        1 => "fixed",
        2 => "variable",
        _ => "default",
    };
    FontEntry {
        name,
        alt_name,
        family: Some(family.to_string()),
        pitch: Some(pitch.to_string()),
        ..FontEntry::default()
    }
}

/// Author names of comments: a sequence of Xst ([MS-DOC] §2.9.353).
pub fn xst_list(bytes: &[u8]) -> Vec<String> {
    let mut at = 0;
    let mut out = Vec::new();
    while let Some(cch) = u16_at(bytes, at) {
        let cch = usize::from(cch);
        out.push(utf16(bytes, at + 2, cch));
        at += 2 + cch * 2;
    }
    out
}

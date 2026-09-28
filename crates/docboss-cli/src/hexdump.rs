//! hexyl-style hexdump: offset gutter, hex columns, ascii column and
//! byte-class coloring.

use std::fmt::Write as _;
use std::io;

/// Hexdump options: bytes per row and ANSI coloring.
pub struct HexOpts {
    pub width: usize,
    pub color: bool,
}

impl Default for HexOpts {
    fn default() -> HexOpts {
        HexOpts {
            width: 16,
            color: false,
        }
    }
}

/// Byte classes for coloring and the ascii column.
#[derive(Clone, Copy)]
enum ByteClass {
    Null,
    Printable,
    Whitespace,
    Other,
}

fn classify(b: u8) -> ByteClass {
    match b {
        0 => ByteClass::Null,
        b'\t' | b'\n' | b'\r' | 0x0B | 0x0C | b' ' => ByteClass::Whitespace,
        0x21..=0x7E => ByteClass::Printable,
        _ => ByteClass::Other,
    }
}

fn color_code(class: ByteClass) -> &'static str {
    match class {
        ByteClass::Null => "\x1b[90m",
        ByteClass::Printable => "\x1b[36m",
        ByteClass::Whitespace => "\x1b[32m",
        ByteClass::Other => "\x1b[33m",
    }
}

fn ascii_char(b: u8) -> char {
    match classify(b) {
        ByteClass::Printable => b as char,
        ByteClass::Whitespace if b == b' ' => ' ',
        ByteClass::Null | ByteClass::Whitespace | ByteClass::Other => '.',
    }
}

/// Dumps `bytes`, labeling the gutter as if the first byte sat at
/// `base_offset` in the file.
pub fn hexdump(
    w: &mut impl io::Write,
    bytes: &[u8],
    base_offset: u64,
    opts: &HexOpts,
) -> io::Result<()> {
    let width = opts.width.max(1);
    for (row_index, row) in bytes.chunks(width).enumerate() {
        let offset = base_offset + (row_index * width) as u64;
        write_row(w, row, offset, width, opts.color)?;
    }
    Ok(())
}

fn write_row(
    w: &mut impl io::Write,
    row: &[u8],
    offset: u64,
    width: usize,
    color: bool,
) -> io::Result<()> {
    const RESET: &str = "\x1b[0m";
    let mut hex = String::new();
    let mut ascii = String::new();
    for (i, &b) in row.iter().enumerate() {
        if i > 0 && i % 8 == 0 {
            hex.push(' ');
        }
        if color {
            let code = color_code(classify(b));
            let _ = write!(hex, "{code}{b:02x}{RESET} ");
            let _ = write!(ascii, "{code}{}{RESET}", ascii_char(b));
        } else {
            let _ = write!(hex, "{b:02x} ");
            ascii.push(ascii_char(b));
        }
    }
    for i in row.len()..width {
        if i > 0 && i % 8 == 0 {
            hex.push(' ');
        }
        hex.push_str("   ");
    }
    writeln!(w, "{offset:08x}  {hex} |{ascii}|")
}

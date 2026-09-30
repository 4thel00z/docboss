//! Little-endian reads that return `None` past the end of the data.

pub(crate) fn u8_at(d: &[u8], at: usize) -> Option<u8> {
    d.get(at).copied()
}

pub(crate) fn u16_at(d: &[u8], at: usize) -> Option<u16> {
    let b = d.get(at..at.checked_add(2)?)?;
    Some(u16::from_le_bytes([b[0], b[1]]))
}

pub(crate) fn i16_at(d: &[u8], at: usize) -> Option<i16> {
    u16_at(d, at).map(|v| v as i16)
}

pub(crate) fn u32_at(d: &[u8], at: usize) -> Option<u32> {
    let b = d.get(at..at.checked_add(4)?)?;
    Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

pub(crate) fn i32_at(d: &[u8], at: usize) -> Option<i32> {
    u32_at(d, at).map(|v| v as i32)
}

pub(crate) fn f32_at(d: &[u8], at: usize) -> Option<f32> {
    u32_at(d, at).map(f32::from_bits)
}

/// `len` bytes from `at`, or `None` when they run past the end.
pub(crate) fn span(d: &[u8], at: usize, len: usize) -> Option<&[u8]> {
    d.get(at..at.checked_add(len)?)
}

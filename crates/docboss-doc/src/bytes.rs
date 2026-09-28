pub fn u8_at(bytes: &[u8], at: usize) -> Option<u8> {
    bytes.get(at).copied()
}

pub fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        bytes.get(at..at.checked_add(2)?)?.try_into().ok()?,
    ))
}

pub fn i16_at(bytes: &[u8], at: usize) -> Option<i16> {
    u16_at(bytes, at).map(|v| v as i16)
}

pub fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(at..at.checked_add(4)?)?.try_into().ok()?,
    ))
}

pub fn i32_at(bytes: &[u8], at: usize) -> Option<i32> {
    u32_at(bytes, at).map(|v| v as i32)
}

/// A slice `len` bytes long at `at`, cut at the end of `bytes`.
pub fn slice(bytes: &[u8], at: usize, len: usize) -> &[u8] {
    let start = at.min(bytes.len());
    let end = at.saturating_add(len).min(bytes.len());
    &bytes[start..end]
}

/// UTF-16LE code units of `count` characters at `at`.
pub fn utf16(bytes: &[u8], at: usize, count: usize) -> String {
    docboss_cfb::codepage::utf16le(slice(bytes, at, count.saturating_mul(2)))
}

/// A PLC ([MS-DOC] §2.2.2): `n + 1` CPs followed by `n` fixed-size data
/// elements. Returns the CPs and the data slices.
pub fn plc(bytes: &[u8], element_size: usize) -> (Vec<u32>, Vec<&[u8]>) {
    if bytes.len() < 4 {
        return (Vec::new(), Vec::new());
    }
    let n = (bytes.len() - 4) / (4 + element_size);
    let cps = (0..=n).filter_map(|i| u32_at(bytes, i * 4)).collect();
    let base = (n + 1) * 4;
    let data = (0..n)
        .map(|i| slice(bytes, base + i * element_size, element_size))
        .collect();
    (cps, data)
}

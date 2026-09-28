//! Bounds-checked big-endian readers.

pub(crate) fn u8_at(d: &[u8], o: usize) -> Option<u8> {
    d.get(o).copied()
}

pub(crate) fn u16_at(d: &[u8], o: usize) -> Option<u16> {
    d.get(o..o.checked_add(2)?)
        .map(|b| u16::from_be_bytes([b[0], b[1]]))
}

pub(crate) fn i16_at(d: &[u8], o: usize) -> Option<i16> {
    u16_at(d, o).map(|v| v as i16)
}

pub(crate) fn u32_at(d: &[u8], o: usize) -> Option<u32> {
    d.get(o..o.checked_add(4)?)
        .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

pub(crate) fn f2dot14(d: &[u8], o: usize) -> f32 {
    i16_at(d, o).map_or(0.0, |v| v as f32 / 16384.0)
}

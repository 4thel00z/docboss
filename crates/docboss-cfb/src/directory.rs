use crate::{u16_at, u32_at, u64_at};

/// The object type of a directory entry ([MS-CFB] §2.6.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    Unused,
    Storage,
    Stream,
    Root,
}

/// One directory entry ([MS-CFB] §2.6.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub kind: EntryKind,
    pub left: u32,
    pub right: u32,
    pub child: u32,
    pub clsid: [u8; 16],
    pub created: u64,
    pub modified: u64,
    pub start_sector: u32,
    pub size: u64,
}

impl Entry {
    pub(crate) fn parse(raw: &[u8], major_version: u16) -> Entry {
        let name_length = (u16_at(raw, 64).unwrap_or(0) as usize).min(64);
        let units: Vec<u16> = raw[..name_length]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        let name: String = char::decode_utf16(units.into_iter().take_while(|&u| u != 0))
            .map(|r| r.unwrap_or('\u{FFFD}'))
            .collect();
        let kind = match raw[66] {
            1 => EntryKind::Storage,
            2 => EntryKind::Stream,
            5 => EntryKind::Root,
            _ => EntryKind::Unused,
        };
        let raw_size = u64_at(raw, 120).unwrap_or(0);
        let size = if major_version == 3 {
            raw_size & 0xFFFF_FFFF
        } else {
            raw_size
        };
        let mut clsid = [0u8; 16];
        clsid.copy_from_slice(&raw[80..96]);
        Entry {
            name,
            kind,
            left: u32_at(raw, 68).unwrap_or(u32::MAX),
            right: u32_at(raw, 72).unwrap_or(u32::MAX),
            child: u32_at(raw, 76).unwrap_or(u32::MAX),
            clsid,
            created: u64_at(raw, 100).unwrap_or(0),
            modified: u64_at(raw, 108).unwrap_or(0),
            start_sector: u32_at(raw, 116).unwrap_or(u32::MAX),
            size,
        }
    }
}

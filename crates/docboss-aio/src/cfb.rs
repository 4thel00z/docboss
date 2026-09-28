//! Planning range reads of a compound file ([MS-CFB]): the header, the
//! DIFAT and FAT sectors, the directory, the mini FAT and the mini stream
//! are fetched first, then only the sectors of the streams a read needs.
//! The synchronous reader then parses an image of the file holding just the
//! fetched sectors, every other sector left as zeros.

use crate::fetch::Fetcher;
use crate::{Error, Result};

const MAGIC: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
const MAXREGSECT: u32 = 0xFFFF_FFFA;
const HEADER_DIFAT: usize = 109;
const ENTRY_SIZE: usize = 128;
/// Compound files larger than this are read whole rather than imaged.
pub const MAX_IMAGE: u64 = 2 << 30;

/// The streams the Word binary and encrypted-package readers open.
pub const WANTED: [&str; 8] = [
    "WordDocument",
    "0Table",
    "1Table",
    "Data",
    "\u{5}SummaryInformation",
    "\u{5}DocumentSummaryInformation",
    "EncryptionInfo",
    "EncryptedPackage",
];

pub fn is_compound_file(head: &[u8]) -> bool {
    head.starts_with(&MAGIC)
}

fn u16_at(data: &[u8], at: usize) -> Option<u16> {
    let bytes = data.get(at..at.checked_add(2)?)?;
    Some(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn u32_at(data: &[u8], at: usize) -> Option<u32> {
    let bytes = data.get(at..at.checked_add(4)?)?;
    Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn u32s(data: &[u8]) -> impl Iterator<Item = u32> + '_ {
    data.as_chunks::<4>()
        .0
        .iter()
        .map(|c| u32::from_le_bytes(*c))
}

/// A directory entry's name, kind and extent ([MS-CFB] §2.6.1).
#[derive(Debug, Clone)]
pub struct DirEntry {
    pub name: String,
    pub is_stream: bool,
    pub start: u32,
    pub size: u64,
}

#[derive(Debug, Clone)]
pub struct CfbIndex {
    sector_size: u64,
    mini_cutoff: u64,
    fat: Vec<u32>,
    pub entries: Vec<DirEntry>,
}

impl CfbIndex {
    fn sector_range(&self, sector: u32) -> std::ops::Range<u64> {
        let start = (u64::from(sector) + 1) * self.sector_size;
        start..start + self.sector_size
    }

    /// A FAT chain ([MS-CFB] §2.3), cut at a loop or an invalid sector.
    fn chain(&self, start: u32, limit: usize) -> Vec<u32> {
        chain(&self.fat, start, limit)
    }

    /// Fetches the sectors of the named streams that live in the regular
    /// FAT; streams below the mini stream cutoff are already fetched with
    /// the mini stream.
    pub async fn fetch_streams(&self, fetcher: &Fetcher, names: &[&str]) -> Result<()> {
        let ranges: Vec<_> = self
            .entries
            .iter()
            .filter(|e| e.is_stream && e.size >= self.mini_cutoff)
            .filter(|e| names.iter().any(|n| n.eq_ignore_ascii_case(&e.name)))
            .flat_map(|e| {
                let sectors = e.size.div_ceil(self.sector_size) as usize;
                self.chain(e.start, sectors)
            })
            .map(|s| self.sector_range(s))
            .collect();
        fetcher.fetch(&ranges).await?;
        Ok(())
    }
}

fn chain(fat: &[u32], start: u32, limit: usize) -> Vec<u32> {
    let mut sectors = Vec::new();
    let mut next = start;
    while next <= MAXREGSECT && sectors.len() < limit.min(fat.len()) {
        sectors.push(next);
        next = fat.get(next as usize).copied().unwrap_or(u32::MAX);
    }
    sectors
}

async fn fetch_sectors(fetcher: &Fetcher, sector_size: u64, sectors: &[u32]) -> Result<Vec<u8>> {
    let ranges: Vec<_> = sectors
        .iter()
        .map(|&s| {
            let start = (u64::from(s) + 1) * sector_size;
            start..start + sector_size
        })
        .collect();
    Ok(fetcher.fetch(&ranges).await?.concat())
}

fn parse_entry(raw: &[u8], major: u16) -> DirEntry {
    let name_length = usize::from(u16_at(raw, 64).unwrap_or(0)).min(64);
    let units: Vec<u16> = raw[..name_length]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .collect();
    let name = char::decode_utf16(units.into_iter().take_while(|&u| u != 0))
        .map(|r| r.unwrap_or('\u{FFFD}'))
        .collect();
    let low = u64::from(u32_at(raw, 120).unwrap_or(0));
    let high = u64::from(u32_at(raw, 124).unwrap_or(0));
    DirEntry {
        name,
        is_stream: raw[66] == 2,
        start: u32_at(raw, 116).unwrap_or(u32::MAX),
        size: if major == 3 { low } else { low | high << 32 },
    }
}

/// Reads the header ([MS-CFB] §2.2), the DIFAT (§2.5), the FAT (§2.3), the
/// directory (§2.6), the mini FAT (§2.4) and the mini stream.
pub async fn index(fetcher: &Fetcher) -> Result<CfbIndex> {
    let head = fetcher.fetch_one(0..512).await?;
    if !is_compound_file(&head) || head.len() < 512 {
        return Err(Error::Container("not a compound file".into()));
    }
    let shift = u16_at(&head, 0x1E).unwrap_or(9);
    if shift != 9 && shift != 12 {
        return Err(Error::Container(format!("sector shift {shift}")));
    }
    let sector_size = 1u64 << shift;
    let major = u16_at(&head, 0x1A).unwrap_or(3);
    let field = |at: usize| u32_at(&head, at).unwrap_or(u32::MAX);
    let fat_count = field(0x2C) as usize;
    let (first_directory, mini_cutoff) = (field(0x30), u64::from(field(0x38)));
    let (first_mini_fat, first_difat, difat_count) = (field(0x3C), field(0x44), field(0x48));
    let per_sector = (sector_size / 4) as usize;
    let max_sectors = (fetcher.len() / sector_size) as usize + 1;
    let mut fat_sectors: Vec<u32> = u32s(&head[0x4C..0x4C + HEADER_DIFAT * 4]).collect();
    let mut next = first_difat;
    let mut seen = 0;
    while next <= MAXREGSECT && seen < (difat_count as usize).min(max_sectors) {
        let sector = fetch_sectors(fetcher, sector_size, &[next]).await?;
        let ids: Vec<u32> = u32s(&sector).collect();
        fat_sectors.extend(ids.iter().take(per_sector - 1));
        next = ids.last().copied().unwrap_or(u32::MAX);
        seen += 1;
    }
    fat_sectors.retain(|&s| s <= MAXREGSECT);
    fat_sectors.truncate(fat_count.min(max_sectors));
    let fat: Vec<u32> = u32s(&fetch_sectors(fetcher, sector_size, &fat_sectors).await?).collect();
    let directory_sectors = chain(&fat, first_directory, max_sectors);
    let directory = fetch_sectors(fetcher, sector_size, &directory_sectors).await?;
    let entries: Vec<DirEntry> = directory
        .as_chunks::<ENTRY_SIZE>()
        .0
        .iter()
        .map(|raw| parse_entry(raw, major))
        .collect();
    let root = entries
        .first()
        .ok_or_else(|| Error::Container("no directory entries".into()))?;
    let mut support = chain(&fat, first_mini_fat, max_sectors);
    if root.size > 0 {
        support.extend(chain(
            &fat,
            root.start,
            root.size.div_ceil(sector_size) as usize,
        ));
    }
    fetch_sectors(fetcher, sector_size, &support).await?;
    Ok(CfbIndex {
        sector_size,
        mini_cutoff,
        fat,
        entries,
    })
}

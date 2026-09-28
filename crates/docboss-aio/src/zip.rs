//! Planning range reads of a ZIP package: the end of central directory
//! record and the central directory are read from the tail, then only the
//! entries a read needs are fetched and repacked into a compact archive the
//! synchronous reader opens.

use std::ops::Range;

use docboss_zip::Entry;

use crate::fetch::Fetcher;
use crate::{Error, Result};

const EOCD_SIG: [u8; 4] = [0x50, 0x4b, 0x05, 0x06];
const ZIP64_LOCATOR_SIG: u32 = 0x0706_4b50;
const ZIP64_EOCD_SIG: u32 = 0x0606_4b50;
const LOCAL_SIG: u32 = 0x0403_4b50;
const EOCD_LEN: u64 = 22;
const MAX_COMMENT: u64 = 0xFFFF;

fn u16_at(data: &[u8], at: usize) -> Option<u16> {
    let bytes = data.get(at..at.checked_add(2)?)?;
    Some(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn u32_at(data: &[u8], at: usize) -> Option<u32> {
    let bytes = data.get(at..at.checked_add(4)?)?;
    Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn u64_at(data: &[u8], at: usize) -> Option<u64> {
    let bytes = data.get(at..at.checked_add(8)?)?;
    Some(u64::from_le_bytes(bytes.try_into().ok()?))
}

/// The central directory of a remote package and the local header offsets
/// that bound each entry's bytes.
#[derive(Debug, Clone)]
pub struct ZipIndex {
    pub entries: Vec<Entry>,
    /// Every local header offset and the directory offset, sorted: an
    /// entry's bytes run from its offset to the next one.
    bounds: Vec<u64>,
}

struct EndRecord {
    size: u64,
    offset: u64,
    eocd_at: u64,
}

/// Finds the end of central directory record in the tail (APPNOTE
/// §4.3.16) and follows the ZIP64 locator to the ZIP64 record when present
/// (APPNOTE §4.3.14, §4.3.15).
async fn end_record(fetcher: &Fetcher) -> Result<EndRecord> {
    let len = fetcher.len();
    let tail_start = len.saturating_sub(MAX_COMMENT + EOCD_LEN + 20);
    let tail = fetcher.fetch_one(tail_start..len).await?;
    let at = tail
        .windows(4)
        .enumerate()
        .rev()
        .filter(|(_, window)| *window == EOCD_SIG)
        .map(|(at, _)| at)
        .find(|&at| {
            u16_at(&tail, at + 20)
                .is_some_and(|comment| at + 22 + usize::from(comment) <= tail.len())
        })
        .ok_or_else(|| Error::Container("no end of central directory record".into()))?;
    let field32 = |off: usize| u32_at(&tail, at + off).map(u64::from);
    let mut record = EndRecord {
        size: field32(12).unwrap_or(0),
        offset: field32(16).unwrap_or(0),
        eocd_at: tail_start + at as u64,
    };
    let locator = at
        .checked_sub(20)
        .filter(|&l| u32_at(&tail, l) == Some(ZIP64_LOCATOR_SIG));
    let Some(locator) = locator else {
        return Ok(record);
    };
    let Some(zip64_at) = u64_at(&tail, locator + 8) else {
        return Ok(record);
    };
    let zip64 = fetcher.fetch_one(zip64_at..zip64_at + 56).await?;
    if u32_at(&zip64, 0) != Some(ZIP64_EOCD_SIG) {
        return Ok(record);
    }
    record.size = u64_at(&zip64, 40).unwrap_or(record.size);
    record.offset = u64_at(&zip64, 48).unwrap_or(record.offset);
    Ok(record)
}

/// Reads the end record and the central directory (APPNOTE §4.3.12). A
/// directory offset shifted by data prepended to the archive is corrected
/// from the record's position, as the synchronous reader does.
pub async fn index(fetcher: &Fetcher) -> Result<ZipIndex> {
    let record = end_record(fetcher).await?;
    let mut offset = record.offset;
    let mut directory = fetcher
        .fetch_one(offset..offset.saturating_add(record.size))
        .await?;
    let mut entries = docboss_zip::central_headers(&directory).unwrap_or_default();
    let mut shift = 0i64;
    if entries.is_empty() {
        let shifted = record.eocd_at.saturating_sub(record.size);
        directory = fetcher.fetch_one(shifted..record.eocd_at).await?;
        entries = docboss_zip::central_headers(&directory).map_err(Error::Container)?;
        shift = shifted as i64 - offset as i64;
        offset = shifted;
    }
    if entries.is_empty() {
        return Err(Error::Container(
            "the central directory lists no entries".into(),
        ));
    }
    for entry in &mut entries {
        entry.local_header_offset = (entry.local_header_offset as i64 + shift).max(0) as u64;
    }
    let mut bounds: Vec<u64> = entries.iter().map(|e| e.local_header_offset).collect();
    bounds.push(offset);
    bounds.sort_unstable();
    bounds.dedup();
    Ok(ZipIndex { entries, bounds })
}

impl ZipIndex {
    pub fn find(&self, name: &str) -> Option<&Entry> {
        let folded = name.trim_start_matches('/').to_ascii_lowercase();
        self.entries.iter().find(|e| e.name == name).or_else(|| {
            self.entries
                .iter()
                .find(|e| e.name.to_ascii_lowercase() == folded)
        })
    }

    /// The bytes of an entry's local header, data and data descriptor:
    /// from its offset to the next local header or the directory.
    pub fn region(&self, entry: &Entry) -> Range<u64> {
        let start = entry.local_header_offset;
        let at = self.bounds.partition_point(|&b| b <= start);
        let end = self
            .bounds
            .get(at)
            .copied()
            .unwrap_or(start + 30 + entry.compressed_size + 1024);
        start..end
    }
}

/// Whether a part is markup the reader always needs: XML parts and
/// relationship parts. Images, embedded fonts and OLE objects are not.
pub fn is_markup(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".xml") || lower.ends_with(".rels")
}

/// The compressed data of an entry inside its fetched region, after the
/// local header (APPNOTE §4.3.7).
fn entry_data<'a>(entry: &Entry, region: &'a [u8]) -> Result<&'a [u8]> {
    let truncated = || Error::Container(format!("zip entry {:?} is truncated", entry.name));
    if u32_at(region, 0) != Some(LOCAL_SIG) {
        return Err(truncated());
    }
    let name_len = usize::from(u16_at(region, 26).ok_or_else(truncated)?);
    let extra_len = usize::from(u16_at(region, 28).ok_or_else(truncated)?);
    let start = 30 + name_len + extra_len;
    let size = usize::try_from(entry.compressed_size).map_err(|_| truncated())?;
    region
        .get(start..start.checked_add(size).ok_or_else(truncated)?)
        .ok_or_else(truncated)
}

/// Writes the fetched entries into a compact archive with their data as it
/// was compressed, so nothing is inflated or deflated here. Every entry is
/// renamed in UTF-8 and loses its data descriptor, since the sizes are
/// known from the central directory (APPNOTE §4.3.7, §4.3.12, §4.3.16).
/// An entry without a region becomes an empty stored entry, so the reader
/// still lists the part and its data can be fetched later.
pub fn repack(parts: &[(&Entry, Option<&[u8]>)]) -> Result<Vec<u8>> {
    let too_large =
        || Error::Container("the selected parts need ZIP64; read the whole file".into());
    if parts.len() > 0xFFFF {
        return Err(too_large());
    }
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (entry, region) in parts {
        let (data, method, crc, size) = match region {
            Some(region) => (
                entry_data(entry, region)?,
                entry.method,
                entry.crc32,
                entry.uncompressed_size,
            ),
            None => (&[][..], docboss_zip::STORED, 0, 0),
        };
        let offset = u32::try_from(out.len()).map_err(|_| too_large())?;
        let compressed = u32::try_from(data.len()).map_err(|_| too_large())?;
        let uncompressed = u32::try_from(size).map_err(|_| too_large())?;
        let name = entry.name.as_bytes();
        let name_len = u16::try_from(name.len()).map_err(|_| too_large())?;
        let flags = (entry.flags & 0x0001) | 0x0800;
        let common = |buffer: &mut Vec<u8>| {
            buffer.extend_from_slice(&20u16.to_le_bytes());
            buffer.extend_from_slice(&flags.to_le_bytes());
            buffer.extend_from_slice(&method.to_le_bytes());
            buffer.extend_from_slice(&[0, 0, 0x21, 0]);
            buffer.extend_from_slice(&crc.to_le_bytes());
            buffer.extend_from_slice(&compressed.to_le_bytes());
            buffer.extend_from_slice(&uncompressed.to_le_bytes());
            buffer.extend_from_slice(&name_len.to_le_bytes());
            buffer.extend_from_slice(&0u16.to_le_bytes());
        };
        out.extend_from_slice(&LOCAL_SIG.to_le_bytes());
        common(&mut out);
        out.extend_from_slice(name);
        out.extend_from_slice(data);
        central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        common(&mut central);
        central.extend_from_slice(&[0; 10]);
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(name);
    }
    let directory_offset = u32::try_from(out.len()).map_err(|_| too_large())?;
    let directory_size = u32::try_from(central.len()).map_err(|_| too_large())?;
    out.extend_from_slice(&central);
    let count = parts.len() as u16;
    out.extend_from_slice(&EOCD_SIG);
    out.extend_from_slice(&[0, 0, 0, 0]);
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&directory_size.to_le_bytes());
    out.extend_from_slice(&directory_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    Ok(out)
}

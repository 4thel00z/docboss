//! A ZIP container reader over an in-memory byte slice, following the PKWARE
//! application note (APPNOTE 6.3.10).
//!
//! The central directory is found through the end of central directory
//! record, including its ZIP64 forms. When that record or the directory is
//! missing or damaged, entries are recovered by scanning for local file
//! headers and each recovery is reported as a [`Diagnostic`]. Stored and
//! deflated entries are read; stored ones borrow from the input.

mod cp437;
mod crc;

use std::borrow::Cow;
use std::collections::HashMap;

use docboss_model::Diagnostic;
use memchr::memmem;

pub use crc::{crc32, crc32_update};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum Error {
    #[error("not a ZIP archive: no central directory and no local file headers")]
    NotZip,
    #[error("entry {0:?} not found")]
    NotFound(String),
    #[error("entry {name:?} uses unsupported compression method {method}")]
    UnsupportedMethod { name: String, method: u16 },
    #[error("entry {0:?} is encrypted")]
    Encrypted(String),
    #[error("entry {0:?} is truncated")]
    Truncated(String),
    #[error("entry {name:?} failed to inflate: {message}")]
    Inflate { name: String, message: String },
    #[error("entry {name:?} exceeds the size limit ({limit} bytes)")]
    TooLarge { name: String, limit: u64 },
    #[error("entry {name:?} expands more than {ratio}x its compressed size")]
    Bomb { name: String, ratio: u64 },
    #[error("entry {0:?} fails its CRC-32 check")]
    CrcMismatch(String),
}

/// Bounds on what reading one entry may allocate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// The largest uncompressed entry, in bytes.
    pub max_entry_size: u64,
    /// The largest ratio of uncompressed to compressed size, enforced once
    /// an entry has grown past `ratio_floor` bytes.
    pub max_ratio: u64,
    pub ratio_floor: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_entry_size: 1 << 30,
            max_ratio: 1000,
            ratio_floor: 16 << 20,
        }
    }
}

/// The compression methods of APPNOTE §4.4.5 this reader handles.
pub const STORED: u16 = 0;
pub const DEFLATED: u16 = 8;

/// One entry of the archive as its central directory (or, after recovery,
/// its local header) describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub method: u16,
    pub flags: u16,
    pub crc32: u32,
    pub compressed_size: u64,
    pub uncompressed_size: u64,
    pub local_header_offset: u64,
    /// Whether the sizes are known; false for a recovered entry whose local
    /// header defers them to a data descriptor.
    pub sizes_known: bool,
}

impl Entry {
    pub fn is_dir(&self) -> bool {
        self.name.ends_with('/')
    }

    pub fn is_encrypted(&self) -> bool {
        self.flags & 1 != 0
    }
}

/// The bytes of an entry and whether they matched the stored CRC-32.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contents<'a> {
    pub data: Cow<'a, [u8]>,
    pub crc_ok: bool,
}

const EOCD_SIG: u32 = 0x0605_4b50;
const ZIP64_EOCD_SIG: u32 = 0x0606_4b50;
const ZIP64_LOCATOR_SIG: u32 = 0x0706_4b50;
const CENTRAL_SIG: u32 = 0x0201_4b50;
const LOCAL_SIG: u32 = 0x0403_4b50;
const DESCRIPTOR_SIG: u32 = 0x0807_4b50;
const MAX_COMMENT: usize = 0xFFFF;

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

fn decode_name(raw: &[u8], flags: u16) -> String {
    if flags & 0x0800 != 0 {
        return String::from_utf8_lossy(raw).into_owned();
    }
    if let Ok(ascii) = std::str::from_utf8(raw) {
        if raw.is_ascii() {
            return ascii.to_string();
        }
    }
    cp437::decode(raw)
}

fn normalize(name: &str) -> String {
    name.trim_start_matches('/')
        .replace('\\', "/")
        .to_ascii_lowercase()
}

/// A ZIP archive over borrowed bytes.
#[derive(Debug, Clone)]
pub struct Archive<'a> {
    data: &'a [u8],
    entries: Vec<Entry>,
    exact: HashMap<String, usize>,
    folded: HashMap<String, usize>,
    limits: Limits,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Archive<'a> {
    /// Opens an archive with the default [`Limits`].
    pub fn new(data: &'a [u8]) -> Result<Self> {
        Self::with_limits(data, Limits::default())
    }

    pub fn with_limits(data: &'a [u8], limits: Limits) -> Result<Self> {
        let mut diagnostics = Vec::new();
        let entries = match central_directory(data) {
            Ok(entries) if !entries.is_empty() => entries,
            other => {
                let reason = other
                    .err()
                    .unwrap_or_else(|| "the central directory lists no entries".into());
                let recovered = scan_local_headers(data);
                if recovered.is_empty() {
                    return Err(Error::NotZip);
                }
                diagnostics.push(Diagnostic::approximated(
                    "zip",
                    format!(
                        "{reason}; recovered {} entries from local file headers",
                        recovered.len()
                    ),
                ));
                recovered
            }
        };
        let mut exact = HashMap::with_capacity(entries.len());
        let mut folded = HashMap::with_capacity(entries.len());
        for (i, entry) in entries.iter().enumerate() {
            exact.entry(entry.name.clone()).or_insert(i);
            folded.entry(normalize(&entry.name)).or_insert(i);
        }
        Ok(Self {
            data,
            entries,
            exact,
            folded,
            limits,
            diagnostics,
        })
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|entry| entry.name.as_str())
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// What was recovered or approximated while opening the archive.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// The entry named `name`, falling back to a case-insensitive match
    /// that also ignores a leading `/` and treats `\` as `/`.
    pub fn find(&self, name: &str) -> Option<&Entry> {
        if let Some(&i) = self.exact.get(name) {
            return Some(&self.entries[i]);
        }
        self.folded.get(&normalize(name)).map(|&i| &self.entries[i])
    }

    pub fn contains(&self, name: &str) -> bool {
        self.find(name).is_some()
    }

    /// The uncompressed bytes of an entry; a CRC-32 mismatch is an error.
    pub fn entry(&self, name: &str) -> Result<Cow<'a, [u8]>> {
        let contents = self.read(name)?;
        if !contents.crc_ok {
            return Err(Error::CrcMismatch(name.to_string()));
        }
        Ok(contents.data)
    }

    /// The uncompressed bytes of an entry and whether its CRC-32 matched,
    /// for lenient callers that keep damaged data and report it.
    pub fn read(&self, name: &str) -> Result<Contents<'a>> {
        let entry = self
            .find(name)
            .ok_or_else(|| Error::NotFound(name.to_string()))?;
        self.read_entry(entry)
    }

    pub fn read_entry(&self, entry: &Entry) -> Result<Contents<'a>> {
        if entry.is_encrypted() {
            return Err(Error::Encrypted(entry.name.clone()));
        }
        let start = self.data_start(entry)?;
        let data = match entry.method {
            STORED => self.stored(entry, start)?,
            DEFLATED => Cow::Owned(self.inflate(entry, start)?),
            method => {
                return Err(Error::UnsupportedMethod {
                    name: entry.name.clone(),
                    method,
                })
            }
        };
        let crc_ok = !entry.sizes_known && entry.crc32 == 0 || crc32(&data) == entry.crc32;
        Ok(Contents { data, crc_ok })
    }

    fn data_start(&self, entry: &Entry) -> Result<usize> {
        let truncated = || Error::Truncated(entry.name.clone());
        let offset = usize::try_from(entry.local_header_offset).map_err(|_| truncated())?;
        if u32_at(self.data, offset) != Some(LOCAL_SIG) {
            return Err(truncated());
        }
        let name_len = usize::from(u16_at(self.data, offset + 26).ok_or_else(truncated)?);
        let extra_len = usize::from(u16_at(self.data, offset + 28).ok_or_else(truncated)?);
        let start = offset + 30 + name_len + extra_len;
        if start > self.data.len() {
            return Err(truncated());
        }
        Ok(start)
    }

    fn stored(&self, entry: &Entry, start: usize) -> Result<Cow<'a, [u8]>> {
        let rest = &self.data[start..];
        if !entry.sizes_known {
            let end = memmem::find(rest, &DESCRIPTOR_SIG.to_le_bytes())
                .or_else(|| memmem::find(rest, &LOCAL_SIG.to_le_bytes()))
                .unwrap_or(rest.len());
            return Ok(Cow::Borrowed(&rest[..end]));
        }
        let size = usize::try_from(entry.uncompressed_size).unwrap_or(usize::MAX);
        if size as u64 > self.limits.max_entry_size {
            return Err(Error::TooLarge {
                name: entry.name.clone(),
                limit: self.limits.max_entry_size,
            });
        }
        rest.get(..size)
            .map(Cow::Borrowed)
            .ok_or_else(|| Error::Truncated(entry.name.clone()))
    }

    fn inflate(&self, entry: &Entry, start: usize) -> Result<Vec<u8>> {
        let input = match entry.sizes_known {
            true => {
                let size = usize::try_from(entry.compressed_size).unwrap_or(usize::MAX);
                let end = start.saturating_add(size).min(self.data.len());
                &self.data[start..end]
            }
            false => &self.data[start..],
        };
        let limits = self.limits;
        let declared = entry.uncompressed_size.min(limits.max_entry_size);
        let mut out: Vec<u8> = Vec::with_capacity(declared.min(64 << 20) as usize);
        let mut inflater = flate2::Decompress::new(false);
        loop {
            if out.len() == out.capacity() {
                out.reserve(out.capacity().clamp(64 << 10, 64 << 20));
            }
            let consumed = inflater.total_in() as usize;
            let status = inflater
                .decompress_vec(
                    &input[consumed.min(input.len())..],
                    &mut out,
                    flate2::FlushDecompress::None,
                )
                .map_err(|e| Error::Inflate {
                    name: entry.name.clone(),
                    message: e.to_string(),
                })?;
            let produced = out.len() as u64;
            if produced > limits.max_entry_size {
                return Err(Error::TooLarge {
                    name: entry.name.clone(),
                    limit: limits.max_entry_size,
                });
            }
            let read = inflater.total_in().max(1);
            if produced > limits.ratio_floor && produced / read > limits.max_ratio {
                return Err(Error::Bomb {
                    name: entry.name.clone(),
                    ratio: limits.max_ratio,
                });
            }
            match status {
                flate2::Status::StreamEnd => return Ok(out),
                flate2::Status::Ok => continue,
                flate2::Status::BufError => {
                    if out.len() < out.capacity() {
                        return Err(Error::Truncated(entry.name.clone()));
                    }
                }
            }
        }
    }
}

struct EndRecord {
    entries: u64,
    size: u64,
    offset: u64,
    eocd_at: usize,
}

fn find_end_record(data: &[u8]) -> Option<EndRecord> {
    let window_start = data.len().saturating_sub(MAX_COMMENT + 22);
    let window = &data[window_start..];
    let sig = EOCD_SIG.to_le_bytes();
    let at = memmem::rfind_iter(window, &sig)
        .map(|i| i + window_start)
        .find(|&at| {
            let comment = u16_at(data, at + 20).map(usize::from);
            comment.is_some_and(|len| at + 22 + len <= data.len())
        })?;
    let mut record = EndRecord {
        entries: u64::from(u16_at(data, at + 10)?),
        size: u64::from(u32_at(data, at + 12)?),
        offset: u64::from(u32_at(data, at + 16)?),
        eocd_at: at,
    };
    let needs_zip64 =
        record.entries == 0xFFFF || record.size == 0xFFFF_FFFF || record.offset == 0xFFFF_FFFF;
    let locator = at
        .checked_sub(20)
        .filter(|&l| u32_at(data, l) == Some(ZIP64_LOCATOR_SIG));
    if let Some(locator) = locator {
        let zip64_at = u64_at(data, locator + 8).and_then(|o| usize::try_from(o).ok());
        if let Some(z) = zip64_at.filter(|&z| u32_at(data, z) == Some(ZIP64_EOCD_SIG)) {
            record.entries = u64_at(data, z + 32)?;
            record.size = u64_at(data, z + 40)?;
            record.offset = u64_at(data, z + 48)?;
        }
    } else if needs_zip64 {
        return None;
    }
    Some(record)
}

fn zip64_extra(extra: &[u8], entry: &mut Entry, raw_usize: u32, raw_csize: u32, raw_offset: u32) {
    let mut at = 0;
    while at + 4 <= extra.len() {
        let id = u16_at(extra, at).unwrap_or(0);
        let len = usize::from(u16_at(extra, at + 2).unwrap_or(0));
        let body = extra.get(at + 4..at + 4 + len).unwrap_or(&[]);
        at += 4 + len;
        if id != 0x0001 {
            continue;
        }
        let mut field = 0;
        let mut next = || {
            let value = u64_at(body, field);
            field += 8;
            value
        };
        if raw_usize == 0xFFFF_FFFF {
            entry.uncompressed_size = next().unwrap_or(entry.uncompressed_size);
        }
        if raw_csize == 0xFFFF_FFFF {
            entry.compressed_size = next().unwrap_or(entry.compressed_size);
        }
        if raw_offset == 0xFFFF_FFFF {
            entry.local_header_offset = next().unwrap_or(entry.local_header_offset);
        }
        return;
    }
}

/// Reads the central directory (APPNOTE §4.3.12) through the end of central
/// directory record (APPNOTE §4.3.16) and its ZIP64 forms (§4.3.14, §4.3.15).
fn central_directory(data: &[u8]) -> std::result::Result<Vec<Entry>, String> {
    let record = find_end_record(data).ok_or("no end of central directory record")?;
    let mut at =
        usize::try_from(record.offset).map_err(|_| "central directory offset overflows")?;
    if u32_at(data, at) != Some(CENTRAL_SIG) {
        let shifted = record.eocd_at.checked_sub(record.size as usize);
        match shifted.filter(|&s| u32_at(data, s) == Some(CENTRAL_SIG)) {
            Some(s) => at = s,
            None => return Err("the central directory offset points at no directory".into()),
        }
    }
    let base_shift = at as i64 - record.offset as i64;
    let capacity = (record.entries as usize).min(data.len() / 46 + 1);
    let mut entries = Vec::with_capacity(capacity);
    while u32_at(data, at) == Some(CENTRAL_SIG) {
        let field = |off: usize| u16_at(data, at + off);
        let (Some(flags), Some(method), Some(name_len), Some(extra_len), Some(comment_len)) =
            (field(8), field(10), field(28), field(30), field(32))
        else {
            return Err("a central directory header is truncated".into());
        };
        let (Some(crc), Some(csize), Some(usize_raw), Some(offset)) = (
            u32_at(data, at + 16),
            u32_at(data, at + 20),
            u32_at(data, at + 24),
            u32_at(data, at + 42),
        ) else {
            return Err("a central directory header is truncated".into());
        };
        let name_start = at + 46;
        let extra_start = name_start + usize::from(name_len);
        let end = extra_start + usize::from(extra_len) + usize::from(comment_len);
        let Some(raw_name) = data.get(name_start..extra_start) else {
            return Err("a central directory name is truncated".into());
        };
        let extra = data
            .get(extra_start..extra_start + usize::from(extra_len))
            .unwrap_or(&[]);
        let mut entry = Entry {
            name: decode_name(raw_name, flags),
            method,
            flags,
            crc32: crc,
            compressed_size: u64::from(csize),
            uncompressed_size: u64::from(usize_raw),
            local_header_offset: u64::from(offset),
            sizes_known: true,
        };
        zip64_extra(extra, &mut entry, usize_raw, csize, offset);
        entry.local_header_offset = (entry.local_header_offset as i64 + base_shift).max(0) as u64;
        entries.push(entry);
        at = end;
    }
    if entries.is_empty() && record.entries > 0 {
        return Err("the central directory holds no readable headers".into());
    }
    Ok(entries)
}

/// Recovers entries from local file headers (APPNOTE §4.3.7) when the
/// central directory cannot be read.
fn scan_local_headers(data: &[u8]) -> Vec<Entry> {
    let sig = LOCAL_SIG.to_le_bytes();
    let mut entries: Vec<Entry> = Vec::new();
    for at in memmem::find_iter(data, &sig) {
        let (Some(flags), Some(method), Some(name_len), Some(extra_len)) = (
            u16_at(data, at + 6),
            u16_at(data, at + 8),
            u16_at(data, at + 26),
            u16_at(data, at + 28),
        ) else {
            continue;
        };
        let (Some(crc), Some(csize), Some(usize_raw)) = (
            u32_at(data, at + 14),
            u32_at(data, at + 18),
            u32_at(data, at + 22),
        ) else {
            continue;
        };
        if method != STORED && method != DEFLATED {
            continue;
        }
        let name_start = at + 30;
        let Some(raw_name) = data.get(name_start..name_start + usize::from(name_len)) else {
            continue;
        };
        if raw_name.is_empty() {
            continue;
        }
        let deferred = flags & 0x0008 != 0;
        let mut entry = Entry {
            name: decode_name(raw_name, flags),
            method,
            flags,
            crc32: crc,
            compressed_size: u64::from(csize),
            uncompressed_size: u64::from(usize_raw),
            local_header_offset: at as u64,
            sizes_known: !deferred || csize != 0,
        };
        let extra_start = name_start + usize::from(name_len);
        let extra = data
            .get(extra_start..extra_start + usize::from(extra_len))
            .unwrap_or(&[]);
        zip64_extra(extra, &mut entry, usize_raw, csize, 0);
        if deferred && !entry.sizes_known {
            entry.crc32 = 0;
        }
        if entries.iter().any(|seen| seen.name == entry.name) {
            continue;
        }
        entries.push(entry);
    }
    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn garbage_is_not_a_zip() {
        assert_eq!(Archive::new(b"hello world").unwrap_err(), Error::NotZip);
        assert_eq!(Archive::new(b"").unwrap_err(), Error::NotZip);
    }

    #[test]
    fn name_normalization() {
        assert_eq!(normalize("/Word\\Document.XML"), "word/document.xml");
    }
}

//! Compound File Binary reader ([MS-CFB]) with OLE property sets
//! ([MS-OLEPS]).
//!
//! A compound file is a small FAT file system inside one file: fixed-size
//! sectors chained by a file allocation table, a directory of storages and
//! streams kept as red-black trees, and a mini stream for small streams.
//! [`CompoundFile::parse`] validates the header and reads the tables once;
//! [`CompoundFile::stream`] borrows a stream's bytes straight from the input
//! when its sectors are contiguous and copies them otherwise.
//!
//! The reader is lenient: a chain that runs off the end of the file, loops
//! or hits a free sector ends there, and the damage is recorded in
//! [`CompoundFile::diagnostics`].

pub mod codepage;
mod directory;
mod error;
pub mod property;
pub mod time;

use std::borrow::Cow;
use std::cell::RefCell;

use docboss_model::Diagnostic;

pub use directory::{Entry, EntryKind};
pub use error::{Error, Result};

/// The eight-byte signature every compound file starts with ([MS-CFB] §2.2).
pub const SIGNATURE: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];

const MAXREGSECT: u32 = 0xFFFF_FFFA;
const ENDOFCHAIN: u32 = 0xFFFF_FFFE;
const FREESECT: u32 = 0xFFFF_FFFF;
const NOSTREAM: u32 = 0xFFFF_FFFF;
const HEADER_DIFAT_ENTRIES: usize = 109;
const MINI_SECTOR_SIZE: usize = 64;
const DIRECTORY_ENTRY_SIZE: usize = 128;

/// Whether `bytes` starts with the compound file signature.
pub fn is_compound_file(bytes: &[u8]) -> bool {
    bytes.starts_with(&SIGNATURE)
}

fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

fn u64_at(bytes: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(bytes.get(at..at + 8)?.try_into().ok()?))
}

/// A parsed compound file borrowing its input.
pub struct CompoundFile<'a> {
    data: &'a [u8],
    major_version: u16,
    sector_shift: u16,
    mini_stream_cutoff: u32,
    fat: Vec<u32>,
    mini_fat: Vec<u32>,
    entries: Vec<Entry>,
    mini_stream: Vec<u8>,
    diagnostics: RefCell<Vec<Diagnostic>>,
}

impl<'a> CompoundFile<'a> {
    /// Reads the header, the FAT, the mini FAT and the directory
    /// ([MS-CFB] §2.2 to §2.6).
    pub fn parse(data: &'a [u8]) -> Result<Self> {
        if !is_compound_file(data) {
            return Err(Error::NotCompoundFile);
        }
        if data.len() < 512 {
            return Err(Error::Truncated("header"));
        }
        let mut diagnostics = Vec::new();
        let major_version = u16_at(data, 0x1A).unwrap_or(3);
        let sector_shift = u16_at(data, 0x1E).unwrap_or(9);
        if sector_shift != 9 && sector_shift != 12 {
            return Err(Error::SectorShift(sector_shift));
        }
        let expected_shift = if major_version == 4 { 12 } else { 9 };
        if sector_shift != expected_shift {
            diagnostics.push(Diagnostic::approximated(
                "header",
                format!("major version {major_version} with sector shift {sector_shift}; using the shift"),
            ));
        }
        if u16_at(data, 0x1C) != Some(0xFFFE) {
            diagnostics.push(Diagnostic::approximated(
                "header",
                "byte order mark is not 0xFFFE",
            ));
        }
        let mini_shift = u16_at(data, 0x20).unwrap_or(6);
        if mini_shift != 6 {
            diagnostics.push(Diagnostic::approximated(
                "header",
                format!("mini sector shift {mini_shift}; using 6"),
            ));
        }
        let mut file = CompoundFile {
            data,
            major_version,
            sector_shift,
            mini_stream_cutoff: u32_at(data, 0x38).unwrap_or(4096),
            fat: Vec::new(),
            mini_fat: Vec::new(),
            entries: Vec::new(),
            mini_stream: Vec::new(),
            diagnostics: RefCell::new(diagnostics),
        };
        if file.mini_stream_cutoff != 4096 {
            file.report(Diagnostic::approximated(
                "header",
                format!(
                    "mini stream cutoff {} instead of 4096",
                    file.mini_stream_cutoff
                ),
            ));
        }
        file.fat = file.read_fat();
        let first_mini_fat = u32_at(data, 0x3C).unwrap_or(ENDOFCHAIN);
        let mini_fat_bytes = file.chain_bytes(first_mini_fat, None, "mini FAT");
        file.mini_fat = mini_fat_bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();
        let first_directory = u32_at(data, 0x30).unwrap_or(ENDOFCHAIN);
        let directory = file.chain_bytes(first_directory, None, "directory");
        file.entries = directory
            .as_chunks::<DIRECTORY_ENTRY_SIZE>()
            .0
            .iter()
            .map(|raw| Entry::parse(raw, major_version))
            .collect();
        let Some(root) = file.entries.first() else {
            return Err(Error::NoDirectory);
        };
        if root.kind != EntryKind::Root {
            file.report(Diagnostic::approximated(
                "directory",
                "entry 0 is not the root entry",
            ));
        }
        let (root_start, root_size) = (root.start_sector, root.size);
        file.mini_stream = file
            .chain_bytes(root_start, Some(root_size), "mini stream")
            .into_owned();
        Ok(file)
    }

    fn report(&self, diagnostic: Diagnostic) {
        self.diagnostics.borrow_mut().push(diagnostic);
    }

    /// Everything the reader approximated or dropped so far.
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        self.diagnostics.borrow().clone()
    }

    pub fn major_version(&self) -> u16 {
        self.major_version
    }

    pub fn sector_size(&self) -> usize {
        1 << self.sector_shift
    }

    fn sector(&self, index: u32) -> Option<&'a [u8]> {
        if index > MAXREGSECT {
            return None;
        }
        let size = self.sector_size();
        let start = (index as usize).checked_add(1)?.checked_mul(size)?;
        let data = self.data;
        let end = (start + size).min(data.len());
        if start >= data.len() {
            return None;
        }
        Some(&data[start..end])
    }

    /// The FAT sector list: the 109 header DIFAT entries, then the DIFAT
    /// chain ([MS-CFB] §2.5), then the FAT sectors themselves (§2.3).
    fn read_fat(&self) -> Vec<u32> {
        let data = self.data;
        let fat_sector_count = u32_at(data, 0x2C).unwrap_or(0) as usize;
        let mut fat_sectors: Vec<u32> = (0..HEADER_DIFAT_ENTRIES)
            .filter_map(|i| u32_at(data, 0x4C + i * 4))
            .filter(|&s| s <= MAXREGSECT)
            .collect();
        let per_difat = self.sector_size() / 4 - 1;
        let mut next = u32_at(data, 0x44).unwrap_or(ENDOFCHAIN);
        let mut seen = 0usize;
        let limit = data.len() / self.sector_size() + 1;
        while next <= MAXREGSECT {
            seen += 1;
            if seen > limit {
                self.report(Diagnostic::dropped("DIFAT", "DIFAT chain loops; cut"));
                break;
            }
            let Some(sector) = self.sector(next) else {
                self.report(Diagnostic::dropped(
                    "DIFAT",
                    format!("DIFAT sector {next} lies past the end of the file"),
                ));
                break;
            };
            fat_sectors.extend(
                (0..per_difat)
                    .filter_map(|i| u32_at(sector, i * 4))
                    .filter(|&s| s <= MAXREGSECT),
            );
            next = u32_at(sector, per_difat * 4).unwrap_or(ENDOFCHAIN);
        }
        if fat_sector_count != 0 && fat_sectors.len() > fat_sector_count {
            fat_sectors.truncate(fat_sector_count);
        }
        if fat_sectors.len() < fat_sector_count {
            self.report(Diagnostic::dropped(
                "FAT",
                format!(
                    "header declares {fat_sector_count} FAT sectors, {} found",
                    fat_sectors.len()
                ),
            ));
        }
        let mut fat = Vec::with_capacity(fat_sectors.len() * self.sector_size() / 4);
        for index in fat_sectors {
            let Some(sector) = self.sector(index) else {
                self.report(Diagnostic::dropped(
                    "FAT",
                    format!("FAT sector {index} lies past the end of the file"),
                ));
                continue;
            };
            fat.extend(
                sector
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])),
            );
        }
        fat
    }

    /// The sector indices of a FAT chain, cut at a loop, a free or special
    /// value, or the end of the table.
    fn chain(&self, start: u32, what: &str) -> Vec<u32> {
        let mut chain = Vec::new();
        let mut visited = vec![false; self.fat.len()];
        let mut next = start;
        while next != ENDOFCHAIN {
            let index = next as usize;
            if next > MAXREGSECT || index >= self.fat.len() {
                if next != FREESECT || !chain.is_empty() {
                    self.report(Diagnostic::dropped(
                        what.to_string(),
                        format!("chain hits invalid sector {next:#x}; cut"),
                    ));
                }
                break;
            }
            if visited[index] {
                self.report(Diagnostic::dropped(
                    what.to_string(),
                    "sector chain loops; cut",
                ));
                break;
            }
            visited[index] = true;
            chain.push(next);
            next = self.fat[index];
        }
        chain
    }

    /// The bytes of a FAT chain, borrowed when its sectors are consecutive,
    /// cut to `size` when given.
    fn chain_bytes(&self, start: u32, size: Option<u64>, what: &str) -> Cow<'a, [u8]> {
        if start == ENDOFCHAIN || start == FREESECT {
            return Cow::Borrowed(&[]);
        }
        let chain = self.chain(start, what);
        let wanted = size.map(|s| s as usize);
        let sector_size = self.sector_size();
        let contiguous = chain.windows(2).all(|w| w[1] == w[0] + 1);
        let available = chain.len() * sector_size;
        let length = wanted.unwrap_or(available);
        if length > available {
            self.report(Diagnostic::dropped(
                what.to_string(),
                format!("stream declares {length} bytes, its chain holds {available}"),
            ));
        }
        if let (true, Some(&first)) = (contiguous, chain.first()) {
            let begin = (first as usize + 1) * sector_size;
            let end = begin
                .saturating_add(length.min(available))
                .min(self.data.len());
            if begin <= end {
                return Cow::Borrowed(&self.data[begin..end]);
            }
        }
        let mut out = Vec::with_capacity(length.min(available));
        for index in chain {
            let Some(sector) = self.sector(index) else {
                self.report(Diagnostic::dropped(
                    what.to_string(),
                    format!("sector {index} lies past the end of the file"),
                ));
                break;
            };
            out.extend_from_slice(sector);
            if out.len() >= length {
                break;
            }
        }
        out.truncate(length);
        Cow::Owned(out)
    }

    /// The bytes of a mini FAT chain from the mini stream ([MS-CFB] §2.4).
    fn mini_chain_bytes(&self, start: u32, size: u64, what: &str) -> Vec<u8> {
        let length = size as usize;
        let mut out = Vec::with_capacity(length.min(self.mini_stream.len()));
        let mut visited = vec![false; self.mini_fat.len()];
        let mut next = start;
        while next != ENDOFCHAIN && out.len() < length {
            let index = next as usize;
            if index >= self.mini_fat.len() || visited[index] {
                self.report(Diagnostic::dropped(
                    what.to_string(),
                    format!("mini chain broken at {next:#x}; cut"),
                ));
                break;
            }
            visited[index] = true;
            let begin = index * MINI_SECTOR_SIZE;
            let Some(chunk) = self
                .mini_stream
                .get(begin..(begin + MINI_SECTOR_SIZE).min(self.mini_stream.len()))
            else {
                self.report(Diagnostic::dropped(
                    what.to_string(),
                    "mini sector past the end of the mini stream",
                ));
                break;
            };
            out.extend_from_slice(chunk);
            next = self.mini_fat[index];
        }
        if out.len() < length {
            self.report(Diagnostic::dropped(
                what.to_string(),
                format!("stream declares {length} bytes, read {}", out.len()),
            ));
        }
        out.truncate(length);
        out
    }

    /// Every directory entry, in directory order. Entry 0 is the root.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn root(&self) -> &Entry {
        &self.entries[0]
    }

    /// The indices of a storage's children, in tree order ([MS-CFB] §2.6.4).
    pub fn children(&self, storage: usize) -> Vec<usize> {
        let Some(entry) = self.entries.get(storage) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let mut stack = Vec::new();
        let mut visited = vec![false; self.entries.len()];
        let mut current = entry.child;
        loop {
            while current != NOSTREAM {
                let index = current as usize;
                if index >= self.entries.len() || visited[index] {
                    break;
                }
                visited[index] = true;
                stack.push(index);
                current = self.entries[index].left;
            }
            let Some(index) = stack.pop() else {
                break;
            };
            out.push(index);
            current = self.entries[index].right;
        }
        out
    }

    /// Finds a child of a storage by name; names compare case-insensitively
    /// as [MS-CFB] §2.6.4 orders them.
    pub fn child(&self, storage: usize, name: &str) -> Option<usize> {
        self.children(storage)
            .into_iter()
            .find(|&i| self.entries[i].name.to_uppercase() == name.to_uppercase())
    }

    /// Resolves a `/`-separated path from the root, such as `ObjectPool/_1/\u{1}Ole`.
    pub fn find(&self, path: &str) -> Option<usize> {
        path.split('/')
            .filter(|part| !part.is_empty())
            .try_fold(0, |storage, part| self.child(storage, part))
    }

    /// The bytes of the stream at `path`.
    pub fn open_stream(&self, path: &str) -> Result<Cow<'a, [u8]>> {
        let index = self
            .find(path)
            .ok_or_else(|| Error::NotFound(path.to_string()))?;
        if self.entries[index].kind != EntryKind::Stream {
            return Err(Error::NotAStream(path.to_string()));
        }
        Ok(self.stream(index))
    }

    /// The bytes of the stream entry at `index`; empty for storages.
    pub fn stream(&self, index: usize) -> Cow<'a, [u8]> {
        let Some(entry) = self.entries.get(index) else {
            return Cow::Borrowed(&[]);
        };
        if entry.kind != EntryKind::Stream {
            return Cow::Borrowed(&[]);
        }
        if entry.size < u64::from(self.mini_stream_cutoff) {
            return Cow::Owned(self.mini_chain_bytes(entry.start_sector, entry.size, &entry.name));
        }
        self.chain_bytes(entry.start_sector, Some(entry.size), &entry.name)
    }

    /// Every stream path in the file, depth first, with its size.
    /// Each entry is visited once, so a directory whose trees point back at
    /// an ancestor still yields a finite list.
    pub fn walk(&self) -> Vec<(String, u64)> {
        let mut out = Vec::new();
        let mut visited = vec![false; self.entries.len()];
        visited[0] = true;
        self.walk_into(0, "", &mut out, &mut visited);
        out
    }

    fn walk_into(
        &self,
        storage: usize,
        prefix: &str,
        out: &mut Vec<(String, u64)>,
        visited: &mut [bool],
    ) {
        for index in self.children(storage) {
            if visited[index] {
                continue;
            }
            visited[index] = true;
            let entry = &self.entries[index];
            let path = format!("{prefix}{}", entry.name);
            match entry.kind {
                EntryKind::Stream => out.push((path, entry.size)),
                EntryKind::Storage => self.walk_into(index, &format!("{path}/"), out, visited),
                _ => {}
            }
        }
    }
}

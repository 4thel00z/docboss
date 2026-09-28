use std::io::Write;

use flate2::write::DeflateEncoder;
use flate2::{Compression, Crc};

/// How the writer lays out each entry.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ZipOptions {
    /// Write sizes and CRC in a trailing data descriptor instead of the
    /// local header.
    pub data_descriptor: bool,
    /// Write ZIP64 extra fields and ZIP64 end records even for small files.
    pub zip64: bool,
    /// Mark names as UTF-8 (general purpose bit 11).
    pub utf8: bool,
}

struct Written {
    name: Vec<u8>,
    method: u16,
    flags: u16,
    crc: u32,
    compressed: u64,
    uncompressed: u64,
    offset: u64,
}

/// Writes a ZIP archive into memory.
pub struct ZipWriter {
    out: Vec<u8>,
    entries: Vec<Written>,
    options: ZipOptions,
}

impl Default for ZipWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl ZipWriter {
    pub fn new() -> Self {
        Self::with_options(ZipOptions::default())
    }

    pub fn with_options(options: ZipOptions) -> Self {
        Self {
            out: Vec::new(),
            entries: Vec::new(),
            options,
        }
    }

    pub fn stored(&mut self, name: &str, data: &[u8]) -> &mut Self {
        self.add(name.as_bytes(), data, 0)
    }

    pub fn deflated(&mut self, name: &str, data: &[u8]) -> &mut Self {
        self.add(name.as_bytes(), data, 8)
    }

    /// Adds an entry whose name is given as raw bytes, for testing name
    /// encodings.
    pub fn raw_name(&mut self, name: &[u8], data: &[u8]) -> &mut Self {
        self.add(name, data, 0)
    }

    fn add(&mut self, name: &[u8], data: &[u8], method: u16) -> &mut Self {
        let mut crc = Crc::new();
        crc.update(data);
        let body = match method {
            8 => {
                let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
                encoder.write_all(data).expect("in-memory write");
                encoder.finish().expect("in-memory write")
            }
            _ => data.to_vec(),
        };
        let mut flags = 0u16;
        if self.options.data_descriptor {
            flags |= 0x0008;
        }
        if self.options.utf8 {
            flags |= 0x0800;
        }
        let entry = Written {
            name: name.to_vec(),
            method,
            flags,
            crc: crc.sum(),
            compressed: body.len() as u64,
            uncompressed: data.len() as u64,
            offset: self.out.len() as u64,
        };
        let deferred = self.options.data_descriptor;
        let zip64 = self.options.zip64;
        let out = &mut self.out;
        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        out.extend_from_slice(&(if zip64 { 45u16 } else { 20u16 }).to_le_bytes());
        out.extend_from_slice(&flags.to_le_bytes());
        out.extend_from_slice(&method.to_le_bytes());
        out.extend_from_slice(&[0, 0, 0x21, 0]);
        let (crc, csize, usize_) = match (deferred, zip64) {
            (true, _) => (0, 0, 0),
            (false, true) => (entry.crc, u32::MAX, u32::MAX),
            (false, false) => (
                entry.crc,
                entry.compressed as u32,
                entry.uncompressed as u32,
            ),
        };
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&csize.to_le_bytes());
        out.extend_from_slice(&usize_.to_le_bytes());
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        let extra = if zip64 && !deferred {
            let mut extra = Vec::new();
            extra.extend_from_slice(&1u16.to_le_bytes());
            extra.extend_from_slice(&16u16.to_le_bytes());
            extra.extend_from_slice(&entry.uncompressed.to_le_bytes());
            extra.extend_from_slice(&entry.compressed.to_le_bytes());
            extra
        } else {
            Vec::new()
        };
        out.extend_from_slice(&(extra.len() as u16).to_le_bytes());
        out.extend_from_slice(name);
        out.extend_from_slice(&extra);
        out.extend_from_slice(&body);
        if deferred {
            out.extend_from_slice(&0x0807_4b50u32.to_le_bytes());
            out.extend_from_slice(&entry.crc.to_le_bytes());
            out.extend_from_slice(&(entry.compressed as u32).to_le_bytes());
            out.extend_from_slice(&(entry.uncompressed as u32).to_le_bytes());
        }
        self.entries.push(entry);
        self
    }

    /// The archive bytes: entries, central directory and end records.
    pub fn finish(&mut self) -> Vec<u8> {
        let mut out = std::mem::take(&mut self.out);
        let directory_start = out.len() as u64;
        let zip64 = self.options.zip64;
        for entry in &self.entries {
            out.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
            out.extend_from_slice(&(if zip64 { 45u16 } else { 20u16 }).to_le_bytes());
            out.extend_from_slice(&(if zip64 { 45u16 } else { 20u16 }).to_le_bytes());
            out.extend_from_slice(&entry.flags.to_le_bytes());
            out.extend_from_slice(&entry.method.to_le_bytes());
            out.extend_from_slice(&[0, 0, 0x21, 0]);
            out.extend_from_slice(&entry.crc.to_le_bytes());
            let (csize, usize_, offset) = match zip64 {
                true => (u32::MAX, u32::MAX, u32::MAX),
                false => (
                    entry.compressed as u32,
                    entry.uncompressed as u32,
                    entry.offset as u32,
                ),
            };
            out.extend_from_slice(&csize.to_le_bytes());
            out.extend_from_slice(&usize_.to_le_bytes());
            out.extend_from_slice(&(entry.name.len() as u16).to_le_bytes());
            let mut extra = Vec::new();
            if zip64 {
                extra.extend_from_slice(&1u16.to_le_bytes());
                extra.extend_from_slice(&24u16.to_le_bytes());
                extra.extend_from_slice(&entry.uncompressed.to_le_bytes());
                extra.extend_from_slice(&entry.compressed.to_le_bytes());
                extra.extend_from_slice(&entry.offset.to_le_bytes());
            }
            out.extend_from_slice(&(extra.len() as u16).to_le_bytes());
            out.extend_from_slice(&[0; 10]);
            out.extend_from_slice(&offset.to_le_bytes());
            out.extend_from_slice(&entry.name);
            out.extend_from_slice(&extra);
        }
        let directory_size = out.len() as u64 - directory_start;
        let count = self.entries.len() as u64;
        if zip64 {
            let record_at = out.len() as u64;
            out.extend_from_slice(&0x0606_4b50u32.to_le_bytes());
            out.extend_from_slice(&44u64.to_le_bytes());
            out.extend_from_slice(&45u16.to_le_bytes());
            out.extend_from_slice(&45u16.to_le_bytes());
            out.extend_from_slice(&[0; 8]);
            out.extend_from_slice(&count.to_le_bytes());
            out.extend_from_slice(&count.to_le_bytes());
            out.extend_from_slice(&directory_size.to_le_bytes());
            out.extend_from_slice(&directory_start.to_le_bytes());
            out.extend_from_slice(&0x0706_4b50u32.to_le_bytes());
            out.extend_from_slice(&0u32.to_le_bytes());
            out.extend_from_slice(&record_at.to_le_bytes());
            out.extend_from_slice(&1u32.to_le_bytes());
        }
        out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        out.extend_from_slice(&[0; 4]);
        let (short_count, short_size, short_start) = match zip64 {
            true => (u16::MAX, u32::MAX, u32::MAX),
            false => (count as u16, directory_size as u32, directory_start as u32),
        };
        out.extend_from_slice(&short_count.to_le_bytes());
        out.extend_from_slice(&short_count.to_le_bytes());
        out.extend_from_slice(&short_size.to_le_bytes());
        out.extend_from_slice(&short_start.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }
}

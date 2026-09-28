//! A deterministic ZIP writer: entries in insertion order, a fixed
//! timestamp of 1980-01-01 00:00, stored or deflated, with optional data
//! descriptors and ZIP64 records. The same input always yields the same
//! bytes.

use std::io::Write;

use flate2::write::DeflateEncoder;
use flate2::Compression;

use crate::{crc32, Error, Result, DEFLATED, STORED};

const LOCAL_SIGNATURE: u32 = 0x0403_4b50;
const DESCRIPTOR_SIGNATURE: u32 = 0x0807_4b50;
const CENTRAL_SIGNATURE: u32 = 0x0201_4b50;
const ZIP64_END_SIGNATURE: u32 = 0x0606_4b50;
const ZIP64_LOCATOR_SIGNATURE: u32 = 0x0706_4b50;
const END_SIGNATURE: u32 = 0x0605_4b50;
const DOS_TIME_MIDNIGHT: u16 = 0;
const DOS_DATE_1980_01_01: u16 = 0x0021;
const VERSION_DEFAULT: u16 = 20;
const VERSION_ZIP64: u16 = 45;
const FLAG_DATA_DESCRIPTOR: u16 = 0x0008;
const FLAG_UTF8: u16 = 0x0800;
const ZIP64_EXTRA_ID: u16 = 0x0001;

/// Whether an entry is compressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Stored,
    Deflated,
}

/// How the writer lays out each entry.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WriteOptions {
    /// Write the CRC and sizes in a data descriptor after the entry data
    /// instead of in the local header.
    pub data_descriptor: bool,
    /// Write ZIP64 extra fields and end records for every archive.
    pub zip64: bool,
    /// Mark entry names as UTF-8 (general purpose bit 11).
    pub utf8: bool,
}

struct Written {
    name: Vec<u8>,
    method: u16,
    flags: u16,
    crc: u32,
    compressed: u64,
    size: u64,
    offset: u64,
}

/// Builds a ZIP archive in memory.
#[derive(Default)]
pub struct ZipWriter {
    out: Vec<u8>,
    entries: Vec<Written>,
    options: WriteOptions,
}

impl ZipWriter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_options(options: WriteOptions) -> Self {
        Self {
            out: Vec::new(),
            entries: Vec::new(),
            options,
        }
    }

    /// Appends one entry.
    pub fn add(&mut self, name: &str, data: &[u8], method: Method) -> Result<()> {
        self.add_raw_name(name.as_bytes(), data, method)
    }

    /// Appends one entry whose name is given as raw bytes.
    pub fn add_raw_name(&mut self, name: &[u8], data: &[u8], method: Method) -> Result<()> {
        let (method, body) = match method {
            Method::Stored => (STORED, std::borrow::Cow::Borrowed(data)),
            Method::Deflated => (DEFLATED, std::borrow::Cow::Owned(deflate(data)?)),
        };
        let name_length =
            u16::try_from(name.len()).map_err(|_| Error::NeedsZip64("an entry name"))?;
        let flags = self.flags();
        let entry = Written {
            name: name.to_vec(),
            method,
            flags,
            crc: crc32(data),
            compressed: body.len() as u64,
            size: data.len() as u64,
            offset: self.out.len() as u64,
        };
        let zip64 = self.options.zip64;
        let deferred = self.options.data_descriptor;
        if !zip64
            && [entry.compressed, entry.size, entry.offset]
                .iter()
                .any(|&v| v > u64::from(u32::MAX - 1))
        {
            return Err(Error::NeedsZip64("an entry size or offset"));
        }
        let extra = match zip64 && !deferred {
            true => zip64_extra(&[entry.size, entry.compressed]),
            false => Vec::new(),
        };
        let (crc, compressed, size) = match (deferred, zip64) {
            (true, _) => (0, 0, 0),
            (false, true) => (entry.crc, u32::MAX, u32::MAX),
            (false, false) => (entry.crc, entry.compressed as u32, entry.size as u32),
        };
        let out = &mut self.out;
        put32(out, LOCAL_SIGNATURE);
        put16(out, version(zip64));
        put16(out, flags);
        put16(out, method);
        put16(out, DOS_TIME_MIDNIGHT);
        put16(out, DOS_DATE_1980_01_01);
        put32(out, crc);
        put32(out, compressed);
        put32(out, size);
        put16(out, name_length);
        put16(out, extra.len() as u16);
        out.extend_from_slice(name);
        out.extend_from_slice(&extra);
        out.extend_from_slice(&body);
        if deferred {
            put32(out, DESCRIPTOR_SIGNATURE);
            put32(out, entry.crc);
            put32(out, entry.compressed as u32);
            put32(out, entry.size as u32);
        }
        self.entries.push(entry);
        Ok(())
    }

    fn flags(&self) -> u16 {
        let descriptor = if self.options.data_descriptor {
            FLAG_DATA_DESCRIPTOR
        } else {
            0
        };
        let utf8 = if self.options.utf8 { FLAG_UTF8 } else { 0 };
        descriptor | utf8
    }

    /// Writes the central directory and the end records and returns the
    /// archive bytes.
    pub fn finish(self) -> Result<Vec<u8>> {
        let Self {
            mut out,
            entries,
            options,
        } = self;
        let zip64 = options.zip64;
        let directory_start = out.len() as u64;
        for entry in &entries {
            let extra = match zip64 {
                true => zip64_extra(&[entry.size, entry.compressed, entry.offset]),
                false => Vec::new(),
            };
            let (compressed, size, offset) = match zip64 {
                true => (u32::MAX, u32::MAX, u32::MAX),
                false => (
                    entry.compressed as u32,
                    entry.size as u32,
                    entry.offset as u32,
                ),
            };
            put32(&mut out, CENTRAL_SIGNATURE);
            put16(&mut out, version(zip64));
            put16(&mut out, version(zip64));
            put16(&mut out, entry.flags);
            put16(&mut out, entry.method);
            put16(&mut out, DOS_TIME_MIDNIGHT);
            put16(&mut out, DOS_DATE_1980_01_01);
            put32(&mut out, entry.crc);
            put32(&mut out, compressed);
            put32(&mut out, size);
            put16(&mut out, entry.name.len() as u16);
            put16(&mut out, extra.len() as u16);
            put16(&mut out, 0);
            put16(&mut out, 0);
            put16(&mut out, 0);
            put32(&mut out, 0);
            put32(&mut out, offset);
            out.extend_from_slice(&entry.name);
            out.extend_from_slice(&extra);
        }
        let directory_size = out.len() as u64 - directory_start;
        let count = entries.len() as u64;
        if !zip64 && (count > u64::from(u16::MAX - 1) || out.len() as u64 > u64::from(u32::MAX - 1))
        {
            return Err(Error::NeedsZip64("the entry count or directory offset"));
        }
        if zip64 {
            let record_at = out.len() as u64;
            put32(&mut out, ZIP64_END_SIGNATURE);
            put64(&mut out, 44);
            put16(&mut out, VERSION_ZIP64);
            put16(&mut out, VERSION_ZIP64);
            put32(&mut out, 0);
            put32(&mut out, 0);
            put64(&mut out, count);
            put64(&mut out, count);
            put64(&mut out, directory_size);
            put64(&mut out, directory_start);
            put32(&mut out, ZIP64_LOCATOR_SIGNATURE);
            put32(&mut out, 0);
            put64(&mut out, record_at);
            put32(&mut out, 1);
        }
        let (short_count, short_size, short_start) = match zip64 {
            true => (u16::MAX, u32::MAX, u32::MAX),
            false => (count as u16, directory_size as u32, directory_start as u32),
        };
        put32(&mut out, END_SIGNATURE);
        put16(&mut out, 0);
        put16(&mut out, 0);
        put16(&mut out, short_count);
        put16(&mut out, short_count);
        put32(&mut out, short_size);
        put32(&mut out, short_start);
        put16(&mut out, 0);
        Ok(out)
    }
}

fn version(zip64: bool) -> u16 {
    if zip64 {
        return VERSION_ZIP64;
    }
    VERSION_DEFAULT
}

/// The ZIP64 extended information extra field of APPNOTE §4.5.3.
fn zip64_extra(values: &[u64]) -> Vec<u8> {
    let mut extra = Vec::with_capacity(4 + 8 * values.len());
    put16(&mut extra, ZIP64_EXTRA_ID);
    put16(&mut extra, (8 * values.len()) as u16);
    values.iter().for_each(|&value| put64(&mut extra, value));
    extra
}

fn deflate(data: &[u8]) -> Result<Vec<u8>> {
    let mut encoder = DeflateEncoder::new(
        Vec::with_capacity(data.len() / 3 + 64),
        Compression::default(),
    );
    encoder
        .write_all(data)
        .map_err(|e| Error::Deflate(e.to_string()))?;
    encoder.finish().map_err(|e| Error::Deflate(e.to_string()))
}

fn put16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Archive;

    fn archive(options: WriteOptions) -> Vec<u8> {
        let mut zip = ZipWriter::with_options(options);
        zip.add("a.txt", b"hello", Method::Stored).unwrap();
        zip.add("b/c.xml", &b"<x/>".repeat(500), Method::Deflated)
            .unwrap();
        zip.finish().unwrap()
    }

    /// APPNOTE §4.3.16: the archive ends with the end of central directory
    /// record naming the entry count and the directory's offset (§4.3.12).
    #[test]
    fn archive_ends_with_directory_record() {
        let bytes = archive(WriteOptions::default());
        let end = &bytes[bytes.len() - 22..];
        assert_eq!(&end[..4], &END_SIGNATURE.to_le_bytes());
        assert_eq!(u16::from_le_bytes([end[10], end[11]]), 2);
        let offset = u32::from_le_bytes([end[16], end[17], end[18], end[19]]) as usize;
        assert_eq!(&bytes[offset..offset + 4], &CENTRAL_SIGNATURE.to_le_bytes());
    }

    /// APPNOTE §4.3.7, §4.3.9 and §4.3.14: every layout the writer offers
    /// reads back through the reader with the same entries and contents.
    #[test]
    fn every_layout_reads_back() {
        let layouts = [
            WriteOptions::default(),
            WriteOptions {
                data_descriptor: true,
                ..WriteOptions::default()
            },
            WriteOptions {
                zip64: true,
                ..WriteOptions::default()
            },
            WriteOptions {
                zip64: true,
                data_descriptor: true,
                utf8: true,
            },
        ];
        for options in layouts {
            let bytes = archive(options);
            let read = Archive::new(&bytes).unwrap();
            assert_eq!(
                read.entry("a.txt").unwrap().as_ref(),
                b"hello",
                "{options:?}"
            );
            assert_eq!(read.entry("b/c.xml").unwrap().len(), 2000, "{options:?}");
            assert!(read.diagnostics().is_empty(), "{options:?}");
        }
    }

    #[test]
    fn output_is_deterministic() {
        assert_eq!(
            archive(WriteOptions::default()),
            archive(WriteOptions::default())
        );
    }
}

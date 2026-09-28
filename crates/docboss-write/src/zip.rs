//! A deterministic ZIP writer: fixed timestamps, entries in insertion
//! order, stored or deflated (APPNOTE §4.3.7 local headers, §4.3.12 central
//! directory, §4.3.16 end of central directory).

use std::io::Write;

use flate2::write::DeflateEncoder;
use flate2::Compression;

use crate::{Error, Result};

const LOCAL_SIGNATURE: u32 = 0x0403_4b50;
const CENTRAL_SIGNATURE: u32 = 0x0201_4b50;
const END_SIGNATURE: u32 = 0x0605_4b50;
const DOS_DATE_1980_01_01: u16 = 0x0021;
const METHOD_STORED: u16 = 0;
const METHOD_DEFLATED: u16 = 8;
const VERSION_NEEDED: u16 = 20;

/// Whether an entry is compressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Stored,
    Deflated,
}

struct CentralEntry {
    name: Vec<u8>,
    method: u16,
    crc: u32,
    compressed: u32,
    size: u32,
    offset: u32,
}

/// Builds a ZIP archive in memory.
#[derive(Default)]
pub struct ZipWriter {
    out: Vec<u8>,
    entries: Vec<CentralEntry>,
}

fn to_u32(value: usize) -> Result<u32> {
    u32::try_from(value).map_err(|_| Error::TooLarge)
}

impl ZipWriter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends one entry.
    pub fn add(&mut self, name: &str, data: &[u8], method: Method) -> Result<()> {
        let crc = crc32(data);
        let (method_code, payload) = match method {
            Method::Stored => (METHOD_STORED, data.to_vec()),
            Method::Deflated => (METHOD_DEFLATED, deflate(data)?),
        };
        let offset = to_u32(self.out.len())?;
        let compressed = to_u32(payload.len())?;
        let size = to_u32(data.len())?;
        let name_bytes = name.as_bytes().to_vec();
        let name_len = u16::try_from(name_bytes.len()).map_err(|_| Error::TooLarge)?;
        let out = &mut self.out;
        put32(out, LOCAL_SIGNATURE);
        put16(out, VERSION_NEEDED);
        put16(out, 0);
        put16(out, method_code);
        put16(out, 0);
        put16(out, DOS_DATE_1980_01_01);
        put32(out, crc);
        put32(out, compressed);
        put32(out, size);
        put16(out, name_len);
        put16(out, 0);
        out.extend_from_slice(&name_bytes);
        out.extend_from_slice(&payload);
        self.entries.push(CentralEntry {
            name: name_bytes,
            method: method_code,
            crc,
            compressed,
            size,
            offset,
        });
        Ok(())
    }

    /// Writes the central directory and returns the archive bytes.
    pub fn finish(mut self) -> Result<Vec<u8>> {
        let directory_offset = to_u32(self.out.len())?;
        for entry in &self.entries {
            let out = &mut self.out;
            put32(out, CENTRAL_SIGNATURE);
            put16(out, VERSION_NEEDED);
            put16(out, VERSION_NEEDED);
            put16(out, 0);
            put16(out, entry.method);
            put16(out, 0);
            put16(out, DOS_DATE_1980_01_01);
            put32(out, entry.crc);
            put32(out, entry.compressed);
            put32(out, entry.size);
            put16(out, entry.name.len() as u16);
            put16(out, 0);
            put16(out, 0);
            put16(out, 0);
            put16(out, 0);
            put32(out, 0);
            put32(out, entry.offset);
            out.extend_from_slice(&entry.name);
        }
        let directory_size = to_u32(self.out.len())? - directory_offset;
        let count = u16::try_from(self.entries.len()).map_err(|_| Error::TooLarge)?;
        let out = &mut self.out;
        put32(out, END_SIGNATURE);
        put16(out, 0);
        put16(out, 0);
        put16(out, count);
        put16(out, count);
        put32(out, directory_size);
        put32(out, directory_offset);
        put16(out, 0);
        Ok(self.out)
    }
}

fn deflate(data: &[u8]) -> Result<Vec<u8>> {
    let mut encoder = DeflateEncoder::new(
        Vec::with_capacity(data.len() / 3 + 64),
        Compression::default(),
    );
    encoder.write_all(data)?;
    Ok(encoder.finish()?)
}

fn put16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

const CRC_TABLE: [u32; 256] = crc_table();

const fn crc_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut n = 0;
    while n < 256 {
        let mut c = n as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
            k += 1;
        }
        table[n] = c;
        n += 1;
    }
    table
}

/// The CRC-32 (IEEE 802.3 polynomial) ZIP stores for each entry.
pub fn crc32(data: &[u8]) -> u32 {
    !data.iter().fold(!0u32, |crc, &byte| {
        CRC_TABLE[((crc ^ u32::from(byte)) & 0xFF) as usize] ^ (crc >> 8)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_matches_known_values() {
        assert_eq!(crc32(b""), 0);
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    /// APPNOTE §4.3.16: the archive ends with the end of central directory
    /// record naming the entry count and the directory's offset.
    #[test]
    fn archive_ends_with_directory_record() {
        let mut zip = ZipWriter::new();
        zip.add("a.txt", b"hello", Method::Stored).unwrap();
        zip.add("b.txt", &b"x".repeat(1000), Method::Deflated)
            .unwrap();
        let bytes = zip.finish().unwrap();
        let end = &bytes[bytes.len() - 22..];
        assert_eq!(&end[..4], &END_SIGNATURE.to_le_bytes());
        assert_eq!(u16::from_le_bytes([end[10], end[11]]), 2);
        let offset = u32::from_le_bytes([end[16], end[17], end[18], end[19]]) as usize;
        assert_eq!(&bytes[offset..offset + 4], &CENTRAL_SIGNATURE.to_le_bytes());
        assert!(bytes.len() < 1000);
    }
}

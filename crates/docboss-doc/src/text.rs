//! The piece table ([MS-DOC] §2.9.38 Clx, §2.8.35 PlcPcd) and text
//! retrieval ([MS-DOC] §2.4.1).

use docboss_cfb::codepage::cp1252_char;

use crate::bytes::{i16_at, slice, u16_at, u32_at, u8_at};

/// One piece: a run of consecutive CPs stored contiguously in the
/// WordDocument stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Piece {
    pub cp_start: u32,
    pub cp_end: u32,
    /// Byte offset of the first character in the WordDocument stream.
    pub fc: u32,
    pub compressed: bool,
    pub prm: u16,
}

impl Piece {
    pub fn char_size(&self) -> u32 {
        if self.compressed {
            1
        } else {
            2
        }
    }

    pub fn fc_of(&self, cp: u32) -> u32 {
        self.fc.saturating_add(
            cp.saturating_sub(self.cp_start)
                .saturating_mul(self.char_size()),
        )
    }
}

#[derive(Debug, Default)]
pub struct PieceTable {
    pub pieces: Vec<Piece>,
    /// The grpprls of Clx.RgPrc, which Prm1 values index.
    pub prcs: Vec<Vec<u8>>,
}

/// Prm0 isprm values ([MS-DOC] §2.9.215) mapped to the sprms docboss reads.
fn isprm_sprm(isprm: u8) -> Option<u16> {
    Some(match isprm {
        0x05 => 0x2461,
        0x07 => 0x2405,
        0x08 => 0x2406,
        0x09 => 0x2407,
        0x0C => 0x260A,
        0x18 => 0x2416,
        0x19 => 0x2417,
        0x33 => 0x2431,
        0x41 => 0x0800,
        0x42 => 0x0801,
        0x43 => 0x0802,
        0x47 => 0x0806,
        0x4B => 0x080A,
        0x4D => 0x2A0C,
        0x53 => 0x2A33,
        0x55 => 0x0835,
        0x56 => 0x0836,
        0x57 => 0x0837,
        0x5A => 0x083A,
        0x5B => 0x083B,
        0x5C => 0x083C,
        0x5E => 0x2A3E,
        0x62 => 0x2A42,
        0x68 => 0x2A48,
        0x73 => 0x2A53,
        0x75 => 0x0855,
        0x76 => 0x0856,
        0x78 => 0x2640,
        _ => return None,
    })
}

impl PieceTable {
    /// Reads a Clx: any number of Prc, then one Pcdt ([MS-DOC] §2.9.38).
    pub fn parse(clx: &[u8]) -> Option<PieceTable> {
        let mut table = PieceTable::default();
        let mut at = 0;
        while u8_at(clx, at) == Some(0x01) {
            let size = i16_at(clx, at + 1)?.max(0) as usize;
            table.prcs.push(slice(clx, at + 3, size).to_vec());
            at += 3 + size;
        }
        if u8_at(clx, at) != Some(0x02) {
            return None;
        }
        let length = u32_at(clx, at + 1)? as usize;
        let plc = slice(clx, at + 5, length);
        let n = plc.len().checked_sub(4)? / 12;
        for i in 0..n {
            let cp_start = u32_at(plc, i * 4)?;
            let cp_end = u32_at(plc, (i + 1) * 4)?;
            let pcd = (n + 1) * 4 + i * 8;
            let fc_raw = u32_at(plc, pcd + 2)?;
            let compressed = fc_raw & 0x4000_0000 != 0;
            let fc = fc_raw & 0x3FFF_FFFF;
            let fc = if compressed { fc / 2 } else { fc };
            if cp_end <= cp_start {
                continue;
            }
            table.pieces.push(Piece {
                cp_start,
                cp_end,
                fc,
                compressed,
                prm: u16_at(plc, pcd + 6).unwrap_or(0),
            });
        }
        Some(table)
    }

    /// A single uncompressed or compressed piece over a contiguous range,
    /// for files without a usable Clx.
    pub fn single(fc: u32, count: u32, compressed: bool) -> PieceTable {
        let piece = Piece {
            cp_start: 0,
            cp_end: count,
            fc,
            compressed,
            prm: 0,
        };
        PieceTable {
            pieces: vec![piece],
            prcs: Vec::new(),
        }
    }

    /// Word 6 and 95 pieces: every piece holds 8-bit text at its stated
    /// byte offset, and the compressed flag is not used.
    pub fn eight_bit(mut self) -> PieceTable {
        for piece in &mut self.pieces {
            piece.compressed = true;
        }
        self
    }

    pub fn end(&self) -> u32 {
        self.pieces.iter().map(|p| p.cp_end).max().unwrap_or(0)
    }

    /// The piece holding `cp`.
    pub fn piece_at(&self, cp: u32) -> Option<&Piece> {
        let index = self.pieces.partition_point(|p| p.cp_end <= cp);
        self.pieces.get(index).filter(|p| p.cp_start <= cp)
    }

    /// The property modifiers a piece's Prm applies ([MS-DOC] §2.9.214).
    pub fn prm_grpprl(&self, prm: u16) -> Vec<u8> {
        if prm & 1 == 1 {
            return self
                .prcs
                .get(usize::from(prm >> 1))
                .cloned()
                .unwrap_or_default();
        }
        let isprm = ((prm >> 1) & 0x7F) as u8;
        let value = (prm >> 8) as u8;
        match isprm_sprm(isprm) {
            Some(sprm) if prm != 0 => vec![sprm as u8, (sprm >> 8) as u8, value],
            _ => Vec::new(),
        }
    }

    /// Decodes every CP into UTF-16 code units, in CP order. A piece whose
    /// bytes run off the stream ends early; missing CPs read as U+0000.
    pub fn decode(&self, word: &[u8]) -> Vec<u16> {
        let length = (self.end() as usize).min(word.len());
        let mut text = vec![0u16; length];
        for piece in &self.pieces {
            let start = (piece.cp_start as usize).min(length);
            let count = (piece.cp_end as usize).min(length) - start;
            let bytes = slice(word, piece.fc as usize, count * piece.char_size() as usize);
            let target = &mut text[start..start + count];
            if piece.compressed {
                for (slot, &byte) in target.iter_mut().zip(bytes) {
                    *slot = compressed_char(byte);
                }
                continue;
            }
            for (slot, pair) in target.iter_mut().zip(bytes.as_chunks::<2>().0) {
                *slot = u16::from_le_bytes(*pair);
            }
        }
        text
    }
}

/// A compressed (8-bit) character: Windows-1252, with the exceptions
/// [MS-DOC] §2.9.73 FcCompressed lists all falling inside 1252's own
/// mapping of 0x80 to 0x9F.
pub fn compressed_char(byte: u8) -> u16 {
    cp1252_char(byte) as u32 as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    /// [MS-DOC] §2.9.73: fCompressed halves the stored fc.
    #[test]
    fn clx_with_prc_and_two_pieces() {
        let mut clx = vec![0x01, 0x03, 0x00, 0x35, 0x08, 0x01];
        let mut plc = Vec::new();
        for cp in [0u32, 3, 5] {
            plc.extend_from_slice(&cp.to_le_bytes());
        }
        for (fc, prm) in [(0x4000_0000u32 | 200, 0u16), (300, 1)] {
            plc.extend_from_slice(&0u16.to_le_bytes());
            plc.extend_from_slice(&fc.to_le_bytes());
            plc.extend_from_slice(&prm.to_le_bytes());
        }
        clx.push(0x02);
        clx.extend_from_slice(&(plc.len() as u32).to_le_bytes());
        clx.extend_from_slice(&plc);
        let table = PieceTable::parse(&clx).unwrap();
        assert_eq!(table.pieces.len(), 2);
        assert_eq!(
            (table.pieces[0].fc, table.pieces[0].compressed),
            (100, true)
        );
        assert_eq!(
            (table.pieces[1].fc, table.pieces[1].compressed),
            (300, false)
        );
        assert_eq!(table.prm_grpprl(1), vec![0x35, 0x08, 0x01]);
        let mut word = vec![0u8; 400];
        word[100..103].copy_from_slice(b"a\x93c");
        word[300..304].copy_from_slice(&[0x3A, 0x04, 0x20, 0x00]);
        let text = String::from_utf16(&table.decode(&word)).unwrap();
        assert_eq!(text, "a\u{201C}c\u{43A} ");
    }
}

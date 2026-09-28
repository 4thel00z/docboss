//! The File Information Block ([MS-DOC] §2.5).

use crate::bytes::{u16_at, u32_at};
use crate::{Error, Result};

/// Indices of the FibRgFcLcb97 pairs docboss reads ([MS-DOC] §2.5.6).
pub mod slot {
    pub const STSHF: usize = 1;
    pub const PLCFFND_REF: usize = 2;
    pub const PLCFFND_TXT: usize = 3;
    pub const PLCFAND_REF: usize = 4;
    pub const PLCFAND_TXT: usize = 5;
    pub const PLCF_SED: usize = 6;
    pub const PLCF_HDD: usize = 11;
    pub const PLCF_BTE_CHPX: usize = 12;
    pub const PLCF_BTE_PAPX: usize = 13;
    pub const STTBF_FFN: usize = 15;
    pub const STTBF_BKMK: usize = 21;
    pub const PLCF_BKF: usize = 22;
    pub const PLCF_BKL: usize = 23;
    pub const DOP: usize = 31;
    pub const CLX: usize = 33;
    pub const GRP_XST_ATN_OWNERS: usize = 36;
    pub const PLC_SPA_MOM: usize = 40;
    pub const PLCFEND_REF: usize = 46;
    pub const PLCFEND_TXT: usize = 47;
    pub const DGG_INFO: usize = 50;
    pub const STTBF_RMARK: usize = 51;
    pub const PLF_LST: usize = 73;
    pub const PLF_LFO: usize = 74;
}

/// The character counts of each document part, in order ([MS-DOC] §2.5.4).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    pub text: u32,
    pub footnotes: u32,
    pub headers: u32,
    pub comments: u32,
    pub endnotes: u32,
    pub textboxes: u32,
    pub header_textboxes: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fib {
    pub n_fib: u16,
    pub complex: bool,
    pub encrypted: bool,
    pub which_table_1: bool,
    pub obfuscated: bool,
    pub key: u32,
    pub counts: Counts,
    pub fc_lcb: Vec<(u32, u32)>,
    /// Word 6 and 95 only: the offset and limit of the text in the
    /// WordDocument stream.
    pub fc_min: u32,
    pub fc_mac: u32,
}

impl Fib {
    /// Reads the FIB at the start of the WordDocument stream
    /// ([MS-DOC] §2.5.15 How to read the FIB).
    pub fn parse(word: &[u8]) -> Result<Fib> {
        if word.len() < 32 {
            return Err(Error::NotWord("WordDocument stream shorter than FibBase"));
        }
        if u16_at(word, 0) != Some(0xA5EC) && u16_at(word, 0) != Some(0xA5DC) {
            return Err(Error::NotWord("FibBase.wIdent is not 0xA5EC"));
        }
        let n_fib = u16_at(word, 2).unwrap_or(0);
        let flags = u16_at(word, 10).unwrap_or(0);
        let mut fib = Fib {
            n_fib,
            complex: flags & 0x0004 != 0,
            encrypted: flags & 0x0100 != 0,
            which_table_1: flags & 0x0200 != 0,
            obfuscated: flags & 0x8000 != 0,
            key: u32_at(word, 14).unwrap_or(0),
            counts: Counts::default(),
            fc_lcb: Vec::new(),
            fc_min: u32_at(word, 24).unwrap_or(0),
            fc_mac: u32_at(word, 28).unwrap_or(0),
        };
        if !fib.is_word97() {
            fib.counts.text = u32_at(word, 0x34).unwrap_or(0);
            return Ok(fib);
        }
        let csw = usize::from(u16_at(word, 32).unwrap_or(14));
        let lw_at = 34 + csw * 2;
        let cslw = usize::from(u16_at(word, lw_at).unwrap_or(22));
        let lw = |i: usize| u32_at(word, lw_at + 2 + i * 4).unwrap_or(0);
        fib.counts = Counts {
            text: lw(3),
            footnotes: lw(4),
            headers: lw(5),
            comments: lw(7),
            endnotes: lw(8),
            textboxes: lw(9),
            header_textboxes: lw(10),
        };
        let blob_at = lw_at + 2 + cslw * 4;
        let pairs = usize::from(u16_at(word, blob_at).unwrap_or(0));
        fib.fc_lcb = (0..pairs)
            .map_while(|i| {
                let at = blob_at + 2 + i * 8;
                Some((u32_at(word, at)?, u32_at(word, at + 4)?))
            })
            .collect();
        Ok(fib)
    }

    /// Whether this is a Word 97 or later FIB. Word 6 and 95 files carry
    /// nFib values below 0x00C1 and an older layout.
    pub fn is_word97(&self) -> bool {
        self.n_fib >= 0x00C0
    }

    /// The table stream offset and size of one FibRgFcLcb pair, or `None`
    /// when absent or empty.
    pub fn range(&self, slot: usize) -> Option<(usize, usize)> {
        let (fc, lcb) = *self.fc_lcb.get(slot)?;
        if lcb == 0 {
            return None;
        }
        Some((fc as usize, lcb as usize))
    }

    pub fn table_stream_name(&self) -> &'static str {
        if self.which_table_1 {
            "1Table"
        } else {
            "0Table"
        }
    }
}

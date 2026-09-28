//! Formatted disk pages ([MS-DOC] §2.9.33 ChpxFkp, §2.9.174 PapxFkp) and
//! their bin tables ([MS-DOC] §2.8.5, §2.8.6).
//! [MS-DOC] §2.9.33, §2.9.174, §2.8.5, §2.8.6, §2.9.206, §2.9.207.

use crate::bytes::{slice, u16_at, u32_at, u8_at};

const PAGE: usize = 512;

/// A range of the WordDocument stream and the grpprl formatting it, kept
/// as an offset and length into the stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormatRun {
    pub fc_start: u32,
    pub fc_end: u32,
    pub grpprl: (usize, usize),
    /// Paragraph runs only: the paragraph style.
    pub istd: u16,
}

fn pages(plc: &[u8]) -> Vec<u32> {
    let (_, data) = crate::bytes::plc(plc, 4);
    data.iter()
        .filter_map(|pn| u32_at(pn, 0).map(|v| v & 0x003F_FFFF))
        .collect()
}

/// Every character run of every CHPX FKP, sorted by fc.
pub fn chpx_runs(word: &[u8], bin_table: &[u8]) -> Vec<FormatRun> {
    let mut runs = Vec::new();
    for pn in pages(bin_table) {
        let base = pn as usize * PAGE;
        let page = slice(word, base, PAGE);
        if page.len() < PAGE {
            continue;
        }
        let count = usize::from(page[511]).min(101);
        for i in 0..count {
            let (Some(fc_start), Some(fc_end)) = (u32_at(page, i * 4), u32_at(page, (i + 1) * 4))
            else {
                break;
            };
            let offset = usize::from(u8_at(page, (count + 1) * 4 + i).unwrap_or(0)) * 2;
            let grpprl = match offset {
                0 => (0, 0),
                _ => (
                    base + offset + 1,
                    usize::from(page[offset.min(511)]).min(PAGE - offset - 1),
                ),
            };
            runs.push(FormatRun {
                fc_start,
                fc_end,
                grpprl,
                istd: 0,
            });
        }
    }
    runs.sort_by_key(|run| run.fc_start);
    runs
}

/// Every paragraph run of every PAPX FKP, sorted by fc.
pub fn papx_runs(word: &[u8], bin_table: &[u8]) -> Vec<FormatRun> {
    let mut runs = Vec::new();
    for pn in pages(bin_table) {
        let base = pn as usize * PAGE;
        let page = slice(word, base, PAGE);
        if page.len() < PAGE {
            continue;
        }
        let count = usize::from(page[511]).min(0x1D);
        for i in 0..count {
            let (Some(fc_start), Some(fc_end)) = (u32_at(page, i * 4), u32_at(page, (i + 1) * 4))
            else {
                break;
            };
            let offset = usize::from(u8_at(page, (count + 1) * 4 + i * 13).unwrap_or(0)) * 2;
            if offset == 0 || offset >= 511 {
                runs.push(FormatRun {
                    fc_start,
                    fc_end,
                    grpprl: (0, 0),
                    istd: 0,
                });
                continue;
            }
            let cb = usize::from(page[offset]);
            let (start, size) = match cb {
                0 => (
                    offset + 2,
                    usize::from(u8_at(page, offset + 1).unwrap_or(0)) * 2,
                ),
                _ => (offset + 1, cb * 2 - 1),
            };
            let size = size.min(PAGE.saturating_sub(start));
            let istd = u16_at(page, start).unwrap_or(0);
            let grpprl = (base + start + 2, size.saturating_sub(2));
            runs.push(FormatRun {
                fc_start,
                fc_end,
                grpprl,
                istd,
            });
        }
    }
    runs.sort_by_key(|run| run.fc_start);
    runs
}

/// The run containing `fc`, by binary search.
pub fn find(runs: &[FormatRun], fc: u32) -> Option<&FormatRun> {
    let index = runs.partition_point(|run| run.fc_end <= fc);
    runs.get(index).filter(|run| run.fc_start <= fc)
}

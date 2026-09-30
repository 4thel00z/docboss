//! Word 6 and Word 95 files (nFib 0x65 to 0x68). Their FIB, piece table,
//! FKPs, stylesheet, section table and tables have the Word 97 layout with
//! older details: 1-byte sprm codes, 2-byte bin table page numbers, 7-byte
//! paragraph BX entries, 2-byte BRCs, 10-byte TCs, 8-bit style and font
//! names. The sprms are translated to their Word 97 equivalents so the
//! Word 97 property code reads them. [MS-DOC] §2.2.5 and §2.5.15 describe
//! the Word 97 forms these map onto; the Word 6 sprm codes are the Prm0
//! isprm values of [MS-DOC] §2.9.215.

use std::collections::BTreeSet;

use docboss_model::FontEntry;

use crate::bytes::{slice, u16_at, u32_at, u8_at};
use crate::fkp::FormatRun;

const PAGE: usize = 512;

/// The FibRgFcLcb pairs a Word 6 FIB holds, from fcStshfOrig at 0x58 to
/// fcSttbfAtnbkmk, in the Word 97 order.
const PAIRS: usize = 38;

/// How a Word 6 sprm's operand is sized.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Size {
    Fixed(usize),
    /// A 1-byte size, then that many bytes.
    Variable,
    /// A 2-byte size one more than the bytes after it (sprmTDefTable).
    Long,
    /// sprmPChgTabs: a size byte, or 255 and a self-describing operand.
    Tabs,
}

fn size(code: u8) -> Option<Size> {
    Some(match code {
        2 | 16..=19 | 21 | 22 | 26..=28 | 30..=36 | 38..=43 | 45..=49 => Size::Fixed(2),
        4..=11 | 13 | 14 | 24 | 25 | 29 | 37 | 44 | 50 | 51 | 53..=58 => Size::Fixed(1),
        20 => Size::Fixed(4),
        3 | 12 | 15 | 52 | 68 | 74 | 81 | 82 | 103 | 105 | 106 | 108 | 120 | 133 | 191 => {
            Size::Variable
        }
        23 => Size::Tabs,
        65..=67 | 71 | 75 | 85..=92 | 94 | 98 | 100 | 102 | 104 | 117..=119 => Size::Fixed(1),
        69 | 72 | 80 | 93 | 96 | 97 | 99 | 101 | 107 | 109 | 110 | 121..=124 => Size::Fixed(2),
        70 => Size::Fixed(4),
        73 | 95 => Size::Fixed(3),
        83 => Size::Fixed(0),
        131 | 132 | 138 | 139 | 142 | 143 | 146 | 147 | 150..=153 | 158 | 159 | 162 | 163 => {
            Size::Fixed(1)
        }
        140 | 141 | 144 | 145 | 148 | 149 | 154..=157 | 160 | 161 | 164..=171 => Size::Fixed(2),
        136 | 137 => Size::Fixed(3),
        182..=184 | 189 | 195 | 197 | 198 => Size::Fixed(2),
        185 | 186 => Size::Fixed(1),
        187 => Size::Fixed(12),
        188 | 190 => Size::Long,
        192 | 194 | 196 | 200 => Size::Fixed(4),
        193 | 199 => Size::Fixed(5),
        _ => return None,
    })
}

/// One Word 6 property modifier: its 1-byte code and operand, size prefix
/// included.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Prl6<'a> {
    pub code: u8,
    pub operand: &'a [u8],
}

/// Walks a Word 6 grpprl. Trailing zero padding ends it; an unknown code
/// or a truncated operand stops it and is returned as the error.
pub fn prls(grpprl: &[u8]) -> (Vec<Prl6<'_>>, Option<u8>) {
    let mut out = Vec::new();
    let mut at = 0;
    while at < grpprl.len() {
        let code = grpprl[at];
        if code == 0 && grpprl[at..].iter().all(|&b| b == 0) {
            break;
        }
        let Some(kind) = size(code) else {
            return (out, Some(code));
        };
        let operand = at + 1;
        let length = match kind {
            Size::Fixed(n) => n,
            Size::Variable => 1 + usize::from(u8_at(grpprl, operand).unwrap_or(0)),
            Size::Long => 1 + usize::from(u16_at(grpprl, operand).unwrap_or(0)),
            Size::Tabs => tabs_size(grpprl, operand),
        };
        let Some(bytes) = grpprl.get(operand..operand + length) else {
            return (out, Some(code));
        };
        out.push(Prl6 {
            code,
            operand: bytes,
        });
        at = operand + length;
    }
    (out, None)
}

fn tabs_size(grpprl: &[u8], operand: usize) -> usize {
    let cb = u8_at(grpprl, operand).unwrap_or(0);
    if cb != 255 {
        return usize::from(cb) + 1;
    }
    let deletions = usize::from(u8_at(grpprl, operand + 1).unwrap_or(0));
    let additions_at = operand + 2 + deletions * 4;
    let additions = usize::from(u8_at(grpprl, additions_at).unwrap_or(0));
    2 + deletions * 4 + 1 + additions * 3
}

/// The Word 97 sprm a Word 6 code carries over to with an operand of the
/// same bytes.
fn same(code: u8) -> Option<u16> {
    Some(match code {
        2 => 0x4600,
        5 => 0x2461,
        7 => 0x2405,
        8 => 0x2406,
        9 => 0x2407,
        15 => 0xC60D,
        16 => 0x840E,
        17 => 0x840F,
        19 => 0x8411,
        20 => 0x6412,
        21 => 0xA413,
        22 => 0xA414,
        23 => 0xC615,
        24 => 0x2416,
        25 => 0x2417,
        26 => 0x8418,
        27 => 0x8419,
        28 => 0x841A,
        29 => 0x261B,
        37 => 0x2423,
        45 => 0x442B,
        46 => 0x442C,
        47 => 0x442D,
        48 => 0x842E,
        49 => 0x842F,
        51 => 0x2431,
        65 => 0x0800,
        66 => 0x0801,
        67 => 0x0802,
        69 => 0x4804,
        70 => 0x6805,
        71 => 0x0806,
        75 => 0x080A,
        80 => 0x4A30,
        83 => 0x2A33,
        85..=92 => 0x0835 + u16::from(code - 85),
        94 => 0x2A3E,
        96 => 0x8840,
        97 => 0x486D,
        98 => 0x2A42,
        99 => 0x4A43,
        101 => 0x4845,
        104 => 0x2A48,
        117 => 0x0855,
        118 => 0x0856,
        136 => 0xF203,
        137 => 0xF204,
        138 => 0x3005,
        142 => 0x3009,
        143 => 0x300A,
        144 => 0x500B,
        145 => 0x900C,
        147 => 0x300E,
        150 => 0x3011,
        156 => 0xB017,
        157 => 0xB018,
        158 => 0x3019,
        161 => 0x501C,
        162 => 0x301D,
        164 => 0xB01F,
        165 => 0xB020,
        166 => 0xB021,
        167 => 0xB022,
        168 => 0x9023,
        169 => 0x9024,
        170 => 0xB025,
        182 => 0x5400,
        183 => 0x9601,
        184 => 0x9602,
        185 => 0x3403,
        186 => 0x3404,
        189 => 0x9407,
        191 => 0xD609,
        197 => 0x5624,
        _ => return None,
    })
}

/// A Word 6 BRC (2 bytes: line width in 0.75 pt, 6 dotted, 7 dashed; type;
/// shadow; ico; space) as a Word 97 Brc80 ([MS-DOC] §2.9.17).
fn brc80(brc: u16) -> [u8; 4] {
    if brc == 0 {
        return [0; 4];
    }
    let width = (brc & 7) as u8;
    let kind = ((brc >> 3) & 3) as u8;
    let shadow = ((brc >> 5) & 1) as u8;
    let ico = ((brc >> 6) & 0x1F) as u8;
    let space = ((brc >> 11) & 0x1F) as u8;
    let (width, kind) = match width {
        6 => (6, 6),
        7 => (6, 7),
        w => (w.max(1) * 6, kind),
    };
    [width, kind, ico, space | shadow << 5]
}

fn push(out: &mut Vec<u8>, sprm: u16, operand: &[u8]) {
    out.extend_from_slice(&sprm.to_le_bytes());
    out.extend_from_slice(operand);
}

fn brc_at(operand: &[u8], at: usize) -> [u8; 4] {
    brc80(u16_at(operand, at).unwrap_or(0))
}

/// sprmTDefTable with 10-byte TCs (a grf word and four BRCs) as the Word 97
/// operand with 20-byte TC80s ([MS-DOC] §2.9.321, §2.9.313). The Word 6
/// fFirstMerged and fMerged bits become horzMerge 2 and 1 ([MS-DOC] §2.9.317).
fn def_table(operand: &[u8], out: &mut Vec<u8>) {
    let data = operand.get(2..).unwrap_or(&[]);
    let count = usize::from(u8_at(data, 0).unwrap_or(0)).min(63);
    let edges = slice(data, 1, (count + 1) * 2);
    let tcs_at = 1 + (count + 1) * 2;
    let mut body = vec![count as u8];
    body.extend_from_slice(edges);
    for i in 0..count {
        let at = tcs_at + i * 10;
        if at + 10 > data.len() {
            break;
        }
        let grf: u16 = match u16_at(data, at).unwrap_or(0) & 3 {
            1 => 2,
            2 => 1,
            other => other,
        };
        body.extend_from_slice(&grf.to_le_bytes());
        body.extend_from_slice(&[0, 0]);
        for side in 0..4 {
            body.extend_from_slice(&brc_at(data, at + 2 + side * 2));
        }
    }
    let cb = (body.len() + 1) as u16;
    out.extend_from_slice(&0xD608u16.to_le_bytes());
    out.extend_from_slice(&cb.to_le_bytes());
    out.extend_from_slice(&body);
}

/// A Word 6 sprmPAnld operand as the Word 97 one: its size byte, the
/// 20-byte head, then the 32 label characters widened from 8 to 16 bits.
fn anld(operand: &[u8]) -> Vec<u8> {
    let body = operand.get(1..).unwrap_or(&[]);
    let mut out = vec![84u8];
    out.extend((0..20).map(|i| body.get(i).copied().unwrap_or(0)));
    out.extend((0..32).flat_map(|i| [body.get(20 + i).copied().unwrap_or(0), 0]));
    out
}

/// Translates a Word 6 grpprl into a Word 97 one. Codes without a Word 97
/// counterpart docboss reads are left out and added to `dropped`.
pub fn translate(grpprl: &[u8], dropped: &mut BTreeSet<u8>) -> Vec<u8> {
    let (prls, stop) = prls(grpprl);
    if let Some(code) = stop {
        dropped.insert(code);
    }
    let mut out = Vec::with_capacity(grpprl.len() * 2);
    for prl in prls {
        let operand = prl.operand;
        if let Some(sprm) = same(prl.code) {
            push(&mut out, sprm, operand);
            continue;
        }
        match prl.code {
            38..=42 => push(
                &mut out,
                0x6424 + u16::from(prl.code - 38),
                &brc_at(operand, 0),
            ),
            68 => push(&mut out, 0x6A03, slice(operand, 1, 4)),
            74 => {
                let font = u16_at(operand, 1).unwrap_or(0);
                let char = u16::from(u8_at(operand, 3).unwrap_or(0));
                let mut symbol = font.to_le_bytes().to_vec();
                symbol.extend_from_slice(&char.to_le_bytes());
                push(&mut out, 0x6A09, &symbol);
            }
            93 => {
                push(&mut out, 0x4A4F, operand);
                push(&mut out, 0x4A51, operand);
            }
            95 => {
                let size = u8_at(operand, 0).unwrap_or(0);
                if size != 0 {
                    push(&mut out, 0x4A43, &u16::from(size).to_le_bytes());
                }
                let position = u8_at(operand, 2).unwrap_or(0x80);
                if position != 0x80 {
                    push(&mut out, 0x4845, &i16::from(position as i8).to_le_bytes());
                }
            }
            187 => {
                let mut borders = vec![24u8];
                for side in 0..6 {
                    borders.extend_from_slice(&brc_at(operand, side * 2));
                }
                push(&mut out, 0xD605, &borders);
            }
            190 => def_table(operand, &mut out),
            12 => push(&mut out, 0xC63E, &anld(operand)),
            13 => push(&mut out, 0x263D, operand),
            4
            | 6
            | 10
            | 11
            | 14
            | 26..=37
            | 43..=46
            | 48..=50
            | 53..=58
            | 72
            | 73
            | 100
            | 102
            | 107
            | 109
            | 110
            | 119..=124
            | 131..=133
            | 139..=141
            | 146
            | 148
            | 149
            | 151
            | 152
            | 153..=155
            | 159
            | 160
            | 163
            | 171
            | 192..=196
            | 198..=200 => {}
            code => {
                dropped.insert(code);
            }
        }
    }
    out
}

/// The value of sprmSGprfIhdt (code 153) in a Word 6 section grpprl: which
/// of the six headers and footers the section has.
pub fn header_flags(grpprl: &[u8]) -> u8 {
    prls(grpprl)
        .0
        .iter()
        .rev()
        .find(|p| p.code == 153)
        .map_or(0, |p| p.operand[0])
}

/// Reads the Word 6 FIB fields past FibBase: the character counts at 0x34
/// and the fc/lcb pairs at 0x58 ([MS-DOC] §2.5.15 for the Word 97 forms).
pub fn fill_fib(word: &[u8], fib: &mut crate::fib::Fib) {
    let count = |i: usize| u32_at(word, 0x34 + i * 4).unwrap_or(0);
    fib.counts = crate::fib::Counts {
        text: count(0),
        footnotes: count(1),
        headers: count(2),
        comments: count(4),
        endnotes: count(5),
        textboxes: count(6),
        header_textboxes: count(7),
    };
    fib.fc_lcb = (0..PAIRS)
        .map_while(|i| {
            let at = 0x58 + i * 8;
            Some((u32_at(word, at)?, u32_at(word, at + 4)?))
        })
        .collect();
}

/// Whether the pieces of a fast-saved Word 6 or 95 file hold UTF-16 text,
/// as some writers store it, rather than 8-bit text: nearly every second
/// byte of the pieces read as UTF-16 is zero. Pieces carry no flag for it.
pub fn unicode_pieces(word: &[u8], pieces: &crate::text::PieceTable) -> bool {
    let mut zeros = 0usize;
    let mut total = 0usize;
    for piece in &pieces.pieces {
        let count = (piece.cp_end - piece.cp_start) as usize;
        let bytes = slice(word, piece.fc as usize, count.saturating_mul(2));
        if bytes.len() < count * 2 {
            return false;
        }
        total += count;
        zeros += bytes.iter().skip(1).step_by(2).filter(|&&b| b == 0).count();
    }
    total > 0 && zeros * 10 >= total * 9
}

/// The FKP page numbers of a Word 6 bin table: 2-byte PNs, completed with
/// consecutive pages from `first` up to the `count` the FIB states when the
/// table lists fewer (files saved without fast save).
fn pages(plc: &[u8], first: u16, count: u16) -> Vec<u32> {
    let (_, data) = crate::bytes::plc(plc, 2);
    let mut pages: Vec<u32> = data
        .iter()
        .filter_map(|pn| u16_at(pn, 0).map(u32::from))
        .collect();
    let wanted = usize::from(count).min(0xFFFF);
    let mut next = pages.last().map_or(u32::from(first), |&p| p + 1);
    while pages.len() < wanted {
        pages.push(next);
        next += 1;
    }
    pages
}

/// The runs of the character or paragraph FKPs of a Word 6 file, their
/// grpprls translated into `arena` (offsets past `base`, the end of the
/// WordDocument stream).
pub struct Fkps {
    pub chpx: Vec<FormatRun>,
    pub papx: Vec<FormatRun>,
}

/// Reads both bin tables and their FKPs ([MS-DOC] §2.9.33, §2.9.174 for
/// the Word 97 forms; the Word 6 PAPX holds `cw` words, istd included).
pub fn fkps(
    word: &[u8],
    chpx_bins: &[u8],
    papx_bins: &[u8],
    arena: &mut Vec<u8>,
    dropped: &mut BTreeSet<u8>,
) -> Fkps {
    let base = word.len();
    let mut place = |grpprl: &[u8], arena: &mut Vec<u8>| {
        let translated = translate(grpprl, dropped);
        let at = base + arena.len();
        arena.extend_from_slice(&translated);
        (at, translated.len())
    };
    let first_chp = u16_at(word, 0x18A).unwrap_or(0);
    let first_pap = u16_at(word, 0x18C).unwrap_or(0);
    let count_chp = u16_at(word, 0x18E).unwrap_or(0);
    let count_pap = u16_at(word, 0x190).unwrap_or(0);
    let mut chpx = Vec::new();
    for pn in pages(chpx_bins, first_chp, count_chp) {
        let page = slice(word, pn as usize * PAGE, PAGE);
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
                _ => {
                    let size = usize::from(page[offset.min(511)]);
                    place(slice(page, offset + 1, size), arena)
                }
            };
            chpx.push(FormatRun {
                fc_start,
                fc_end,
                grpprl,
                istd: 0,
            });
        }
    }
    let mut papx = Vec::new();
    for pn in pages(papx_bins, first_pap, count_pap) {
        let page = slice(word, pn as usize * PAGE, PAGE);
        if page.len() < PAGE {
            continue;
        }
        let count = usize::from(page[511]).min(46);
        for i in 0..count {
            let (Some(fc_start), Some(fc_end)) = (u32_at(page, i * 4), u32_at(page, (i + 1) * 4))
            else {
                break;
            };
            let offset = usize::from(u8_at(page, (count + 1) * 4 + i * 7).unwrap_or(0)) * 2;
            if offset == 0 || offset >= 511 {
                papx.push(FormatRun {
                    fc_start,
                    fc_end,
                    grpprl: (0, 0),
                    istd: 0,
                });
                continue;
            }
            let size = usize::from(page[offset]) * 2;
            let body = slice(page, offset + 1, size);
            let istd = u16_at(body, 0).unwrap_or(0);
            let grpprl = place(body.get(2..).unwrap_or(&[]), arena);
            papx.push(FormatRun {
                fc_start,
                fc_end,
                grpprl,
                istd,
            });
        }
    }
    chpx.sort_by_key(|run| run.fc_start);
    papx.sort_by_key(|run| run.fc_start);
    Fkps { chpx, papx }
}

/// The Word 6 font table: its total size, then FFNs of a size byte, the
/// family and pitch byte, the weight, the character set, the offset of the
/// alternate name and the 8-bit name ([MS-DOC] §2.9.82 for the Word 97 FFN).
pub fn fonts(bytes: &[u8], code_page: u32) -> Vec<FontEntry> {
    let total = usize::from(u16_at(bytes, 0).unwrap_or(0)).min(bytes.len());
    let mut at = 2;
    let mut out = Vec::new();
    while at < total && out.len() < 0x7FFF {
        let length = usize::from(bytes[at]) + 1;
        let ffn = slice(bytes, at, length);
        at += length;
        let ffid = u8_at(ffn, 1).unwrap_or(0);
        let alt = usize::from(u8_at(ffn, 5).unwrap_or(0));
        let text = |from: usize| {
            let raw = ffn.get(from..).unwrap_or(&[]);
            let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
            docboss_cfb::codepage::decode(code_page, &raw[..end]).0
        };
        let name = text(6);
        let alt_name = (alt != 0).then(|| text(6 + alt)).filter(|s| !s.is_empty());
        let family = match (ffid >> 4) & 7 {
            1 => "roman",
            2 => "swiss",
            3 => "modern",
            4 => "script",
            5 => "decorative",
            _ => "auto",
        };
        let pitch = match ffid & 3 {
            1 => "fixed",
            2 => "variable",
            _ => "default",
        };
        out.push(FontEntry {
            name,
            alt_name,
            family: Some(family.to_string()),
            pitch: Some(pitch.to_string()),
            ..FontEntry::default()
        });
    }
    out
}

/// The header and footer story of each of the six kinds (even header,
/// odd header, even footer, odd footer, first header, first footer) of
/// each section: Plcfhdd lists only the stories that exist, first the
/// note separators DOP.grpfIhdt names, then per section those its
/// sprmSGprfIhdt names.
pub fn header_stories(dop_flags: u8, section_flags: &[u8]) -> Vec<[Option<usize>; 6]> {
    let mut next = (dop_flags & 0x3F).count_ones() as usize;
    section_flags
        .iter()
        .map(|&flags| {
            let mut stories = [None; 6];
            for (kind, story) in stories.iter_mut().enumerate() {
                if flags & (1 << kind) == 0 {
                    continue;
                }
                *story = Some(next);
                next += 1;
            }
            stories
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A Word 6 numbered paragraph: sprmPAnld's 52-byte ANLD, lower roman
    /// from 3 with "(" before and ")" after, and sprmPNLvlAnm 10.
    #[test]
    fn numbered_paragraphs_carry_their_anld() {
        let mut anld = vec![0u8; 52];
        anld[0] = 2;
        anld[1] = 1;
        anld[2] = 2;
        anld[10] = 3;
        anld[20] = b'(';
        anld[21] = b')';
        let mut grpprl = vec![12, 52];
        grpprl.extend(&anld);
        grpprl.extend([13, 10]);
        let mut dropped = BTreeSet::new();
        let out = translate(&grpprl, &mut dropped);
        assert!(dropped.is_empty());
        let mut props = docboss_model::ParagraphProperties::default();
        let mut extra = crate::props::ParaExtra::default();
        crate::props::apply_pap(&out, &mut props, &mut extra);
        assert_eq!(extra.anld_level, Some(10));
        let anld = extra.anld.expect("the ANLD reads");
        assert_eq!((anld.nfc, anld.start), (2, 3));
        assert_eq!((anld.before.as_str(), anld.after.as_str()), ("(", ")"));
    }

    #[test]
    fn grpprls_walk_by_the_word6_sizes() {
        let grpprl = [
            0x05, 0x01, 0x33, 0x01, 0x63, 0x18, 0x00, 0x5D, 0x02, 0x00, 0x44, 0x04, 0x10, 0x20,
            0x30, 0x40, 0x00, 0x00,
        ];
        let (prls, stop) = prls(&grpprl);
        assert_eq!(stop, None);
        let codes: Vec<u8> = prls.iter().map(|p| p.code).collect();
        assert_eq!(codes, [5, 51, 99, 93, 68]);
        let mut dropped = BTreeSet::new();
        let out = translate(&grpprl, &mut dropped);
        let sprms: Vec<(u16, usize)> = crate::sprm::prls(&out)
            .map(|p| (p.sprm, p.operand.len()))
            .collect();
        assert_eq!(
            sprms,
            [
                (0x2461, 1),
                (0x2431, 1),
                (0x4A43, 2),
                (0x4A4F, 2),
                (0x4A51, 2),
                (0x6A03, 4)
            ]
        );
        assert!(dropped.is_empty());
        assert_eq!(prls_stop(&[0xF0, 1]), Some(0xF0));
    }

    fn prls_stop(grpprl: &[u8]) -> Option<u8> {
        prls(grpprl).1
    }

    #[test]
    fn def_table_widens_its_cells() {
        let mut grpprl = vec![190, 28, 0, 2, 0, 0, 0xC5, 0x02, 0x66, 0x0E];
        grpprl.extend_from_slice(&[1, 0, 0xCA, 0x03, 0xCA, 0x03, 0xCA, 0x03, 0, 0]);
        grpprl.extend_from_slice(&[0, 0, 0xCA, 0x03, 0, 0, 0xCA, 0x03, 0xCA, 0x03]);
        grpprl.extend_from_slice(&[191, 4, 1, 5, 1, 5]);
        let out = translate(&grpprl, &mut BTreeSet::new());
        let prls: Vec<crate::sprm::Prl> = crate::sprm::prls(&out).collect();
        assert_eq!(prls.len(), 2);
        assert_eq!(prls[0].sprm, 0xD608);
        let row = crate::table::RowInfo::parse(&out);
        assert_eq!(row.edges, [0, 0x02C5, 0x0E66]);
        assert_eq!(row.cells.len(), 2);
        assert_eq!(row.cells[0].horizontal_merge, 2);
        let top = row.cells[0]
            .borders
            .and_then(|b| b.top)
            .expect("top border");
        assert_eq!(top.size, 12);
        assert_eq!(prls[1].sprm, 0xD609);
    }

    #[test]
    fn header_stories_skip_absent_kinds() {
        let stories = header_stories(0b11, &[0b10, 0, 0b1010]);
        assert_eq!(stories[0][1], Some(2));
        assert_eq!(stories[1], [None; 6]);
        assert_eq!(stories[2][1], Some(3));
        assert_eq!(stories[2][3], Some(4));
    }
}

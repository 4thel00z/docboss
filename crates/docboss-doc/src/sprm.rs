//! Single property modifiers ([MS-DOC] §2.2.5): a grpprl is a sequence of
//! Prl, each a 2-byte Sprm and an operand whose size the Sprm encodes.

use crate::bytes::{u16_at, u8_at};

/// One property modifier and its operand bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Prl<'a> {
    pub sprm: u16,
    pub operand: &'a [u8],
}

impl<'a> Prl<'a> {
    /// The sprm group: 1 paragraph, 2 character, 3 picture, 4 section,
    /// 5 table ([MS-DOC] §2.2.5.1).
    pub fn sgc(&self) -> u8 {
        ((self.sprm >> 10) & 7) as u8
    }

    pub fn u8(&self) -> u8 {
        self.operand.first().copied().unwrap_or(0)
    }

    pub fn u16(&self) -> u16 {
        u16_at(self.operand, 0).unwrap_or_else(|| u16::from(self.u8()))
    }

    pub fn i16(&self) -> i16 {
        self.u16() as i16
    }

    pub fn u32(&self) -> u32 {
        crate::bytes::u32_at(self.operand, 0).unwrap_or_else(|| u32::from(self.u16()))
    }

    /// The payload of a variable-length operand after its size prefix.
    pub fn variable(&self) -> &'a [u8] {
        self.operand.get(1..).unwrap_or(&[])
    }
}

const SPRM_T_DEF_TABLE: u16 = 0xD608;
const SPRM_P_CHG_TABS: u16 = 0xC615;

/// The operand size of a sprm at `at` whose operand starts at `at + 2`
/// ([MS-DOC] §2.2.5.1 spra), including any size prefix.
fn operand_size(grpprl: &[u8], sprm: u16, at: usize) -> usize {
    let operand = at + 2;
    match sprm >> 13 {
        0 | 1 => 1,
        2 | 4 | 5 => 2,
        3 => 4,
        7 => 3,
        _ => match sprm {
            SPRM_T_DEF_TABLE => usize::from(u16_at(grpprl, operand).unwrap_or(0)) + 1,
            SPRM_P_CHG_TABS => {
                let cb = u8_at(grpprl, operand).unwrap_or(0);
                if cb != 255 {
                    return usize::from(cb) + 1;
                }
                let deletions = usize::from(u8_at(grpprl, operand + 1).unwrap_or(0));
                let additions_at = operand + 2 + deletions * 4;
                let additions = usize::from(u8_at(grpprl, additions_at).unwrap_or(0));
                2 + deletions * 4 + 1 + additions * 3
            }
            _ => usize::from(u8_at(grpprl, operand).unwrap_or(0)) + 1,
        },
    }
}

/// Iterates the Prls of a grpprl, stopping at a truncated one.
pub fn prls(grpprl: &[u8]) -> impl Iterator<Item = Prl<'_>> {
    let mut at = 0usize;
    std::iter::from_fn(move || {
        let sprm = u16_at(grpprl, at)?;
        let size = operand_size(grpprl, sprm, at);
        let operand = grpprl.get(at + 2..at + 2 + size)?;
        at += 2 + size;
        Some(Prl { sprm, operand })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// [MS-DOC] §2.2.5.1: spra selects the operand size; spra 6 carries its
    /// own size byte, and sprmTDefTable a 2-byte size plus one.
    #[test]
    fn operand_sizes_follow_spra() {
        let grpprl = [
            0x35, 0x08, 0x01, // sprmCFBold, toggle
            0x43, 0x4A, 0x18, 0x00, // sprmCHps 24
            0x70, 0x68, 1, 2, 3, 0, // sprmCCv, 4 bytes
            0x08, 0xD6, 0x03, 0x00, 1, 0xAA, // sprmTDefTable, cb 3: two bytes follow
            0x2D, 0xD6, 0x02, 9, 9, // spra 6, 2 bytes
        ];
        let got: Vec<(u16, usize)> = prls(&grpprl).map(|p| (p.sprm, p.operand.len())).collect();
        assert_eq!(
            got,
            [
                (0x0835, 1),
                (0x4A43, 2),
                (0x6870, 4),
                (0xD608, 4),
                (0xD62D, 3)
            ]
        );
        assert_eq!(prls(&grpprl).nth(1).unwrap().u16(), 24);
    }

    #[test]
    fn truncated_prl_stops_the_walk() {
        assert_eq!(prls(&[0x43, 0x4A, 0x18]).count(), 0);
        assert_eq!(prls(&[0x35]).count(), 0);
    }
}

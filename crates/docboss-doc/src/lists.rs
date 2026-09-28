//! List definitions ([MS-DOC] §2.9.201 PlfLst, §2.9.200 PlfLfo) as model
//! numbering.

use docboss_model::{
    AbstractNumbering, Justification, Level, NumberFormat, Numbering, NumberingInstance,
    RunProperties,
};

use crate::bytes::{i32_at, slice, u16_at, u32_at, u8_at};
use crate::props::{apply_chp, apply_pap, CharContext, CharExtra, ParaExtra};

const LSTF_SIZE: usize = 28;
const LVLF_SIZE: usize = 28;
const LFO_SIZE: usize = 16;

/// MSONFC ([MS-OSHARED] §2.2.1.3) as a model format.
pub(crate) fn format(nfc: u8) -> NumberFormat {
    match nfc {
        0 => NumberFormat::Decimal,
        1 => NumberFormat::UpperRoman,
        2 => NumberFormat::LowerRoman,
        3 => NumberFormat::UpperLetter,
        4 => NumberFormat::LowerLetter,
        5 => NumberFormat::Ordinal,
        22 => NumberFormat::DecimalZero,
        23 => NumberFormat::Bullet,
        255 => NumberFormat::None,
        other => NumberFormat::Other(format!("msonfc{other}")),
    }
}

/// Bullets in the Symbol font's private-use range, as the characters they
/// show.
fn bullet(c: u16) -> char {
    match c {
        0xF0B7 | 0xF0A7 => '\u{2022}',
        0xF06F => '\u{25E6}',
        0xF0A8 => '\u{25AA}',
        0xF0D8 => '\u{27A2}',
        0xF0FC => '\u{2713}',
        0xF076 => '\u{2756}',
        0xF02D => '-',
        _ => char::from_u32(u32::from(c)).unwrap_or('\u{2022}'),
    }
}

/// Reads one LVL ([MS-DOC] §2.9.149) at `at`, returning it and the offset
/// after it.
fn level(bytes: &[u8], at: usize, index: u8, context: &CharContext<'_>) -> Option<(Level, usize)> {
    let lvlf = bytes.get(at..at + LVLF_SIZE)?;
    let start = i32_at(lvlf, 0)?.max(0) as u32;
    let nfc = lvlf[4];
    let flags = lvlf[5];
    let placeholders: Vec<usize> = lvlf[6..15]
        .iter()
        .take_while(|&&b| b != 0)
        .map(|&b| usize::from(b))
        .collect();
    let cb_chpx = usize::from(lvlf[24]);
    let cb_papx = usize::from(lvlf[25]);
    let restart = lvlf[26];
    let papx_at = at + LVLF_SIZE;
    let chpx_at = papx_at + cb_papx;
    let xst_at = chpx_at + cb_chpx;
    let cch = usize::from(u16_at(bytes, xst_at)?);
    let units: Vec<u16> = (0..cch)
        .filter_map(|i| u16_at(bytes, xst_at + 2 + i * 2))
        .collect();
    let end = xst_at + 2 + cch * 2;
    let format = format(nfc);
    let text = if format == NumberFormat::Bullet {
        units
            .first()
            .map(|&c| bullet(c).to_string())
            .unwrap_or_else(|| "\u{2022}".to_string())
    } else {
        units
            .iter()
            .enumerate()
            .map(|(i, &c)| match placeholders.contains(&(i + 1)) && c < 9 {
                true => format!("%{}", c + 1),
                false => char::from_u32(u32::from(c))
                    .map(String::from)
                    .unwrap_or_default(),
            })
            .collect()
    };
    let mut result = Level {
        level: index,
        start,
        format,
        text,
        justification: Some(match flags & 3 {
            1 => Justification::Center,
            2 => Justification::Right,
            _ => Justification::Left,
        }),
        restart_after: (flags & 0x08 != 0).then_some(restart),
        legal: flags & 0x04 != 0,
        ..Level::default()
    };
    let mut extra = ParaExtra::default();
    apply_pap(
        slice(bytes, papx_at, cb_papx),
        &mut result.paragraph,
        &mut extra,
    );
    let mut char_extra = CharExtra::default();
    let mut run = RunProperties::default();
    apply_chp(
        slice(bytes, chpx_at, cb_chpx),
        &mut run,
        &mut char_extra,
        context,
    );
    result.run = run;
    Some((result, end))
}

/// Reads PlfLst with the LVLs that follow it, then PlfLfo, into model
/// numbering. An abstract definition's id is the list's lsid; a numbering
/// instance's id is its 1-based LFO index, which sprmPIlfo refers to.
pub fn parse(
    table: &[u8],
    lst: Option<(usize, usize)>,
    lfo: Option<(usize, usize)>,
    fonts: &[String],
) -> Numbering {
    let empty = RunProperties::default();
    let context = CharContext {
        fonts,
        styles: &[],
        base: &empty,
    };
    let mut numbering = Numbering::default();
    if let Some((fc, _)) = lst {
        let count = usize::from(u16_at(table, fc).unwrap_or(0));
        let mut at = fc + 2 + count * LSTF_SIZE;
        for i in 0..count {
            let lstf_at = fc + 2 + i * LSTF_SIZE;
            let Some(lsid) = i32_at(table, lstf_at) else {
                break;
            };
            let simple = u8_at(table, lstf_at + 26).unwrap_or(0) & 1 != 0;
            let levels = if simple { 1 } else { 9 };
            let mut definition = AbstractNumbering {
                id: i64::from(lsid),
                levels: Vec::with_capacity(levels),
            };
            for l in 0..levels {
                let Some((level, next)) = level(table, at, l as u8, &context) else {
                    break;
                };
                definition.levels.push(level);
                at = next;
            }
            numbering.abstracts.push(definition);
        }
    }
    let Some((fc, _)) = lfo else {
        return numbering;
    };
    let count = u32_at(table, fc).unwrap_or(0).min(0x7FFF) as usize;
    let mut data_at = fc + 4 + count * LFO_SIZE;
    for i in 0..count {
        let lfo_at = fc + 4 + i * LFO_SIZE;
        let Some(lsid) = i32_at(table, lfo_at) else {
            break;
        };
        let overrides = usize::from(u8_at(table, lfo_at + 12).unwrap_or(0));
        let mut instance = NumberingInstance {
            num_id: i as i64 + 1,
            abstract_id: i64::from(lsid),
            ..NumberingInstance::default()
        };
        data_at += 4;
        for _ in 0..overrides {
            let Some(start) = i32_at(table, data_at) else {
                break;
            };
            let flags = u32_at(table, data_at + 4).unwrap_or(0);
            let ilvl = (flags & 0xF) as u8;
            data_at += 8;
            if flags & 0x10 != 0 {
                instance.start_overrides.push((ilvl, start.max(0) as u32));
            }
            if flags & 0x20 != 0 {
                let Some((level, next)) = level(table, data_at, ilvl, &context) else {
                    break;
                };
                instance.level_overrides.push(level);
                data_at = next;
            }
        }
        numbering.instances.push(instance);
    }
    numbering
}

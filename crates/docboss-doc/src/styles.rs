//! The stylesheet ([MS-DOC] §2.9.271 STSH) as model styles.
//! [MS-DOC] §2.9.271, §2.9.272, §2.9.258, §2.9.259, §2.9.260, §2.9.336, §2.9.338.

use docboss_model::{
    ParagraphProperties, RunProperties, Style, StyleKind, Styles, TableProperties,
};

use std::collections::{BTreeSet, HashSet};

use crate::bytes::{slice, u16_at, u8_at, utf16};
use crate::props::{apply_chp, apply_pap, CharContext, CharExtra, ParaExtra};

/// A raw style definition: kind, base, next, name and its UPX grpprls.
pub struct RawStyle {
    pub istd: usize,
    pub sti: u16,
    pub kind: StyleKind,
    pub base: Option<usize>,
    pub next: Option<usize>,
    pub name: String,
    pub papx: Vec<u8>,
    pub chpx: Vec<u8>,
}

pub struct Stylesheet {
    pub raw: Vec<Option<RawStyle>>,
    /// Model style id by istd.
    pub ids: Vec<Option<String>>,
    /// Font indices of the default fonts: ascii, east Asian, other.
    pub default_fonts: [u16; 3],
}

fn istd(value: u16) -> Option<usize> {
    let value = value & 0x0FFF;
    (value < 0x0FFE).then_some(usize::from(value))
}

fn sanitize(name: &str) -> String {
    name.split(',')
        .next()
        .unwrap_or("")
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

/// How a stylesheet stores style names and property modifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Version {
    Word97,
    /// Word 6 and 95: 8-bit names in this code page and 1-byte sprms.
    Word6(u32),
}

/// Reads STSH ([MS-DOC] §2.9.271, §2.9.258 STD, §2.9.260 StdfBase).
pub fn parse(bytes: &[u8]) -> Stylesheet {
    parse_as(bytes, Version::Word97, &mut BTreeSet::new())
}

/// Reads a Word 6 or 95 stylesheet: the Word 97 layout with an 8-byte
/// StdfBase, names of a size byte and 8-bit characters, and Word 6 sprms,
/// translated. Codes left out are added to `dropped`.
pub fn parse_word6(bytes: &[u8], code_page: u32, dropped: &mut BTreeSet<u8>) -> Stylesheet {
    parse_as(bytes, Version::Word6(code_page), dropped)
}

fn parse_as(bytes: &[u8], version: Version, dropped: &mut BTreeSet<u8>) -> Stylesheet {
    let cb_stshi = usize::from(u16_at(bytes, 0).unwrap_or(0));
    let stshi = slice(bytes, 2, cb_stshi);
    let count = usize::from(u16_at(stshi, 0).unwrap_or(0));
    let cb_base = usize::from(u16_at(stshi, 2).unwrap_or(10));
    let default_fonts = [
        u16_at(stshi, 12).unwrap_or(0),
        u16_at(stshi, 14).unwrap_or(0),
        u16_at(stshi, 16).unwrap_or(0),
    ];
    let mut at = 2 + cb_stshi;
    let mut raw = Vec::with_capacity(count.min(4096));
    for index in 0..count.min(4094) {
        let Some(cb) = u16_at(bytes, at) else {
            break;
        };
        let std = slice(bytes, at + 2, usize::from(cb));
        at += 2 + usize::from(cb);
        raw.push(parse_std(index, std, cb_base, version, dropped));
    }
    let mut ids: Vec<Option<String>> = Vec::with_capacity(raw.len());
    let mut taken: HashSet<String> = HashSet::with_capacity(raw.len());
    for style in &raw {
        let id = style.as_ref().map(|style| {
            let mut id = sanitize(&style.name);
            if id.is_empty() {
                id = format!("Style{}", style.istd);
            }
            if taken.contains(&id) {
                id = format!("{id}{}", style.istd);
            }
            taken.insert(id.clone());
            id
        });
        ids.push(id);
    }
    Stylesheet {
        raw,
        ids,
        default_fonts,
    }
}

fn parse_std(
    index: usize,
    std: &[u8],
    cb_base: usize,
    version: Version,
    dropped: &mut BTreeSet<u8>,
) -> Option<RawStyle> {
    if std.len() < 8 {
        return None;
    }
    let sti = u16_at(std, 0)? & 0x0FFF;
    let word2 = u16_at(std, 2)?;
    let kind = match word2 & 0xF {
        1 => StyleKind::Paragraph,
        2 => StyleKind::Character,
        3 => StyleKind::Table,
        4 => StyleKind::Numbering,
        _ => return None,
    };
    let base = istd(word2 >> 4);
    let word4 = u16_at(std, 4)?;
    let cupx = usize::from(word4 & 0xF);
    let next = istd(word4 >> 4);
    let name_at = cb_base;
    let (name, mut at) = match version {
        Version::Word97 => {
            let cch = usize::from(u16_at(std, name_at).unwrap_or(0));
            (utf16(std, name_at + 2, cch), name_at + 2 + cch * 2 + 2)
        }
        Version::Word6(code_page) => {
            let cch = usize::from(u8_at(std, name_at).unwrap_or(0));
            let bytes = slice(std, name_at + 1, cch);
            let name = docboss_cfb::codepage::decode(code_page, bytes).0;
            (name, name_at + 1 + cch + 1)
        }
    };
    let mut upxs: Vec<&[u8]> = Vec::with_capacity(cupx);
    for _ in 0..cupx {
        at += at & 1;
        let Some(cb) = u16_at(std, at) else {
            break;
        };
        upxs.push(slice(std, at + 2, usize::from(cb)));
        at += 2 + usize::from(cb);
    }
    let (papx, chpx) = match kind {
        StyleKind::Paragraph => (upxs.first().copied(), upxs.get(1).copied()),
        StyleKind::Character => (None, upxs.first().copied()),
        StyleKind::Table => (upxs.get(1).copied(), upxs.get(2).copied()),
        StyleKind::Numbering => (upxs.first().copied(), None),
    };
    let papx = papx.map_or(&[][..], |p| p.get(2..).unwrap_or(&[]));
    let chpx = chpx.unwrap_or(&[]);
    let (papx, chpx) = match version {
        Version::Word97 => (papx.to_vec(), chpx.to_vec()),
        Version::Word6(_) => (
            crate::word6::translate(papx, dropped),
            crate::word6::translate(chpx, dropped),
        ),
    };
    Some(RawStyle {
        istd: index,
        sti,
        kind,
        base,
        next,
        name,
        papx,
        chpx,
    })
}

impl Stylesheet {
    pub fn id(&self, istd: u16) -> Option<String> {
        self.ids.get(usize::from(istd)).cloned().flatten()
    }

    /// The model stylesheet. Heading styles (sti 1 to 9) get their outline
    /// level; istd 0 and 10 are the default paragraph and character styles.
    pub fn to_model(&self, fonts: &[String]) -> Styles {
        let empty = RunProperties::default();
        let context = CharContext {
            fonts,
            styles: &self.ids,
            base: &empty,
        };
        let mut default_run = RunProperties {
            size: Some(20),
            ..RunProperties::default()
        };
        default_run.fonts.ascii = fonts.get(usize::from(self.default_fonts[0])).cloned();
        default_run.fonts.high_ansi = fonts.get(usize::from(self.default_fonts[2])).cloned();
        default_run.fonts.east_asia = fonts.get(usize::from(self.default_fonts[1])).cloned();
        let mut styles = Styles::new(ParagraphProperties::default(), default_run, Vec::new());
        for raw in self.raw.iter().flatten() {
            let Some(id) = self.id(raw.istd as u16) else {
                continue;
            };
            let mut style = Style::new(id, raw.kind);
            style.name = Some(raw.name.split(',').next().unwrap_or("").to_string());
            style.based_on = raw.base.and_then(|b| self.id(b as u16));
            style.next = raw.next.and_then(|n| self.id(n as u16));
            style.is_default = raw.istd == 0 || raw.istd == 10;
            let mut extra = ParaExtra::default();
            apply_pap(&raw.papx, &mut style.paragraph, &mut extra);
            if let Some(numbering) = crate::props::numbering_ref(&extra) {
                style.paragraph.numbering = Some(numbering);
            }
            if (1..=9).contains(&raw.sti) && style.paragraph.outline_level.is_none() {
                style.paragraph.outline_level = Some((raw.sti - 1) as u8);
            }
            let mut char_extra = CharExtra::default();
            apply_chp(&raw.chpx, &mut style.run, &mut char_extra, &context);
            style.run.style_id = None;
            if raw.kind == StyleKind::Table {
                style.table = Some(TableProperties::default());
            }
            styles.push(style);
        }
        styles
    }
}

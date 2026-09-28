//! The stylesheet ([MS-DOC] §2.9.271 STSH) as model styles.
//! [MS-DOC] §2.9.271, §2.9.272, §2.9.258, §2.9.259, §2.9.260, §2.9.336, §2.9.338.

use docboss_model::{
    ParagraphProperties, RunProperties, Style, StyleKind, Styles, TableProperties,
};

use crate::bytes::{slice, u16_at, utf16};
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

/// Reads STSH ([MS-DOC] §2.9.271, §2.9.258 STD, §2.9.260 StdfBase).
pub fn parse(bytes: &[u8]) -> Stylesheet {
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
        raw.push(parse_std(index, std, cb_base));
    }
    let mut ids: Vec<Option<String>> = Vec::with_capacity(raw.len());
    for style in &raw {
        let id = style.as_ref().map(|style| {
            let mut id = sanitize(&style.name);
            if id.is_empty() {
                id = format!("Style{}", style.istd);
            }
            if ids.iter().flatten().any(|existing| *existing == id) {
                id = format!("{id}{}", style.istd);
            }
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

fn parse_std(index: usize, std: &[u8], cb_base: usize) -> Option<RawStyle> {
    if std.len() < 10 {
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
    let cch = usize::from(u16_at(std, name_at).unwrap_or(0));
    let name = utf16(std, name_at + 2, cch);
    let mut at = name_at + 2 + cch * 2 + 2;
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
    let papx = papx
        .map(|p| p.get(2..).unwrap_or(&[]).to_vec())
        .unwrap_or_default();
    Some(RawStyle {
        istd: index,
        sti,
        kind,
        base,
        next,
        name,
        papx,
        chpx: chpx.unwrap_or(&[]).to_vec(),
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

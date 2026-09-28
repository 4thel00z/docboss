//! Conversion of property modifiers ([MS-DOC] §2.6) into model properties.

use docboss_model::{
    Border, BorderStyle, Borders, Color, Justification, LineRule, NumberingRef, Orientation,
    ParagraphProperties, RunProperties, SectionBreak, SectionProperties, Shading, TabAlignment,
    TabLeader, TabStop, Underline, VerticalAlign,
};

use crate::bytes::{i16_at, u16_at, u32_at, u8_at};
use crate::sprm::{prls, Prl};

/// The 16 colors of an Ico ([MS-DOC] §2.9.119); index 0 is `auto`.
pub fn ico(index: u8) -> Option<Color> {
    const TABLE: [(u8, u8, u8); 17] = [
        (0, 0, 0),
        (0, 0, 0),
        (0, 0, 255),
        (0, 255, 255),
        (0, 255, 0),
        (255, 0, 255),
        (255, 0, 0),
        (255, 255, 0),
        (255, 255, 255),
        (0, 0, 128),
        (0, 128, 128),
        (0, 128, 0),
        (128, 0, 128),
        (128, 0, 0),
        (128, 128, 0),
        (128, 128, 128),
        (192, 192, 192),
    ];
    if index == 0 {
        return None;
    }
    TABLE
        .get(usize::from(index))
        .map(|&(r, g, b)| Color(r, g, b))
}

/// A COLORREF ([MS-DOC] §2.9.43); `None` for cvAuto.
pub fn colorref(value: u32) -> Option<Color> {
    if value >> 24 == 0xFF {
        return None;
    }
    Some(Color(value as u8, (value >> 8) as u8, (value >> 16) as u8))
}

/// A DTTM ([MS-DOC] §2.9.65) as ISO 8601, or `None` when zero.
pub fn dttm(value: u32) -> Option<String> {
    if value == 0 {
        return None;
    }
    let minute = value & 0x3F;
    let hour = (value >> 6) & 0x1F;
    let day = (value >> 11) & 0x1F;
    let month = (value >> 16) & 0x0F;
    let year = 1900 + ((value >> 20) & 0x1FF);
    Some(format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:00Z"
    ))
}

/// A language id as a BCP 47 tag for the common ones, else its hex value.
pub fn language(lid: u16) -> String {
    let tag = match lid {
        0x0409 => "en-US",
        0x0809 => "en-GB",
        0x0C09 => "en-AU",
        0x1009 => "en-CA",
        0x0407 => "de-DE",
        0x0807 => "de-CH",
        0x0C07 => "de-AT",
        0x040C => "fr-FR",
        0x0C0C => "fr-CA",
        0x040A | 0x0C0A => "es-ES",
        0x080A => "es-MX",
        0x0410 => "it-IT",
        0x0413 => "nl-NL",
        0x0419 => "ru-RU",
        0x0411 => "ja-JP",
        0x0804 => "zh-CN",
        0x0404 => "zh-TW",
        0x0412 => "ko-KR",
        0x040D => "he-IL",
        0x0401 => "ar-SA",
        0x0416 => "pt-BR",
        0x0816 => "pt-PT",
        0x0415 => "pl-PL",
        0x041D => "sv-SE",
        0x0406 => "da-DK",
        0x0414 => "nb-NO",
        0x040B => "fi-FI",
        0x0405 => "cs-CZ",
        0x0408 => "el-GR",
        0x041F => "tr-TR",
        0x040E => "hu-HU",
        0x0422 => "uk-UA",
        0x041E => "th-TH",
        0x042A => "vi-VN",
        _ => return format!("{lid:04X}"),
    };
    tag.to_string()
}

fn brc_style(kind: u8) -> BorderStyle {
    match kind {
        0 | 0xFF => BorderStyle::None,
        1 | 5 => BorderStyle::Single,
        2 => BorderStyle::Thick,
        3 => BorderStyle::Double,
        6 => BorderStyle::Dotted,
        7 | 8 | 9 | 22 => BorderStyle::Dashed,
        _ => BorderStyle::Other,
    }
}

/// A Brc80 ([MS-DOC] §2.9.17); `None` for the nil border.
pub fn brc80(bytes: &[u8]) -> Option<Border> {
    if bytes.len() < 4 || bytes == [0xFF; 4] {
        return None;
    }
    let style = brc_style(bytes[1]);
    Some(Border {
        style,
        size: u32::from(bytes[0]),
        space: u32::from(bytes[3] & 0x1F),
        color: ico(bytes[2]),
    })
}

/// A Brc ([MS-DOC] §2.9.16).
pub fn brc(bytes: &[u8]) -> Option<Border> {
    if bytes.len() < 8 || bytes[4..8] == [0xFF; 4] || bytes[4..8] == [0; 4] && bytes[..4] == [0; 4]
    {
        return None;
    }
    let width = u32::from(bytes[4]);
    let size = if bytes[5] >= 0x40 { width * 8 } else { width };
    Some(Border {
        style: brc_style(bytes[5]),
        size,
        space: u32::from(bytes[6] & 0x1F),
        color: colorref(u32_at(bytes, 0).unwrap_or(0xFF00_0000)),
    })
}

/// A Shd ([MS-DOC] §2.9.247): the background color, or `None` for auto.
pub fn shd(bytes: &[u8]) -> Option<Shading> {
    let back = u32_at(bytes, 4)?;
    let pattern = u16_at(bytes, 8).unwrap_or(0);
    if pattern == 0xFFFF {
        return None;
    }
    let fore = u32_at(bytes, 0).and_then(colorref);
    let fill = if pattern == 1 { fore } else { colorref(back) };
    Some(Shading { fill })
}

/// A Shd80 ([MS-DOC] §2.9.248).
pub fn shd80(value: u16) -> Option<Shading> {
    if value == 0xFFFF {
        return None;
    }
    let fore = ico((value & 0x1F) as u8);
    let back = ico(((value >> 5) & 0x1F) as u8);
    let pattern = value >> 10;
    let fill = if pattern == 1 { fore } else { back };
    Some(Shading { fill })
}

/// Character properties that the model does not carry but the story
/// builder needs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CharExtra {
    pub special: bool,
    pub pic_location: Option<u32>,
    pub data: bool,
    pub ole2: bool,
    pub object: bool,
    pub symbol: Option<(u16, u16)>,
    pub deleted: bool,
    pub inserted: bool,
    pub author: Option<u16>,
    pub date: Option<u32>,
    pub author_deleted: Option<u16>,
    pub date_deleted: Option<u32>,
}

/// What a character property needs to know to apply toggles and fonts.
pub struct CharContext<'a> {
    pub fonts: &'a [String],
    pub styles: &'a [Option<String>],
    /// Style-level value of each toggle, for the 0x80 and 0x81 operands.
    pub base: &'a RunProperties,
}

fn toggle(prl: &Prl<'_>, base: Option<bool>) -> Option<bool> {
    match prl.u8() {
        0 => Some(false),
        1 => Some(true),
        0x80 => base,
        0x81 => Some(!base.unwrap_or(false)),
        _ => base,
    }
}

fn font(context: &CharContext<'_>, index: u16) -> Option<String> {
    context.fonts.get(usize::from(index)).cloned()
}

/// Applies a character grpprl ([MS-DOC] §2.6.1).
pub fn apply_chp(
    grpprl: &[u8],
    props: &mut RunProperties,
    extra: &mut CharExtra,
    context: &CharContext<'_>,
) {
    for prl in prls(grpprl) {
        apply_chp_prl(&prl, props, extra, context);
    }
}

fn apply_chp_prl(
    prl: &Prl<'_>,
    props: &mut RunProperties,
    extra: &mut CharExtra,
    context: &CharContext<'_>,
) {
    let base = context.base;
    match prl.sprm {
        0x0800 => extra.deleted = prl.u8() & 1 != 0,
        0x0801 => extra.inserted = prl.u8() & 1 != 0,
        0x4804 => extra.author = Some(prl.u16()),
        0x6805 => extra.date = Some(prl.u32()),
        0x4863 => extra.author_deleted = Some(prl.u16()),
        0x6864 => extra.date_deleted = Some(prl.u32()),
        0x6A03 => extra.pic_location = Some(prl.u32()),
        0x0806 => extra.data = prl.u8() & 1 != 0,
        0x080A => extra.ole2 = prl.u8() & 1 != 0,
        0x0856 => extra.object = prl.u8() & 1 != 0,
        0x0855 => extra.special = prl.u8() & 1 != 0,
        0x6A09 => {
            extra.symbol = Some((
                u16_at(prl.operand, 0).unwrap_or(0),
                u16_at(prl.operand, 2).unwrap_or(0),
            ))
        }
        0x4A30 => {
            props.style_id = context
                .styles
                .get(usize::from(prl.u16()))
                .cloned()
                .flatten()
        }
        0x2A33 => {
            let style_id = props.style_id.take();
            *props = RunProperties {
                style_id,
                ..RunProperties::default()
            };
        }
        0x0835 => props.bold = toggle(prl, base.bold),
        0x0836 => props.italic = toggle(prl, base.italic),
        0x0837 => props.strike = toggle(prl, base.strike),
        0x083A => props.small_caps = toggle(prl, base.small_caps),
        0x083B => props.caps = toggle(prl, base.caps),
        0x083C => props.vanish = toggle(prl, base.vanish),
        0x2A53 => props.double_strike = toggle(prl, base.double_strike),
        0x2A3E => {
            props.underline = Some(match prl.u8() {
                0 => Underline::None,
                1 => Underline::Single,
                2 => Underline::Words,
                3 => Underline::Double,
                4 => Underline::Dotted,
                6 => Underline::Thick,
                7 | 15 | 23 | 39 | 55 => Underline::Dashed,
                11 | 27 | 43 => Underline::Wave,
                _ => Underline::Other,
            })
        }
        0x4A43 => props.size = Some(u32::from(prl.u16())),
        0x2A42 => props.color = Some(ico(prl.u8())),
        0x6870 => props.color = Some(colorref(prl.u32())),
        0x2A0C => props.highlight = ico(prl.u8()),
        0x2A48 => {
            props.vertical_align = Some(match prl.u8() {
                1 => VerticalAlign::Superscript,
                2 => VerticalAlign::Subscript,
                _ => VerticalAlign::Baseline,
            })
        }
        0x4A4F => props.fonts.ascii = font(context, prl.u16()),
        0x4A50 => props.fonts.east_asia = font(context, prl.u16()),
        0x4A51 => props.fonts.high_ansi = font(context, prl.u16()),
        0x4A5E => props.fonts.complex = font(context, prl.u16()),
        0x8840 => props.spacing = Some(i32::from(prl.i16())),
        0x4845 => props.position = Some(i32::from(prl.i16())),
        0x486D | 0x4873 => props.language = Some(language(prl.u16())),
        0x085A => props.right_to_left = Some(prl.u8() & 1 != 0),
        0x4866 => props.shading = shd80(prl.u16()),
        0xCA71 => props.shading = shd(prl.variable()),
        _ => {}
    }
}

/// Paragraph properties the model does not carry but table and list
/// assembly needs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParaExtra {
    pub istd: u16,
    pub in_table: bool,
    pub ttp: bool,
    pub itap: Option<i32>,
    pub inner_cell: bool,
    pub inner_ttp: bool,
    pub ilfo: Option<i16>,
    pub ilvl: Option<u8>,
    pub huge_papx: Option<u32>,
}

impl ParaExtra {
    /// The table depth of the paragraph: 0 outside tables.
    pub fn depth(&self) -> i32 {
        self.itap
            .unwrap_or(if self.in_table { 1 } else { 0 })
            .max(0)
    }
}

fn xas(prl: &Prl<'_>) -> Option<i32> {
    Some(i32::from(prl.i16()))
}

fn tabs(operand: &[u8], props: &mut ParagraphProperties, papx: bool) {
    let deletions = usize::from(u8_at(operand, 1).unwrap_or(0));
    let deleted_at = 2;
    let step = if papx { 2 } else { 4 };
    let positions_deleted: Vec<i32> = (0..deletions)
        .filter_map(|i| i16_at(operand, deleted_at + i * 2).map(i32::from))
        .collect();
    props
        .tabs
        .retain(|stop| !positions_deleted.contains(&stop.position));
    let additions_at = deleted_at + deletions * step;
    let additions = usize::from(u8_at(operand, additions_at).unwrap_or(0));
    let positions_at = additions_at + 1;
    let kinds_at = positions_at + additions * 2;
    for i in 0..additions {
        let Some(position) = i16_at(operand, positions_at + i * 2) else {
            break;
        };
        let kind = u8_at(operand, kinds_at + i).unwrap_or(0);
        let alignment = match kind & 7 {
            1 => TabAlignment::Center,
            2 => TabAlignment::Right,
            3 => TabAlignment::Decimal,
            4 => TabAlignment::Bar,
            _ => TabAlignment::Left,
        };
        let leader = match (kind >> 3) & 7 {
            1 => TabLeader::Dot,
            2 => TabLeader::Hyphen,
            3 => TabLeader::Underscore,
            4 => TabLeader::Heavy,
            5 => TabLeader::MiddleDot,
            _ => TabLeader::None,
        };
        props
            .tabs
            .retain(|stop| stop.position != i32::from(position));
        props.tabs.push(TabStop {
            position: i32::from(position),
            alignment,
            leader,
        });
    }
    props.tabs.sort_by_key(|stop| stop.position);
}

fn set_border(
    props: &mut ParagraphProperties,
    border: Option<Border>,
    side: fn(&mut Borders) -> &mut Option<Border>,
) {
    let borders = props.borders.get_or_insert_with(Borders::default);
    *side(borders) = border;
}

/// Applies a paragraph grpprl ([MS-DOC] §2.6.2). Table sprms in the same
/// grpprl are left to [`crate::table::RowInfo`].
pub fn apply_pap(grpprl: &[u8], props: &mut ParagraphProperties, extra: &mut ParaExtra) {
    for prl in prls(grpprl) {
        apply_pap_prl(&prl, props, extra);
    }
}

fn justification(value: u8) -> Justification {
    match value {
        1 => Justification::Center,
        2 => Justification::Right,
        3 => Justification::Both,
        4 => Justification::Distribute,
        _ => Justification::Left,
    }
}

fn apply_pap_prl(prl: &Prl<'_>, props: &mut ParagraphProperties, extra: &mut ParaExtra) {
    match prl.sprm {
        0x4600 => extra.istd = prl.u16(),
        0x2403 | 0x2461 => props.justification = Some(justification(prl.u8())),
        0x2405 => props.keep_lines = Some(prl.u8() != 0),
        0x2406 => props.keep_next = Some(prl.u8() != 0),
        0x2407 => props.page_break_before = Some(prl.u8() != 0),
        0x2431 => props.widow_control = Some(prl.u8() != 0),
        0x246D => props.contextual_spacing = Some(prl.u8() != 0),
        0x260A => extra.ilvl = Some(prl.u8().min(8)),
        0x460B => extra.ilfo = Some(prl.i16()),
        0x840F | 0x845E => props.indentation.left = xas(prl),
        0x840E | 0x845D => props.indentation.right = xas(prl),
        0x8411 | 0x8460 => {
            let value = i32::from(prl.i16());
            props.indentation.first_line = (value >= 0).then_some(value);
            props.indentation.hanging = (value < 0).then_some(-value);
        }
        0x6412 => {
            let line = u16_at(prl.operand, 0).unwrap_or(240);
            let multiple = u16_at(prl.operand, 2).unwrap_or(1) == 1;
            let (value, rule) = match (line >= 0x8440, multiple) {
                (true, _) => (0x10000 - i32::from(line), LineRule::Exact),
                (false, true) => (i32::from(line), LineRule::Auto),
                (false, false) => (i32::from(line), LineRule::AtLeast),
            };
            props.spacing.line = Some(value);
            props.spacing.line_rule = Some(rule);
        }
        0xA413 => props.spacing.before = Some(i32::from(prl.u16())),
        0xA414 => props.spacing.after = Some(i32::from(prl.u16())),
        0x245B => props.spacing.before_auto = Some(prl.u8() != 0),
        0x245C => props.spacing.after_auto = Some(prl.u8() != 0),
        0xC615 => tabs(prl.operand, props, false),
        0xC60D => tabs(prl.operand, props, true),
        0x2416 => extra.in_table = prl.u8() != 0,
        0x2417 => extra.ttp = prl.u8() != 0,
        0x6649 => extra.itap = Some(prl.u32() as i32),
        0x664A => extra.itap = Some(extra.itap.unwrap_or(0) + prl.u32() as i32),
        0x244B => extra.inner_cell = prl.u8() != 0,
        0x244C => extra.inner_ttp = prl.u8() != 0,
        0x2640 => props.outline_level = Some(prl.u8().min(9)),
        0x2441 => props.bidi = Some(prl.u8() != 0),
        0x442D => props.shading = shd80(prl.u16()),
        0xC64D => props.shading = shd(prl.variable()),
        0x6424 => set_border(props, brc80(prl.operand), |b| &mut b.top),
        0x6425 => set_border(props, brc80(prl.operand), |b| &mut b.left),
        0x6426 => set_border(props, brc80(prl.operand), |b| &mut b.bottom),
        0x6427 => set_border(props, brc80(prl.operand), |b| &mut b.right),
        0x6428 => set_border(props, brc80(prl.operand), |b| &mut b.inside_horizontal),
        0xC64E => set_border(props, brc(prl.variable()), |b| &mut b.top),
        0xC64F => set_border(props, brc(prl.variable()), |b| &mut b.left),
        0xC650 => set_border(props, brc(prl.variable()), |b| &mut b.bottom),
        0xC651 => set_border(props, brc(prl.variable()), |b| &mut b.right),
        0xC652 => set_border(props, brc(prl.variable()), |b| &mut b.inside_horizontal),
        0x6646 => extra.huge_papx = Some(prl.u32()),
        _ => {}
    }
}

/// The paragraph's list reference from its list override index and level.
pub fn numbering_ref(extra: &ParaExtra) -> Option<NumberingRef> {
    let ilfo = extra.ilfo?;
    if ilfo == 0 || ilfo == 0xF801u16 as i16 {
        return Some(NumberingRef {
            num_id: 0,
            level: 0,
        });
    }
    Some(NumberingRef {
        num_id: i64::from(ilfo.unsigned_abs()),
        level: extra.ilvl.unwrap_or(0),
    })
}

/// Word's section defaults ([MS-DOC] §2.6.4): Letter paper, 1.25 inch side
/// margins, 1 inch top and bottom, a new page for each section.
pub fn default_section() -> SectionProperties {
    let mut section = SectionProperties::default();
    section.margins.left = 1800;
    section.margins.right = 1800;
    section.start = SectionBreak::NextPage;
    section
}

/// Applies a section grpprl ([MS-DOC] §2.6.4).
pub fn apply_sep(grpprl: &[u8], section: &mut SectionProperties) {
    let mut column_widths: Vec<(usize, i32)> = Vec::new();
    let mut column_spacings: Vec<(usize, i32)> = Vec::new();
    let mut evenly_spaced = true;
    for prl in prls(grpprl) {
        let value = i32::from(prl.u16());
        let signed = i32::from(prl.i16());
        match prl.sprm {
            0xB01F => section.page_size.width = value,
            0xB020 => section.page_size.height = value,
            0x301D => {
                section.page_size.orientation = if prl.u8() == 2 {
                    Orientation::Landscape
                } else {
                    Orientation::Portrait
                }
            }
            0xB021 => section.margins.left = value,
            0xB022 => section.margins.right = value,
            0x9023 => section.margins.top = signed.abs(),
            0x9024 => section.margins.bottom = signed.abs(),
            0xB025 => section.margins.gutter = value,
            0xB017 => section.margins.header = value,
            0xB018 => section.margins.footer = value,
            0x500B => section.columns.count = u32::from(prl.u16()) + 1,
            0x900C => section.columns.space = value,
            0x3019 => section.columns.separator = prl.u8() != 0,
            0x3005 => evenly_spaced = prl.u8() != 0,
            0xF203 => column_widths.push((
                usize::from(prl.u8()),
                i32::from(i16_at(prl.operand, 1).unwrap_or(0)),
            )),
            0xF204 => column_spacings.push((
                usize::from(prl.u8()),
                i32::from(i16_at(prl.operand, 1).unwrap_or(0)),
            )),
            0x3009 => {
                section.start = match prl.u8() {
                    0 => SectionBreak::Continuous,
                    1 => SectionBreak::NextColumn,
                    3 => SectionBreak::EvenPage,
                    4 => SectionBreak::OddPage,
                    _ => SectionBreak::NextPage,
                }
            }
            0x300A => section.title_page = prl.u8() != 0,
            0x3011 => {
                if prl.u8() != 0 && section.page_number_start.is_none() {
                    section.page_number_start = Some(1);
                }
            }
            0x501C => section.page_number_start = Some(u32::from(prl.u16())),
            0x300E => section.page_number_format = Some(crate::lists::format(prl.u8())),
            0x7044 => section.page_number_start = Some(prl.u32()),
            _ => {}
        }
    }
    if evenly_spaced || column_widths.is_empty() {
        return;
    }
    let count = section.columns.count as usize;
    section.columns.widths = (0..count)
        .map(|i| {
            let width = column_widths
                .iter()
                .rev()
                .find(|(c, _)| *c == i)
                .map_or(0, |(_, w)| *w);
            let space = column_spacings
                .iter()
                .rev()
                .find(|(c, _)| *c == i)
                .map_or(0, |(_, s)| *s);
            (width, space)
        })
        .collect();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// [MS-DOC] §2.9.65 DTTM packs minutes, hours, day, month and years
    /// since 1900.
    #[test]
    fn dttm_unpacks() {
        let value = 30 | (14 << 6) | (5 << 11) | (3 << 16) | (124 << 20);
        assert_eq!(dttm(value).as_deref(), Some("2024-03-05T14:30:00Z"));
        assert_eq!(dttm(0), None);
    }

    /// [MS-DOC] §2.9.43 COLORREF stores red in the low byte; fAuto 0xFF.
    #[test]
    fn colorref_is_bgr_with_auto() {
        assert_eq!(colorref(0x0000_80FF), Some(Color(0xFF, 0x80, 0)));
        assert_eq!(colorref(0xFF00_0000), None);
    }

    /// [MS-DOC] §2.9.146 LSPD: 0x8440 and above is an exact height.
    #[test]
    fn line_spacing_rules() {
        let mut props = ParagraphProperties::default();
        let mut extra = ParaExtra::default();
        apply_pap(&[0x12, 0x64, 0xE0, 0xFE, 0, 0], &mut props, &mut extra);
        assert_eq!(
            (props.spacing.line, props.spacing.line_rule),
            (Some(288), Some(LineRule::Exact))
        );
        apply_pap(&[0x12, 0x64, 0xE0, 0x01, 1, 0], &mut props, &mut extra);
        assert_eq!(
            (props.spacing.line, props.spacing.line_rule),
            (Some(480), Some(LineRule::Auto))
        );
    }

    /// [MS-DOC] §2.6.1 toggle operands 0x80 and 0x81 are relative to the
    /// style's value.
    #[test]
    fn toggles_relative_to_style() {
        let base = RunProperties {
            bold: Some(true),
            ..RunProperties::default()
        };
        let context = CharContext {
            fonts: &[],
            styles: &[],
            base: &base,
        };
        let mut props = RunProperties::default();
        let mut extra = CharExtra::default();
        apply_chp(&[0x35, 0x08, 0x81], &mut props, &mut extra, &context);
        assert_eq!(props.bold, Some(false));
        apply_chp(&[0x35, 0x08, 0x80], &mut props, &mut extra, &context);
        assert_eq!(props.bold, Some(true));
    }
}

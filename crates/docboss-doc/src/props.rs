//! Conversion of property modifiers ([MS-DOC] §2.6) into model properties.

use docboss_model::{
    Border, BorderStyle, Borders, Color, DrawingPosition, DropCap, FrameProperties, Justification,
    LineRule, NumberingRef, Orientation, PageBorderDisplay, PageBorderOffset, ParagraphProperties,
    PositionAlign, PositionBase, RunProperties, SectionBreak, SectionProperties, Shading,
    TabAlignment, TabLeader, TabStop, Underline, VerticalAlign, WrapKind,
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

/// [MS-DOC] §2.9.22: a BrcType as a border style.
fn brc_style(kind: u8) -> BorderStyle {
    match kind {
        0 | 0xFF => BorderStyle::None,
        1 | 5 => BorderStyle::Single,
        2 => BorderStyle::Thick,
        3 => BorderStyle::Double,
        6 => BorderStyle::Dotted,
        7 => BorderStyle::Dashed,
        8 => BorderStyle::DotDash,
        9 => BorderStyle::DotDotDash,
        22 => BorderStyle::DashSmallGap,
        23 => BorderStyle::DashDotStroked,
        0x40..=0xE3 => BorderStyle::Art,
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
        shadow: bytes[3] & 0x20 != 0,
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
        shadow: bytes[6] & 0x20 != 0,
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
    Some(Shading {
        fill: pattern_fill(pattern, fore, colorref(back)),
    })
}

/// A Shd80 ([MS-DOC] §2.9.248).
pub fn shd80(value: u16) -> Option<Shading> {
    if value == 0xFFFF {
        return None;
    }
    let fore = ico((value & 0x1F) as u8);
    let back = ico(((value >> 5) & 0x1F) as u8);
    Some(Shading {
        fill: pattern_fill(value >> 10, fore, back),
    })
}

/// [MS-DOC] §2.9.121 Ipat: a solid pattern fills with the foreground; a
/// percentage pattern with the foreground (black when auto) mixed into the
/// background (white when auto) by its percentage, as LibreOffice paints
/// it; stripes and crosses keep the background.
fn pattern_fill(pattern: u16, fore: Option<Color>, back: Option<Color>) -> Option<Color> {
    let percent: u16 = match pattern {
        1 => return fore,
        2 => 5,
        3 => 10,
        4 => 20,
        5 => 25,
        6 => 30,
        7 => 40,
        8 => 50,
        9 => 60,
        0x0A => 70,
        0x0B => 75,
        0x0C => 80,
        0x0D => 90,
        0x25 => 12,
        0x26 => 15,
        _ => return back,
    };
    let Color(fr, fg, fb) = fore.unwrap_or(Color(0, 0, 0));
    let Color(br, bg, bb) = back.unwrap_or(Color(255, 255, 255));
    let mix =
        |f: u8, b: u8| ((u16::from(f) * percent + u16::from(b) * (100 - percent)) / 100) as u8;
    Some(Color(mix(fr, br), mix(fg, bg), mix(fb, bb)))
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
        0x085C => props.bold_complex = toggle(prl, base.bold_complex),
        0x085D => props.italic_complex = toggle(prl, base.italic_complex),
        0x4A61 => props.size_complex = Some(u32::from(prl.u16())),
        0x0882 => props.complex_script = toggle(prl, base.complex_script),
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
    /// The old-style list of a Word 6 paragraph: its ANLD and its
    /// sprmPNLvlAnm level.
    pub anld: Option<Anld>,
    pub anld_level: Option<u8>,
}

/// An ANLD: the numbering of an old-style (Word 6) numbered or bulleted
/// paragraph.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct Anld {
    pub nfc: u8,
    /// The label's text before and after the number.
    pub before: String,
    pub after: String,
    pub jc: u8,
    pub bold: bool,
    pub italic: bool,
    pub font: Option<u16>,
    /// The label size in half-points, when stated.
    pub size: Option<u16>,
    pub start: u16,
}

impl Anld {
    /// Reads a Word 97 form ANLD: nfc, cxchTextBefore, cxchTextAfter, the
    /// jc and formatting flags, ftc, hps, iStartAt, then 32 UTF-16 label
    /// characters.
    pub fn parse(bytes: &[u8]) -> Option<Anld> {
        let nfc = u8_at(bytes, 0)?;
        let before = usize::from(u8_at(bytes, 1)?).min(32);
        let after = usize::from(u8_at(bytes, 2)?).clamp(before, 32);
        let flags = u8_at(bytes, 3)?;
        let styled = u8_at(bytes, 4)?;
        let chars: Vec<u16> = (0..32)
            .map(|i| u16_at(bytes, 20 + i * 2).unwrap_or(0))
            .collect();
        let text = |range: std::ops::Range<usize>| String::from_utf16_lossy(&chars[range]);
        let font = i16_at(bytes, 6).filter(|f| *f >= 0).map(|f| f as u16);
        Some(Anld {
            nfc,
            before: text(0..before),
            after: text(before..after),
            jc: flags & 3,
            bold: flags & 0x10 != 0 && styled & 0x08 != 0,
            italic: flags & 0x20 != 0 && styled & 0x10 != 0,
            font: font.filter(|_| nfc == 23),
            size: u16_at(bytes, 8).filter(|s| *s > 0),
            start: u16_at(bytes, 10).unwrap_or(1),
        })
    }
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

const TWIPS_TO_EMU: i64 = 635;

/// A frame as a positioned paragraph starts: at the top margin and the left
/// of the column, sized to its content, text wrapping around it.
fn default_frame() -> FrameProperties {
    FrameProperties {
        width: None,
        height: 0,
        height_rule: LineRule::Auto,
        horizontal: DrawingPosition::offset(PositionBase::Column, 0),
        vertical: DrawingPosition::offset(PositionBase::Margin, 0),
        wrap: WrapKind::Square,
        h_space: 0,
        v_space: 0,
        drop_cap: DropCap::None,
        lines: 1,
    }
}

/// The bases a PositionCodeOperand ([MS-DOC] §2.9.208) gives the frame
/// or table position: pcVert, then pcHorz; `None` for "not positioned".
pub fn position_code(value: u8) -> (Option<PositionBase>, Option<PositionBase>) {
    let vertical = match (value >> 4) & 3 {
        0 => Some(PositionBase::Margin),
        1 => Some(PositionBase::Page),
        2 => Some(PositionBase::Paragraph),
        _ => None,
    };
    let horizontal = match (value >> 6) & 3 {
        0 => Some(PositionBase::Column),
        1 => Some(PositionBase::Margin),
        2 => Some(PositionBase::Page),
        _ => None,
    };
    (vertical, horizontal)
}

/// An XAS_plusOne or YAS_plusOne position ([MS-DOC] §2.9.351, §2.9.357)
/// over `position`: the special values align (ST_XAlign, ST_YAlign), any
/// other is the offset plus one. A vertical 0 is inline: at the paragraph.
pub fn plus_one_position(value: i16, horizontal: bool, position: &mut DrawingPosition) {
    let aligns: &[(i16, PositionAlign)] = match horizontal {
        true => &[
            (0, PositionAlign::Start),
            (-4, PositionAlign::Center),
            (-8, PositionAlign::End),
            (-12, PositionAlign::Inside),
            (-16, PositionAlign::Outside),
        ],
        false => &[
            (-4, PositionAlign::Start),
            (-8, PositionAlign::Center),
            (-12, PositionAlign::End),
            (-16, PositionAlign::Inside),
            (-20, PositionAlign::Outside),
        ],
    };
    position.align = aligns.iter().find(|(v, _)| *v == value).map(|(_, a)| *a);
    position.offset = match position.align {
        Some(_) => 0,
        None => (i64::from(value) - 1).max(-31_680) * TWIPS_TO_EMU,
    };
    if !horizontal && value == 0 {
        position.base = PositionBase::Paragraph;
        position.offset = 0;
    }
}

/// The frame sprms: sprmPPc, sprmPDxaAbs, sprmPDyaAbs, sprmPDxaWidth,
/// sprmPWr, sprmPWHeightAbs, sprmPDcs, sprmPDyaFromText, sprmPDxaFromText.
/// [MS-DOC] §2.6.2, §2.9.345, §2.9.51.
fn apply_frame_prl(prl: &Prl<'_>, frame: &mut FrameProperties) {
    match prl.sprm {
        0x261B => {
            let (vertical, horizontal) = position_code(prl.u8());
            if let Some(base) = vertical {
                frame.vertical.base = base;
            }
            if let Some(base) = horizontal {
                frame.horizontal.base = base;
            }
        }
        0x8418 => plus_one_position(prl.i16(), true, &mut frame.horizontal),
        0x8419 => plus_one_position(prl.i16(), false, &mut frame.vertical),
        0x841A => frame.width = Some(i32::from(prl.u16())).filter(|w| *w > 0),
        0x2423 => {
            frame.wrap = match prl.u8() {
                1 => WrapKind::TopAndBottom,
                3 => WrapKind::None,
                4 => WrapKind::Tight,
                5 => WrapKind::Through,
                _ => WrapKind::Square,
            }
        }
        0x442B => {
            let value = prl.u16();
            frame.height = i32::from(value & 0x7FFF);
            frame.height_rule = match (frame.height, value & 0x8000 != 0) {
                (0, _) => LineRule::Auto,
                (_, true) => LineRule::AtLeast,
                (_, false) => LineRule::Exact,
            };
        }
        0x442C => {
            let value = prl.u16();
            frame.drop_cap = match value & 7 {
                1 => DropCap::Drop,
                2 => DropCap::Margin,
                _ => DropCap::None,
            };
            frame.lines = u32::from((value >> 3) & 0x1F).clamp(1, 10);
        }
        0x842E => frame.v_space = i32::from(prl.u16()).min(31_680),
        0x842F => frame.h_space = i32::from(prl.u16()).min(31_680),
        _ => {}
    }
}

fn apply_pap_prl(prl: &Prl<'_>, props: &mut ParagraphProperties, extra: &mut ParaExtra) {
    let positions = match prl.sprm {
        0x261B => prl.u8() & 0xF0 != 0xF0,
        0x8418 | 0x8419 | 0x841A | 0x442B => true,
        0x442C => prl.u16() & 7 != 0,
        0x2423 | 0x842E | 0x842F => false,
        _ => {
            apply_paragraph_prl(prl, props, extra);
            return;
        }
    };
    if positions {
        apply_frame_prl(prl, props.frame.get_or_insert_with(default_frame));
        return;
    }
    if let Some(frame) = props.frame.as_mut() {
        apply_frame_prl(prl, frame);
    }
}

fn apply_paragraph_prl(prl: &Prl<'_>, props: &mut ParagraphProperties, extra: &mut ParaExtra) {
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
        // [MS-DOC] §2.9.146: sprmPDyaLine's LSPD.
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
        0xC63E => extra.anld = Anld::parse(prl.variable()),
        0x263D => extra.anld_level = Some(prl.u8()),
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

/// Sets one side of the section's page borders: 0 top, 1 left, 2 bottom,
/// 3 right, in the order of sprmSBrcTop80 to sprmSBrcRight80 and
/// sprmSBrcTop to sprmSBrcRight ([MS-DOC] §2.6.4).
fn page_border_side(section: &mut SectionProperties, side: u16, border: Option<Border>) {
    let borders = section.page_borders.get_or_insert_with(Default::default);
    let slot = match side {
        0 => &mut borders.sides.top,
        1 => &mut borders.sides.left,
        2 => &mut borders.sides.bottom,
        _ => &mut borders.sides.right,
    };
    *slot = border;
}

/// Applies a section grpprl ([MS-DOC] §2.6.4): page size, margins,
/// columns, numbering and the page borders with their SPgbPropOperand
/// ([MS-DOC] §2.9.255, §2.9.184, §2.9.185, §2.9.186, §2.9.21).
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
            0x702B..=0x702E => {
                let border = brc80(prl.operand);
                page_border_side(section, prl.sprm - 0x702B, border);
            }
            0xD234..=0xD237 => {
                let border = brc(prl.variable());
                page_border_side(section, prl.sprm - 0xD234, border);
            }
            0x522F => {
                let value = prl.u16();
                let borders = section.page_borders.get_or_insert_with(Default::default);
                borders.display = match value & 0x7 {
                    1 => PageBorderDisplay::FirstPage,
                    2 => PageBorderDisplay::NotFirstPage,
                    _ => PageBorderDisplay::AllPages,
                };
                borders.behind_text = (value >> 3) & 0x3 == 1;
                borders.offset_from = match (value >> 5) & 0x7 {
                    1 => PageBorderOffset::Page,
                    _ => PageBorderOffset::Text,
                };
            }
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

    /// [MS-DOC] §2.9.121, §2.9.247, §2.9.248: percentage patterns mix their
    /// colors; clear keeps the background and solid takes the foreground.
    #[test]
    fn shading_patterns_mix_their_colors() {
        let pct25 = 0x0005u16 << 10;
        assert_eq!(
            shd80(pct25).and_then(|s| s.fill),
            Some(Color(191, 191, 191))
        );
        assert_eq!(
            shd80(1 << 10 | 6).and_then(|s| s.fill),
            Some(Color(255, 0, 0))
        );
        assert_eq!(
            shd80(8 << 5).and_then(|s| s.fill),
            Some(Color(255, 255, 255))
        );
        let mut red_on_white = [0u8; 10];
        red_on_white[0..4].copy_from_slice(&0x0000_00FFu32.to_le_bytes());
        red_on_white[4..8].copy_from_slice(&0x00FF_FFFFu32.to_le_bytes());
        red_on_white[8] = 8;
        assert_eq!(
            shd(&red_on_white).and_then(|s| s.fill),
            Some(Color(255, 127, 127))
        );
    }

    /// The frame sprms of a positioned paragraph: sprmPPc against the page,
    /// a centered sprmPDxaAbs, sprmPDyaAbs 1 inch down, a minimum height
    /// from sprmPWHeightAbs and a three-line drop cap from sprmPDcs.
    /// [MS-DOC] §2.6.2, §2.9.208, §2.9.351, §2.9.357, §2.9.345, §2.9.51.
    #[test]
    fn frame_sprms_position_a_paragraph() {
        let mut grpprl = vec![0x1B, 0x26, 0x90];
        grpprl.extend([0x18, 0x84, 0xFC, 0xFF]);
        grpprl.extend([0x19, 0x84, 0xA1, 0x05]);
        grpprl.extend([0x2B, 0x44, 0xD0, 0x82]);
        grpprl.extend([0x2C, 0x44, 0x19, 0x00]);
        let mut props = ParagraphProperties::default();
        apply_pap(&grpprl, &mut props, &mut ParaExtra::default());
        let frame = props.frame.expect("the sprms make a frame");
        assert_eq!(frame.horizontal.base, PositionBase::Page);
        assert_eq!(frame.horizontal.align, Some(PositionAlign::Center));
        assert_eq!(frame.vertical.base, PositionBase::Page);
        assert_eq!(frame.vertical.offset, 1440 * 635);
        assert_eq!((frame.height, frame.height_rule), (720, LineRule::AtLeast));
        assert_eq!((frame.drop_cap, frame.lines), (DropCap::Drop, 3));
        let mut plain = ParagraphProperties::default();
        apply_pap(&[0x23, 0x24, 0x02], &mut plain, &mut ParaExtra::default());
        assert!(plain.frame.is_none());
    }

    /// [MS-DOC] §2.9.17 and §2.9.16: the fShadow bit sits above dptSpace.
    #[test]
    fn border_shadow_bits() {
        assert!(brc80(&[8, 1, 1, 0x20 | 3]).unwrap().shadow);
        assert!(!brc80(&[8, 1, 1, 3]).unwrap().shadow);
        assert!(brc(&[0, 0, 0, 0, 8, 1, 0x24, 0]).unwrap().shadow);
    }

    /// [MS-DOC] §2.9.22: the dashed BrcType values.
    #[test]
    fn dashed_border_types() {
        assert_eq!(brc_style(6), BorderStyle::Dotted);
        assert_eq!(brc_style(7), BorderStyle::Dashed);
        assert_eq!(brc_style(8), BorderStyle::DotDash);
        assert_eq!(brc_style(9), BorderStyle::DotDotDash);
        assert_eq!(brc_style(0x16), BorderStyle::DashSmallGap);
        assert_eq!(brc_style(0x17), BorderStyle::DashDotStroked);
        assert_eq!(brc_style(0x50), BorderStyle::Art);
    }

    /// [MS-DOC] §2.9.65 DTTM packs minutes, hours, day, month and years
    /// since 1900.
    /// Page borders from sprmSBrcTop80, sprmSBrcRight and sprmSPgbProp.
    /// [MS-DOC] §2.6.4, §2.9.255, §2.9.184, §2.9.185, §2.9.186, §2.9.17, §2.9.21.
    #[test]
    fn page_borders_from_section_sprms() {
        let mut section = default_section();
        let grpprl = [
            0x2B, 0x70, 12, 7, 6, 24, 0x37, 0xD2, 8, 0, 0, 0xFF, 0, 16, 3, 5, 0, 0x2F, 0x52, 0x29,
            0x00,
        ];
        apply_sep(&grpprl, &mut section);
        let borders = section.page_borders.unwrap();
        let top = borders.sides.top.unwrap();
        assert_eq!(
            (top.style, top.size, top.space),
            (BorderStyle::Dashed, 12, 24)
        );
        let right = borders.sides.right.unwrap();
        assert_eq!(right.style, BorderStyle::Double);
        assert_eq!((right.size, right.space), (16, 5));
        assert_eq!(right.color, Some(Color(0, 0, 0xFF)));
        assert_eq!(borders.display, PageBorderDisplay::FirstPage);
        assert!(borders.behind_text);
        assert_eq!(borders.offset_from, PageBorderOffset::Page);
        assert_eq!(borders.sides.left, None);
    }

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

    /// [MS-DOC] §2.6.1: sprmCFBoldBi, sprmCFItalicBi, sprmCHpsBi and
    /// sprmCFComplexScripts set the complex script formatting.
    #[test]
    fn complex_script_character_properties() {
        let base = RunProperties::default();
        let context = CharContext {
            fonts: &[],
            styles: &[],
            base: &base,
        };
        let mut props = RunProperties::default();
        let mut extra = CharExtra::default();
        let grpprl = [
            0x5C, 0x08, 0x01, 0x5D, 0x08, 0x00, 0x61, 0x4A, 0x1C, 0x00, 0x82, 0x08, 0x01,
        ];
        apply_chp(&grpprl, &mut props, &mut extra, &context);
        assert_eq!(props.bold_complex, Some(true));
        assert_eq!(props.italic_complex, Some(false));
        assert_eq!(props.size_complex, Some(28));
        assert_eq!(props.complex_script, Some(true));
    }
}

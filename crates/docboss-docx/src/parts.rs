//! The non-story parts: styles, numbering, settings, fonts, theme and the
//! document properties.

use std::sync::Arc;

use docboss_model::{
    AbstractNumbering, Diagnostic, FontEntry, Level, Metadata, NumberFormat, Numbering,
    NumberingInstance, ParagraphProperties, RunProperties, Settings, Style, StyleKind, Styles,
};
use docboss_xml::{Ns, Reader};

use crate::package::{Package, Relationships};
use crate::props::{justification, paragraph_properties, run_properties, table_properties, Theme};
use crate::xml::{attr, children, int_attr, on_off, root, twips_attr, u32_attr, val};

/// `w:styles` (ECMA-376 Part 1 §17.7).
pub fn styles(text: &str, theme: &Theme) -> Styles {
    let mut reader = Reader::new(text);
    let mut default_paragraph = ParagraphProperties::default();
    let mut default_run = RunProperties::default();
    let mut list = Vec::new();
    if root(&mut reader).is_none() {
        return Styles::default();
    }
    children(&mut reader, |reader, e| match e.local {
        "docDefaults" => children(reader, |reader, e| match e.local {
            "rPrDefault" => children(reader, |reader, e| {
                if e.local == "rPr" {
                    default_run = run_properties(reader, theme);
                }
            }),
            "pPrDefault" => children(reader, |reader, e| {
                if e.local == "pPr" {
                    default_paragraph = paragraph_properties(reader, theme).properties;
                }
            }),
            _ => {}
        }),
        "style" => {
            let kind = match attr(&e, "type").as_deref() {
                Some("character") => StyleKind::Character,
                Some("table") => StyleKind::Table,
                Some("numbering") => StyleKind::Numbering,
                _ => StyleKind::Paragraph,
            };
            let Some(id) = attr(&e, "styleId") else {
                return;
            };
            let mut style = Style::new(id.into_owned(), kind);
            style.is_default = e
                .attr_raw(Ns::W, "default")
                .is_some_and(|v| crate::xml::on_off_value(Some(v)));
            children(reader, |reader, e| match e.local {
                "name" => style.name = val(&e).map(Into::into),
                "basedOn" => style.based_on = val(&e).map(Into::into),
                "next" => style.next = val(&e).map(Into::into),
                "pPr" => style.paragraph = paragraph_properties(reader, theme).properties,
                "rPr" => style.run = run_properties(reader, theme),
                "tblPr" => style.table = Some(table_properties(reader)),
                _ => {}
            });
            list.push(style);
        }
        _ => {}
    });
    Styles::new(default_paragraph, default_run, list)
}

pub(crate) fn number_format(value: &str) -> NumberFormat {
    match value {
        "decimal" => NumberFormat::Decimal,
        "decimalZero" => NumberFormat::DecimalZero,
        "upperRoman" => NumberFormat::UpperRoman,
        "lowerRoman" => NumberFormat::LowerRoman,
        "upperLetter" => NumberFormat::UpperLetter,
        "lowerLetter" => NumberFormat::LowerLetter,
        "ordinal" => NumberFormat::Ordinal,
        "bullet" => NumberFormat::Bullet,
        "none" => NumberFormat::None,
        other => NumberFormat::Other(other.to_string()),
    }
}

/// Bullet characters stored in the Symbol and Wingdings private use range
/// (U+F000 to U+F0FF), mapped to their Unicode equivalents.
fn bullet_text(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '\u{F0B7}' | '\u{F0A1}' => '\u{2022}',
            '\u{F0A7}' | '\u{F06E}' => '\u{25AA}',
            '\u{F0FC}' => '\u{2713}',
            '\u{F0D8}' => '\u{27A2}',
            '\u{F076}' => '\u{2756}',
            '\u{F0A8}' => '\u{25A1}',
            '\u{F0E0}' => '\u{27A2}',
            other => other,
        })
        .collect()
}

fn level(reader: &mut Reader<'_>, index: u8, theme: &Theme) -> Level {
    let mut level = Level {
        level: index,
        text: String::new(),
        ..Level::default()
    };
    let mut text_seen = false;
    children(reader, |reader, e| match e.local {
        "start" => level.start = u32_attr(&e, "val").unwrap_or(1),
        "numFmt" => level.format = val(&e).map_or(NumberFormat::Decimal, |v| number_format(&v)),
        "lvlText" => {
            text_seen = true;
            level.text = val(&e).map(|v| bullet_text(&v)).unwrap_or_default();
        }
        "lvlJc" => level.justification = val(&e).and_then(|v| justification(&v)),
        "lvlRestart" => level.restart_after = int_attr(&e, "val").map(|n| n.clamp(0, 9) as u8),
        "isLgl" => level.legal = on_off(&e),
        "pPr" => level.paragraph = paragraph_properties(reader, theme).properties,
        "rPr" => level.run = run_properties(reader, theme),
        _ => {}
    });
    if !text_seen && level.format != NumberFormat::Bullet {
        level.text = format!("%{}.", index + 1);
    }
    level
}

fn level_index(e: &docboss_xml::Element<'_>) -> u8 {
    int_attr(e, "ilvl").unwrap_or(0).clamp(0, 8) as u8
}

/// `w:numbering` (ECMA-376 Part 1 §17.9), with the style links of
/// abstract definitions that borrow their levels from a numbering style.
pub fn numbering(text: &str, theme: &Theme) -> (Numbering, Vec<(i64, String)>) {
    let mut reader = Reader::new(text);
    let mut numbering = Numbering::default();
    let mut links = Vec::new();
    if root(&mut reader).is_none() {
        return (numbering, links);
    }
    children(&mut reader, |reader, e| match e.local {
        "abstractNum" => {
            let id = int_attr(&e, "abstractNumId").unwrap_or(-1);
            let mut definition = AbstractNumbering {
                id,
                levels: Vec::new(),
            };
            children(reader, |reader, e| match e.local {
                "lvl" => definition
                    .levels
                    .push(level(reader, level_index(&e), theme)),
                "numStyleLink" => links.extend(val(&e).map(|style| (id, style.into_owned()))),
                _ => {}
            });
            numbering.abstracts.push(definition);
        }
        "num" => {
            let num_id = int_attr(&e, "numId").unwrap_or(-1);
            let mut instance = NumberingInstance {
                num_id,
                abstract_id: -1,
                ..NumberingInstance::default()
            };
            children(reader, |reader, e| match e.local {
                "abstractNumId" => instance.abstract_id = int_attr(&e, "val").unwrap_or(-1),
                "lvlOverride" => {
                    let index = level_index(&e);
                    children(reader, |reader, e| match e.local {
                        "startOverride" => instance
                            .start_overrides
                            .push((index, u32_attr(&e, "val").unwrap_or(1))),
                        "lvl" => instance.level_overrides.push(level(reader, index, theme)),
                        _ => {}
                    });
                }
                _ => {}
            });
            numbering.instances.push(instance);
        }
        _ => {}
    });
    (numbering, links)
}

/// Gives abstract definitions linked to a numbering style (ECMA-376 Part 1
/// §17.9.21) the levels of the definition that style points to.
pub fn resolve_style_links(numbering: &mut Numbering, links: &[(i64, String)], styles: &Styles) {
    for (abstract_id, style_id) in links {
        let target = styles
            .get(style_id)
            .and_then(|style| style.paragraph.numbering)
            .and_then(|reference| numbering.instance(reference.num_id))
            .map(|instance| instance.abstract_id)
            .filter(|target| target != abstract_id);
        let Some(levels) = target
            .and_then(|target| numbering.abstracts.iter().find(|a| a.id == target))
            .map(|definition| definition.levels.clone())
        else {
            continue;
        };
        if let Some(definition) = numbering
            .abstracts
            .iter_mut()
            .find(|a| a.id == *abstract_id && a.levels.is_empty())
        {
            definition.levels = levels;
        }
    }
}

/// `w:settings` (ECMA-376 Part 1 §17.15).
pub fn settings(text: &str) -> Settings {
    let mut reader = Reader::new(text);
    let mut settings = Settings::default();
    if root(&mut reader).is_none() {
        return settings;
    }
    children(&mut reader, |_, e| match e.local {
        "defaultTabStop" => {
            settings.default_tab_stop = twips_attr(&e, "val").filter(|&t| t > 0).unwrap_or(720)
        }
        "evenAndOddHeaders" => settings.even_and_odd_headers = on_off(&e),
        _ => {}
    });
    settings
}

/// `a:theme` font scheme (ECMA-376 Part 1 §20.1.4.1.18).
pub fn theme(text: &str) -> Theme {
    let mut reader = Reader::new(text);
    let mut theme = Theme::default();
    fn walk(reader: &mut Reader<'_>, theme: &mut Theme) {
        children(reader, |reader, e| {
            let major = match e.local {
                "majorFont" => true,
                "minorFont" => false,
                _ => return walk(reader, theme),
            };
            let scheme = if major {
                &mut theme.major
            } else {
                &mut theme.minor
            };
            children(reader, |_, face| {
                let slot = match face.local {
                    "latin" => 0,
                    "ea" => 1,
                    "cs" => 2,
                    _ => return,
                };
                scheme[slot] = face
                    .attr(Ns::NONE, "typeface")
                    .map(Into::into)
                    .filter(|f: &String| !f.is_empty());
            });
        });
    }
    if root(&mut reader).is_some() {
        walk(&mut reader, &mut theme);
    }
    theme
}

/// Core properties (ECMA-376 Part 2 §8.3) and extended properties
/// (ECMA-376 Part 1 §22.2) merged into one set.
pub fn properties(core: Option<&str>, app: Option<&str>) -> Metadata {
    let mut metadata = Metadata::default();
    if let Some(text) = core {
        let mut reader = Reader::new(text);
        if root(&mut reader).is_some() {
            children(&mut reader, |reader, e| {
                let slot = match e.local {
                    "title" => &mut metadata.title,
                    "subject" => &mut metadata.subject,
                    "creator" => &mut metadata.creator,
                    "keywords" => &mut metadata.keywords,
                    "description" => &mut metadata.description,
                    "lastModifiedBy" => &mut metadata.last_modified_by,
                    "revision" => &mut metadata.revision,
                    "created" => &mut metadata.created,
                    "modified" => &mut metadata.modified,
                    "category" => &mut metadata.category,
                    _ => return,
                };
                let text = reader.read_text();
                let text = text.trim();
                *slot = (!text.is_empty()).then(|| text.to_string());
            });
        }
    }
    if let Some(text) = app {
        let mut reader = Reader::new(text);
        if root(&mut reader).is_some() {
            children(&mut reader, |reader, e| {
                let text = reader.read_text();
                let text = text.trim();
                let owned = (!text.is_empty()).then(|| text.to_string());
                match e.local {
                    "Application" => metadata.application = owned,
                    "Company" => metadata.company = owned,
                    "Pages" => metadata.pages = text.parse().ok(),
                    "Words" => metadata.words = text.parse().ok(),
                    "Characters" => metadata.characters = text.parse().ok(),
                    _ => {}
                }
            });
        }
    }
    metadata
}

/// Reverses the font obfuscation of ECMA-376 Part 1 §17.8.1: the first 32
/// bytes are XORed with the GUID of `w:fontKey`, its bytes read from the
/// last hex pair to the first.
pub fn deobfuscate(data: &mut [u8], key: &str) -> bool {
    let hex: String = key.chars().filter(char::is_ascii_hexdigit).collect();
    if hex.len() != 32 {
        return false;
    }
    let mut guid = [0u8; 16];
    for (i, byte) in guid.iter_mut().enumerate() {
        let at = 30 - 2 * i;
        let Ok(value) = u8::from_str_radix(&hex[at..at + 2], 16) else {
            return false;
        };
        *byte = value;
    }
    for (i, byte) in data.iter_mut().take(32).enumerate() {
        *byte ^= guid[i % 16];
    }
    true
}

/// `w:fonts` (ECMA-376 Part 1 §17.8.3) with embedded fonts loaded through
/// the font table's relationships.
pub fn fonts(
    text: &str,
    rels: &Relationships,
    package: &Package<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<FontEntry> {
    let mut reader = Reader::new(text);
    let mut list = Vec::new();
    if root(&mut reader).is_none() {
        return list;
    }
    children(&mut reader, |reader, e| {
        if e.local != "font" {
            return;
        }
        let mut entry = FontEntry {
            name: attr(&e, "name").unwrap_or_default().into_owned(),
            ..FontEntry::default()
        };
        children(reader, |_, e| {
            let slot = match e.local {
                "altName" => return entry.alt_name = val(&e).map(Into::into),
                "family" => return entry.family = val(&e).map(Into::into),
                "pitch" => return entry.pitch = val(&e).map(Into::into),
                "embedRegular" => &mut entry.embedded_regular,
                "embedBold" => &mut entry.embedded_bold,
                "embedItalic" => &mut entry.embedded_italic,
                "embedBoldItalic" => &mut entry.embedded_bold_italic,
                _ => return,
            };
            let Some(rel) = e.attr(Ns::R, "id").and_then(|id| rels.get(&id)) else {
                return;
            };
            let Some(bytes) = package.part_into(&rel.target, diagnostics) else {
                diagnostics.push(Diagnostic::dropped(
                    "word/fontTable.xml",
                    format!("embedded font {} is missing", rel.target),
                ));
                return;
            };
            let mut bytes = bytes.into_owned();
            if let Some(key) = attr(&e, "fontKey") {
                if !deobfuscate(&mut bytes, &key) {
                    diagnostics.push(Diagnostic::dropped(
                        "word/fontTable.xml",
                        format!("embedded font {} has an unreadable key", rel.target),
                    ));
                    return;
                }
            }
            *slot = Some(Arc::from(bytes));
        });
        list.push(entry);
    });
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ECMA-376 Part 1 §17.8.1: obfuscation XORs the first 32 bytes with the
    /// reversed key, so applying it twice restores the font.
    #[test]
    fn deobfuscation_uses_the_reversed_guid() {
        let key = "{00112233-4455-6677-8899-AABBCCDDEEFF}";
        let mut data = vec![0u8; 40];
        assert!(deobfuscate(&mut data, key));
        assert_eq!(data[0], 0xFF);
        assert_eq!(data[15], 0x00);
        assert_eq!(data[16], 0xFF);
        assert_eq!(data[32], 0);
        assert!(deobfuscate(&mut data, key));
        assert!(data.iter().all(|&b| b == 0));
        assert!(!deobfuscate(&mut data, "short"));
    }

    #[test]
    fn theme_fonts() {
        let xml = r#"<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:themeElements><a:fontScheme><a:majorFont><a:latin typeface="Calibri Light"/><a:ea typeface=""/></a:majorFont><a:minorFont><a:latin typeface="Calibri"/><a:cs typeface="Arial"/></a:minorFont></a:fontScheme></a:themeElements></a:theme>"#;
        let theme = theme(xml);
        assert_eq!(theme.font("majorHAnsi").as_deref(), Some("Calibri Light"));
        assert_eq!(theme.font("minorAscii").as_deref(), Some("Calibri"));
        assert_eq!(theme.font("minorBidi").as_deref(), Some("Arial"));
        assert_eq!(theme.font("majorEastAsia"), None);
    }
}

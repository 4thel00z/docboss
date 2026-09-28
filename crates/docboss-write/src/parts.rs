//! The package parts other than stories: styles, numbering, settings, the
//! font table and the document properties.

use docboss_model::{Document, Level, Metadata, Numbering, StyleKind, Styles};

use crate::package::NS_W;
use crate::props;
use crate::xml::Xml;

/// ECMA-376 Part 1 §17.7.4: the style definitions, preceded by the
/// document defaults of §17.7.5.
pub fn styles(styles: &Styles) -> Vec<u8> {
    let mut xml = Xml::new();
    xml.open("w:styles", &[("xmlns:w", NS_W)]);
    xml.open("w:docDefaults", &[]);
    xml.open("w:rPrDefault", &[]);
    let mut run = Xml { out: String::new() };
    props::run_properties(&mut run, &styles.default_run, false);
    if run.out.is_empty() {
        run.out.push_str("<w:rPr/>");
    }
    xml.out.push_str(&run.out);
    xml.close("w:rPrDefault");
    xml.open("w:pPrDefault", &[]);
    let mut paragraph = Xml { out: String::new() };
    props::paragraph_properties(&mut paragraph, None, &styles.default_paragraph, None, None);
    if paragraph.out.is_empty() {
        paragraph.out.push_str("<w:pPr/>");
    }
    xml.out.push_str(&paragraph.out);
    xml.close("w:pPrDefault");
    xml.close("w:docDefaults");
    for style in &styles.styles {
        let kind = match style.kind {
            StyleKind::Paragraph => "paragraph",
            StyleKind::Character => "character",
            StyleKind::Table => "table",
            StyleKind::Numbering => "numbering",
        };
        let mut attributes = vec![("w:type", kind)];
        if style.is_default {
            attributes.push(("w:default", "1"));
        }
        attributes.push(("w:styleId", style.id.as_str()));
        xml.open("w:style", &attributes);
        xml.val("w:name", style.name.as_deref().unwrap_or(&style.id));
        if let Some(based_on) = &style.based_on {
            xml.val("w:basedOn", based_on);
        }
        if let Some(next) = &style.next {
            xml.val("w:next", next);
        }
        xml.empty("w:qFormat", &[]);
        if style.kind != StyleKind::Character {
            props::paragraph_properties(&mut xml, None, &style.paragraph, None, None);
        }
        props::run_properties(&mut xml, &style.run, false);
        if let Some(table) = &style.table {
            props::table_properties(&mut xml, table);
        }
        xml.close("w:style");
    }
    xml.close("w:styles");
    xml.finish()
}

fn level(xml: &mut Xml, level: &Level) {
    xml.open("w:lvl", &[("w:ilvl", &level.level.to_string())]);
    xml.val("w:start", &level.start.to_string());
    xml.val("w:numFmt", props::number_format_name(&level.format));
    if let Some(restart) = level.restart_after {
        xml.val("w:lvlRestart", &(u32::from(restart) + 1).to_string());
    }
    if level.legal {
        xml.empty("w:isLgl", &[]);
    }
    xml.val("w:lvlText", &level.text);
    if let Some(value) = level.justification {
        xml.val("w:lvlJc", props::justification_name(value));
    }
    props::paragraph_properties(xml, None, &level.paragraph, None, None);
    props::run_properties(xml, &level.run, false);
    xml.close("w:lvl");
}

/// ECMA-376 Part 1 §17.9: abstract list definitions, then the numbering
/// instances that refer to them, with their level overrides.
pub fn numbering(numbering: &Numbering) -> Vec<u8> {
    let mut xml = Xml::new();
    xml.open("w:numbering", &[("xmlns:w", NS_W)]);
    for definition in &numbering.abstracts {
        xml.open(
            "w:abstractNum",
            &[("w:abstractNumId", &definition.id.to_string())],
        );
        xml.val(
            "w:multiLevelType",
            if definition.levels.len() > 1 {
                "multilevel"
            } else {
                "singleLevel"
            },
        );
        definition.levels.iter().for_each(|l| level(&mut xml, l));
        xml.close("w:abstractNum");
    }
    for instance in &numbering.instances {
        xml.open("w:num", &[("w:numId", &instance.num_id.to_string())]);
        xml.val("w:abstractNumId", &instance.abstract_id.to_string());
        let mut levels: Vec<u8> = instance.start_overrides.iter().map(|(l, _)| *l).collect();
        levels.extend(instance.level_overrides.iter().map(|l| l.level));
        levels.sort_unstable();
        levels.dedup();
        for ilvl in levels {
            xml.open("w:lvlOverride", &[("w:ilvl", &ilvl.to_string())]);
            if let Some((_, start)) = instance.start_overrides.iter().find(|(l, _)| *l == ilvl) {
                xml.val("w:startOverride", &start.to_string());
            }
            if let Some(definition) = instance.level_overrides.iter().find(|l| l.level == ilvl) {
                level(&mut xml, definition);
            }
            xml.close("w:lvlOverride");
        }
        xml.close("w:num");
    }
    xml.close("w:numbering");
    xml.finish()
}

/// ECMA-376 Part 1 §17.15.1: document settings.
pub fn settings(document: &Document) -> Vec<u8> {
    let mut xml = Xml::new();
    xml.open("w:settings", &[("xmlns:w", NS_W)]);
    xml.val(
        "w:defaultTabStop",
        &document.settings.default_tab_stop.to_string(),
    );
    if document.settings.even_and_odd_headers {
        xml.empty("w:evenAndOddHeaders", &[]);
    }
    xml.open("w:compat", &[]);
    xml.empty(
        "w:compatSetting",
        &[
            ("w:name", "compatibilityMode"),
            ("w:uri", "http://schemas.microsoft.com/office/word"),
            ("w:val", "15"),
        ],
    );
    xml.close("w:compat");
    xml.close("w:settings");
    xml.finish()
}

/// ECMA-376 Part 1 §17.8.3: the font table.
pub fn font_table(document: &Document) -> Vec<u8> {
    let mut xml = Xml::new();
    xml.open("w:fonts", &[("xmlns:w", NS_W)]);
    for font in &document.fonts {
        xml.open("w:font", &[("w:name", &font.name)]);
        if let Some(alt) = &font.alt_name {
            xml.val("w:altName", alt);
        }
        if let Some(family) = &font.family {
            xml.val("w:family", family);
        }
        if let Some(pitch) = &font.pitch {
            xml.val("w:pitch", pitch);
        }
        xml.close("w:font");
    }
    xml.close("w:fonts");
    xml.finish()
}

/// ECMA-376 Part 2 §11: the core properties part.
pub fn core_properties(metadata: &Metadata) -> Vec<u8> {
    let mut xml = Xml::new();
    xml.open(
        "cp:coreProperties",
        &[
            (
                "xmlns:cp",
                "http://schemas.openxmlformats.org/package/2006/metadata/core-properties",
            ),
            ("xmlns:dc", "http://purl.org/dc/elements/1.1/"),
            ("xmlns:dcterms", "http://purl.org/dc/terms/"),
            ("xmlns:dcmitype", "http://purl.org/dc/dcmitype/"),
            ("xmlns:xsi", "http://www.w3.org/2001/XMLSchema-instance"),
        ],
    );
    let fields = [
        ("dc:title", &metadata.title),
        ("dc:subject", &metadata.subject),
        ("dc:creator", &metadata.creator),
        ("cp:keywords", &metadata.keywords),
        ("dc:description", &metadata.description),
        ("cp:lastModifiedBy", &metadata.last_modified_by),
        ("cp:revision", &metadata.revision),
    ];
    for (name, value) in fields {
        if let Some(value) = value {
            xml.text_element(name, &[], value);
        }
    }
    for (name, value) in [
        ("dcterms:created", &metadata.created),
        ("dcterms:modified", &metadata.modified),
    ] {
        if let Some(value) = value {
            xml.text_element(name, &[("xsi:type", "dcterms:W3CDTF")], value);
        }
    }
    if let Some(category) = &metadata.category {
        xml.text_element("cp:category", &[], category);
    }
    xml.close("cp:coreProperties");
    xml.finish()
}

/// The extended (application) properties part.
pub fn app_properties(metadata: &Metadata) -> Vec<u8> {
    let mut xml = Xml::new();
    xml.open(
        "Properties",
        &[
            (
                "xmlns",
                "http://schemas.openxmlformats.org/officeDocument/2006/extended-properties",
            ),
            (
                "xmlns:vt",
                "http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes",
            ),
        ],
    );
    xml.text_element(
        "Application",
        &[],
        metadata.application.as_deref().unwrap_or("docboss"),
    );
    let counts = [
        ("Pages", metadata.pages),
        ("Words", metadata.words),
        ("Characters", metadata.characters),
    ];
    for (name, value) in counts {
        if let Some(value) = value {
            xml.text_element(name, &[], &value.to_string());
        }
    }
    if let Some(company) = &metadata.company {
        xml.text_element("Company", &[], company);
    }
    xml.close("Properties");
    xml.finish()
}

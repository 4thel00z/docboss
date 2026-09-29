#![allow(dead_code)]

use std::sync::Arc;

use docboss_model::*;

pub fn text_run(text: &str, properties: RunProperties) -> Inline {
    Inline::Run(Run {
        properties,
        content: vec![RunContent::Text(text.into())],
    })
}

pub fn run(text: &str) -> Inline {
    text_run(text, RunProperties::default())
}

pub fn bold(text: &str) -> Inline {
    text_run(
        text,
        RunProperties {
            bold: Some(true),
            ..RunProperties::default()
        },
    )
}

pub fn italic(text: &str) -> Inline {
    text_run(
        text,
        RunProperties {
            italic: Some(true),
            ..RunProperties::default()
        },
    )
}

pub fn mono(text: &str) -> Inline {
    let fonts = FontSlots {
        ascii: Some("Courier New".into()),
        ..FontSlots::default()
    };
    text_run(
        text,
        RunProperties {
            fonts,
            ..RunProperties::default()
        },
    )
}

pub fn content(content: RunContent) -> Inline {
    Inline::Run(Run {
        properties: RunProperties::default(),
        content: vec![content],
    })
}

pub fn link(target: &str, inlines: Vec<Inline>) -> Inline {
    Inline::Hyperlink(Hyperlink {
        target: Some(target.into()),
        inlines,
        ..Hyperlink::default()
    })
}

pub fn paragraph(inlines: Vec<Inline>) -> Paragraph {
    Paragraph {
        inlines,
        ..Paragraph::default()
    }
}

pub fn para(inlines: Vec<Inline>) -> Block {
    Block::Paragraph(paragraph(inlines))
}

pub fn styled(style: &str, inlines: Vec<Inline>) -> Block {
    Block::Paragraph(Paragraph {
        style_id: Some(style.into()),
        ..paragraph(inlines)
    })
}

pub fn item(num_id: i64, level: u8, inlines: Vec<Inline>) -> Block {
    let mut p = paragraph(inlines);
    p.properties.numbering = Some(NumberingRef { num_id, level });
    Block::Paragraph(p)
}

pub fn cell(blocks: Vec<Block>) -> TableCell {
    TableCell {
        blocks,
        ..TableCell::default()
    }
}

pub fn row(cells: Vec<TableCell>) -> TableRow {
    TableRow {
        cells,
        ..TableRow::default()
    }
}

pub fn table(rows: Vec<TableRow>) -> Block {
    Block::Table(Table {
        rows,
        ..Table::default()
    })
}

fn heading_style(n: u8) -> Style {
    let mut style = Style::new(format!("Heading{n}"), StyleKind::Paragraph);
    style.name = Some(format!("heading {n}"));
    style.based_on = Some("Normal".into());
    style.run.bold = Some(true);
    style
}

fn numbering() -> Numbering {
    let decimal = |level: u8, format: NumberFormat, text: &str| Level {
        level,
        format,
        text: text.into(),
        ..Level::default()
    };
    let bullet = |level: u8, text: &str, font: &str| Level {
        level,
        format: NumberFormat::Bullet,
        text: text.into(),
        run: RunProperties {
            fonts: FontSlots {
                ascii: Some(font.into()),
                ..FontSlots::default()
            },
            ..RunProperties::default()
        },
        ..Level::default()
    };
    Numbering {
        abstracts: vec![
            AbstractNumbering {
                id: 0,
                levels: vec![
                    decimal(0, NumberFormat::Decimal, "%1."),
                    decimal(1, NumberFormat::LowerLetter, "%2)"),
                    decimal(2, NumberFormat::LowerRoman, "%3."),
                ],
            },
            AbstractNumbering {
                id: 1,
                levels: vec![
                    bullet(0, "\u{f0b7}", "Symbol"),
                    bullet(1, "o", "Courier New"),
                ],
            },
        ],
        instances: vec![
            NumberingInstance {
                num_id: 1,
                abstract_id: 0,
                ..NumberingInstance::default()
            },
            NumberingInstance {
                num_id: 2,
                abstract_id: 1,
                ..NumberingInstance::default()
            },
        ],
    }
}

pub fn styles() -> Styles {
    let mut normal = Style::new("Normal", StyleKind::Paragraph);
    normal.is_default = true;
    let mut code = Style::new("SourceCode", StyleKind::Paragraph);
    code.name = Some("Source Code".into());
    let mut hidden = Style::new("Hidden", StyleKind::Character);
    hidden.run.vanish = Some(true);
    let mut styles = vec![normal, code, hidden];
    styles.extend((1..=3).map(heading_style));
    Styles::new(
        ParagraphProperties::default(),
        RunProperties::default(),
        styles,
    )
}

pub fn document(blocks: Vec<Block>) -> Document {
    Document {
        styles: styles(),
        numbering: numbering(),
        sections: vec![Section {
            properties: SectionProperties::default(),
            blocks,
        }],
        ..Document::default()
    }
}

pub fn note(id: i64, kind: NoteKind, text: &str) -> Note {
    Note {
        id,
        kind,
        blocks: vec![para(vec![
            content(RunContent::NoteNumber),
            run(" "),
            run(text),
        ])],
    }
}

pub fn png(name: &str) -> Media {
    Media {
        name: format!("word/media/{name}"),
        content_type: "image/png".into(),
        data: Arc::from(&b"\x89PNG\r\n\x1a\n"[..]),
    }
}

pub fn drawing(media: u32, description: &str) -> Inline {
    content(RunContent::Drawing(Box::new(Drawing {
        media: Some(MediaId(media)),
        width: 952500,
        height: 476250,
        placement: DrawingPlacement::Inline,
        name: Some("Picture 1".into()),
        description: Some(description.into()),
        text_box: Vec::new(),
        shape: Default::default(),
        geometry: None,
        members: Vec::new(),
        data_text: Vec::new(),
    })))
}

pub fn text_box(blocks: Vec<Block>) -> Inline {
    content(RunContent::Drawing(Box::new(Drawing {
        media: None,
        width: 1905000,
        height: 952500,
        placement: DrawingPlacement::Anchored {
            horizontal: DrawingPosition::offset(PositionBase::Column, 0),
            vertical: DrawingPosition::offset(PositionBase::Paragraph, 0),
            behind_text: false,
        },
        name: Some("Text Box 1".into()),
        description: None,
        text_box: blocks,
        shape: Default::default(),
        geometry: None,
        members: Vec::new(),
        data_text: Vec::new(),
    })))
}

//! A document built in code that exercises most of the layout engine:
//! headings, justified text, lists, a table with a shaded header row,
//! footnotes, an image, headers and footers with page numbers and a
//! two-column section.

use docboss_model::{
    AbstractNumbering, Block, Border, BorderStyle, Borders, Color, Document, Drawing,
    DrawingPlacement, Field, HeaderFooter, HeaderFooterKind, HeaderFooterRefs, Indentation, Inline,
    Justification, Level, Media, MediaId, Note, NoteKind, NumberFormat, Numbering,
    NumberingInstance, NumberingRef, Paragraph, ParagraphProperties, Run, RunContent,
    RunProperties, Section, SectionBreak, SectionProperties, Shading, Spacing, Style, StyleKind,
    Styles, Table, TableCell, TableCellProperties, TableProperties, TableRow, TableRowProperties,
};
use docboss_render::{Format, Pixmap};

const LOREM: &str = "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat.";

fn text(t: &str) -> Inline {
    Inline::Run(Run {
        properties: RunProperties::default(),
        content: vec![RunContent::Text(t.into())],
    })
}

fn styled(t: &str, properties: RunProperties) -> Inline {
    Inline::Run(Run {
        properties,
        content: vec![RunContent::Text(t.into())],
    })
}

fn paragraph(style: Option<&str>, properties: ParagraphProperties, inlines: Vec<Inline>) -> Block {
    Block::Paragraph(Paragraph {
        style_id: style.map(str::to_string),
        properties,
        inlines,
        ..Paragraph::default()
    })
}

fn styles() -> Styles {
    let mut normal = Style::new("Normal", StyleKind::Paragraph);
    normal.is_default = true;
    normal.run.fonts.ascii = Some("Calibri".into());
    normal.run.fonts.high_ansi = Some("Calibri".into());
    normal.run.size = Some(22);
    normal.paragraph.spacing = Spacing {
        after: Some(160),
        line: Some(259),
        ..Spacing::default()
    };
    let mut heading = Style::new("Heading1", StyleKind::Paragraph);
    heading.name = Some("heading 1".into());
    heading.based_on = Some("Normal".into());
    heading.run.size = Some(32);
    heading.run.color = Some(Some(Color(0x2F, 0x54, 0x96)));
    heading.run.fonts.ascii = Some("Calibri Light".into());
    heading.paragraph.spacing = Spacing {
        before: Some(240),
        after: Some(120),
        ..Spacing::default()
    };
    heading.paragraph.keep_next = Some(true);
    let mut title = Style::new("Title", StyleKind::Paragraph);
    title.based_on = Some("Normal".into());
    title.run.size = Some(56);
    title.run.fonts.ascii = Some("Calibri Light".into());
    let mut note = Style::new("FootnoteText", StyleKind::Paragraph);
    note.based_on = Some("Normal".into());
    note.run.size = Some(18);
    note.paragraph.spacing = Spacing {
        after: Some(0),
        line: Some(240),
        ..Spacing::default()
    };
    Styles::new(
        ParagraphProperties::default(),
        RunProperties::default(),
        vec![normal, heading, title, note],
    )
}

fn numbering() -> Numbering {
    let level = |format: NumberFormat, text: &str| Level {
        format,
        text: text.into(),
        paragraph: ParagraphProperties {
            indentation: Indentation {
                left: Some(720),
                hanging: Some(360),
                ..Indentation::default()
            },
            ..ParagraphProperties::default()
        },
        ..Level::default()
    };
    Numbering {
        abstracts: vec![
            AbstractNumbering {
                id: 0,
                levels: vec![level(NumberFormat::Bullet, "\u{2022}")],
            },
            AbstractNumbering {
                id: 1,
                levels: vec![level(NumberFormat::Decimal, "%1.")],
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

fn gradient_png() -> Vec<u8> {
    let (w, h) = (160u32, 80u32);
    let mut image = Pixmap::new(w, h).unwrap_or_else(|_| unreachable!());
    for (i, pixel) in image.data.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let (x, y) = (i as u32 % w, i as u32 / w);
        *pixel = [(x * 255 / w) as u8, (y * 255 / h) as u8, 200, 255];
    }
    image.encode(Format::Png).unwrap_or_default()
}

fn chunk(index: usize, note_id: i64) -> Vec<Block> {
    let justified = ParagraphProperties {
        justification: Some(Justification::Both),
        ..ParagraphProperties::default()
    };
    let list = |num_id: i64| ParagraphProperties {
        numbering: Some(NumberingRef { num_id, level: 0 }),
        ..ParagraphProperties::default()
    };
    let bold = RunProperties {
        bold: Some(true),
        ..RunProperties::default()
    };
    let italic = RunProperties {
        italic: Some(true),
        ..RunProperties::default()
    };
    let border = Some(Border {
        style: BorderStyle::Single,
        size: 4,
        space: 0,
        color: Some(Color(0x80, 0x80, 0x80)),
    });
    let header_cell = |t: &str| TableCell {
        properties: TableCellProperties {
            shading: Some(Shading {
                fill: Some(Color(0xD9, 0xE2, 0xF3)),
            }),
            ..TableCellProperties::default()
        },
        blocks: vec![paragraph(
            None,
            ParagraphProperties::default(),
            vec![styled(t, bold.clone())],
        )],
    };
    let cell = |t: &str| TableCell {
        blocks: vec![paragraph(
            None,
            ParagraphProperties::default(),
            vec![text(t)],
        )],
        ..TableCell::default()
    };
    let table = Table {
        properties: TableProperties {
            borders: Some(Borders {
                top: border,
                left: border,
                bottom: border,
                right: border,
                inside_horizontal: border,
                inside_vertical: border,
            }),
            ..TableProperties::default()
        },
        grid: vec![2400, 3480, 3480],
        rows: std::iter::once(TableRow {
            properties: TableRowProperties {
                header: true,
                ..TableRowProperties::default()
            },
            cells: vec![
                header_cell("Region"),
                header_cell("Revenue"),
                header_cell("Growth"),
            ],
        })
        .chain((0..4).map(|r| TableRow {
            cells: vec![
                cell(&format!("Region {r}")),
                cell(&format!("{} k", 120 + r * 17)),
                cell(&format!("{}.{} %", r + 2, r)),
            ],
            ..TableRow::default()
        }))
        .collect(),
    };
    let image = RunContent::Drawing(Drawing {
        media: Some(MediaId(0)),
        width: 12700 * 180,
        height: 12700 * 90,
        placement: DrawingPlacement::Inline,
        name: None,
        description: None,
        text_box: Vec::new(),
        shape: Default::default(),
    });
    vec![
        paragraph(
            Some("Heading1"),
            ParagraphProperties::default(),
            vec![text(&format!("{}. Quarterly summary", index + 1))],
        ),
        paragraph(
            None,
            justified.clone(),
            vec![
                text("This report covers "),
                styled("revenue", bold.clone()),
                text(", "),
                styled("growth", italic),
                Inline::Run(Run {
                    properties: RunProperties::default(),
                    content: vec![
                        RunContent::Text(" and outlook.".into()),
                        RunContent::FootnoteReference(note_id),
                    ],
                }),
                text(&format!(" {LOREM} {LOREM}")),
            ],
        ),
        paragraph(None, list(1), vec![text("Bullet items hang at the indent")]),
        paragraph(
            None,
            list(1),
            vec![text(&format!(
                "A longer bullet that wraps onto a second line. {LOREM}"
            ))],
        ),
        paragraph(None, list(2), vec![text("First numbered item")]),
        paragraph(None, list(2), vec![text("Second numbered item")]),
        Block::Table(table),
        paragraph(
            None,
            ParagraphProperties::default(),
            vec![Inline::Run(Run {
                properties: RunProperties::default(),
                content: vec![image],
            })],
        ),
        paragraph(None, justified, vec![text(LOREM)]),
    ]
}

/// A document of `chunks` report sections, roughly 1.3 pages each.
pub fn document(chunks: usize) -> Document {
    let page_field = Inline::Field(Field {
        instruction: "PAGE".into(),
        result: vec![text("1")],
    });
    let total_field = Inline::Field(Field {
        instruction: "NUMPAGES".into(),
        result: vec![text("1")],
    });
    let centered = ParagraphProperties {
        justification: Some(Justification::Center),
        ..ParagraphProperties::default()
    };
    let right = ParagraphProperties {
        justification: Some(Justification::Right),
        ..ParagraphProperties::default()
    };
    let mut blocks = vec![paragraph(
        Some("Title"),
        ParagraphProperties::default(),
        vec![text("Annual Report")],
    )];
    let mut footnotes = Vec::new();
    for i in 0..chunks {
        blocks.extend(chunk(i, i as i64 + 1));
        footnotes.push(Note {
            id: i as i64 + 1,
            kind: NoteKind::Footnote,
            blocks: vec![paragraph(
                Some("FootnoteText"),
                ParagraphProperties::default(),
                vec![
                    Inline::Run(Run {
                        properties: RunProperties {
                            vertical_align: Some(docboss_model::VerticalAlign::Superscript),
                            ..RunProperties::default()
                        },
                        content: vec![RunContent::NoteNumber],
                    }),
                    text(" Figures are unaudited and rounded to the nearest thousand."),
                ],
            )],
        });
    }
    let first = SectionProperties {
        headers: HeaderFooterRefs {
            default: Some("h1".into()),
            ..HeaderFooterRefs::default()
        },
        footers: HeaderFooterRefs {
            default: Some("f1".into()),
            ..HeaderFooterRefs::default()
        },
        ..SectionProperties::default()
    };
    let mut columns = SectionProperties {
        start: SectionBreak::NextPage,
        ..SectionProperties::default()
    };
    columns.columns.count = 2;
    let column_blocks = (0..6)
        .map(|_| paragraph(None, ParagraphProperties::default(), vec![text(LOREM)]))
        .collect();
    Document {
        styles: styles(),
        numbering: numbering(),
        sections: vec![
            Section {
                properties: first,
                blocks,
            },
            Section {
                properties: columns,
                blocks: column_blocks,
            },
        ],
        headers_footers: vec![
            HeaderFooter {
                id: "h1".into(),
                kind: HeaderFooterKind::Header,
                blocks: vec![paragraph(None, right, vec![text("docboss sample")])],
            },
            HeaderFooter {
                id: "f1".into(),
                kind: HeaderFooterKind::Footer,
                blocks: vec![paragraph(
                    None,
                    centered,
                    vec![text("Page "), page_field, text(" of "), total_field],
                )],
            },
        ],
        footnotes,
        media: vec![Media {
            name: "word/media/image1.png".into(),
            content_type: "image/png".into(),
            data: gradient_png().into(),
        }],
        ..Document::default()
    }
}

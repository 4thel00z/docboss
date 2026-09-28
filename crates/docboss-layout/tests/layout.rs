use std::path::Path;
use std::sync::Arc;

use docboss_font::FontDatabase;
use docboss_layout::{layout, Item, Layout};
use docboss_model::{
    AbstractNumbering, Block, Border, BorderStyle, Borders, Break, Document, Field, HeaderFooter,
    HeaderFooterKind, HeaderFooterRefs, Indentation, Inline, Justification, Level, Note, NoteKind,
    NumberFormat, Numbering, NumberingInstance, NumberingRef, Paragraph, ParagraphProperties, Run,
    RunContent, RunProperties, Section, SectionProperties, Style, StyleKind, Styles, TabAlignment,
    TabLeader, TabStop, Table, TableCell, TableProperties, TableRow,
};

const COUSINE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../docboss-font/tests/fixtures/Cousine-Regular.ttf"
);

fn fonts() -> Arc<FontDatabase> {
    let mut db = FontDatabase::new();
    db.add_file(Path::new(COUSINE));
    Arc::new(db)
}

fn run(text: &str) -> Inline {
    Inline::Run(Run {
        properties: RunProperties::default(),
        content: vec![RunContent::Text(text.into())],
    })
}

fn para(text: &str) -> Block {
    Block::Paragraph(Paragraph {
        inlines: vec![run(text)],
        ..Paragraph::default()
    })
}

fn para_with(properties: ParagraphProperties, inlines: Vec<Inline>) -> Block {
    Block::Paragraph(Paragraph {
        properties,
        inlines,
        ..Paragraph::default()
    })
}

fn doc(blocks: Vec<Block>) -> Document {
    let mut normal = Style::new("Normal", StyleKind::Paragraph);
    normal.is_default = true;
    normal.run.fonts.ascii = Some("Courier New".into());
    normal.run.size = Some(20);
    Document {
        styles: Styles::new(
            ParagraphProperties::default(),
            RunProperties::default(),
            vec![normal],
        ),
        sections: vec![Section {
            properties: SectionProperties::default(),
            blocks,
        }],
        ..Document::default()
    }
}

fn laid(document: &Document) -> Layout {
    layout(document, &fonts())
}

fn runs(layout: &Layout, page: usize) -> Vec<&docboss_layout::GlyphRun> {
    layout.pages[page].glyph_runs().collect()
}

/// Cousine advances 1229/2048 em: 6.0 points per character at 10 points.
const ADVANCE: f32 = 1229.0 / 2048.0 * 10.0;

#[test]
fn one_paragraph_starts_at_the_margins() {
    let layout = laid(&doc(vec![para("Hello world")]));
    assert_eq!(layout.pages.len(), 1);
    let page = &layout.pages[0];
    assert_eq!((page.width, page.height), (612.0, 792.0));
    let runs = runs(&layout, 0);
    assert_eq!(
        runs.iter().map(|r| r.text.as_str()).collect::<String>(),
        "Helloworld"
    );
    let first = runs[0];
    assert!((first.glyphs[0].x - 72.0).abs() < 0.01);
    assert!(
        first.baseline > 72.0 && first.baseline < 72.0 + 12.0,
        "{}",
        first.baseline
    );
    let world = runs
        .iter()
        .find(|r| r.text.starts_with('w'))
        .map_or(first.glyphs[5].x, |r| r.glyphs[0].x);
    assert!((world - (72.0 + 6.0 * ADVANCE)).abs() < 0.01);
}

#[test]
fn long_paragraphs_wrap_inside_the_margins() {
    let text = "lorem ipsum dolor sit amet ".repeat(40);
    let layout = laid(&doc(vec![para(&text)]));
    let right = 612.0 - 72.0;
    let mut baselines: Vec<f32> = Vec::new();
    for run in runs(&layout, 0) {
        for g in &run.glyphs {
            assert!(
                g.x + ADVANCE <= right + 0.01,
                "glyph at {} past the margin",
                g.x
            );
        }
        if baselines.last() != Some(&run.baseline) {
            baselines.push(run.baseline);
        }
    }
    assert!(baselines.len() > 10);
    assert!(baselines.windows(2).all(|w| w[1] > w[0]));
}

#[test]
fn many_paragraphs_paginate() {
    let blocks = (0..200).map(|i| para(&format!("line {i}"))).collect();
    let layout = laid(&doc(blocks));
    assert!(layout.pages.len() >= 3, "{} pages", layout.pages.len());
    for page in &layout.pages {
        for run in page.glyph_runs() {
            assert!(
                run.baseline > 72.0 && run.baseline <= 792.0 - 72.0 + 0.01,
                "{}",
                run.baseline
            );
        }
    }
    let text: String = layout
        .pages
        .iter()
        .map(|p| p.text())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("line199"));
}

/// ECMA-376 Part 1 §17.3.3: a page break ends the page.
#[test]
fn page_breaks_start_a_new_page() {
    let broken = Inline::Run(Run {
        properties: RunProperties::default(),
        content: vec![
            RunContent::Text("before".into()),
            RunContent::Break(Break::Page),
            RunContent::Text("after".into()),
        ],
    });
    let layout = laid(&doc(vec![para_with(
        ParagraphProperties::default(),
        vec![broken],
    )]));
    assert_eq!(layout.pages.len(), 2);
    assert_eq!(layout.pages[0].text(), "before");
    assert_eq!(layout.pages[1].text(), "after");
}

fn border() -> Border {
    Border {
        style: BorderStyle::Single,
        size: 4,
        space: 0,
        color: None,
    }
}

/// ECMA-376 Part 1 §17.4: cells sit on the table grid with their margins
/// and borders.
#[test]
fn table_cells_sit_on_the_grid_with_borders() {
    let cell = |text: &str| TableCell {
        blocks: vec![para(text)],
        ..TableCell::default()
    };
    let table = Table {
        properties: TableProperties {
            borders: Some(Borders {
                top: Some(border()),
                left: Some(border()),
                bottom: Some(border()),
                right: Some(border()),
                inside_horizontal: Some(border()),
                inside_vertical: Some(border()),
            }),
            ..TableProperties::default()
        },
        grid: vec![2880, 4320],
        rows: vec![
            TableRow {
                cells: vec![cell("a1"), cell("b1")],
                ..TableRow::default()
            },
            TableRow {
                cells: vec![cell("a2"), cell("b2")],
                ..TableRow::default()
            },
        ],
    };
    let layout = laid(&doc(vec![Block::Table(table)]));
    let runs = runs(&layout, 0);
    let x_of = |text: &str| runs.iter().find(|r| r.text == text).unwrap().glyphs[0].x;
    assert!((x_of("a1") - (72.0 + 5.4)).abs() < 0.01);
    assert!((x_of("b1") - (72.0 + 144.0 + 5.4)).abs() < 0.01);
    let row_of = |text: &str| runs.iter().find(|r| r.text == text).unwrap().baseline;
    assert!(row_of("a2") > row_of("a1"));
    assert_eq!(row_of("a1"), row_of("b1"));
    let verticals: Vec<f32> = layout.pages[0]
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Line { from, to, .. } if from.0 == to.0 => Some(from.0),
            _ => None,
        })
        .collect();
    for x in [72.0, 216.0, 432.0] {
        assert!(
            verticals.iter().any(|v| (v - x).abs() < 0.01),
            "no border at {x}: {verticals:?}"
        );
    }
}

/// ECMA-376 Part 1 §17.10 and §17.16: every page's header evaluates its
/// `PAGE` and `NUMPAGES` fields.
#[test]
fn headers_show_the_page_number() {
    let page_field = Inline::Field(Field {
        instruction: " PAGE ".into(),
        result: vec![run("1")],
    });
    let total_field = Inline::Field(Field {
        instruction: "NUMPAGES".into(),
        result: vec![run("1")],
    });
    let header = HeaderFooter {
        id: "rId1".into(),
        kind: HeaderFooterKind::Header,
        blocks: vec![para_with(
            ParagraphProperties::default(),
            vec![run("Page "), page_field, run(" of "), total_field],
        )],
    };
    let mut document = doc((0..120).map(|i| para(&format!("body {i}"))).collect());
    document.sections[0].properties.headers = HeaderFooterRefs {
        default: Some("rId1".into()),
        ..HeaderFooterRefs::default()
    };
    document.headers_footers.push(header);
    let layout = laid(&document);
    let pages = layout.pages.len();
    assert!(pages >= 2);
    for (i, page) in layout.pages.iter().enumerate() {
        let top: String = page
            .glyph_runs()
            .filter(|r| r.baseline < 72.0)
            .map(|r| r.text.clone())
            .collect();
        assert_eq!(top, format!("Page{}of{}", i + 1, pages));
    }
}

/// ECMA-376 Part 1 §17.11: a footnote sits at the bottom of the page that
/// references it.
#[test]
fn footnotes_sit_at_the_bottom_of_their_page() {
    let reference = Inline::Run(Run {
        properties: RunProperties::default(),
        content: vec![
            RunContent::Text("claim".into()),
            RunContent::FootnoteReference(7),
        ],
    });
    let mut document = doc(vec![para_with(
        ParagraphProperties::default(),
        vec![reference],
    )]);
    document.footnotes.push(Note {
        id: 7,
        kind: NoteKind::Footnote,
        blocks: vec![para_with(
            ParagraphProperties::default(),
            vec![Inline::Run(Run {
                properties: RunProperties::default(),
                content: vec![
                    RunContent::NoteNumber,
                    RunContent::Text(" the source".into()),
                ],
            })],
        )],
    });
    let layout = laid(&document);
    let runs = runs(&layout, 0);
    let mark = runs
        .iter()
        .find(|r| r.text.ends_with('1') && r.baseline < 100.0)
        .expect("reference mark");
    assert!(mark.size < 10.0, "the mark is superscript");
    let note = runs
        .iter()
        .find(|r| r.text.contains("source"))
        .expect("footnote text");
    assert!(note.baseline > 792.0 - 72.0 - 30.0, "{}", note.baseline);
}

/// ECMA-376 Part 1 §17.9: list labels sit at the hanging indent and the
/// text at the left indent.
#[test]
fn list_paragraphs_get_labels_at_the_hanging_indent() {
    let numbering = Numbering {
        abstracts: vec![AbstractNumbering {
            id: 0,
            levels: vec![Level {
                format: NumberFormat::Decimal,
                text: "%1.".into(),
                paragraph: ParagraphProperties {
                    indentation: Indentation {
                        left: Some(720),
                        hanging: Some(360),
                        ..Indentation::default()
                    },
                    ..ParagraphProperties::default()
                },
                ..Level::default()
            }],
        }],
        instances: vec![NumberingInstance {
            num_id: 1,
            abstract_id: 0,
            ..NumberingInstance::default()
        }],
    };
    let item = || ParagraphProperties {
        numbering: Some(NumberingRef {
            num_id: 1,
            level: 0,
        }),
        ..ParagraphProperties::default()
    };
    let mut document = doc(vec![
        para_with(item(), vec![run("first")]),
        para_with(item(), vec![run("second")]),
    ]);
    document.numbering = numbering;
    let layout = laid(&document);
    let runs = runs(&layout, 0);
    let label = runs
        .iter()
        .find(|r| r.text.starts_with("2."))
        .expect("second label");
    assert!((label.glyphs[0].x - (72.0 + 18.0)).abs() < 0.01);
    let body = label
        .glyphs
        .iter()
        .zip(label.text.chars())
        .find(|(_, c)| *c == 's')
        .map(|(g, _)| g.x);
    let body =
        body.unwrap_or_else(|| runs.iter().find(|r| r.text == "second").unwrap().glyphs[0].x);
    assert!((body - (72.0 + 36.0)).abs() < 0.01, "{body}");
}

/// ECMA-376 Part 1 §17.3.1: `w:jc` both stretches wrapped lines to the
/// right indent.
#[test]
fn justified_lines_reach_the_right_margin() {
    let props = ParagraphProperties {
        justification: Some(Justification::Both),
        ..ParagraphProperties::default()
    };
    let layout = laid(&doc(vec![para_with(
        props,
        vec![run(&"justify these words ".repeat(20))],
    )]));
    let runs = runs(&layout, 0);
    let first_line = runs[0].baseline;
    let end = runs
        .iter()
        .filter(|r| r.baseline == first_line)
        .flat_map(|r| r.glyphs.iter())
        .map(|g| g.x + ADVANCE)
        .fold(0.0, f32::max);
    assert!((end - (612.0 - 72.0)).abs() < 0.05, "{end}");
}

/// ECMA-376 Part 1 §17.3.1: a right tab stop ends the following text at
/// the stop, with its leader filling the gap.
#[test]
fn right_tabs_align_text_to_the_stop() {
    let props = ParagraphProperties {
        tabs: vec![TabStop {
            position: 7200,
            alignment: TabAlignment::Right,
            leader: TabLeader::Dot,
        }],
        ..ParagraphProperties::default()
    };
    let content = Inline::Run(Run {
        properties: RunProperties::default(),
        content: vec![
            RunContent::Text("Intro".into()),
            RunContent::Tab,
            RunContent::Text("12".into()),
        ],
    });
    let layout = laid(&doc(vec![para_with(props, vec![content])]));
    let runs = runs(&layout, 0);
    let digits = runs
        .iter()
        .flat_map(|r| r.text.chars().zip(r.glyphs.iter()))
        .filter(|(c, _)| c.is_ascii_digit());
    let end = digits.map(|(_, g)| g.x + ADVANCE).fold(0.0, f32::max);
    assert!((end - (72.0 + 360.0)).abs() < 0.05, "{end}");
    let dots = runs
        .iter()
        .flat_map(|r| r.text.chars())
        .filter(|&c| c == '.')
        .count();
    assert!(dots > 20, "{dots} leader dots");
}

/// ECMA-376 Part 1 §17.6: `w:cols` flows text down the first column
/// before the second.
#[test]
fn two_columns_fill_left_then_right() {
    let mut document = doc((0..130).map(|i| para(&format!("c{i}"))).collect());
    document.sections[0].properties.columns.count = 2;
    let layout = laid(&document);
    let runs = runs(&layout, 0);
    let xs: Vec<f32> = runs.iter().map(|r| r.glyphs[0].x).collect();
    let middle = 306.0;
    assert!(xs.iter().any(|&x| x < middle) && xs.iter().any(|&x| x > middle));
    let first_right = runs.iter().position(|r| r.glyphs[0].x > middle).unwrap();
    assert!(runs[..first_right].iter().all(|r| r.glyphs[0].x < middle));
}

/// ECMA-376 Part 1 §17.3.1: `w:keepNext` keeps a heading on the page of
/// the paragraph after it.
#[test]
fn keep_with_next_moves_a_heading_to_the_next_page() {
    let heading = ParagraphProperties {
        keep_next: Some(true),
        ..ParagraphProperties::default()
    };
    let mut blocks: Vec<Block> = Vec::new();
    let per_page = {
        let probe = laid(&doc((0..200).map(|i| para(&format!("{i}"))).collect()));
        probe.pages[0].glyph_runs().count()
    };
    blocks.extend((0..per_page - 1).map(|i| para(&format!("{i}"))));
    blocks.push(para_with(heading, vec![run("Heading")]));
    blocks.push(para("body"));
    let layout = laid(&doc(blocks));
    assert_eq!(layout.pages.len(), 2);
    assert!(layout.pages[1].text().starts_with("Heading"));
}

/// ECMA-376 Part 1 §17.4: a row taller than the page breaks across pages.
#[test]
fn a_tall_row_splits_across_pages() {
    let lines: Vec<Block> = (0..120).map(|i| para(&format!("row line {i}"))).collect();
    let table = Table {
        grid: vec![9360],
        rows: vec![TableRow {
            cells: vec![TableCell {
                blocks: lines,
                ..TableCell::default()
            }],
            ..TableRow::default()
        }],
        ..Table::default()
    };
    let layout = laid(&doc(vec![Block::Table(table)]));
    assert!(layout.pages.len() >= 2);
    let all: String = layout.pages.iter().map(|p| p.text()).collect();
    assert!(all.contains("line119"));
    for page in &layout.pages {
        assert!(page.glyph_runs().all(|r| r.baseline <= 792.0 - 72.0 + 0.01));
    }
}

fn nested(depth: usize) -> Block {
    if depth == 0 {
        return para("core");
    }
    Block::Table(Table {
        grid: vec![4000],
        rows: vec![TableRow {
            cells: vec![TableCell {
                blocks: vec![nested(depth - 1)],
                ..TableCell::default()
            }],
            ..TableRow::default()
        }],
        ..Table::default()
    })
}

#[test]
fn hostile_values_lay_out_without_panicking() {
    let wild = RunProperties {
        size: Some(u32::MAX),
        spacing: Some(-100_000),
        position: Some(i32::MIN / 4),
        ..RunProperties::default()
    };
    let odd = ParagraphProperties {
        indentation: Indentation {
            left: Some(i32::MAX / 2),
            right: Some(-50_000),
            hanging: Some(i32::MIN / 2),
            ..Indentation::default()
        },
        tabs: vec![TabStop {
            position: -500,
            alignment: TabAlignment::Decimal,
            leader: TabLeader::Dot,
        }],
        ..ParagraphProperties::default()
    };
    let mut document = doc(vec![
        para_with(
            odd,
            vec![Inline::Run(Run {
                properties: wild,
                content: vec![RunContent::Text("x\ty".into()), RunContent::Tab],
            })],
        ),
        Block::Table(Table {
            grid: vec![],
            rows: vec![TableRow {
                cells: vec![TableCell {
                    properties: docboss_model::TableCellProperties {
                        grid_span: u32::MAX,
                        ..Default::default()
                    },
                    blocks: vec![para("span")],
                }],
                ..TableRow::default()
            }],
            ..Table::default()
        }),
        nested(60),
    ]);
    document.sections[0].properties.page_size.width = -5;
    document.sections[0].properties.margins.top = i32::MIN / 2;
    document.sections[0].properties.columns.count = u32::MAX;
    let started = std::time::Instant::now();
    let layout = laid(&document);
    assert!(
        started.elapsed() < std::time::Duration::from_secs(10),
        "{:?}",
        started.elapsed()
    );
    assert!(!layout.pages.is_empty());
    assert!(layout
        .diagnostics
        .iter()
        .any(|d| d.message.contains("nested")));
}

fn one_cell_table(
    style_id: Option<&str>,
    fill: Option<docboss_model::Color>,
    texts: &[&str],
) -> Block {
    Block::Table(Table {
        properties: TableProperties {
            style_id: style_id.map(str::to_string),
            ..TableProperties::default()
        },
        grid: vec![4320],
        rows: vec![TableRow {
            cells: vec![TableCell {
                properties: docboss_model::TableCellProperties {
                    shading: fill.map(|fill| docboss_model::Shading { fill: Some(fill) }),
                    ..docboss_model::TableCellProperties::default()
                },
                blocks: texts.iter().map(|t| para(t)).collect(),
            }],
            ..TableRow::default()
        }],
    })
}

/// ECMA-376 Part 1 §17.7.2: a table style's paragraph properties apply to
/// the paragraphs in its cells, above the document defaults.
#[test]
fn table_style_paragraph_spacing_applies_inside_cells() {
    let mut document = doc(vec![one_cell_table(Some("Grid"), None, &["one", "two"])]);
    document.styles.default_paragraph.spacing.after = Some(400);
    let mut grid = Style::new("Grid", StyleKind::Table);
    grid.paragraph.spacing.after = Some(0);
    document.styles.push(grid);
    let layout = laid(&document);
    let styled_runs = runs(&layout, 0);
    let baseline = |text: &str| {
        styled_runs
            .iter()
            .find(|r| r.text == text)
            .unwrap()
            .baseline
    };
    let pitch = baseline("two") - baseline("one");
    assert!(
        pitch < 15.0,
        "20 pt of spacing after leaked into the cell: {pitch}"
    );

    let mut unstyled = doc(vec![one_cell_table(None, None, &["one", "two"])]);
    unstyled.styles.default_paragraph.spacing.after = Some(400);
    let layout = laid(&unstyled);
    let unstyled_runs = runs(&layout, 0);
    let baseline = |text: &str| {
        unstyled_runs
            .iter()
            .find(|r| r.text == text)
            .unwrap()
            .baseline
    };
    assert!(baseline("two") - baseline("one") > 30.0);
}

/// ECMA-376 Part 1 §17.3.2.6: `auto` text is drawn white over a dark cell
/// fill and black over a light one.
#[test]
fn automatic_text_color_follows_the_cell_fill() {
    let dark = docboss_model::Color(0xC0, 0, 0);
    let light = docboss_model::Color(0x9C, 0xC2, 0xE5);
    let document = doc(vec![
        one_cell_table(None, Some(dark), &["dark"]),
        one_cell_table(None, Some(light), &["light"]),
    ]);
    let layout = laid(&document);
    let runs = runs(&layout, 0);
    let color = |text: &str| runs.iter().find(|r| r.text == text).unwrap().color;
    assert_eq!(color("dark"), docboss_model::Color::WHITE);
    assert_eq!(color("light"), docboss_model::Color::BLACK);
}

/// A list label in a dark cell turns white with its text.
#[test]
fn list_labels_in_dark_cells_turn_white_too() {
    let dark = docboss_model::Color(0xC0, 0, 0);
    let mut document = doc(vec![one_cell_table(None, Some(dark), &["item"])]);
    document.numbering = Numbering {
        abstracts: vec![AbstractNumbering {
            id: 0,
            levels: vec![Level::default()],
        }],
        instances: vec![NumberingInstance {
            num_id: 1,
            abstract_id: 0,
            ..NumberingInstance::default()
        }],
    };
    let Block::Table(table) = &mut document.sections[0].blocks[0] else {
        unreachable!()
    };
    let Block::Paragraph(paragraph) = &mut table.rows[0].cells[0].blocks[0] else {
        unreachable!()
    };
    paragraph.properties.numbering = Some(NumberingRef {
        num_id: 1,
        level: 0,
    });
    let layout = laid(&document);
    let runs = runs(&layout, 0);
    let label = runs.iter().find(|r| r.text.starts_with("1.")).unwrap();
    assert_eq!(label.color, docboss_model::Color::WHITE);
}

/// ECMA-376 Part 1 §17.3.3.30: a bullet stored as a symbol font's code in
/// the U+F000 range draws its Unicode form when that font is missing,
/// instead of the missing-glyph box.
#[test]
fn symbol_font_bullets_draw_their_unicode_form() {
    for (font, code) in [("starbats", '\u{F095}'), ("Symbol", '\u{F0B7}')] {
        let mut level = Level {
            format: NumberFormat::Bullet,
            text: code.to_string(),
            ..Level::default()
        };
        level.run.fonts.ascii = Some(font.into());
        level.run.fonts.high_ansi = Some(font.into());
        let mut document = doc(vec![para_with(
            ParagraphProperties {
                numbering: Some(NumberingRef {
                    num_id: 1,
                    level: 0,
                }),
                ..ParagraphProperties::default()
            },
            vec![run("item")],
        )]);
        document.numbering = Numbering {
            abstracts: vec![AbstractNumbering {
                id: 0,
                levels: vec![level],
            }],
            instances: vec![NumberingInstance {
                num_id: 1,
                abstract_id: 0,
                ..NumberingInstance::default()
            }],
        };
        let layout = laid(&document);
        let runs = runs(&layout, 0);
        let label = runs.iter().find(|r| r.text.contains(code)).unwrap();
        assert_ne!(
            label.glyphs[0].id, 0,
            "{font} bullet drew the missing glyph"
        );
    }
}

fn bordered(text: &str) -> Block {
    para_with(
        ParagraphProperties {
            borders: Some(Borders {
                bottom: Some(border()),
                ..Borders::default()
            }),
            ..ParagraphProperties::default()
        },
        vec![run(text)],
    )
}

/// ECMA-376 Part 1 §17.3.1.24: three paragraphs with the same bottom border
/// draw it once, under the last of them.
#[test]
fn paragraphs_with_equal_borders_share_one_bottom_border() {
    let layout = laid(&doc(vec![
        bordered("a"),
        bordered("b"),
        bordered("c"),
        para("after"),
    ]));
    let horizontals: Vec<f32> = layout.pages[0]
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Line { from, to, .. } if from.1 == to.1 => Some(from.1),
            _ => None,
        })
        .collect();
    assert_eq!(horizontals.len(), 1, "{horizontals:?}");
    let runs = runs(&layout, 0);
    let c = runs.iter().find(|r| r.text == "c").unwrap().baseline;
    assert!(horizontals[0] > c);
}

/// ECMA-376 Part 1 §17.6.12 and §17.16.4.3: `PAGE` shows the section's
/// page number format, and a `\*` switch overrides it.
#[test]
fn page_fields_follow_the_section_number_format() {
    let field = |instruction: &str| {
        Inline::Field(Field {
            instruction: instruction.into(),
            result: vec![run("1")],
        })
    };
    let header = HeaderFooter {
        id: "rId1".into(),
        kind: HeaderFooterKind::Header,
        blocks: vec![para_with(
            ParagraphProperties::default(),
            vec![field(" PAGE "), run("/"), field(" PAGE \\* ALPHABETIC ")],
        )],
    };
    let mut document = doc((0..120).map(|i| para(&format!("body {i}"))).collect());
    document.sections[0].properties.headers = HeaderFooterRefs {
        default: Some("rId1".into()),
        ..HeaderFooterRefs::default()
    };
    document.sections[0].properties.page_number_format = Some(NumberFormat::LowerRoman);
    document.headers_footers.push(header);
    let layout = laid(&document);
    let tops: Vec<String> = layout
        .pages
        .iter()
        .take(2)
        .map(|page| {
            page.glyph_runs()
                .filter(|r| r.baseline < 72.0)
                .map(|r| r.text.clone())
                .collect()
        })
        .collect();
    assert_eq!(tops, ["i/A", "ii/B"]);
}

/// ECMA-376 Part 1 §17.4.84: no border is drawn between the rows of a
/// vertically merged cell, while the unmerged column keeps its rule.
#[test]
fn vertically_merged_cells_have_no_inner_border() {
    let all = Borders {
        top: Some(border()),
        left: Some(border()),
        bottom: Some(border()),
        right: Some(border()),
        inside_horizontal: Some(border()),
        inside_vertical: Some(border()),
    };
    let cell = |text: &str, merge: Option<docboss_model::VerticalMerge>| TableCell {
        properties: docboss_model::TableCellProperties {
            vertical_merge: merge,
            ..docboss_model::TableCellProperties::default()
        },
        blocks: vec![para(text)],
    };
    use docboss_model::VerticalMerge::{Continue, Restart};
    let table = Table {
        properties: TableProperties {
            borders: Some(all),
            ..TableProperties::default()
        },
        grid: vec![2880, 2880],
        rows: vec![
            TableRow {
                cells: vec![cell("merged", Some(Restart)), cell("b1", None)],
                ..TableRow::default()
            },
            TableRow {
                cells: vec![cell("", Some(Continue)), cell("b2", None)],
                ..TableRow::default()
            },
        ],
    };
    let layout = laid(&doc(vec![Block::Table(table)]));
    let runs = runs(&layout, 0);
    let b2 = runs.iter().find(|r| r.text == "b2").unwrap().baseline;
    let b1 = runs.iter().find(|r| r.text == "b1").unwrap().baseline;
    let inner: Vec<(f32, f32)> = layout.pages[0]
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Line { from, to, .. } if from.1 == to.1 && from.1 > b1 && from.1 < b2 => {
                Some((from.0.min(to.0), from.0.max(to.0)))
            }
            _ => None,
        })
        .collect();
    assert!(inner.iter().all(|(x0, _)| *x0 >= 216.0 - 0.01), "{inner:?}");
    assert!(inner.iter().any(|(_, x1)| *x1 > 300.0), "{inner:?}");
}

/// ECMA-376 Part 1 §17.4.48: a table whose grid is wider than the text
/// keeps its grid widths and extends past the right margin, as Word and
/// LibreOffice draw it; only a percentage width scales the grid.
#[test]
fn wide_grids_keep_their_widths() {
    let cell = |text: &str| TableCell {
        blocks: vec![para(text)],
        ..TableCell::default()
    };
    let table = Table {
        grid: vec![5760, 5760],
        rows: vec![TableRow {
            cells: vec![cell("left"), cell("right")],
            ..TableRow::default()
        }],
        ..Table::default()
    };
    let layout = laid(&doc(vec![Block::Table(table)]));
    let runs = runs(&layout, 0);
    let right = runs.iter().find(|r| r.text == "right").unwrap().glyphs[0].x;
    assert!((right - (72.0 + 288.0 + 5.4)).abs() < 0.01, "{right}");
}

fn text_box(
    placement: docboss_model::DrawingPlacement,
    shape: docboss_model::ShapeFormat,
    blocks: Vec<Block>,
) -> Inline {
    Inline::Run(Run {
        properties: RunProperties::default(),
        content: vec![RunContent::Drawing(docboss_model::Drawing {
            media: None,
            width: 2_540_000,
            height: 1_270_000,
            placement,
            name: None,
            description: None,
            text_box: blocks,
            shape,
        })],
    })
}

fn framed() -> docboss_model::ShapeFormat {
    docboss_model::ShapeFormat {
        fill: Some(docboss_model::Color(255, 255, 0)),
        outline: Some(docboss_model::Color::BLACK),
        outline_width: Some(12_700),
        insets: Some([127_000, 127_000, 127_000, 127_000]),
        text_anchor: None,
        auto_fit: false,
    }
}

fn anchored(x: i64, y: i64) -> docboss_model::DrawingPlacement {
    docboss_model::DrawingPlacement::Anchored {
        x,
        y,
        behind_text: false,
        relative_to_page: false,
    }
}

/// ECMA-376 Part 1 §20.4.2.38, §20.4.2.22, §20.4.2.35: an anchored text box
/// paints its fill at the anchor offset, its text inside the insets and its
/// outline on the four edges, and no image placeholder.
#[test]
fn anchored_text_boxes_paint_fill_text_and_outline() {
    let anchor = Block::Paragraph(Paragraph {
        inlines: vec![
            run("Anchor"),
            text_box(anchored(1_270_000, 635_000), framed(), vec![para("Boxed")]),
        ],
        ..Paragraph::default()
    });
    let layout = laid(&doc(vec![anchor]));
    let page = &layout.pages[0];
    let fill = page.items.iter().find_map(|item| match item {
        Item::Rect { rect, color } if *color == docboss_model::Color(255, 255, 0) => Some(*rect),
        _ => None,
    });
    let fill = fill.expect("the text box fill is painted");
    assert!(
        (fill.x - 172.0).abs() < 0.01 && (fill.y - 122.0).abs() < 0.01,
        "{fill:?}"
    );
    assert!((fill.width - 200.0).abs() < 0.01 && (fill.height - 100.0).abs() < 0.01);
    let boxed = runs(&layout, 0)
        .into_iter()
        .find(|run| run.text == "Boxed")
        .expect("box text");
    assert!(
        (boxed.glyphs[0].x - 182.0).abs() < 0.01,
        "{}",
        boxed.glyphs[0].x
    );
    assert!(
        boxed.baseline > fill.y + 10.0 && boxed.baseline < fill.y + 30.0,
        "{}",
        boxed.baseline
    );
    let outline = page
        .items
        .iter()
        .filter(|item| matches!(item, Item::Line { width, .. } if (*width - 1.0).abs() < 0.01))
        .count();
    assert_eq!(outline, 4);
    assert!(!page
        .items
        .iter()
        .any(|item| matches!(item, Item::Image { .. })));
}

/// ECMA-376 Part 1 §20.4.2.22: `a:spAutoFit` sizes the shape to its text,
/// growing or shrinking it, and the `anchor` attribute places the text
/// vertically.
#[test]
fn text_boxes_grow_to_fit_and_anchor_their_text() {
    let lines: Vec<Block> = (0..12).map(|i| para(&format!("line {i}"))).collect();
    let mut grow = framed();
    grow.auto_fit = true;
    let grown = laid(&doc(vec![Block::Paragraph(Paragraph {
        inlines: vec![text_box(anchored(0, 0), grow, lines)],
        ..Paragraph::default()
    })]));
    let fill = grown.pages[0].items.iter().find_map(|item| match item {
        Item::Rect { rect, .. } => Some(*rect),
        _ => None,
    });
    let fill = fill.expect("fill");
    let last = runs(&grown, 0)
        .into_iter()
        .filter(|r| r.text.starts_with("line"))
        .map(|r| r.baseline)
        .fold(0.0, f32::max);
    assert!(
        fill.height > 100.0 && fill.bottom() > last,
        "{fill:?} {last}"
    );

    let mut centered = framed();
    centered.text_anchor = Some(docboss_model::VerticalAlign::Center);
    let middle = laid(&doc(vec![Block::Paragraph(Paragraph {
        inlines: vec![text_box(anchored(0, 0), centered, vec![para("Mid")])],
        ..Paragraph::default()
    })]));
    let top = laid(&doc(vec![Block::Paragraph(Paragraph {
        inlines: vec![text_box(anchored(0, 0), framed(), vec![para("Mid")])],
        ..Paragraph::default()
    })]));
    let baseline = |layout: &Layout| {
        runs(layout, 0)
            .into_iter()
            .find(|r| r.text == "Mid")
            .map(|r| r.baseline)
            .unwrap_or(0.0)
    };
    let shift = baseline(&middle) - baseline(&top);
    assert!(shift > 25.0 && shift < 45.0, "{shift}");
}

/// ECMA-376 Part 1 §20.4.2.38: an inline text box sits on the line like a
/// picture, its text laid out inside it.
#[test]
fn inline_text_boxes_carry_their_text_on_the_line() {
    let layout = laid(&doc(vec![Block::Paragraph(Paragraph {
        inlines: vec![
            run("Before"),
            text_box(
                docboss_model::DrawingPlacement::Inline,
                framed(),
                vec![para("Inside")],
            ),
        ],
        ..Paragraph::default()
    })]));
    let before = runs(&layout, 0)
        .into_iter()
        .find(|r| r.text == "Before")
        .expect("before");
    let inside = runs(&layout, 0)
        .into_iter()
        .find(|r| r.text == "Inside")
        .expect("inside");
    assert!(
        inside.glyphs[0].x > before.glyphs[0].x + 6.0 * ADVANCE,
        "{}",
        inside.glyphs[0].x
    );
    assert!(
        inside.baseline < before.baseline,
        "{} {}",
        inside.baseline,
        before.baseline
    );
}

/// A text box in a header is painted with the header, not dropped.
#[test]
fn header_text_boxes_are_painted() {
    let mut document = doc(vec![para("Body")]);
    document.headers_footers.push(HeaderFooter {
        id: "h1".into(),
        kind: HeaderFooterKind::Header,
        blocks: vec![Block::Paragraph(Paragraph {
            inlines: vec![text_box(anchored(0, 0), framed(), vec![para("Logo")])],
            ..Paragraph::default()
        })],
    });
    document.sections[0].properties.headers = HeaderFooterRefs {
        default: Some("h1".into()),
        ..HeaderFooterRefs::default()
    };
    let layout = laid(&document);
    assert!(runs(&layout, 0).iter().any(|r| r.text == "Logo"));
}

/// ECMA-376 Part 1 §17.3.1.33: auto line spacing multiplies the text of a
/// line, not an inline picture on it, so a 100 pt picture under 1.15 line
/// spacing takes 100 pt plus the text descent.
#[test]
fn auto_line_spacing_leaves_inline_pictures_unscaled() {
    let picture = Inline::Run(Run {
        properties: RunProperties::default(),
        content: vec![RunContent::Drawing(docboss_model::Drawing {
            media: None,
            width: 1_270_000,
            height: 1_270_000,
            placement: docboss_model::DrawingPlacement::Inline,
            name: None,
            description: None,
            text_box: Vec::new(),
            shape: Default::default(),
        })],
    });
    let mut properties = ParagraphProperties::default();
    properties.spacing.line = Some(276);
    let layout = laid(&doc(vec![
        para_with(properties.clone(), vec![picture]),
        para_with(properties, vec![run("after")]),
    ]));
    let image = layout.pages[0]
        .items
        .iter()
        .find_map(|item| match item {
            Item::Image { rect, .. } => Some(*rect),
            _ => None,
        })
        .expect("picture");
    assert!((image.height - 100.0).abs() < 0.01);
    assert!(
        (image.y - 72.0).abs() < 0.01,
        "the picture starts at {} pt, not at the margin",
        image.y
    );
}

/// ECMA-376 Part 1 §20.4.2.22: an `a:spAutoFit` box taller than its one
/// line of text shrinks to that line plus its insets.
#[test]
fn auto_fit_text_boxes_shrink_to_their_text() {
    let mut fit = framed();
    fit.auto_fit = true;
    let layout = laid(&doc(vec![Block::Paragraph(Paragraph {
        inlines: vec![text_box(anchored(0, 0), fit, vec![para("short")])],
        ..Paragraph::default()
    })]));
    let fill = layout.pages[0]
        .items
        .iter()
        .find_map(|item| match item {
            Item::Rect { rect, .. } => Some(*rect),
            _ => None,
        })
        .expect("fill");
    assert!(fill.height > 20.0 && fill.height < 40.0, "{fill:?}");
}

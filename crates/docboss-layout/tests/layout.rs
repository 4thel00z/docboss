use std::path::Path;
use std::sync::Arc;

use docboss_font::FontDatabase;
use docboss_layout::{layout, Item, Layout, Rect};
use docboss_model::{
    AbstractNumbering, Block, Border, BorderStyle, Borders, Break, Document, DrawingPosition,
    Field, HeaderFooter, HeaderFooterKind, HeaderFooterRefs, Indentation, Inline, Justification,
    Level, Note, NoteKind, NumberFormat, Numbering, NumberingInstance, NumberingRef, Paragraph,
    ParagraphProperties, PositionAlign, PositionBase, Run, RunContent, RunProperties, Section,
    SectionProperties, Style, StyleKind, Styles, TabAlignment, TabLeader, TabStop, Table,
    TableCell, TableProperties, TableRow,
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
        shadow: false,
        style: BorderStyle::Single,
        size: 4,
        space: 0,
        color: None,
    }
}

type Segment = ((f32, f32), (f32, f32), f32);

fn border_lines(layout: &Layout, page: usize) -> Vec<Segment> {
    layout.pages[page]
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Line {
                from, to, width, ..
            } => Some((*from, *to, *width)),
            _ => None,
        })
        .collect()
}

/// ECMA-376 Part 1 §17.6.10: page borders measured from the page edge sit
/// `space` points inside it; measured from the text, `space` points outside
/// the margins; `display` picks the pages.
#[test]
fn page_borders_sit_at_their_offsets() {
    let side = |size: u32, space: u32| {
        Some(Border {
            shadow: false,
            style: BorderStyle::Single,
            size,
            space,
            color: None,
        })
    };
    let mut document = doc(vec![para("one")]);
    let sides = Borders {
        top: side(8, 24),
        left: side(16, 20),
        bottom: side(8, 16),
        right: None,
        ..Borders::default()
    };
    document.sections[0].properties.page_borders = Some(docboss_model::PageBorders {
        sides,
        offset_from: docboss_model::PageBorderOffset::Page,
        ..Default::default()
    });
    let lines = border_lines(&laid(&document), 0);
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0], ((20.0, 24.5), (612.0, 24.5), 1.0));
    assert_eq!(lines[1], ((20.0, 775.5), (612.0, 775.5), 1.0));
    assert_eq!(lines[2], ((21.0, 24.0), (21.0, 776.0), 2.0));

    document.sections[0].properties.page_borders = Some(docboss_model::PageBorders {
        sides,
        offset_from: docboss_model::PageBorderOffset::Text,
        behind_text: true,
        ..Default::default()
    });
    let layout = laid(&document);
    assert!(matches!(layout.pages[0].items[0], Item::Line { .. }));
    let lines = border_lines(&layout, 0);
    assert_eq!(lines[0], ((50.0, 47.5), (540.0, 47.5), 1.0));
    assert_eq!(lines[2], ((51.0, 47.0), (51.0, 737.0), 2.0));

    document.sections[0].properties.page_borders = Some(docboss_model::PageBorders {
        sides,
        display: docboss_model::PageBorderDisplay::NotFirstPage,
        ..Default::default()
    });
    assert!(border_lines(&laid(&document), 0).is_empty());
}

/// ECMA-376 Part 1 §17.6.15 and §17.6.2: a page border with a shadow casts
/// a black shadow as wide as the border to the right and below, and the
/// right and bottom lines move in by that width; §17.18.2: an art border is
/// left out and reported.
#[test]
fn page_border_shadows_and_art_borders() {
    let side = |style: BorderStyle, shadow: bool| {
        Some(Border {
            shadow,
            style,
            size: 48,
            space: 24,
            color: None,
        })
    };
    let mut document = doc(vec![para("one")]);
    let all = |style: BorderStyle, shadow: bool| Borders {
        top: side(style, shadow),
        left: side(style, shadow),
        bottom: side(style, shadow),
        right: side(style, shadow),
        ..Borders::default()
    };
    document.sections[0].properties.page_borders = Some(docboss_model::PageBorders {
        sides: all(BorderStyle::Single, true),
        offset_from: docboss_model::PageBorderOffset::Page,
        ..Default::default()
    });
    let layout = laid(&document);
    let lines = border_lines(&layout, 0);
    assert_eq!(lines[1], ((579.0, 24.0), (579.0, 762.0), 6.0));
    let shadows: Vec<Rect> = layout.pages[0]
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Rect { rect, color } if *color == docboss_model::Color::BLACK => Some(*rect),
            _ => None,
        })
        .collect();
    assert_eq!(shadows.len(), 2);
    assert_eq!(shadows[0], Rect::new(582.0, 30.0, 6.0, 738.0));
    assert_eq!(shadows[1], Rect::new(30.0, 762.0, 558.0, 6.0));

    document.sections[0].properties.page_borders = Some(docboss_model::PageBorders {
        sides: all(BorderStyle::Art, false),
        ..Default::default()
    });
    let layout = laid(&document);
    assert!(border_lines(&layout, 0).is_empty());
    assert!(layout
        .diagnostics
        .iter()
        .any(|d| d.message.contains("art page borders")));
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

/// ECMA-376 Part 1 §17.4.66 and §17.15.3.4: the border above a row, and
/// the last row's bottom border, count in the row's height; a table in a
/// document laid out like Word 2007 (no compatibilityMode 15) moves left by
/// its first cell's left margin, so the cell text lines up with the margin.
#[test]
fn table_borders_take_row_height_and_old_tables_outdent() {
    let cell = |text: &str| TableCell {
        blocks: vec![para(text)],
        ..TableCell::default()
    };
    let table = |thick: bool| {
        let mut line = border();
        if thick {
            line.size = 32;
        }
        Block::Table(Table {
            properties: TableProperties {
                borders: Some(Borders {
                    top: Some(line),
                    bottom: Some(line),
                    inside_horizontal: Some(line),
                    ..Borders::default()
                }),
                ..TableProperties::default()
            },
            grid: vec![2880],
            rows: vec![
                TableRow {
                    cells: vec![cell("a1")],
                    ..TableRow::default()
                },
                TableRow {
                    cells: vec![cell("a2")],
                    ..TableRow::default()
                },
            ],
        })
    };
    let pitch = |document: &Document| {
        let layout = laid(document);
        let runs = runs(&layout, 0);
        let baseline = |text: &str| runs.iter().find(|r| r.text == text).unwrap().baseline;
        (
            baseline("a2") - baseline("a1"),
            runs.iter().find(|r| r.text == "a1").unwrap().glyphs[0].x,
        )
    };
    let (thin, x_new) = pitch(&doc(vec![table(false)]));
    let (thick, _) = pitch(&doc(vec![table(true)]));
    assert!((thick - thin - 3.5).abs() < 0.01, "{thin} {thick}");
    assert!((x_new - (72.0 + 5.4)).abs() < 0.01);
    let mut old = doc(vec![table(false)]);
    old.settings.compatibility_mode = Some(12);
    let (_, x_old) = pitch(&old);
    assert!((x_old - 72.0).abs() < 0.01, "{x_old}");
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
        content: vec![RunContent::Drawing(Box::new(docboss_model::Drawing {
            media: None,
            width: 2_540_000,
            height: 1_270_000,
            wrap: Default::default(),
            placement,
            name: None,
            description: None,
            text_box: blocks,
            shape,
            geometry: None,
            members: Vec::new(),
            data_text: Vec::new(),
            chart: None,
        }))],
    })
}

fn framed() -> docboss_model::ShapeFormat {
    docboss_model::ShapeFormat {
        fill: Some(docboss_model::Color(255, 255, 0)),
        outline: Some(docboss_model::Color::BLACK),
        outline_width: Some(12_700),
        insets: Some([127_000, 127_000, 127_000, 127_000]),
        ..docboss_model::ShapeFormat::default()
    }
}

fn anchored(x: i64, y: i64) -> docboss_model::DrawingPlacement {
    docboss_model::DrawingPlacement::Anchored {
        horizontal: DrawingPosition::offset(PositionBase::Column, x),
        vertical: DrawingPosition::offset(PositionBase::Paragraph, y),
        behind_text: false,
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
        .filter(|item| matches!(item, Item::Outline { width, .. } if (*width - 1.0).abs() < 0.01))
        .count();
    assert_eq!(outline, 1);
    assert!(!page
        .items
        .iter()
        .any(|item| matches!(item, Item::Image { .. })));
}

fn fill_of(layout: &Layout) -> docboss_layout::Rect {
    layout.pages[0]
        .items
        .iter()
        .find_map(|item| match item {
            Item::Rect { rect, color } if *color == docboss_model::Color(255, 255, 0) => {
                Some(*rect)
            }
            _ => None,
        })
        .expect("the text box fill is painted")
}

/// ECMA-376 Part 1 §20.4.2.1, §20.4.2.2, §20.4.3.1, §20.4.3.2, §20.4.3.4,
/// §20.4.3.5: an aligned float sits at the start, center or end of the
/// page, margin or column it is aligned on; inside is the start on an odd
/// page.
#[test]
fn aligned_floats_sit_on_their_base() {
    let aligned = |base, align| DrawingPosition {
        base,
        align: Some(align),
        offset: 0,
    };
    let place = |horizontal, vertical| {
        let placement = docboss_model::DrawingPlacement::Anchored {
            horizontal,
            vertical,
            behind_text: false,
        };
        fill_of(&laid(&doc(vec![Block::Paragraph(Paragraph {
            inlines: vec![
                run("Anchor"),
                text_box(placement, framed(), vec![para("Boxed")]),
            ],
            ..Paragraph::default()
        })])))
    };
    let cases = [
        (
            aligned(PositionBase::Page, PositionAlign::End),
            aligned(PositionBase::Margin, PositionAlign::End),
            (412.0, 620.0),
        ),
        (
            aligned(PositionBase::Margin, PositionAlign::Center),
            aligned(PositionBase::Page, PositionAlign::Center),
            (206.0, 346.0),
        ),
        (
            aligned(PositionBase::Page, PositionAlign::Inside),
            aligned(PositionBase::BottomMargin, PositionAlign::Start),
            (0.0, 720.0),
        ),
        (
            aligned(PositionBase::RightMargin, PositionAlign::Start),
            DrawingPosition::offset(PositionBase::TopMargin, 127_000),
            (540.0, 10.0),
        ),
    ];
    for (horizontal, vertical, (x, y)) in cases {
        let fill = place(horizontal, vertical);
        assert!(
            (fill.x - x).abs() < 0.01 && (fill.y - y).abs() < 0.01,
            "{horizontal:?} {vertical:?}: {fill:?}"
        );
    }
}

/// ECMA-376 Part 1 §20.4.2.1, §20.4.2.10, §20.4.3.1, §20.4.3.4: the Word
/// text box in the fixture is centered on its column: its 188.1 pt outline
/// sits halfway between the 70.85 pt margins of the 612 pt page.
#[test]
fn word_text_box_is_centered_on_its_column() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../docboss-docx/tests/fixtures/dml-picture-in-textframe.docx"
    );
    let document = docboss_docx::read(&std::fs::read(path).unwrap()).unwrap();
    let layout = laid(&document);
    let left = layout.pages[0]
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Outline { rect, .. } => Some(rect.x),
            _ => None,
        })
        .fold(f32::MAX, f32::min);
    let width = 2_388_870.0 / 12_700.0;
    let expected = 70.85 + (612.0 - 2.0 * 70.85 - width) / 2.0;
    assert!((left - expected).abs() < 0.05, "{left} {expected}");
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
        content: vec![RunContent::Drawing(Box::new(docboss_model::Drawing {
            media: None,
            width: 1_270_000,
            height: 1_270_000,
            wrap: Default::default(),
            placement: docboss_model::DrawingPlacement::Inline,
            name: None,
            description: None,
            text_box: Vec::new(),
            shape: Default::default(),
            geometry: None,
            members: Vec::new(),
            data_text: Vec::new(),
            chart: None,
        }))],
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

/// ECMA-376 Part 1 §17.3.2.24: a run's position raises or lowers an inline
/// picture with it: its line holds the part above the baseline and the
/// part below.
#[test]
fn run_position_lowers_inline_pictures() {
    let picture = |position: Option<i32>| {
        Inline::Run(Run {
            properties: RunProperties {
                position,
                ..RunProperties::default()
            },
            content: vec![RunContent::Drawing(Box::new(docboss_model::Drawing {
                media: None,
                width: 254_000,
                height: 254_000,
                wrap: Default::default(),
                placement: docboss_model::DrawingPlacement::Inline,
                name: None,
                description: None,
                text_box: Vec::new(),
                shape: Default::default(),
                geometry: None,
                members: Vec::new(),
                data_text: Vec::new(),
                chart: None,
            }))],
        })
    };
    let measure = |position: Option<i32>| {
        let layout = laid(&doc(vec![
            para_with(
                ParagraphProperties::default(),
                vec![run("Let"), picture(position)],
            ),
            para("next"),
        ]));
        let image = layout.pages[0]
            .items
            .iter()
            .find_map(|item| match item {
                Item::Image { rect, .. } => Some(*rect),
                _ => None,
            })
            .expect("picture");
        let baseline = |text: &str| {
            runs(&layout, 0)
                .into_iter()
                .find(|r| r.text == text)
                .map(|r| r.baseline)
                .expect("run")
        };
        (image.y + image.height - baseline("Let"), baseline("next"))
    };
    let (flat_drop, flat_next) = measure(None);
    let (lowered_drop, lowered_next) = measure(Some(-12));
    let (sunk_drop, sunk_next) = measure(Some(-40));
    assert!(flat_drop.abs() < 0.01, "{flat_drop}");
    assert!((lowered_drop - 6.0).abs() < 0.01, "{lowered_drop}");
    assert!((sunk_drop - 20.0).abs() < 0.01, "{sunk_drop}");
    assert!(lowered_next < flat_next, "{lowered_next} {flat_next}");
    assert!(sunk_next > flat_next + 5.0, "{sunk_next} {flat_next}");
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

fn shape(geometry: docboss_model::Geometry, shape: docboss_model::ShapeFormat) -> Block {
    para_with(
        ParagraphProperties::default(),
        vec![Inline::Run(Run {
            properties: RunProperties::default(),
            content: vec![RunContent::Drawing(Box::new(docboss_model::Drawing {
                media: None,
                width: 1_270_000,
                height: 635_000,
                wrap: Default::default(),
                placement: anchored(914_400, 914_400),
                name: None,
                description: None,
                text_box: Vec::new(),
                shape,
                geometry: Some(Box::new(geometry)),
                members: Vec::new(),
                data_text: Vec::new(),
                chart: None,
            }))],
        })],
    )
}

fn paths(layout: &Layout) -> Vec<&Item> {
    layout.pages[0]
        .items
        .iter()
        .filter(|item| matches!(item, Item::Path { .. }))
        .collect()
}

/// A shape without text paints its preset geometry: the fill under the
/// outline, no placeholder, and a rectangle keeps its rectangle items.
/// ECMA-376 Part 1 §20.1.9.18, §20.1.10.56.
#[test]
fn shapes_without_text_paint_their_geometry() {
    let triangle = docboss_model::Geometry::Preset {
        name: "triangle".into(),
        adjust: Vec::new(),
    };
    let layout = laid(&doc(vec![shape(triangle, framed())]));
    let items = paths(&layout);
    assert_eq!(items.len(), 2, "{:?}", layout.pages[0].items);
    let Item::Path {
        segs, fill, stroke, ..
    } = items[0]
    else {
        unreachable!()
    };
    assert_eq!(*fill, Some(docboss_model::Color(255, 255, 0)));
    assert!(stroke.is_none());
    assert_eq!(segs[0], docboss_font::Seg::Move(144.0, 194.0));
    assert_eq!(segs[1], docboss_font::Seg::Line(194.0, 144.0));
    let Item::Path { fill, stroke, .. } = items[1] else {
        unreachable!()
    };
    assert!(fill.is_none() && stroke.unwrap().width == 1.0);
    assert!(!layout.pages[0]
        .items
        .iter()
        .any(|item| matches!(item, Item::Image { .. })));
    let layout = laid(&doc(vec![shape(
        docboss_model::Geometry::rectangle(),
        framed(),
    )]));
    assert!(paths(&layout).is_empty());
    assert!(layout.pages[0]
        .items
        .iter()
        .any(|item| matches!(item, Item::Outline { .. })));
}

/// A flipped, turned line with a triangle at its tail: the flip picks the
/// other diagonal, the turn goes about the center, and the arrowhead
/// sits at the end with the line pulled back under it.
/// ECMA-376 Part 1 §20.1.7.6, §20.1.8.57, §20.1.10.33.
#[test]
fn lines_flip_turn_and_end_in_arrowheads() {
    let line = docboss_model::Geometry::Preset {
        name: "line".into(),
        adjust: Vec::new(),
    };
    let format = docboss_model::ShapeFormat {
        outline: Some(docboss_model::Color::BLACK),
        outline_width: Some(12_700),
        flip_vertical: true,
        tail_end: Some(docboss_model::LineEnd {
            kind: docboss_model::LineEndKind::Triangle,
            width: docboss_model::LineEndSize::Medium,
            length: docboss_model::LineEndSize::Medium,
        }),
        ..docboss_model::ShapeFormat::default()
    };
    let layout = laid(&doc(vec![shape(line.clone(), format)]));
    let items = paths(&layout);
    assert_eq!(items.len(), 2);
    let Item::Path { segs, .. } = items[0] else {
        unreachable!()
    };
    assert_eq!(segs[0], docboss_font::Seg::Move(144.0, 194.0));
    let docboss_font::Seg::Line(x, y) = segs[1] else {
        panic!("{segs:?}")
    };
    assert!(x < 244.0 && y > 144.0, "{x} {y}");
    let Item::Path { segs, fill, .. } = items[1] else {
        unreachable!()
    };
    assert_eq!(*fill, Some(docboss_model::Color::BLACK));
    assert_eq!(segs[0], docboss_font::Seg::Move(244.0, 144.0));
    let turned = docboss_model::ShapeFormat {
        rotation: 5_400_000,
        tail_end: None,
        ..format
    };
    let layout = laid(&doc(vec![shape(line, turned)]));
    let Item::Path { segs, .. } = paths(&layout)[0] else {
        unreachable!()
    };
    let docboss_font::Seg::Move(x, y) = segs[0] else {
        panic!("{segs:?}")
    };
    assert!(
        (x - 169.0).abs() < 0.01 && (y - 119.0).abs() < 0.01,
        "{x} {y}"
    );
}

/// A group paints each member at its box inside the group: pictures as
/// images, shapes with a gradient as shaded outlines laid over their box.
/// ECMA-376 Part 1 §20.4.2.39, §20.1.8.33.
#[test]
fn groups_paint_their_members_in_place() {
    let red = docboss_model::Color(255, 0, 0);
    let gradient = docboss_model::Gradient::new(&[(0, red), (100_000, red)]);
    let square = docboss_model::Drawing {
        width: 635_000,
        height: 635_000,
        geometry: Some(Box::new(docboss_model::Geometry::rectangle())),
        shape: docboss_model::ShapeFormat {
            fill: Some(red),
            gradient,
            ..docboss_model::ShapeFormat::default()
        },
        ..docboss_model::Drawing::default()
    };
    let picture = docboss_model::Drawing {
        width: 254_000,
        height: 127_000,
        media: Some(docboss_model::MediaId(0)),
        ..docboss_model::Drawing::default()
    };
    let group = docboss_model::Drawing {
        width: 1_270_000,
        height: 635_000,
        placement: anchored(0, 0),
        members: vec![
            docboss_model::GroupMember {
                x: 0,
                y: 0,
                drawing: square,
            },
            docboss_model::GroupMember {
                x: 635_000,
                y: 127_000,
                drawing: picture,
            },
        ],
        ..docboss_model::Drawing::default()
    };
    let layout = laid(&doc(vec![para_with(
        ParagraphProperties::default(),
        vec![Inline::Run(Run {
            properties: RunProperties::default(),
            content: vec![RunContent::Drawing(Box::new(group))],
        })],
    )]));
    let items = &layout.pages[0].items;
    let (frame, size) = items
        .iter()
        .find_map(|item| match item {
            Item::Shade { frame, size, .. } => Some((*frame, *size)),
            _ => None,
        })
        .expect("a shaded square");
    assert_eq!(size, (50.0, 50.0));
    let images: Vec<_> = items
        .iter()
        .filter_map(|item| match item {
            Item::Image { rect, .. } => Some(*rect),
            _ => None,
        })
        .collect();
    assert_eq!(images.len(), 1, "{items:?}");
    assert!((images[0].x - frame[4] - 50.0).abs() < 0.01);
    assert!((images[0].y - frame[5] - 10.0).abs() < 0.01);
    assert_eq!((images[0].width, images[0].height), (20.0, 10.0));
}

fn transforms(layout: &Layout) -> Vec<[f32; 6]> {
    layout.pages[0]
        .items
        .iter()
        .filter_map(|item| match item {
            Item::TransformBegin(m) => Some(*m),
            _ => None,
        })
        .collect()
}

/// A text box's text turns with its shape unless it stays upright, and
/// vertical text turns a quarter inside the box.
/// ECMA-376 Part 1 §20.1.7.6, §20.4.2.22, §20.1.10.83.
#[test]
fn text_boxes_turn_their_text() {
    let turned = docboss_model::ShapeFormat {
        rotation: 5_400_000,
        ..framed()
    };
    let layout = laid(&doc(vec![Block::Paragraph(Paragraph {
        inlines: vec![text_box(anchored(0, 0), turned, vec![para("turned")])],
        ..Paragraph::default()
    })]));
    let maps = transforms(&layout);
    assert_eq!(maps.len(), 1);
    let [a, b, c, d, ..] = maps[0];
    assert!(a.abs() < 1e-6 && (b - 1.0).abs() < 1e-6 && (c + 1.0).abs() < 1e-6 && d.abs() < 1e-6);
    let upright = docboss_model::ShapeFormat {
        text_upright: true,
        ..turned
    };
    let layout = laid(&doc(vec![Block::Paragraph(Paragraph {
        inlines: vec![text_box(anchored(0, 0), upright, vec![para("upright")])],
        ..Paragraph::default()
    })]));
    assert!(transforms(&layout).is_empty());
    let vertical = docboss_model::ShapeFormat {
        text_direction: docboss_model::TextDirection::BottomToTop,
        ..framed()
    };
    let layout = laid(&doc(vec![Block::Paragraph(Paragraph {
        inlines: vec![text_box(anchored(0, 0), vertical, vec![para("up")])],
        ..Paragraph::default()
    })]));
    let maps = transforms(&layout);
    assert_eq!(maps.len(), 1);
    assert_eq!(&maps[0][..4], &[0.0, -1.0, 1.0, 0.0]);
}

/// A bottom-to-top cell turns its text into the cell, and the row grows
/// to hold its unwrapped line.
/// ECMA-376 Part 1 §17.4.72.
#[test]
fn turned_cells_rotate_their_text() {
    let cell = TableCell {
        properties: docboss_model::TableCellProperties {
            text_direction: docboss_model::TextDirection::BottomToTop,
            ..Default::default()
        },
        blocks: vec![para("a long turned line")],
    };
    let table = Table {
        grid: vec![1440],
        rows: vec![TableRow {
            cells: vec![cell],
            ..TableRow::default()
        }],
        ..Table::default()
    };
    let layout = laid(&doc(vec![Block::Table(table)]));
    let maps = transforms(&layout);
    assert_eq!(maps.len(), 1);
    assert_eq!(&maps[0][..4], &[0.0, -1.0, 1.0, 0.0]);
    assert!(maps[0][5] > 100.0, "{:?}", maps[0]);
}

const NOTO_ARABIC: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../docboss-font/tests/fixtures/NotoSansArabic-Subset.ttf"
);
const NOTO_HEBREW: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../docboss-font/tests/fixtures/NotoSansHebrew-Subset.ttf"
);

fn script_fonts() -> Arc<FontDatabase> {
    let mut db = FontDatabase::new();
    db.add_file(Path::new(COUSINE));
    db.add_file(Path::new(NOTO_ARABIC));
    db.add_file(Path::new(NOTO_HEBREW));
    Arc::new(db)
}

fn complex_run(text: &str, complex: &str, rtl: bool) -> Inline {
    let mut properties = RunProperties::default();
    properties.fonts.complex = Some(complex.into());
    properties.right_to_left = rtl.then_some(true);
    Inline::Run(Run {
        properties,
        content: vec![RunContent::Text(text.into())],
    })
}

fn rtl_para(inlines: Vec<Inline>) -> Block {
    para_with(
        ParagraphProperties {
            bidi: Some(true),
            ..ParagraphProperties::default()
        },
        inlines,
    )
}

/// Every glyph of the first page with the text of its run, in painting order.
fn placed_glyphs(layout: &Layout) -> Vec<(String, u16, f32)> {
    runs(layout, 0)
        .iter()
        .flat_map(|r| r.glyphs.iter().map(|g| (r.text.clone(), g.id, g.x)))
        .collect()
}

/// ECMA-376 Part 1 §17.3.1.6 and §17.3.1.13: a right-to-left paragraph
/// starts at the right margin; its Hebrew runs right to left and an
/// English word inside it keeps its own order. Run text stays in logical
/// order.
#[test]
fn right_to_left_paragraphs_start_at_the_right_margin() {
    let layout = layout(
        &doc(vec![rtl_para(vec![
            complex_run("שלום ", "Noto Sans Hebrew", true),
            run("abc"),
        ])]),
        &script_fonts(),
    );
    let runs = runs(&layout, 0);
    let hebrew = runs.iter().find(|r| r.text == "שלום").unwrap();
    let xs: Vec<f32> = hebrew.glyphs.iter().map(|g| g.x).collect();
    assert!(xs.windows(2).all(|w| w[1] < w[0]), "{xs:?}");
    assert!(
        xs[0] > 612.0 - 72.0 - 12.0 && xs[0] < 612.0 - 72.0,
        "{xs:?}"
    );
    let latin = runs.iter().find(|r| r.text == "abc").unwrap();
    let ls: Vec<f32> = latin.glyphs.iter().map(|g| g.x).collect();
    assert!(ls.windows(2).all(|w| w[1] > w[0]), "{ls:?}");
    assert!(
        ls[2] < xs[3],
        "the English word sits left of the Hebrew: {ls:?} {xs:?}"
    );
}

/// ECMA-376 Part 1 §17.3.1.13: `end` alignment in a right-to-left
/// paragraph puts its text at the left margin.
#[test]
fn end_alignment_of_a_right_to_left_paragraph_is_the_left_margin() {
    let layout = layout(
        &doc(vec![para_with(
            ParagraphProperties {
                bidi: Some(true),
                justification: Some(Justification::Right),
                ..ParagraphProperties::default()
            },
            vec![complex_run("שלום", "Noto Sans Hebrew", true)],
        )]),
        &script_fonts(),
    );
    let xs: Vec<f32> = runs(&layout, 0)[0].glyphs.iter().map(|g| g.x).collect();
    let left = xs.iter().copied().fold(f32::MAX, f32::min);
    assert!((left - 72.0).abs() < 0.5, "{xs:?}");
}

/// ECMA-376 Part 1 §17.3.2.30: brackets of a right-to-left run are drawn
/// mirrored, and digits inside it keep their left-to-right order.
#[test]
fn right_to_left_runs_mirror_brackets_and_keep_numbers_in_order() {
    let layout = layout(
        &doc(vec![rtl_para(vec![complex_run(
            "(א) 12",
            "Noto Sans Hebrew",
            true,
        )])]),
        &script_fonts(),
    );
    let fonts = script_fonts();
    let glyphs = placed_glyphs(&layout);
    let run = runs(&layout, 0)[0];
    let face = fonts.font(run.font).unwrap();
    let (open, close) = (
        face.glyph_index('(').unwrap(),
        face.glyph_index(')').unwrap(),
    );
    assert_eq!(glyphs[0].1, close, "{glyphs:?}");
    assert_eq!(glyphs[2].1, open, "{glyphs:?}");
    assert!(glyphs[0].2 > glyphs[2].2);
    let one = glyphs
        .iter()
        .find(|g| g.1 == face.glyph_index('1').unwrap())
        .unwrap();
    let two = glyphs
        .iter()
        .find(|g| g.1 == face.glyph_index('2').unwrap())
        .unwrap();
    assert!(one.2 < two.2, "{glyphs:?}");
    assert!(two.2 < glyphs[2].2, "the number sits left of the brackets");
}

/// Arabic letters take their joined forms and marks attach to them; the
/// glyphs are those HarfBuzz gives Noto Sans Arabic.
#[test]
fn arabic_words_join_and_carry_their_marks() {
    let layout = layout(
        &doc(vec![rtl_para(vec![complex_run(
            "بِسْمِ",
            "Noto Sans Arabic",
            true,
        )])]),
        &script_fonts(),
    );
    let runs = runs(&layout, 0);
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].text, "بِسْمِ");
    let mut ids: Vec<u16> = runs[0].glyphs.iter().map(|g| g.id).collect();
    ids.sort_unstable();
    assert_eq!(ids, vec![14, 28, 67, 220, 249, 269, 269]);
    let kasra_below = runs[0]
        .glyphs
        .iter()
        .filter(|g| g.id == 269)
        .map(|g| g.y)
        .fold(f32::MIN, f32::max);
    assert!(kasra_below > 0.5, "a kasra hangs below the baseline");
}

/// ECMA-376 Part 1 §17.3.2.26 (step 2a), §17.3.2.2, §17.3.2.7 and
/// §17.3.2.39: Arabic characters take the complex script font, size and
/// bold; Latin ones keep the run's own, unless the run asks for complex
/// script formatting.
#[test]
fn complex_script_characters_take_the_complex_script_formatting() {
    let mut properties = RunProperties {
        size: Some(20),
        size_complex: Some(40),
        bold_complex: Some(true),
        ..RunProperties::default()
    };
    properties.fonts.complex = Some("Noto Sans Arabic".into());
    let mixed = Inline::Run(Run {
        properties: properties.clone(),
        content: vec![RunContent::Text("ab بسم".into())],
    });
    let forced = Inline::Run(Run {
        properties: RunProperties {
            complex_script: Some(true),
            ..properties
        },
        content: vec![RunContent::Text("cd".into())],
    });
    let layout = layout(
        &doc(vec![para_with(
            ParagraphProperties::default(),
            vec![mixed, forced],
        )]),
        &script_fonts(),
    );
    let fonts = script_fonts();
    let runs = runs(&layout, 0);
    let latin = runs.iter().find(|r| r.text == "ab").unwrap();
    assert_eq!(latin.size, 10.0);
    let arabic = runs.iter().find(|r| r.text == "بسم").unwrap();
    assert_eq!(arabic.size, 20.0);
    assert_eq!(
        fonts.font(arabic.font).unwrap().names().family,
        "Noto Sans Arabic"
    );
    assert!(arabic.synthetic_bold);
    let forced = runs.iter().find(|r| r.text == "cd").unwrap();
    assert_eq!(forced.size, 20.0);
}

/// ECMA-376 Part 1 §17.4.1: a visually right-to-left table puts its first
/// cell at the right.
#[test]
fn visually_right_to_left_tables_start_at_the_right() {
    let cell = |text: &str| TableCell {
        blocks: vec![para(text)],
        ..TableCell::default()
    };
    let table = Table {
        properties: TableProperties {
            bidi_visual: true,
            ..TableProperties::default()
        },
        grid: vec![2880, 2880],
        rows: vec![TableRow {
            cells: vec![cell("one"), cell("two")],
            ..TableRow::default()
        }],
    };
    let layout = laid(&doc(vec![Block::Table(table)]));
    let runs = runs(&layout, 0);
    let x_of = |text: &str| runs.iter().find(|r| r.text == text).unwrap().glyphs[0].x;
    assert!(x_of("one") > x_of("two"));
    assert!(
        (x_of("one") - (612.0 - 72.0 - 144.0 + 5.4)).abs() < 0.01,
        "{}",
        x_of("one")
    );
}

fn chart_drawing(kind: docboss_model::ChartKind, values: Vec<Option<f64>>) -> Block {
    use docboss_model::{Chart, ChartAxis, ChartPlot, ChartSeries, ChartText, Color};
    let series = ChartSeries {
        name: Some("Sales".into()),
        categories: vec!["North".into(), "South".into(), "East".into()],
        values,
        fill: Some(Color(255, 0, 0)),
        line: None,
        point_fills: vec![Some(Color(0, 0, 255)), None, Some(Color(0, 255, 0))],
        ..ChartSeries::default()
    };
    let axis = |side| ChartAxis {
        side,
        labels: true,
        text: ChartText::default(),
        gridlines: Some(Color(200, 200, 200)),
        ..ChartAxis::default()
    };
    let chart = Chart {
        title: Some(ChartText {
            text: "Title".into(),
            ..ChartText::default()
        }),
        plots: vec![ChartPlot {
            kind,
            series: vec![series],
        }],
        category_axis: Some(axis(docboss_model::ChartSide::Bottom)),
        value_axis: Some(axis(docboss_model::ChartSide::Left)),
        ..Chart::default()
    };
    let drawing = docboss_model::Drawing {
        width: 3_810_000,
        height: 2_540_000,
        chart: Some(Box::new(chart)),
        ..docboss_model::Drawing::default()
    };
    para_with(
        ParagraphProperties::default(),
        vec![Inline::Run(Run {
            properties: RunProperties::default(),
            content: vec![RunContent::Drawing(Box::new(drawing))],
        })],
    )
}

/// A column chart draws one bar per cached value, taller for larger
/// values, with its title, category and tick labels; a pie draws one
/// slice per value in the points' colors.
/// ECMA-376 Part 1 §21.2.2.16, §21.2.2.141, §21.2.2.25, §21.2.2.226.
#[test]
fn charts_draw_bars_labels_and_slices() {
    let column = docboss_model::ChartKind::Bar {
        horizontal: false,
        grouping: docboss_model::ChartGrouping::Standard,
        gap: 150,
        overlap: 0,
    };
    let layout = laid(&doc(vec![chart_drawing(
        column,
        vec![Some(1.0), Some(3.0), None],
    )]));
    let items = &layout.pages[0].items;
    let bars: Vec<docboss_layout::Rect> = items
        .iter()
        .filter_map(|item| match item {
            Item::Rect { rect, color } if color.0 == 255 || color.2 == 255 => Some(*rect),
            _ => None,
        })
        .collect();
    assert_eq!(bars.len(), 2, "{bars:?}");
    assert!(bars[1].height > bars[0].height * 2.5);
    assert!((bars[0].bottom() - bars[1].bottom()).abs() < 0.01);
    assert!(bars[0].x < bars[1].x);
    let text: String = runs(&layout, 0).iter().map(|r| r.text.as_str()).collect();
    for word in ["Title", "North", "South", "East", "0", "3"] {
        assert!(text.contains(word), "{text}");
    }
    let pie = docboss_model::ChartKind::Pie {
        hole: 0,
        first_angle: 0,
    };
    let layout = laid(&doc(vec![chart_drawing(
        pie,
        vec![Some(1.0), Some(1.0), Some(2.0)],
    )]));
    let slices: Vec<Option<docboss_model::Color>> = layout.pages[0]
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Path { fill, .. } => Some(*fill),
            _ => None,
        })
        .collect();
    assert_eq!(
        slices,
        [
            Some(docboss_model::Color(0, 0, 255)),
            Some(docboss_model::Color(255, 0, 0)),
            Some(docboss_model::Color(0, 255, 0))
        ]
    );
}

/// A fraction draws its numerator above a rule and its denominator below
/// it on the line's baseline; a display equation is centered.
/// ECMA-376 Part 1 §22.1.2.36, §22.1.2.78.
#[test]
fn fractions_stack_over_a_rule_and_display_math_centers() {
    use docboss_model::{FractionKind, Math, MathJustification, MathNode, MathStyle};
    let m = |text: &str| {
        vec![MathNode::Run {
            text: text.into(),
            style: MathStyle::Plain,
        }]
    };
    let math = Math {
        display: true,
        justification: MathJustification::Center,
        nodes: vec![MathNode::Fraction {
            kind: FractionKind::Bar,
            numerator: m("1"),
            denominator: m("2"),
        }],
    };
    let layout = laid(&doc(vec![para_with(
        ParagraphProperties::default(),
        vec![Inline::Run(Run {
            properties: RunProperties::default(),
            content: vec![RunContent::Math(Box::new(math))],
        })],
    )]));
    let page = &layout.pages[0];
    let rule = page
        .items
        .iter()
        .find_map(|item| match item {
            Item::Rect { rect, .. } => Some(*rect),
            _ => None,
        })
        .expect("a fraction rule");
    let glyphs: Vec<(f32, f32)> = runs(&layout, 0)
        .iter()
        .flat_map(|r| r.glyphs.iter().map(move |g| (g.x, r.baseline)))
        .collect();
    assert_eq!(glyphs.len(), 2, "{glyphs:?}");
    assert!(glyphs[0].1 < rule.y && glyphs[1].1 > rule.bottom());
    let center = rule.x + rule.width / 2.0;
    assert!((center - page.width / 2.0).abs() < 2.0, "{center}");
}

fn wrapped(
    kind: docboss_model::WrapKind,
    side: docboss_model::WrapSide,
) -> docboss_model::TextWrap {
    docboss_model::TextWrap {
        kind,
        side,
        distance: [0, 0, 127_000, 127_000],
        polygon: Vec::new(),
    }
}

/// A 100 by 100 point placeholder picture wrapped as `wrap` says.
fn floating_picture(
    placement: docboss_model::DrawingPlacement,
    wrap: docboss_model::TextWrap,
) -> Inline {
    Inline::Run(Run {
        properties: RunProperties::default(),
        content: vec![RunContent::Drawing(Box::new(docboss_model::Drawing {
            width: 1_270_000,
            height: 1_270_000,
            placement,
            wrap,
            ..docboss_model::Drawing::default()
        }))],
    })
}

fn placeholders(layout: &Layout, page: usize) -> Vec<Rect> {
    layout.pages[page]
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Image { media: None, rect } => Some(*rect),
            _ => None,
        })
        .collect()
}

/// The left and right edges of each glyph run with its baseline.
fn run_spans(layout: &Layout, page: usize) -> Vec<(f32, f32, f32)> {
    runs(layout, page)
        .iter()
        .filter(|run| !run.glyphs.is_empty())
        .map(|run| {
            let left = run.glyphs[0].x;
            let right = run.glyphs[run.glyphs.len() - 1].x + ADVANCE;
            (run.baseline, left, right)
        })
        .collect()
}

fn words(count: usize) -> String {
    "lorem ipsum dolor sit amet ".repeat(count)
}

/// Square wrapping on both sides flows text left and right of the picture
/// on the same lines, its 10 point distances away, and none into it.
/// ECMA-376 Part 1 §20.4.2.17, §20.4.3.7, §20.4.3.6.
#[test]
fn square_wrapping_flows_text_on_both_sides() {
    use docboss_model::{WrapKind, WrapSide};
    let picture = floating_picture(
        anchored(2_540_000, 0),
        wrapped(WrapKind::Square, WrapSide::Both),
    );
    let layout = laid(&doc(vec![Block::Paragraph(Paragraph {
        inlines: vec![picture, run(&words(30))],
        ..Paragraph::default()
    })]));
    let rect = placeholders(&layout, 0)[0];
    assert_eq!((rect.x, rect.y), (272.0, 72.0));
    let spans = run_spans(&layout, 0);
    let beside: Vec<&(f32, f32, f32)> = spans.iter().filter(|s| s.0 < rect.bottom()).collect();
    assert!(!beside.is_empty());
    for (_, left, right) in &beside {
        assert!(
            *right <= rect.x - 10.0 + 0.01 || *left >= rect.right() + 10.0 - 0.01,
            "{left} {right}"
        );
    }
    let first = beside[0].0;
    assert!(beside.iter().any(|s| s.0 == first && s.1 < rect.x));
    assert!(beside.iter().any(|s| s.0 == first && s.1 > rect.right()));
    assert!(spans
        .iter()
        .any(|s| s.0 > rect.bottom() && s.1 < rect.x && s.2 > rect.right()));
}

/// A picture that lets text flow on its left only leaves the right of it
/// empty, top and bottom wrapping moves the lines it meets below it, and
/// text runs under a picture that does not wrap.
/// ECMA-376 Part 1 §20.4.3.7, §20.4.2.20, §20.4.2.15.
#[test]
fn one_sided_and_top_and_bottom_wrapping() {
    use docboss_model::{WrapKind, WrapSide};
    let none = floating_picture(anchored(0, 0), wrapped(WrapKind::None, WrapSide::Both));
    let layout = laid(&doc(vec![Block::Paragraph(Paragraph {
        inlines: vec![none, run(&words(30))],
        ..Paragraph::default()
    })]));
    assert_eq!(run_spans(&layout, 0)[0].1, 72.0);
    let left = floating_picture(
        anchored(2_540_000, 0),
        wrapped(WrapKind::Square, WrapSide::Left),
    );
    let layout = laid(&doc(vec![Block::Paragraph(Paragraph {
        inlines: vec![left, run(&words(30))],
        ..Paragraph::default()
    })]));
    let rect = placeholders(&layout, 0)[0];
    let spans = run_spans(&layout, 0);
    assert!(spans
        .iter()
        .filter(|s| s.0 < rect.bottom())
        .all(|s| s.2 <= rect.x));
    let blocking = floating_picture(
        anchored(0, 0),
        wrapped(WrapKind::TopAndBottom, WrapSide::Both),
    );
    let layout = laid(&doc(vec![Block::Paragraph(Paragraph {
        inlines: vec![blocking, run(&words(30))],
        ..Paragraph::default()
    })]));
    let rect = placeholders(&layout, 0)[0];
    let spans = run_spans(&layout, 0);
    assert!(spans.iter().all(|s| s.0 > rect.bottom()), "{spans:?}");
}

/// ECMA-376 Part 1 §20.4.2.17: an object keeps text out only on its own
/// page: a paragraph whose picture moves to the next page takes the
/// wrapping with it, and the lines left on the first page stay full.
#[test]
fn wrapping_moves_with_its_page() {
    use docboss_model::{WrapKind, WrapSide};
    let mut blocks: Vec<Block> = (0..55).map(|i| para(&format!("line {i}"))).collect();
    let picture = floating_picture(anchored(0, 0), wrapped(WrapKind::Square, WrapSide::Both));
    blocks.push(Block::Paragraph(Paragraph {
        inlines: vec![picture, run(&words(20))],
        properties: ParagraphProperties {
            keep_lines: Some(true),
            ..ParagraphProperties::default()
        },
        ..Paragraph::default()
    }));
    let layout = laid(&doc(blocks));
    assert!(placeholders(&layout, 0).is_empty());
    let rect = placeholders(&layout, 1)[0];
    assert_eq!(rect.y, 72.0);
    let spans = run_spans(&layout, 1);
    assert!(spans
        .iter()
        .filter(|s| s.0 < rect.bottom())
        .all(|s| s.1 >= rect.right()));
}

/// ECMA-376 Part 1 §20.4.2.17, §20.4.3.5: a picture placed on the page
/// above text already laid out there lays the page out again, so the
/// paragraphs before its own flow around it too.
#[test]
fn a_page_placed_picture_wraps_the_text_above_its_anchor() {
    use docboss_model::{WrapKind, WrapSide};
    let placement = docboss_model::DrawingPlacement::Anchored {
        horizontal: DrawingPosition::offset(PositionBase::Margin, 0),
        vertical: DrawingPosition::offset(PositionBase::Page, 914_400),
        behind_text: false,
    };
    let picture = floating_picture(placement, wrapped(WrapKind::Square, WrapSide::Both));
    let layout = laid(&doc(vec![
        para(&words(10)),
        para(&words(10)),
        Block::Paragraph(Paragraph {
            inlines: vec![picture, run("anchor")],
            ..Paragraph::default()
        }),
    ]));
    let rect = placeholders(&layout, 0)[0];
    assert_eq!((rect.x, rect.y), (72.0, 72.0));
    let spans = run_spans(&layout, 0);
    let first = spans[0];
    assert!(
        first.0 < rect.bottom() && first.1 >= rect.right(),
        "{first:?}"
    );
}

/// ECMA-376 Part 1 §17.3.1.11, §17.18.104, §17.18.35, §17.18.100: a text frame is laid out apart
/// at its width and position, and the next paragraph flows around it,
/// `hSpace` away.
#[test]
fn text_frames_sit_apart_and_text_flows_around_them() {
    use docboss_model::{DropCap, FrameProperties, LineRule, WrapKind};
    let frame = FrameProperties {
        width: Some(1440),
        height: 1440,
        height_rule: LineRule::Exact,
        horizontal: DrawingPosition::offset(PositionBase::Margin, 0),
        vertical: DrawingPosition::offset(PositionBase::Paragraph, 0),
        wrap: WrapKind::Square,
        h_space: 200,
        v_space: 0,
        drop_cap: DropCap::None,
        lines: 1,
    };
    let framed = para_with(
        ParagraphProperties {
            frame: Some(frame),
            ..ParagraphProperties::default()
        },
        vec![run("framed")],
    );
    let layout = laid(&doc(vec![framed, para(&words(20))]));
    let spans = run_spans(&layout, 0);
    let framed = runs(&layout, 0)
        .into_iter()
        .find(|r| r.text == "framed")
        .expect("the frame's text is painted");
    assert_eq!(framed.glyphs[0].x, 72.0);
    let beside: Vec<_> = spans
        .iter()
        .filter(|s| s.0 < 72.0 + 72.0 && s.1 > 72.0)
        .collect();
    assert!(!beside.is_empty());
    assert!(
        beside.iter().all(|s| s.1 >= 72.0 + 72.0 + 10.0 - 0.01),
        "{beside:?}"
    );
}

/// ECMA-376 Part 1 §17.4.57: a floating table sits at its position, out of
/// the text flow, and the paragraph after it flows beside it.
#[test]
fn floating_tables_sit_at_their_position() {
    let cell = |text: &str| TableCell {
        properties: docboss_model::TableCellProperties {
            grid_span: 1,
            ..Default::default()
        },
        blocks: vec![para(text)],
    };
    let table = Block::Table(Table {
        properties: TableProperties {
            floating: Some(docboss_model::TableFloat {
                horizontal: DrawingPosition::offset(PositionBase::Page, 3_810_000),
                vertical: DrawingPosition::offset(PositionBase::Paragraph, 0),
                distance: [0, 0, 180, 180],
            }),
            ..TableProperties::default()
        },
        grid: vec![1440],
        rows: vec![TableRow {
            properties: Default::default(),
            cells: vec![cell("cell")],
        }],
    });
    let layout = laid(&doc(vec![table, para(&words(20))]));
    let cell_run = runs(&layout, 0)
        .into_iter()
        .find(|r| r.text == "cell")
        .expect("the cell text is painted");
    assert!(cell_run.glyphs[0].x > 300.0, "{}", cell_run.glyphs[0].x);
    let first = run_spans(&layout, 0)[0];
    assert!(
        first.0 < cell_run.baseline + 1.0 && first.1 < 300.0,
        "{first:?}"
    );
    assert!(run_spans(&layout, 0)
        .iter()
        .filter(|s| s.0 <= cell_run.baseline && s.1 < 300.0)
        .all(|s| s.2 <= 300.0 - 9.0 + 0.01));
}

/// ECMA-376 Part 1 §20.4.2.3: a floating picture inside a table cell is
/// painted, placed against its cell.
#[test]
fn floats_inside_table_cells_are_painted() {
    use docboss_model::{WrapKind, WrapSide};
    let picture = floating_picture(anchored(0, 0), wrapped(WrapKind::Square, WrapSide::Both));
    let cell = TableCell {
        properties: docboss_model::TableCellProperties {
            grid_span: 1,
            ..Default::default()
        },
        blocks: vec![Block::Paragraph(Paragraph {
            inlines: vec![picture, run("cell")],
            ..Paragraph::default()
        })],
    };
    let table = Block::Table(Table {
        grid: vec![2880, 2880],
        rows: vec![TableRow {
            properties: Default::default(),
            cells: vec![
                TableCell {
                    blocks: vec![para("first")],
                    ..cell.clone()
                },
                cell,
            ],
        }],
        ..Table::default()
    });
    let layout = laid(&doc(vec![table]));
    let rects = placeholders(&layout, 0);
    assert_eq!(rects.len(), 1);
    assert!(rects[0].x > 72.0 + 144.0, "{:?}", rects[0]);
}

/// A drop cap frame stands at the start of the next paragraph, whose lines
/// flow beside it.
/// ECMA-376 Part 1 §17.3.1.11, §17.18.20.
#[test]
fn drop_caps_start_their_paragraph() {
    use docboss_model::{DropCap, FrameProperties, LineRule, WrapKind};
    let frame = FrameProperties {
        width: None,
        height: 0,
        height_rule: LineRule::Auto,
        horizontal: DrawingPosition::offset(PositionBase::Page, 0),
        vertical: DrawingPosition::offset(PositionBase::Page, 0),
        wrap: WrapKind::Square,
        h_space: 0,
        v_space: 0,
        drop_cap: DropCap::Drop,
        lines: 2,
    };
    let cap = para_with(
        ParagraphProperties {
            frame: Some(frame),
            ..ParagraphProperties::default()
        },
        vec![run("W")],
    );
    let layout = laid(&doc(vec![para("before"), cap, para(&words(20))]));
    let letter = runs(&layout, 0)
        .into_iter()
        .find(|r| r.text == "W")
        .expect("the drop cap is painted");
    assert_eq!(letter.glyphs[0].x, 72.0);
    let after = run_spans(&layout, 0)
        .into_iter()
        .filter(|s| s.0 > letter.baseline - 1.0 && s.0 < letter.baseline + 1.0 && s.1 > 72.0)
        .count();
    assert!(after > 0);
}

use std::path::Path;
use std::sync::Arc;

use docboss_font::FontDatabase;
use docboss_layout::layout;
use docboss_model::{
    Block, Border, BorderStyle, Borders, Color, DashPattern, Document, Drawing, DrawingPlacement,
    DrawingPosition, Inline, LineCap, LineJoin, Media, MediaId, Paragraph, ParagraphProperties,
    PositionBase, Run, RunContent, RunProperties, Section, ShapeFormat, Style, StyleKind, Styles,
    Table, TableCell, TableProperties, TableRow,
};
use docboss_render::{render_page, render_pages, Format, Pixmap};

const COUSINE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../docboss-font/tests/fixtures/Cousine-Regular.ttf"
);

fn fonts() -> Arc<FontDatabase> {
    let mut db = FontDatabase::new();
    db.add_file(Path::new(COUSINE));
    Arc::new(db)
}

fn run(text: &str, properties: RunProperties) -> Inline {
    Inline::Run(Run {
        properties,
        content: vec![RunContent::Text(text.into())],
    })
}

fn para(inlines: Vec<Inline>) -> Block {
    Block::Paragraph(Paragraph {
        inlines,
        ..Paragraph::default()
    })
}

fn document(blocks: Vec<Block>) -> Document {
    let mut normal = Style::new("Normal", StyleKind::Paragraph);
    normal.is_default = true;
    normal.run.fonts.ascii = Some("Cousine".into());
    normal.run.size = Some(24);
    Document {
        styles: Styles::new(
            ParagraphProperties::default(),
            RunProperties::default(),
            vec![normal],
        ),
        sections: vec![Section {
            blocks,
            ..Section::default()
        }],
        ..Document::default()
    }
}

fn ink(pixmap: &Pixmap, x0: u32, y0: u32, x1: u32, y1: u32) -> usize {
    (y0..y1)
        .flat_map(|y| (x0..x1).map(move |x| (x, y)))
        .filter(|&(x, y)| pixmap.pixel(x, y) != Some([255; 4]))
        .count()
}

fn red_png() -> Vec<u8> {
    let mut image = Pixmap::new(4, 4).unwrap();
    for pixel in image.data.as_chunks_mut::<4>().0 {
        *pixel = [255, 0, 0, 255];
    }
    image.encode(Format::Png).unwrap()
}

#[test]
fn text_paints_ink_inside_the_text_area_only() {
    let doc = document(vec![para(vec![run(
        "The quick brown fox jumps over the lazy dog",
        RunProperties::default(),
    )])]);
    let laid = layout(&doc, &fonts());
    let pixmap = render_page(&laid, 0, 1.0).unwrap();
    assert_eq!((pixmap.width, pixmap.height), (612, 792));
    let line = ink(&pixmap, 72, 72, 540, 90);
    assert!(line > 400, "{line} ink pixels on the first line");
    assert_eq!(ink(&pixmap, 0, 0, 612, 70), 0);
    assert_eq!(ink(&pixmap, 0, 100, 612, 792), 0);
    let twice = render_page(&laid, 0, 2.0).unwrap();
    assert_eq!((twice.width, twice.height), (1224, 1584));
    let scaled = ink(&twice, 144, 144, 1080, 180);
    assert!(scaled > line * 2, "{scaled} vs {line}");
}

#[test]
fn color_highlight_and_underline_paint() {
    let props = RunProperties {
        color: Some(Some(Color(0, 0, 255))),
        highlight: Some(Color(255, 255, 0)),
        underline: Some(docboss_model::Underline::Single),
        ..RunProperties::default()
    };
    let laid = layout(&document(vec![para(vec![run("marked", props)])]), &fonts());
    let pixmap = render_page(&laid, 0, 2.0).unwrap();
    let pixels: Vec<[u8; 4]> = (144..200)
        .flat_map(|y| (144..300).map(move |x| (x, y)))
        .filter_map(|(x, y)| pixmap.pixel(x, y))
        .collect();
    assert!(pixels.contains(&[255, 255, 0, 255]), "highlight");
    assert!(
        pixels.iter().any(|p| p[2] > 200 && p[0] < 60 && p[1] < 60),
        "blue text"
    );
}

#[test]
fn table_borders_land_on_the_grid() {
    let border = Border {
        style: BorderStyle::Single,
        size: 8,
        space: 0,
        color: None,
    };
    let all = Some(border);
    let cell = |t: &str| TableCell {
        blocks: vec![para(vec![run(t, RunProperties::default())])],
        ..TableCell::default()
    };
    let table = Table {
        properties: TableProperties {
            borders: Some(Borders {
                top: all,
                left: all,
                bottom: all,
                right: all,
                inside_horizontal: all,
                inside_vertical: all,
            }),
            ..TableProperties::default()
        },
        grid: vec![2880, 2880],
        rows: vec![TableRow {
            cells: vec![cell("a"), cell("b")],
            ..TableRow::default()
        }],
    };
    let laid = layout(&document(vec![Block::Table(table)]), &fonts());
    let pixmap = render_page(&laid, 0, 1.0).unwrap();
    let darkness = |x: u32, y: u32| 255 - u32::from(pixmap.pixel(x, y).unwrap()[0]);
    for x in [72, 216, 360] {
        assert!(
            darkness(x - 1, 80) + darkness(x, 80) >= 250,
            "border at x={x}"
        );
        assert_eq!(
            darkness(x - 3, 80) + darkness(x + 2, 80),
            0,
            "border wider than 1pt at x={x}"
        );
    }
    assert!(darkness(144, 71) + darkness(144, 72) >= 250, "top border");
}

#[test]
fn images_draw_scaled_and_metafiles_fall_back_to_placeholders() {
    let drawing = |media: u32| {
        RunContent::Drawing(Box::new(Drawing {
            media: Some(MediaId(media)),
            width: 12700 * 100,
            height: 12700 * 50,
            placement: DrawingPlacement::Inline,
            name: None,
            description: None,
            text_box: Vec::new(),
            shape: Default::default(),
            geometry: None,
        }))
    };
    let mut doc = document(vec![
        para(vec![Inline::Run(Run {
            properties: RunProperties::default(),
            content: vec![drawing(0)],
        })]),
        para(vec![Inline::Run(Run {
            properties: RunProperties::default(),
            content: vec![drawing(1)],
        })]),
    ]);
    doc.media = vec![
        Media {
            name: "word/media/image1.png".into(),
            content_type: "image/png".into(),
            data: red_png().into(),
        },
        Media {
            name: "word/media/image2.wmf".into(),
            content_type: "image/x-wmf".into(),
            data: b"\xd7\xcd\xc6\x9a\0\0\0\0".to_vec().into(),
        },
    ];
    let laid = layout(&doc, &fonts());
    let pixmap = render_page(&laid, 0, 1.0).unwrap();
    assert_eq!(pixmap.pixel(122, 100), Some([255, 0, 0, 255]));
    assert_eq!(pixmap.diagnostics.len(), 1);
    assert!(pixmap.diagnostics[0].message.contains("WMF"));
}

/// ECMA-376 Part 1 §20.4.2.22: text that overflows a text box without
/// `a:spAutoFit` is cut off at the shape less its insets, while the fill
/// and outline still paint.
#[test]
fn overflowing_text_box_content_is_clipped_to_its_insets() {
    let yellow = Color(255, 255, 0);
    let lines = (0..20)
        .map(|_| para(vec![run("Clipped text", RunProperties::default())]))
        .collect();
    let drawing = Drawing {
        media: None,
        width: 2_540_000,
        height: 1_270_000,
        placement: DrawingPlacement::Anchored {
            horizontal: DrawingPosition::offset(PositionBase::Page, 914_400),
            vertical: DrawingPosition::offset(PositionBase::Page, 914_400),
            behind_text: false,
        },
        name: None,
        description: None,
        text_box: lines,
        shape: ShapeFormat {
            fill: Some(yellow),
            outline: Some(Color::BLACK),
            outline_width: Some(12_700),
            insets: Some([127_000; 4]),
            ..ShapeFormat::default()
        },
        geometry: None,
    };
    let doc = document(vec![para(vec![Inline::Run(Run {
        properties: RunProperties::default(),
        content: vec![RunContent::Drawing(Box::new(drawing))],
    })])]);
    let pixmap = render_page(&layout(&doc, &fonts()), 0, 1.0).unwrap();
    let not_yellow = |x0: u32, y0: u32, x1: u32, y1: u32| {
        (y0..y1)
            .flat_map(|y| (x0..x1).map(move |x| (x, y)))
            .filter(|&(x, y)| pixmap.pixel(x, y) != Some([255, 255, 0, 255]))
            .count()
    };
    assert!(not_yellow(82, 82, 262, 162) > 100);
    assert_eq!(not_yellow(80, 163, 264, 171), 0);
    assert_eq!(ink(&pixmap, 60, 176, 290, 500), 0);
    assert!(pixmap
        .pixel(72, 120)
        .is_some_and(|p| p[0] < 200 && p[2] == 0));
}

fn outlined_box(x_pt: i64, shape: ShapeFormat) -> Inline {
    Inline::Run(Run {
        properties: RunProperties::default(),
        content: vec![RunContent::Drawing(Box::new(Drawing {
            media: None,
            width: 2_540_000,
            height: 1_270_000,
            placement: DrawingPlacement::Anchored {
                horizontal: DrawingPosition::offset(PositionBase::Page, x_pt * 12_700),
                vertical: DrawingPosition::offset(PositionBase::Page, 914_400),
                behind_text: false,
            },
            name: None,
            description: None,
            text_box: vec![para(vec![])],
            shape,
            geometry: None,
        }))],
    })
}

fn dark(pixmap: &Pixmap, x: u32, y: u32) -> bool {
    pixmap.pixel(x, y).is_some_and(|p| p[0] < 128)
}

/// ECMA-376 Part 1 §20.1.8.48 and §20.1.10.49: a dashed outline runs its
/// pattern on around the corners; §20.1.8.43: a miter join squares a
/// corner a dash runs through; §20.1.10.31: flat caps end a dash at its
/// length, round caps reach half the width past it.
#[test]
fn dashed_outlines_follow_their_pattern_caps_and_joins() {
    let dashed = |cap: LineCap, bits: &str| ShapeFormat {
        outline: Some(Color::BLACK),
        outline_width: Some(25_400),
        outline_dash: DashPattern::from_bits(bits),
        outline_cap: cap,
        outline_join: LineJoin::Miter,
        insets: Some([0; 4]),
        ..ShapeFormat::default()
    };
    let doc = document(vec![para(vec![
        outlined_box(72, dashed(LineCap::Flat, "1111000")),
        outlined_box(300, dashed(LineCap::Flat, "1000")),
    ])]);
    let pixmap = render_page(&layout(&doc, &fonts()), 0, 1.0).unwrap();
    assert!(dark(&pixmap, 75, 72));
    assert!(!dark(&pixmap, 83, 72));
    assert!(dark(&pixmap, 89, 72));
    assert!(dark(&pixmap, 272, 74));
    assert!(!dark(&pixmap, 272, 79));
    assert!(dark(&pixmap, 272, 71));
    assert!(dark(&pixmap, 300, 72));
    assert!(!dark(&pixmap, 302, 72));

    let doc = document(vec![para(vec![outlined_box(
        300,
        dashed(LineCap::Round, "1000"),
    )])]);
    let pixmap = render_page(&layout(&doc, &fonts()), 0, 1.0).unwrap();
    assert!(dark(&pixmap, 302, 72));
    assert!(!dark(&pixmap, 304, 72));
}

/// ECMA-376 Part 1 §17.18.2: a dashed paragraph border is drawn as dashes
/// with gaps between them, not as a solid line.
#[test]
fn dashed_borders_paint_dashes() {
    let border = Border {
        style: BorderStyle::Dashed,
        size: 8,
        space: 1,
        color: None,
    };
    let doc = document(vec![Block::Paragraph(Paragraph {
        properties: ParagraphProperties {
            borders: Some(Borders {
                bottom: Some(border),
                ..Borders::default()
            }),
            ..ParagraphProperties::default()
        },
        inlines: vec![run("Bordered", RunProperties::default())],
        ..Paragraph::default()
    })]);
    let pixmap = render_page(&layout(&doc, &fonts()), 0, 4.0).unwrap();
    let row = (300..600)
        .max_by_key(|&y| (300..2100).filter(|&x| dark(&pixmap, x, y)).count())
        .unwrap();
    let marks: Vec<bool> = (300..2100).map(|x| dark(&pixmap, x, row)).collect();
    let gaps = marks.windows(2).filter(|w| w[0] && !w[1]).count();
    let ink = marks.iter().filter(|&&m| m).count();
    assert!((38..=46).contains(&gaps), "{gaps}");
    assert!(ink > marks.len() * 7 / 10, "{ink}");
}

#[test]
fn rendering_is_deterministic_and_parallel_matches_sequential() {
    let blocks = (0..150)
        .map(|i| {
            para(vec![run(
                &format!("paragraph number {i} with text"),
                RunProperties::default(),
            )])
        })
        .collect();
    let laid = layout(&document(blocks), &fonts());
    assert!(laid.pages.len() > 2);
    let sequential: Vec<Pixmap> = (0..laid.pages.len())
        .map(|i| render_page(&laid, i, 1.0).unwrap())
        .collect();
    let parallel: Vec<Pixmap> = render_pages(&laid, 1.0)
        .into_iter()
        .map(Result::unwrap)
        .collect();
    assert_eq!(sequential, parallel);
}

#[test]
fn every_format_encodes() {
    let laid = layout(
        &document(vec![para(vec![run("formats", RunProperties::default())])]),
        &fonts(),
    );
    let pixmap = render_page(&laid, 0, 0.5).unwrap();
    assert!(pixmap.encode(Format::Png).unwrap().starts_with(b"\x89PNG"));
    assert!(pixmap
        .encode(Format::Ppm)
        .unwrap()
        .starts_with(b"P6\n306 396\n255\n"));
    assert!(pixmap.encode(Format::Bmp).unwrap().starts_with(b"BM"));
    let jpeg = pixmap.encode(Format::Jpeg { quality: 80 }).unwrap();
    assert!(jpeg.starts_with(&[0xFF, 0xD8]));
    assert!(render_page(&laid, 3, 1.0).is_err());
}

fn drawn_shape(x_pt: i64, name: &str, shape: ShapeFormat) -> Inline {
    Inline::Run(Run {
        properties: RunProperties::default(),
        content: vec![RunContent::Drawing(Box::new(Drawing {
            media: None,
            width: 1_270_000,
            height: 1_270_000,
            placement: DrawingPlacement::Anchored {
                horizontal: DrawingPosition::offset(PositionBase::Page, x_pt * 12_700),
                vertical: DrawingPosition::offset(PositionBase::Page, 914_400),
                behind_text: false,
            },
            name: None,
            description: None,
            text_box: Vec::new(),
            shape,
            geometry: Some(Box::new(docboss_model::Geometry::Preset {
                name: name.into(),
                adjust: Vec::new(),
            })),
        }))],
    })
}

/// Preset geometries paint their own outline: an ellipse fills its
/// middle but not its corners, a triangle's apex is outlined, and a line
/// ends in a filled arrowhead.
/// ECMA-376 Part 1 §20.1.9.18, §20.1.9.4, §20.1.8.57.
#[test]
fn preset_shapes_fill_and_stroke_their_outline() {
    let filled = ShapeFormat {
        fill: Some(Color(0, 0, 0)),
        ..ShapeFormat::default()
    };
    let outlined = ShapeFormat {
        outline: Some(Color::BLACK),
        outline_width: Some(25_400),
        ..ShapeFormat::default()
    };
    let arrow = ShapeFormat {
        outline: Some(Color::BLACK),
        outline_width: Some(12_700),
        tail_end: Some(docboss_model::LineEnd {
            kind: docboss_model::LineEndKind::Triangle,
            width: docboss_model::LineEndSize::Large,
            length: docboss_model::LineEndSize::Large,
        }),
        ..ShapeFormat::default()
    };
    let doc = document(vec![para(vec![
        drawn_shape(72, "ellipse", filled),
        drawn_shape(200, "triangle", outlined),
        drawn_shape(350, "line", arrow),
    ])]);
    let pixmap = render_page(&layout(&doc, &fonts()), 0, 1.0).unwrap();
    assert!(dark(&pixmap, 122, 122));
    assert!(!dark(&pixmap, 75, 75));
    assert!(!dark(&pixmap, 168, 168));
    assert!(dark(&pixmap, 250, 73));
    assert!(!dark(&pixmap, 250, 120));
    assert!(!dark(&pixmap, 210, 80));
    assert!(dark(&pixmap, 400, 122));
    assert!(dark(&pixmap, 446, 164));
    assert!(dark(&pixmap, 444, 168));
    assert!(!dark(&pixmap, 441, 150));
}

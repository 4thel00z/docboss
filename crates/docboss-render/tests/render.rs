use std::path::Path;
use std::sync::Arc;

use docboss_font::FontDatabase;
use docboss_layout::layout;
use docboss_model::{
    Block, Border, BorderStyle, Borders, Color, Document, Drawing, DrawingPlacement, Inline, Media,
    MediaId, Paragraph, ParagraphProperties, Run, RunContent, RunProperties, Section, Style,
    StyleKind, Styles, Table, TableCell, TableProperties, TableRow,
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
        RunContent::Drawing(Drawing {
            media: Some(MediaId(media)),
            width: 12700 * 100,
            height: 12700 * 50,
            placement: DrawingPlacement::Inline,
            name: None,
            description: None,
            text_box: Vec::new(),
        })
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

use std::path::Path;
use std::sync::Arc;

use docboss_font::{bounds, Feature, Font, FontDatabase, Seg, ShapeInput};
use docboss_model::FontEntry;

const COUSINE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/Cousine-Regular.ttf"
);

fn cousine() -> Font {
    Font::parse(std::fs::read(COUSINE).unwrap(), 0).unwrap()
}

#[test]
fn reads_names_style_and_metrics() {
    let font = cousine();
    assert_eq!(font.names().family, "Cousine");
    assert!(font.style().monospace);
    assert!(!font.style().is_bold());
    let m = font.metrics();
    assert_eq!(m.units_per_em, 2048);
    assert!(m.ascent > 1500.0 && m.descent > 300.0, "{m:?}");
}

#[test]
fn maps_characters_and_advances() {
    let font = cousine();
    let a = font.glyph_index('A').unwrap();
    let w = font.glyph_index('W').unwrap();
    assert_ne!(a, w);
    assert_eq!(font.advance(a), font.advance(w));
    assert_eq!(font.advance(a), 1229);
    assert_eq!(font.glyph_index('\u{10FFFD}'), None);
}

#[test]
fn outlines_close_and_fit_the_em() {
    let font = cousine();
    let outline = font.outline(font.glyph_index('O').unwrap());
    assert!(matches!(outline.first(), Some(Seg::Move(..))));
    assert_eq!(
        outline.iter().filter(|s| matches!(s, Seg::Close)).count(),
        2
    );
    let [x0, y0, x1, y1] = bounds(&outline).unwrap();
    assert!(x0 >= 0.0 && x1 <= 1229.0 && y0 >= -50.0 && y1 < 1600.0);
    let accented = font.outline(font.glyph_index('Å').unwrap());
    assert!(
        accented.len() > outline.len(),
        "composite glyph adds the ring"
    );
    assert!(font.outline(font.glyph_index(' ').unwrap()).is_empty());
}

#[test]
fn malformed_input_is_an_error_not_a_panic() {
    assert!(Font::parse(vec![0u8; 3], 0).is_err());
    let mut data = std::fs::read(COUSINE).unwrap();
    data.truncate(4000);
    let _ = Font::parse(data, 0);
    let mut noise: Vec<u8> = std::fs::read(COUSINE).unwrap();
    for (i, byte) in noise.iter_mut().enumerate().skip(12) {
        if i % 97 == 0 {
            *byte ^= 0x5A;
        }
    }
    if let Ok(font) = Font::parse(noise, 0) {
        for c in ['a', 'Z', 'é', '€'] {
            if let Some(g) = font.glyph_index(c) {
                let _ = font.outline(g);
                let _ = font.advance(g);
            }
        }
    }
}

#[test]
fn database_substitutes_metric_compatible_faces() {
    let mut db = FontDatabase::new();
    assert_eq!(db.add_file(Path::new(COUSINE)), 1);
    let id = db.select("Courier New", None, false, false).unwrap();
    assert_eq!(db.font(id).unwrap().names().family, "Cousine");
    assert_eq!(
        db.select("Unknown Face", Some("modern"), true, false),
        Some(id)
    );
    assert_eq!(db.fallback('A', false, false), Some(id));
    assert_eq!(db.fallback('\u{10FFFD}', false, false), None);
}

#[test]
fn embedded_fonts_answer_to_their_font_table_name() {
    let mut db = FontDatabase::new();
    db.add_file(Path::new(COUSINE));
    let data: Arc<[u8]> = std::fs::read(COUSINE).unwrap().into();
    db.add_document_fonts(&[FontEntry {
        name: "Corporate Sans".into(),
        embedded_bold: Some(data),
        ..FontEntry::default()
    }]);
    let id = db.select("Corporate Sans", None, true, false).unwrap();
    let info = db.info(id).unwrap();
    assert!(info.embedded && info.style.is_bold());
}

#[test]
fn system_scan_finds_faces_and_is_cached() {
    let start = std::time::Instant::now();
    let db = FontDatabase::system();
    let first = start.elapsed();
    let again = std::time::Instant::now();
    let second = FontDatabase::system();
    assert_eq!(db.len(), second.len());
    assert!(again.elapsed() <= first + std::time::Duration::from_millis(50));
    eprintln!("{} system faces scanned in {first:?}", db.len());
}

#[test]
fn kerning_tightens_known_pairs_when_a_kerned_face_exists() {
    let db = FontDatabase::system();
    let Some(id) = db.select("Times New Roman", Some("roman"), false, false) else {
        return;
    };
    let font = db.font(id).unwrap();
    if !font.has_kerning() {
        return;
    }
    let (a, v) = (
        font.glyph_index('A').unwrap(),
        font.glyph_index('V').unwrap(),
    );
    assert!(
        font.kerning(a, v) < 0,
        "{} A-V kerning",
        font.names().family
    );
}

const ARABIC: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/NotoSansArabic-Subset.ttf"
);
const HEBREW: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/NotoSansHebrew-Subset.ttf"
);

const GLOBAL: u32 = 1;
const ISOL: u32 = 2;
const FINA: u32 = 4;
const MEDI: u32 = 8;
const INIT: u32 = 16;

fn stages() -> Vec<Vec<Feature>> {
    vec![
        vec![Feature::new(b"ccmp", GLOBAL), Feature::new(b"locl", GLOBAL)],
        vec![Feature::new(b"isol", ISOL)],
        vec![Feature::new(b"fina", FINA)],
        vec![Feature::new(b"medi", MEDI)],
        vec![Feature::new(b"init", INIT)],
        vec![Feature::new(b"rlig", GLOBAL)],
        vec![Feature::new(b"calt", GLOBAL)],
        vec![Feature::new(b"liga", GLOBAL), Feature::new(b"clig", GLOBAL)],
    ]
}

const POSITION: [Feature; 3] = [
    Feature::new(b"kern", GLOBAL),
    Feature::new(b"mark", GLOBAL),
    Feature::new(b"mkmk", GLOBAL),
];

/// Shapes right-to-left text, one joining mask per character, and returns
/// each glyph with its position on the line, sorted.
fn shape_rtl(font: &Font, script: &[u8; 4], text: &str, masks: &[u32]) -> Vec<(u16, i32, i32)> {
    let input: Vec<ShapeInput> = text
        .chars()
        .zip(masks)
        .enumerate()
        .map(|(i, (c, &mask))| ShapeInput {
            id: font.glyph_index(c).unwrap_or(0),
            cluster: i as u32,
            mask: mask | GLOBAL,
            mark: false,
        })
        .collect();
    let stages = stages();
    let stages: Vec<&[Feature]> = stages.iter().map(|s| s.as_slice()).collect();
    let shaped = font.shape(&input, *script, &stages, &POSITION);
    let mut at = vec![(0i32, 0i32); shaped.len()];
    let mut pen = 0;
    for i in (0..shaped.len()).rev() {
        let g = shaped[i];
        if g.attach.is_some() {
            continue;
        }
        at[i] = (pen + g.dx, g.dy);
        pen += g.advance;
    }
    for i in 0..shaped.len() {
        let g = shaped[i];
        if let Some(base) = g.attach {
            let (x, y) = at[base as usize];
            at[i] = (x + g.dx, y + g.dy);
        }
    }
    let mut out: Vec<(u16, i32, i32)> = shaped
        .iter()
        .zip(at)
        .map(|(g, (x, y))| (g.id, x, y))
        .collect();
    out.sort_unstable();
    out
}

/// The same, from HarfBuzz's visual-order output of glyph, advance and
/// offsets.
fn placed(visual: &[(u16, i32, i32, i32)]) -> Vec<(u16, i32, i32)> {
    let mut pen = 0;
    let mut out: Vec<(u16, i32, i32)> = visual
        .iter()
        .map(|&(id, advance, dx, dy)| {
            let at = (id, pen + dx, dy);
            pen += advance;
            at
        })
        .collect();
    out.sort_unstable();
    out
}

/// GSUB extension lookups (type 7) wrapping multiple substitution in ccmp
/// and single substitution in init, medi and fina; GPOS mark to base
/// (type 4). The expected glyphs and positions are HarfBuzz's.
#[test]
fn arabic_joining_forms_and_marks_match_harfbuzz() {
    let font = Font::parse(std::fs::read(ARABIC).unwrap(), 0).unwrap();
    assert!(font.has_layout());
    assert_eq!(
        shape_rtl(&font, b"arab", "بسم", &[INIT, MEDI, FINA]),
        placed(&[
            (67, 562, 0, 0),
            (28, 850, 0, 0),
            (220, 0, 31, -3),
            (14, 269, 0, 0)
        ])
    );
    assert_eq!(
        shape_rtl(&font, b"arab", "ب", &[ISOL]),
        placed(&[(220, 0, 410, -23), (10, 993, 0, 0)])
    );
}

/// GPOS mark to base and mark to mark (types 4 and 6) with harakat over
/// and under joined letters.
#[test]
fn arabic_harakat_attach_like_harfbuzz() {
    let font = Font::parse(std::fs::read(ARABIC).unwrap(), 0).unwrap();
    let got = shape_rtl(&font, b"arab", "بِسْمِ", &[INIT, 0, MEDI, 0, FINA, 0]);
    let want = placed(&[
        (269, 0, 139, 0),
        (67, 562, 0, 0),
        (249, 0, 301, -126),
        (28, 850, 0, 0),
        (269, 0, -35, -191),
        (220, 0, 31, -3),
        (14, 269, 0, 0),
    ]);
    assert_eq!(got, want);
}

/// GSUB ligature substitution (type 4) through rlig: lam and alef become
/// one glyph.
#[test]
fn lam_alef_forms_a_ligature() {
    let font = Font::parse(std::fs::read(ARABIC).unwrap(), 0).unwrap();
    assert_eq!(
        shape_rtl(&font, b"arab", "لا", &[INIT, FINA]),
        placed(&[(6, 363, 0, 0), (63, 219, 0, 0)])
    );
    assert_eq!(
        shape_rtl(&font, b"arab", "في", &[INIT, FINA]),
        placed(&[
            (221, 0, 185, -232),
            (91, 736, 0, 0),
            (199, 0, 147, 38),
            (45, 447, 0, 0)
        ])
    );
}

/// Hebrew points: GPOS mark to base and pair adjustments (types 4 and 2).
#[test]
fn hebrew_points_attach_like_harfbuzz() {
    let font = Font::parse(std::fs::read(HEBREW).unwrap(), 0).unwrap();
    let text = "שָׁלוֹם";
    let masks = vec![0; text.chars().count()];
    assert_eq!(
        shape_rtl(&font, b"hebr", text, &masks),
        placed(&[
            (21, 684, 0, 0),
            (41, 0, 82, 0),
            (111, 291, -10, 0),
            (49, 522, 0, 0),
            (72, 0, 227, 0),
            (92, 0, 539, 0),
            (88, 730, 0, 0),
        ])
    );
}

#[test]
fn shaping_damaged_layout_tables_does_not_panic() {
    let clean = std::fs::read(ARABIC).unwrap();
    for step in [53usize, 97, 211] {
        let mut noise = clean.clone();
        for (i, byte) in noise.iter_mut().enumerate().skip(400) {
            if i % step == 0 {
                *byte ^= 0xA5;
            }
        }
        let Ok(font) = Font::parse(noise, 0) else {
            continue;
        };
        let _ = shape_rtl(
            &font,
            b"arab",
            "بِسْمِ لا",
            &[INIT, 0, MEDI, 0, FINA, 0, 0, INIT, FINA],
        );
    }
}

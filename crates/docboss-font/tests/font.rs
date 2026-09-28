use std::path::Path;
use std::sync::Arc;

use docboss_font::{bounds, Font, FontDatabase, Seg};
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

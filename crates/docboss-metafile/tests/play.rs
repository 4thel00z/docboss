use docboss_metafile::{bitmap, play, text, Error, Kind, Op, Picture};
use docboss_model::Color;

const EMF: &[u8] = include_bytes!("fixtures/shapes.emf");
const WMF: &[u8] = include_bytes!("fixtures/shapes.wmf");

fn fills(picture: &Picture) -> Vec<Color> {
    picture
        .ops
        .iter()
        .filter_map(|op| match op {
            Op::Path { fill: Some(f), .. } => Some(f.color),
            _ => None,
        })
        .collect()
}

/// The bounds of every path filled with `color`, on the frame.
fn bounds_of(picture: &Picture, color: Color) -> (f32, f32, f32, f32) {
    let mut b = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for op in &picture.ops {
        let Op::Path {
            segs,
            fill: Some(f),
            ..
        } = op
        else {
            continue;
        };
        if f.color != color {
            continue;
        }
        for seg in segs {
            let (x, y) = match *seg {
                docboss_metafile::Seg::Move(x, y) | docboss_metafile::Seg::Line(x, y) => (x, y),
                docboss_metafile::Seg::Cubic(_, _, _, _, x, y) => (x, y),
                docboss_metafile::Seg::Close => continue,
            };
            b = (b.0.min(x), b.1.min(y), b.2.max(x), b.3.max(y));
        }
    }
    b
}

/// The WMF records alone: the file without its META_ESCAPE records.
fn without_escapes(wmf: &[u8]) -> Vec<u8> {
    let mut out = wmf[..40].to_vec();
    let mut at = 40;
    while at + 6 <= wmf.len() {
        let words = u32::from_le_bytes(wmf[at..at + 4].try_into().unwrap()) as usize;
        let function = u16::from_le_bytes([wmf[at + 4], wmf[at + 5]]);
        let end = (at + words * 2).min(wmf.len());
        if function != 0x0626 {
            out.extend_from_slice(&wmf[at..end]);
        }
        if words < 3 {
            break;
        }
        at = end;
    }
    out
}

/// [MS-EMF] §2.3.4.2.1 EmfMetafileHeader: the frame, in hundredths of a
/// millimetre, sets the picture's size; [MS-EMF] §2.3.5.29 and §2.3.7.1:
/// the filled polygons keep their brush colors and sit where the SVG put
/// them relative to each other.
#[test]
fn emf_shapes_fill_with_their_brushes_on_the_frame() {
    let picture = play(EMF).unwrap();
    assert_eq!(picture.kind, Kind::Emf);
    assert!((picture.width - 105.3).abs() < 0.5, "{}", picture.width);
    assert!((picture.height - 77.0).abs() < 0.5, "{}", picture.height);
    let colors = fills(&picture);
    assert!(colors.contains(&Color(255, 0, 0)));
    assert!(colors.contains(&Color(0, 0, 255)));
    let red = bounds_of(&picture, Color(255, 0, 0));
    let blue = bounds_of(&picture, Color(0, 0, 255));
    assert!(
        red.0 < 3.0 && red.1 < 3.0 && red.2 < blue.0,
        "{red:?} {blue:?}"
    );
    assert!((red.2 - red.0) / (red.3 - red.1) > 1.5, "{red:?}");
    assert!(picture.notes.is_empty(), "{:?}", picture.notes);
}

/// [MS-EMF] §2.3.5.8 EMR_EXTTEXTOUTW with §2.3.7.8
/// EMR_EXTCREATEFONTINDIRECTW: the text, its face, its advances and a size
/// that is a fraction of the frame's height.
#[test]
fn emf_text_keeps_its_face_size_and_advances() {
    let picture = play(EMF).unwrap();
    let texts: Vec<_> = picture
        .ops
        .iter()
        .filter_map(|op| match op {
            Op::Text(t) => Some(t),
            _ => None,
        })
        .collect();
    assert_eq!(texts.len(), 1);
    let t = texts[0];
    assert_eq!(t.text, "Hi");
    assert_eq!(t.font.face, "Liberation Sans");
    assert!(t.font.size > 5.0 && t.font.size < 15.0, "{}", t.font.size);
    assert_eq!(t.advances.as_ref().map(Vec::len), Some(2));
    assert!(t.at.0 > 50.0 && t.at.1 > 50.0, "{:?}", t.at);
    assert_eq!(text(&picture), "Hi");
}

/// [MS-EMF] §2.3.1.7 EMR_STRETCHDIBITS: the bitmap's pixels and where it
/// is drawn.
#[test]
fn emf_bitmap_is_decoded_and_placed() {
    let picture = play(EMF).unwrap();
    let bitmaps: Vec<_> = picture
        .ops
        .iter()
        .filter_map(|op| match op {
            Op::Bitmap(b) => Some(b),
            _ => None,
        })
        .collect();
    assert_eq!(bitmaps.len(), 1);
    let b = bitmaps[0];
    let image = bitmap::decode_bmp(&b.bmp).unwrap();
    let center = ((image.height / 2 * image.width + image.width / 2) * 4) as usize;
    assert_eq!(&image.pixels[center..center + 4], &[0, 160, 0, 255]);
    assert!(b.dest.x < 10.0 && b.dest.y > 50.0, "{:?}", b.dest);
    assert!((b.dest.width - b.dest.height).abs() < 1.0, "{:?}", b.dest);
}

/// [MS-WMF] §2.3.6.25 META_ESCAPE_ENHANCED_METAFILE: the EMF a WMF carries
/// is played in its place.
#[test]
fn wmf_plays_the_emf_its_escapes_carry() {
    let picture = play(WMF).unwrap();
    assert_eq!(picture.kind, Kind::EmbeddedEmf);
    assert_eq!(fills(&picture), fills(&play(EMF).unwrap()));
}

/// [MS-WMF] §2.3.2.3 META_PLACEABLE, §2.3.3.15 META_POLYGON, §2.3.4.1
/// META_CREATEBRUSHINDIRECT and §2.3.3.20 META_TEXTOUT: without the escapes
/// the WMF records draw the same shapes on the same frame.
#[test]
fn wmf_records_draw_the_same_shapes() {
    let wmf = without_escapes(WMF);
    let picture = play(&wmf).unwrap();
    assert_eq!(picture.kind, Kind::Wmf);
    let emf = play(EMF).unwrap();
    assert!(
        (picture.width - emf.width).abs() < 2.0,
        "{} {}",
        picture.width,
        emf.width
    );
    let colors = fills(&picture);
    assert!(colors.contains(&Color(255, 0, 0)));
    assert!(colors.contains(&Color(0, 0, 255)));
    let (a, b) = (
        bounds_of(&picture, Color(255, 0, 0)),
        bounds_of(&emf, Color(255, 0, 0)),
    );
    assert!(
        (a.0 - b.0).abs() < 1.0 && (a.2 - b.2).abs() < 1.0,
        "{a:?} {b:?}"
    );
    assert!(text(&picture).contains("Hi"), "{:?}", text(&picture));
}

/// Every truncation of the fixtures plays or fails without panicking, and
/// a record claiming more bytes than the file holds ends playback.
#[test]
fn truncated_and_oversized_records_do_not_panic() {
    for data in [EMF, WMF, &without_escapes(WMF)[..]] {
        for len in (0..data.len()).step_by(7) {
            let _ = play(&data[..len]);
        }
    }
    let mut bad = EMF.to_vec();
    let first = u32::from_le_bytes(bad[4..8].try_into().unwrap()) as usize;
    bad[first + 4..first + 8].copy_from_slice(&u32::MAX.to_le_bytes());
    let picture = play(&bad).unwrap();
    assert!(picture.ops.is_empty());
    assert_eq!(play(b"not a metafile"), Err(Error::NotAMetafile));
}

/// Corrupted bytes, flipped at pseudo-random places, never panic.
#[test]
fn corrupted_metafiles_do_not_panic() {
    let mut state = 0x2545_F491_4F6C_DD1Du64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    for data in [EMF, WMF, &without_escapes(WMF)[..]] {
        for _ in 0..400 {
            let mut bytes = data.to_vec();
            for _ in 0..8 {
                let at = (next() % bytes.len() as u64) as usize;
                bytes[at] = next() as u8;
            }
            let _ = play(&bytes);
        }
    }
}

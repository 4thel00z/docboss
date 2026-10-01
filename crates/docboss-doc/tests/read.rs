use docboss_model::{
    plain_text, Block, Document, DrawingPlacement, DrawingPosition, Inline, Justification,
    NoteKind, Paragraph, PositionBase, RunContent, SourceFormat, Table, VerticalMerge,
};

fn fixture(name: &str) -> Document {
    let path = format!("{}/tests/fixtures/{name}.doc", env!("CARGO_MANIFEST_DIR"));
    docboss_doc::read(&std::fs::read(path).unwrap()).unwrap()
}

fn libreoffice_text(name: &str) -> String {
    let path = format!("{}/tests/fixtures/{name}.txt", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(path)
        .unwrap()
        .trim_start_matches('\u{FEFF}')
        .replace("\r\n", "\n")
}

fn paragraphs(document: &Document) -> Vec<&Paragraph> {
    document
        .blocks()
        .filter_map(|block| match block {
            Block::Paragraph(p) => Some(p),
            Block::Table(_) => None,
        })
        .collect()
}

fn first_table(document: &Document) -> &Table {
    document
        .blocks()
        .find_map(|block| match block {
            Block::Table(t) => Some(t),
            Block::Paragraph(_) => None,
        })
        .unwrap()
}

fn runs(paragraph: &Paragraph) -> Vec<&docboss_model::Run> {
    paragraph
        .inlines
        .iter()
        .filter_map(|inline| match inline {
            Inline::Run(run) => Some(run),
            _ => None,
        })
        .collect()
}

/// [MS-DOC] §2.4.1 retrieving text and §2.4.2 paragraph boundaries: the
/// text of paragraphs outside tables matches LibreOffice's own export.
/// [MS-DOC] §2.1.1, §2.5.14, §2.2.2, §2.9.214, §2.4.1, §2.4.2, §2.9.38, §2.8.35, §2.9.177, §2.9.178, §2.9.73, §2.5.1, §2.5.2, §2.5.4, §2.5.5, §2.5.6, §2.5.15, §2.3.1, §2.2.1.
#[test]
fn text_matches_libreoffice() {
    for name in ["text", "image", "sections"] {
        assert_eq!(plain_text(&fixture(name)), libreoffice_text(name), "{name}");
    }
}

/// [MS-OLEPS] §2.21 and [MS-DOC] §2.5.1: metadata and format.
/// [MS-OLEPS] §2.20, §2.15; [MS-DOC] §2.1.6.
#[test]
fn metadata_and_format() {
    let document = fixture("text");
    assert_eq!(document.format, SourceFormat::Doc);
    assert_eq!(document.metadata.title.as_deref(), Some("Fixture text"));
    assert_eq!(document.metadata.creator.as_deref(), Some("Ada Lovelace"));
    assert!(
        document.diagnostics.is_empty(),
        "{:?}",
        document.diagnostics
    );
}

/// [MS-DOC] §2.4.6.2 direct character formatting through CHPX FKPs and
/// §2.6.1 character sprms; §2.4.6.5 style formatting through the STSH.
/// [MS-DOC] §2.4.6, §2.8.5, §2.8.6, §2.9.33, §2.9.174, §2.9.206, §2.9.207, §2.6.1, §2.6.2, §2.2.5, §2.9.271, §2.9.272, §2.9.258, §2.9.260, §2.9.259, §2.9.336, §2.9.338.
#[test]
fn character_and_paragraph_formatting() {
    let document = fixture("text");
    let all = paragraphs(&document);
    let body = runs(all[1]);
    let find = |needle: &str| {
        body.iter()
            .find(|run| {
                run.content
                    .iter()
                    .any(|c| matches!(c, RunContent::Text(t) if t.contains(needle)))
            })
            .unwrap()
    };
    let italic = &find("italic underlined").properties;
    assert_eq!(italic.italic, Some(true));
    assert!(italic.underline.is_some());
    let blue = &find("blue big").properties;
    assert_eq!(blue.color, Some(Some(docboss_model::Color(0, 0, 255))));
    assert_eq!(blue.size, Some(28));
    let bold = find("bold words");
    let resolved = document
        .styles
        .resolve_run(all[1].style_id.as_deref(), &bold.properties);
    assert!(resolved.is_bold());

    assert_eq!(
        document.styles.heading_level(all[0], &document.numbering),
        Some(0)
    );
    assert_eq!(
        document.styles.heading_level(all[3], &document.numbering),
        Some(1)
    );
    let centered = all.iter().find(|p| p.text().contains("Centered")).unwrap();
    assert_eq!(
        centered.properties.justification,
        Some(Justification::Center)
    );
    assert_eq!(centered.properties.page_break_before, Some(true));
    let quote = all.iter().find(|p| p.text().contains("quoted")).unwrap();
    let resolved = document
        .styles
        .resolve_paragraph(quote, &document.numbering);
    assert_eq!(resolved.indentation.left, Some(720));
    assert!(quote
        .text()
        .contains("paragraph.\nAfter a line break.\tAfter a tab."));
}

/// [MS-DOC] §2.4.6.3 list formatting: PlfLst levels and PlfLfo instances
/// give the labels LibreOffice shows.
/// [MS-DOC] §2.9.201, §2.9.200, §2.9.147, §2.9.149, §2.9.150, §2.9.131; [MS-OSHARED] §2.2.1.3.
#[test]
fn list_labels() {
    let document = fixture("lists");
    let mut counter = document.numbering.counter();
    let labels: Vec<String> = paragraphs(&document)
        .iter()
        .filter_map(|p| p.properties.numbering.and_then(|r| counter.next(r)))
        .collect();
    assert_eq!(labels, ["1.", "a)", "b)", "2.", "\u{2022}", "\u{2022}"]);
}

/// [MS-DOC] §2.4.3 to §2.4.5 tables, §2.9.321 TDefTableOperand and
/// §2.9.317 TCGRF merges: horizontal spans, vertical merges and a nested
/// table.
/// [MS-DOC] §2.4.3, §2.4.4, §2.4.5, §2.6.3, §2.9.321, §2.9.313, §2.9.317.
#[test]
fn tables_with_merges_and_nesting() {
    let document = fixture("tables");
    let table = first_table(&document);
    assert_eq!(table.grid.len(), 3);
    assert!(
        table.properties.indent.is_some(),
        "the leftmost cell edge places the table"
    );
    assert!(
        table.properties.cell_margins.is_some(),
        "sprmTDxaGapHalf gives the cell margins"
    );
    assert_eq!(table.rows.len(), 3);
    let first = &table.rows[0].cells;
    assert_eq!(first.len(), 2);
    assert_eq!(first[0].span(), 2);
    assert_eq!(
        first[1].properties.vertical_merge,
        Some(VerticalMerge::Restart)
    );
    assert_eq!(
        table.rows[1].cells[2].properties.vertical_merge,
        Some(VerticalMerge::Continue)
    );
    let nested = table.rows[2].cells[1].blocks.iter().find_map(|b| match b {
        Block::Table(t) => Some(t),
        Block::Paragraph(_) => None,
    });
    let nested = nested.unwrap();
    assert_eq!(nested.rows[0].cells.len(), 2);
    let texts: Vec<String> = paragraphs(&document).iter().map(|p| p.text()).collect();
    assert_eq!(texts, ["Before table", "After table"]);
}

/// [MS-DOC] §2.3.2, §2.3.4, §2.3.5: footnote, endnote and comment
/// references in the main text and their bodies; §2.9.90 HYPERLINK fields;
/// §2.8.10 bookmarks.
/// [MS-DOC] §2.3.2, §2.3.4, §2.3.5, §2.8.19, §2.8.20, §2.8.16, §2.8.17, §2.8.7, §2.8.8, §2.9.7, §2.9.353, §2.9.88, §2.9.89, §2.9.90, §2.8.10, §2.8.12, §2.9.279, §2.9.9, §2.9.11, §2.9.277.
#[test]
fn notes_comments_fields_bookmarks() {
    let document = fixture("notes");
    let all = paragraphs(&document);
    let contents: Vec<&RunContent> = runs(all[0]).into_iter().flat_map(|r| &r.content).collect();
    assert!(contents.contains(&&RunContent::FootnoteReference(1)));
    assert!(contents.contains(&&RunContent::EndnoteReference(1)));
    assert_eq!(document.footnotes.len(), 1);
    assert_eq!(document.footnotes[0].kind, NoteKind::Footnote);
    let Block::Paragraph(note) = &document.footnotes[0].blocks[0] else {
        panic!()
    };
    assert!(note.text().contains("The footnote text."));
    assert_eq!(document.endnotes.len(), 1);

    assert_eq!(document.comments.len(), 1);
    let comment = &document.comments[0];
    assert_eq!(comment.author.as_deref(), Some("Grace Hopper"));
    let Block::Paragraph(body) = &comment.blocks[0] else {
        panic!()
    };
    assert_eq!(body.text(), "A comment body.");

    let link = all[2].inlines.iter().find_map(|i| match i {
        Inline::Hyperlink(h) => Some(h),
        _ => None,
    });
    let link = link.unwrap();
    assert_eq!(link.target.as_deref(), Some("https://example.com/docboss"));
    assert!(all[2]
        .inlines
        .iter()
        .any(|i| matches!(i, Inline::Field(f) if f.instruction == "PAGE")));
    let starts = all[3]
        .inlines
        .iter()
        .position(|i| matches!(i, Inline::BookmarkStart { name, .. } if name == "mark1"));
    let ends = all[3]
        .inlines
        .iter()
        .position(|i| matches!(i, Inline::BookmarkEnd { .. }));
    assert!(starts.unwrap() < ends.unwrap());
}

/// [MS-DOC] §2.9.192 PICFAndOfficeArtData and [MS-ODRAW] §2.2.24 PNG BLIP:
/// the inline picture's bytes and displayed size.
/// [MS-DOC] §2.9.192, §2.9.190, §2.1.3.
#[test]
fn inline_picture() {
    let document = fixture("image");
    let drawing = paragraphs(&document)
        .iter()
        .flat_map(|p| runs(p))
        .flat_map(|r| &r.content)
        .find_map(|c| match c {
            RunContent::Drawing(d) => Some(d.as_ref().clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(drawing.placement, DrawingPlacement::Inline);
    assert_eq!((drawing.width, drawing.height), (914_400, 457_200));
    let media = document.media(drawing.media.unwrap()).unwrap();
    assert_eq!(media.content_type, "image/png");
    assert!(media.data.starts_with(b"\x89PNG"));
}

/// [MS-DOC] §2.8.26 PlcfSed and §2.6.4 section sprms; §2.3.3 headers.
/// [MS-DOC] §2.8.26, §2.9.243, §2.9.245, §2.6.4, §2.8.22, §2.3.3.
#[test]
fn sections_and_headers() {
    let document = fixture("sections");
    assert_eq!(document.sections.len(), 3);
    assert_eq!(document.sections[1].properties.columns.count, 2);
    assert_eq!(document.sections[0].properties.page_size.width, 11906);
    let header = document.sections[0]
        .properties
        .headers
        .default
        .as_deref()
        .unwrap();
    let part = document.header_footer(header).unwrap();
    assert_eq!(part.blocks.len(), 1);
    let Block::Paragraph(p) = &part.blocks[0] else {
        panic!()
    };
    assert_eq!(p.text(), "Header text here");
}

/// Every prefix of a real file and scattered byte damage read without
/// panicking.
#[test]
fn damage_is_survivable() {
    for name in ["text", "lists", "tables", "notes", "image", "sections"] {
        let path = format!("{}/tests/fixtures/{name}.doc", env!("CARGO_MANIFEST_DIR"));
        let bytes = std::fs::read(path).unwrap();
        for length in (0..bytes.len()).step_by(211) {
            let _ = docboss_doc::read(&bytes[..length]);
        }
        let mut state = 0x9E37_79B9u32;
        for _ in 0..60 {
            let mut damaged = bytes.clone();
            for _ in 0..24 {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let at = (state as usize >> 7) % damaged.len();
                damaged[at] = (state >> 24) as u8;
            }
            let _ = docboss_doc::read(&damaged);
        }
    }
}

/// [MS-CFB] §2.2 and [MS-DOC] §2.5.1: a compound file whose header or
/// directory is lost still gives its text, read on from the FIB found at the
/// start of a sector; a Word 6 stream found that way is read in full.
#[test]
fn text_survives_a_lost_header_or_directory() {
    let read = |name: &str| {
        std::fs::read(format!(
            "{}/tests/fixtures/{name}.doc",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap()
    };
    let bytes = read("text");
    let mut zeroed = bytes.clone();
    zeroed[..512].fill(0);
    let truncated = &bytes[..bytes.len() * 3 / 4];
    for damaged in [&zeroed[..], truncated] {
        assert!(docboss_doc::is_doc(damaged));
        let document = docboss_doc::read(damaged).unwrap();
        assert!(plain_text(&document).contains("Main Title"));
        assert!(document
            .diagnostics
            .iter()
            .any(|d| d.message.contains("read on from the FIB")));
    }
    let mut word6 = read("word6-sections");
    word6[..512].fill(0);
    let document = docboss_doc::read(&word6).unwrap();
    assert!(plain_text(&document).contains("Corporate Insolvency"));
    assert!(!document.headers_footers.is_empty());
    assert!(!docboss_doc::is_doc(&[0u8; 4096]));
}

/// [MS-DOC] §2.2.6.2 RC4 and §2.2.6.3 RC4 CryptoAPI: the right password
/// decrypts, a wrong one and none are refused.
#[test]
fn encrypted_documents() {
    let read = |name: &str| {
        std::fs::read(format!(
            "{}/tests/fixtures/{name}.doc",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap()
    };
    let cryptoapi = read("encrypted-cryptoapi");
    let document = docboss_doc::read_with_password(&cryptoapi, "password").unwrap();
    assert_eq!(plain_text(&document).trim(), "This is a test");
    assert!(matches!(
        docboss_doc::read(&cryptoapi),
        Err(docboss_doc::Error::Encrypted)
    ));
    assert!(matches!(
        docboss_doc::read_with_password(&cryptoapi, "nope"),
        Err(docboss_doc::Error::WrongPassword)
    ));
    let rc4 = read("encrypted-rc4");
    let document = docboss_doc::read_with_password(&rc4, "tika").unwrap();
    assert!(plain_text(&document).contains("This is an encrypted Word 2007 File."));
    assert!(matches!(
        docboss_doc::read_with_password(&rc4, "nope"),
        Err(docboss_doc::Error::WrongPassword)
    ));
}

/// [MS-DOC] §2.2.6.1 XOR obfuscation: the fixtures are text.doc and
/// image.doc obfuscated by tools/xor_obfuscate.py; the right password gives
/// back their text and picture.
#[test]
fn xor_obfuscated_documents() {
    let read = |name: &str| {
        std::fs::read(format!(
            "{}/tests/fixtures/{name}.doc",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap()
    };
    let plain = docboss_doc::read(&read("text")).unwrap();
    let obfuscated = read("encrypted-xor");
    let document = docboss_doc::read_with_password(&obfuscated, "docboss").unwrap();
    assert_eq!(plain_text(&document), plain_text(&plain));
    assert_eq!(document.styles.styles.len(), plain.styles.styles.len());
    assert!(matches!(
        docboss_doc::read(&obfuscated),
        Err(docboss_doc::Error::Encrypted)
    ));
    assert!(matches!(
        docboss_doc::read_with_password(&obfuscated, "docbos"),
        Err(docboss_doc::Error::WrongPassword)
    ));
    let picture = docboss_doc::read(&read("image")).unwrap();
    let document =
        docboss_doc::read_with_password(&read("encrypted-xor-image"), "docboss").unwrap();
    assert_eq!(document.media.len(), 1);
    assert_eq!(document.media[0].data, picture.media[0].data);
}

/// [MS-DOC] §2.8.27 PlcfSpa anchors, [MS-ODRAW] §2.2.32 BLIP store and
/// [MS-DOC] §2.8.32 PlcftxbxTxt: a floating picture and a text box, each
/// placed from the column and paragraph its Spa bx and by name.
/// [MS-DOC] §2.8.27, §2.9.253, §2.9.171, §2.9.172, §2.8.32, §2.3.6.
#[test]
fn floating_picture_and_text_box() {
    let document = fixture("floating");
    let drawings: Vec<docboss_model::Drawing> = paragraphs(&document)
        .iter()
        .flat_map(|p| runs(p))
        .flat_map(|r| &r.content)
        .filter_map(|c| match c {
            RunContent::Drawing(d) => Some(d.as_ref().clone()),
            _ => None,
        })
        .collect();
    assert_eq!(drawings.len(), 2);
    assert_eq!(
        drawings[0].placement,
        DrawingPlacement::Anchored {
            horizontal: DrawingPosition::offset(PositionBase::Column, 2_602_865),
            vertical: DrawingPosition::offset(PositionBase::Paragraph, 0),
            behind_text: false,
        }
    );
    let media = document.media(drawings[0].media.unwrap()).unwrap();
    assert_eq!(media.content_type, "image/png");
    assert!(drawings[1].media.is_none());
    let texts: Vec<String> = drawings[1]
        .text_box
        .iter()
        .filter_map(|b| match b {
            Block::Paragraph(p) => Some(p.text()),
            Block::Table(_) => None,
        })
        .collect();
    assert_eq!(texts, ["Inside the box.", "Second box line."]);
    assert!(
        document.diagnostics.is_empty(),
        "{:?}",
        document.diagnostics
    );
}

/// A group of shapes becomes one drawing whose members sit at their child
/// anchors mapped from the group's coordinate space to its Spa rectangle;
/// a shaded fill becomes a gradient from the fill color to the back color.
/// [MS-ODRAW] §2.2.16, §2.2.38, §2.2.39, §2.3.7.1, §2.3.7.4, §2.3.7.14, §2.3.7.15.
#[test]
fn group_members_and_shaded_fill() {
    let document = fixture("group");
    let drawing = paragraphs(&document)
        .iter()
        .flat_map(|p| runs(p))
        .flat_map(|r| &r.content)
        .find_map(|c| match c {
            RunContent::Drawing(d) => Some(d.as_ref().clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!((drawing.width, drawing.height), (3_200_400, 914_400));
    assert_eq!(drawing.members.len(), 2);
    let [rect, ellipse] = [&drawing.members[0], &drawing.members[1]];
    assert_eq!((rect.x, rect.y), (0, 0));
    assert!((rect.drawing.width - 1_828_800).abs() < 1_000);
    assert!((ellipse.x - 2_286_000).abs() < 1_000);
    assert!(ellipse.drawing.shape.gradient.is_none());
    let gradient = rect.drawing.shape.gradient.unwrap();
    assert_eq!(gradient.angle, 5_400_000);
    assert_eq!(gradient.color_at(0.0), docboss_model::Color(255, 0, 0));
    assert_eq!(gradient.color_at(1.0), docboss_model::Color(0, 0, 255));
    assert!(
        document.diagnostics.is_empty(),
        "{:?}",
        document.diagnostics
    );
}

/// [MS-DOC] §2.9.286, §2.9.82, §2.2.4: the font table names the faces the
/// document uses.
#[test]
fn font_table() {
    let document = fixture("text");
    assert!(!document.fonts.is_empty());
    assert!(
        document.fonts.iter().all(|font| !font.name.is_empty()),
        "{:?}",
        document.fonts
    );
    assert!(
        document
            .fonts
            .iter()
            .any(|font| font.name == "Times New Roman"),
        "{:?}",
        document.fonts.iter().map(|f| &f.name).collect::<Vec<_>>()
    );
}

/// Word 6 and 95 files: the Word 6 FIB, 1-byte sprms in the FKPs, the
/// stylesheet and the section table, and the Plcfhdd stories sprmSGprfIhdt
/// names, read into the same model as Word 97 ([MS-DOC] §2.5.15 and §2.9.215
/// for the forms they map onto). Fixtures: Apache POI test data.
#[test]
fn word6_formatting_sections_and_headers() {
    let document = fixture("word6-sections");
    assert!(!document
        .diagnostics
        .iter()
        .any(|d| d.message.contains("text only")));
    let bold = paragraphs(&document)
        .into_iter()
        .flat_map(runs)
        .find(|run| {
            run.content
                .iter()
                .any(|c| matches!(c, RunContent::Text(t) if t.contains("Corporate Insolvency")))
        })
        .expect("heading run");
    assert_eq!(bold.properties.bold, Some(true));
    assert_eq!(document.sections[0].properties.page_size.width, 11907);
    let footers: Vec<_> = document
        .headers_footers
        .iter()
        .filter(|h| h.kind == docboss_model::HeaderFooterKind::Footer)
        .collect();
    assert!(!footers.is_empty());
    let footer_text: String = footers
        .iter()
        .flat_map(|f| f.blocks.iter())
        .map(|b| match b {
            Block::Paragraph(p) => p.text(),
            Block::Table(_) => String::new(),
        })
        .collect();
    assert!(footer_text.contains("sip10sco"), "{footer_text}");
    let three = fixture("word6-three-sections");
    assert_eq!(three.sections.len(), 3);
}

/// Word 95 tables: sprmTDefTable with 10-byte TCs, their merge bits and
/// Shd80 percentage shading ([MS-DOC] §2.9.321, §2.9.317, §2.9.248,
/// §2.9.121), and Cyrillic text in the code page the bytes name.
#[test]
fn word95_tables_merge_and_shade_their_cells() {
    let document = fixture("word95-tables");
    let table = first_table(&document);
    let title = &table.rows[0].cells;
    assert_eq!(title.len(), 1);
    assert!(title[0].properties.grid_span > 1);
    assert_eq!(
        title[0].properties.shading.and_then(|s| s.fill),
        Some(docboss_model::Color(191, 191, 191))
    );
    assert_eq!(table.rows[2].cells.len(), 5);
    assert!(plain_text(&document).contains("Алиготе"));
}

/// Word 2 files are a bare FIB and text: read as text.
#[test]
fn word2_files_read_as_text() {
    let bytes = std::fs::read(format!(
        "{}/tests/fixtures/word2.doc",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    assert!(docboss_doc::is_doc(&bytes));
    let document = docboss_doc::read(&bytes).unwrap();
    assert!(!plain_text(&document).trim().is_empty());
}

/// [MS-DOC] §2.9.253: the Spa's wr and wrk give each shape's wrapping,
/// with its distances from the text from the OfficeArt properties
/// ([MS-ODRAW] §2.3.4.9, §2.3.4.10, §2.3.4.11, §2.3.4.12); [MS-DOC]
/// §2.6.3, §2.9.208, §2.9.351, §2.9.357: sprmTPc, sprmTDxaAbs, sprmTDyaAbs
/// and the distance sprms place a floating table. The fixture is
/// LibreOffice's DOC export of the DOCX writer's wrap fixture.
#[test]
fn shape_wrapping_and_floating_tables() {
    use docboss_model::{TableFloat, WrapKind, WrapSide};
    let document = fixture("wrap");
    let drawings: Vec<docboss_model::Drawing> = paragraphs(&document)
        .iter()
        .flat_map(|p| runs(p))
        .flat_map(|r| &r.content)
        .filter_map(|c| match c {
            RunContent::Drawing(d) => Some(d.as_ref().clone()),
            _ => None,
        })
        .collect();
    let wraps: Vec<(WrapKind, WrapSide)> = drawings
        .iter()
        .map(|d| (d.wrap.kind, d.wrap.side))
        .collect();
    assert_eq!(
        wraps,
        [
            (WrapKind::Square, WrapSide::Both),
            (WrapKind::Tight, WrapSide::Left),
            (WrapKind::Square, WrapSide::Both),
            (WrapKind::TopAndBottom, WrapSide::Both),
        ]
    );
    assert_eq!(drawings[1].wrap.distance, [12_700, 25_400, 38_100, 50_800]);
    assert_eq!(
        first_table(&document).properties.floating,
        Some(TableFloat {
            horizontal: DrawingPosition::offset(PositionBase::Page, 1326 * 635),
            vertical: DrawingPosition::offset(PositionBase::Paragraph, 199 * 635),
            distance: [60, 120, 180, 360],
        })
    );
}

/// [MS-DOC] §2.6.2, §2.9.51, §2.9.208: a positioned paragraph holding a
/// drop cap, from sprmPDcs and sprmPPc. Apache POI's `test.doc`.
#[test]
fn drop_cap_frames() {
    use docboss_model::{DropCap, WrapKind};
    let document = fixture("drop-cap");
    let framed: Vec<&Paragraph> = paragraphs(&document)
        .into_iter()
        .filter(|p| p.properties.frame.is_some())
        .collect();
    assert_eq!(framed.len(), 1);
    let frame = framed[0].properties.frame.unwrap();
    assert_eq!(frame.drop_cap, DropCap::Margin);
    assert_eq!(frame.lines, 3);
    assert_eq!(frame.wrap, WrapKind::Square);
    assert_eq!(frame.horizontal.base, PositionBase::Page);
}

/// [MS-DOC] §2.1.4: an Equation Editor object's ObjectPool storage, named
/// by the sprmCPicLocation of its field separator, gives the equation of
/// the field's preview picture.
#[test]
fn equation_editor_objects() {
    let document = fixture("equation-editor");
    let math = paragraphs(&document)
        .iter()
        .flat_map(|p| p.inlines.iter())
        .filter_map(|inline| match inline {
            Inline::Field(field) => Some(&field.result),
            _ => None,
        })
        .flatten()
        .filter_map(|inline| match inline {
            Inline::Run(run) => Some(run),
            _ => None,
        })
        .flat_map(|run| &run.content)
        .find_map(|content| match content {
            RunContent::Drawing(d) => d.math.clone(),
            RunContent::Math(m) => Some(m.clone()),
            _ => None,
        })
        .expect("the equation reads");
    assert_eq!(docboss_model::linear_text(&math.nodes), "a=b/c");
}

fn fixture_bytes(name: &str) -> Vec<u8> {
    let path = format!("{}/tests/fixtures/{name}.doc", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(path).unwrap()
}

/// An `Equation Native` stream in a newer MTEF version is reported, not
/// dropped silently: the equation-editor fixture with its MTEF version
/// byte set to 4 and to 5.
#[test]
fn unreadable_equations_are_reported() {
    let original = fixture_bytes("equation-editor");
    let header = [0x1C, 0x00, 0x00, 0x00, 0x02, 0x00];
    let at = original
        .windows(header.len())
        .position(|w| w == header)
        .expect("the EQNOLEFILEHDR is in the file")
        + 28;
    assert_eq!(original[at], 3);
    for version in [4, 5] {
        let mut bytes = original.clone();
        bytes[at] = version;
        let document = docboss_doc::read(&bytes).unwrap();
        assert!(!plain_text(&document).contains("a=b/c"));
        assert!(
            document
                .diagnostics
                .iter()
                .any(|d| d.location.ends_with("Equation Native")
                    && d.message.contains("does not read")),
            "{:?}",
            document.diagnostics
        );
    }
}

/// A Word 6 file whose FIB counts endnote text (ccpEdn at 0x48) reports
/// that its endnotes are not read.
#[test]
fn word6_endnotes_are_reported() {
    let mut bytes = fixture_bytes("word6-sections");
    let fib = (512..bytes.len())
        .step_by(512)
        .find(|&at| bytes[at..at + 2] == [0xDC, 0xA5])
        .expect("the Word 6 FIB");
    bytes[fib + 0x48] = 1;
    let document = docboss_doc::read(&bytes).unwrap();
    assert!(document
        .diagnostics
        .iter()
        .any(|d| d.message == "Word 6/95 endnotes are not read"));
}

fn numbered_lines(document: &Document) -> Vec<String> {
    docboss_output::to_text(document, &Default::default())
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(str::to_string)
        .collect()
}

/// Word 6 ANLD numbering as LibreOffice shows it: a numbered run that an
/// unnumbered paragraph ends starts again (1., 1., 1.), and a Word 6.0
/// bullet (nfc 255) draws its Wingdings character.
#[test]
fn word6_anld_runs_restart_and_bullet() {
    let lines = numbered_lines(&fixture("word6-anld"));
    let labelled = |start: &str| lines.iter().find(|l| l.contains(start)).cloned().unwrap();
    assert!(labelled("Monsieur,").starts_with("1. "));
    assert!(labelled("Suite à notre").starts_with("1. "));
    assert!(labelled("Je vous présenterai").starts_with("1. "));
    assert!(labelled("Dans cette attente").starts_with("■ "));
}

/// A paragraph style's ANLD numbers the paragraphs of that style.
#[test]
fn word6_style_anld_numbers_its_paragraphs() {
    let lines = numbered_lines(&fixture("word6-style-anld"));
    let labelled = |start: &str| lines.iter().find(|l| l.contains(start)).cloned().unwrap();
    assert!(labelled("Suite à notre").starts_with("■ "));
    assert!(labelled("Je vous présenterai").starts_with("■ "));
    assert!(labelled("Monsieur,").starts_with("Monsieur"));
}

/// [MS-ODRAW] §2.3.4.7, §2.3.4.8: the tight picture of wrap.doc carries its
/// pWrapPolygonVertices, five points as in wrap.docx.
#[test]
fn wrap_polygons_are_read() {
    let document = fixture("wrap");
    let polygons: Vec<usize> = document
        .blocks()
        .filter_map(|block| match block {
            Block::Paragraph(p) => Some(p),
            Block::Table(_) => None,
        })
        .flat_map(|p| p.inlines.iter())
        .filter_map(|inline| match inline {
            Inline::Run(run) => Some(run),
            _ => None,
        })
        .flat_map(|run| &run.content)
        .filter_map(|content| match content {
            RunContent::Drawing(d) if d.wrap.kind == docboss_model::WrapKind::Tight => {
                Some(d.wrap.polygon.len())
            }
            _ => None,
        })
        .collect();
    assert_eq!(polygons, vec![5]);
}

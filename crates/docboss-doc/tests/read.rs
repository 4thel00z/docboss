use docboss_model::{
    plain_text, Block, Document, DrawingPlacement, Inline, Justification, NoteKind, Paragraph,
    RunContent, SourceFormat, Table, VerticalMerge,
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
            RunContent::Drawing(d) => Some(d.clone()),
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
}

/// [MS-DOC] §2.8.27 PlcfSpa anchors, [MS-ODRAW] §2.2.32 BLIP store and
/// [MS-DOC] §2.8.32 PlcftxbxTxt: a floating picture and a text box.
/// [MS-DOC] §2.8.27, §2.9.253, §2.9.171, §2.9.172, §2.8.32, §2.3.6.
#[test]
fn floating_picture_and_text_box() {
    let document = fixture("floating");
    let drawings: Vec<docboss_model::Drawing> = paragraphs(&document)
        .iter()
        .flat_map(|p| runs(p))
        .flat_map(|r| &r.content)
        .filter_map(|c| match c {
            RunContent::Drawing(d) => Some(d.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(drawings.len(), 2);
    assert!(matches!(
        drawings[0].placement,
        DrawingPlacement::Anchored { .. }
    ));
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

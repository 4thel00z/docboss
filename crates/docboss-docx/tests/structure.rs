use docboss_docx::read;
use docboss_model::{
    plain_text, Block, BorderStyle, Break, Color, DrawingPlacement, HeaderFooterKind, Inline,
    Justification, NumberFormat, NumberingRef, RevisionKind, RunContent, SectionBreak,
    VerticalAlign, VerticalMerge,
};
use docboss_testkit::Docx;

fn paragraphs(document: &docboss_model::Document) -> Vec<&docboss_model::Paragraph> {
    document
        .blocks()
        .filter_map(|block| match block {
            Block::Paragraph(p) => Some(p),
            _ => None,
        })
        .collect()
}

/// ECMA-376 Part 1 §17.3.1.22 and §17.3.2.25: paragraphs of runs with
/// text, tabs and breaks.
#[test]
fn paragraphs_runs_and_breaks() {
    let body = r#"<w:p><w:pPr><w:jc w:val="center"/></w:pPr><w:r><w:t>Hello</w:t></w:r><w:r><w:rPr><w:b/><w:sz w:val="28"/></w:rPr><w:t xml:space="preserve"> world</w:t><w:tab/><w:t>x</w:t><w:br/><w:t>y</w:t><w:br w:type="page"/></w:r></w:p><w:p/>"#;
    let doc = read(&Docx::new(body).build()).unwrap();
    assert_eq!(plain_text(&doc), "Hello world\tx\ny\n\n\n");
    let ps = paragraphs(&doc);
    assert_eq!(ps[0].properties.justification, Some(Justification::Center));
    let Inline::Run(run) = &ps[0].inlines[1] else {
        panic!()
    };
    assert_eq!(run.properties.bold, Some(true));
    assert_eq!(run.properties.size, Some(28));
    assert!(run.content.contains(&RunContent::Break(Break::Page)));
    assert!(doc.diagnostics.is_empty(), "{:?}", doc.diagnostics);
}

/// ECMA-376 Part 1 §17.7.4 and §17.7.5: styles and document defaults.
#[test]
fn styles_and_defaults() {
    let styles = r#"<w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val="22"/></w:rPr></w:rPrDefault><w:pPrDefault><w:pPr><w:spacing w:after="160"/></w:pPr></w:pPrDefault></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style><w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/><w:pPr><w:outlineLvl w:val="0"/></w:pPr><w:rPr><w:b/></w:rPr></w:style>"#;
    let body =
        r#"<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Title</w:t></w:r></w:p>"#;
    let doc = read(&Docx::new(body).styles(styles).build()).unwrap();
    let p = paragraphs(&doc)[0];
    assert_eq!(doc.styles.heading_level(p, &doc.numbering), Some(0));
    assert_eq!(doc.styles.default_run.size, Some(22));
    assert_eq!(doc.styles.default_paragraph.spacing.after, Some(160));
    assert_eq!(
        doc.styles
            .resolve_run(p.style_id.as_deref(), &Default::default())
            .bold,
        Some(true)
    );
}

/// ECMA-376 Part 1 §17.9: abstract numbering, instances and overrides.
/// ECMA-376 Part 1 §17.9.16, §17.9.1, §17.9.2, §17.9.15, §17.9.8, §17.9.26, §17.9.6, §17.9.25, §17.9.17, §17.9.11.
/// ECMA-376 Part 1 §17.9.3, §17.9.18: the paragraph's `w:ilvl` and `w:numId`.
#[test]
fn numbering_definitions() {
    let numbering = r#"<w:abstractNum w:abstractNumId="0"><w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%1."/></w:lvl><w:lvl w:ilvl="1"><w:numFmt w:val="bullet"/><w:lvlText w:val="&#xF0B7;"/></w:lvl></w:abstractNum><w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num><w:num w:numId="2"><w:abstractNumId w:val="0"/><w:lvlOverride w:ilvl="0"><w:startOverride w:val="5"/></w:lvlOverride></w:num>"#;
    let body = r#"<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="2"/></w:numPr></w:pPr><w:r><w:t>item</w:t></w:r></w:p>"#;
    let doc = read(&Docx::new(body).numbering(numbering).build()).unwrap();
    let reference = paragraphs(&doc)[0].properties.numbering.unwrap();
    assert_eq!(
        reference,
        NumberingRef {
            num_id: 2,
            level: 0
        }
    );
    let bullet = doc
        .numbering
        .level(NumberingRef {
            num_id: 1,
            level: 1,
        })
        .unwrap();
    assert_eq!(bullet.format, NumberFormat::Bullet);
    assert_eq!(bullet.text, "\u{2022}");
    assert_eq!(
        doc.numbering.counter().next(reference).as_deref(),
        Some("5.")
    );
}

/// ECMA-376 Part 1 §17.4: tables with grid spans and vertical merges.
/// ECMA-376 Part 1 §17.4.37, §17.4.48, §17.4.16, §17.4.59, §17.4.63, §17.4.78, §17.4.81, §17.4.49, §17.4.65, §17.4.69, §17.4.17, §17.4.84.
#[test]
fn tables() {
    let body = r#"<w:tbl><w:tblPr><w:tblW w:w="5000" w:type="pct"/></w:tblPr><w:tblGrid><w:gridCol w:w="2000"/><w:gridCol w:w="3000"/></w:tblGrid><w:tr><w:trPr><w:tblHeader/></w:trPr><w:tc><w:tcPr><w:gridSpan w:val="2"/></w:tcPr><w:p><w:r><w:t>wide</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:tc><w:tcPr><w:vMerge w:val="restart"/></w:tcPr><w:p><w:r><w:t>a</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>b</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:tc><w:tcPr><w:vMerge/></w:tcPr><w:p/></w:tc><w:tc><w:p><w:r><w:t>c</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#;
    let doc = read(&Docx::new(body).build()).unwrap();
    let Some(Block::Table(table)) = doc.blocks().next() else {
        panic!()
    };
    assert_eq!(table.grid, [2000, 3000]);
    assert_eq!(table.properties.width_pct, Some(5000));
    assert!(table.rows[0].properties.header);
    assert_eq!(table.rows[0].cells[0].span(), 2);
    assert_eq!(
        table.rows[1].cells[0].properties.vertical_merge,
        Some(VerticalMerge::Restart)
    );
    assert_eq!(
        table.rows[2].cells[0].properties.vertical_merge,
        Some(VerticalMerge::Continue)
    );
    assert_eq!(plain_text(&doc), "wide\na\nb\n\nc\n");
}

/// ECMA-376 Part 1 §17.6.17: a `w:sectPr` in a paragraph ends a section;
/// the body's own `w:sectPr` describes the last one.
/// ECMA-376 Part 1 §17.6.18, §17.6.13, §17.6.22, §17.6.11, §17.6.4, §17.6.12, §17.10.6, §17.10.5, §17.10.4.
#[test]
fn sections_split_at_paragraph_section_properties() {
    let body = r#"<w:p><w:pPr><w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:type w:val="continuous"/></w:sectPr></w:pPr><w:r><w:t>one</w:t></w:r></w:p><w:p><w:r><w:t>two</w:t></w:r></w:p><w:sectPr><w:headerReference w:type="default" r:id="rIdH"/><w:pgSz w:w="16838" w:h="11906" w:orient="landscape"/><w:pgMar w:top="720" w:right="720" w:bottom="720" w:left="720" w:header="360" w:footer="360" w:gutter="0"/><w:cols w:num="2" w:space="360"/><w:titlePg/><w:pgNumType w:start="3"/></w:sectPr>"#;
    let doc = read(
        &Docx::new(body)
            .header(
                "rIdH",
                "header1.xml",
                r#"<w:p><w:r><w:t>head</w:t></w:r></w:p>"#,
            )
            .build(),
    )
    .unwrap();
    assert_eq!(doc.sections.len(), 2);
    assert_eq!(doc.sections[0].properties.page_size.width, 11906);
    assert_eq!(doc.sections[0].properties.start, SectionBreak::Continuous);
    let last = &doc.sections[1].properties;
    assert_eq!(last.page_size.width, 16838);
    assert_eq!(last.margins.top, 720);
    assert_eq!(last.columns.count, 2);
    assert!(last.title_page);
    assert_eq!(last.page_number_start, Some(3));
    assert_eq!(last.headers.default.as_deref(), Some("rIdH"));
    let header = doc.header_footer("rIdH").unwrap();
    assert_eq!(header.blocks.len(), 1);
}

/// ECMA-376 Part 1 §17.16.18 and §17.16.5.25: complex fields, nested and
/// spanning runs; HYPERLINK fields become hyperlinks.
/// ECMA-376 Part 1 §17.16.23: the instruction comes from `w:instrText`.
#[test]
fn complex_fields() {
    let body = r#"<w:p><w:r><w:t>Page </w:t></w:r><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> PAGE </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>7</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r><w:r><w:fldChar w:fldCharType="begin"/><w:instrText>HYPERLINK "https://example.com"</w:instrText><w:fldChar w:fldCharType="separate"/><w:t>link</w:t><w:fldChar w:fldCharType="end"/></w:r></w:p>"#;
    let doc = read(&Docx::new(body).build()).unwrap();
    let p = paragraphs(&doc)[0];
    assert_eq!(p.text(), "Page 7link");
    let Inline::Field(field) = &p.inlines[1] else {
        panic!("{:?}", p.inlines)
    };
    assert_eq!(field.instruction, "PAGE");
    let Inline::Hyperlink(link) = &p.inlines[2] else {
        panic!("{:?}", p.inlines)
    };
    assert_eq!(link.target.as_deref(), Some("https://example.com"));
}

/// A field whose result spans paragraphs, like a table of contents, keeps
/// every paragraph's text.
#[test]
fn fields_spanning_paragraphs() {
    let body = r#"<w:p><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText>TOC \o "1-3"</w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>Intro</w:t></w:r></w:p><w:p><w:r><w:t>Body</w:t></w:r></w:p><w:p><w:r><w:t>Last</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p><w:p><w:r><w:t>After</w:t></w:r></w:p>"#;
    let doc = read(&Docx::new(body).build()).unwrap();
    assert_eq!(plain_text(&doc), "Intro\nBody\nLast\nAfter\n");
    let Inline::Field(field) = &paragraphs(&doc)[0].inlines[0] else {
        panic!()
    };
    assert!(field.instruction.starts_with("TOC"));
}

/// ECMA-376 Part 1 §17.16.19 and §17.16.22: simple fields and hyperlinks
/// by relationship.
#[test]
fn hyperlinks_and_simple_fields() {
    let body = r#"<w:p><w:hyperlink r:id="rIdL"><w:r><w:t>site</w:t></w:r></w:hyperlink><w:hyperlink w:anchor="top"><w:r><w:t>up</w:t></w:r></w:hyperlink><w:fldSimple w:instr=" NUMPAGES "><w:r><w:t>9</w:t></w:r></w:fldSimple></w:p>"#;
    let doc = read(
        &Docx::new(body)
            .external("rIdL", "hyperlink", "https://example.org/")
            .build(),
    )
    .unwrap();
    let p = paragraphs(&doc)[0];
    let Inline::Hyperlink(site) = &p.inlines[0] else {
        panic!()
    };
    assert_eq!(site.target.as_deref(), Some("https://example.org/"));
    let Inline::Hyperlink(up) = &p.inlines[1] else {
        panic!()
    };
    assert_eq!(up.anchor.as_deref(), Some("top"));
    let Inline::Field(field) = &p.inlines[2] else {
        panic!()
    };
    assert_eq!(field.instruction, "NUMPAGES");
    assert_eq!(p.text(), "siteup9");
}

/// ECMA-376 Part 1 §17.13.5: tracked insertions and deletions.
#[test]
fn revisions() {
    let body = r#"<w:p><w:r><w:t>keep </w:t></w:r><w:del w:author="A" w:date="2024-01-01T00:00:00Z"><w:r><w:delText>gone</w:delText></w:r></w:del><w:ins w:author="B"><w:r><w:t>new</w:t></w:r></w:ins></w:p>"#;
    let doc = read(&Docx::new(body).build()).unwrap();
    let p = paragraphs(&doc)[0];
    let Inline::Revision(deleted) = &p.inlines[1] else {
        panic!()
    };
    assert_eq!(deleted.kind, RevisionKind::Deletion);
    assert_eq!(deleted.author.as_deref(), Some("A"));
    assert_eq!(p.text(), "keep new");
}

/// ECMA-376 Part 3 §9.3: `mc:AlternateContent` reads the first understood
/// choice, else the fallback; `w:sdt` content is unwrapped.
/// ECMA-376 Part 3 §7.5; ECMA-376 Part 1 §17.5.2.
#[test]
fn markup_compatibility_and_content_controls() {
    let body = r#"<w:p><mc:AlternateContent><mc:Choice Requires="w14"><w:r><w:t>choice</w:t></w:r></mc:Choice><mc:Fallback><w:r><w:t>fallback</w:t></w:r></mc:Fallback></mc:AlternateContent></w:p><w:p xmlns:x="urn:unknown"><mc:AlternateContent><mc:Choice Requires="x"><w:r><w:t>unknown</w:t></w:r></mc:Choice><mc:Fallback><w:r><w:t>used</w:t></w:r></mc:Fallback></mc:AlternateContent></w:p><w:sdt><w:sdtPr/><w:sdtContent><w:p><w:sdt><w:sdtContent><w:r><w:t>control</w:t></w:r></w:sdtContent></w:sdt></w:p></w:sdtContent></w:sdt>"#;
    let doc = read(&Docx::new(body).build()).unwrap();
    assert_eq!(plain_text(&doc), "choice\nused\ncontrol\n");
}

const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x01\0\0\0\x01\x08\x06\0\0\0\x1f\x15\xc4\x89\0\0\0\rIDATx\x9cc\xf8\x0f\0\0\x01\x01\0\x05\x18\xd8N\0\0\0\0IEND\xaeB`\x82";

/// ECMA-376 Part 1 §20.4.2.8 and §20.4.2.3: inline and anchored drawings
/// with their images.
/// ECMA-376 Part 1 §20.4.2.7, §20.4.2.5, §20.4.2.10, §20.4.2.11, §20.4.2.12, §20.4.2.38.
#[test]
fn drawings_and_media() {
    let body = r#"<w:p><w:r><w:drawing><wp:inline><wp:extent cx="914400" cy="457200"/><wp:docPr id="1" name="Pic" descr="A dot"/><a:graphic><a:graphicData><pic:pic><pic:blipFill><a:blip r:embed="rIdImg"/></pic:blipFill></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r><w:r><w:drawing><wp:anchor behindDoc="1"><wp:positionH relativeFrom="page"><wp:posOffset>100</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>200</wp:posOffset></wp:positionV><wp:extent cx="10" cy="20"/><a:graphic><a:graphicData><wps:wsp xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:txbx><w:txbxContent><w:p><w:r><w:t>boxed</w:t></w:r></w:p></w:txbxContent></wps:txbx></wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r></w:p>"#;
    let doc = read(
        &Docx::new(body)
            .image("rIdImg", "media/image1.png", PNG)
            .build(),
    )
    .unwrap();
    assert_eq!(doc.media.len(), 1);
    assert_eq!(doc.media[0].content_type, "image/png");
    assert_eq!(doc.media[0].name, "word/media/image1.png");
    let p = paragraphs(&doc)[0];
    let Inline::Run(run) = &p.inlines[0] else {
        panic!()
    };
    let RunContent::Drawing(picture) = &run.content[0] else {
        panic!()
    };
    assert_eq!(picture.media, Some(docboss_model::MediaId(0)));
    assert_eq!((picture.width, picture.height), (914400, 457200));
    assert_eq!(picture.description.as_deref(), Some("A dot"));
    let Inline::Run(run) = &p.inlines[1] else {
        panic!()
    };
    let RunContent::Drawing(shape) = &run.content[0] else {
        panic!()
    };
    assert_eq!(
        shape.placement,
        DrawingPlacement::Anchored {
            x: 100,
            y: 200,
            behind_text: true,
            relative_to_page: true
        }
    );
    let Block::Paragraph(boxed) = &shape.text_box[0] else {
        panic!()
    };
    assert_eq!(boxed.text(), "boxed");
}

/// ECMA-376 Part 1 §17.11 and §17.13.4: footnotes, endnotes and comments,
/// separator notes skipped.
/// ECMA-376 Part 1 §17.11.15, §17.11.10, §17.11.23, §17.11.13, §17.11.14, §17.11.7, §17.11.8, §17.11.2.
#[test]
fn notes_and_comments() {
    let body = r#"<w:p><w:commentRangeStart w:id="0"/><w:r><w:t>text</w:t></w:r><w:commentRangeEnd w:id="0"/><w:r><w:commentReference w:id="0"/></w:r><w:r><w:footnoteReference w:id="1"/></w:r><w:r><w:endnoteReference w:id="2"/></w:r></w:p>"#;
    let footnotes = r#"<w:footnote w:type="separator" w:id="-1"><w:p><w:r><w:separator/></w:r></w:p></w:footnote><w:footnote w:id="1"><w:p><w:r><w:footnoteRef/></w:r><w:r><w:t>note</w:t></w:r></w:p></w:footnote>"#;
    let endnotes = r#"<w:endnote w:id="2"><w:p><w:r><w:t>end</w:t></w:r></w:p></w:endnote>"#;
    let comments = r#"<w:comment w:id="0" w:author="Ann" w:initials="A" w:date="2024-02-02T00:00:00Z"><w:p><w:r><w:t>remark</w:t></w:r></w:p></w:comment>"#;
    let doc = read(
        &Docx::new(body)
            .footnotes(footnotes)
            .endnotes(endnotes)
            .comments(comments)
            .build(),
    )
    .unwrap();
    assert_eq!(doc.footnotes.len(), 1);
    let Block::Paragraph(note) = &doc.footnote(1).unwrap().blocks[0] else {
        panic!()
    };
    let Inline::Run(first) = &note.inlines[0] else {
        panic!()
    };
    assert_eq!(first.content, [RunContent::NoteNumber]);
    assert_eq!(doc.endnote(2).unwrap().blocks.len(), 1);
    let comment = doc.comment(0).unwrap();
    assert_eq!(comment.author.as_deref(), Some("Ann"));
    let p = paragraphs(&doc)[0];
    assert!(matches!(p.inlines[0], Inline::CommentRangeStart(0)));
}

/// ECMA-376 Part 2 §8.3: core properties, and extended properties.
/// ECMA-376 Part 2 §8.2, §8.3.3, §8.3.4; ECMA-376 Part 1 §22.2.
#[test]
fn document_properties() {
    let core = r#"<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/"><dc:title>Report</dc:title><dc:creator>Me</dc:creator><dcterms:created>2024-05-01T10:00:00Z</dcterms:created></cp:coreProperties>"#;
    let app = r#"<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties"><Application>Microsoft Office Word</Application><Pages>3</Pages></Properties>"#;
    let doc = read(
        &Docx::new("<w:p/>")
            .raw("docProps/core.xml", core.as_bytes())
            .raw("docProps/app.xml", app.as_bytes())
            .build(),
    )
    .unwrap();
    assert_eq!(doc.metadata.title.as_deref(), Some("Report"));
    assert_eq!(doc.metadata.creator.as_deref(), Some("Me"));
    assert_eq!(
        doc.metadata.created.as_deref(),
        Some("2024-05-01T10:00:00Z")
    );
    assert_eq!(
        doc.metadata.application.as_deref(),
        Some("Microsoft Office Word")
    );
    assert_eq!(doc.metadata.pages, Some(3));
}

/// ISO/IEC 29500 strict namespaces read like transitional ones.
#[test]
fn strict_namespaces() {
    let document = r#"<w:document xmlns:w="http://purl.oclc.org/ooxml/wordprocessingml/main"><w:body><w:p><w:pPr><w:ind w:left="1in"/></w:pPr><w:r><w:rPr><w:b w:val="true"/></w:rPr><w:t>strict</w:t></w:r></w:p></w:body></w:document>"#;
    let rels = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://purl.oclc.org/ooxml/officeDocument/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
    let data = docboss_testkit::ZipWriter::new()
        .deflated("_rels/.rels", rels.as_bytes())
        .deflated("word/document.xml", document.as_bytes())
        .finish();
    let doc = read(&data).unwrap();
    assert_eq!(plain_text(&doc), "strict\n");
    let p = paragraphs(&doc)[0];
    assert_eq!(p.properties.indentation.left, Some(1440));
    let Inline::Run(run) = &p.inlines[0] else {
        panic!()
    };
    assert_eq!(run.properties.bold, Some(true));
}

/// A package without content types or relationships still opens through
/// `word/document.xml`, with the loss reported.
#[test]
fn bare_package_falls_back_to_the_default_part_name() {
    let document = r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>bare</w:t></w:r></w:p></w:body></w:document>"#;
    let data = docboss_testkit::ZipWriter::new()
        .deflated("word/document.xml", document.as_bytes())
        .finish();
    let doc = read(&data).unwrap();
    assert_eq!(plain_text(&doc), "bare\n");
    assert!(!doc.diagnostics.is_empty());
}

#[test]
fn rejects_non_packages() {
    assert!(read(b"not a zip").is_err());
    let data = docboss_testkit::ZipWriter::new()
        .stored("a.txt", b"x")
        .finish();
    assert!(matches!(
        read(&data),
        Err(docboss_docx::Error::NotWordDocument(_))
    ));
    assert!(matches!(
        read(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1, 0]),
        Err(docboss_docx::Error::Encrypted)
    ));
}

/// Parts cut short or garbled never panic the reader.
#[test]
fn damaged_packages_never_panic() {
    let body = r#"<w:p><w:r><w:t>Hello</w:t></w:r></w:p><w:tbl><w:tr><w:tc><w:p><w:r><w:fldChar w:fldCharType="begin"/></w:r></w:p></w:tc></w:tr></w:tbl>"#;
    let data = Docx::new(body).styles("<w:style/>").build();
    for len in (0..data.len()).step_by(3) {
        let _ = read(&data[..len]);
    }
    let mut flipped = data.clone();
    for i in (0..flipped.len()).step_by(11) {
        flipped[i] ^= 0x5A;
        let _ = read(&flipped);
    }
}

/// ECMA-376 Part 1 §17.17.2.1: imported alternative format content is not
/// read, and the loss is reported.
#[test]
fn alternative_format_chunks_are_reported() {
    let doc = read(&Docx::new(r#"<w:altChunk r:id="rIdA"/><w:p/>"#).build()).unwrap();
    assert!(doc
        .diagnostics
        .iter()
        .any(|d| d.message.contains("altChunk")));
}

/// ECMA-376 Part 1 §17.4.62, §17.4.28, §17.4.50, §17.4.38, §17.4.31, §17.4.42, §17.4.52: table-level properties.
/// ECMA-376 Part 1 §17.4.80, §17.4.6: row height and rows that cannot split.
/// ECMA-376 Part 1 §17.4.71, §17.4.66, §17.4.32, §17.4.83, §17.4.68: cell width, borders, shading, alignment and margins.
/// ECMA-376 Part 1 §17.3.4: each border's style, width and color.
#[test]
fn table_row_and_cell_properties() {
    let body = r#"<w:tbl><w:tblPr><w:tblStyle w:val="Grid"/><w:jc w:val="center"/><w:tblInd w:w="144" w:type="dxa"/><w:tblBorders><w:top w:val="double" w:sz="8" w:space="0" w:color="FF0000"/></w:tblBorders><w:shd w:val="clear" w:fill="EEEEEE"/><w:tblCellMar><w:left w:w="72" w:type="dxa"/></w:tblCellMar><w:tblLayout w:type="fixed"/></w:tblPr><w:tblGrid><w:gridCol w:w="4000"/></w:tblGrid><w:tr><w:trPr><w:trHeight w:val="500" w:hRule="exact"/><w:cantSplit/></w:trPr><w:tc><w:tcPr><w:tcW w:w="4000" w:type="dxa"/><w:tcBorders><w:bottom w:val="single" w:sz="4" w:color="00FF00"/></w:tcBorders><w:shd w:val="clear" w:fill="112233"/><w:vAlign w:val="center"/><w:tcMar><w:top w:w="40" w:type="dxa"/></w:tcMar></w:tcPr><w:p/></w:tc></w:tr></w:tbl>"#;
    let doc = read(&Docx::new(body).build()).unwrap();
    let Some(Block::Table(table)) = doc.blocks().next() else {
        panic!()
    };
    let p = &table.properties;
    assert_eq!(p.style_id.as_deref(), Some("Grid"));
    assert_eq!(p.justification, Some(Justification::Center));
    assert_eq!(p.indent, Some(144));
    let top = p.borders.unwrap().top.unwrap();
    assert_eq!(top.style, BorderStyle::Double);
    assert_eq!(top.size, 8);
    assert_eq!(top.color, Some(Color(255, 0, 0)));
    assert_eq!(p.shading.unwrap().fill, Some(Color(0xEE, 0xEE, 0xEE)));
    assert_eq!(p.cell_margins.unwrap()[1], 72);
    assert!(p.fixed_layout);
    let row = &table.rows[0].properties;
    assert_eq!(
        (row.height, row.height_exact, row.cant_split),
        (Some(500), true, true)
    );
    let cell = &table.rows[0].cells[0].properties;
    assert_eq!(cell.width, Some(4000));
    assert_eq!(
        cell.borders.unwrap().bottom.unwrap().color,
        Some(Color(0, 255, 0))
    );
    assert_eq!(cell.shading.unwrap().fill, Some(Color(0x11, 0x22, 0x33)));
    assert_eq!(cell.vertical_align, Some(VerticalAlign::Center));
    assert_eq!(cell.margins.unwrap()[0], 40);
}

/// ECMA-376 Part 1 §17.6.3: explicit column widths; §17.10.2 and §17.10.3:
/// footer references resolve to footer parts; §17.10.1: even and odd
/// headers come from the settings part.
#[test]
fn columns_footers_and_even_odd_settings() {
    let body = r#"<w:p/><w:sectPr><w:footerReference w:type="even" r:id="rIdF"/><w:cols w:num="2" w:equalWidth="0"><w:col w:w="3000" w:space="400"/><w:col w:w="5000"/></w:cols></w:sectPr>"#;
    let settings = Docx::wrap(
        "w:settings",
        r#"<w:evenAndOddHeaders/><w:defaultTabStop w:val="360"/>"#,
    );
    let doc = read(
        &Docx::new(body)
            .footer(
                "rIdF",
                "footer1.xml",
                r#"<w:p><w:r><w:t>foot</w:t></w:r></w:p>"#,
            )
            .part(
                "rIdS",
                "settings",
                "settings.xml",
                "application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml",
                settings.as_bytes(),
            )
            .build(),
    )
    .unwrap();
    let section = &doc.sections[0].properties;
    assert_eq!(section.columns.widths, [(3000, 400), (5000, 0)]);
    assert_eq!(section.footers.even.as_deref(), Some("rIdF"));
    let footer = doc.header_footer("rIdF").unwrap();
    assert_eq!(footer.kind, HeaderFooterKind::Footer);
    assert!(doc.settings.even_and_odd_headers);
    assert_eq!(doc.settings.default_tab_stop, 360);
}

/// ECMA-376 Part 1 §17.13.6: bookmark starts and ends keep their ids and
/// names.
#[test]
fn bookmarks() {
    let body = r#"<w:p><w:bookmarkStart w:id="3" w:name="intro"/><w:r><w:t>x</w:t></w:r><w:bookmarkEnd w:id="3"/></w:p>"#;
    let doc = read(&Docx::new(body).build()).unwrap();
    let p = paragraphs(&doc)[0];
    assert_eq!(
        p.inlines[0],
        Inline::BookmarkStart {
            id: 3,
            name: "intro".into()
        }
    );
    assert_eq!(p.inlines[2], Inline::BookmarkEnd { id: 3 });
}

/// ECMA-376 Part 2 §6.2.2.3 and §7.3.5: part names compare
/// case-insensitively, so a relationship to `/word/document.xml` finds the
/// ZIP item `WORD/Document.XML`; §7.2.3.5 and §6.2.3: its media type comes
/// from the `Default` element for its extension; §6.5.3: the package
/// relationship names it.
#[test]
fn part_names_compare_case_insensitively() {
    let types = r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#;
    let rels = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="/word/document.xml"/></Relationships>"#;
    let document = Docx::wrap(
        "w:document",
        r#"<w:body><w:p><w:r><w:t>found</w:t></w:r></w:p></w:body>"#,
    );
    let data = docboss_testkit::ZipWriter::new()
        .stored("[Content_Types].xml", types.as_bytes())
        .stored("_rels/.rels", rels.as_bytes())
        .deflated("WORD/Document.XML", document.as_bytes())
        .finish();
    let doc = read(&data).unwrap();
    assert_eq!(plain_text(&doc), "found\n");
}

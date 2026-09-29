use docboss_docx::read;
use docboss_model::{
    plain_text, Block, BorderStyle, Break, Color, DrawingPlacement, DrawingPosition,
    HeaderFooterKind, Inline, Justification, LineCap, LineJoin, NumberFormat, NumberingRef,
    PositionAlign, PositionBase, RevisionKind, RunContent, SectionBreak, VerticalAlign,
    VerticalMerge,
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
/// ECMA-376 Part 1 §17.2.3, §17.2.2, §17.3.1, §17.3.2, §17.3.3.
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
/// ECMA-376 Part 1 §17.7.8, §17.7.2, §17.7.1: a paragraph style inherits from the style it is based on.
/// ECMA-376 Part 1 §11.3.12.
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
/// ECMA-376 Part 1 §11.3.11.
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
/// ECMA-376 Part 1 §17.6.18, §17.6.13, §17.6.22, §17.6.11, §17.6.4, §17.6.12, §17.10.6, §17.10.5, §17.10.4, §11.3.9.
#[test]
fn sections_split_at_paragraph_section_properties() {
    let body = r#"<w:p><w:pPr><w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:type w:val="continuous"/></w:sectPr></w:pPr><w:r><w:t>one</w:t></w:r></w:p><w:p><w:r><w:t>two</w:t></w:r></w:p><w:sectPr><w:headerReference w:type="default" r:id="rIdH"/><w:pgSz w:w="16838" w:h="11906" w:orient="landscape"/><w:pgMar w:top="720" w:right="720" w:bottom="720" w:left="720" w:header="360" w:footer="360" w:gutter="0"/><w:cols w:num="2" w:space="360"/><w:titlePg/><w:pgNumType w:fmt="lowerRoman" w:start="3"/></w:sectPr>"#;
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
    assert_eq!(
        last.page_number_format,
        Some(docboss_model::NumberFormat::LowerRoman)
    );
    assert_eq!(last.headers.default.as_deref(), Some("rIdH"));
    let header = doc.header_footer("rIdH").unwrap();
    assert_eq!(header.blocks.len(), 1);
}

/// Page borders, their sides and where they are measured from.
/// ECMA-376 Part 1 §17.6.10, §17.6.21, §17.6.7, §17.6.2, §17.6.15.
/// ECMA-376 Part 1 §17.18.62, §17.18.63, §17.18.64.
#[test]
fn page_borders() {
    let body = r#"<w:p/><w:sectPr><w:pgBorders w:offsetFrom="page" w:display="notFirstPage" w:zOrder="back"><w:top w:val="dashed" w:sz="8" w:space="24" w:color="FF0000"/><w:left w:val="single" w:sz="4" w:space="12"/><w:bottom w:val="double" w:sz="4" w:space="24"/><w:right w:val="none" w:sz="0" w:space="0"/></w:pgBorders></w:sectPr>"#;
    let doc = read(&Docx::new(body).build()).unwrap();
    let borders = doc.sections[0].properties.page_borders.unwrap();
    assert_eq!(borders.offset_from, docboss_model::PageBorderOffset::Page);
    assert_eq!(
        borders.display,
        docboss_model::PageBorderDisplay::NotFirstPage
    );
    assert!(borders.behind_text);
    let top = borders.sides.top.unwrap();
    assert_eq!(
        (top.style, top.size, top.space),
        (BorderStyle::Dashed, 8, 24)
    );
    assert_eq!(top.color, Some(Color(0xFF, 0, 0)));
    assert_eq!(borders.sides.left.unwrap().space, 12);
    assert_eq!(borders.sides.bottom.unwrap().style, BorderStyle::Double);
    assert_eq!(borders.sides.right.unwrap().style, BorderStyle::None);
    let plain = read(
        &Docx::new(
            r#"<w:p/><w:sectPr><w:pgBorders><w:top w:val="single"/></w:pgBorders></w:sectPr>"#,
        )
        .build(),
    )
    .unwrap();
    let plain = plain.sections[0].properties.page_borders.unwrap();
    assert_eq!(plain.offset_from, docboss_model::PageBorderOffset::Text);
    assert_eq!(plain.display, docboss_model::PageBorderDisplay::AllPages);
    assert!(!plain.behind_text);
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
/// ECMA-376 Part 3 §7.5, §7.6, §7.7; ECMA-376 Part 1 §17.5.2.
#[test]
fn markup_compatibility_and_content_controls() {
    let body = r#"<w:p><mc:AlternateContent><mc:Choice Requires="w14"><w:r><w:t>choice</w:t></w:r></mc:Choice><mc:Fallback><w:r><w:t>fallback</w:t></w:r></mc:Fallback></mc:AlternateContent></w:p><w:p xmlns:x="urn:unknown"><mc:AlternateContent><mc:Choice Requires="x"><w:r><w:t>unknown</w:t></w:r></mc:Choice><mc:Fallback><w:r><w:t>used</w:t></w:r></mc:Fallback></mc:AlternateContent></w:p><w:sdt><w:sdtPr/><w:sdtContent><w:p><w:sdt><w:sdtContent><w:r><w:t>control</w:t></w:r></w:sdtContent></w:sdt></w:p></w:sdtContent></w:sdt>"#;
    let doc = read(&Docx::new(body).build()).unwrap();
    assert_eq!(plain_text(&doc), "choice\nused\ncontrol\n");
}

/// ECMA-376 Part 1 §20.4.2.1, §20.4.2.2, §20.4.3.1, §20.4.3.2, §20.4.3.4,
/// §20.4.3.5: `wp:align` and every `relativeFrom` base on each axis; a base
/// the axis does not allow is reported and read as the default.
#[test]
fn drawing_alignment_and_bases() {
    let anchor = |h: &str, v: &str| {
        format!(
            r#"<w:r><w:drawing><wp:anchor>{h}{v}<wp:extent cx="10" cy="20"/><a:graphic><a:graphicData><wps:wsp xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:txbx><w:txbxContent><w:p><w:r><w:t>x</w:t></w:r></w:p></w:txbxContent></wps:txbx></wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>"#
        )
    };
    let body = format!(
        "<w:p>{}{}{}</w:p>",
        anchor(
            r#"<wp:positionH relativeFrom="rightMargin"><wp:align>right</wp:align></wp:positionH>"#,
            r#"<wp:positionV relativeFrom="bottomMargin"><wp:align>center</wp:align></wp:positionV>"#,
        ),
        anchor(
            r#"<wp:positionH relativeFrom="insideMargin"><wp:align>outside</wp:align></wp:positionH>"#,
            r#"<wp:positionV relativeFrom="line"><wp:posOffset>-50</wp:posOffset></wp:positionV>"#,
        ),
        anchor(
            r#"<wp:positionH relativeFrom="paragraph"><wp:posOffset>7</wp:posOffset></wp:positionH>"#,
            r#"<wp:positionV relativeFrom="margin"><wp:align>bottom</wp:align></wp:positionV>"#,
        ),
    );
    let doc = read(&Docx::new(&body).build()).unwrap();
    let placements: Vec<DrawingPlacement> = paragraphs(&doc)[0]
        .inlines
        .iter()
        .filter_map(|inline| match inline {
            Inline::Run(run) => match &run.content[0] {
                RunContent::Drawing(drawing) => Some(drawing.placement),
                _ => None,
            },
            _ => None,
        })
        .collect();
    let aligned = |base, align| DrawingPosition {
        base,
        align: Some(align),
        offset: 0,
    };
    let anchored = |horizontal, vertical| DrawingPlacement::Anchored {
        horizontal,
        vertical,
        behind_text: false,
    };
    assert_eq!(
        placements,
        vec![
            anchored(
                aligned(PositionBase::RightMargin, PositionAlign::End),
                aligned(PositionBase::BottomMargin, PositionAlign::Center),
            ),
            anchored(
                aligned(PositionBase::InsideMargin, PositionAlign::Outside),
                DrawingPosition::offset(PositionBase::Line, -50),
            ),
            anchored(
                DrawingPosition::offset(PositionBase::Column, 7),
                aligned(PositionBase::Margin, PositionAlign::End),
            ),
        ]
    );
    assert!(doc
        .diagnostics
        .iter()
        .any(|d| d.message.contains("relativeFrom=\"paragraph\"")));
}

const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x01\0\0\0\x01\x08\x06\0\0\0\x1f\x15\xc4\x89\0\0\0\rIDATx\x9cc\xf8\x0f\0\0\x01\x01\0\x05\x18\xd8N\0\0\0\0IEND\xaeB`\x82";

/// ECMA-376 Part 1 §20.4.2.8 and §20.4.2.3: inline and anchored drawings
/// with their images.
/// ECMA-376 Part 1 §20.4.2.7, §20.4.2.5, §20.4.2.10, §20.4.2.11, §20.4.2.12, §20.4.2.38, §20.4.2.42, §20.4.2.37, §15.2.14.
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
            horizontal: DrawingPosition::offset(PositionBase::Page, 100),
            vertical: DrawingPosition::offset(PositionBase::Paragraph, 200),
            behind_text: true,
        }
    );
    let Block::Paragraph(boxed) = &shape.text_box[0] else {
        panic!()
    };
    assert_eq!(boxed.text(), "boxed");
}

/// ECMA-376 Part 1 §17.11 and §17.13.4: footnotes, endnotes and comments,
/// separator notes skipped.
/// ECMA-376 Part 1 §17.11.15, §17.11.10, §17.11.23, §17.11.13, §17.11.14, §17.11.7, §17.11.8, §17.11.2, §11.3.7, §11.3.4, §11.3.2.
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
/// ECMA-376 Part 2 §8.2, §8.3.3, §8.3.4; ECMA-376 Part 1 §22.2, §22.2.2, §15.2.12.
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
/// ECMA-376 Part 1 §17.4.76, §17.4.3, §17.4.34: a table top border, a cell bottom border and a leading default margin.
/// ECMA-376 Part 1 §17.4.71, §17.4.66, §17.4.32, §17.4.83, §17.4.68: cell width, borders, shading, alignment and margins.
/// ECMA-376 Part 1 §17.3.4: each border's style, width and color.
/// ECMA-376 Part 1 §17.3.5: shading fills.
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
/// ECMA-376 Part 1 §17.6.3, §17.10.2, §17.10.3, §17.10.1, §17.15.1, §11.3.6, §11.3.3.
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
/// ECMA-376 Part 2 §6.2.2, §6.2.3, §6.5.3, §7.2.3, §7.3.5, §7.2.5, §7.3.2, §7.3.3, §7.3.7; ECMA-376 Part 1 §11.3.10.
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

fn first_drawing(doc: &docboss_model::Document) -> docboss_model::Drawing {
    paragraphs(doc)[0]
        .inlines
        .iter()
        .find_map(|inline| match inline {
            Inline::Run(run) => run.content.iter().find_map(|content| match content {
                RunContent::Drawing(drawing) => Some(drawing.as_ref().clone()),
                _ => None,
            }),
            _ => None,
        })
        .expect("a drawing")
}

/// ECMA-376 Part 1 §20.4.2.35 and §20.1.2.2.24: a text box shape's fill
/// and outline; §20.4.2.22: its insets, text anchor and `a:spAutoFit`.
/// ECMA-376 Part 1 §20.1.8.54, §20.1.2.3.32, §20.1.2.3.29, §20.1.10.48.
#[test]
fn text_box_shape_format() {
    let body = r#"<w:p><w:r><w:drawing><wp:anchor><wp:extent cx="1270000" cy="635000"/><a:graphic><a:graphicData><wps:wsp xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:spPr><a:solidFill><a:schemeClr val="lt1"/></a:solidFill><a:ln w="6350"><a:solidFill><a:srgbClr val="FF0000"/></a:solidFill></a:ln></wps:spPr><wps:txbx><w:txbxContent><w:p><w:r><w:t>boxed</w:t></w:r></w:p></w:txbxContent></wps:txbx><wps:bodyPr lIns="0" tIns="12700" anchor="ctr"><a:spAutoFit/></wps:bodyPr></wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r></w:p>"#;
    let shape = first_drawing(&read(&Docx::new(body).build()).unwrap()).shape;
    assert_eq!(shape.fill, Some(docboss_model::Color::WHITE));
    assert_eq!(shape.outline, Some(docboss_model::Color(255, 0, 0)));
    assert_eq!(shape.outline_width, Some(6350));
    assert_eq!(shape.insets, Some([0, 12700, 91440, 45720]));
    assert_eq!(
        shape.text_anchor,
        Some(docboss_model::VerticalAlign::Center)
    );
    assert!(shape.auto_fit);
}

/// ECMA-376 Part 1 §17.3.3.19: a VML text box's fill, stroke, inset and
/// fit-to-text style.
#[test]
fn vml_text_box_shape_format() {
    let body = r##"<w:p><w:r><w:pict xmlns:v="urn:schemas-microsoft-com:vml"><v:shape style="position:absolute;width:100pt;height:50pt" fillcolor="#0f0" stroked="f"><v:textbox inset="1pt,2pt,3pt,4pt" style="mso-fit-shape-to-text:t"><w:txbxContent><w:p><w:r><w:t>vml</w:t></w:r></w:p></w:txbxContent></v:textbox></v:shape></w:pict></w:r></w:p>"##;
    let drawing = first_drawing(&read(&Docx::new(body).build()).unwrap());
    assert_eq!((drawing.width, drawing.height), (1_270_000, 635_000));
    assert_eq!(drawing.shape.fill, Some(docboss_model::Color(0, 255, 0)));
    assert_eq!(drawing.shape.outline, None);
    assert_eq!(drawing.shape.insets, Some([12_700, 25_400, 38_100, 50_800]));
    assert!(drawing.shape.auto_fit);
}

/// ECMA-376 Part 1 §20.1.8.48, §20.1.10.49: a preset dash; §20.1.10.31:
/// the `cap` attribute; §20.1.8.43: a miter join; §20.1.8.21 and
/// §20.1.8.22: custom dash stops, in thousandths of a percent or with a
/// percent sign.
#[test]
fn text_box_outline_dashes_caps_and_joins() {
    let shape = |ln: &str| {
        let body = format!(
            r#"<w:p><w:r><w:drawing><wp:anchor><wp:extent cx="1270000" cy="635000"/><a:graphic><a:graphicData><wps:wsp xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:spPr>{ln}</wps:spPr><wps:txbx><w:txbxContent><w:p/></w:txbxContent></wps:txbx><wps:bodyPr/></wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r></w:p>"#
        );
        first_drawing(&read(&Docx::new(&body).build()).unwrap()).shape
    };
    let preset = shape(
        r#"<a:ln w="12700" cap="flat"><a:prstDash val="lgDashDotDot"/><a:miter lim="800000"/></a:ln>"#,
    );
    assert_eq!(
        preset.outline_dash.unwrap().stops(),
        &[(800, 300), (100, 300), (100, 300)]
    );
    assert_eq!(preset.outline_cap, LineCap::Flat);
    assert_eq!(preset.outline_join, LineJoin::Miter);
    let solid = shape(r#"<a:ln><a:prstDash val="solid"/></a:ln>"#);
    assert_eq!(solid.outline_dash, None);
    assert_eq!(solid.outline_cap, LineCap::Round);
    let custom = shape(
        r#"<a:ln cap="sq"><a:custDash><a:ds d="800000" sp="300000"/><a:ds d="100%" sp="300%"/></a:custDash></a:ln>"#,
    );
    assert_eq!(
        custom.outline_dash.unwrap().stops(),
        &[(800, 300), (100, 300)]
    );
    assert_eq!(custom.outline_cap, LineCap::Square);
}

/// ECMA-376 Part 1 §17.3.3.19: a VML shape's `v:stroke` dash style, end
/// cap and join style.
#[test]
fn vml_text_box_stroke_dashes() {
    let body = r##"<w:p><w:r><w:pict xmlns:v="urn:schemas-microsoft-com:vml"><v:shape style="position:absolute;width:100pt;height:50pt"><v:stroke dashstyle="longDashDot" endcap="flat" joinstyle="miter"/><v:textbox><w:txbxContent><w:p/></w:txbxContent></v:textbox></v:shape></w:pict></w:r></w:p>"##;
    let shape = first_drawing(&read(&Docx::new(body).build()).unwrap()).shape;
    assert_eq!(
        shape.outline_dash.unwrap().stops(),
        &[(800, 300), (100, 300)]
    );
    assert_eq!(shape.outline_cap, LineCap::Flat);
    assert_eq!(shape.outline_join, LineJoin::Miter);
}

/// ECMA-376 Part 1 §17.18.2: the dashed border styles.
#[test]
fn dashed_border_styles() {
    let border = |val: &str| {
        let body = format!(
            r#"<w:p><w:pPr><w:pBdr><w:bottom w:val="{val}" w:sz="4" w:space="1" w:color="000000"/></w:pBdr></w:pPr></w:p>"#
        );
        let doc = read(&Docx::new(&body).build()).unwrap();
        paragraphs(&doc)[0]
            .properties
            .borders
            .unwrap()
            .bottom
            .unwrap()
            .style
    };
    assert_eq!(border("dotted"), BorderStyle::Dotted);
    assert_eq!(border("dashed"), BorderStyle::Dashed);
    assert_eq!(border("dotDash"), BorderStyle::DotDash);
    assert_eq!(border("dotDotDash"), BorderStyle::DotDotDash);
    assert_eq!(border("dashSmallGap"), BorderStyle::DashSmallGap);
    assert_eq!(border("dashDotStroked"), BorderStyle::Other);
}

/// ECMA-376 Part 1 §20.1.2.2.37, §20.1.4.2.10, §20.1.4.2.19: a shape whose
/// `wps:spPr` states no fill or line takes them from its `wps:style`
/// references, theme slots resolved through the color scheme
/// (§20.1.6.2) with `shade` and `lumMod` applied (§20.1.2.3.31,
/// §20.1.2.3.20).
#[test]
fn text_box_colors_come_from_the_theme() {
    let theme = r#"<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:themeElements><a:clrScheme name="Office"><a:dk1><a:sysClr val="windowText" lastClr="000000"/></a:dk1><a:lt1><a:sysClr val="window" lastClr="FFFFFF"/></a:lt1><a:accent1><a:srgbClr val="4472C4"/></a:accent1></a:clrScheme></a:themeElements></a:theme>"#;
    let body = r#"<w:p><w:r><w:drawing><wp:anchor><wp:extent cx="1270000" cy="635000"/><a:graphic><a:graphicData><wps:wsp xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:spPr><a:prstGeom prst="rect"/></wps:spPr><wps:style><a:lnRef idx="2"><a:schemeClr val="accent1"><a:shade val="50000"/></a:schemeClr></a:lnRef><a:fillRef idx="1"><a:schemeClr val="accent1"><a:lumMod val="50000"/></a:schemeClr></a:fillRef></wps:style><wps:txbx><w:txbxContent><w:p><w:r><w:t>themed</w:t></w:r></w:p></w:txbxContent></wps:txbx><wps:bodyPr/></wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r></w:p>"#;
    let package = Docx::new(body)
        .part(
            "rIdTheme",
            "theme",
            "theme/theme1.xml",
            "application/vnd.openxmlformats-officedocument.theme+xml",
            theme.as_bytes(),
        )
        .build();
    let shape = first_drawing(&read(&package).unwrap()).shape;
    assert_eq!(shape.outline, Some(docboss_model::Color(0x22, 0x39, 0x62)));
    assert_eq!(shape.outline_width, Some(12_700));
    let fill = shape.fill.expect("fill from fillRef");
    assert!(
        fill.0 < 0x44 && fill.2 < 0xC4 && fill.2 > fill.0,
        "{fill:?}"
    );
}

fn shape_drawing(sp_pr: &str) -> docboss_model::Drawing {
    let body = format!(
        r#"<w:p><w:r><w:drawing><wp:anchor><wp:extent cx="1270000" cy="635000"/><a:graphic><a:graphicData><wps:wsp xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:spPr>{sp_pr}</wps:spPr><wps:bodyPr/></wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r></w:p>"#
    );
    first_drawing(&read(&Docx::new(&body).build()).unwrap())
}

/// A shape's preset geometry with its adjust values, its rotation and
/// flips, and the ends of its line.
/// ECMA-376 Part 1 §20.1.9.18, §20.1.9.5, §20.1.7.6, §20.1.8.38, §20.1.8.57.
/// ECMA-376 Part 1 §20.1.10.33, §20.1.10.34, §20.1.10.32.
#[test]
fn shape_preset_geometry_transform_and_line_ends() {
    let drawing = shape_drawing(
        r#"<a:xfrm rot="5400000" flipV="1"><a:off x="0" y="0"/><a:ext cx="1270000" cy="635000"/></a:xfrm><a:prstGeom prst="rightArrow"><a:avLst><a:gd name="adj1" fmla="val 25000"/></a:avLst></a:prstGeom><a:ln w="12700"><a:solidFill><a:srgbClr val="000000"/></a:solidFill><a:headEnd type="none"/><a:tailEnd type="stealth" w="lg" len="sm"/></a:ln>"#,
    );
    assert_eq!(
        drawing.geometry.as_deref(),
        Some(&docboss_model::Geometry::Preset {
            name: "rightArrow".into(),
            adjust: vec![("adj1".into(), 25_000)],
        })
    );
    assert_eq!(drawing.shape.rotation, 5_400_000);
    assert!(drawing.shape.flip_vertical && !drawing.shape.flip_horizontal);
    assert_eq!(drawing.shape.head_end, None);
    assert_eq!(
        drawing.shape.tail_end,
        Some(docboss_model::LineEnd {
            kind: docboss_model::LineEndKind::Stealth,
            width: docboss_model::LineEndSize::Large,
            length: docboss_model::LineEndSize::Small,
        })
    );
    let plain = shape_drawing(r#"<a:solidFill><a:srgbClr val="FF0000"/></a:solidFill>"#);
    assert_eq!(
        plain.geometry.as_deref(),
        Some(&docboss_model::Geometry::rectangle())
    );
}

/// A custom geometry's guides and paths.
/// ECMA-376 Part 1 §20.1.9.8, §20.1.9.12, §20.1.9.11, §20.1.9.16, §20.1.9.15, §20.1.9.20.
/// ECMA-376 Part 1 §20.1.9.14, §20.1.9.13, §20.1.9.4, §20.1.9.21, §20.1.9.7, §20.1.9.6.
#[test]
fn shape_custom_geometry() {
    let drawing = shape_drawing(
        r#"<a:custGeom><a:avLst/><a:gdLst><a:gd name="half" fmla="*/ w 1 2"/></a:gdLst><a:pathLst><a:path w="100" h="50" fill="none" stroke="0"><a:moveTo><a:pt x="0" y="0"/></a:moveTo><a:lnTo><a:pt x="half" y="50"/></a:lnTo><a:arcTo wR="10" hR="20" stAng="0" swAng="5400000"/><a:quadBezTo><a:pt x="1" y="2"/><a:pt x="3" y="4"/></a:quadBezTo><a:cubicBezTo><a:pt x="1" y="2"/><a:pt x="3" y="4"/><a:pt x="5" y="6"/></a:cubicBezTo><a:close/></a:path></a:pathLst></a:custGeom>"#,
    );
    let Some(docboss_model::Geometry::Custom(custom)) = drawing.geometry.as_deref() else {
        panic!("{:?}", drawing.geometry)
    };
    assert_eq!(custom.guides[0].formula, "*/ w 1 2");
    let path = &custom.paths[0];
    assert_eq!((path.width, path.height), (100, 50));
    assert_eq!(path.fill, docboss_model::PathFill::None);
    assert!(!path.stroke);
    use docboss_model::PathCommand::*;
    let s = |v: &str| v.to_string();
    assert_eq!(
        path.commands,
        vec![
            MoveTo([s("0"), s("0")]),
            LineTo([s("half"), s("50")]),
            ArcTo([s("10"), s("20"), s("0"), s("5400000")]),
            QuadTo([s("1"), s("2"), s("3"), s("4")]),
            CubicTo([s("1"), s("2"), s("3"), s("4"), s("5"), s("6")]),
            Close,
        ]
    );
}

const GROUP_NS: &str = r#"xmlns:wpg="http://schemas.microsoft.com/office/word/2010/wordprocessingGroup" xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape" xmlns:wpc="http://schemas.microsoft.com/office/word/2010/wordprocessingCanvas" xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture""#;

fn group_drawing(graphic: &str) -> docboss_model::Drawing {
    let body = format!(
        r#"<w:p><w:r><w:drawing><wp:anchor><wp:extent cx="2000000" cy="1000000"/><a:graphic {GROUP_NS}><a:graphicData>{graphic}</a:graphicData></a:graphic></wp:anchor></w:drawing></w:r></w:p>"#
    );
    first_drawing(&read(&Docx::new(&body).build()).unwrap())
}

/// A group's shapes, text boxes and nested groups become its members, their
/// `a:xfrm` boxes mapped from the child offset and extent to the group's
/// extent and through the nested group's own frame.
/// ECMA-376 Part 1 §20.4.2.39, §20.4.2.32, §20.4.2.33, §20.1.7.5, §20.1.7.2, §20.1.7.1.
#[test]
fn group_members_are_placed_through_their_groups() {
    let drawing = group_drawing(
        r#"<wpg:wgp><wpg:grpSpPr><a:xfrm><a:off x="500" y="500"/><a:ext cx="2000000" cy="1000000"/><a:chOff x="0" y="0"/><a:chExt cx="1000000" cy="500000"/></a:xfrm></wpg:grpSpPr><wps:wsp><wps:spPr><a:xfrm rot="5400000"><a:off x="100000" y="50000"/><a:ext cx="200000" cy="100000"/></a:xfrm><a:prstGeom prst="ellipse"/><a:solidFill><a:srgbClr val="FF0000"/></a:solidFill></wps:spPr><wps:txbx><w:txbxContent><w:p><w:r><w:t>Inside</w:t></w:r></w:p></w:txbxContent></wps:txbx><wps:bodyPr/></wps:wsp><wpg:grpSp><wpg:grpSpPr><a:xfrm flipH="1"><a:off x="500000" y="0"/><a:ext cx="500000" cy="500000"/><a:chOff x="0" y="0"/><a:chExt cx="100" cy="100"/></a:xfrm></wpg:grpSpPr><wps:wsp><wps:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="50" cy="50"/></a:xfrm></wps:spPr><wps:bodyPr/></wps:wsp></wpg:grpSp></wpg:wgp>"#,
    );
    assert_eq!(drawing.geometry, None);
    assert_eq!(drawing.members.len(), 2);
    let ellipse = &drawing.members[0];
    assert_eq!((ellipse.x, ellipse.y), (200_000, 100_000));
    assert_eq!(
        (ellipse.drawing.width, ellipse.drawing.height),
        (400_000, 200_000)
    );
    assert_eq!(ellipse.drawing.shape.rotation, 5_400_000);
    assert_eq!(ellipse.drawing.shape.fill, Some(Color(255, 0, 0)));
    assert!(matches!(
        ellipse.drawing.geometry.as_deref(),
        Some(docboss_model::Geometry::Preset { name, .. }) if name == "ellipse"
    ));
    let Block::Paragraph(text) = &ellipse.drawing.text_box[0] else {
        panic!()
    };
    assert_eq!(text.text(), "Inside");
    let nested = &drawing.members[1];
    assert_eq!((nested.x, nested.y), (1_500_000, 0));
    assert_eq!(
        (nested.drawing.width, nested.drawing.height),
        (500_000, 500_000)
    );
    assert!(nested.drawing.shape.flip_horizontal);
}

/// A drawing canvas places its shapes and pictures in its own EMU; an
/// empty canvas keeps its extent and draws nothing.
/// ECMA-376 Part 1 §20.4.2.41.
#[test]
fn canvas_members_keep_their_offsets() {
    let drawing = group_drawing(
        r#"<wpc:wpc><wpc:bg/><wps:wsp><wps:spPr><a:xfrm><a:off x="10" y="20"/><a:ext cx="30" cy="40"/></a:xfrm><a:prstGeom prst="rect"/></wps:spPr><wps:bodyPr/></wps:wsp><pic:pic><pic:blipFill><a:blip/></pic:blipFill><pic:spPr><a:xfrm flipV="1"><a:off x="100" y="200"/><a:ext cx="300" cy="400"/></a:xfrm></pic:spPr></pic:pic></wpc:wpc>"#,
    );
    assert_eq!(drawing.members.len(), 2);
    assert_eq!((drawing.members[0].x, drawing.members[0].y), (10, 20));
    let picture = &drawing.members[1].drawing;
    assert_eq!((picture.width, picture.height), (300, 400));
    assert!(picture.geometry.is_none() && picture.shape.flip_vertical);
    let empty = group_drawing(r#"<wpc:wpc><wpc:bg/><wpc:whole/></wpc:wpc>"#);
    assert!(empty.members.is_empty());
    assert_eq!(empty.shape.fill, None);
    assert_eq!(empty.shape.outline, None);
    assert!(empty.geometry.is_some());
}

/// A gradient fill's stops, linear angle and path focus.
/// ECMA-376 Part 1 §20.1.8.33, §20.1.8.37, §20.1.8.36, §20.1.8.41, §20.1.8.46, §20.1.8.31.
#[test]
fn shape_gradient_fill() {
    let linear = shape_drawing(
        r#"<a:gradFill><a:gsLst><a:gs pos="100000"><a:srgbClr val="0000FF"/></a:gs><a:gs pos="0"><a:srgbClr val="FF0000"/></a:gs></a:gsLst><a:lin ang="5400000" scaled="1"/></a:gradFill>"#,
    );
    let gradient = linear.shape.gradient.unwrap();
    assert_eq!(
        gradient.stops(),
        &[(0, Color(255, 0, 0)), (100_000, Color(0, 0, 255))]
    );
    assert_eq!(gradient.angle, 5_400_000);
    assert_eq!(gradient.path, None);
    assert_eq!(linear.shape.fill, Some(gradient.average()));
    let path = shape_drawing(
        r#"<a:gradFill><a:gsLst><a:gs pos="0"><a:srgbClr val="FFFFFF"/></a:gs></a:gsLst><a:path path="circle"><a:fillToRect l="50000" t="50000" r="50000" b="50000"/></a:path></a:gradFill>"#,
    );
    let gradient = path.shape.gradient.unwrap();
    assert_eq!(gradient.path, Some(docboss_model::GradientPath::Circle));
    assert_eq!(gradient.focus, [50_000; 4]);
    let solid = shape_drawing(r#"<a:solidFill><a:srgbClr val="FF0000"/></a:solidFill>"#);
    assert_eq!(solid.shape.gradient, None);
}

/// VML groups place their children through `coordorigin` and `coordsize`,
/// nested groups through their own; a `v:fill` gradient runs from the
/// fill color to `color2`, and `v:stroke` arrowheads become line ends.
/// ECMA-376 Part 1 §17.3.3.19; [MS-ODRAW] §2.3.7.1, §2.3.7.14, §2.3.7.15, §2.4.16.
#[test]
fn vml_group_members_and_fills() {
    let body = r#"<w:p><w:r><w:pict xmlns:v="urn:schemas-microsoft-com:vml"><v:group style="position:absolute;margin-left:0;margin-top:0;width:100pt;height:50pt" coordorigin="100,100" coordsize="200,100"><v:rect style="position:absolute;left:100;top:150;width:100;height:50" fillcolor="red"><v:fill type="gradient" color2="blue" angle="90"/></v:rect><v:group style="position:absolute;left:200;top:100;width:100;height:100" coordsize="10,10"><v:line from="0,0" to="10,10"><v:stroke endarrow="block" endarrowwidth="wide"/></v:line></v:group></v:group></w:pict></w:r></w:p>"#;
    let drawing = first_drawing(&read(&Docx::new(body).build()).unwrap());
    assert_eq!((drawing.width, drawing.height), (1_270_000, 635_000));
    assert_eq!(drawing.members.len(), 2);
    let rect = &drawing.members[0];
    assert_eq!((rect.x, rect.y), (0, 317_500));
    assert_eq!(
        (rect.drawing.width, rect.drawing.height),
        (635_000, 317_500)
    );
    let gradient = rect.drawing.shape.gradient.unwrap();
    assert_eq!(gradient.angle, 10_800_000);
    assert_eq!(gradient.color_at(0.0), Color(0, 0, 255));
    assert_eq!(gradient.color_at(1.0), Color(255, 0, 0));
    let line = &drawing.members[1];
    assert_eq!((line.x, line.y), (635_000, 0));
    assert_eq!(
        line.drawing.shape.tail_end.unwrap().width,
        docboss_model::LineEndSize::Large
    );
}

/// VML shape elements take their preset geometry: an oval, a rounded
/// rectangle's arcsize, a line between its end points and a shape type,
/// with the style's flip and rotation.
/// [MS-ODRAW] §2.4.24.
#[test]
fn vml_shape_geometry() {
    let vml = |element: &str| {
        let body = format!(
            r#"<w:p><w:r><w:pict xmlns:v="urn:schemas-microsoft-com:vml" xmlns:o="urn:schemas-microsoft-com:office:office">{element}</w:pict></w:r></w:p>"#
        );
        first_drawing(&read(&Docx::new(&body).build()).unwrap())
    };
    let preset = |name: &str, adjust: Vec<(String, i64)>| {
        Some(Box::new(docboss_model::Geometry::Preset {
            name: name.into(),
            adjust,
        }))
    };
    let oval =
        vml(r#"<v:oval style="position:absolute;width:10pt;height:10pt;rotation:90;flip:x"/>"#);
    assert_eq!(oval.geometry, preset("ellipse", vec![]));
    assert_eq!(oval.shape.rotation, 5_400_000);
    assert!(oval.shape.flip_horizontal);
    let round = vml(r#"<v:roundrect style="width:10pt;height:10pt" arcsize="10923f"/>"#);
    assert_eq!(
        round.geometry,
        preset("roundRect", vec![("adj".into(), 8_334)])
    );
    let line = vml(r#"<v:line style="position:absolute" from="10pt,40pt" to="30pt,20pt"/>"#);
    assert_eq!(line.geometry, preset("line", vec![]));
    assert_eq!((line.width, line.height), (254_000, 254_000));
    assert!(line.shape.flip_vertical && !line.shape.flip_horizontal);
    let DrawingPlacement::Anchored {
        horizontal,
        vertical,
        ..
    } = line.placement
    else {
        panic!("{:?}", line.placement)
    };
    assert_eq!((horizontal.offset, vertical.offset), (127_000, 254_000));
    let arrow = vml(r##"<v:shape type="#_x0000_t13" style="width:10pt;height:10pt"/>"##);
    assert_eq!(arrow.geometry, preset("rightArrow", vec![]));
    let custom = r#"<w:p><w:r><w:pict xmlns:v="urn:schemas-microsoft-com:vml"><v:shape style="width:10pt;height:10pt" path="m,l10,10e"/></w:pict></w:r></w:p>"#;
    let doc = read(&Docx::new(custom).build()).unwrap();
    assert!(!format!("{:?}", paragraphs(&doc)[0].inlines).contains("Drawing"));
}

/// Vertical and upright text in a shape's `wps:bodyPr` and in a VML text
/// box's `layout-flow`, and a cell's `w:textDirection`.
/// ECMA-376 Part 1 §20.4.2.22, §20.1.10.83, §17.4.72, §17.18.93.
#[test]
fn vertical_text_directions() {
    use docboss_model::TextDirection::*;
    let shape = |body_pr: &str| {
        let body = format!(
            r#"<w:p><w:r><w:drawing><wp:anchor><wp:extent cx="1270000" cy="635000"/><a:graphic><a:graphicData><wps:wsp xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:spPr/><wps:txbx><w:txbxContent><w:p/></w:txbxContent></wps:txbx>{body_pr}</wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r></w:p>"#
        );
        first_drawing(&read(&Docx::new(&body).build()).unwrap()).shape
    };
    assert_eq!(
        shape(r#"<wps:bodyPr vert="vert"/>"#).text_direction,
        TopToBottom
    );
    assert_eq!(
        shape(r#"<wps:bodyPr vert="eaVert"/>"#).text_direction,
        TopToBottom
    );
    assert_eq!(
        shape(r#"<wps:bodyPr vert="vert270"/>"#).text_direction,
        BottomToTop
    );
    let upright = shape(r#"<wps:bodyPr vert="horz" upright="1"/>"#);
    assert_eq!(upright.text_direction, LeftToRight);
    assert!(upright.text_upright);
    let vml = r#"<w:p><w:r><w:pict xmlns:v="urn:schemas-microsoft-com:vml"><v:rect style="width:10pt;height:10pt"><v:textbox style="layout-flow:vertical;mso-layout-flow-alt:bottom-to-top"><w:txbxContent><w:p/></w:txbxContent></v:textbox></v:rect></w:pict></w:r></w:p>"#;
    let drawing = first_drawing(&read(&Docx::new(vml).build()).unwrap());
    assert_eq!(drawing.shape.text_direction, BottomToTop);
    let table = r#"<w:tbl><w:tr><w:tc><w:tcPr><w:textDirection w:val="btLr"/></w:tcPr><w:p/></w:tc><w:tc><w:tcPr><w:textDirection w:val="rl"/></w:tcPr><w:p/></w:tc><w:tc><w:p/></w:tc></w:tr></w:tbl>"#;
    let doc = read(&Docx::new(table).build()).unwrap();
    let Some(Block::Table(table)) = doc.blocks().next() else {
        panic!()
    };
    let directions: Vec<_> = table.rows[0]
        .cells
        .iter()
        .map(|c| c.properties.text_direction)
        .collect();
    assert_eq!(directions, [BottomToTop, TopToBottom, LeftToRight]);
}

/// A VML shape's `adj` values carry over to the preset where the guides
/// take the same fraction of the shape.
/// [MS-ODRAW] §2.3.6.10.
#[test]
fn vml_adjust_values() {
    let body = r##"<w:p><w:r><w:pict xmlns:v="urn:schemas-microsoft-com:vml"><v:shape type="#_x0000_t5" adj="5400" style="width:10pt;height:10pt"/></w:pict></w:r></w:p>"##;
    let drawing = first_drawing(&read(&Docx::new(body).build()).unwrap());
    assert_eq!(
        drawing.geometry.as_deref(),
        Some(&docboss_model::Geometry::Preset {
            name: "triangle".into(),
            adjust: vec![("adj".into(), 25_000)],
        })
    );
}

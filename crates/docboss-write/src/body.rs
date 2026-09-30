//! Story content: blocks, paragraphs, runs, tables and drawings
//! (ECMA-376 Part 1 §17.3 paragraphs and runs, §17.4 tables, §20.4
//! DrawingML placement in WordprocessingML).

use std::collections::BTreeMap;

use docboss_model::{
    Block, Break, Document, Drawing, DrawingPlacement, DrawingPosition, Field, Hyperlink, Inline,
    Paragraph, PositionAlign, PositionBase, Revision, RevisionKind, Run, RunContent, Table,
    TableCell,
};

use crate::package::{office_rel, Rels, NS_A, NS_PIC};
use crate::props;
use crate::xml::Xml;

/// Which story a part holds; it decides how a note-number run is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoryKind {
    Main,
    HeaderFooter,
    Footnote,
    Endnote,
    Comment,
}

/// State shared by every part of one package.
pub struct Shared<'a> {
    pub document: &'a Document,
    /// Media index to its target relative to `word/`, such as `media/image1.png`.
    pub media_targets: Vec<String>,
    pub footnote_ids: BTreeMap<i64, i64>,
    pub endnote_ids: BTreeMap<i64, i64>,
    pub drawing_id: u32,
    pub revision_id: i64,
}

/// Writes one story into `xml`, adding relationships to `rels`.
pub struct Story<'s, 'a> {
    pub shared: &'s mut Shared<'a>,
    pub rels: &'s mut Rels,
    pub xml: Xml,
    pub kind: StoryKind,
    in_link: bool,
    deleted: bool,
}

impl<'s, 'a> Story<'s, 'a> {
    pub fn new(shared: &'s mut Shared<'a>, rels: &'s mut Rels, kind: StoryKind) -> Self {
        Self {
            shared,
            rels,
            xml: Xml::new(),
            kind,
            in_link: false,
            deleted: false,
        }
    }

    pub fn finish(self) -> Vec<u8> {
        self.xml.finish()
    }

    /// Writes blocks; a story that must end in a paragraph gets an empty
    /// one appended when it does not.
    pub fn blocks(&mut self, blocks: &[Block], need_paragraph: bool) {
        for block in blocks {
            match block {
                Block::Paragraph(paragraph) => self.paragraph(paragraph, None),
                Block::Table(table) => self.table(table),
            }
        }
        let ends_in_paragraph = matches!(blocks.last(), Some(Block::Paragraph(_)));
        if need_paragraph && !ends_in_paragraph {
            self.xml.empty("w:p", &[]);
        }
    }

    /// Writes a paragraph; `section` is a rendered `<w:sectPr>` ending a
    /// section at this paragraph.
    pub fn paragraph(&mut self, paragraph: &Paragraph, section: Option<&str>) {
        self.xml.open("w:p", &[]);
        let mark = Some(&paragraph.mark).filter(|mark| **mark != Default::default());
        props::paragraph_properties(
            &mut self.xml,
            paragraph.style_id.as_deref(),
            &paragraph.properties,
            mark,
            section,
        );
        self.inlines(&paragraph.inlines);
        self.xml.close("w:p");
    }

    fn inlines(&mut self, inlines: &[Inline]) {
        inlines.iter().for_each(|inline| self.inline(inline));
    }

    fn inline(&mut self, inline: &Inline) {
        match inline {
            Inline::Run(run) => self.run(run),
            Inline::Hyperlink(link) => self.hyperlink(link),
            Inline::Field(field) => self.field(field),
            Inline::Revision(revision) => self.revision(revision),
            Inline::BookmarkStart { id, name } => self.xml.empty(
                "w:bookmarkStart",
                &[("w:id", &id.to_string()), ("w:name", name)],
            ),
            Inline::BookmarkEnd { id } => self
                .xml
                .empty("w:bookmarkEnd", &[("w:id", &id.to_string())]),
            Inline::CommentRangeStart { id } => self
                .xml
                .empty("w:commentRangeStart", &[("w:id", &id.to_string())]),
            Inline::CommentRangeEnd { id } => self
                .xml
                .empty("w:commentRangeEnd", &[("w:id", &id.to_string())]),
        }
    }

    /// ECMA-376 Part 1 §17.16.22: a hyperlink targets an external
    /// relationship, a bookmark anchor, or both. Nested hyperlinks are
    /// flattened into the outer one.
    fn hyperlink(&mut self, link: &Hyperlink) {
        if self.in_link {
            self.inlines(&link.inlines);
            return;
        }
        let rid = link
            .target
            .as_deref()
            .map(|target| self.rels.add(&office_rel("hyperlink"), target, true));
        let mut attributes: Vec<(&str, &str)> = Vec::new();
        if let Some(rid) = rid.as_deref() {
            attributes.push(("r:id", rid));
        }
        if let Some(anchor) = link.anchor.as_deref() {
            attributes.push(("w:anchor", anchor));
        }
        if let Some(tooltip) = link.tooltip.as_deref() {
            attributes.push(("w:tooltip", tooltip));
        }
        attributes.push(("w:history", "1"));
        self.xml.open("w:hyperlink", &attributes);
        self.in_link = true;
        self.inlines(&link.inlines);
        self.in_link = false;
        self.xml.close("w:hyperlink");
    }

    fn field_char(&mut self, kind: &str) {
        self.xml.open("w:r", &[]);
        self.xml.empty("w:fldChar", &[("w:fldCharType", kind)]);
        self.xml.close("w:r");
    }

    /// ECMA-376 Part 1 §17.16.18: a complex field is a begin character,
    /// the instruction, a separator, the cached result and an end character.
    fn field(&mut self, field: &Field) {
        self.field_char("begin");
        let name = if self.deleted {
            "w:delInstrText"
        } else {
            "w:instrText"
        };
        self.xml.open("w:r", &[]);
        self.xml
            .text_element(name, &[("xml:space", "preserve")], &field.instruction);
        self.xml.close("w:r");
        self.field_char("separate");
        self.inlines(&field.result);
        self.field_char("end");
    }

    /// ECMA-376 Part 1 §17.13.5: tracked insertions and deletions wrap the
    /// runs they changed; deleted text is written as `w:delText`.
    fn revision(&mut self, revision: &Revision) {
        if self.deleted || self.in_link {
            self.inlines(&revision.inlines);
            return;
        }
        let name = match revision.kind {
            RevisionKind::Insertion => "w:ins",
            RevisionKind::Deletion => "w:del",
        };
        let id = self.shared.revision_id.to_string();
        self.shared.revision_id += 1;
        let author = revision.author.as_deref().unwrap_or("");
        let mut attributes = vec![("w:id", id.as_str()), ("w:author", author)];
        if let Some(date) = revision.date.as_deref() {
            attributes.push(("w:date", date));
        }
        self.xml.open(name, &attributes);
        self.deleted = revision.kind == RevisionKind::Deletion;
        self.inlines(&revision.inlines);
        self.deleted = false;
        self.xml.close(name);
    }

    fn run(&mut self, run: &Run) {
        self.xml.open("w:r", &[]);
        props::run_properties(&mut self.xml, &run.properties, true);
        run.content.iter().for_each(|content| self.content(content));
        self.xml.close("w:r");
    }

    fn text(&mut self, text: &str) {
        let name = if self.deleted { "w:delText" } else { "w:t" };
        let mut pieces = text.split(['\t', '\n']).peekable();
        let mut separators = text.chars().filter(|c| *c == '\t' || *c == '\n');
        while let Some(piece) = pieces.next() {
            if !piece.is_empty() {
                let preserve =
                    piece.starts_with(char::is_whitespace) || piece.ends_with(char::is_whitespace);
                let attributes: &[(&str, &str)] = if preserve {
                    &[("xml:space", "preserve")]
                } else {
                    &[]
                };
                self.xml.text_element(name, attributes, piece);
            }
            if pieces.peek().is_none() {
                continue;
            }
            match separators.next() {
                Some('\t') => self.xml.empty("w:tab", &[]),
                _ => self.xml.empty("w:br", &[]),
            }
        }
    }

    fn content(&mut self, content: &RunContent) {
        match content {
            RunContent::Text(text) => self.text(text),
            RunContent::Tab => self.xml.empty("w:tab", &[]),
            RunContent::Break(Break::Line) => self.xml.empty("w:br", &[]),
            RunContent::Break(Break::Page) => self.xml.empty("w:br", &[("w:type", "page")]),
            RunContent::Break(Break::Column) => self.xml.empty("w:br", &[("w:type", "column")]),
            RunContent::CarriageReturn => self.xml.empty("w:cr", &[]),
            RunContent::NoBreakHyphen => self.xml.empty("w:noBreakHyphen", &[]),
            RunContent::SoftHyphen => self.xml.empty("w:softHyphen", &[]),
            RunContent::Symbol { font, char } => {
                let code = format!("{char:04X}");
                let mut attributes = Vec::new();
                if let Some(font) = font.as_deref() {
                    attributes.push(("w:font", font));
                }
                attributes.push(("w:char", code.as_str()));
                self.xml.empty("w:sym", &attributes);
            }
            RunContent::FootnoteReference(id) => {
                if let Some(id) = self.shared.footnote_ids.get(id) {
                    self.xml
                        .empty("w:footnoteReference", &[("w:id", &id.to_string())]);
                }
            }
            RunContent::EndnoteReference(id) => {
                if let Some(id) = self.shared.endnote_ids.get(id) {
                    self.xml
                        .empty("w:endnoteReference", &[("w:id", &id.to_string())]);
                }
            }
            RunContent::CommentReference(id) => {
                if self.shared.document.comment(*id).is_some() {
                    self.xml
                        .empty("w:commentReference", &[("w:id", &id.to_string())]);
                }
            }
            RunContent::NoteNumber => match self.kind {
                StoryKind::Footnote => self.xml.empty("w:footnoteRef", &[]),
                StoryKind::Endnote => self.xml.empty("w:endnoteRef", &[]),
                _ => {}
            },
            RunContent::Drawing(drawing) => self.drawing(drawing),
            RunContent::Math(math) => self.text(&docboss_model::linear_text(&math.nodes)),
        }
    }

    /// ECMA-376 Part 1 §20.4.2.8 (inline) and §20.4.2.3 (anchor): a picture
    /// is a DrawingML graphic whose blip embeds the image relationship.
    fn drawing(&mut self, drawing: &Drawing) {
        let Some(target) = drawing
            .media
            .and_then(|id| self.shared.media_targets.get(id.0 as usize))
        else {
            return;
        };
        let rid = self.rels.add(&office_rel("image"), target, false);
        self.shared.drawing_id += 1;
        let id = self.shared.drawing_id.to_string();
        let default_name = format!("Picture {id}");
        let name = drawing.name.as_deref().unwrap_or(&default_name);
        let cx = drawing.width.max(0).to_string();
        let cy = drawing.height.max(0).to_string();
        let xml = &mut self.xml;
        xml.open("w:drawing", &[]);
        let distances = [
            ("distT", "0"),
            ("distB", "0"),
            ("distL", "0"),
            ("distR", "0"),
        ];
        match drawing.placement {
            DrawingPlacement::Inline => {
                xml.open("wp:inline", &distances);
                xml.empty("wp:extent", &[("cx", &cx), ("cy", &cy)]);
            }
            DrawingPlacement::Anchored {
                horizontal,
                vertical,
                behind_text,
            } => {
                let behind = if behind_text { "1" } else { "0" };
                let height = id.as_str();
                xml.open(
                    "wp:anchor",
                    &[
                        ("distT", "0"),
                        ("distB", "0"),
                        ("distL", "114300"),
                        ("distR", "114300"),
                        ("simplePos", "0"),
                        ("relativeHeight", height),
                        ("behindDoc", behind),
                        ("locked", "0"),
                        ("layoutInCell", "1"),
                        ("allowOverlap", "1"),
                    ],
                );
                xml.empty("wp:simplePos", &[("x", "0"), ("y", "0")]);
                position(xml, "wp:positionH", horizontal, true);
                position(xml, "wp:positionV", vertical, false);
                xml.empty("wp:extent", &[("cx", &cx), ("cy", &cy)]);
            }
        }
        xml.empty(
            "wp:effectExtent",
            &[("l", "0"), ("t", "0"), ("r", "0"), ("b", "0")],
        );
        if let DrawingPlacement::Anchored { behind_text, .. } = drawing.placement {
            if behind_text {
                xml.empty("wp:wrapNone", &[]);
            } else {
                xml.empty("wp:wrapSquare", &[("wrapText", "bothSides")]);
            }
        }
        let mut doc_pr = vec![("id", id.as_str()), ("name", name)];
        if let Some(description) = drawing.description.as_deref() {
            doc_pr.push(("descr", description));
        }
        xml.empty("wp:docPr", &doc_pr);
        xml.open("wp:cNvGraphicFramePr", &[]);
        xml.empty(
            "a:graphicFrameLocks",
            &[("xmlns:a", NS_A), ("noChangeAspect", "1")],
        );
        xml.close("wp:cNvGraphicFramePr");
        xml.open("a:graphic", &[("xmlns:a", NS_A)]);
        xml.open("a:graphicData", &[("uri", NS_PIC)]);
        xml.open("pic:pic", &[("xmlns:pic", NS_PIC)]);
        xml.open("pic:nvPicPr", &[]);
        xml.empty("pic:cNvPr", &[("id", "0"), ("name", name)]);
        xml.empty("pic:cNvPicPr", &[]);
        xml.close("pic:nvPicPr");
        xml.open("pic:blipFill", &[]);
        xml.empty("a:blip", &[("r:embed", &rid)]);
        xml.open("a:stretch", &[]);
        xml.empty("a:fillRect", &[]);
        xml.close("a:stretch");
        xml.close("pic:blipFill");
        xml.open("pic:spPr", &[]);
        xml.open("a:xfrm", &[]);
        xml.empty("a:off", &[("x", "0"), ("y", "0")]);
        xml.empty("a:ext", &[("cx", &cx), ("cy", &cy)]);
        xml.close("a:xfrm");
        xml.open("a:prstGeom", &[("prst", "rect")]);
        xml.empty("a:avLst", &[]);
        xml.close("a:prstGeom");
        xml.close("pic:spPr");
        xml.close("pic:pic");
        xml.close("a:graphicData");
        xml.close("a:graphic");
        match drawing.placement {
            DrawingPlacement::Inline => xml.close("wp:inline"),
            DrawingPlacement::Anchored { .. } => xml.close("wp:anchor"),
        }
        xml.close("w:drawing");
    }

    /// ECMA-376 Part 1 §17.4.38: a table is its properties, its grid and
    /// its rows; every cell ends in a paragraph.
    fn table(&mut self, table: &Table) {
        if table.rows.is_empty() {
            return;
        }
        self.xml.open("w:tbl", &[]);
        props::table_properties(&mut self.xml, &table.properties);
        self.xml.open("w:tblGrid", &[]);
        for width in grid(table) {
            self.xml.empty("w:gridCol", &[("w:w", &width.to_string())]);
        }
        self.xml.close("w:tblGrid");
        for row in &table.rows {
            self.xml.open("w:tr", &[]);
            props::row_properties(&mut self.xml, &row.properties);
            if row.cells.is_empty() {
                self.cell(&TableCell::default());
            }
            row.cells.iter().for_each(|cell| self.cell(cell));
            self.xml.close("w:tr");
        }
        self.xml.close("w:tbl");
    }

    fn cell(&mut self, cell: &TableCell) {
        self.xml.open("w:tc", &[]);
        props::cell_properties(&mut self.xml, &cell.properties);
        self.blocks(&cell.blocks, true);
        self.xml.close("w:tc");
    }
}

/// A floating drawing's `wp:positionH` or `wp:positionV`: its
/// `relativeFrom` and a `wp:align` or `wp:posOffset` (ECMA-376 Part 1
/// §20.4.2.10, §20.4.2.11, §20.4.2.1, §20.4.2.2). A base the axis does not
/// allow is written as the axis's default, column or paragraph.
fn position(xml: &mut Xml, element: &str, position: DrawingPosition, horizontal: bool) {
    let from = match (position.base, horizontal) {
        (PositionBase::Page, _) => "page",
        (PositionBase::Margin, _) => "margin",
        (PositionBase::InsideMargin, _) => "insideMargin",
        (PositionBase::OutsideMargin, _) => "outsideMargin",
        (PositionBase::Character, true) => "character",
        (PositionBase::LeftMargin, true) => "leftMargin",
        (PositionBase::RightMargin, true) => "rightMargin",
        (PositionBase::Line, false) => "line",
        (PositionBase::TopMargin, false) => "topMargin",
        (PositionBase::BottomMargin, false) => "bottomMargin",
        (_, true) => "column",
        (_, false) => "paragraph",
    };
    xml.open(element, &[("relativeFrom", from)]);
    let Some(align) = position.align else {
        xml.text_element("wp:posOffset", &[], &position.offset.to_string());
        xml.close(element);
        return;
    };
    let value = match (align, horizontal) {
        (PositionAlign::Start, true) => "left",
        (PositionAlign::Start, false) => "top",
        (PositionAlign::Center, _) => "center",
        (PositionAlign::End, true) => "right",
        (PositionAlign::End, false) => "bottom",
        (PositionAlign::Inside, _) => "inside",
        (PositionAlign::Outside, _) => "outside",
    };
    xml.text_element("wp:align", &[], value);
    xml.close(element);
}

/// The grid a table is written with: its own, or equal columns spanning
/// 9360 twips (6.5 inches) when the model carries none.
fn grid(table: &Table) -> Vec<i32> {
    if !table.grid.is_empty() {
        return table.grid.clone();
    }
    let columns = table
        .rows
        .iter()
        .map(|row| row.cells.iter().map(TableCell::span).sum::<u32>())
        .max()
        .unwrap_or(1)
        .max(1);
    vec![9360 / columns as i32; columns as usize]
}

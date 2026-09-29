//! A composition API over the model: a [`DocumentBuilder`] collecting
//! blocks, [`Para`] composing a paragraph run by run, and [`TableBuilder`].

use std::path::Path;
use std::sync::Arc;

use docboss_model::{
    AbstractNumbering, Block, Border, BorderStyle, Borders, Break, Color, Document, Drawing,
    DrawingPlacement, FontEntry, HeaderFooter, HeaderFooterKind, Hyperlink, Indentation, Inline,
    Justification, Level, LineRule, Media, MediaId, Metadata, Note, NoteKind, NumberFormat,
    NumberingInstance, NumberingRef, Orientation, PageMargins, PageSize, Paragraph,
    ParagraphProperties, Run, RunContent, RunProperties, Section, SectionProperties, Shading,
    Spacing, Style, StyleKind, Styles, Table, TableCell, TableCellProperties, TableProperties,
    TableRow, TableRowProperties, Underline, VerticalAlign,
};

use crate::Result;

/// EMU per pixel at 96 DPI.
pub const EMU_PER_PIXEL: i64 = 9525;

/// A paragraph under composition.
#[derive(Debug, Clone, Default)]
pub struct Para {
    pub paragraph: Paragraph,
}

impl Para {
    pub fn new() -> Self {
        Self::default()
    }

    /// A paragraph in the given paragraph style.
    pub fn styled(style_id: &str) -> Self {
        Self::new().style(style_id)
    }

    pub fn style(mut self, style_id: &str) -> Self {
        self.paragraph.style_id = Some(style_id.to_string());
        self
    }

    pub fn align(mut self, justification: Justification) -> Self {
        self.paragraph.properties.justification = Some(justification);
        self
    }

    pub fn properties(mut self, update: impl FnOnce(&mut ParagraphProperties)) -> Self {
        update(&mut self.paragraph.properties);
        self
    }

    /// Appends a run with explicit properties.
    pub fn run(mut self, text: &str, properties: RunProperties) -> Self {
        self.paragraph.inlines.push(text_run(text, properties));
        self
    }

    pub fn text(self, text: &str) -> Self {
        self.run(text, RunProperties::default())
    }

    pub fn bold(self, text: &str) -> Self {
        self.run(
            text,
            RunProperties {
                bold: Some(true),
                ..RunProperties::default()
            },
        )
    }

    pub fn italic(self, text: &str) -> Self {
        self.run(
            text,
            RunProperties {
                italic: Some(true),
                ..RunProperties::default()
            },
        )
    }

    pub fn underline(self, text: &str) -> Self {
        self.run(
            text,
            RunProperties {
                underline: Some(Underline::Single),
                ..RunProperties::default()
            },
        )
    }

    /// A run in a character style.
    pub fn character(self, text: &str, style_id: &str) -> Self {
        self.run(
            text,
            RunProperties {
                style_id: Some(style_id.to_string()),
                ..RunProperties::default()
            },
        )
    }

    /// An external hyperlink in the `Hyperlink` character style.
    pub fn link(mut self, url: &str, text: &str) -> Self {
        let run = text_run(
            text,
            RunProperties {
                style_id: Some("Hyperlink".into()),
                ..RunProperties::default()
            },
        );
        self.paragraph.inlines.push(Inline::Hyperlink(Hyperlink {
            target: Some(url.to_string()),
            inlines: vec![run],
            ..Hyperlink::default()
        }));
        self
    }

    pub fn content(mut self, content: RunContent, properties: RunProperties) -> Self {
        self.paragraph.inlines.push(Inline::Run(Run {
            properties,
            content: vec![content],
        }));
        self
    }

    pub fn tab(self) -> Self {
        self.content(RunContent::Tab, RunProperties::default())
    }

    pub fn line_break(self) -> Self {
        self.content(RunContent::Break(Break::Line), RunProperties::default())
    }

    pub fn page_break(self) -> Self {
        self.content(RunContent::Break(Break::Page), RunProperties::default())
    }

    /// A reference to a note added with [`DocumentBuilder::footnote`].
    pub fn footnote_ref(self, id: i64) -> Self {
        let properties = RunProperties {
            style_id: Some("FootnoteReference".into()),
            ..RunProperties::default()
        };
        self.content(RunContent::FootnoteReference(id), properties)
    }

    /// An inline picture of an image added with [`DocumentBuilder::image`],
    /// `width` and `height` in EMU.
    pub fn image(self, media: MediaId, width: i64, height: i64) -> Self {
        let drawing = Drawing {
            media: Some(media),
            width,
            height,
            placement: DrawingPlacement::Inline,
            name: None,
            description: None,
            text_box: Vec::new(),
            shape: Default::default(),
            geometry: None,
            members: Vec::new(),
        };
        self.content(
            RunContent::Drawing(Box::new(drawing)),
            RunProperties::default(),
        )
    }

    /// Places the paragraph in a list created with [`DocumentBuilder::list`].
    pub fn list(mut self, num_id: i64, level: u8) -> Self {
        self.paragraph.properties.numbering = Some(NumberingRef { num_id, level });
        if self.paragraph.style_id.is_none() {
            self.paragraph.style_id = Some("ListParagraph".into());
        }
        self
    }

    pub fn inline(mut self, inline: Inline) -> Self {
        self.paragraph.inlines.push(inline);
        self
    }

    pub fn build(self) -> Paragraph {
        self.paragraph
    }
}

impl From<Para> for Block {
    fn from(para: Para) -> Block {
        Block::Paragraph(para.paragraph)
    }
}

impl From<Para> for Paragraph {
    fn from(para: Para) -> Paragraph {
        para.paragraph
    }
}

pub(crate) fn text_run(text: &str, properties: RunProperties) -> Inline {
    Inline::Run(Run {
        properties,
        content: vec![RunContent::Text(text.to_string())],
    })
}

/// Whether a list is bulleted or numbered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListKind {
    Bullet,
    Numbered,
}

const BULLETS: [&str; 3] = ["\u{2022}", "\u{25E6}", "\u{25AA}"];

fn list_levels(kind: ListKind) -> Vec<Level> {
    (0..9u8)
        .map(|level| {
            let indent = 720 * (i32::from(level) + 1);
            let paragraph = ParagraphProperties {
                indentation: Indentation {
                    left: Some(indent),
                    hanging: Some(360),
                    ..Indentation::default()
                },
                ..ParagraphProperties::default()
            };
            let (format, text) = match kind {
                ListKind::Bullet => (
                    NumberFormat::Bullet,
                    BULLETS[usize::from(level) % 3].to_string(),
                ),
                ListKind::Numbered => {
                    let format = [
                        NumberFormat::Decimal,
                        NumberFormat::LowerLetter,
                        NumberFormat::LowerRoman,
                    ][usize::from(level) % 3]
                        .clone();
                    (format, format!("%{}.", level + 1))
                }
            };
            Level {
                level,
                format,
                text,
                justification: Some(Justification::Left),
                paragraph,
                ..Level::default()
            }
        })
        .collect()
}

fn pt(points: u32) -> Option<u32> {
    Some(points * 2)
}

fn paragraph_style(id: &str, name: &str) -> Style {
    let mut style = Style::new(id, StyleKind::Paragraph);
    style.name = Some(name.to_string());
    style.based_on = Some("Normal".into());
    style.next = Some("Normal".into());
    style
}

fn character_style(id: &str, name: &str, run: RunProperties) -> Style {
    let mut style = Style::new(id, StyleKind::Character);
    style.name = Some(name.to_string());
    style.run = run;
    style
}

const MONOSPACE: &str = "Courier New";

fn monospace() -> docboss_model::FontSlots {
    docboss_model::FontSlots {
        ascii: Some(MONOSPACE.into()),
        high_ansi: Some(MONOSPACE.into()),
        complex: Some(MONOSPACE.into()),
        east_asia: None,
    }
}

fn single_border(size: u32) -> Option<Border> {
    Some(Border {
        style: BorderStyle::Single,
        size,
        space: 0,
        color: Some(Color(0xBF, 0xBF, 0xBF)),
    })
}

/// The built-in style sheet: `Normal`, `Title`, `Heading1`..`Heading6`,
/// `Quote`, `SourceCode`, `ListParagraph`, `FootnoteText`, and the
/// `Hyperlink`, `VerbatimChar`, `FootnoteReference` character styles and
/// the `TableGrid` table style.
pub fn default_styles() -> Styles {
    let default_run = RunProperties {
        fonts: docboss_model::FontSlots {
            ascii: Some("Calibri".into()),
            high_ansi: Some("Calibri".into()),
            east_asia: Some("Calibri".into()),
            complex: Some("Calibri".into()),
        },
        size: pt(11),
        language: Some("en-US".into()),
        ..RunProperties::default()
    };
    let default_paragraph = ParagraphProperties {
        spacing: Spacing {
            after: Some(160),
            line: Some(259),
            line_rule: Some(LineRule::Auto),
            ..Spacing::default()
        },
        ..ParagraphProperties::default()
    };
    let mut normal = Style::new("Normal", StyleKind::Paragraph);
    normal.name = Some("Normal".into());
    normal.is_default = true;
    let mut styles = vec![normal];

    let mut title = paragraph_style("Title", "Title");
    title.paragraph.spacing.after = Some(80);
    title.paragraph.contextual_spacing = Some(true);
    title.run.size = pt(28);
    styles.push(title);

    let heading_sizes = [16, 13, 12, 11, 11, 11];
    for (level, size) in heading_sizes.into_iter().enumerate() {
        let n = level + 1;
        let mut heading = paragraph_style(&format!("Heading{n}"), &format!("heading {n}"));
        heading.paragraph.keep_next = Some(true);
        heading.paragraph.keep_lines = Some(true);
        heading.paragraph.spacing.before = Some(if level == 0 { 360 } else { 160 });
        heading.paragraph.spacing.after = Some(80);
        heading.paragraph.outline_level = Some(level as u8);
        heading.run.size = pt(size);
        heading.run.bold = Some(true);
        heading.run.color = Some(Some(Color(0x1F, 0x38, 0x64)));
        heading.run.italic = (level >= 3).then_some(true);
        styles.push(heading);
    }

    let mut quote = paragraph_style("Quote", "Quote");
    quote.paragraph.indentation = Indentation {
        left: Some(720),
        right: Some(720),
        ..Indentation::default()
    };
    quote.run.italic = Some(true);
    quote.run.color = Some(Some(Color(0x40, 0x40, 0x40)));
    styles.push(quote);

    let mut code = paragraph_style("SourceCode", "Source Code");
    code.paragraph.spacing = Spacing {
        after: Some(0),
        line: Some(240),
        line_rule: Some(LineRule::Auto),
        ..Spacing::default()
    };
    code.paragraph.shading = Some(Shading {
        fill: Some(Color(0xF2, 0xF2, 0xF2)),
    });
    code.run.fonts = monospace();
    code.run.size = pt(10);
    styles.push(code);

    let mut list = paragraph_style("ListParagraph", "List Paragraph");
    list.paragraph.indentation.left = Some(720);
    list.paragraph.contextual_spacing = Some(true);
    styles.push(list);

    let mut footnote = paragraph_style("FootnoteText", "footnote text");
    footnote.paragraph.spacing = Spacing {
        after: Some(0),
        line: Some(240),
        line_rule: Some(LineRule::Auto),
        ..Spacing::default()
    };
    footnote.run.size = pt(10);
    styles.push(footnote);

    styles.push(character_style(
        "Hyperlink",
        "Hyperlink",
        RunProperties {
            color: Some(Some(Color(0x05, 0x63, 0xC1))),
            underline: Some(Underline::Single),
            ..RunProperties::default()
        },
    ));
    styles.push(character_style(
        "VerbatimChar",
        "Verbatim Char",
        RunProperties {
            fonts: monospace(),
            size: pt(10),
            ..RunProperties::default()
        },
    ));
    styles.push(character_style(
        "FootnoteReference",
        "footnote reference",
        RunProperties {
            vertical_align: Some(VerticalAlign::Superscript),
            ..RunProperties::default()
        },
    ));

    let mut grid = Style::new("TableGrid", StyleKind::Table);
    grid.name = Some("Table Grid".into());
    grid.paragraph.spacing = Spacing {
        after: Some(0),
        line: Some(240),
        line_rule: Some(LineRule::Auto),
        ..Spacing::default()
    };
    grid.table = Some(TableProperties {
        borders: Some(Borders {
            top: single_border(4),
            left: single_border(4),
            bottom: single_border(4),
            right: single_border(4),
            inside_horizontal: single_border(4),
            inside_vertical: single_border(4),
        }),
        cell_margins: Some([0, 108, 0, 108]),
        ..TableProperties::default()
    });
    styles.push(grid);
    Styles::new(default_paragraph, default_run, styles)
}

/// Composes a [`Document`]: blocks go into the current section; a new
/// section starts with [`DocumentBuilder::section`].
#[derive(Debug, Clone)]
pub struct DocumentBuilder {
    document: Document,
    blocks: Vec<Block>,
    section: SectionProperties,
}

impl Default for DocumentBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl DocumentBuilder {
    /// An empty letter-size document with the [`default_styles`].
    pub fn new() -> Self {
        let document = Document {
            styles: default_styles(),
            fonts: ["Calibri", MONOSPACE]
                .iter()
                .map(|name| FontEntry {
                    name: name.to_string(),
                    ..FontEntry::default()
                })
                .collect(),
            ..Document::default()
        };
        Self {
            document,
            blocks: Vec::new(),
            section: SectionProperties::default(),
        }
    }

    pub fn metadata(&mut self, metadata: Metadata) -> &mut Self {
        self.document.metadata = metadata;
        self
    }

    pub fn title(&mut self, title: &str) -> &mut Self {
        self.document.metadata.title = Some(title.to_string());
        self
    }

    pub fn author(&mut self, author: &str) -> &mut Self {
        self.document.metadata.creator = Some(author.to_string());
        self
    }

    /// Replaces a style or adds a new one.
    pub fn style(&mut self, style: Style) -> &mut Self {
        let mut styles: Vec<Style> = self
            .document
            .styles
            .styles
            .drain(..)
            .filter(|s| s.id != style.id)
            .collect();
        styles.push(style);
        let styles = Styles::new(
            self.document.styles.default_paragraph.clone(),
            self.document.styles.default_run.clone(),
            styles,
        );
        self.document.styles = styles;
        self
    }

    pub fn page_size(&mut self, size: PageSize) -> &mut Self {
        self.section.page_size = size;
        self
    }

    /// A4 portrait, 210 by 297 mm.
    pub fn a4(&mut self) -> &mut Self {
        self.page_size(PageSize {
            width: 11906,
            height: 16838,
            orientation: Orientation::Portrait,
        })
    }

    pub fn landscape(&mut self) -> &mut Self {
        let size = self.section.page_size;
        let (width, height) = (size.width.max(size.height), size.width.min(size.height));
        self.page_size(PageSize {
            width,
            height,
            orientation: Orientation::Landscape,
        })
    }

    pub fn margins(&mut self, margins: PageMargins) -> &mut Self {
        self.section.margins = margins;
        self
    }

    /// The width between the margins of the current section, in twips.
    pub fn text_width(&self) -> i32 {
        let margins = self.section.margins;
        (self.section.page_size.width - margins.left - margins.right).max(720)
    }

    pub fn block(&mut self, block: impl Into<Block>) -> &mut Self {
        self.blocks.push(block.into());
        self
    }

    pub fn paragraph(&mut self, para: Para) -> &mut Self {
        self.block(para)
    }

    /// A paragraph of plain text in the default style.
    pub fn text(&mut self, text: &str) -> &mut Self {
        self.block(Para::new().text(text))
    }

    /// A heading paragraph; level 1 is `Heading1`, 0 is `Title`.
    pub fn heading(&mut self, level: u8, text: &str) -> &mut Self {
        let style = match level {
            0 => "Title".to_string(),
            n => format!("Heading{}", n.min(6)),
        };
        self.block(Para::styled(&style).text(text))
    }

    pub fn page_break(&mut self) -> &mut Self {
        self.block(Para::new().page_break())
    }

    /// Creates a list and returns its numbering id for [`Para::list`]. Each
    /// call starts a new list that counts from 1.
    pub fn list(&mut self, kind: ListKind) -> i64 {
        let numbering = &mut self.document.numbering;
        let abstract_id = match kind {
            ListKind::Bullet => 0,
            ListKind::Numbered => 1,
        };
        if !numbering.abstracts.iter().any(|a| a.id == abstract_id) {
            numbering.abstracts.push(AbstractNumbering {
                id: abstract_id,
                levels: list_levels(kind),
            });
            numbering.abstracts.sort_by_key(|a| a.id);
        }
        let num_id = numbering
            .instances
            .iter()
            .map(|i| i.num_id)
            .max()
            .unwrap_or(0)
            + 1;
        let start_overrides = match kind {
            ListKind::Bullet => Vec::new(),
            ListKind::Numbered => (0..9).map(|level| (level, 1)).collect(),
        };
        numbering.instances.push(NumberingInstance {
            num_id,
            abstract_id,
            start_overrides,
            level_overrides: Vec::new(),
        });
        num_id
    }

    /// A single-level list of plain-text items.
    pub fn items(&mut self, kind: ListKind, items: &[&str]) -> &mut Self {
        let num_id = self.list(kind);
        for item in items {
            self.block(Para::new().text(item).list(num_id, 0));
        }
        self
    }

    /// Adds an image and returns its id for [`Para::image`]; the MIME type
    /// is detected from the bytes.
    pub fn image(&mut self, data: impl Into<Arc<[u8]>>) -> MediaId {
        let data: Arc<[u8]> = data.into();
        let id = MediaId(self.document.media.len() as u32);
        let content_type = docboss_model::sniff_image(&data).to_string();
        let name = format!("image{}", id.0 + 1);
        self.document.media.push(Media {
            name,
            content_type,
            data,
        });
        id
    }

    /// A paragraph holding one inline picture, sized from the image's
    /// pixels at 96 DPI and scaled down to the text width.
    pub fn picture(&mut self, data: impl Into<Arc<[u8]>>) -> &mut Self {
        let data: Arc<[u8]> = data.into();
        let (width, height) = crate::image_dimensions(&data).unwrap_or((96, 96));
        let (cx, cy) = fit(width, height, self.text_width());
        let media = self.image(data);
        self.block(Para::new().image(media, cx, cy))
    }

    /// Adds a footnote whose body is `para` and returns its id for
    /// [`Para::footnote_ref`].
    pub fn footnote(&mut self, para: Para) -> i64 {
        let id = self
            .document
            .footnotes
            .iter()
            .map(|n| n.id)
            .max()
            .unwrap_or(0)
            + 1;
        let mut paragraph = para.build();
        if paragraph.style_id.is_none() {
            paragraph.style_id = Some("FootnoteText".into());
        }
        let number = Inline::Run(Run {
            properties: RunProperties {
                style_id: Some("FootnoteReference".into()),
                ..RunProperties::default()
            },
            content: vec![RunContent::NoteNumber],
        });
        paragraph
            .inlines
            .insert(0, text_run(" ", RunProperties::default()));
        paragraph.inlines.insert(0, number);
        self.document.footnotes.push(Note {
            id,
            kind: NoteKind::Footnote,
            blocks: vec![Block::Paragraph(paragraph)],
        });
        id
    }

    fn header_footer(&mut self, kind: HeaderFooterKind, blocks: Vec<Block>) -> String {
        let id = format!("hf{}", self.document.headers_footers.len() + 1);
        self.document.headers_footers.push(HeaderFooter {
            id: id.clone(),
            kind,
            blocks,
        });
        id
    }

    /// Sets the default header of the current section.
    pub fn header(&mut self, para: Para) -> &mut Self {
        let id = self.header_footer(HeaderFooterKind::Header, vec![para.into()]);
        self.section.headers.default = Some(id);
        self
    }

    /// Sets the default footer of the current section.
    pub fn footer(&mut self, para: Para) -> &mut Self {
        let id = self.header_footer(HeaderFooterKind::Footer, vec![para.into()]);
        self.section.footers.default = Some(id);
        self
    }

    /// Ends the current section and starts a new one with `properties`.
    pub fn section(&mut self, properties: SectionProperties) -> &mut Self {
        let blocks = std::mem::take(&mut self.blocks);
        let finished = std::mem::replace(&mut self.section, properties);
        self.document.sections.push(Section {
            properties: finished,
            blocks,
        });
        self
    }

    pub fn document_mut(&mut self) -> &mut Document {
        &mut self.document
    }

    /// The finished document; the builder keeps its state.
    pub fn build(&self) -> Document {
        let mut document = self.document.clone();
        document.sections.push(Section {
            properties: self.section.clone(),
            blocks: self.blocks.clone(),
        });
        document
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        crate::to_bytes(&self.build())
    }

    pub fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        crate::save(&self.build(), path)
    }
}

/// EMU extents of a `width` by `height` pixel image at 96 DPI, scaled
/// down to fit `max_twips`.
pub(crate) fn fit(width: u32, height: u32, max_twips: i32) -> (i64, i64) {
    let cx = i64::from(width.max(1)) * EMU_PER_PIXEL;
    let cy = i64::from(height.max(1)) * EMU_PER_PIXEL;
    let max = i64::from(max_twips) * 635;
    if cx <= max {
        return (cx, cy);
    }
    (max, cy * max / cx)
}

/// Composes a table with equal-width columns across a given width.
#[derive(Debug, Clone)]
pub struct TableBuilder {
    table: Table,
}

impl TableBuilder {
    /// A table of `columns` equal columns spanning `width` twips, in the
    /// `TableGrid` style.
    pub fn new(columns: usize, width: i32) -> Self {
        let columns = columns.max(1);
        let column = width / columns as i32;
        let table = Table {
            properties: TableProperties {
                style_id: Some("TableGrid".into()),
                width: Some(column * columns as i32),
                ..TableProperties::default()
            },
            grid: vec![column; columns],
            rows: Vec::new(),
        };
        Self { table }
    }

    fn push_row(&mut self, cells: Vec<Vec<Block>>, header: bool) {
        let cells = cells
            .into_iter()
            .zip(self.table.grid.iter())
            .map(|(blocks, width)| TableCell {
                properties: TableCellProperties {
                    width: Some(*width),
                    grid_span: 1,
                    ..TableCellProperties::default()
                },
                blocks,
            })
            .collect();
        let properties = TableRowProperties {
            header,
            ..TableRowProperties::default()
        };
        self.table.rows.push(TableRow { properties, cells });
    }

    /// A row of plain-text cells.
    pub fn row(mut self, cells: &[&str]) -> Self {
        let cells = cells
            .iter()
            .map(|text| vec![Para::new().text(text).into()])
            .collect();
        self.push_row(cells, false);
        self
    }

    /// A row of bold cells repeated at the top of each page.
    pub fn header(mut self, cells: &[&str]) -> Self {
        let cells = cells
            .iter()
            .map(|text| vec![Para::new().bold(text).into()])
            .collect();
        self.push_row(cells, true);
        self
    }

    /// A header row whose cells hold arbitrary blocks.
    pub fn header_blocks(mut self, cells: Vec<Vec<Block>>) -> Self {
        self.push_row(cells, true);
        self
    }

    /// A row whose cells hold arbitrary blocks.
    pub fn row_blocks(mut self, cells: Vec<Vec<Block>>) -> Self {
        self.push_row(cells, false);
        self
    }

    pub fn build(self) -> Table {
        self.table
    }
}

impl From<TableBuilder> for Block {
    fn from(builder: TableBuilder) -> Block {
        Block::Table(builder.table)
    }
}

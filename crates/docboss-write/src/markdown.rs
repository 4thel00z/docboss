//! CommonMark + GFM to DOCX: headings become `Heading1`..`Heading6`,
//! lists become numbering definitions, code uses the `SourceCode` and
//! `VerbatimChar` styles, GFM tables use `TableGrid`, footnotes become
//! WordprocessingML footnotes. Raw HTML is dropped, except `<br>`.

use std::collections::BTreeMap;
use std::path::PathBuf;

use docboss_model::{
    Block, Border, BorderStyle, Borders, Break, Color, Document, Drawing, DrawingPlacement,
    Hyperlink, Inline, Justification, Note, NoteKind, PageMargins, PageSize, Paragraph, Run,
    RunContent, RunProperties, Table,
};
use pulldown_cmark::{
    Alignment, Event, HeadingLevel, Options as ParseOptions, Parser, Tag, TagEnd,
};

use crate::builder::{fit, text_run};
use crate::{DocumentBuilder, ListKind, Result, TableBuilder};

/// Loads the bytes of an image by the URL or path the Markdown gives.
pub type ImageResolver<'a> = dyn Fn(&str) -> Option<Vec<u8>> + 'a;

/// Composition options for [`to_document`].
#[derive(Default)]
pub struct Options<'a> {
    pub page_size: PageSize,
    pub margins: PageMargins,
    /// The document title; the first heading's text when `None`.
    pub title: Option<String>,
    /// Images the resolver cannot load are replaced by their alt text.
    pub images: Option<Box<ImageResolver<'a>>>,
}

impl Options<'_> {
    /// Options whose resolver reads relative image paths under `dir`;
    /// URLs with a scheme are not fetched.
    pub fn local_images(dir: impl Into<PathBuf>) -> Self {
        let dir = dir.into();
        let resolver = move |source: &str| {
            if source.contains("://") {
                return None;
            }
            std::fs::read(dir.join(source)).ok()
        };
        Self {
            images: Some(Box::new(resolver)),
            ..Self::default()
        }
    }
}

struct ListState {
    num_id: i64,
    level: u8,
    first_paragraph: bool,
}

struct TableState {
    alignments: Vec<Alignment>,
    header: Vec<Vec<Block>>,
    rows: Vec<Vec<Vec<Block>>>,
    row: Vec<Vec<Block>>,
    in_head: bool,
}

struct LinkFrame {
    url: String,
    title: String,
    inlines: Vec<Inline>,
}

struct Converter<'o, 'a> {
    options: &'o Options<'a>,
    builder: DocumentBuilder,
    targets: Vec<Vec<Block>>,
    paragraph: Option<Paragraph>,
    links: Vec<LinkFrame>,
    bold: u32,
    italic: u32,
    strike: u32,
    quote_depth: u32,
    lists: Vec<ListState>,
    code: Option<String>,
    table: Option<TableState>,
    image: Option<(String, String)>,
    footnote_ids: BTreeMap<String, i64>,
    notes: Vec<(i64, Vec<Block>)>,
    heading: bool,
    first_heading: Option<String>,
}

const INDENT: i32 = 720;

fn horizontal_rule() -> Paragraph {
    let border = Border {
        style: BorderStyle::Single,
        size: 6,
        space: 1,
        color: Some(Color(0xA0, 0xA0, 0xA0)),
    };
    let mut paragraph = Paragraph::default();
    paragraph.properties.borders = Some(Borders {
        bottom: Some(border),
        ..Borders::default()
    });
    paragraph
}

impl<'o, 'a> Converter<'o, 'a> {
    fn new(options: &'o Options<'a>) -> Self {
        let mut builder = DocumentBuilder::new();
        builder
            .page_size(options.page_size)
            .margins(options.margins);
        Self {
            options,
            builder,
            targets: vec![Vec::new()],
            paragraph: None,
            links: Vec::new(),
            bold: 0,
            italic: 0,
            strike: 0,
            quote_depth: 0,
            lists: Vec::new(),
            code: None,
            table: None,
            image: None,
            footnote_ids: BTreeMap::new(),
            notes: Vec::new(),
            heading: false,
            first_heading: None,
        }
    }

    fn target(&mut self) -> &mut Vec<Block> {
        if self.targets.is_empty() {
            self.targets.push(Vec::new());
        }
        let last = self.targets.len() - 1;
        &mut self.targets[last]
    }

    fn cell_alignment(&self) -> Option<Justification> {
        let table = self.table.as_ref()?;
        let index = if table.in_head {
            table.header.len()
        } else {
            table.row.len()
        };
        match table.alignments.get(index)? {
            Alignment::Center => Some(Justification::Center),
            Alignment::Right => Some(Justification::Right),
            Alignment::Left => Some(Justification::Left),
            Alignment::None => None,
        }
    }

    fn start_paragraph(&mut self, style: Option<String>) {
        self.end_paragraph();
        let mut paragraph = Paragraph {
            style_id: style,
            ..Paragraph::default()
        };
        let list_indent = self
            .lists
            .last()
            .map_or(0, |list| INDENT * (i32::from(list.level) + 1));
        let quote_indent = INDENT * self.quote_depth as i32;
        match self.lists.last_mut() {
            Some(list) if list.first_paragraph && paragraph.style_id.is_none() => {
                list.first_paragraph = false;
                paragraph.properties.numbering = Some(docboss_model::NumberingRef {
                    num_id: list.num_id,
                    level: list.level,
                });
                paragraph.style_id = Some("ListParagraph".into());
                if quote_indent > 0 {
                    paragraph.properties.indentation.left = Some(list_indent + quote_indent);
                }
            }
            _ if list_indent + quote_indent > 0 => {
                paragraph.properties.indentation.left = Some(list_indent + quote_indent);
            }
            _ => {}
        }
        if self.quote_depth > 0 && paragraph.style_id.is_none() {
            paragraph.style_id = Some("Quote".into());
        }
        if !self.lists.is_empty() && paragraph.properties.numbering.is_none() {
            paragraph.properties.contextual_spacing = Some(true);
        }
        paragraph.properties.justification = self.cell_alignment();
        self.paragraph = Some(paragraph);
    }

    fn end_paragraph(&mut self) {
        let Some(paragraph) = self.paragraph.take() else {
            return;
        };
        if self.heading && self.first_heading.is_none() {
            self.first_heading = Some(paragraph.text());
        }
        self.target().push(Block::Paragraph(paragraph));
    }

    fn push_inline(&mut self, inline: Inline) {
        if let Some(frame) = self.links.last_mut() {
            frame.inlines.push(inline);
            return;
        }
        if self.paragraph.is_none() {
            self.start_paragraph(None);
        }
        if let Some(paragraph) = self.paragraph.as_mut() {
            paragraph.inlines.push(inline);
        }
    }

    fn run_properties(&self, code: bool) -> RunProperties {
        let style_id = match (code, self.links.is_empty()) {
            (true, _) => Some("VerbatimChar".to_string()),
            (false, false) => Some("Hyperlink".to_string()),
            (false, true) => None,
        };
        let in_head = self.table.as_ref().is_some_and(|t| t.in_head);
        RunProperties {
            style_id,
            bold: (self.bold > 0 || in_head).then_some(true),
            italic: (self.italic > 0).then_some(true),
            strike: (self.strike > 0).then_some(true),
            ..RunProperties::default()
        }
    }

    fn text(&mut self, text: &str, code: bool) {
        if let Some(buffer) = self.code.as_mut() {
            buffer.push_str(text);
            return;
        }
        if let Some((_, alt)) = self.image.as_mut() {
            alt.push_str(text);
            return;
        }
        let properties = self.run_properties(code);
        self.push_inline(text_run(text, properties));
    }

    fn content(&mut self, content: RunContent, properties: RunProperties) {
        if self.image.is_some() || self.code.is_some() {
            return;
        }
        self.push_inline(Inline::Run(Run {
            properties,
            content: vec![content],
        }));
    }

    fn footnote_id(&mut self, label: &str) -> i64 {
        let next = self.footnote_ids.len() as i64 + 1;
        *self.footnote_ids.entry(label.to_string()).or_insert(next)
    }

    fn start_list(&mut self, start: Option<u64>) {
        self.end_paragraph();
        let kind = if start.is_some() {
            ListKind::Numbered
        } else {
            ListKind::Bullet
        };
        let num_id = self.builder.list(kind);
        let level = self.lists.len().min(8) as u8;
        if let Some(start) = start {
            let numbering = &mut self.builder.document_mut().numbering;
            if let Some(instance) = numbering.instances.iter_mut().find(|i| i.num_id == num_id) {
                instance.start_overrides.retain(|(l, _)| *l != level);
                instance
                    .start_overrides
                    .push((level, start.min(u64::from(u32::MAX)) as u32));
                instance.start_overrides.sort_unstable();
            }
        }
        self.lists.push(ListState {
            num_id,
            level,
            first_paragraph: false,
        });
    }

    fn code_block(&mut self, source: String) {
        self.start_paragraph(Some("SourceCode".into()));
        let source = source.strip_suffix('\n').unwrap_or(&source);
        let properties = RunProperties::default();
        for (index, line) in source.split('\n').enumerate() {
            if index > 0 {
                self.content(RunContent::Break(Break::Line), properties.clone());
            }
            if !line.is_empty() {
                self.push_inline(text_run(line, properties.clone()));
            }
        }
        self.end_paragraph();
    }

    fn image(&mut self, url: &str, alt: String) {
        let bytes = self
            .options
            .images
            .as_ref()
            .and_then(|resolve| resolve(url));
        let Some(bytes) = bytes else {
            let fallback = if alt.is_empty() { url.to_string() } else { alt };
            let properties = self.run_properties(false);
            self.push_inline(text_run(&fallback, properties));
            return;
        };
        let (width, height) = crate::image_dimensions(&bytes).unwrap_or((96, 96));
        let (cx, cy) = fit(width, height, self.builder.text_width());
        let media = self.builder.image(bytes);
        let drawing = Drawing {
            media: Some(media),
            width: cx,
            height: cy,
            placement: DrawingPlacement::Inline,
            name: None,
            description: (!alt.is_empty()).then_some(alt),
            text_box: Vec::new(),
        };
        self.push_inline(Inline::Run(Run {
            properties: RunProperties::default(),
            content: vec![RunContent::Drawing(drawing)],
        }));
    }

    fn end_link(&mut self) {
        let Some(frame) = self.links.pop() else {
            return;
        };
        let (target, anchor) = match frame.url.strip_prefix('#') {
            Some(anchor) => (None, Some(anchor.to_string())),
            None => (Some(frame.url), None),
        };
        let tooltip = (!frame.title.is_empty()).then_some(frame.title);
        self.push_inline(Inline::Hyperlink(Hyperlink {
            target,
            anchor,
            tooltip,
            inlines: frame.inlines,
        }));
    }

    fn end_cell(&mut self) {
        self.end_paragraph();
        let blocks = self.targets.pop().unwrap_or_default();
        let Some(table) = self.table.as_mut() else {
            return;
        };
        if table.in_head {
            table.header.push(blocks);
            return;
        }
        table.row.push(blocks);
    }

    fn end_table(&mut self) {
        let Some(state) = self.table.take() else {
            return;
        };
        let columns = state.alignments.len().max(state.header.len()).max(1);
        let mut builder = TableBuilder::new(columns, self.builder.text_width());
        if !state.header.is_empty() {
            builder = builder.header_blocks(state.header);
        }
        for row in state.rows {
            builder = builder.row_blocks(row);
        }
        let table: Table = builder.build();
        self.target().push(Block::Table(table));
    }

    fn start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Paragraph => self.start_paragraph(None),
            Tag::Heading { level, .. } => {
                let n = match level {
                    HeadingLevel::H1 => 1,
                    HeadingLevel::H2 => 2,
                    HeadingLevel::H3 => 3,
                    HeadingLevel::H4 => 4,
                    HeadingLevel::H5 => 5,
                    HeadingLevel::H6 => 6,
                };
                self.heading = true;
                self.start_paragraph(Some(format!("Heading{n}")));
            }
            Tag::BlockQuote(_) => {
                self.end_paragraph();
                self.quote_depth += 1;
            }
            Tag::CodeBlock(_) => {
                self.end_paragraph();
                self.code = Some(String::new());
            }
            Tag::List(start) => self.start_list(start),
            Tag::Item => {
                self.end_paragraph();
                if let Some(list) = self.lists.last_mut() {
                    list.first_paragraph = true;
                }
            }
            Tag::FootnoteDefinition(label) => {
                self.end_paragraph();
                let id = self.footnote_id(&label);
                self.notes.push((id, Vec::new()));
                self.targets.push(Vec::new());
            }
            Tag::Table(alignments) => {
                self.end_paragraph();
                self.table = Some(TableState {
                    alignments,
                    header: Vec::new(),
                    rows: Vec::new(),
                    row: Vec::new(),
                    in_head: false,
                });
            }
            Tag::TableHead => {
                if let Some(table) = self.table.as_mut() {
                    table.in_head = true;
                }
            }
            Tag::TableCell => self.targets.push(Vec::new()),
            Tag::Emphasis => self.italic += 1,
            Tag::Strong => self.bold += 1,
            Tag::Strikethrough => self.strike += 1,
            Tag::Link {
                dest_url, title, ..
            } => self.links.push(LinkFrame {
                url: dest_url.to_string(),
                title: title.to_string(),
                inlines: Vec::new(),
            }),
            Tag::Image { dest_url, .. } => self.image = Some((dest_url.to_string(), String::new())),
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => self.end_paragraph(),
            TagEnd::Heading(_) => {
                self.end_paragraph();
                self.heading = false;
            }
            TagEnd::BlockQuote(_) => {
                self.end_paragraph();
                self.quote_depth = self.quote_depth.saturating_sub(1);
            }
            TagEnd::CodeBlock => {
                if let Some(source) = self.code.take() {
                    self.code_block(source);
                }
            }
            TagEnd::List(_) => {
                self.end_paragraph();
                self.lists.pop();
            }
            TagEnd::Item => self.end_paragraph(),
            TagEnd::FootnoteDefinition => {
                self.end_paragraph();
                let blocks = self.targets.pop().unwrap_or_default();
                if let Some((_, body)) = self.notes.last_mut() {
                    *body = blocks;
                }
            }
            TagEnd::TableCell => self.end_cell(),
            TagEnd::TableHead => {
                if let Some(table) = self.table.as_mut() {
                    table.in_head = false;
                }
            }
            TagEnd::TableRow => {
                if let Some(table) = self.table.as_mut() {
                    let row = std::mem::take(&mut table.row);
                    table.rows.push(row);
                }
            }
            TagEnd::Table => self.end_table(),
            TagEnd::Emphasis => self.italic = self.italic.saturating_sub(1),
            TagEnd::Strong => self.bold = self.bold.saturating_sub(1),
            TagEnd::Strikethrough => self.strike = self.strike.saturating_sub(1),
            TagEnd::Link => self.end_link(),
            TagEnd::Image => {
                if let Some((url, alt)) = self.image.take() {
                    self.image(&url, alt);
                }
            }
            _ => {}
        }
    }

    fn event(&mut self, event: Event<'_>) {
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(text) => self.text(&text, false),
            Event::Code(text) | Event::InlineMath(text) | Event::DisplayMath(text) => {
                self.text(&text, true)
            }
            Event::SoftBreak => self.text(" ", false),
            Event::HardBreak => {
                let properties = self.run_properties(false);
                self.content(RunContent::Break(Break::Line), properties);
            }
            Event::Rule => {
                self.end_paragraph();
                self.target().push(Block::Paragraph(horizontal_rule()));
            }
            Event::FootnoteReference(label) => {
                let id = self.footnote_id(&label);
                let properties = RunProperties {
                    style_id: Some("FootnoteReference".into()),
                    ..RunProperties::default()
                };
                self.content(RunContent::FootnoteReference(id), properties);
            }
            Event::TaskListMarker(checked) => {
                let marker = if checked { "\u{2612} " } else { "\u{2610} " };
                self.text(marker, false);
            }
            Event::InlineHtml(html) | Event::Html(html) => {
                let tag = html.trim().to_ascii_lowercase().replace(' ', "");
                if matches!(tag.as_str(), "<br>" | "<br/>") {
                    let properties = self.run_properties(false);
                    self.content(RunContent::Break(Break::Line), properties);
                }
            }
        }
    }

    fn finish(mut self) -> Document {
        self.end_paragraph();
        let body = self.targets.drain(..).next().unwrap_or_default();
        for block in body {
            self.builder.block(block);
        }
        let title = self.options.title.clone().or(self.first_heading.take());
        if let Some(title) = title {
            self.builder.title(&title);
        }
        let mut document = self.builder.build();
        self.notes.sort_by_key(|(id, _)| *id);
        document.footnotes = self
            .notes
            .into_iter()
            .map(|(id, blocks)| footnote(id, blocks))
            .collect();
        document
    }
}

fn footnote(id: i64, mut blocks: Vec<Block>) -> Note {
    for block in &mut blocks {
        if let Block::Paragraph(paragraph) = block {
            paragraph
                .style_id
                .get_or_insert_with(|| "FootnoteText".into());
        }
    }
    if !matches!(blocks.first(), Some(Block::Paragraph(_))) {
        blocks.insert(
            0,
            Block::Paragraph(Paragraph {
                style_id: Some("FootnoteText".into()),
                ..Paragraph::default()
            }),
        );
    }
    if let Some(Block::Paragraph(first)) = blocks.first_mut() {
        let number = Inline::Run(Run {
            properties: RunProperties {
                style_id: Some("FootnoteReference".into()),
                ..RunProperties::default()
            },
            content: vec![RunContent::NoteNumber],
        });
        first
            .inlines
            .insert(0, text_run(" ", RunProperties::default()));
        first.inlines.insert(0, number);
    }
    Note {
        id,
        kind: NoteKind::Footnote,
        blocks,
    }
}

fn parse_options() -> ParseOptions {
    ParseOptions::ENABLE_TABLES
        | ParseOptions::ENABLE_FOOTNOTES
        | ParseOptions::ENABLE_STRIKETHROUGH
        | ParseOptions::ENABLE_TASKLISTS
}

/// Converts Markdown into a document with the built-in style sheet.
pub fn to_document(markdown: &str, options: &Options<'_>) -> Document {
    let mut converter = Converter::new(options);
    Parser::new_ext(markdown, parse_options()).for_each(|event| converter.event(event));
    converter.finish()
}

/// Converts Markdown into DOCX bytes.
pub fn to_docx(markdown: &str, options: &Options<'_>) -> Result<Vec<u8>> {
    crate::to_bytes(&to_document(markdown, options))
}

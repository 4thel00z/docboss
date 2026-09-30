//! Semantic HTML5: headings, paragraphs, nested lists, tables with merged
//! cells, links, images and a footnotes section.

use std::fmt::{self, Write};

use docboss_model::{
    Block, Document, Drawing, NoteKind, NumberFormat, Paragraph, Section, Table, TableCell,
    VerticalMerge,
};

use crate::walk::{self, Fmt, ListItem, Lists, Notes, Piece, Resolver};
use crate::ImageMode;

/// What HTML output includes and how.
#[derive(Debug, Clone)]
pub struct HtmlOptions {
    /// Write a whole document with `<head>` and the metadata title instead
    /// of a body fragment.
    pub standalone: bool,
    pub images: ImageMode,
    /// Write each section's headers before it and its footers after it.
    pub headers_footers: bool,
    /// Link note references to a footnotes section at the end.
    pub notes: bool,
    /// Link comment references to a comments section at the end.
    pub comments: bool,
}

impl Default for HtmlOptions {
    fn default() -> Self {
        Self {
            standalone: false,
            images: ImageMode::Embed,
            headers_footers: false,
            notes: true,
            comments: false,
        }
    }
}

/// Writes the document as HTML to `out`.
pub fn write_html<W: Write>(doc: &Document, options: &HtmlOptions, out: &mut W) -> fmt::Result {
    if options.standalone {
        out.write_str("<!DOCTYPE html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n")?;
        if let Some(title) = doc.metadata.title.as_deref() {
            out.write_str("<title>")?;
            escape(title, out)?;
            out.write_str("</title>\n")?;
        }
        out.write_str("</head>\n<body>\n")?;
    }
    let mut writer = HtmlWriter {
        resolver: Resolver::new(doc),
        lists: Lists::new(doc),
        notes: Notes::default(),
        options,
        stack: Vec::new(),
        in_code: false,
        buf: String::new(),
    };
    for section in &doc.sections {
        writer.section(section, out)?;
    }
    writer.close_blocks(out)?;
    writer.notes_sections(doc, out)?;
    if options.standalone {
        out.write_str("</body>\n</html>\n")?;
    }
    Ok(())
}

/// The document as an HTML string.
pub fn to_html(doc: &Document, options: &HtmlOptions) -> String {
    let mut out = String::with_capacity(crate::text::estimate(doc) * 2);
    let _ = write_html(doc, options, &mut out);
    out
}

struct OpenList {
    level: u8,
    tag: &'static str,
}

struct HtmlWriter<'a, 'o> {
    resolver: Resolver<'a>,
    lists: Lists<'a>,
    notes: Notes,
    options: &'o HtmlOptions,
    stack: Vec<OpenList>,
    in_code: bool,
    buf: String,
}

impl<'a> HtmlWriter<'a, '_> {
    fn section<W: Write>(&mut self, section: &'a Section, out: &mut W) -> fmt::Result {
        let doc = self.resolver.doc;
        let parts = crate::section_parts(section);
        if self.options.headers_footers {
            for id in parts.headers {
                let Some(part) = doc.header_footer(id) else {
                    continue;
                };
                self.close_blocks(out)?;
                out.write_str("<header>\n")?;
                self.blocks(&part.blocks, out)?;
                self.close_blocks(out)?;
                out.write_str("</header>\n")?;
            }
        }
        self.blocks(&section.blocks, out)?;
        if !self.options.headers_footers {
            return Ok(());
        }
        for id in parts.footers {
            let Some(part) = doc.header_footer(id) else {
                continue;
            };
            self.close_blocks(out)?;
            out.write_str("<footer>\n")?;
            self.blocks(&part.blocks, out)?;
            self.close_blocks(out)?;
            out.write_str("</footer>\n")?;
        }
        Ok(())
    }

    fn close_code<W: Write>(&mut self, out: &mut W) -> fmt::Result {
        if !self.in_code {
            return Ok(());
        }
        self.in_code = false;
        out.write_str("</code></pre>\n")
    }

    fn close_lists<W: Write>(&mut self, out: &mut W) -> fmt::Result {
        while let Some(list) = self.stack.pop() {
            write!(out, "</li>\n</{}>\n", list.tag)?;
        }
        Ok(())
    }

    fn close_blocks<W: Write>(&mut self, out: &mut W) -> fmt::Result {
        self.close_code(out)?;
        self.close_lists(out)
    }

    fn blocks<W: Write>(&mut self, blocks: &'a [Block], out: &mut W) -> fmt::Result {
        for block in blocks {
            match block {
                Block::Paragraph(paragraph) => {
                    for body in self.paragraph(paragraph, out)? {
                        self.blocks(body, out)?;
                    }
                }
                Block::Table(table) => {
                    self.close_blocks(out)?;
                    self.table(table, out)?;
                }
            }
        }
        Ok(())
    }

    /// Writes the paragraph and returns the bodies of the text boxes it
    /// anchors, for the caller to write after it.
    fn paragraph<W: Write>(
        &mut self,
        paragraph: &'a Paragraph,
        out: &mut W,
    ) -> Result<Vec<&'a [Block]>, fmt::Error> {
        let info = self.resolver.paragraph(paragraph);
        let mut pieces = Vec::new();
        walk::flatten(&mut self.resolver, paragraph, &mut pieces);
        if info.code || walk::all_monospace(&pieces) {
            self.close_lists(out)?;
            let mut text = String::new();
            walk::pieces_text(&pieces, &mut text);
            if self.in_code {
                out.write_char('\n')?;
            } else {
                out.write_str("<pre><code>")?;
                self.in_code = true;
            }
            escape(text.trim_end(), out)?;
            return Ok(walk::text_boxes(&pieces));
        }
        self.close_code(out)?;
        let item = info
            .numbering
            .and_then(|reference| self.lists.next(reference));
        let heading = info.heading.filter(|_| item.is_none());
        let mut buf = std::mem::take(&mut self.buf);
        buf.clear();
        self.inline(&pieces, heading.is_some(), &mut buf);
        let content = buf.trim();
        let result = match (heading, item) {
            _ if content.is_empty() => Ok(()),
            (Some(level), _) => {
                self.close_lists(out)?;
                let n = level.min(5) + 1;
                writeln!(out, "<h{n}>{content}</h{n}>")
            }
            (None, Some(item)) => self.list_item(&item, content, out),
            (None, None) => {
                self.close_lists(out)?;
                writeln!(out, "<p>{content}</p>")
            }
        };
        self.buf = buf;
        result.map(|()| walk::text_boxes(&pieces))
    }

    fn list_item<W: Write>(&mut self, item: &ListItem, content: &str, out: &mut W) -> fmt::Result {
        let tag = if item.bullet { "ul" } else { "ol" };
        while self
            .stack
            .last()
            .is_some_and(|list| list.level > item.level)
        {
            let list = self.stack.pop().map_or("ul", |list| list.tag);
            write!(out, "</li>\n</{list}>\n")?;
        }
        let same_level = self
            .stack
            .last()
            .is_some_and(|list| list.level == item.level);
        if same_level && self.stack.last().is_some_and(|list| list.tag == tag) {
            out.write_str("</li>\n")?;
        }
        if same_level && self.stack.last().is_some_and(|list| list.tag != tag) {
            let list = self.stack.pop().map_or("ul", |list| list.tag);
            write!(out, "</li>\n</{list}>\n")?;
        }
        let open = self.stack.last().is_none_or(|list| list.level < item.level);
        if open {
            out.write_char('<')?;
            out.write_str(tag)?;
            if !item.bullet {
                let kind = match item.format {
                    NumberFormat::LowerLetter => Some("a"),
                    NumberFormat::UpperLetter => Some("A"),
                    NumberFormat::LowerRoman => Some("i"),
                    NumberFormat::UpperRoman => Some("I"),
                    _ => None,
                };
                if let Some(kind) = kind {
                    write!(out, " type=\"{kind}\"")?;
                }
                if item.ordinal != 1 {
                    write!(out, " start=\"{}\"", item.ordinal)?;
                }
            }
            out.write_str(">\n")?;
            self.stack.push(OpenList {
                level: item.level,
                tag,
            });
        }
        write!(out, "<li>{content}")
    }

    fn inline(&mut self, pieces: &[Piece<'a>], heading: bool, out: &mut String) {
        let mut group = String::new();
        let mut group_fmt = Fmt::default();
        let mut depth = 0usize;
        for piece in pieces {
            match piece {
                Piece::Text(text, fmt) => {
                    let fmt = Fmt {
                        bold: fmt.bold && !heading,
                        ..*fmt
                    };
                    let blank = text.trim().is_empty();
                    if !group.is_empty() && fmt != group_fmt && !blank {
                        flush(&mut group, group_fmt, out);
                    }
                    if group.is_empty() {
                        group_fmt = fmt;
                    }
                    group.push_str(text);
                }
                Piece::Tab => group.push(' '),
                Piece::LineBreak | Piece::ColumnBreak => {
                    flush(&mut group, group_fmt, out);
                    out.push_str("<br>");
                }
                Piece::Note(kind, id) if self.options.notes => {
                    flush(&mut group, group_fmt, out);
                    let n = self.notes.number(*kind, *id);
                    let label = walk::note_label(*kind, n);
                    let prefix = note_prefix(*kind);
                    let _ = write!(
                        out,
                        "<sup><a href=\"#{prefix}-{label}\" id=\"{prefix}ref-{label}\">{label}</a></sup>"
                    );
                }
                Piece::Comment(id) if self.options.comments => {
                    flush(&mut group, group_fmt, out);
                    let n = self.notes.comment_number(*id);
                    let _ = write!(
                        out,
                        "<sup><a href=\"#comment-{n}\" id=\"commentref-{n}\">[c{n}]</a></sup>"
                    );
                }
                Piece::Image(drawing) => {
                    flush(&mut group, group_fmt, out);
                    self.image(drawing, out);
                }
                Piece::Math(math) => {
                    flush(&mut group, group_fmt, out);
                    out.push_str("<span class=\"math\">");
                    let _ = escape(&docboss_model::linear_text(&math.nodes), out);
                    out.push_str("</span>");
                }
                Piece::LinkStart(link) => {
                    flush(&mut group, group_fmt, out);
                    depth += 1;
                    if depth == 1 {
                        out.push_str("<a href=\"");
                        let _ = escape(&link.href(), out);
                        out.push_str("\">");
                    }
                }
                Piece::LinkEnd => {
                    flush(&mut group, group_fmt, out);
                    if depth == 1 {
                        out.push_str("</a>");
                    }
                    depth = depth.saturating_sub(1);
                }
                Piece::Bookmark(name) if !name.starts_with('_') => {
                    flush(&mut group, group_fmt, out);
                    out.push_str("<a id=\"");
                    let _ = escape(name, out);
                    out.push_str("\"></a>");
                }
                _ => {}
            }
        }
        flush(&mut group, group_fmt, out);
        if depth > 0 {
            out.push_str("</a>");
        }
    }

    fn image(&self, drawing: &Drawing, out: &mut String) {
        for member in &drawing.members {
            self.image(&member.drawing, out);
        }
        let doc = self.resolver.doc;
        let Some(media) = drawing.media.and_then(|id| doc.media(id)) else {
            return;
        };
        let alt = drawing
            .description
            .as_deref()
            .filter(|d| !d.trim().is_empty())
            .or(drawing.name.as_deref())
            .unwrap_or("");
        out.push_str("<img src=\"");
        match &self.options.images {
            ImageMode::Omit => {
                out.truncate(out.len() - "<img src=\"".len());
                return;
            }
            ImageMode::Reference { prefix } => {
                let path = format!("{prefix}{}", walk::media_file_name(&media.name));
                let _ = escape(&path, out);
            }
            ImageMode::Embed => {
                let _ = write!(out, "data:{};base64,", media.content_type);
                walk::base64(&media.data, out);
            }
        }
        out.push_str("\" alt=\"");
        let _ = escape(alt.trim(), out);
        out.push('"');
        const EMU_PER_PIXEL: i64 = 9525;
        if drawing.width > 0 && drawing.height > 0 {
            let _ = write!(
                out,
                " width=\"{}\" height=\"{}\"",
                drawing.width / EMU_PER_PIXEL,
                drawing.height / EMU_PER_PIXEL
            );
        }
        out.push('>');
    }

    fn table<W: Write>(&mut self, table: &'a Table, out: &mut W) -> fmt::Result {
        let positioned: Vec<Vec<(usize, &TableCell)>> = table
            .rows
            .iter()
            .map(|row| {
                let mut column = 0;
                row.cells
                    .iter()
                    .map(|cell| {
                        let start = column;
                        column += cell.span() as usize;
                        (start, cell)
                    })
                    .collect()
            })
            .collect();
        out.write_str("<table>\n")?;
        for (r, row) in table.rows.iter().enumerate() {
            let tag = if row.properties.header { "th" } else { "td" };
            out.write_str("<tr>")?;
            for &(column, cell) in &positioned[r] {
                if cell.properties.vertical_merge == Some(VerticalMerge::Continue) {
                    continue;
                }
                write!(out, "<{tag}")?;
                if cell.span() > 1 {
                    write!(out, " colspan=\"{}\"", cell.span())?;
                }
                if cell.properties.vertical_merge == Some(VerticalMerge::Restart) {
                    let rows = positioned[r + 1..]
                        .iter()
                        .take_while(|cells| {
                            cells.iter().any(|&(start, below)| {
                                start == column
                                    && below.properties.vertical_merge
                                        == Some(VerticalMerge::Continue)
                            })
                        })
                        .count();
                    if rows > 0 {
                        write!(out, " rowspan=\"{}\"", rows + 1)?;
                    }
                }
                out.write_char('>')?;
                self.cell(&cell.blocks, out)?;
                write!(out, "</{tag}>")?;
            }
            out.write_str("</tr>\n")?;
        }
        out.write_str("</table>\n")
    }

    fn cell<W: Write>(&mut self, blocks: &'a [Block], out: &mut W) -> fmt::Result {
        let simple = blocks.len() == 1 && matches!(blocks[0], Block::Paragraph(_));
        let Some(Block::Paragraph(paragraph)) = blocks.first().filter(|_| simple) else {
            let saved_stack = std::mem::take(&mut self.stack);
            let saved_code = std::mem::replace(&mut self.in_code, false);
            self.blocks(blocks, out)?;
            self.close_blocks(out)?;
            self.stack = saved_stack;
            self.in_code = saved_code;
            return Ok(());
        };
        let info = self.resolver.paragraph(paragraph);
        if info.numbering.is_some() || info.code {
            let saved_stack = std::mem::take(&mut self.stack);
            self.blocks(blocks, out)?;
            self.close_blocks(out)?;
            self.stack = saved_stack;
            return Ok(());
        }
        let mut pieces = Vec::new();
        walk::flatten(&mut self.resolver, paragraph, &mut pieces);
        let mut text = String::new();
        self.inline(&pieces, false, &mut text);
        out.write_str(text.trim())
    }

    fn notes_sections<W: Write>(&mut self, doc: &'a Document, out: &mut W) -> fmt::Result {
        let mut index = 0;
        let mut open: Option<NoteKind> = None;
        while index < self.notes.order.len() {
            let (kind, id) = self.notes.order[index];
            index += 1;
            let note = match kind {
                NoteKind::Footnote => doc.footnote(id),
                NoteKind::Endnote => doc.endnote(id),
            };
            let Some(note) = note else { continue };
            if open != Some(kind) {
                if open.is_some() {
                    out.write_str("</ol>\n</section>\n")?;
                }
                let (class, list) = match kind {
                    NoteKind::Footnote => ("footnotes", "<ol>"),
                    NoteKind::Endnote => ("endnotes", "<ol type=\"i\">"),
                };
                write!(out, "<section class=\"{class}\">\n{list}\n")?;
                open = Some(kind);
            }
            let label = walk::note_label(kind, self.notes.number(kind, id));
            let prefix = note_prefix(kind);
            writeln!(out, "<li id=\"{prefix}-{label}\">")?;
            self.blocks(&note.blocks, out)?;
            self.close_blocks(out)?;
            writeln!(out, "<a href=\"#{prefix}ref-{label}\">\u{21a9}</a></li>")?;
        }
        if open.is_some() {
            out.write_str("</ol>\n</section>\n")?;
        }
        if !self.options.comments || self.notes.comments.is_empty() {
            return Ok(());
        }
        out.write_str("<section class=\"comments\">\n<ol>\n")?;
        let mut index = 0;
        while index < self.notes.comments.len() {
            let id = self.notes.comments[index];
            index += 1;
            let Some(comment) = doc.comment(id) else {
                continue;
            };
            writeln!(out, "<li id=\"comment-{index}\">")?;
            if let Some(author) = comment.author.as_deref() {
                out.write_str("<strong>")?;
                escape(author, out)?;
                out.write_str("</strong>\n")?;
            }
            self.blocks(&comment.blocks, out)?;
            self.close_blocks(out)?;
            out.write_str("</li>\n")?;
        }
        out.write_str("</ol>\n</section>\n")
    }
}

fn note_prefix(kind: NoteKind) -> &'static str {
    match kind {
        NoteKind::Footnote => "fn",
        NoteKind::Endnote => "en",
    }
}

type TagTest = fn(&Fmt) -> bool;

const TAGS: [(TagTest, &str); 7] = [
    (|f| f.bold, "strong"),
    (|f| f.italic, "em"),
    (|f| f.underline, "u"),
    (|f| f.strike, "s"),
    (|f| f.superscript, "sup"),
    (|f| f.subscript, "sub"),
    (|f| f.mono, "code"),
];

fn flush(group: &mut String, fmt: Fmt, out: &mut String) {
    if group.is_empty() {
        return;
    }
    let tags: Vec<&str> = TAGS
        .iter()
        .filter(|(on, _)| on(&fmt))
        .map(|(_, tag)| *tag)
        .collect();
    let after_lead = group.trim_start();
    let core = after_lead.trim_end();
    if core.is_empty() || tags.is_empty() {
        let _ = escape(group, out);
        group.clear();
        return;
    }
    let lead = group.len() - after_lead.len();
    let _ = escape(&group[..lead], out);
    tags.iter().for_each(|tag| {
        out.push('<');
        out.push_str(tag);
        out.push('>');
    });
    let _ = escape(core, out);
    tags.iter().rev().for_each(|tag| {
        out.push_str("</");
        out.push_str(tag);
        out.push('>');
    });
    let _ = escape(&group[lead + core.len()..], out);
    group.clear();
}

/// Escapes text for element content and double-quoted attributes.
fn escape<W: Write>(text: &str, out: &mut W) -> fmt::Result {
    let mut last = 0;
    for (i, byte) in text.bytes().enumerate() {
        let entity = match byte {
            b'&' => "&amp;",
            b'<' => "&lt;",
            b'>' => "&gt;",
            b'"' => "&quot;",
            _ => continue,
        };
        out.write_str(&text[last..i])?;
        out.write_str(entity)?;
        last = i + 1;
    }
    out.write_str(&text[last..])
}

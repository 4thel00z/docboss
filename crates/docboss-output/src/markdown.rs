//! GitHub Flavored Markdown: headings from styles, emphasis merged across
//! runs, lists from numbering, pipe tables, footnotes and images.

use std::fmt::{self, Write};

use docboss_model::{Block, Document, Drawing, NoteKind, Paragraph, Section, Table, VerticalMerge};

use crate::walk::{self, Fmt, ListItem, Lists, Notes, Piece, Resolver};
use crate::ImageMode;

/// What Markdown output includes and how.
#[derive(Debug, Clone)]
pub struct MarkdownOptions {
    /// Start with the metadata title as a level-one heading.
    pub title: bool,
    /// Write page breaks as thematic breaks (`---`).
    pub page_breaks: bool,
    pub images: ImageMode,
    /// Write each section's headers before it and its footers after it.
    pub headers_footers: bool,
    /// Write note references as `[^1]` and the notes as definitions.
    pub notes: bool,
    /// Write comment references as `[^c1]` and the comments as definitions.
    pub comments: bool,
}

impl Default for MarkdownOptions {
    fn default() -> Self {
        Self {
            title: false,
            page_breaks: false,
            images: ImageMode::Reference {
                prefix: String::new(),
            },
            headers_footers: false,
            notes: true,
            comments: false,
        }
    }
}

/// Writes the document as Markdown to `out`.
pub fn write_markdown<W: Write>(
    doc: &Document,
    options: &MarkdownOptions,
    out: &mut W,
) -> fmt::Result {
    let mut writer = MarkdownWriter::new(doc, options);
    if options.title {
        if let Some(title) = doc
            .metadata
            .title
            .as_deref()
            .filter(|t| !t.trim().is_empty())
        {
            writer.begin(Last::Block, out)?;
            let mut escaped = String::new();
            escape(title.trim(), false, &mut escaped);
            write!(out, "# {escaped}")?;
        }
    }
    for section in &doc.sections {
        writer.section(section, out)?;
    }
    writer.close_code(out)?;
    writer.definitions(doc, out)?;
    if writer.last != Last::None {
        out.write_char('\n')?;
    }
    Ok(())
}

/// The document as a Markdown string.
pub fn to_markdown(doc: &Document, options: &MarkdownOptions) -> String {
    let mut out = String::with_capacity(crate::text::estimate(doc));
    let _ = write_markdown(doc, options, &mut out);
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Last {
    None,
    Block,
    ListItem,
}

#[derive(Debug, Clone, Copy, Default)]
struct Context {
    heading: bool,
    table: bool,
}

struct MarkdownWriter<'a, 'o> {
    resolver: Resolver<'a>,
    lists: Lists<'a>,
    notes: Notes,
    options: &'o MarkdownOptions,
    last: Last,
    in_code: bool,
    list_stack: Vec<(u8, usize)>,
    page_break: bool,
    buf: String,
    pieces: Vec<Piece<'a>>,
}

impl<'a, 'o> MarkdownWriter<'a, 'o> {
    fn new(doc: &'a Document, options: &'o MarkdownOptions) -> Self {
        Self {
            resolver: Resolver::new(doc),
            lists: Lists::new(doc),
            notes: Notes::default(),
            options,
            last: Last::None,
            in_code: false,
            list_stack: Vec::new(),
            page_break: false,
            buf: String::new(),
            pieces: Vec::new(),
        }
    }

    fn begin<W: Write>(&mut self, kind: Last, out: &mut W) -> fmt::Result {
        self.close_code(out)?;
        if kind != Last::ListItem {
            self.list_stack.clear();
        }
        match (self.last, kind) {
            (Last::None, _) => {}
            (Last::ListItem, Last::ListItem) => out.write_char('\n')?,
            _ => out.write_str("\n\n")?,
        }
        self.last = kind;
        Ok(())
    }

    fn close_code<W: Write>(&mut self, out: &mut W) -> fmt::Result {
        if !self.in_code {
            return Ok(());
        }
        self.in_code = false;
        out.write_str("\n```")
    }

    fn section<W: Write>(&mut self, section: &'a Section, out: &mut W) -> fmt::Result {
        let doc = self.resolver.doc;
        let parts = crate::section_parts(section);
        if self.options.headers_footers {
            for id in parts.headers {
                let Some(part) = doc.header_footer(id) else {
                    continue;
                };
                self.blocks(&part.blocks, out)?;
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
            self.blocks(&part.blocks, out)?;
        }
        Ok(())
    }

    fn blocks<W: Write>(&mut self, blocks: &'a [Block], out: &mut W) -> fmt::Result {
        for block in blocks {
            match block {
                Block::Paragraph(paragraph) => {
                    for body in self.paragraph(paragraph, out)? {
                        self.blocks(body, out)?;
                    }
                }
                Block::Table(table) => self.table(table, out)?,
            }
        }
        Ok(())
    }

    fn thematic_break<W: Write>(&mut self, out: &mut W) -> fmt::Result {
        self.begin(Last::Block, out)?;
        out.write_str("---")
    }

    /// Writes the paragraph and returns the bodies of the text boxes it
    /// anchors, for the caller to write after it.
    fn paragraph<W: Write>(
        &mut self,
        paragraph: &'a Paragraph,
        out: &mut W,
    ) -> Result<Vec<&'a [Block]>, fmt::Error> {
        let info = self.resolver.paragraph(paragraph);
        let mut pieces = std::mem::take(&mut self.pieces);
        walk::flatten(&mut self.resolver, paragraph, &mut pieces);
        let result = self.paragraph_pieces(paragraph, info, &pieces, out);
        let boxes = walk::text_boxes(&pieces);
        self.pieces = pieces;
        result.map(|()| boxes)
    }

    fn paragraph_pieces<W: Write>(
        &mut self,
        paragraph: &'a Paragraph,
        info: walk::ParagraphInfo,
        pieces: &[Piece<'a>],
        out: &mut W,
    ) -> fmt::Result {
        if self.options.page_breaks
            && paragraph.properties.page_break_before == Some(true)
            && self.last != Last::None
        {
            self.thematic_break(out)?;
        }
        if info.code || walk::all_monospace(pieces) {
            return self.code_line(pieces, out);
        }
        let item = info
            .numbering
            .and_then(|reference| self.lists.next(reference));
        let heading = info.heading.filter(|_| item.is_none());
        let mut buf = std::mem::take(&mut self.buf);
        buf.clear();
        self.page_break = false;
        self.inline(
            pieces,
            Context {
                heading: heading.is_some(),
                table: false,
            },
            &mut buf,
        );
        let content = buf.trim();
        if !content.is_empty() {
            match (heading, item) {
                (Some(level), _) => {
                    self.begin(Last::Block, out)?;
                    let hashes = "######".get(..usize::from(level.min(5)) + 1).unwrap_or("#");
                    write!(out, "{hashes} {content}")?;
                }
                (None, Some(item)) => self.list_item(&item, content, out)?,
                (None, None) => {
                    self.begin(Last::Block, out)?;
                    write_escaped_lines(content, "", out)?;
                }
            }
        }
        self.buf = buf;
        if self.page_break && self.options.page_breaks {
            self.thematic_break(out)?;
        }
        Ok(())
    }

    fn list_item<W: Write>(&mut self, item: &ListItem, content: &str, out: &mut W) -> fmt::Result {
        self.begin(Last::ListItem, out)?;
        while self
            .list_stack
            .last()
            .is_some_and(|&(level, _)| level >= item.level)
        {
            self.list_stack.pop();
        }
        let indent: usize = self.list_stack.iter().map(|&(_, width)| width).sum();
        let marker = list_marker(item);
        let continuation = " ".repeat(indent + marker.len() + 1);
        write!(out, "{:indent$}{marker} ", "")?;
        write_escaped_lines(content, &continuation, out)?;
        self.list_stack.push((item.level, marker.len() + 1));
        Ok(())
    }

    fn code_line<W: Write>(&mut self, pieces: &[Piece<'a>], out: &mut W) -> fmt::Result {
        let mut text = String::new();
        walk::pieces_text(pieces, &mut text);
        if !self.in_code {
            self.begin(Last::Block, out)?;
            out.write_str("```\n")?;
            self.in_code = true;
        } else {
            out.write_char('\n')?;
        }
        out.write_str(text.trim_end().trim_start_matches('\n'))
    }

    fn inline(&mut self, pieces: &[Piece<'a>], context: Context, out: &mut String) {
        let mut group = String::new();
        let mut group_fmt = Fmt::default();
        let mut link_open: Option<(usize, String)> = None;
        let mut depth = 0usize;
        for piece in pieces {
            match piece {
                Piece::Text(text, fmt) => {
                    let fmt = normalize(*fmt, context);
                    let blank = text.trim().is_empty();
                    if !group.is_empty() && fmt != group_fmt && !blank {
                        flush(&mut group, group_fmt, context, out);
                    }
                    if group.is_empty() {
                        group_fmt = fmt;
                    }
                    group.push_str(text);
                }
                Piece::Tab => group.push(' '),
                Piece::LineBreak | Piece::ColumnBreak => {
                    flush(&mut group, group_fmt, context, out);
                    out.push_str(match (context.table, context.heading) {
                        (true, _) => "<br>",
                        (_, true) => " ",
                        _ => "\\\n",
                    });
                }
                Piece::PageBreak => {
                    flush(&mut group, group_fmt, context, out);
                    self.page_break = true;
                }
                Piece::Note(kind, id) if self.options.notes => {
                    flush(&mut group, group_fmt, context, out);
                    let n = self.notes.number(*kind, *id);
                    let _ = write!(out, "[^{}]", footnote_label(*kind, n));
                }
                Piece::Comment(id) if self.options.comments => {
                    flush(&mut group, group_fmt, context, out);
                    let n = self.notes.comment_number(*id);
                    let _ = write!(out, "[^c{n}]");
                }
                Piece::Image(drawing) => {
                    flush(&mut group, group_fmt, context, out);
                    self.image(drawing, out);
                }
                Piece::Math(math) => {
                    flush(&mut group, group_fmt, context, out);
                    let fence = if math.display && !context.table {
                        "$$"
                    } else {
                        "$"
                    };
                    if out.ends_with('$') {
                        out.push(' ');
                    }
                    out.push_str(fence);
                    crate::latex::write(&math.nodes, out);
                    out.push_str(fence);
                }
                Piece::LinkStart(link) => {
                    flush(&mut group, group_fmt, context, out);
                    depth += 1;
                    if depth == 1 {
                        link_open = Some((out.len(), link.href()));
                        out.push('[');
                    }
                }
                Piece::LinkEnd => {
                    flush(&mut group, group_fmt, context, out);
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        close_link(link_open.take(), out);
                    }
                }
                _ => {}
            }
        }
        flush(&mut group, group_fmt, context, out);
        close_link(link_open.take(), out);
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
        let mut escaped = String::new();
        escape(alt.trim(), false, &mut escaped);
        match &self.options.images {
            ImageMode::Omit => {}
            ImageMode::Reference { prefix } => {
                let name = walk::media_file_name(&media.name);
                let _ = write!(out, "![{escaped}]({})", url(&format!("{prefix}{name}")));
            }
            ImageMode::Embed => {
                let _ = write!(out, "![{escaped}](data:{};base64,", media.content_type);
                walk::base64(&media.data, out);
                out.push(')');
            }
        }
    }

    fn table<W: Write>(&mut self, table: &'a Table, out: &mut W) -> fmt::Result {
        let mut rows: Vec<Vec<String>> = Vec::with_capacity(table.rows.len());
        for row in &table.rows {
            let mut cells = Vec::with_capacity(row.cells.len());
            for cell in &row.cells {
                let content = match cell.properties.vertical_merge {
                    Some(VerticalMerge::Continue) => String::new(),
                    _ => self.cell(&cell.blocks),
                };
                cells.push(content);
                cells.extend((1..cell.span()).map(|_| String::new()));
            }
            rows.push(cells);
        }
        let columns = rows
            .iter()
            .map(Vec::len)
            .max()
            .unwrap_or(0)
            .max(table.grid.len())
            .max(1);
        if rows.is_empty() {
            return Ok(());
        }
        self.begin(Last::Block, out)?;
        for (i, row) in rows.iter().enumerate() {
            if i > 0 {
                out.write_char('\n')?;
            }
            out.write_char('|')?;
            for column in 0..columns {
                let cell = row.get(column).map_or("", String::as_str);
                write!(out, " {cell} |")?;
            }
            if i == 0 {
                out.write_str("\n|")?;
                (0..columns).try_for_each(|_| out.write_str(" --- |"))?;
            }
        }
        Ok(())
    }

    fn cell(&mut self, blocks: &'a [Block]) -> String {
        let mut content = String::new();
        let context = Context {
            heading: false,
            table: true,
        };
        for block in blocks {
            let start = content.len();
            match block {
                Block::Paragraph(paragraph) => {
                    let info = self.resolver.paragraph(paragraph);
                    let mut pieces = Vec::new();
                    walk::flatten(&mut self.resolver, paragraph, &mut pieces);
                    let mut text = String::new();
                    if let Some(item) = info
                        .numbering
                        .and_then(|reference| self.lists.next(reference))
                    {
                        text.push_str(&list_marker(&item));
                        text.push(' ');
                    }
                    self.inline(&pieces, context, &mut text);
                    let text = text.trim();
                    if !text.is_empty() && start > 0 {
                        content.push_str("<br>");
                    }
                    content.push_str(text);
                }
                Block::Table(table) => {
                    for row in &table.rows {
                        let cells: Vec<String> = row
                            .cells
                            .iter()
                            .map(|cell| self.cell(&cell.blocks))
                            .filter(|text| !text.is_empty())
                            .collect();
                        if cells.is_empty() {
                            continue;
                        }
                        if !content.is_empty() {
                            content.push_str("<br>");
                        }
                        content.push_str(&cells.join(" "));
                    }
                }
            }
        }
        content.replace('\n', "<br>")
    }

    fn definitions<W: Write>(&mut self, doc: &'a Document, out: &mut W) -> fmt::Result {
        let mut index = 0;
        while index < self.notes.order.len() {
            let (kind, id) = self.notes.order[index];
            index += 1;
            let note = match kind {
                NoteKind::Footnote => doc.footnote(id),
                NoteKind::Endnote => doc.endnote(id),
            };
            let Some(note) = note else { continue };
            let label = footnote_label(kind, self.notes.number(kind, id));
            let body = self.note_body(&note.blocks);
            self.begin(Last::Block, out)?;
            write!(out, "[^{label}]: {body}")?;
        }
        if !self.options.comments {
            return Ok(());
        }
        let mut index = 0;
        while index < self.notes.comments.len() {
            let id = self.notes.comments[index];
            index += 1;
            let Some(comment) = doc.comment(id) else {
                continue;
            };
            let body = self.note_body(&comment.blocks);
            self.begin(Last::Block, out)?;
            match comment.author.as_deref() {
                Some(author) => {
                    let mut escaped = String::new();
                    escape(author, false, &mut escaped);
                    write!(out, "[^c{index}]: **{escaped}:** {body}")?;
                }
                None => write!(out, "[^c{index}]: {body}")?,
            }
        }
        Ok(())
    }

    fn note_body(&mut self, blocks: &'a [Block]) -> String {
        let mut paragraphs: Vec<String> = Vec::new();
        for block in blocks {
            let text = match block {
                Block::Paragraph(paragraph) => {
                    let mut pieces = Vec::new();
                    walk::flatten(&mut self.resolver, paragraph, &mut pieces);
                    let mut text = String::new();
                    self.inline(&pieces, Context::default(), &mut text);
                    text
                }
                Block::Table(table) => table
                    .rows
                    .iter()
                    .flat_map(|row| &row.cells)
                    .map(|cell| self.cell(&cell.blocks))
                    .collect::<Vec<_>>()
                    .join(" "),
            };
            let text = text.trim();
            if !text.is_empty() {
                paragraphs.push(text.replace('\n', "\n    "));
            }
        }
        paragraphs.join("\n\n    ")
    }
}

fn footnote_label(kind: NoteKind, n: u32) -> String {
    walk::note_label(kind, n)
}

fn list_marker(item: &ListItem) -> String {
    if item.bullet {
        return "-".to_string();
    }
    let label = item.label.trim();
    let numeric = label.len() > 1
        && label.ends_with(['.', ')'])
        && label[..label.len() - 1].bytes().all(|b| b.is_ascii_digit());
    if numeric && label.len() <= 10 {
        return label.to_string();
    }
    format!("{}.", item.ordinal)
}

fn normalize(fmt: Fmt, context: Context) -> Fmt {
    Fmt {
        bold: fmt.bold && !context.heading,
        underline: false,
        ..fmt
    }
}

fn close_link(open: Option<(usize, String)>, out: &mut String) {
    let Some((start, href)) = open else { return };
    let text = out[start + 1..].trim();
    if text.is_empty() {
        out.truncate(start);
        return;
    }
    if href.is_empty() {
        out.remove(start);
        return;
    }
    let _ = write!(out, "]({})", url(&href));
}

fn url(href: &str) -> String {
    let needs_brackets = href.contains([' ', '(', ')']) || href.is_empty();
    if needs_brackets {
        return format!("<{}>", href.replace('<', "%3C").replace('>', "%3E"));
    }
    href.to_string()
}

fn flush(group: &mut String, fmt: Fmt, context: Context, out: &mut String) {
    if group.is_empty() {
        return;
    }
    let after_lead = group.trim_start();
    let lead = group.len() - after_lead.len();
    let core = after_lead.trim_end();
    if core.is_empty() {
        out.push_str(group);
        group.clear();
        return;
    }
    let trail_start = lead + core.len();
    out.push_str(&group[..lead]);
    if fmt.mono {
        code_span(core, out);
    } else {
        let emphasis = match (fmt.bold, fmt.italic) {
            (true, true) => "***",
            (true, false) => "**",
            (false, true) => "*",
            (false, false) => "",
        };
        let strike = if fmt.strike { "~~" } else { "" };
        if fmt.superscript {
            out.push_str("<sup>");
        }
        if fmt.subscript {
            out.push_str("<sub>");
        }
        out.push_str(strike);
        out.push_str(emphasis);
        escape(core, context.table, out);
        out.push_str(emphasis);
        out.push_str(strike);
        if fmt.subscript {
            out.push_str("</sub>");
        }
        if fmt.superscript {
            out.push_str("</sup>");
        }
    }
    out.push_str(&group[trail_start..]);
    group.clear();
}

fn code_span(text: &str, out: &mut String) {
    let mut longest = 0;
    let mut current = 0;
    for c in text.chars() {
        current = if c == '`' { current + 1 } else { 0 };
        longest = longest.max(current);
    }
    let fence = "`".repeat(longest + 1);
    let pad = if text.starts_with('`') || text.ends_with('`') {
        " "
    } else {
        ""
    };
    out.push_str(&fence);
    out.push_str(pad);
    out.push_str(text);
    out.push_str(pad);
    out.push_str(&fence);
}

/// Escapes the characters that start Markdown inline syntax. Underscores
/// between two alphanumerics cannot open emphasis and are kept bare.
fn escape(text: &str, table: bool, out: &mut String) {
    let plain = !text.bytes().any(|b| {
        matches!(
            b,
            b'\\' | b'`' | b'*' | b'[' | b']' | b'<' | b'~' | b'_' | b'|'
        )
    });
    if plain {
        out.push_str(text);
        return;
    }
    out.reserve(text.len() + 8);
    let mut previous: Option<char> = None;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        let special = match c {
            '\\' | '`' | '*' | '[' | ']' | '<' | '~' => true,
            '_' => {
                let inside = previous.is_some_and(char::is_alphanumeric)
                    && chars.peek().is_some_and(|next| next.is_alphanumeric());
                !inside
            }
            '|' => table,
            _ => false,
        };
        if special {
            out.push('\\');
        }
        out.push(c);
        previous = Some(c);
    }
}

/// Writes lines, escaping a leading `#`, `>`, `-`, `+`, `=` or `1.` that
/// would start a block, and indenting every line after the first.
fn write_escaped_lines<W: Write>(text: &str, indent: &str, out: &mut W) -> fmt::Result {
    for (i, line) in text.split('\n').enumerate() {
        if i > 0 {
            out.write_char('\n')?;
            out.write_str(indent)?;
        }
        let line = if i > 0 { line.trim_start() } else { line };
        write_line_start_escaped(line, out)?;
    }
    Ok(())
}

fn write_line_start_escaped<W: Write>(line: &str, out: &mut W) -> fmt::Result {
    if line.starts_with(['#', '>', '-', '+', '=']) {
        out.write_char('\\')?;
        return out.write_str(line);
    }
    let digits = line.bytes().take_while(u8::is_ascii_digit).count();
    let marker = line.as_bytes().get(digits).copied();
    let follows = line.as_bytes().get(digits + 1).copied();
    let ordered = digits > 0
        && digits <= 9
        && matches!(marker, Some(b'.' | b')'))
        && follows.is_none_or(|b| b == b' ' || b == b'\t');
    if !ordered {
        return out.write_str(line);
    }
    out.write_str(&line[..digits])?;
    out.write_char('\\')?;
    out.write_str(&line[digits..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escaping_keeps_intraword_underscores() {
        let mut out = String::new();
        escape("snake_case and _lead *star* [x] a|b", true, &mut out);
        assert_eq!(out, "snake_case and \\_lead \\*star\\* \\[x\\] a\\|b");
    }

    #[test]
    fn line_starts_are_escaped() {
        let mut out = String::new();
        write_escaped_lines("# not a heading\n1. not a list\n2024 was", "", &mut out).unwrap();
        assert_eq!(out, "\\# not a heading\n1\\. not a list\n2024 was");
    }

    #[test]
    fn code_spans_grow_fences() {
        let mut out = String::new();
        code_span("a `b` c", &mut out);
        assert_eq!(out, "``a `b` c``");
    }
}

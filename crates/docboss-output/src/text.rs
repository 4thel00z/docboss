//! Plain text: one line per paragraph, list labels in front, table rows as
//! tab-separated cells, notes and comments appended.

use std::fmt::{self, Write};

use docboss_model::{Block, Document, NoteKind, Paragraph, Section};

use crate::walk::{self, Lists, Notes, Piece, Resolver};

/// What plain text output includes.
#[derive(Debug, Clone)]
pub struct TextOptions {
    /// Prefix list paragraphs with their labels, such as `1.` or `•`.
    pub list_labels: bool,
    /// Write each section's headers before it and its footers after it.
    pub headers_footers: bool,
    /// Mark note references as `[1]` and append the notes' text.
    pub notes: bool,
    /// Mark comment references as `[c1]` and append the comments.
    pub comments: bool,
}

impl Default for TextOptions {
    fn default() -> Self {
        Self {
            list_labels: true,
            headers_footers: false,
            notes: true,
            comments: false,
        }
    }
}

/// Writes the document's text to `out`.
pub fn write_text<W: Write>(doc: &Document, options: &TextOptions, out: &mut W) -> fmt::Result {
    let mut writer = TextWriter {
        resolver: Resolver::new(doc),
        lists: Lists::new(doc),
        notes: Notes::default(),
        options,
        pieces: Vec::new(),
        line: String::new(),
    };
    for section in &doc.sections {
        writer.section(section, out)?;
    }
    writer.appendix(doc, out)
}

/// The document's text as a string.
pub fn to_text(doc: &Document, options: &TextOptions) -> String {
    let mut out = String::with_capacity(estimate(doc));
    let _ = write_text(doc, options, &mut out);
    out
}

pub(crate) fn estimate(doc: &Document) -> usize {
    doc.sections
        .iter()
        .map(|section| section.blocks.len() * 80)
        .sum::<usize>()
        + 64
}

struct TextWriter<'a, 'o> {
    resolver: Resolver<'a>,
    lists: Lists<'a>,
    notes: Notes,
    options: &'o TextOptions,
    pieces: Vec<Piece<'a>>,
    line: String,
}

impl<'a> TextWriter<'a, '_> {
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
            self.line.clear();
            let boxes = match block {
                Block::Paragraph(paragraph) => self.paragraph(paragraph),
                Block::Table(table) => {
                    for (i, row) in table.rows.iter().enumerate() {
                        if i > 0 {
                            self.line.push('\n');
                        }
                        for (j, cell) in row.cells.iter().enumerate() {
                            if j > 0 {
                                self.line.push('\t');
                            }
                            self.cell(&cell.blocks);
                        }
                    }
                    Vec::new()
                }
            };
            out.write_str(&self.line)?;
            out.write_char('\n')?;
            for body in boxes {
                self.blocks(body, out)?;
            }
        }
        Ok(())
    }

    fn cell(&mut self, blocks: &'a [Block]) {
        for (i, block) in blocks.iter().enumerate() {
            if i > 0 {
                self.line.push(' ');
            }
            match block {
                Block::Paragraph(paragraph) => {
                    let start = self.line.len();
                    let boxes = self.paragraph(paragraph);
                    let flattened = self.line[start..].replace(['\n', '\t'], " ");
                    self.line.truncate(start);
                    self.line.push_str(&flattened);
                    for body in boxes {
                        self.line.push(' ');
                        self.cell(body);
                    }
                }
                Block::Table(table) => {
                    for row in &table.rows {
                        for cell in &row.cells {
                            self.cell(&cell.blocks);
                            self.line.push(' ');
                        }
                    }
                }
            }
        }
    }

    /// Writes the paragraph to the line and returns the bodies of the text
    /// boxes it anchors, for the caller to write after it.
    fn paragraph(&mut self, paragraph: &'a Paragraph) -> Vec<&'a [Block]> {
        let info = self.resolver.paragraph(paragraph);
        let item = info
            .numbering
            .and_then(|reference| self.lists.next(reference));
        if let Some(item) = item.filter(|_| self.options.list_labels) {
            self.line.push_str(&item.label);
            self.line.push(' ');
        }
        let mut pieces = std::mem::take(&mut self.pieces);
        walk::flatten(&mut self.resolver, paragraph, &mut pieces);
        for piece in &pieces {
            match piece {
                Piece::Text(text, _) => self.line.push_str(text),
                Piece::Tab => self.line.push('\t'),
                Piece::LineBreak | Piece::PageBreak | Piece::ColumnBreak => self.line.push('\n'),
                Piece::Note(kind, id) if self.options.notes => {
                    let n = self.notes.number(*kind, *id);
                    let _ = write!(self.line, "[{}]", walk::note_label(*kind, n));
                }
                Piece::Comment(id) if self.options.comments => {
                    let n = self.notes.comment_number(*id);
                    let _ = write!(self.line, "[c{n}]");
                }
                _ => {}
            }
        }
        let boxes = walk::text_boxes(&pieces);
        self.pieces = pieces;
        boxes
    }

    fn body(&mut self, blocks: &'a [Block]) -> String {
        let mut text = String::new();
        for block in blocks {
            self.line.clear();
            match block {
                Block::Paragraph(paragraph) => {
                    for body in self.paragraph(paragraph) {
                        self.line.push(' ');
                        self.cell(body);
                    }
                }
                Block::Table(table) => {
                    for row in &table.rows {
                        for cell in &row.cells {
                            self.cell(&cell.blocks);
                            self.line.push('\t');
                        }
                    }
                }
            }
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(self.line.trim());
        }
        text
    }

    fn appendix<W: Write>(&mut self, doc: &'a Document, out: &mut W) -> fmt::Result {
        let mut index = 0;
        while index < self.notes.order.len() {
            let (kind, id) = self.notes.order[index];
            index += 1;
            let note = match kind {
                NoteKind::Footnote => doc.footnote(id),
                NoteKind::Endnote => doc.endnote(id),
            };
            let Some(note) = note else { continue };
            if index == 1 {
                out.write_char('\n')?;
            }
            let label = walk::note_label(kind, self.notes.number(kind, id));
            let body = self.body(&note.blocks);
            writeln!(out, "[{label}] {body}")?;
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
            if index == 1 {
                out.write_char('\n')?;
            }
            let body = self.body(&comment.blocks);
            match comment.author.as_deref() {
                Some(author) => writeln!(out, "[c{index}] {author}: {body}")?,
                None => writeln!(out, "[c{index}] {body}")?,
            }
        }
        Ok(())
    }
}

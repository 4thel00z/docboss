//! The inspector: what the selected node holds, with paragraph and run
//! properties shown both as written and as resolved through the styles.

use std::fmt::Debug;

use docboss_model::{Block, Document, Inline, StyleKind};

use crate::container::Part;
use crate::tree::{paragraph_of, resolve, NodeKind, Resolved, Sources, Story};

/// A line of inspector text; headings are drawn bold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Line {
    Heading(String),
    Text(String),
}

fn is_unstated(line: &str) -> bool {
    let trimmed = line.trim_end();
    ["None,", "[],", "false,", "\"\","]
        .iter()
        .any(|empty| trimmed.ends_with(&format!(": {empty}")))
}

fn opens_block(line: &str) -> bool {
    let trimmed = line.trim_end();
    trimmed.ends_with('{') || trimmed.ends_with('[') || trimmed.ends_with('(')
}

fn closes_block(line: &str) -> bool {
    matches!(line.trim(), "}," | "}" | "]," | "]" | ")," | ")")
}

/// The pretty `Debug` form of a value with every unstated field (`None`,
/// empty lists, `false`) left out, and structs left empty by that removed.
pub fn stated(value: &impl Debug) -> Vec<String> {
    let mut lines: Vec<String> = format!("{value:#?}")
        .lines()
        .filter(|line| !is_unstated(line))
        .map(str::to_string)
        .collect();
    loop {
        let empty = lines
            .windows(2)
            .skip(1)
            .position(|pair| opens_block(&pair[0]) && closes_block(&pair[1]));
        let Some(at) = empty else {
            break;
        };
        lines.drain(at + 1..at + 3);
    }
    inline_wrappers(lines)
}

/// Joins a wrapper around one scalar, `Some(` / `0,` / `),`, into
/// `Some(0),` so the inspector spends one line per value.
fn inline_wrappers(lines: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut rest = lines.into_iter().peekable();
    while let Some(line) = rest.next() {
        if !line.trim_end().ends_with('(') {
            out.push(line);
            continue;
        }
        let mut window: Vec<String> = rest.by_ref().take(2).collect();
        let joinable =
            window.len() == 2 && !opens_block(&window[0]) && matches!(window[1].trim(), ")," | ")");
        if !joinable {
            out.push(line);
            out.append(&mut window);
            continue;
        }
        let value = window[0].trim().trim_end_matches(',');
        out.push(format!("{}{value}{}", line.trim_end(), window[1].trim()));
    }
    out
}

fn section(out: &mut Vec<Line>, heading: &str, lines: impl IntoIterator<Item = String>) {
    out.push(Line::Heading(heading.to_string()));
    out.extend(
        lines
            .into_iter()
            .map(|line| Line::Text(format!("  {line}"))),
    );
}

fn style_chain(doc: &Document, id: &str) -> String {
    let chain: Vec<&str> = doc
        .styles
        .chain(id)
        .iter()
        .map(|style| style.id.as_str())
        .collect();
    if chain.is_empty() {
        return format!("{id} (not in the style sheet)");
    }
    chain.join(" \u{2192} ")
}

fn document_lines(doc: &Document, parts: &[Part], out: &mut Vec<Line>) {
    let blocks = doc.blocks().count();
    section(
        out,
        "Document",
        [
            format!("format: {:?}", doc.format),
            format!("sections: {}", doc.sections.len()),
            format!("blocks in the main text: {blocks}"),
            format!("styles: {}", doc.styles.styles.len()),
            format!("list definitions: {}", doc.numbering.abstracts.len()),
            format!(
                "footnotes: {}, endnotes: {}, comments: {}",
                doc.footnotes.len(),
                doc.endnotes.len(),
                doc.comments.len()
            ),
            format!(
                "media: {}, fonts: {}, parts: {}",
                doc.media.len(),
                doc.fonts.len(),
                parts.len()
            ),
            format!("diagnostics: {}", doc.diagnostics.len()),
        ],
    );
    section(out, "Metadata", stated(&doc.metadata));
    section(out, "Settings", stated(&doc.settings));
    section(
        out,
        "Default paragraph properties",
        stated(&doc.styles.default_paragraph),
    );
    section(
        out,
        "Default run properties",
        stated(&doc.styles.default_run),
    );
}

fn paragraph_lines(doc: &Document, paragraph: &docboss_model::Paragraph, out: &mut Vec<Line>) {
    let style = paragraph.style_id.as_deref().or_else(|| {
        doc.styles
            .default_of(StyleKind::Paragraph)
            .map(|style| style.id.as_str())
    });
    let mut summary = vec![format!(
        "style: {}",
        style.map_or_else(|| "(none)".to_string(), |id| style_chain(doc, id))
    )];
    if let Some(level) = doc.styles.heading_level(paragraph, &doc.numbering) {
        summary.push(format!("heading level: {}", level + 1));
    }
    let resolved = doc.styles.resolve_paragraph(paragraph, &doc.numbering);
    if let Some(list) = resolved.numbering.filter(|list| list.num_id != 0) {
        summary.push(format!("list: num {} level {}", list.num_id, list.level));
    }
    summary.push(format!("inlines: {}", paragraph.inlines.len()));
    section(out, "Paragraph", summary);
    section(
        out,
        "Text",
        paragraph
            .text()
            .lines()
            .map(str::to_string)
            .collect::<Vec<_>>(),
    );
    section(out, "Direct properties", stated(&paragraph.properties));
    section(out, "Resolved properties", stated(&resolved));
    section(out, "Paragraph mark (direct)", stated(&paragraph.mark));
}

fn inline_lines(
    doc: &Document,
    story: Story,
    steps: &[crate::tree::Step],
    inline: &Inline,
    out: &mut Vec<Line>,
) {
    let Inline::Run(run) = inline else {
        section(out, "Inline", stated(inline));
        return;
    };
    let paragraph_style = paragraph_of(doc, story, steps).and_then(|p| p.style_id.as_deref());
    let mut summary = Vec::new();
    if let Some(id) = paragraph_style {
        summary.push(format!("paragraph style: {}", style_chain(doc, id)));
    }
    if let Some(id) = run.properties.style_id.as_deref() {
        summary.push(format!("character style: {}", style_chain(doc, id)));
    }
    summary.push(format!("content items: {}", run.content.len()));
    section(out, "Run", summary);
    section(
        out,
        "Content",
        run.content.iter().flat_map(stated).collect::<Vec<_>>(),
    );
    section(out, "Direct properties", stated(&run.properties));
    section(
        out,
        "Resolved properties",
        stated(&doc.styles.resolve_run(paragraph_style, &run.properties)),
    );
}

fn content_lines(doc: &Document, story: Story, steps: &[crate::tree::Step], out: &mut Vec<Line>) {
    let Some(resolved) = resolve(doc, story, steps) else {
        section(out, "Missing", ["the path no longer resolves".to_string()]);
        return;
    };
    match resolved {
        Resolved::Block(Block::Paragraph(paragraph)) => paragraph_lines(doc, paragraph, out),
        Resolved::Block(Block::Table(table)) => {
            section(
                out,
                "Table",
                [
                    format!("rows: {}", table.rows.len()),
                    format!("grid (twips): {:?}", table.grid),
                    format!(
                        "style: {}",
                        table
                            .properties
                            .style_id
                            .as_deref()
                            .map_or_else(|| "(none)".to_string(), |id| style_chain(doc, id))
                    ),
                ],
            );
            section(out, "Properties", stated(&table.properties));
        }
        Resolved::Row(row) => {
            section(out, "Row", [format!("cells: {}", row.cells.len())]);
            section(out, "Properties", stated(&row.properties));
        }
        Resolved::Cell(cell) => {
            section(
                out,
                "Cell",
                [
                    format!("blocks: {}", cell.blocks.len()),
                    format!("span: {}", cell.span()),
                ],
            );
            section(out, "Properties", stated(&cell.properties));
        }
        Resolved::Inline(inline) => inline_lines(doc, story, steps, inline, out),
    }
}

fn story_lines(doc: &Document, story: Story, out: &mut Vec<Line>) {
    match story {
        Story::Section(i) => {
            if let Some(s) = doc.sections.get(i) {
                section(out, "Section", [format!("blocks: {}", s.blocks.len())]);
                section(out, "Properties", stated(&s.properties));
            }
        }
        Story::HeaderFooter(i) => {
            if let Some(part) = doc.headers_footers.get(i) {
                section(
                    out,
                    "Header or footer",
                    [
                        format!("id: {}", part.id),
                        format!("kind: {:?}", part.kind),
                        format!("blocks: {}", part.blocks.len()),
                    ],
                );
            }
        }
        Story::Footnote(i) | Story::Endnote(i) => {
            let notes = if matches!(story, Story::Footnote(_)) {
                &doc.footnotes
            } else {
                &doc.endnotes
            };
            if let Some(note) = notes.get(i) {
                section(
                    out,
                    "Note",
                    [
                        format!("id: {}", note.id),
                        format!("kind: {:?}", note.kind),
                        format!("blocks: {}", note.blocks.len()),
                    ],
                );
            }
        }
        Story::Comment(i) => {
            if let Some(comment) = doc.comments.get(i) {
                section(
                    out,
                    "Comment",
                    [
                        format!("id: {}", comment.id),
                        format!("author: {}", comment.author.as_deref().unwrap_or("")),
                        format!("initials: {}", comment.initials.as_deref().unwrap_or("")),
                        format!("date: {}", comment.date.as_deref().unwrap_or("")),
                    ],
                );
            }
        }
    }
}

/// The inspector lines for a node.
pub fn lines(sources: &Sources<'_>, kind: &NodeKind) -> Vec<Line> {
    let doc = sources.doc;
    let mut out = Vec::new();
    match kind {
        NodeKind::Document => document_lines(doc, sources.parts, &mut out),
        NodeKind::Group(_) => section(&mut out, "Group", [crate::tree::label(sources, kind)]),
        NodeKind::Story(story) => story_lines(doc, *story, &mut out),
        NodeKind::Content(story, steps) => content_lines(doc, *story, steps, &mut out),
        NodeKind::Style(i) => {
            if let Some(style) = doc.styles.styles.get(*i) {
                section(
                    &mut out,
                    "Style",
                    [format!("chain: {}", style_chain(doc, &style.id))],
                );
                section(&mut out, "Definition", stated(style));
            }
        }
        NodeKind::Abstract(i) => {
            if let Some(definition) = doc.numbering.abstracts.get(*i) {
                section(&mut out, "List definition", stated(definition));
            }
        }
        NodeKind::Instance(i) => {
            if let Some(instance) = doc.numbering.instances.get(*i) {
                section(&mut out, "Numbering instance", stated(instance));
            }
        }
        NodeKind::Media(i) => {
            if let Some(media) = doc.media.get(*i) {
                section(
                    &mut out,
                    "Media",
                    [
                        format!("name: {}", media.name),
                        format!("type: {}", media.content_type),
                        format!("bytes: {}", media.data.len()),
                    ],
                );
            }
        }
        NodeKind::Font(i) => {
            if let Some(font) = doc.fonts.get(*i) {
                section(&mut out, "Font", stated(font));
            }
        }
        NodeKind::Part(i) => {
            if let Some(part) = sources.parts.get(*i) {
                let mut summary = vec![
                    format!("path: {}", part.display_name()),
                    format!("size: {}", part.size),
                ];
                if let Some(stored) = part.stored {
                    summary.push(format!("stored: {stored}"));
                }
                if part.is_xml() {
                    summary.push("press x for the XML view".to_string());
                }
                section(&mut out, "Part", summary);
            }
        }
        NodeKind::Diagnostic(i) => {
            if let Some(diagnostic) = doc.diagnostics.get(*i) {
                section(
                    &mut out,
                    "Diagnostic",
                    [
                        format!("severity: {:?}", diagnostic.severity),
                        format!("location: {}", diagnostic.location),
                        format!("message: {}", diagnostic.message),
                    ],
                );
            }
        }
    }
    out
}

/// The inspector as plain text, for copying.
pub fn plain(lines: &[Line]) -> String {
    let mut out = String::new();
    for line in lines {
        let text = match line {
            Line::Heading(text) | Line::Text(text) => text,
        };
        out.push_str(text);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::stated;

    #[allow(dead_code)]
    #[derive(Debug, Default)]
    struct Inner {
        a: Option<u8>,
        b: Option<u8>,
    }

    #[allow(dead_code)]
    #[derive(Debug, Default)]
    struct Outer {
        name: Option<String>,
        inner: Inner,
        flag: bool,
        count: u8,
    }

    #[test]
    fn unstated_fields_and_emptied_structs_are_left_out() {
        let value = Outer {
            count: 3,
            ..Outer::default()
        };
        let text = stated(&value).join("\n");
        assert!(text.contains("count: 3"));
        assert!(!text.contains("name"));
        assert!(!text.contains("inner"));
        assert!(!text.contains("flag"));

        let value = Outer {
            inner: Inner {
                a: Some(1),
                b: None,
            },
            ..Outer::default()
        };
        let text = stated(&value).join("\n");
        assert!(text.contains("a: Some(1),"), "{text}");
        assert!(!text.contains("b:"));
    }
}

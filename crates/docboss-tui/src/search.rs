//! Search over the whole document, collapsed nodes included: paragraph
//! text in every story, then style ids and names, parts, media, fonts and
//! diagnostics. Matching ignores case.

use docboss_model::{Block, Document};

use crate::container::Part;
use crate::tree::{NodeKind, Step, Story};

fn blocks_hits(
    blocks: &[Block],
    story: Story,
    prefix: &mut Vec<Step>,
    needle: &str,
    hits: &mut Vec<NodeKind>,
) {
    for (i, block) in blocks.iter().enumerate() {
        prefix.push(Step::Block(i));
        match block {
            Block::Paragraph(paragraph) => {
                if paragraph.text().to_lowercase().contains(needle) {
                    hits.push(NodeKind::Content(story, prefix.clone()));
                }
            }
            Block::Table(table) => {
                for (r, row) in table.rows.iter().enumerate() {
                    for (c, cell) in row.cells.iter().enumerate() {
                        prefix.extend([Step::Row(r), Step::Cell(c)]);
                        blocks_hits(&cell.blocks, story, prefix, needle, hits);
                        prefix.truncate(prefix.len() - 2);
                    }
                }
            }
        }
        prefix.pop();
    }
}

fn contains(haystack: &str, needle: &str) -> bool {
    haystack.to_lowercase().contains(needle)
}

/// Every node matching `query`, in tree order.
pub fn search(doc: &Document, parts: &[Part], query: &str) -> Vec<NodeKind> {
    let needle = query.to_lowercase();
    if needle.is_empty() {
        return Vec::new();
    }
    let mut hits = Vec::new();
    let mut prefix = Vec::new();
    let stories = [
        (doc.sections.len(), Story::Section as fn(usize) -> Story),
        (doc.headers_footers.len(), Story::HeaderFooter),
        (doc.footnotes.len(), Story::Footnote),
        (doc.endnotes.len(), Story::Endnote),
        (doc.comments.len(), Story::Comment),
    ];
    for (count, make) in stories {
        for i in 0..count {
            let story = make(i);
            let blocks = crate::tree::story_blocks(doc, story).unwrap_or_default();
            blocks_hits(blocks, story, &mut prefix, &needle, &mut hits);
        }
    }
    hits.extend(
        doc.styles
            .styles
            .iter()
            .enumerate()
            .filter(|(_, style)| {
                contains(&style.id, &needle)
                    || style
                        .name
                        .as_deref()
                        .is_some_and(|name| contains(name, &needle))
            })
            .map(|(i, _)| NodeKind::Style(i)),
    );
    hits.extend(
        parts
            .iter()
            .enumerate()
            .filter(|(_, part)| contains(&part.display_name(), &needle))
            .map(|(i, _)| NodeKind::Part(i)),
    );
    hits.extend(
        doc.media
            .iter()
            .enumerate()
            .filter(|(_, media)| contains(&media.name, &needle))
            .map(|(i, _)| NodeKind::Media(i)),
    );
    hits.extend(
        doc.fonts
            .iter()
            .enumerate()
            .filter(|(_, font)| contains(&font.name, &needle))
            .map(|(i, _)| NodeKind::Font(i)),
    );
    hits.extend(
        doc.diagnostics
            .iter()
            .enumerate()
            .filter(|(_, d)| contains(&d.message, &needle) || contains(&d.location, &needle))
            .map(|(i, _)| NodeKind::Diagnostic(i)),
    );
    hits
}

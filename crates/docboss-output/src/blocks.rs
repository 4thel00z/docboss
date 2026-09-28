//! A flat view of the main text, one entry per paragraph or table, for
//! chunking and retrieval pipelines.

use docboss_model::{Block, Document};

use crate::walk::{self, Lists, Piece, Resolver};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum BlockKind {
    Paragraph,
    Heading,
    ListItem,
    Table,
}

/// One block of the main text with its role and plain text.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct BlockView {
    pub kind: BlockKind,
    /// Index of the section the block belongs to.
    pub section: usize,
    pub style: Option<String>,
    /// 1 for `Heading 1`.
    pub heading_level: Option<u8>,
    /// 0 for the outermost list level.
    pub list_level: Option<u8>,
    pub list_label: Option<String>,
    /// The text; tables give one line per row with tab-separated cells.
    pub text: String,
}

/// The main text as a flat list of blocks, empty paragraphs left out.
pub fn blocks_view(doc: &Document) -> Vec<BlockView> {
    let mut resolver = Resolver::new(doc);
    let mut lists = Lists::new(doc);
    let mut pieces: Vec<Piece<'_>> = Vec::new();
    let mut views = Vec::new();
    for (section, part) in doc.sections.iter().enumerate() {
        push_blocks(
            &mut resolver,
            &mut lists,
            &mut pieces,
            section,
            &part.blocks,
            &mut views,
        );
    }
    views
}

/// Appends the views of `blocks`, each paragraph followed by the blocks of
/// the text boxes it anchors.
fn push_blocks<'a>(
    resolver: &mut Resolver<'a>,
    lists: &mut Lists<'a>,
    pieces: &mut Vec<Piece<'a>>,
    section: usize,
    blocks: &'a [Block],
    views: &mut Vec<BlockView>,
) {
    for block in blocks {
        let Block::Paragraph(paragraph) = block else {
            let Block::Table(table) = block else { continue };
            let text = table
                .rows
                .iter()
                .map(|row| {
                    row.cells
                        .iter()
                        .map(|cell| cell_text(resolver, &cell.blocks))
                        .collect::<Vec<_>>()
                        .join("\t")
                })
                .collect::<Vec<_>>()
                .join("\n");
            views.push(BlockView {
                kind: BlockKind::Table,
                section,
                style: table.properties.style_id.clone(),
                heading_level: None,
                list_level: None,
                list_label: None,
                text,
            });
            continue;
        };
        let info = resolver.paragraph(paragraph);
        let item = info.numbering.and_then(|reference| lists.next(reference));
        walk::flatten(resolver, paragraph, pieces);
        let mut text = String::new();
        walk::pieces_text(pieces, &mut text);
        let text = text.trim().to_string();
        let boxes = walk::text_boxes(pieces);
        if text.is_empty() {
            boxes
                .into_iter()
                .for_each(|body| push_blocks(resolver, lists, pieces, section, body, views));
            continue;
        }
        let heading = info.heading.filter(|_| item.is_none());
        let kind = match (heading, &item) {
            (Some(_), _) => BlockKind::Heading,
            (None, Some(_)) => BlockKind::ListItem,
            (None, None) => BlockKind::Paragraph,
        };
        views.push(BlockView {
            kind,
            section,
            style: paragraph.style_id.clone(),
            heading_level: heading.map(|level| level + 1),
            list_level: item.as_ref().map(|item| item.level),
            list_label: item.map(|item| item.label),
            text,
        });
        boxes
            .into_iter()
            .for_each(|body| push_blocks(resolver, lists, pieces, section, body, views));
    }
}

fn cell_text<'a>(resolver: &mut Resolver<'a>, blocks: &'a [Block]) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut pieces = Vec::new();
    for block in blocks {
        match block {
            Block::Paragraph(paragraph) => {
                walk::flatten(resolver, paragraph, &mut pieces);
                let mut text = String::new();
                walk::pieces_text(&pieces, &mut text);
                let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
                if !text.is_empty() {
                    parts.push(text);
                }
            }
            Block::Table(table) => {
                let nested = table
                    .rows
                    .iter()
                    .flat_map(|row| &row.cells)
                    .map(|cell| cell_text(resolver, &cell.blocks))
                    .filter(|text| !text.is_empty())
                    .collect::<Vec<_>>();
                parts.extend(nested);
            }
        }
    }
    parts.join(" ")
}

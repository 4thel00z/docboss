//! `docboss fonts`: the fonts a document asks for and the faces they
//! resolve to on this machine.

use std::collections::BTreeSet;

use docboss_core::{Block, Document, FontSlots, Inline, RunProperties};
use docboss_font::FontDatabase;

/// One requested family and what it resolved to.
pub struct Resolution {
    pub requested: String,
    pub face: Option<String>,
    pub embedded: bool,
}

fn slot_names(slots: &FontSlots, out: &mut BTreeSet<String>) {
    [
        &slots.ascii,
        &slots.high_ansi,
        &slots.east_asia,
        &slots.complex,
    ]
    .into_iter()
    .flatten()
    .filter(|name| !name.trim().is_empty())
    .for_each(|name| {
        out.insert(name.clone());
    });
}

fn run_names(properties: &RunProperties, out: &mut BTreeSet<String>) {
    slot_names(&properties.fonts, out);
}

fn inline_names(inlines: &[Inline], out: &mut BTreeSet<String>) {
    for inline in inlines {
        match inline {
            Inline::Run(run) => run_names(&run.properties, out),
            Inline::Hyperlink(link) => inline_names(&link.inlines, out),
            Inline::Field(field) => inline_names(&field.result, out),
            Inline::Revision(revision) => inline_names(&revision.inlines, out),
            _ => {}
        }
    }
}

fn block_names(blocks: &[Block], out: &mut BTreeSet<String>) {
    for block in blocks {
        match block {
            Block::Paragraph(paragraph) => {
                run_names(&paragraph.mark, out);
                inline_names(&paragraph.inlines, out);
            }
            Block::Table(table) => table
                .rows
                .iter()
                .flat_map(|row| row.cells.iter())
                .for_each(|cell| block_names(&cell.blocks, out)),
        }
    }
}

/// Every family the document names: its font table, its styles and
/// document defaults, and the runs of every story.
pub fn requested(document: &Document) -> BTreeSet<String> {
    let mut names: BTreeSet<String> = document
        .fonts
        .iter()
        .map(|font| font.name.clone())
        .collect();
    run_names(&document.styles.default_run, &mut names);
    document
        .styles
        .styles
        .iter()
        .for_each(|style| run_names(&style.run, &mut names));
    document
        .sections
        .iter()
        .for_each(|section| block_names(&section.blocks, &mut names));
    document
        .headers_footers
        .iter()
        .for_each(|part| block_names(&part.blocks, &mut names));
    document
        .footnotes
        .iter()
        .chain(&document.endnotes)
        .for_each(|note| block_names(&note.blocks, &mut names));
    names.retain(|name| !name.trim().is_empty());
    names
}

/// Resolves every requested family against `fonts`, regular weight.
pub fn resolve(document: &Document, fonts: &FontDatabase) -> Vec<Resolution> {
    requested(document)
        .into_iter()
        .map(|requested| {
            let class = document
                .fonts
                .iter()
                .find(|font| font.name == requested)
                .and_then(|font| font.family.as_deref());
            let info = fonts
                .select(&requested, class, false, false)
                .and_then(|id| fonts.info(id));
            Resolution {
                face: info.map(|info| info.family.clone()),
                embedded: info.is_some_and(|info| info.embedded),
                requested,
            }
        })
        .collect()
}

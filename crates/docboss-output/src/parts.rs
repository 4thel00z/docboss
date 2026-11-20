//! Output split into parts: runs of top-level body blocks, such as the
//! blocks that start on each laid-out page.

use std::fmt;
use std::ops::Range;

use docboss_model::{Block, Document};

/// A run of top-level body blocks written as one output, with the header
/// and footer shown beside it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Part {
    /// Indices among the body blocks of every section, in order.
    pub blocks: Range<usize>,
    /// The id of the header part written before the blocks when headers
    /// and footers are asked for.
    pub header: Option<String>,
    /// The id of the footer part written after the blocks when headers and
    /// footers are asked for.
    pub footer: Option<String>,
}

/// A writer that can stop and start again at part boundaries while list
/// counters and note numbers carry on across them.
pub(crate) trait PartWriter<'a> {
    /// Forgets the layout state of the part before, such as open lists.
    fn reset(&mut self) {}
    /// Writes what opens the document, such as its title, into the part
    /// that starts it.
    fn opening(&mut self, out: &mut String) -> fmt::Result {
        let _ = out;
        Ok(())
    }
    /// Starts a part with its header.
    fn begin(&mut self, part: &Part, out: &mut String) -> fmt::Result;
    /// Writes one top-level body block.
    fn block(&mut self, block: &'a Block, out: &mut String) -> fmt::Result;
    /// Ends a part: closes open blocks and writes its footer and the notes
    /// and comments first referenced in it. With no part, the blocks since
    /// the last part were skipped and their notes are left out.
    fn end(&mut self, part: Option<&Part>, out: &mut String) -> fmt::Result;
}

/// Writes each part to its own string, in the order given. Parts are
/// written in document order in one pass over the body; blocks before a
/// part are walked but not kept, so numbering matches the whole document.
/// A block in two parts is written only into the earlier one.
pub(crate) fn write_parts<'a>(
    doc: &'a Document,
    parts: &[Part],
    writer: &mut impl PartWriter<'a>,
) -> Vec<String> {
    let mut outputs: Vec<String> = parts.iter().map(|_| String::new()).collect();
    if write_sorted(doc, parts, writer, &mut outputs).is_err() {
        outputs.iter_mut().for_each(String::clear);
    }
    outputs
}

fn write_sorted<'a>(
    doc: &'a Document,
    parts: &[Part],
    writer: &mut impl PartWriter<'a>,
    outputs: &mut [String],
) -> fmt::Result {
    let mut sorted: Vec<(usize, &Part)> = parts.iter().enumerate().collect();
    sorted.sort_by_key(|(_, part)| (part.blocks.start, part.blocks.end));
    let mut body = doc
        .sections
        .iter()
        .flat_map(|section| section.blocks.iter())
        .enumerate()
        .peekable();
    let mut skipped = String::new();
    for (rank, (slot, part)) in sorted.into_iter().enumerate() {
        let mut skipping = false;
        while let Some((_, block)) = body.next_if(|(i, _)| *i < part.blocks.start) {
            writer.block(block, &mut skipped)?;
            skipped.clear();
            skipping = true;
        }
        if skipping {
            writer.end(None, &mut skipped)?;
            skipped.clear();
        }
        let Some(out) = outputs.get_mut(slot) else {
            continue;
        };
        writer.reset();
        if rank == 0 && part.blocks.start == 0 {
            writer.opening(out)?;
        }
        writer.begin(part, out)?;
        while let Some((_, block)) = body.next_if(|(i, _)| *i < part.blocks.end) {
            writer.block(block, out)?;
        }
        writer.end(Some(part), out)?;
    }
    Ok(())
}

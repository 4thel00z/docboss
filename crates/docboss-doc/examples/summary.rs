//! Prints a compact structural summary of a Word binary document.

use docboss_model::{Block, Document, Inline, RunContent};

fn inline(i: &Inline, out: &mut String) {
    match i {
        Inline::Run(r) => {
            for c in &r.content {
                match c {
                    RunContent::Text(t) => out.push_str(t),
                    RunContent::Drawing(d) => out.push_str(&format!(
                        "[img {:?} {}x{} {:?} box={:?}]",
                        d.media,
                        d.width,
                        d.height,
                        d.placement,
                        d.text_box.iter().map(block_text).collect::<Vec<_>>()
                    )),
                    RunContent::FootnoteReference(n) => out.push_str(&format!("[fn{n}]")),
                    RunContent::EndnoteReference(n) => out.push_str(&format!("[en{n}]")),
                    RunContent::CommentReference(n) => out.push_str(&format!("[c{n}]")),
                    other => out.push_str(&format!("[{other:?}]")),
                }
            }
        }
        Inline::Hyperlink(h) => {
            out.push_str(&format!("[link {:?} ", h.target));
            h.inlines.iter().for_each(|i| inline(i, out));
            out.push(']');
        }
        Inline::Field(f) => {
            out.push_str(&format!("[field {:?} ", f.instruction));
            f.result.iter().for_each(|i| inline(i, out));
            out.push(']');
        }
        other => out.push_str(&format!("[{other:?}]")),
    }
}

fn block_text(block: &Block) -> String {
    match block {
        Block::Paragraph(p) => p.text(),
        Block::Table(_) => "table".to_string(),
    }
}

fn blocks(document: &Document, list: &[Block], indent: usize) {
    let pad = "  ".repeat(indent);
    for block in list {
        match block {
            Block::Paragraph(p) => {
                let mut text = String::new();
                p.inlines.iter().for_each(|i| inline(i, &mut text));
                let heading = document.styles.heading_level(p, &document.numbering);
                println!(
                    "{pad}P style={:?} h={heading:?} num={:?} {text}",
                    p.style_id, p.properties.numbering
                );
            }
            Block::Table(t) => {
                println!("{pad}TABLE grid={:?}", t.grid);
                for row in &t.rows {
                    println!("{pad} ROW {:?}", row.properties);
                    for cell in &row.cells {
                        println!(
                            "{pad}  CELL span={} vm={:?} w={:?}",
                            cell.properties.grid_span,
                            cell.properties.vertical_merge,
                            cell.properties.width
                        );
                        blocks(document, &cell.blocks, indent + 3);
                    }
                }
            }
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("usage: summary <file.doc>")?;
    let document = docboss_doc::read(&std::fs::read(&path)?)?;
    println!("metadata {:?}", document.metadata.title);
    for (i, section) in document.sections.iter().enumerate() {
        let p = &section.properties;
        println!(
            "SECTION {i} page={:?} margins={:?} cols={:?} headers={:?} footers={:?}",
            p.page_size, p.margins, p.columns.count, p.headers, p.footers
        );
        blocks(&document, &section.blocks, 1);
    }
    for part in &document.headers_footers {
        println!("{:?} {}", part.kind, part.id);
        blocks(&document, &part.blocks, 1);
    }
    for note in document.footnotes.iter().chain(&document.endnotes) {
        println!("NOTE {:?} {}", note.kind, note.id);
        blocks(&document, &note.blocks, 1);
    }
    for comment in &document.comments {
        println!(
            "COMMENT {} {:?} {:?} {:?}",
            comment.id, comment.author, comment.initials, comment.date
        );
        blocks(&document, &comment.blocks, 1);
    }
    for media in &document.media {
        println!(
            "MEDIA {} {} {} bytes",
            media.name,
            media.content_type,
            media.data.len()
        );
    }
    println!(
        "styles {} abstracts {} instances {} fonts {}",
        document.styles.styles.len(),
        document.numbering.abstracts.len(),
        document.numbering.instances.len(),
        document.fonts.len()
    );
    for d in &document.diagnostics {
        println!("DIAG {d:?}");
    }
    Ok(())
}

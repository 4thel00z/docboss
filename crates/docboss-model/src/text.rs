use crate::{Block, Document, Inline, NumberFormat, RevisionKind, RunContent};

/// Formats `n` in a list numbering format.
pub fn number_label(format: &NumberFormat, n: u32) -> String {
    match format {
        NumberFormat::Decimal | NumberFormat::Other(_) => n.to_string(),
        NumberFormat::DecimalZero => format!("{n:02}"),
        NumberFormat::UpperRoman => roman(n),
        NumberFormat::LowerRoman => roman(n).to_lowercase(),
        NumberFormat::UpperLetter => letters(n),
        NumberFormat::LowerLetter => letters(n).to_lowercase(),
        NumberFormat::Ordinal => format!("{n}{}", ordinal_suffix(n)),
        NumberFormat::Bullet | NumberFormat::None => String::new(),
    }
}

fn roman(mut n: u32) -> String {
    const NUMERALS: [(u32, &str); 13] = [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    if n == 0 {
        return "0".to_string();
    }
    let mut out = String::new();
    for (value, numeral) in NUMERALS {
        while n >= value {
            out.push_str(numeral);
            n -= value;
        }
    }
    out
}

fn letters(n: u32) -> String {
    if n == 0 {
        return String::new();
    }
    let letter = char::from(b'A' + ((n - 1) % 26) as u8);
    std::iter::repeat_n(letter, ((n - 1) / 26 + 1) as usize).collect()
}

fn ordinal_suffix(n: u32) -> &'static str {
    if (11..=13).contains(&(n % 100)) {
        return "th";
    }
    match n % 10 {
        1 => "st",
        2 => "nd",
        3 => "rd",
        _ => "th",
    }
}

pub(crate) fn inlines_text(inlines: &[Inline], out: &mut String) {
    for inline in inlines {
        match inline {
            Inline::Run(run) => run
                .content
                .iter()
                .for_each(|content| content_text(content, out)),
            Inline::Hyperlink(link) => inlines_text(&link.inlines, out),
            Inline::Field(field) => inlines_text(&field.result, out),
            Inline::Revision(revision) if revision.kind == RevisionKind::Insertion => {
                inlines_text(&revision.inlines, out)
            }
            _ => {}
        }
    }
}

fn content_text(content: &RunContent, out: &mut String) {
    match content {
        RunContent::Text(text) => out.push_str(text),
        RunContent::Tab => out.push('\t'),
        RunContent::Break(_) | RunContent::CarriageReturn => out.push('\n'),
        RunContent::NoBreakHyphen => out.push('\u{2011}'),
        RunContent::Symbol { char, .. } => out.extend(char::from_u32(*char)),
        RunContent::Math(math) => out.push_str(&crate::linear_text(&math.nodes)),
        RunContent::Drawing(drawing) => {
            if let Some(math) = &drawing.math {
                out.push_str(&crate::linear_text(&math.nodes));
            }
        }
        _ => {}
    }
}

fn blocks_text(blocks: &[Block], out: &mut String) {
    for block in blocks {
        match block {
            Block::Paragraph(paragraph) => {
                inlines_text(&paragraph.inlines, out);
                out.push('\n');
            }
            Block::Table(table) => {
                for row in &table.rows {
                    for cell in &row.cells {
                        blocks_text(&cell.blocks, out);
                    }
                }
            }
        }
    }
}

/// The main text of a document with one line per paragraph, deleted
/// revisions left out. Output crates offer richer extraction; this is the
/// minimal form every reader test can compare against.
pub fn plain_text(document: &Document) -> String {
    let mut out = String::new();
    for section in &document.sections {
        blocks_text(&section.blocks, &mut out);
    }
    out
}

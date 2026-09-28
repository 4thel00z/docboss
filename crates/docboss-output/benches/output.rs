use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use docboss_model::*;
use docboss_output::{
    blocks_view, to_html, to_markdown, to_text, HtmlOptions, MarkdownOptions, TextOptions,
};

const PAGES: usize = 500;
const WORDS: [&str; 12] = [
    "lorem",
    "ipsum",
    "dolor",
    "sit",
    "amet",
    "consectetur",
    "adipiscing",
    "elit",
    "sed",
    "do",
    "eiusmod",
    "tempor",
];

fn run(text: String, bold: bool) -> Inline {
    Inline::Run(Run {
        properties: RunProperties {
            bold: bold.then_some(true),
            ..RunProperties::default()
        },
        content: vec![RunContent::Text(text)],
    })
}

fn sentence(seed: usize, words: usize) -> String {
    (0..words)
        .map(|i| WORDS[(seed * 7 + i * 3) % WORDS.len()])
        .collect::<Vec<_>>()
        .join(" ")
}

fn paragraph(seed: usize) -> Block {
    let inlines = (0..4)
        .map(|i| run(format!("{} ", sentence(seed + i, 15)), i == 1))
        .collect();
    Block::Paragraph(Paragraph {
        style_id: Some("Normal".into()),
        inlines,
        ..Paragraph::default()
    })
}

fn synthetic() -> Document {
    let mut normal = Style::new("Normal", StyleKind::Paragraph);
    normal.is_default = true;
    let mut heading = Style::new("Heading1", StyleKind::Paragraph);
    heading.name = Some("heading 1".into());
    heading.based_on = Some("Normal".into());
    let styles = Styles::new(
        ParagraphProperties::default(),
        RunProperties::default(),
        vec![normal, heading],
    );
    let numbering = Numbering {
        abstracts: vec![AbstractNumbering {
            id: 0,
            levels: vec![Level::default()],
        }],
        instances: vec![NumberingInstance {
            num_id: 1,
            abstract_id: 0,
            ..NumberingInstance::default()
        }],
    };
    let mut blocks = Vec::new();
    for page in 0..PAGES {
        blocks.push(Block::Paragraph(Paragraph {
            style_id: Some("Heading1".into()),
            inlines: vec![run(format!("Chapter {page}"), false)],
            ..Paragraph::default()
        }));
        blocks.extend((0..6).map(|i| paragraph(page * 10 + i)));
        blocks.extend((0..3).map(|i| {
            let mut p = Paragraph {
                inlines: vec![run(sentence(page + i, 8), false)],
                ..Paragraph::default()
            };
            p.properties.numbering = Some(NumberingRef {
                num_id: 1,
                level: 0,
            });
            Block::Paragraph(p)
        }));
        let rows = (0..4)
            .map(|r| TableRow {
                cells: (0..3)
                    .map(|c| TableCell {
                        blocks: vec![Block::Paragraph(Paragraph {
                            inlines: vec![run(sentence(r * 3 + c, 3), false)],
                            ..Paragraph::default()
                        })],
                        ..TableCell::default()
                    })
                    .collect(),
                ..TableRow::default()
            })
            .collect();
        blocks.push(Block::Table(Table {
            rows,
            ..Table::default()
        }));
    }
    Document {
        styles,
        numbering,
        sections: vec![Section {
            blocks,
            ..Section::default()
        }],
        ..Document::default()
    }
}

fn bench(c: &mut Criterion) {
    let doc = synthetic();
    let bytes = docboss_model::plain_text(&doc).len() as u64;
    let mut group = c.benchmark_group("500 pages");
    group.throughput(Throughput::Bytes(bytes));
    group.bench_function("text", |b| {
        b.iter(|| to_text(black_box(&doc), &TextOptions::default()))
    });
    group.bench_function("markdown", |b| {
        b.iter(|| to_markdown(black_box(&doc), &MarkdownOptions::default()))
    });
    group.bench_function("html", |b| {
        b.iter(|| to_html(black_box(&doc), &HtmlOptions::default()))
    });
    group.bench_function("blocks", |b| b.iter(|| blocks_view(black_box(&doc))));
    group.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);

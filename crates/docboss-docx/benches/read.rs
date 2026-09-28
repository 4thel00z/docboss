use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use docboss_testkit::Docx;
use docboss_xml::{Event, Reader};

/// A synthetic report: `paragraphs` formatted paragraphs with a heading, a
/// hyperlink, a field and a small table every hundred.
fn large_document(paragraphs: usize) -> Vec<u8> {
    let mut body = String::new();
    for i in 0..paragraphs {
        if i % 100 == 0 {
            body.push_str(&format!(r#"<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Chapter {i}</w:t></w:r></w:p>"#));
            body.push_str(r#"<w:tbl><w:tblPr><w:tblW w:w="5000" w:type="pct"/></w:tblPr><w:tblGrid><w:gridCol w:w="4000"/><w:gridCol w:w="4000"/></w:tblGrid>"#);
            for row in 0..4 {
                body.push_str(&format!(r#"<w:tr><w:tc><w:p><w:r><w:t>cell {row}a</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>cell {row}b</w:t></w:r></w:p></w:tc></w:tr>"#));
            }
            body.push_str("</w:tbl>");
        }
        body.push_str(&format!(
            r#"<w:p w:rsidR="00A1B2C3" w:rsidRDefault="00A1B2C3"><w:pPr><w:spacing w:after="120" w:line="276" w:lineRule="auto"/><w:jc w:val="both"/></w:pPr><w:r w:rsidRPr="00D4E5F6"><w:rPr><w:rFonts w:ascii="Calibri" w:hAnsi="Calibri"/><w:sz w:val="22"/></w:rPr><w:t xml:space="preserve">Paragraph {i} explains the quarterly figures in some detail, </w:t></w:r><w:r><w:rPr><w:b/><w:i/></w:rPr><w:t>with emphasis &amp; care</w:t></w:r><w:hyperlink r:id="rIdL"><w:r><w:rPr><w:rStyle w:val="Hyperlink"/></w:rPr><w:t>a link</w:t></w:r></w:hyperlink><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> PAGE </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>{i}</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>"#
        ));
    }
    let styles = r#"<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style><w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/><w:pPr><w:outlineLvl w:val="0"/></w:pPr></w:style>"#;
    Docx::new(&body)
        .styles(styles)
        .external("rIdL", "hyperlink", "https://example.com/")
        .build()
}

fn bench(c: &mut Criterion) {
    let large = large_document(20_000);
    let archive = docboss_zip::Archive::new(&large).unwrap();
    let xml = archive.entry("word/document.xml").unwrap().into_owned();
    let text = String::from_utf8(xml).unwrap();

    let mut group = c.benchmark_group("docx");
    group.throughput(Throughput::Bytes(text.len() as u64));
    group.sample_size(20);
    group.bench_function("read 20k paragraphs", |b| {
        b.iter(|| docboss_docx::read(black_box(&large)).unwrap())
    });
    group.bench_function("read 20k paragraphs, one thread", |b| {
        let options = docboss_docx::Options {
            parallel: false,
            ..docboss_docx::Options::default()
        };
        b.iter(|| docboss_docx::read_with(black_box(&large), options).unwrap())
    });
    group.bench_function("inflate document.xml", |b| {
        b.iter(|| {
            docboss_zip::Archive::new(black_box(&large))
                .unwrap()
                .entry("word/document.xml")
                .unwrap()
                .len()
        })
    });
    group.bench_function("tokenize document.xml", |b| {
        b.iter(|| {
            let mut reader = Reader::new(black_box(&text));
            let mut events = 0usize;
            while !matches!(reader.next_event(), Event::Eof) {
                events += 1;
            }
            events
        })
    });
    group.finish();

    let fixture = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/libreoffice-rich.docx"
    ))
    .unwrap();
    c.bench_function("docx/read small fixture", |b| {
        b.iter(|| docboss_docx::read(black_box(&fixture)).unwrap())
    });
}

criterion_group!(benches, bench);
criterion_main!(benches);

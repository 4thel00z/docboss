#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::OnceLock;

use docboss_output::{HtmlOptions, ImageMode, MarkdownOptions, TextOptions};
use docboss_pages::{Document, Error, Format, Output};
use serde_json::json;

fn parse(value: serde_json::Value) -> Result<Format, String> {
    serde_json::from_value(value).map_err(|e| e.to_string())
}

fn markdown_source() -> String {
    let mut source = String::from("# Report\n\n");
    for i in 0..240 {
        source.push_str(&format!(
            "Paragraph p{i} has words a{i} b{i} c{i} d{i} e{i} f{i} g{i} h{i} i{i} j{i} k{i} l{i} m{i} n{i} o{i}."
        ));
        if i % 40 == 7 {
            source.push_str(&format!("[^n{i}]"));
        }
        source.push_str("\n\n");
        if i % 60 == 30 {
            source.push_str(&format!("- item x{i}\n- item y{i}\n- item z{i}\n\n"));
            source.push_str(&format!("| q{i} | r{i} |\n|---|---|\n| s{i} | t{i} |\n\n"));
        }
    }
    for i in (0..240).filter(|i| i % 40 == 7) {
        source.push_str(&format!("[^n{i}]: Note for p{i} with words u{i} v{i}.\n\n"));
    }
    source
}

fn document() -> Document {
    static DOCUMENT: OnceLock<Document> = OnceLock::new();
    DOCUMENT
        .get_or_init(|| {
            let options = docboss_write::markdown::Options::default();
            Document::new(docboss_write::markdown::to_document(
                &markdown_source(),
                &options,
            ))
        })
        .clone()
}

fn texts(outputs: Vec<Output>) -> Vec<String> {
    outputs
        .into_iter()
        .map(|output| output.as_text().expect("text output").to_string())
        .collect()
}

fn sorted_words(text: &str) -> Vec<String> {
    let mut words: Vec<String> = text.split_whitespace().map(str::to_string).collect();
    words.sort();
    words
}

#[test]
fn a_tagged_map_parses_into_its_format() {
    let format = parse(json!({"format": "markdown", "images": "omit"})).unwrap();
    assert_eq!(
        format,
        Format::Markdown(MarkdownOptions {
            images: ImageMode::Omit,
            ..MarkdownOptions::default()
        })
    );
}

#[test]
fn defaults_match_the_extraction_methods() {
    assert_eq!(
        parse(json!({"format": "text"})).unwrap(),
        Format::Text(TextOptions::default())
    );
    assert_eq!(
        parse(json!({"format": "markdown"})).unwrap(),
        Format::Markdown(MarkdownOptions::default())
    );
    assert_eq!(
        parse(json!({"format": "html"})).unwrap(),
        Format::Html(HtmlOptions::default())
    );
    assert_eq!(
        parse(json!({"format": "json"})).unwrap(),
        Format::Json { pretty: false }
    );
    assert_eq!(
        parse(json!({"format": "png"})).unwrap(),
        Format::Png { scale: 1.0 }
    );
    assert_eq!(
        parse(json!({"format": "jpeg"})).unwrap(),
        Format::Jpeg {
            scale: 1.0,
            quality: 90
        }
    );
}

#[test]
fn image_references_carry_their_prefix() {
    let format =
        parse(json!({"format": "html", "images": "reference", "image_prefix": "media/"})).unwrap();
    let Format::Html(options) = format else {
        panic!("not html: {format:?}");
    };
    assert_eq!(
        options.images,
        ImageMode::Reference {
            prefix: "media/".into()
        }
    );
}

#[test]
fn jpg_names_jpeg() {
    assert_eq!(
        parse(json!({"format": "jpg", "scale": 2.0, "quality": 70})).unwrap(),
        Format::Jpeg {
            scale: 2.0,
            quality: 70
        }
    );
}

#[test]
fn unknown_keys_are_refused() {
    let error = parse(json!({"format": "markdown", "standalone": true})).unwrap_err();
    assert!(error.contains("standalone"), "{error}");
    let error = parse(json!({"format": "png", "quality": 80})).unwrap_err();
    assert!(error.contains("quality"), "{error}");
}

#[test]
fn a_missing_or_unknown_format_is_refused() {
    let error = parse(json!({"images": "omit"})).unwrap_err();
    assert!(error.contains("format"), "{error}");
    let error = parse(json!({"format": "pdf"})).unwrap_err();
    assert!(error.contains("pdf"), "{error}");
}

#[test]
fn bad_scales_and_qualities_are_refused() {
    let error = parse(json!({"format": "png", "scale": 0})).unwrap_err();
    assert!(error.contains("scale"), "{error}");
    let error = parse(json!({"format": "jpeg", "quality": 0})).unwrap_err();
    assert!(error.contains("quality"), "{error}");
    let error = parse(json!({"format": "jpeg", "quality": 101})).unwrap_err();
    assert!(error.contains("quality"), "{error}");
    let format = Format::Png { scale: f32::NAN };
    let page = document().page(0).unwrap();
    assert!(matches!(page.to(&format), Err(Error::Format(_))));
}

#[test]
fn page_block_ranges_are_contiguous_and_cover_the_body() {
    let document = document();
    let pages = document.pages();
    assert!(pages.len() > 3, "{} pages", pages.len());
    let body: usize = document
        .model()
        .sections
        .iter()
        .map(|section| section.blocks.len())
        .sum();
    let mut end = 0;
    for page in pages.iter() {
        let blocks = page.blocks();
        assert_eq!(blocks.start, end, "page {}", page.index());
        end = blocks.end;
    }
    assert_eq!(end, body);
}

#[test]
fn pages_together_hold_every_word_once() {
    let document = document();
    let formats = [
        Format::Text(TextOptions::default()),
        Format::Markdown(MarkdownOptions {
            images: ImageMode::Omit,
            ..MarkdownOptions::default()
        }),
    ];
    for format in formats {
        let whole = document.to(&format).unwrap();
        let whole = whole.as_text().unwrap();
        let pages = texts(document.pages().to(&format).unwrap());
        assert_eq!(
            sorted_words(&pages.concat()),
            sorted_words(whole),
            "{}",
            format.name()
        );
    }
}

#[test]
fn notes_go_on_the_page_of_their_reference() {
    let document = document();
    let pages = texts(
        document
            .pages()
            .to(&Format::Text(TextOptions::default()))
            .unwrap(),
    );
    for i in (0..240).filter(|i| i % 40 == 7) {
        let note = format!("u{i}");
        let reference = format!("p{i} ");
        let on: Vec<usize> = pages
            .iter()
            .enumerate()
            .filter(|(_, text)| text.contains(&note))
            .map(|(index, _)| index)
            .collect();
        assert_eq!(on.len(), 1, "note {i} on pages {on:?}");
        assert!(pages[on[0]].contains(&reference), "note {i}");
    }
}

#[test]
fn a_page_matches_its_place_in_all_pages() {
    let document = document();
    let format = Format::Html(HtmlOptions::default());
    let all = texts(document.pages().to(&format).unwrap());
    let last = all.len() - 1;
    for index in [0, 1, last] {
        let page = document.page(index).unwrap().to(&format).unwrap();
        assert_eq!(page.as_text().unwrap(), all[index]);
    }
    assert!(document.page(all.len()).is_none());
}

#[test]
fn a_selection_converts_its_pages_in_order() {
    let document = document();
    let format = Format::Markdown(MarkdownOptions::default());
    let all = texts(document.pages().to(&format).unwrap());
    let picked = document.pages().pick([2, 0, 99, 1]);
    assert_eq!(picked.indices(), &[2, 0, 1]);
    let outputs = texts(picked.to(&format).unwrap());
    assert_eq!(
        outputs,
        vec![all[2].clone(), all[0].clone(), all[1].clone()]
    );
}

#[test]
fn page_json_holds_the_blocks_that_start_on_it() {
    let document = document();
    let outputs = texts(
        document
            .pages()
            .to(&Format::Json { pretty: false })
            .unwrap(),
    );
    for (page, json) in document.pages().iter().zip(outputs) {
        let blocks: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap();
        assert_eq!(blocks.len(), page.blocks().len());
    }
}

#[test]
fn image_formats_render_pages() {
    let document = document();
    let page = document.page(0).unwrap();
    let png = page.to(&Format::Png { scale: 0.5 }).unwrap();
    assert!(png.as_bytes().unwrap().starts_with(b"\x89PNG"));
    let jpegs = document
        .pages()
        .pick([0, 1])
        .to(&Format::Jpeg {
            scale: 0.5,
            quality: 80,
        })
        .unwrap();
    assert_eq!(jpegs.len(), 2);
    assert!(jpegs
        .iter()
        .all(|jpeg| jpeg.as_bytes().unwrap().starts_with(&[0xFF, 0xD8])));
}

#[test]
fn the_whole_document_does_not_render_to_an_image() {
    let error = document().to(&Format::Png { scale: 1.0 }).unwrap_err();
    assert!(matches!(error, Error::WholeDocument("png")), "{error}");
}

#[test]
fn page_numbers_follow_the_pages() {
    let document = document();
    let numbers: Vec<u32> = document.pages().iter().map(|page| page.number()).collect();
    let expected: Vec<u32> = (1..=numbers.len() as u32).collect();
    assert_eq!(numbers, expected);
}

#[test]
fn a_page_where_no_block_starts_is_empty() {
    let long: Vec<String> = (0..4000).map(|i| format!("long{i}")).collect();
    let source = format!("Before.\n\n{}\n\nAfter.\n", long.join(" "));
    let options = docboss_write::markdown::Options::default();
    let document = Document::new(docboss_write::markdown::to_document(&source, &options));
    let pages = document.pages();
    let empty: Vec<usize> = pages
        .iter()
        .filter(|page| page.blocks().is_empty())
        .map(|page| page.index())
        .collect();
    assert!(!empty.is_empty(), "no empty page among {}", pages.len());
    let formats = [
        Format::Text(TextOptions::default()),
        Format::Markdown(MarkdownOptions::default()),
        Format::Html(HtmlOptions::default()),
        Format::Json { pretty: false },
    ];
    for format in formats {
        let all = texts(pages.to(&format).unwrap());
        for &index in &empty {
            assert_eq!(all[index], "", "{} page {index}", format.name());
            let page = document.page(index).unwrap().to(&format).unwrap();
            assert_eq!(page.as_text(), Some(""), "{} page {index}", format.name());
        }
        assert!(all[0].contains("long3999"), "{}", format.name());
    }
}

#[cfg(feature = "http")]
mod common;

use std::path::PathBuf;

use docboss_aio::{AsyncDocument, ReadOptions};
use docboss_core::Format;
use docboss_testkit::Docx;

fn fixture(crate_name: &str, name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(crate_name)
        .join("tests/fixtures")
        .join(name)
}

/// A DOCX whose one picture is 2 MiB of incompressible bytes, so a text
/// read that skips it fetches a small fraction of the file.
fn docx_with_big_image() -> Vec<u8> {
    let mut image = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut state = 12345u32;
    image.extend((0..2 << 20).map(|_| {
        state = state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        (state >> 16) as u8
    }));
    let drawing = r#"<w:p><w:r><w:t>Before the picture.</w:t></w:r><w:r><w:drawing><wp:inline><wp:extent cx="914400" cy="914400"/><wp:docPr id="1" name="Big"/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic><pic:blipFill><a:blip r:embed="rIdImage"/></pic:blipFill></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p><w:p><w:r><w:t>After the picture.</w:t></w:r></w:p>"#;
    Docx::new(drawing)
        .image("rIdImage", "media/big.png", &image)
        .build()
}

fn sync_text(bytes: &[u8]) -> String {
    docboss_core::plain_text(&docboss_core::read(bytes).unwrap())
}

/// APPNOTE §4.3.16, §4.3.12: the end record and the central directory come
/// from the tail, and a text read fetches the XML parts only.
#[tokio::test]
async fn docx_text_read_skips_media_until_asked() {
    let bytes = docx_with_big_image();
    let document = AsyncDocument::from_bytes(bytes.clone()).await.unwrap();
    assert_eq!(document.format(), Format::Docx);
    let mut read = document.read(&ReadOptions::default()).await.unwrap();
    assert_eq!(docboss_core::plain_text(&read), sync_text(&bytes));
    assert!(read.diagnostics.is_empty(), "{:?}", read.diagnostics);
    assert!(
        document.bytes_fetched() < bytes.len() as u64 / 20,
        "fetched {}",
        document.bytes_fetched()
    );
    assert_eq!(read.media.len(), 1);
    assert!(read.media[0].data.is_empty());

    document.load_media(&mut read).await.unwrap();
    let full = docboss_core::read(&bytes).unwrap();
    assert_eq!(read.media[0].data, full.media[0].data);
    assert_eq!(read.media[0].content_type, "image/png");
}

#[tokio::test]
async fn docx_fixtures_match_the_synchronous_reader() {
    for name in [
        "libreoffice-rich.docx",
        "libreoffice-rtf-notes.docx",
        "textutil-lists.docx",
    ] {
        let path = fixture("docboss-docx", name);
        let bytes = std::fs::read(&path).unwrap();
        let document = AsyncDocument::open(&path).await.unwrap();
        let read = document
            .read(&ReadOptions {
                media: true,
                ..ReadOptions::default()
            })
            .await
            .unwrap();
        let full = docboss_core::read(&bytes).unwrap();
        assert_eq!(read.sections, full.sections, "{name}");
        assert_eq!(read.media, full.media, "{name}");
        assert_eq!(read.styles, full.styles, "{name}");
        let part = document.part("word/document.xml").await.unwrap();
        assert!(String::from_utf8_lossy(&part).contains("w:body"), "{name}");
    }
}

/// [MS-CFB] §2.2 to §2.6: the header, FAT, directory and mini stream come
/// first, then the Word streams; the read equals the synchronous one.
#[tokio::test]
async fn doc_fixtures_match_the_synchronous_reader() {
    for name in [
        "image.doc",
        "lists.doc",
        "tables.doc",
        "notes.doc",
        "sections.doc",
    ] {
        let path = fixture("docboss-doc", name);
        let bytes = std::fs::read(&path).unwrap();
        let document = AsyncDocument::open(&path).await.unwrap();
        assert_eq!(document.format(), Format::Doc, "{name}");
        let read = document.read(&ReadOptions::default()).await.unwrap();
        let full = docboss_core::read(&bytes).unwrap();
        assert_eq!(read.sections, full.sections, "{name}");
        assert_eq!(read.media, full.media, "{name}");
        assert!(document.part_names().iter().any(|n| n == "WordDocument"));
        let stream = document.part("WordDocument").await.unwrap();
        assert_eq!(u16::from_le_bytes([stream[0], stream[1]]), 0xA5EC, "{name}");
    }
}

/// [MS-OFFCRYPTO] §2.3.4.4: an encrypted DOCX is read through its two
/// streams and opened with the password.
#[tokio::test]
async fn encrypted_docx_reads_with_password() {
    let path = fixture("docboss-crypt", "Encrypted_MSO2013_abc.docx");
    let document = AsyncDocument::open(&path).await.unwrap();
    assert_eq!(document.format(), Format::EncryptedDocx);
    assert!(document.read(&ReadOptions::default()).await.is_err());
    let options = ReadOptions {
        password: Some("abc".into()),
        ..ReadOptions::default()
    };
    let read = document.read(&options).await.unwrap();
    let sync = docboss_core::read_with(
        &std::fs::read(&path).unwrap(),
        &docboss_core::Options {
            password: Some("abc".into()),
        },
    )
    .unwrap();
    assert_eq!(read.sections, sync.sections);
}

#[tokio::test]
async fn garbage_is_refused_cleanly() {
    let document = AsyncDocument::from_bytes(b"neither zip nor compound file".to_vec())
        .await
        .unwrap();
    assert_eq!(document.format(), Format::Unknown);
    assert!(document.read(&ReadOptions::default()).await.is_err());
    let truncated = docx_with_big_image()[..5000].to_vec();
    let document = AsyncDocument::from_bytes(truncated).await.unwrap();
    let _ = document.read(&ReadOptions::default()).await;
}

#[cfg(feature = "http")]
mod http {
    use super::*;
    use std::sync::atomic::Ordering;

    #[tokio::test]
    async fn ranged_server_serves_only_what_the_read_needs() {
        let bytes = docx_with_big_image();
        let server = common::start(bytes.clone(), true).await;
        let document = AsyncDocument::open_url(&server.url).await.unwrap();
        let read = document.read(&ReadOptions::default()).await.unwrap();
        assert_eq!(docboss_core::plain_text(&read), sync_text(&bytes));
        let served = server.served.load(Ordering::SeqCst);
        assert_eq!(served, document.bytes_fetched());
        assert!(
            served < bytes.len() as u64 / 20,
            "served {served} of {}",
            bytes.len()
        );
    }

    #[tokio::test]
    async fn range_ignoring_server_costs_one_download() {
        let bytes = docx_with_big_image();
        let server = common::start(bytes.clone(), false).await;
        let document = AsyncDocument::open_url(&server.url).await.unwrap();
        let read = document.read(&ReadOptions::default()).await.unwrap();
        assert_eq!(docboss_core::plain_text(&read), sync_text(&bytes));
        assert_eq!(server.gets.load(Ordering::SeqCst), 1);
        assert_eq!(server.served.load(Ordering::SeqCst), bytes.len() as u64);
    }

    #[tokio::test]
    async fn doc_over_http_matches_local() {
        let bytes = std::fs::read(fixture("docboss-doc", "image.doc")).unwrap();
        let server = common::start(bytes.clone(), true).await;
        let document = AsyncDocument::open_url(&server.url).await.unwrap();
        let read = document.read(&ReadOptions::default()).await.unwrap();
        assert_eq!(read.sections, docboss_core::read(&bytes).unwrap().sections);
    }

    #[tokio::test]
    async fn missing_server_is_an_http_error() {
        let error = AsyncDocument::open_url("http://127.0.0.1:1/nothing")
            .await
            .err()
            .unwrap();
        assert!(error.to_string().starts_with("http"), "{error}");
    }
}

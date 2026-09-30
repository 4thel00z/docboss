use std::path::PathBuf;

use docboss_core::{detect, open, read, Error, Format, SourceFormat};

fn fixture(crate_name: &str, name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(crate_name)
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn docx_and_doc_open_through_one_call() {
    let docx = open(fixture("docboss-docx", "libreoffice-rich.docx")).unwrap();
    assert_eq!(docx.format, SourceFormat::Docx);
    assert!(!docx.sections.is_empty());

    let doc = open(fixture("docboss-doc", "lists.doc")).unwrap();
    assert_eq!(doc.format, SourceFormat::Doc);
    assert!(!docboss_core::plain_text(&doc).trim().is_empty());
}

#[test]
fn format_comes_from_bytes_not_names() {
    let bytes = std::fs::read(fixture("docboss-doc", "lists.doc")).unwrap();
    assert_eq!(detect(&bytes), Format::Doc);
    assert_eq!(detect(b"{\\rtf1 hello}"), Format::Rtf);
    assert!(matches!(read(b"plain text"), Err(Error::Unsupported(_))));
    let word2 = std::fs::read(fixture("docboss-doc", "word2.doc")).unwrap();
    assert_eq!(detect(&word2), Format::Doc);
    assert!(!docboss_core::plain_text(&read(&word2).unwrap())
        .trim()
        .is_empty());
}

#[test]
fn encrypted_doc_needs_its_password() {
    let bytes = std::fs::read(fixture("docboss-doc", "encrypted-rc4.doc")).unwrap();
    assert!(read(&bytes).is_err());
}

/// [MS-OFFCRYPTO] §2.3.4.4: a DOCX protected with a password to open is a
/// compound file; with the password it opens like any other DOCX.
#[test]
fn encrypted_docx_opens_with_its_password() {
    let bytes = std::fs::read(fixture("docboss-crypt", "Encrypted_MSO2013_abc.docx")).unwrap();
    assert_eq!(detect(&bytes), Format::EncryptedDocx);
    assert!(matches!(read(&bytes), Err(Error::Encrypted)));
    let options = docboss_core::Options {
        password: Some("abc".into()),
    };
    let document = docboss_core::read_with(&bytes, &options).unwrap();
    assert_eq!(document.format, SourceFormat::Docx);
    assert!(!docboss_core::plain_text(&document).trim().is_empty());
    let wrong = docboss_core::Options {
        password: Some("abd".into()),
    };
    assert!(matches!(
        docboss_core::read_with(&bytes, &wrong),
        Err(Error::Crypt(docboss_crypt::Error::WrongPassword))
    ));
}

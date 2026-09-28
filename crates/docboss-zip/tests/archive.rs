use docboss_testkit::{ZipOptions, ZipWriter};
use docboss_zip::{Archive, Error, Limits};

fn sample(options: ZipOptions) -> Vec<u8> {
    let big: Vec<u8> = b"<w:p>hello</w:p>".repeat(5000);
    ZipWriter::with_options(options)
        .stored("a.txt", b"stored bytes")
        .deflated("word/document.xml", &big)
        .deflated("empty", b"")
        .finish()
}

fn check(data: &[u8]) {
    let archive = Archive::new(data).unwrap();
    assert_eq!(
        archive.names().collect::<Vec<_>>(),
        ["a.txt", "word/document.xml", "empty"]
    );
    assert_eq!(&*archive.entry("a.txt").unwrap(), b"stored bytes");
    assert_eq!(archive.entry("word/document.xml").unwrap().len(), 16 * 5000);
    assert!(archive.entry("empty").unwrap().is_empty());
}

/// APPNOTE §4.3.12 and §4.3.16: entries are listed by the central directory
/// found through the end of central directory record.
/// APPNOTE §4.3.6, §4.3.8, §4.4.5, §5.5: stored and deflated file data.
#[test]
fn reads_stored_and_deflated_entries() {
    check(&sample(ZipOptions::default()));
}

/// APPNOTE §4.3.9: sizes and CRC deferred to a data descriptor.
/// APPNOTE §4.4.4: bit 3 of the general purpose flags defers them.
#[test]
fn reads_entries_with_data_descriptors() {
    check(&sample(ZipOptions {
        data_descriptor: true,
        ..ZipOptions::default()
    }));
}

/// APPNOTE §4.3.14, §4.3.15 and §4.5.3: ZIP64 end records and extra fields.
#[test]
fn reads_zip64_archives() {
    check(&sample(ZipOptions {
        zip64: true,
        ..ZipOptions::default()
    }));
}

#[test]
fn stored_entries_borrow_from_the_input() {
    let data = sample(ZipOptions::default());
    let archive = Archive::new(&data).unwrap();
    assert!(matches!(
        archive.entry("a.txt").unwrap(),
        std::borrow::Cow::Borrowed(_)
    ));
}

#[test]
fn lookup_falls_back_to_case_insensitive() {
    let data = sample(ZipOptions::default());
    let archive = Archive::new(&data).unwrap();
    assert!(archive.find("/Word/Document.XML").is_some());
    assert_eq!(
        archive.entry("missing").unwrap_err(),
        Error::NotFound("missing".into())
    );
}

/// APPNOTE §4.3.7: with the central directory cut off, entries are
/// recovered from their local file headers and the recovery is reported.
#[test]
fn recovers_from_a_missing_central_directory() {
    for options in [
        ZipOptions::default(),
        ZipOptions {
            data_descriptor: true,
            ..ZipOptions::default()
        },
    ] {
        let data = sample(options);
        let cut = memchr::memmem::find(&data, &0x0201_4b50u32.to_le_bytes()).unwrap();
        let archive = Archive::new(&data[..cut]).unwrap();
        assert_eq!(archive.diagnostics().len(), 1);
        check_recovered(&archive);
    }
}

fn check_recovered(archive: &Archive<'_>) {
    assert_eq!(archive.len(), 3);
    assert_eq!(&*archive.read("a.txt").unwrap().data, b"stored bytes");
    assert_eq!(
        archive.read("word/document.xml").unwrap().data.len(),
        16 * 5000
    );
}

/// APPNOTE §4.4.7, §4.1.5: a damaged entry fails its CRC-32 check and the
/// lenient read keeps its bytes.
#[test]
fn corrupted_bytes_fail_the_crc_check() {
    let mut data = sample(ZipOptions::default());
    let at = memchr::memmem::find(&data, b"stored bytes").unwrap();
    data[at] = b'S';
    let archive = Archive::new(&data).unwrap();
    assert_eq!(
        archive.entry("a.txt").unwrap_err(),
        Error::CrcMismatch("a.txt".into())
    );
    let lenient = archive.read("a.txt").unwrap();
    assert!(!lenient.crc_ok);
    assert_eq!(&*lenient.data, b"Stored bytes");
}

#[test]
fn size_limits_stop_decompression_bombs() {
    let zeros = vec![0u8; 8 << 20];
    let data = ZipWriter::new().deflated("bomb", &zeros).finish();
    let tight = Limits {
        max_entry_size: 1 << 20,
        ..Limits::default()
    };
    let archive = Archive::with_limits(&data, tight).unwrap();
    assert!(matches!(archive.entry("bomb"), Err(Error::TooLarge { .. })));
    let ratio = Limits {
        max_ratio: 10,
        ratio_floor: 1 << 20,
        ..Limits::default()
    };
    let archive = Archive::with_limits(&data, ratio).unwrap();
    assert!(matches!(archive.entry("bomb"), Err(Error::Bomb { .. })));
}

/// APPNOTE Appendix D: names without bit 11 are code page 437; with it,
/// UTF-8.
/// APPNOTE §4.4.4, §4.4.17.
#[test]
fn decodes_name_encodings() {
    let cp437 = ZipWriter::new().raw_name(b"caf\x82.txt", b"x").finish();
    assert_eq!(
        Archive::new(&cp437).unwrap().names().next(),
        Some("café.txt")
    );
    let utf8 = ZipWriter::with_options(ZipOptions {
        utf8: true,
        ..ZipOptions::default()
    })
    .raw_name("café.txt".as_bytes(), b"x")
    .finish();
    assert_eq!(
        Archive::new(&utf8).unwrap().names().next(),
        Some("café.txt")
    );
}

#[test]
fn truncated_archives_never_panic() {
    let data = sample(ZipOptions::default());
    for len in (0..data.len()).step_by(7) {
        let Ok(archive) = Archive::new(&data[..len]) else {
            continue;
        };
        for entry in archive.entries() {
            let _ = archive.read_entry(entry);
        }
    }
}

/// A deflate stream cut short keeps what it inflated, flagged as failing
/// its CRC check.
#[test]
fn truncated_deflate_keeps_the_readable_prefix() {
    let text: Vec<u8> = (0..20000u32)
        .flat_map(|i| i.to_string().into_bytes())
        .collect();
    let data = ZipWriter::new().deflated("t", &text).finish();
    let archive = Archive::new(&data).unwrap();
    let entry = archive.entries()[0].clone();
    let mut short = entry.clone();
    short.compressed_size /= 2;
    let contents = archive.read_entry(&short).unwrap();
    assert!(!contents.crc_ok);
    assert!(!contents.data.is_empty());
    assert!(text.starts_with(&contents.data));
}

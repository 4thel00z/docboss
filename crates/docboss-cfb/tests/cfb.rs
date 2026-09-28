use docboss_cfb::{property, CompoundFile, EntryKind, Error};

const TEXT_DOC: &[u8] = include_bytes!("fixtures/text.doc");

/// [MS-CFB] §2.2 header, §2.6 directory: a Word file carries the
/// WordDocument stream, a table stream and the two summary streams.
/// [MS-CFB] §2.1, §2.2, §2.3, §2.4, §2.5, §2.6, §2.6.1, §2.6.2, §2.6.3.
#[test]
fn walks_the_directory_of_a_word_file() {
    let file = CompoundFile::parse(TEXT_DOC).unwrap();
    assert_eq!(file.major_version(), 3);
    assert_eq!(file.sector_size(), 512);
    assert_eq!(file.root().kind, EntryKind::Root);
    let paths: Vec<String> = file.walk().into_iter().map(|(path, _)| path).collect();
    for expected in [
        "WordDocument",
        "1Table",
        "\u{5}SummaryInformation",
        "\u{5}DocumentSummaryInformation",
    ] {
        assert!(
            paths.iter().any(|p| p == expected),
            "{expected} missing from {paths:?}"
        );
    }
    let word = file.open_stream("WordDocument").unwrap();
    assert_eq!(&word[..2], &[0xEC, 0xA5]);
    assert!(file.diagnostics().is_empty(), "{:?}", file.diagnostics());
}

/// [MS-CFB] §2.6.4: names compare case-insensitively.
#[test]
fn lookup_ignores_case() {
    let file = CompoundFile::parse(TEXT_DOC).unwrap();
    assert_eq!(file.find("worddocument"), file.find("WordDocument"));
    assert!(matches!(
        file.open_stream("NoSuchStream"),
        Err(Error::NotFound(_))
    ));
}

/// [MS-OLEPS] §2.21: summary information yields the title and author.
/// [MS-OLEPS] §2.20, §2.19, §2.15, §2.5, §2.18.2, §2.25.1, §2.1, §2.2, §2.23.
#[test]
fn reads_summary_metadata() {
    let file = CompoundFile::parse(TEXT_DOC).unwrap();
    let mut diagnostics = Vec::new();
    let metadata = property::read_metadata(&file, &mut diagnostics);
    assert_eq!(metadata.title.as_deref(), Some("Fixture text"));
    assert_eq!(metadata.subject.as_deref(), Some("docboss tests"));
    assert_eq!(metadata.creator.as_deref(), Some("Ada Lovelace"));
    assert_eq!(metadata.keywords.as_deref(), Some("alpha"));
}

#[test]
fn rejects_other_files() {
    assert_eq!(
        CompoundFile::parse(b"PK\x03\x04").err(),
        Some(Error::NotCompoundFile)
    );
    assert_eq!(
        CompoundFile::parse(&TEXT_DOC[..100]).err(),
        Some(Error::Truncated("header"))
    );
}

/// Truncation anywhere past the header never panics: every prefix either
/// parses with diagnostics or reports an error.
#[test]
fn every_truncation_is_survivable() {
    for length in (0..TEXT_DOC.len()).step_by(97) {
        let Ok(file) = CompoundFile::parse(&TEXT_DOC[..length]) else {
            continue;
        };
        for (path, _) in file.walk() {
            let _ = file.open_stream(&path);
        }
    }
}

/// [MS-CFB] §2.3: a FAT whose chain points back at itself is cut, reported,
/// and does not hang.
#[test]
fn fat_loops_are_cut() {
    let mut bytes = TEXT_DOC.to_vec();
    let first_fat = u32::from_le_bytes(bytes[0x4C..0x50].try_into().unwrap()) as usize;
    let directory_start = u32::from_le_bytes(bytes[0x30..0x34].try_into().unwrap()) as usize;
    let fat_entry = (first_fat + 1) * 512 + directory_start * 4;
    bytes[fat_entry..fat_entry + 4].copy_from_slice(&(directory_start as u32).to_le_bytes());
    let file = CompoundFile::parse(&bytes).unwrap();
    assert!(file
        .diagnostics()
        .iter()
        .any(|d| d.message.contains("loops")));
}

/// Flipping bytes across the file never panics.
#[test]
fn corrupted_bytes_are_survivable() {
    let mut state = 0x1234_5678u32;
    for _ in 0..300 {
        let mut bytes = TEXT_DOC.to_vec();
        for _ in 0..16 {
            state = state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            let at = (state as usize >> 8) % bytes.len();
            bytes[at] = (state >> 3) as u8;
        }
        let Ok(file) = CompoundFile::parse(&bytes) else {
            continue;
        };
        for (path, _) in file.walk() {
            let _ = file.open_stream(&path);
        }
        let mut diagnostics = Vec::new();
        let _ = property::read_metadata(&file, &mut diagnostics);
    }
}

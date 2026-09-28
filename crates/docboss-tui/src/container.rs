//! The container a document is stored in: the entries of a ZIP package or
//! the streams of a compound file.

use docboss_cfb::CompoundFile;
use docboss_zip::Archive;

/// One part of a container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Part {
    /// The entry name or the `/`-separated stream path as stored.
    pub path: String,
    pub size: u64,
    /// The stored size, for compressed ZIP entries.
    pub stored: Option<u64>,
}

impl Part {
    /// The name with control characters written as `\xNN`, so
    /// `\u{5}SummaryInformation` shows as `\x05SummaryInformation`.
    pub fn display_name(&self) -> String {
        self.path
            .chars()
            .map(|c| match c.is_control() {
                true => format!("\\x{:02x}", c as u32),
                false => c.to_string(),
            })
            .collect()
    }

    /// Whether the part holds XML, judged by its name.
    pub fn is_xml(&self) -> bool {
        let lower = self.path.to_ascii_lowercase();
        lower.ends_with(".xml") || lower.ends_with(".rels")
    }
}

/// Which container the file is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Zip,
    CompoundFile,
    Unknown,
}

/// Every part of the container in stored order, or an empty list when the
/// bytes are neither a ZIP archive nor a compound file.
pub fn parts(bytes: &[u8]) -> (Kind, Vec<Part>) {
    if docboss_cfb::is_compound_file(bytes) {
        let Ok(file) = CompoundFile::parse(bytes) else {
            return (Kind::CompoundFile, Vec::new());
        };
        let parts = file
            .walk()
            .into_iter()
            .map(|(path, size)| Part {
                path,
                size,
                stored: None,
            })
            .collect();
        return (Kind::CompoundFile, parts);
    }
    let Ok(archive) = Archive::new(bytes) else {
        return (Kind::Unknown, Vec::new());
    };
    let parts = archive
        .entries()
        .iter()
        .filter(|entry| !entry.is_dir())
        .map(|entry| Part {
            path: entry.name.clone(),
            size: entry.uncompressed_size,
            stored: Some(entry.compressed_size),
        })
        .collect();
    (Kind::Zip, parts)
}

/// The bytes of one part.
pub fn part_bytes(bytes: &[u8], part: &Part) -> Result<Vec<u8>, String> {
    if docboss_cfb::is_compound_file(bytes) {
        let file = CompoundFile::parse(bytes).map_err(|e| format!("compound file: {e}"))?;
        return file
            .open_stream(&part.path)
            .map(|data| data.into_owned())
            .map_err(|e| format!("{}: {e}", part.display_name()));
    }
    let archive = Archive::new(bytes).map_err(|e| format!("zip: {e}"))?;
    archive
        .entry(&part.path)
        .map(|data| data.into_owned())
        .map_err(|e| format!("{}: {e}", part.path))
}

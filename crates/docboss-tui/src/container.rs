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
        display_name(&self.path)
    }

    /// Whether the part holds XML, judged by its name.
    pub fn is_xml(&self) -> bool {
        let lower = self.path.to_ascii_lowercase();
        lower.ends_with(".xml") || lower.ends_with(".rels")
    }
}

/// A stored path with control characters written as `\xNN`.
pub fn display_name(path: &str) -> String {
    path.chars()
        .map(|c| match c.is_control() {
            true => format!("\\x{:02x}", c as u32),
            false => c.to_string(),
        })
        .collect()
}

/// The part a user-typed name refers to: the stored path itself, its
/// `\xNN`-escaped display form, or the path without leading control
/// characters compared case-insensitively (`SummaryInformation` for
/// `\u{5}SummaryInformation`). A leading `/` is ignored.
pub fn find(bytes: &[u8], name: &str) -> Result<Part, String> {
    let name = name.trim_start_matches('/');
    let (kind, parts) = parts(bytes);
    if kind == Kind::Unknown {
        return Err("neither a ZIP package nor a compound file".to_string());
    }
    let strip = |path: &str| -> String {
        path.split('/')
            .map(|segment| segment.trim_start_matches(char::is_control))
            .collect::<Vec<_>>()
            .join("/")
    };
    parts
        .iter()
        .find(|part| part.path == name)
        .or_else(|| parts.iter().find(|part| part.display_name() == name))
        .or_else(|| {
            parts
                .iter()
                .find(|part| strip(&part.path).eq_ignore_ascii_case(name))
        })
        .cloned()
        .ok_or_else(|| format!("{name}: no such part"))
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

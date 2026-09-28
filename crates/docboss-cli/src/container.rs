//! The container a document is stored in: the entries of a ZIP package or
//! the streams of a compound file, for `parts`, `hex` and `xml`.

use docboss_cfb::CompoundFile;
use docboss_zip::Archive;

/// One part of a container.
pub struct Part {
    pub name: String,
    pub size: u64,
    /// The stored size, for compressed ZIP entries.
    pub stored: Option<u64>,
}

enum Container<'a> {
    Zip(Archive<'a>),
    Cfb(CompoundFile<'a>),
}

fn open(bytes: &[u8]) -> Result<Container<'_>, String> {
    if docboss_cfb::is_compound_file(bytes) {
        return CompoundFile::parse(bytes)
            .map(Container::Cfb)
            .map_err(|e| format!("compound file: {e}"));
    }
    Archive::new(bytes)
        .map(Container::Zip)
        .map_err(|e| format!("zip: {e}"))
}

/// Every part of the container, in stored order.
pub fn parts(bytes: &[u8]) -> Result<Vec<Part>, String> {
    let parts = match open(bytes)? {
        Container::Zip(archive) => archive
            .entries()
            .iter()
            .filter(|entry| !entry.is_dir())
            .map(|entry| Part {
                name: entry.name.clone(),
                size: entry.uncompressed_size,
                stored: Some(entry.compressed_size),
            })
            .collect(),
        Container::Cfb(file) => file
            .walk()
            .into_iter()
            .map(|(name, size)| Part {
                name: display_name(&name),
                size,
                stored: None,
            })
            .collect(),
    };
    Ok(parts)
}

/// The bytes of one part: a ZIP entry by name, or a compound file stream
/// by its `/`-separated path.
pub fn part_bytes(bytes: &[u8], name: &str) -> Result<Vec<u8>, String> {
    let name = name.trim_start_matches('/');
    match open(bytes)? {
        Container::Zip(archive) => archive
            .entry(name)
            .map(|data| data.into_owned())
            .map_err(|e| format!("{name}: {e}")),
        Container::Cfb(file) => file
            .open_stream(&stream_path(&file, name))
            .map(|data| data.into_owned())
            .map_err(|e| format!("{name}: {e}")),
    }
}

/// A stream path with control characters written as `\xNN`, so
/// `\u{5}SummaryInformation` prints as `\x05SummaryInformation`.
pub fn display_name(name: &str) -> String {
    name.chars()
        .map(|c| match c.is_control() {
            true => format!("\\x{:02x}", c as u32),
            false => c.to_string(),
        })
        .collect()
}

/// The stored path a user-typed name refers to: the name itself, its
/// `\xNN`-escaped display form, or the name without a leading control
/// character (`SummaryInformation` for `\u{5}SummaryInformation`).
fn stream_path(file: &CompoundFile<'_>, name: &str) -> String {
    let strip = |path: &str| -> String {
        path.split('/')
            .map(|segment| segment.trim_start_matches(char::is_control))
            .collect::<Vec<_>>()
            .join("/")
    };
    file.walk()
        .into_iter()
        .map(|(path, _)| path)
        .find(|path| {
            path == name || display_name(path) == name || strip(path).eq_ignore_ascii_case(name)
        })
        .unwrap_or_else(|| name.to_string())
}

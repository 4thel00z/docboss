//! OLE property sets ([MS-OLEPS]): the `\u{5}SummaryInformation` and
//! `\u{5}DocumentSummaryInformation` streams that carry document metadata.

use docboss_model::{Diagnostic, Metadata};

use crate::codepage::{self, Fidelity};
use crate::time::filetime_to_iso8601;
use crate::{u16_at, u32_at, u64_at, CompoundFile};

/// FMTID_SummaryInformation, as stored (little-endian GUID fields).
pub const SUMMARY_INFORMATION: [u8; 16] = [
    0xE0, 0x85, 0x9F, 0xF2, 0xF9, 0x4F, 0x68, 0x10, 0xAB, 0x91, 0x08, 0x00, 0x2B, 0x27, 0xB3, 0xD9,
];
/// FMTID_DocSummaryInformation, as stored.
pub const DOC_SUMMARY_INFORMATION: [u8; 16] = [
    0x02, 0xD5, 0xCD, 0xD5, 0x9C, 0x2E, 0x1B, 0x10, 0x93, 0x97, 0x08, 0x00, 0x2B, 0x2C, 0xF9, 0xAE,
];

/// A property value of the types metadata uses ([MS-OLEPS] §2.15).
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    I16(i16),
    I32(i32),
    U32(u32),
    Bool(bool),
    Text(String),
    FileTime(u64),
    /// A type docboss does not decode, with its VARTYPE.
    Other(u16),
}

/// One property set: its format id and `(property id, value)` pairs.
#[derive(Debug, Clone, PartialEq)]
pub struct PropertySet {
    pub fmtid: [u8; 16],
    pub code_page: u32,
    pub properties: Vec<(u32, Value)>,
}

impl PropertySet {
    pub fn get(&self, id: u32) -> Option<&Value> {
        self.properties
            .iter()
            .find(|(pid, _)| *pid == id)
            .map(|(_, value)| value)
    }

    fn text(&self, id: u32) -> Option<String> {
        match self.get(id)? {
            Value::Text(text) if !text.is_empty() => Some(text.clone()),
            _ => None,
        }
    }

    fn count(&self, id: u32) -> Option<u32> {
        match self.get(id)? {
            Value::I32(n) if *n >= 0 => Some(*n as u32),
            Value::U32(n) => Some(*n),
            Value::I16(n) if *n >= 0 => Some(*n as u32),
            _ => None,
        }
    }

    fn date(&self, id: u32) -> Option<String> {
        match self.get(id)? {
            Value::FileTime(t) => filetime_to_iso8601(*t),
            _ => None,
        }
    }
}

/// Parses a property set stream ([MS-OLEPS] §2.21 PropertySetStream).
/// Only the first two sets are read; the second holds user-defined
/// properties in DocumentSummaryInformation and is kept as a second entry.
pub fn parse_stream(
    bytes: &[u8],
    diagnostics: &mut Vec<Diagnostic>,
    location: &str,
) -> Vec<PropertySet> {
    if u16_at(bytes, 0) != Some(0xFFFE) {
        diagnostics.push(Diagnostic::dropped(
            location,
            "property set stream has no byte order mark",
        ));
        return Vec::new();
    }
    let count = u32_at(bytes, 24).unwrap_or(0).min(2) as usize;
    (0..count)
        .filter_map(|i| {
            let at = 28 + i * 20;
            let fmtid: [u8; 16] = bytes.get(at..at + 16)?.try_into().ok()?;
            let offset = u32_at(bytes, at + 16)? as usize;
            parse_set(bytes, offset, fmtid, diagnostics, location)
        })
        .collect()
}

/// [MS-OLEPS] §2.20 PropertySet.
fn parse_set(
    bytes: &[u8],
    offset: usize,
    fmtid: [u8; 16],
    diagnostics: &mut Vec<Diagnostic>,
    location: &str,
) -> Option<PropertySet> {
    let size = u32_at(bytes, offset)? as usize;
    let set = bytes.get(offset..offset.checked_add(size)?.min(bytes.len()))?;
    let count = (u32_at(set, 4)? as usize).min(set.len() / 8);
    let entries: Vec<(u32, usize)> = (0..count)
        .filter_map(|i| Some((u32_at(set, 8 + i * 8)?, u32_at(set, 12 + i * 8)? as usize)))
        .collect();
    let code_page = entries
        .iter()
        .find(|(id, _)| *id == 1)
        .and_then(|(_, at)| u16_at(set, at + 4))
        .map_or(1252, u32::from);
    let mut properties = Vec::new();
    for (id, at) in entries {
        if id == 0 || id == 1 {
            continue;
        }
        match parse_value(set, at, code_page) {
            Some((value, fidelity)) => {
                if fidelity == Fidelity::Approximated {
                    diagnostics.push(Diagnostic::approximated(
                        location,
                        format!("code page {code_page} is not supported; property {id} read as ISO 8859-1"),
                    ));
                }
                properties.push((id, value));
            }
            None => diagnostics.push(Diagnostic::dropped(
                location,
                format!("property {id} is truncated"),
            )),
        }
    }
    Some(PropertySet {
        fmtid,
        code_page,
        properties,
    })
}

/// [MS-OLEPS] §2.15 TypedPropertyValue.
fn parse_value(set: &[u8], at: usize, code_page: u32) -> Option<(Value, Fidelity)> {
    let kind = u16_at(set, at)?;
    let body = at + 4;
    let exact = |value| Some((value, Fidelity::Exact));
    match kind {
        0x02 => exact(Value::I16(u16_at(set, body)? as i16)),
        0x03 => exact(Value::I32(u32_at(set, body)? as i32)),
        0x13 => exact(Value::U32(u32_at(set, body)?)),
        0x0B => exact(Value::Bool(u16_at(set, body)? != 0)),
        0x40 => exact(Value::FileTime(u64_at(set, body)?)),
        0x1E => {
            let length = u32_at(set, body)? as usize;
            let raw = set.get(body + 4..(body + 4).checked_add(length)?.min(set.len()))?;
            let (text, fidelity) = if code_page == 1200 {
                (codepage::utf16le(raw), Fidelity::Exact)
            } else {
                codepage::decode(code_page, raw)
            };
            Some((
                Value::Text(text.trim_end_matches('\0').to_string()),
                fidelity,
            ))
        }
        0x1F => {
            let length = (u32_at(set, body)? as usize).checked_mul(2)?;
            let raw = set.get(body + 4..(body + 4).checked_add(length)?.min(set.len()))?;
            exact(Value::Text(
                codepage::utf16le(raw).trim_end_matches('\0').to_string(),
            ))
        }
        other => exact(Value::Other(other)),
    }
}

/// Reads both summary streams of a compound file into [`Metadata`],
/// leaving fields the file does not state as `None`.
pub fn read_metadata(file: &CompoundFile<'_>, diagnostics: &mut Vec<Diagnostic>) -> Metadata {
    let mut metadata = Metadata::default();
    if let Ok(stream) = file.open_stream("\u{5}SummaryInformation") {
        let sets = parse_stream(&stream, diagnostics, "\u{5}SummaryInformation");
        if let Some(set) = sets.iter().find(|set| set.fmtid == SUMMARY_INFORMATION) {
            metadata.title = set.text(2);
            metadata.subject = set.text(3);
            metadata.creator = set.text(4);
            metadata.keywords = set.text(5);
            metadata.description = set.text(6);
            metadata.last_modified_by = set.text(8);
            metadata.revision = set.text(9);
            metadata.created = set.date(12);
            metadata.modified = set.date(13);
            metadata.pages = set.count(14);
            metadata.words = set.count(15);
            metadata.characters = set.count(16);
            metadata.application = set.text(18);
        }
    }
    if let Ok(stream) = file.open_stream("\u{5}DocumentSummaryInformation") {
        let sets = parse_stream(&stream, diagnostics, "\u{5}DocumentSummaryInformation");
        if let Some(set) = sets.iter().find(|set| set.fmtid == DOC_SUMMARY_INFORMATION) {
            metadata.category = set.text(2);
            metadata.company = set.text(15);
        }
    }
    metadata
}

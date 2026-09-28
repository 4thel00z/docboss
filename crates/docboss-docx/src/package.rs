//! The Open Packaging Conventions layer (ECMA-376 Part 2): content types,
//! relationships and part name resolution.

use std::collections::HashMap;

use docboss_model::Diagnostic;
use docboss_xml::{decode, Event, Ns, Reader};
use docboss_zip::Archive;

/// One relationship of a part (ECMA-376 Part 2 §6.5.3.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relationship {
    pub id: String,
    /// The last segment of the relationship type URI, such as `styles`;
    /// transitional and strict type URIs end the same way.
    pub kind: String,
    /// The resolved part name without a leading `/`, or the target URI as
    /// written for an external relationship.
    pub target: String,
    pub external: bool,
}

/// The relationships of one source part, by id.
#[derive(Debug, Clone, Default)]
pub struct Relationships {
    pub list: Vec<Relationship>,
    by_id: HashMap<String, usize>,
}

impl Relationships {
    pub fn get(&self, id: &str) -> Option<&Relationship> {
        self.by_id.get(id).map(|&i| &self.list[i])
    }

    pub fn of_kind<'a>(&'a self, kind: &'a str) -> impl Iterator<Item = &'a Relationship> + 'a {
        self.list
            .iter()
            .filter(move |rel| rel.kind.eq_ignore_ascii_case(kind))
    }

    pub fn first(&self, kind: &str) -> Option<&Relationship> {
        self.list
            .iter()
            .find(|rel| !rel.external && rel.kind.eq_ignore_ascii_case(kind))
    }
}

/// An opened package.
pub struct Package<'a> {
    pub archive: Archive<'a>,
    overrides: HashMap<String, String>,
    defaults: HashMap<String, String>,
    pub diagnostics: Vec<Diagnostic>,
}

/// The directory of a part name: `word/document.xml` gives `word`.
fn directory(part: &str) -> &str {
    part.rfind('/').map_or("", |i| &part[..i])
}

/// Resolves a relationship target against its source part (ECMA-376 Part 2
/// §6.4, with the relative reference resolution of RFC 3986 §5.2).
pub fn resolve_target(source: &str, target: &str) -> String {
    let target = target
        .split(['#', '?'])
        .next()
        .unwrap_or("")
        .replace('\\', "/");
    let target = percent_decode(&target);
    let joined = match target.strip_prefix('/') {
        Some(absolute) => absolute.to_string(),
        None => {
            let base = directory(source);
            if base.is_empty() {
                target
            } else {
                format!("{base}/{target}")
            }
        }
    };
    let mut segments: Vec<&str> = Vec::new();
    for segment in joined.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            other => segments.push(other),
        }
    }
    segments.join("/")
}

fn percent_decode(text: &str) -> String {
    if !text.contains('%') {
        return text.to_string();
    }
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes
            .get(i + 1..i + 3)
            .and_then(|h| std::str::from_utf8(h).ok());
        match (bytes[i], hex.and_then(|h| u8::from_str_radix(h, 16).ok())) {
            (b'%', Some(byte)) => {
                out.push(byte);
                i += 3;
            }
            (byte, _) => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The relationships part of a source part: `word/document.xml` gives
/// `word/_rels/document.xml.rels`, and the package itself (`""`) gives
/// `_rels/.rels` (ECMA-376 Part 2 §6.5.2).
pub fn rels_name(source: &str) -> String {
    let dir = directory(source);
    let file = &source[source.rfind('/').map_or(0, |i| i + 1)..];
    match dir {
        "" => format!("_rels/{file}.rels"),
        _ => format!("{dir}/_rels/{file}.rels"),
    }
}

impl<'a> Package<'a> {
    pub fn open(archive: Archive<'a>) -> Self {
        let mut package = Self {
            diagnostics: archive.diagnostics().to_vec(),
            archive,
            overrides: HashMap::new(),
            defaults: HashMap::new(),
        };
        package.read_content_types();
        package
    }

    /// The bytes of a part, or `None` when it is absent or unreadable. A
    /// CRC mismatch keeps the bytes and is reported.
    pub fn part(&self, name: &str) -> Option<(std::borrow::Cow<'a, [u8]>, Option<Diagnostic>)> {
        let contents = match self.archive.read(name) {
            Ok(contents) => contents,
            Err(docboss_zip::Error::NotFound(_)) => return None,
            Err(error) => {
                return Some((
                    Default::default(),
                    Some(Diagnostic::dropped(name, error.to_string())),
                ))
            }
        };
        let warning = (!contents.crc_ok).then(|| {
            Diagnostic::approximated(name, "the part fails its CRC-32 check; its bytes were kept")
        });
        Some((contents.data, warning))
    }

    /// The bytes of a part, recording any problem in `diagnostics`.
    pub fn part_into(
        &self,
        name: &str,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Option<std::borrow::Cow<'a, [u8]>> {
        let (data, warning) = self.part(name)?;
        diagnostics.extend(warning);
        Some(data)
    }

    /// Reads `[Content_Types].xml` (ECMA-376 Part 2 §7.2.3.2).
    fn read_content_types(&mut self) {
        let mut diagnostics = Vec::new();
        let Some(data) = self.part_into("[Content_Types].xml", &mut diagnostics) else {
            self.diagnostics.push(Diagnostic::approximated(
                "[Content_Types].xml",
                "the package has no content types part",
            ));
            return;
        };
        self.diagnostics.extend(diagnostics);
        let text = decode(&data);
        let mut reader = Reader::new(&text);
        loop {
            match reader.next_event() {
                Event::Start(e) if e.local == "Override" => {
                    let (Some(part), Some(kind)) = (
                        e.attr(Ns::NONE, "PartName"),
                        e.attr(Ns::NONE, "ContentType"),
                    ) else {
                        continue;
                    };
                    self.overrides.insert(
                        part.trim_start_matches('/').to_ascii_lowercase(),
                        kind.into_owned(),
                    );
                }
                Event::Start(e) if e.local == "Default" => {
                    let (Some(ext), Some(kind)) = (
                        e.attr(Ns::NONE, "Extension"),
                        e.attr(Ns::NONE, "ContentType"),
                    ) else {
                        continue;
                    };
                    self.defaults
                        .insert(ext.to_ascii_lowercase(), kind.into_owned());
                }
                Event::Eof => return,
                _ => {}
            }
        }
    }

    /// The content type of a part, from its override or its extension
    /// (ECMA-376 Part 2 §7.2.3.5, §6.2.3).
    pub fn content_type(&self, part: &str) -> Option<&str> {
        let key = part.trim_start_matches('/').to_ascii_lowercase();
        if let Some(kind) = self.overrides.get(&key) {
            return Some(kind);
        }
        let ext = key.rsplit_once('.')?.1;
        self.defaults.get(ext).map(String::as_str)
    }

    /// The part with an overridden content type ending in `suffix`.
    pub fn part_with_type(&self, suffix: &str) -> Option<String> {
        let mut names: Vec<&String> = self
            .overrides
            .iter()
            .filter(|(_, kind)| kind.ends_with(suffix))
            .map(|(name, _)| name)
            .collect();
        names.sort();
        let name = names.first()?;
        self.archive.find(name).map(|entry| entry.name.clone())
    }

    /// The relationships of `source` (`""` for the package), empty when it
    /// has none (ECMA-376 Part 2 §6.5.2, §6.5.3).
    pub fn relationships(&self, source: &str, diagnostics: &mut Vec<Diagnostic>) -> Relationships {
        let name = rels_name(source);
        let Some(data) = self.part_into(&name, diagnostics) else {
            return Relationships::default();
        };
        let text = decode(&data);
        let mut reader = Reader::new(&text);
        let mut rels = Relationships::default();
        loop {
            match reader.next_event() {
                Event::Start(e) if e.local == "Relationship" => {
                    let (Some(id), Some(kind), Some(target)) = (
                        e.attr(Ns::NONE, "Id"),
                        e.attr(Ns::NONE, "Type"),
                        e.attr(Ns::NONE, "Target"),
                    ) else {
                        continue;
                    };
                    let external = e
                        .attr(Ns::NONE, "TargetMode")
                        .is_some_and(|mode| mode.eq_ignore_ascii_case("External"));
                    let target = match external {
                        true => target.into_owned(),
                        false => self.existing_name(&resolve_target(source, &target)),
                    };
                    let kind = kind.rsplit('/').next().unwrap_or("").to_string();
                    rels.by_id.entry(id.to_string()).or_insert(rels.list.len());
                    rels.list.push(Relationship {
                        id: id.into_owned(),
                        kind,
                        target,
                        external,
                    });
                }
                Event::Eof => return rels,
                _ => {}
            }
        }
    }

    /// The archive's spelling of a part name, which may differ in case:
    /// part names compare case-insensitively (ECMA-376 Part 2 §6.2.2.3, §7.3.5).
    fn existing_name(&self, name: &str) -> String {
        self.archive
            .find(name)
            .map_or_else(|| name.to_string(), |entry| entry.name.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ECMA-376 Part 2 §6.4.3: targets resolve relative to the source
    /// part's directory; `..` climbs and a leading `/` is absolute.
    #[test]
    fn resolves_targets() {
        assert_eq!(
            resolve_target("word/document.xml", "media/image1.png"),
            "word/media/image1.png"
        );
        assert_eq!(
            resolve_target("word/document.xml", "../customXml/item1.xml"),
            "customXml/item1.xml"
        );
        assert_eq!(
            resolve_target("word/document.xml", "/word/styles.xml"),
            "word/styles.xml"
        );
        assert_eq!(resolve_target("", "word/document.xml"), "word/document.xml");
        assert_eq!(
            resolve_target("word/document.xml", "media/my%20image.png"),
            "word/media/my image.png"
        );
        assert_eq!(
            resolve_target("word/document.xml", "./a.xml#frag"),
            "word/a.xml"
        );
    }

    /// ECMA-376 Part 2 §6.5.2: the relationships part of a source part.
    #[test]
    fn names_relationship_parts() {
        assert_eq!(rels_name(""), "_rels/.rels");
        assert_eq!(
            rels_name("word/document.xml"),
            "word/_rels/document.xml.rels"
        );
    }
}

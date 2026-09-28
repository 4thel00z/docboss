//! Relationships and content types of the package (ECMA-376 Part 2 §6.5
//! relationships, §7.2.3 content types).

use crate::xml::Xml;

pub const NS_W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
pub const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
pub const NS_WP: &str = "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing";
pub const NS_A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
pub const NS_PIC: &str = "http://schemas.openxmlformats.org/drawingml/2006/picture";
const NS_RELS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const NS_TYPES: &str = "http://schemas.openxmlformats.org/package/2006/content-types";

const OFFICE_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/";

/// A relationship type by its short name: `styles` becomes the full
/// officeDocument relationship URI.
pub fn office_rel(kind: &str) -> String {
    format!("{OFFICE_REL}{kind}")
}

pub const CORE_PROPERTIES_REL: &str =
    "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties";

pub const WML: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.";

struct Relationship {
    id: String,
    kind: String,
    target: String,
    external: bool,
}

/// The relationships of one part, numbered `rId1`, `rId2`, ... in the
/// order they are added. Adding an identical relationship twice returns
/// the first id.
#[derive(Default)]
pub struct Rels {
    items: Vec<Relationship>,
}

impl Rels {
    pub fn add(&mut self, kind: &str, target: &str, external: bool) -> String {
        let existing = self
            .items
            .iter()
            .find(|rel| rel.kind == kind && rel.target == target && rel.external == external);
        if let Some(rel) = existing {
            return rel.id.clone();
        }
        let id = format!("rId{}", self.items.len() + 1);
        self.items.push(Relationship {
            id: id.clone(),
            kind: kind.to_string(),
            target: target.to_string(),
            external,
        });
        id
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn to_xml(&self) -> Vec<u8> {
        let mut xml = Xml::new();
        xml.open("Relationships", &[("xmlns", NS_RELS)]);
        for rel in &self.items {
            let mut attributes = vec![
                ("Id", rel.id.as_str()),
                ("Type", rel.kind.as_str()),
                ("Target", rel.target.as_str()),
            ];
            if rel.external {
                attributes.push(("TargetMode", "External"));
            }
            xml.empty("Relationship", &attributes);
        }
        xml.close("Relationships");
        xml.finish()
    }
}

/// The `[Content_Types].xml` part: defaults by extension and overrides by
/// part name.
#[derive(Default)]
pub struct ContentTypes {
    defaults: Vec<(String, String)>,
    overrides: Vec<(String, String)>,
}

impl ContentTypes {
    pub fn new() -> Self {
        let mut types = Self::default();
        types.add_default(
            "rels",
            "application/vnd.openxmlformats-package.relationships+xml",
        );
        types.add_default("xml", "application/xml");
        types
    }

    pub fn add_default(&mut self, extension: &str, content_type: &str) {
        if self.defaults.iter().any(|(ext, _)| ext == extension) {
            return;
        }
        self.defaults
            .push((extension.to_string(), content_type.to_string()));
    }

    pub fn default_type(&self, extension: &str) -> Option<&str> {
        self.defaults
            .iter()
            .find(|(ext, _)| ext == extension)
            .map(|(_, ty)| ty.as_str())
    }

    /// `part` is the absolute part name, such as `/word/document.xml`.
    pub fn override_part(&mut self, part: &str, content_type: &str) {
        self.overrides
            .push((part.to_string(), content_type.to_string()));
    }

    pub fn to_xml(&self) -> Vec<u8> {
        let mut xml = Xml::new();
        xml.open("Types", &[("xmlns", NS_TYPES)]);
        for (extension, content_type) in &self.defaults {
            xml.empty(
                "Default",
                &[("Extension", extension), ("ContentType", content_type)],
            );
        }
        for (part, content_type) in &self.overrides {
            xml.empty(
                "Override",
                &[("PartName", part), ("ContentType", content_type)],
            );
        }
        xml.close("Types");
        xml.finish()
    }
}

/// The file extension a media part is stored under, from its MIME type.
pub fn media_extension(content_type: &str) -> &'static str {
    match content_type {
        "image/png" => "png",
        "image/jpeg" => "jpeg",
        "image/gif" => "gif",
        "image/bmp" => "bmp",
        "image/tiff" => "tiff",
        "image/x-wmf" => "wmf",
        "image/x-emf" => "emf",
        "image/svg+xml" => "svg",
        _ => "bin",
    }
}

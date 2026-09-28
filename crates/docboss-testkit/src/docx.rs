use crate::ZipWriter;

/// The transitional WordprocessingML main namespace.
pub const W_NS: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

const NAMESPACES: &str = concat!(
    r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" "#,
    r#"xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" "#,
    r#"xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" "#,
    r#"xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" "#,
    r#"xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture" "#,
    r#"xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" "#,
    r#"xmlns:v="urn:schemas-microsoft-com:vml" "#,
    r#"xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml""#,
);

const REL_BASE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/";

struct Part {
    name: String,
    content_type: Option<String>,
    data: Vec<u8>,
}

/// Builds a DOCX package from WordprocessingML fragments.
///
/// `Docx::new(body)` wraps `body` (the children of `w:body`) in a
/// `w:document` with the common namespace declarations. Further parts are
/// added with their relationship from the main document.
pub struct Docx {
    body: String,
    parts: Vec<Part>,
    relationships: Vec<(String, String, String, bool)>,
}

impl Docx {
    pub fn new(body: &str) -> Self {
        Self {
            body: body.to_string(),
            parts: Vec::new(),
            relationships: Vec::new(),
        }
    }

    /// Wraps the children of a root element in that element with the common
    /// namespace declarations, such as `wrap("w:styles", "...")`.
    pub fn wrap(root: &str, children: &str) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><{root} {NAMESPACES}>{children}</{root}>"#
        )
    }

    /// Adds a part related to the main document with relationship type
    /// `kind` (the last segment, such as `styles`) and id `id`.
    pub fn part(
        mut self,
        id: &str,
        kind: &str,
        name: &str,
        content_type: &str,
        data: &[u8],
    ) -> Self {
        self.relationships.push((
            id.to_string(),
            format!("{REL_BASE}{kind}"),
            name.to_string(),
            false,
        ));
        self.parts.push(Part {
            name: format!("word/{name}"),
            content_type: Some(content_type.to_string()),
            data: data.to_vec(),
        });
        self
    }

    /// Adds an external relationship, such as a hyperlink target.
    pub fn external(mut self, id: &str, kind: &str, target: &str) -> Self {
        self.relationships.push((
            id.to_string(),
            format!("{REL_BASE}{kind}"),
            target.to_string(),
            true,
        ));
        self
    }

    pub fn styles(self, children: &str) -> Self {
        let xml = Self::wrap("w:styles", children);
        self.part(
            "rIdStyles",
            "styles",
            "styles.xml",
            &main_type("styles"),
            xml.as_bytes(),
        )
    }

    pub fn numbering(self, children: &str) -> Self {
        let xml = Self::wrap("w:numbering", children);
        self.part(
            "rIdNumbering",
            "numbering",
            "numbering.xml",
            &main_type("numbering"),
            xml.as_bytes(),
        )
    }

    pub fn footnotes(self, children: &str) -> Self {
        let xml = Self::wrap("w:footnotes", children);
        self.part(
            "rIdFootnotes",
            "footnotes",
            "footnotes.xml",
            &main_type("footnotes"),
            xml.as_bytes(),
        )
    }

    pub fn endnotes(self, children: &str) -> Self {
        let xml = Self::wrap("w:endnotes", children);
        self.part(
            "rIdEndnotes",
            "endnotes",
            "endnotes.xml",
            &main_type("endnotes"),
            xml.as_bytes(),
        )
    }

    pub fn comments(self, children: &str) -> Self {
        let xml = Self::wrap("w:comments", children);
        self.part(
            "rIdComments",
            "comments",
            "comments.xml",
            &main_type("comments"),
            xml.as_bytes(),
        )
    }

    pub fn header(self, id: &str, name: &str, children: &str) -> Self {
        let xml = Self::wrap("w:hdr", children);
        self.part(id, "header", name, &main_type("header"), xml.as_bytes())
    }

    pub fn footer(self, id: &str, name: &str, children: &str) -> Self {
        let xml = Self::wrap("w:ftr", children);
        self.part(id, "footer", name, &main_type("footer"), xml.as_bytes())
    }

    pub fn image(self, id: &str, name: &str, data: &[u8]) -> Self {
        self.part(id, "image", name, "image/png", data)
    }

    /// Adds a package part with no relationship, such as `docProps/core.xml`.
    pub fn raw(mut self, name: &str, data: &[u8]) -> Self {
        self.parts.push(Part {
            name: name.to_string(),
            content_type: None,
            data: data.to_vec(),
        });
        self
    }

    pub fn build(&self) -> Vec<u8> {
        let mut types = String::from(
            r#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>"#,
        );
        for part in &self.parts {
            if let Some(content_type) = &part.content_type {
                types.push_str(&format!(
                    r#"<Override PartName="/{}" ContentType="{}"/>"#,
                    part.name, content_type
                ));
            }
        }
        types.push_str("</Types>");
        let root_rels = r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
        let mut rels = String::from(
            r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#,
        );
        for (id, kind, target, external) in &self.relationships {
            let mode = if *external {
                r#" TargetMode="External""#
            } else {
                ""
            };
            rels.push_str(&format!(
                r#"<Relationship Id="{id}" Type="{kind}" Target="{target}"{mode}/>"#
            ));
        }
        rels.push_str("</Relationships>");
        let document = Self::wrap("w:document", &format!("<w:body>{}</w:body>", self.body));
        let mut zip = ZipWriter::new();
        zip.deflated("[Content_Types].xml", types.as_bytes());
        zip.deflated("_rels/.rels", root_rels.as_bytes());
        zip.deflated("word/document.xml", document.as_bytes());
        zip.deflated("word/_rels/document.xml.rels", rels.as_bytes());
        for part in &self.parts {
            zip.deflated(&part.name, &part.data);
        }
        zip.finish()
    }
}

fn main_type(kind: &str) -> String {
    format!("application/vnd.openxmlformats-officedocument.wordprocessingml.{kind}+xml")
}

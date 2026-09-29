//! Namespace ids. The transitional and strict URIs of each Office Open XML
//! namespace (the transitional and strict conformance classes) map to one id, so a reader
//! matches `w:p` the same way in both conformance classes.

/// A resolved namespace. Ids below [`Ns::FIRST_UNKNOWN`] are the known
/// namespaces below; other URIs get ids from there up, per reader.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Ns(pub u16);

impl Ns {
    /// No namespace: unprefixed attributes, and elements outside any
    /// default namespace declaration.
    pub const NONE: Ns = Ns(0);
    pub const XML: Ns = Ns(1);
    pub const XMLNS: Ns = Ns(2);
    /// WordprocessingML main.
    pub const W: Ns = Ns(3);
    /// Office document relationships (`r:id`, `r:embed`).
    pub const R: Ns = Ns(4);
    /// DrawingML WordprocessingML drawing (`wp:inline`, `wp:anchor`).
    pub const WP: Ns = Ns(5);
    /// DrawingML main (`a:blip`, `a:graphic`, theme fonts).
    pub const A: Ns = Ns(6);
    /// DrawingML picture.
    pub const PIC: Ns = Ns(7);
    /// Markup compatibility (ECMA-376 Part 3).
    pub const MC: Ns = Ns(8);
    /// VML.
    pub const V: Ns = Ns(9);
    /// VML Office extensions.
    pub const O: Ns = Ns(10);
    /// Package relationships (`.rels` parts).
    pub const PKG_REL: Ns = Ns(11);
    /// Package content types (`[Content_Types].xml`).
    pub const CONTENT_TYPES: Ns = Ns(12);
    /// Core properties.
    pub const CP: Ns = Ns(13);
    pub const DC: Ns = Ns(14);
    pub const DCTERMS: Ns = Ns(15);
    /// Extended properties (`docProps/app.xml`).
    pub const EXTENDED: Ns = Ns(16);
    /// Office Math.
    pub const M: Ns = Ns(17);
    /// Word 2010 extensions.
    pub const W14: Ns = Ns(18);
    pub const W15: Ns = Ns(19);
    /// WordprocessingML shapes (`wps:wsp`).
    pub const WPS: Ns = Ns(20);
    /// WordprocessingML groups.
    pub const WPG: Ns = Ns(21);
    /// VML Word extensions.
    pub const W10: Ns = Ns(22);
    /// DrawingML WordprocessingML drawing, Word 2010 extensions.
    pub const WP14: Ns = Ns(23);
    pub const XSI: Ns = Ns(24);
    /// WordprocessingML drawing canvases (`wpc:wpc`).
    pub const WPC: Ns = Ns(25);

    pub const FIRST_UNKNOWN: u16 = 1000;
}

const KNOWN: &[(&str, Ns)] = &[
    ("http://www.w3.org/XML/1998/namespace", Ns::XML),
    ("http://www.w3.org/2000/xmlns/", Ns::XMLNS),
    (
        "http://schemas.openxmlformats.org/wordprocessingml/2006/main",
        Ns::W,
    ),
    ("http://purl.oclc.org/ooxml/wordprocessingml/main", Ns::W),
    (
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
        Ns::R,
    ),
    (
        "http://purl.oclc.org/ooxml/officeDocument/relationships",
        Ns::R,
    ),
    (
        "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing",
        Ns::WP,
    ),
    (
        "http://purl.oclc.org/ooxml/drawingml/wordprocessingDrawing",
        Ns::WP,
    ),
    (
        "http://schemas.openxmlformats.org/drawingml/2006/main",
        Ns::A,
    ),
    ("http://purl.oclc.org/ooxml/drawingml/main", Ns::A),
    (
        "http://schemas.openxmlformats.org/drawingml/2006/picture",
        Ns::PIC,
    ),
    ("http://purl.oclc.org/ooxml/drawingml/picture", Ns::PIC),
    (
        "http://schemas.openxmlformats.org/markup-compatibility/2006",
        Ns::MC,
    ),
    ("urn:schemas-microsoft-com:vml", Ns::V),
    ("urn:schemas-microsoft-com:office:office", Ns::O),
    ("urn:schemas-microsoft-com:office:word", Ns::W10),
    (
        "http://schemas.openxmlformats.org/package/2006/relationships",
        Ns::PKG_REL,
    ),
    (
        "http://schemas.openxmlformats.org/package/2006/content-types",
        Ns::CONTENT_TYPES,
    ),
    (
        "http://schemas.openxmlformats.org/package/2006/metadata/core-properties",
        Ns::CP,
    ),
    ("http://purl.org/dc/elements/1.1/", Ns::DC),
    ("http://purl.org/dc/terms/", Ns::DCTERMS),
    (
        "http://schemas.openxmlformats.org/officeDocument/2006/extended-properties",
        Ns::EXTENDED,
    ),
    (
        "http://purl.oclc.org/ooxml/officeDocument/extendedProperties",
        Ns::EXTENDED,
    ),
    (
        "http://schemas.openxmlformats.org/officeDocument/2006/math",
        Ns::M,
    ),
    ("http://purl.oclc.org/ooxml/officeDocument/math", Ns::M),
    (
        "http://schemas.microsoft.com/office/word/2010/wordml",
        Ns::W14,
    ),
    (
        "http://schemas.microsoft.com/office/word/2012/wordml",
        Ns::W15,
    ),
    (
        "http://schemas.microsoft.com/office/word/2010/wordprocessingShape",
        Ns::WPS,
    ),
    (
        "http://schemas.microsoft.com/office/word/2010/wordprocessingGroup",
        Ns::WPG,
    ),
    (
        "http://schemas.microsoft.com/office/word/2010/wordprocessingDrawing",
        Ns::WP14,
    ),
    ("http://www.w3.org/2001/XMLSchema-instance", Ns::XSI),
    (
        "http://schemas.microsoft.com/office/word/2010/wordprocessingCanvas",
        Ns::WPC,
    ),
];

/// The id of a known namespace URI.
pub fn known(uri: &str) -> Option<Ns> {
    KNOWN
        .iter()
        .find(|(known, _)| *known == uri)
        .map(|(_, ns)| *ns)
}

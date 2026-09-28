//! A zero-copy XML pull tokenizer with namespace resolution, built for the
//! Office Open XML parts docboss reads.
//!
//! [`Reader`] yields [`Event`]s over a `&str`. Element and attribute names
//! are resolved to [`Ns`] ids, with the transitional and strict URIs of each
//! Office namespace mapped to the same id. Text borrows from the input
//! unless an entity or line ending has to be rewritten.
//!
//! The reader is lenient: every start tag is matched by exactly one
//! [`Event::End`], even when the input closes elements out of order,
//! leaves them open at the end, or carries stray end tags. Comments,
//! processing instructions and the document type declaration are skipped.
//! It never panics on any input.

mod decode;
mod ns;

use std::borrow::Cow;
use std::rc::Rc;

use memchr::{memchr, memchr3, memmem};

pub use decode::{decode, unescape, windows1252};
pub use ns::{known, Ns};

type Scope<'a> = Rc<[(&'a str, Ns)]>;

/// A resolved element name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Name<'a> {
    pub ns: Ns,
    pub local: &'a str,
}

/// A start tag. Empty elements (`<a/>`) are reported as a start tag with
/// `empty` set, followed by their end.
#[derive(Debug, Clone)]
pub struct Element<'a> {
    pub ns: Ns,
    pub prefix: &'a str,
    pub local: &'a str,
    pub empty: bool,
    raw: &'a str,
    scope: Scope<'a>,
}

/// One attribute of a start tag, its value still escaped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Attr<'a> {
    pub ns: Ns,
    pub prefix: &'a str,
    pub local: &'a str,
    pub raw_value: &'a str,
}

impl<'a> Attr<'a> {
    /// The value with entities replaced and white space normalized.
    pub fn value(&self) -> Cow<'a, str> {
        unescape(self.raw_value, true)
    }
}

fn resolve(scope: &[(&str, Ns)], prefix: &str) -> Option<Ns> {
    scope
        .iter()
        .rev()
        .find(|(bound, _)| *bound == prefix)
        .map(|(_, ns)| *ns)
}

impl<'a> Element<'a> {
    pub fn is(&self, ns: Ns, local: &str) -> bool {
        self.ns == ns && self.local == local
    }

    pub fn name(&self) -> Name<'a> {
        Name {
            ns: self.ns,
            local: self.local,
        }
    }

    /// The namespace a prefix is bound to where this element starts; the
    /// empty prefix gives the default namespace.
    pub fn resolve_prefix(&self, prefix: &str) -> Option<Ns> {
        match prefix {
            "xml" => Some(Ns::XML),
            "xmlns" => Some(Ns::XMLNS),
            _ => resolve(&self.scope, prefix),
        }
    }

    pub fn attrs(&self) -> Attrs<'a, '_> {
        Attrs {
            rest: self.raw,
            element: self,
        }
    }

    /// The value of the attribute `ns:local`. An unprefixed attribute has
    /// no namespace, so `attr(Ns::NONE, "Id")` finds `Id="..."`.
    pub fn attr(&self, ns: Ns, local: &str) -> Option<Cow<'a, str>> {
        self.attr_raw(ns, local).map(|raw| unescape(raw, true))
    }

    /// The raw value of `ns:local` without unescaping, for values known to
    /// hold no entities such as numbers and ids.
    pub fn attr_raw(&self, ns: Ns, local: &str) -> Option<&'a str> {
        let mut rest = self.raw;
        while let Some((qname, raw_value)) = next_pair(&mut rest) {
            let (prefix, name) = split_qname(qname);
            if name != local || prefix == "xmlns" || qname == "xmlns" {
                continue;
            }
            let found = match prefix {
                "" => Ns::NONE,
                _ => self.resolve_prefix(prefix).unwrap_or(Ns::NONE),
            };
            if found == ns {
                return Some(raw_value);
            }
        }
        None
    }
}

/// The attributes of an [`Element`], namespace declarations excluded.
pub struct Attrs<'a, 'e> {
    rest: &'a str,
    element: &'e Element<'a>,
}

fn split_qname(qname: &str) -> (&str, &str) {
    match qname.split_once(':') {
        Some((prefix, local)) => (prefix, local),
        None => ("", qname),
    }
}

fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r')
}

/// Parses one `name="value"` pair from the front of `rest`. Tokens that are
/// not attributes are skipped.
fn next_pair<'a>(rest: &mut &'a str) -> Option<(&'a str, &'a str)> {
    loop {
        let trimmed = rest.trim_start_matches([' ', '\t', '\n', '\r', '/']);
        if trimmed.is_empty() {
            *rest = trimmed;
            return None;
        }
        let bytes = trimmed.as_bytes();
        let name_end = bytes
            .iter()
            .position(|&b| b == b'=' || is_space(b) || b == b'/')
            .unwrap_or(bytes.len());
        let name = &trimmed[..name_end];
        let after = trimmed[name_end..].trim_start_matches([' ', '\t', '\n', '\r']);
        let Some(after_eq) = after.strip_prefix('=') else {
            *rest = &trimmed[name_end.max(1).min(trimmed.len())..];
            continue;
        };
        let after_eq = after_eq.trim_start_matches([' ', '\t', '\n', '\r']);
        let Some(quote) = after_eq.chars().next().filter(|&c| c == '"' || c == '\'') else {
            let end = after_eq
                .find([' ', '\t', '\n', '\r'])
                .unwrap_or(after_eq.len());
            *rest = &after_eq[end..];
            if name.is_empty() {
                continue;
            }
            return Some((name, &after_eq[..end]));
        };
        let body = &after_eq[1..];
        let end = memchr(quote as u8, body.as_bytes()).unwrap_or(body.len());
        *rest = body.get(end + 1..).unwrap_or("");
        if name.is_empty() {
            continue;
        }
        return Some((name, &body[..end]));
    }
}

impl<'a> Iterator for Attrs<'a, '_> {
    type Item = Attr<'a>;

    fn next(&mut self) -> Option<Attr<'a>> {
        loop {
            let (qname, raw_value) = next_pair(&mut self.rest)?;
            if qname == "xmlns" || qname.starts_with("xmlns:") {
                continue;
            }
            let (prefix, local) = split_qname(qname);
            let ns = match prefix {
                "" => Ns::NONE,
                _ => self.element.resolve_prefix(prefix).unwrap_or(Ns::NONE),
            };
            return Some(Attr {
                ns,
                prefix,
                local,
                raw_value,
            });
        }
    }
}

/// One step of the document.
#[derive(Debug, Clone)]
pub enum Event<'a> {
    Start(Element<'a>),
    End(Name<'a>),
    /// Character data or a CDATA section, entities replaced.
    Text(Cow<'a, str>),
    Eof,
}

struct Open<'a> {
    qname: &'a str,
    name: Name<'a>,
    bindings: usize,
}

/// A pull tokenizer over one XML document.
pub struct Reader<'a> {
    text: &'a str,
    pos: usize,
    stack: Vec<Open<'a>>,
    bindings: Vec<(&'a str, Ns)>,
    scope: Scope<'a>,
    unknown: Vec<String>,
    empty_pending: bool,
    close_to: Option<usize>,
    last_prefix: (&'a str, Ns),
}

/// Elements nest no deeper than this; deeper start tags are skipped with
/// their content so hostile input cannot exhaust memory.
pub const MAX_DEPTH: usize = 4096;

impl<'a> Reader<'a> {
    pub fn new(text: &'a str) -> Self {
        Self {
            text,
            pos: 0,
            stack: Vec::new(),
            bindings: Vec::new(),
            scope: Rc::from(Vec::new()),
            unknown: Vec::new(),
            empty_pending: false,
            close_to: None,
            last_prefix: ("\u{0}", Ns::NONE),
        }
    }

    /// The number of open elements.
    pub fn depth(&self) -> usize {
        self.stack.len()
    }

    /// The byte offset of the next unread input.
    pub fn position(&self) -> usize {
        self.pos
    }

    /// The URI behind a namespace id, for known and unknown namespaces.
    pub fn uri(&self, ns: Ns) -> Option<&str> {
        let index = ns.0.checked_sub(Ns::FIRST_UNKNOWN)?;
        self.unknown.get(usize::from(index)).map(String::as_str)
    }

    fn intern(&mut self, uri: &str) -> Ns {
        if uri.is_empty() {
            return Ns::NONE;
        }
        if let Some(ns) = known(uri) {
            return ns;
        }
        if let Some(i) = self.unknown.iter().position(|seen| seen == uri) {
            return Ns(Ns::FIRST_UNKNOWN + i as u16);
        }
        let id = Ns(Ns::FIRST_UNKNOWN.saturating_add(self.unknown.len().min(60_000) as u16));
        self.unknown.push(uri.to_string());
        id
    }

    fn pop(&mut self) -> Event<'a> {
        let Some(open) = self.stack.pop() else {
            return Event::Eof;
        };
        if self.bindings.len() != open.bindings {
            self.bindings.truncate(open.bindings);
            self.scope = Rc::from(self.bindings.as_slice());
            self.last_prefix = ("\u{0}", Ns::NONE);
        }
        Event::End(open.name)
    }

    /// The next event. After [`Event::Eof`] every call returns `Eof`.
    pub fn next_event(&mut self) -> Event<'a> {
        if self.empty_pending {
            self.empty_pending = false;
            return self.pop();
        }
        if let Some(target) = self.close_to {
            if self.stack.len() > target {
                return self.pop();
            }
            self.close_to = None;
        }
        loop {
            let bytes = self.text.as_bytes();
            if self.pos >= bytes.len() {
                return self.pop();
            }
            if bytes[self.pos] != b'<' {
                let start = self.pos;
                let end = memchr(b'<', &bytes[start..]).map_or(bytes.len(), |i| start + i);
                self.pos = end;
                return Event::Text(unescape(&self.text[start..end], false));
            }
            let rest = &self.text[self.pos..];
            if let Some(body) = rest.strip_prefix("<![CDATA[") {
                let end = memmem::find(body.as_bytes(), b"]]>").unwrap_or(body.len());
                self.pos += 9 + end + 3;
                self.pos = self.pos.min(bytes.len());
                return Event::Text(Cow::Borrowed(&body[..end]));
            }
            if rest.starts_with("<!--") {
                self.skip_past(4, b"-->");
                continue;
            }
            if rest.starts_with("<?") {
                self.skip_past(2, b"?>");
                continue;
            }
            if rest.starts_with("<!") {
                self.skip_declaration();
                continue;
            }
            if rest.starts_with("</") {
                if let Some(event) = self.end_tag() {
                    return event;
                }
                continue;
            }
            let starts_name = rest[1..]
                .chars()
                .next()
                .is_some_and(|c| c.is_alphabetic() || c == '_' || c == ':');
            if !starts_name {
                let start = self.pos;
                let end = memchr(b'<', &bytes[start + 1..]).map_or(bytes.len(), |i| start + 1 + i);
                self.pos = end;
                return Event::Text(unescape(&self.text[start..end], false));
            }
            if let Some(event) = self.start_tag() {
                return event;
            }
        }
    }

    fn skip_past(&mut self, from: usize, terminator: &[u8]) {
        let bytes = &self.text.as_bytes()[self.pos + from..];
        self.pos = match memmem::find(bytes, terminator) {
            Some(i) => self.pos + from + i + terminator.len(),
            None => self.text.len(),
        };
    }

    fn skip_declaration(&mut self) {
        let bytes = self.text.as_bytes();
        let mut depth = 0usize;
        let mut at = self.pos + 2;
        while at < bytes.len() {
            match bytes[at] {
                b'[' => depth += 1,
                b']' => depth = depth.saturating_sub(1),
                b'>' if depth == 0 => {
                    self.pos = at + 1;
                    return;
                }
                _ => {}
            }
            at += 1;
        }
        self.pos = bytes.len();
    }

    /// The offset of the `>` closing a tag opened at `from`, skipping `>`
    /// inside quoted attribute values.
    fn tag_end(&self, from: usize) -> Option<usize> {
        let bytes = self.text.as_bytes();
        let mut at = from;
        loop {
            let found = at + memchr3(b'>', b'"', b'\'', bytes.get(at..)?)?;
            if bytes[found] == b'>' {
                return Some(found);
            }
            let close = memchr(bytes[found], &bytes[found + 1..])?;
            at = found + 1 + close + 1;
        }
    }

    fn end_tag(&mut self) -> Option<Event<'a>> {
        let bytes = self.text.as_bytes();
        let start = self.pos + 2;
        let Some(end) = memchr(b'>', &bytes[start..]).map(|i| start + i) else {
            self.pos = bytes.len();
            return None;
        };
        self.pos = end + 1;
        let qname = self.text[start..end].trim_end_matches([' ', '\t', '\n', '\r']);
        let matched = self.stack.iter().rposition(|open| open.qname == qname)?;
        self.close_to = Some(matched);
        match self.stack.len() > matched + 1 {
            true => Some(self.pop()),
            false => {
                self.close_to = None;
                Some(self.pop())
            }
        }
    }

    fn start_tag(&mut self) -> Option<Event<'a>> {
        let bytes = self.text.as_bytes();
        let name_start = self.pos + 1;
        let name_end = bytes[name_start..]
            .iter()
            .position(|&b| is_space(b) || b == b'/' || b == b'>')
            .map_or(bytes.len(), |i| name_start + i);
        let Some(close) = self.tag_end(name_end) else {
            self.pos = bytes.len();
            return None;
        };
        self.pos = close + 1;
        let qname = &self.text[name_start..name_end];
        let mut raw = &self.text[name_end..close];
        let empty = raw.ends_with('/') || qname.ends_with('/');
        let qname = qname.trim_end_matches('/');
        if empty {
            raw = raw.strip_suffix('/').unwrap_or(raw);
        }
        if self.stack.len() >= MAX_DEPTH {
            if !empty {
                self.skip_subtree_raw(qname);
            }
            return None;
        }
        let bindings = self.bindings.len();
        if memmem::find(raw.as_bytes(), b"xmlns").is_some() {
            let mut rest = raw;
            while let Some((attr, value)) = next_pair(&mut rest) {
                let prefix = match attr {
                    "xmlns" => "",
                    _ => match attr.strip_prefix("xmlns:") {
                        Some(prefix) => prefix,
                        None => continue,
                    },
                };
                let uri = unescape(value, true);
                let ns = self.intern(&uri);
                self.bindings.push((prefix, ns));
            }
            if self.bindings.len() != bindings {
                self.scope = Rc::from(self.bindings.as_slice());
                self.last_prefix = ("\u{0}", Ns::NONE);
            }
        }
        let (prefix, local) = split_qname(qname);
        let ns = match prefix {
            "xml" => Ns::XML,
            _ if self.last_prefix.0 == prefix => self.last_prefix.1,
            _ => {
                let ns = resolve(&self.scope, prefix).unwrap_or(Ns::NONE);
                self.last_prefix = (prefix, ns);
                ns
            }
        };
        let name = Name { ns, local };
        self.stack.push(Open {
            qname,
            name,
            bindings,
        });
        self.empty_pending = empty;
        Some(Event::Start(Element {
            ns,
            prefix,
            local,
            empty,
            raw,
            scope: self.scope.clone(),
        }))
    }

    fn skip_subtree_raw(&mut self, qname: &str) {
        let closing = format!("</{qname}");
        let bytes = &self.text.as_bytes()[self.pos..];
        self.pos = match memmem::find(bytes, closing.as_bytes()) {
            Some(i) => self.pos + i,
            None => self.text.len(),
        };
    }

    /// Skips the content and end of the element whose start was just read.
    pub fn skip(&mut self) {
        let depth = self.stack.len();
        if depth == 0 {
            return;
        }
        loop {
            match self.next_event() {
                Event::End(_) if self.stack.len() < depth => return,
                Event::Eof => return,
                _ => {}
            }
        }
    }

    /// The text content of the element whose start was just read, up to
    /// and including its end. Text in child elements is included.
    pub fn read_text(&mut self) -> Cow<'a, str> {
        let depth = self.stack.len();
        let mut out: Cow<'a, str> = Cow::Borrowed("");
        if depth == 0 {
            return out;
        }
        loop {
            match self.next_event() {
                Event::Text(text) if out.is_empty() => out = text,
                Event::Text(text) => out.to_mut().push_str(&text),
                Event::End(_) if self.stack.len() < depth => return out,
                Event::Eof => return out,
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trace(xml: &str) -> Vec<String> {
        let mut reader = Reader::new(xml);
        let mut out = Vec::new();
        loop {
            match reader.next_event() {
                Event::Start(e) => {
                    let attrs: Vec<String> = e
                        .attrs()
                        .map(|a| format!("{}:{}={}", a.ns.0, a.local, a.value()))
                        .collect();
                    out.push(format!("<{}:{} {}>", e.ns.0, e.local, attrs.join(",")));
                }
                Event::End(n) => out.push(format!("</{}:{}>", n.ns.0, n.local)),
                Event::Text(t) => out.push(format!("'{t}'")),
                Event::Eof => return out,
            }
        }
    }

    #[test]
    fn basic_document() {
        let xml = r#"<?xml version="1.0"?><!DOCTYPE x [<!ENTITY e "v">]><!-- c --><a x="1" y='2&amp;3'><b/>t&lt;<![CDATA[<raw>]]></a>"#;
        assert_eq!(
            trace(xml),
            [
                "<0:a 0:x=1,0:y=2&3>",
                "<0:b >",
                "</0:b>",
                "'t<'",
                "'<raw>'",
                "</0:a>"
            ]
        );
    }

    #[test]
    fn transitional_and_strict_resolve_alike() {
        for uri in [
            "http://schemas.openxmlformats.org/wordprocessingml/2006/main",
            "http://purl.oclc.org/ooxml/wordprocessingml/main",
        ] {
            let xml = format!(r#"<w:document xmlns:w="{uri}"><w:p w:rsid="1"/></w:document>"#);
            let mut reader = Reader::new(&xml);
            let Event::Start(root) = reader.next_event() else {
                panic!()
            };
            assert!(root.is(Ns::W, "document"));
            let Event::Start(p) = reader.next_event() else {
                panic!()
            };
            assert!(p.is(Ns::W, "p"));
            assert_eq!(p.attr(Ns::W, "rsid").as_deref(), Some("1"));
        }
    }

    #[test]
    fn default_namespace_and_scoping() {
        let xml =
            r#"<Types xmlns="urn:x"><inner xmlns="urn:y"/><Default Extension="xml"/></Types>"#;
        let mut reader = Reader::new(xml);
        let Event::Start(types) = reader.next_event() else {
            panic!()
        };
        let x = types.ns;
        assert!(x.0 >= Ns::FIRST_UNKNOWN);
        assert_eq!(reader.uri(x), Some("urn:x"));
        let Event::Start(inner) = reader.next_event() else {
            panic!()
        };
        assert_ne!(inner.ns, x);
        reader.next_event();
        let Event::Start(default) = reader.next_event() else {
            panic!()
        };
        assert_eq!(default.ns, x);
        assert_eq!(default.attr(Ns::NONE, "Extension").as_deref(), Some("xml"));
    }

    #[test]
    fn malformed_input_stays_balanced() {
        assert_eq!(
            trace("<a><b><c></a>tail"),
            ["<0:a >", "<0:b >", "<0:c >", "</0:c>", "</0:b>", "</0:a>", "'tail'"]
        );
        assert_eq!(trace("<a></z>x</a>"), ["<0:a >", "'x'", "</0:a>"]);
        assert_eq!(trace("<a><b>"), ["<0:a >", "<0:b >", "</0:b>", "</0:a>"]);
        assert_eq!(
            trace("<a b=\"x>y\" c>1 < 2</a>"),
            ["<0:a 0:b=x>y>", "'1 '", "'< 2'", "</0:a>"]
        );
        assert_eq!(
            trace("<a attr=unquoted/>"),
            ["<0:a 0:attr=unquoted>", "</0:a>"]
        );
    }

    #[test]
    fn read_text_and_skip() {
        let xml = "<r><t>a<x>b</x>c</t><skip><deep><deeper/></deep></skip><after/></r>";
        let mut reader = Reader::new(xml);
        reader.next_event();
        reader.next_event();
        assert_eq!(reader.read_text(), "abc");
        let Event::Start(skip) = reader.next_event() else {
            panic!()
        };
        assert_eq!(skip.local, "skip");
        reader.skip();
        let Event::Start(after) = reader.next_event() else {
            panic!()
        };
        assert_eq!(after.local, "after");
    }

    #[test]
    fn never_panics_on_prefixes_of_a_document() {
        let xml = r#"<?xml version="1.0"?><w:document xmlns:w="urn:w"><w:body><w:p><w:r><w:t xml:space="preserve"> a &amp; b </w:t></w:r></w:p><![CDATA[x]]><!-- c --></w:body></w:document>"#;
        for end in 0..=xml.len() {
            if !xml.is_char_boundary(end) {
                continue;
            }
            let mut reader = Reader::new(&xml[..end]);
            let mut guard = 0;
            while !matches!(reader.next_event(), Event::Eof) {
                guard += 1;
                assert!(guard < 1000);
            }
        }
    }

    #[test]
    fn deep_nesting_is_capped() {
        let xml = "<a>".repeat(MAX_DEPTH + 10);
        let mut reader = Reader::new(&xml);
        let mut deepest = 0;
        while !matches!(reader.next_event(), Event::Eof) {
            deepest = deepest.max(reader.depth());
        }
        assert_eq!(deepest, MAX_DEPTH);
    }
}

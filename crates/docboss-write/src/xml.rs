//! A small XML writer: escaping, attributes and self-closing elements,
//! appended to one `String`.

pub struct Xml {
    pub out: String,
}

pub const DECLARATION: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n";

impl Xml {
    pub fn new() -> Self {
        let mut out = String::with_capacity(4096);
        out.push_str(DECLARATION);
        Self { out }
    }

    /// Opens `<name a="v" ...>`.
    pub fn open(&mut self, name: &str, attributes: &[(&str, &str)]) {
        self.start(name, attributes);
        self.out.push('>');
    }

    /// Writes `<name a="v" .../>`.
    pub fn empty(&mut self, name: &str, attributes: &[(&str, &str)]) {
        self.start(name, attributes);
        self.out.push_str("/>");
    }

    pub fn close(&mut self, name: &str) {
        self.out.push_str("</");
        self.out.push_str(name);
        self.out.push('>');
    }

    /// Writes `<name>text</name>` with the text escaped.
    pub fn text_element(&mut self, name: &str, attributes: &[(&str, &str)], text: &str) {
        self.open(name, attributes);
        self.text(text);
        self.close(name);
    }

    pub fn text(&mut self, text: &str) {
        escape_into(&mut self.out, text, false);
    }

    /// Writes an element holding only a `w:val` attribute.
    pub fn val(&mut self, name: &str, value: &str) {
        self.empty(name, &[("w:val", value)]);
    }

    fn start(&mut self, name: &str, attributes: &[(&str, &str)]) {
        self.out.push('<');
        self.out.push_str(name);
        for (key, value) in attributes {
            self.out.push(' ');
            self.out.push_str(key);
            self.out.push_str("=\"");
            escape_into(&mut self.out, value, true);
            self.out.push('"');
        }
    }

    pub fn finish(self) -> Vec<u8> {
        self.out.into_bytes()
    }
}

/// Escapes markup characters and drops characters XML 1.0 cannot carry.
pub fn escape_into(out: &mut String, text: &str, attribute: bool) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' if attribute => out.push_str("&quot;"),
            '\t' | '\n' | '\r' if attribute => {
                out.push_str(&format!("&#x{:X};", c as u32));
            }
            '\t' | '\n' | '\r' => out.push(c),
            c if (c as u32) < 0x20 || c == '\u{FFFE}' || c == '\u{FFFF}' => {}
            c => out.push(c),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_text_and_attributes() {
        let mut xml = Xml { out: String::new() };
        xml.text_element("w:t", &[("a", "x\"<y>&")], "a<b & \u{1}c");
        assert_eq!(
            xml.out,
            "<w:t a=\"x&quot;&lt;y&gt;&amp;\">a&lt;b &amp; c</w:t>"
        );
    }
}

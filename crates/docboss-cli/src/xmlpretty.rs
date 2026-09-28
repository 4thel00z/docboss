//! Re-indents an XML part for reading: one element per line, text kept on
//! the line of the element that holds it.

/// One markup or text token of the input.
enum Token<'a> {
    Open(&'a str),
    Close(&'a str),
    Empty(&'a str),
    Other(&'a str),
    Text(&'a str),
}

const TERMINATORS: [(&str, &str); 3] = [("<!--", "-->"), ("<![CDATA[", "]]>"), ("<?", "?>")];

fn next_token<'a>(rest: &mut &'a str) -> Option<Token<'a>> {
    let text = *rest;
    if text.is_empty() {
        return None;
    }
    if !text.starts_with('<') {
        let end = text.find('<').unwrap_or(text.len());
        *rest = &text[end..];
        return Some(Token::Text(&text[..end]));
    }
    let terminator = TERMINATORS
        .iter()
        .find(|(opener, _)| text.starts_with(opener))
        .map_or(">", |(_, closer)| closer);
    let end = text[1..]
        .find(terminator)
        .map_or(text.len(), |at| at + 1 + terminator.len());
    let markup = &text[..end];
    *rest = &text[end..];
    if markup.starts_with("<!") || markup.starts_with("<?") {
        return Some(Token::Other(markup));
    }
    if markup.starts_with("</") {
        return Some(Token::Close(markup));
    }
    if markup.ends_with("/>") {
        return Some(Token::Empty(markup));
    }
    Some(Token::Open(markup))
}

/// The input re-indented by `indent` spaces per level. Malformed input is
/// printed as far as it tokenizes, never rejected.
pub fn pretty(xml: &str, indent: usize) -> String {
    let mut out = String::with_capacity(xml.len() + xml.len() / 4);
    let mut rest = xml.trim_start_matches('\u{feff}');
    let mut depth = 0usize;
    let mut pending_text: Option<&str> = None;
    let mut last_was_open = false;
    while let Some(token) = next_token(&mut rest) {
        match token {
            Token::Text(text) => {
                if !text.trim().is_empty() {
                    pending_text = Some(text);
                }
            }
            Token::Close(markup) => {
                depth = depth.saturating_sub(1);
                if last_was_open {
                    out.push_str(pending_text.take().map_or("", str::trim));
                    out.push_str(markup);
                    out.push('\n');
                    last_was_open = false;
                    continue;
                }
                flush_text(
                    &mut out,
                    pending_text.take(),
                    depth + 1,
                    indent,
                    last_was_open,
                );
                line(&mut out, depth, indent, markup);
                last_was_open = false;
            }
            Token::Open(markup) => {
                flush_text(&mut out, pending_text.take(), depth, indent, last_was_open);
                pad(&mut out, depth, indent);
                out.push_str(markup);
                depth += 1;
                last_was_open = true;
            }
            Token::Empty(markup) | Token::Other(markup) => {
                flush_text(&mut out, pending_text.take(), depth, indent, last_was_open);
                line(&mut out, depth, indent, markup);
                last_was_open = false;
            }
        }
    }
    if last_was_open {
        out.push('\n');
    }
    out
}

fn flush_text(out: &mut String, text: Option<&str>, depth: usize, indent: usize, after_open: bool) {
    if after_open {
        out.push('\n');
    }
    let Some(text) = text else {
        return;
    };
    line(out, depth, indent, text.trim());
}

fn pad(out: &mut String, depth: usize, indent: usize) {
    out.extend(std::iter::repeat_n(' ', depth * indent));
}

fn line(out: &mut String, depth: usize, indent: usize, content: &str) {
    pad(out, depth, indent);
    out.push_str(content);
    out.push('\n');
}

#[cfg(test)]
mod tests {
    use super::pretty;

    #[test]
    fn indents_elements_and_keeps_text_inline() {
        let xml = r#"<?xml version="1.0"?><w:document><w:body><w:p><w:r><w:t>Hi</w:t></w:r></w:p><w:sectPr/></w:body></w:document>"#;
        let expected = "<?xml version=\"1.0\"?>\n<w:document>\n  <w:body>\n    <w:p>\n      <w:r>\n        <w:t>Hi</w:t>\n      </w:r>\n    </w:p>\n    <w:sectPr/>\n  </w:body>\n</w:document>\n";
        assert_eq!(pretty(xml, 2), expected);
    }

    #[test]
    fn empty_pairs_close_on_their_line() {
        assert_eq!(pretty("<a><b></b></a>", 1), "<a>\n <b></b>\n</a>\n");
    }

    #[test]
    fn malformed_input_does_not_panic() {
        for xml in ["<a", "</a></b>", "<a>text", "<!-- open", "<![CDATA[x", ">"] {
            let _ = pretty(xml, 2);
        }
    }
}

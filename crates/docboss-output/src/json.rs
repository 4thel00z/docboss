//! JSON: the whole model, or the flat block view.

use docboss_model::Document;

/// The document model as JSON. Binary media and embedded fonts are left
/// out; media entries keep their names and content types.
pub fn to_json(doc: &Document, pretty: bool) -> serde_json::Result<String> {
    if pretty {
        return serde_json::to_string_pretty(doc);
    }
    serde_json::to_string(doc)
}

/// [`crate::blocks_view`] as a JSON array.
pub fn blocks_json(doc: &Document, pretty: bool) -> serde_json::Result<String> {
    let blocks = crate::blocks_view(doc);
    if pretty {
        return serde_json::to_string_pretty(&blocks);
    }
    serde_json::to_string(&blocks)
}

#[cfg(test)]
mod tests {
    use docboss_model::{Block, Inline, Paragraph, Section};

    use super::*;

    #[test]
    fn comment_ranges_serialize_with_their_ids() {
        let paragraph = Paragraph {
            inlines: vec![
                Inline::CommentRangeStart { id: 3 },
                Inline::CommentRangeEnd { id: 3 },
            ],
            ..Paragraph::default()
        };
        let doc = Document {
            sections: vec![Section {
                properties: Default::default(),
                blocks: vec![Block::Paragraph(paragraph)],
            }],
            ..Document::default()
        };
        let json = to_json(&doc, false).expect("serializes");
        assert!(json.contains(r#"{"type":"comment_range_start","id":3}"#));
        assert!(json.contains(r#"{"type":"comment_range_end","id":3}"#));
    }
}

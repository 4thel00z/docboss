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

//! Text, Markdown, HTML and JSON output from a [`docboss_model::Document`].
//!
//! Every format is one pass over the model. Functions write to any
//! [`std::fmt::Write`] or return a `String`; an options struct with a
//! `Default` controls each format.

mod blocks;
mod html;
#[cfg(feature = "serde")]
mod json;
mod latex;
mod markdown;
mod symbol;
mod text;
mod walk;

pub use blocks::{blocks_view, BlockKind, BlockView};
pub use html::{to_html, write_html, HtmlOptions};
#[cfg(feature = "serde")]
pub use json::{blocks_json, to_json};
pub use markdown::{to_markdown, write_markdown, MarkdownOptions};
pub use text::{to_text, write_text, TextOptions};
pub use walk::media_file_name;

use docboss_model::Section;

/// How images are written into Markdown and HTML.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ImageMode {
    /// Reference each image by its media file name under a prefix, such as
    /// `media/` for `media/image1.png`. The caller writes the files, named
    /// by [`media_file_name`].
    Reference { prefix: String },
    /// Embed each image as a `data:` URI.
    Embed,
    /// Leave images out.
    #[default]
    Omit,
}

struct SectionParts<'a> {
    headers: Vec<&'a str>,
    footers: Vec<&'a str>,
}

fn distinct_ids(refs: &docboss_model::HeaderFooterRefs) -> Vec<&str> {
    let mut ids: Vec<&str> = Vec::new();
    for id in [&refs.first, &refs.default, &refs.even]
        .into_iter()
        .flatten()
    {
        if !ids.contains(&id.as_str()) {
            ids.push(id);
        }
    }
    ids
}

/// The distinct header and footer ids a section refers to: first page,
/// default, then even.
fn section_parts(section: &Section) -> SectionParts<'_> {
    SectionParts {
        headers: distinct_ids(&section.properties.headers),
        footers: distinct_ids(&section.properties.footers),
    }
}

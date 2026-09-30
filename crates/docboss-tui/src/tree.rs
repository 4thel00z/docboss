//! The element tree: groups for the document's stories, styles, numbering,
//! media, fonts, container parts and diagnostics, with paragraphs, tables,
//! rows, cells and inlines below each story. Children are built when a
//! node is first expanded, so a large document costs nothing until opened.

use docboss_model::{Block, Document, Inline, Paragraph, RunContent, TableCell, TableRow};

use crate::container::Part;

/// A body of blocks: the main text of a section, a header or footer, a
/// note or a comment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Story {
    Section(usize),
    HeaderFooter(usize),
    Footnote(usize),
    Endnote(usize),
    Comment(usize),
}

/// One step from a story down to a block, row, cell or inline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Block(usize),
    Row(usize),
    Cell(usize),
    Inline(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    Sections,
    HeadersFooters,
    Footnotes,
    Endnotes,
    Comments,
    Styles,
    Numbering,
    Media,
    Fonts,
    Container,
    Diagnostics,
}

impl Group {
    pub const ALL: [Group; 11] = [
        Group::Sections,
        Group::HeadersFooters,
        Group::Footnotes,
        Group::Endnotes,
        Group::Comments,
        Group::Styles,
        Group::Numbering,
        Group::Media,
        Group::Fonts,
        Group::Container,
        Group::Diagnostics,
    ];

    fn title(self) -> &'static str {
        match self {
            Group::Sections => "Sections",
            Group::HeadersFooters => "Headers and footers",
            Group::Footnotes => "Footnotes",
            Group::Endnotes => "Endnotes",
            Group::Comments => "Comments",
            Group::Styles => "Styles",
            Group::Numbering => "Numbering",
            Group::Media => "Media",
            Group::Fonts => "Fonts",
            Group::Container => "Container",
            Group::Diagnostics => "Diagnostics",
        }
    }
}

/// What a tree node stands for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeKind {
    Document,
    Group(Group),
    Story(Story),
    Content(Story, Vec<Step>),
    Style(usize),
    Abstract(usize),
    Instance(usize),
    Media(usize),
    Font(usize),
    Part(usize),
    Diagnostic(usize),
}

/// A block, row, cell or inline a content path leads to.
#[derive(Debug, Clone, Copy)]
pub enum Resolved<'a> {
    Block(&'a Block),
    Row(&'a TableRow),
    Cell(&'a TableCell),
    Inline(&'a Inline),
}

pub fn story_blocks(doc: &Document, story: Story) -> Option<&[Block]> {
    let blocks = match story {
        Story::Section(i) => &doc.sections.get(i)?.blocks,
        Story::HeaderFooter(i) => &doc.headers_footers.get(i)?.blocks,
        Story::Footnote(i) => &doc.footnotes.get(i)?.blocks,
        Story::Endnote(i) => &doc.endnotes.get(i)?.blocks,
        Story::Comment(i) => &doc.comments.get(i)?.blocks,
    };
    Some(blocks)
}

/// The inlines nested inside a hyperlink, field result or revision.
pub fn inline_children(inline: &Inline) -> &[Inline] {
    match inline {
        Inline::Hyperlink(link) => &link.inlines,
        Inline::Field(field) => &field.result,
        Inline::Revision(revision) => &revision.inlines,
        _ => &[],
    }
}

/// Follows a content path from its story.
pub fn resolve<'a>(doc: &'a Document, story: Story, steps: &[Step]) -> Option<Resolved<'a>> {
    let mut blocks = story_blocks(doc, story)?;
    let mut current: Option<Resolved<'a>> = None;
    for step in steps {
        let next = match (*step, current) {
            (Step::Block(i), None | Some(Resolved::Cell(_))) => Resolved::Block(blocks.get(i)?),
            (Step::Row(r), Some(Resolved::Block(Block::Table(table)))) => {
                Resolved::Row(table.rows.get(r)?)
            }
            (Step::Cell(c), Some(Resolved::Row(row))) => {
                let cell = row.cells.get(c)?;
                blocks = &cell.blocks;
                Resolved::Cell(cell)
            }
            (Step::Inline(i), Some(Resolved::Block(Block::Paragraph(paragraph)))) => {
                Resolved::Inline(paragraph.inlines.get(i)?)
            }
            (Step::Inline(i), Some(Resolved::Inline(inline))) => {
                Resolved::Inline(inline_children(inline).get(i)?)
            }
            _ => return None,
        };
        current = Some(next);
    }
    current
}

/// The paragraph that holds the inline a path ends at.
pub fn paragraph_of<'a>(doc: &'a Document, story: Story, steps: &[Step]) -> Option<&'a Paragraph> {
    let last_block = steps
        .iter()
        .rposition(|step| matches!(step, Step::Block(_)))?;
    match resolve(doc, story, &steps[..=last_block])? {
        Resolved::Block(Block::Paragraph(paragraph)) => Some(paragraph),
        _ => None,
    }
}

/// Everything the tree reads besides the document model.
pub struct Sources<'a> {
    pub doc: &'a Document,
    pub parts: &'a [Part],
}

fn story_count(doc: &Document, group: Group, parts: &[Part]) -> usize {
    match group {
        Group::Sections => doc.sections.len(),
        Group::HeadersFooters => doc.headers_footers.len(),
        Group::Footnotes => doc.footnotes.len(),
        Group::Endnotes => doc.endnotes.len(),
        Group::Comments => doc.comments.len(),
        Group::Styles => doc.styles.styles.len(),
        Group::Numbering => doc.numbering.abstracts.len() + doc.numbering.instances.len(),
        Group::Media => doc.media.len(),
        Group::Fonts => doc.fonts.len(),
        Group::Container => parts.len(),
        Group::Diagnostics => doc.diagnostics.len(),
    }
}

fn group_children(doc: &Document, group: Group, parts: &[Part]) -> Vec<NodeKind> {
    let n = story_count(doc, group, parts);
    match group {
        Group::Sections => (0..n).map(|i| NodeKind::Story(Story::Section(i))).collect(),
        Group::HeadersFooters => (0..n)
            .map(|i| NodeKind::Story(Story::HeaderFooter(i)))
            .collect(),
        Group::Footnotes => (0..n)
            .map(|i| NodeKind::Story(Story::Footnote(i)))
            .collect(),
        Group::Endnotes => (0..n).map(|i| NodeKind::Story(Story::Endnote(i))).collect(),
        Group::Comments => (0..n).map(|i| NodeKind::Story(Story::Comment(i))).collect(),
        Group::Styles => (0..n).map(NodeKind::Style).collect(),
        Group::Numbering => (0..doc.numbering.abstracts.len())
            .map(NodeKind::Abstract)
            .chain((0..doc.numbering.instances.len()).map(NodeKind::Instance))
            .collect(),
        Group::Media => (0..n).map(NodeKind::Media).collect(),
        Group::Fonts => (0..n).map(NodeKind::Font).collect(),
        Group::Container => (0..n).map(NodeKind::Part).collect(),
        Group::Diagnostics => (0..n).map(NodeKind::Diagnostic).collect(),
    }
}

fn extended(steps: &[Step], step: Step) -> Vec<Step> {
    let mut path = steps.to_vec();
    path.push(step);
    path
}

fn content_children(doc: &Document, story: Story, steps: &[Step]) -> Vec<NodeKind> {
    let content = |step: Step| NodeKind::Content(story, extended(steps, step));
    if steps.is_empty() {
        let count = story_blocks(doc, story).map_or(0, <[Block]>::len);
        return (0..count).map(|i| content(Step::Block(i))).collect();
    }
    let Some(resolved) = resolve(doc, story, steps) else {
        return Vec::new();
    };
    match resolved {
        Resolved::Block(Block::Paragraph(p)) => (0..p.inlines.len())
            .map(|i| content(Step::Inline(i)))
            .collect(),
        Resolved::Block(Block::Table(t)) => {
            (0..t.rows.len()).map(|r| content(Step::Row(r))).collect()
        }
        Resolved::Row(row) => (0..row.cells.len())
            .map(|c| content(Step::Cell(c)))
            .collect(),
        Resolved::Cell(cell) => (0..cell.blocks.len())
            .map(|i| content(Step::Block(i)))
            .collect(),
        Resolved::Inline(inline) => (0..inline_children(inline).len())
            .map(|i| content(Step::Inline(i)))
            .collect(),
    }
}

/// The children of a node, in display order.
pub fn children(sources: &Sources<'_>, kind: &NodeKind) -> Vec<NodeKind> {
    match kind {
        NodeKind::Group(group) => group_children(sources.doc, *group, sources.parts),
        NodeKind::Story(story) => content_children(sources.doc, *story, &[]),
        NodeKind::Content(story, steps) => content_children(sources.doc, *story, steps),
        _ => Vec::new(),
    }
}

/// Whether a node has children, without building them.
pub fn has_children(sources: &Sources<'_>, kind: &NodeKind) -> bool {
    let doc = sources.doc;
    match kind {
        NodeKind::Group(group) => story_count(doc, *group, sources.parts) > 0,
        NodeKind::Story(story) => {
            story_blocks(doc, *story).is_some_and(|blocks| !blocks.is_empty())
        }
        NodeKind::Content(story, steps) => match resolve(doc, *story, steps) {
            Some(Resolved::Block(Block::Paragraph(p))) => !p.inlines.is_empty(),
            Some(Resolved::Block(Block::Table(t))) => !t.rows.is_empty(),
            Some(Resolved::Row(row)) => !row.cells.is_empty(),
            Some(Resolved::Cell(cell)) => !cell.blocks.is_empty(),
            Some(Resolved::Inline(inline)) => !inline_children(inline).is_empty(),
            None => false,
        },
        _ => false,
    }
}

/// The nodes from the top level down to `kind`, excluding `kind` itself.
pub fn ancestors(kind: &NodeKind) -> Vec<NodeKind> {
    let group_of_story = |story: Story| match story {
        Story::Section(_) => Group::Sections,
        Story::HeaderFooter(_) => Group::HeadersFooters,
        Story::Footnote(_) => Group::Footnotes,
        Story::Endnote(_) => Group::Endnotes,
        Story::Comment(_) => Group::Comments,
    };
    match kind {
        NodeKind::Document | NodeKind::Group(_) => Vec::new(),
        NodeKind::Story(story) => vec![NodeKind::Group(group_of_story(*story))],
        NodeKind::Content(story, steps) => {
            let mut chain = vec![
                NodeKind::Group(group_of_story(*story)),
                NodeKind::Story(*story),
            ];
            chain.extend((1..steps.len()).map(|n| NodeKind::Content(*story, steps[..n].to_vec())));
            chain
        }
        NodeKind::Style(_) => vec![NodeKind::Group(Group::Styles)],
        NodeKind::Abstract(_) | NodeKind::Instance(_) => vec![NodeKind::Group(Group::Numbering)],
        NodeKind::Media(_) => vec![NodeKind::Group(Group::Media)],
        NodeKind::Font(_) => vec![NodeKind::Group(Group::Fonts)],
        NodeKind::Part(_) => vec![NodeKind::Group(Group::Container)],
        NodeKind::Diagnostic(_) => vec![NodeKind::Group(Group::Diagnostics)],
    }
}

/// `text` cut to `max` characters, one line, with an ellipsis when cut.
pub fn preview(text: &str, max: usize) -> String {
    let flat: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let flat = flat.trim();
    if flat.chars().count() <= max {
        return flat.to_string();
    }
    let cut: String = flat.chars().take(max.saturating_sub(1)).collect();
    format!("{cut}\u{2026}")
}

fn run_text(content: &[RunContent]) -> String {
    let mut out = String::new();
    for item in content {
        match item {
            RunContent::Text(text) => out.push_str(text),
            RunContent::Tab => out.push('\u{2192}'),
            RunContent::Break(_) | RunContent::CarriageReturn => out.push('\u{21b5}'),
            RunContent::Drawing(_) => out.push_str("[drawing]"),
            RunContent::FootnoteReference(id) => out.push_str(&format!("[fn {id}]")),
            RunContent::EndnoteReference(id) => out.push_str(&format!("[en {id}]")),
            RunContent::CommentReference(id) => out.push_str(&format!("[comment {id}]")),
            RunContent::Symbol { char, .. } => out.extend(char::from_u32(*char)),
            _ => {}
        }
    }
    out
}

fn inline_label(inline: &Inline) -> String {
    match inline {
        Inline::Run(run) => {
            let mut flags = String::new();
            if run.properties.bold == Some(true) {
                flags.push_str(" b");
            }
            if run.properties.italic == Some(true) {
                flags.push_str(" i");
            }
            if run.properties.underline.is_some() {
                flags.push_str(" u");
            }
            format!(
                "run \u{201c}{}\u{201d}{flags}",
                preview(&run_text(&run.content), 40)
            )
        }
        Inline::Hyperlink(link) => {
            let target = link
                .target
                .as_deref()
                .or(link.anchor.as_deref())
                .unwrap_or("");
            format!("link \u{2192} {}", preview(target, 48))
        }
        Inline::Field(field) => format!("field {}", preview(&field.instruction, 48)),
        Inline::Revision(revision) => {
            let author = revision.author.as_deref().unwrap_or("unknown");
            format!("{:?} by {author}", revision.kind).to_lowercase()
        }
        Inline::BookmarkStart { id, name } => format!("bookmark {id} start {name}"),
        Inline::BookmarkEnd { id } => format!("bookmark {id} end"),
        Inline::CommentRangeStart { id } => format!("comment {id} range start"),
        Inline::CommentRangeEnd { id } => format!("comment {id} range end"),
    }
}

fn resolved_label(resolved: Resolved<'_>, last: Step) -> String {
    let index = match last {
        Step::Block(i) | Step::Row(i) | Step::Cell(i) | Step::Inline(i) => i + 1,
    };
    match resolved {
        Resolved::Block(Block::Paragraph(p)) => {
            let style = p
                .style_id
                .as_deref()
                .map(|s| format!("[{s}] "))
                .unwrap_or_default();
            format!("\u{00b6} {style}{}", preview(&p.text(), 60))
        }
        Resolved::Block(Block::Table(t)) => {
            let columns = t.rows.iter().map(|row| row.cells.len()).max().unwrap_or(0);
            format!("table {}\u{00d7}{columns}", t.rows.len())
        }
        Resolved::Row(row) => format!("row {index} \u{00b7} {} cells", row.cells.len()),
        Resolved::Cell(cell) => {
            let span = cell.span();
            let span = if span > 1 {
                format!(" \u{00b7} span {span}")
            } else {
                String::new()
            };
            format!("cell {index}{span} \u{00b7} {} blocks", cell.blocks.len())
        }
        Resolved::Inline(inline) => inline_label(inline),
    }
}

fn human_size(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    if bytes < 1024 * 1024 {
        return format!("{:.1} KB", bytes as f64 / 1024.0);
    }
    format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
}

/// The one-line label of a node.
pub fn label(sources: &Sources<'_>, kind: &NodeKind) -> String {
    let doc = sources.doc;
    match kind {
        NodeKind::Document => format!("Document ({:?})", doc.format).to_string(),
        NodeKind::Group(group) => format!(
            "{} ({})",
            group.title(),
            story_count(doc, *group, sources.parts)
        ),
        NodeKind::Story(Story::Section(i)) => {
            doc.sections.get(*i).map_or_else(String::new, |section| {
                let size = section.properties.page_size;
                format!(
                    "section {} \u{00b7} {}\u{00d7}{} tw \u{00b7} {} blocks",
                    i + 1,
                    size.width,
                    size.height,
                    section.blocks.len()
                )
            })
        }
        NodeKind::Story(Story::HeaderFooter(i)) => {
            doc.headers_footers
                .get(*i)
                .map_or_else(String::new, |part| {
                    format!(
                        "{:?} {} \u{00b7} {} blocks",
                        part.kind,
                        part.id,
                        part.blocks.len()
                    )
                    .to_lowercase()
                })
        }
        NodeKind::Story(Story::Footnote(i)) => doc
            .footnotes
            .get(*i)
            .map_or_else(String::new, |note| format!("footnote {}", note.id)),
        NodeKind::Story(Story::Endnote(i)) => doc
            .endnotes
            .get(*i)
            .map_or_else(String::new, |note| format!("endnote {}", note.id)),
        NodeKind::Story(Story::Comment(i)) => {
            doc.comments.get(*i).map_or_else(String::new, |comment| {
                format!(
                    "comment {} \u{00b7} {}",
                    comment.id,
                    comment.author.as_deref().unwrap_or("unknown")
                )
            })
        }
        NodeKind::Content(story, steps) => match (resolve(doc, *story, steps), steps.last()) {
            (Some(resolved), Some(last)) => resolved_label(resolved, *last),
            _ => "(missing)".to_string(),
        },
        NodeKind::Style(i) => doc.styles.styles.get(*i).map_or_else(String::new, |style| {
            let name = style
                .name
                .as_deref()
                .map(|n| format!(" ({n})"))
                .unwrap_or_default();
            format!("{}{name} \u{00b7} {:?}", style.id, style.kind).to_string()
        }),
        NodeKind::Abstract(i) => {
            doc.numbering
                .abstracts
                .get(*i)
                .map_or_else(String::new, |definition| {
                    format!(
                        "abstract {} \u{00b7} {} levels",
                        definition.id,
                        definition.levels.len()
                    )
                })
        }
        NodeKind::Instance(i) => {
            doc.numbering
                .instances
                .get(*i)
                .map_or_else(String::new, |instance| {
                    format!(
                        "num {} \u{2192} abstract {}",
                        instance.num_id, instance.abstract_id
                    )
                })
        }
        NodeKind::Media(i) => doc.media.get(*i).map_or_else(String::new, |media| {
            format!(
                "{} \u{00b7} {} \u{00b7} {}",
                media.name,
                media.content_type,
                human_size(media.data.len() as u64)
            )
        }),
        NodeKind::Font(i) => doc.fonts.get(*i).map_or_else(String::new, |font| {
            let embedded = [
                &font.embedded_regular,
                &font.embedded_bold,
                &font.embedded_italic,
                &font.embedded_bold_italic,
            ]
            .iter()
            .filter(|face| face.is_some())
            .count();
            let embedded = if embedded > 0 {
                format!(" \u{00b7} {embedded} embedded")
            } else {
                String::new()
            };
            format!("{}{embedded}", font.name)
        }),
        NodeKind::Part(i) => sources.parts.get(*i).map_or_else(String::new, |part| {
            format!("{} \u{00b7} {}", part.display_name(), human_size(part.size))
        }),
        NodeKind::Diagnostic(i) => doc
            .diagnostics
            .get(*i)
            .map_or_else(String::new, |diagnostic| {
                format!(
                    "{:?} {}: {}",
                    diagnostic.severity,
                    diagnostic.location,
                    preview(&diagnostic.message, 60)
                )
                .to_lowercase()
            }),
    }
}

/// A tree node with its children built on first expansion.
#[derive(Debug, Clone)]
pub struct Node {
    pub kind: NodeKind,
    pub parent: Option<usize>,
    pub depth: usize,
    pub expanded: bool,
    pub children: Option<Vec<usize>>,
}

/// One row of the visible tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row {
    pub id: usize,
    pub depth: usize,
}

/// The tree model: nodes, the top level, and the selection.
#[derive(Debug, Clone)]
pub struct Tree {
    pub nodes: Vec<Node>,
    pub roots: Vec<usize>,
    pub selected: usize,
}

impl Tree {
    pub fn new() -> Tree {
        let kinds =
            std::iter::once(NodeKind::Document).chain(Group::ALL.into_iter().map(NodeKind::Group));
        let nodes: Vec<Node> = kinds
            .map(|kind| Node {
                kind,
                parent: None,
                depth: 0,
                expanded: false,
                children: None,
            })
            .collect();
        let roots = (0..nodes.len()).collect();
        Tree {
            nodes,
            roots,
            selected: 0,
        }
    }

    pub fn node(&self, id: usize) -> &Node {
        &self.nodes[id]
    }

    pub fn selected_kind(&self) -> &NodeKind {
        &self.nodes[self.selected].kind
    }

    fn ensure_children(&mut self, sources: &Sources<'_>, id: usize) -> Vec<usize> {
        if let Some(children) = &self.nodes[id].children {
            return children.clone();
        }
        let depth = self.nodes[id].depth + 1;
        let kinds = children(sources, &self.nodes[id].kind);
        let start = self.nodes.len();
        self.nodes.extend(kinds.into_iter().map(|kind| Node {
            kind,
            parent: Some(id),
            depth,
            expanded: false,
            children: None,
        }));
        let ids: Vec<usize> = (start..self.nodes.len()).collect();
        self.nodes[id].children = Some(ids.clone());
        ids
    }

    pub fn expand(&mut self, sources: &Sources<'_>, id: usize) {
        if !has_children(sources, &self.nodes[id].kind) {
            return;
        }
        self.ensure_children(sources, id);
        self.nodes[id].expanded = true;
    }

    pub fn collapse(&mut self, id: usize) {
        self.nodes[id].expanded = false;
    }

    /// The expanded part of the tree in display order.
    pub fn visible_rows(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        let mut stack: Vec<usize> = self.roots.iter().rev().copied().collect();
        while let Some(id) = stack.pop() {
            let node = &self.nodes[id];
            rows.push(Row {
                id,
                depth: node.depth,
            });
            if !node.expanded {
                continue;
            }
            if let Some(children) = &node.children {
                stack.extend(children.iter().rev());
            }
        }
        rows
    }

    /// Expands the ancestors of `kind` and selects it. Returns whether the
    /// node exists.
    pub fn reveal(&mut self, sources: &Sources<'_>, kind: &NodeKind) -> bool {
        let mut level = self.roots.clone();
        for ancestor in ancestors(kind) {
            let Some(id) = level
                .iter()
                .copied()
                .find(|&id| self.nodes[id].kind == ancestor)
            else {
                return false;
            };
            self.expand(sources, id);
            level = self.ensure_children(sources, id);
        }
        let Some(id) = level
            .iter()
            .copied()
            .find(|&id| self.nodes[id].kind == *kind)
        else {
            return false;
        };
        self.selected = id;
        true
    }
}

impl Default for Tree {
    fn default() -> Self {
        Tree::new()
    }
}

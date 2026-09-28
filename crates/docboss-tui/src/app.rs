//! The explorer's state machine. [`App::update`] takes a [`Msg`] and
//! returns the [`Cmd`]s to run; layout, rendering and clipboard access
//! happen outside, so the whole machine runs headless in tests.

use std::sync::Arc;

use crossterm::event::KeyEvent;
use docboss_layout::Layout;
use docboss_model::{Block, Document, Inline, RunContent};
use docboss_output::MarkdownOptions;
use docboss_render::Pixmap;
use ratatui::layout::Rect;

use crate::container::{self, Kind, Part};
use crate::input::{self, Action, Context, YankTarget};
use crate::tree::{resolve, NodeKind, Resolved, Sources, Tree};
use crate::{hexview, inspector, search, ui};

/// Which pane the movement keys drive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Tree,
    Right,
    Hex,
}

/// What the upper right pane shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Inspector,
    Xml,
    Markdown,
    Preview,
}

/// Work the event loop runs off the state machine.
#[derive(Debug, Clone, PartialEq)]
pub enum Cmd {
    /// Lay the document out.
    Layout,
    /// Paint `page` at `scale`; the result comes back tagged `generation`.
    Render {
        generation: u64,
        page: usize,
        scale: f32,
    },
    /// Put text on the clipboard.
    Copy(String),
}

/// Input to the state machine.
#[derive(Debug)]
pub enum Msg {
    Key(KeyEvent),
    Resize(u16, u16),
    /// A 100 ms clock, for toasts and the resize debounce.
    Tick,
    LayoutReady(Result<Arc<Layout>, String>),
    Rendered {
        generation: u64,
        result: Result<Pixmap, String>,
    },
    Toast(String),
}

#[derive(Debug, Clone)]
pub enum LayoutState {
    NotStarted,
    Pending,
    Ready(Arc<Layout>),
    Failed(String),
}

/// The page preview's state.
#[derive(Debug, Default)]
pub struct Preview {
    pub page: usize,
    pub generation: u64,
    pub pixmap: Option<Pixmap>,
    pub rendering: bool,
    pub error: Option<String>,
    /// The page and pane size of the last request, so a render is asked
    /// for once per change.
    pub requested: Option<(usize, u16, u16)>,
    /// Ticks until a resize re-render may start.
    pub debounce: u8,
}

/// The search prompt and its results.
#[derive(Debug, Default)]
pub struct Search {
    pub editing: bool,
    pub query: String,
    pub hits: Vec<NodeKind>,
    pub current: usize,
}

/// The bytes the hex pane shows and what they are.
#[derive(Debug, Clone)]
pub struct HexSource {
    pub label: String,
    pub bytes: Arc<Vec<u8>>,
}

const TOAST_TICKS: u8 = 30;
const RESIZE_DEBOUNCE_TICKS: u8 = 2;
const HEXDUMP_LIMIT: usize = 64 << 10;

pub struct App {
    pub doc: Arc<Document>,
    pub bytes: Arc<Vec<u8>>,
    /// The path as given on the command line, for copied commands.
    pub target: String,
    pub container: Kind,
    pub parts: Vec<Part>,
    pub tree: Tree,
    pub focus: Focus,
    pub view: View,
    pub right_scroll: usize,
    pub hex_scroll: usize,
    pub hex: HexSource,
    /// The pretty-printed XML of a part, by part index.
    pub xml: Option<(usize, Vec<String>)>,
    pub markdown: Option<Vec<String>>,
    pub layout: LayoutState,
    pub preview: Preview,
    pub search: Search,
    pub yank_menu: bool,
    pub help: bool,
    pub toast: Option<(String, u8)>,
    pub size: (u16, u16),
    /// Width of the tree pane in percent of the screen.
    pub tree_percent: u16,
    pub should_quit: bool,
}

impl App {
    pub fn new(doc: Arc<Document>, bytes: Arc<Vec<u8>>, target: String, size: (u16, u16)) -> App {
        let (container, parts) = container::parts(&bytes);
        let hex = HexSource {
            label: "file".to_string(),
            bytes: Arc::clone(&bytes),
        };
        App {
            doc,
            bytes,
            target,
            container,
            parts,
            tree: Tree::new(),
            focus: Focus::Tree,
            view: View::Inspector,
            right_scroll: 0,
            hex_scroll: 0,
            hex,
            xml: None,
            markdown: None,
            layout: LayoutState::NotStarted,
            preview: Preview::default(),
            search: Search::default(),
            yank_menu: false,
            help: false,
            toast: None,
            size,
            tree_percent: 40,
            should_quit: false,
        }
    }

    pub fn sources(&self) -> Sources<'_> {
        Sources {
            doc: &self.doc,
            parts: &self.parts,
        }
    }

    fn context(&self) -> Context {
        if self.search.editing {
            return Context::Search;
        }
        if self.yank_menu {
            return Context::Yank;
        }
        if self.help {
            return Context::Help;
        }
        Context::Normal
    }

    /// The pane rectangles for the current terminal size.
    pub fn panes(&self) -> ui::Panes {
        ui::panes(Rect::new(0, 0, self.size.0, self.size.1), self.tree_percent)
    }

    /// The inner size of the upper right pane in cells.
    pub fn right_inner(&self) -> (u16, u16) {
        let area = self.panes().right;
        (area.width.saturating_sub(2), area.height.saturating_sub(2))
    }

    fn hex_inner_rows(&self) -> usize {
        usize::from(self.panes().hex.height.saturating_sub(2))
    }

    pub fn toast(&mut self, message: impl Into<String>) {
        self.toast = Some((message.into(), TOAST_TICKS));
    }

    pub fn update(&mut self, msg: Msg) -> Vec<Cmd> {
        let mut cmds = match msg {
            Msg::Key(key) => self.on_action(input::action(key, self.context())),
            Msg::Resize(width, height) => {
                self.size = (width, height);
                self.preview.debounce = RESIZE_DEBOUNCE_TICKS;
                Vec::new()
            }
            Msg::Tick => {
                self.tick();
                Vec::new()
            }
            Msg::LayoutReady(result) => {
                self.layout = match result {
                    Ok(layout) => LayoutState::Ready(layout),
                    Err(message) => LayoutState::Failed(message),
                };
                Vec::new()
            }
            Msg::Rendered { generation, result } => {
                self.on_rendered(generation, result);
                Vec::new()
            }
            Msg::Toast(message) => {
                self.toast(message);
                Vec::new()
            }
        };
        cmds.extend(self.preview_cmds());
        cmds
    }

    fn tick(&mut self) {
        self.preview.debounce = self.preview.debounce.saturating_sub(1);
        let Some((_, ticks)) = &mut self.toast else {
            return;
        };
        *ticks = ticks.saturating_sub(1);
        if *ticks == 0 {
            self.toast = None;
        }
    }

    fn on_rendered(&mut self, generation: u64, result: Result<Pixmap, String>) {
        if generation != self.preview.generation {
            return;
        }
        self.preview.rendering = false;
        match result {
            Ok(pixmap) => {
                self.preview.pixmap = Some(pixmap);
                self.preview.error = None;
            }
            Err(message) => {
                self.preview.pixmap = None;
                self.preview.error = Some(message);
            }
        }
    }

    /// The page count once the layout is ready.
    pub fn page_count(&self) -> Option<usize> {
        match &self.layout {
            LayoutState::Ready(layout) => Some(layout.pages.len()),
            _ => None,
        }
    }

    fn preview_cmds(&mut self) -> Vec<Cmd> {
        if self.view != View::Preview {
            return Vec::new();
        }
        let layout = match &self.layout {
            LayoutState::NotStarted => {
                self.layout = LayoutState::Pending;
                return vec![Cmd::Layout];
            }
            LayoutState::Pending | LayoutState::Failed(_) => return Vec::new(),
            LayoutState::Ready(layout) => Arc::clone(layout),
        };
        if self.preview.debounce > 0 {
            return Vec::new();
        }
        let Some(page) = layout
            .pages
            .get(self.preview.page.min(layout.pages.len().saturating_sub(1)))
        else {
            return Vec::new();
        };
        self.preview.page = self.preview.page.min(layout.pages.len() - 1);
        let (cols, rows) = self.right_inner();
        let key = (self.preview.page, cols, rows.saturating_sub(1));
        if self.preview.requested == Some(key) {
            return Vec::new();
        }
        self.preview.requested = Some(key);
        self.preview.generation += 1;
        self.preview.rendering = true;
        let scale = crate::preview::fit_scale(page.width, page.height, key.1, key.2);
        vec![Cmd::Render {
            generation: self.preview.generation,
            page: self.preview.page,
            scale,
        }]
    }

    fn on_action(&mut self, action: Action) -> Vec<Cmd> {
        match action {
            Action::Quit => self.should_quit = true,
            Action::Up => self.move_by(-1),
            Action::Down => self.move_by(1),
            Action::PageUp => self.move_by(-(self.page_rows() as isize)),
            Action::PageDown => self.move_by(self.page_rows() as isize),
            Action::Top => self.move_by(isize::MIN / 2),
            Action::Bottom => self.move_by(isize::MAX / 2),
            Action::Collapse => self.collapse(),
            Action::Expand => self.expand(),
            Action::Toggle => self.toggle(),
            Action::FocusNext => {
                self.focus = match self.focus {
                    Focus::Tree => Focus::Right,
                    Focus::Right => Focus::Hex,
                    Focus::Hex => Focus::Tree,
                }
            }
            Action::ShowInspector => self.show(View::Inspector),
            Action::ShowXml => self.show_xml(),
            Action::ShowMarkdown => self.show_markdown(),
            Action::ShowPreview => self.show(View::Preview),
            Action::PreviousPage => self.turn_page(-1),
            Action::NextPage => self.turn_page(1),
            Action::OpenSearch => {
                self.search.editing = true;
                self.search.query.clear();
            }
            Action::SearchChar(c) => self.search.query.push(c),
            Action::SearchBackspace => {
                self.search.query.pop();
            }
            Action::SearchCancel => self.search.editing = false,
            Action::SearchAccept => self.run_search(),
            Action::NextHit => self.jump_hit(1),
            Action::PreviousHit => self.jump_hit(-1),
            Action::OpenYank => self.yank_menu = true,
            Action::Yank(target) => {
                self.yank_menu = false;
                return vec![Cmd::Copy(self.yank_text(target))];
            }
            Action::CloseMenu => {
                self.yank_menu = false;
                self.help = false;
            }
            Action::Narrower => self.tree_percent = self.tree_percent.saturating_sub(5).max(15),
            Action::Wider => self.tree_percent = (self.tree_percent + 5).min(80),
            Action::Help => self.help = true,
            Action::Noop => {}
        }
        Vec::new()
    }

    fn page_rows(&self) -> usize {
        match self.focus {
            Focus::Tree => usize::from(self.panes().tree.height.saturating_sub(2)).max(1),
            Focus::Right => usize::from(self.right_inner().1).max(1),
            Focus::Hex => self.hex_inner_rows().max(1),
        }
    }

    fn move_by(&mut self, delta: isize) {
        match self.focus {
            Focus::Tree => self.move_selection(delta),
            Focus::Right => {
                let max = self.right_len().saturating_sub(1);
                self.right_scroll = self.right_scroll.saturating_add_signed(delta).min(max);
            }
            Focus::Hex => {
                let max = hexview::line_count(self.hex.bytes.len()).saturating_sub(1);
                self.hex_scroll = self.hex_scroll.saturating_add_signed(delta).min(max);
            }
        }
    }

    fn move_selection(&mut self, delta: isize) {
        let rows = self.tree.visible_rows();
        let position = rows
            .iter()
            .position(|row| row.id == self.tree.selected)
            .unwrap_or(0);
        let target = position
            .saturating_add_signed(delta)
            .min(rows.len().saturating_sub(1));
        let Some(row) = rows.get(target) else {
            return;
        };
        self.select(row.id);
    }

    /// Selects a node and points the hex pane and scroll positions at it.
    pub fn select(&mut self, id: usize) {
        if self.tree.selected != id {
            self.right_scroll = 0;
        }
        self.tree.selected = id;
        self.refresh_hex();
    }

    fn refresh_hex(&mut self) {
        let source = match self.tree.selected_kind().clone() {
            NodeKind::Part(i) => self.part_source(i),
            NodeKind::Media(i) => self.doc.media.get(i).map(|media| HexSource {
                label: media.name.clone(),
                bytes: Arc::new(media.data.to_vec()),
            }),
            _ => None,
        };
        let source = source.unwrap_or_else(|| HexSource {
            label: "file".to_string(),
            bytes: Arc::clone(&self.bytes),
        });
        if source.label != self.hex.label {
            self.hex_scroll = 0;
        }
        self.hex = source;
    }

    fn part_source(&mut self, index: usize) -> Option<HexSource> {
        let part = self.parts.get(index)?;
        match container::part_bytes(&self.bytes, part) {
            Ok(bytes) => Some(HexSource {
                label: part.display_name(),
                bytes: Arc::new(bytes),
            }),
            Err(message) => {
                self.toast(message);
                None
            }
        }
    }

    fn collapse(&mut self) {
        if self.focus != Focus::Tree {
            return;
        }
        let id = self.tree.selected;
        if self.tree.node(id).expanded {
            self.tree.collapse(id);
            return;
        }
        let Some(parent) = self.tree.node(id).parent else {
            return;
        };
        self.select(parent);
    }

    fn expand(&mut self) {
        if self.focus != Focus::Tree {
            return;
        }
        let id = self.tree.selected;
        if !self.tree.node(id).expanded {
            let doc = Arc::clone(&self.doc);
            let sources = Sources {
                doc: &doc,
                parts: &self.parts,
            };
            self.tree.expand(&sources, id);
            return;
        }
        let first = self
            .tree
            .node(id)
            .children
            .as_ref()
            .and_then(|children| children.first().copied());
        let Some(first) = first else {
            return;
        };
        self.select(first);
    }

    fn toggle(&mut self) {
        if self.focus != Focus::Tree {
            return;
        }
        let id = self.tree.selected;
        if let NodeKind::Part(i) = self.tree.node(id).kind {
            if self.parts.get(i).is_some_and(Part::is_xml) {
                self.show_xml();
            }
            return;
        }
        if self.tree.node(id).expanded {
            self.tree.collapse(id);
            return;
        }
        let doc = Arc::clone(&self.doc);
        let sources = Sources {
            doc: &doc,
            parts: &self.parts,
        };
        self.tree.expand(&sources, id);
    }

    fn show(&mut self, view: View) {
        if self.view != view {
            self.right_scroll = 0;
        }
        self.view = view;
    }

    fn show_xml(&mut self) {
        let NodeKind::Part(index) = *self.tree.selected_kind() else {
            self.toast("select a part under Container to see its XML");
            return;
        };
        let Some(part) = self.parts.get(index) else {
            return;
        };
        if !part.is_xml() {
            self.toast(format!("{} is not XML", part.display_name()));
            return;
        }
        if self.xml.as_ref().is_none_or(|(cached, _)| *cached != index) {
            let text = match container::part_bytes(&self.bytes, part) {
                Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
                Err(message) => {
                    self.toast(message);
                    return;
                }
            };
            let pretty = crate::xmlpretty::pretty(&text, 2)
                .lines()
                .map(str::to_string)
                .collect();
            self.xml = Some((index, pretty));
        }
        self.show(View::Xml);
    }

    fn show_markdown(&mut self) {
        if self.markdown.is_none() {
            let text = docboss_output::to_markdown(&self.doc, &MarkdownOptions::default());
            self.markdown = Some(text.lines().map(str::to_string).collect());
        }
        self.show(View::Markdown);
    }

    fn turn_page(&mut self, delta: isize) {
        let Some(count) = self.page_count() else {
            return;
        };
        self.preview.page = self
            .preview
            .page
            .saturating_add_signed(delta)
            .min(count.saturating_sub(1));
    }

    /// How many lines the upper right pane holds in the current view.
    pub fn right_len(&self) -> usize {
        match self.view {
            View::Inspector => inspector::lines(&self.sources(), self.tree.selected_kind()).len(),
            View::Xml => self.xml.as_ref().map_or(0, |(_, lines)| lines.len()),
            View::Markdown => self.markdown.as_ref().map_or(0, Vec::len),
            View::Preview => 0,
        }
    }

    fn run_search(&mut self) {
        self.search.editing = false;
        self.search.hits = search::search(&self.doc, &self.parts, &self.search.query);
        self.search.current = 0;
        if self.search.hits.is_empty() {
            let message = format!("no match for {:?}", self.search.query);
            self.toast(message);
            return;
        }
        self.reveal_hit();
    }

    fn jump_hit(&mut self, delta: isize) {
        let count = self.search.hits.len();
        if count == 0 {
            return;
        }
        let current = self.search.current as isize + delta;
        self.search.current = current.rem_euclid(count as isize) as usize;
        self.reveal_hit();
    }

    fn reveal_hit(&mut self) {
        let Some(kind) = self.search.hits.get(self.search.current).cloned() else {
            return;
        };
        let doc = Arc::clone(&self.doc);
        let sources = Sources {
            doc: &doc,
            parts: &self.parts,
        };
        if !self.tree.reveal(&sources, &kind) {
            return;
        }
        self.focus = Focus::Tree;
        self.right_scroll = 0;
        self.refresh_hex();
        let message = format!(
            "match {} of {}",
            self.search.current + 1,
            self.search.hits.len()
        );
        self.toast(message);
    }

    /// The selected node's text: a paragraph's text, a run's text, else
    /// its label.
    pub fn node_text(&self) -> String {
        let sources = self.sources();
        let kind = self.tree.selected_kind();
        let NodeKind::Content(story, steps) = kind else {
            return crate::tree::label(&sources, kind);
        };
        match resolve(&self.doc, *story, steps) {
            Some(Resolved::Block(Block::Paragraph(paragraph))) => paragraph.text(),
            Some(Resolved::Inline(Inline::Run(run))) => run
                .content
                .iter()
                .filter_map(|item| match item {
                    RunContent::Text(text) => Some(text.as_str()),
                    RunContent::Tab => Some("\t"),
                    _ => None,
                })
                .collect(),
            _ => crate::tree::label(&sources, kind),
        }
    }

    fn shell_quote(text: &str) -> String {
        if text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./".contains(c))
        {
            return text.to_string();
        }
        format!("'{}'", text.replace('\'', "'\\''"))
    }

    fn yank_text(&self, target: YankTarget) -> String {
        match target {
            YankTarget::Text => self.node_text(),
            YankTarget::Inspector => inspector::plain(&inspector::lines(
                &self.sources(),
                self.tree.selected_kind(),
            )),
            YankTarget::Hexdump => hexview::hexdump(&self.hex.bytes, HEXDUMP_LIMIT),
            YankTarget::Command => {
                let file = App::shell_quote(&self.target);
                match self.tree.selected_kind() {
                    NodeKind::Part(i) => {
                        let part = self
                            .parts
                            .get(*i)
                            .map(Part::display_name)
                            .unwrap_or_default();
                        let verb = if self.view == View::Xml { "xml" } else { "hex" };
                        format!("docboss {verb} {file} {}", App::shell_quote(&part))
                    }
                    _ if self.view == View::Markdown => format!("docboss md {file}"),
                    _ if self.view == View::Preview => {
                        format!(
                            "docboss render {file} --page {} -o page.png",
                            self.preview.page + 1
                        )
                    }
                    _ => format!("docboss json {file}"),
                }
            }
        }
    }
}

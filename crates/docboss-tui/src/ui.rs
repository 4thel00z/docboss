//! Drawing: the tree on the left, the inspector, XML, Markdown or page
//! preview on the upper right, the hex view below it and a status line.

use ratatui::layout::{Constraint, Direction, Layout as Split, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Clear, Paragraph};
use ratatui::Frame;

use crate::app::{App, Focus, LayoutState, View};
use crate::{hexview, inspector, preview};

/// The screen split into panes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Panes {
    pub tree: Rect,
    pub right: Rect,
    pub hex: Rect,
    pub status: Rect,
}

pub fn panes(area: Rect, tree_percent: u16) -> Panes {
    let [main, status] = Split::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .areas(area);
    let [tree, rest] = Split::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(tree_percent), Constraint::Min(1)])
        .areas(main);
    let [right, hex] = Split::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
        .areas(rest);
    Panes {
        tree,
        right,
        hex,
        status,
    }
}

fn pane(title: String, focused: bool) -> Block<'static> {
    let block = Block::bordered().title(title);
    if !focused {
        return block;
    }
    block.title_style(Style::default().add_modifier(Modifier::BOLD))
}

pub fn draw(app: &App, frame: &mut Frame) {
    let split = panes(frame.area(), app.tree_percent);
    draw_tree(app, frame, split.tree);
    draw_right(app, frame, split.right);
    draw_hex(app, frame, split.hex);
    draw_status(app, frame, split.status);
    if app.help {
        draw_help(frame);
    }
}

fn draw_tree(app: &App, frame: &mut Frame, area: Rect) {
    let sources = app.sources();
    let height = usize::from(area.height.saturating_sub(2));
    let rows = app.tree.visible_rows();
    let position = rows
        .iter()
        .position(|row| row.id == app.tree.selected)
        .unwrap_or(0);
    let offset = position.saturating_sub(height.saturating_sub(1));
    let lines: Vec<Line> = rows
        .iter()
        .skip(offset)
        .take(height)
        .map(|row| {
            let node = app.tree.node(row.id);
            let glyph = match (
                crate::tree::has_children(&sources, &node.kind),
                node.expanded,
            ) {
                (false, _) => "  ",
                (true, true) => "\u{25be} ",
                (true, false) => "\u{25b8} ",
            };
            let text = format!(
                "{}{glyph}{}",
                "  ".repeat(row.depth),
                crate::tree::label(&sources, &node.kind)
            );
            let style = match row.id == app.tree.selected {
                true => Style::default().add_modifier(Modifier::REVERSED),
                false => Style::default(),
            };
            Line::styled(text, style)
        })
        .collect();
    let title = format!("Tree {}/{}", position + 1, rows.len());
    frame.render_widget(
        Paragraph::new(Text::from(lines)).block(pane(title, app.focus == Focus::Tree)),
        area,
    );
}

fn scrolled(lines: Vec<Line<'static>>, scroll: usize, height: usize) -> Text<'static> {
    Text::from(
        lines
            .into_iter()
            .skip(scroll)
            .take(height)
            .collect::<Vec<_>>(),
    )
}

fn plain_lines(lines: &[String], scroll: usize, height: usize) -> Text<'static> {
    Text::from(
        lines
            .iter()
            .skip(scroll)
            .take(height)
            .map(|line| Line::raw(line.clone()))
            .collect::<Vec<_>>(),
    )
}

fn draw_right(app: &App, frame: &mut Frame, area: Rect) {
    let focused = app.focus == Focus::Right;
    let height = usize::from(area.height.saturating_sub(2));
    let scroll = app.right_scroll;
    match app.view {
        View::Inspector => {
            let lines = inspector::lines(&app.sources(), app.tree.selected_kind())
                .into_iter()
                .map(|line| match line {
                    inspector::Line::Heading(text) => {
                        Line::styled(text, Style::default().add_modifier(Modifier::BOLD))
                    }
                    inspector::Line::Text(text) => Line::raw(text),
                })
                .collect();
            let block = pane("Inspector".to_string(), focused);
            frame.render_widget(
                Paragraph::new(scrolled(lines, scroll, height)).block(block),
                area,
            );
        }
        View::Xml => {
            let (title, text) = match &app.xml {
                Some((index, lines)) => {
                    let name = app
                        .parts
                        .get(*index)
                        .map(|part| part.display_name())
                        .unwrap_or_default();
                    (format!("XML {name}"), plain_lines(lines, scroll, height))
                }
                None => ("XML".to_string(), Text::raw("")),
            };
            frame.render_widget(Paragraph::new(text).block(pane(title, focused)), area);
        }
        View::Markdown => {
            let lines: Vec<Line<'static>> = app
                .markdown
                .as_deref()
                .unwrap_or_default()
                .iter()
                .map(|line| match line.starts_with('#') {
                    true => {
                        Line::styled(line.clone(), Style::default().add_modifier(Modifier::BOLD))
                    }
                    false => Line::raw(line.clone()),
                })
                .collect();
            let block = pane("Markdown".to_string(), focused);
            frame.render_widget(
                Paragraph::new(scrolled(lines, scroll, height)).block(block),
                area,
            );
        }
        View::Preview => draw_preview(app, frame, area, focused),
    }
}

fn draw_preview(app: &App, frame: &mut Frame, area: Rect, focused: bool) {
    let pages = app
        .page_count()
        .map_or_else(|| "?".to_string(), |n| n.to_string());
    let mut title = format!("Page {} of {pages}", app.preview.page + 1);
    if app.preview.rendering {
        title.push_str(" (rendering)");
    }
    let block = pane(title, focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let status = match (&app.layout, &app.preview.error, &app.preview.pixmap) {
        (LayoutState::Failed(message), _, _) => Some(format!("layout failed: {message}")),
        (LayoutState::Pending | LayoutState::NotStarted, _, _) => {
            Some("laying out\u{2026}".to_string())
        }
        (_, Some(message), _) => Some(format!("render failed: {message}")),
        (_, None, None) => Some("rendering\u{2026}".to_string()),
        _ => None,
    };
    if let Some(message) = status {
        frame.render_widget(Paragraph::new(message), inner);
        return;
    }
    let Some(pixmap) = &app.preview.pixmap else {
        return;
    };
    let rows = inner.height.saturating_sub(1);
    let mut lines = preview::half_blocks(pixmap, inner.width, rows);
    let dropped = pixmap.diagnostics.len();
    let footer = match dropped {
        0 => "[ and ] turn pages".to_string(),
        n => format!("{n} items could not be painted; [ and ] turn pages"),
    };
    lines.push(Line::styled(
        footer,
        Style::default().add_modifier(Modifier::DIM),
    ));
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn draw_hex(app: &App, frame: &mut Frame, area: Rect) {
    let height = usize::from(area.height.saturating_sub(2));
    let lines: Vec<Line> = hexview::lines(&app.hex.bytes, app.hex_scroll, height)
        .into_iter()
        .map(Line::raw)
        .collect();
    let title = format!("Hex {} ({} bytes)", app.hex.label, app.hex.bytes.len());
    frame.render_widget(
        Paragraph::new(Text::from(lines)).block(pane(title, app.focus == Focus::Hex)),
        area,
    );
}

fn draw_status(app: &App, frame: &mut Frame, area: Rect) {
    if app.search.editing {
        let line = Line::from(vec![
            Span::styled("/", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(app.search.query.clone()),
        ]);
        frame.render_widget(Paragraph::new(line), area);
        return;
    }
    if app.yank_menu {
        let menu = "copy: t text  i inspector  c command  x hexdump  (any other key cancels)";
        frame.render_widget(Paragraph::new(menu), area);
        return;
    }
    let pages = app
        .page_count()
        .map(|n| format!(" \u{00b7} {n} pages"))
        .unwrap_or_default();
    let left = format!(
        "{} \u{00b7} {:?}{pages} \u{00b7} {} diagnostics",
        app.target,
        app.doc.format,
        app.doc.diagnostics.len()
    );
    let right = match &app.toast {
        Some((message, _)) => message.clone(),
        None => "? help  q quit".to_string(),
    };
    let width = usize::from(area.width);
    let gap = width
        .saturating_sub(left.chars().count() + right.chars().count())
        .max(1);
    let line = Line::from(vec![
        Span::styled(left, Style::default().add_modifier(Modifier::REVERSED)),
        Span::raw(" ".repeat(gap)),
        Span::raw(right),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

const HELP: [&str; 16] = [
    "j k \u{2191} \u{2193}   move in the focused pane",
    "h l \u{2190} \u{2192}   collapse / expand, go to parent / child",
    "Enter        toggle a node; open a part's XML",
    "g G PgUp PgDn  jump",
    "Tab          focus tree, right pane, hex",
    "i            inspector",
    "x            XML of the selected part",
    "m            Markdown preview",
    "p            page preview; [ ] turn pages",
    "/            search; n N next and previous match",
    "y            copy: t text, i inspector, c command, x hexdump",
    "< >          narrower / wider tree",
    "?            this help",
    "q Esc        quit",
    "",
    "any key closes this help",
];

fn draw_help(frame: &mut Frame) {
    let area = frame.area();
    let width = 64.min(area.width);
    let height = (HELP.len() as u16 + 2).min(area.height);
    let popup = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, popup);
    let lines: Vec<Line> = HELP.iter().map(|line| Line::raw(*line)).collect();
    frame.render_widget(
        Paragraph::new(Text::from(lines)).block(Block::bordered().title("Keys")),
        popup,
    );
}

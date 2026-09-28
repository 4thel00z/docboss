//! Interactive terminal explorer for DOCX and DOC files.
//!
//! The state machine ([`app`]), pane models ([`tree`], [`inspector`],
//! [`hexview`], [`preview`], [`search`]), key mapping ([`input`]) and
//! drawing ([`ui`]) are pure and testable headless; only [`run`] touches
//! the terminal. Layout and page rendering run on worker threads and
//! report back over a channel, so input never waits on them.

pub mod app;
pub mod clipboard;
pub mod container;
pub mod hexview;
pub mod input;
pub mod inspector;
pub mod preview;
pub mod search;
pub mod tree;
pub mod ui;
pub mod xmlpretty;

use std::sync::mpsc::{self, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use docboss_layout::Layout;
use docboss_model::Document;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::app::{App, Cmd, LayoutState, Msg};

const TICK: Duration = Duration::from_millis(100);

/// Runs a command to completion on the calling thread and returns the
/// message it answers with. The event loop calls this on worker threads;
/// tests call it directly.
pub fn perform(cmd: Cmd, doc: &Document, layout: Option<&Arc<Layout>>) -> Msg {
    match cmd {
        Cmd::Layout => Msg::LayoutReady(Ok(Arc::new(docboss_layout::layout_document(doc)))),
        Cmd::Render {
            generation,
            page,
            scale,
        } => {
            let result = match layout {
                Some(layout) => {
                    docboss_render::render_page(layout, page, scale).map_err(|e| e.to_string())
                }
                None => Err("the document is not laid out yet".to_string()),
            };
            Msg::Rendered { generation, result }
        }
        Cmd::Copy(text) => match clipboard::copy(&text) {
            Ok(clipboard::Transport::Native) => {
                Msg::Toast(format!("copied {} characters", text.chars().count()))
            }
            Ok(clipboard::Transport::Osc52) => {
                Msg::Toast("sent to the terminal clipboard (OSC 52)".to_string())
            }
            Err(message) => Msg::Toast(format!("copy failed: {message}")),
        },
    }
}

fn spawn(app: &App, tx: &Sender<Msg>, cmd: Cmd) {
    let doc = Arc::clone(&app.doc);
    let layout = match &app.layout {
        LayoutState::Ready(layout) => Some(Arc::clone(layout)),
        _ => None,
    };
    let tx = tx.clone();
    std::thread::spawn(move || {
        let msg = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            perform(cmd, &doc, layout.as_ref())
        }))
        .unwrap_or_else(|_| Msg::Toast("a background task failed".to_string()));
        let _ = tx.send(msg);
    });
}

/// Turns the failure to attach to a terminal into an actionable message.
fn no_tty(err: std::io::Error) -> std::io::Error {
    match err.raw_os_error() {
        Some(6) | Some(25) => {
            std::io::Error::new(err.kind(), "docboss tui requires an interactive terminal")
        }
        _ => err,
    }
}

/// Restores the terminal on drop, so panics and early returns never leave
/// the shell in raw mode.
struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        disable_raw_mode().ok();
        execute!(std::io::stdout(), LeaveAlternateScreen).ok();
    }
}

/// Runs the explorer until the user quits. `bytes` are the file's bytes,
/// `doc` the document read from them, and `target` the path as given,
/// used in copied commands. Only terminal I/O errors are returned.
pub fn run(doc: Document, bytes: Vec<u8>, target: String) -> std::io::Result<()> {
    enable_raw_mode().map_err(no_tty)?;
    let guard = TerminalGuard;
    execute!(std::io::stdout(), EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(std::io::stdout()))?;
    let size = terminal.size()?;
    let mut app = App::new(
        Arc::new(doc),
        Arc::new(bytes),
        target,
        (size.width, size.height),
    );
    let (tx, rx) = mpsc::channel::<Msg>();
    let mut last_tick = Instant::now();
    loop {
        terminal.draw(|frame| ui::draw(&app, frame))?;
        let timeout = TICK.saturating_sub(last_tick.elapsed());
        let mut messages: Vec<Msg> = rx.try_iter().collect();
        if event::poll(timeout)? {
            match event::read()? {
                Event::Key(key) => messages.push(Msg::Key(key)),
                Event::Resize(width, height) => messages.push(Msg::Resize(width, height)),
                _ => {}
            }
        }
        if last_tick.elapsed() >= TICK {
            messages.push(Msg::Tick);
            last_tick = Instant::now();
        }
        messages.extend(rx.try_iter());
        for msg in messages {
            for cmd in app.update(msg) {
                spawn(&app, &tx, cmd);
            }
        }
        if app.should_quit {
            break;
        }
    }
    drop(guard);
    Ok(())
}

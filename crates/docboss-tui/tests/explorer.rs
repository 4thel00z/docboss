use std::path::{Path, PathBuf};
use std::sync::Arc;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use docboss_core::Options;
use docboss_tui::app::{App, Cmd, LayoutState, Msg, View};
use docboss_tui::tree::NodeKind;
use ratatui::backend::TestBackend;
use ratatui::Terminal;

const WIDTH: u16 = 120;
const HEIGHT: u16 = 40;

fn fixtures() -> Vec<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut files: Vec<PathBuf> = ["docboss-docx", "docboss-doc"]
        .iter()
        .flat_map(|krate| std::fs::read_dir(root.join(krate).join("tests/fixtures")).unwrap())
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            matches!(
                path.extension().and_then(|e| e.to_str()),
                Some("docx" | "doc")
            )
        })
        .collect();
    files.sort();
    files
}

fn password_for(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?;
    match name {
        "encrypted-cryptoapi.doc" => Some("password".to_string()),
        "encrypted-rc4.doc" => Some("tika".to_string()),
        _ => None,
    }
}

fn fixture(name: &str) -> PathBuf {
    fixtures()
        .into_iter()
        .find(|path| path.file_name().is_some_and(|n| n == name))
        .unwrap()
}

fn open(path: &Path) -> App {
    let bytes = std::fs::read(path).unwrap();
    let options = Options {
        password: password_for(path),
    };
    let doc = docboss_core::read_with(&bytes, &options).unwrap();
    App::new(
        Arc::new(doc),
        Arc::new(bytes),
        path.display().to_string(),
        (WIDTH, HEIGHT),
    )
}

/// Feeds a message and runs every command it returns to completion, the
/// way the event loop's workers would. Copies are collected instead of
/// touching the clipboard.
fn feed(app: &mut App, msg: Msg, copies: &mut Vec<String>) {
    let mut queue = vec![msg];
    while let Some(msg) = queue.pop() {
        for cmd in app.update(msg) {
            if let Cmd::Copy(text) = cmd {
                copies.push(text);
                continue;
            }
            let layout = match &app.layout {
                LayoutState::Ready(layout) => Some(Arc::clone(layout)),
                _ => None,
            };
            queue.push(docboss_tui::perform(cmd, &app.doc, layout.as_ref()));
        }
    }
}

fn key(code: KeyCode) -> Msg {
    Msg::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn press(app: &mut App, codes: &[KeyCode]) -> Vec<String> {
    let mut copies = Vec::new();
    for code in codes {
        feed(app, key(*code), &mut copies);
    }
    copies
}

fn type_text(app: &mut App, text: &str) {
    let mut copies = Vec::new();
    for c in text.chars() {
        feed(app, key(KeyCode::Char(c)), &mut copies);
    }
}

fn screen(app: &App) -> String {
    let mut terminal = Terminal::new(TestBackend::new(WIDTH, HEIGHT)).unwrap();
    terminal
        .draw(|frame| docboss_tui::ui::draw(app, frame))
        .unwrap();
    let buffer = terminal.backend().buffer();
    let mut out = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            out.push_str(buffer[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}

#[test]
fn every_fixture_survives_a_long_key_sequence() {
    let keys = [
        KeyCode::Char('j'),
        KeyCode::Char('j'),
        KeyCode::Char('l'),
        KeyCode::Char('l'),
        KeyCode::Char('k'),
        KeyCode::Enter,
        KeyCode::Char('h'),
        KeyCode::Tab,
        KeyCode::PageDown,
        KeyCode::Char('G'),
        KeyCode::Char('g'),
        KeyCode::Char('m'),
        KeyCode::Char('x'),
        KeyCode::Char('i'),
        KeyCode::Char('p'),
        KeyCode::Char(']'),
        KeyCode::Char('['),
        KeyCode::Char('<'),
        KeyCode::Char('>'),
        KeyCode::Char('?'),
        KeyCode::Char('n'),
        KeyCode::Char('N'),
        KeyCode::Char('y'),
        KeyCode::Char('i'),
    ];
    for path in fixtures() {
        let mut app = open(&path);
        let mut state: u64 = 0x2545_f491_4f6c_dd1d;
        let mut copies = Vec::new();
        for step in 0..300 {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let code = keys[(state >> 33) as usize % keys.len()];
            feed(&mut app, key(code), &mut copies);
            if app.should_quit {
                break;
            }
            if step % 50 == 0 {
                feed(
                    &mut app,
                    Msg::Resize(WIDTH - (step % 7) as u16, HEIGHT),
                    &mut copies,
                );
                feed(&mut app, Msg::Tick, &mut copies);
                feed(&mut app, Msg::Tick, &mut copies);
            }
            screen(&app);
        }
        assert!(!app.should_quit, "{}", path.display());
    }
}

/// ECMA-376 Part 1 §17.7.2: the inspector resolves a paragraph's
/// properties through its style chain as well as showing the direct ones.
#[test]
fn inspector_shows_direct_and_resolved_paragraph_properties() {
    let mut app = open(&fixture("libreoffice-rich.docx"));
    assert!(screen(&app).contains("Sections (1)"));
    press(
        &mut app,
        &[
            KeyCode::Char('j'),
            KeyCode::Char('l'),
            KeyCode::Char('l'),
            KeyCode::Char('l'),
            KeyCode::Char('l'),
        ],
    );
    assert!(matches!(app.tree.selected_kind(), NodeKind::Content(_, steps) if steps.len() == 1));
    let lines = docboss_tui::inspector::plain(&docboss_tui::inspector::lines(
        &app.sources(),
        app.tree.selected_kind(),
    ));
    assert!(lines.contains("Resolved properties"), "{lines}");
    assert!(lines.contains("Direct properties"));
    assert!(lines.contains("style: "));
    assert!(screen(&app).contains("\u{00b6}"));
}

#[test]
fn search_reveals_parts_and_x_shows_their_xml() {
    let mut app = open(&fixture("libreoffice-rich.docx"));
    press(&mut app, &[KeyCode::Char('/')]);
    type_text(&mut app, "word/document.xml");
    press(&mut app, &[KeyCode::Enter]);
    let NodeKind::Part(index) = *app.tree.selected_kind() else {
        panic!("selected {:?}", app.tree.selected_kind());
    };
    assert_eq!(app.parts[index].path, "word/document.xml");
    assert_eq!(app.hex.label, "word/document.xml");
    assert!(app.hex.bytes.starts_with(b"<?xml"));
    press(&mut app, &[KeyCode::Char('x')]);
    assert_eq!(app.view, View::Xml);
    let (_, lines) = app.xml.as_ref().unwrap();
    assert!(lines
        .iter()
        .any(|line| line.trim_start().starts_with("<w:body")));
    assert!(screen(&app).contains("XML word/document.xml"));
}

#[test]
fn search_finds_paragraph_text_and_cycles() {
    let mut app = open(&fixture("tables.doc"));
    let word = app
        .doc
        .blocks()
        .find_map(|block| match block {
            docboss_model::Block::Paragraph(p) => p
                .text()
                .split_whitespace()
                .find(|w| w.len() > 3)
                .map(str::to_string),
            docboss_model::Block::Table(_) => None,
        })
        .unwrap_or_else(|| "cell".to_string());
    press(&mut app, &[KeyCode::Char('/')]);
    type_text(&mut app, &word);
    press(&mut app, &[KeyCode::Enter]);
    assert!(!app.search.hits.is_empty(), "no hit for {word}");
    assert_eq!(app.tree.selected_kind(), &app.search.hits[0]);
    let first = app.tree.selected;
    press(&mut app, &[KeyCode::Char('n'), KeyCode::Char('N')]);
    assert_eq!(app.tree.selected, first);
}

#[test]
fn compound_file_streams_are_listed_with_hex() {
    let mut app = open(&fixture("text.doc"));
    assert!(app.parts.iter().any(|part| part.path == "WordDocument"));
    press(&mut app, &[KeyCode::Char('/')]);
    type_text(&mut app, "WordDocument");
    press(&mut app, &[KeyCode::Enter]);
    assert_eq!(app.hex.label, "WordDocument");
    assert_eq!(&app.hex.bytes[..2], &[0xec, 0xa5]);
    press(&mut app, &[KeyCode::Char('x')]);
    assert_eq!(app.view, View::Inspector);
    assert!(app
        .toast
        .as_ref()
        .is_some_and(|(message, _)| message.contains("not XML")));
}

#[test]
fn page_preview_lays_out_renders_and_turns_pages() {
    let mut app = open(&fixture("sections.doc"));
    press(&mut app, &[KeyCode::Char('p')]);
    assert_eq!(app.view, View::Preview);
    let pages = app.page_count().unwrap();
    assert!(pages >= 1);
    let pixmap = app.preview.pixmap.as_ref().unwrap();
    let (cols, rows) = app.right_inner();
    assert!(pixmap.width <= u32::from(cols));
    assert!(pixmap.height <= u32::from(rows) * 2);
    assert!(screen(&app).contains('\u{2580}'));
    let generation = app.preview.generation;
    press(&mut app, &[KeyCode::Char(']')]);
    if pages > 1 {
        assert_eq!(app.preview.page, 1);
        assert!(app.preview.generation > generation);
    }
    press(&mut app, &[KeyCode::Char('['), KeyCode::Char('[')]);
    assert_eq!(app.preview.page, 0);
}

#[test]
fn copy_menu_copies_paragraph_text_and_commands() {
    let mut app = open(&fixture("lists.doc"));
    press(
        &mut app,
        &[
            KeyCode::Char('j'),
            KeyCode::Char('l'),
            KeyCode::Char('l'),
            KeyCode::Char('l'),
            KeyCode::Char('l'),
        ],
    );
    let NodeKind::Content(story, steps) = app.tree.selected_kind().clone() else {
        panic!("selected {:?}", app.tree.selected_kind());
    };
    let Some(docboss_tui::tree::Resolved::Block(docboss_model::Block::Paragraph(paragraph))) =
        docboss_tui::tree::resolve(&app.doc, story, &steps)
    else {
        panic!("not a paragraph");
    };
    let expected = paragraph.text();
    let copies = press(&mut app, &[KeyCode::Char('y'), KeyCode::Char('t')]);
    assert_eq!(copies, vec![expected]);
    press(&mut app, &[KeyCode::Char('m')]);
    let copies = press(&mut app, &[KeyCode::Char('y'), KeyCode::Char('c')]);
    assert!(copies[0].starts_with("docboss md "), "{copies:?}");
    assert!(!app.yank_menu);
}

#[test]
fn markdown_view_shows_the_documents_markdown() {
    let mut app = open(&fixture("lists.doc"));
    press(&mut app, &[KeyCode::Char('m')]);
    let expected =
        docboss_output::to_markdown(&app.doc, &docboss_output::MarkdownOptions::default());
    let first = expected
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap()
        .to_string();
    assert!(screen(&app).contains(first.trim()));
}

#[test]
fn quit_and_escape_contexts() {
    let mut app = open(&fixture("text.doc"));
    press(&mut app, &[KeyCode::Char('/'), KeyCode::Esc]);
    assert!(!app.search.editing);
    assert!(!app.should_quit);
    press(&mut app, &[KeyCode::Char('?'), KeyCode::Char('q')]);
    assert!(!app.help);
    assert!(!app.should_quit);
    press(&mut app, &[KeyCode::Char('q')]);
    assert!(app.should_quit);
}

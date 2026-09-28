//! Key-event to action mapping, pure so bindings are unit-testable.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

/// Where key events go: the search prompt, the copy menu and the help
/// overlay take keys before the normal bindings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Context {
    Normal,
    Search,
    Yank,
    Help,
}

/// What the copy menu copies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum YankTarget {
    /// The selected node's text.
    Text,
    /// The inspector contents.
    Inspector,
    /// A `docboss` command reproducing the current view.
    Command,
    /// A hexdump of the bytes in the hex pane.
    Hexdump,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Up,
    Down,
    Collapse,
    Expand,
    Toggle,
    Top,
    Bottom,
    PageUp,
    PageDown,
    FocusNext,
    ShowInspector,
    ShowXml,
    ShowMarkdown,
    ShowPreview,
    PreviousPage,
    NextPage,
    OpenSearch,
    SearchChar(char),
    SearchBackspace,
    SearchAccept,
    SearchCancel,
    NextHit,
    PreviousHit,
    OpenYank,
    Yank(YankTarget),
    CloseMenu,
    Narrower,
    Wider,
    Help,
    Quit,
    Noop,
}

pub fn action(key: KeyEvent, context: Context) -> Action {
    if key.kind != KeyEventKind::Press {
        return Action::Noop;
    }
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return Action::Quit;
    }
    match context {
        Context::Search => search_action(key.code),
        Context::Yank => yank_action(key.code),
        Context::Help => Action::CloseMenu,
        Context::Normal => normal_action(key.code),
    }
}

fn search_action(code: KeyCode) -> Action {
    match code {
        KeyCode::Esc => Action::SearchCancel,
        KeyCode::Enter => Action::SearchAccept,
        KeyCode::Backspace => Action::SearchBackspace,
        KeyCode::Char(c) => Action::SearchChar(c),
        _ => Action::Noop,
    }
}

fn yank_action(code: KeyCode) -> Action {
    match code {
        KeyCode::Char('t') => Action::Yank(YankTarget::Text),
        KeyCode::Char('i') => Action::Yank(YankTarget::Inspector),
        KeyCode::Char('c') => Action::Yank(YankTarget::Command),
        KeyCode::Char('x') => Action::Yank(YankTarget::Hexdump),
        _ => Action::CloseMenu,
    }
}

fn normal_action(code: KeyCode) -> Action {
    match code {
        KeyCode::Char('q') | KeyCode::Esc => Action::Quit,
        KeyCode::Up | KeyCode::Char('k') => Action::Up,
        KeyCode::Down | KeyCode::Char('j') => Action::Down,
        KeyCode::Left | KeyCode::Char('h') => Action::Collapse,
        KeyCode::Right | KeyCode::Char('l') => Action::Expand,
        KeyCode::Enter | KeyCode::Char(' ') => Action::Toggle,
        KeyCode::Char('g') | KeyCode::Home => Action::Top,
        KeyCode::Char('G') | KeyCode::End => Action::Bottom,
        KeyCode::PageUp => Action::PageUp,
        KeyCode::PageDown => Action::PageDown,
        KeyCode::Tab => Action::FocusNext,
        KeyCode::Char('i') => Action::ShowInspector,
        KeyCode::Char('x') => Action::ShowXml,
        KeyCode::Char('m') => Action::ShowMarkdown,
        KeyCode::Char('p') => Action::ShowPreview,
        KeyCode::Char('[') => Action::PreviousPage,
        KeyCode::Char(']') => Action::NextPage,
        KeyCode::Char('/') => Action::OpenSearch,
        KeyCode::Char('n') => Action::NextHit,
        KeyCode::Char('N') => Action::PreviousHit,
        KeyCode::Char('y') => Action::OpenYank,
        KeyCode::Char('<') => Action::Narrower,
        KeyCode::Char('>') => Action::Wider,
        KeyCode::Char('?') => Action::Help,
        _ => Action::Noop,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn contexts_route_keys() {
        assert_eq!(
            action(press(KeyCode::Char('q')), Context::Normal),
            Action::Quit
        );
        assert_eq!(
            action(press(KeyCode::Char('q')), Context::Search),
            Action::SearchChar('q')
        );
        assert_eq!(
            action(press(KeyCode::Char('t')), Context::Yank),
            Action::Yank(YankTarget::Text)
        );
        assert_eq!(
            action(press(KeyCode::Char('z')), Context::Yank),
            Action::CloseMenu
        );
        assert_eq!(
            action(press(KeyCode::Char('j')), Context::Help),
            Action::CloseMenu
        );
        let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(action(ctrl_c, Context::Search), Action::Quit);
    }
}

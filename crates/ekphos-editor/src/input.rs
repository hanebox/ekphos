use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, PartialEq)]
pub enum InputAction {
    InsertChar(char),
    InsertNewline,
    DeleteChar,
    DeleteCharBefore,
    Move(super::cursor::CursorMove),
    None,
}

pub fn process_key(key: KeyEvent) -> InputAction {
    use super::cursor::CursorMove;

    match key.code {
        KeyCode::Char(c) => {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                InputAction::None
            } else {
                InputAction::InsertChar(c)
            }
        }
        KeyCode::Enter => InputAction::InsertNewline,
        KeyCode::Backspace => InputAction::DeleteCharBefore,
        KeyCode::Delete => InputAction::DeleteChar,
        KeyCode::Left => InputAction::Move(CursorMove::Back),
        KeyCode::Right => InputAction::Move(CursorMove::Forward),
        KeyCode::Up => InputAction::Move(CursorMove::Up),
        KeyCode::Down => InputAction::Move(CursorMove::Down),
        KeyCode::Home => InputAction::Move(CursorMove::Head),
        KeyCode::End => InputAction::Move(CursorMove::End),
        KeyCode::Tab => InputAction::InsertChar('\t'),
        _ => InputAction::None,
    }
}

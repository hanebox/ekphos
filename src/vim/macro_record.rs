//! Vim macro recording and playback (q, @)

use crossterm::event::KeyEvent;
use std::collections::HashMap;

#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub struct MacroState {
    macros: HashMap<char, Vec<KeyEvent>>,
    recording: Option<char>,
    current_keys: Vec<KeyEvent>,
    last_played: Option<char>,
}

impl MacroState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_recording(&self) -> bool {
        self.recording.is_some()
    }

    pub fn recording_register(&self) -> Option<char> {
        self.recording
    }

    pub fn start_recording(&mut self, register: char) {
        self.recording = Some(register);
        self.current_keys.clear();
    }

    pub fn stop_recording(&mut self) {
        if let Some(reg) = self.recording.take() {
            if !self.current_keys.is_empty() {
                self.macros.insert(reg, self.current_keys.clone());
            }
            self.current_keys.clear();
        }
    }

    pub fn record_key(&mut self, key: KeyEvent) {
        if self.recording.is_some() {
            self.current_keys.push(key);
        }
    }

    pub fn get_macro(&self, register: char) -> Option<&Vec<KeyEvent>> {
        self.macros.get(&register)
    }

    #[allow(dead_code)]
    pub fn get_last_macro(&self) -> Option<&Vec<KeyEvent>> {
        self.last_played.and_then(|r| self.macros.get(&r))
    }

    pub fn set_last_played(&mut self, register: char) {
        self.last_played = Some(register);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyModifiers};

    fn make_key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn make_key_with_mod(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    // ==================== Basic Recording Tests ====================

    #[test]
    fn test_new_state() {
        let state = MacroState::new();
        assert!(!state.is_recording());
        assert_eq!(state.recording_register(), None);
    }

    #[test]
    fn test_start_recording() {
        let mut state = MacroState::new();
        assert!(!state.is_recording());
        state.start_recording('a');
        assert!(state.is_recording());
        assert_eq!(state.recording_register(), Some('a'));
    }

    #[test]
    fn test_start_recording_different_registers() {
        let mut state = MacroState::new();

        state.start_recording('a');
        assert_eq!(state.recording_register(), Some('a'));
        state.stop_recording();

        state.start_recording('z');
        assert_eq!(state.recording_register(), Some('z'));
        state.stop_recording();
    }

    #[test]
    fn test_record_and_stop() {
        let mut state = MacroState::new();
        state.start_recording('a');
        state.record_key(make_key(KeyCode::Char('j')));
        state.record_key(make_key(KeyCode::Char('k')));
        state.stop_recording();

        assert!(!state.is_recording());
        let recorded = state.get_macro('a').unwrap();
        assert_eq!(recorded.len(), 2);
    }

    #[test]
    fn test_record_single_key() {
        let mut state = MacroState::new();
        state.start_recording('a');
        state.record_key(make_key(KeyCode::Char('x')));
        state.stop_recording();

        let recorded = state.get_macro('a').unwrap();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].code, KeyCode::Char('x'));
    }

    #[test]
    fn test_record_many_keys() {
        let mut state = MacroState::new();
        state.start_recording('a');
        for i in 0..100 {
            state.record_key(make_key(KeyCode::Char('j')));
        }
        state.stop_recording();

        let recorded = state.get_macro('a').unwrap();
        assert_eq!(recorded.len(), 100);
    }

    // ==================== Empty Macro Tests ====================

    #[test]
    fn test_empty_macro_not_saved() {
        let mut state = MacroState::new();
        state.start_recording('a');
        state.stop_recording();
        assert!(state.get_macro('a').is_none());
    }

    #[test]
    fn test_stop_without_start() {
        let mut state = MacroState::new();
        state.stop_recording();
        assert!(!state.is_recording());
    }

    // ==================== Overwrite Tests ====================

    #[test]
    fn test_overwrite_macro() {
        let mut state = MacroState::new();
        state.start_recording('a');
        state.record_key(make_key(KeyCode::Char('j')));
        state.stop_recording();

        state.start_recording('a');
        state.record_key(make_key(KeyCode::Char('k')));
        state.record_key(make_key(KeyCode::Char('l')));
        state.stop_recording();

        let recorded = state.get_macro('a').unwrap();
        assert_eq!(recorded.len(), 2);
        assert_eq!(recorded[0].code, KeyCode::Char('k'));
        assert_eq!(recorded[1].code, KeyCode::Char('l'));
    }

    // ==================== Get Macro Tests ====================

    #[test]
    fn test_get_nonexistent_macro() {
        let state = MacroState::new();
        assert!(state.get_macro('a').is_none());
        assert!(state.get_macro('z').is_none());
    }

    #[test]
    fn test_get_macro_preserves_order() {
        let mut state = MacroState::new();
        state.start_recording('a');
        state.record_key(make_key(KeyCode::Char('1')));
        state.record_key(make_key(KeyCode::Char('2')));
        state.record_key(make_key(KeyCode::Char('3')));
        state.stop_recording();

        let recorded = state.get_macro('a').unwrap();
        assert_eq!(recorded[0].code, KeyCode::Char('1'));
        assert_eq!(recorded[1].code, KeyCode::Char('2'));
        assert_eq!(recorded[2].code, KeyCode::Char('3'));
    }

    // ==================== Multiple Registers Tests ====================

    #[test]
    fn test_multiple_registers() {
        let mut state = MacroState::new();

        state.start_recording('a');
        state.record_key(make_key(KeyCode::Char('a')));
        state.stop_recording();

        state.start_recording('b');
        state.record_key(make_key(KeyCode::Char('b')));
        state.record_key(make_key(KeyCode::Char('b')));
        state.stop_recording();

        let macro_a = state.get_macro('a').unwrap();
        let macro_b = state.get_macro('b').unwrap();
        assert_eq!(macro_a.len(), 1);
        assert_eq!(macro_b.len(), 2);
    }

    #[test]
    fn test_all_lowercase_registers() {
        let mut state = MacroState::new();
        for c in 'a'..='z' {
            state.start_recording(c);
            state.record_key(make_key(KeyCode::Char(c)));
            state.stop_recording();
        }

        for c in 'a'..='z' {
            let recorded = state.get_macro(c).unwrap();
            assert_eq!(recorded.len(), 1);
        }
    }

    // ==================== Last Played Tests ====================

    #[test]
    fn test_last_macro_initially_none() {
        let state = MacroState::new();
        assert!(state.get_last_macro().is_none());
    }

    #[test]
    fn test_set_last_played() {
        let mut state = MacroState::new();
        state.start_recording('a');
        state.record_key(make_key(KeyCode::Char('j')));
        state.stop_recording();

        state.set_last_played('a');
        assert!(state.get_last_macro().is_some());
        assert_eq!(state.get_last_macro().unwrap().len(), 1);
    }

    #[test]
    fn test_last_played_nonexistent_register() {
        let mut state = MacroState::new();
        state.set_last_played('x');
        assert!(state.get_last_macro().is_none());
    }

    #[test]
    fn test_last_played_updates() {
        let mut state = MacroState::new();

        state.start_recording('a');
        state.record_key(make_key(KeyCode::Char('a')));
        state.stop_recording();

        state.start_recording('b');
        state.record_key(make_key(KeyCode::Char('b')));
        state.record_key(make_key(KeyCode::Char('b')));
        state.stop_recording();

        state.set_last_played('a');
        assert_eq!(state.get_last_macro().unwrap().len(), 1);

        state.set_last_played('b');
        assert_eq!(state.get_last_macro().unwrap().len(), 2);
    }

    // ==================== Recording While Not Recording Tests ====================

    #[test]
    fn test_record_key_while_not_recording() {
        let mut state = MacroState::new();
        state.record_key(make_key(KeyCode::Char('j')));
        assert!(!state.is_recording());
    }

    // ==================== Key With Modifiers Tests ====================

    #[test]
    fn test_record_key_with_modifiers() {
        let mut state = MacroState::new();
        state.start_recording('a');
        state.record_key(make_key_with_mod(KeyCode::Char('c'), KeyModifiers::CONTROL));
        state.record_key(make_key_with_mod(KeyCode::Char('v'), KeyModifiers::CONTROL));
        state.stop_recording();

        let recorded = state.get_macro('a').unwrap();
        assert_eq!(recorded.len(), 2);
        assert_eq!(recorded[0].modifiers, KeyModifiers::CONTROL);
        assert_eq!(recorded[1].modifiers, KeyModifiers::CONTROL);
    }

    #[test]
    fn test_record_special_keys() {
        let mut state = MacroState::new();
        state.start_recording('a');
        state.record_key(make_key(KeyCode::Enter));
        state.record_key(make_key(KeyCode::Backspace));
        state.record_key(make_key(KeyCode::Tab));
        state.record_key(make_key(KeyCode::Esc));
        state.stop_recording();

        let recorded = state.get_macro('a').unwrap();
        assert_eq!(recorded.len(), 4);
    }

    // ==================== Start Recording Clears Buffer Tests ====================

    #[test]
    fn test_start_recording_clears_buffer() {
        let mut state = MacroState::new();
        state.start_recording('a');
        state.record_key(make_key(KeyCode::Char('x')));
        state.start_recording('a');
        state.record_key(make_key(KeyCode::Char('y')));
        state.stop_recording();

        let recorded = state.get_macro('a').unwrap();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].code, KeyCode::Char('y'));
    }
}

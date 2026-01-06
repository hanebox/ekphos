//! Vim register system

use std::collections::HashMap;

#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub struct RegisterMap {
    named: HashMap<char, RegisterContent>,
    numbered: [RegisterContent; 10],
    unnamed: RegisterContent,
    small_delete: RegisterContent,
    last_search: String,
    last_command: String,
    selected: Option<char>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub struct RegisterContent {
    pub text: String,
    pub linewise: bool,
}

#[allow(dead_code)]
impl RegisterMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn select(&mut self, reg: char) {
        self.selected = Some(reg);
    }

    pub fn clear_selection(&mut self) {
        self.selected = None;
    }

    pub fn get_selected(&self) -> Option<char> {
        self.selected
    }

    pub fn get(&self, reg: char) -> Option<&RegisterContent> {
        match reg {
            'a'..='z' => self.named.get(&reg),
            'A'..='Z' => self.named.get(&reg.to_ascii_lowercase()),
            '0'..='9' => {
                let idx = reg.to_digit(10).unwrap() as usize;
                Some(&self.numbered[idx])
            }
            '"' => Some(&self.unnamed),
            '-' => Some(&self.small_delete),
            '+' | '*' => Some(&self.unnamed),
            _ => None,
        }
    }

    pub fn get_for_paste(&self) -> &RegisterContent {
        if let Some(reg) = self.selected {
            self.get(reg).unwrap_or(&self.unnamed)
        } else {
            &self.unnamed
        }
    }

    pub fn set(&mut self, reg: char, content: RegisterContent) {
        match reg {
            'a'..='z' => {
                self.named.insert(reg, content);
            }
            'A'..='Z' => {
                let lower = reg.to_ascii_lowercase();
                if let Some(existing) = self.named.get_mut(&lower) {
                    existing.text.push_str(&content.text);
                } else {
                    self.named.insert(lower, content);
                }
            }
            '0'..='9' => {
                let idx = reg.to_digit(10).unwrap() as usize;
                self.numbered[idx] = content;
            }
            '"' => {
                self.unnamed = content;
            }
            '-' => {
                self.small_delete = content;
            }
            _ => {}
        }
    }

    pub fn yank(&mut self, text: String, linewise: bool) {
        let content = RegisterContent { text, linewise };
        self.unnamed = content.clone();
        self.numbered[0] = content.clone();

        if let Some(reg) = self.selected.take() {
            self.set(reg, content);
        }
    }

    pub fn delete(&mut self, text: String, linewise: bool) {
        let content = RegisterContent { text: text.clone(), linewise };
        self.unnamed = content.clone();

        if let Some(reg) = self.selected.take() {
            self.set(reg, content);
            return;
        }

        if !linewise && !text.contains('\n') {
            self.small_delete = content;
        } else {
            for i in (2..=9).rev() {
                self.numbered[i] = self.numbered[i - 1].clone();
            }
            self.numbered[1] = content;
        }
    }

    pub fn set_search(&mut self, pattern: String) {
        self.last_search = pattern;
    }

    pub fn get_search(&self) -> &str {
        &self.last_search
    }

    pub fn set_command(&mut self, command: String) {
        self.last_command = command;
    }

    pub fn get_command(&self) -> &str {
        &self.last_command
    }

    pub fn is_clipboard_selected(&self) -> bool {
        matches!(self.selected, Some('+') | Some('*'))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== Basic Tests ====================

    #[test]
    fn test_new_register_map() {
        let regs = RegisterMap::new();
        assert!(regs.get_selected().is_none());
    }

    #[test]
    fn test_unnamed_register_initially_empty() {
        let regs = RegisterMap::new();
        assert_eq!(regs.get('"').unwrap().text, "");
    }

    // ==================== Yank Tests ====================

    #[test]
    fn test_yank_to_unnamed() {
        let mut regs = RegisterMap::new();
        regs.yank("hello".to_string(), false);
        assert_eq!(regs.get('"').unwrap().text, "hello");
        assert_eq!(regs.get('0').unwrap().text, "hello");
    }

    #[test]
    fn test_yank_to_named() {
        let mut regs = RegisterMap::new();
        regs.select('a');
        regs.yank("hello".to_string(), false);
        assert_eq!(regs.get('a').unwrap().text, "hello");
        assert_eq!(regs.get('"').unwrap().text, "hello");
    }

    #[test]
    fn test_yank_linewise() {
        let mut regs = RegisterMap::new();
        regs.yank("line\n".to_string(), true);
        assert!(regs.get('"').unwrap().linewise);
    }

    #[test]
    fn test_yank_characterwise() {
        let mut regs = RegisterMap::new();
        regs.yank("word".to_string(), false);
        assert!(!regs.get('"').unwrap().linewise);
    }

    #[test]
    fn test_yank_clears_selection() {
        let mut regs = RegisterMap::new();
        regs.select('a');
        regs.yank("hello".to_string(), false);
        assert!(regs.get_selected().is_none());
    }

    #[test]
    fn test_yank_multiple_times() {
        let mut regs = RegisterMap::new();
        regs.yank("first".to_string(), false);
        regs.yank("second".to_string(), false);
        assert_eq!(regs.get('"').unwrap().text, "second");
        assert_eq!(regs.get('0').unwrap().text, "second");
    }

    // ==================== Named Register Tests ====================

    #[test]
    fn test_all_lowercase_named_registers() {
        let mut regs = RegisterMap::new();
        for c in 'a'..='z' {
            regs.set(c, RegisterContent { text: c.to_string(), linewise: false });
        }
        for c in 'a'..='z' {
            assert_eq!(regs.get(c).unwrap().text, c.to_string());
        }
    }

    #[test]
    fn test_append_to_named() {
        let mut regs = RegisterMap::new();
        regs.set('a', RegisterContent { text: "hello".to_string(), linewise: false });
        regs.set('A', RegisterContent { text: " world".to_string(), linewise: false });
        assert_eq!(regs.get('a').unwrap().text, "hello world");
    }

    #[test]
    fn test_append_to_empty_register() {
        let mut regs = RegisterMap::new();
        regs.set('A', RegisterContent { text: "hello".to_string(), linewise: false });
        assert_eq!(regs.get('a').unwrap().text, "hello");
    }

    #[test]
    fn test_uppercase_get_returns_lowercase() {
        let mut regs = RegisterMap::new();
        regs.set('a', RegisterContent { text: "hello".to_string(), linewise: false });
        assert_eq!(regs.get('A').unwrap().text, "hello");
    }

    // ==================== Numbered Register Tests ====================

    #[test]
    fn test_numbered_register_0() {
        let mut regs = RegisterMap::new();
        regs.yank("yanked".to_string(), false);
        assert_eq!(regs.get('0').unwrap().text, "yanked");
    }

    #[test]
    fn test_all_numbered_registers() {
        let mut regs = RegisterMap::new();
        for i in 0..=9 {
            let c = char::from_digit(i, 10).unwrap();
            regs.set(c, RegisterContent { text: i.to_string(), linewise: false });
        }
        for i in 0..=9 {
            let c = char::from_digit(i, 10).unwrap();
            assert_eq!(regs.get(c).unwrap().text, i.to_string());
        }
    }

    #[test]
    fn test_delete_shifts_numbered() {
        let mut regs = RegisterMap::new();
        regs.delete("first\n".to_string(), true);
        regs.delete("second\n".to_string(), true);
        regs.delete("third\n".to_string(), true);
        assert_eq!(regs.get('1').unwrap().text, "third\n");
        assert_eq!(regs.get('2').unwrap().text, "second\n");
        assert_eq!(regs.get('3').unwrap().text, "first\n");
    }

    #[test]
    fn test_delete_shifts_up_to_9() {
        let mut regs = RegisterMap::new();
        for i in 1..=10 {
            regs.delete(format!("delete{}\n", i), true);
        }
        assert_eq!(regs.get('1').unwrap().text, "delete10\n");
        assert_eq!(regs.get('9').unwrap().text, "delete2\n");
    }

    // ==================== Small Delete Register Tests ====================

    #[test]
    fn test_small_delete() {
        let mut regs = RegisterMap::new();
        regs.delete("word".to_string(), false);
        assert_eq!(regs.get('-').unwrap().text, "word");
    }

    #[test]
    fn test_small_delete_not_linewise() {
        let mut regs = RegisterMap::new();
        regs.delete("small".to_string(), false);
        assert_eq!(regs.get('-').unwrap().text, "small");
    }

    #[test]
    fn test_linewise_delete_not_small() {
        let mut regs = RegisterMap::new();
        regs.delete("line\n".to_string(), true);
        assert_eq!(regs.get('1').unwrap().text, "line\n");
    }

    #[test]
    fn test_multiline_delete_not_small() {
        let mut regs = RegisterMap::new();
        regs.delete("line1\nline2".to_string(), false);
        assert_eq!(regs.get('1').unwrap().text, "line1\nline2");
    }

    // ==================== Selection Tests ====================

    #[test]
    fn test_select_register() {
        let mut regs = RegisterMap::new();
        regs.select('a');
        assert_eq!(regs.get_selected(), Some('a'));
    }

    #[test]
    fn test_clear_selection() {
        let mut regs = RegisterMap::new();
        regs.select('a');
        regs.clear_selection();
        assert!(regs.get_selected().is_none());
    }

    #[test]
    fn test_select_changes_register() {
        let mut regs = RegisterMap::new();
        regs.select('a');
        regs.select('b');
        assert_eq!(regs.get_selected(), Some('b'));
    }

    // ==================== Get For Paste Tests ====================

    #[test]
    fn test_get_for_paste_default() {
        let mut regs = RegisterMap::new();
        regs.yank("default".to_string(), false);
        assert_eq!(regs.get_for_paste().text, "default");
    }

    #[test]
    fn test_get_for_paste_selected() {
        let mut regs = RegisterMap::new();
        regs.yank("default".to_string(), false);
        regs.set('a', RegisterContent { text: "from_a".to_string(), linewise: false });
        regs.select('a');
        assert_eq!(regs.get_for_paste().text, "from_a");
    }

    #[test]
    fn test_get_for_paste_nonexistent_register() {
        let mut regs = RegisterMap::new();
        regs.yank("default".to_string(), false);
        regs.select('z');
        assert_eq!(regs.get_for_paste().text, "default");
    }

    // ==================== Search Register Tests ====================

    #[test]
    fn test_search_register() {
        let mut regs = RegisterMap::new();
        regs.set_search("pattern".to_string());
        assert_eq!(regs.get_search(), "pattern");
    }

    #[test]
    fn test_search_register_initially_empty() {
        let regs = RegisterMap::new();
        assert_eq!(regs.get_search(), "");
    }

    #[test]
    fn test_search_register_overwrite() {
        let mut regs = RegisterMap::new();
        regs.set_search("first".to_string());
        regs.set_search("second".to_string());
        assert_eq!(regs.get_search(), "second");
    }

    // ==================== Command Register Tests ====================

    #[test]
    fn test_command_register() {
        let mut regs = RegisterMap::new();
        regs.set_command("wq".to_string());
        assert_eq!(regs.get_command(), "wq");
    }

    #[test]
    fn test_command_register_initially_empty() {
        let regs = RegisterMap::new();
        assert_eq!(regs.get_command(), "");
    }

    #[test]
    fn test_command_register_overwrite() {
        let mut regs = RegisterMap::new();
        regs.set_command("w".to_string());
        regs.set_command("q!".to_string());
        assert_eq!(regs.get_command(), "q!");
    }

    // ==================== Clipboard Register Tests ====================

    #[test]
    fn test_is_clipboard_selected_none() {
        let regs = RegisterMap::new();
        assert!(!regs.is_clipboard_selected());
    }

    #[test]
    fn test_is_clipboard_selected_plus() {
        let mut regs = RegisterMap::new();
        regs.select('+');
        assert!(regs.is_clipboard_selected());
    }

    #[test]
    fn test_is_clipboard_selected_star() {
        let mut regs = RegisterMap::new();
        regs.select('*');
        assert!(regs.is_clipboard_selected());
    }

    #[test]
    fn test_is_clipboard_selected_named() {
        let mut regs = RegisterMap::new();
        regs.select('a');
        assert!(!regs.is_clipboard_selected());
    }

    // ==================== Special Register Get Tests ====================

    #[test]
    fn test_get_unnamed_register() {
        let mut regs = RegisterMap::new();
        regs.yank("test".to_string(), false);
        assert_eq!(regs.get('"').unwrap().text, "test");
    }

    #[test]
    fn test_get_small_delete_register() {
        let mut regs = RegisterMap::new();
        regs.delete("small".to_string(), false);
        assert_eq!(regs.get('-').unwrap().text, "small");
    }

    #[test]
    fn test_get_clipboard_registers() {
        let mut regs = RegisterMap::new();
        regs.yank("clipboard".to_string(), false);
        assert_eq!(regs.get('+').unwrap().text, "clipboard");
        assert_eq!(regs.get('*').unwrap().text, "clipboard");
    }

    #[test]
    fn test_get_invalid_register() {
        let regs = RegisterMap::new();
        assert!(regs.get('@').is_none());
        assert!(regs.get('!').is_none());
        assert!(regs.get(' ').is_none());
    }

    // ==================== Delete to Named Register Tests ====================

    #[test]
    fn test_delete_to_named_register() {
        let mut regs = RegisterMap::new();
        regs.select('a');
        regs.delete("deleted".to_string(), false);
        assert_eq!(regs.get('a').unwrap().text, "deleted");
    }

    #[test]
    fn test_delete_to_named_skips_numbered() {
        let mut regs = RegisterMap::new();
        regs.select('a');
        regs.delete("deleted\n".to_string(), true);
        assert_eq!(regs.get('1').unwrap().text, "");
    }

    // ==================== Linewise Flag Tests ====================

    #[test]
    fn test_linewise_preserved_on_yank() {
        let mut regs = RegisterMap::new();
        regs.yank("line\n".to_string(), true);
        assert!(regs.get('"').unwrap().linewise);
        assert!(regs.get('0').unwrap().linewise);
    }

    #[test]
    fn test_linewise_preserved_on_delete() {
        let mut regs = RegisterMap::new();
        regs.delete("line\n".to_string(), true);
        assert!(regs.get('1').unwrap().linewise);
    }

    #[test]
    fn test_characterwise_preserved() {
        let mut regs = RegisterMap::new();
        regs.yank("word".to_string(), false);
        assert!(!regs.get('"').unwrap().linewise);
    }
}

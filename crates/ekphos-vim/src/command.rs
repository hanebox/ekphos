//! Vim command mode (:w, :q, :%s)

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Write,
    Quit,
    WriteQuit,
    ForceQuit,
    GoToLine(usize),
    Substitute {
        pattern: String,
        replacement: String,
        flags: SubstituteFlags,
    },
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SubstituteFlags {
    pub global: bool,
    pub case_insensitive: bool,
    pub confirm: bool,
}

impl SubstituteFlags {
    pub fn parse(s: &str) -> Self {
        let mut flags = Self::default();
        for c in s.chars() {
            match c {
                'g' => flags.global = true,
                'i' | 'I' => flags.case_insensitive = true,
                'c' => flags.confirm = true,
                _ => {}
            }
        }
        flags
    }
}

pub fn parse_command(input: &str) -> Option<Command> {
    let input = input.trim();

    if input.is_empty() {
        return None;
    }

    if let Some(rest) = input.strip_prefix("%s") {
        return parse_substitute(rest);
    }
    if let Some(rest) = input.strip_prefix("s") {
        if rest.starts_with('/') {
            return parse_substitute(rest);
        }
    }

    match input {
        "w" | "w!" => return Some(Command::Write),
        "q" => return Some(Command::Quit),
        "wq" | "x" => return Some(Command::WriteQuit),
        "q!" => return Some(Command::ForceQuit),
        _ => {}
    }

    if let Ok(line) = input.parse::<usize>() {
        return Some(Command::GoToLine(line));
    }

    None
}

fn parse_substitute(input: &str) -> Option<Command> {
    if input.is_empty() {
        return None;
    }

    let delimiter = input.chars().next()?;
    // Slice past the delimiter by its UTF-8 byte length, not a hardcoded 1, so a
    // multi-byte delimiter char (e.g. `:%s世a世b`) doesn't slice on a non-char
    // boundary and panic.
    let rest = &input[delimiter.len_utf8()..];
    let parts: Vec<&str> = rest.split(delimiter).collect();

    if parts.len() < 2 {
        return None;
    }

    let pattern = parts[0].to_string();
    let replacement = parts[1].to_string();
    let flags = if parts.len() > 2 {
        SubstituteFlags::parse(parts[2])
    } else {
        SubstituteFlags::default()
    };

    Some(Command::Substitute { pattern, replacement, flags })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_commands() {
        assert_eq!(parse_command("w"), Some(Command::Write));
        assert_eq!(parse_command("w!"), Some(Command::Write));
        assert_eq!(parse_command("q"), Some(Command::Quit));
        assert_eq!(parse_command("wq"), Some(Command::WriteQuit));
        assert_eq!(parse_command("x"), Some(Command::WriteQuit));
        assert_eq!(parse_command("q!"), Some(Command::ForceQuit));
    }

    #[test]
    fn test_parse_line_number() {
        assert_eq!(parse_command("42"), Some(Command::GoToLine(42)));
        assert_eq!(parse_command("1"), Some(Command::GoToLine(1)));
        assert_eq!(parse_command("999"), Some(Command::GoToLine(999)));
    }

    #[test]
    fn test_parse_substitute_with_global() {
        let cmd = parse_command("%s/foo/bar/g");
        assert!(matches!(
            cmd,
            Some(Command::Substitute { pattern, replacement, flags })
            if pattern == "foo" && replacement == "bar" && flags.global
        ));
    }

    #[test]
    fn test_parse_substitute_no_flags() {
        let cmd = parse_command("%s/foo/bar");
        assert!(matches!(
            cmd,
            Some(Command::Substitute { pattern, replacement, flags })
            if pattern == "foo" && replacement == "bar" && !flags.global
        ));
    }

    #[test]
    fn test_parse_substitute_case_insensitive() {
        let cmd = parse_command("%s/foo/bar/gi");
        assert!(matches!(
            cmd,
            Some(Command::Substitute { flags, .. })
            if flags.global && flags.case_insensitive
        ));
    }

    #[test]
    fn test_parse_substitute_different_delimiter() {
        let cmd = parse_command("%s#foo#bar#g");
        assert!(matches!(
            cmd,
            Some(Command::Substitute { pattern, replacement, .. })
            if pattern == "foo" && replacement == "bar"
        ));
    }

    // Regression: a multi-byte delimiter used to slice the input at byte index 1,
    // landing inside the char and panicking. It must parse (or reject) safely.
    #[test]
    fn test_parse_substitute_multibyte_delimiter_no_panic() {
        let cmd = parse_command("%s世foo世bar世g");
        assert!(matches!(
            cmd,
            Some(Command::Substitute { pattern, replacement, .. })
            if pattern == "foo" && replacement == "bar"
        ));
        // A multi-byte delimiter with too few parts is rejected, still no panic.
        assert_eq!(parse_command("%s€only"), None);
    }

    #[test]
    fn test_parse_empty() {
        assert_eq!(parse_command(""), None);
        assert_eq!(parse_command("   "), None);
    }

    #[test]
    fn test_parse_invalid() {
        assert_eq!(parse_command("invalid"), None);
        assert_eq!(parse_command("xyz"), None);
    }

    #[test]
    fn test_substitute_flags_parse() {
        let flags = SubstituteFlags::parse("gic");
        assert!(flags.global);
        assert!(flags.case_insensitive);
        assert!(flags.confirm);
    }
}

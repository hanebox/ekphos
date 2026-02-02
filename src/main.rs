mod app;
mod clipboard;
mod config;
mod editor;
mod event;
mod graph;
mod highlight;
mod highlight_worker;
mod search;
mod ui;
mod vim;

use std::env;
use std::fs;
use std::io;
use std::path::PathBuf;

use crossterm::{
    cursor::SetCursorStyle,
    event::{DisableBracketedPaste, DisableFocusChange, DisableMouseCapture, EnableBracketedPaste, EnableFocusChange, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use app::App;
use event::run_app;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn check_for_updates() -> bool {
    use std::io::Write;
    use std::thread;
    use std::time::Duration;

    let config = config::Config::load();
    if !config.check_updates {
        return true;
    }
    let latest = match get_latest_version() {
        Some(v) => v,
        None => return true, 
    };
    if !is_newer_version(&latest, VERSION) {
        return true; 
    }

    let skipped = get_skipped_version();
    let already_skipped = skipped.as_ref() == Some(&latest);

    println!();
    println!("  A new version of ekphos is available: v{} (current: v{})", latest, VERSION);
    println!();
    println!("  To update:");
    println!("    Cargo:    cargo install ekphos");
    println!("    Homebrew: brew upgrade ekphos");
    println!("    AUR:      yay -S ekphos");
    println!();
    println!("  Changelog: https://github.com/hanebox/ekphos/releases");
    println!();

    if already_skipped {
        println!("  Please update. Launching in 1 second...");
        let _ = io::stdout().flush();
        thread::sleep(Duration::from_secs(1));
        return true;
    }

    print!("  Press Enter to continue, or 'q' to quit and update: ");
    let _ = io::stdout().flush();

    let mut input = String::new();
    if io::stdin().read_line(&mut input).is_err() {
        return true;
    }

    let input = input.trim().to_lowercase();
    if input == "q" || input == "quit" {
        println!();
        return false;
    }

    // User cose to continue, save this version as skipped
    save_skipped_version(&latest);
    true
}

fn skipped_version_path() -> PathBuf {
    config::Config::config_dir().join(".skipped_update")
}

fn get_skipped_version() -> Option<String> {
    fs::read_to_string(skipped_version_path()).ok()
}

fn save_skipped_version(version: &str) {
    let path = skipped_version_path();
    let _ = fs::write(path, version);
}
fn get_latest_version() -> Option<String> {
    let response = ureq::get("https://api.github.com/repos/hanebox/ekphos/releases/latest")
        .set("User-Agent", "ekphos")
        .timeout(std::time::Duration::from_secs(3))
        .call()
        .ok()?;

    let body = response.into_string().ok()?;
    let tag_start = body.find("\"tag_name\":")?;
    let after_tag = &body[tag_start + 11..];
    let quote_start = after_tag.find('"')? + 1;
    let quote_end = after_tag[quote_start..].find('"')?;
    let version = after_tag[quote_start..quote_start + quote_end].trim_start_matches('v');

    Some(version.to_string())
}

fn is_newer_version(remote: &str, local: &str) -> bool {
    let parse = |v: &str| -> (u32, u32, u32) {
        let parts: Vec<u32> = v.split('.').filter_map(|p| p.parse().ok()).collect();
        (
            parts.first().copied().unwrap_or(0),
            parts.get(1).copied().unwrap_or(0),
            parts.get(2).copied().unwrap_or(0),
        )
    };
    parse(remote) > parse(local)
}

fn print_help() {
    println!("ekphos {}", VERSION);
    println!("A lightweight, fast, terminal-based markdown research tool");
    println!();
    println!("USAGE:");
    println!("    ekphos [OPTIONS] [PATH]");
    println!();
    println!("ARGUMENTS:");
    println!("    [PATH]           Open a file or folder directly");
    println!("                     - If PATH is a folder, opens it as the notes directory");
    println!("                     - If PATH is a .md file, opens it and its parent folder");
    println!();
    println!("OPTIONS:");
    println!("    -h, --help       Print help information");
    println!("    -v, --version    Print version information");
    println!("    -c, --config     Print config file path");
    println!("    -d, --dir        Print notes directory path");
    println!("    --reset          Reset config and themes to defaults");
    println!("    --clean-cache    Clear the search index cache");
    println!();
    println!("EXAMPLES:");
    println!("    ekphos ~/notes           Open the ~/notes folder");
    println!("    ekphos ./my-note.md      Open a specific markdown file");
    println!("    ekphos .                 Open current directory as notes folder");
}

fn reset_config_and_themes() {
    let config_path = config::Config::config_path();
    let themes_dir = config::Config::themes_dir();

    println!("Resetting ekphos configuration...");
    println!();

    if config_path.exists() {
        match fs::remove_file(&config_path) {
            Ok(_) => println!("  Deleted: {}", config_path.display()),
            Err(e) => eprintln!("  Failed to remove config: {}", e),
        }
    } else {
        println!("  Config file not found (skipped)");
    }

    if themes_dir.exists() {
        match fs::remove_dir_all(&themes_dir) {
            Ok(_) => println!("  Deleted: {}", themes_dir.display()),
            Err(e) => eprintln!("  Failed to remove themes: {}", e),
        }
    } else {
        println!("  Themes directory not found (skipped)");
    }

    println!();
    println!("Generating fresh defaults...");
    println!();

    let _config = config::Config::load_or_create();

    println!("  Created: {}", config_path.display());
    println!("  Created: {}", themes_dir.join("ekphos-dawn.toml").display());

    println!();
    println!("Reset complete! Configuration restored to v{} defaults.", VERSION);
}

fn clean_cache() {
    let cache_dir = dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from(env::var("HOME").unwrap_or_default()).join(".cache"))
        .join("ekphos");

    println!("Cleaning ekphos search cache...");
    println!();

    if cache_dir.exists() {
        let total_size = get_dir_size(&cache_dir);
        match fs::remove_dir_all(&cache_dir) {
            Ok(_) => {
                let size_str = format_size(total_size);
                println!("  Deleted: {} ({})", cache_dir.display(), size_str);
            }
            Err(e) => eprintln!("  Failed to remove cache: {}", e),
        }
    } else {
        println!("  Cache directory not found (skipped)");
    }

    println!();
    println!("Cache cleared! Search index will be rebuilt on next launch.");
}

fn get_dir_size(path: &PathBuf) -> u64 {
    let mut total = 0;
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            let entry_path = entry.path();
            if entry_path.is_dir() {
                total += get_dir_size(&entry_path);
            } else if let Ok(metadata) = entry.metadata() {
                total += metadata.len();
            }
        }
    }
    total
}
fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;

    if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} bytes", bytes)
    }
}

fn resolve_path(path_str: &str) -> Option<PathBuf> {
    let expanded = shellexpand::tilde(path_str).to_string();
    let path = PathBuf::from(&expanded);
    let absolute = if path.is_absolute() {
        path
    } else {
        env::current_dir().ok()?.join(path)
    };

    absolute.canonicalize().ok().or(Some(absolute))
}

fn main() -> io::Result<()> {
    // Handle CLI arguments
    let args: Vec<String> = env::args().collect();
    let mut initial_path: Option<PathBuf> = None;

    if args.len() > 1 {
        match args[1].as_str() {
            "-v" | "--version" => {
                println!("ekphos {}", VERSION);
                return Ok(());
            }
            "-h" | "--help" => {
                print_help();
                return Ok(());
            }
            "-c" | "--config" => {
                println!("{}", config::Config::config_path().display());
                return Ok(());
            }
            "-d" | "--dir" => {
                let config = config::Config::load();
                println!("{}", config.notes_path().display());
                return Ok(());
            }
            "--reset" => {
                reset_config_and_themes();
                return Ok(());
            }
            "--clean-cache" => {
                clean_cache();
                return Ok(());
            }
            arg if arg.starts_with('-') => {
                eprintln!("Unknown option: {}", arg);
                eprintln!("Run 'ekphos --help' for usage information");
                return Ok(());
            }
            path_arg => {
                match resolve_path(path_arg) {
                    Some(path) => {
                        if !path.exists() {
                            eprintln!("Path does not exist: {}", path.display());
                            return Ok(());
                        }
                        initial_path = Some(path);
                    }
                    None => {
                        eprintln!("Invalid path: {}", path_arg);
                        return Ok(());
                    }
                }
            }
        }
    }

    if !check_for_updates() {
        return Ok(());
    }

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture, EnableBracketedPaste, EnableFocusChange, SetCursorStyle::SteadyBlock)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app state
    let mut app = App::new_with_path(initial_path);

    // Main loop
    let result = run_app(&mut terminal, &mut app);

    app.save_last_opened_note_to_cache();

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        SetCursorStyle::DefaultUserShape,
        LeaveAlternateScreen,
        DisableMouseCapture,
        DisableBracketedPaste,
        DisableFocusChange
    )?;
    terminal.show_cursor()?;

    if let Err(err) = result {
        eprintln!("Error: {err:?}");
    }

    Ok(())
}

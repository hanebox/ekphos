mod app;
mod editor;
mod event;
mod highlight;
mod config;
mod ui;

use std::env;
use std::io;

use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use app::App;
use event::run_app;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn print_help() {
    println!("ekphos {}", VERSION);
    println!("A lightweight, fast, terminal-based markdown research tool");
    println!();
    println!("USAGE:");
    println!("    ekphos [OPTIONS]");
    println!();
    println!("OPTIONS:");
    println!("    -h, --help       Print help information");
    println!("    -v, --version    Print version information");
    println!("    -c, --config     Print config file path");
    println!("    -d, --dir        Print notes directory path");
}

fn main() -> io::Result<()> {
    // Handle CLI arguments
    let args: Vec<String> = env::args().collect();
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
            _ => {
                eprintln!("Unknown option: {}", args[1]);
                eprintln!("Run 'ekphos --help' for usage information");
                return Ok(());
            }
        }
    }

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app state
    let mut app = App::new();

    // Main loop
    let result = run_app(&mut terminal, &mut app);

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = result {
        eprintln!("Error: {err:?}");
    }

    Ok(())
}

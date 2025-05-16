use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use miette::IntoDiagnostic;
use ratatui::prelude::*;
use std::io;

pub fn setup_terminal() -> miette::Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode().into_diagnostic()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture).into_diagnostic()?;

    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend).into_diagnostic()?;

    Ok(terminal)
}

pub fn restore_terminal() -> miette::Result<()> {
    // Restore terminal
    disable_raw_mode().into_diagnostic()?;
    let mut stdout = io::stdout();
    execute!(stdout, LeaveAlternateScreen, DisableMouseCapture).into_diagnostic()?;

    Ok(())
}

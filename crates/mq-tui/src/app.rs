use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use miette::IntoDiagnostic;
use mq_lang::Engine;
use mq_markdown::Markdown;
use ratatui::prelude::*;
use std::{
    io::Stdout,
    time::{Duration, Instant},
};

use std::str::FromStr;

use crate::{
    event::{EventHandler, EventHandlerExt},
    ui::draw_ui,
    util,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Query,
    Help,
}

pub struct App {
    /// The Markdown content to process
    content: String,
    /// The query to run on the Markdown content
    query: String,
    /// The current results from the query
    results: Vec<mq_lang::Value>,
    /// Currently selected result index
    selected_idx: usize,
    /// Last query execution time
    last_exec_time: Duration,
    /// Last query execution timestamp
    last_exec: Instant,
    /// Should the application exit
    should_quit: bool,
    /// Error message if the query fails
    error_msg: Option<String>,
    /// Delay before executing query after typing
    typing_delay: Duration,
    /// Current app mode
    mode: Mode,
    /// Show detailed view of selected item
    show_detail: bool,
    /// History of executed queries
    query_history: Vec<String>,
    /// Current position in query history
    history_position: Option<usize>,
    /// Current cursor position in query string
    cursor_position: usize,
    /// Filename (if loaded from a file)
    filename: Option<String>,
}

impl App {
    pub fn new(content: String) -> Self {
        Self {
            content,
            query: String::new(),
            results: Vec::new(),
            selected_idx: 0,
            last_exec_time: Duration::from_millis(0),
            last_exec: Instant::now(),
            should_quit: false,
            error_msg: None,
            typing_delay: Duration::from_millis(300),
            mode: Mode::Normal,
            show_detail: false,
            query_history: Vec::new(),
            history_position: None,
            cursor_position: 0,
            filename: None,
        }
    }

    pub fn with_file(content: String, filename: String) -> Self {
        let mut app = Self::new(content);
        app.filename = Some(filename);
        app
    }

    pub fn run(&mut self) -> miette::Result<()> {
        let mut terminal = util::setup_terminal()?;
        let events = EventHandler::new(Duration::from_millis(100));

        self.exec_query();

        while !self.should_quit {
            self.draw(&mut terminal)?;

            if let Some(event) = events.next()? {
                self.handle_event(event)?;
            }

            if self.mode == Mode::Query
                && !self.query.is_empty()
                && Instant::now().duration_since(self.last_exec) > self.typing_delay
            {
                self.exec_query();
            }
        }

        util::restore_terminal()?;

        Ok(())
    }

    fn draw(&self, terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> miette::Result<()> {
        terminal
            .draw(|frame| draw_ui(frame, self))
            .into_diagnostic()?;
        Ok(())
    }

    pub fn handle_event(&mut self, event: Event) -> miette::Result<()> {
        self.error_msg = None;
        match self.mode {
            Mode::Normal => self.handle_normal_mode_event(event),
            Mode::Query => self.handle_query_mode_event(event),
            Mode::Help => self.handle_help_mode_event(event),
        }
    }

    fn handle_normal_mode_event(&mut self, event: Event) -> miette::Result<()> {
        if let Event::Key(KeyEvent {
            code, modifiers, ..
        }) = event
        {
            match (code, modifiers) {
                // Quit on Escape or q
                (KeyCode::Char('q'), _) | (KeyCode::Esc, _) => {
                    self.should_quit = true;
                }
                // Toggle detailed view
                (KeyCode::Char('d'), _) => {
                    self.show_detail = !self.show_detail;
                }
                // Enter query mode
                (KeyCode::Char(':'), _) => {
                    self.mode = Mode::Query;
                    self.cursor_position = self.query.len();
                }
                // Show help
                (KeyCode::Char('?'), _) | (KeyCode::F(1), _) => {
                    self.mode = Mode::Help;
                }
                // Navigate results
                (KeyCode::Down, _) | (KeyCode::Char('j'), _) => {
                    if !self.results.is_empty() {
                        self.selected_idx = (self.selected_idx + 1) % self.results.len();
                    }
                }
                (KeyCode::Up, _) | (KeyCode::Char('k'), _) => {
                    if !self.results.is_empty() {
                        self.selected_idx = if self.selected_idx > 0 {
                            self.selected_idx - 1
                        } else {
                            self.results.len() - 1
                        };
                    }
                }
                (KeyCode::PageDown, _) => {
                    if !self.results.is_empty() {
                        self.selected_idx = (self.selected_idx + 10).min(self.results.len() - 1);
                    }
                }
                (KeyCode::PageUp, _) => {
                    if !self.results.is_empty() {
                        self.selected_idx = self.selected_idx.saturating_sub(10);
                    }
                }
                (KeyCode::Home, _) => {
                    if !self.results.is_empty() {
                        self.selected_idx = 0;
                    }
                }
                (KeyCode::End, _) => {
                    if !self.results.is_empty() {
                        self.selected_idx = self.results.len() - 1;
                    }
                }
                // Clear query with Ctrl+L
                (KeyCode::Char('l'), KeyModifiers::CONTROL) => {
                    self.query.clear();
                    self.cursor_position = 0;
                    self.exec_query();
                }

                _ => {}
            }
        }

        Ok(())
    }

    fn handle_query_mode_event(&mut self, event: Event) -> miette::Result<()> {
        if let Event::Key(KeyEvent {
            code, modifiers, ..
        }) = event
        {
            match (code, modifiers) {
                // Exit query mode on Escape
                (KeyCode::Esc, _) => {
                    self.mode = Mode::Normal;
                    self.history_position = None;
                }
                // Execute query on Enter
                (KeyCode::Enter, _) => {
                    self.mode = Mode::Normal;
                    if !self.query.is_empty() {
                        // Add query to history if it's not a duplicate
                        if self.query_history.is_empty()
                            || self.query_history.last() != Some(&self.query)
                        {
                            self.query_history.push(self.query.clone());
                        }
                    }
                    self.history_position = None;
                    self.exec_query();
                }
                // Edit query
                (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                    self.query.insert(self.cursor_position, c);
                    self.cursor_position += 1;
                    self.last_exec = Instant::now();
                }
                (KeyCode::Backspace, _) => {
                    if self.cursor_position > 0 {
                        self.query.remove(self.cursor_position - 1);
                        self.cursor_position -= 1;
                        self.last_exec = Instant::now();
                    }
                }
                (KeyCode::Delete, _) => {
                    if self.cursor_position < self.query.len() {
                        self.query.remove(self.cursor_position);
                        self.last_exec = Instant::now();
                    }
                }
                // Move cursor
                (KeyCode::Left, _) => {
                    if self.cursor_position > 0 {
                        self.cursor_position -= 1;
                    }
                }
                (KeyCode::Right, _) => {
                    if self.cursor_position < self.query.len() {
                        self.cursor_position += 1;
                    }
                }
                (KeyCode::Home, _) => {
                    self.cursor_position = 0;
                }
                (KeyCode::End, _) => {
                    self.cursor_position = self.query.len();
                }
                // Navigate history
                (KeyCode::Up, _) => {
                    if !self.query_history.is_empty() {
                        match self.history_position {
                            None => {
                                self.history_position = Some(self.query_history.len() - 1);
                                self.query =
                                    self.query_history[self.history_position.unwrap()].clone();
                            }
                            Some(pos) if pos > 0 => {
                                self.history_position = Some(pos - 1);
                                self.query =
                                    self.query_history[self.history_position.unwrap()].clone();
                            }
                            _ => {}
                        }
                        self.cursor_position = self.query.len();
                    }
                }
                (KeyCode::Down, _) => {
                    if let Some(pos) = self.history_position {
                        if pos < self.query_history.len() - 1 {
                            self.history_position = Some(pos + 1);
                            self.query = self.query_history[self.history_position.unwrap()].clone();
                        } else {
                            self.history_position = None;
                            self.query.clear();
                        }
                        self.cursor_position = self.query.len();
                    }
                }

                _ => {}
            }
        }

        Ok(())
    }

    fn handle_help_mode_event(&mut self, event: Event) -> miette::Result<()> {
        if let Event::Key(KeyEvent { .. }) = event {
            self.mode = Mode::Normal;
        }

        Ok(())
    }

    pub fn exec_query(&mut self) {
        let mut engine = Engine::default();
        engine.load_builtin_module();
        let start = Instant::now();
        let markdown_result = Markdown::from_str(&self.content);
        match markdown_result {
            Ok(markdown) => {
                let md_nodes = markdown
                    .nodes
                    .into_iter()
                    .map(mq_lang::Value::from)
                    .collect::<Vec<_>>();

                if !self.query.is_empty() {
                    match engine.eval(&self.query, md_nodes.into_iter()) {
                        Ok(results) => {
                            self.results = results.compact();
                            self.error_msg = None;
                        }
                        Err(err) => {
                            self.error_msg = Some(format!("Query error: {}", err));
                            // Keep previous results
                        }
                    }
                } else {
                    // Show all nodes when query is empty
                    self.results = md_nodes;
                    self.error_msg = None;
                }
            }
            Err(err) => {
                self.error_msg = Some(format!("Markdown parse error: {}", err));
                self.results = Vec::new();
            }
        }

        // Reset selected index if it's now out of bounds
        if self.selected_idx >= self.results.len() {
            self.selected_idx = if self.results.is_empty() {
                0
            } else {
                self.results.len() - 1
            };
        }

        self.last_exec_time = start.elapsed();
        self.last_exec = Instant::now();
    }

    /// Get the current query string
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Get the current results
    pub fn results(&self) -> &[mq_lang::Value] {
        &self.results
    }

    /// Get the currently selected result index
    pub fn selected_idx(&self) -> usize {
        self.selected_idx
    }

    /// Get the last execution time
    pub fn last_exec_time(&self) -> Duration {
        self.last_exec_time
    }

    /// Get the current error message, if any
    pub fn error_msg(&self) -> Option<&str> {
        self.error_msg.as_deref()
    }

    /// Get the current app mode
    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// Check if detailed view is enabled
    pub fn show_detail(&self) -> bool {
        self.show_detail
    }

    /// Get the cursor position in the query
    pub fn cursor_position(&self) -> usize {
        self.cursor_position
    }

    /// Get the filename, if any
    pub fn filename(&self) -> Option<&str> {
        self.filename.as_deref()
    }

    /// Get the query history
    pub fn query_history(&self) -> &[String] {
        &self.query_history
    }

    /// Set the query string (primarily for testing)
    #[cfg(test)]
    pub fn set_query(&mut self, query: String) {
        self.query = query;
        self.cursor_position = self.query.len();
    }

    // No need for a separate testing exec_query since the method already exists

    /// Set the mode (primarily for testing)
    #[cfg(test)]
    pub fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
    }
}

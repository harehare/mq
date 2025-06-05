use arboard::Clipboard;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use miette::IntoDiagnostic;
use mq_lang::Engine;
use mq_markdown::Markdown;
use ratatui::prelude::*;
use std::str::FromStr;
use std::{
    io::Stdout,
    time::{Duration, Instant},
};

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
    results: Vec<mq_markdown::Node>,
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
                (KeyCode::Char('y'), _) => {
                    if !self.results.is_empty() {
                        let selected_result = &self.results[self.selected_idx];
                        let result_text = format!("{}", selected_result); // Or a more detailed format if needed
                        if let Ok(mut clipboard) = Clipboard::new() {
                            if clipboard.set_text(result_text.clone()).is_ok() {
                                // Optionally, set a temporary message indicating success
                                // self.status_message = Some("Result copied to clipboard!".to_string());
                            } else {
                                self.error_msg = Some("Error: Could not copy to clipboard".to_string());
                            }
                        } else {
                            self.error_msg = Some("Error: Could not access clipboard".to_string());
                        }
                    }
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
                if !self.query.is_empty() {
                    let md_nodes = markdown
                        .nodes
                        .into_iter()
                        .map(mq_lang::Value::from)
                        .collect::<Vec<_>>();

                    match engine.eval(&self.query, md_nodes.into_iter()) {
                        Ok(results) => {
                            self.results = results
                                .into_iter()
                                .map(|runtime_value| match runtime_value {
                                    mq_lang::Value::Markdown(node) => node.clone(),
                                    _ => runtime_value.to_string().into(),
                                })
                                .collect();
                            self.error_msg = None;
                        }
                        Err(err) => {
                            self.error_msg = Some(format!("Query error: {}", err));
                            // Keep previous results
                        }
                    }
                } else {
                    // Show all nodes when query is empty
                    self.results = markdown.nodes;
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
    pub fn results(&self) -> &[mq_markdown::Node] {
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

    pub fn set_query(&mut self, query: String) {
        self.query = query;
        self.cursor_position = self.query.len();
    }

    pub fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
    }

    #[cfg(test)]
    pub fn set_results(&mut self, results: Vec<mq_markdown::Node>) {
        self.results = results;
    }

    #[cfg(test)]
    pub fn set_last_exec_time(&mut self, duration: Duration) {
        self.last_exec_time = duration;
    }

    #[cfg(test)]
    pub fn set_error_msg(&mut self, msg: String) {
        self.error_msg = Some(msg);
    }

    #[cfg(test)]
    pub fn set_cursor_position(&mut self, position: usize) {
        self.cursor_position = position;
    }
}
#[cfg(test)]
mod tests {
    use mq_markdown::Node;

    use super::*;

    fn create_test_app() -> App {
        App::new("# Test\nSome content".to_string())
    }

    fn create_test_app_with_file() -> App {
        App::with_file("# Test\nSome content".to_string(), "test.md".to_string())
    }

    #[test]
    fn test_app_creation() {
        let app = create_test_app();
        assert_eq!(app.query(), "");
        assert_eq!(app.selected_idx(), 0);
        assert_eq!(app.mode(), Mode::Normal);
        assert!(!app.show_detail());
        assert_eq!(app.cursor_position(), 0);
        assert!(app.filename().is_none());
        assert!(app.error_msg().is_none());
    }

    #[test]
    fn test_app_with_file() {
        let app = create_test_app_with_file();
        assert_eq!(app.filename(), Some("test.md"));
    }

    #[test]
    fn test_mode_switching() {
        let mut app = create_test_app();

        // Normal to Query mode
        app.set_mode(Mode::Query);
        assert_eq!(app.mode(), Mode::Query);

        // Query to Help mode
        app.set_mode(Mode::Help);
        assert_eq!(app.mode(), Mode::Help);

        // Help to Normal mode
        app.set_mode(Mode::Normal);
        assert_eq!(app.mode(), Mode::Normal);
    }

    #[test]
    fn test_query_setting() {
        let mut app = create_test_app();
        let test_query = "select(.type == 'heading')";

        app.set_query(test_query.to_string());
        assert_eq!(app.query(), test_query);
        assert_eq!(app.cursor_position(), test_query.len());
    }

    #[test]
    fn test_normal_mode_navigation() {
        let mut app = create_test_app();
        let test_results = vec![
            Node::from("result1"),
            Node::from("result2"),
            Node::from("result3"),
        ];
        app.set_results(test_results);

        // Test down navigation
        let down_event = Event::Key(KeyEvent {
            code: KeyCode::Down,
            modifiers: KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        });
        app.handle_event(down_event).unwrap();
        assert_eq!(app.selected_idx(), 1);

        // Test up navigation
        let up_event = Event::Key(KeyEvent {
            code: KeyCode::Up,
            modifiers: KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        });
        app.handle_event(up_event).unwrap();
        assert_eq!(app.selected_idx(), 0);
    }

    #[test]
    fn test_normal_mode_vim_navigation() {
        let mut app = create_test_app();
        let test_results = vec![Node::from("result1"), Node::from("result2")];
        app.set_results(test_results);

        // Test j (down)
        let j_event = Event::Key(KeyEvent {
            code: KeyCode::Char('j'),
            modifiers: KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        });
        app.handle_event(j_event).unwrap();
        assert_eq!(app.selected_idx(), 1);

        // Test k (up)
        let k_event = Event::Key(KeyEvent {
            code: KeyCode::Char('k'),
            modifiers: KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        });
        app.handle_event(k_event).unwrap();
        assert_eq!(app.selected_idx(), 0);
    }

    #[test]
    fn test_normal_mode_page_navigation() {
        let mut app = create_test_app();
        let test_results = (0..20)
            .map(|i| Node::from(format!("result{}", i)))
            .collect();
        app.set_results(test_results);

        // Test PageDown
        let page_down_event = Event::Key(KeyEvent {
            code: KeyCode::PageDown,
            modifiers: KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        });
        app.handle_event(page_down_event).unwrap();
        assert_eq!(app.selected_idx(), 10);

        // Test PageUp
        let page_up_event = Event::Key(KeyEvent {
            code: KeyCode::PageUp,
            modifiers: KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        });
        app.handle_event(page_up_event).unwrap();
        assert_eq!(app.selected_idx(), 0);
    }

    #[test]
    fn test_normal_mode_home_end_navigation() {
        let mut app = create_test_app();
        let test_results = vec![
            Node::from("result1"),
            Node::from("result2"),
            Node::from("result3"),
        ];
        app.set_results(test_results);
        app.selected_idx = 1;

        // Test End
        let end_event = Event::Key(KeyEvent {
            code: KeyCode::End,
            modifiers: KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        });
        app.handle_event(end_event).unwrap();
        assert_eq!(app.selected_idx(), 2);

        // Test Home
        let home_event = Event::Key(KeyEvent {
            code: KeyCode::Home,
            modifiers: KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        });
        app.handle_event(home_event).unwrap();
        assert_eq!(app.selected_idx(), 0);
    }

    #[test]
    fn test_normal_mode_toggle_detail() {
        let mut app = create_test_app();
        assert!(!app.show_detail());

        let detail_event = Event::Key(KeyEvent {
            code: KeyCode::Char('d'),
            modifiers: KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        });
        app.handle_event(detail_event.clone()).unwrap();
        assert!(app.show_detail());

        // Toggle again
        app.handle_event(detail_event).unwrap();
        assert!(!app.show_detail());
    }

    #[test]
    fn test_normal_mode_enter_query_mode() {
        let mut app = create_test_app();

        let colon_event = Event::Key(KeyEvent {
            code: KeyCode::Char(':'),
            modifiers: KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        });
        app.handle_event(colon_event).unwrap();
        assert_eq!(app.mode(), Mode::Query);
    }

    #[test]
    fn test_normal_mode_enter_help_mode() {
        let mut app = create_test_app();

        let help_event = Event::Key(KeyEvent {
            code: KeyCode::Char('?'),
            modifiers: KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        });
        app.handle_event(help_event).unwrap();
        assert_eq!(app.mode(), Mode::Help);

        // Test F1 as well
        app.set_mode(Mode::Normal);
        let f1_event = Event::Key(KeyEvent {
            code: KeyCode::F(1),
            modifiers: KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        });
        app.handle_event(f1_event).unwrap();
        assert_eq!(app.mode(), Mode::Help);
    }

    #[test]
    fn test_normal_mode_clear_query() {
        let mut app = create_test_app();
        app.set_query("test query".to_string());

        let clear_event = Event::Key(KeyEvent {
            code: KeyCode::Char('l'),
            modifiers: KeyModifiers::CONTROL,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        });
        app.handle_event(clear_event).unwrap();
        assert_eq!(app.query(), "");
        assert_eq!(app.cursor_position(), 0);
    }

    #[test]
    fn test_query_mode_text_input() {
        let mut app = create_test_app();
        app.set_mode(Mode::Query);

        let char_event = Event::Key(KeyEvent {
            code: KeyCode::Char('t'),
            modifiers: KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        });
        app.handle_event(char_event).unwrap();
        assert_eq!(app.query(), "t");
        assert_eq!(app.cursor_position(), 1);
    }

    #[test]
    fn test_query_mode_backspace() {
        let mut app = create_test_app();
        app.set_mode(Mode::Query);
        app.set_query("test".to_string());

        let backspace_event = Event::Key(KeyEvent {
            code: KeyCode::Backspace,
            modifiers: KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        });
        app.handle_event(backspace_event).unwrap();
        assert_eq!(app.query(), "tes");
        assert_eq!(app.cursor_position(), 3);
    }

    #[test]
    fn test_query_mode_delete() {
        let mut app = create_test_app();
        app.set_mode(Mode::Query);
        app.set_query("test".to_string());
        app.set_cursor_position(2);

        let delete_event = Event::Key(KeyEvent {
            code: KeyCode::Delete,
            modifiers: KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        });
        app.handle_event(delete_event).unwrap();
        assert_eq!(app.query(), "tet");
        assert_eq!(app.cursor_position(), 2);
    }

    #[test]
    fn test_query_mode_cursor_movement() {
        let mut app = create_test_app();
        app.set_mode(Mode::Query);
        app.set_query("test".to_string());

        // Test left arrow
        let left_event = Event::Key(KeyEvent {
            code: KeyCode::Left,
            modifiers: KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        });
        app.handle_event(left_event).unwrap();
        assert_eq!(app.cursor_position(), 3);

        // Test right arrow
        let right_event = Event::Key(KeyEvent {
            code: KeyCode::Right,
            modifiers: KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        });
        app.handle_event(right_event).unwrap();
        assert_eq!(app.cursor_position(), 4);
    }

    #[test]
    fn test_query_mode_home_end() {
        let mut app = create_test_app();
        app.set_mode(Mode::Query);
        app.set_query("test".to_string());
        app.set_cursor_position(2);

        // Test Home
        let home_event = Event::Key(KeyEvent {
            code: KeyCode::Home,
            modifiers: KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        });
        app.handle_event(home_event).unwrap();
        assert_eq!(app.cursor_position(), 0);

        // Test End
        let end_event = Event::Key(KeyEvent {
            code: KeyCode::End,
            modifiers: KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        });
        app.handle_event(end_event).unwrap();
        assert_eq!(app.cursor_position(), 4);
    }

    #[test]
    fn test_query_mode_exit_on_escape() {
        let mut app = create_test_app();
        app.set_mode(Mode::Query);

        let escape_event = Event::Key(KeyEvent {
            code: KeyCode::Esc,
            modifiers: KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        });
        app.handle_event(escape_event).unwrap();
        assert_eq!(app.mode(), Mode::Normal);
    }

    #[test]
    fn test_query_mode_execute_on_enter() {
        let mut app = create_test_app();
        app.set_mode(Mode::Query);
        app.set_query("test query".to_string());

        let enter_event = Event::Key(KeyEvent {
            code: KeyCode::Enter,
            modifiers: KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        });
        app.handle_event(enter_event).unwrap();
        assert_eq!(app.mode(), Mode::Normal);
        assert!(app.query_history().contains(&"test query".to_string()));
    }

    #[test]
    fn test_help_mode_exit_on_any_key() {
        let mut app = create_test_app();
        app.set_mode(Mode::Help);

        let any_key_event = Event::Key(KeyEvent {
            code: KeyCode::Char('x'),
            modifiers: KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        });
        app.handle_event(any_key_event).unwrap();
        assert_eq!(app.mode(), Mode::Normal);
    }

    #[test]
    fn test_quit_on_q_or_escape() {
        let mut app = create_test_app();

        let q_event = Event::Key(KeyEvent {
            code: KeyCode::Char('q'),
            modifiers: KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        });
        app.handle_event(q_event).unwrap();
        assert!(app.should_quit);

        // Reset and test escape
        app.should_quit = false;
        let escape_event = Event::Key(KeyEvent {
            code: KeyCode::Esc,
            modifiers: KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        });
        app.handle_event(escape_event).unwrap();
        assert!(app.should_quit);
    }

    #[test]
    fn test_error_message_cleared_on_event() {
        let mut app = create_test_app();
        app.set_error_msg("Test error".to_string());
        assert!(app.error_msg().is_some());

        let any_event = Event::Key(KeyEvent {
            code: KeyCode::Char('x'),
            modifiers: KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        });
        app.handle_event(any_event).unwrap();
        assert!(app.error_msg().is_none());
    }

    #[test]
    fn test_navigation_with_empty_results() {
        let mut app = create_test_app();
        app.set_results(vec![]);

        let down_event = Event::Key(KeyEvent {
            code: KeyCode::Down,
            modifiers: KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        });
        app.handle_event(down_event).unwrap();
        assert_eq!(app.selected_idx(), 0);
    }

    #[test]
    fn test_navigation_wraparound() {
        let mut app = create_test_app();
        let test_results = vec!["result1".into(), Node::from("result2")];
        app.set_results(test_results);
        app.selected_idx = 1;

        let down_event = Event::Key(KeyEvent {
            code: KeyCode::Down,
            modifiers: KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        });
        app.handle_event(down_event).unwrap();
        assert_eq!(app.selected_idx(), 0);

        let up_event = Event::Key(KeyEvent {
            code: KeyCode::Up,
            modifiers: KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        });
        app.handle_event(up_event).unwrap();
        assert_eq!(app.selected_idx(), 1);
    }

    #[test]
    fn test_execution_time_tracking() {
        let mut app = create_test_app();
        let test_duration = Duration::from_millis(100);
        app.set_last_exec_time(test_duration);
        assert_eq!(app.last_exec_time(), test_duration);
    }

    #[test]
    fn test_query_history_functionality() {
        let app = create_test_app();
        assert!(app.query_history().is_empty());
    }

    #[test]
    fn test_copy_result_to_clipboard() {
        let mut app = create_test_app();
        let test_results = vec![
            Node::from("result1"),
            Node::from("result2"),
        ];
        app.set_results(test_results.clone());
        app.selected_idx = 0; // Select the first result

        // Simulate 'y' key press
        let event = Event::Key(KeyEvent {
            code: KeyCode::Char('y'),
            modifiers: KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        });
        app.handle_normal_mode_event(event).unwrap();

        // Check clipboard content
        if let Ok(mut clipboard) = Clipboard::new() {
            match clipboard.get_text() {
                Ok(text) => assert_eq!(text, format!("{}", test_results[0])),
                Err(_) => {
                    // Depending on the environment (e.g. CI without a clipboard manager),
                    // this part might fail. We can choose to ignore this error for CI.
                    // For now, let's print a warning.
                    // If running in CI and this fails, consider cfg attributes to exclude test.
                    eprintln!("Warning: Could not get text from clipboard during test. This might be normal in CI environments.");
                    // Or, if clipboard is absolutely essential for the test to pass:
                    // panic!("Could not get text from clipboard during test");
                }
            }
        } else {
            // Handle case where clipboard could not be initialized
            eprintln!("Warning: Could not initialize clipboard during test. This might be normal in CI environments.");
            // Or, if clipboard is absolutely essential:
            // panic!("Could not initialize clipboard during test");
        }

        // Check that no error message was set in the app
        assert!(app.error_msg().is_none());

        // Test with no results
        app.set_results(vec![]);
        app.selected_idx = 0;
        let event = Event::Key(KeyEvent {
            code: KeyCode::Char('y'),
            modifiers: KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        });
        app.handle_normal_mode_event(event).unwrap();
        // Should not panic and error_msg should be None as the handler doesn't set error for empty results.
        assert!(app.error_msg().is_none());
    }
}

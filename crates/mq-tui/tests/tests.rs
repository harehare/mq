use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use mq_tui::{App, Mode};

fn create_test_app() -> App {
    let content = r#"# Test Heading

This is a paragraph.

## Second Heading

- List item 1
- List item 2
"#;
    App::with_file(content.to_string(), "test.md".to_string())
}

#[test]
fn test_app_creation() {
    let app = create_test_app();
    assert_eq!(app.query(), "");
    assert_eq!(app.mode(), Mode::Normal);
    assert!(!app.show_detail());
    assert_eq!(app.selected_idx(), 0);
    assert_eq!(app.filename().unwrap(), "test.md");
}

#[test]
fn test_query_execution() {
    let mut app = create_test_app();

    app.set_query(".h".to_string());
    app.exec_query();

    assert_eq!(app.results().len(), 5);
}

#[test]
fn test_mode_switching() {
    let mut app = create_test_app();
    assert_eq!(app.mode(), Mode::Normal);

    app.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Char(':'),
        KeyModifiers::NONE,
    )))
    .unwrap();
    assert_eq!(app.mode(), Mode::Query);

    // Simulate exit query mode with Esc
    app.handle_event(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)))
        .unwrap();
    assert_eq!(app.mode(), Mode::Normal);

    // Simulate help mode
    app.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Char('?'),
        KeyModifiers::NONE,
    )))
    .unwrap();
    assert_eq!(app.mode(), Mode::Help);

    // Any key exits help mode
    app.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Enter,
        KeyModifiers::NONE,
    )))
    .unwrap();
    assert_eq!(app.mode(), Mode::Normal);
}

#[test]
fn test_detail_view_toggle() {
    let mut app = create_test_app();
    assert!(!app.show_detail());

    // Toggle detail view on
    app.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Char('d'),
        KeyModifiers::NONE,
    )))
    .unwrap();
    assert!(app.show_detail());

    // Toggle detail view off
    app.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Char('d'),
        KeyModifiers::NONE,
    )))
    .unwrap();
    assert!(!app.show_detail());
}

#[test]
fn test_navigation() {
    let mut app = create_test_app();
    app.exec_query();
    assert_eq!(app.selected_idx(), 0);

    // Navigate down
    app.handle_event(Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)))
        .unwrap();
    assert_eq!(app.selected_idx(), 1);

    // Navigate up
    app.handle_event(Event::Key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)))
        .unwrap();
    assert_eq!(app.selected_idx(), 0);
}

#[test]
fn test_query_editing() {
    let mut app = create_test_app();

    // Enter query mode
    app.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Char(':'),
        KeyModifiers::NONE,
    )))
    .unwrap();

    // Type a query
    app.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Char('t'),
        KeyModifiers::NONE,
    )))
    .unwrap();
    app.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Char('e'),
        KeyModifiers::NONE,
    )))
    .unwrap();
    app.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Char('s'),
        KeyModifiers::NONE,
    )))
    .unwrap();
    app.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Char('t'),
        KeyModifiers::NONE,
    )))
    .unwrap();

    assert_eq!(app.query(), "test");

    // Backspace
    app.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Backspace,
        KeyModifiers::NONE,
    )))
    .unwrap();
    assert_eq!(app.query(), "tes");
}

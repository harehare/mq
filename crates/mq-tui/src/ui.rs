use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Position, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Clear, List, ListItem, ListState, Padding, Paragraph, Wrap,
    },
};

use crate::app::{App, Mode};

pub fn draw_ui(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Query input
            Constraint::Min(0),    // Results area
            Constraint::Length(1), // Status line
        ])
        .split(frame.area());

    if app.mode() == Mode::Query {
        draw_query_input(frame, app, chunks[0]);
    } else {
        draw_title_bar(frame, app, chunks[0]);
    }

    if app.show_detail() && !app.results().is_empty() {
        let detail_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(40), // Results list
                Constraint::Percentage(60), // Detail view
            ])
            .split(chunks[1]);

        draw_results_list(frame, app, detail_chunks[0]);
        draw_detail_view(frame, app, detail_chunks[1]);
    } else {
        draw_results_list(frame, app, chunks[1]);
    }

    draw_status_line(frame, app, chunks[2]);

    if let Some(error) = app.error_msg() {
        draw_error_popup(frame, error);
    }

    if app.mode() == Mode::Help {
        draw_help_screen(frame);
    }
}

fn draw_query_input(frame: &mut Frame, app: &App, area: Rect) {
    let query_block = Block::default()
        .title("Query")
        .borders(Borders::ALL)
        .style(Style::default());

    let query_text = Paragraph::new(app.query())
        .style(Style::default().fg(Color::Yellow))
        .block(query_block);

    frame.render_widget(query_text, area);

    let cursor_x = app.cursor_position() as u16 + 1; // +1 for block border
    frame.set_cursor_position(Position::new(
        area.x + cursor_x,
        area.y + 1, // +1 for block border
    ));
}

fn draw_results_list(frame: &mut Frame, app: &App, area: Rect) {
    let results = app.results();

    let results_block = Block::default().title("Results").borders(Borders::ALL);

    if results.is_empty() {
        let text = if app.query().is_empty() {
            "Enter a query to filter results"
        } else {
            "No results found"
        };

        let empty_text = Paragraph::new(text)
            .style(Style::default().fg(Color::DarkGray))
            .block(results_block);

        frame.render_widget(empty_text, area);
        return;
    }

    let items: Vec<ListItem> = mq_markdown::Markdown::new(results.to_vec())
        .to_string()
        .lines()
        .enumerate()
        .map(|(i, value)| {
            let content = Line::from(value.to_string());

            ListItem::new(content).style(if i == app.selected_idx() {
                Style::default().fg(Color::Black).bg(Color::White)
            } else {
                Style::default()
            })
        })
        .collect();

    let list = List::new(items)
        .block(results_block)
        .highlight_style(Style::default().add_modifier(Modifier::BOLD));

    let mut state = ListState::default();
    state.select(Some(app.selected_idx()));

    frame.render_stateful_widget(list, area, &mut state);
}

/// Draw the status line at the bottom
fn draw_status_line(frame: &mut Frame, app: &App, area: Rect) {
    let exec_time = app.last_exec_time();
    let results_count = app.results().len();

    let status = format!(
        "{} results | Execution time: {:.2}ms | Press q to quit",
        results_count,
        exec_time.as_secs_f64() * 1000.0
    );

    let status_text = Paragraph::new(status).style(Style::default().fg(Color::DarkGray));

    frame.render_widget(status_text, area);
}

fn draw_title_bar(frame: &mut Frame, app: &App, area: Rect) {
    let title = match app.filename() {
        Some(filename) => format!("mq-tui - {}", filename),
        None => "mq-tui".to_string(),
    };

    let mode_indicator = match app.mode() {
        Mode::Normal => "NORMAL",
        Mode::Query => "QUERY",
        Mode::Help => "HELP",
    };

    let title_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded);

    let title_spans = vec![
        Span::styled(title, Style::default().fg(Color::Green).bold()),
        Span::raw(" | "),
        Span::styled(
            mode_indicator,
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" | "),
        Span::styled("Press '?' for help", Style::default().fg(Color::Gray)),
    ];

    let title_text = Paragraph::new(Line::from(title_spans))
        .block(title_block)
        .alignment(Alignment::Center);

    frame.render_widget(title_text, area);
}

fn draw_detail_view(frame: &mut Frame, app: &App, area: Rect) {
    let results = app.results();
    if results.is_empty() || app.selected_idx() >= results.len() {
        return;
    }

    let selected_item = &results[app.selected_idx()];
    let detail_block = Block::default()
        .title("Detail View")
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .padding(Padding::new(1, 1, 1, 1));

    let detailed_content = format!("{:#?}", selected_item);

    let detail_text = Paragraph::new(detailed_content)
        .style(Style::default())
        .block(detail_block)
        .wrap(Wrap { trim: false });

    frame.render_widget(detail_text, area);
}

fn draw_help_screen(frame: &mut Frame) {
    let area = frame.area();

    let width = area.width.clamp(20, 60);
    let height = area.height.clamp(10, 25);
    let x = (area.width.saturating_sub(width)) / 2;
    let y = (area.height.saturating_sub(height)) / 2;

    let help_area = Rect::new(x, y, width, height);

    frame.render_widget(Clear, help_area);

    let help_block = Block::default()
        .title("Keyboard Controls")
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .style(Style::default().bg(Color::Black));

    let help_text = vec![
        Line::from(vec![Span::styled(
            "Navigation",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::UNDERLINED),
        )]),
        Line::from(""),
        Line::from(vec![
            Span::styled("↑/k", Style::default().fg(Color::Yellow)),
            Span::raw(" - Move up"),
        ]),
        Line::from(vec![
            Span::styled("↓/j", Style::default().fg(Color::Yellow)),
            Span::raw(" - Move down"),
        ]),
        Line::from(vec![
            Span::styled("PgUp", Style::default().fg(Color::Yellow)),
            Span::raw(" - Page up"),
        ]),
        Line::from(vec![
            Span::styled("PgDn", Style::default().fg(Color::Yellow)),
            Span::raw(" - Page down"),
        ]),
        Line::from(vec![
            Span::styled("Home", Style::default().fg(Color::Yellow)),
            Span::raw(" - Go to first item"),
        ]),
        Line::from(vec![
            Span::styled("End", Style::default().fg(Color::Yellow)),
            Span::raw(" - Go to last item"),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Query Mode",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::UNDERLINED),
        )]),
        Line::from(""),
        Line::from(vec![
            Span::styled(":", Style::default().fg(Color::Yellow)),
            Span::raw(" - Enter query mode"),
        ]),
        Line::from(vec![
            Span::styled("Enter", Style::default().fg(Color::Yellow)),
            Span::raw(" - Execute query"),
        ]),
        Line::from(vec![
            Span::styled("Esc", Style::default().fg(Color::Yellow)),
            Span::raw(" - Exit query mode"),
        ]),
        Line::from(vec![
            Span::styled("↑/↓", Style::default().fg(Color::Yellow)),
            Span::raw(" - Navigate query history"),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Other Commands",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::UNDERLINED),
        )]),
        Line::from(""),
        Line::from(vec![
            Span::styled("d", Style::default().fg(Color::Yellow)),
            Span::raw(" - Toggle detail view"),
        ]),
        Line::from(vec![
            Span::styled("y", Style::default().fg(Color::Yellow)),
            Span::raw(" - Copy selected result to clipboard"),
        ]),
        Line::from(vec![
            Span::styled("q/Esc", Style::default().fg(Color::Yellow)),
            Span::raw(" - Quit application"),
        ]),
        Line::from(vec![
            Span::styled("?", Style::default().fg(Color::Yellow)),
            Span::raw(" - Show this help"),
        ]),
        Line::from(vec![
            Span::styled("Ctrl+l", Style::default().fg(Color::Yellow)),
            Span::raw(" - Clear query"),
        ]),
    ];

    let help_paragraph = Paragraph::new(help_text)
        .block(help_block)
        .style(Style::default())
        .alignment(Alignment::Left);

    frame.render_widget(help_paragraph, help_area);
}

fn draw_error_popup(frame: &mut Frame, error: &str) {
    let frame_size = frame.area();

    let width = frame_size.width.clamp(20, 60);
    let height = 3;

    let x = (frame_size.width.saturating_sub(width)) / 2;
    let y = (frame_size.height.saturating_sub(height)) / 2;

    let popup_area = Rect::new(x, y, width, height);

    frame.render_widget(Clear, popup_area);

    let error_block = Block::default()
        .title("Error")
        .borders(Borders::ALL)
        .style(Style::default().bg(Color::Red).fg(Color::White));

    let error_text = Paragraph::new(error)
        .wrap(Wrap { trim: true })
        .style(Style::default().bg(Color::Red).fg(Color::White))
        .block(error_block);

    frame.render_widget(error_text, popup_area);
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use itertools::Itertools;
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;

    fn create_test_app() -> App {
        let mut app = App::new("".to_string());
        app.set_query("test query".to_string());
        app
    }

    fn create_app_with_results() -> App {
        let mut app = App::new("test.md".to_string());
        let results = vec![
            mq_markdown::Node::Heading(mq_markdown::Heading {
                depth: 1,
                position: None,
                values: vec![mq_markdown::Node::Text(mq_markdown::Text {
                    value: "Test Heading".to_string(),
                    position: None,
                })],
            }),
            mq_markdown::Node::Text(mq_markdown::Text {
                value: "Test paragraph content".to_string(),
                position: None,
            }),
            mq_markdown::Node::Code(mq_markdown::Code {
                meta: None,
                fence: true,
                lang: Some("rust".to_string()),
                value: "fn main() {}".to_string(),
                position: None,
            }),
        ];
        app.set_results(results);
        app.set_last_exec_time(Duration::from_millis(150));
        app
    }

    #[test]
    fn test_draw_ui_normal_mode() {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        let app = create_test_app();

        terminal
            .draw(|frame| {
                draw_ui(frame, &app);
            })
            .unwrap();

        let backend = terminal.backend();
        let buffer = backend.buffer();

        assert!(
            buffer
                .content()
                .iter()
                .map(|c| c.symbol())
                .join("")
                .contains("mq-tui")
        );
    }

    #[test]
    fn test_draw_ui_query_mode() {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        let mut app = create_test_app();
        app.set_mode(Mode::Query);

        terminal
            .draw(|frame| {
                draw_ui(frame, &app);
            })
            .unwrap();

        let backend = terminal.backend();
        let buffer = backend.buffer();

        assert!(
            buffer
                .content()
                .iter()
                .map(|c| c.symbol())
                .join("")
                .contains("Query")
        );
    }

    #[test]
    fn test_draw_ui_help_mode() {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        let mut app = create_test_app();
        app.set_mode(Mode::Help);

        terminal
            .draw(|frame| {
                draw_ui(frame, &app);
            })
            .unwrap();

        let backend = terminal.backend();
        let buffer = backend.buffer();

        assert!(
            buffer
                .content()
                .iter()
                .map(|c| c.symbol())
                .join("")
                .contains("Keyboard Controls")
        );
    }

    #[test]
    fn test_draw_ui_with_error() {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        let mut app = create_test_app();
        app.set_error_msg("Test error message".to_string());

        terminal
            .draw(|frame| {
                draw_ui(frame, &app);
            })
            .unwrap();

        let backend = terminal.backend();
        let buffer = backend.buffer();

        assert!(
            buffer
                .content()
                .iter()
                .map(|c| c.symbol())
                .join("")
                .contains("Error")
        );
    }

    #[test]
    fn test_draw_results_list_with_data() {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        let app = create_app_with_results();

        terminal
            .draw(|frame| {
                let area = frame.area();
                draw_results_list(frame, &app, area);
            })
            .unwrap();

        let backend = terminal.backend();
        let buffer = backend.buffer();

        assert!(
            buffer
                .content()
                .iter()
                .map(|c| c.symbol())
                .join("")
                .contains("Results")
        );
    }

    #[test]
    fn test_draw_title_bar_without_filename() {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        let app = create_test_app();

        terminal
            .draw(|frame| {
                let area = frame.area();
                draw_title_bar(frame, &app, area);
            })
            .unwrap();

        let backend = terminal.backend();
        let buffer = backend.buffer();

        assert!(
            buffer
                .content()
                .iter()
                .map(|c| c.symbol())
                .join("")
                .contains("mq-tui")
        );
    }

    #[test]
    fn test_draw_detail_view_empty_results() {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        let app = create_test_app();

        terminal
            .draw(|frame| {
                let area = frame.area();
                draw_detail_view(frame, &app, area);
            })
            .unwrap();
    }

    #[test]
    fn test_draw_detail_view_with_selection() {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        let app = create_app_with_results();

        terminal
            .draw(|frame| {
                let area = frame.area();
                draw_detail_view(frame, &app, area);
            })
            .unwrap();

        let backend = terminal.backend();
        let buffer = backend.buffer();

        assert!(
            buffer
                .content()
                .iter()
                .map(|c| c.symbol())
                .join("")
                .contains("Detail View")
        );
    }

    #[test]
    fn test_draw_help_screen_content() {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();

        terminal
            .draw(|frame| {
                draw_help_screen(frame);
            })
            .unwrap();

        let backend = terminal.backend();
        let buffer = backend.buffer();

        assert!(
            buffer
                .content()
                .iter()
                .map(|c| c.symbol())
                .join("")
                .contains("Navigation")
        );

        assert!(
            buffer
                .content()
                .iter()
                .map(|c| c.symbol())
                .join("")
                .contains("Query Mode")
        );

        assert!(
            buffer
                .content()
                .iter()
                .map(|c| c.symbol())
                .join("")
                .contains("Other Commands")
        );
    }

    #[test]
    fn test_draw_error_popup_content() {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        let error_msg = "Test error message";

        terminal
            .draw(|frame| {
                draw_error_popup(frame, error_msg);
            })
            .unwrap();

        let backend = terminal.backend();
        let buffer = backend.buffer();
        let content = buffer.content().iter().map(|c| c.symbol()).join("");

        assert!(content.contains("Error"));
        assert!(content.contains(error_msg));
    }

    #[test]
    fn test_draw_query_input_cursor_position() {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        let mut app = create_test_app();
        app.set_mode(Mode::Query);
        app.set_cursor_position(5);

        terminal
            .draw(|frame| {
                let area = frame.area();
                draw_query_input(frame, &app, area);
            })
            .unwrap();
    }

    #[test]
    fn test_ui_layout_constraints() {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        let app = create_test_app();

        terminal
            .draw(|frame| {
                draw_ui(frame, &app);
            })
            .unwrap();

        let mut small_terminal = Terminal::new(TestBackend::new(20, 10)).unwrap();
        small_terminal
            .draw(|frame| {
                draw_ui(frame, &app);
            })
            .unwrap();
    }
}

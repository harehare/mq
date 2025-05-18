use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
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
        .split(frame.size());

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
    frame.set_cursor(
        area.x + cursor_x,
        area.y + 1, // +1 for block border
    );
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
    let area = frame.size();

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
    let frame_size = frame.size();

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

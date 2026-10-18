use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::app::{App, ChatEntry, Form, FormKind, Screen, HOME_ITEMS};

pub fn draw(frame: &mut Frame, app: &App) {
    match app.screen {
        Screen::Home => draw_home(frame, app),
        Screen::Form => {
            draw_home(frame, app);

            if let Some(form) = &app.form {
                draw_form(frame, form);
            }
        }
        Screen::Chat => draw_chat(frame, app),
    }
}

fn draw_home(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area);

    let title = Paragraph::new("terminal-chat-rs")
        .alignment(Alignment::Center)
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .block(Block::default().borders(Borders::BOTTOM));
    frame.render_widget(title, layout[0]);

    let menu_area = centered_rect(30, HOME_ITEMS.len() as u16 + 2, layout[1]);
    let items: Vec<ListItem> = HOME_ITEMS.iter().map(|item| ListItem::new(*item)).collect();
    let menu = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Menu"))
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ");

    let mut state = ListState::default();
    state.select(Some(app.selected));
    frame.render_stateful_widget(menu, menu_area, &mut state);

    let help = Paragraph::new("[Up/Down] select   [Enter] confirm   [q] quit")
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(help, layout[2]);
}

fn draw_form(frame: &mut Frame, form: &Form) {
    let area = frame.area();
    let height = (form.fields.len() as u16 * 3 + 4).min(area.height);
    let popup = centered_rect(52, height, area);
    frame.render_widget(Clear, popup);

    let title = match form.kind {
        FormKind::Host => " Host a server ",
        FormKind::Join => " Join a server ",
    };
    let block = Block::default().borders(Borders::ALL).title(title);
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let mut constraints: Vec<Constraint> =
        form.fields.iter().map(|_| Constraint::Length(3)).collect();
    constraints.push(Constraint::Min(0));
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    for (index, field) in form.fields.iter().enumerate() {
        let focused = index == form.focus;
        let border_style = if focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let input_width = chunks[index].width.saturating_sub(2) as usize;
        let cursor = field.input.cursor();
        let offset = cursor.saturating_sub(input_width.saturating_sub(1));

        let field_block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(field.label);
        let paragraph = Paragraph::new(field.input.value())
            .block(field_block)
            .scroll((0, offset as u16));
        frame.render_widget(paragraph, chunks[index]);

        if focused {
            frame.set_cursor_position((
                chunks[index].x + 1 + (cursor - offset) as u16,
                chunks[index].y + 1,
            ));
        }
    }

    let footer = chunks[form.fields.len()];
    let mut lines: Vec<Line> = Vec::new();

    if let Some(error) = &form.error {
        lines.push(Line::from(Span::styled(
            error.clone(),
            Style::default().fg(Color::Red),
        )));
    }

    lines.push(Line::from(Span::styled(
        "[Tab] next   [Enter] submit   [Esc] cancel",
        Style::default().fg(Color::DarkGray),
    )));
    frame.render_widget(Paragraph::new(lines), footer);
}

fn draw_chat(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(area);

    let messages_area = layout[0];
    let inner_width = messages_area.width.saturating_sub(2) as usize;
    let inner_height = messages_area.height.saturating_sub(2) as usize;

    let lines = build_lines(app, inner_width);
    let max_offset = lines.len().saturating_sub(inner_height);
    let offset = max_offset.saturating_sub(app.scroll_back.min(max_offset));

    let messages = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" Room ({} users) ", app.users.len().max(1))),
        )
        .scroll((offset as u16, 0));
    frame.render_widget(messages, messages_area);

    let input_area = layout[1];
    let input_width = input_area.width.saturating_sub(2) as usize;
    let cursor = app.input.cursor();
    let offset = cursor.saturating_sub(input_width.saturating_sub(1));

    let input = Paragraph::new(app.input.value())
        .block(Block::default().borders(Borders::ALL).title("Message"))
        .scroll((0, offset as u16));
    frame.render_widget(input, input_area);

    if app.connected {
        frame.set_cursor_position((
            input_area.x + 1 + (cursor - offset) as u16,
            input_area.y + 1,
        ));
    }

    draw_status(frame, app, layout[2]);
}

fn draw_status(frame: &mut Frame, app: &App, area: Rect) {
    let (color, state) = if app.connected {
        (Color::Green, "connected")
    } else {
        (Color::Red, "disconnected")
    };

    let label = if app.connection_label.is_empty() {
        "not connected".to_string()
    } else {
        app.connection_label.clone()
    };

    let mut spans = vec![
        Span::styled(
            format!(" {} ", state),
            Style::default().fg(Color::Black).bg(color),
        ),
        Span::raw(" "),
        Span::styled(label, Style::default().fg(Color::White)),
    ];

    if !app.nickname.is_empty() {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            format!("[{}]", app.nickname),
            Style::default().fg(Color::Cyan),
        ));
    }

    if app.screen == Screen::Chat {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            "Enter send   PgUp/PgDn scroll   Esc leave   Ctrl-C quit",
            Style::default().fg(Color::DarkGray),
        ));
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn build_lines(app: &App, width: usize) -> Vec<Line<'static>> {
    let width = width.max(1);
    let mut lines = Vec::new();

    for entry in &app.entries {
        let (text, style) = match entry {
            ChatEntry::Message {
                nickname,
                content,
                timestamp,
            } => (
                format!("[{}] {}: {}", timestamp, nickname, content),
                Style::default().fg(Color::White),
            ),
            ChatEntry::System(message) => (
                format!("* {}", message),
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::ITALIC),
            ),
            ChatEntry::Error(message) => {
                (format!("! {}", message), Style::default().fg(Color::Red))
            }
        };

        for wrapped in wrap_line(&text, width) {
            lines.push(Line::from(wrapped).style(style));
        }
    }

    lines
}

/// Word-wrap `text` into lines no wider than `width` characters.
fn wrap_line(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![String::new()];
    }

    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();

    for word in text.split_whitespace() {
        push_word(&mut lines, &mut current, word, width);
    }

    if !current.is_empty() || lines.is_empty() {
        lines.push(current);
    }

    lines
}

fn push_word(lines: &mut Vec<String>, current: &mut String, word: &str, width: usize) {
    if current.is_empty() {
        hard_split(lines, current, word, width);
    } else if current.chars().count() + 1 + word.chars().count() <= width {
        current.push(' ');
        current.push_str(word);
    } else {
        lines.push(std::mem::take(current));
        hard_split(lines, current, word, width);
    }
}

fn hard_split(lines: &mut Vec<String>, current: &mut String, word: &str, width: usize) {
    let mut remaining = word;

    while remaining.chars().count() > width {
        let chunk: String = remaining.chars().take(width).collect();
        let split = chunk.len();
        lines.push(chunk);
        remaining = &remaining[split..];
    }

    current.push_str(remaining);
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_on_word_boundaries() {
        let lines = wrap_line("the quick brown fox", 9);
        assert_eq!(lines, vec!["the quick", "brown fox"]);
    }

    #[test]
    fn hard_splits_long_words() {
        let lines = wrap_line("abcdefghij", 4);
        assert_eq!(lines, vec!["abcd", "efgh", "ij"]);
    }

    #[test]
    fn keeps_empty_input_on_one_line() {
        assert_eq!(wrap_line("", 10), vec!["".to_string()]);
    }

    #[test]
    fn never_exceeds_width() {
        for line in wrap_line("a bb ccc dddd eeeee ffffff", 5) {
            assert!(line.chars().count() <= 5, "line '{line}' was too wide");
        }
    }
}

#[cfg(test)]
mod render_tests {
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use tokio::sync::mpsc;

    use super::*;
    use crate::input::InputState;

    fn render(app: &App, width: u16, height: u16) {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| draw(frame, app)).unwrap();
    }

    fn app() -> App {
        let (events, _rx) = mpsc::unbounded_channel();
        App::new(events)
    }

    #[test]
    fn renders_home() {
        render(&app(), 80, 24);
        render(&app(), 20, 5);
    }

    #[test]
    fn renders_form() {
        let mut app = app();
        app.screen = Screen::Form;
        app.form = Some(Form {
            kind: FormKind::Join,
            fields: vec![
                crate::app::Field {
                    label: "Address",
                    input: InputState::new("127.0.0.1:8080"),
                },
                crate::app::Field {
                    label: "Nickname",
                    input: InputState::new("alice"),
                },
            ],
            focus: 1,
            error: Some("something went wrong".to_string()),
        });

        render(&app, 80, 24);
        render(&app, 20, 5);
    }

    #[test]
    fn renders_chat() {
        let mut app = app();
        app.screen = Screen::Chat;
        app.connected = true;
        app.nickname = "alice".to_string();
        app.connection_label = "Connected to 127.0.0.1:8080".to_string();
        app.users = vec!["alice".to_string(), "bob".to_string()];
        app.entries
            .push(ChatEntry::System("You joined as alice".to_string()));
        app.entries.push(ChatEntry::Message {
            nickname: "bob".to_string(),
            content: "hello there, this is a fairly long message to force wrapping".to_string(),
            timestamp: "12:00:00".to_string(),
        });
        app.entries.push(ChatEntry::Error("boom".to_string()));

        render(&app, 80, 24);
        render(&app, 20, 5);
    }
}

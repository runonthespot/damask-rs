use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::theme;

pub fn render(f: &mut Frame, area: Rect) {
    let popup = centered_rect(60, 70, area);
    f.render_widget(Clear, popup);

    let text = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  Tab / Shift+Tab  ", theme::HEADER),
            Span::raw("Switch views"),
        ]),
        Line::from(vec![
            Span::styled("  j/k or ↑/↓       ", theme::HEADER),
            Span::raw("Navigate lists"),
        ]),
        Line::from(vec![
            Span::styled("  Enter             ", theme::HEADER),
            Span::raw("Select / drill into detail"),
        ]),
        Line::from(vec![
            Span::styled("  Backspace / ←     ", theme::HEADER),
            Span::raw("Back from detail view"),
        ]),
        Line::from(vec![
            Span::styled("  g / G             ", theme::HEADER),
            Span::raw("Jump to top / bottom"),
        ]),
        Line::from(vec![
            Span::styled("  ?                 ", theme::HEADER),
            Span::raw("Toggle this help"),
        ]),
        Line::from(vec![
            Span::styled("  q / Esc           ", theme::HEADER),
            Span::raw("Quit"),
        ]),
        Line::from(""),
        Line::from(Span::styled("  Views:", theme::TITLE)),
        Line::from(vec![
            Span::raw("  "),
            Span::styled("Dashboard", theme::ACTIVE),
            Span::raw(" → "),
            Span::styled("Explorer", theme::ACTIVE),
            Span::raw(" → "),
            Span::styled("At", theme::ACTIVE),
            Span::raw(" → "),
            Span::styled("Graph", theme::ACTIVE),
            Span::raw(" → "),
            Span::styled("Detail", theme::ACTIVE),
            Span::raw(" → "),
            Span::styled("Log", theme::ACTIVE),
        ]),
    ];

    let block = Block::default()
        .title(Span::styled(" Help ", theme::TITLE))
        .borders(Borders::ALL);
    let paragraph = Paragraph::new(text).block(block);
    f.render_widget(paragraph, popup);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

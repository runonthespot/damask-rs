use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::theme;

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    if app.graph_lines.is_empty() {
        let block = Block::default()
            .title(Span::styled(" Graph ", theme::TITLE))
            .borders(Borders::ALL);
        let paragraph = Paragraph::new(Line::from(Span::styled(
            "  Select an edge and press Enter to explore its graph",
            theme::DIM,
        )))
        .block(block);
        f.render_widget(paragraph, area);
        return;
    }

    let items: Vec<ListItem> = app
        .graph_lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
            let style = if i == app.graph_selected {
                theme::SELECTED
            } else {
                theme::NORMAL
            };
            ListItem::new(Line::from(Span::styled(format!("  {line}"), style)))
        })
        .collect();

    let block = Block::default()
        .title(Span::styled(" Graph ", theme::TITLE))
        .borders(Borders::ALL);
    let list = List::new(items).block(block);
    f.render_widget(list, area);
}

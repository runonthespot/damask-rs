use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem};
use ratatui::Frame;

use crate::app::App;
use crate::theme;

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = app
        .files
        .iter()
        .enumerate()
        .map(|(i, file)| {
            let style = if i == app.explorer_selected {
                theme::SELECTED
            } else {
                theme::NORMAL
            };
            let badge = if file.edge_count > 0 {
                format!(" ({} spans, {} edges)", file.span_count, file.edge_count)
            } else {
                format!(" ({} spans)", file.span_count)
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!("  {}", file.path), style),
                Span::styled(badge, theme::DIM),
            ]))
        })
        .collect();

    let title = format!(" Files ({}) ", app.files.len());
    let block = Block::default()
        .title(Span::styled(title, theme::TITLE))
        .borders(Borders::ALL);
    let list = List::new(items).block(block);
    f.render_widget(list, area);
}

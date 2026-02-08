use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem};
use ratatui::Frame;

use crate::app::App;
use crate::theme;

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = app
        .log_entries
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let date = entry.ts.split('T').next().unwrap_or(&entry.ts);
            let kind_style = if entry.kind == "span" {
                theme::DIM
            } else {
                theme::NORMAL
            };
            let style = if i == app.log_selected {
                theme::SELECTED
            } else {
                kind_style
            };
            ListItem::new(Line::from(Span::styled(
                format!("  {} {} {} {}", date, entry.kind, entry.id, entry.detail),
                style,
            )))
        })
        .collect();

    let title = format!(" Log ({} entries) ", app.log_entries.len());
    let block = Block::default()
        .title(Span::styled(title, theme::TITLE))
        .borders(Borders::ALL);
    let list = List::new(items).block(block);
    f.render_widget(list, area);
}

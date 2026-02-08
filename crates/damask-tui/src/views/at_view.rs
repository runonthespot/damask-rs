use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::theme;

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    if app.at_edges.is_empty() {
        let msg = if app.at_location.is_empty() {
            "Select a file in Explorer view (Tab to switch, Enter to select)".to_string()
        } else {
            format!("No active edges for {}", app.at_location)
        };
        let block = Block::default()
            .title(Span::styled(" At ", theme::TITLE))
            .borders(Borders::ALL);
        let paragraph =
            Paragraph::new(Line::from(Span::styled(format!("  {msg}"), theme::DIM))).block(block);
        f.render_widget(paragraph, area);
        return;
    }

    let items: Vec<ListItem> = app
        .at_edges
        .iter()
        .enumerate()
        .map(|(i, edge)| {
            let date = edge.row.ts.split('T').next().unwrap_or(&edge.row.ts);
            let agent = edge.row.agent.as_deref().unwrap_or("");
            let style = if i == app.at_selected {
                theme::SELECTED
            } else {
                theme::rel_style(&edge.row.rel)
            };
            ListItem::new(Line::from(Span::styled(
                format!(
                    "  {} [{}] {} {} — {}",
                    edge.row.id, edge.row.rel, date, agent, edge.summary
                ),
                style,
            )))
        })
        .collect();

    let title = format!(" At: {} ({} edges) ", app.at_location, app.at_edges.len());
    let block = Block::default()
        .title(Span::styled(title, theme::TITLE))
        .borders(Borders::ALL);
    let list = List::new(items).block(block);
    f.render_widget(list, area);
}

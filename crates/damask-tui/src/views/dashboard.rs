use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::theme;

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8), // Stats
            Constraint::Length(5), // Namespaces
            Constraint::Min(5),    // Recent edges
        ])
        .split(area);

    render_stats(f, chunks[0], app);
    render_namespaces(f, chunks[1], app);
    render_recent(f, chunks[2], app);
}

fn render_stats(f: &mut Frame, area: Rect, app: &App) {
    let stats = &app.stats;
    let text = vec![
        Line::from(vec![
            Span::styled("Spans: ", theme::HEADER),
            Span::styled(stats.span_count.to_string(), theme::NORMAL),
            Span::raw("  "),
            Span::styled("Edges: ", theme::HEADER),
            Span::styled(stats.edge_count.to_string(), theme::NORMAL),
            Span::raw("  "),
            Span::styled("Active: ", theme::HEADER),
            Span::styled(stats.active_edge_count.to_string(), theme::ACTIVE),
        ]),
        Line::from(vec![
            Span::styled("Endorsements: ", theme::HEADER),
            Span::styled(stats.endorsement_count.to_string(), theme::ENDORSED),
            Span::raw("  "),
            Span::styled("Disputes: ", theme::HEADER),
            Span::styled(stats.dispute_count.to_string(), theme::DISPUTED),
            Span::raw("  "),
            Span::styled("Superseded: ", theme::HEADER),
            Span::styled(stats.superseded_count.to_string(), theme::DIM),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Empty payloads: ", theme::HEADER),
            Span::styled(
                stats.empty_payload_count.to_string(),
                if stats.empty_payload_count > 0 {
                    theme::WARNING
                } else {
                    theme::ACTIVE
                },
            ),
            Span::raw("  "),
            Span::styled("Missing summaries: ", theme::HEADER),
            Span::styled(
                stats.missing_summary_count.to_string(),
                if stats.missing_summary_count > 0 {
                    theme::WARNING
                } else {
                    theme::ACTIVE
                },
            ),
        ]),
    ];

    let block = Block::default()
        .title(Span::styled(" Overview ", theme::TITLE))
        .borders(Borders::ALL);
    let paragraph = Paragraph::new(text).block(block);
    f.render_widget(paragraph, area);
}

fn render_namespaces(f: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = app
        .namespaces
        .iter()
        .map(|ns| {
            let marker = if app.active_ns.as_deref() == Some(ns.as_str()) {
                " *"
            } else {
                ""
            };
            let style = if app.active_ns.as_deref() == Some(ns.as_str()) {
                theme::ACTIVE
            } else {
                theme::NORMAL
            };
            ListItem::new(Line::from(Span::styled(format!("  {ns}{marker}"), style)))
        })
        .collect();

    let block = Block::default()
        .title(Span::styled(" Namespaces ", theme::TITLE))
        .borders(Borders::ALL);
    let list = List::new(items).block(block);
    f.render_widget(list, area);
}

fn render_recent(f: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = app
        .recent_edges
        .iter()
        .enumerate()
        .map(|(i, edge)| {
            let date = edge.row.ts.split('T').next().unwrap_or(&edge.row.ts);
            let agent = edge.row.agent.as_deref().unwrap_or("");
            let active = if edge.row.is_active {
                ""
            } else {
                " (inactive)"
            };
            let style = if i == app.explorer_selected {
                theme::SELECTED
            } else {
                theme::rel_style(&edge.row.rel)
            };
            ListItem::new(Line::from(Span::styled(
                format!(
                    "  {} [{}] {} {} — {}{}",
                    edge.row.id, edge.row.rel, date, agent, edge.summary, active
                ),
                style,
            )))
        })
        .collect();

    let block = Block::default()
        .title(Span::styled(" Recent Edges ", theme::TITLE))
        .borders(Borders::ALL);
    let list = List::new(items).block(block);
    f.render_widget(list, area);
}

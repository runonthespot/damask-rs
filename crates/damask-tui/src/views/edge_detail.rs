use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::App;
use crate::theme;

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let edge = match &app.detail_edge {
        Some(e) => e,
        None => {
            let block = Block::default()
                .title(Span::styled(" Edge Detail ", theme::TITLE))
                .borders(Borders::ALL);
            let paragraph =
                Paragraph::new(Line::from(Span::styled("  No edge selected", theme::DIM)))
                    .block(block);
            f.render_widget(paragraph, area);
            return;
        }
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(10), // Edge info
            Constraint::Length(6),  // Payload
            Constraint::Min(4),     // Endorsements/disputes
        ])
        .split(area);

    // Edge info
    let date = edge.row.ts.split('T').next().unwrap_or(&edge.row.ts);
    let agent = edge.row.agent.as_deref().unwrap_or("unknown");
    let active = if edge.row.is_active { "yes" } else { "no" };
    let from = edge.row.from_id.as_deref().unwrap_or("(null)");
    let to = edge.row.to_id.as_deref().unwrap_or("(null)");

    let info_text = vec![
        Line::from(vec![
            Span::styled("ID: ", theme::HEADER),
            Span::styled(&edge.row.id, theme::NORMAL),
        ]),
        Line::from(vec![
            Span::styled("Rel: ", theme::HEADER),
            Span::styled(&edge.row.rel, theme::rel_style(&edge.row.rel)),
        ]),
        Line::from(vec![
            Span::styled("Summary: ", theme::HEADER),
            Span::styled(&edge.summary, theme::NORMAL),
        ]),
        Line::from(vec![
            Span::styled("From: ", theme::HEADER),
            Span::styled(from, theme::DIM),
            Span::raw("  "),
            Span::styled("To: ", theme::HEADER),
            Span::styled(to, theme::DIM),
        ]),
        Line::from(vec![
            Span::styled("Date: ", theme::HEADER),
            Span::styled(date, theme::NORMAL),
            Span::raw("  "),
            Span::styled("Agent: ", theme::HEADER),
            Span::styled(agent, theme::NORMAL),
        ]),
        Line::from(vec![
            Span::styled("Active: ", theme::HEADER),
            Span::styled(
                active,
                if edge.row.is_active {
                    theme::ACTIVE
                } else {
                    theme::INACTIVE
                },
            ),
            Span::raw("  "),
            Span::styled("Namespace: ", theme::HEADER),
            Span::styled(&edge.row.ns, theme::NORMAL),
        ]),
    ];

    let info_block = Block::default()
        .title(Span::styled(" Edge Detail ", theme::TITLE))
        .borders(Borders::ALL);
    let info_paragraph = Paragraph::new(info_text).block(info_block);
    f.render_widget(info_paragraph, chunks[0]);

    // Payload
    let payload_formatted = serde_json::from_str::<serde_json::Value>(&edge.row.payload)
        .ok()
        .and_then(|v| serde_json::to_string_pretty(&v).ok())
        .unwrap_or_else(|| edge.row.payload.clone());

    let payload_block = Block::default()
        .title(Span::styled(" Payload ", theme::TITLE))
        .borders(Borders::ALL);
    let payload_paragraph = Paragraph::new(payload_formatted)
        .block(payload_block)
        .wrap(Wrap { trim: false })
        .scroll((app.detail_scroll, 0));
    f.render_widget(payload_paragraph, chunks[1]);

    // Endorsements and disputes
    let mut provenance_items: Vec<ListItem> = Vec::new();

    if !app.detail_endorsements.is_empty() {
        provenance_items.push(ListItem::new(Line::from(Span::styled(
            format!("  Endorsements ({}):", app.detail_endorsements.len()),
            theme::ENDORSED,
        ))));
        for e in &app.detail_endorsements {
            let date = e.row.ts.split('T').next().unwrap_or(&e.row.ts);
            let agent = e.row.agent.as_deref().unwrap_or("unknown");
            provenance_items.push(ListItem::new(Line::from(Span::styled(
                format!("    \u{2713} {date} {agent} {}", e.summary),
                theme::ENDORSED,
            ))));
        }
    }

    if !app.detail_disputes.is_empty() {
        provenance_items.push(ListItem::new(Line::from(Span::styled(
            format!("  Disputes ({}):", app.detail_disputes.len()),
            theme::DISPUTED,
        ))));
        for e in &app.detail_disputes {
            let date = e.row.ts.split('T').next().unwrap_or(&e.row.ts);
            let agent = e.row.agent.as_deref().unwrap_or("unknown");
            provenance_items.push(ListItem::new(Line::from(Span::styled(
                format!("    \u{2717} {date} {agent} {}", e.summary),
                theme::DISPUTED,
            ))));
        }
    }

    if provenance_items.is_empty() {
        provenance_items.push(ListItem::new(Line::from(Span::styled(
            "  No endorsements or disputes",
            theme::DIM,
        ))));
    }

    let provenance_block = Block::default()
        .title(Span::styled(" Provenance ", theme::TITLE))
        .borders(Borders::ALL);
    let provenance_list = List::new(provenance_items).block(provenance_block);
    f.render_widget(provenance_list, chunks[2]);
}

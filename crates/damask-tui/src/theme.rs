use ratatui::style::{Color, Modifier, Style};

pub const TITLE: Style = Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD);
pub const HEADER: Style = Style::new().fg(Color::White).add_modifier(Modifier::BOLD);
pub const SELECTED: Style = Style::new().fg(Color::Black).bg(Color::Cyan);
pub const DIM: Style = Style::new().fg(Color::DarkGray);
pub const ACTIVE: Style = Style::new().fg(Color::Green);
pub const INACTIVE: Style = Style::new().fg(Color::DarkGray);
pub const RISK: Style = Style::new().fg(Color::Red);
pub const WARNING: Style = Style::new().fg(Color::Yellow);
pub const ENDORSED: Style = Style::new().fg(Color::Green);
pub const DISPUTED: Style = Style::new().fg(Color::Red);
pub const NORMAL: Style = Style::new().fg(Color::White);

pub const TAB_ACTIVE: Style = Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD);
pub const TAB_INACTIVE: Style = Style::new().fg(Color::DarkGray);

pub const KEY_HINT: Style = Style::new().fg(Color::DarkGray);

pub fn rel_style(rel: &str) -> Style {
    match rel {
        "risk" | "vulnerability" | "threat" => RISK,
        "warning" | "concern" | "caution" => WARNING,
        "endorsed" => ENDORSED,
        "disputed" => DISPUTED,
        _ => NORMAL,
    }
}

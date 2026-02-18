//! Interactive terminal UI for exploring a Damask knowledge fabric.
//!
//! Provides dashboard, explorer, graph traversal, and edge detail views
//! powered by ratatui + crossterm.

pub mod app;
mod input;
mod theme;
mod views;

use std::io;

use crossterm::event::{self, Event, KeyEventKind};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Tabs;
use ratatui::Frame;

use app::{ActiveView, App};
use damask_store::{update_index_with_mode, DamaskProject, IndexMode};

/// Run the TUI application.
pub fn run_tui(project: &DamaskProject) -> anyhow::Result<()> {
    let db_path = project.damask_dir.join("index.db");
    let edges_dir = project.damask_dir.join("edges");
    let conn = update_index_with_mode(&db_path, &edges_dir, IndexMode::ViewsPreferred)
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    let mut app = App::load(project, &conn)?;

    // Setup terminal
    enable_raw_mode()?;
    io::stdout().execute(EnterAlternateScreen)?;
    let mut terminal = ratatui::init();

    let result = run_loop(&mut terminal, &mut app);

    // Restore terminal
    ratatui::restore();
    disable_raw_mode()?;
    io::stdout().execute(LeaveAlternateScreen)?;

    result
}

fn run_loop(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> anyhow::Result<()> {
    loop {
        terminal.draw(|f| ui(f, app))?;

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            if !input::handle_key(app, key.code) {
                return Ok(());
            }
        }
    }
}

fn ui(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Tab bar
            Constraint::Min(5),    // Content
            Constraint::Length(1), // Status bar
        ])
        .split(f.area());

    render_tabs(f, chunks[0], app);
    render_content(f, chunks[1], app);
    render_status_bar(f, chunks[2], app);

    if app.show_help {
        views::help::render(f, f.area());
    }
}

fn render_tabs(f: &mut Frame, area: Rect, app: &App) {
    let titles: Vec<Line> = ["Dashboard", "Explorer", "At", "Graph", "Detail", "Log"]
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let view = match i {
                0 => ActiveView::Dashboard,
                1 => ActiveView::Explorer,
                2 => ActiveView::At,
                3 => ActiveView::Graph,
                4 => ActiveView::EdgeDetail,
                5 => ActiveView::Log,
                _ => ActiveView::Dashboard,
            };
            let style = if app.active_view == view {
                theme::TAB_ACTIVE
            } else {
                theme::TAB_INACTIVE
            };
            Line::from(Span::styled(*t, style))
        })
        .collect();

    let selected = match app.active_view {
        ActiveView::Dashboard => 0,
        ActiveView::Explorer => 1,
        ActiveView::At => 2,
        ActiveView::Graph => 3,
        ActiveView::EdgeDetail => 4,
        ActiveView::Log => 5,
    };

    let tabs = Tabs::new(titles)
        .select(selected)
        .highlight_style(theme::TAB_ACTIVE);
    f.render_widget(tabs, area);
}

fn render_content(f: &mut Frame, area: Rect, app: &App) {
    match app.active_view {
        ActiveView::Dashboard => views::dashboard::render(f, area, app),
        ActiveView::Explorer => views::explorer::render(f, area, app),
        ActiveView::At => views::at_view::render(f, area, app),
        ActiveView::Graph => views::graph::render(f, area, app),
        ActiveView::EdgeDetail => views::edge_detail::render(f, area, app),
        ActiveView::Log => views::log_view::render(f, area, app),
    }
}

fn render_status_bar(f: &mut Frame, area: Rect, _app: &App) {
    let line = Line::from(vec![
        Span::styled(" Tab", theme::HEADER),
        Span::styled(": switch view  ", theme::KEY_HINT),
        Span::styled("j/k", theme::HEADER),
        Span::styled(": navigate  ", theme::KEY_HINT),
        Span::styled("Enter", theme::HEADER),
        Span::styled(": select  ", theme::KEY_HINT),
        Span::styled("?", theme::HEADER),
        Span::styled(": help  ", theme::KEY_HINT),
        Span::styled("q", theme::HEADER),
        Span::styled(": quit", theme::KEY_HINT),
    ]);
    f.render_widget(line, area);
}

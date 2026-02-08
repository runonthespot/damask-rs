use crossterm::event::KeyCode;

use crate::app::{ActiveView, App};

/// Process a key press and return whether the app should continue running.
pub fn handle_key(app: &mut App, key: KeyCode) -> bool {
    // Global keys
    match key {
        KeyCode::Char('q') | KeyCode::Esc => return false,
        KeyCode::Tab => {
            app.next_view();
            return true;
        }
        KeyCode::BackTab => {
            app.prev_view();
            return true;
        }
        KeyCode::Char('?') => {
            app.show_help = !app.show_help;
            return true;
        }
        _ => {}
    }

    if app.show_help {
        // Any key dismisses help
        app.show_help = false;
        return true;
    }

    // View-specific keys
    match app.active_view {
        ActiveView::Dashboard => handle_dashboard_key(app, key),
        ActiveView::Explorer => handle_list_key(app, key, app.explorer_items_len()),
        ActiveView::At => handle_list_key(app, key, app.at_items_len()),
        ActiveView::Graph => handle_list_key(app, key, app.graph_items_len()),
        ActiveView::EdgeDetail => handle_edge_detail_key(app, key),
        ActiveView::Log => handle_list_key(app, key, app.log_items_len()),
    }

    true
}

fn handle_dashboard_key(app: &mut App, key: KeyCode) {
    match key {
        KeyCode::Char('j') | KeyCode::Down => app.dashboard_scroll_down(),
        KeyCode::Char('k') | KeyCode::Up => app.dashboard_scroll_up(),
        KeyCode::Enter => app.dashboard_select(),
        _ => {}
    }
}

fn handle_list_key(app: &mut App, key: KeyCode, len: usize) {
    if len == 0 {
        return;
    }
    match key {
        KeyCode::Char('j') | KeyCode::Down => app.list_down(len),
        KeyCode::Char('k') | KeyCode::Up => app.list_up(),
        KeyCode::Home | KeyCode::Char('g') => app.list_home(),
        KeyCode::End | KeyCode::Char('G') => app.list_end(len),
        KeyCode::Enter => app.list_select(),
        _ => {}
    }
}

fn handle_edge_detail_key(app: &mut App, key: KeyCode) {
    match key {
        KeyCode::Char('j') | KeyCode::Down => app.detail_scroll_down(),
        KeyCode::Char('k') | KeyCode::Up => app.detail_scroll_up(),
        KeyCode::Backspace | KeyCode::Left => app.back_from_detail(),
        _ => {}
    }
}

use damask_core::PayloadEnvelope;
use damask_store::index::query::{EdgeRow, SpanRow};
use damask_store::{update_index_with_mode, DamaskProject, IndexMode, IndexQuery, ProjectStats};
use rusqlite::Connection;
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Which view is currently active.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveView {
    Dashboard,
    Explorer,
    At,
    Graph,
    EdgeDetail,
    Log,
}

const VIEWS: [ActiveView; 6] = [
    ActiveView::Dashboard,
    ActiveView::Explorer,
    ActiveView::At,
    ActiveView::Graph,
    ActiveView::EdgeDetail,
    ActiveView::Log,
];

/// A file entry in the explorer view.
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub path: String,
    pub span_count: usize,
    pub edge_count: usize,
}

/// A log entry combining spans and edges.
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub ts: String,
    pub kind: String, // "span" or "edge"
    pub id: String,
    pub detail: String,
}

/// Edge with its summary pre-extracted for display.
#[derive(Debug, Clone)]
pub struct DisplayEdge {
    pub row: EdgeRow,
    pub summary: String,
}

/// Main application state.
pub struct App {
    pub active_view: ActiveView,
    pub show_help: bool,

    // Data
    pub stats: ProjectStats,
    pub namespaces: Vec<String>,
    pub active_ns: Option<String>,
    pub recent_edges: Vec<DisplayEdge>,

    // Explorer
    pub files: Vec<FileEntry>,
    pub explorer_selected: usize,

    // At view
    pub at_edges: Vec<DisplayEdge>,
    pub at_selected: usize,
    pub at_location: String,

    // Graph view
    pub graph_lines: Vec<String>,
    pub graph_selected: usize,

    // Edge detail
    pub detail_edge: Option<DisplayEdge>,
    pub detail_scroll: u16,
    pub detail_endorsements: Vec<DisplayEdge>,
    pub detail_disputes: Vec<DisplayEdge>,

    // Log
    pub log_entries: Vec<LogEntry>,
    pub log_selected: usize,

    // Internal
    pub project_root: PathBuf,
    pub damask_dir: PathBuf,
    prev_view: ActiveView,
}

impl App {
    /// Create a new App by loading data from the project at `project_root`.
    pub fn load(project: &DamaskProject, conn: &Connection) -> anyhow::Result<Self> {
        let q = IndexQuery::new(conn);

        let stats = q.project_stats().map_err(|e| anyhow::anyhow!("{}", e))?;
        let namespaces = project
            .list_namespaces()
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        let active_ns = project.active_ns();

        // Recent edges (last 20)
        let all_edges = q
            .all_edges_chronological()
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        let recent_edges: Vec<DisplayEdge> = all_edges
            .iter()
            .rev()
            .take(20)
            .map(|e| to_display_edge(e.clone()))
            .collect();

        // Build file list with edge counts
        let all_spans = q
            .all_spans_chronological()
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        let mut file_map: BTreeMap<String, (usize, usize)> = BTreeMap::new();
        for span in &all_spans {
            let entry = file_map.entry(span.path.clone()).or_insert((0, 0));
            entry.0 += 1;
        }
        // Count edges per file through spans
        for span in &all_spans {
            let edges = q
                .edges_for_span(&span.id)
                .map_err(|e| anyhow::anyhow!("{}", e))?;
            let entry = file_map.entry(span.path.clone()).or_insert((0, 0));
            entry.1 += edges.len();
        }
        let files: Vec<FileEntry> = file_map
            .into_iter()
            .map(|(path, (span_count, edge_count))| FileEntry {
                path,
                span_count,
                edge_count,
            })
            .collect();

        // Log entries
        let log_entries = build_log_entries(&all_spans, &all_edges);

        Ok(Self {
            active_view: ActiveView::Dashboard,
            show_help: false,

            stats,
            namespaces,
            active_ns,
            recent_edges,

            files,
            explorer_selected: 0,

            at_edges: Vec::new(),
            at_selected: 0,
            at_location: String::new(),

            graph_lines: Vec::new(),
            graph_selected: 0,

            detail_edge: None,
            detail_scroll: 0,
            detail_endorsements: Vec::new(),
            detail_disputes: Vec::new(),

            log_entries,
            log_selected: 0,

            project_root: project.root.clone(),
            damask_dir: project.damask_dir.clone(),
            prev_view: ActiveView::Dashboard,
        })
    }

    /// Reload data from the index (e.g., after an action).
    pub fn reload(&mut self) -> anyhow::Result<()> {
        let project =
            DamaskProject::discover(&self.project_root).map_err(|e| anyhow::anyhow!("{}", e))?;
        let db_path = project.damask_dir.join("index.db");
        let edges_dir = project.damask_dir.join("edges");
        let conn = update_index_with_mode(&db_path, &edges_dir, IndexMode::ViewsPreferred)
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        let fresh = App::load(&project, &conn)?;

        self.stats = fresh.stats;
        self.namespaces = fresh.namespaces;
        self.active_ns = fresh.active_ns;
        self.recent_edges = fresh.recent_edges;
        self.files = fresh.files;
        self.log_entries = fresh.log_entries;

        Ok(())
    }

    // Navigation

    pub fn next_view(&mut self) {
        let idx = VIEWS
            .iter()
            .position(|v| *v == self.active_view)
            .unwrap_or(0);
        self.active_view = VIEWS[(idx + 1) % VIEWS.len()];
    }

    pub fn prev_view(&mut self) {
        let idx = VIEWS
            .iter()
            .position(|v| *v == self.active_view)
            .unwrap_or(0);
        self.active_view = VIEWS[(idx + VIEWS.len() - 1) % VIEWS.len()];
    }

    // List navigation helpers

    pub fn list_down(&mut self, len: usize) {
        let sel = self.current_selected_mut();
        if *sel + 1 < len {
            *sel += 1;
        }
    }

    pub fn list_up(&mut self) {
        let sel = self.current_selected_mut();
        if *sel > 0 {
            *sel -= 1;
        }
    }

    pub fn list_home(&mut self) {
        *self.current_selected_mut() = 0;
    }

    pub fn list_end(&mut self, len: usize) {
        if len > 0 {
            *self.current_selected_mut() = len - 1;
        }
    }

    pub fn list_select(&mut self) {
        match self.active_view {
            ActiveView::Explorer => {
                if let Some(file) = self.files.get(self.explorer_selected) {
                    self.at_location = file.path.clone();
                    // Load edges for this file
                    self.load_at_for_file(&file.path.clone());
                    self.active_view = ActiveView::At;
                }
            }
            ActiveView::At => {
                if let Some(edge) = self.at_edges.get(self.at_selected) {
                    self.show_edge_detail(edge.clone());
                }
            }
            ActiveView::Graph => {}
            ActiveView::Log => {
                // Try to show edge detail for log entries
                if let Some(entry) = self.log_entries.get(self.log_selected) {
                    if entry.id.starts_with("e_") {
                        // Find this edge in recent_edges
                        if let Some(edge) = self.recent_edges.iter().find(|e| e.row.id == entry.id)
                        {
                            self.show_edge_detail(edge.clone());
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn load_at_for_file(&mut self, path: &str) {
        let project = match DamaskProject::discover(&self.project_root) {
            Ok(p) => p,
            Err(_) => return,
        };
        let db_path = project.damask_dir.join("index.db");
        let edges_dir = project.damask_dir.join("edges");
        let conn = match update_index_with_mode(&db_path, &edges_dir, IndexMode::ViewsPreferred) {
            Ok(c) => c,
            Err(_) => return,
        };
        let q = IndexQuery::new(&conn);

        let spans = q.spans_for_file(path).unwrap_or_default();
        let mut edges = Vec::new();
        for span in &spans {
            if let Ok(span_edges) = q.edges_for_span(&span.id) {
                for e in span_edges {
                    edges.push(to_display_edge(e));
                }
            }
        }
        self.at_edges = edges;
        self.at_selected = 0;
    }

    fn show_edge_detail(&mut self, edge: DisplayEdge) {
        self.prev_view = self.active_view;
        self.detail_edge = Some(edge.clone());
        self.detail_scroll = 0;
        self.detail_endorsements = Vec::new();
        self.detail_disputes = Vec::new();

        // Load endorsements/disputes
        let project = match DamaskProject::discover(&self.project_root) {
            Ok(p) => p,
            Err(_) => {
                self.active_view = ActiveView::EdgeDetail;
                return;
            }
        };
        let db_path = project.damask_dir.join("index.db");
        let edges_dir = project.damask_dir.join("edges");
        let conn = match update_index_with_mode(&db_path, &edges_dir, IndexMode::ViewsPreferred) {
            Ok(c) => c,
            Err(_) => {
                self.active_view = ActiveView::EdgeDetail;
                return;
            }
        };
        let q = IndexQuery::new(&conn);

        if let Ok(targeting) = q.edges_targeting(&edge.row.id) {
            for e in targeting {
                let de = to_display_edge(e.clone());
                match e.rel.as_str() {
                    "endorsed" => self.detail_endorsements.push(de),
                    "disputed" => self.detail_disputes.push(de),
                    _ => {}
                }
            }
        }

        self.active_view = ActiveView::EdgeDetail;
    }

    pub fn back_from_detail(&mut self) {
        self.active_view = self.prev_view;
    }

    // Dashboard

    pub fn dashboard_scroll_down(&mut self) {
        // Scroll through recent edges in dashboard
        if !self.recent_edges.is_empty() {
            self.explorer_selected = (self.explorer_selected + 1).min(self.recent_edges.len() - 1);
        }
    }

    pub fn dashboard_scroll_up(&mut self) {
        if self.explorer_selected > 0 {
            self.explorer_selected -= 1;
        }
    }

    pub fn dashboard_select(&mut self) {
        // Enter edge detail from dashboard recent edges list
        if let Some(edge) = self.recent_edges.get(self.explorer_selected) {
            self.show_edge_detail(edge.clone());
        }
    }

    // Detail

    pub fn detail_scroll_down(&mut self) {
        self.detail_scroll = self.detail_scroll.saturating_add(1);
    }

    pub fn detail_scroll_up(&mut self) {
        self.detail_scroll = self.detail_scroll.saturating_sub(1);
    }

    // Item counts for navigation bounds

    pub fn explorer_items_len(&self) -> usize {
        self.files.len()
    }

    pub fn at_items_len(&self) -> usize {
        self.at_edges.len()
    }

    pub fn graph_items_len(&self) -> usize {
        self.graph_lines.len()
    }

    pub fn log_items_len(&self) -> usize {
        self.log_entries.len()
    }

    fn current_selected_mut(&mut self) -> &mut usize {
        match self.active_view {
            ActiveView::Dashboard => &mut self.explorer_selected,
            ActiveView::Explorer => &mut self.explorer_selected,
            ActiveView::At => &mut self.at_selected,
            ActiveView::Graph => &mut self.graph_selected,
            ActiveView::EdgeDetail => &mut self.at_selected,
            ActiveView::Log => &mut self.log_selected,
        }
    }
}

fn to_display_edge(row: EdgeRow) -> DisplayEdge {
    let p: serde_json::Value = serde_json::from_str(&row.payload).unwrap_or(serde_json::json!({}));
    let env = PayloadEnvelope::new(&p);
    let summary = env.summary().unwrap_or("(no summary)").to_string();
    DisplayEdge { row, summary }
}

fn build_log_entries(spans: &[SpanRow], edges: &[EdgeRow]) -> Vec<LogEntry> {
    let mut entries = Vec::new();

    for span in spans {
        let lines = match (span.line_start, span.line_end) {
            (Some(s), Some(e)) => format!(":{}-{}", s, e),
            _ => String::new(),
        };
        entries.push(LogEntry {
            ts: span.ts.clone(),
            kind: "span".to_string(),
            id: span.id.clone(),
            detail: format!("{}{}", span.path, lines),
        });
    }

    for edge in edges {
        let p: serde_json::Value =
            serde_json::from_str(&edge.payload).unwrap_or(serde_json::json!({}));
        let env = PayloadEnvelope::new(&p);
        let summary = env.summary().unwrap_or("").to_string();
        let active = if edge.is_active { "" } else { " (inactive)" };
        entries.push(LogEntry {
            ts: edge.ts.clone(),
            kind: "edge".to_string(),
            id: edge.id.clone(),
            detail: format!("[{}]{} {}", edge.rel, active, summary),
        });
    }

    entries.sort_by(|a, b| a.ts.cmp(&b.ts));
    entries
}

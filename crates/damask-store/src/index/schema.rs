use rusqlite::Connection;

use crate::StoreError;

/// Create the Damask SQLite schema (spans + edges + indexes).
pub fn create_schema(conn: &Connection) -> Result<(), StoreError> {
    conn.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        PRAGMA synchronous = NORMAL;

        CREATE TABLE IF NOT EXISTS spans (
            id            TEXT PRIMARY KEY,
            path          TEXT NOT NULL,
            line_start    INTEGER,
            line_end      INTEGER,
            snippet       TEXT,
            symbol        TEXT,
            content_hash  TEXT,
            [commit]      TEXT,
            ns            TEXT NOT NULL,
            ts            TEXT NOT NULL,
            agent         TEXT,
            session       TEXT,
            resolution    TEXT,
            recency       TEXT,
            source_file   TEXT
        );

        CREATE TABLE IF NOT EXISTS edges (
            id          TEXT PRIMARY KEY,
            from_id     TEXT,
            to_id       TEXT,
            rel         TEXT NOT NULL,
            payload     TEXT NOT NULL,
            ns          TEXT NOT NULL,
            ts          TEXT NOT NULL,
            agent       TEXT,
            session     TEXT,
            source_file TEXT,
            is_active   INTEGER DEFAULT 1
        );

        CREATE TABLE IF NOT EXISTS index_meta (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_edges_from ON edges(from_id);
        CREATE INDEX IF NOT EXISTS idx_edges_to ON edges(to_id);
        CREATE INDEX IF NOT EXISTS idx_edges_rel ON edges(rel);
        CREATE INDEX IF NOT EXISTS idx_edges_active ON edges(is_active);
        CREATE INDEX IF NOT EXISTS idx_spans_path ON spans(path);
        CREATE INDEX IF NOT EXISTS idx_spans_symbol ON spans(symbol);
        CREATE INDEX IF NOT EXISTS idx_spans_hash ON spans(content_hash);

        CREATE VIRTUAL TABLE IF NOT EXISTS edges_fts USING fts5(
            payload,
            content=edges,
            content_rowid=rowid
        );

        CREATE TRIGGER IF NOT EXISTS edges_ai AFTER INSERT ON edges BEGIN
            INSERT INTO edges_fts(rowid, payload) VALUES (new.rowid, new.payload);
        END;

        CREATE TRIGGER IF NOT EXISTS edges_ad AFTER DELETE ON edges BEGIN
            INSERT INTO edges_fts(edges_fts, rowid, payload) VALUES ('delete', old.rowid, old.payload);
        END;

        CREATE TRIGGER IF NOT EXISTS edges_au AFTER UPDATE ON edges BEGIN
            INSERT INTO edges_fts(edges_fts, rowid, payload) VALUES ('delete', old.rowid, old.payload);
            INSERT INTO edges_fts(rowid, payload) VALUES (new.rowid, new.payload);
        END;
        ",
    )
    .map_err(|e| StoreError::Io(e.to_string()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_creates_tables() {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();

        // Verify tables exist
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('spans','edges','index_meta','edges_fts')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 4);
    }

    #[test]
    fn schema_is_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();
        create_schema(&conn).unwrap(); // should not error
    }
}

use rusqlite::Connection;

use crate::StoreError;

/// Compute the active/inactive state for all edges in the index.
///
/// Per spec §7.6:
/// - A content edge is active if no effective supersedes edge targets it.
/// - A supersedes edge is effective unless invalidated.
/// - Meta edges (supersedes, invalidates, endorsed, disputed) are never shown in `at` output.
pub fn compute_active_state(conn: &Connection) -> Result<(), StoreError> {
    // Start with all edges marked active
    conn.execute("UPDATE edges SET is_active = 1", [])
        .map_err(|e| StoreError::Io(e.to_string()))?;

    // Find all content edges that are superseded by an effective supersedes edge.
    // A supersedes edge is effective if it is NOT the target of an invalidates edge.
    conn.execute(
        "UPDATE edges SET is_active = 0
         WHERE id IN (
             SELECT e.to_id
             FROM edges e
             WHERE e.rel = 'supersedes'
               AND e.to_id IS NOT NULL
               AND e.id NOT IN (
                   SELECT inv.to_id
                   FROM edges inv
                   WHERE inv.rel = 'invalidates'
                     AND inv.to_id IS NOT NULL
               )
         )",
        [],
    )
    .map_err(|e| StoreError::Io(e.to_string()))?;

    // Meta edges are never shown in `at` output — mark them as inactive for display purposes.
    conn.execute(
        "UPDATE edges SET is_active = 0
         WHERE rel IN ('supersedes', 'invalidates', 'endorsed', 'disputed')",
        [],
    )
    .map_err(|e| StoreError::Io(e.to_string()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::schema::create_schema;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();
        conn
    }

    fn insert_edge(conn: &Connection, id: &str, from: Option<&str>, to: Option<&str>, rel: &str) {
        conn.execute(
            "INSERT INTO edges (id, from_id, to_id, rel, payload, ns, ts) VALUES (?1, ?2, ?3, ?4, '{}', 'test', '2025-01-01T00:00:00Z')",
            rusqlite::params![id, from, to, rel],
        )
        .unwrap();
    }

    #[test]
    fn simple_active_edge() {
        let conn = setup_db();
        insert_edge(&conn, "e_1", Some("s_1"), None, "risk");
        compute_active_state(&conn).unwrap();

        let active: bool = conn
            .query_row("SELECT is_active FROM edges WHERE id = 'e_1'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert!(active);
    }

    #[test]
    fn superseded_edge_inactive() {
        let conn = setup_db();
        insert_edge(&conn, "e_old", Some("s_1"), None, "risk");
        insert_edge(&conn, "e_new", Some("s_1"), None, "risk");
        insert_edge(&conn, "e_sup", Some("e_new"), Some("e_old"), "supersedes");

        compute_active_state(&conn).unwrap();

        let old_active: bool = conn
            .query_row("SELECT is_active FROM edges WHERE id = 'e_old'", [], |r| {
                r.get(0)
            })
            .unwrap();
        let new_active: bool = conn
            .query_row("SELECT is_active FROM edges WHERE id = 'e_new'", [], |r| {
                r.get(0)
            })
            .unwrap();

        assert!(!old_active, "superseded edge should be inactive");
        assert!(new_active, "replacement edge should be active");
    }

    #[test]
    fn invalidated_supersession_reinstates() {
        let conn = setup_db();
        insert_edge(&conn, "e_orig", Some("s_1"), None, "risk");
        insert_edge(&conn, "e_new", Some("s_1"), None, "risk");
        insert_edge(&conn, "e_sup", Some("e_new"), Some("e_orig"), "supersedes");
        insert_edge(&conn, "e_inv", Some("e_inv"), Some("e_sup"), "invalidates");

        compute_active_state(&conn).unwrap();

        let orig_active: bool = conn
            .query_row("SELECT is_active FROM edges WHERE id = 'e_orig'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert!(orig_active, "original edge should be reinstated");
    }

    #[test]
    fn meta_edges_not_active() {
        let conn = setup_db();
        insert_edge(&conn, "e_content", Some("s_1"), None, "risk");
        insert_edge(
            &conn,
            "e_endorse",
            Some("e_content"),
            None,
            "endorsed",
        );
        insert_edge(
            &conn,
            "e_dispute",
            Some("e_content"),
            None,
            "disputed",
        );

        compute_active_state(&conn).unwrap();

        let content_active: bool = conn
            .query_row(
                "SELECT is_active FROM edges WHERE id = 'e_content'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let endorse_active: bool = conn
            .query_row(
                "SELECT is_active FROM edges WHERE id = 'e_endorse'",
                [],
                |r| r.get(0),
            )
            .unwrap();

        assert!(content_active);
        assert!(!endorse_active, "meta edges should not be active");
    }
}

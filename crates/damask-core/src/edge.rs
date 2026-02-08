use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{DamaskId, EdgeId};

/// A relationship between spans (or edges), carrying a JSON payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    /// Unique identifier (e_ + ULID).
    pub id: EdgeId,

    /// Source span or edge ID (null for tags/notes without a source).
    pub from: Option<DamaskId>,

    /// Target span or edge ID (null for single-endpoint edges).
    pub to: Option<DamaskId>,

    /// Relationship type (e.g., "risk", "depends_on", "supersedes").
    pub rel: String,

    /// The knowledge payload — arbitrary JSON.
    pub payload: serde_json::Value,

    /// Namespace this edge belongs to.
    pub ns: String,

    /// Timestamp of creation.
    pub ts: DateTime<Utc>,

    /// Agent that created this edge.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,

    /// Session that produced this edge.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SpanId;

    fn sample_edge() -> Edge {
        let span_id = SpanId::new();
        Edge {
            id: EdgeId::new(),
            from: Some(DamaskId::Span(span_id)),
            to: None,
            rel: "risk".to_string(),
            payload: serde_json::json!({
                "summary": "No token expiry check",
                "confidence": 0.95,
                "action": "Add expiry validation"
            }),
            ns: "security-audit".to_string(),
            ts: Utc::now(),
            agent: Some("claude-opus-4-6".to_string()),
            session: Some("abc123".to_string()),
        }
    }

    #[test]
    fn edge_serde_roundtrip() {
        let edge = sample_edge();
        let json = serde_json::to_string(&edge).unwrap();
        let parsed: Edge = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.rel, "risk");
        assert!(parsed.from.is_some());
        assert!(parsed.to.is_none());
    }

    #[test]
    fn edge_with_null_endpoints() {
        let edge = Edge {
            id: EdgeId::new(),
            from: None,
            to: None,
            rel: "observation".to_string(),
            payload: serde_json::json!({"summary": "General note"}),
            ns: "notes".to_string(),
            ts: Utc::now(),
            agent: None,
            session: None,
        };
        let json = serde_json::to_string(&edge).unwrap();
        assert!(json.contains("\"from\":null"));
        assert!(json.contains("\"to\":null"));
        let parsed: Edge = serde_json::from_str(&json).unwrap();
        assert!(parsed.from.is_none());
        assert!(parsed.to.is_none());
    }

    #[test]
    fn edge_with_edge_endpoint() {
        let target_edge_id = EdgeId::new();
        let edge = Edge {
            id: EdgeId::new(),
            from: Some(DamaskId::Edge(EdgeId::new())),
            to: Some(DamaskId::Edge(target_edge_id)),
            rel: "supersedes".to_string(),
            payload: serde_json::json!({"summary": "Updated finding"}),
            ns: "security-audit".to_string(),
            ts: Utc::now(),
            agent: None,
            session: None,
        };
        let json = serde_json::to_string(&edge).unwrap();
        let parsed: Edge = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed.from, Some(DamaskId::Edge(_))));
        assert!(matches!(parsed.to, Some(DamaskId::Edge(_))));
    }
}

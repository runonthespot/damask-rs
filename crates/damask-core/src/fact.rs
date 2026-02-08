use serde::{Deserialize, Serialize};

use crate::{Edge, Span};

/// A single fact in the Damask knowledge fabric — either a Span or an Edge.
/// Serialized with a `"t"` discriminator tag: `{"t":"span",...}` or `{"t":"edge",...}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "t")]
pub enum Fact {
    #[serde(rename = "span")]
    Span(Span),
    #[serde(rename = "edge")]
    Edge(Edge),
}

impl Fact {
    /// Return the namespace of this fact.
    pub fn ns(&self) -> &str {
        match self {
            Fact::Span(s) => &s.ns,
            Fact::Edge(e) => &e.ns,
        }
    }

    /// Return the timestamp of this fact.
    pub fn ts(&self) -> chrono::DateTime<chrono::Utc> {
        match self {
            Fact::Span(s) => s.ts,
            Fact::Edge(e) => e.ts,
        }
    }

    /// Return the agent that created this fact, if any.
    pub fn agent(&self) -> Option<&str> {
        match self {
            Fact::Span(s) => s.agent.as_deref(),
            Fact::Edge(e) => e.agent.as_deref(),
        }
    }

    /// Set the namespace on this fact.
    pub fn set_ns(&mut self, ns: String) {
        match self {
            Fact::Span(s) => s.ns = ns,
            Fact::Edge(e) => e.ns = ns,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DamaskId, EdgeId, SpanId};
    use chrono::Utc;

    #[test]
    fn fact_span_serde_has_t_tag() {
        let span = Span {
            id: SpanId::new(),
            path: "src/main.rs".to_string(),
            lines: Some([1, 10]),
            snippet: Some("fn main()".to_string()),
            symbol: Some("main".to_string()),
            content_hash: None,
            commit: None,
            ns: "test".to_string(),
            ts: Utc::now(),
            agent: None,
            session: None,
        };
        let fact = Fact::Span(span);
        let json = serde_json::to_string(&fact).unwrap();
        assert!(json.contains("\"t\":\"span\""));
        let parsed: Fact = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, Fact::Span(_)));
    }

    #[test]
    fn fact_edge_serde_has_t_tag() {
        let edge = Edge {
            id: EdgeId::new(),
            from: Some(DamaskId::Span(SpanId::new())),
            to: None,
            rel: "risk".to_string(),
            payload: serde_json::json!({"summary": "test"}),
            ns: "test".to_string(),
            ts: Utc::now(),
            agent: None,
            session: None,
        };
        let fact = Fact::Edge(edge);
        let json = serde_json::to_string(&fact).unwrap();
        assert!(json.contains("\"t\":\"edge\""));
        let parsed: Fact = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, Fact::Edge(_)));
    }

    #[test]
    fn fact_ns_accessor() {
        let span = Span {
            id: SpanId::new(),
            path: "test.rs".to_string(),
            lines: None,
            snippet: None,
            symbol: None,
            content_hash: None,
            commit: None,
            ns: "my-namespace".to_string(),
            ts: Utc::now(),
            agent: None,
            session: None,
        };
        let fact = Fact::Span(span);
        assert_eq!(fact.ns(), "my-namespace");
    }

    #[test]
    fn deserialize_spec_example_span() {
        let json = r#"{"t":"span","id":"s_01JKX1A0000000000000000000","path":"src/auth.py","lines":[42,67],"snippet":"def validate_token(token):","symbol":"validate_token","content_hash":"a3f7c2","ns":"security-audit","ts":"2025-01-15T10:30:00Z","agent":"claude-opus-4-6"}"#;
        let fact: Fact = serde_json::from_str(json).unwrap();
        match fact {
            Fact::Span(s) => {
                assert_eq!(s.path, "src/auth.py");
                assert_eq!(s.lines, Some([42, 67]));
                assert_eq!(s.symbol.as_deref(), Some("validate_token"));
            }
            _ => panic!("expected Span"),
        }
    }

    #[test]
    fn deserialize_spec_example_edge() {
        let json = r#"{"t":"edge","id":"e_01JKX1B0000000000000000000","from":"s_01JKX1A0000000000000000000","to":null,"rel":"risk","payload":{"summary":"No token expiry check","confidence":0.95,"action":"Add expiry validation","level":"high","cvss":9.1},"ns":"security-audit","ts":"2025-01-15T10:30:02Z","agent":"claude-opus-4-6"}"#;
        let fact: Fact = serde_json::from_str(json).unwrap();
        match fact {
            Fact::Edge(e) => {
                assert_eq!(e.rel, "risk");
                assert!(e.from.is_some());
                assert!(e.to.is_none());
                assert_eq!(e.payload["summary"], "No token expiry check");
            }
            _ => panic!("expected Edge"),
        }
    }
}

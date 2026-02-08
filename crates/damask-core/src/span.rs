use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::SpanId;

/// A reference to a region within a file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Span {
    /// Unique identifier (s_ + ULID).
    pub id: SpanId,

    /// Root-relative file path.
    pub path: String,

    /// Line range [start, end], 1-indexed, inclusive.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lines: Option<[u32; 2]>,

    /// Short text excerpt for fuzzy re-anchoring.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,

    /// Semantic anchor (function name, section heading, clause number).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,

    /// Truncated SHA-256 of span text (first 12 hex chars).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,

    /// Git commit hash at the time the span was created.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,

    /// Namespace this span belongs to.
    pub ns: String,

    /// Timestamp of creation.
    pub ts: DateTime<Utc>,

    /// Agent that created this span.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,

    /// Session that produced this span.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
}

impl Span {
    /// Start line (1-indexed), or None if no line range.
    pub fn start_line(&self) -> Option<u32> {
        self.lines.map(|l| l[0])
    }

    /// End line (1-indexed, inclusive), or None if no line range.
    pub fn end_line(&self) -> Option<u32> {
        self.lines.map(|l| l[1])
    }

    /// Whether a given line falls within this span's range.
    pub fn contains_line(&self, line: u32) -> bool {
        match self.lines {
            Some([start, end]) => line >= start && line <= end,
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_span() -> Span {
        Span {
            id: SpanId::new(),
            path: "src/auth.py".to_string(),
            lines: Some([42, 67]),
            snippet: Some("def validate_token(token):".to_string()),
            symbol: Some("validate_token".to_string()),
            content_hash: Some("a3f7c2d1e8b9".to_string()),
            commit: Some("abc1234def56".to_string()),
            ns: "security-audit".to_string(),
            ts: Utc::now(),
            agent: Some("claude-opus-4-6".to_string()),
            session: Some("abc123".to_string()),
        }
    }

    #[test]
    fn span_serde_roundtrip() {
        let span = sample_span();
        let json = serde_json::to_string(&span).unwrap();
        let parsed: Span = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.path, "src/auth.py");
        assert_eq!(parsed.lines, Some([42, 67]));
        assert_eq!(parsed.symbol.as_deref(), Some("validate_token"));
    }

    #[test]
    fn span_contains_line() {
        let span = sample_span();
        assert!(span.contains_line(42));
        assert!(span.contains_line(55));
        assert!(span.contains_line(67));
        assert!(!span.contains_line(41));
        assert!(!span.contains_line(68));
    }

    #[test]
    fn span_without_optional_fields() {
        let span = Span {
            id: SpanId::new(),
            path: "readme.md".to_string(),
            lines: Some([1, 10]),
            snippet: None,
            symbol: None,
            content_hash: None,
            commit: None,
            ns: "notes".to_string(),
            ts: Utc::now(),
            agent: None,
            session: None,
        };
        let json = serde_json::to_string(&span).unwrap();
        // Optional None fields should be omitted
        assert!(!json.contains("snippet"));
        assert!(!json.contains("symbol"));
        assert!(!json.contains("content_hash"));
        assert!(!json.contains("commit"));
        // Round-trip
        let parsed: Span = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.snippet, None);
    }
}

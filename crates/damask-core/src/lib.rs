//! Core data types for the Damask knowledge fabric.
//!
//! This crate contains the pure data types — no I/O, no filesystem access.
//! Everything here is serializable and testable in isolation.

pub mod config;
pub mod edge;
pub mod fact;
pub mod freshness;
pub mod id;
pub mod payload;
pub mod span;
pub mod vocabulary;

// Re-exports for convenience.
pub use config::{DamaskConfig, NamespaceConfig};
pub use edge::Edge;
pub use fact::Fact;
pub use freshness::{Freshness, Recency, Resolution};
pub use id::{DamaskId, EdgeId, SpanId};
pub use payload::PayloadEnvelope;
pub use span::Span;
pub use vocabulary::RelClass;

/// Truncate a string to at most `max` bytes, respecting UTF-8 char boundaries.
pub fn truncate_str(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_str_ascii() {
        assert_eq!(truncate_str("hello world", 5), "hello");
        assert_eq!(truncate_str("hello", 10), "hello");
        assert_eq!(truncate_str("", 5), "");
    }

    #[test]
    fn truncate_str_multibyte() {
        // '€' is 3 bytes (E2 82 AC)
        let s = "€€€"; // 9 bytes
        assert_eq!(truncate_str(s, 9), "€€€");
        assert_eq!(truncate_str(s, 6), "€€");
        // 4 bytes lands mid-char, should back up to 3
        assert_eq!(truncate_str(s, 4), "€");
        assert_eq!(truncate_str(s, 2), "");
    }

    #[test]
    fn truncate_str_emoji() {
        // '🦀' is 4 bytes
        let s = "🦀abc";
        assert_eq!(truncate_str(s, 7), "🦀abc");
        assert_eq!(truncate_str(s, 5), "🦀a");
        assert_eq!(truncate_str(s, 4), "🦀");
        assert_eq!(truncate_str(s, 3), "");
    }
}

/// Errors produced by core type operations.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("invalid ID format: {0}")]
    InvalidId(String),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

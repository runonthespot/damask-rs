use serde::{Deserialize, Serialize};
use std::fmt;
use ulid::Ulid;

use crate::CoreError;

/// A span identifier: "s_" prefix + ULID.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct SpanId(String);

/// An edge identifier: "e_" prefix + ULID.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct EdgeId(String);

/// A generic Damask identifier — either a span or edge ID.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum DamaskId {
    Span(SpanId),
    Edge(EdgeId),
}

impl SpanId {
    /// Generate a new span ID with the current timestamp.
    pub fn new() -> Self {
        Self(format!("s_{}", Ulid::new()))
    }

    /// Parse a string into a SpanId, validating the prefix and ULID format.
    pub fn parse(s: &str) -> Result<Self, CoreError> {
        if !s.starts_with("s_") {
            return Err(CoreError::InvalidId(format!(
                "span ID must start with 's_', got '{s}'"
            )));
        }
        let ulid_part = &s[2..];
        ulid_part
            .parse::<Ulid>()
            .map_err(|e| CoreError::InvalidId(format!("invalid ULID in span ID '{s}': {e}")))?;
        Ok(Self(s.to_string()))
    }

    /// Return the string representation.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for SpanId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for SpanId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<SpanId> for String {
    fn from(id: SpanId) -> String {
        id.0
    }
}

impl TryFrom<String> for SpanId {
    type Error = CoreError;
    fn try_from(s: String) -> Result<Self, CoreError> {
        SpanId::parse(&s)
    }
}

impl EdgeId {
    /// Generate a new edge ID with the current timestamp.
    pub fn new() -> Self {
        Self(format!("e_{}", Ulid::new()))
    }

    /// Parse a string into an EdgeId, validating the prefix and ULID format.
    pub fn parse(s: &str) -> Result<Self, CoreError> {
        if !s.starts_with("e_") {
            return Err(CoreError::InvalidId(format!(
                "edge ID must start with 'e_', got '{s}'"
            )));
        }
        let ulid_part = &s[2..];
        ulid_part
            .parse::<Ulid>()
            .map_err(|e| CoreError::InvalidId(format!("invalid ULID in edge ID '{s}': {e}")))?;
        Ok(Self(s.to_string()))
    }

    /// Return the string representation.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for EdgeId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for EdgeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<EdgeId> for String {
    fn from(id: EdgeId) -> String {
        id.0
    }
}

impl TryFrom<String> for EdgeId {
    type Error = CoreError;
    fn try_from(s: String) -> Result<Self, CoreError> {
        EdgeId::parse(&s)
    }
}

impl DamaskId {
    /// Parse a string into a DamaskId (tries span first, then edge).
    pub fn parse(s: &str) -> Result<Self, CoreError> {
        if s.starts_with("s_") {
            Ok(DamaskId::Span(SpanId::parse(s)?))
        } else if s.starts_with("e_") {
            Ok(DamaskId::Edge(EdgeId::parse(s)?))
        } else {
            Err(CoreError::InvalidId(format!(
                "ID must start with 's_' or 'e_', got '{s}'"
            )))
        }
    }

    /// Return the string representation.
    pub fn as_str(&self) -> &str {
        match self {
            DamaskId::Span(id) => id.as_str(),
            DamaskId::Edge(id) => id.as_str(),
        }
    }
}

impl fmt::Display for DamaskId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<DamaskId> for String {
    fn from(id: DamaskId) -> String {
        match id {
            DamaskId::Span(s) => s.0,
            DamaskId::Edge(e) => e.0,
        }
    }
}

impl TryFrom<String> for DamaskId {
    type Error = CoreError;
    fn try_from(s: String) -> Result<Self, CoreError> {
        DamaskId::parse(&s)
    }
}

impl From<SpanId> for DamaskId {
    fn from(id: SpanId) -> Self {
        DamaskId::Span(id)
    }
}

impl From<EdgeId> for DamaskId {
    fn from(id: EdgeId) -> Self {
        DamaskId::Edge(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn span_id_new_has_correct_prefix() {
        let id = SpanId::new();
        assert!(id.as_str().starts_with("s_"));
        assert_eq!(id.as_str().len(), 28); // "s_" + 26-char ULID
    }

    #[test]
    fn edge_id_new_has_correct_prefix() {
        let id = EdgeId::new();
        assert!(id.as_str().starts_with("e_"));
        assert_eq!(id.as_str().len(), 28);
    }

    #[test]
    fn span_id_parse_valid() {
        let id = SpanId::new();
        let parsed = SpanId::parse(id.as_str()).unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn span_id_parse_rejects_edge_prefix() {
        let result = SpanId::parse("e_01JKXYZ01234567890ABCDEFG");
        assert!(result.is_err());
    }

    #[test]
    fn edge_id_parse_rejects_span_prefix() {
        let result = EdgeId::parse("s_01JKXYZ01234567890ABCDEFG");
        assert!(result.is_err());
    }

    #[test]
    fn damask_id_parse_span() {
        let id = SpanId::new();
        let did = DamaskId::parse(id.as_str()).unwrap();
        assert!(matches!(did, DamaskId::Span(_)));
    }

    #[test]
    fn damask_id_parse_edge() {
        let id = EdgeId::new();
        let did = DamaskId::parse(id.as_str()).unwrap();
        assert!(matches!(did, DamaskId::Edge(_)));
    }

    #[test]
    fn damask_id_parse_rejects_garbage() {
        let result = DamaskId::parse("garbage");
        assert!(result.is_err());
    }

    #[test]
    fn span_id_serde_roundtrip() {
        let id = SpanId::new();
        let json = serde_json::to_string(&id).unwrap();
        let parsed: SpanId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn edge_id_serde_roundtrip() {
        let id = EdgeId::new();
        let json = serde_json::to_string(&id).unwrap();
        let parsed: EdgeId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, parsed);
    }
}

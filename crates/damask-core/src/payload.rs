use serde_json::Value;

/// Accessor for the conventional payload envelope keys.
/// Wraps a reference to a JSON value and provides typed access to
/// the standard fields: summary, confidence, status, evidence, action, tags.
pub struct PayloadEnvelope<'a> {
    raw: &'a Value,
}

impl<'a> PayloadEnvelope<'a> {
    /// Wrap a JSON payload value.
    pub fn new(raw: &'a Value) -> Self {
        Self { raw }
    }

    /// One-line human-readable description.
    pub fn summary(&self) -> Option<&str> {
        self.raw.get("summary").and_then(|v| v.as_str())
    }

    /// Agent confidence in this edge (0.0–1.0).
    pub fn confidence(&self) -> Option<f64> {
        self.raw.get("confidence").and_then(|v| v.as_f64())
    }

    /// Edge status: "assertion" (default), "hypothesis", or "ruled_out".
    pub fn status(&self) -> Option<&str> {
        self.raw.get("status").and_then(|v| v.as_str())
    }

    /// Supporting evidence (span IDs or text snippets).
    pub fn evidence(&self) -> Option<Vec<&str>> {
        self.raw.get("evidence").and_then(|v| {
            v.as_array().map(|arr| {
                arr.iter()
                    .filter_map(|item| item.as_str())
                    .collect::<Vec<_>>()
            })
        })
    }

    /// What should be done about this edge.
    pub fn action(&self) -> Option<&str> {
        self.raw.get("action").and_then(|v| v.as_str())
    }

    /// Freeform labels for filtering.
    pub fn tags(&self) -> Option<Vec<&str>> {
        self.raw.get("tags").and_then(|v| {
            v.as_array().map(|arr| {
                arr.iter()
                    .filter_map(|item| item.as_str())
                    .collect::<Vec<_>>()
            })
        })
    }

    /// Access the raw JSON value.
    pub fn raw(&self) -> &Value {
        self.raw
    }

    /// Check if the payload is an empty object `{}`.
    pub fn is_empty(&self) -> bool {
        matches!(self.raw, Value::Object(map) if map.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn full_envelope() {
        let payload = json!({
            "summary": "No token expiry check",
            "confidence": 0.95,
            "status": "assertion",
            "evidence": ["s_01JKX1A..."],
            "action": "Add expiry validation",
            "tags": ["security", "auth"]
        });
        let env = PayloadEnvelope::new(&payload);
        assert_eq!(env.summary(), Some("No token expiry check"));
        assert_eq!(env.confidence(), Some(0.95));
        assert_eq!(env.status(), Some("assertion"));
        assert_eq!(env.evidence(), Some(vec!["s_01JKX1A..."]));
        assert_eq!(env.action(), Some("Add expiry validation"));
        assert_eq!(env.tags(), Some(vec!["security", "auth"]));
    }

    #[test]
    fn empty_payload() {
        let payload = json!({});
        let env = PayloadEnvelope::new(&payload);
        assert!(env.is_empty());
        assert_eq!(env.summary(), None);
        assert_eq!(env.confidence(), None);
    }

    #[test]
    fn partial_envelope() {
        let payload = json!({"summary": "A finding"});
        let env = PayloadEnvelope::new(&payload);
        assert_eq!(env.summary(), Some("A finding"));
        assert_eq!(env.confidence(), None);
        assert_eq!(env.action(), None);
        assert!(!env.is_empty());
    }
}

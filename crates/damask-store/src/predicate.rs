use damask_core::PayloadEnvelope;

use crate::index::query::EdgeRow;
use crate::StoreError;

/// A simple predicate: `field op value`.
///
/// Phase 1 supports single predicates only (no AND/OR).
/// Compose via shell pipes: `damask where rel=risk --format json | ...`
#[derive(Debug, Clone)]
pub struct Predicate {
    pub field: String,
    pub op: CompareOp,
    pub value: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CompareOp {
    Eq,
    Ne,
    Gt,
    Lt,
    Gte,
    Lte,
}

impl Predicate {
    /// Parse a predicate string like `rel=risk` or `confidence>0.8`.
    pub fn parse(s: &str) -> Result<Self, StoreError> {
        // Try two-char ops first (order matters: != before =, >= before >, <= before <)
        for (pat, op) in &[
            ("!=", CompareOp::Ne),
            (">=", CompareOp::Gte),
            ("<=", CompareOp::Lte),
        ] {
            if let Some(pos) = s.find(pat) {
                let field = s[..pos].trim().to_string();
                let value = s[pos + pat.len()..].trim().to_string();
                if field.is_empty() {
                    return Err(StoreError::Io(format!("empty field in predicate: {s}")));
                }
                return Ok(Predicate {
                    field,
                    op: *op,
                    value,
                });
            }
        }
        // Then single-char ops
        for (pat, op) in &[
            ("=", CompareOp::Eq),
            (">", CompareOp::Gt),
            ("<", CompareOp::Lt),
        ] {
            if let Some(pos) = s.find(pat) {
                let field = s[..pos].trim().to_string();
                let value = s[pos + pat.len()..].trim().to_string();
                if field.is_empty() {
                    return Err(StoreError::Io(format!("empty field in predicate: {s}")));
                }
                return Ok(Predicate {
                    field,
                    op: *op,
                    value,
                });
            }
        }
        Err(StoreError::Io(format!(
            "invalid predicate (expected field=value, field>value, etc.): {s}"
        )))
    }

    /// Check if an edge matches this predicate.
    pub fn matches(&self, edge: &EdgeRow, endorsement_count: u32, dispute_count: u32) -> bool {
        match self.field.as_str() {
            "rel" => self.compare_str(&edge.rel),
            "ns" => self.compare_str(&edge.ns),
            "agent" => {
                let agent = edge.agent.as_deref().unwrap_or("");
                self.compare_str(agent)
            }
            "endorsed" => {
                if let Ok(val) = self.value.parse::<f64>() {
                    self.compare_num(endorsement_count as f64, val)
                } else {
                    false
                }
            }
            "disputed" => {
                // "disputed=true" means dispute_count > 0
                if self.value == "true" || self.value == "false" {
                    let is_disputed = dispute_count > 0;
                    let want = self.value == "true";
                    match self.op {
                        CompareOp::Eq => is_disputed == want,
                        CompareOp::Ne => is_disputed != want,
                        _ => false,
                    }
                } else if let Ok(val) = self.value.parse::<f64>() {
                    self.compare_num(dispute_count as f64, val)
                } else {
                    false
                }
            }
            // Payload envelope fields
            "confidence" => {
                let payload: serde_json::Value =
                    serde_json::from_str(&edge.payload).unwrap_or(serde_json::json!({}));
                let env = PayloadEnvelope::new(&payload);
                if let (Some(conf), Ok(val)) = (env.confidence(), self.value.parse::<f64>()) {
                    self.compare_num(conf, val)
                } else {
                    false
                }
            }
            "status" => {
                let payload: serde_json::Value =
                    serde_json::from_str(&edge.payload).unwrap_or(serde_json::json!({}));
                let env = PayloadEnvelope::new(&payload);
                let status = env.status().unwrap_or("");
                self.compare_str(status)
            }
            "summary" => {
                let payload: serde_json::Value =
                    serde_json::from_str(&edge.payload).unwrap_or(serde_json::json!({}));
                let env = PayloadEnvelope::new(&payload);
                let summary = env.summary().unwrap_or("");
                self.compare_str(summary)
            }
            "tags" => {
                let payload: serde_json::Value =
                    serde_json::from_str(&edge.payload).unwrap_or(serde_json::json!({}));
                let env = PayloadEnvelope::new(&payload);
                let tags = env.tags().unwrap_or_default();
                match self.op {
                    CompareOp::Eq => tags.iter().any(|t| *t == self.value),
                    CompareOp::Ne => !tags.iter().any(|t| *t == self.value),
                    _ => false,
                }
            }
            _ => false,
        }
    }

    fn compare_str(&self, actual: &str) -> bool {
        match self.op {
            CompareOp::Eq => actual == self.value,
            CompareOp::Ne => actual != self.value,
            CompareOp::Gt => actual > self.value.as_str(),
            CompareOp::Lt => actual < self.value.as_str(),
            CompareOp::Gte => actual >= self.value.as_str(),
            CompareOp::Lte => actual <= self.value.as_str(),
        }
    }

    fn compare_num(&self, actual: f64, expected: f64) -> bool {
        match self.op {
            CompareOp::Eq => (actual - expected).abs() < f64::EPSILON,
            CompareOp::Ne => (actual - expected).abs() >= f64::EPSILON,
            CompareOp::Gt => actual > expected,
            CompareOp::Lt => actual < expected,
            CompareOp::Gte => actual >= expected,
            CompareOp::Lte => actual <= expected,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_eq() {
        let p = Predicate::parse("rel=risk").unwrap();
        assert_eq!(p.field, "rel");
        assert_eq!(p.op, CompareOp::Eq);
        assert_eq!(p.value, "risk");
    }

    #[test]
    fn parse_ne() {
        let p = Predicate::parse("rel!=risk").unwrap();
        assert_eq!(p.field, "rel");
        assert_eq!(p.op, CompareOp::Ne);
        assert_eq!(p.value, "risk");
    }

    #[test]
    fn parse_gt() {
        let p = Predicate::parse("confidence>0.8").unwrap();
        assert_eq!(p.field, "confidence");
        assert_eq!(p.op, CompareOp::Gt);
        assert_eq!(p.value, "0.8");
    }

    #[test]
    fn parse_gte() {
        let p = Predicate::parse("endorsed>=2").unwrap();
        assert_eq!(p.field, "endorsed");
        assert_eq!(p.op, CompareOp::Gte);
        assert_eq!(p.value, "2");
    }

    #[test]
    fn parse_lt() {
        let p = Predicate::parse("confidence<0.5").unwrap();
        assert_eq!(p.field, "confidence");
        assert_eq!(p.op, CompareOp::Lt);
        assert_eq!(p.value, "0.5");
    }

    #[test]
    fn parse_lte() {
        let p = Predicate::parse("endorsed<=1").unwrap();
        assert_eq!(p.field, "endorsed");
        assert_eq!(p.op, CompareOp::Lte);
        assert_eq!(p.value, "1");
    }

    #[test]
    fn parse_invalid() {
        assert!(Predicate::parse("garbage").is_err());
    }

    #[test]
    fn parse_empty_field() {
        assert!(Predicate::parse("=value").is_err());
    }

    #[test]
    fn matches_rel() {
        let p = Predicate::parse("rel=risk").unwrap();
        let edge = EdgeRow {
            id: "e_1".into(),
            from_id: None,
            to_id: None,
            rel: "risk".into(),
            payload: "{}".into(),
            ns: "test".into(),
            ts: "2025-01-01T00:00:00Z".into(),
            agent: None,
            is_active: true,
        };
        assert!(p.matches(&edge, 0, 0));
    }

    #[test]
    fn matches_confidence_gt() {
        let p = Predicate::parse("confidence>0.8").unwrap();
        let edge = EdgeRow {
            id: "e_1".into(),
            from_id: None,
            to_id: None,
            rel: "risk".into(),
            payload: r#"{"confidence":0.95}"#.into(),
            ns: "test".into(),
            ts: "2025-01-01T00:00:00Z".into(),
            agent: None,
            is_active: true,
        };
        assert!(p.matches(&edge, 0, 0));

        let low = EdgeRow {
            payload: r#"{"confidence":0.5}"#.into(),
            ..edge
        };
        assert!(!p.matches(&low, 0, 0));
    }

    #[test]
    fn matches_endorsed_count() {
        let p = Predicate::parse("endorsed>2").unwrap();
        let edge = EdgeRow {
            id: "e_1".into(),
            from_id: None,
            to_id: None,
            rel: "risk".into(),
            payload: "{}".into(),
            ns: "test".into(),
            ts: "2025-01-01T00:00:00Z".into(),
            agent: None,
            is_active: true,
        };
        assert!(p.matches(&edge, 3, 0));
        assert!(!p.matches(&edge, 1, 0));
    }

    #[test]
    fn matches_disputed_bool() {
        let p = Predicate::parse("disputed=true").unwrap();
        let edge = EdgeRow {
            id: "e_1".into(),
            from_id: None,
            to_id: None,
            rel: "risk".into(),
            payload: "{}".into(),
            ns: "test".into(),
            ts: "2025-01-01T00:00:00Z".into(),
            agent: None,
            is_active: true,
        };
        assert!(p.matches(&edge, 0, 1));
        assert!(!p.matches(&edge, 0, 0));
    }

    #[test]
    fn matches_tags() {
        let p = Predicate::parse("tags=security").unwrap();
        let edge = EdgeRow {
            id: "e_1".into(),
            from_id: None,
            to_id: None,
            rel: "risk".into(),
            payload: r#"{"tags":["security","auth"]}"#.into(),
            ns: "test".into(),
            ts: "2025-01-01T00:00:00Z".into(),
            agent: None,
            is_active: true,
        };
        assert!(p.matches(&edge, 0, 0));

        let p_ne = Predicate::parse("tags=unrelated").unwrap();
        assert!(!p_ne.matches(&edge, 0, 0));
    }
}

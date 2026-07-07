use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Top-level Damask project configuration (`.damask/config.json`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DamaskConfig {
    /// Default namespace for commands when none is specified.
    #[serde(default)]
    pub default_ns: Option<String>,

    /// Default decay half-life in days, used when a namespace doesn't override it.
    /// Falls back to 180 if not set.
    #[serde(default)]
    pub default_decay_half_life_days: Option<u32>,

    /// Per-namespace configuration overrides.
    #[serde(default)]
    pub namespaces: HashMap<String, NamespaceConfig>,
}

/// Per-namespace configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NamespaceConfig {
    /// Decay half-life in days. Edges older than this lose ranking weight.
    /// Spec default: 180 days for code, 365 for compliance/legal.
    #[serde(default)]
    pub decay_half_life_days: Option<u32>,

    /// Description of this namespace's purpose.
    #[serde(default)]
    pub description: Option<String>,

    /// Payload schema asserted by this namespace. Damask is a protocol —
    /// domains bring their own fields. `None` = the built-in default
    /// convention applies (severity); `Some({})` = explicitly schema-less.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<HashMap<String, FieldSchema>>,
}

/// Declared semantics for one payload field within a namespace.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FieldSchema {
    /// Allowed values; writes outside this set are rejected with a
    /// teaching error. Omit for free-form fields.
    #[serde(default, rename = "enum", skip_serializing_if = "Option::is_none")]
    pub enum_values: Option<Vec<String>>,

    /// Ranking multipliers per value — how this field orders attention.
    /// Domain semantics live here as data, not in damask's code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rank: Option<HashMap<String, f64>>,

    /// What this field means, for humans and agents reading the config.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// The built-in default convention, used only when a namespace asserts no
/// schema at all: `severity` (critical|high|medium|low) with modest rank
/// weights. It is a CONVENTION, not core — declare any schema (even `{}`)
/// on a namespace to replace or remove it.
pub fn default_convention_schema() -> HashMap<String, FieldSchema> {
    let mut rank = HashMap::new();
    rank.insert("critical".to_string(), 1.12);
    rank.insert("high".to_string(), 1.06);
    rank.insert("medium".to_string(), 1.0);
    rank.insert("low".to_string(), 0.92);
    let mut m = HashMap::new();
    m.insert(
        "severity".to_string(),
        FieldSchema {
            enum_values: Some(
                ["critical", "high", "medium", "low"]
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
            ),
            rank: Some(rank),
            description: Some("How much it matters — orthogonal to confidence".to_string()),
        },
    );
    m
}

impl DamaskConfig {
    /// The effective payload schema for a namespace: its own declaration,
    /// or the default convention when it declares none.
    pub fn effective_schema(&self, ns: &str) -> HashMap<String, FieldSchema> {
        match self.namespaces.get(ns).and_then(|c| c.schema.clone()) {
            Some(schema) => schema,
            None => default_convention_schema(),
        }
    }

    /// Ranking multiplier from the namespace's schema: for every declared
    /// field with rank weights, multiply by the weight of the payload's
    /// value. Unknown values and undeclared fields are neutral.
    pub fn schema_rank_factor(&self, ns: &str, payload: &serde_json::Value) -> f64 {
        let mut factor = 1.0;
        for (field, fs) in self.effective_schema(ns) {
            let (Some(rank), Some(value)) = (
                fs.rank.as_ref(),
                payload.get(&field).and_then(|v| v.as_str()),
            ) else {
                continue;
            };
            if let Some(w) = rank.get(value) {
                factor *= w;
            }
        }
        factor
    }

    /// Validate a payload against the namespace's schema (enum fields).
    /// Returns a teaching error naming the field and allowed values.
    pub fn validate_ns_payload(&self, ns: &str, payload: &serde_json::Value) -> Result<(), String> {
        for (field, fs) in self.effective_schema(ns) {
            let (Some(allowed), Some(value)) = (
                fs.enum_values.as_ref(),
                payload.get(&field).and_then(|v| v.as_str()),
            ) else {
                continue;
            };
            if !allowed.iter().any(|a| a == value) {
                return Err(format!(
                    "namespace '{ns}' schema: {field} must be one of [{}] (got {value:?})",
                    allowed.join(", ")
                ));
            }
        }
        Ok(())
    }

    /// Get the decay half-life for a namespace, falling back to project default, then 180 days.
    pub fn decay_half_life_days(&self, ns: &str) -> u32 {
        self.namespaces
            .get(ns)
            .and_then(|c| c.decay_half_life_days)
            .or(self.default_decay_half_life_days)
            .unwrap_or(180)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config() {
        let config = DamaskConfig::default();
        assert!(config.default_ns.is_none());
        assert!(config.namespaces.is_empty());
        assert_eq!(config.decay_half_life_days("anything"), 180);
    }

    #[test]
    fn serde_round_trip() {
        let json = r#"{
            "default_ns": "security-audit",
            "namespaces": {
                "security-audit": {
                    "decay_half_life_days": 90,
                    "description": "Security findings"
                },
                "legal-review": {
                    "decay_half_life_days": 365
                }
            }
        }"#;
        let config: DamaskConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.default_ns.as_deref(), Some("security-audit"));
        assert_eq!(config.decay_half_life_days("security-audit"), 90);
        assert_eq!(config.decay_half_life_days("legal-review"), 365);
        assert_eq!(config.decay_half_life_days("unknown"), 180);
    }

    #[test]
    fn minimal_config() {
        let json = "{}";
        let config: DamaskConfig = serde_json::from_str(json).unwrap();
        assert!(config.default_ns.is_none());
        assert!(config.namespaces.is_empty());
    }
}

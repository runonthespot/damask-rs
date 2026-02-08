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
}

impl DamaskConfig {
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

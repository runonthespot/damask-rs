use serde::{Deserialize, Serialize};

/// Resolution axis — how well a span's location can be determined.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Resolution {
    /// Content hash matches and lines are correct.
    Exact,
    /// Snippet/symbol matched but lines shifted; span was relocated.
    Relocated,
    /// File exists but content cannot be matched to this span.
    Unresolved,
    /// File no longer exists at the expected path.
    Missing,
}

/// Recency axis — is the file's working tree dirty? Compared against HEAD,
/// NOT the span's original commit, so a file that merely evolved across
/// commits but is now committed-clean is `Unchanged`; only an uncommitted
/// edit is `FileChanged`. This drives the neutral "uncommitted" (grey)
/// display state, orthogonal to whether the anchor content itself moved
/// (that is the Resolution axis).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Recency {
    /// On-disk content matches HEAD — the file is committed as it stands.
    Unchanged,
    /// On-disk content differs from HEAD — uncommitted working-tree changes.
    FileChanged,
    /// Cannot determine (no git, or the file isn't tracked in HEAD).
    Unknown,
}

/// Combined freshness state for a span.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Freshness {
    pub resolution: Resolution,
    pub recency: Recency,
}

impl Freshness {
    pub fn new(resolution: Resolution, recency: Recency) -> Self {
        Self {
            resolution,
            recency,
        }
    }

    /// Whether this span is considered "fresh" for ranking purposes.
    /// Exact+Unchanged is fully fresh; Relocated+Unchanged is still usable.
    pub fn is_fresh(&self) -> bool {
        matches!(self.resolution, Resolution::Exact | Resolution::Relocated)
            && self.recency == Recency::Unchanged
    }

    /// Ranking weight for resolution (used in the 10-signal ranking).
    pub fn resolution_weight(&self) -> f64 {
        match self.resolution {
            Resolution::Exact => 1.0,
            Resolution::Relocated => 0.7,
            Resolution::Unresolved => 0.3,
            Resolution::Missing => 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_unchanged_is_fresh() {
        let f = Freshness::new(Resolution::Exact, Recency::Unchanged);
        assert!(f.is_fresh());
        assert_eq!(f.resolution_weight(), 1.0);
    }

    #[test]
    fn relocated_unchanged_is_fresh() {
        let f = Freshness::new(Resolution::Relocated, Recency::Unchanged);
        assert!(f.is_fresh());
        assert_eq!(f.resolution_weight(), 0.7);
    }

    #[test]
    fn exact_file_changed_not_fresh() {
        let f = Freshness::new(Resolution::Exact, Recency::FileChanged);
        assert!(!f.is_fresh());
    }

    #[test]
    fn unresolved_not_fresh() {
        let f = Freshness::new(Resolution::Unresolved, Recency::Unknown);
        assert!(!f.is_fresh());
        assert_eq!(f.resolution_weight(), 0.3);
    }

    #[test]
    fn missing_zero_weight() {
        let f = Freshness::new(Resolution::Missing, Recency::Unknown);
        assert!(!f.is_fresh());
        assert_eq!(f.resolution_weight(), 0.0);
    }

    #[test]
    fn resolution_serde_round_trip() {
        let json = serde_json::to_string(&Resolution::Exact).unwrap();
        assert_eq!(json, "\"exact\"");
        let parsed: Resolution = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, Resolution::Exact);
    }

    #[test]
    fn recency_serde_round_trip() {
        let json = serde_json::to_string(&Recency::FileChanged).unwrap();
        assert_eq!(json, "\"file_changed\"");
        let parsed: Recency = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, Recency::FileChanged);
    }
}

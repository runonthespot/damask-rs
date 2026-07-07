/// Meta-relationship types that operate on edges rather than spans.
pub const META_RELS: &[&str] = &["supersedes", "invalidates", "endorsed", "disputed", "closed"];

/// Judgment relationship types — edges that represent analysis findings.
pub const JUDGMENT_RELS: &[&str] = &[
    "risk",
    "gotcha",
    "decision",
    "contradicts",
    "ruled_out",
    "conflicts_with",
];

/// Descriptive relationship types — edges that describe or link content.
pub const DESCRIPTIVE_RELS: &[&str] = &[
    "depends_on",
    "supports",
    "describes",
    "derived_from",
    "co_change",
    "implements",
    "env",
    "perf",
];

/// Check if a rel type is a meta-relationship.
pub fn is_meta_rel(rel: &str) -> bool {
    META_RELS.contains(&rel)
}

/// Classification of relationship types for display grouping.
/// All content classes rank equally — classification is for organization, not priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelClass {
    /// Analytical rels: risk, gotcha, decision, contradicts, ruled_out, conflicts_with.
    /// Edges that interpret or evaluate.
    Judgment,

    /// Factual rels: depends_on, supports, describes, derived_from, etc.
    /// Edges that document or link.
    Descriptive,

    /// Meta rels: supersedes, invalidates, endorsed, disputed, closed.
    /// Structural edges excluded from content display.
    Meta,

    /// Any unrecognized rel type.
    Other,
}

impl RelClass {
    /// Classify a relationship type string.
    pub fn classify(rel: &str) -> Self {
        if META_RELS.contains(&rel) {
            RelClass::Meta
        } else if JUDGMENT_RELS.contains(&rel) {
            RelClass::Judgment
        } else if DESCRIPTIVE_RELS.contains(&rel) {
            RelClass::Descriptive
        } else {
            RelClass::Other
        }
    }

    /// Whether this rel type is a meta-relationship (operates on edges, not spans).
    pub fn is_meta(rel: &str) -> bool {
        META_RELS.contains(&rel)
    }

    /// Ranking weight for this rel class.
    /// All content classes rank equally (1.0) — only meta-edges are excluded (0.0).
    pub fn rank_weight(self) -> f64 {
        match self {
            RelClass::Meta => 0.0,
            _ => 1.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_judgment() {
        assert_eq!(RelClass::classify("risk"), RelClass::Judgment);
        assert_eq!(RelClass::classify("gotcha"), RelClass::Judgment);
        assert_eq!(RelClass::classify("decision"), RelClass::Judgment);
        assert_eq!(RelClass::classify("contradicts"), RelClass::Judgment);
        assert_eq!(RelClass::classify("ruled_out"), RelClass::Judgment);
    }

    #[test]
    fn classify_descriptive() {
        assert_eq!(RelClass::classify("depends_on"), RelClass::Descriptive);
        assert_eq!(RelClass::classify("supports"), RelClass::Descriptive);
        assert_eq!(RelClass::classify("describes"), RelClass::Descriptive);
    }

    #[test]
    fn classify_meta() {
        assert_eq!(RelClass::classify("supersedes"), RelClass::Meta);
        assert_eq!(RelClass::classify("invalidates"), RelClass::Meta);
        assert_eq!(RelClass::classify("endorsed"), RelClass::Meta);
        assert_eq!(RelClass::classify("disputed"), RelClass::Meta);
    }

    #[test]
    fn classify_unknown() {
        assert_eq!(RelClass::classify("custom_rel"), RelClass::Other);
        assert_eq!(RelClass::classify("amends"), RelClass::Other);
    }

    #[test]
    fn is_meta() {
        assert!(RelClass::is_meta("supersedes"));
        assert!(RelClass::is_meta("endorsed"));
        assert!(!RelClass::is_meta("risk"));
        assert!(!RelClass::is_meta("custom"));
    }

    #[test]
    fn rank_weights_content_equal() {
        // All content classes rank equally
        assert_eq!(RelClass::Judgment.rank_weight(), RelClass::Other.rank_weight());
        assert_eq!(RelClass::Other.rank_weight(), RelClass::Descriptive.rank_weight());
        // Only meta-edges are excluded
        assert!(RelClass::Descriptive.rank_weight() > RelClass::Meta.rank_weight());
        assert_eq!(RelClass::Meta.rank_weight(), 0.0);
    }
}

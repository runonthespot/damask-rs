use chrono::{DateTime, Utc};

use damask_core::{PayloadEnvelope, RelClass};

use crate::decay::compute_decay;
use crate::index::query::EdgeRow;

/// Computed ranking data for a single edge.
#[derive(Debug, Clone)]
pub struct RankedEdge {
    pub edge: EdgeRow,
    pub score: f64,
    pub endorsement_count: u32,
    pub dispute_count: u32,
}

/// Input signals for ranking a single edge.
pub struct RankingInput {
    pub edge: EdgeRow,
    pub endorsement_count: u32,
    pub dispute_count: u32,
    pub effective_ts: DateTime<Utc>,
    pub half_life_days: u32,
    pub now: DateTime<Utc>,
    /// Resolution weight from span freshness (1.0 = exact, 0.7 = relocated, 0.3 = unresolved, 0.0 = missing).
    /// Defaults to 1.0 when freshness is not computed.
    pub resolution_weight: f64,
    /// Signal density score (1.0 = original insight, penalized toward 0.5 for restatements).
    /// Defaults to 1.0 when not computed.
    pub signal_density: f64,
}

/// Compute the composite ranking score for an edge.
/// Implements the 10-signal ranking policy from spec §10.3.
pub fn rank_edge(input: &RankingInput) -> f64 {
    let payload: serde_json::Value =
        serde_json::from_str(&input.edge.payload).unwrap_or(serde_json::json!({}));
    let env = PayloadEnvelope::new(&payload);

    // Signal 1: Resolution — from span freshness computation
    let resolution_score = input.resolution_weight;

    // Signal 2: Confidence
    let confidence_score = env.confidence().unwrap_or(0.5);

    // Signal 3: Actionability — edges with action field rank higher
    let actionability_score = if env.action().is_some() { 1.0 } else { 0.5 };

    // Signal 4: Rel class
    let rel_class = RelClass::classify(&input.edge.rel);
    let rel_class_score = rel_class.rank_weight();

    // Signal 5: Signal density — penalizes restatements of span content
    let signal_density_score = input.signal_density;

    // Signal 6: Completeness — summary + confidence present
    let completeness_score = {
        let has_summary = env.summary().is_some();
        let has_confidence = env.confidence().is_some();
        match (has_summary, has_confidence) {
            (true, true) => 1.0,
            (true, false) | (false, true) => 0.7,
            (false, false) => 0.4,
        }
    };

    // Signal 7: Endorsement count (logarithmic boost)
    let endorsement_score = if input.endorsement_count > 0 {
        1.0 + (input.endorsement_count as f64).ln() * 0.3
    } else {
        1.0
    };

    // Signal 8: Dispute signal — disputed edges get penalized
    let dispute_score = if input.dispute_count > 0 && input.endorsement_count == 0 {
        0.3 // disputed with no endorsements = low rank
    } else if input.dispute_count > 0 {
        0.7 // disputed but also endorsed = surface the disagreement
    } else {
        1.0
    };

    // Signal 9: Recency decay
    let decay_score = compute_decay(input.effective_ts, input.now, input.half_life_days);

    // Signal 10: Source (local vs community — all local for now)
    let source_score = 1.0;

    // Composite score: weighted product
    resolution_score * 0.15
        + confidence_score * 0.15
        + actionability_score * 0.10
        + rel_class_score * 0.10
        + signal_density_score * 0.05
        + completeness_score * 0.10
        + endorsement_score * 0.15
        + dispute_score * 0.05
        + decay_score * 0.10
        + source_score * 0.05
}

/// Rank a list of edges and return them sorted by score (highest first).
/// Maximum `limit` edges returned.
pub fn rank_edges(mut inputs: Vec<RankingInput>, limit: usize) -> Vec<RankedEdge> {
    let mut ranked: Vec<RankedEdge> = inputs
        .drain(..)
        .map(|input| {
            let score = rank_edge(&input);
            RankedEdge {
                edge: input.edge,
                score,
                endorsement_count: input.endorsement_count,
                dispute_count: input.dispute_count,
            }
        })
        .collect();

    ranked.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    ranked.truncate(limit);
    ranked
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_edge(rel: &str, payload: &str) -> EdgeRow {
        EdgeRow {
            id: "e_1".to_string(),
            from_id: Some("s_1".to_string()),
            to_id: None,
            rel: rel.to_string(),
            payload: payload.to_string(),
            ns: "test".to_string(),
            ts: "2025-01-01T00:00:00Z".to_string(),
            agent: None,
            is_active: true,
        }
    }

    fn make_input(edge: EdgeRow, endorsements: u32, disputes: u32) -> RankingInput {
        RankingInput {
            edge,
            endorsement_count: endorsements,
            dispute_count: disputes,
            effective_ts: Utc::now(),
            half_life_days: 180,
            now: Utc::now(),
            resolution_weight: 1.0,
            signal_density: 1.0,
        }
    }

    #[test]
    fn high_confidence_ranks_higher() {
        let e1 = make_edge("risk", r#"{"summary":"test","confidence":0.95}"#);
        let e2 = make_edge("risk", r#"{"summary":"test","confidence":0.5}"#);

        let s1 = rank_edge(&make_input(e1, 0, 0));
        let s2 = rank_edge(&make_input(e2, 0, 0));
        assert!(s1 > s2);
    }

    #[test]
    fn judgment_ranks_above_descriptive() {
        let e1 = make_edge("risk", r#"{"summary":"test","confidence":0.8}"#);
        let e2 = make_edge("describes", r#"{"summary":"test","confidence":0.8}"#);

        let s1 = rank_edge(&make_input(e1, 0, 0));
        let s2 = rank_edge(&make_input(e2, 0, 0));
        assert!(s1 > s2);
    }

    #[test]
    fn endorsements_boost_rank() {
        let e1 = make_edge("risk", r#"{"summary":"test","confidence":0.8}"#);
        let e2 = make_edge("risk", r#"{"summary":"test","confidence":0.8}"#);

        let s1 = rank_edge(&make_input(e1, 3, 0));
        let s2 = rank_edge(&make_input(e2, 0, 0));
        assert!(s1 > s2);
    }

    #[test]
    fn disputed_unendorsed_ranks_low() {
        let e1 = make_edge("risk", r#"{"summary":"test","confidence":0.8}"#);
        let e2 = make_edge("risk", r#"{"summary":"test","confidence":0.8}"#);

        let s1 = rank_edge(&make_input(e1, 0, 2));
        let s2 = rank_edge(&make_input(e2, 0, 0));
        assert!(s1 < s2);
    }

    #[test]
    fn actionable_ranks_higher() {
        let e1 = make_edge(
            "risk",
            r#"{"summary":"test","confidence":0.8,"action":"fix it"}"#,
        );
        let e2 = make_edge("risk", r#"{"summary":"test","confidence":0.8}"#);

        let s1 = rank_edge(&make_input(e1, 0, 0));
        let s2 = rank_edge(&make_input(e2, 0, 0));
        assert!(s1 > s2);
    }

    #[test]
    fn rank_edges_sorts_and_limits() {
        let inputs = vec![
            make_input(
                make_edge("risk", r#"{"summary":"low","confidence":0.3}"#),
                0,
                0,
            ),
            make_input(
                make_edge("risk", r#"{"summary":"high","confidence":0.99}"#),
                0,
                0,
            ),
            make_input(
                make_edge("risk", r#"{"summary":"mid","confidence":0.6}"#),
                0,
                0,
            ),
        ];

        let ranked = rank_edges(inputs, 2);
        assert_eq!(ranked.len(), 2);
        assert!(ranked[0].score >= ranked[1].score);
    }
}

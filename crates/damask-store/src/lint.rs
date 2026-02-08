use damask_core::vocabulary;
use damask_core::PayloadEnvelope;

use crate::index::query::EdgeRow;

/// A lint issue found in the edge data.
#[derive(Debug, Clone)]
pub struct LintIssue {
    pub edge_id: String,
    pub severity: Severity,
    pub rule: &'static str,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Severity {
    Error,
    Warning,
}

/// Run all lint rules against a set of edges with their associated span snippets.
pub fn lint_edges(edges: &[LintInput]) -> Vec<LintIssue> {
    let mut issues = Vec::new();

    for input in edges {
        let payload: serde_json::Value =
            serde_json::from_str(&input.edge.payload).unwrap_or(serde_json::json!({}));
        let env = PayloadEnvelope::new(&payload);

        // Hard flags (always reported)
        check_empty_payload(&input.edge, &env, &mut issues);
        check_missing_summary(&input.edge, &env, &mut issues);
        check_broken_json(&input.edge, &mut issues);
        check_missing_timestamp(&input.edge, &mut issues);
        check_edge_to_edge_domain_rel(&input.edge, &mut issues);

        // Staleness flags (from resolution data)
        check_stale_resolution(&input.edge, input.resolution.as_deref(), &mut issues);

        // Signal density flags (warnings)
        check_restatement(
            &input.edge,
            &env,
            input.span_snippet.as_deref(),
            &mut issues,
        );
        check_confidence_floor(&input.edge, &env, &mut issues);
        check_actionable_without_action(&input.edge, &env, &mut issues);
    }

    // Speed check: flag edges created suspiciously close together
    check_speed_creation(edges, &mut issues);

    issues
}

/// Input for lint checking: an edge with optional span snippet and resolution.
pub struct LintInput {
    pub edge: EdgeRow,
    pub span_snippet: Option<String>,
    pub resolution: Option<String>,
}

// --- Hard flags ---

fn check_empty_payload(edge: &EdgeRow, env: &PayloadEnvelope, issues: &mut Vec<LintIssue>) {
    if env.is_empty() {
        // Meta-edges with empty payloads are okay (endorsements don't require payload)
        if !vocabulary::is_meta_rel(&edge.rel) {
            issues.push(LintIssue {
                edge_id: edge.id.clone(),
                severity: Severity::Error,
                rule: "empty-payload",
                message: format!("[{}] empty payload — edge carries no knowledge", edge.rel),
            });
        }
    }
}

fn check_missing_summary(edge: &EdgeRow, env: &PayloadEnvelope, issues: &mut Vec<LintIssue>) {
    if env.summary().is_none() && !env.is_empty() && !vocabulary::is_meta_rel(&edge.rel) {
        issues.push(LintIssue {
            edge_id: edge.id.clone(),
            severity: Severity::Error,
            rule: "missing-summary",
            message: format!(
                "[{}] payload has fields but no summary — add a summary for damask at",
                edge.rel
            ),
        });
    }
}

fn check_broken_json(edge: &EdgeRow, issues: &mut Vec<LintIssue>) {
    if serde_json::from_str::<serde_json::Value>(&edge.payload).is_err() {
        issues.push(LintIssue {
            edge_id: edge.id.clone(),
            severity: Severity::Error,
            rule: "broken-json",
            message: "payload is not valid JSON".to_string(),
        });
    }
}

fn check_missing_timestamp(edge: &EdgeRow, issues: &mut Vec<LintIssue>) {
    if edge.ts.is_empty() {
        issues.push(LintIssue {
            edge_id: edge.id.clone(),
            severity: Severity::Error,
            rule: "missing-timestamp",
            message: "edge has no timestamp".to_string(),
        });
    }
}

fn check_edge_to_edge_domain_rel(edge: &EdgeRow, issues: &mut Vec<LintIssue>) {
    // If both from and to are edge IDs, only meta rels should be used
    let from_is_edge = edge
        .from_id
        .as_deref()
        .is_some_and(|id| id.starts_with("e_"));
    let to_is_edge = edge.to_id.as_deref().is_some_and(|id| id.starts_with("e_"));

    if from_is_edge && to_is_edge && !vocabulary::is_meta_rel(&edge.rel) {
        issues.push(LintIssue {
            edge_id: edge.id.clone(),
            severity: Severity::Error,
            rule: "edge-to-edge-domain-rel",
            message: format!(
                "[{}] edge-to-edge link uses domain rel — should use meta rels (supersedes, invalidates, endorsed, disputed)",
                edge.rel
            ),
        });
    }
}

// --- Signal density flags (warnings) ---

fn check_restatement(
    edge: &EdgeRow,
    env: &PayloadEnvelope,
    span_snippet: Option<&str>,
    issues: &mut Vec<LintIssue>,
) {
    if let (Some(summary), Some(snippet)) = (env.summary(), span_snippet) {
        let overlap = token_overlap_ratio(summary, snippet);
        if overlap > 0.6 {
            issues.push(LintIssue {
                edge_id: edge.id.clone(),
                severity: Severity::Warning,
                rule: "restatement-suspicion",
                message: format!(
                    "[{}] summary shares {:.0}% tokens with span snippet — likely observation, not discovery",
                    edge.rel,
                    overlap * 100.0
                ),
            });
        }
    }
}

fn check_confidence_floor(edge: &EdgeRow, env: &PayloadEnvelope, issues: &mut Vec<LintIssue>) {
    if let Some(conf) = env.confidence() {
        if conf < 0.5 {
            let status = env.status().unwrap_or("");
            if status != "hypothesis" {
                issues.push(LintIssue {
                    edge_id: edge.id.clone(),
                    severity: Severity::Warning,
                    rule: "low-confidence",
                    message: format!(
                        "[{}] confidence {:.2} < 0.5 without status:\"hypothesis\"",
                        edge.rel, conf
                    ),
                });
            }
        }
    }
}

fn check_actionable_without_action(
    edge: &EdgeRow,
    env: &PayloadEnvelope,
    issues: &mut Vec<LintIssue>,
) {
    let actionable_rels = ["risk", "gotcha"];
    if actionable_rels.contains(&edge.rel.as_str()) && env.action().is_none() {
        issues.push(LintIssue {
            edge_id: edge.id.clone(),
            severity: Severity::Warning,
            rule: "actionable-without-action",
            message: format!(
                "[{}] edge type suggests action is needed but no action field provided",
                edge.rel
            ),
        });
    }
}

// --- Staleness flags ---

fn check_stale_resolution(edge: &EdgeRow, resolution: Option<&str>, issues: &mut Vec<LintIssue>) {
    if vocabulary::is_meta_rel(&edge.rel) {
        return;
    }
    match resolution {
        Some("missing") => {
            issues.push(LintIssue {
                edge_id: edge.id.clone(),
                severity: Severity::Error,
                rule: "stale-missing",
                message: format!(
                    "[{}] anchored span's file is missing — edge is stale",
                    edge.rel
                ),
            });
        }
        Some("unresolved") => {
            issues.push(LintIssue {
                edge_id: edge.id.clone(),
                severity: Severity::Warning,
                rule: "stale-unresolved",
                message: format!(
                    "[{}] anchored span cannot be resolved — content may have changed significantly",
                    edge.rel
                ),
            });
        }
        _ => {}
    }
}

// --- Speed check ---

/// Flag edges created suspiciously close together (< 5 seconds apart in same namespace).
fn check_speed_creation(edges: &[LintInput], issues: &mut Vec<LintIssue>) {
    // Group by namespace
    let mut by_ns: std::collections::HashMap<&str, Vec<&EdgeRow>> =
        std::collections::HashMap::new();
    for input in edges {
        by_ns.entry(input.edge.ns.as_str()).or_default().push(&input.edge);
    }

    for (_ns, mut ns_edges) in by_ns {
        ns_edges.sort_by(|a, b| a.ts.cmp(&b.ts));

        for window in ns_edges.windows(2) {
            let (a, b) = (&window[0], &window[1]);
            // Skip meta edges
            if vocabulary::is_meta_rel(&a.rel) || vocabulary::is_meta_rel(&b.rel) {
                continue;
            }
            if let (Ok(ta), Ok(tb)) = (
                chrono::DateTime::parse_from_rfc3339(&a.ts),
                chrono::DateTime::parse_from_rfc3339(&b.ts),
            ) {
                let diff = (tb - ta).num_seconds().unsigned_abs();
                if diff < 5 {
                    issues.push(LintIssue {
                        edge_id: b.id.clone(),
                        severity: Severity::Warning,
                        rule: "speed-creation",
                        message: format!(
                            "[{}] created {}s after previous edge — possible batch generation",
                            b.rel, diff
                        ),
                    });
                }
            }
        }
    }
}

/// Compute token overlap ratio between two strings.
/// Returns a value between 0.0 and 1.0.
pub fn token_overlap_ratio(a: &str, b: &str) -> f64 {
    let tokens_a: std::collections::HashSet<&str> = a.split_whitespace().collect();
    let tokens_b: std::collections::HashSet<&str> = b.split_whitespace().collect();

    if tokens_a.is_empty() || tokens_b.is_empty() {
        return 0.0;
    }

    let intersection = tokens_a.intersection(&tokens_b).count();
    let smaller = tokens_a.len().min(tokens_b.len());

    intersection as f64 / smaller as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_edge(id: &str, rel: &str, payload: &str) -> EdgeRow {
        EdgeRow {
            id: id.to_string(),
            from_id: Some("s_test".to_string()),
            to_id: None,
            rel: rel.to_string(),
            payload: payload.to_string(),
            ns: "test".to_string(),
            ts: "2025-01-01T00:00:00Z".to_string(),
            agent: None,
            is_active: true,
        }
    }

    #[test]
    fn flags_empty_payload() {
        let edge = make_edge("e_1", "risk", "{}");
        let input = LintInput {
            edge,
            span_snippet: None,
            resolution: None,
        };
        let issues = lint_edges(&[input]);
        assert!(issues.iter().any(|i| i.rule == "empty-payload"));
    }

    #[test]
    fn empty_payload_ok_for_meta_edges() {
        let edge = make_edge("e_1", "endorsed", "{}");
        let input = LintInput {
            edge,
            span_snippet: None,
            resolution: None,
        };
        let issues = lint_edges(&[input]);
        assert!(!issues.iter().any(|i| i.rule == "empty-payload"));
    }

    #[test]
    fn flags_missing_summary() {
        let edge = make_edge("e_1", "risk", r#"{"confidence":0.9}"#);
        let input = LintInput {
            edge,
            span_snippet: None,
            resolution: None,
        };
        let issues = lint_edges(&[input]);
        assert!(issues.iter().any(|i| i.rule == "missing-summary"));
    }

    #[test]
    fn flags_restatement() {
        let edge = make_edge(
            "e_1",
            "describes",
            r#"{"summary":"fn validate token check"}"#,
        );
        let input = LintInput {
            edge,
            span_snippet: Some("fn validate token check auth".to_string()),
            resolution: None,
        };
        let issues = lint_edges(&[input]);
        assert!(issues.iter().any(|i| i.rule == "restatement-suspicion"));
    }

    #[test]
    fn flags_low_confidence() {
        let edge = make_edge(
            "e_1",
            "risk",
            r#"{"summary":"Maybe risky","confidence":0.3}"#,
        );
        let input = LintInput {
            edge,
            span_snippet: None,
            resolution: None,
        };
        let issues = lint_edges(&[input]);
        assert!(issues.iter().any(|i| i.rule == "low-confidence"));
    }

    #[test]
    fn low_confidence_ok_with_hypothesis() {
        let edge = make_edge(
            "e_1",
            "risk",
            r#"{"summary":"Maybe risky","confidence":0.3,"status":"hypothesis"}"#,
        );
        let input = LintInput {
            edge,
            span_snippet: None,
            resolution: None,
        };
        let issues = lint_edges(&[input]);
        assert!(!issues.iter().any(|i| i.rule == "low-confidence"));
    }

    #[test]
    fn flags_actionable_without_action() {
        let edge = make_edge("e_1", "risk", r#"{"summary":"A risk"}"#);
        let input = LintInput {
            edge,
            span_snippet: None,
            resolution: None,
        };
        let issues = lint_edges(&[input]);
        assert!(issues.iter().any(|i| i.rule == "actionable-without-action"));
    }

    #[test]
    fn actionable_with_action_ok() {
        let edge = make_edge("e_1", "risk", r#"{"summary":"A risk","action":"Fix it"}"#);
        let input = LintInput {
            edge,
            span_snippet: None,
            resolution: None,
        };
        let issues = lint_edges(&[input]);
        assert!(!issues.iter().any(|i| i.rule == "actionable-without-action"));
    }

    #[test]
    fn token_overlap_high() {
        let ratio = token_overlap_ratio("fn validate token check", "fn validate token check auth");
        assert!(ratio > 0.6);
    }

    #[test]
    fn token_overlap_low() {
        let ratio = token_overlap_ratio("security vulnerability", "fn validate token check");
        assert!(ratio < 0.3);
    }
}

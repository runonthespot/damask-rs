//! Canonical trust-signal rendering, shared by every read surface.
//!
//! A confidence number, a freshness glyph, and a dispute marker must mean
//! the same thing wherever an agent encounters them — `at`, `where`,
//! `search`, `orient`, `peek`, human or JSON. Read commands compose their
//! layouts from these pieces instead of hand-rolling them, so a trust
//! signal can never silently go missing from one surface while its
//! meaning is taught by another.

use damask_core::PayloadEnvelope;
use damask_store::index::query::SpanRow;
use damask_store::RankedEdge;

use super::glyphs;

/// Freshness glyph per spec §10.3, from a span's stored resolution/recency.
pub fn freshness_glyph(resolution: Option<&str>, recency: Option<&str>) -> &'static str {
    match (resolution, recency) {
        (Some("missing"), _) => glyphs::UNRESOLVED,
        (Some("unresolved"), _) => glyphs::UNRESOLVED,
        (Some("relocated"), _) => glyphs::RELOCATED,
        (_, Some("file_changed")) => glyphs::FILE_CHANGED,
        (Some("exact"), Some("unchanged")) => glyphs::EXACT_UNCHANGED,
        _ => "",
    }
}

/// Freshness in words — same classification as the glyphs, for contexts
/// consumed by models acting on text (peek injections): a bare glyph is
/// decoration to a model; the words are instruction.
pub fn freshness_words(resolution: Option<&str>, recency: Option<&str>) -> &'static str {
    match (resolution, recency) {
        (Some("missing"), _) => " [\u{274C} anchor code no longer exists]",
        (Some("unresolved"), _) => " [\u{274C} anchor unresolvable]",
        (Some("relocated"), _) => " [\u{21AA} code moved]",
        (_, Some("file_changed")) => " [\u{26A0} file changed since recorded]",
        _ => "",
    }
}

/// Relationship glyph prefix for alarming rels (`at`-style listings).
pub fn rel_glyph(rel: &str) -> &'static str {
    match rel {
        "risk" | "gotcha" => "\u{26A0} ",                // ⚠
        "contradicts" | "conflicts_with" => "\u{2717} ", // ✗
        _ => "  ",
    }
}

/// The canonical signal cluster following a rel name:
/// ` (0.90) [high] [hypothesis] ×2✓ ×1✗ ⚡`.
///
/// The parenthesised number is ALWAYS the author's confidence, never a
/// rank score — agents are taught to read `(x.xx)` as "how sure was the
/// author", and any surface printing something else there poisons that
/// lesson.
pub fn signal_cluster(env: &PayloadEnvelope, endorsements: u32, disputes: u32) -> String {
    let conf = env
        .confidence()
        .map(|c| format!(" ({c:.2})"))
        .unwrap_or_default();
    let severity = env
        .severity()
        .map(|sv| format!(" [{sv}]"))
        .unwrap_or_default();
    let status = match env.status() {
        Some("ruled_out") => " [ruled out]",
        Some("hypothesis") => " [hypothesis]",
        _ => "",
    };
    let endorsed = if endorsements > 0 {
        format!(" \u{00D7}{endorsements}\u{2713}")
    } else {
        String::new()
    };
    let disputed = if disputes > 0 {
        format!(" \u{00D7}{disputes}\u{2717} {}", glyphs::DISPUTED)
    } else {
        String::new()
    };
    format!("{conf}{severity}{status}{endorsed}{disputed}")
}

/// Full span object for JSON output. Every span a machine consumer sees
/// carries its freshness — resolution/recency are the product's core
/// correctness signal and must never be dropped from the machine path.
pub fn span_json(span: &SpanRow) -> serde_json::Value {
    serde_json::json!({
        "id": span.id,
        "path": span.path,
        "line_start": span.line_start,
        "line_end": span.line_end,
        "snippet": span.snippet,
        "symbol": span.symbol,
        "resolution": span.resolution,
        "recency": span.recency,
    })
}

/// Canonical edge object for JSON output: identity, payload, rank score,
/// social signals, and — when the edge is anchored — its span with
/// freshness. `anchor` is the edge's target span, if resolved.
pub fn edge_json(re: &RankedEdge, anchor: Option<&SpanRow>) -> serde_json::Value {
    let payload: serde_json::Value =
        serde_json::from_str(&re.edge.payload).unwrap_or(serde_json::json!({}));
    let span = anchor
        .map(|s| {
            serde_json::json!({
                "id": s.id,
                "path": s.path,
                "line_start": s.line_start,
                "line_end": s.line_end,
                "resolution": s.resolution,
                "recency": s.recency,
            })
        })
        .unwrap_or(serde_json::Value::Null);
    serde_json::json!({
        "id": re.edge.id,
        "from": re.edge.from_id,
        "to": re.edge.to_id,
        "rel": re.edge.rel,
        "payload": payload,
        "ns": re.edge.ns,
        "ts": re.edge.ts,
        "score": re.score,
        "endorsements": re.endorsement_count,
        "disputes": re.dispute_count,
        "span": span,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn glyphs_and_words_agree_on_classification() {
        let cases = [
            (Some("missing"), None),
            (Some("relocated"), Some("unchanged")),
            (None, Some("file_changed")),
            (Some("exact"), Some("unchanged")),
            (None, None),
        ];
        for (res, rec) in cases {
            let glyph = freshness_glyph(res, rec);
            let words = freshness_words(res, rec);
            assert_eq!(
                glyph.is_empty() || glyph == glyphs::EXACT_UNCHANGED,
                words.is_empty(),
                "glyph/words diverge for ({res:?}, {rec:?})"
            );
        }
    }

    #[test]
    fn signal_cluster_shows_confidence_not_score() {
        let payload = json!({"confidence": 0.9, "severity": "high"});
        let env = PayloadEnvelope::new(&payload);
        let s = signal_cluster(&env, 2, 1);
        assert!(s.contains("(0.90)"));
        assert!(s.contains("[high]"));
        assert!(s.contains("\u{00D7}2\u{2713}"));
        assert!(s.contains("\u{00D7}1\u{2717}"));
        assert!(s.contains(glyphs::DISPUTED));
    }

    #[test]
    fn signal_cluster_empty_payload_is_quiet() {
        let payload = json!({});
        let env = PayloadEnvelope::new(&payload);
        assert_eq!(signal_cluster(&env, 0, 0), "");
    }
}

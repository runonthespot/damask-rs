use chrono::{DateTime, Utc};

/// Compute the exponential decay factor for an edge.
///
/// Returns a value between 0.0 and 1.0:
/// - 1.0 = brand new edge
/// - 0.5 = edge is exactly half_life_days old
/// - Approaches 0.0 as age increases
///
/// `effective_ts` is the edge's creation time, or the latest endorsement time
/// (whichever is more recent — endorsements reset the decay clock).
pub fn compute_decay(effective_ts: DateTime<Utc>, now: DateTime<Utc>, half_life_days: u32) -> f64 {
    if half_life_days == 0 {
        return 1.0; // decay disabled
    }

    let age_days = (now - effective_ts).num_seconds() as f64 / 86400.0;
    if age_days <= 0.0 {
        return 1.0;
    }

    let half_life = half_life_days as f64;
    0.5_f64.powf(age_days / half_life)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn brand_new_edge() {
        let now = Utc::now();
        let decay = compute_decay(now, now, 90);
        assert!((decay - 1.0).abs() < 0.001);
    }

    #[test]
    fn at_half_life() {
        let now = Utc::now();
        let ts = now - Duration::days(90);
        let decay = compute_decay(ts, now, 90);
        assert!((decay - 0.5).abs() < 0.01);
    }

    #[test]
    fn at_two_half_lives() {
        let now = Utc::now();
        let ts = now - Duration::days(180);
        let decay = compute_decay(ts, now, 90);
        assert!((decay - 0.25).abs() < 0.01);
    }

    #[test]
    fn zero_half_life_disables_decay() {
        let now = Utc::now();
        let ts = now - Duration::days(9999);
        let decay = compute_decay(ts, now, 0);
        assert_eq!(decay, 1.0);
    }

    #[test]
    fn future_edge() {
        let now = Utc::now();
        let ts = now + Duration::days(1);
        let decay = compute_decay(ts, now, 90);
        assert_eq!(decay, 1.0);
    }
}

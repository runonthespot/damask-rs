/// Freshness glyphs per spec §10.3.
pub const EXACT_UNCHANGED: &str = "\u{2705}"; // ✅
pub const RELOCATED: &str = "\u{21AA}"; // ↪
pub const UNRESOLVED: &str = "\u{274C}"; // ❌
pub const DISPUTED: &str = "\u{26A1}"; // ⚡
/// Exact anchor whose file has uncommitted changes — content matches, but the
/// working tree is dirty. Neutral, not an alarm: recheck after committing.
pub const UNCOMMITTED: &str = "\u{26AA}"; // ⚪
